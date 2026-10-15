// SPDX-License-Identifier: GPL-3.0-or-later
//! Live CLI output consumes the same bounded byte stream as the desktop. The
//! execution thread never owns stdout or waits on a slow terminal while running.

use mathlab_engine::{ConsoleEventKind, ConsoleSubscription, Engine, ExecutionResult};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Default)]
struct Progress {
    position: (u64, u64),
    error: Option<String>,
    finished: bool,
}

#[derive(Clone)]
pub(crate) struct ConsoleCursor {
    progress: Arc<(Mutex<Progress>, Condvar)>,
}

impl ConsoleCursor {
    pub(crate) fn reached(&self, position: (u64, u64)) -> io::Result<bool> {
        let progress = self
            .progress
            .0
            .lock()
            .map_err(|_| io::Error::other("console printer state is poisoned"))?;
        if let Some(error) = &progress.error {
            return Err(io::Error::other(error.clone()));
        }
        Ok(progress.position >= position)
    }
}

pub(crate) struct ConsolePrinter {
    subscription: Arc<ConsoleSubscription>,
    progress: Arc<(Mutex<Progress>, Condvar)>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ConsolePrinter {
    pub(crate) fn cursor(&self) -> ConsoleCursor {
        ConsoleCursor {
            progress: self.progress.clone(),
        }
    }
    pub(crate) fn start(engine: &Engine) -> io::Result<Self> {
        let subscription = Arc::new(engine.subscribe_console());
        let progress = Arc::new((
            Mutex::new(Progress {
                position: subscription.initial_sequence(),
                ..Progress::default()
            }),
            Condvar::new(),
        ));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_subscription = Arc::clone(&subscription);
        let thread_progress = Arc::clone(&progress);
        let thread_stop = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("mathlab-console-printer".into())
            .spawn(move || {
                let stdout = io::stdout();
                while !thread_stop.load(Ordering::Acquire) {
                    let event = match thread_subscription.recv_timeout(Duration::from_millis(25)) {
                        Ok(event) => event,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    let outcome = match &event.kind {
                        ConsoleEventKind::Output { bytes } => {
                            let mut output = stdout.lock();
                            output.write_all(bytes).and_then(|()| output.flush())
                        }
                        ConsoleEventKind::OutputLimit { dropped_bytes } => writeln!(
                            io::stderr().lock(),
                            "[MathLab console: {dropped_bytes} output bytes dropped because the terminal fell behind]"
                        ),
                        _ => Ok(()),
                    };
                    let (lock, changed) = &*thread_progress;
                    let Ok(mut progress) = lock.lock() else {
                        return;
                    };
                    if let Err(error) = outcome {
                        progress.error = Some(error.to_string());
                        progress.finished = true;
                        changed.notify_all();
                        return;
                    }
                    progress.position = progress.position.max((event.generation, event.sequence));
                    changed.notify_all();
                }
                let (lock, changed) = &*thread_progress;
                if let Ok(mut progress) = lock.lock() {
                    progress.finished = true;
                    changed.notify_all();
                }
            })?;
        Ok(Self {
            subscription,
            progress,
            stop,
            thread: Some(thread),
        })
    }

    pub(crate) fn synchronize_result(&self, result: &ExecutionResult) -> io::Result<()> {
        self.synchronize((result.console_generation, result.console_end_sequence))
    }

    /// Capture one fixed position. Waiting on a moving latest position would let
    /// a background producer prevent the CLI from ever displaying completion.
    pub(crate) fn synchronize_latest(&self) -> io::Result<()> {
        self.synchronize(self.subscription.latest_sequence())
    }

    fn synchronize(&self, position: (u64, u64)) -> io::Result<()> {
        let (lock, changed) = &*self.progress;
        let mut progress = lock
            .lock()
            .map_err(|_| io::Error::other("console printer state is poisoned"))?;
        loop {
            if let Some(error) = &progress.error {
                return Err(io::Error::other(error.clone()));
            }
            if progress.position >= position {
                return Ok(());
            }
            if progress.finished {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "console printer stopped before all received output was displayed",
                ));
            }
            progress = changed
                .wait(progress)
                .map_err(|_| io::Error::other("console printer state is poisoned"))?;
        }
    }
}

impl Drop for ConsolePrinter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // A redirected stdout can block inside write_all. Never hold engine
        // teardown hostage to that writer; dropping its JoinHandle detaches it.
        match self.thread.take() {
            Some(thread) if thread.is_finished() => {
                let _ = thread.join();
            }
            _ => {}
        }
    }
}
