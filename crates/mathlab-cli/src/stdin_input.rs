// SPDX-License-Identifier: GPL-3.0-or-later
//! One raw stdin owner serves REPL commands and authentic kernel answers. An
//! expired input task leaves unclassified bytes for the next task. From the
//! first answer byte, cancellation discards that logical line, including a tail
//! arriving later, instead of turning input data into a REPL command.

use crate::streaming::ConsoleCursor;
use mathlab_engine::{Engine, InputHandle, InputKey, InputPhase, MAX_INPUT_BYTES};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const POLL: Duration = Duration::from_millis(25);
const READ_BYTES: usize = 8192;

enum Task {
    Command(mpsc::SyncSender<io::Result<Option<String>>>),
    Input { handle: InputHandle, key: InputKey },
}

#[derive(Clone)]
struct Client {
    sender: mpsc::SyncSender<Task>,
    stop: Arc<AtomicBool>,
}

pub(crate) struct StdinRouter {
    client: Client,
    thread: Option<JoinHandle<()>>,
}

impl StdinRouter {
    pub(crate) fn start() -> io::Result<Self> {
        platform::supported()?;
        let (sender, receiver) = mpsc::sync_channel(8);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let thread = thread::Builder::new()
            .name("mathlab-stdin".into())
            .spawn(move || {
                let mut buffer = LineBuffer::default();
                while !worker_stop.load(Ordering::Acquire) {
                    let task = match receiver.recv_timeout(POLL) {
                        Ok(task) => task,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    serve(task, &mut buffer, &worker_stop);
                }
            })?;
        Ok(Self {
            client: Client { sender, stop },
            thread: Some(thread),
        })
    }

    pub(crate) fn read_command(&self) -> io::Result<Option<String>> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.client
            .sender
            .send(Task::Command(sender))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "stdin router stopped"))?;
        receiver
            .recv()
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "stdin router stopped"))?
    }
}

impl Drop for StdinRouter {
    fn drop(&mut self) {
        self.client.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take().filter(JoinHandle::is_finished) {
            let _ = thread.join();
        }
    }
}

pub(crate) struct InputResponder {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl InputResponder {
    pub(crate) fn start(
        handle: InputHandle,
        router: &StdinRouter,
        cursor: Option<ConsoleCursor>,
    ) -> io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let client = router.client.clone();
        let thread = thread::Builder::new()
            .name("mathlab-input-responder".into())
            .spawn(move || {
                let mut scheduled = None;
                while !worker_stop.load(Ordering::Acquire) && !client.stop.load(Ordering::Acquire) {
                    if let Ok(Some(pending)) = handle.pending() {
                        let key = pending.request.key;
                        if pending.phase == InputPhase::Waiting && scheduled != Some(key) {
                            let visible = cursor.as_ref().map_or(Ok(true), |cursor| {
                                cursor.reached((key.generation, pending.request.console_sequence))
                            });
                            match visible {
                                Ok(false) => {}
                                Err(error) => {
                                    eprintln!("mathlab input: cannot display prompt: {error}");
                                    let _ = handle.eof(&key);
                                    scheduled = Some(key);
                                }
                                Ok(true) => match client.sender.try_send(Task::Input {
                                    handle: handle.clone(),
                                    key,
                                }) {
                                    Ok(()) => scheduled = Some(key),
                                    Err(mpsc::TrySendError::Full(_)) => {}
                                    Err(mpsc::TrySendError::Disconnected(_)) => {
                                        let _ = handle.eof(&key);
                                        scheduled = Some(key);
                                    }
                                },
                            }
                        }
                    }
                    thread::sleep(POLL);
                }
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for InputResponder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take().filter(JoinHandle::is_finished) {
            let _ = thread.join();
        }
    }
}

pub(crate) struct InputSupport {
    _responder: InputResponder,
    _router: StdinRouter,
}

impl InputSupport {
    pub(crate) fn start(
        engine: &Engine,
        cursor: Option<ConsoleCursor>,
    ) -> io::Result<Option<Self>> {
        let Some(handle) = engine.input_handle() else {
            return Ok(None);
        };
        let router = StdinRouter::start()?;
        let responder = InputResponder::start(handle, &router, cursor)?;
        Ok(Some(Self {
            _responder: responder,
            _router: router,
        }))
    }
}

fn waiting(handle: &InputHandle, key: InputKey) -> bool {
    handle
        .pending()
        .ok()
        .flatten()
        .is_some_and(|pending| pending.request.key == key && pending.phase == InputPhase::Waiting)
}

fn serve(task: Task, buffer: &mut LineBuffer, stop: &AtomicBool) {
    let mut answer_started = false;
    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }
        if let Task::Input { handle, key } = &task {
            if !waiting(handle, *key) {
                if answer_started {
                    buffer.discard_answer_line();
                    eprintln!("mathlab input: discarded an unfinished answer because its input request ended");
                }
                return;
            }
            if !buffer.discarding_answer && (!buffer.bytes.is_empty() || buffer.discarding) {
                answer_started = true;
            }
        }
        if let Some(line) = buffer.peek() {
            match &task {
                Task::Command(sender) => {
                    let result = line.and_then(|line| {
                        line.map(|(text, count)| {
                            buffer.consume(count);
                            text
                        })
                        .transpose()
                    });
                    let _ = sender.send(result);
                }
                Task::Input { handle, key } => {
                    if !waiting(handle, *key) {
                        if let Ok(Some((_, count))) = line {
                            buffer.consume(count);
                            eprintln!("mathlab input: discarded an answer whose input request ended before submission");
                        }
                        return;
                    }
                    match line {
                        Ok(Some((text, count))) => match text {
                            Ok(text) => {
                                match handle.reply_with_echo(key, &text, !platform::terminal_echo())
                                {
                                    Ok(()) => buffer.consume(count),
                                    Err(error) if waiting(handle, *key) => {
                                        eprintln!("mathlab input: {error}");
                                        buffer.consume(count);
                                        let _ = handle.eof(key);
                                    }
                                    Err(_) => {
                                        buffer.consume(count);
                                        eprintln!("mathlab input: discarded an answer whose input request ended during submission");
                                    }
                                }
                            }
                            Err(error) => {
                                eprintln!("mathlab input: {error}");
                                buffer.consume(count);
                                let _ = handle.eof(key);
                            }
                        },
                        Ok(None) => {
                            let _ = handle.eof(key);
                        }
                        Err(error) => {
                            eprintln!("mathlab input: {error}");
                            let _ = handle.eof(key);
                        }
                    }
                }
            }
            return;
        }
        // peek() can remove a previously expired answer's tail and expose a
        // fresh partial answer. It belongs to this input even without a newline.
        if matches!(task, Task::Input { .. })
            && !buffer.discarding_answer
            && (!buffer.bytes.is_empty() || buffer.discarding)
        {
            answer_started = true;
        }
        match platform::read(|| {
            !stop.load(Ordering::Acquire)
                && match &task {
                    Task::Input { handle, key } => waiting(handle, *key),
                    Task::Command(_) => true,
                }
        }) {
            Ok(Some(bytes)) if bytes.is_empty() => buffer.eof = true,
            Ok(Some(bytes)) => {
                buffer.bytes.extend(bytes);
                buffer.discard_expired_prefix();
                if matches!(task, Task::Input { .. })
                    && !buffer.discarding_answer
                    && (!buffer.bytes.is_empty() || buffer.discarding)
                {
                    answer_started = true;
                }
            }
            Ok(None) => {}
            Err(error) => {
                match &task {
                    Task::Command(sender) => {
                        let _ = sender.send(Err(error));
                    }
                    Task::Input { handle, key } => {
                        eprintln!("mathlab input: {error}");
                        if answer_started {
                            buffer.discard_answer_line();
                        }
                        let _ = handle.eof(key);
                    }
                }
                return;
            }
        }
    }
}

type PeekedLine = io::Result<Option<(io::Result<String>, usize)>>;

#[derive(Default)]
struct LineBuffer {
    bytes: Vec<u8>,
    eof: bool,
    discarding: bool,
    discarding_answer: bool,
}

impl LineBuffer {
    fn consume(&mut self, count: usize) {
        self.bytes.drain(..count);
    }

    /// Drop one line assigned to the expired input, including any tail that
    /// arrives later, without consuming subsequent complete command lines.
    fn discard_answer_line(&mut self) {
        self.discarding = false;
        if let Some(end) = self.bytes.iter().position(|byte| *byte == b'\n') {
            self.consume(end + 1);
            self.discarding_answer = false;
        } else {
            self.bytes.clear();
            self.discarding_answer = !self.eof;
        }
    }

    fn discard_expired_prefix(&mut self) {
        if !self.discarding_answer {
            return;
        }
        if let Some(end) = self.bytes.iter().position(|byte| *byte == b'\n') {
            self.consume(end + 1);
            self.discarding_answer = false;
        } else {
            self.bytes.clear();
            if self.eof {
                self.discarding_answer = false;
            }
        }
    }

    fn peek(&mut self) -> Option<PeekedLine> {
        self.discard_expired_prefix();
        if self.discarding_answer {
            return None;
        }
        let newline = self.bytes.iter().position(|byte| *byte == b'\n');
        if self.discarding {
            if let Some(end) = newline {
                self.consume(end + 1);
                self.discarding = false;
            } else if self.eof {
                self.bytes.clear();
                self.discarding = false;
            } else {
                self.bytes.clear();
                return None;
            }
            return Some(Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "stdin line exceeds the 1 MiB input limit",
            )));
        }
        if newline.is_none() && self.bytes.len() > MAX_INPUT_BYTES {
            self.discarding = true;
            self.bytes.clear();
            return self.peek();
        }
        if let Some(end) = newline {
            if end > MAX_INPUT_BYTES {
                self.consume(end + 1);
                return Some(Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "stdin line exceeds the 1 MiB input limit",
                )));
            }
            let text_end = if end > 0 && self.bytes[end - 1] == b'\r' {
                end - 1
            } else {
                end
            };
            let text = String::from_utf8(self.bytes[..text_end].to_vec()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "stdin line is not valid UTF-8")
            });
            return Some(Ok(Some((text, end + 1))));
        }
        if self.eof {
            if self.bytes.is_empty() {
                return Some(Ok(None));
            }
            let text = String::from_utf8(self.bytes.clone()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "stdin line is not valid UTF-8")
            });
            return Some(Ok(Some((text, self.bytes.len()))));
        }
        None
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    pub(super) fn supported() -> io::Result<()> {
        Ok(())
    }
    pub(super) fn terminal_echo() -> bool {
        let mut state = std::mem::MaybeUninit::<libc::termios>::uninit();
        unsafe {
            libc::isatty(0) == 1
                && libc::tcgetattr(0, state.as_mut_ptr()) == 0
                && state.assume_init().c_lflag & libc::ECHO != 0
        }
    }
    pub(super) fn read(keep_reading: impl FnOnce() -> bool) -> io::Result<Option<Vec<u8>>> {
        let mut poll = libc::pollfd {
            fd: 0,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut poll, 1, POLL.as_millis() as i32) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::Interrupted {
                Ok(None)
            } else {
                Err(error)
            };
        }
        if ready == 0 {
            return Ok(None);
        }
        if !keep_reading() {
            return Ok(None);
        }
        let mut bytes = vec![0; READ_BYTES];
        let count = unsafe { libc::read(0, bytes.as_mut_ptr().cast(), bytes.len()) };
        if count < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::Interrupted {
                Ok(None)
            } else {
                Err(error)
            };
        }
        bytes.truncate(count as usize);
        Ok(Some(bytes))
    }
}

#[cfg(not(unix))]
mod platform {
    use super::*;
    pub(super) fn supported() -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "CLI input routing currently requires Unix",
        ))
    }
    pub(super) fn terminal_echo() -> bool {
        false
    }
    pub(super) fn read(_keep_reading: impl FnOnce() -> bool) -> io::Result<Option<Vec<u8>>> {
        supported().map(|()| None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn line_peeking_preserves_exact_spaces_crlf_and_uncommitted_command_bytes() {
        let mut buffer = LineBuffer {
            bytes: b"  first  \r\nnext\n".to_vec(),
            ..LineBuffer::default()
        };
        let first = buffer.peek().unwrap().unwrap().unwrap();
        assert_eq!(first.0.unwrap(), "  first  ");
        assert_eq!(
            buffer.peek().unwrap().unwrap().unwrap().0.unwrap(),
            "  first  "
        );
        buffer.consume(first.1);
        assert_eq!(buffer.peek().unwrap().unwrap().unwrap().0.unwrap(), "next");
    }
    #[test]
    fn oversized_lines_are_discarded_boundedly_without_becoming_commands() {
        let mut buffer = LineBuffer {
            bytes: vec![b'x'; MAX_INPUT_BYTES + 1],
            ..LineBuffer::default()
        };
        assert!(buffer.peek().is_none());
        assert!(buffer.bytes.is_empty());
        buffer.bytes.extend_from_slice(b"tail\nvalid\n");
        assert!(buffer.peek().unwrap().is_err());
        assert_eq!(buffer.peek().unwrap().unwrap().unwrap().0.unwrap(), "valid");
    }
    #[test]
    fn expired_partial_answer_discards_its_tail_and_preserves_following_commands() {
        let mut buffer = LineBuffer {
            bytes: b"system('unfinished".to_vec(),
            ..LineBuffer::default()
        };
        assert!(buffer.peek().is_none());
        buffer.discard_answer_line();
        assert!(buffer.bytes.is_empty());
        assert!(buffer.discarding_answer);

        buffer.bytes.extend_from_slice(b" answer');\n:restart\n");
        let (text, count) = buffer.peek().unwrap().unwrap().unwrap();
        assert_eq!(text.unwrap(), ":restart");
        buffer.consume(count);
        assert!(buffer.bytes.is_empty());
        assert!(!buffer.discarding_answer);

        buffer
            .bytes
            .extend_from_slice(b"expired answer\nnext command\n");
        buffer.discard_answer_line();
        assert_eq!(
            buffer.peek().unwrap().unwrap().unwrap().0.unwrap(),
            "next command"
        );
    }
}
