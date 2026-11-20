use anyhow::{Context, Result, ensure};
use semfind_core::Hit;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

#[derive(Deserialize)]
struct Config {
    semfind: PathBuf,
    #[serde(default)]
    environment: BTreeMap<String, String>,
}

impl Config {
    fn load() -> Result<Self> {
        let path = env::current_exe()?.with_file_name("config.json");
        match fs::read(&path) {
            Ok(data) => serde_json::from_slice(&data)
                .with_context(|| format!("read launcher configuration {}", path.display())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self {
                semfind: env::var_os("SEMFIND_CLI").map_or_else(|| "semfind".into(), PathBuf::from),
                environment: BTreeMap::new(),
            }),
            Err(error) => Err(error)
                .with_context(|| format!("read launcher configuration {}", path.display())),
        }
    }
}

pub(crate) struct SearchProcess {
    child: Child,
    input: Option<ChildStdin>,
    output: Option<BufReader<ChildStdout>>,
}

impl SearchProcess {
    pub fn start() -> Result<Self> {
        let config = Config::load()?;
        let mut child = Command::new(&config.semfind)
            .args(["search", "--stdin", "--limit", "8"])
            .envs(&config.environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .with_context(|| format!("start {}", config.semfind.display()))?;
        Ok(Self {
            input: child.stdin.take(),
            output: child.stdout.take().map(BufReader::new),
            child,
        })
    }

    pub fn search(&mut self, query: &str) -> Result<Vec<Hit>> {
        let input = self.input.as_mut().expect("semfind stdin was piped");
        writeln!(input, "{query}")?;
        let mut line = String::new();
        let output = self.output.as_mut().expect("semfind stdout was piped");
        ensure!(
            output.read_line(&mut line)? > 0,
            "semfind closed its output"
        );
        serde_json::from_str(&line).context("decode semfind search results")
    }
}

impl Drop for SearchProcess {
    fn drop(&mut self) {
        // EOF lets the CLI close its SQLite connection and reap its own model worker.
        drop(self.input.take());
        drop(self.output.take());
        let _ = self.child.wait();
    }
}
