use anyhow::{Context, Result, bail, ensure};
use std::{
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

    /// Encode a batch in a shared vector space.
    fn encode(&mut self, kind: InputKind, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

/// Execution backend for the local ONNX worker.
#[derive(Clone, Copy, Debug, Default)]
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

/// ONNX encoder through a local Python worker.
pub struct LocalModel {
    child: Child,
    input: BufWriter<ChildStdin>,
    output: BufReader<ChildStdout>,
    identity: String,
    minimum_similarity: Option<f32>,
}

#[derive(serde::Deserialize)]
struct WorkerReady {
    identity: String,
    minimum_similarity: Option<f32>,
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
        let mut child = Command::new(python)
            .args(["-I", "-u", "-c", include_str!("worker.py")])
            .arg(directory)
            .arg(device)
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
        let mut model = Self {
            child,
            input,
            output,
            identity: String::new(),
            minimum_similarity: None,
        };
        let ready: WorkerReady = model
            .receive()
            .context("load the local model (see README setup)")?;
        ensure!(!ready.identity.is_empty(), "empty model identity");
        model.identity = ready.identity;
        model.minimum_similarity = ready.minimum_similarity;
        Ok(model)
    }

    fn receive<T: serde::de::DeserializeOwned>(&mut self) -> Result<T> {
        let mut line = String::new();
        if self.output.read_line(&mut line)? == 0 {
            bail!("embedding worker closed its output");
        }
        serde_json::from_str(&line).context("decode embedding worker response")
    }
}

impl Embedder for LocalModel {
    fn identity(&self) -> &str {
        &self.identity
    }

    fn minimum_similarity(&self) -> Option<f32> {
        self.minimum_similarity
    }

    fn encode(&mut self, kind: InputKind, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let prefix = match kind {
            InputKind::Query => "query: ",
            InputKind::Passage => "passage: ",
        };
        serde_json::to_writer(&mut self.input, &(prefix, texts))?;
        self.input.write_all(b"\n")?;
        self.input.flush()?;
        self.receive()
    }
}

impl Drop for LocalModel {
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
