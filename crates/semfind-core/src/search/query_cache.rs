use super::QueryEmbedding;
use crate::index::model_identity;
use crate::model::local_model_revision;
use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

const MAX_CACHE_BYTES: u64 = 128 * 1024;

#[derive(Deserialize, Serialize)]
struct CacheEntry<V> {
    key: [u8; 32],
    vector: V,
    minimum_similarity: Option<f32>,
}

pub(super) struct QueryCache<'a> {
    path: PathBuf,
    model_directory: &'a Path,
}

impl<'a> QueryCache<'a> {
    pub fn new(conn: &Connection, model_directory: &'a Path) -> Option<Self> {
        let path = Path::new(conn.path()?).parent()?.join("last-query.json");
        Some(Self {
            path,
            model_directory,
        })
    }

    pub fn key(&self, conn: &Connection, query: &str) -> Result<Option<[u8; 32]>> {
        let Some(identity) = model_identity(conn)? else {
            return Ok(None);
        };
        let Some(revision) = local_model_revision(self.model_directory) else {
            return Ok(None);
        };
        let mut hash = Sha256::new();
        hash.update(b"semfind-query-cache-v2\0");
        hash.update(revision);
        hash.update(identity);
        hash.update(b"\0");
        hash.update(query);
        Ok(Some(hash.finalize().into()))
    }

    pub fn load(&self, conn: &Connection, key: [u8; 32]) -> Option<QueryEmbedding> {
        let metadata = fs::symlink_metadata(&self.path).ok()?;
        if !metadata.is_file() || metadata.len() > MAX_CACHE_BYTES {
            return None;
        }
        let file = File::open(&self.path).ok()?;
        let entry: CacheEntry<Vec<f32>> =
            serde_json::from_reader(file.take(MAX_CACHE_BYTES)).ok()?;
        if entry.key != key
            || !(1..=4096).contains(&entry.vector.len())
            || entry.vector.iter().any(|value| !value.is_finite())
            || entry
                .minimum_similarity
                .is_some_and(|score| !(-1.0..=1.0).contains(&score))
        {
            return None;
        }
        let dimensions: String = conn
            .query_row(
                "SELECT value FROM metadata WHERE key = 'dimensions'",
                [],
                |row| row.get(0),
            )
            .ok()?;
        if dimensions.parse::<usize>().ok()? != entry.vector.len() {
            return None;
        }
        Some(QueryEmbedding {
            vector: entry.vector,
            minimum_similarity: entry.minimum_similarity,
        })
    }

    pub fn store(&self, key: [u8; 32], embedding: &QueryEmbedding) {
        let Some(directory) = self.path.parent() else {
            return;
        };
        let Ok(mut file) = tempfile::NamedTempFile::new_in(directory) else {
            return;
        };
        let entry = CacheEntry {
            key,
            vector: &embedding.vector,
            minimum_similarity: embedding.minimum_similarity,
        };
        if serde_json::to_writer(file.as_file_mut(), &entry).is_ok() {
            let _ = file.persist(&self.path);
        }
    }
}
