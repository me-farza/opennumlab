//! Command-window input state. Replies bypass the serialized execution worker.

use mathlab_engine::{InputHandle, InputKey, InputMode, InputPhase, InputRequest, PendingInput};

trait Backend {
    fn pending(&self) -> mathlab_engine::Result<Option<PendingInput>>;
    fn reply(&self, key: &InputKey, text: &str) -> mathlab_engine::Result<()>;
    fn eof(&self, key: &InputKey) -> mathlab_engine::Result<()>;
}

impl Backend for InputHandle {
    fn pending(&self) -> mathlab_engine::Result<Option<PendingInput>> {
        InputHandle::pending(self)
    }
    fn reply(&self, key: &InputKey, text: &str) -> mathlab_engine::Result<()> {
        InputHandle::reply(self, key, text)
    }
    fn eof(&self, key: &InputKey) -> mathlab_engine::Result<()> {
        InputHandle::eof(self, key)
    }
}

struct Waiting {
    request: InputRequest,
    reply: String,
    visible: bool,
    submitted: bool,
    stopping: bool,
    error: Option<String>,
}

#[derive(Default)]
pub(crate) struct InputController {
    backend: Option<Box<dyn Backend>>,
    generation: u64,
    waiting: Option<Waiting>,
    error: Option<String>,
}

impl InputController {
    pub(crate) fn install(&mut self, handle: Option<InputHandle>, generation: u64) {
        self.backend = handle.map(|handle| Box::new(handle) as Box<dyn Backend>);
        self.generation = generation;
        self.clear();
    }

    pub(crate) fn disconnect(&mut self) {
        self.backend = None;
        self.clear();
    }

    pub(crate) fn clear(&mut self) {
        self.waiting = None;
        self.error = None;
    }

    /// The reliable snapshot is authoritative even if lossy console notifications
    /// were evicted. Return true only when a new prompt becomes ready for focus.
    pub(crate) fn poll(&mut self, (generation, sequence): (u64, u64)) -> bool {
        let Some(backend) = &self.backend else {
            return false;
        };
        let snapshot = match backend.pending() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.error = Some(error.to_string());
                return false;
            }
        };
        self.error = None;
        let Some(snapshot) = snapshot else {
            self.clear();
            return false;
        };
        if snapshot.request.key.generation != self.generation || generation > self.generation {
            self.clear();
            return false;
        }
        let visible =
            generation == self.generation && sequence >= snapshot.request.console_sequence;
        let submitted = matches!(snapshot.phase, InputPhase::Submitted);
        let same = self
            .waiting
            .as_ref()
            .is_some_and(|waiting| waiting.request.key == snapshot.request.key);
        if same {
            let waiting = self.waiting.as_mut().unwrap();
            let focus = !waiting.visible
                && visible
                && !waiting.submitted
                && !submitted
                && !waiting.stopping;
            waiting.visible = visible;
            waiting.submitted |= submitted;
            focus
        } else {
            self.waiting = Some(Waiting {
                request: snapshot.request,
                reply: String::new(),
                visible,
                submitted,
                stopping: false,
                error: None,
            });
            visible && !submitted
        }
    }

    pub(crate) fn active(&self) -> bool {
        self.waiting.is_some()
    }

    pub(crate) fn can_submit(&self) -> bool {
        self.backend.is_some()
            && self.error.is_none()
            && self
                .waiting
                .as_ref()
                .is_some_and(|waiting| waiting.visible && !waiting.submitted && !waiting.stopping)
    }

    pub(crate) fn reply_mut(&mut self) -> Option<&mut String> {
        self.waiting.as_mut().map(|waiting| &mut waiting.reply)
    }

    pub(crate) fn hint(&self) -> &'static str {
        match self.waiting.as_ref().map(|waiting| &waiting.request.mode) {
            Some(InputMode::String) => "Enter text, including an empty response…",
            Some(InputMode::Expression) => "Enter an expression…",
            None => "Enter a response…",
        }
    }

    pub(crate) fn status(&self) -> Option<&'static str> {
        self.waiting.as_ref().map(|waiting| {
            if waiting.stopping {
                "Interrupting…"
            } else if waiting.submitted {
                "Continuing…"
            } else {
                "Waiting for input"
            }
        })
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref().or_else(|| {
            self.waiting
                .as_ref()
                .and_then(|waiting| waiting.error.as_deref())
        })
    }

    pub(crate) fn submit(&mut self, eof: bool) -> bool {
        if !self.can_submit() {
            return false;
        }
        let waiting = self.waiting.as_mut().unwrap();
        let backend = self.backend.as_ref().unwrap();
        let result = if eof {
            backend.eof(&waiting.request.key)
        } else {
            // Native code evaluates expression input and echoes the accepted line.
            // This frontend neither transforms nor evaluates the user's response.
            backend.reply(&waiting.request.key, &waiting.reply)
        };
        match result {
            Ok(()) => {
                waiting.submitted = true;
                waiting.error = None;
                true
            }
            Err(error) => {
                waiting.error = Some(error.to_string());
                false
            }
        }
    }

    pub(crate) fn reject_multiline_paste(&mut self) {
        if let Some(waiting) = &mut self.waiting {
            waiting.error =
                Some("Input replies must contain one line. Paste one response at a time.".into());
        }
    }

    pub(crate) fn stopping(&mut self) {
        if let Some(waiting) = &mut self.waiting {
            waiting.stopping = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeState {
        pending: Option<PendingInput>,
        replies: Vec<(InputKey, Option<String>)>,
        fail: bool,
    }

    #[derive(Clone)]
    struct Fake(Arc<Mutex<FakeState>>);

    impl Backend for Fake {
        fn pending(&self) -> mathlab_engine::Result<Option<PendingInput>> {
            Ok(self.0.lock().unwrap().pending.clone())
        }
        fn reply(&self, key: &InputKey, text: &str) -> mathlab_engine::Result<()> {
            let mut state = self.0.lock().unwrap();
            if state.fail {
                return Err(mathlab_engine::Error::Protocol(
                    "reply was not accepted".into(),
                ));
            }
            state.replies.push((*key, Some(text.into())));
            Ok(())
        }
        fn eof(&self, key: &InputKey) -> mathlab_engine::Result<()> {
            self.0.lock().unwrap().replies.push((*key, None));
            Ok(())
        }
    }

    fn pending(input_id: u64, generation: u64) -> PendingInput {
        PendingInput {
            request: InputRequest {
                key: InputKey {
                    generation,
                    invocation: 4,
                    request_id: 7,
                    input_id,
                },
                prompt: "Value? ".into(),
                mode: InputMode::String,
                console_sequence: 10,
            },
            phase: InputPhase::Waiting,
        }
    }

    fn fixture() -> (InputController, Fake) {
        let fake = Fake(Arc::new(Mutex::new(FakeState {
            pending: Some(pending(1, 1)),
            ..FakeState::default()
        })));
        let controller = InputController {
            backend: Some(Box::new(fake.clone())),
            generation: 1,
            ..InputController::default()
        };
        (controller, fake)
    }

    #[test]
    fn reliable_snapshot_waits_for_prompt_sequence_and_focuses_only_once() {
        let (mut input, _) = fixture();
        assert!(!input.poll((1, 9)));
        assert!(input.active());
        assert!(!input.can_submit());
        assert!(!input.submit(false));
        assert!(input.poll((1, 10)));
        assert!(input.can_submit());
        assert!(!input.poll((1, 11)));
    }

    #[test]
    fn exact_empty_and_unicode_replies_are_distinct_from_eof_and_send_once() {
        let (mut input, fake) = fixture();
        input.poll((1, 10));
        assert!(input.submit(false));
        assert!(!input.submit(false));
        input.poll((1, 10)); // Even a delayed Waiting snapshot cannot reopen submission.
        assert!(!input.submit(false));
        fake.0.lock().unwrap().pending = Some(pending(2, 1));
        input.poll((1, 11));
        *input.reply_mut().unwrap() = "  λ + 'quoted'  ".into();
        assert!(input.submit(false));
        fake.0.lock().unwrap().pending = Some(pending(3, 1));
        input.poll((1, 12));
        *input.reply_mut().unwrap() = "not sent with EOF".into();
        assert!(input.submit(true));
        let state = fake.0.lock().unwrap();
        assert_eq!(
            state
                .replies
                .iter()
                .map(|(_, text)| text.as_deref())
                .collect::<Vec<_>>(),
            vec![Some(""), Some("  λ + 'quoted'  "), None]
        );
    }

    #[test]
    fn rejected_reply_keeps_text_for_retry_and_stopping_disables_submission() {
        let (mut input, fake) = fixture();
        input.poll((1, 10));
        *input.reply_mut().unwrap() = "  retry me  ".into();
        fake.0.lock().unwrap().fail = true;
        assert!(!input.submit(false));
        assert_eq!(input.error(), Some("reply was not accepted"));
        assert_eq!(input.reply_mut().unwrap(), "  retry me  ");
        assert!(input.can_submit());
        fake.0.lock().unwrap().fail = false;
        assert!(input.submit(false));
        assert!(input.error().is_none());
        fake.0.lock().unwrap().pending = Some(pending(2, 1));
        input.poll((1, 12));
        input.stopping();
        assert!(!input.submit(false));
        assert!(!input.submit(true));
    }

    #[test]
    fn old_generation_cannot_activate_after_restart_and_closed_waiter_does_not_resurrect() {
        let (mut input, fake) = fixture();
        input.generation = 2;
        assert!(!input.poll((2, 10)));
        assert!(!input.active());
        fake.0.lock().unwrap().pending = Some(pending(2, 2));
        assert!(input.poll((2, 10)));
        fake.0.lock().unwrap().pending = None;
        input.poll((2, 11));
        assert!(!input.active());
        fake.0.lock().unwrap().pending = Some(pending(3, 2));
        input.disconnect();
        input.poll((2, 12));
        assert!(!input.active());
    }

    #[test]
    fn submitted_snapshot_is_not_editable_and_prompt_like_output_does_not_create_a_waiter() {
        let (mut input, fake) = fixture();
        fake.0.lock().unwrap().pending.as_mut().unwrap().phase = InputPhase::Submitted;
        assert!(!input.poll((1, 10)));
        assert!(input.active());
        assert!(!input.can_submit());
        assert_eq!(input.status(), Some("Continuing…"));
        fake.0.lock().unwrap().pending = None;
        let mut console = crate::console::ConsoleBuffer::new([]);
        let mut stream = crate::console::ConsoleStream::default();
        stream.accept(
            mathlab_engine::ConsoleEvent {
                generation: 1,
                request_id: Some(7),
                sequence: 11,
                kind: mathlab_engine::ConsoleEventKind::Output {
                    bytes: b"Value? input() waiting for text".to_vec(),
                },
            },
            &mut console,
        );
        input.poll(stream.progress());
        assert!(!input.active());
    }

    #[test]
    fn app_input_reply_bypasses_execution_queue_preserves_draft_and_history_and_has_no_local_echo()
    {
        let (mut input, fake) = fixture();
        input.poll((1, 10));
        *input.reply_mut().unwrap() = "  exact reply  ".into();
        let mut test = crate::lifecycle_tests::harness();
        test.app.input = input;
        test.app.busy = true;
        test.app.command = "draft = 42".into();
        test.app.command_draft = "older draft".into();
        test.app.history = vec!["previous command".into()];
        let console_before = test
            .app
            .console
            .iter()
            .map(|entry| entry.text.as_str())
            .collect::<String>();
        test.app.submit_command();
        assert!(test.app.busy);
        assert_eq!(test.app.command, "draft = 42");
        assert_eq!(test.app.command_draft, "older draft");
        assert_eq!(test.app.history, vec!["previous command"]);
        assert!(test.requests.try_recv().is_err());
        assert_eq!(
            fake.0.lock().unwrap().replies[0].1.as_deref(),
            Some("  exact reply  ")
        );
        assert_eq!(
            test.app
                .console
                .iter()
                .map(|entry| entry.text.as_str())
                .collect::<String>(),
            console_before
        );
    }

    #[test]
    fn input_blocks_normal_source_actions_without_changing_execution_source_identity() {
        let (mut input, _) = fixture();
        input.poll((1, 10));
        let mut test = crate::lifecycle_tests::harness();
        test.app.input = input;
        test.app.busy = false; // Input itself is also an independent dispatch guard.
        test.app.executed_document = Some(123);
        test.app
            .execute("new_source = 1".into(), "history replay".into());
        test.app.run_document();
        test.app.run_or_continue();
        assert!(test.requests.try_recv().is_err());
        assert_eq!(test.app.executed_document, Some(123));
    }

    #[test]
    fn stale_finished_notification_cannot_clear_a_new_reliable_waiter_in_the_same_process() {
        let (mut input, fake) = fixture();
        input.poll((1, 10));
        let old_key = fake.0.lock().unwrap().pending.as_ref().unwrap().request.key;
        let mut next = pending(1, 1);
        next.request.key.invocation += 1;
        fake.0.lock().unwrap().pending = Some(next.clone());
        assert!(input.poll((1, 11)));
        *input.reply_mut().unwrap() = "new waiter draft".into();
        let mut console = crate::console::ConsoleBuffer::new([]);
        let mut stream = crate::console::ConsoleStream::default();
        stream.accept(
            mathlab_engine::ConsoleEvent {
                generation: 1,
                request_id: Some(7),
                sequence: 12,
                kind: mathlab_engine::ConsoleEventKind::InputFinished {
                    finished: mathlab_engine::InputFinished {
                        key: old_key,
                        outcome: mathlab_engine::InputOutcome::Interrupted,
                    },
                },
            },
            &mut console,
        );
        assert!(!input.poll(stream.progress()));
        assert!(input.active());
        assert_eq!(input.reply_mut().unwrap(), "new waiter draft");
        assert!(input.submit(false));
        assert_eq!(fake.0.lock().unwrap().replies[0].0, next.request.key);
    }

    #[test]
    fn command_window_keyboard_entry_sends_the_input_reply_without_trimming_or_dispatching_code() {
        use eframe::egui;
        let (mut input, fake) = fixture();
        input.poll((1, 10));
        let mut test = crate::lifecycle_tests::harness();
        test.app.input = input;
        test.app.busy = true;
        test.app.focus_command = true;
        test.app.command = "preserved normal draft".into();
        let ctx = test.ctx.clone();
        let frame = |events| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        let _ = ctx.run(frame(Vec::new()), |ctx| test.app.command_window(ctx));
        let _ = ctx.run(frame(vec![egui::Event::Text("  λ + 2  ".into())]), |ctx| {
            test.app.command_window(ctx)
        });
        let _ = ctx.run(
            frame(vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]),
            |ctx| test.app.command_window(ctx),
        );
        assert_eq!(
            fake.0.lock().unwrap().replies[0].1.as_deref(),
            Some("  λ + 2  ")
        );
        assert_eq!(test.app.command, "preserved normal draft");
        assert!(test.requests.try_recv().is_err());
        assert!(test.app.busy);
    }

    #[test]
    fn multiline_paste_is_rejected_instead_of_silently_rewritten_to_spaces() {
        use eframe::egui;
        let (mut input, fake) = fixture();
        input.poll((1, 10));
        *input.reply_mut().unwrap() = "kept".into();
        let mut test = crate::lifecycle_tests::harness();
        test.app.input = input;
        test.app.busy = true;
        test.app.focus_command = true;
        let ctx = test.ctx.clone();
        let frame = |events| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        let _ = ctx.run(frame(Vec::new()), |ctx| test.app.command_window(ctx));
        let _ = ctx.run(
            frame(vec![egui::Event::Paste("two\nlines".into())]),
            |ctx| test.app.command_window(ctx),
        );
        assert_eq!(test.app.input.reply_mut().unwrap(), "kept");
        assert!(test.app.input.error().unwrap().contains("one line"));
        assert!(fake.0.lock().unwrap().replies.is_empty());
    }

    #[test]
    fn waiting_input_screenshot_and_focus_wait_for_the_prompt_barrier() {
        let (input, _) = fixture();
        let mut test = crate::lifecycle_tests::harness();
        test.app.input = input;
        test.app.busy = true;
        test.app.screenshot_ready = false;
        test.app.focus_command = false;
        test.app
            .console_stream
            .connect((1, 9), &mut test.app.console);
        test.app.poll_input();
        assert!(!test.app.screenshot_ready);
        assert!(!test.app.focus_command);
        test.app.console_stream.accept(
            mathlab_engine::ConsoleEvent {
                generation: 1,
                request_id: Some(7),
                sequence: 10,
                kind: mathlab_engine::ConsoleEventKind::InputRequested {
                    request: pending(1, 1).request,
                },
            },
            &mut test.app.console,
        );
        test.app.poll_input();
        assert!(test.app.screenshot_ready);
        assert!(test.app.focus_command);
        assert!(test.app.busy);
    }
}
