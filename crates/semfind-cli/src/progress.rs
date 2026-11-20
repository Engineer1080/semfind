use crate::ui::Messages;
use semfind_core::InitProgress;
use std::{
    env,
    io::{self, IsTerminal, Write},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

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
                let [preparing, catalogs, files, applications, commands, saving] =
                    messages.init_progress;
                let (label, count) = match phase {
                    InitProgress::Catalogs => (catalogs, None),
                    InitProgress::Files(count) => (files, Some(count)),
                    InitProgress::Applications => (applications, None),
                    InitProgress::LocalCommands => (commands, None),
                    InitProgress::Saving => (saving, None),
                    _ => (preparing, None),
                };
                let seconds = started.elapsed().as_secs();
                let count = count.map(|count| format!("  {count}")).unwrap_or_default();
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
