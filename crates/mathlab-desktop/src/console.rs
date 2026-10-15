//! Bounded command-window history and decoding of the session's byte stream.

use super::{ConsoleEntry, ConsoleKind};
use std::collections::VecDeque;

pub(crate) const MAX_SCROLLBACK_BYTES: usize = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const OUTPUT_ENTRY_BYTES: usize = 64 * 1024;

pub(crate) struct ConsoleBuffer {
    entries: VecDeque<ConsoleEntry>,
    bytes: usize,
    pub(crate) trimmed: bool,
}

impl ConsoleBuffer {
    pub(crate) fn new(entries: impl IntoIterator<Item = ConsoleEntry>) -> Self {
        let mut buffer = Self {
            entries: VecDeque::new(),
            bytes: 0,
            trimmed: false,
        };
        for entry in entries {
            buffer.push(entry);
        }
        buffer
    }

    pub(crate) fn push(&mut self, mut entry: ConsoleEntry) {
        // A single response or pasted command must not temporarily enlarge retained history.
        if entry.text.len() > MAX_SCROLLBACK_BYTES {
            let offset = char_boundary_after(&entry.text, entry.text.len() - MAX_SCROLLBACK_BYTES);
            entry.text = entry.text[offset..].to_owned();
            entry.sources.clear();
            entry.sources_active = false;
            self.trimmed = true;
        }
        self.bytes += entry.text.len();
        self.entries.push_back(entry);
        self.enforce_limit();
    }

    pub(crate) fn output(&mut self, mut text: &str) {
        while !text.is_empty() {
            let room = self
                .entries
                .back()
                .filter(|entry| {
                    matches!(entry.kind, ConsoleKind::Output)
                        && entry.text.len() < OUTPUT_ENTRY_BYTES
                })
                .map_or(0, |entry| OUTPUT_ENTRY_BYTES - entry.text.len());
            if room > 0 {
                let length = char_boundary_before(text, room.min(text.len()));
                if length > 0 {
                    self.entries
                        .back_mut()
                        .unwrap()
                        .text
                        .push_str(&text[..length]);
                    self.bytes += length;
                    text = &text[length..];
                    self.enforce_limit();
                    continue;
                }
            }
            let mut length = char_boundary_before(text, OUTPUT_ENTRY_BYTES.min(text.len()));
            if length < text.len() {
                if let Some(newline) = text[..length].rfind('\n') {
                    length = newline + 1;
                }
            }
            self.push(ConsoleEntry::new(&text[..length], ConsoleKind::Output));
            text = &text[length..];
        }
    }

    fn enforce_limit(&mut self) {
        while self.entries.len() > MAX_ENTRIES || self.bytes > MAX_SCROLLBACK_BYTES {
            let remove = self.entries.len() > MAX_ENTRIES
                || self.entries.front().is_some_and(|entry| {
                    entry.text.len() <= self.bytes.saturating_sub(MAX_SCROLLBACK_BYTES)
                });
            if remove {
                if let Some(entry) = self.entries.pop_front() {
                    self.bytes -= entry.text.len();
                    self.trimmed = true;
                }
            } else if let Some(entry) = self.entries.front_mut() {
                let offset = char_boundary_after(&entry.text, self.bytes - MAX_SCROLLBACK_BYTES);
                self.bytes -= offset;
                entry.text = entry.text[offset..].to_owned();
                entry.sources.clear();
                entry.sources_active = false;
                self.trimmed = true;
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.trimmed = false;
    }

    pub(crate) fn iter(&self) -> std::collections::vec_deque::Iter<'_, ConsoleEntry> {
        self.entries.iter()
    }

    pub(crate) fn last(&self) -> Option<&ConsoleEntry> {
        self.entries.back()
    }
}

impl<'a> IntoIterator for &'a ConsoleBuffer {
    type Item = &'a ConsoleEntry;
    type IntoIter = std::collections::vec_deque::Iter<'a, ConsoleEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

impl<'a> IntoIterator for &'a mut ConsoleBuffer {
    type Item = &'a mut ConsoleEntry;
    type IntoIter = std::collections::vec_deque::IterMut<'a, ConsoleEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter_mut()
    }
}

fn char_boundary_before(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn char_boundary_after(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

/// An incomplete UTF-8 codepoint can straddle pipe chunks. Invalid bytes are rendered
/// with replacements, while a valid incomplete suffix waits for the next chunk.
#[derive(Default)]
pub(crate) struct Utf8Decoder {
    pending: Vec<u8>,
}

impl Utf8Decoder {
    pub(crate) fn decode(&mut self, bytes: &[u8]) -> String {
        let mut joined = std::mem::take(&mut self.pending);
        joined.extend_from_slice(bytes);
        let mut remaining = joined.as_slice();
        let mut text = String::new();
        loop {
            match std::str::from_utf8(remaining) {
                Ok(valid) => {
                    text.push_str(valid);
                    break;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    // The UTF-8 validator guarantees this prefix is valid.
                    text.push_str(std::str::from_utf8(&remaining[..valid]).unwrap());
                    remaining = &remaining[valid..];
                    if let Some(invalid) = error.error_len() {
                        text.push('\u{fffd}');
                        remaining = &remaining[invalid..];
                    } else {
                        self.pending.extend_from_slice(remaining);
                        break;
                    }
                }
            }
        }
        text
    }

    pub(crate) fn finish(&mut self) -> String {
        String::from_utf8_lossy(&std::mem::take(&mut self.pending)).into_owned()
    }
}

/// Keep each GUI update bounded even when a background process continuously writes.
/// A read may exceed the byte budget by one engine chunk; engine chunks are bounded.
pub(crate) struct DrainBudget {
    events: usize,
    bytes: usize,
}

impl Default for DrainBudget {
    fn default() -> Self {
        Self {
            events: 256,
            bytes: 256 * 1024,
        }
    }
}

impl DrainBudget {
    pub(crate) fn exhausted(&self) -> bool {
        self.events == 0 || self.bytes == 0
    }

    pub(crate) fn record(&mut self, event: &mathlab_engine::ConsoleEvent) {
        self.events = self.events.saturating_sub(1);
        let bytes = match &event.kind {
            mathlab_engine::ConsoleEventKind::Output { bytes } => bytes.len(),
            mathlab_engine::ConsoleEventKind::InputRequested { request } => request.prompt.len(),
            mathlab_engine::ConsoleEventKind::Disconnected { message } => message.len(),
            _ => 0,
        };
        self.bytes = self.bytes.saturating_sub(bytes);
    }
}

#[derive(Default)]
pub(crate) struct ConsoleStream {
    generation: u64,
    last_sequence: u64,
    decoder: Utf8Decoder,
}

impl ConsoleStream {
    pub(crate) fn reset(&mut self, generation: u64, console: &mut ConsoleBuffer) {
        self.finish(console);
        self.generation = generation;
        self.last_sequence = 0;
    }

    pub(crate) fn connect(
        &mut self,
        (generation, sequence): (u64, u64),
        console: &mut ConsoleBuffer,
    ) {
        self.reset(generation, console);
        // Events published before registration are intentionally absent from this
        // subscription. They are already past its completion cursor.
        self.last_sequence = sequence;
    }

    pub(crate) fn progress(&self) -> (u64, u64) {
        (self.generation, self.last_sequence)
    }

    pub(crate) fn reached(&self, (generation, sequence): (u64, u64)) -> bool {
        self.generation > generation
            || (self.generation == generation && self.last_sequence >= sequence)
    }

    pub(crate) fn accept(
        &mut self,
        event: mathlab_engine::ConsoleEvent,
        console: &mut ConsoleBuffer,
    ) -> Option<String> {
        use mathlab_engine::ConsoleEventKind;
        if event.generation < self.generation
            || (event.generation == self.generation && event.sequence <= self.last_sequence)
        {
            return None;
        }
        if event.generation > self.generation {
            self.reset(event.generation, console);
        }
        self.last_sequence = event.sequence;
        match event.kind {
            // Each request is a display decoding boundary, matching the aggregate
            // ExecutionResult. Incomplete tails are replaced rather than carried
            // into a subsequent command whose bytes have unrelated provenance.
            ConsoleEventKind::Begin | ConsoleEventKind::End => self.finish(console),
            ConsoleEventKind::Output { bytes } => console.output(&self.decoder.decode(&bytes)),
            ConsoleEventKind::OutputLimit { dropped_bytes } => {
                // Even zero lost bytes can hide a Begin/End decoding boundary.
                // Finalize the old tail before accepting bytes after any gap.
                self.finish(console);
                if dropped_bytes > 0 {
                    console.push(ConsoleEntry::new(
                        format!("Console output limit: {dropped_bytes} bytes were discarded before display."),
                        ConsoleKind::Notice,
                    ));
                }
            }
            ConsoleEventKind::InputRequested { .. } | ConsoleEventKind::InputFinished { .. } => {}
            ConsoleEventKind::Disconnected { message } => {
                self.finish(console);
                return Some(message);
            }
        }
        None
    }

    pub(crate) fn finish(&mut self, console: &mut ConsoleBuffer) {
        console.output(&self.decoder.finish());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(buffer: &ConsoleBuffer) -> String {
        buffer.iter().map(|entry| entry.text.as_str()).collect()
    }

    #[test]
    fn split_unicode_and_invalid_bytes_decode_without_corrupting_following_output() {
        let mut decoder = Utf8Decoder::default();
        assert_eq!(decoder.decode(b"first \xf0\x9f"), "first ");
        assert_eq!(decoder.decode(b"\x98\x80 \xff\xe2"), "😀 �");
        assert_eq!(decoder.decode(b"\x82\xac"), "€");
        assert!(decoder.finish().is_empty());
        assert_eq!(decoder.decode(b"trailing \xe2\x82"), "trailing ");
        assert_eq!(decoder.finish(), "�");
    }

    #[test]
    fn chunks_append_without_losing_spaces_newlines_or_creating_a_new_entry_per_byte() {
        let mut buffer = ConsoleBuffer::new([]);
        for _ in 0..1000 {
            buffer.output("a");
        }
        buffer.output(" \n\n");
        assert_eq!(buffer.entries.len(), 1);
        assert_eq!(text(&buffer), format!("{} \n\n", "a".repeat(1000)));
        buffer.push(ConsoleEntry::new("next", ConsoleKind::Command));
        buffer.output("βeta");
        assert_eq!(buffer.entries.len(), 3);
        assert!(text(&buffer).ends_with("nextβeta"));
    }

    #[test]
    fn scrollback_drops_oldest_text_on_utf8_boundaries_and_bounds_entry_count() {
        let mut buffer = ConsoleBuffer::new([]);
        buffer.output(&"α".repeat(MAX_SCROLLBACK_BYTES / 2));
        buffer.output("newest 😀\n");
        assert!(buffer.trimmed);
        assert!(buffer.bytes <= MAX_SCROLLBACK_BYTES);
        assert!(text(&buffer).ends_with("newest 😀\n"));
        for _ in 0..MAX_ENTRIES + 1 {
            buffer.push(ConsoleEntry::new("", ConsoleKind::Notice));
        }
        assert!(buffer.entries.len() <= MAX_ENTRIES);
        assert!(buffer.bytes <= MAX_SCROLLBACK_BYTES);
        buffer.clear();
        assert!(!buffer.trimmed);
        assert_eq!(buffer.bytes, 0);
        assert!(buffer.entries.is_empty());
    }

    #[test]
    fn oversized_single_command_keeps_a_bounded_recent_tail() {
        let mut buffer = ConsoleBuffer::new([]);
        buffer.push(ConsoleEntry::new(
            format!("{}end", "界".repeat(MAX_SCROLLBACK_BYTES / 3 + 4)),
            ConsoleKind::Command,
        ));
        assert!(buffer.trimmed);
        assert!(buffer.bytes <= MAX_SCROLLBACK_BYTES);
        assert!(text(&buffer).ends_with("end"));
    }

    fn event(
        generation: u64,
        sequence: u64,
        kind: mathlab_engine::ConsoleEventKind,
    ) -> mathlab_engine::ConsoleEvent {
        mathlab_engine::ConsoleEvent {
            generation,
            request_id: Some(1),
            sequence,
            kind,
        }
    }

    #[test]
    fn stream_preserves_partial_output_after_failure_and_rejects_old_generation_events() {
        use mathlab_engine::ConsoleEventKind::{Begin, Disconnected, Output};
        let mut buffer = ConsoleBuffer::new([]);
        let mut stream = ConsoleStream::default();
        stream.accept(event(1, 1, Begin), &mut buffer);
        stream.accept(
            event(
                1,
                2,
                Output {
                    bytes: b"partial \xe2".to_vec(),
                },
            ),
            &mut buffer,
        );
        assert_eq!(text(&buffer), "partial ");
        let error = stream.accept(
            event(
                1,
                3,
                Disconnected {
                    message: "process died".into(),
                },
            ),
            &mut buffer,
        );
        assert_eq!(error.as_deref(), Some("process died"));
        assert_eq!(text(&buffer), "partial �");
        stream.accept(event(2, 4, Begin), &mut buffer);
        stream.accept(
            event(
                1,
                8,
                Output {
                    bytes: b"obsolete".to_vec(),
                },
            ),
            &mut buffer,
        );
        stream.accept(
            event(
                2,
                5,
                Output {
                    bytes: b"new session".to_vec(),
                },
            ),
            &mut buffer,
        );
        assert_eq!(text(&buffer), "partial �new session");
        assert!(stream.reached((1, 100)));
        assert!(stream.reached((2, 5)));
        assert!(!stream.reached((2, 6)));
    }

    #[test]
    fn loss_notice_advances_completion_even_if_the_end_event_was_evicted() {
        use mathlab_engine::ConsoleEventKind::{Begin, Output, OutputLimit};
        let mut buffer = ConsoleBuffer::new([]);
        let mut stream = ConsoleStream::default();
        stream.accept(event(1, 1, Begin), &mut buffer);
        stream.accept(
            event(
                1,
                2,
                Output {
                    bytes: b"\xf0\x9f".to_vec(),
                },
            ),
            &mut buffer,
        );
        stream.accept(
            event(
                1,
                20,
                OutputLimit {
                    dropped_bytes: 1000,
                },
            ),
            &mut buffer,
        );
        assert!(stream.reached((1, 15)));
        stream.accept(
            event(
                1,
                21,
                Output {
                    bytes: b"tail".to_vec(),
                },
            ),
            &mut buffer,
        );
        let shown = text(&buffer);
        assert!(shown.starts_with("�"));
        assert!(shown.contains("1000 bytes"));
        assert!(shown.ends_with("tail"));
        stream.accept(
            event(
                1,
                20,
                OutputLimit {
                    dropped_bytes: 1000,
                },
            ),
            &mut buffer,
        );
        assert_eq!(shown, text(&buffer));
    }

    #[test]
    fn frame_budget_bounds_tiny_and_large_console_chunks() {
        use mathlab_engine::ConsoleEventKind::Output;
        let mut budget = DrainBudget::default();
        for index in 0..256 {
            assert!(!budget.exhausted());
            budget.record(&event(1, index + 1, Output { bytes: vec![b'x'] }));
        }
        assert!(budget.exhausted());
        let mut budget = DrainBudget::default();
        budget.record(&event(
            1,
            1,
            Output {
                bytes: vec![b'x'; 256 * 1024],
            },
        ));
        assert!(budget.exhausted());
    }

    #[test]
    fn incomplete_codepoints_do_not_join_across_request_boundaries() {
        use mathlab_engine::ConsoleEventKind::{Begin, End, Output};
        let mut buffer = ConsoleBuffer::new([]);
        let mut stream = ConsoleStream::default();
        for (sequence, kind) in [
            (1, Begin),
            (
                2,
                Output {
                    bytes: vec![0xf0, 0x9f],
                },
            ),
            (3, End),
            (4, Begin),
            (
                5,
                Output {
                    bytes: vec![0x98, 0x80],
                },
            ),
            (6, End),
        ] {
            stream.accept(event(1, sequence, kind), &mut buffer);
        }
        assert_eq!(text(&buffer), "���");
    }

    #[test]
    fn subscription_registration_cursor_does_not_wait_for_unsubscribed_startup_fences() {
        use mathlab_engine::ConsoleEventKind::Output;
        let mut buffer = ConsoleBuffer::new([]);
        let mut stream = ConsoleStream::default();
        stream.connect((2, 50), &mut buffer);
        assert!(stream.reached((2, 50)));
        assert!(!stream.reached((2, 51)));
        stream.accept(
            event(
                2,
                51,
                Output {
                    bytes: b"first subscribed bytes".to_vec(),
                },
            ),
            &mut buffer,
        );
        assert_eq!(text(&buffer), "first subscribed bytes");
        assert!(stream.reached((2, 51)));
    }

    #[test]
    fn zero_byte_loss_cannot_join_utf8_across_evicted_request_boundaries() {
        use mathlab_engine::ConsoleEventKind::{Output, OutputLimit};
        let mut buffer = ConsoleBuffer::new([]);
        let mut stream = ConsoleStream::default();
        stream.accept(
            event(
                1,
                1,
                Output {
                    bytes: vec![0xf0, 0x9f],
                },
            ),
            &mut buffer,
        );
        assert!(text(&buffer).is_empty());
        // The intervening End and Begin were lost, although no text bytes were.
        stream.accept(event(1, 3, OutputLimit { dropped_bytes: 0 }), &mut buffer);
        assert_eq!(text(&buffer), "�");
        assert!(stream.reached((1, 3)));
        stream.accept(
            event(
                1,
                4,
                Output {
                    bytes: vec![0x98, 0x80],
                },
            ),
            &mut buffer,
        );
        assert_eq!(text(&buffer), "���");
        assert!(
            buffer
                .iter()
                .all(|entry| matches!(entry.kind, ConsoleKind::Output)),
            "a control-only gap must not claim that text bytes were discarded"
        );
    }
}
