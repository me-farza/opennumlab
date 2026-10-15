// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded session console events and a merged, cancellable byte transport.
use crate::input::{InputFinished, InputHub, InputKey, InputOutcome, InputRequest};
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const READ_BYTES: usize = 8192;
const MAX_FENCE: usize = 512;
const CAPTURE_BYTES: usize = 32 * 1024 * 1024;
const SUBSCRIBER_BYTES: usize = 1024 * 1024;
const SUBSCRIBER_EVENTS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleEvent {
    pub generation: u64,
    /// Transport context, not proof that a background writer belongs to this request.
    pub request_id: Option<u64>,
    pub sequence: u64,
    pub kind: ConsoleEventKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleEventKind {
    Begin,
    Output {
        bytes: Vec<u8>,
    },
    /// This subscriber fell behind. Its sequence advances past the evicted events.
    OutputLimit {
        dropped_bytes: u64,
    },
    End,
    InputRequested {
        request: InputRequest,
    },
    InputFinished {
        finished: InputFinished,
    },
    Disconnected {
        message: String,
    },
}

#[derive(Debug, Default)]
struct SubscriberQueue {
    events: VecDeque<ConsoleEvent>,
    bytes: usize,
    lost: Option<ConsoleEvent>,
    closed: bool,
    latest: (u64, u64),
}

#[derive(Debug, Default)]
struct Subscriber {
    queue: Mutex<SubscriberQueue>,
    changed: Condvar,
}

#[derive(Debug, Default)]
struct HubState {
    generation: u64,
    sequence: u64,
    subscribers: Vec<Weak<Subscriber>>,
    diagnostic_tail: VecDeque<u8>,
}

impl Drop for HubState {
    fn drop(&mut self) {
        for subscriber in self.subscribers.iter().filter_map(Weak::upgrade) {
            if let Ok(mut queue) = subscriber.queue.lock() {
                queue.closed = true;
                subscriber.changed.notify_all();
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ConsoleHub(Arc<Mutex<HubState>>);

impl ConsoleHub {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn subscribe(&self) -> ConsoleSubscription {
        let subscriber = Arc::new(Subscriber::default());
        let mut initial_sequence = (0, 0);
        if let Ok(mut state) = self.0.lock() {
            initial_sequence = (state.generation, state.sequence);
            subscriber.queue.lock().unwrap().latest = initial_sequence;
            state.subscribers.retain(|item| item.strong_count() != 0);
            state.subscribers.push(Arc::downgrade(&subscriber));
        }
        ConsoleSubscription {
            subscriber,
            hub: Arc::downgrade(&self.0),
            initial_sequence,
        }
    }

    fn publish(&self, generation: u64, request_id: Option<u64>, kind: ConsoleEventKind) -> u64 {
        self.publish_with_sequence(generation, request_id, |_| Ok(kind))
            .unwrap_or(0)
    }

    fn publish_with_sequence(
        &self,
        generation: u64,
        request_id: Option<u64>,
        make_kind: impl FnOnce(u64) -> io::Result<ConsoleEventKind>,
    ) -> io::Result<u64> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| protocol_error("Console hub lock poisoned"))?;
        let sequence = state.sequence + 1;
        // Input state receives this exact cursor before a subscriber can wake
        // and attempt a reply. Lock order is console hub -> input state only.
        let kind = make_kind(sequence)?;
        if state.generation != generation {
            state.diagnostic_tail.clear();
        }
        if let ConsoleEventKind::Output { bytes } = &kind {
            let keep = bytes.len().min(8192);
            let remove = state
                .diagnostic_tail
                .len()
                .saturating_add(keep)
                .saturating_sub(8192);
            state.diagnostic_tail.drain(..remove);
            state.diagnostic_tail.extend(&bytes[bytes.len() - keep..]);
        }
        state.sequence = sequence;
        state.generation = generation;
        let event = ConsoleEvent {
            generation,
            request_id,
            sequence: state.sequence,
            kind,
        };
        state.subscribers.retain(|item| {
            let Some(subscriber) = item.upgrade() else {
                return false;
            };
            if let Ok(mut queue) = subscriber.queue.lock() {
                queue.latest = (generation, event.sequence);
                let incoming = event_bytes(&event);
                while queue.events.len() >= SUBSCRIBER_EVENTS
                    || queue.bytes.saturating_add(incoming) > SUBSCRIBER_BYTES
                {
                    let Some(evicted) = queue.events.pop_front() else {
                        break;
                    };
                    let dropped = event_bytes(&evicted);
                    queue.bytes -= dropped;
                    let lost_output = match &evicted.kind {
                        ConsoleEventKind::Output { bytes } => bytes.len() as u64,
                        _ => 0,
                    };
                    let previous = queue.lost.as_ref().map_or(0, |event| match event.kind {
                        ConsoleEventKind::OutputLimit { dropped_bytes } => dropped_bytes,
                        _ => 0,
                    });
                    queue.lost = Some(ConsoleEvent {
                        generation: evicted.generation,
                        request_id: None,
                        sequence: evicted.sequence,
                        kind: ConsoleEventKind::OutputLimit {
                            dropped_bytes: previous.saturating_add(lost_output),
                        },
                    });
                }
                queue.bytes += incoming;
                queue.events.push_back(event.clone());
                subscriber.changed.notify_one();
            }
            true
        });
        Ok(event.sequence)
    }

    pub(crate) fn diagnostics(&self, generation: u64) -> String {
        let Ok(state) = self.0.lock() else {
            return String::new();
        };
        if state.generation != generation {
            return String::new();
        }
        let bytes: Vec<u8> = state.diagnostic_tail.iter().copied().collect();
        let text = String::from_utf8_lossy(&bytes);
        let offset = text
            .char_indices()
            .rev()
            .nth(1999)
            .map_or(0, |(offset, _)| offset);
        text[offset..].trim().to_owned()
    }
}

fn event_bytes(event: &ConsoleEvent) -> usize {
    match &event.kind {
        ConsoleEventKind::Output { bytes } => bytes.len(),
        ConsoleEventKind::Disconnected { message } => message.len(),
        ConsoleEventKind::InputRequested { request } => request.prompt.len(),
        _ => 0,
    }
}

/// An independent bounded stream. Slow consumers lose old events with an explicit
/// loss marker; they never block computation, cancellation, or another consumer.
#[derive(Debug)]
pub struct ConsoleSubscription {
    subscriber: Arc<Subscriber>,
    hub: Weak<Mutex<HubState>>,
    initial_sequence: (u64, u64),
}

impl ConsoleSubscription {
    pub fn initial_sequence(&self) -> (u64, u64) {
        self.initial_sequence
    }

    pub fn latest_sequence(&self) -> (u64, u64) {
        self.hub
            .upgrade()
            .and_then(|hub| {
                hub.lock()
                    .ok()
                    .map(|state| (state.generation, state.sequence))
            })
            .unwrap_or_else(|| {
                self.subscriber
                    .queue
                    .lock()
                    .map_or(self.initial_sequence, |queue| queue.latest)
            })
    }

    pub fn try_recv(&self) -> Result<ConsoleEvent, mpsc::TryRecvError> {
        let mut queue = self
            .subscriber
            .queue
            .lock()
            .map_err(|_| mpsc::TryRecvError::Disconnected)?;
        take_event(&mut queue).ok_or(if queue.closed {
            mpsc::TryRecvError::Disconnected
        } else {
            mpsc::TryRecvError::Empty
        })
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<ConsoleEvent, mpsc::RecvTimeoutError> {
        let started = Instant::now();
        let mut queue = self
            .subscriber
            .queue
            .lock()
            .map_err(|_| mpsc::RecvTimeoutError::Disconnected)?;
        loop {
            if let Some(event) = take_event(&mut queue) {
                return Ok(event);
            }
            if queue.closed {
                return Err(mpsc::RecvTimeoutError::Disconnected);
            }
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(mpsc::RecvTimeoutError::Timeout);
            }
            let (next, _) = self
                .subscriber
                .changed
                .wait_timeout(queue, remaining)
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)?;
            queue = next;
        }
    }
}

fn take_event(queue: &mut SubscriberQueue) -> Option<ConsoleEvent> {
    if let Some(lost) = queue.lost.take() {
        return Some(lost);
    }
    let event = queue.events.pop_front()?;
    queue.bytes -= event_bytes(&event);
    Some(event)
}

pub(crate) struct MergedPipe {
    reader: File,
    writer: File,
}

impl MergedPipe {
    pub(crate) fn new() -> io::Result<Self> {
        platform::pipe().map(|(reader, writer)| Self { reader, writer })
    }

    pub(crate) fn stdout(&self) -> io::Result<Stdio> {
        Ok(Stdio::from(self.writer.try_clone()?))
    }

    pub(crate) fn stderr(&self) -> io::Result<Stdio> {
        Ok(Stdio::from(self.writer.try_clone()?))
    }

    pub(crate) fn into_reader(self) -> File {
        self.reader
    }
}

#[derive(Debug, Default)]
pub(crate) struct ConsoleCapture {
    pub output: String,
    pub truncated: bool,
    pub dropped_bytes: u64,
    pub decoding_loss: bool,
    pub end_sequence: u64,
}

pub(crate) enum TransportEvent {
    Begin {
        invocation: u64,
        request_id: u64,
    },
    Complete {
        invocation: u64,
        request_id: u64,
        capture: ConsoleCapture,
    },
    Recovery {
        invocation: u64,
        request_id: u64,
        capture: Option<ConsoleCapture>,
    },
    Failed(String),
    Closed,
}

pub(crate) struct ConsoleTransport {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    receiver: mpsc::Receiver<TransportEvent>,
}

impl ConsoleTransport {
    pub(crate) fn start(
        mut reader: File,
        nonce: String,
        generation: u64,
        hub: ConsoleHub,
        input: InputHub,
    ) -> io::Result<Self> {
        let mut parser = FrameParser::new(&nonce, generation)?;
        let stop = Arc::new(AtomicBool::new(false));
        let reader_stop = stop.clone();
        // Requests are sequential. Four control slots cover completion/recovery
        // handshakes without an unbounded queue of large captured results.
        let (sender, receiver) = mpsc::sync_channel(4);
        let thread = thread::Builder::new()
            .name("mathlab-console".into())
            .spawn(move || {
                let mut state = ProtocolState {
                    input: Some(input),
                    ..ProtocolState::default()
                };
                let mut bytes = [0_u8; READ_BYTES];
                let outcome = (|| -> io::Result<()> {
                    loop {
                        if reader_stop.load(Ordering::Acquire) {
                            // Preserve ordinary bytes held only because their suffix
                            // resembles a fence prefix. A partial known control fence
                            // is discarded when explicitly stopping this generation.
                            if let Ok(items) = parser.finish() {
                                for item in items {
                                    consume(item, generation, &hub, &sender, &mut state)?;
                                }
                            }
                            close_input(&state, generation, &hub, InputOutcome::Disconnected);
                            hub.publish(
                                generation,
                                state.active.as_ref().map(|value| value.id),
                                ConsoleEventKind::Disconnected {
                                    message: "Engine console stopped".into(),
                                },
                            );
                            let _ = sender.try_send(TransportEvent::Closed);
                            return Ok(());
                        }
                        match platform::read(&mut reader, &mut bytes) {
                            Ok(Some(0)) => {
                                for item in parser.finish()? {
                                    consume(item, generation, &hub, &sender, &mut state)?;
                                }
                                close_input(&state, generation, &hub, InputOutcome::Disconnected);
                                hub.publish(
                                    generation,
                                    state.active.as_ref().map(|value| value.id),
                                    ConsoleEventKind::Disconnected {
                                        message: "Engine console closed".into(),
                                    },
                                );
                                let _ = sender.try_send(TransportEvent::Closed);
                                return Ok(());
                            }
                            Ok(Some(count)) => {
                                let (items, error) = match parser.feed(&bytes[..count]) {
                                    Ok(items) => (items, None),
                                    Err(failure) => (failure.items, Some(failure.error)),
                                };
                                for item in items {
                                    consume(item, generation, &hub, &sender, &mut state)?;
                                }
                                if let Some(error) = error {
                                    return Err(error);
                                }
                            }
                            Ok(None) => platform::wait(&reader)?,
                            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                            Err(error) => return Err(error),
                        }
                    }
                })();
                if let Err(error) = outcome {
                    let message = error.to_string();
                    close_input(&state, generation, &hub, InputOutcome::Error);
                    hub.publish(
                        generation,
                        state.active.as_ref().map(|value| value.id),
                        ConsoleEventKind::Disconnected {
                            message: message.clone(),
                        },
                    );
                    let _ = sender.try_send(TransportEvent::Failed(message));
                }
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
            receiver,
        })
    }

    pub(crate) fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<TransportEvent, mpsc::RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }

    pub(crate) fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for ConsoleTransport {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Capture {
    id: u64,
    bytes: Vec<u8>,
    dropped: u64,
}

struct ProtocolState {
    invocation: u64,
    closed: bool,
    completed: Option<u64>,
    active: Option<Capture>,
    last_input: u64,
    input: Option<InputHub>,
}

impl Default for ProtocolState {
    fn default() -> Self {
        Self {
            invocation: 1,
            closed: false,
            completed: None,
            active: None,
            last_input: 0,
            input: None,
        }
    }
}

fn finish_capture(capture: Capture, generation: u64, hub: &ConsoleHub) -> ConsoleCapture {
    let decoding_loss = std::str::from_utf8(&capture.bytes).is_err();
    let output = String::from_utf8_lossy(&capture.bytes).into_owned();
    let end_sequence = hub.publish(generation, Some(capture.id), ConsoleEventKind::End);
    ConsoleCapture {
        output,
        truncated: capture.dropped != 0,
        dropped_bytes: capture.dropped,
        decoding_loss,
        end_sequence,
    }
}

fn close_input(state: &ProtocolState, generation: u64, hub: &ConsoleHub, reason: InputOutcome) {
    if let Some(input) = &state.input
        && let Some(finished) = input.invalidate(reason)
    {
        hub.publish(
            generation,
            Some(finished.key.request_id),
            ConsoleEventKind::InputFinished { finished },
        );
    }
}

fn consume(
    item: FrameItem,
    generation: u64,
    hub: &ConsoleHub,
    sender: &mpsc::SyncSender<TransportEvent>,
    state: &mut ProtocolState,
) -> io::Result<()> {
    if let FrameItem::Begin(invocation, id)
    | FrameItem::End(invocation, id)
    | FrameItem::Recovery(invocation, id)
    | FrameItem::InputBegin(invocation, id, _)
    | FrameItem::InputEnd(invocation, id, _) = &item
    {
        if *invocation < state.invocation {
            // A fence helper can outlive the interrupted invocation that
            // launched it. Its late atomic write cannot reopen an old capture.
            return Ok(());
        }
        if *invocation > state.invocation {
            if state.invocation.checked_add(1) != Some(*invocation)
                || !state.closed
                || !matches!(item, FrameItem::Begin(_, 0))
            {
                return Err(protocol_error("Unexpected console worker invocation"));
            }
            state.invocation = *invocation;
            state.closed = false;
            state.completed = None;
            state.last_input = 0;
        } else if state.closed
            || (state.completed.is_some_and(|completed| *id <= completed)
                && !matches!(item, FrameItem::Recovery(_, _)))
        {
            return Ok(());
        }
    }
    let control = match item {
        FrameItem::Bytes(bytes) => {
            if let Some(capture) = state.active.as_mut() {
                let keep = bytes.len().min(CAPTURE_BYTES - capture.bytes.len());
                capture.bytes.extend_from_slice(&bytes[..keep]);
                capture.dropped = capture.dropped.saturating_add((bytes.len() - keep) as u64);
            }
            hub.publish(
                generation,
                state.active.as_ref().map(|value| value.id),
                ConsoleEventKind::Output { bytes },
            );
            return Ok(());
        }
        FrameItem::Begin(invocation, id) => {
            if state.active.is_some() {
                return Err(protocol_error("Nested console Begin fence"));
            }
            state.active = Some(Capture {
                id,
                bytes: Vec::new(),
                dropped: 0,
            });
            hub.publish(generation, Some(id), ConsoleEventKind::Begin);
            TransportEvent::Begin {
                invocation,
                request_id: id,
            }
        }
        FrameItem::End(invocation, id) => {
            let capture = state
                .active
                .take()
                .ok_or_else(|| protocol_error("Console End without Begin"))?;
            if capture.id != id {
                return Err(protocol_error("Console End does not match its Begin"));
            }
            close_input(state, generation, hub, InputOutcome::Error);
            state.completed = Some(id);
            TransportEvent::Complete {
                invocation,
                request_id: id,
                capture: finish_capture(capture, generation, hub),
            }
        }
        FrameItem::Recovery(invocation, id) => {
            close_input(state, generation, hub, InputOutcome::Interrupted);
            let capture = if let Some(capture) = state.active.take() {
                if capture.id != id {
                    return Err(protocol_error("Console recovery does not match its Begin"));
                }
                Some(finish_capture(capture, generation, hub))
            } else if state.completed.is_some_and(|completed| id <= completed) {
                // The normal End won. Recovery still requires worker re-entry,
                // but it must not replace that result or emit another public End.
                None
            } else {
                // SIGINT can run cleanup before the Begin helper writes. No
                // user statement ran; expose a complete empty interrupted span.
                hub.publish(generation, Some(id), ConsoleEventKind::Begin);
                Some(finish_capture(
                    Capture {
                        id,
                        bytes: Vec::new(),
                        dropped: 0,
                    },
                    generation,
                    hub,
                ))
            };
            state.closed = true;
            state.completed = Some(id);
            TransportEvent::Recovery {
                invocation,
                request_id: id,
                capture,
            }
        }
        FrameItem::InputBegin(invocation, id, input_id) => {
            if state.active.as_ref().is_none_or(|capture| capture.id != id)
                || input_id <= state.last_input
            {
                return Err(protocol_error(
                    "Input request does not match an active console capture",
                ));
            }
            let input = state
                .input
                .as_ref()
                .ok_or_else(|| protocol_error("Input transport unavailable"))?;
            let key = InputKey {
                generation,
                invocation,
                request_id: id,
                input_id,
            };
            let metadata = input
                .read_requested(key)
                .map_err(|error| protocol_error(error.to_string()))?;
            hub.publish_with_sequence(generation, Some(id), |sequence| {
                let request = input
                    .apply_requested(metadata, sequence)
                    .map_err(|error| protocol_error(error.to_string()))?;
                Ok(ConsoleEventKind::InputRequested { request })
            })?;
            state.last_input = input_id;
            return Ok(());
        }
        FrameItem::InputEnd(invocation, id, input_id) => {
            if state.active.as_ref().is_none_or(|capture| capture.id != id)
                || input_id != state.last_input
            {
                return Err(protocol_error(
                    "Input completion does not match an active console capture",
                ));
            }
            let input = state
                .input
                .as_ref()
                .ok_or_else(|| protocol_error("Input transport unavailable"))?;
            let key = InputKey {
                generation,
                invocation,
                request_id: id,
                input_id,
            };
            let finished = input
                .read_finished(key)
                .map_err(|error| protocol_error(error.to_string()))?;
            hub.publish_with_sequence(generation, Some(id), |_| {
                let finished = input
                    .apply_finished(finished)
                    .map_err(|error| protocol_error(error.to_string()))?;
                Ok(ConsoleEventKind::InputFinished { finished })
            })?;
            return Ok(());
        }
    };
    sender
        .try_send(control)
        .map_err(|error| protocol_error(format!("Console control channel unavailable: {error}")))
}

fn protocol_error(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[derive(Debug, PartialEq, Eq)]
enum FrameItem {
    Bytes(Vec<u8>),
    Begin(u64, u64),
    End(u64, u64),
    Recovery(u64, u64),
    InputBegin(u64, u64, u64),
    InputEnd(u64, u64, u64),
}

struct FrameParser {
    prefix: Vec<u8>,
    pending: Vec<u8>,
}

#[derive(Debug)]
struct ParseFailure {
    items: Vec<FrameItem>,
    error: io::Error,
}

impl FrameParser {
    fn new(nonce: &str, generation: u64) -> io::Result<Self> {
        if nonce.is_empty()
            || nonce.len() > 128
            || !nonce
                .bytes()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'_' | b'-'))
        {
            return Err(protocol_error("Invalid console generation nonce"));
        }
        Ok(Self {
            prefix: format!("\x1eMATHLAB:{nonce}:{generation}:").into_bytes(),
            pending: Vec::new(),
        })
    }

    fn feed(&mut self, bytes: &[u8]) -> Result<Vec<FrameItem>, ParseFailure> {
        let mut items = Vec::new();
        match self.feed_into(bytes, &mut items) {
            Ok(()) => Ok(items),
            Err(error) => Err(ParseFailure { items, error }),
        }
    }

    fn feed_into(&mut self, bytes: &[u8], items: &mut Vec<FrameItem>) -> io::Result<()> {
        self.pending.extend_from_slice(bytes);
        loop {
            let position = self
                .pending
                .windows(self.prefix.len())
                .position(|item| item == self.prefix);
            let Some(position) = position else {
                let carry = (1..self.prefix.len().min(self.pending.len() + 1))
                    .rev()
                    .find(|count| self.pending.ends_with(&self.prefix[..*count]))
                    .unwrap_or(0);
                let emit = self.pending.len() - carry;
                if emit != 0 {
                    items.push(FrameItem::Bytes(self.pending.drain(..emit).collect()));
                }
                return Ok(());
            };
            if position != 0 {
                items.push(FrameItem::Bytes(self.pending.drain(..position).collect()));
            }
            let Some(end) = self.pending[self.prefix.len()..]
                .iter()
                .position(|byte| *byte == 0x1f)
                .map(|end| end + self.prefix.len())
            else {
                if self.pending.len() > MAX_FENCE {
                    return Err(protocol_error("Console fence exceeds 512 bytes"));
                }
                return Ok(());
            };
            if end + 1 > MAX_FENCE {
                return Err(protocol_error("Console fence exceeds 512 bytes"));
            }
            let body = &self.pending[self.prefix.len()..end];
            if body.len() < 5 || body[1] != b':' {
                return Err(protocol_error("Malformed current-generation console fence"));
            }
            let mut fields = body[2..].split(|byte| *byte == b':');
            let mut parse_identifier = || {
                fields
                    .next()
                    .filter(|value| !value.is_empty() && value.iter().all(u8::is_ascii_digit))
                    .and_then(|value| std::str::from_utf8(value).ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .ok_or_else(|| protocol_error("Invalid console invocation/request identifier"))
            };
            let invocation = parse_identifier()?;
            let id = parse_identifier()?;
            let input_id = if matches!(body[0], b'I' | b'F') {
                Some(parse_identifier()?)
            } else {
                None
            };
            if invocation == 0 || fields.next().is_some() {
                return Err(protocol_error(
                    "Malformed console invocation/request identifier",
                ));
            }
            items.push(match body[0] {
                b'B' => FrameItem::Begin(invocation, id),
                b'E' => FrameItem::End(invocation, id),
                b'R' => FrameItem::Recovery(invocation, id),
                b'I' if input_id != Some(0) => {
                    FrameItem::InputBegin(invocation, id, input_id.unwrap())
                }
                b'F' if input_id != Some(0) => {
                    FrameItem::InputEnd(invocation, id, input_id.unwrap())
                }
                _ => return Err(protocol_error("Unknown current-generation console fence")),
            });
            self.pending.drain(..=end);
        }
    }

    fn finish(&mut self) -> io::Result<Vec<FrameItem>> {
        if self.pending.starts_with(&self.prefix) {
            return Err(protocol_error(
                "Engine closed in the middle of a console fence",
            ));
        }
        if self.pending.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(vec![FrameItem::Bytes(std::mem::take(&mut self.pending))])
        }
    }
}

pub(crate) fn write_fence_helper(directory: &Path) -> io::Result<PathBuf> {
    let path = directory.join(if cfg!(windows) {
        "console-fence.exe"
    } else {
        "console-fence"
    });
    fs::write(
        &path,
        include_bytes!(env!("MATHLAB_CONSOLE_FENCE_EXECUTABLE")),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(path)
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::os::fd::{AsRawFd, FromRawFd};

    pub(super) fn pipe() -> io::Result<(File, File)> {
        let mut descriptors = [0; 2];
        if unsafe { libc::pipe(descriptors.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let reader = unsafe { File::from_raw_fd(descriptors[0]) };
        let writer = unsafe { File::from_raw_fd(descriptors[1]) };
        for descriptor in descriptors {
            if unsafe { libc::fcntl(descriptor, libc::F_SETFD, libc::FD_CLOEXEC) } == -1 {
                return Err(io::Error::last_os_error());
            }
        }
        let flags = unsafe { libc::fcntl(reader.as_raw_fd(), libc::F_GETFL) };
        if flags == -1
            || unsafe { libc::fcntl(reader.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) }
                == -1
        {
            return Err(io::Error::last_os_error());
        }
        Ok((reader, writer))
    }

    pub(super) fn read(reader: &mut File, bytes: &mut [u8]) -> io::Result<Option<usize>> {
        match reader.read(bytes) {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            result => result.map(Some),
        }
    }

    pub(super) fn wait(reader: &File) -> io::Result<()> {
        let mut poll = libc::pollfd {
            fd: reader.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut poll, 1, 25) };
        if result == -1 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::ffi::c_void;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    #[repr(C)]
    struct SecurityAttributes {
        length: u32,
        descriptor: *mut c_void,
        inherit: i32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreatePipe(
            read: *mut *mut c_void,
            write: *mut *mut c_void,
            attributes: *mut SecurityAttributes,
            size: u32,
        ) -> i32;
        fn SetHandleInformation(handle: *mut c_void, mask: u32, flags: u32) -> i32;
        fn PeekNamedPipe(
            handle: *mut c_void,
            buffer: *mut c_void,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            left: *mut u32,
        ) -> i32;
    }
    pub(super) fn pipe() -> io::Result<(File, File)> {
        let mut attributes = SecurityAttributes {
            length: std::mem::size_of::<SecurityAttributes>() as u32,
            descriptor: std::ptr::null_mut(),
            inherit: 1,
        };
        let (mut read, mut write) = (std::ptr::null_mut(), std::ptr::null_mut());
        if unsafe { CreatePipe(&mut read, &mut write, &mut attributes, 65536) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let reader = unsafe { File::from_raw_handle(read) };
        let writer = unsafe { File::from_raw_handle(write) };
        if unsafe { SetHandleInformation(read, 1, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((reader, writer))
    }
    pub(super) fn read(reader: &mut File, bytes: &mut [u8]) -> io::Result<Option<usize>> {
        let mut available = 0;
        if unsafe {
            PeekNamedPipe(
                reader.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        } == 0
        {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(109) {
                return Ok(Some(0));
            }
            return Err(error);
        }
        if available == 0 {
            return Ok(None);
        }
        let count = bytes.len().min(available as usize);
        reader.read(&mut bytes[..count]).map(Some)
    }
    pub(super) fn wait(_: &File) -> io::Result<()> {
        thread::sleep(Duration::from_millis(5));
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    use super::*;
    pub(super) fn pipe() -> io::Result<(File, File)> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Merged console pipes require Unix or Windows",
        ))
    }
    pub(super) fn read(_: &mut File, _: &mut [u8]) -> io::Result<Option<usize>> {
        unreachable!()
    }
    pub(super) fn wait(_: &File) -> io::Result<()> {
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_survives_every_split_and_preserves_binary_lookalikes() {
        let payload = b"\xff\0__MATHLAB_JSON__{}\x1eMATHLAB:other:4:B:8\x1fhello";
        let mut input = b"\x1eMATHLAB:test:4:B:1:7\x1f".to_vec();
        input.extend_from_slice(payload);
        input.extend_from_slice(b"\x1eMATHLAB:test:4:E:1:7\x1f");
        for split in 0..=input.len() {
            let mut parser = FrameParser::new("test", 4).unwrap();
            let mut parts = parser.feed(&input[..split]).unwrap();
            parts.extend(parser.feed(&input[split..]).unwrap());
            parts.extend(parser.finish().unwrap());
            assert_eq!(parts.first(), Some(&FrameItem::Begin(1, 7)));
            assert_eq!(parts.last(), Some(&FrameItem::End(1, 7)));
            let actual = parts
                .into_iter()
                .filter_map(|item| {
                    if let FrameItem::Bytes(bytes) = item {
                        Some(bytes)
                    } else {
                        None
                    }
                })
                .flatten()
                .collect::<Vec<_>>();
            assert_eq!(actual, payload);
        }
    }

    #[test]
    fn current_malformed_oversized_and_incomplete_fences_fail_boundedly() {
        for suffix in [
            b"X:1:1\x1f".as_slice(),
            b"B:1:no\x1f",
            b"E:1:18446744073709551616\x1f",
            b"B:0:1\x1f",
            b"R:1:2:3\x1f",
            b"I:1:2\x1f",
            b"F:1:2:0\x1f",
            b"I:1:2:3:4\x1f",
        ] {
            let mut parser = FrameParser::new("nonce", 2).unwrap();
            let mut input = b"\x1eMATHLAB:nonce:2:".to_vec();
            input.extend_from_slice(suffix);
            assert!(parser.feed(&input).is_err());
        }
        let mut parser = FrameParser::new("nonce", 2).unwrap();
        let mut input = b"\x1eMATHLAB:nonce:2:".to_vec();
        input.extend_from_slice(&[b'x'; MAX_FENCE]);
        assert!(parser.feed(&input).is_err());
        let mut parser = FrameParser::new("nonce", 2).unwrap();
        parser.feed(b"\x1eMATHLAB:nonce:2:B:").unwrap();
        assert!(parser.finish().is_err());
        let mut parser = FrameParser::new("nonce", 2).unwrap();
        let failure = parser
            .feed(b"PARTIAL\x1eMATHLAB:nonce:2:X:1:9\x1f")
            .unwrap_err();
        assert_eq!(failure.items, vec![FrameItem::Bytes(b"PARTIAL".to_vec())]);
    }

    #[test]
    fn input_control_frames_survive_every_split_without_escaping_into_output() {
        let input = b"\x1eMATHLAB:test:4:B:1:7\x1fVALUE>\x1eMATHLAB:test:4:I:1:7:9\x1f42\n\x1eMATHLAB:test:4:F:1:7:9\x1f\x1eMATHLAB:test:4:E:1:7\x1f";
        for split in 0..=input.len() {
            let mut parser = FrameParser::new("test", 4).unwrap();
            let mut parts = parser.feed(&input[..split]).unwrap();
            parts.extend(parser.feed(&input[split..]).unwrap());
            parts.extend(parser.finish().unwrap());
            assert_eq!(
                parts
                    .iter()
                    .filter(|part| matches!(part, FrameItem::InputBegin(1, 7, 9)))
                    .count(),
                1
            );
            assert_eq!(
                parts
                    .iter()
                    .filter(|part| matches!(part, FrameItem::InputEnd(1, 7, 9)))
                    .count(),
                1
            );
            let bytes: Vec<u8> = parts
                .into_iter()
                .filter_map(|part| match part {
                    FrameItem::Bytes(bytes) => Some(bytes),
                    _ => None,
                })
                .flatten()
                .collect();
            assert_eq!(bytes, b"VALUE>42\n");
        }
    }

    #[test]
    fn slow_subscriber_is_bounded_and_reports_loss_without_stalling_others() {
        let hub = ConsoleHub::new();
        let slow = hub.subscribe();
        let fast = hub.subscribe();
        let mut expected = 0;
        for _ in 0..400 {
            let sequence = hub.publish(
                1,
                Some(3),
                ConsoleEventKind::Output {
                    bytes: vec![b'a'; READ_BYTES],
                },
            );
            let event = fast.try_recv().unwrap();
            assert_eq!(event.sequence, sequence);
            expected += READ_BYTES;
        }
        assert_eq!(expected, 400 * READ_BYTES);
        let loss = slow.try_recv().unwrap();
        assert!(
            matches!(loss.kind, ConsoleEventKind::OutputLimit { dropped_bytes } if dropped_bytes > 0)
        );
        let mut sequence = loss.sequence;
        let mut retained = 0;
        while let Ok(event) = slow.try_recv() {
            assert!(event.sequence > sequence);
            sequence = event.sequence;
            retained += event_bytes(&event);
        }
        assert!(retained <= SUBSCRIBER_BYTES);
        assert_eq!(slow.latest_sequence(), (1, sequence));
    }

    #[test]
    fn evicted_input_metadata_has_storage_weight_without_claiming_console_byte_loss() {
        let hub = ConsoleHub::new();
        let stream = hub.subscribe();
        for input_id in 1..=20 {
            let key = InputKey {
                generation: 1,
                invocation: 1,
                request_id: 2,
                input_id,
            };
            hub.publish(
                1,
                Some(2),
                ConsoleEventKind::InputRequested {
                    request: InputRequest {
                        key,
                        prompt: "p".repeat(64 * 1024),
                        mode: crate::InputMode::String,
                        console_sequence: input_id,
                    },
                },
            );
        }
        assert!(matches!(
            stream.try_recv().unwrap().kind,
            ConsoleEventKind::OutputLimit { dropped_bytes: 0 }
        ));
        let mut retained = 0;
        while let Ok(event) = stream.try_recv() {
            retained += event_bytes(&event);
        }
        assert!(retained <= SUBSCRIBER_BYTES);
    }

    #[test]
    fn input_state_is_ready_before_event_delivery_and_does_not_fill_owner_queue() {
        let directory = tempfile::tempdir().unwrap();
        let input = InputHub::new(directory.path()).unwrap();
        input.begin_generation(3, 1).unwrap();
        input.set_enabled(true).unwrap();
        let handle = input.handle();
        let hub = ConsoleHub::new();
        let stream = hub.subscribe();
        let (sender, receiver) = mpsc::sync_channel(4);
        let mut state = ProtocolState {
            input: Some(input.clone()),
            ..ProtocolState::default()
        };
        consume(FrameItem::Begin(1, 7), 3, &hub, &sender, &mut state).unwrap();
        receiver.try_recv().unwrap();
        consume(
            FrameItem::Bytes(b"VALUE>".to_vec()),
            3,
            &hub,
            &sender,
            &mut state,
        )
        .unwrap();
        let key = InputKey {
            generation: 3,
            invocation: 1,
            request_id: 7,
            input_id: 9,
        };
        fs::write(input.directory().join("input-request-3-1-7-9.json"),br#"{"generation":3,"invocation":1,"request_id":7,"input_id":9,"prompt":"VALUE>","mode":"expression"}"#).unwrap();
        consume(FrameItem::InputBegin(1, 7, 9), 3, &hub, &sender, &mut state).unwrap();
        let pending = handle.pending().unwrap().unwrap();
        assert_eq!(pending.request.console_sequence, 3);
        assert_eq!(pending.request.key, key);
        handle.reply(&key, "42").unwrap();
        assert_eq!(
            handle.pending().unwrap().unwrap().phase,
            crate::InputPhase::Submitted
        );
        assert!(receiver.try_recv().is_err());
        consume(
            FrameItem::Bytes(b"42\n".to_vec()),
            3,
            &hub,
            &sender,
            &mut state,
        )
        .unwrap();
        fs::write(
            input.directory().join("input-finish-3-1-7-9.json"),
            br#"{"generation":3,"invocation":1,"request_id":7,"input_id":9,"outcome":"line"}"#,
        )
        .unwrap();
        consume(FrameItem::InputEnd(1, 7, 9), 3, &hub, &sender, &mut state).unwrap();
        assert!(handle.pending().unwrap().is_none());
        assert!(receiver.try_recv().is_err());
        consume(FrameItem::End(1, 7), 3, &hub, &sender, &mut state).unwrap();
        let TransportEvent::Complete { capture, .. } = receiver.try_recv().unwrap() else {
            panic!("missing result")
        };
        assert_eq!(capture.output, "VALUE>42\n");
        let mut kinds = Vec::new();
        while let Ok(event) = stream.try_recv() {
            kinds.push(event.kind);
        }
        assert!(
            matches!(&kinds[2],ConsoleEventKind::InputRequested{request} if request.console_sequence==3)
        );
        assert!(matches!(&kinds[4],ConsoleEventKind::InputFinished{finished} if finished.key==key));
        assert!(matches!(&kinds[5], ConsoleEventKind::End));
    }

    #[test]
    fn capture_limit_does_not_prevent_end_or_raw_binary_delivery() {
        let hub = ConsoleHub::new();
        let stream = hub.subscribe();
        let (sender, receiver) = mpsc::sync_channel(4);
        let mut state = ProtocolState::default();
        consume(FrameItem::Begin(1, 9), 1, &hub, &sender, &mut state).unwrap();
        for _ in 0..(CAPTURE_BYTES / READ_BYTES + 1) {
            consume(
                FrameItem::Bytes(vec![0xff; READ_BYTES]),
                1,
                &hub,
                &sender,
                &mut state,
            )
            .unwrap();
        }
        consume(FrameItem::End(1, 9), 1, &hub, &sender, &mut state).unwrap();
        assert!(matches!(
            receiver.try_recv().unwrap(),
            TransportEvent::Begin {
                invocation: 1,
                request_id: 9
            }
        ));
        let TransportEvent::Complete { capture, .. } = receiver.try_recv().unwrap() else {
            panic!("completion missing");
        };
        assert!(capture.truncated && capture.decoding_loss);
        assert_eq!(capture.dropped_bytes, READ_BYTES as u64);
        let mut end = false;
        while let Ok(event) = stream.try_recv() {
            if event.kind == ConsoleEventKind::End {
                end = true;
                assert_eq!(event.sequence, capture.end_sequence);
            }
        }
        assert!(end);
    }

    #[test]
    fn stopping_transport_does_not_wait_for_an_inherited_writer_to_close() {
        let pipe = MergedPipe::new().unwrap();
        let _writer = pipe.stdout().unwrap();
        let hub = ConsoleHub::new();
        let directory = tempfile::tempdir().unwrap();
        let input = InputHub::new(directory.path()).unwrap();
        let mut transport =
            ConsoleTransport::start(pipe.into_reader(), "stop".into(), 1, hub, input).unwrap();
        let started = Instant::now();
        transport.stop();
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn recovery_before_end_preserves_partial_output_and_ignores_late_old_fences() {
        let hub = ConsoleHub::new();
        let stream = hub.subscribe();
        let (sender, receiver) = mpsc::sync_channel(8);
        let mut state = ProtocolState::default();
        for item in [
            FrameItem::Begin(1, 7),
            FrameItem::Bytes(b"partial".to_vec()),
            FrameItem::Recovery(1, 7),
            FrameItem::End(1, 7),
            FrameItem::Begin(1, 7),
            FrameItem::Begin(2, 0),
            FrameItem::Recovery(1, 7),
            FrameItem::Bytes(b"ready".to_vec()),
            FrameItem::End(2, 0),
        ] {
            consume(item, 3, &hub, &sender, &mut state).unwrap();
        }
        assert!(matches!(
            receiver.try_recv().unwrap(),
            TransportEvent::Begin {
                invocation: 1,
                request_id: 7
            }
        ));
        let TransportEvent::Recovery {
            invocation: 1,
            request_id: 7,
            capture: Some(capture),
        } = receiver.try_recv().unwrap()
        else {
            panic!("interrupted capture missing")
        };
        assert_eq!(capture.output, "partial");
        assert!(matches!(
            receiver.try_recv().unwrap(),
            TransportEvent::Begin {
                invocation: 2,
                request_id: 0
            }
        ));
        let TransportEvent::Complete {
            invocation: 2,
            request_id: 0,
            capture,
        } = receiver.try_recv().unwrap()
        else {
            panic!("ready completion missing")
        };
        assert_eq!(capture.output, "ready");
        assert!(receiver.try_recv().is_err());
        let mut ends = 0;
        while let Ok(event) = stream.try_recv() {
            if matches!(event.kind, ConsoleEventKind::End) {
                ends += 1;
            }
        }
        assert_eq!(ends, 2);
    }

    #[test]
    fn recovery_after_end_does_not_replace_normal_result_or_publish_another_end() {
        let hub = ConsoleHub::new();
        let stream = hub.subscribe();
        let (sender, receiver) = mpsc::sync_channel(8);
        let mut state = ProtocolState::default();
        for item in [
            FrameItem::Begin(1, 7),
            FrameItem::End(1, 7),
            FrameItem::End(1, 7),
            FrameItem::Recovery(1, 7),
            FrameItem::Recovery(1, 7),
        ] {
            consume(item, 1, &hub, &sender, &mut state).unwrap();
        }
        assert!(matches!(
            receiver.try_recv().unwrap(),
            TransportEvent::Begin { .. }
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            TransportEvent::Complete { .. }
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            TransportEvent::Recovery { capture: None, .. }
        ));
        assert!(receiver.try_recv().is_err());
        let mut ends = 0;
        while let Ok(event) = stream.try_recv() {
            if matches!(event.kind, ConsoleEventKind::End) {
                ends += 1;
            }
        }
        assert_eq!(ends, 1);
    }

    #[test]
    fn recovery_can_precede_begin_but_normal_orphan_end_is_an_error() {
        let hub = ConsoleHub::new();
        let (sender, receiver) = mpsc::sync_channel(8);
        let mut state = ProtocolState::default();
        consume(FrameItem::Recovery(1, 9), 1, &hub, &sender, &mut state).unwrap();
        assert!(matches!(
            receiver.try_recv().unwrap(),
            TransportEvent::Recovery {
                capture: Some(ConsoleCapture {
                    end_sequence: 2,
                    ..
                }),
                ..
            }
        ));
        consume(FrameItem::Begin(1, 9), 1, &hub, &sender, &mut state).unwrap();
        assert!(receiver.try_recv().is_err());
        consume(FrameItem::Begin(2, 0), 1, &hub, &sender, &mut state).unwrap();
        consume(FrameItem::End(2, 0), 1, &hub, &sender, &mut state).unwrap();
        assert!(consume(FrameItem::End(2, 10), 1, &hub, &sender, &mut state).is_err());
        assert!(consume(FrameItem::Begin(4, 0), 1, &hub, &sender, &mut state).is_err());
    }

    #[test]
    fn readiness_and_idle_recovery_cannot_complete_the_next_request() {
        let hub = ConsoleHub::new();
        let (sender, receiver) = mpsc::sync_channel(4);
        let mut state = ProtocolState::default();
        for (begin, end) in [
            (FrameItem::Begin(1, 0), FrameItem::End(1, 0)),
            (FrameItem::Begin(1, 3), FrameItem::End(1, 3)),
        ] {
            consume(begin, 1, &hub, &sender, &mut state).unwrap();
            consume(end, 1, &hub, &sender, &mut state).unwrap();
            assert!(matches!(
                receiver.try_recv().unwrap(),
                TransportEvent::Begin { .. }
            ));
            assert!(matches!(
                receiver.try_recv().unwrap(),
                TransportEvent::Complete { .. }
            ));
        }
        consume(FrameItem::Recovery(1, 3), 1, &hub, &sender, &mut state).unwrap();
        assert!(matches!(
            receiver.try_recv().unwrap(),
            TransportEvent::Recovery { capture: None, .. }
        ));
        for item in [
            FrameItem::Begin(2, 0),
            FrameItem::End(1, 0),
            FrameItem::End(2, 0),
            FrameItem::Begin(2, 4),
            FrameItem::End(1, 3),
            FrameItem::End(2, 4),
        ] {
            consume(item, 1, &hub, &sender, &mut state).unwrap();
        }
        assert!(matches!(
            receiver.try_recv().unwrap(),
            TransportEvent::Begin {
                invocation: 2,
                request_id: 0
            }
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            TransportEvent::Complete {
                invocation: 2,
                request_id: 0,
                ..
            }
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            TransportEvent::Begin {
                invocation: 2,
                request_id: 4
            }
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            TransportEvent::Complete {
                invocation: 2,
                request_id: 4,
                ..
            }
        ));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn closed_subscription_retains_completion_cursor_and_diagnostics_are_bounded() {
        let hub = ConsoleHub::new();
        hub.publish(
            1,
            None,
            ConsoleEventKind::Output {
                bytes: b"startup".to_vec(),
            },
        );
        let stream = hub.subscribe();
        assert_eq!(stream.initial_sequence(), (1, 1));
        for _ in 0..8 {
            hub.publish(
                1,
                Some(3),
                ConsoleEventKind::Output {
                    bytes: vec![b'x'; READ_BYTES],
                },
            );
        }
        assert_eq!(hub.diagnostics(1).len(), 2000);
        assert!(hub.diagnostics(2).is_empty());
        let end = hub.publish(1, Some(3), ConsoleEventKind::End);
        drop(hub);
        assert_eq!(stream.latest_sequence(), (1, end));
        let mut last = 0;
        while let Ok(event) = stream.try_recv() {
            last = event.sequence;
        }
        assert_eq!(last, end);
        assert_eq!(stream.try_recv(), Err(mpsc::TryRecvError::Disconnected));
    }
}
