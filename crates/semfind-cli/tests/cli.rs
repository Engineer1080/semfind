use anyhow::{Context, Result, ensure};
use semfind_core::{Category, Distro, Hit};
use std::{
    env, fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Output, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

fn cli(index: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_semfind"));
    command.arg("--index-dir").arg(index).env("LC_ALL", "C");
    command
}

fn success(output: Output) -> Result<Vec<u8>> {
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

fn has_init_hint(output: &Output) -> bool {
    String::from_utf8_lossy(&output.stderr).contains("Run `semfind init`")
}

#[test]
fn status_json_and_usage_exit_codes() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let index = temp.path().join("index");
    let status_output = cli(&index).args(["status", "--json"]).output()?;
    assert_eq!(has_init_hint(&status_output), cfg!(target_os = "linux"));
    let status = success(status_output)?;
    let status: serde_json::Value = serde_json::from_slice(&status)?;
    assert_eq!(status["files"], 0, "{status}");
    assert_eq!(status["commands"], 0, "{status}");
    assert!(status["roots"].as_array().unwrap().is_empty());
    let search = cli(&index).args(["missing", "--json"]).output()?;
    assert_eq!(has_init_hint(&search), cfg!(target_os = "linux"));
    assert_eq!(
        serde_json::from_slice::<Vec<Hit>>(&success(search)?)?.len(),
        0
    );
    assert_eq!(cli(&index).output()?.status.code(), Some(2));
    assert_eq!(
        cli(&index).args(["search"]).output()?.status.code(),
        Some(2)
    );
    assert_eq!(
        cli(&index)
            .args(["unindex", "missing"])
            .output()?
            .status
            .code(),
        Some(1)
    );
    Ok(())
}

#[test]
fn search_time_is_opt_in_and_preserves_json() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let index = temp.path().join("index");
    let untimed = cli(&index).args(["missing", "--json"]).output()?;
    assert!(!String::from_utf8_lossy(&untimed.stderr).contains("Search time"));
    let baseline = success(untimed)?;
    for args in [
        &["missing", "--json", "--time"][..],
        &["search", "missing", "--json", "--time"],
        &["--time", "search", "missing", "--json"],
    ] {
        let result = cli(&index).args(args).output()?;
        let stderr = String::from_utf8_lossy(&result.stderr);
        let duration = stderr
            .lines()
            .find_map(|line| line.strip_prefix("semfind: Search time "))
            .and_then(|value| value.strip_suffix(" ms"))
            .with_context(|| format!("missing search duration in {stderr}"))?
            .parse::<f64>()?;
        assert!(duration.is_finite() && duration >= 0.0);
        assert_eq!(success(result)?, baseline);
    }
    let german = cli(&index)
        .env("LC_ALL", "de_DE.UTF-8")
        .env_remove("LANGUAGE")
        .args(["search", "missing", "--time"])
        .output()?;
    assert!(String::from_utf8_lossy(&german.stderr).contains("semfind: Suchzeit "));
    assert!(success(german)?.is_empty());
    Ok(())
}

#[test]
fn stdin_search_flushes_each_result_before_eof() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let index = temp.path().join("index");
    let mut child = cli(&index)
        .arg("--python")
        .arg(temp.path().join("missing-python"))
        .args(["search", "--stdin", "--time"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut input = child.stdin.take().context("missing stdin pipe")?;
    let stdout = child.stdout.take().context("missing stdout pipe")?;
    let (send, receive) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if send.send(line).is_err() {
                break;
            }
        }
    });
    let exchange = (|| -> Result<_> {
        writeln!(input, "first query")?;
        let first = receive.recv_timeout(Duration::from_secs(5))??;
        // Whitespace-only lines produce no records. EOF completes the unterminated query.
        input.write_all(b"\n \t\r\nsecond query")?;
        drop(input);
        let second = receive.recv_timeout(Duration::from_secs(5))??;
        Ok([first, second])
    })();
    let output = child.wait_with_output()?;
    reader.join().expect("stdout reader panicked");
    assert!(receive.try_recv().is_err());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr)
            .matches("semfind: Search time ")
            .count(),
        2
    );
    success(output)?;
    assert_eq!(exchange?, ["[]", "[]"]);
    Ok(())
}

#[test]
fn stdin_search_reports_invalid_input_and_keeps_previous_results() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let index = temp.path().join("index");
    for args in [
        &["search", "--stdin", "query"][..],
        &["--stdin", "search", "query"],
        &["--stdin", "status"],
    ] {
        assert_eq!(cli(&index).args(args).output()?.status.code(), Some(2));
    }
    let mut child = cli(&index)
        .args(["--stdin", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("missing stdin pipe")?
        .write_all(b"first\ninvalid\0query\nlast\n")?;
    let output = child.wait_with_output()?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"[]\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("stdin line 2") && stderr.contains("NUL"),
        "{stderr}"
    );
    Ok(())
}

#[test]
fn terminal_messages_follow_locale_without_changing_json() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let index = temp.path().join("index");
    for (locale, usage, description, files) in [
        ("de_DE.UTF-8", "Aufruf:", "Lokale Datei-", "Dateien:"),
        ("ru_RU.UTF-8", "Использование:", "Локальный поиск", "Файлы:"),
        ("pt_BR.UTF-8", "Uso:", "Busca local", "Arquivos:"),
        (
            "fr_CA.UTF-8",
            "Utilisation:",
            "Recherche locale",
            "Fichiers:",
        ),
        ("es_MX.UTF-8", "Uso:", "Búsqueda local", "Archivos:"),
        ("it_IT.UTF-8", "Utilizzo:", "Ricerca locale", "File:"),
    ] {
        let command = || {
            let mut command = cli(&index);
            command.env("LC_ALL", locale).env_remove("LANGUAGE");
            command
        };
        let help = String::from_utf8(success(command().arg("--help").output()?)?)?;
        assert!(help.contains(usage) && help.contains(description), "{help}");
        let status = command().arg("status").output()?;
        if cfg!(target_os = "linux") {
            let hint = String::from_utf8_lossy(&status.stderr);
            assert!(
                hint.contains("`semfind init`") && !has_init_hint(&status),
                "{hint}"
            );
        }
        let text = String::from_utf8(success(status)?)?;
        assert!(text.starts_with(files), "{text}");
        let status = success(command().args(["status", "--json"]).output()?)?;
        let status: serde_json::Value = serde_json::from_slice(&status)?;
        assert_eq!(status["files"], 0);
    }
    Ok(())
}

#[test]
#[ignore = "requires a local model and Python environment"]
fn real_model_searches_all_added_catalog_languages() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let index = temp.path().join("index");
    success(cli(&index).arg("index").output()?)?;
    for (install, brighter) in [
        (
            "Как установить новый пакет из репозитория?",
            "Как увеличить яркость экрана?",
        ),
        (
            "Como instalo um novo pacote?",
            "Como posso aumentar o brilho da tela?",
        ),
        (
            "Comment installer un nouveau paquet ?",
            "Comment augmenter la luminosité de mon écran ?",
        ),
        (
            "¿Cómo instalo un paquete nuevo?",
            "¿Cómo subo el brillo de la pantalla?",
        ),
        (
            "Come installo un nuovo pacchetto?",
            "Come aumento la luminosità dello schermo?",
        ),
    ] {
        for (query, category, expected) in [
            (install, "command", "command.packages.install.debian"),
            (brighter, "setting", "display.brightness"),
        ] {
            let output = success(
                cli(&index)
                    .args([
                        query,
                        "--category",
                        category,
                        "--distro",
                        "debian",
                        "--limit",
                        "1",
                        "--json",
                    ])
                    .output()?,
            )?;
            let hits: Vec<Hit> = serde_json::from_slice(&output)?;
            assert_eq!(
                hits.first().map(|hit| hit.id.as_str()),
                Some(expected),
                "{query}: {hits:?}"
            );
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires a local model and Python environment"]
fn real_model_finds_commands_from_questions_and_filters_distributions() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let index = temp.path().join("index");
    success(cli(&index).arg("index").output()?)?;
    let search = |query: &str, family: &str| -> Result<Vec<Hit>> {
        let output = success(
            cli(&index)
                .args([query, "--category", "command", "--distro", family, "--json"])
                .output()?,
        )?;
        Ok(serde_json::from_slice(&output)?)
    };
    for (family, distro, example) in [
        ("debian", Distro::Debian, "sudo apt install ripgrep"),
        ("arch", Distro::Arch, "sudo pacman -S ripgrep"),
    ] {
        let results = search("Wie installiere ich ein neues Paket?", family)?;
        let best = results.first().context("no command matched the question")?;
        assert_eq!(best.example.as_deref(), Some(example), "{results:?}");
        assert_eq!(best.category, Category::Command);
        assert_eq!(best.distro, Some(distro));
        assert!(
            results
                .iter()
                .all(|hit| hit.distro == Some(distro) || hit.distro == Some(Distro::Generic))
        );
    }
    for (family, expected) in [("debian", "apt search"), ("arch", "pacman -Ss")] {
        let results = search("Wie suche ich ein Paket in den Paketquellen?", family)?;
        assert_eq!(
            results.first().map(|hit| hit.title.as_str()),
            Some(expected),
            "{results:?}"
        );
    }
    let hits = search("Welche Ordner belegen am meisten Speicherplatz?", "generic")?;
    assert_eq!(
        hits.first().map(|hit| hit.title.as_str()),
        Some("du"),
        "{hits:?}"
    );
    assert!(hits.iter().all(|hit| hit.distro == Some(Distro::Generic)));
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires a local model, Python environment and gzip"]
fn real_model_indexes_manuals_and_executable_names_and_refreshes_them() -> Result<()> {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let temp = tempfile::tempdir()?;
    let index = temp.path().join("index");
    let manuals = temp.path().join("man");
    let section = manuals.join("man1");
    let bin = temp.path().join("bin");
    fs::create_dir_all(&section)?;
    fs::create_dir(&bin)?;
    let plain = section.join("samplehash.1");
    fs::write(
        &plain,
        ".TH SAMPLEHASH 1\n.SH NAME\nsamplehash \\- compute SHA-256 checksums\n.SH SYNOPSIS\n.B samplehash\n[\\fIFILE\\fR]...\n.SH DESCRIPTION\nPrint a checksum for a file and verify its integrity.\n",
    )?;
    fs::write(
        section.join("samplehash.1.gz"),
        success(Command::new("gzip").arg("-c").arg(&plain).output()?)?,
    )?;
    fs::remove_file(plain)?;
    fs::write(section.join("hashshortcut.1"), ".so man1/samplehash.1\n")?;
    symlink("samplehash.1.gz", section.join("hashalias.1.gz"))?;
    let executable = bin.join("undocumented-tool");
    fs::write(&executable, "#!/bin/sh\nexit 0\n")?;
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))?;
    let system_path = env::var_os("PATH").unwrap_or_default();
    let path = env::join_paths(std::iter::once(bin).chain(env::split_paths(&system_path)))?;
    let run = |args: &[&str]| -> Result<Vec<u8>> {
        success(
            cli(&index)
                .env("MANPATH", &manuals)
                .env("PATH", &path)
                .args(args)
                .output()?,
        )
    };
    let search = |query: &str| -> Result<Vec<Hit>> {
        Ok(serde_json::from_slice(&run(&[
            query,
            "--category",
            "command",
            "--json",
        ])?)?)
    };
    run(&["index", "--manpages"])?;
    for name in ["samplehash", "hashshortcut", "hashalias"] {
        let results = search(name)?;
        let hit = results.first().context("manual should be found by name")?;
        assert!(
            hit.exact && hit.snippet.contains("compute SHA-256 checksums"),
            "{hit:?}"
        );
        assert!(hit.snippet.contains("samplehash [FILE]"), "{hit:?}");
        assert_eq!(
            hit.example.as_deref(),
            Some(format!("man -- 1 {name}").as_str())
        );
    }
    let result = search("Wie prüfe ich Dateien mit samplehash?")?;
    let named = result
        .first()
        .context("command explicitly mentioned in query")?;
    assert_eq!(named.title, "samplehash");
    let other = if named.distro == Some(Distro::Arch) {
        "debian"
    } else {
        "arch"
    };
    let foreign: Vec<Hit> = serde_json::from_slice(&run(&[
        "samplehash",
        "--category",
        "command",
        "--distro",
        other,
        "--json",
    ])?)?;
    assert!(
        foreign
            .iter()
            .all(|hit| !hit.id.starts_with("command.local."))
    );
    let results = search("undocumented-tool")?;
    let hit = results
        .first()
        .context("executable should be found by name")?;
    assert!(hit.exact && hit.example.is_none(), "{hit:?}");
    run(&["index"])?;
    assert!(search("samplehash")?[0].exact);

    fs::remove_file(section.join("samplehash.1.gz"))?;
    fs::remove_file(section.join("hashshortcut.1"))?;
    fs::remove_file(section.join("hashalias.1.gz"))?;
    fs::remove_file(executable)?;
    run(&["index", "--manpages"])?;
    for removed in [
        "samplehash",
        "hashshortcut",
        "hashalias",
        "undocumented-tool",
    ] {
        assert!(search(removed)?.iter().all(|hit| hit.title != removed));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires a local model and Python environment"]
fn real_model_init_indexes_home_desktop_applications_and_manuals() -> Result<()> {
    let temp = tempfile::Builder::new().prefix("semfind-init-").tempdir()?;
    let index = temp.path().join("index");
    let home = temp.path().join("home");
    let user_apps = home.join(".local/share/applications");
    let system_data = temp.path().join("system-share");
    let system_apps = system_data.join("applications");
    let manual_root = temp.path().join("man");
    let manual = manual_root.join("man1").join("init-example.1");
    fs::create_dir_all(&user_apps)?;
    fs::create_dir_all(&system_apps)?;
    fs::create_dir_all(manual_root.join("man1"))?;
    fs::write(
        &manual,
        ".TH INIT-EXAMPLE 1\n.SH NAME\ninit-example \\- examine a sparse file\n.SH SYNOPSIS\n.B init-example\nFILE\n",
    )?;
    fs::create_dir(home.join("Private"))?;
    fs::write(home.join("notes.txt"), "Document available for search")?;
    fs::write(home.join("Private/secret.txt"), "Excluded document")?;
    fs::write(
        system_apps.join("viewer.desktop"),
        "[Desktop Entry]\nType=Application\nName=System Viewer\n",
    )?;
    fs::write(
        user_apps.join("viewer.desktop"),
        "[Desktop Entry]\nType=Application\nName=My Viewer\nGenericName[de]=Bildbetrachter\nGenericName[es_MX]=Visor de imágenes\nKeywords=screenshot;photo;\nOnlyShowIn=COSMIC;\n",
    )?;
    fs::write(
        system_apps.join("hidden.desktop"),
        "[Desktop Entry]\nType=Application\nName=Hidden Viewer\n",
    )?;
    fs::write(
        user_apps.join("hidden.desktop"),
        "[Desktop Entry]\nHidden=true\n",
    )?;
    fs::write(
        system_apps.join("other.desktop"),
        "[Desktop Entry]\nType=Application\nName=Other Viewer\nNotShowIn=COSMIC;\n",
    )?;
    let command = || {
        let mut command = cli(&index);
        command
            .env_remove("XDG_DATA_HOME")
            .env("HOME", &home)
            .env("XDG_DATA_DIRS", &system_data)
            .env("XDG_CURRENT_DESKTOP", "COSMIC")
            .env("MANPATH", &manual_root);
        command
    };
    let run = |args: &[&str]| -> Result<Vec<u8>> { success(command().args(args).output()?) };
    let invalid_manual_root = temp.path().join("not-a-manual-directory");
    fs::write(&invalid_manual_root, "")?;
    let failed_init = command()
        .env("MANPATH", &invalid_manual_root)
        .arg("init")
        .output()?;
    assert_eq!(failed_init.status.code(), Some(1));
    let after_failure = command().args(["status", "--json"]).output()?;
    assert!(has_init_hint(&after_failure));
    let status: serde_json::Value = serde_json::from_slice(&success(after_failure)?)?;
    assert_eq!(status["files"], 0, "{status}");

    run(&["init", "--exclude", "Private"])?;
    let status_output = command().args(["status", "--json"]).output()?;
    assert!(!has_init_hint(&status_output));
    let status: serde_json::Value = serde_json::from_slice(&success(status_output)?)?;
    assert_eq!(status["roots"], serde_json::json!([home]));
    assert_eq!(status["files"], 1, "{status}");
    assert_eq!(status["applications"], 1, "{status}");
    assert_eq!(status["settings"], 5, "{status}");
    let commands = status["commands"].as_u64().context("command count")?;
    assert!(commands > 0);
    let hits: Vec<Hit> = serde_json::from_slice(&run(&[
        "Bildbetrachter",
        "--category",
        "application",
        "--json",
    ])?)?;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].category, Category::Application);
    assert_eq!(hits[0].id, "viewer.desktop");
    assert_eq!(hits[0].title, "My Viewer");
    let spanish: Vec<Hit> = serde_json::from_slice(&run(&[
        "Visor de imágenes",
        "--category",
        "application",
        "--json",
    ])?)?;
    assert_eq!(
        spanish.first().map(|hit| hit.id.as_str()),
        Some("viewer.desktop")
    );
    let manuals: Vec<Hit> = serde_json::from_slice(&run(&[
        "init-example",
        "--category",
        "command",
        "--limit",
        "1",
        "--json",
    ])?)?;
    let hit = manuals.first().context("init indexed the local manual")?;
    assert_eq!(hit.id, "command.local.man.1.init-example");
    assert!(hit.snippet.contains("examine a sparse file"), "{hit:?}");
    assert_eq!(hit.example.as_deref(), Some("man -- 1 init-example"));

    fs::remove_file(user_apps.join("viewer.desktop"))?;
    fs::remove_file(system_apps.join("viewer.desktop"))?;
    fs::remove_file(manual)?;
    let report = String::from_utf8(run(&["init"])?)?;
    assert!(
        report.contains(&format!(
            "Written: 0 | Unchanged: {} | Removed: 2",
            5 + commands
        )),
        "{report}"
    );
    let status: serde_json::Value = serde_json::from_slice(&run(&["status", "--json"])?)?;
    assert_eq!(status["files"], 1, "{status}");
    assert_eq!(status["applications"], 0, "{status}");
    assert_eq!(status["commands"], commands - 1, "{status}");
    Ok(())
}

#[test]
#[ignore = "requires a local model, Python environment and pdftotext"]
fn real_model_searches_across_languages_and_indexes_pdf() -> Result<()> {
    ensure!(
        env::var_os("SEMFIND_MODEL_DIR").is_some(),
        "set SEMFIND_MODEL_DIR"
    );
    let temp = tempfile::Builder::new().prefix("semfind-test-").tempdir()?;
    let index = temp.path().join("index");
    let root = temp.path().join("documents");
    fs::create_dir(&root)?;
    fs::write(
        root.join("a.txt"),
        "The hospital must protect patient records. Encrypt personal data and restrict access to authorised staff.",
    )?;
    fs::write(
        root.join("b.txt"),
        "Mix flour, yeast and water, then bake the dough in a hot oven to make bread.",
    )?;
    fs::write(
        root.join("data-retention.md"),
        include_str!("../../../examples/documents/data-retention.md"),
    )?;
    fs::write(
        root.join("c.pdf"),
        pdf("Workshop report: ERROR E-0042. The pump outlet was blocked."),
    )?;
    fs::write(root.join("workshop.png"), b"metadata-only image fixture")?;
    fs::write(root.join("workshop.jpg"), b"metadata-only image fixture")?;
    fs::write(root.join("workshop.mp4"), b"metadata-only video fixture")?;
    success(cli(&index).arg("index").arg(&root).output()?)?;
    let search = |args: &[&str]| -> Result<Vec<Hit>> {
        let result = success(cli(&index).args(args).arg("--json").output()?)?;
        Ok(serde_json::from_slice(&result)?)
    };
    let german = search(&[
        "Wie schütze ich vertrauliche Krankenakten?",
        "--type",
        "txt",
        "--limit",
        "1",
    ])?;
    assert_eq!(german[0].title, "a.txt");
    let english = search(&[
        "How do I make a loaf at home?",
        "--type",
        "txt",
        "--limit",
        "1",
    ])?;
    assert_eq!(english[0].title, "b.txt");
    let pdf = search(&["E-0042", "--type", "pdf", "--limit", "1"])?;
    assert_eq!(pdf[0].title, "c.pdf");
    assert!(pdf[0].exact && pdf[0].snippet.contains("pump outlet"));
    let missing_python = temp.path().join("missing-python");
    let media_search = |args: &[&str]| -> Result<Vec<Hit>> {
        let result = success(
            cli(&index)
                .arg("--python")
                .arg(&missing_python)
                .args(args)
                .arg("--json")
                .output()?,
        )?;
        Ok(serde_json::from_slice(&result)?)
    };
    let images = media_search(&["--kind", "image"])?;
    assert_eq!(
        images
            .iter()
            .map(|hit| hit.title.as_str())
            .collect::<Vec<_>>(),
        ["workshop.jpg", "workshop.png"]
    );
    let videos = media_search(&["workshop", "--kind", "video"])?;
    assert_eq!(videos.len(), 1);
    assert_eq!(videos[0].title, "workshop.mp4");
    assert_eq!(
        media_search(&["workshop", "--type", "mp4"])?[0].title,
        "workshop.mp4"
    );
    let settings = search(&[
        "Mein Bildschirm ist zu dunkel",
        "--category",
        "settings",
        "--limit",
        "1",
    ])?;
    assert_eq!(settings[0].id, "display.brightness");
    let retention = search(&[
        "Wie lange sollen personenbezogene Daten gespeichert werden?",
        "--type",
        "md",
    ])?;
    assert_eq!(retention[0].title, "data-retention.md");
    for unrelated in [
        "neutrino spectroscopy orbital decay",
        "Schwarze Löcher in der Milchstraße",
    ] {
        assert!(
            search(&[unrelated, "--type", "md"])?.is_empty(),
            "{unrelated}"
        );
    }
    Ok(())
}

fn pdf(text: &str) -> Vec<u8> {
    let stream = format!("BT /F1 14 Tf 25 100 Td ({text}) Tj ET\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 200] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()),
    ];
    let mut doc = String::from("%PDF-1.4\n");
    let mut offsets = String::from("0000000000 65535 f \n");
    for (i, object) in objects.iter().enumerate() {
        offsets.push_str(&format!("{:010} 00000 n \n", doc.len()));
        doc.push_str(&format!("{} 0 obj\n{object}\nendobj\n", i + 1));
    }
    let start = doc.len();
    doc.push_str(&format!(
        "xref\n0 6\n{offsets}trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n"
    ));
    doc.into_bytes()
}
