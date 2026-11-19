use crate::{
    catalog::SETTINGS,
    files::{self, FileKind},
    model::{Embedder, InputKind, normalized},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use walkdir::WalkDir;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS roots (path TEXT PRIMARY KEY, exclusions TEXT NOT NULL DEFAULT '[]');
CREATE TABLE IF NOT EXISTS sources (
    id INTEGER PRIMARY KEY,
    key TEXT UNIQUE NOT NULL,
    category TEXT NOT NULL CHECK (category IN ('file', 'setting')),
    title TEXT NOT NULL,
    root TEXT REFERENCES roots(path) ON DELETE CASCADE,
    extension TEXT,
    desktop TEXT,
    setting_group TEXT,
    digest BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS sources_root ON sources(root);
CREATE TABLE IF NOT EXISTS chunks (
    id INTEGER PRIMARY KEY,
    source INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    embedding BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS chunks_source ON chunks(source);
CREATE VIRTUAL TABLE IF NOT EXISTS fulltext USING fts5(
    title, body, tokenize='unicode61 remove_diacritics 2'
);
INSERT INTO fulltext(fulltext, rank) VALUES('secure-delete', 1);
CREATE TRIGGER IF NOT EXISTS delete_chunk AFTER DELETE ON chunks BEGIN
    DELETE FROM fulltext WHERE rowid = old.id;
END;
PRAGMA user_version = 2;
";

/// A persistent search index. Use one connection per client or worker.
pub struct Index {
    pub(crate) conn: Connection,
}

/// Controls a manual synchronization of one selected root.
#[derive(Debug, Default)]
pub struct IndexOptions {
    /// Relative paths or single-component names to exclude, in addition to hidden entries.
    /// None retains the root's saved exclusions. Some replaces them.
    pub exclude: Option<Vec<PathBuf>>,
}

/// Counts and warnings for a completed synchronization.
#[derive(Debug, Default, Serialize)]
pub struct IndexReport {
    pub written: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub skipped: usize,
    pub warnings: Vec<String>,
}

/// A snapshot of persisted index contents.
#[derive(Debug, Serialize)]
pub struct Status {
    pub roots: Vec<PathBuf>,
    pub files: i64,
    pub settings: i64,
    pub chunks: i64,
    pub model: Option<String>,
}

impl Index {
    /// Open or create an index in a dedicated, private directory.
    pub fn open(directory: &Path) -> Result<Self> {
        let dir = files::index_directory(directory)?;
        let path = files::index_file(&dir)?;
        let mut conn = Connection::open(&path)?;
        conn.busy_timeout(Duration::from_secs(10))?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA secure_delete = ON; PRAGMA temp_store = MEMORY;",
        )?;
        let version = schema_version(&conn)?;
        match version {
            0 | 1 => {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                // Another process may have migrated the index while we waited for the lock.
                match schema_version(&tx)? {
                    0 => tx.execute_batch(SCHEMA)?,
                    // Version 2 also permits lexical-only chunks with an empty embedding.
                    1 => tx.pragma_update(None, "user_version", 2)?,
                    2 => (),
                    version => anyhow::bail!("unsupported database schema version {version}"),
                }
                tx.commit()?;
            }
            2 => (),
            _ => anyhow::bail!("unsupported database schema version {version}"),
        }
        Ok(Self { conn })
    }

    /// Inspect the index without loading a model.
    pub fn status(&self) -> Result<Status> {
        let tx = self.conn.unchecked_transaction()?;
        let roots = tx
            .prepare("SELECT path FROM roots ORDER BY path")?
            .query_map([], |row| Ok(PathBuf::from(row.get::<_, String>(0)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let (files, settings, chunks) = tx.query_row(
            "SELECT (SELECT count(*) FROM sources WHERE category='file'),
                    (SELECT count(*) FROM sources WHERE category='setting'),
                    (SELECT count(*) FROM chunks WHERE length(embedding) > 0)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let model = model_identity(&tx)?;
        tx.commit()?;
        Ok(Status {
            roots,
            files,
            settings,
            chunks,
            model,
        })
    }

    /// Synchronize the built-in settings catalog and optionally a selected directory.
    /// Successful runs commit both text and vectors atomically.
    pub fn synchronize(
        &mut self,
        root: Option<&Path>,
        options: &IndexOptions,
        model: &mut impl Embedder,
    ) -> Result<IndexReport> {
        ensure!(
            root.is_some() || options.exclude.is_none(),
            "exclusions require a file root"
        );
        for exclude in options.exclude.iter().flatten() {
            ensure!(
                !exclude.as_os_str().is_empty()
                    && exclude
                        .components()
                        .all(|part| matches!(part, std::path::Component::Normal(_))),
                "exclusions must be non-empty relative paths"
            );
        }
        let root = root.map(files::root).transpose()?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        check_model(&tx, model)?;
        tx.execute(
            "INSERT OR IGNORE INTO metadata VALUES ('model', ?1)",
            [model.identity()],
        )?;
        let mut report = IndexReport::default();
        synchronize_catalog(&tx, model, &mut report)?;
        if let Some(root) = root {
            let name = root.to_str().context("index roots must be valid UTF-8")?;
            let existing = tx
                .prepare("SELECT path FROM roots WHERE path != ?1")?
                .query_map([name], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ensure!(
                existing
                    .iter()
                    .all(|p| !root.starts_with(p) && !Path::new(p).starts_with(&root)),
                "index roots must not overlap"
            );
            tx.execute("INSERT OR IGNORE INTO roots(path) VALUES (?1)", [name])?;
            let exclusions = if let Some(exclusions) = &options.exclude {
                tx.execute(
                    "UPDATE roots SET exclusions=?1 WHERE path=?2",
                    params![serde_json::to_string(exclusions)?, name],
                )?;
                exclusions.clone()
            } else {
                let encoded: String = tx.query_row(
                    "SELECT exclusions FROM roots WHERE path=?1",
                    [name],
                    |row| row.get(0),
                )?;
                serde_json::from_str(&encoded)?
            };
            synchronize_files(&tx, &root, name, &exclusions, model, &mut report)?;
        }
        tx.commit()?;
        Ok(report)
    }

    /// Remove one registered root and its indexed data, including when the root is offline.
    pub fn remove_root(&mut self, path: &Path) -> Result<bool> {
        let path: PathBuf = std::path::absolute(path)?.components().collect();
        let path = path.to_str().context("index roots must be valid UTF-8")?;
        Ok(self
            .conn
            .execute("DELETE FROM roots WHERE path = ?1", [path])?
            > 0)
    }
}

fn schema_version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

pub(crate) fn check_model(conn: &Connection, model: &impl Embedder) -> Result<()> {
    ensure!(!model.identity().is_empty(), "model identity is empty");
    if let Some(identity) = model_identity(conn)? {
        ensure!(
            identity == model.identity(),
            "index uses a different embedding model (use a new index directory or rebuild)"
        );
    }
    Ok(())
}

fn model_identity(conn: &Connection) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT value FROM metadata WHERE key = 'model'",
            [],
            |row| row.get(0),
        )
        .optional()?)
}

struct Source<'a> {
    key: &'a str,
    category: &'static str,
    title: &'a str,
    root: Option<&'a str>,
    extension: Option<&'a str>,
    desktop: Option<&'static str>,
    setting_group: Option<&'static str>,
}

fn source_state(conn: &Connection, key: &str) -> Result<Option<(i64, Vec<u8>)>> {
    Ok(conn
        .query_row(
            "SELECT id, digest FROM sources WHERE key = ?1",
            [key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

fn write_source(
    tx: &Transaction<'_>,
    source: Source<'_>,
    text: Option<&str>,
    digest: &[u8],
    model: &mut impl Embedder,
) -> Result<i64> {
    let id: i64 = tx.query_row(
        "INSERT INTO sources(key,category,title,root,extension,desktop,setting_group,digest)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(key) DO UPDATE SET title=excluded.title, extension=excluded.extension,
           setting_group=excluded.setting_group, digest=excluded.digest RETURNING id",
        params![
            source.key,
            source.category,
            source.title,
            source.root,
            source.extension,
            source.desktop,
            source.setting_group,
            digest
        ],
        |row| row.get(0),
    )?;
    tx.execute("DELETE FROM chunks WHERE source = ?1", [id])?;
    let Some(text) = text else {
        store_chunk(tx, id, &source, "", &[])?;
        return Ok(id);
    };
    let mut parts = files::chunks(text).peekable();
    if parts.peek().is_none() {
        write_chunks(tx, id, &source, &[""], model)?;
    }
    loop {
        let batch: Vec<_> = parts.by_ref().take(8).collect();
        if batch.is_empty() {
            break;
        }
        write_chunks(tx, id, &source, &batch, model)?;
    }
    Ok(id)
}

fn write_chunks(
    tx: &Transaction<'_>,
    id: i64,
    source: &Source<'_>,
    parts: &[&str],
    model: &mut impl Embedder,
) -> Result<()> {
    let title: String = source.title.chars().take(256).collect();
    let input: Vec<_> = parts
        .iter()
        .map(|part| format!("{title}\n{part}"))
        .collect();
    let vectors = model
        .encode(InputKind::Passage, &input)
        .context("encode document passages")?;
    ensure!(
        vectors.len() == parts.len(),
        "unexpected number of embeddings"
    );
    for (text, vector) in parts.iter().zip(vectors) {
        let vector = normalized(vector)?;
        let width = vector.len();
        tx.execute(
            "INSERT OR IGNORE INTO metadata VALUES ('dimensions', ?1)",
            [width.to_string()],
        )?;
        let declared: String = tx.query_row(
            "SELECT value FROM metadata WHERE key='dimensions'",
            [],
            |row| row.get(0),
        )?;
        ensure!(
            declared.parse::<usize>()? == width,
            "inconsistent embedding dimensions"
        );
        let bytes: Vec<_> = vector.into_iter().flat_map(f32::to_le_bytes).collect();
        store_chunk(tx, id, source, text, &bytes)?;
    }
    Ok(())
}

fn store_chunk(
    tx: &Transaction<'_>,
    id: i64,
    source: &Source<'_>,
    text: &str,
    embedding: &[u8],
) -> Result<()> {
    tx.execute(
        "INSERT INTO chunks(source, embedding) VALUES (?1, ?2)",
        params![id, embedding],
    )?;
    tx.execute(
        "INSERT INTO fulltext(rowid, title, body) VALUES (?1, ?2, ?3)",
        params![
            tx.last_insert_rowid(),
            format!("{} {}", source.title, source.key),
            text
        ],
    )?;
    Ok(())
}

fn synchronize_catalog(
    tx: &Transaction<'_>,
    model: &mut impl Embedder,
    report: &mut IndexReport,
) -> Result<()> {
    let mut keep = Vec::new();
    for setting in SETTINGS {
        let text = format!("{} {}", setting.description, setting.synonyms);
        let digest = Sha256::digest(format!("v1\n{}\n{}\n{text}", setting.title, setting.group));
        let stored = source_state(tx, setting.id)?;
        if let Some((id, _)) = stored.filter(|(_, saved)| saved == digest.as_slice()) {
            keep.push(id);
            report.unchanged += 1;
            continue;
        }
        keep.push(write_source(
            tx,
            Source {
                key: setting.id,
                category: "setting",
                title: setting.title,
                root: None,
                extension: None,
                desktop: Some("cosmic"),
                setting_group: Some(setting.group),
            },
            Some(&text),
            &digest,
            model,
        )?);
        report.written += 1;
    }
    let ids = tx
        .prepare("SELECT id FROM sources WHERE category = 'setting'")?
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in ids.into_iter().filter(|id| !keep.contains(id)) {
        report.removed += tx.execute("DELETE FROM sources WHERE id = ?1", [id])?;
    }
    Ok(())
}

fn synchronize_files(
    tx: &Transaction<'_>,
    root: &Path,
    root_name: &str,
    exclusions: &[PathBuf],
    model: &mut impl Embedder,
    report: &mut IndexReport,
) -> Result<()> {
    tx.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS seen (id INTEGER PRIMARY KEY); DELETE FROM seen;",
    )?;
    let walker = WalkDir::new(root)
        .follow_links(false)
        .follow_root_links(false)
        .same_file_system(true)
        .into_iter()
        .filter_entry(|entry| {
            let relative = entry
                .path()
                .strip_prefix(root)
                .expect("entry is under root");
            files::allowed(entry.path())
                && (relative.as_os_str().is_empty()
                    || !exclusions.iter().any(|rule| {
                        if rule.components().count() == 1 {
                            entry.file_name() == rule.as_os_str()
                        } else {
                            relative.starts_with(rule)
                        }
                    }))
        });
    for entry in walker {
        let entry = entry.context("scan directory (changes rolled back)")?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Some(extension) = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
        else {
            continue;
        };
        let Some(kind) = FileKind::from_extension(&extension) else {
            continue;
        };
        let key = path.to_str().context("file paths must be valid UTF-8")?;
        let title = entry
            .file_name()
            .to_str()
            .context("file names must be valid UTF-8")?;
        let bytes = match files::open_file(root, path, kind).and_then(|file| {
            if kind == FileKind::Text {
                files::read(file).map(Some)
            } else {
                Ok(None)
            }
        }) {
            Ok(bytes) => bytes,
            Err(error) => {
                warn(report, path, &error);
                continue;
            }
        };
        let digest = match &bytes {
            Some(bytes) => Sha256::new()
                .chain_update(b"extract-v1\0")
                .chain_update(bytes)
                .finalize(),
            None => Sha256::digest(b"file-name-v1"),
        };
        let stored = source_state(tx, key)?;
        let id = if let Some((id, _)) = stored.filter(|(_, saved)| saved == digest.as_slice()) {
            report.unchanged += 1;
            id
        } else {
            let text = match bytes
                .map(|bytes| files::extract(&extension, bytes))
                .transpose()
            {
                Ok(text) => text,
                Err(error) => {
                    warn(report, path, &error);
                    continue;
                }
            };
            let id = write_source(
                tx,
                Source {
                    key,
                    category: "file",
                    title,
                    root: Some(root_name),
                    extension: Some(&extension),
                    desktop: None,
                    setting_group: None,
                },
                text.as_deref(),
                &digest,
                model,
            )?;
            report.written += 1;
            id
        };
        tx.execute("INSERT INTO seen VALUES (?1)", [id])?;
    }
    report.removed += tx.execute(
        "DELETE FROM sources WHERE root = ?1 AND id NOT IN (SELECT id FROM seen)",
        [root_name],
    )?;
    Ok(())
}

fn warn(report: &mut IndexReport, path: &Path, error: &anyhow::Error) {
    report.skipped += 1;
    if report.warnings.len() < 20 {
        report
            .warnings
            .push(format!("{}: {error:#}", path.display()));
    }
}
