use crate::{
    files::{self, FileKind},
    index::{Index, check_model},
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
    Action,
}

impl Category {
    /// The serialized category name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Application => "application",
            Self::Setting => "setting",
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
    pub limit: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            path: None,
            extension: None,
            file_kind: None,
            category: None,
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
    pub score: f64,
    pub exact: bool,
    pub snippet: String,
}

struct Candidate {
    hit: Hit,
    literal_name: bool,
    lexical_chunk: Option<i64>,
    vector_chunk: Option<(i64, f32)>,
    exact_chunk: Option<i64>,
}

struct QueryEmbedding {
    vector: Vec<f32>,
    minimum_similarity: Option<f32>,
}

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
        self.search_with_encoder(query, options, |conn, query| {
            encode_query(conn, query, &mut start_model()?)
        })
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
        {
            let mut statement = tx.prepare(
                "SELECT id,key,category,title,root,extension,desktop,setting_group FROM sources
                 WHERE (?1 IS NULL OR category=?1) AND (?2 IS NULL OR extension=?2)",
            )?;
            let mut rows =
                statement.query(params![options.category.map(Category::as_str), extension])?;
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
                    category: match category.as_str() {
                        "file" => Category::File,
                        "setting" => Category::Setting,
                        "application" => Category::Application,
                        _ => anyhow::bail!("unsupported source category {category}"),
                    },
                    score: 0.0,
                    snippet: String::new(),
                };
                candidates.insert(
                    id,
                    Candidate {
                        hit,
                        literal_name,
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
        if let Some(expression) = fts_expression(query, identifier) {
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
        let file_kind = options
            .file_kind
            .or_else(|| extension.as_deref().and_then(FileKind::from_extension));
        if !matches!(file_kind, Some(FileKind::Image | FileKind::Video)) {
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
                    None => {
                        query_embedding.insert(encode.take().expect("first embedding")(&tx, query)?)
                    }
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
            .filter(|item| item.hit.exact || item.hit.score > 0.0)
            .collect();
        ranked.sort_unstable_by(|a, b| {
            b.hit
                .exact
                .cmp(&a.hit.exact)
                .then_with(|| b.literal_name.cmp(&a.literal_name))
                .then_with(|| a.exact_chunk.is_some().cmp(&b.exact_chunk.is_some()))
                .then_with(|| b.hit.score.total_cmp(&a.hit.score))
                .then_with(|| a.hit.id.cmp(&b.hit.id))
        });
        let mut hits = Vec::new();
        {
            let mut body = tx.prepare("SELECT body FROM fulltext WHERE rowid=?1")?;
            for mut item in ranked.into_iter().take(options.limit) {
                if let Some(chunk) = item
                    .exact_chunk
                    .or(item.lexical_chunk)
                    .or(item.vector_chunk.map(|(id, _)| id))
                {
                    let text: Option<String> =
                        body.query_row([chunk], |row| row.get(0)).optional()?;
                    item.hit.snippet = text.map_or_else(String::new, |text| {
                        snippet(
                            &text,
                            if item.exact_chunk.is_some() {
                                Some(&needle)
                            } else {
                                None
                            },
                        )
                    });
                }
                hits.push(item.hit);
            }
        }
        tx.commit()?;
        Ok(hits)
    }
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
