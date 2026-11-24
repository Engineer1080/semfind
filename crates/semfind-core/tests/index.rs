use anyhow::{Result, bail};
use rusqlite::{Connection, TransactionBehavior};
use semfind_core::{
    Category, Embedder, FileKind, HitLocation, ImageCaptioner, Index, IndexOptions, InputKind,
    SearchOptions,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct FixtureModel {
    identity: &'static str,
    fail: bool,
    passages: usize,
}

struct FixtureCaptioner {
    revision: &'static str,
    calls: usize,
    batches: Vec<usize>,
}

impl ImageCaptioner for FixtureCaptioner {
    fn identity(&self) -> &str {
        self.revision
    }

    fn batch_size(&self) -> usize {
        2
    }

    fn captions(&mut self, images: &[&[u8]]) -> Result<Vec<Option<String>>> {
        self.batches.push(images.len());
        images.iter().map(|image| self.caption(image)).collect()
    }

    fn caption(&mut self, image: &[u8]) -> Result<Option<String>> {
        self.calls += 1;
        match image {
            b"kitchen" => Ok(Some("A refrigerator next to a counter.".into())),
            b"outdoors" => Ok(Some("A garden bench outside.".into())),
            b"broken-worker" => bail!("fixture caption worker failed"),
            _ => Ok(None),
        }
    }
}

impl Embedder for FixtureModel {
    fn identity(&self) -> &str {
        self.identity
    }

    fn minimum_similarity(&self) -> Option<f32> {
        Some(0.78)
    }

    fn encode(&mut self, kind: InputKind, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if self.fail && matches!(kind, InputKind::Passage) {
            bail!("fixture inference failed");
        }
        if matches!(kind, InputKind::Passage) {
            self.passages += texts.len();
        }
        // Deterministic vectors exercise storage and ranking without downloading a model.
        Ok(texts
            .iter()
            .map(|text| {
                let text = text.to_lowercase();
                if text.contains("refrigerator") || text.contains("cold storage") {
                    vec![1.0, 0.0, 0.0]
                } else if text.contains("garden") || text.contains("watering") {
                    vec![0.0, 1.0, 0.0]
                } else {
                    vec![0.0, 0.0, 1.0]
                }
            })
            .collect())
    }
}

fn fixture(root: &Path, name: &str, text: &str) -> Result<PathBuf> {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, text)?;
    Ok(path)
}

fn restore_legacy_passages(conn: &Connection, text: &str) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE chunks DROP COLUMN line_start;
         ALTER TABLE chunks DROP COLUMN line_end;
         ALTER TABLE chunks DROP COLUMN page;",
    )?;
    let digest = Sha256::new()
        .chain_update(b"extract-v1\0")
        .chain_update(text)
        .finalize();
    conn.execute(
        "UPDATE sources SET digest = ?1 WHERE category = 'file'",
        [digest.as_slice()],
    )?;
    Ok(())
}

fn new_fixture() -> Result<(tempfile::TempDir, Index, PathBuf, FixtureModel)> {
    let temp = tempfile::Builder::new().prefix("semfind-test-").tempdir()?;
    let index = Index::open(&temp.path().join("index"))?;
    let root = temp.path().join("documents");
    fs::create_dir(&root)?;
    let model = FixtureModel {
        identity: "fixture-v1",
        fail: false,
        passages: 0,
    };
    Ok((temp, index, root, model))
}

#[test]
fn switching_models_keeps_saved_content_and_rolls_back_partial_rebuilds() -> Result<()> {
    struct NewEncoder {
        batches: usize,
        fail: bool,
    }

    impl Embedder for NewEncoder {
        fn identity(&self) -> &str {
            "fixture-v2"
        }

        fn encode(&mut self, kind: InputKind, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            if matches!(kind, InputKind::Passage) {
                self.batches += 1;
                if self.fail && self.batches > 1 {
                    bail!("fixture failed after writing the first batch");
                }
            }
            Ok(vec![vec![1.0, 0.0]; texts.len()])
        }
    }

    let (temp, mut index, root, mut original) = new_fixture()?;
    fixture(&root, "kitchen.jpg", "kitchen")?;
    fixture(&root, "notes.md", "Available storage")?;
    fixture(&root, "clip.mp4", "metadata only")?;
    fixture(&root, "excluded/secret.md", "excluded from the index")?;
    let options = IndexOptions {
        exclude: Some(vec![PathBuf::from("excluded")]),
    };
    let mut captioner = FixtureCaptioner {
        revision: "caption-v1",
        calls: 0,
        batches: Vec::new(),
    };
    index.synchronize_with_image_captions(&root, &options, None, &mut original, &mut captioner)?;
    let before = index.status()?;
    let directory = temp.path().join("new-model");
    let images = SearchOptions {
        file_kind: Some(FileKind::Image),
        ..Default::default()
    };
    let mut new = NewEncoder {
        batches: 0,
        fail: true,
    };
    assert!(index.rebuild_embeddings(&directory, &mut new).is_err());
    assert!(new.batches > 1);
    assert_eq!(index.embedding_model()?, before.model);
    assert_eq!(index.embedding_model_directory()?, None);
    assert_eq!(
        index.search("refrigerator", &images, &mut original)?.len(),
        1
    );

    new.fail = false;
    assert_eq!(
        index.rebuild_embeddings(&directory, &mut new)? as i64,
        before.chunks
    );
    let after = index.status()?;
    assert_eq!(after.model.as_deref(), Some(new.identity()));
    assert_eq!(after.model_directory.as_deref(), Some(directory.as_path()));
    assert_eq!(after.roots, before.roots);
    assert_eq!((after.files, after.chunks), (before.files, before.chunks));
    let hits = index.search("refrigerator", &images, &mut new)?;
    assert!(
        hits[0]
            .snippet
            .contains("A refrigerator next to a counter.")
    );
    let refreshed = index.synchronize(Some(&root), &Default::default(), &mut new)?;
    assert_eq!((refreshed.written, refreshed.removed), (0, 0));
    Ok(())
}

#[test]
fn direct_images_share_search_and_are_rebuilt_from_the_source_when_switching() -> Result<()> {
    struct DirectEncoder {
        text: FixtureModel,
        images: usize,
        fail: bool,
    }

    impl Embedder for DirectEncoder {
        fn identity(&self) -> &str {
            "test-direct-images"
        }

        fn minimum_similarity(&self) -> Option<f32> {
            self.text.minimum_similarity()
        }

        fn supports_images(&self) -> bool {
            true
        }

        fn encode_image(&mut self, image: &[u8]) -> Result<Option<Vec<f32>>> {
            self.images += 1;
            if self.fail {
                bail!("image inference failed");
            }
            Ok((image == b"kitchen").then(|| vec![0.6, 0.8, 0.0]))
        }

        fn encode(&mut self, kind: InputKind, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            self.text.encode(kind, texts)
        }
    }

    let (temp, mut index, root, mut original) = new_fixture()?;
    let notes = fixture(&root, "notes.md", "Cold storage keeps samples fresh.")?;
    let picture = fixture(&root, "asset.jpg", "kitchen")?;
    let corrupt = fixture(&root, "corrupt.png", "invalid bytes")?;
    let ignored = fixture(&root, "video.webm", "metadata only")?;
    let scoped = SearchOptions {
        path: Some(root.clone()),
        ..Default::default()
    };
    let mut captions = FixtureCaptioner {
        revision: "caption-v1",
        calls: 0,
        batches: Vec::new(),
    };
    index.synchronize_with_image_captions(
        &root,
        &Default::default(),
        None,
        &mut original,
        &mut captions,
    )?;
    let saved = index.status()?;
    let mut direct = DirectEncoder {
        text: FixtureModel {
            identity: "direct-text",
            passages: 0,
            fail: false,
        },
        images: 0,
        fail: true,
    };
    let directory = temp.path().join("model");
    assert!(index.rebuild_embeddings(&directory, &mut direct).is_err());
    assert_eq!(index.embedding_model()?, saved.model);
    assert_eq!(index.embedding_model_directory()?, None);
    assert!(
        index
            .search("cold storage", &scoped, &mut original)?
            .iter()
            .any(|hit| hit.path.as_ref() == Some(&picture) && hit.snippet.contains("refrigerator"))
    );

    direct.fail = false;
    index.rebuild_embeddings(&directory, &mut direct)?;
    let hits = index.search("cold storage", &scoped, &mut direct)?;
    assert!(hits.iter().any(|hit| hit.path.as_ref() == Some(&notes)));
    assert!(
        hits.iter()
            .any(|hit| hit.path.as_ref() == Some(&picture) && hit.snippet.is_empty())
    );
    assert_eq!(hits.len(), 2);
    let strict = SearchOptions {
        minimum_similarity: Some("0.78".parse().unwrap()),
        ..scoped.clone()
    };
    assert_eq!(index.search("cold storage", &strict, &mut direct)?.len(), 1);
    assert_eq!(
        index.search("corrupt.png", &scoped, &mut direct)?[0]
            .path
            .as_ref(),
        Some(&corrupt)
    );
    assert_eq!(
        index.search("video.webm", &scoped, &mut direct)?[0]
            .path
            .as_ref(),
        Some(&ignored)
    );

    let calls = direct.images;
    index.synchronize(Some(&root), &Default::default(), &mut direct)?;
    assert_eq!(direct.images, calls);
    fixture(&root, "asset.jpg", "invalid bytes")?;
    index.synchronize(Some(&root), &Default::default(), &mut direct)?;
    assert_eq!(direct.images, calls + 1);
    assert_eq!(index.search("cold storage", &scoped, &mut direct)?.len(), 1);

    fixture(&root, "asset.jpg", "kitchen")?;
    index.rebuild_embeddings_with_images(&directory, &mut original, Some(&mut captions), false)?;
    assert_eq!(index.embedding_model()?, saved.model);
    assert_eq!(index.status()?.roots, saved.roots);
    assert!(
        index
            .search("cold storage", &scoped, &mut original)?
            .iter()
            .any(|hit| hit.path.as_ref() == Some(&picture) && hit.snippet.contains("refrigerator"))
    );
    Ok(())
}

#[test]
fn image_captions_share_document_search_and_refresh_incrementally() -> Result<()> {
    let (_temp, mut index, root, mut model) = new_fixture()?;
    let picture = fixture(&root, "0001.jpg", "kitchen")?;
    fixture(&root, "0002.png", "outdoors")?;
    fixture(&root, "0003.webp", "unreadable")?;
    let document = fixture(&root, "notes.md", "Cold storage keeps samples fresh.")?;
    let scan = IndexOptions::default();
    let scoped = SearchOptions {
        path: Some(root.clone()),
        ..Default::default()
    };
    let image_only = SearchOptions {
        file_kind: Some(FileKind::Image),
        path: Some(root.clone()),
        ..Default::default()
    };
    index.synchronize(Some(&root), &scan, &mut model)?;
    assert!(
        index
            .search_with_model::<FixtureModel>("cold storage", &image_only, || {
                bail!("filename-only images should not need embeddings")
            })?
            .is_empty()
    );
    let mut captioner = FixtureCaptioner {
        revision: "fixture-caption-v1",
        calls: 0,
        batches: Vec::new(),
    };
    let report =
        index.synchronize_with_image_captions(&root, &scan, None, &mut model, &mut captioner)?;
    assert_eq!((report.written, report.skipped, captioner.calls), (3, 1, 3));
    assert_eq!(captioner.batches, [2, 1]);
    let hits = index.search("cold storage", &scoped, &mut model)?;
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().any(|hit| hit.path.as_ref() == Some(&document)));
    let image = hits
        .iter()
        .find(|hit| hit.path.as_ref() == Some(&picture))
        .unwrap();
    assert!(image.snippet.contains("refrigerator") && image.location.is_none());
    assert_eq!(
        index.search("cold storage", &image_only, &mut model)?[0]
            .path
            .as_ref(),
        Some(&picture)
    );
    assert_eq!(
        index.search("0003.webp", &image_only, &mut model)?[0].title,
        "0003.webp"
    );

    assert_eq!(
        index.synchronize(Some(&root), &scan, &mut model)?.written,
        0
    );
    assert_eq!(
        index
            .synchronize_with_image_captions(&root, &scan, None, &mut model, &mut captioner)?
            .written,
        0
    );
    assert_eq!(captioner.calls, 3);
    assert_eq!(captioner.batches, [2, 1]);

    fs::write(&picture, "broken-worker")?;
    assert!(
        index
            .synchronize_with_image_captions(&root, &scan, None, &mut model, &mut captioner)
            .is_err()
    );
    assert_eq!(
        index.search("cold storage", &image_only, &mut model)?[0]
            .path
            .as_ref(),
        Some(&picture)
    );
    fs::write(&picture, "outdoors")?;
    assert_eq!(
        index
            .synchronize_with_image_captions(&root, &scan, None, &mut model, &mut captioner)?
            .written,
        1
    );
    assert_eq!(captioner.calls, 5);
    assert!(
        index
            .search("cold storage", &image_only, &mut model)?
            .is_empty()
    );

    captioner.revision = "fixture-caption-v2";
    assert_eq!(
        index
            .synchronize_with_image_captions(&root, &scan, None, &mut model, &mut captioner)?
            .written,
        3
    );
    assert_eq!(captioner.calls, 8);
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn persisted_query_embedding_skips_model_start_and_ranks_current_files() -> Result<()> {
    let (temp, mut index, root, mut model) = new_fixture()?;
    let original = fixture(&root, "cold.md", "Cold storage keeps samples fresh.")?;
    let garden = fixture(&root, "garden.md", "There is a bench in the garden.")?;
    let index_dir = temp.path().join("index");
    let model_dir = temp.path().join("model");
    fs::create_dir(&model_dir)?;
    fs::write(model_dir.join("model.onnx"), "fixture weights")?;
    fs::write(model_dir.join("tokenizer.json"), "fixture tokenizer")?;
    index.synchronize(Some(&root), &IndexOptions::default(), &mut model)?;
    let options = SearchOptions {
        path: Some(root.clone()),
        ..Default::default()
    };
    let query = "cold storage";
    let (hits, reused) =
        index.search_with_cached_query(query, &options, &model_dir, &mut None, || {
            Ok(FixtureModel {
                identity: model.identity,
                fail: false,
                passages: 0,
            })
        })?;
    assert!(!reused);
    assert_eq!(hits[0].path.as_ref(), Some(&original));
    let cache_contents = fs::read_to_string(index_dir.join("last-query.json"))?;
    assert!(!cache_contents.contains(query));
    drop(index);

    let mut index = Index::open(&index_dir)?;
    let search_cached = |index: &Index| {
        index.search_with_cached_query::<FixtureModel>(
            query,
            &options,
            &model_dir,
            &mut None,
            || bail!("an existing cached query should not start its model"),
        )
    };
    let (hits, reused) = search_cached(&index)?;
    assert!(reused);
    assert_eq!(hits[0].path.as_ref(), Some(&original));
    assert!(hits.iter().all(|hit| hit.path.as_ref() != Some(&garden)));

    let overridden = SearchOptions {
        minimum_similarity: Some("off".parse().unwrap()),
        ..options.clone()
    };
    let (hits, reused) = index.search_with_cached_query::<FixtureModel>(
        query,
        &overridden,
        &model_dir,
        &mut None,
        || bail!("changing the threshold must reuse the cached vector"),
    )?;
    assert!(reused);
    assert!(hits.iter().any(|hit| hit.path.as_ref() == Some(&garden)));
    assert!(
        search_cached(&index)?
            .0
            .iter()
            .all(|hit| hit.path.as_ref() != Some(&garden))
    );

    let replacement = fixture(&root, "new.md", "The refrigerator supports cold storage.")?;
    index.synchronize(Some(&root), &IndexOptions::default(), &mut model)?;
    fs::remove_file(&original)?;
    let (hits, reused) = search_cached(&index)?;
    assert!(reused);
    assert_eq!(hits[0].path.as_ref(), Some(&replacement));
    assert!(hits.iter().all(|hit| hit.path.as_ref() != Some(&original)));

    fs::write(
        model_dir.join("tokenizer.json"),
        "changed tokenizer configuration",
    )?;
    let (hits, reused) =
        index.search_with_cached_query(query, &options, &model_dir, &mut None, || {
            Ok(FixtureModel {
                identity: model.identity,
                fail: false,
                passages: 0,
            })
        })?;
    assert!(!reused);
    assert_eq!(hits[0].path.as_ref(), Some(&replacement));
    Ok(())
}

#[test]
fn cached_search_model_initializes_only_when_needed_and_is_reused() -> Result<()> {
    let (_temp, mut index, root, mut indexing_model) = new_fixture()?;
    let options = SearchOptions {
        category: Some(Category::File),
        ..Default::default()
    };
    let mut model = None;
    let mut starts = 0;
    let mut start_model = || {
        starts += 1;
        Ok(FixtureModel {
            identity: "fixture-v1",
            fail: false,
            passages: 0,
        })
    };
    assert!(
        index
            .search_with_cached_model("cold storage", &options, &mut model, &mut start_model)?
            .is_empty()
    );
    assert!(model.is_none());
    let path = fixture(&root, "notes.md", "Put the samples in the refrigerator.")?;
    index.synchronize(Some(&root), &IndexOptions::default(), &mut indexing_model)?;
    for query in ["cold storage", "refrigerator"] {
        let hits = index.search_with_cached_model(query, &options, &mut model, &mut start_model)?;
        assert_eq!(hits[0].path.as_ref(), Some(&path));
    }
    assert_eq!(starts, 1);
    Ok(())
}

#[test]
fn old_index_remains_searchable_and_rescan_adds_passage_locations() -> Result<()> {
    let (temp, mut index, root, mut model) = new_fixture()?;
    let body = "\r\n \r\nThe refrigerator is ready.\r\nWrite E-9912 before leaving.\r\n";
    fixture(&root, "notes.md", body)?;
    let scan = IndexOptions::default();
    let search = SearchOptions {
        category: Some(Category::File),
        ..Default::default()
    };
    index.synchronize(Some(&root), &scan, &mut model)?;
    drop(index);
    let index_path = temp.path().join("index");
    let conn = Connection::open(index_path.join("index.sqlite3"))?;
    restore_legacy_passages(&conn, body)?;
    conn.pragma_update(None, "user_version", 4)?;
    drop(conn);

    let mut index = Index::open(&index_path)?;
    let old = index.search("E-9912", &search, &mut model)?;
    assert!(old[0].exact && old[0].snippet.contains("E-9912"));
    assert!(old[0].location.is_none());
    assert!(serde_json::to_value(&old[0])?.get("location").is_none());

    assert_eq!(
        index.synchronize(Some(&root), &scan, &mut model)?.written,
        1
    );
    let updated = index.search("E-9912", &search, &mut model)?;
    assert_eq!(updated[0].snippet, old[0].snippet);
    assert_eq!(
        updated[0].location,
        Some(HitLocation::Lines {
            line_start: 3,
            line_end: 4,
        })
    );
    assert_eq!(
        index.synchronize(Some(&root), &scan, &mut model)?.written,
        0
    );
    Ok(())
}

#[test]
fn persistent_hybrid_search_filters_and_prioritizes_exact_literals() -> Result<()> {
    let (temp, mut index, root, mut model) = new_fixture()?;
    let cold = fixture(
        &root,
        "notes/blue.md",
        "Put the samples in cold storage. A refrigerator is fine.",
    )?;
    let log = fixture(
        &root,
        "garden.log",
        "Irrigation error E-0042 during garden maintenance.",
    )?;
    fixture(&root, "noise.txt", "Watering error XE-0042 before lunch.")?;
    fixture(&root, "notes2/other.md", "Garden watering directions.")?;
    fixture(&root, ".ssh/private.txt", "private key")?;
    fixture(&root, "secrets/private.md", "private key")?;
    let options = IndexOptions {
        exclude: Some(vec!["secrets".into()]),
    };
    let first = index.synchronize(Some(&root), &options, &mut model)?;
    let commands = usize::try_from(index.status()?.commands)?;
    assert_eq!(first.written, 9 + commands);
    let rescan = index.synchronize(Some(&root), &IndexOptions::default(), &mut model)?;
    assert_eq!((rescan.written, rescan.unchanged), (0, 9 + commands));
    drop(index);

    let index = Index::open(&temp.path().join("index"))?;
    let path_options = SearchOptions {
        path: Some(root.join("notes")),
        extension: Some(".MD".into()),
        ..Default::default()
    };
    let hits = index.search("cold storage", &path_options, &mut model)?;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path.as_ref(), Some(&cold));
    let hits = index.search("refrigerator", &SearchOptions::default(), &mut model)?;
    assert_eq!(hits[0].path.as_ref(), Some(&cold));
    let hits = index.search("E-0042", &SearchOptions::default(), &mut model)?;
    assert_eq!(hits[0].path.as_ref(), Some(&log));
    assert!(hits[0].exact && hits[0].snippet.contains("E-0042"));
    assert_eq!(hits.iter().filter(|hit| hit.exact).count(), 1);
    let hits = index.search("blue.md", &SearchOptions::default(), &mut model)?;
    assert!(hits[0].exact);
    assert_eq!(hits[0].path.as_ref(), Some(&cold));
    let settings = SearchOptions {
        category: Some(Category::Setting),
        ..Default::default()
    };
    let hits = index.search("audio.output.volume", &settings, &mut model)?;
    assert_eq!(hits[0].id, "audio.output.volume");
    assert!(hits[0].exact);
    assert_eq!(hits[0].desktop.as_deref(), Some("cosmic"));
    let incompatible = &mut FixtureModel {
        identity: "fixture-v2",
        ..model
    };
    assert!(index.search("volume", &settings, incompatible).is_err());
    Ok(())
}

#[test]
fn rescans_skip_unchanged_passages_and_roll_back_failed_changes() -> Result<()> {
    let (_temp, mut index, root, mut model) = new_fixture()?;
    let disappearing = fixture(&root, "a.txt", "old contents")?;
    fixture(&root, "z.txt", "cold storage")?;
    index.synchronize(Some(&root), &Default::default(), &mut model)?;
    let count = model.passages;
    let catalog_chunks = index.status()?.chunks - 2;
    let report = index.synchronize(Some(&root), &Default::default(), &mut model)?;
    assert_eq!(count, model.passages);
    assert_eq!(
        report.unchanged,
        7 + usize::try_from(index.status()?.commands)?
    );

    fs::remove_file(disappearing)?;
    fixture(&root, "z.txt", "watering")?;
    model.fail = true;
    assert!(
        index
            .synchronize(Some(&root), &Default::default(), &mut model)
            .is_err()
    );
    assert_eq!(index.status()?.files, 2);
    model.fail = false;
    let hits = index.search("cold storage", &Default::default(), &mut model)?;
    assert_eq!(hits[0].title, "z.txt");
    assert!(hits[0].snippet.contains("cold storage"));
    assert!(hits.iter().all(|hit| hit.title != "a.txt"));
    let report = index.synchronize(Some(&root), &Default::default(), &mut model)?;
    assert_eq!((report.written, report.removed), (1, 1));
    assert!(index.remove_root(&root)?);
    let status = index.status()?;
    assert_eq!((status.files, status.settings), (0, 5));
    assert_eq!(status.chunks, catalog_chunks);
    Ok(())
}

#[test]
fn weak_semantic_similarity_does_not_remove_lexical_matches() -> Result<()> {
    let (_temp, mut index, root, mut model) = new_fixture()?;
    fixture(&root, "notes.txt", "Store the supplies in a refrigerator.")?;
    index.synchronize(Some(&root), &Default::default(), &mut model)?;
    let options = SearchOptions {
        path: Some(root),
        ..Default::default()
    };
    assert!(index.search("watering", &options, &mut model)?.is_empty());
    assert_eq!(index.search("supplies", &options, &mut model)?.len(), 1);
    assert_eq!(index.search("cold storage", &options, &mut model)?.len(), 1);
    assert!(index.search("notes.txt", &options, &mut model)?[0].exact);
    Ok(())
}

#[test]
fn existing_index_can_be_read_during_a_write_transaction() -> Result<()> {
    let (temp, mut index, root, mut model) = new_fixture()?;
    fixture(&root, "workshop.jpg", "")?;
    index.synchronize(Some(&root), &Default::default(), &mut model)?;
    let directory = temp.path().join("index");
    let mut writer = Connection::open(directory.join("index.sqlite3"))?;
    let pending = writer.transaction_with_behavior(TransactionBehavior::Immediate)?;
    pending.execute("DELETE FROM sources WHERE category = 'file'", [])?;

    let reader = Index::open(&directory)?;
    assert_eq!(reader.status()?.files, 1);
    let options = SearchOptions {
        file_kind: Some(FileKind::Image),
        ..Default::default()
    };
    let hits = reader.search_with_model::<FixtureModel>("workshop", &options, || {
        bail!("a filename search should not start the model")
    })?;
    assert_eq!(hits[0].title, "workshop.jpg");
    pending.rollback()?;
    Ok(())
}

#[test]
fn file_kind_filters_list_multiple_formats_and_search_media_names() -> Result<()> {
    let (temp, mut index, root, mut model) = new_fixture()?;
    let body = "Summer flowers";
    fixture(&root, "garden.txt", body)?;
    index.synchronize(Some(&root), &IndexOptions::default(), &mut model)?;
    let original_chunks = index.status()?.chunks;
    drop(index);
    let index_dir = temp.path().join("index");
    // Emulate a pre-command database containing text and settings.
    let db = rusqlite::Connection::open(index_dir.join("index.sqlite3"))?;
    restore_legacy_passages(&db, body)?;
    db.execute_batch(
        "PRAGMA foreign_keys = ON;
         DELETE FROM sources WHERE category = 'command';
         ALTER TABLE sources DROP COLUMN command_distro;
         ALTER TABLE sources DROP COLUMN command_example;
         PRAGMA user_version = 1;",
    )?;
    drop(db);
    let mut index = Index::open(&index_dir)?;
    assert_eq!(index.status()?.files, 1);
    assert_eq!(index.status()?.commands, 0);
    let missing_chunks = original_chunks - index.status()?.chunks;
    fixture(&root, "garden.JPG", "hiddenmarker")?;
    fixture(&root, "garden.PNG", "other image bytes")?;
    fixture(&root, "clips/garden.webm", "video bytes")?;
    let large_video = fs::File::create(root.join("clips/garden.mp4"))?;
    large_video.set_len(17 * 1024 * 1024)?;
    drop(large_video);
    fixture(&root, "garden.zip", "unsupported")?;
    fixture(&root, ".private/garden.png", "excluded")?;
    let previously_encoded = model.passages;
    index.synchronize(Some(&root), &IndexOptions::default(), &mut model)?;
    let status = index.status()?;
    assert_eq!(status.files, 5);
    assert_eq!(status.chunks, original_chunks);
    let encoded = previously_encoded + usize::try_from(missing_chunks)? + 1;
    assert_eq!(model.passages, encoded);
    drop(index);
    let mut index = Index::open(&temp.path().join("index"))?;
    for (kind, expected) in [
        (FileKind::Text, vec!["txt"]),
        (FileKind::Image, vec!["jpg", "png"]),
        (FileKind::Video, vec!["mp4", "webm"]),
    ] {
        let filters = SearchOptions {
            file_kind: Some(kind),
            ..Default::default()
        };
        let listed = index.search("", &filters, &mut model)?;
        let extensions: Vec<_> = listed
            .iter()
            .filter_map(|hit| hit.extension.as_deref())
            .collect();
        assert_eq!(extensions, expected);
        assert_eq!(
            index.search("garden", &filters, &mut model)?.len(),
            expected.len()
        );
    }
    let mut jpg = SearchOptions {
        extension: Some(".JPG".into()),
        file_kind: Some(FileKind::Image),
        ..Default::default()
    };
    assert!(index.search("hiddenmarker", &jpg, &mut model)?.is_empty());
    let results = index.search("garden.JPG", &jpg, &mut model)?;
    assert_eq!(results.len(), 1);
    assert!(results[0].exact && results[0].snippet.is_empty());
    fs::remove_file(root.join("garden.JPG"))?;
    assert!(index.search("garden.JPG", &jpg, &mut model)?.is_empty());
    index.synchronize(Some(&root), &IndexOptions::default(), &mut model)?;
    assert_eq!(index.status()?.files, 4);
    assert_eq!(model.passages, encoded);
    jpg.file_kind = Some(FileKind::Video);
    assert!(index.search("", &jpg, &mut model)?.is_empty());
    Ok(())
}

#[cfg(unix)]
#[test]
fn exact_filename_case_wins_on_case_sensitive_filesystems() -> Result<()> {
    let (_temp, mut index, root, mut model) = new_fixture()?;
    fixture(&root, "Status.txt", "cold storage")?;
    fixture(&root, "status.txt", "status.txt")?;
    // Some Unix systems use a case-insensitive filesystem.
    if fs::read_dir(&root)?.count() == 2 {
        index.synchronize(Some(&root), &Default::default(), &mut model)?;
        let hits = index.search("Status.txt", &Default::default(), &mut model)?;
        assert_eq!(hits[0].title, "Status.txt");
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn skips_file_and_directory_symlinks_and_uses_private_modes() -> Result<()> {
    use std::os::unix::fs::{MetadataExt, symlink};
    let (temp, mut index, root, mut model) = new_fixture()?;
    let outside = temp.path().join("outside");
    let outside_file = fixture(&outside, "secret.txt", "outside")?;
    symlink(outside_file, root.join("file.txt"))?;
    symlink(&outside, root.join("directory"))?;
    assert!(
        index
            .synchronize(
                Some(&root.join("directory")),
                &Default::default(),
                &mut model
            )
            .is_err()
    );
    index.synchronize(Some(&root), &Default::default(), &mut model)?;
    assert_eq!(index.status()?.files, 0);
    assert_eq!(
        fs::metadata(temp.path().join("index"))?.mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(temp.path().join("index/index.sqlite3"))?.mode() & 0o777,
        0o600
    );
    Ok(())
}
