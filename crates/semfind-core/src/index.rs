use crate::{
    applications::{self, Application},
    catalog::SETTINGS,
    commands::{COMMANDS, Distro},
    files::{self, FileKind, HitLocation, Passage},
    images::{self, ImageCaptioner},
    manuals,
    model::{Embedder, InputKind, normalized},
    translations::catalog_texts,
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

const SOURCE_COLUMNS: &str = "(
    id INTEGER PRIMARY KEY,
    key TEXT UNIQUE NOT NULL,
    category TEXT NOT NULL CHECK (category IN ('file', 'setting', 'application', 'command')),
    title TEXT NOT NULL,
    root TEXT REFERENCES roots(path) ON DELETE CASCADE,
    extension TEXT,
    desktop TEXT,
    setting_group TEXT,
    digest BLOB NOT NULL,
    command_distro TEXT CHECK (command_distro IN ('generic', 'debian', 'arch')),
    command_example TEXT
)";

const SCHEMA: &str = "
CREATE INDEX IF NOT EXISTS sources_root ON sources(root);
CREATE TABLE IF NOT EXISTS chunks (
    id INTEGER PRIMARY KEY,
    source INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    embedding BLOB NOT NULL,
    line_start INTEGER,
    line_end INTEGER,
    page INTEGER
);
CREATE INDEX IF NOT EXISTS chunks_source ON chunks(source);
CREATE VIRTUAL TABLE IF NOT EXISTS fulltext USING fts5(
    title, body, tokenize='unicode61 remove_diacritics 2'
);
INSERT INTO fulltext(fulltext, rank) VALUES('secure-delete', 1);
CREATE TRIGGER IF NOT EXISTS delete_chunk AFTER DELETE ON chunks BEGIN
    DELETE FROM fulltext WHERE rowid = old.id;
END;
PRAGMA user_version = 5;
";
const INIT_COMPLETED_KEY: &str = "init_complete";

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
    pub applications: i64,
    pub commands: i64,
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
            0..=4 => migrate(&mut conn)?,
            5 => (),
            _ => anyhow::bail!("unsupported database schema version {version}"),
        }
        Ok(Self { conn })
    }

    /// Whether `init` has successfully completed for this index.
    pub fn is_initialized(&self) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM metadata WHERE key = ?1)",
            [INIT_COMPLETED_KEY],
            |row| row.get(0),
        )?)
    }

    /// Inspect the index without loading a model.
    pub fn status(&self) -> Result<Status> {
        let tx = self.conn.unchecked_transaction()?;
        let roots = tx
            .prepare("SELECT path FROM roots ORDER BY path")?
            .query_map([], |row| Ok(PathBuf::from(row.get::<_, String>(0)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let (files, settings, applications, commands, chunks) = tx.query_row(
            "SELECT (SELECT count(*) FROM sources WHERE category='file'),
                    (SELECT count(*) FROM sources WHERE category='setting'),
                    (SELECT count(*) FROM sources WHERE category='application'),
                    (SELECT count(*) FROM sources WHERE category='command'),
                    (SELECT count(*) FROM chunks WHERE length(embedding) > 0)",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;
        let model = model_identity(&tx)?;
        tx.commit()?;
        Ok(Status {
            roots,
            files,
            settings,
            applications,
            commands,
            chunks,
            model,
        })
    }

    /// Return the identity of the embedding model that populated this index.
    pub fn embedding_model(&self) -> Result<Option<String>> {
        model_identity(&self.conn)
    }

    /// Synchronize the built-in catalogs and optionally a selected directory.
    /// Successful runs commit both text and vectors atomically.
    pub fn synchronize(
        &mut self,
        root: Option<&Path>,
        options: &IndexOptions,
        model: &mut impl Embedder,
    ) -> Result<IndexReport> {
        self.synchronize_sources(root, options, false, None, model, None)
    }

    /// Refresh the installed command catalog alongside the ordinary index sources.
    pub fn synchronize_with_manpages(
        &mut self,
        root: Option<&Path>,
        options: &IndexOptions,
        distro: Distro,
        model: &mut impl Embedder,
    ) -> Result<IndexReport> {
        ensure!(
            cfg!(target_os = "linux"),
            "index --manpages is supported on Linux"
        );
        self.synchronize_sources(root, options, false, Some(distro), model, None)
    }

    /// Synchronize a selected folder and caption its supported images locally.
    pub fn synchronize_with_image_captions(
        &mut self,
        root: &Path,
        options: &IndexOptions,
        manpages: Option<Distro>,
        model: &mut impl Embedder,
        captioner: &mut dyn ImageCaptioner,
    ) -> Result<IndexReport> {
        ensure!(
            manpages.is_none() || cfg!(target_os = "linux"),
            "index --manpages is supported on Linux"
        );
        self.synchronize_sources(Some(root), options, false, manpages, model, Some(captioner))
    }

    /// Synchronize the home directory, installed desktop applications, local commands and catalogs.
    pub fn initialize_home(
        &mut self,
        home: &Path,
        options: &IndexOptions,
        model: &mut impl Embedder,
    ) -> Result<IndexReport> {
        self.initialize_home_with_image_captions(home, options, model, None)
    }

    /// Initialize the home index, optionally generating captions for supported images.
    /// If caption inference fails, keep existing captions and finish the scan without new ones.
    pub fn initialize_home_with_image_captions(
        &mut self,
        home: &Path,
        options: &IndexOptions,
        model: &mut impl Embedder,
        captioner: Option<&mut dyn ImageCaptioner>,
    ) -> Result<IndexReport> {
        ensure!(cfg!(target_os = "linux"), "init is supported on Linux");
        ensure!(home.is_absolute(), "HOME must be an absolute path");
        self.synchronize_sources(
            Some(home),
            options,
            true,
            Some(Distro::current()),
            model,
            captioner,
        )
    }

    fn synchronize_sources(
        &mut self,
        root: Option<&Path>,
        options: &IndexOptions,
        initializing: bool,
        local_command_distro: Option<Distro>,
        model: &mut impl Embedder,
        captions: Option<&mut dyn ImageCaptioner>,
    ) -> Result<IndexReport> {
        ensure!(
            captions
                .as_ref()
                .is_none_or(|captioner| !captioner.identity().is_empty()),
            "caption model identity is empty"
        );
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
        synchronize_commands(&tx, model, &mut report)?;
        if let Some(root) = root {
            let name = root.to_str().context("index roots must be valid UTF-8")?;
            let existing = tx
                .prepare("SELECT path FROM roots WHERE path != ?1")?
                .query_map([name], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if let Some(conflict) = existing
                .iter()
                .find(|p| root.starts_with(p) || Path::new(p).starts_with(&root))
            {
                anyhow::bail!(
                    "index roots must not overlap: {} conflicts with already indexed {}",
                    root.display(),
                    conflict
                );
            }
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
            let captions = captions.map(|captioner| CaptionScan {
                captioner,
                required: !initializing,
            });
            synchronize_files(&tx, &root, name, &exclusions, model, captions, &mut report)?;
            if initializing {
                let applications = applications::discover(&root)?;
                synchronize_applications(&tx, &applications, model, &mut report)?;
            }
        }
        if let Some(distro) = local_command_distro {
            synchronize_local_commands(&tx, distro, model, &mut report)?;
        }
        if initializing {
            tx.execute(
                "INSERT OR IGNORE INTO metadata (key, value) VALUES (?1, '1')",
                [INIT_COMPLETED_KEY],
            )?;
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

fn migrate(conn: &mut Connection) -> Result<()> {
    // Copy the parent table while preserving the chunks and their FTS rows.
    conn.pragma_update(None, "foreign_keys", false)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // Another process may have migrated the index while we waited for the lock.
    let version = schema_version(&tx)?;
    match version {
        0 => tx.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS roots (path TEXT PRIMARY KEY, exclusions TEXT NOT NULL DEFAULT '[]');
             CREATE TABLE IF NOT EXISTS sources {SOURCE_COLUMNS};
             {SCHEMA}"
        ))?,
        1..=3 => {
            tx.execute_batch(&format!(
                "CREATE TABLE sources_new {SOURCE_COLUMNS};
                 INSERT INTO sources_new (id, key, category, title, root, extension, desktop, setting_group, digest)
                 SELECT id, key, category, title, root, extension, desktop, setting_group, digest FROM sources;
                 DROP TABLE sources;
                 ALTER TABLE sources_new RENAME TO sources;
                 CREATE INDEX sources_root ON sources(root);
                 PRAGMA user_version = 4;"
            ))?;
            let invalid: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM pragma_foreign_key_check)",
                [],
                |row| row.get(0),
            )?;
            ensure!(!invalid, "index contains invalid foreign keys");
        }
        4 | 5 => (),
        version => anyhow::bail!("unsupported database schema version {version}"),
    }
    if (1..=4).contains(&version) {
        tx.execute_batch(
            "ALTER TABLE chunks ADD COLUMN line_start INTEGER;
             ALTER TABLE chunks ADD COLUMN line_end INTEGER;
             ALTER TABLE chunks ADD COLUMN page INTEGER;
             PRAGMA user_version = 5;",
        )?;
    }
    tx.commit()?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(())
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
    distro: Option<Distro>,
    example: Option<&'a str>,
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

fn write_source<'a>(
    tx: &Transaction<'_>,
    source: Source<'_>,
    passages: Option<impl Iterator<Item = Passage<'a>>>,
    digest: &[u8],
    model: &mut impl Embedder,
) -> Result<i64> {
    let id: i64 = tx.query_row(
        "INSERT INTO sources(key,category,title,root,extension,desktop,setting_group,digest,command_distro,command_example)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(key) DO UPDATE SET title=excluded.title, extension=excluded.extension,
           setting_group=excluded.setting_group, digest=excluded.digest,
           command_distro=excluded.command_distro, command_example=excluded.command_example RETURNING id",
        params![
            source.key,
            source.category,
            source.title,
            source.root,
            source.extension,
            source.desktop,
            source.setting_group,
            digest,
            source.distro.map(Distro::as_str),
            source.example
        ],
        |row| row.get(0),
    )?;
    tx.execute("DELETE FROM chunks WHERE source = ?1", [id])?;
    let Some(passages) = passages else {
        store_chunk(tx, id, &source, &Passage::unlocated(""), &[])?;
        return Ok(id);
    };
    let mut parts = passages.peekable();
    if parts.peek().is_none() {
        write_chunks(tx, id, &source, &[Passage::unlocated("")], model)?;
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
    parts: &[Passage<'_>],
    model: &mut impl Embedder,
) -> Result<()> {
    let title: String = source.title.chars().take(256).collect();
    let input: Vec<_> = parts
        .iter()
        .map(|part| format!("{title}\n{}", part.text))
        .collect();
    let vectors = model
        .encode(InputKind::Passage, &input)
        .context("encode document passages")?;
    ensure!(
        vectors.len() == parts.len(),
        "unexpected number of embeddings"
    );
    for (passage, vector) in parts.iter().zip(vectors) {
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
        store_chunk(tx, id, source, passage, &bytes)?;
    }
    Ok(())
}

fn store_chunk(
    tx: &Transaction<'_>,
    id: i64,
    source: &Source<'_>,
    passage: &Passage<'_>,
    embedding: &[u8],
) -> Result<()> {
    let searchable_title = if source.key.starts_with(manuals::KEY_PREFIX) {
        source.title.to_owned()
    } else {
        format!("{} {}", source.title, source.key)
    };
    let (line_start, line_end, page) = match passage.location {
        Some(HitLocation::Lines {
            line_start,
            line_end,
        }) => (Some(line_start), Some(line_end), None),
        Some(HitLocation::Page { page }) => (None, None, Some(page)),
        None => (None, None, None),
    };
    tx.execute(
        "INSERT INTO chunks(source, embedding, line_start, line_end, page) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, embedding, line_start, line_end, page],
    )?;
    tx.execute(
        "INSERT INTO fulltext(rowid, title, body) VALUES (?1, ?2, ?3)",
        params![tx.last_insert_rowid(), searchable_title, passage.text],
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
        let texts = translated_passages(setting.id, &text);
        let digest = Sha256::digest(format!(
            "setting-v2\n{}\n{}\n{}",
            setting.title,
            setting.group,
            texts.join("\0")
        ));
        keep.push(sync_catalog_entry(
            tx,
            Source {
                key: setting.id,
                category: "setting",
                title: setting.title,
                root: None,
                extension: None,
                desktop: Some("cosmic"),
                setting_group: Some(setting.group),
                distro: None,
                example: None,
            },
            &texts,
            &digest,
            model,
            report,
        )?);
    }
    remove_missing_sources(tx, "setting", false, &keep, report)
}

fn synchronize_applications(
    tx: &Transaction<'_>,
    applications: &[Application],
    model: &mut impl Embedder,
    report: &mut IndexReport,
) -> Result<()> {
    let mut keep = Vec::new();
    for app in applications {
        let texts: Vec<_> = app.texts.iter().map(String::as_str).collect();
        let digest = Sha256::digest(format!(
            "application-v2\n{}\n{}",
            app.name,
            texts.join("\0")
        ));
        keep.push(sync_catalog_entry(
            tx,
            Source {
                key: &app.id,
                category: "application",
                title: &app.name,
                root: None,
                extension: None,
                desktop: None,
                setting_group: None,
                distro: None,
                example: None,
            },
            &texts,
            &digest,
            model,
            report,
        )?);
    }
    remove_missing_sources(tx, "application", false, &keep, report)
}

fn synchronize_commands(
    tx: &Transaction<'_>,
    model: &mut impl Embedder,
    report: &mut IndexReport,
) -> Result<()> {
    let mut keep = Vec::new();
    for command in COMMANDS {
        let texts = translated_passages(command.id, command.description);
        let digest = Sha256::digest(format!(
            "command-v2\n{}\n{}\n{}\n{}",
            command.title,
            command.distro.as_str(),
            texts.join("\0"),
            command.example
        ));
        keep.push(sync_catalog_entry(
            tx,
            Source {
                key: command.id,
                category: "command",
                title: command.title,
                root: None,
                extension: None,
                desktop: None,
                setting_group: None,
                distro: Some(command.distro),
                example: Some(command.example),
            },
            &texts,
            &digest,
            model,
            report,
        )?);
    }
    remove_missing_sources(tx, "command", false, &keep, report)
}

fn translated_passages<'a>(id: &str, primary: &'a str) -> Vec<&'a str> {
    std::iter::once(primary)
        .chain(catalog_texts(id).iter().copied())
        .collect()
}

fn synchronize_local_commands(
    tx: &Transaction<'_>,
    distro: Distro,
    model: &mut impl Embedder,
    report: &mut IndexReport,
) -> Result<()> {
    let mut keep = Vec::new();
    for command in manuals::discover()? {
        let digest = Sha256::digest(format!(
            "local-command-v1\n{}\n{}\n{}\n{}",
            distro.as_str(),
            command.name,
            command.text.as_deref().unwrap_or_default(),
            command.example.as_deref().unwrap_or_default()
        ));
        keep.push(sync_catalog_entry(
            tx,
            Source {
                key: &command.key,
                category: "command",
                title: &command.name,
                root: None,
                extension: None,
                desktop: None,
                setting_group: None,
                distro: Some(distro),
                example: command.example.as_deref(),
            },
            command.text.as_deref().as_slice(),
            &digest,
            model,
            report,
        )?);
    }
    remove_missing_sources(tx, "command", true, &keep, report)
}

fn sync_catalog_entry(
    tx: &Transaction<'_>,
    source: Source<'_>,
    texts: &[&str],
    digest: &[u8],
    model: &mut impl Embedder,
    report: &mut IndexReport,
) -> Result<i64> {
    if let Some((id, _)) = source_state(tx, source.key)?.filter(|(_, saved)| saved == digest) {
        report.unchanged += 1;
        return Ok(id);
    }
    let passages = (!texts.is_empty()).then(|| {
        texts
            .iter()
            .flat_map(|text| files::chunks(text).map(|(_, chunk)| Passage::unlocated(chunk)))
    });
    let id = write_source(tx, source, passages, digest, model)?;
    report.written += 1;
    Ok(id)
}

fn remove_missing_sources(
    tx: &Transaction<'_>,
    category: &str,
    local_commands: bool,
    keep: &[i64],
    report: &mut IndexReport,
) -> Result<()> {
    let ids = tx
        .prepare("SELECT id FROM sources WHERE category = ?1 AND (key GLOB ?2) = ?3")?
        .query_map(
            params![
                category,
                format!("{}*", manuals::KEY_PREFIX),
                local_commands
            ],
            |row| row.get::<_, i64>(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let keep: std::collections::HashSet<_> = keep.iter().copied().collect();
    for id in ids.into_iter().filter(|id| !keep.contains(id)) {
        report.removed += tx.execute("DELETE FROM sources WHERE id = ?1", [id])?;
    }
    Ok(())
}

struct CaptionScan<'a> {
    captioner: &'a mut dyn ImageCaptioner,
    required: bool,
}

fn synchronize_files(
    tx: &Transaction<'_>,
    root: &Path,
    root_name: &str,
    exclusions: &[PathBuf],
    model: &mut impl Embedder,
    mut captions: Option<CaptionScan<'_>>,
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
        let file = match files::open_file(root, path, kind) {
            Ok(file) => file,
            Err(error) => {
                warn(report, path, &error);
                continue;
            }
        };
        let caption_scan = captions
            .as_mut()
            .filter(|_| images::supports_caption(&extension));
        let bytes = if kind == FileKind::Text || caption_scan.is_some() {
            match files::read(file) {
                Ok(bytes) => Some(bytes),
                Err(error) => {
                    warn(report, path, &error);
                    if kind == FileKind::Text {
                        continue;
                    }
                    None
                }
            }
        } else {
            None
        };
        let mut digest = match (&bytes, &caption_scan) {
            (Some(bytes), Some(scan)) => Sha256::new()
                .chain_update(b"image-caption-v1\0")
                .chain_update(scan.captioner.identity())
                .chain_update(b"\0")
                .chain_update(bytes)
                .finalize(),
            (Some(bytes), None) => Sha256::new()
                .chain_update(b"extract-v2\0")
                .chain_update(bytes)
                .finalize(),
            (None, _) => Sha256::digest(b"file-name-v1"),
        };
        // A filename-only scan must preserve previously indexed captions.
        let preserve_caption = kind == FileKind::Image && caption_scan.is_none();
        let stored = source_state(tx, key)?;
        let id = if let Some((id, _)) = stored
            .as_ref()
            .filter(|(_, saved)| preserve_caption || saved == digest.as_slice())
        {
            report.unchanged += 1;
            *id
        } else {
            let text = match (bytes, caption_scan) {
                (Some(bytes), Some(scan)) => match scan.captioner.caption(&bytes) {
                    Ok(caption) => {
                        let caption = caption.filter(|text| !text.trim().is_empty());
                        if caption.is_none() {
                            warn(
                                report,
                                path,
                                &anyhow::anyhow!("image could not be captioned; indexed filename"),
                            );
                        }
                        caption
                    }
                    Err(error) if scan.required => {
                        return Err(error).with_context(|| format!("caption {}", path.display()));
                    }
                    Err(error) => {
                        warn(
                            report,
                            path,
                            &error
                                .context("Florence-2 unavailable. Continuing without new captions"),
                        );
                        captions = None;
                        if let Some((id, _)) = stored {
                            report.unchanged += 1;
                            tx.execute("INSERT INTO seen VALUES (?1)", [id])?;
                            continue;
                        }
                        digest = Sha256::digest(b"file-name-v1");
                        None
                    }
                },
                (Some(bytes), None) => match files::extract(&extension, bytes) {
                    Ok(text) => Some(text),
                    Err(error) => {
                        warn(report, path, &error);
                        continue;
                    }
                },
                (None, _) => None,
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
                    distro: None,
                    example: None,
                },
                text.as_deref().map(|text| {
                    files::file_passages(text, extension == "pdf").map(|mut passage| {
                        if kind == FileKind::Image {
                            passage.location = None;
                        }
                        passage
                    })
                }),
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
