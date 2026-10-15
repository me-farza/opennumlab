// SPDX-License-Identifier: GPL-3.0-or-later
//! A private mailbox for authentic interpreter input requests. Answers never use
//! the stdin channel reserved for worker recovery, and never require &mut Engine.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
const MAX_PROMPT_BYTES: usize = 64 * 1024;
const MAX_METADATA_BYTES: usize = 128 * 1024;
const MAX_REPLY_FILE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputKey {
    pub generation: u64,
    pub invocation: u64,
    pub request_id: u64,
    pub input_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    Expression,
    String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputRequest {
    pub key: InputKey,
    pub prompt: String,
    pub mode: InputMode,
    /// The prompt's preceding console bytes have been consumed once a subscriber
    /// reaches this position. Recovering a pending snapshot does not replay them.
    pub console_sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputPhase {
    Waiting,
    Submitted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingInput {
    pub request: InputRequest,
    pub phase: InputPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputOutcome {
    Line,
    Eof,
    Interrupted,
    Disconnected,
    Error,
}

/// Line/Eof mean consumption before expression evaluation, not successful
/// completion of the full input() call. Nested input therefore gets a new key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputFinished {
    pub key: InputKey,
    pub outcome: InputOutcome,
}

#[derive(Debug, Default)]
struct State {
    generation: u64,
    invocation: u64,
    enabled: bool,
    closed: bool,
    pending: Option<PendingInput>,
}

#[derive(Debug)]
struct Client {
    directory: PathBuf,
    state: Mutex<State>,
}

impl Client {
    fn state(&self) -> Result<MutexGuard<'_, State>> {
        self.state
            .lock()
            .map_err(|_| Error::Protocol("Input state lock is poisoned".into()))
    }

    fn discard_pending(&self, state: &mut State) -> Option<PendingInput> {
        let pending = state.pending.take()?;
        // Stop can win after Rust publishes a reply but before the native reader
        // opens/removes it. Revoke its exact mailbox under the same state lock
        // used by reply publication, so restart cannot accumulate old answers.
        let _ = fs::remove_file(mailbox_path(
            &self.directory,
            "input-reply",
            pending.request.key,
        ));
        Some(pending)
    }
}

/// Independent of the numerical request owner. A cloned handle remains valid
/// across restart, but an old InputKey can never answer a new request.
#[derive(Clone, Debug)]
pub struct InputHandle {
    client: Arc<Client>,
}

#[derive(Serialize)]
struct Reply<'a> {
    #[serde(flatten)]
    key: InputKey,
    eof: bool,
    text: &'a str,
    echo: bool,
}

impl InputHandle {
    pub fn pending(&self) -> Result<Option<PendingInput>> {
        let state = self.client.state()?;
        if state.closed {
            return Err(Error::Protocol(
                "The input session is no longer available".into(),
            ));
        }
        Ok(state.pending.clone())
    }

    /// Submit one UTF-8 line. The native reader owns accepted-text echo.
    pub fn reply(&self, key: &InputKey, text: &str) -> Result<()> {
        self.reply_with_echo(key, text, true)
    }

    /// Terminals that already echoed their cooked input can suppress native echo.
    pub fn reply_with_echo(&self, key: &InputKey, text: &str, echo: bool) -> Result<()> {
        if text.len() > MAX_INPUT_BYTES {
            return Err(Error::LimitExceeded(
                "Input replies are limited to 1 MiB of UTF-8 text".into(),
            ));
        }
        if text.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n')) {
            return Err(Error::Protocol(
                "Input replies must be one line without NUL, CR or LF bytes".into(),
            ));
        }
        self.submit(key, false, text, echo)
    }

    /// Deliver EOF to the original reader. This is distinct from an empty line.
    pub fn eof(&self, key: &InputKey) -> Result<()> {
        self.submit(key, true, "", false)
    }

    fn submit(&self, key: &InputKey, eof: bool, text: &str, echo: bool) -> Result<()> {
        let mut state = self.client.state()?;
        if state.closed || !state.enabled {
            return Err(Error::Protocol("The input adapter is not available".into()));
        }
        if state.generation != key.generation || state.invocation != key.invocation {
            return Err(Error::Protocol(
                "The input request belongs to an earlier worker generation or invocation".into(),
            ));
        }
        let pending = state.pending.as_mut().ok_or_else(|| {
            Error::Protocol("The interpreter is no longer waiting for this input".into())
        })?;
        if pending.request.key != *key {
            return Err(Error::Protocol(
                "The pending input request changed before the answer was submitted".into(),
            ));
        }
        if pending.phase != InputPhase::Waiting {
            return Err(Error::Protocol(
                "An answer has already been submitted for this input request".into(),
            ));
        }

        let bytes = serde_json::to_vec(&Reply {
            key: *key,
            eof,
            text,
            echo,
        })?;
        if bytes.len() > MAX_REPLY_FILE_BYTES {
            return Err(Error::LimitExceeded(
                "Encoded input reply exceeds the 8 MiB mailbox limit".into(),
            ));
        }
        // Serialize publication and lifecycle changes under this one lock. A
        // cancellation/restart cannot validate an old key then publish its answer
        // into a later request. No console-hub lock is acquired from this module.
        let mut temporary = tempfile::NamedTempFile::new_in(&self.client.directory)?;
        temporary.write_all(&bytes)?;
        temporary.flush()?;
        temporary
            .into_temp_path()
            .persist_noclobber(mailbox_path(&self.client.directory, "input-reply", *key))
            .map_err(|failure| Error::Io(failure.error))?;
        pending.phase = InputPhase::Submitted;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct RequestedMetadata {
    #[serde(flatten)]
    pub(crate) key: InputKey,
    pub(crate) prompt: String,
    pub(crate) mode: InputMode,
}

#[derive(Deserialize)]
struct FinishedMetadata {
    #[serde(flatten)]
    key: InputKey,
    outcome: InputOutcome,
}

#[derive(Clone, Debug)]
pub(crate) struct InputHub {
    client: Arc<Client>,
}

impl InputHub {
    pub(crate) fn new(session_directory: &Path) -> Result<Self> {
        let directory = session_directory.join("input");
        fs::create_dir_all(&directory)?;
        Ok(Self {
            client: Arc::new(Client {
                directory,
                state: Mutex::new(State::default()),
            }),
        })
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.client.directory
    }

    pub(crate) fn handle(&self) -> InputHandle {
        InputHandle {
            client: self.client.clone(),
        }
    }

    pub(crate) fn begin_generation(&self, generation: u64, invocation: u64) -> Result<()> {
        let mut state = self.client.state()?;
        self.client.discard_pending(&mut state);
        if state.generation != generation {
            state.enabled = false;
        }
        state.generation = generation;
        state.invocation = invocation;
        state.closed = false;
        Ok(())
    }

    pub(crate) fn set_enabled(&self, enabled: bool) -> Result<()> {
        let mut state = self.client.state()?;
        state.enabled = enabled;
        if !enabled {
            self.client.discard_pending(&mut state);
        }
        Ok(())
    }

    pub(crate) fn close(&self) {
        if let Ok(mut state) = self.client.state.lock() {
            self.client.discard_pending(&mut state);
            state.enabled = false;
            state.closed = true;
        }
    }

    pub(crate) fn read_requested(&self, key: InputKey) -> Result<RequestedMetadata> {
        let metadata: RequestedMetadata = self.read_metadata("input-request", key)?;
        if metadata.key != key {
            return Err(Error::Protocol(
                "Input request metadata does not match its fence identity".into(),
            ));
        }
        if metadata.prompt.len() > MAX_PROMPT_BYTES {
            return Err(Error::LimitExceeded(
                "Input prompts are limited to 64 KiB of UTF-8 text".into(),
            ));
        }
        fs::remove_file(mailbox_path(self.directory(), "input-request", key))?;
        Ok(metadata)
    }

    pub(crate) fn apply_requested(
        &self,
        metadata: RequestedMetadata,
        console_sequence: u64,
    ) -> Result<InputRequest> {
        let mut state = self.client.state()?;
        if state.closed || !state.enabled {
            return Err(Error::Protocol(
                "Input request received without an enabled adapter".into(),
            ));
        }
        if metadata.key.generation != state.generation
            || metadata.key.invocation != state.invocation
        {
            return Err(Error::Protocol(
                "Input request metadata belongs to a stale worker".into(),
            ));
        }
        if state.pending.is_some() {
            return Err(Error::Protocol(
                "A second input request arrived before the previous line was consumed".into(),
            ));
        }
        let request = InputRequest {
            key: metadata.key,
            prompt: metadata.prompt,
            mode: metadata.mode,
            console_sequence,
        };
        state.pending = Some(PendingInput {
            request: request.clone(),
            phase: InputPhase::Waiting,
        });
        Ok(request)
    }

    pub(crate) fn read_finished(&self, key: InputKey) -> Result<InputFinished> {
        let metadata: FinishedMetadata = self.read_metadata("input-finish", key)?;
        if metadata.key != key {
            return Err(Error::Protocol(
                "Input finish metadata does not match its fence identity".into(),
            ));
        }
        if !matches!(
            metadata.outcome,
            InputOutcome::Line
                | InputOutcome::Eof
                | InputOutcome::Interrupted
                | InputOutcome::Error
        ) {
            return Err(Error::Protocol(
                "Native input finish must report consumption or a read-abort outcome".into(),
            ));
        }
        fs::remove_file(mailbox_path(self.directory(), "input-finish", key))?;
        Ok(InputFinished {
            key,
            outcome: metadata.outcome,
        })
    }

    pub(crate) fn apply_finished(&self, finished: InputFinished) -> Result<InputFinished> {
        let mut state = self.client.state()?;
        let pending = state
            .pending
            .as_ref()
            .ok_or_else(|| Error::Protocol("Input finished without a pending request".into()))?;
        if pending.request.key != finished.key {
            return Err(Error::Protocol(
                "Input finish belongs to another pending request".into(),
            ));
        }
        self.client.discard_pending(&mut state);
        Ok(finished)
    }

    pub(crate) fn invalidate(&self, outcome: InputOutcome) -> Option<InputFinished> {
        let mut state = self.client.state.lock().ok()?;
        self.client
            .discard_pending(&mut state)
            .map(|pending| InputFinished {
                key: pending.request.key,
                outcome,
            })
    }

    fn read_metadata<T: serde::de::DeserializeOwned>(
        &self,
        prefix: &str,
        key: InputKey,
    ) -> Result<T> {
        let file = File::open(mailbox_path(self.directory(), prefix, key))?;
        if file.metadata()?.len() > MAX_METADATA_BYTES as u64 {
            return Err(Error::LimitExceeded(
                "Input metadata exceeds the 128 KiB limit".into(),
            ));
        }
        let mut bytes = Vec::new();
        file.take((MAX_METADATA_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_METADATA_BYTES {
            return Err(Error::LimitExceeded(
                "Input metadata exceeds the 128 KiB limit".into(),
            ));
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
}

fn mailbox_path(directory: &Path, prefix: &str, key: InputKey) -> PathBuf {
    directory.join(format!(
        "{prefix}-{}-{}-{}-{}.json",
        key.generation, key.invocation, key.request_id, key.input_id
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn hub() -> (InputHub, tempfile::TempDir, InputKey) {
        let directory = tempfile::tempdir().unwrap();
        let hub = InputHub::new(directory.path()).unwrap();
        hub.begin_generation(2, 3).unwrap();
        hub.set_enabled(true).unwrap();
        let key = InputKey {
            generation: 2,
            invocation: 3,
            request_id: 7,
            input_id: 1,
        };
        (hub, directory, key)
    }

    fn request(hub: &InputHub, key: InputKey) -> InputRequest {
        let wire = json!({"generation":key.generation,"invocation":key.invocation,"request_id":key.request_id,"input_id":key.input_id,"prompt":"Value λ? ","mode":"string"});
        fs::write(
            mailbox_path(hub.directory(), "input-request", key),
            serde_json::to_vec(&wire).unwrap(),
        )
        .unwrap();
        hub.apply_requested(hub.read_requested(key).unwrap(), 42)
            .unwrap()
    }

    #[test]
    fn exact_unicode_reply_is_atomic_private_and_submitted_until_consumed() {
        let (hub, _directory, key) = hub();
        let asked = request(&hub, key);
        let handle = hub.handle();
        assert_eq!(
            handle.pending().unwrap().unwrap(),
            PendingInput {
                request: asked,
                phase: InputPhase::Waiting
            }
        );
        handle.reply(&key, "  λ 'quoted' \\ tail\t").unwrap();
        let wire: Value = serde_json::from_slice(
            &fs::read(mailbox_path(hub.directory(), "input-reply", key)).unwrap(),
        )
        .unwrap();
        assert_eq!(wire["text"], "  λ 'quoted' \\ tail\t");
        assert_eq!(wire["echo"], true);
        assert_eq!(wire["eof"], false);
        assert_eq!(wire["input_id"], 1);
        assert_eq!(
            handle.pending().unwrap().unwrap().phase,
            InputPhase::Submitted
        );
        assert!(handle.reply(&key, "duplicate").is_err());
        let finish = InputFinished {
            key,
            outcome: InputOutcome::Line,
        };
        assert_eq!(hub.apply_finished(finish.clone()).unwrap(), finish);
        assert!(handle.pending().unwrap().is_none());
        assert!(handle.reply(&key, "late").is_err());
    }

    #[test]
    fn eof_is_distinct_from_empty_line_and_echo_is_selectable() {
        let (hub, _directory, key) = hub();
        request(&hub, key);
        hub.handle().reply_with_echo(&key, "", false).unwrap();
        let path = mailbox_path(hub.directory(), "input-reply", key);
        let line: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(line["eof"], false);
        assert_eq!(line["echo"], false);
        fs::remove_file(path).unwrap();
        hub.apply_finished(InputFinished {
            key,
            outcome: InputOutcome::Line,
        })
        .unwrap();
        let next = InputKey { input_id: 2, ..key };
        request(&hub, next);
        hub.handle().eof(&next).unwrap();
        let eof: Value = serde_json::from_slice(
            &fs::read(mailbox_path(hub.directory(), "input-reply", next)).unwrap(),
        )
        .unwrap();
        assert_eq!(eof["eof"], true);
        assert_eq!(eof["text"], "");
    }

    #[test]
    fn stale_cancelled_restarted_and_closed_handles_never_publish_answers() {
        let (hub, directory, key) = hub();
        let handle = hub.handle();
        request(&hub, key);
        assert_eq!(hub.invalidate(InputOutcome::Interrupted).unwrap().key, key);
        assert!(handle.reply(&key, "stale").is_err());
        hub.begin_generation(2, 4).unwrap();
        request(
            &hub,
            InputKey {
                invocation: 4,
                ..key
            },
        );
        assert!(handle.reply(&key, "stale invocation").is_err());
        hub.begin_generation(3, 1).unwrap();
        hub.set_enabled(true).unwrap();
        request(
            &hub,
            InputKey {
                generation: 3,
                invocation: 1,
                ..key
            },
        );
        assert!(handle.reply(&key, "stale generation").is_err());
        hub.close();
        assert!(handle.pending().is_err());
        drop(directory);
        assert!(handle.reply(&key, "closed").is_err());
    }

    #[test]
    fn oversized_and_multiline_answers_leave_request_waiting() {
        let (hub, _directory, key) = hub();
        request(&hub, key);
        for invalid in ["line\nnext", "line\rnext", "a\0b"] {
            assert!(hub.handle().reply(&key, invalid).is_err());
        }
        assert!(
            hub.handle()
                .reply(&key, &"λ".repeat(MAX_INPUT_BYTES / 2 + 1))
                .is_err()
        );
        assert_eq!(
            hub.handle().pending().unwrap().unwrap().phase,
            InputPhase::Waiting
        );
        assert!(!mailbox_path(hub.directory(), "input-reply", key).exists());
    }

    #[test]
    fn bounded_metadata_rejects_foreign_identity_and_large_files() {
        let (hub, _directory, key) = hub();
        let path = mailbox_path(hub.directory(), "input-request", key);
        fs::write(&path,b"{\"generation\":2,\"invocation\":3,\"request_id\":7,\"input_id\":99,\"prompt\":\"x\",\"mode\":\"string\"}").unwrap();
        assert!(hub.read_requested(key).is_err());
        File::create(&path)
            .unwrap()
            .set_len(MAX_METADATA_BYTES as u64 + 1)
            .unwrap();
        assert!(matches!(
            hub.read_requested(key),
            Err(Error::LimitExceeded(_))
        ));
        assert!(hub.handle().pending().unwrap().is_none());
    }

    #[test]
    fn simultaneous_answers_accept_exactly_one_publication() {
        let (hub, _directory, key) = hub();
        request(&hub, key);
        let first = hub.handle();
        let second = hub.handle();
        let a = std::thread::spawn(move || first.reply(&key, "first"));
        let b = std::thread::spawn(move || second.reply(&key, "second"));
        let accepted = [a.join().unwrap().is_ok(), b.join().unwrap().is_ok()];
        assert_eq!(accepted.into_iter().filter(|value| *value).count(), 1);
        let wire: Value = serde_json::from_slice(
            &fs::read(mailbox_path(hub.directory(), "input-reply", key)).unwrap(),
        )
        .unwrap();
        assert!(wire["text"] == "first" || wire["text"] == "second");
    }

    #[test]
    fn terminal_input_paths_remove_only_the_revoked_reply_mailbox() {
        for terminal in 0..6 {
            let (hub, _directory, key) = hub();
            request(&hub, key);
            hub.handle()
                .reply(&key, &"x".repeat(MAX_INPUT_BYTES))
                .unwrap();
            let reply = mailbox_path(hub.directory(), "input-reply", key);
            let other = mailbox_path(
                hub.directory(),
                "input-reply",
                InputKey {
                    input_id: 99,
                    ..key
                },
            );
            fs::write(&other, b"unrelated").unwrap();
            assert!(reply.exists());
            match terminal {
                0 | 1 => {
                    hub.apply_finished(InputFinished {
                        key,
                        outcome: if terminal == 0 {
                            InputOutcome::Line
                        } else {
                            InputOutcome::Error
                        },
                    })
                    .unwrap();
                }
                2 => {
                    hub.invalidate(InputOutcome::Interrupted).unwrap();
                }
                3 => {
                    hub.begin_generation(3, 1).unwrap();
                }
                4 => {
                    hub.set_enabled(false).unwrap();
                }
                _ => hub.close(),
            }
            assert!(
                !reply.exists(),
                "terminal path {terminal} retained its reply"
            );
            assert_eq!(fs::read(other).unwrap(), b"unrelated");
            assert!(hub.handle().reply(&key, "obsolete").is_err());
        }
    }
}
