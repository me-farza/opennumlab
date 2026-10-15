//! Typed client for the Octave debugger event adapter.
//!
//! The owning engine must keep the session directory and interpreter alive.
//! Commands run on Octave's interpreter thread, in the actual paused scope.
use anyhow::{bail, ensure, Context, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

const MAX_REQUEST: usize = 1024 * 1024;
const MAX_RESPONSE: u64 = 8 * 1024 * 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// An immutable prepared source generation. Function spans use original lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMapping {
    pub generated_path: PathBuf,
    pub original_path: PathBuf,
    pub scope_file: PathBuf,
    pub local_function: Option<String>,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    /// Exact full original text for this immutable prepared generation.
    pub snapshot_path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct SourceSnapshot {
    pub original_path: PathBuf,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct DebugState {
    pub revision: u64,
    pub paused: bool,
    pub file: PathBuf,
    pub line: usize,
    pub function_name: String,
    /// Interpreter scope identity before original-source mapping.
    #[serde(default)]
    pub runtime_file: PathBuf,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct DebugFrame {
    pub file: PathBuf,
    pub name: String,
    pub line: usize,
    #[serde(default)]
    pub column: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct DebugVariable {
    pub name: String,
    #[serde(rename = "class")]
    pub class_name: String,
    #[serde(rename = "size")]
    pub dimensions: Vec<usize>,
    pub bytes: u64,
    #[serde(rename = "global", default)]
    pub is_global: bool,
    #[serde(default)]
    pub complex: bool,
    #[serde(default)]
    pub sparse: bool,
    #[serde(default)]
    pub persistent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluationOutput {
    pub output: String,
    /// The native adapter also forwarded evaluation text to its console stream.
    pub streamed: bool,
    /// Only the returned copy was clipped; live output is independently bounded
    /// by each console consumer.
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum ResumeAction {
    Continue,
    Step,
    StepIn,
    StepOut,
    Quit,
}

impl ResumeAction {
    fn command(self) -> &'static str {
        match self {
            Self::Continue => "continue",
            Self::Step => "step",
            Self::StepIn => "step_in",
            Self::StepOut => "step_out",
            Self::Quit => "quit",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Debugger {
    inner: Arc<Client>,
}

#[derive(Debug)]
struct Client {
    directory: PathBuf,
    sequence: AtomicU64,
    request_lock: Mutex<()>,
    sources: Mutex<Vec<SourceMapping>>,
    breakpoints: Mutex<BreakpointRegistry>,
}

#[derive(Debug, Default)]
struct BreakpointRegistry {
    next_id: u64,
    groups: Vec<BreakpointGroup>,
    installed: Vec<InstalledBreakpoint>,
}

#[derive(Debug)]
struct BreakpointGroup {
    id: u64,
    original_path: PathBuf,
    active_lines: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InstalledBreakpoint {
    group_id: u64,
    actual_line: usize,
    mapping: Option<SourceMapping>,
}

#[derive(Serialize)]
struct Request<'a> {
    id: u64,
    command: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope_file: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_function: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_file: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot_file: Option<&'a Path>,
}

#[derive(Deserialize)]
struct Response {
    id: u64,
    ok: bool,
    #[serde(default)]
    result: Value,
    #[serde(default)]
    error: String,
    #[serde(default)]
    evaluation_streamed: bool,
    #[serde(default)]
    output_truncated: bool,
}

impl Debugger {
    /// Connect to an adapter installed by the owning engine.
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            inner: Arc::new(Client {
                directory: directory.into(),
                sequence: AtomicU64::new(1),
                request_lock: Mutex::new(()),
                sources: Mutex::new(Vec::new()),
                breakpoints: Mutex::new(BreakpointRegistry::default()),
            }),
        }
    }

    pub fn directory(&self) -> &Path {
        &self.inner.directory
    }

    /// Register a generation before execution. Clones share these mappings.
    /// Re-registering a cached generation makes it the current breakpoint target.
    pub fn register_sources(&self, mut mappings: Vec<SourceMapping>) -> Result<()> {
        for mapping in &mut mappings {
            ensure!(
                mapping.original_path.is_absolute(),
                "Original source path must be absolute"
            );
            mapping.generated_path =
                fs::canonicalize(&mapping.generated_path).context("Find prepared source")?;
            mapping.scope_file =
                fs::canonicalize(&mapping.scope_file).context("Find prepared script scope")?;
            if let Some(path) = &mapping.snapshot_path {
                mapping.snapshot_path =
                    Some(fs::canonicalize(path).context("Find prepared source snapshot")?);
            }
            mapping.original_path = fs::canonicalize(&mapping.original_path)
                .unwrap_or_else(|_| mapping.original_path.clone());
            if mapping.local_function.is_some() {
                ensure!(
                    matches!((mapping.start_line,mapping.end_line), (Some(start),Some(end)) if start>0 && end>=start),
                    "A local function requires a valid original line span"
                );
            } else {
                ensure!(
                    mapping.start_line.is_none() && mapping.end_line.is_none(),
                    "Script mapping must not have a local-function span"
                );
            }
        }
        let mut sources = self
            .inner
            .sources
            .lock()
            .map_err(|_| anyhow::anyhow!("Source mapping lock is poisoned"))?;
        sources.retain(|old| {
            !mappings
                .iter()
                .any(|new| new.generated_path == old.generated_path)
        });
        sources.extend(mappings);
        Ok(())
    }

    fn map_location(&self, file: &mut PathBuf, name: &mut String) -> Result<()> {
        let sources = self
            .inner
            .sources
            .lock()
            .map_err(|_| anyhow::anyhow!("Source mapping lock is poisoned"))?;
        let normalized = fs::canonicalize(&*file).unwrap_or_else(|_| file.clone());
        if let Some(mapping) = sources.iter().find(|mapping| {
            mapping.generated_path == normalized && mapping.generated_path != mapping.original_path
        }) {
            let stem = mapping
                .original_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy();
            *name = match &mapping.local_function {
                Some(_) if name.contains('>') => format!("{stem}>{name}"),
                Some(local) if !name.is_empty() && name != local => {
                    format!("{stem}>{local}>{name}")
                }
                Some(local) => format!("{stem}>{local}"),
                None => stem.into_owned(),
            };
            *file = mapping.original_path.clone();
        }
        Ok(())
    }

    fn source_for_breakpoint(&self, file: &Path, line: usize) -> Result<Option<SourceMapping>> {
        let normalized = fs::canonicalize(file).unwrap_or_else(|_| file.to_owned());
        let sources = self
            .inner
            .sources
            .lock()
            .map_err(|_| anyhow::anyhow!("Source mapping lock is poisoned"))?;
        let Some(latest) = sources
            .iter()
            .rev()
            .find(|mapping| mapping.original_path == normalized)
        else {
            return Ok(None);
        };
        if latest.generated_path == latest.original_path {
            return Ok(None);
        }
        let active = sources.iter().filter(|mapping| {
            mapping.original_path == normalized && mapping.scope_file == latest.scope_file
        });
        let selected = active.clone().filter(|mapping| matches!((mapping.start_line,mapping.end_line),(Some(start),Some(end)) if line>=start && line<=end)).min_by_key(|mapping| mapping.end_line.unwrap() - mapping.start_line.unwrap());
        Ok(selected
            .or_else(|| {
                active
                    .into_iter()
                    .find(|mapping| mapping.local_function.is_none())
            })
            .cloned())
    }

    /// Read the latest state without waiting for the engine's execution request.
    pub fn state(&self) -> Result<Option<DebugState>> {
        let path = self.directory().join("debug-state.json");
        match read_bounded(&path) {
            Ok(bytes) => {
                let mut state: DebugState =
                    serde_json::from_slice(&bytes).context("Invalid debugger state")?;
                state.runtime_file = state.file.clone();
                self.map_location(&mut state.file, &mut state.function_name)?;
                Ok(Some(state))
            }
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    /// Read a prepared generation's immutable text without evaluating user code.
    pub fn source_snapshot(&self, runtime_file: &Path) -> Result<Option<SourceSnapshot>> {
        let normalized = fs::canonicalize(runtime_file).unwrap_or_else(|_| runtime_file.to_owned());
        let mapping = self
            .inner
            .sources
            .lock()
            .map_err(|_| anyhow::anyhow!("Source mapping lock is poisoned"))?
            .iter()
            .find(|mapping| mapping.generated_path == normalized)
            .cloned();
        let Some(mapping) = mapping else {
            return Ok(None);
        };
        let Some(path) = mapping.snapshot_path else {
            return Ok(None);
        };
        let source = String::from_utf8(read_bounded(&path)?)
            .context("Prepared source snapshot is not UTF-8")?;
        Ok(Some(SourceSnapshot {
            original_path: mapping.original_path,
            source,
        }))
    }

    /// Acknowledges a resume request. Poll `state` for the next pause or finish.
    /// A revision prevents a delayed command acting on a newer paused scope.
    pub fn resume(&self, action: ResumeAction, revision: u64) -> Result<()> {
        self.request(
            action.command(),
            Some(revision),
            None,
            None,
            None,
            DEFAULT_TIMEOUT,
        )?;
        Ok(())
    }

    pub fn workspace(&self, revision: u64) -> Result<Vec<DebugVariable>> {
        let value = self.request(
            "workspace",
            Some(revision),
            None,
            None,
            None,
            DEFAULT_TIMEOUT,
        )?;
        decode_array_or_single(value)
    }

    pub fn stack(&self, revision: u64) -> Result<Vec<DebugFrame>> {
        let value = self.request("stack", Some(revision), None, None, None, DEFAULT_TIMEOUT)?;
        let mut frames: Vec<DebugFrame> = decode_array_or_single(value)?;
        for frame in &mut frames {
            self.map_location(&mut frame.file, &mut frame.name)?;
        }
        Ok(frames)
    }

    /// Evaluate in the paused scope. This can mutate user variables, just as a
    /// debugger command window can. A timeout does not terminate the interpreter.
    pub fn evaluate(&self, code: &str, revision: u64) -> Result<String> {
        let value = self.request(
            "evaluate",
            Some(revision),
            Some(code),
            None,
            None,
            DEFAULT_TIMEOUT,
        )?;
        serde_json::from_value(value).context("Debugger evaluation did not return text")
    }

    /// Evaluate with live console output and a bounded returned copy. This uses
    /// the actual paused scope, supports input(), and never redirects the pager
    /// buffer owned and reset by the numerical interpreter.
    pub fn evaluate_output(&self, code: &str, revision: u64) -> Result<EvaluationOutput> {
        let response = self.exchange_response(
            Request {
                id: 0,
                command: "evaluate_stream",
                revision: Some(revision),
                code: Some(code),
                file: None,
                line: None,
                scope_file: None,
                local_function: None,
                original_file: None,
                snapshot_file: None,
            },
            DEFAULT_TIMEOUT,
        )?;
        Ok(EvaluationOutput {
            output: serde_json::from_value(response.result)
                .context("Debugger evaluation did not return text")?,
            streamed: response.evaluation_streamed,
            truncated: response.output_truncated,
        })
    }

    pub fn set_breakpoint(&self, file: &Path, line: usize) -> Result<Vec<usize>> {
        ensure!(line > 0, "Breakpoint lines are one based");
        ensure!(
            line <= i32::MAX as usize,
            "Breakpoint line exceeds the runtime's integer range"
        );
        ensure!(
            file.is_absolute(),
            "A breakpoint requires an absolute source path"
        );
        let mapping = self.source_for_breakpoint(file, line)?;
        let mut installed = self
            .inner
            .breakpoints
            .lock()
            .map_err(|_| anyhow::anyhow!("Breakpoint lock is poisoned"))?;
        let value = self.breakpoint_request(file, line, false, mapping.as_ref())?;
        // Octave encodes a one-element numeric array as a scalar.
        let lines: Vec<usize> = decode_array_or_single(value)?;
        ensure!(
            !lines.is_empty() && lines.iter().all(|line| *line > 0),
            "The runtime did not install a breakpoint"
        );
        let original_path = fs::canonicalize(file).unwrap_or_else(|_| file.to_owned());
        let mut groups: Vec<_> = installed
            .groups
            .iter()
            .filter(|group| {
                group.original_path == original_path && group.active_lines.contains(&line)
            })
            .map(|group| group.id)
            .collect();
        if groups.is_empty() {
            installed.next_id += 1;
            let id = installed.next_id;
            installed.groups.push(BreakpointGroup {
                id,
                original_path,
                active_lines: lines.clone(),
            });
            groups.push(id);
        } else {
            for group in &mut installed.groups {
                if groups.contains(&group.id) {
                    group.active_lines = lines.clone();
                }
            }
        }
        for group_id in groups {
            for actual_line in &lines {
                let point = InstalledBreakpoint {
                    group_id,
                    actual_line: *actual_line,
                    mapping: mapping.clone(),
                };
                if !installed.installed.contains(&point) {
                    installed.installed.push(point);
                }
            }
        }
        Ok(lines)
    }

    pub fn clear_breakpoint(&self, file: &Path, line: usize) -> Result<()> {
        ensure!(line > 0, "Breakpoint lines are one based");
        ensure!(
            line <= i32::MAX as usize,
            "Breakpoint line exceeds the runtime's integer range"
        );
        ensure!(
            file.is_absolute(),
            "A breakpoint requires an absolute source path"
        );
        let original_path = fs::canonicalize(file).unwrap_or_else(|_| file.to_owned());
        let mut installed = self
            .inner
            .breakpoints
            .lock()
            .map_err(|_| anyhow::anyhow!("Breakpoint lock is poisoned"))?;
        let groups: Vec<_> = installed
            .groups
            .iter()
            .filter(|group| {
                group.original_path == original_path && group.active_lines.contains(&line)
            })
            .map(|group| group.id)
            .collect();
        let selected: Vec<_> = installed
            .installed
            .iter()
            .filter(|point| groups.contains(&point.group_id))
            .cloned()
            .collect();
        if selected.is_empty() {
            let mapping = self.source_for_breakpoint(file, line)?;
            self.breakpoint_request(file, line, true, mapping.as_ref())?;
        } else {
            let mut removed = Vec::new();
            for point in &selected {
                let target = (point.mapping.clone(), point.actual_line);
                if !removed.contains(&target) {
                    self.breakpoint_request(file, point.actual_line, true, point.mapping.as_ref())?;
                    removed.push(target);
                }
            }
            installed
                .installed
                .retain(|point| !groups.contains(&point.group_id));
            installed.groups.retain(|group| !groups.contains(&group.id));
        }
        Ok(())
    }

    fn breakpoint_request(
        &self,
        file: &Path,
        line: usize,
        clear: bool,
        mapping: Option<&SourceMapping>,
    ) -> Result<Value> {
        if let Some(mapping) = mapping {
            self.exchange(
                Request {
                    id: 0,
                    command: if clear {
                        "prepared_clear_breakpoint"
                    } else {
                        "prepared_breakpoint"
                    },
                    revision: None,
                    code: None,
                    file: Some(&mapping.generated_path),
                    line: Some(line),
                    scope_file: Some(&mapping.scope_file),
                    local_function: mapping.local_function.as_deref(),
                    original_file: mapping
                        .snapshot_path
                        .as_ref()
                        .map(|_| mapping.original_path.as_path()),
                    snapshot_file: mapping.snapshot_path.as_deref(),
                },
                DEFAULT_TIMEOUT,
            )
        } else {
            self.request(
                if clear {
                    "clear_breakpoint"
                } else {
                    "breakpoint"
                },
                None,
                None,
                Some(file),
                Some(line),
                DEFAULT_TIMEOUT,
            )
        }
    }

    fn request(
        &self,
        command: &str,
        revision: Option<u64>,
        code: Option<&str>,
        file: Option<&Path>,
        line: Option<usize>,
        timeout: Duration,
    ) -> Result<Value> {
        self.exchange(
            Request {
                id: 0,
                command,
                revision,
                code,
                file,
                line,
                scope_file: None,
                local_function: None,
                original_file: None,
                snapshot_file: None,
            },
            timeout,
        )
    }

    fn exchange(&self, request: Request<'_>, timeout: Duration) -> Result<Value> {
        self.exchange_response(request, timeout)
            .map(|response| response.result)
    }

    fn exchange_response(&self, mut request: Request<'_>, timeout: Duration) -> Result<Response> {
        let guard = self
            .inner
            .request_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("Debugger request lock is poisoned"))?;
        ensure!(self.directory().is_dir(), "The debugger session has ended");
        let id = self.inner.sequence.fetch_add(1, Ordering::Relaxed);
        request.id = id;
        let raw = serde_json::to_vec(&request)?;
        ensure!(
            raw.len() <= MAX_REQUEST,
            "Debugger command exceeds the 1 MiB limit"
        );
        let temporary = self.directory().join(format!("debug-command-{id}.pending"));
        let command_path = self.directory().join("debug-command.json");
        let response_path = self.directory().join(format!("debug-response-{id}.json"));
        let deadline = Instant::now() + timeout;
        while command_path.exists() {
            ensure!(
                Instant::now() < deadline,
                "Another debugger command is still pending"
            );
            ensure!(self.directory().is_dir(), "The debugger session has ended");
            thread::sleep(Duration::from_millis(5));
        }
        fs::write(&temporary, raw).context("Write debugger command")?;
        if let Err(error) = fs::rename(&temporary, &command_path) {
            let _ = fs::remove_file(&temporary);
            return Err(error).context("Submit debugger command");
        }
        // Only publication is serialized. A paused expression can itself hit a
        // breakpoint, so another client thread must be able to resume its scope
        // while this call waits for the expression's response.
        drop(guard);
        loop {
            match read_bounded(&response_path) {
                Ok(bytes) => {
                    let _ = fs::remove_file(&response_path);
                    let response: Response =
                        serde_json::from_slice(&bytes).context("Invalid debugger response")?;
                    ensure!(
                        response.id == id,
                        "Debugger response ID does not match the request"
                    );
                    ensure!(response.ok, "Debugger: {}", response.error);
                    return Ok(response);
                }
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) => {}
                Err(error) => return Err(error),
            }
            if Instant::now() >= deadline {
                // Remove an unconsumed request, but never remove a newer request.
                let _guard = self
                    .inner
                    .request_lock
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Debugger request lock is poisoned"))?;
                if let Ok(bytes) = fs::read(&command_path) {
                    if serde_json::from_slice::<Value>(&bytes)
                        .ok()
                        .and_then(|v| v.get("id").and_then(Value::as_u64))
                        == Some(id)
                    {
                        let _ = fs::remove_file(&command_path);
                    }
                }
                bail!("Debugger command timed out; the interpreter may still be running it");
            }
            ensure!(self.directory().is_dir(), "The debugger session has ended");
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    ensure!(
        fs::metadata(path)?.len() <= MAX_RESPONSE,
        "Debugger response exceeds the 8 MiB limit"
    );
    let bytes = fs::read(path)?;
    ensure!(
        bytes.len() as u64 <= MAX_RESPONSE,
        "Debugger response exceeds the 8 MiB limit"
    );
    Ok(bytes)
}

fn decode_array_or_single<T: DeserializeOwned>(value: Value) -> Result<Vec<T>> {
    if value.is_array() {
        serde_json::from_value(value).context("Invalid debugger result array")
    } else if value.is_null() {
        Ok(Vec::new())
    } else {
        Ok(vec![
            serde_json::from_value(value).context("Invalid debugger result")?
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_generations_route_original_lines_and_keep_old_stack_locations() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let original = root.join("edited.m");
        let script = root.join("generation-one.mlsource");
        let function = root.join("local.m");
        fs::write(&script, "value=local();\n").unwrap();
        fs::write(&function, "function value=local()\nvalue=3;\nend\n").unwrap();
        let debugger = Debugger::new(&root);
        let first = vec![
            SourceMapping {
                generated_path: script.clone(),
                original_path: original.clone(),
                scope_file: script.clone(),
                local_function: None,
                start_line: None,
                end_line: None,
                snapshot_path: None,
            },
            SourceMapping {
                generated_path: function.clone(),
                original_path: original.clone(),
                scope_file: script.clone(),
                local_function: Some("local".into()),
                start_line: Some(4),
                end_line: Some(8),
                snapshot_path: None,
            },
        ];
        debugger.register_sources(first.clone()).unwrap();
        assert_eq!(
            debugger
                .source_for_breakpoint(&original, 2)
                .unwrap()
                .unwrap()
                .generated_path,
            script
        );
        assert_eq!(
            debugger
                .source_for_breakpoint(&original, 6)
                .unwrap()
                .unwrap()
                .generated_path,
            function
        );
        let next = root.join("generation-two.mlsource");
        fs::write(&next, "value=42;\n").unwrap();
        debugger
            .register_sources(vec![SourceMapping {
                generated_path: next.clone(),
                original_path: original.clone(),
                scope_file: next.clone(),
                local_function: None,
                start_line: None,
                end_line: None,
                snapshot_path: None,
            }])
            .unwrap();
        assert_eq!(
            debugger
                .source_for_breakpoint(&original, 6)
                .unwrap()
                .unwrap()
                .generated_path,
            next
        );
        let mut nested_file = function.clone();
        let mut nested_name = "nested".to_owned();
        debugger
            .map_location(&mut nested_file, &mut nested_name)
            .unwrap();
        assert_eq!(nested_name, "edited>local>nested");
        let mut old_file = function;
        let mut old_name = "local>nested".to_owned();
        debugger.map_location(&mut old_file, &mut old_name).unwrap();
        assert_eq!(old_file, original);
        assert_eq!(old_name, "edited>local>nested");
        debugger.register_sources(first).unwrap();
        assert_eq!(
            debugger
                .source_for_breakpoint(&original, 2)
                .unwrap()
                .unwrap()
                .generated_path,
            script
        );
        fs::write(&original, "value=1;\n").unwrap();
        debugger
            .register_sources(vec![SourceMapping {
                generated_path: original.clone(),
                original_path: original.clone(),
                scope_file: original.clone(),
                local_function: None,
                start_line: None,
                end_line: None,
                snapshot_path: None,
            }])
            .unwrap();
        assert!(debugger
            .source_for_breakpoint(&original, 2)
            .unwrap()
            .is_none());
        let mut direct = original.clone();
        let mut direct_name = "edited>method".to_owned();
        debugger
            .map_location(&mut direct, &mut direct_name)
            .unwrap();
        assert_eq!(direct_name, "edited>method");
    }

    #[test]
    fn paused_source_snapshot_keeps_old_generations_after_original_file_edits() {
        let directory = tempfile::tempdir().unwrap();
        let debugger = Debugger::new(directory.path());
        let original = directory.path().join("original.m");
        fs::write(&original, "current = 99;\n").unwrap();
        let mut generated = Vec::new();
        for (name, source) in [("old", "answer = 1;\n"), ("new", "answer = 2;\n")] {
            let script = directory.path().join(format!("{name}.mlsource"));
            let snapshot = directory.path().join(format!("{name}.mlsnapshot"));
            fs::write(&script, source).unwrap();
            fs::write(&snapshot, source).unwrap();
            debugger
                .register_sources(vec![SourceMapping {
                    generated_path: script.clone(),
                    original_path: original.clone(),
                    scope_file: script.clone(),
                    local_function: None,
                    start_line: None,
                    end_line: None,
                    snapshot_path: Some(snapshot),
                }])
                .unwrap();
            generated.push(script);
        }
        fs::write(directory.path().join("debug-state.json"), serde_json::to_vec(&serde_json::json!({ "revision": 7, "paused": true, "file": generated[0], "line": 1, "function_name": "old" })).unwrap()).unwrap();
        let state = debugger.state().unwrap().unwrap();
        assert_eq!(state.file, original.canonicalize().unwrap());
        assert_eq!(state.runtime_file, generated[0]);
        assert_eq!(
            debugger
                .source_snapshot(&state.runtime_file)
                .unwrap()
                .unwrap()
                .source,
            "answer = 1;\n"
        );
        assert_eq!(
            debugger
                .source_snapshot(&generated[1])
                .unwrap()
                .unwrap()
                .source,
            "answer = 2;\n"
        );
        assert_eq!(fs::read_to_string(original).unwrap(), "current = 99;\n");
    }

    #[test]
    fn source_snapshot_read_is_bounded_without_reading_an_oversized_file() {
        let directory = tempfile::tempdir().unwrap();
        let debugger = Debugger::new(directory.path());
        let script = directory.path().join("script.mlsource");
        let snapshot = directory.path().join("source.mlsnapshot");
        fs::write(&script, "x = 1;").unwrap();
        fs::File::create(&snapshot)
            .unwrap()
            .set_len(MAX_RESPONSE + 1)
            .unwrap();
        debugger
            .register_sources(vec![SourceMapping {
                generated_path: script.clone(),
                original_path: directory.path().join("original.m"),
                scope_file: script.clone(),
                local_function: None,
                start_line: None,
                end_line: None,
                snapshot_path: Some(snapshot),
            }])
            .unwrap();
        assert!(debugger.source_snapshot(&script).is_err());
    }

    #[test]
    fn scalar_and_array_stack_metadata_decode() {
        let frame = serde_json::json!({"file":"/tmp/proof.m","name":"proof","line":2,"column":1});
        let single: Vec<DebugFrame> = decode_array_or_single(frame.clone()).unwrap();
        let array: Vec<DebugFrame> = decode_array_or_single(Value::Array(vec![frame])).unwrap();
        assert_eq!(single, array);
    }

    #[test]
    fn revision_and_response_identity_are_preserved() {
        let directory = tempfile::tempdir().unwrap();
        let debugger = Debugger::new(directory.path());
        let peer = debugger.clone();
        let thread = thread::spawn(move || loop {
            let path = peer.directory().join("debug-command.json");
            if let Ok(raw) = fs::read(&path) {
                let value: Value = serde_json::from_slice(&raw).unwrap();
                assert_eq!(value["revision"], 17);
                fs::remove_file(path).unwrap();
                fs::write(
                    peer.directory().join("debug-response-1.json"),
                    b"{\"id\":1,\"ok\":true,\"result\":\"42\\n\"}",
                )
                .unwrap();
                break;
            }
            thread::sleep(Duration::from_millis(5));
        });
        assert_eq!(debugger.evaluate("disp(42)", 17).unwrap(), "42\n");
        thread.join().unwrap();
        assert!(!directory.path().join("debug-response-1.json").exists());
    }

    #[test]
    fn timeout_removes_unconsumed_request() {
        let directory = tempfile::tempdir().unwrap();
        let debugger = Debugger::new(directory.path());
        let error = debugger
            .request("step", Some(1), None, None, None, Duration::from_millis(15))
            .unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert!(!directory.path().join("debug-command.json").exists());
    }
}
