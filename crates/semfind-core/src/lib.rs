//! Persistent, local hybrid search across documents and desktop settings.
mod catalog;
mod files;
mod index;
mod model;
mod search;

pub use index::{Index, IndexOptions, IndexReport, Status};
pub use model::{Embedder, InputKind, LocalModel};
pub use search::{Category, Hit, SearchOptions};
