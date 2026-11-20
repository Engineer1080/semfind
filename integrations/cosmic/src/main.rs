mod results;
mod semfind;

use anyhow::Result;
use results::{LauncherResult, activate};
use semfind::SearchProcess;
use semfind_core::Hit;
use serde::{Deserialize, Serialize};
use std::{
    io::{self, BufRead, BufWriter, Write},
    process::ExitCode,
    sync::mpsc,
    thread,
};

#[derive(Deserialize)]
enum Request {
    Search(String),
    Activate(u32),
    Context(u32),
    Interrupt,
    Close,
    Exit,
    #[serde(other)]
    Ignored,
}

#[derive(Serialize)]
enum Response {
    Append(LauncherResult),
    Context { id: u32, options: [(); 0] },
    Close,
    Finished,
}

enum Event {
    Request(Request),
    Results(u64, Result<Vec<Hit>>),
    Disconnected,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error)
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|error| error.kind() == io::ErrorKind::BrokenPipe) =>
        {
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("semfind-cosmic: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let (events_tx, events) = mpsc::channel();
    let (search_tx, searches) = mpsc::channel::<(u64, String)>();
    let input_tx = events_tx.clone();
    thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let request = match line {
                Ok(line) => serde_json::from_str(&line),
                Err(error) => {
                    eprintln!("semfind-cosmic: read launcher request: {error}");
                    break;
                }
            };
            match request {
                Ok(request) => {
                    if input_tx.send(Event::Request(request)).is_err() {
                        return;
                    }
                }
                Err(error) => eprintln!("semfind-cosmic: invalid launcher request: {error}"),
            }
        }
        let _ = input_tx.send(Event::Disconnected);
    });
    let worker = thread::spawn(move || {
        let mut process = None;
        for (id, query) in searches {
            let result = match &mut process {
                Some(running) => SearchProcess::search(running, &query),
                None => SearchProcess::start()
                    .and_then(|started| process.insert(started).search(&query)),
            };
            if result.is_err() {
                // Restart on the next request if the CLI exited or its stream failed.
                process = None;
            }
            if events_tx.send(Event::Results(id, result)).is_err() {
                break;
            }
        }
    });

    let mut output = BufWriter::new(io::stdout().lock());
    let mut send = |response: Response| -> Result<()> {
        serde_json::to_writer(&mut output, &response)?;
        writeln!(output)?;
        output.flush()?;
        Ok(())
    };
    let mut next_search = 0u64;
    let mut next_hit = 0u32;
    let mut active = None;
    let mut pending = None;
    let mut busy = false;
    let mut hits = Vec::new();
    for event in events {
        match event {
            Event::Request(Request::Search(query)) => {
                hits.clear();
                let normalized = query.split_whitespace().collect::<Vec<_>>().join(" ");
                let prefix = normalized.split_once(' ');
                let query = match prefix {
                    Some((first, rest)) if first.eq_ignore_ascii_case("semfind") => rest,
                    None if normalized.eq_ignore_ascii_case("semfind") => "",
                    _ => &normalized,
                };
                if query.is_empty() {
                    active = None;
                    pending = None;
                    send(Response::Finished)?;
                } else {
                    next_search += 1;
                    active = Some(next_search);
                    pending = Some((next_search, query.to_owned()));
                }
            }
            Event::Request(Request::Interrupt | Request::Close) => {
                hits.clear();
                pending = None;
                if active.take().is_some() {
                    // Release the launcher immediately; discard the old answer when it arrives.
                    send(Response::Finished)?;
                }
            }
            Event::Request(Request::Exit) | Event::Disconnected => break,
            Event::Request(Request::Activate(id)) => {
                if let Some((_, hit)) = hits.iter().find(|(candidate, _)| *candidate == id) {
                    match activate(hit) {
                        Ok(true) => send(Response::Close)?,
                        Ok(false) => (),
                        Err(error) => eprintln!("semfind-cosmic: {error:#}"),
                    }
                }
            }
            Event::Request(Request::Context(id)) => send(Response::Context { id, options: [] })?,
            Event::Request(Request::Ignored) => (),
            Event::Results(id, result) => {
                busy = false;
                if active == Some(id) {
                    active = None;
                    match result {
                        Ok(results) => {
                            for hit in results {
                                let id = next_hit;
                                next_hit = next_hit.wrapping_add(1);
                                send(Response::Append(LauncherResult::from_hit(id, &hit)))?;
                                hits.push((id, hit));
                            }
                        }
                        Err(error) => eprintln!("semfind-cosmic: {error:#}"),
                    }
                    send(Response::Finished)?;
                }
            }
        }
        if !busy && let Some(search) = pending.take() {
            search_tx.send(search)?;
            busy = true;
        }
    }
    drop(search_tx);
    if !busy {
        let _ = worker.join();
    }
    Ok(())
}
