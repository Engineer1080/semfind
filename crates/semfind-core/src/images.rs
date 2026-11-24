use crate::model::PythonWorker;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
};

const GPU_BATCH_SIZE: usize = 8;

const FILES: &[&str] = &[
    "added_tokens.json",
    "config.json",
    "generation_config.json",
    "merges.txt",
    "model.safetensors",
    "preprocessor_config.json",
    "processor_config.json",
    "special_tokens_map.json",
    "tokenizer.json",
    "tokenizer_config.json",
    "vocab.json",
];

/// A local image-to-text encoder. Unreadable images can return `None`.
pub trait ImageCaptioner {
    /// Stable identity of the checkpoint and caption recipe.
    fn identity(&self) -> &str;

    /// Describe one encoded image, or return `None` when it cannot be decoded or described.
    fn caption(&mut self, image: &[u8]) -> Result<Option<String>>;

    /// Preferred number of images per request.
    fn batch_size(&self) -> usize {
        1
    }

    /// Return one result per image, in input order.
    fn captions(&mut self, images: &[&[u8]]) -> Result<Vec<Option<String>>> {
        images.iter().map(|image| self.caption(image)).collect()
    }
}

/// Local Florence-2 caption generation. The worker is started on the first image.
pub struct FlorenceCaptioner<'a> {
    python: &'a Path,
    directory: &'a Path,
    runtime_dir: Option<&'a Path>,
    allow_gpu: bool,
    identity: String,
    worker: Option<CaptionWorker>,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Backend {
    Cpu,
    Cuda,
    Rocm,
}

impl Backend {
    fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Cuda => "CUDA",
            Self::Rocm => "ROCm",
        }
    }
}

struct CaptionWorker {
    process: PythonWorker,
    python: PathBuf,
    backend: Backend,
}

impl CaptionWorker {
    fn start(python: &Path, directory: &Path, device: &str) -> Result<Self> {
        let mut process = PythonWorker::start(
            python,
            include_str!("caption_worker.py"),
            &[directory.as_os_str(), OsStr::new(device)],
        )?;
        let backend = process.receive().with_context(|| {
            format!(
                "load Florence-2 ({device}) with {} (see README setup)",
                python.display()
            )
        })?;
        Ok(Self {
            process,
            python: python.to_owned(),
            backend,
        })
    }

    fn captions(&mut self, images: &[&[u8]]) -> Result<Vec<Option<String>>> {
        let input = &mut self.process.input;
        input.write_all(&[u8::try_from(images.len())?])?;
        for image in images {
            input.write_all(&u32::try_from(image.len())?.to_le_bytes())?;
            input.write_all(image)?;
        }
        input.flush()?;
        let captions: Vec<Option<String>> = self.process.receive()?;
        ensure!(
            captions.len() == images.len(),
            "unexpected number of captions"
        );
        Ok(captions)
    }
}

impl<'a> FlorenceCaptioner<'a> {
    /// Whether the expected checkpoint files are present without loading the model.
    pub fn files_present(directory: &Path) -> bool {
        FILES.iter().all(|name| directory.join(name).is_file())
    }

    /// Validate local model files and auto-select a device in the given Python runtime.
    pub fn new(python: &'a Path, directory: &'a Path) -> Result<Self> {
        Self::with_runtime(python, directory, None, true)
    }

    /// Prefer installed Florence GPU runtimes when GPU inference is enabled.
    pub fn with_runtime(
        python: &'a Path,
        directory: &'a Path,
        runtime_dir: Option<&'a Path>,
        allow_gpu: bool,
    ) -> Result<Self> {
        let mut digest = Sha256::new();
        for name in FILES {
            let path = directory.join(name);
            let mut file = File::open(&path).with_context(|| {
                format!(
                    "open Florence-2 artifact {} (see README setup)",
                    path.display()
                )
            })?;
            // Hash each file separately so boundaries are unambiguous.
            let mut hash = Sha256::new();
            io::copy(&mut file, &mut hash)?;
            digest.update(hash.finalize());
        }
        Ok(Self {
            python,
            directory,
            runtime_dir,
            allow_gpu,
            identity: format!(
                "florence-2-more-detailed-caption-v1:{:x}",
                digest.finalize()
            ),
            worker: None,
        })
    }

    /// Device used by the current worker, once captioning has started.
    pub fn backend(&self) -> Option<&'static str> {
        self.worker.as_ref().map(|worker| worker.backend.label())
    }

    fn gpu_runtimes(&self) -> impl Iterator<Item = (PathBuf, &'static str)> + '_ {
        self.runtime_dir.into_iter().flat_map(|root| {
            ["cuda", "rocm"].into_iter().filter_map(move |device| {
                let python = root.join(format!("florence-{device}/bin/python"));
                python.is_file().then_some((python, device))
            })
        })
    }

    fn start_worker(&self) -> Result<CaptionWorker> {
        if self.allow_gpu {
            for (python, device) in self.gpu_runtimes() {
                match CaptionWorker::start(&python, self.directory, device) {
                    Ok(worker) => return Ok(worker),
                    Err(error) => eprintln!("semfind: {error:#}"),
                }
            }
            match CaptionWorker::start(self.python, self.directory, "auto") {
                Ok(worker) => return Ok(worker),
                Err(error) => eprintln!("semfind: {error:#}. Retrying Florence-2 on the CPU."),
            }
        }
        self.start_cpu(self.python)
    }

    fn start_cpu(&self, preferred: &Path) -> Result<CaptionWorker> {
        let mut result = CaptionWorker::start(preferred, self.directory, "cpu");
        if result.is_err() && preferred != self.python {
            result = CaptionWorker::start(self.python, self.directory, "cpu");
        }
        let cpu = self
            .runtime_dir
            .map(|root| root.join("florence-cpu/bin/python"));
        if result.is_err()
            && let Some(python) =
                cpu.filter(|path| path.is_file() && path != preferred && path != self.python)
        {
            result = CaptionWorker::start(&python, self.directory, "cpu");
        }
        for (python, _) in self.gpu_runtimes() {
            if result.is_ok() {
                break;
            }
            if python != preferred && python != self.python {
                result = CaptionWorker::start(&python, self.directory, "cpu");
            }
        }
        result
    }
}

impl ImageCaptioner for FlorenceCaptioner<'_> {
    fn identity(&self) -> &str {
        &self.identity
    }

    fn caption(&mut self, image: &[u8]) -> Result<Option<String>> {
        Ok(self.captions(&[image])?.pop().expect("one caption"))
    }

    fn batch_size(&self) -> usize {
        if self.allow_gpu
            && self
                .worker
                .as_ref()
                .is_none_or(|worker| worker.backend != Backend::Cpu)
        {
            GPU_BATCH_SIZE
        } else {
            1
        }
    }

    fn captions(&mut self, images: &[&[u8]]) -> Result<Vec<Option<String>>> {
        if images.is_empty() {
            return Ok(Vec::new());
        }
        ensure!(images.len() <= GPU_BATCH_SIZE, "invalid image batch size");
        if self.worker.is_none() {
            self.worker = Some(self.start_worker()?);
        }
        let worker = self.worker.as_mut().expect("caption worker started");
        let result = worker.captions(images);
        if result.is_ok() || worker.backend == Backend::Cpu {
            return result;
        }
        eprintln!(
            "semfind: Florence-2 {} failed. Retrying captioning on the CPU.",
            worker.backend.label()
        );
        let CaptionWorker {
            python, process, ..
        } = self.worker.take().expect("failed caption worker");
        drop(process);
        self.allow_gpu = false;
        let worker = self.start_cpu(&python)?;
        self.worker.insert(worker).captions(images)
    }
}

pub(crate) fn supports_caption(extension: &str) -> bool {
    matches!(extension, "jpg" | "jpeg" | "png" | "webp")
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn retries_failed_gpu_caption_on_cpu_and_keeps_processing() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let runtime = temp.path().join("runtime");
        let model = temp.path().join("model");
        fs::create_dir(&model)?;
        for artifact in FILES {
            fs::write(model.join(artifact), "")?;
        }
        let interpreter = runtime.join("florence-cuda/bin/python");
        fs::create_dir_all(interpreter.parent().unwrap())?;
        fs::write(
            &interpreter,
            r#"#!/usr/bin/python3
import json
import sys
device = sys.argv[6]
print(json.dumps(device), flush=True)
stream = sys.stdin.buffer
while header := stream.read(1):
    images = [stream.read(int.from_bytes(stream.read(4), "little")) for _ in range(header[0])]
    if device == "cuda":
        sys.exit(1)
    print(json.dumps([image.decode() + " from the CPU" for image in images]), flush=True)
"#,
        )?;
        fs::set_permissions(&interpreter, fs::Permissions::from_mode(0o700))?;
        let configured_python = temp.path().join("unused-python");
        let mut captioner =
            FlorenceCaptioner::with_runtime(&configured_python, &model, Some(&runtime), true)?;
        assert_eq!(captioner.backend(), None);
        assert_eq!(captioner.batch_size(), 8);
        assert_eq!(
            captioner.captions(&[b"one", b"two"])?,
            vec![
                Some("one from the CPU".into()),
                Some("two from the CPU".into())
            ]
        );
        assert_eq!(captioner.backend(), Some("CPU"));
        assert_eq!(captioner.batch_size(), 1);
        assert_eq!(
            captioner.caption(b"three")?.as_deref(),
            Some("three from the CPU")
        );
        Ok(())
    }
}
