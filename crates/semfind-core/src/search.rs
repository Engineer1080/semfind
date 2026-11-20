use crate::{
    commands::Distro,
    files::{self, FileKind, HitLocation},
    index::{Index, check_model},
    manuals,
    model::{Embedder, InputKind, normalized},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Stable category names shared by clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    File,
    Application,
    Setting,
    Command,
    Action,
}

impl Category {
    /// The serialized category name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Application => "application",
            Self::Setting => "setting",
            Self::Command => "command",
            Self::Action => "action",
        }
    }
}

/// Filters applied before ranking. The default limit is ten.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub path: Option<PathBuf>,
    pub extension: Option<String>,
    pub file_kind: Option<FileKind>,
    pub category: Option<Category>,
    /// Restrict distribution-specific command hits to the selected family.
    pub distro: Option<Distro>,
    /// Include locally installed manual pages and executable names.
    pub include_local_commands: bool,
    pub limit: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            path: None,
            extension: None,
            file_kind: None,
            category: None,
            distro: None,
            include_local_commands: true,
            limit: 10,
        }
    }
}

/// One result per indexed source. File IDs are absolute paths.
/// The fused score is a ranking signal and has no probability interpretation.
#[derive(Debug, Serialize, Deserialize)]
pub struct Hit {
    pub id: String,
    pub category: Category,
    pub title: String,
    pub path: Option<PathBuf>,
    pub extension: Option<String>,
    pub desktop: Option<String>,
    pub setting_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distro: Option<Distro>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
    pub score: f64,
    pub exact: bool,
    pub snippet: String,
    /// Source location for the passage used to build the snippet, when indexed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<HitLocation>,
}

struct Candidate {
    hit: Hit,
    literal_name: bool,
    explicit_command: bool,
    lexical_chunk: Option<i64>,
    vector_chunk: Option<(i64, f32)>,
    exact_chunk: Option<i64>,
}

struct QueryEmbedding {
    vector: Vec<f32>,
    minimum_similarity: Option<f32>,
}

#[cfg(target_os = "linux")]
mod query_cache;

impl Index {
    /// Search persisted contents with BM25, cosine similarity and reciprocal rank fusion.
    /// An empty query lists files alphabetically when a file kind is selected.
    pub fn search(
        &self,
        query: &str,
        options: &SearchOptions,
        model: &mut impl Embedder,
    ) -> Result<Vec<Hit>> {
        self.search_with_encoder(query, options, |conn, query| {
            encode_query(conn, query, model)
        })
    }

    /// Search with an encoder that is created only when eligible passages have embeddings.
    pub fn search_with_model<M: Embedder>(
        &self,
        query: &str,
        options: &SearchOptions,
        start_model: impl FnOnce() -> Result<M>,
    ) -> Result<Vec<Hit>> {
        self.search_with_cached_model(query, options, &mut None, start_model)
    }

    /// Reuse the encoder in the given slot, initializing it only when search needs embeddings.
    pub fn search_with_cached_model<M: Embedder>(
        &self,
        query: &str,
        options: &SearchOptions,
        model: &mut Option<M>,
        start_model: impl FnOnce() -> Result<M>,
    ) -> Result<Vec<Hit>> {
        self.search_with_encoder(query, options, |conn, query| {
            encode_with_cached_model(conn, query, model, start_model)
        })
    }

    /// Reuse the last query embedding stored in the private index directory.
    /// Results are ranked against the current index and file access checks.
    pub fn search_with_cached_query<M: Embedder>(
        &self,
        query: &str,
        options: &SearchOptions,
        model_directory: &Path,
        model: &mut Option<M>,
        start_model: impl FnOnce() -> Result<M>,
    ) -> Result<(Vec<Hit>, bool)> {
        #[cfg(target_os = "linux")]
        {
            let Some(cache) = query_cache::QueryCache::new(&self.conn, model_directory) else {
                return self
                    .search_with_cached_model(query, options, model, start_model)
                    .map(|hits| (hits, false));
            };
            let mut cache_hit = false;
            let hits = self.search_with_encoder(query, options, |conn, query| {
                let key = cache.key(conn, query)?;
                if let Some(vector) = key.and_then(|key| cache.load(conn, key)) {
                    cache_hit = true;
                    return Ok(vector);
                }
                let embedding = encode_with_cached_model(conn, query, model, start_model)?;
                if let Some(key) = key
                    && cache.key(conn, query)? == Some(key)
                {
                    cache.store(key, &embedding);
                }
                Ok(embedding)
            })?;
            Ok((hits, cache_hit))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = model_directory;
            self.search_with_cached_model(query, options, model, start_model)
                .map(|hits| (hits, false))
        }
    }

    fn search_with_encoder(
        &self,
        query: &str,
        options: &SearchOptions,
        encode: impl FnOnce(&Connection, &str) -> Result<QueryEmbedding>,
    ) -> Result<Vec<Hit>> {
        let query = query.trim();
        ensure!(
            query.len() <= 2048 && !query.contains('\0'),
            "query must contain at most 2048 bytes and no NUL characters"
        );
        ensure!(
            !query.is_empty() || options.file_kind.is_some(),
            "provide a query or select a file kind"
        );
        ensure!(
            (1..=100).contains(&options.limit),
            "limit must be between 1 and 100"
        );
        let path = options.path.as_deref().map(files::root).transpose()?;
        let extension = options
            .extension
            .as_ref()
            .map(|text| text.trim_start_matches('.').to_ascii_lowercase());
        ensure!(
            extension.as_ref().is_none_or(|ext| !ext.is_empty()),
            "file type cannot be empty"
        );
        let tx = self.conn.unchecked_transaction()?;
        let mut candidates = HashMap::new();
        let needle = query.to_lowercase();
        let mut category = options.category;
        let mut semantic_query = query;
        let mut lexical_query = query;
        let mut chat_task = false;
        if category.is_none_or(|category| category == Category::Application)
            && options.file_kind.is_none()
            && path.is_none()
            && extension.is_none()
            && let Some(task) = application_task(&needle)
            && !tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sources
                 WHERE (key = ?1 COLLATE NOCASE OR title = ?1 COLLATE NOCASE)
                   AND (?2 IS NULL OR category = ?2))",
                params![query, category.map(Category::as_str)],
                |row| row.get::<_, bool>(0),
            )?
        {
            category = Some(Category::Application);
            chat_task = matches!(
                task,
                "chat"
                    | "chatten"
                    | "chatting"
                    | "messenger"
                    | "instant messaging"
                    | "charlar"
                    | "chatear"
                    | "chatter"
                    | "discuter"
                    | "conversar"
                    | "chattare"
                    | "общения"
            );
            semantic_query = if chat_task { "chat messenger" } else { task };
            lexical_query = if chat_task {
                "chat chatten chatting messenger messaging instantmessaging sofortnachrichten \
                 messagerie mensajería mensagens messaggistica мессенджер"
            } else {
                task
            };
        }
        {
            let mut statement = tx.prepare(
                "SELECT id,key,category,title,root,extension,desktop,setting_group,
                        command_distro,command_example FROM sources
                 WHERE (?1 IS NULL OR category=?1) AND (?2 IS NULL OR extension=?2)
                   AND (?3 IS NULL OR command_distro IS NULL
                        OR command_distro = 'generic' OR command_distro = ?3)
                   AND (?4 OR key NOT GLOB ?5)",
            )?;
            let mut rows = statement.query(params![
                category.map(Category::as_str),
                extension,
                options.distro.map(Distro::as_str),
                options.include_local_commands,
                format!("{}*", manuals::KEY_PREFIX)
            ])?;
            while let Some(row) = rows.next()? {
                let id: i64 = row.get(0)?;
                let key: String = row.get(1)?;
                let category: String = row.get(2)?;
                let title: String = row.get(3)?;
                let root: Option<String> = row.get(4)?;
                let extension: Option<String> = row.get(5)?;
                let kind = extension.as_deref().and_then(FileKind::from_extension);
                if options.file_kind.is_some() && options.file_kind != kind {
                    continue;
                }
                let file_path = if let Some(root) = root {
                    let entry = Path::new(&key);
                    let Some(kind) = kind else {
                        continue;
                    };
                    if path.as_ref().is_some_and(|path| !entry.starts_with(path))
                        || files::open_file(Path::new(&root), entry, kind).is_err()
                    {
                        continue;
                    }
                    Some(entry.to_path_buf())
                } else {
                    if path.is_some() {
                        continue;
                    }
                    None
                };
                let literal_name = key == query || title == query;
                let explicit_command = options.category == Some(Category::Command)
                    && command_mentioned(&needle, &title.to_lowercase());
                let hit = Hit {
                    exact: literal_name
                        || key.to_lowercase() == needle
                        || title.to_lowercase() == needle,
                    id: key,
                    title,
                    path: file_path,
                    extension,
                    desktop: row.get(6)?,
                    setting_group: row.get(7)?,
                    distro: row
                        .get::<_, Option<String>>(8)?
                        .map(|value| Distro::from_str(&value).context("invalid command distro"))
                        .transpose()?,
                    example: row.get(9)?,
                    category: match category.as_str() {
                        "file" => Category::File,
                        "setting" => Category::Setting,
                        "application" => Category::Application,
                        "command" => Category::Command,
                        _ => anyhow::bail!("unsupported source category {category}"),
                    },
                    score: 0.0,
                    snippet: String::new(),
                    location: None,
                };
                candidates.insert(
                    id,
                    Candidate {
                        hit,
                        literal_name,
                        explicit_command,
                        lexical_chunk: None,
                        vector_chunk: None,
                        exact_chunk: None,
                    },
                );
            }
        }
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        if query.is_empty() {
            let mut hits: Vec<_> = candidates.into_values().map(|item| item.hit).collect();
            hits.sort_unstable_by(|a, b| a.title.cmp(&b.title).then(a.id.cmp(&b.id)));
            hits.truncate(options.limit);
            tx.commit()?;
            return Ok(hits);
        }
        let window = options.limit.saturating_mul(4).max(40);
        let mut lexical = Vec::new();
        let identifier = !query.contains(char::is_whitespace)
            && query
                .chars()
                .any(|part| part.is_numeric() || matches!(part, '-' | '_' | '.'));
        if let Some(expression) = fts_expression(lexical_query, identifier) {
            let mut statement = tx.prepare(
                "SELECT chunks.source, chunks.id, fulltext.body FROM fulltext
                 JOIN chunks ON chunks.id=fulltext.rowid
                 WHERE fulltext MATCH ?1 ORDER BY bm25(fulltext, 3.0, 1.0), chunks.id",
            )?;
            let mut rows = statement.query([expression])?;
            while let Some(row) = rows.next()? {
                let source: i64 = row.get(0)?;
                let Some(candidate) = candidates.get_mut(&source) else {
                    continue;
                };
                let chunk: i64 = row.get(1)?;
                if candidate.lexical_chunk.is_none() && lexical.len() < window {
                    candidate.lexical_chunk = Some(chunk);
                    lexical.push(source);
                }
                if identifier && !candidate.hit.exact {
                    let body: String = row.get(2)?;
                    if contains_identifier(&body.to_lowercase(), &needle) {
                        candidate.hit.exact = true;
                        candidate.exact_chunk = Some(chunk);
                    }
                }
                if !identifier && lexical.len() >= window {
                    break;
                }
            }
        }
        if chat_task {
            candidates.retain(|_, item| item.lexical_chunk.is_some());
        }
        let file_kind = options
            .file_kind
            .or_else(|| extension.as_deref().and_then(FileKind::from_extension));
        if file_kind != Some(FileKind::Video) {
            let mut encode = Some(encode);
            let mut query_embedding = None;
            let mut statement =
                tx.prepare("SELECT source, id, embedding FROM chunks WHERE length(embedding) > 0")?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                let Some(candidate) = candidates.get_mut(&row.get::<_, i64>(0)?) else {
                    continue;
                };
                let id: i64 = row.get(1)?;
                let bytes = row.get_ref(2)?.as_blob()?;
                let embedding = match &query_embedding {
                    Some(embedding) => embedding,
                    None => query_embedding.insert(encode.take().expect("first embedding")(
                        &tx,
                        semantic_query,
                    )?),
                };
                let score = similarity(&embedding.vector, bytes)?;
                if embedding.minimum_similarity.is_none_or(|min| score >= min)
                    && candidate
                        .vector_chunk
                        .is_none_or(|(prev, best)| score > best || (score == best && id < prev))
                {
                    candidate.vector_chunk = Some((id, score));
                }
            }
        }
        let mut semantic: Vec<_> = candidates
            .iter()
            .filter_map(|(&id, item)| item.vector_chunk.map(|(_, score)| (id, score)))
            .collect();
        semantic.sort_unstable_by(|(a, score_a), (b, score_b)| {
            score_b.total_cmp(score_a).then(a.cmp(b))
        });
        for ranking in [
            lexical,
            semantic
                .into_iter()
                .take(window)
                .map(|(id, _)| id)
                .collect(),
        ] {
            for (rank, id) in ranking.into_iter().enumerate() {
                candidates.get_mut(&id).expect("ranked candidate").hit.score +=
                    1.0 / (61 + rank) as f64;
            }
        }
        let mut ranked: Vec<_> = candidates
            .into_values()
            .filter(|item| item.hit.exact || item.explicit_command || item.hit.score > 0.0)
            .collect();
        ranked.sort_unstable_by(|a, b| {
            b.hit
                .exact
                .cmp(&a.hit.exact)
                .then_with(|| b.literal_name.cmp(&a.literal_name))
                .then_with(|| a.exact_chunk.is_some().cmp(&b.exact_chunk.is_some()))
                .then_with(|| b.explicit_command.cmp(&a.explicit_command))
                .then_with(|| b.hit.score.total_cmp(&a.hit.score))
                .then_with(|| a.hit.id.cmp(&b.hit.id))
        });
        let mut hits = Vec::new();
        {
            let mut body = tx.prepare(
                "SELECT fulltext.body, chunks.line_start, chunks.line_end, chunks.page
                 FROM fulltext JOIN chunks ON chunks.id=fulltext.rowid
                 WHERE fulltext.rowid=?1",
            )?;
            for mut item in ranked.into_iter().take(options.limit) {
                let chunk = item
                    .exact_chunk
                    .or(item.lexical_chunk)
                    .or(item.vector_chunk.map(|(id, _)| id));
                let chunk = if item.explicit_command && chunk.is_none() {
                    tx.query_row(
                        "SELECT id FROM chunks WHERE source=(SELECT id FROM sources WHERE key=?1)
                         ORDER BY id LIMIT 1",
                        [&item.hit.id],
                        |row| row.get(0),
                    )
                    .optional()?
                } else {
                    chunk
                };
                if let Some(chunk) = chunk
                    && let Some((text, location)) = body
                        .query_row([chunk], |row| {
                            let location = match (row.get(1)?, row.get(2)?, row.get(3)?) {
                                (Some(line_start), Some(line_end), None) => {
                                    Some(HitLocation::Lines {
                                        line_start,
                                        line_end,
                                    })
                                }
                                (None, None, Some(page)) => Some(HitLocation::Page { page }),
                                _ => None,
                            };
                            Ok((row.get::<_, String>(0)?, location))
                        })
                        .optional()?
                {
                    let exact = item.exact_chunk.is_some().then_some(needle.as_str());
                    item.hit.snippet = snippet(&text, exact);
                    item.hit.location = location;
                }
                hits.push(item.hit);
            }
        }
        tx.commit()?;
        Ok(hits)
    }
}

fn encode_with_cached_model<M: Embedder>(
    conn: &Connection,
    query: &str,
    model: &mut Option<M>,
    start_model: impl FnOnce() -> Result<M>,
) -> Result<QueryEmbedding> {
    let model = match model {
        Some(model) => model,
        None => model.insert(start_model()?),
    };
    encode_query(conn, query, model)
}

fn encode_query(
    conn: &Connection,
    query: &str,
    model: &mut impl Embedder,
) -> Result<QueryEmbedding> {
    check_model(conn, model)?;
    let mut vectors = model
        .encode(InputKind::Query, &[query.to_owned()])
        .context("encode query")?;
    ensure!(vectors.len() == 1, "unexpected query embedding count");
    Ok(QueryEmbedding {
        vector: normalized(vectors.pop().expect("one vector"))?,
        minimum_similarity: model.minimum_similarity(),
    })
}

fn application_task(query: &str) -> Option<&str> {
    let prefixes = [
        "ein programm zum ",
        "ein programm für ",
        "eine anwendung zum ",
        "eine anwendung für ",
        "eine app zum ",
        "eine app für ",
        "ich suche ein programm zum ",
        "ich suche eine app zum ",
        "a program to ",
        "a program for ",
        "an application to ",
        "an application for ",
        "an app to ",
        "an app for ",
        "i need an app to ",
        "une application pour ",
        "un programme pour ",
        "una aplicación para ",
        "un programa para ",
        "uma aplicação para ",
        "um aplicativo para ",
        "um programa para ",
        "un’app per ",
        "un'app per ",
        "un programma per ",
        "приложение для ",
        "программа для ",
    ];
    prefixes
        .iter()
        .find_map(|prefix| query.strip_prefix(prefix))
        .map(|task| task.trim().trim_end_matches(['?', '!', '.']).trim_end())
        .filter(|task| task.chars().any(char::is_alphanumeric))
}

fn fts_expression(query: &str, identifier: bool) -> Option<String> {
    let terms: Vec<_> = query
        .split(|part: char| !part.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .take(64)
        .collect();
    if terms.is_empty() {
        return None;
    }
    Some(if identifier {
        format!("\"{}\"", terms.join(" "))
    } else {
        terms
            .into_iter()
            .map(|term| format!("\"{term}\""))
            .collect::<Vec<_>>()
            .join(" OR ")
    })
}

fn contains_identifier(text: &str, needle: &str) -> bool {
    let char_in_code = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-');
    text.match_indices(needle).any(|(start, matched)| {
        !text[..start].chars().next_back().is_some_and(char_in_code)
            && !text[start + matched.len()..]
                .chars()
                .next()
                .is_some_and(char_in_code)
    })
}

fn command_mentioned(query: &str, command: &str) -> bool {
    query.match_indices(command).any(|(start, matched)| {
        let before = &query[..start];
        let after = &query[start + matched.len()..];
        if before.ends_with('`') && after.starts_with('`') {
            return true;
        }
        before.ends_with(char::is_whitespace)
            && matches!(
                before.split_whitespace().next_back(),
                Some("mit" | "with" | "using")
            )
            && !after.starts_with(|ch: char| ch.is_alphanumeric() || matches!(ch, '-' | '_'))
    })
}

fn snippet(text: &str, needle: Option<&str>) -> String {
    // Align an exact-code snippet near the matching whitespace-delimited word.
    let mut words: Vec<_> = text.split_whitespace().collect();
    if let Some(needle) = needle
        && let Some(index) = words
            .iter()
            .position(|word| contains_identifier(&word.to_lowercase(), needle))
    {
        words.drain(..index.saturating_sub(8));
    }
    let flat = words.join(" ");
    let mut chars = flat.chars();
    let mut result: String = chars.by_ref().take(240).collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}

fn similarity(query: &[f32], bytes: &[u8]) -> Result<f32> {
    ensure!(
        bytes.len() == query.len() * 4,
        "index embedding dimension mismatch"
    );
    let (values, _) = bytes.as_chunks::<4>();
    let score = query
        .iter()
        .zip(values)
        .map(|(query, chunk)| query * f32::from_le_bytes(*chunk))
        .sum::<f32>();
    ensure!(score.is_finite(), "invalid stored embedding");
    Ok(score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_query_does_not_inject_fts_syntax() {
        assert_eq!(
            fts_expression("\"privacy\" OR path:foo*", false).as_deref(),
            Some("\"privacy\" OR \"OR\" OR \"path\" OR \"foo\"")
        );
        assert_eq!(
            fts_expression("E-0042", true).as_deref(),
            Some("\"E 0042\"")
        );
        assert!(contains_identifier("error: e-0042.", "e-0042"));
        assert!(!contains_identifier("xe-0042 or e-00420", "e-0042"));
    }
}
