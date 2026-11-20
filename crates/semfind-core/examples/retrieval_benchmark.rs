use anyhow::{Context, Result, ensure};
use semfind_core::{
    Category, Distro, Embedder, Hit, Index, IndexOptions, InputKind, LocalModel, SearchOptions,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    env, fs,
    path::{Component, Path, PathBuf},
    time::Instant,
};

const INDEX_ROUNDS: usize = 3;
const WARM_ROUNDS: usize = 5;
const RESULT_LIMIT: usize = 10;
const MODEL_COUNT: usize = 4;

#[derive(Deserialize)]
struct Dataset {
    documents: BTreeMap<String, String>,
    queries: Vec<Query>,
}

#[derive(Deserialize, Serialize)]
struct Query {
    split: String,
    group: String,
    query: String,
    category: Option<Category>,
    relevant: Vec<String>,
}

struct BenchmarkModel {
    worker: LocalModel,
    cutoff: Option<f32>,
    dimensions: usize,
    calibration_cache: Option<HashMap<String, Vec<f32>>>,
}

impl Embedder for BenchmarkModel {
    fn identity(&self) -> &str {
        self.worker.identity()
    }

    fn minimum_similarity(&self) -> Option<f32> {
        self.cutoff
    }

    fn encode(&mut self, kind: InputKind, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let cache_key = match (kind, texts) {
            (InputKind::Query, [query]) => Some(query),
            _ => None,
        };
        if let (Some(query), Some(cache)) = (cache_key, &self.calibration_cache)
            && let Some(vector) = cache.get(query)
        {
            return Ok(vec![vector.clone()]);
        }
        let vectors = self.worker.encode(kind, texts)?;
        ensure!(
            vectors.len() == texts.len()
                && vectors.iter().all(|vector| vector.len() == self.dimensions),
            "unexpected embedding dimension"
        );
        if let (Some(query), Some(cache)) = (cache_key, &mut self.calibration_cache) {
            cache.insert(query.clone(), vectors[0].clone());
        }
        Ok(vectors)
    }
}

struct ModelSpec<'a> {
    name: &'static str,
    python: &'a Path,
    weights: &'a Path,
    dimensions: usize,
    external_weights: bool,
}

impl ModelSpec<'_> {
    fn start(&self) -> Result<BenchmarkModel> {
        let worker = LocalModel::start(self.python, self.weights)
            .with_context(|| format!("start {}", self.name))?;
        Ok(BenchmarkModel {
            worker,
            cutoff: None,
            dimensions: self.dimensions,
            calibration_cache: None,
        })
    }
}

#[derive(Default, Serialize)]
struct Measurements {
    index_ms: Vec<f64>,
    startup_ms: Vec<f64>,
    warm_query_ms: Vec<f64>,
    cold_query_ms: Vec<f64>,
}

#[derive(Serialize)]
struct Calibration {
    cutoff: Option<f32>,
    validation_score: f64,
}

#[derive(Serialize)]
struct ModelResult {
    name: &'static str,
    identity: String,
    artifacts_bytes: u64,
    dimensions: usize,
    indexed_sources: i64,
    indexed_passages: i64,
    production_cutoff: Option<f32>,
    tuned_cutoff: Option<f32>,
    evaluated_cutoff: Option<f32>,
    calibration: Vec<Calibration>,
    evaluated_ranks: Vec<Vec<String>>,
    tuned_ranks: Vec<Vec<String>>,
    unfiltered_ranks: Vec<Vec<String>>,
    timing: Measurements,
}

fn options(query: &Query) -> SearchOptions {
    SearchOptions {
        category: query.category,
        distro: Some(Distro::Debian),
        include_local_commands: false,
        limit: RESULT_LIMIT,
        ..Default::default()
    }
}

fn ranks(hits: Vec<Hit>, corpus: &Path) -> Result<Vec<String>> {
    hits.into_iter()
        .map(|hit| match hit.path {
            Some(path) => Ok(path
                .strip_prefix(corpus)?
                .to_str()
                .context("non-UTF-8 fixture path")?
                .to_owned()),
            None => Ok(hit.id),
        })
        .collect()
}

fn validation_score(query: &Query, ranked: &[String]) -> f64 {
    if query.relevant.is_empty() {
        return if ranked.is_empty() { 1.0 } else { 0.0 };
    }
    ranked
        .iter()
        .position(|id| query.relevant.contains(id))
        .map_or(0.0, |rank| 1.0 / (rank + 1) as f64)
}

fn calibrate(
    index: &Index,
    model: &mut BenchmarkModel,
    corpus: &Path,
    validation: &[&Query],
) -> Result<(Option<f32>, Vec<Calibration>)> {
    let thresholds = std::iter::once(None).chain(
        (20..=100)
            .step_by(2)
            .map(|percent| Some(percent as f32 / 100.0)),
    );
    let mut tested = Vec::new();
    let mut best = (None, -1.0);
    model.calibration_cache = Some(HashMap::new());
    for cutoff in thresholds {
        model.cutoff = cutoff;
        let mut total = 0.0;
        for query in validation {
            let ranked = ranks(index.search(&query.query, &options(query), model)?, corpus)?;
            total += validation_score(query, &ranked);
        }
        let validation_score = total / validation.len() as f64;
        tested.push(Calibration {
            cutoff,
            validation_score,
        });
        // Keep the lower threshold when validation scores tie.
        if validation_score > best.1 {
            best = (cutoff, validation_score);
        }
    }
    model.calibration_cache = None;
    Ok((best.0, tested))
}

fn evaluate(
    index: &Index,
    model: &mut BenchmarkModel,
    corpus: &Path,
    queries: &[&Query],
    cutoff: Option<f32>,
) -> Result<Vec<Vec<String>>> {
    model.cutoff = cutoff;
    queries
        .iter()
        .map(|query| ranks(index.search(&query.query, &options(query), model)?, corpus))
        .collect()
}

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn order(round: usize) -> impl Iterator<Item = usize> {
    (0..MODEL_COUNT).map(move |index| (index + round) % MODEL_COUNT)
}

fn prepare_corpus(dataset: &Dataset, root: &Path) -> Result<()> {
    for (name, contents) in &dataset.documents {
        let relative = Path::new(name);
        ensure!(
            relative
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
            "invalid fixture path {name}"
        );
        let path = root.join(relative);
        fs::create_dir_all(path.parent().context("fixture parent")?)?;
        fs::write(path, contents)?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    let [dataset_path, e5, granite, e5_large, granite_311, python] = args.as_slice() else {
        anyhow::bail!(
            "expected DATASET E5_SMALL_DIR GRANITE_97_DIR E5_LARGE_DIR GRANITE_311_DIR PYTHON"
        );
    };
    let dataset: Dataset = serde_json::from_slice(&fs::read(dataset_path)?)?;
    let validation: Vec<_> = dataset
        .queries
        .iter()
        .filter(|query| query.split == "validation")
        .collect();
    let test: Vec<_> = dataset
        .queries
        .iter()
        .filter(|query| query.split == "test")
        .collect();
    ensure!(
        !validation.is_empty()
            && !test.is_empty()
            && validation.len() + test.len() == dataset.queries.len(),
        "expected validation and test queries"
    );
    let scratch = tempfile::Builder::new()
        .prefix("semfind-retrieval-")
        .tempdir()?;
    let corpus = scratch.path().join("corpus");
    prepare_corpus(&dataset, &corpus)?;
    let specs: [ModelSpec<'_>; MODEL_COUNT] = [
        ModelSpec {
            name: "E5 Small",
            python,
            weights: e5,
            dimensions: 384,
            external_weights: false,
        },
        ModelSpec {
            name: "Granite 97M",
            python,
            weights: granite,
            dimensions: 384,
            external_weights: false,
        },
        ModelSpec {
            name: "E5 Large",
            python,
            weights: e5_large,
            dimensions: 1024,
            external_weights: true,
        },
        ModelSpec {
            name: "Granite 311M",
            python,
            weights: granite_311,
            dimensions: 768,
            external_weights: false,
        },
    ];
    let mut measurements: [Measurements; MODEL_COUNT] = Default::default();
    let mut index_paths: [PathBuf; MODEL_COUNT] = Default::default();
    let exclusions = IndexOptions::default();
    for round in 0..INDEX_ROUNDS {
        for i in order(round) {
            let mut model = specs[i].start()?;
            let path = scratch.path().join(format!("index-{i}-{round}"));
            let started = Instant::now();
            let mut index = Index::open(&path)?;
            let report = index.synchronize(Some(&corpus), &exclusions, &mut model)?;
            let elapsed = ms(started);
            ensure!(
                report.skipped == 0,
                "index skipped documents: {:?}",
                report.warnings
            );
            eprintln!(
                "{} index round {}: {elapsed:.0} ms",
                specs[i].name,
                round + 1
            );
            measurements[i].index_ms.push(elapsed);
            index_paths[i] = path;
        }
    }
    let mut results = Vec::new();
    let mut live = Vec::new();
    for (i, spec) in specs.iter().enumerate() {
        let index = Index::open(&index_paths[i])?;
        let mut model = spec.start()?;
        let production_cutoff = (i == 0)
            .then(|| model.worker.minimum_similarity())
            .flatten();
        let (tuned_cutoff, calibration) = calibrate(&index, &mut model, &corpus, &validation)?;
        let evaluated_cutoff = production_cutoff.or(tuned_cutoff);
        eprintln!(
            "{} tuned cutoff {tuned_cutoff:?}, evaluated cutoff {evaluated_cutoff:?}",
            spec.name
        );
        let evaluated_ranks = evaluate(&index, &mut model, &corpus, &test, evaluated_cutoff)?;
        let tuned_ranks = if tuned_cutoff == evaluated_cutoff {
            evaluated_ranks.clone()
        } else {
            evaluate(&index, &mut model, &corpus, &test, tuned_cutoff)?
        };
        let unfiltered_ranks = evaluate(&index, &mut model, &corpus, &test, None)?;
        let status = index.status()?;
        ensure!(
            status.files == i64::try_from(dataset.documents.len())?,
            "incomplete index"
        );
        let mut artifacts_bytes = fs::metadata(spec.weights.join("model.onnx"))?.len()
            + fs::metadata(spec.weights.join("tokenizer.json"))?.len();
        if spec.external_weights {
            artifacts_bytes += fs::metadata(spec.weights.join("model.onnx_data"))?.len();
        }
        results.push(ModelResult {
            name: spec.name,
            identity: model.identity().to_owned(),
            artifacts_bytes,
            dimensions: spec.dimensions,
            indexed_sources: status.files + status.settings + status.commands,
            indexed_passages: status.chunks,
            production_cutoff,
            tuned_cutoff,
            evaluated_cutoff,
            calibration,
            evaluated_ranks,
            tuned_ranks,
            unfiltered_ranks,
            timing: std::mem::take(&mut measurements[i]),
        });
        model.cutoff = evaluated_cutoff;
        live.push((index, model));
    }
    for round in 0..WARM_ROUNDS {
        for query in &test {
            for i in order(round) {
                let (index, model) = &mut live[i];
                let started = Instant::now();
                index.search(&query.query, &options(query), model)?;
                results[i].timing.warm_query_ms.push(ms(started));
            }
        }
    }
    drop(live);

    let mut groups = BTreeSet::new();
    let cold_queries: Vec<_> = test
        .iter()
        .copied()
        .filter(|query| !query.relevant.is_empty() && groups.insert(&query.group))
        .collect();
    for (round, query) in cold_queries.iter().enumerate() {
        for i in order(round) {
            let cold = Instant::now();
            let index = Index::open(&index_paths[i])?;
            let startup = Instant::now();
            let mut model = specs[i].start()?;
            let startup_ms = ms(startup);
            model.cutoff = results[i].evaluated_cutoff;
            index.search(&query.query, &options(query), &mut model)?;
            results[i].timing.cold_query_ms.push(ms(cold));
            results[i].timing.startup_ms.push(startup_ms);
        }
    }
    println!(
        "{}",
        serde_json::json!({
            "semfind_version": env!("CARGO_PKG_VERSION"),
            "document_count": dataset.documents.len(),
            "test_queries": test,
            "validation_queries": validation,
            "cold_queries": cold_queries.iter().map(|query| &query.query).collect::<Vec<_>>(),
            "models": results,
        })
    );
    Ok(())
}
