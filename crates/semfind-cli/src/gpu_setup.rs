use crate::ui::Messages;
use anyhow::{Context, Result, ensure};
use clap::ValueEnum;
use std::{
    ffi::OsStr,
    fs,
    io::{self, IsTerminal, Write},
    os::unix::fs::symlink,
    path::Path,
    process::Command,
};

const AMD_WHEEL: &str = concat!(
    "https://repo.radeon.com/rocm/manylinux/rocm-rel-7.2.1/",
    "onnxruntime_migraphx-1.23.2-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl"
);

#[derive(Clone, Copy, ValueEnum)]
pub(super) enum Backend {
    Cuda,
    Migraphx,
}

impl Backend {
    fn runtime(self) -> &'static str {
        match self {
            Self::Cuda => "cuda",
            Self::Migraphx => "migraphx",
        }
    }

    fn provider(self) -> &'static str {
        match self {
            Self::Cuda => "CUDAExecutionProvider",
            Self::Migraphx => "MIGraphXExecutionProvider",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Cuda => "NVIDIA CUDA",
            Self::Migraphx => "AMD MIGraphX",
        }
    }
}

fn missing_backend(root: &Path, devices: &Path) -> Option<Backend> {
    let missing = |backend: Backend| {
        fs::symlink_metadata(root.join(backend.runtime()))
            .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
    };
    let mut amd = false;
    for device in fs::read_dir(devices).ok()?.flatten() {
        let path = device.path();
        if !fs::read_to_string(path.join("class")).is_ok_and(|class| class.starts_with("0x03")) {
            continue;
        }
        let Ok(vendor) = fs::read_to_string(path.join("vendor")) else {
            continue;
        };
        let driver = fs::read_link(path.join("driver")).ok();
        let driver = driver
            .as_deref()
            .and_then(Path::file_name)
            .and_then(OsStr::to_str);
        match (vendor.trim(), driver) {
            ("0x10de", Some("nvidia")) if missing(Backend::Cuda) => return Some(Backend::Cuda),
            ("0x1002", Some("amdgpu")) => amd = true,
            _ => {}
        }
    }
    (amd && missing(Backend::Migraphx)).then_some(Backend::Migraphx)
}

pub(super) fn offer_on_init(root: &Path, python: &Path, messages: &Messages) -> bool {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return false;
    }
    let Some(backend) = missing_backend(root, Path::new("/sys/bus/pci/devices")) else {
        return false;
    };
    let mut stderr = io::stderr();
    let prompt = messages.gpu_offer.replace("{backend}", backend.label());
    let mut answer = String::new();
    if write!(stderr, "semfind: {prompt} ").is_err()
        || stderr.flush().is_err()
        || io::stdin().read_line(&mut answer).is_err()
        || !matches!(
            answer.trim().to_lowercase().as_str(),
            "y" | "yes" | "j" | "ja" | "s" | "si" | "sí" | "sim" | "o" | "oui" | "д" | "да"
        )
    {
        return false;
    }
    match install(root, python, backend, None, &mut stderr) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("semfind: {error:#}\nsemfind: {}", messages.gpu_offer_failed);
            false
        }
    }
}

pub(super) fn install(
    root: &Path,
    python: &Path,
    backend: Backend,
    wheel: Option<&Path>,
    output: &mut impl Write,
) -> Result<()> {
    let destination = root.join(backend.runtime());
    match fs::symlink_metadata(&destination) {
        Ok(_) => {
            verify(&destination.join("bin/python"), backend).with_context(|| {
                format!("check the existing runtime at {}", destination.display())
            })?;
            writeln!(
                output,
                "{} is ready at {}",
                backend.provider(),
                destination.display()
            )?;
            return Ok(());
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("check the GPU runtime directory"),
    }

    let wheel = wheel
        .map(fs::canonicalize)
        .transpose()
        .context("open the MIGraphX wheel")?;
    if let Some(wheel) = &wheel {
        ensure!(
            wheel.is_file() && wheel.extension() == Some(OsStr::new("whl")),
            "provide a local MIGraphX .whl file"
        );
    }
    if matches!(backend, Backend::Migraphx) && wheel.is_none() {
        run(
            Command::new(python).args([
                "-I", "-c", "import platform, sys; sys.exit(sys.version_info[:2] != (3, 12) or platform.machine() != 'x86_64')",
            ]),
            "the bundled MIGraphX selection requires Python 3.12 on x86_64; use --wheel on other setups",
        )?;
    }

    fs::create_dir_all(root).with_context(|| format!("create {}", root.display()))?;
    // Virtual environments record absolute paths. Keep the verified environment where it was built.
    let staged = tempfile::Builder::new()
        .prefix(&format!(".{}-", backend.runtime()))
        .tempdir_in(root)?;
    let interpreter = staged.path().join("bin/python");
    let created = Command::new(python)
        .args(["-I", "-m", "venv"])
        .arg(staged.path())
        .output()
        .with_context(|| format!("start Python interpreter {}", python.display()))?;
    let mut pip = if created.status.success() {
        let mut command = Command::new(&interpreter);
        command.args(["-I", "-m", "pip"]);
        command
    } else {
        eprintln!(
            "semfind: Trying to set up the environment with pip from {}.",
            python.display()
        );
        run(
            Command::new(python)
                .args(["-I", "-m", "venv", "--without-pip"])
                .arg(staged.path()),
            "create the GPU virtual environment",
        )?;
        let mut command = Command::new(python);
        command
            .args(["-I", "-m", "pip", "--python"])
            .arg(&interpreter);
        command
    };
    pip.args([
        "--isolated",
        "--disable-pip-version-check",
        "install",
        "--only-binary=:all:",
        "--index-url",
        "https://pypi.org/simple",
        "tokenizers==0.23.3",
    ]);
    match backend {
        Backend::Cuda => pip.args(["onnxruntime-gpu[cuda,cudnn]==1.32.0", "numpy==2.5.5"]),
        Backend::Migraphx => pip
            .arg(
                wheel
                    .as_deref()
                    .map_or_else(|| OsStr::new(AMD_WHEEL), Path::as_os_str),
            )
            .arg(if wheel.is_some() {
                "numpy>=1.26.4"
            } else {
                "numpy==1.26.4"
            }),
    };
    run(
        &mut pip,
        "install GPU packages (requires Python with pip or python3-venv)",
    )?;
    verify(&interpreter, backend)?;

    let target = staged
        .path()
        .file_name()
        .context("locate the GPU environment")?;
    symlink(target, &destination)
        .with_context(|| format!("register GPU runtime at {}", destination.display()))?;
    let _ = staged.keep();
    writeln!(
        output,
        "{} is ready at {}",
        backend.provider(),
        destination.display()
    )?;
    Ok(())
}

fn run(command: &mut Command, step: &str) -> Result<()> {
    let status = command.status().with_context(|| step.to_owned())?;
    ensure!(status.success(), "{step} failed ({status})");
    Ok(())
}

fn verify(python: &Path, backend: Backend) -> Result<()> {
    run(
        Command::new(python)
            .args(["-I", "-c", include_str!("gpu_probe.py"), backend.provider()])
            .env("ORT_DISABLE_TELEMETRY", "1"),
        &format!("run a GPU inference check with {}", backend.provider()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_only_missing_runtimes_for_bound_gpu_drivers() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let runtime = temp.path().join("runtime");
        let devices = temp.path().join("devices");
        fs::create_dir(&runtime)?;
        let pci = |name: &str, class: &str, vendor: &str, driver: &str| -> Result<()> {
            let path = devices.join(name);
            fs::create_dir_all(&path)?;
            fs::write(path.join("class"), class)?;
            fs::write(path.join("vendor"), vendor)?;
            symlink(Path::new("/drivers").join(driver), path.join("driver"))?;
            Ok(())
        };
        pci("nvidia-network", "0x020000\n", "0x10de\n", "nvidia")?;
        pci("nvidia-other-driver", "0x030000\n", "0x10de\n", "nouveau")?;
        assert!(missing_backend(&runtime, &devices).is_none());
        pci("amd", "0x030000\n", "0x1002\n", "amdgpu")?;
        assert!(matches!(
            missing_backend(&runtime, &devices),
            Some(Backend::Migraphx)
        ));
        pci("nvidia", "0x030200\n", "0x10de\n", "nvidia")?;
        assert!(matches!(
            missing_backend(&runtime, &devices),
            Some(Backend::Cuda)
        ));
        symlink("unavailable-runtime", runtime.join("cuda"))?;
        fs::create_dir(runtime.join("migraphx"))?;
        assert!(missing_backend(&runtime, &devices).is_none());
        Ok(())
    }
}
