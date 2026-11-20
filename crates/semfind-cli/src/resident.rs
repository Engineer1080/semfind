use crate::start_text_model;
use anyhow::{Context, Result, ensure};
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    net::sockopt::socket_peercred,
    process::geteuid,
};
use semfind_core::{Embedder, ExecutionDevice, InputKind, LocalModel, local_model_revision};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    env, fmt, fs,
    io::{self, BufRead, BufReader, Read, Write},
    num::NonZeroU32,
    os::{
        linux::{fs::MetadataExt, net::SocketAddrExt},
        unix::{
            net::{SocketAddr, UnixListener, UnixStream},
            process::CommandExt,
        },
    },
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const MODEL_TIMEOUT: Duration = Duration::from_secs(120);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_QUERY_BYTES: usize = 16 * 1024;
const MAX_REPLY_BYTES: usize = 128 * 1024;

#[derive(Deserialize, Serialize)]
struct Ready {
    identity: String,
    minimum_similarity: Option<f32>,
    provider: String,
    reused: bool,
}

#[derive(Deserialize, Serialize)]
struct Request<Q> {
    query: Q,
    idle_seconds: NonZeroU32,
}

#[derive(Deserialize, Serialize)]
struct Embedding {
    vectors: Vec<Vec<f32>>,
    uses_cpu: bool,
}

#[derive(Debug)]
pub(super) struct Disconnected;

impl fmt::Display for Disconnected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the background model disconnected")
    }
}

pub(super) struct Client {
    stream: UnixStream,
    ready: Ready,
    uses_cpu: bool,
    idle_seconds: NonZeroU32,
}

impl Client {
    pub(super) fn connect(
        python: &Path,
        model: &Path,
        runtime: Option<&Path>,
        idle_seconds: NonZeroU32,
    ) -> Result<Self> {
        let model = std::path::absolute(model)?;
        let executable = env::current_exe()?;
        let address = address(&executable, python, &model, runtime)?;
        let stream = match connect(&address) {
            Ok(stream) => stream,
            Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                let mut command = Command::new(&executable);
                command
                    .arg("--python")
                    .arg(python)
                    .arg("serve-model")
                    .arg(&model)
                    .env_remove("SEMFIND_DAEMON_IDLE_SECONDS")
                    .process_group(0)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                if let Some(runtime) = runtime {
                    command.arg("--runtime").arg(runtime);
                }
                let mut child = command.spawn().context("start the background model")?;
                let deadline = Instant::now() + Duration::from_millis(500);
                loop {
                    match connect(&address) {
                        Ok(stream) => break stream,
                        Err(error)
                            if error.kind() == io::ErrorKind::ConnectionRefused
                                && Instant::now() < deadline =>
                        {
                            // Another caller may have won the race to bind the socket.
                            let _ = child.try_wait();
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
            }
            Err(error) => return Err(error.into()),
        };
        Self::from_stream(stream, idle_seconds)
    }

    fn from_stream(stream: UnixStream, idle_seconds: NonZeroU32) -> Result<Self> {
        stream.set_read_timeout(Some(MODEL_TIMEOUT))?;
        stream.set_write_timeout(Some(CLIENT_TIMEOUT))?;
        let ready: Ready = receive(&stream, MAX_REPLY_BYTES)?;
        Ok(Self {
            stream,
            uses_cpu: ready.provider == "CPUExecutionProvider",
            ready,
            idle_seconds,
        })
    }

    pub(super) fn provider(&self) -> &str {
        if self.ready.reused && self.uses_cpu {
            return "CPUExecutionProvider";
        }
        &self.ready.provider
    }

    pub(super) fn uses_cpu(&self) -> bool {
        self.uses_cpu
    }

    pub(super) fn reused(&self) -> bool {
        self.ready.reused
    }
}

impl Embedder for Client {
    fn identity(&self) -> &str {
        &self.ready.identity
    }

    fn minimum_similarity(&self) -> Option<f32> {
        self.ready.minimum_similarity
    }

    fn encode(&mut self, kind: InputKind, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let [query] = texts else {
            anyhow::bail!("the background model accepts one search query at a time");
        };
        ensure!(matches!(kind, InputKind::Query), "expected a search query");
        let request = Request {
            query,
            idle_seconds: self.idle_seconds,
        };
        let embedding: Embedding = send(&self.stream, &request)
            .and_then(|()| receive(&self.stream, MAX_REPLY_BYTES))
            .context(Disconnected)?;
        self.uses_cpu = embedding.uses_cpu;
        Ok(embedding.vectors)
    }
}

pub(super) fn serve(python: &Path, model: &Path, runtime: Option<&Path>) -> Result<()> {
    let address = address(&env::current_exe()?, python, model, runtime)?;
    let listener = match UnixListener::bind_addr(&address) {
        Ok(listener) => listener,
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    serve_on(listener, || {
        start_text_model(python, model, ExecutionDevice::Auto, runtime)
    })
}

fn serve_on(listener: UnixListener, start_model: impl Fn() -> Result<LocalModel>) -> Result<()> {
    let mut model = None;
    let mut deadline = Instant::now() + MODEL_TIMEOUT;
    let mut poll_fds = [PollFd::new(&listener, PollFlags::IN)];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }
        let timeout = Timespec {
            tv_sec: remaining.as_secs() as _,
            tv_nsec: remaining.subsec_nanos().into(),
        };
        match poll(&mut poll_fds, Some(&timeout)) {
            Ok(0) | Err(rustix::io::Errno::INTR) => continue,
            Ok(_) => {}
            Err(error) => return Err(error.into()),
        }
        let (stream, _) = listener.accept()?;
        if same_user(&stream).is_err() {
            continue;
        }
        if let Ok(idle) = serve_query(&stream, &mut model, &start_model) {
            deadline = Instant::now() + idle;
        }
        if model.is_none() {
            return Ok(());
        }
    }
}

fn serve_query(
    stream: &UnixStream,
    model: &mut Option<LocalModel>,
    start_model: impl FnOnce() -> Result<LocalModel>,
) -> Result<Duration> {
    stream.set_read_timeout(Some(CLIENT_TIMEOUT))?;
    stream.set_write_timeout(Some(CLIENT_TIMEOUT))?;
    let reused = model.is_some();
    let worker = match model {
        Some(worker) => worker,
        None => model.insert(start_model()?),
    };
    send(
        stream,
        &Ready {
            identity: worker.identity().to_owned(),
            minimum_similarity: worker.minimum_similarity(),
            provider: worker.initial_provider().to_owned(),
            reused,
        },
    )?;
    let Request {
        query,
        idle_seconds,
    }: Request<String> = receive(stream, MAX_QUERY_BYTES)?;
    ensure!(!query.contains('\0'), "invalid query");
    let result = worker.encode(InputKind::Query, &[query]);
    let uses_cpu = worker.uses_cpu();
    if result.is_err() {
        *model = None;
    }
    send(
        stream,
        &Embedding {
            vectors: result?,
            uses_cpu,
        },
    )?;
    Ok(Duration::from_secs(idle_seconds.get().into()))
}

fn connect(address: &SocketAddr) -> io::Result<UnixStream> {
    let stream = UnixStream::connect_addr(address)?;
    same_user(&stream)?;
    Ok(stream)
}

fn same_user(stream: &UnixStream) -> io::Result<()> {
    if socket_peercred(stream)?.uid == geteuid() {
        Ok(())
    } else {
        Err(io::ErrorKind::PermissionDenied.into())
    }
}

fn send(mut stream: &UnixStream, value: &impl Serialize) -> Result<()> {
    let mut json = serde_json::to_vec(value)?;
    json.push(b'\n');
    stream.write_all(&json)?;
    Ok(())
}

fn receive<T: DeserializeOwned>(stream: &UnixStream, limit: usize) -> Result<T> {
    let mut json = Vec::new();
    BufReader::new(stream.take(limit as u64)).read_until(b'\n', &mut json)?;
    ensure!(json.ends_with(b"\n"), "incomplete background model message");
    Ok(serde_json::from_slice(&json)?)
}

fn address(
    executable: &Path,
    python: &Path,
    model: &Path,
    runtime: Option<&Path>,
) -> Result<SocketAddr> {
    let revision = local_model_revision(model).context("read local model artifacts")?;
    let mut hash = Sha256::new();
    hash.update(b"semfind-resident-model-v2\0");
    hash.update(geteuid().as_raw().to_le_bytes());
    hash.update(revision);
    hash_path(&mut hash, executable);
    hash_path(&mut hash, model);
    hash_path(&mut hash, python);
    if python.components().count() == 1
        && let Some(path) = env::var_os("PATH")
        && let Some(interpreter) =
            env::split_paths(&path)
                .map(|dir| dir.join(python))
                .find(|path| {
                    fs::metadata(path)
                        .is_ok_and(|file| file.is_file() && file.st_mode() & 0o111 != 0)
                })
    {
        hash_path(&mut hash, &interpreter);
    }
    if let Some(runtime) = runtime {
        hash_path(&mut hash, runtime);
        for backend in ["cuda", "migraphx"] {
            let environment = runtime.join(backend);
            hash_path(&mut hash, &environment);
            hash_path(&mut hash, &environment.join("bin/python"));
        }
    }
    for name in [
        "PATH",
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
        "CUDA_VISIBLE_DEVICES",
        "CUDA_DEVICE_ORDER",
        "NVIDIA_VISIBLE_DEVICES",
        "HIP_VISIBLE_DEVICES",
        "ROCR_VISIBLE_DEVICES",
        "GPU_DEVICE_ORDINAL",
    ] {
        if let Some(value) = env::var_os(name) {
            hash.update([1]);
            hash.update(value.as_encoded_bytes());
        }
        hash.update([0]);
    }
    let name = format!("semfind-model-{:x}", hash.finalize());
    Ok(SocketAddr::from_abstract_name(name)?)
}

fn hash_path(hash: &mut Sha256, path: &Path) {
    hash.update(path.as_os_str().as_encoded_bytes());
    hash.update([0]);
    if let Ok(metadata) = fs::metadata(path) {
        hash.update([1]);
        hash.update(metadata.st_dev().to_le_bytes());
        hash.update(metadata.st_ino().to_le_bytes());
        // Directory timestamps can change when unrelated files are written.
        if !metadata.is_dir() {
            hash.update(metadata.st_ctime().to_le_bytes());
            hash.update(metadata.st_ctime_nsec().to_le_bytes());
        }
    }
    hash.update([0]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::fs::PermissionsExt, sync::mpsc};

    #[test]
    #[ignore = "requires access to local Unix sockets"]
    fn different_queries_reuse_the_model_until_idle() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let interpreter = temp.path().join("mock-python");
        fs::write(
            &interpreter,
            r#"#!/bin/sh
printf '%s\n' '{"identity":"test","provider":"CPUExecutionProvider","minimum_similarity":0.3}'
while IFS= read -r query; do
    case "$query" in
        *first*) vector='[1,2]' ;;
        *second*) vector='[2,1]' ;;
        *) exit 1 ;;
    esac
    printf '{"vectors":[%s],"uses_cpu":true}\n' "$vector"
done
"#,
        )?;
        fs::set_permissions(&interpreter, fs::Permissions::from_mode(0o700))?;
        let name = format!(
            "semfind-test-{}-{}",
            std::process::id(),
            temp.path().file_name().unwrap().to_string_lossy()
        );
        let address = SocketAddr::from_abstract_name(name)?;
        let listener = UnixListener::bind_addr(&address)?;
        let (exited, shutdown) = mpsc::channel();
        thread::spawn(move || {
            let result = serve_on(listener, || {
                LocalModel::start_on(&interpreter, Path::new("."), ExecutionDevice::Cpu)
            });
            let _ = exited.send(result);
        });
        for (query, idle_seconds, reused, expected) in [
            ("first", 30_u32, false, vec![1.0, 2.0]),
            ("second", 1, true, vec![2.0, 1.0]),
        ] {
            let mut client = Client::from_stream(connect(&address)?, idle_seconds.try_into()?)?;
            assert_eq!(client.identity(), "test");
            assert_eq!(client.minimum_similarity(), Some(0.3));
            assert_eq!(client.provider(), "CPUExecutionProvider");
            assert_eq!(client.reused(), reused);
            assert_eq!(
                client.encode(InputKind::Query, &[query.into()])?,
                [expected]
            );
            assert!(client.uses_cpu());
        }
        shutdown.recv_timeout(Duration::from_secs(5))??;
        assert_eq!(
            connect(&address).unwrap_err().kind(),
            io::ErrorKind::ConnectionRefused
        );
        Ok(())
    }
}
