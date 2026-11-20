#![cfg(unix)]

use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

struct Launcher(Child);

impl Launcher {
    fn send(&mut self, request: Value) -> Result<()> {
        writeln!(
            self.0.stdin.as_mut().context("launcher input")?,
            "{request}"
        )?;
        Ok(())
    }
}

impl Drop for Launcher {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for_query(log: &Path, query: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if fs::read_to_string(log).is_ok_and(|log| log.lines().any(|line| line == query)) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    }
    anyhow::bail!("the mock CLI did not receive {query:?}");
}

#[test]
fn interrupts_a_search_reuses_the_stream_and_recovers_from_cli_exit() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let command = temp.path().join("semfind");
    let log = temp.path().join("cli.log");
    let release = temp.path().join("release-search");
    fs::write(
        &command,
        r#"#!/bin/sh
[ "$*" = "search --stdin --limit 8" ] || exit 64
printf 'start\n' >> "$TEST_LOG"
while IFS= read -r query; do
    printf '%s\n' "$query" >> "$TEST_LOG"
    case "$query" in
        slow)
            for attempt in 1 2 3 4 5 6 7 8 9 10; do
                [ -f "$TEST_RELEASE" ] && break
                sleep 1
            done
            ;;
        fail) exit 1 ;;
    esac
    printf '[{"id":"demo","category":"command","title":"%s","exact":true,"score":1.0,"snippet":"Manual excerpt","example":"demo --help"}]\n' "$query"
done
"#,
    )?;
    fs::set_permissions(&command, fs::Permissions::from_mode(0o700))?;
    let mut launcher = Launcher(
        Command::new(env!("CARGO_BIN_EXE_semfind-cosmic"))
            .env("SEMFIND_CLI", &command)
            .env("TEST_LOG", &log)
            .env("TEST_RELEASE", &release)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let stdout = launcher.0.stdout.take().context("launcher output")?;
    let (send, receive) = mpsc::channel();
    let reader = thread::spawn(move || -> Result<()> {
        for line in BufReader::new(stdout).lines() {
            let response: Value = serde_json::from_str(&line?)?;
            if send.send(response).is_err() {
                break;
            }
        }
        Ok(())
    });
    let response = || receive.recv_timeout(Duration::from_secs(5));

    launcher.send(json!({"Search": "slow"}))?;
    wait_for_query(&log, "slow")?;
    launcher.send(json!("Interrupt"))?;
    assert_eq!(response()?, "Finished");
    launcher.send(json!({"Search": "  SemFind  fresh\nrequest  "}))?;
    fs::write(&release, "")?;
    let hit = response()?;
    assert_eq!(hit["Append"]["name"], "fresh request");
    assert!(
        hit["Append"]["description"]
            .as_str()
            .unwrap()
            .contains("demo --help")
    );
    assert_eq!(response()?, "Finished");
    assert_eq!(fs::read_to_string(&log)?, "start\nslow\nfresh request\n");

    launcher.send(json!({"Search": "fail"}))?;
    assert_eq!(response()?, "Finished");
    launcher.send(json!({"Search": "recovered"}))?;
    assert_eq!(response()?["Append"]["name"], "recovered");
    assert_eq!(response()?, "Finished");
    assert_eq!(
        fs::read_to_string(&log)?,
        "start\nslow\nfresh request\nfail\nstart\nrecovered\n"
    );
    launcher.send(json!("Exit"))?;
    assert!(matches!(
        receive.recv_timeout(Duration::from_secs(5)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
    assert!(launcher.0.wait()?.success());
    reader.join().expect("launcher reader panicked")?;
    Ok(())
}
