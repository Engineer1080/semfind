use crate::model::PythonWorker;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Write},
    path::Path,
};

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
}

/// Local Florence-2 Base caption generation. The worker is started on the first image.
pub struct FlorenceCaptioner<'a> {
    python: &'a Path,
    directory: &'a Path,
    identity: String,
    worker: Option<PythonWorker>,
}

impl<'a> FlorenceCaptioner<'a> {
    /// Validate local model files and fingerprint the caption recipe.
    pub fn new(python: &'a Path, directory: &'a Path) -> Result<Self> {
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
            identity: format!("florence-2-detailed-caption-v1:{:x}", digest.finalize()),
            worker: None,
        })
    }
}

impl ImageCaptioner for FlorenceCaptioner<'_> {
    fn identity(&self) -> &str {
        &self.identity
    }

    fn caption(&mut self, image: &[u8]) -> Result<Option<String>> {
        let worker = match &mut self.worker {
            Some(worker) => worker,
            None => {
                let mut worker = PythonWorker::start(
                    self.python,
                    include_str!("caption_worker.py"),
                    &[self.directory.as_os_str()],
                )?;
                let ready: bool = worker
                    .receive()
                    .context("load Florence-2 (see README setup)")?;
                ensure!(ready, "Florence-2 worker did not become ready");
                self.worker.insert(worker)
            }
        };
        worker
            .input
            .write_all(&u32::try_from(image.len())?.to_le_bytes())?;
        worker.input.write_all(image)?;
        worker.input.flush()?;
        worker.receive()
    }
}

pub(crate) fn supports_caption(extension: &str) -> bool {
    matches!(extension, "jpg" | "jpeg" | "png" | "webp")
}
