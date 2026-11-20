//! Persistent, local hybrid search across documents and desktop settings.
mod applications;
mod catalog;
mod commands;
mod files;
mod index;
mod locale;
mod manuals;
mod model;
mod search;
mod translations;

pub use commands::Distro;
pub use files::FileKind;
pub use index::{Index, IndexOptions, IndexReport, Status};
pub use locale::Language;
pub use model::{Embedder, ExecutionDevice, InputKind, LocalModel};
pub use search::{Category, Hit, SearchOptions};
