//! Persistent, local hybrid search across documents and desktop settings.
mod applications;
mod catalog;
mod commands;
mod files;
mod images;
mod index;
mod locale;
mod manuals;
mod model;
mod search;
mod translations;

pub use commands::Distro;
pub use files::{FileKind, HitLocation};
pub use images::{FlorenceCaptioner, ImageCaptioner};
pub use index::{Index, IndexOptions, IndexReport, InitProgress, Status};
pub use locale::Language;
#[cfg(target_os = "linux")]
pub use model::local_model_revision;
pub use model::{Embedder, ExecutionDevice, InputKind, LocalModel};
pub use search::{Category, Hit, SearchOptions, SimilarityThreshold};
