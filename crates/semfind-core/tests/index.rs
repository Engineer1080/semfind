use anyhow::{Result, bail};
use semfind_core::{Category, Embedder, Index, IndexOptions, InputKind, SearchOptions};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct FixtureModel {
    identity: &'static str,
    fail: bool,
    passages: usize,
}

impl Embedder for FixtureModel {
    fn identity(&self) -> &str {
        self.identity
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
    assert_eq!(first.written, 9);
    let rescan = index.synchronize(Some(&root), &IndexOptions::default(), &mut model)?;
    assert_eq!((rescan.written, rescan.unchanged), (0, 9));
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
    let report = index.synchronize(Some(&root), &Default::default(), &mut model)?;
    assert_eq!(count, model.passages);
    assert_eq!(report.unchanged, 7);

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
    assert_eq!((status.files, status.settings, status.chunks), (0, 5, 5));
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
