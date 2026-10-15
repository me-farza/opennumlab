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
    anchor_line: usize,
    actual_line: usize,
    mapping: Option<SourceMapping>,
    confirmed: bool,
}

impl BreakpointRegistry {
    fn confirm_installation(&mut self, point: InstalledBreakpoint) {
        if let Some(existing) = self.installed.iter_mut().find(|existing| {
            existing.group_id == point.group_id
                && existing.anchor_line == point.anchor_line
                && existing.actual_line == point.actual_line
                && existing.mapping == point.mapping
        }) {
            existing.confirmed = true;
        } else {
            self.installed.push(point);
        }
    }
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
        let registered_originals: Vec<_> = mappings
            .iter()
            .map(|mapping| mapping.original_path.clone())
            .collect();
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
        let routing = sources.clone();
        // A breakpoint request can wait for the interpreter. Do not retain the
        // source lock across it: concurrent state reads still need old mappings.
        drop(sources);
        self.rebind_registered_breakpoints(&routing, &registered_originals)
    }

    fn rebind_registered_breakpoints(
        &self,
        sources: &[SourceMapping],
        originals: &[PathBuf],
    ) -> Result<()> {
        let mut installed = self
            .inner
            .breakpoints
            .lock()
            .map_err(|_| anyhow::anyhow!("Breakpoint lock is poisoned"))?;
        let groups: Vec<_> = installed
            .groups
            .iter()
            .filter(|group| originals.contains(&group.original_path))
            .map(|group| {
                (
                    group.id,
                    group.original_path.clone(),
                    group.active_lines.clone(),
                )
            })
            .collect();
        for (group_id, original, lines) in groups {
            for line in lines {
                let mapping = source_for_breakpoint_in(sources, &original, line);
                if mapping.is_none()
                    && !installed.installed.iter().any(|point| {
                        point.group_id == group_id
                            && point.anchor_line == line
                            && point.mapping.is_none()
                            && !point.confirmed
                    })
                {
                    // Ordinary source registration keeps its direct identity
                    // and normally sends no command. Repair only a known
                    // ordinary target whose earlier clear became uncertain.
                    continue;
                }
                if installed.installed.iter().any(|point| {
                    point.confirmed
                        && point.group_id == group_id
                        && point.anchor_line == line
                        && point.mapping == mapping
                }) {
                    continue;
                }
                let value = self.breakpoint_request(&original, line, false, mapping.as_ref())?;
                let actual_lines: Vec<usize> = decode_array_or_single(value)?;
                ensure!(
                    !actual_lines.is_empty() && actual_lines.iter().all(|line| *line > 0),
                    "The runtime did not install a breakpoint in the registered source"
                );
                for actual_line in actual_lines {
                    installed.confirm_installation(InstalledBreakpoint {
                        group_id,
                        anchor_line: line,
                        actual_line,
                        mapping: mapping.clone(),
                        confirmed: true,
                    });
                }
                // Keep the existing editor marker as the group's anchor. If a
                // new blank/comment line moves the runtime's actual location,
                // clearing that marker must still remove every generation.
                // Record each successful target immediately so a later failed
                // group does not leave untracked marks behind.
            }
        }
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
        Ok(source_for_breakpoint_in(&sources, &normalized, line))
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
        let mut installed = self
            .inner
            .breakpoints
            .lock()
            .map_err(|_| anyhow::anyhow!("Breakpoint lock is poisoned"))?;
        // Hold the registry before choosing the source generation. Otherwise a
        // concurrent registration could finish migration, then this command
        // could install a new group using a mapping read before that migration.
        let mapping = self.source_for_breakpoint(file, line)?;
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
                    anchor_line: *actual_line,
                    actual_line: *actual_line,
                    mapping: mapping.clone(),
                    confirmed: true,
                };
                installed.confirm_installation(point);
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
        let mut groups: Vec<_> = installed
            .groups
            .iter()
            .filter(|group| {
                group.original_path == original_path && group.active_lines.contains(&line)
            })
            .map(|group| group.id)
            .collect();
        // Prefer the editor's exact marker. An earlier generation's relocated
        // actual line may coincide with another independently enabled marker.
        if groups.is_empty() {
            groups = installed
                .groups
                .iter()
                .filter(|group| {
                    group.original_path == original_path
                        && installed
                            .installed
                            .iter()
                            .any(|point| point.group_id == group.id && point.actual_line == line)
                })
                .map(|group| group.id)
                .collect();
        }
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
            // Any clear may have been accepted before a later request fails or
            // its reply is lost. Keep logical ownership for an explicit retry,
            // but do not let these records suppress a cached-generation rebind.
            for point in &mut installed.installed {
                if groups.contains(&point.group_id) {
                    point.confirmed = false;
                }
            }
            let mut removed = Vec::new();
            for point in &selected {
                let shared = installed.installed.iter().any(|other| {
                    !groups.contains(&other.group_id)
                        && other.mapping == point.mapping
                        && other.actual_line == point.actual_line
                        && (point.mapping.is_some()
                            || installed.groups.iter().any(|group| {
                                group.id == other.group_id && group.original_path == original_path
                            }))
                });
                // A prepared body/line can serve multiple markers after edits.
                // Remove its physical mark only after its final owner is cleared.
                // Ordinary targets additionally require the same original file;
                // mapping=None and equal lines alone do not identify one target.
                if shared {
                    continue;
                }
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

fn source_for_breakpoint_in(
    sources: &[SourceMapping],
    original: &Path,
    line: usize,
) -> Option<SourceMapping> {
    let latest = sources
        .iter()
        .rev()
        .find(|mapping| mapping.original_path == original)?;
    if latest.generated_path == latest.original_path {
        return None;
    }
    let active = sources.iter().filter(|mapping| {
        mapping.original_path == original && mapping.scope_file == latest.scope_file
    });
    let selected = active
        .clone()
        .filter(|mapping| {
            matches!((mapping.start_line, mapping.end_line), (Some(start), Some(end)) if line >= start && line <= end)
        })
        .min_by_key(|mapping| mapping.end_line.unwrap() - mapping.start_line.unwrap());
    selected
        .or_else(|| {
            active
                .into_iter()
                .find(|mapping| mapping.local_function.is_none())
        })
        .cloned()
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

    fn scripted_adapter(
        debugger: &Debugger,
        responses: Vec<Value>,
    ) -> thread::JoinHandle<Vec<Value>> {
        let directory = debugger.directory().to_owned();
        thread::spawn(move || {
            let mut commands = Vec::new();
            for mut response in responses {
                let deadline = Instant::now() + Duration::from_secs(5);
                let command_path = directory.join("debug-command.json");
                let command: Value = loop {
                    match fs::read(&command_path) {
                        Ok(raw) => break serde_json::from_slice(&raw).unwrap(),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            assert!(Instant::now() < deadline, "Missing debugger request");
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(error) => panic!("Read debugger request: {error}"),
                    }
                };
                let id = command["id"].as_u64().unwrap();
                fs::remove_file(command_path).unwrap();
                response["id"] = Value::from(id);
                let temporary = directory.join("test-response.tmp");
                fs::write(&temporary, serde_json::to_vec(&response).unwrap()).unwrap();
                fs::rename(
                    temporary,
                    directory.join(format!("debug-response-{id}.json")),
                )
                .unwrap();
                commands.push(command);
            }
            commands
        })
    }

    #[test]
    fn source_generations_migrate_existing_breakpoints_and_clear_every_target() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let original = root.join("saved.m");
        let first = root.join("generation-one.m");
        let second = root.join("generation-two.m");
        for path in [&original, &first, &second] {
            fs::write(path, "before=1;\nafter=before+1;\n").unwrap();
        }
        let mapping = |generated: &Path| SourceMapping {
            generated_path: generated.to_owned(),
            original_path: original.clone(),
            scope_file: generated.to_owned(),
            local_function: None,
            start_line: None,
            end_line: None,
            snapshot_path: None,
        };
        let debugger = Debugger::new(&root);
        let adapter = scripted_adapter(
            &debugger,
            vec![
                serde_json::json!({"ok":true,"result":2}),
                serde_json::json!({"ok":true,"result":2}),
                // The new source moves the executable target past a blank line.
                serde_json::json!({"ok":true,"result":3}),
                serde_json::json!({"ok":true,"result":[]}),
                serde_json::json!({"ok":true,"result":[]}),
                serde_json::json!({"ok":true,"result":[]}),
            ],
        );
        assert_eq!(debugger.set_breakpoint(&original, 2).unwrap(), vec![2]);
        debugger.register_sources(vec![mapping(&first)]).unwrap();
        debugger.register_sources(vec![mapping(&first)]).unwrap();
        debugger.register_sources(vec![mapping(&second)]).unwrap();
        debugger.register_sources(vec![mapping(&second)]).unwrap();
        // Returning to an already installed immutable generation needs no bind.
        debugger.register_sources(vec![mapping(&first)]).unwrap();
        debugger.register_sources(vec![mapping(&second)]).unwrap();
        let mut old_frame = first.clone();
        let mut old_name = String::new();
        debugger
            .map_location(&mut old_frame, &mut old_name)
            .unwrap();
        assert_eq!(old_frame, original);
        // A later direct registration must not create additional installations.
        debugger.register_sources(vec![mapping(&original)]).unwrap();
        debugger.clear_breakpoint(&original, 2).unwrap();
        assert!(debugger.inner.breakpoints.lock().unwrap().groups.is_empty());
        assert!(debugger
            .inner
            .breakpoints
            .lock()
            .unwrap()
            .installed
            .is_empty());
        let commands = adapter.join().unwrap();
        assert_eq!(
            commands
                .iter()
                .map(|command| command["command"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![
                "breakpoint",
                "prepared_breakpoint",
                "prepared_breakpoint",
                "clear_breakpoint",
                "prepared_clear_breakpoint",
                "prepared_clear_breakpoint",
            ]
        );
        assert_eq!(commands[1]["file"], serde_json::json!(first));
        assert_eq!(commands[2]["file"], serde_json::json!(second));
        assert_eq!(commands[3]["line"], 2);
        assert_eq!(commands[4]["line"], 2);
        assert_eq!(commands[5]["line"], 3);
    }

    #[test]
    fn failed_generation_rebind_preserves_successful_targets_and_retries_only_missing_ones() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let original = root.join("saved.m");
        let generated = root.join("generation.m");
        for path in [&original, &generated] {
            fs::write(path, "a=1;\nb=2;\nc=3;\nd=4;\n").unwrap();
        }
        let mapping = SourceMapping {
            generated_path: generated.clone(),
            original_path: original.clone(),
            scope_file: generated.clone(),
            local_function: None,
            start_line: None,
            end_line: None,
            snapshot_path: None,
        };
        let debugger = Debugger::new(&root);
        let adapter = scripted_adapter(
            &debugger,
            vec![
                serde_json::json!({"ok":true,"result":2}),
                serde_json::json!({"ok":true,"result":4}),
                serde_json::json!({"ok":true,"result":2}),
                serde_json::json!({"ok":false,"error":"synthetic rebind failure"}),
                serde_json::json!({"ok":true,"result":5}),
                serde_json::json!({"ok":true,"result":[]}),
                serde_json::json!({"ok":true,"result":[]}),
                serde_json::json!({"ok":true,"result":[]}),
                serde_json::json!({"ok":true,"result":[]}),
            ],
        );
        debugger.set_breakpoint(&original, 2).unwrap();
        debugger.set_breakpoint(&original, 4).unwrap();
        let error = debugger
            .register_sources(vec![mapping.clone()])
            .unwrap_err();
        assert!(error.to_string().contains("synthetic rebind failure"));
        debugger.register_sources(vec![mapping.clone()]).unwrap();
        debugger.register_sources(vec![mapping]).unwrap();
        debugger.clear_breakpoint(&original, 2).unwrap();
        // Clearing the runtime's shifted actual line also finds its old marker.
        debugger.clear_breakpoint(&original, 5).unwrap();
        let commands = adapter.join().unwrap();
        assert_eq!(commands[2]["line"], 2);
        assert_eq!(commands[3]["line"], 4);
        assert_eq!(commands[4]["line"], 4);
        assert_eq!(commands[5]["command"], "clear_breakpoint");
        assert_eq!(commands[5]["line"], 2);
        assert_eq!(commands[6]["command"], "prepared_clear_breakpoint");
        assert_eq!(commands[6]["line"], 2);
        assert_eq!(commands[7]["line"], 4);
        assert_eq!(commands[8]["line"], 5);
        assert!(debugger
            .inner
            .breakpoints
            .lock()
            .unwrap()
            .installed
            .is_empty());
    }

    #[test]
    fn converging_breakpoint_groups_keep_shared_targets_until_the_final_owner_clears() {
        for first_clear in [2, 3] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().canonicalize().unwrap();
            let original = root.join("saved.m");
            let generated = root.join("generation.m");
            for path in [&original, &generated] {
                fs::write(path, "a=1;\nb=2;\nc=3;\n").unwrap();
            }
            let debugger = Debugger::new(&root);
            let adapter = scripted_adapter(
                &debugger,
                vec![
                    serde_json::json!({"ok":true,"result":2}),
                    serde_json::json!({"ok":true,"result":3}),
                    serde_json::json!({"ok":true,"result":3}),
                    serde_json::json!({"ok":true,"result":[]}),
                    serde_json::json!({"ok":true,"result":[]}),
                ],
            );
            debugger.set_breakpoint(&original, 2).unwrap();
            debugger
                .register_sources(vec![SourceMapping {
                    generated_path: generated.clone(),
                    original_path: original.clone(),
                    scope_file: generated.clone(),
                    local_function: None,
                    start_line: None,
                    end_line: None,
                    snapshot_path: None,
                }])
                .unwrap();
            debugger.set_breakpoint(&original, 3).unwrap();
            debugger.clear_breakpoint(&original, first_clear).unwrap();
            let last_clear = if first_clear == 2 { 3 } else { 2 };
            {
                let registry = debugger.inner.breakpoints.lock().unwrap();
                assert_eq!(registry.groups.len(), 1);
                assert_eq!(registry.groups[0].active_lines, vec![last_clear]);
                assert!(registry.installed.iter().any(|point| {
                    point.mapping.as_ref().is_some_and(|mapping| {
                        mapping.generated_path == generated && point.actual_line == 3
                    })
                }));
            }
            debugger.clear_breakpoint(&original, last_clear).unwrap();
            let commands = adapter.join().unwrap();
            assert_eq!(commands[3]["command"], "clear_breakpoint");
            assert_eq!(commands[3]["file"], serde_json::json!(original));
            assert_eq!(commands[3]["line"], 2);
            assert_eq!(commands[4]["command"], "prepared_clear_breakpoint");
            assert_eq!(commands[4]["file"], serde_json::json!(generated));
            assert_eq!(commands[4]["line"], 3);
            let registry = debugger.inner.breakpoints.lock().unwrap();
            assert!(registry.groups.is_empty());
            assert!(registry.installed.is_empty());
        }
    }

    #[test]
    fn ordinary_breakpoints_at_equal_lines_in_different_files_are_distinct_targets() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let first = root.join("first.m");
        let second = root.join("second.m");
        for path in [&first, &second] {
            fs::write(path, "a=1;\nb=2;\n").unwrap();
        }
        let debugger = Debugger::new(&root);
        let adapter = scripted_adapter(
            &debugger,
            vec![
                serde_json::json!({"ok":true,"result":2}),
                serde_json::json!({"ok":true,"result":2}),
                serde_json::json!({"ok":true,"result":[]}),
                serde_json::json!({"ok":true,"result":[]}),
            ],
        );
        debugger.set_breakpoint(&first, 2).unwrap();
        debugger.set_breakpoint(&second, 2).unwrap();
        debugger.clear_breakpoint(&first, 2).unwrap();
        assert_eq!(debugger.inner.breakpoints.lock().unwrap().groups.len(), 1);
        debugger.clear_breakpoint(&second, 2).unwrap();
        let commands = adapter.join().unwrap();
        assert_eq!(commands[2]["command"], "clear_breakpoint");
        assert_eq!(commands[2]["file"], serde_json::json!(first));
        assert_eq!(commands[3]["command"], "clear_breakpoint");
        assert_eq!(commands[3]["file"], serde_json::json!(second));
    }

    #[test]
    fn failed_clear_keeps_group_records_for_explicit_retry() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let original = root.join("saved.m");
        let generated = root.join("generation.m");
        for path in [&original, &generated] {
            fs::write(path, "a=1;\nb=2;\n").unwrap();
        }
        let debugger = Debugger::new(&root);
        let adapter = scripted_adapter(
            &debugger,
            vec![
                serde_json::json!({"ok":true,"result":2}),
                serde_json::json!({"ok":true,"result":2}),
                serde_json::json!({"ok":true,"result":[]}),
                serde_json::json!({"ok":false,"error":"synthetic clear failure"}),
                serde_json::json!({"ok":true,"result":[]}),
                serde_json::json!({"ok":true,"result":[]}),
            ],
        );
        debugger.set_breakpoint(&original, 2).unwrap();
        debugger
            .register_sources(vec![SourceMapping {
                generated_path: generated.clone(),
                original_path: original.clone(),
                scope_file: generated.clone(),
                local_function: None,
                start_line: None,
                end_line: None,
                snapshot_path: None,
            }])
            .unwrap();
        let error = debugger.clear_breakpoint(&original, 2).unwrap_err();
        assert!(error.to_string().contains("synthetic clear failure"));
        {
            let registry = debugger.inner.breakpoints.lock().unwrap();
            assert_eq!(registry.groups.len(), 1);
            assert_eq!(registry.installed.len(), 2);
        }
        debugger.clear_breakpoint(&original, 2).unwrap();
        let commands = adapter.join().unwrap();
        assert_eq!(commands[2]["command"], "clear_breakpoint");
        assert_eq!(commands[3]["command"], "prepared_clear_breakpoint");
        assert_eq!(commands[4]["command"], "clear_breakpoint");
        assert_eq!(commands[5]["command"], "prepared_clear_breakpoint");
        assert!(debugger.inner.breakpoints.lock().unwrap().groups.is_empty());
    }

    #[test]
    fn cached_generation_rebinds_after_partial_clear_failure_without_duplicate_targets() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let original = root.join("saved.m");
        let first = root.join("generation-one.m");
        let second = root.join("generation-two.m");
        for path in [&original, &first, &second] {
            fs::write(path, "a=1;\nb=2;\n").unwrap();
        }
        let mapping = |generated: &Path| SourceMapping {
            generated_path: generated.to_owned(),
            original_path: original.clone(),
            scope_file: generated.to_owned(),
            local_function: None,
            start_line: None,
            end_line: None,
            snapshot_path: None,
        };
        let debugger = Debugger::new(&root);
        let adapter = scripted_adapter(
            &debugger,
            vec![
                serde_json::json!({"ok":true,"result":2}),
                serde_json::json!({"ok":true,"result":2}),
                serde_json::json!({"ok":true,"result":2}),
                serde_json::json!({"ok":true,"result":[]}),
                serde_json::json!({"ok":true,"result":[]}),
                serde_json::json!({"ok":false,"error":"synthetic late clear failure"}),
                serde_json::json!({"ok":true,"result":2}),
                serde_json::json!({"ok":true,"result":[]}),
                serde_json::json!({"ok":true,"result":[]}),
                serde_json::json!({"ok":true,"result":[]}),
            ],
        );
        debugger.set_breakpoint(&original, 2).unwrap();
        debugger.register_sources(vec![mapping(&first)]).unwrap();
        debugger.register_sources(vec![mapping(&second)]).unwrap();
        debugger.register_sources(vec![mapping(&first)]).unwrap();
        let error = debugger.clear_breakpoint(&original, 2).unwrap_err();
        assert!(error.to_string().contains("synthetic late clear failure"));
        {
            let registry = debugger.inner.breakpoints.lock().unwrap();
            assert_eq!(registry.groups.len(), 1);
            assert_eq!(registry.installed.len(), 3);
            assert!(registry.installed.iter().all(|point| !point.confirmed));
        }
        // The current cached generation was physically cleared before the
        // later failure. Its retained record must not suppress this rebind.
        debugger.register_sources(vec![mapping(&first)]).unwrap();
        debugger.register_sources(vec![mapping(&first)]).unwrap();
        {
            let registry = debugger.inner.breakpoints.lock().unwrap();
            assert_eq!(registry.installed.len(), 3);
            assert_eq!(
                registry
                    .installed
                    .iter()
                    .filter(|point| point.confirmed)
                    .count(),
                1
            );
            assert!(registry.installed.iter().any(|point| {
                point.confirmed
                    && point
                        .mapping
                        .as_ref()
                        .is_some_and(|mapping| mapping.generated_path == first)
            }));
        }
        debugger.clear_breakpoint(&original, 2).unwrap();
        let commands = adapter.join().unwrap();
        assert_eq!(commands[4]["command"], "prepared_clear_breakpoint");
        assert_eq!(commands[4]["file"], serde_json::json!(first));
        assert_eq!(commands[5]["file"], serde_json::json!(second));
        assert_eq!(commands[6]["command"], "prepared_breakpoint");
        assert_eq!(commands[6]["file"], serde_json::json!(first));
        assert_eq!(commands[7]["command"], "clear_breakpoint");
        assert_eq!(commands[8]["file"], serde_json::json!(first));
        assert_eq!(commands[9]["file"], serde_json::json!(second));
        assert!(debugger
            .inner
            .breakpoints
            .lock()
            .unwrap()
            .installed
            .is_empty());
    }

    #[test]
    fn direct_registration_repairs_only_unconfirmed_ordinary_breakpoints() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let original = root.join("saved.m");
        fs::write(&original, "a=1;\nb=2;\n").unwrap();
        let mapping = SourceMapping {
            generated_path: original.clone(),
            original_path: original.clone(),
            scope_file: original.clone(),
            local_function: None,
            start_line: None,
            end_line: None,
            snapshot_path: None,
        };
        let debugger = Debugger::new(&root);
        let adapter = scripted_adapter(
            &debugger,
            vec![
                serde_json::json!({"ok":true,"result":2}),
                serde_json::json!({"ok":false,"error":"synthetic ordinary clear failure"}),
                serde_json::json!({"ok":true,"result":2}),
                serde_json::json!({"ok":true,"result":[]}),
            ],
        );
        debugger.set_breakpoint(&original, 2).unwrap();
        debugger.register_sources(vec![mapping.clone()]).unwrap();
        let error = debugger.clear_breakpoint(&original, 2).unwrap_err();
        assert!(error
            .to_string()
            .contains("synthetic ordinary clear failure"));
        assert!(!debugger.inner.breakpoints.lock().unwrap().installed[0].confirmed);
        debugger.register_sources(vec![mapping.clone()]).unwrap();
        debugger.register_sources(vec![mapping]).unwrap();
        {
            let registry = debugger.inner.breakpoints.lock().unwrap();
            assert_eq!(registry.installed.len(), 1);
            assert!(registry.installed[0].confirmed);
        }
        debugger.clear_breakpoint(&original, 2).unwrap();
        let commands = adapter.join().unwrap();
        assert_eq!(commands[2]["command"], "breakpoint");
        assert_eq!(commands[2]["file"], serde_json::json!(original));
        assert_eq!(commands[3]["command"], "clear_breakpoint");
        assert!(debugger
            .inner
            .breakpoints
            .lock()
            .unwrap()
            .installed
            .is_empty());
    }

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
