use anyhow::{Context, Result, bail, ensure};
use std::{
    ffi::OsStr,
    io::{BufRead, BufReader, BufWriter, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

/// Distinguishes a user's query from content to be indexed.
#[derive(Debug, Clone, Copy)]
pub enum InputKind {
    Query,
    Passage,
}

/// A local encoder. Return one vector per input, in input order.
pub trait Embedder {
    /// Stable identity including weights, tokenizer and encoding recipe.
    fn identity(&self) -> &str;

    /// Minimum cosine similarity for semantic matches, when calibrated for this encoder.
    fn minimum_similarity(&self) -> Option<f32> {
        None
    }

    /// Whether this encoder accepts image bytes in the same space as text.
    fn supports_images(&self) -> bool {
        false
    }

    /// Encode a supported image, or return None for an unreadable image.
    fn encode_image(&mut self, _image: &[u8]) -> Result<Option<Vec<f32>>> {
        bail!("this embedding model cannot encode images")
    }

    /// Encode a batch in a shared vector space.
    fn encode(&mut self, kind: InputKind, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

/// Execution backend for the local ONNX worker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExecutionDevice {
    /// Prefer CUDA, then MIGraphX, then the CPU.
    #[default]
    Auto,
    /// Run inference on the CPU.
    Cpu,
    /// Run inference with the NVIDIA CUDA execution provider.
    Cuda,
    /// Run inference with the AMD MIGraphX execution provider.
    Migraphx,
}

const GEMMA_FILES: &[(&str, bool)] = &[
    ("model.safetensors", true),
    ("tokenizer.json", true),
    ("config.json", true),
    ("processor_config.json", true),
    ("preprocessor_config.json", true),
    ("tokenizer_config.json", true),
    ("tokenizer.model", true),
    ("chat_template.jinja", true),
];

/// Local encoder through a Python worker.
pub struct LocalModel {
    worker: PythonWorker,
    identity: String,
    minimum_similarity: Option<f32>,
    initial_provider: String,
    uses_cpu: bool,
    direct_images: bool,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum WorkerReady {
    Ready {
        identity: String,
        minimum_similarity: Option<f32>,
        provider: String,
    },
    Failed {
        error: String,
    },
}

#[derive(serde::Deserialize)]
struct WorkerEmbeddings {
    vectors: Vec<Vec<f32>>,
    uses_cpu: bool,
}

impl LocalModel {
    /// Start a worker with a local directory containing model.onnx and tokenizer.json.
    pub fn start(python: &Path, directory: &Path) -> Result<Self> {
        Self::start_on(python, directory, ExecutionDevice::Auto)
    }

    /// Start a worker on the requested execution backend.
    pub fn start_on(python: &Path, directory: &Path, device: ExecutionDevice) -> Result<Self> {
        let device = match device {
            ExecutionDevice::Auto => "auto",
            ExecutionDevice::Cpu => "cpu",
            ExecutionDevice::Cuda => "cuda",
            ExecutionDevice::Migraphx => "migraphx",
        };
        Self::start_worker(python, directory, device)
    }

    /// Require the selected GPU at startup and retry failed inference on the CPU.
    pub fn start_gpu_with_fallback(
        python: &Path,
        directory: &Path,
        device: ExecutionDevice,
    ) -> Result<Self> {
        let device = match device {
            ExecutionDevice::Cuda => "cuda-auto",
            ExecutionDevice::Migraphx => "migraphx-auto",
            _ => bail!("a GPU backend is required"),
        };
        Self::start_worker(python, directory, device)
    }

    /// Whether the checkpoint uses the PyTorch multimodal worker.
    pub fn is_multimodal_checkpoint(directory: &Path) -> bool {
        directory.join("model.safetensors").is_file() && !directory.join("model.onnx").is_file()
    }

    fn start_worker(python: &Path, directory: &Path, device: &str) -> Result<Self> {
        let direct_images = Self::is_multimodal_checkpoint(directory);
        let script = if direct_images {
            for (name, _) in GEMMA_FILES {
                ensure!(
                    directory.join(name).is_file(),
                    "missing EmbeddingGemma 2 artifact {}",
                    directory.join(name).display()
                );
            }
            include_str!("gemma2_worker.py")
        } else {
            include_str!("worker.py")
        };
        // PyTorch uses ROCm directly, whereas ONNX uses the MIGraphX provider.
        let mapped;
        let device = if direct_images && device.starts_with("migraphx") {
            mapped = device.replacen("migraphx", "rocm", 1);
            &mapped
        } else {
            device
        };
        let mut worker =
            PythonWorker::start(python, script, &[directory.as_os_str(), OsStr::new(device)])?;
        let ready: WorkerReady = worker
            .receive()
            .context("load the local model (see README setup)")?;
        let (identity, minimum_similarity, initial_provider) = match ready {
            WorkerReady::Ready {
                identity,
                minimum_similarity,
                provider,
            } => (identity, minimum_similarity, provider),
            WorkerReady::Failed { error } => bail!("{error}"),
        };
        ensure!(!identity.is_empty(), "empty model identity");
        Ok(Self {
            worker,
            identity,
            minimum_similarity,
            uses_cpu: matches!(
                initial_provider.as_str(),
                "CPUExecutionProvider" | "TorchCPUProvider"
            ),
            direct_images,
            initial_provider,
        })
    }

    /// ONNX Runtime provider selected at worker startup.
    pub fn initial_provider(&self) -> &str {
        &self.initial_provider
    }

    /// Whether the worker currently uses the CPU as its primary provider.
    pub fn uses_cpu(&self) -> bool {
        self.uses_cpu
    }
}

pub(crate) struct PythonWorker {
    child: Child,
    pub input: BufWriter<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl PythonWorker {
    pub fn start(python: &Path, script: &str, args: &[&OsStr]) -> Result<Self> {
        let mut child = Command::new(python)
            .args(["-I", "-u", "-c", script])
            .args(args)
            .env("RAYON_NUM_THREADS", "1")
            .env("OPENBLAS_NUM_THREADS", "1")
            .env("OMP_NUM_THREADS", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("start Python interpreter {}", python.display()))?;
        let input = BufWriter::new(child.stdin.take().expect("piped stdin"));
        let output = BufReader::new(child.stdout.take().expect("piped stdout"));
        Ok(Self {
            child,
            input,
            output,
        })
    }

    pub fn receive<T: serde::de::DeserializeOwned>(&mut self) -> Result<T> {
        let mut line = String::new();
        if self.output.read_line(&mut line)? == 0 {
            bail!("model worker closed its output");
        }
        serde_json::from_str(&line).context("decode model worker response")
    }
}

impl Embedder for LocalModel {
    fn identity(&self) -> &str {
        &self.identity
    }

    fn minimum_similarity(&self) -> Option<f32> {
        self.minimum_similarity
    }

    fn supports_images(&self) -> bool {
        self.direct_images
    }

    fn encode_image(&mut self, image: &[u8]) -> Result<Option<Vec<f32>>> {
        ensure!(
            self.direct_images,
            "this embedding model cannot encode images"
        );
        serde_json::to_writer(&mut self.worker.input, &("image", image.len()))?;
        self.worker.input.write_all(b"\n")?;
        self.worker.input.write_all(image)?;
        self.worker.input.flush()?;
        let mut result: WorkerEmbeddings = self.worker.receive().context("encode image")?;
        self.uses_cpu = result.uses_cpu;
        ensure!(
            result.vectors.len() <= 1,
            "unexpected number of image embeddings"
        );
        Ok(result.vectors.pop())
    }

    fn encode(&mut self, kind: InputKind, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let prefix = match kind {
            InputKind::Query => "query: ",
            InputKind::Passage => "passage: ",
        };
        serde_json::to_writer(&mut self.worker.input, &(prefix, texts))?;
        self.worker.input.write_all(b"\n")?;
        self.worker.input.flush()?;
        let result: WorkerEmbeddings = self.worker.receive()?;
        self.uses_cpu = result.uses_cpu;
        Ok(result.vectors)
    }
}

impl Drop for PythonWorker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(crate) fn normalized(mut vector: Vec<f32>) -> Result<Vec<f32>> {
    ensure!(
        !vector.is_empty() && vector.len() <= 4096,
        "invalid embedding dimensions"
    );
    let norm = vector
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt();
    ensure!(norm.is_finite() && norm > 0.0, "invalid embedding norm");
    for value in &mut vector {
        *value = (f64::from(*value) / norm) as f32;
    }
    Ok(vector)
}

/// Identify the current local artifacts and encoding recipe without loading the model.
#[cfg(target_os = "linux")]
pub fn local_model_revision(directory: &Path) -> Option<[u8; 32]> {
    use sha2::{Digest, Sha256};
    use std::{fs, io, os::linux::fs::MetadataExt};

    let mut hash = Sha256::new();
    let files = if LocalModel::is_multimodal_checkpoint(directory) {
        hash.update(include_bytes!("gemma2_worker.py"));
        GEMMA_FILES
    } else {
        hash.update(include_bytes!("worker.py"));
        &[
            ("model.onnx", true),
            ("tokenizer.json", true),
            ("model.onnx_data", false),
            ("model_quantized.onnx_data", false),
            ("semfind-model.json", false),
        ]
    };
    for &(name, required) in files {
        let metadata = match fs::metadata(directory.join(name)) {
            Ok(metadata) if metadata.is_file() => metadata,
            Err(error) if !required && error.kind() == io::ErrorKind::NotFound => {
                hash.update([0]);
                continue;
            }
            _ => return None,
        };
        hash.update([1]);
        hash.update(metadata.st_dev().to_le_bytes());
        hash.update(metadata.st_ino().to_le_bytes());
        hash.update(metadata.st_size().to_le_bytes());
        hash.update(metadata.st_mtime().to_le_bytes());
        hash.update(metadata.st_mtime_nsec().to_le_bytes());
        hash.update(metadata.st_ctime().to_le_bytes());
        hash.update(metadata.st_ctime_nsec().to_le_bytes());
    }
    Some(hash.finalize().into())
}
