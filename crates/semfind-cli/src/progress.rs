use crate::ui::Messages;
use semfind_core::InitProgress;
use std::{
    env,
    io::{self, IsTerminal, Write},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

fn phase_index(phase: InitProgress) -> usize {
    match phase {
        InitProgress::Catalogs => 1,
        InitProgress::Files(_) => 2,
        InitProgress::Applications => 3,
        InitProgress::LocalCommands => 4,
        InitProgress::Saving => 5,
        _ => 0,
    }
}

pub(crate) struct Timing {
    current: usize,
    started: Instant,
    elapsed: [Duration; 6],
}

impl Timing {
    pub fn start() -> Self {
        Self {
            current: phase_index(InitProgress::Preparing),
            started: Instant::now(),
            elapsed: [Duration::ZERO; 6],
        }
    }

    pub fn update(&mut self, phase: InitProgress) {
        let next = phase_index(phase);
        if next != self.current {
            let now = Instant::now();
            self.elapsed[self.current] += now - self.started;
            self.current = next;
            self.started = now;
        }
    }

    pub fn finish(mut self) -> Self {
        self.elapsed[self.current] += self.started.elapsed();
        self
    }

    pub fn print(self, messages: &Messages) {
        let total: Duration = self.elapsed.iter().sum();
        eprintln!(
            "semfind: {} {:.2} s",
            messages.init_elapsed,
            total.as_secs_f64()
        );
        for (label, duration) in messages.init_progress.into_iter().zip(self.elapsed) {
            eprintln!("  {label} {:.2} s", duration.as_secs_f64());
        }
    }
}

pub(crate) struct Progress {
    phase: Arc<Mutex<InitProgress>>,
    stop: mpsc::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Progress {
    pub fn start(messages: &'static Messages) -> Option<Self> {
        if !io::stderr().is_terminal() || env::var_os("TERM").is_some_and(|term| term == "dumb") {
            return None;
        }
        let phase = Arc::new(Mutex::new(InitProgress::Preparing));
        let state = Arc::clone(&phase);
        let (stop, stopped) = mpsc::channel();
        let started = Instant::now();
        let thread = thread::spawn(move || {
            for frame in ['|', '/', '-', '\\'].into_iter().cycle() {
                let phase = *state.lock().expect("progress state poisoned");
                let label = messages.init_progress[phase_index(phase)];
                let count = match phase {
                    InitProgress::Files(count) => format!("  {count}"),
                    _ => String::new(),
                };
                let seconds = started.elapsed().as_secs();
                {
                    let mut stderr = io::stderr().lock();
                    let _ = write!(
                        stderr,
                        "\r\x1b[K{frame} {label}{count}  [{:02}:{:02}]",
                        seconds / 60,
                        seconds % 60
                    );
                    let _ = stderr.flush();
                }
                if !matches!(
                    stopped.recv_timeout(Duration::from_millis(150)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    break;
                }
            }
            let _ = io::stderr().write_all(b"\r\x1b[K");
        });
        Some(Self {
            phase,
            stop,
            thread: Some(thread),
        })
    }

    pub fn update(&self, phase: InitProgress) {
        *self.phase.lock().expect("progress state poisoned") = phase;
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
