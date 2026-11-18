use anyhow::{Result, ensure};
use semfind_core::Hit;
use std::{
    env, fs,
    path::Path,
    process::{Command, Output},
};

fn cli(index: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_semfind"));
    command.arg("--index-dir").arg(index);
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

#[test]
fn status_json_and_usage_exit_codes() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let index = temp.path().join("index");
    let status = success(cli(&index).args(["status", "--json"]).output()?)?;
    let status: serde_json::Value = serde_json::from_slice(&status)?;
    assert_eq!(status["files"], 0, "{status}");
    assert!(status["roots"].as_array().unwrap().is_empty());
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
    let images = search(&["--kind", "image"])?;
    assert_eq!(
        images
            .iter()
            .map(|hit| hit.title.as_str())
            .collect::<Vec<_>>(),
        ["workshop.jpg", "workshop.png"]
    );
    let videos = search(&["workshop", "--kind", "video"])?;
    assert_eq!(videos.len(), 1);
    assert_eq!(videos[0].title, "workshop.mp4");
    let settings = search(&[
        "Mein Bildschirm ist zu dunkel",
        "--category",
        "settings",
        "--limit",
        "1",
    ])?;
    assert_eq!(settings[0].id, "display.brightness");
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
