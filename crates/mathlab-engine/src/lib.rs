//! Persistent GNU Octave sessions. User code runs in Octave's base workspace;
//! transport state lives in a separate function scope.
// SPDX-License-Identifier: GPL-3.0-or-later

#[cfg(all(test, unix))]
mod cancellation_boundary_tests;
mod console;
mod input;
mod protocol;
#[cfg(test)]
mod protocol_startup_tests;
#[cfg(all(test, unix))]
mod value_bridge_recovery_tests;
pub mod value_edit;
#[cfg(all(test, unix))]
mod value_edit_cancel_tests;
#[cfg(all(test, unix))]
mod value_edit_lifetime_tests;
#[cfg(all(test, unix))]
mod value_edit_submission_tests;
#[cfg(all(test, unix))]
mod value_edit_worker_tests;
mod value_page;
use console::{ConsoleCapture, ConsoleHub, ConsoleTransport, MergedPipe, TransportEvent};
pub use console::{ConsoleEvent, ConsoleEventKind, ConsoleSubscription};
use input::InputHub;
pub use input::{
    InputFinished, InputHandle, InputKey, InputMode, InputOutcome, InputPhase, InputRequest,
    MAX_INPUT_BYTES, PendingInput,
};
pub use protocol::{
    BindingToken, BoundVariable, BoundVariableSlice, MutationReceipt, SessionContext,
    WorkerCapabilities, WorkerLimits, WorkspaceSnapshot,
};
pub use value_page::{
    DecimalU64, NumericClass, NumericComponents, PageData, PageSelection, PageWindow,
    SourceStorage, ValuePage, ValuePageError, ValuePageReadMetrics, ValuePageRequest,
    ValuePageResult, ValuePath, ValuePathStep, WirePageData, WireValuePage,
};

use mathlab_debugger::{Debugger, SourceMapping};
use mathlab_language::{MaterializedScript, SourceKind, prepare_script};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json, value::RawValue};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const WORKER: &str = include_str!("../../../octave/mathlab_worker.m");
const VALUE_PAGE_WORKER: &str = include_str!("../../../octave/mathlab_value_page.m");
const VALUE_EDIT_WORKER: &str = include_str!("../../../octave/mathlab_value_edit.m");
const READY_JSON_WORKER: &str = include_str!("../../../octave/mathlab_ready_json.m");
const SCOPE_REGISTRY: &str = r#"## SPDX-License-Identifier: GPL-3.0-or-later
function varargout = mathlab_scope_register (key, handles, configuration = [], original_source = [])
  persistent keys = {};
  persistent scopes = {};
  persistent bridge_enabled = false;
  persistent debugger_enabled = false;
  varargout = {};
  if (nargin == 3)
    if (ischar (configuration) && strcmp (configuration, 'lookup'))
      retained = {};
      index = find (strcmp (keys, key), 1);
      if (! isempty (index)); retained = scopes{index}; endif
      if (nargout > 0); varargout = {retained}; endif
    elseif (strcmp (key, '__debug_configuration__'))
      debugger_enabled = logical (configuration);
    else
      bridge_enabled = logical (configuration);
    endif
    return;
  endif
  index = find (strcmp (keys, key), 1);
  if (isempty (index))
    keys{end + 1} = key;
    scopes{end + 1} = handles;
    index = numel (keys);
  endif
  if (bridge_enabled)
    if (nargin == 4)
      mathlab_scope_bridge (key, scopes{index}, configuration, original_source);
    else
      mathlab_scope_bridge (key, scopes{index});
    endif
  endif
  if (debugger_enabled)
    mathlab_debug_bridge ('refresh_source', key);
  endif
  if (nargout > 0); varargout = {scopes{index}}; endif
endfunction
"#;
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const MAX_CODE_BYTES: usize = 8 * 1024 * 1024;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_VARIABLE_CELLS: usize = 2000;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Protocol(String),
    Worker(EngineError),
    ProcessExited(String),
    Timeout,
    LimitExceeded(String),
    ValuePage(ValuePageError),
    ValueEdit(value_edit::EditError),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Json(error) => write!(formatter, "Invalid engine response: {error}"),
            Self::Protocol(message) | Self::LimitExceeded(message) => formatter.write_str(message),
            Self::Worker(error) => write!(formatter, "{}", error.message),
            Self::ProcessExited(details) => write!(formatter, "Octave process exited. {details}"),
            Self::Timeout => formatter.write_str("Octave response timed out; restart the session."),
            Self::ValuePage(error) => write!(formatter, "{error}"),
            Self::ValueEdit(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<ValuePageError> for Error {
    fn from(error: ValuePageError) -> Self {
        Self::ValuePage(error)
    }
}

impl From<value_edit::EditError> for Error {
    fn from(error: value_edit::EditError) -> Self {
        Self::ValueEdit(error)
    }
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub executable: PathBuf,
    pub working_directory: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub class_name: String,
    pub dimensions: Vec<usize>,
    pub bytes: u64,
    pub preview: String,
    /// False when safe inspection cannot establish an object's shape.
    #[serde(default = "metadata_known")]
    pub dimensions_known: bool,
    /// False when byte accounting would require inspecting user objects.
    #[serde(default = "metadata_known")]
    pub bytes_known: bool,
}

fn metadata_known() -> bool {
    true
}

/// Bounded, row-major display cells from the first two dimensions of a variable.
/// Starts are one-based; rows/columns describe the clipped region actually returned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableSlice {
    pub name: String,
    pub class_name: String,
    pub dimensions: Vec<usize>,
    pub row_start: usize,
    pub column_start: usize,
    pub rows: usize,
    pub columns: usize,
    pub cells: Vec<Vec<String>>,
    pub editable: bool,
}

/// A source snapshot staged for debugging without executing user code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedDebugSource {
    pub original_path: PathBuf,
    pub script_path: PathBuf,
    pub prepared: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    pub file: String,
    pub name: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineError {
    pub message: String,
    pub identifier: String,
    pub stack: Vec<StackFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Figure {
    /// PNG paths remain valid until this Engine is dropped.
    pub path: PathBuf,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub description: String,
    pub loaded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub output: String,
    /// Aggregate output was clipped at the session transport's byte limit.
    #[serde(default)]
    pub output_truncated: bool,
    #[serde(default)]
    pub output_dropped_bytes: u64,
    /// Invalid or incomplete UTF-8 was replaced in this display string. Console
    /// subscriptions retain the original byte chunks within their own limits.
    #[serde(default)]
    pub output_decoding_loss: bool,
    #[serde(default)]
    pub console_generation: u64,
    /// The stream subscriber has seen all request bytes once it reaches this
    /// sequence, even if its bounded queue reports a delivery gap.
    #[serde(default)]
    pub console_end_sequence: u64,
    #[serde(deserialize_with = "deserialize_execution_error")]
    pub error: Option<EngineError>,
    pub workspace: Vec<Variable>,
    pub figures: Vec<Figure>,
    pub working_directory: PathBuf,
    pub duration_ms: f64,
    pub figure_warnings: Vec<String>,
}

// Octave represents an absent struct as [] rather than JSON null.
fn deserialize_execution_error<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<EngineError>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    if value.is_null() || value.as_array().is_some_and(Vec::is_empty) {
        Ok(None)
    } else {
        serde_json::from_value(value)
            .map(Some)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Deserialize)]
struct Response {
    id: u64,
    ok: bool,
    #[serde(default)]
    result: Box<RawValue>,
    #[serde(default)]
    error: Option<EngineError>,
}

struct PreparedSource {
    original_path: PathBuf,
    original_source: String,
    materialized: MaterializedScript,
    local_functions: Vec<LocalFunctionLocation>,
    factory_path: Option<PathBuf>,
    snapshot_path: PathBuf,
    source_kind: SourceKind,
}

struct LocalFunctionLocation {
    name: String,
    start_line: usize,
    end_line: usize,
}

/// Can interrupt execution from a UI thread while Engine::execute blocks elsewhere.
/// Unix sends SIGINT and retains the base workspace. Unresponsive foreign routines
/// may require terminate(); non-Unix interrupt falls back to hard termination.
/// Existing handles continue to target the current process after restart.
#[derive(Clone)]
pub struct CancellationHandle {
    child: Arc<Mutex<Option<Child>>>,
    active: Arc<AtomicBool>,
    interrupt_pending: Arc<AtomicBool>,
    interrupt_sent: Arc<AtomicBool>,
    active_request: Arc<AtomicU64>,
    worker_generation: Arc<AtomicU64>,
    cancel_directory: PathBuf,
}

impl CancellationHandle {
    pub fn interrupt(&self) -> Result<()> {
        let mut guard = self
            .child
            .lock()
            .map_err(|_| Error::Protocol("Engine process lock poisoned".into()))?;
        self.interrupt_locked(&mut guard)
    }

    fn interrupt_locked(&self, guard: &mut Option<Child>) -> Result<()> {
        if !self.active.load(Ordering::Acquire) || self.interrupt_pending.load(Ordering::Acquire) {
            return Ok(());
        }
        if let Some(child) = guard.as_mut()
            && child.try_wait()?.is_none()
        {
            // The marker survives a signal delivered before the idle worker
            // consumes its mailbox. Re-entry must not run the canceled source.
            let marker = self.cancel_directory.join(format!(
                "cancel-{}-{}",
                self.worker_generation.load(Ordering::Acquire),
                self.active_request.load(Ordering::Acquire)
            ));
            fs::write(marker, [])?;
            self.interrupt_pending.store(true, Ordering::Release);
            #[cfg(unix)]
            {
                let status = unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGINT) };
                if status != 0 {
                    self.interrupt_pending.store(false, Ordering::Release);
                    return Err(Error::Io(io::Error::last_os_error()));
                }
                self.interrupt_sent.store(true, Ordering::Release);
            }
            #[cfg(not(unix))]
            child.kill()?;
        }
        Ok(())
    }

    /// Force termination, discarding the workspace. Restart is required afterwards.
    /// Use for deadlines, shutdown, or foreign code that does not respond to SIGINT.
    pub fn terminate(&self) -> Result<()> {
        let mut guard = self
            .child
            .lock()
            .map_err(|_| Error::Protocol("Engine process lock poisoned".into()))?;
        if let Some(child) = guard.as_mut()
            && child.try_wait()?.is_none()
        {
            child.kill()?;
        }
        Ok(())
    }
}

pub struct Engine {
    config: EngineConfig,
    session: TempDir,
    child: Arc<Mutex<Option<Child>>>,
    active: Arc<AtomicBool>,
    interrupt_pending: Arc<AtomicBool>,
    interrupt_sent: Arc<AtomicBool>,
    active_request: Arc<AtomicU64>,
    worker_generation: Arc<AtomicU64>,
    transport: Option<ConsoleTransport>,
    console: ConsoleHub,
    input: InputHub,
    input_bridge: Option<PathBuf>,
    user_execution_started: bool,
    generation: u64,
    invocation: u64,
    next_id: u64,
    response_prefix: String,
    console_helper: PathBuf,
    version: String,
    capabilities: Option<WorkerCapabilities>,
    prepared_sources: Vec<PreparedSource>,
    active_sources: BTreeMap<PathBuf, PathBuf>,
    next_preparation_id: u64,
    debug_bridge: Option<PathBuf>,
    debugger: Option<Debugger>,
    debug_session: Option<TempDir>,
    scope_bridge: Option<PathBuf>,
    value_bridge: Option<PathBuf>,
    edit_history_uncertain: bool,
}

impl Engine {
    pub fn start(config: EngineConfig) -> Result<Self> {
        Self::start_with_worker(config, WORKER)
    }

    fn start_with_worker(config: EngineConfig, worker: &str) -> Result<Self> {
        if cfg!(windows) {
            return Err(Error::Protocol(
                "The ordered console worker launcher currently requires a Unix shell. Windows session launch is not yet supported.".into(),
            ));
        }
        if !config.working_directory.is_dir() {
            return Err(Error::Protocol(format!(
                "Working directory does not exist: {}",
                config.working_directory.display()
            )));
        }
        let session = tempfile::Builder::new()
            .prefix("mathlab-session-")
            .tempdir()?;
        let worker =
            format!("{worker}\n{VALUE_PAGE_WORKER}\n{VALUE_EDIT_WORKER}\n{READY_JSON_WORKER}");
        fs::write(session.path().join("mathlab_worker.m"), worker)?;
        fs::create_dir(session.path().join("registry"))?;
        fs::write(
            session.path().join("registry/mathlab_scope_register.m"),
            SCOPE_REGISTRY,
        )?;
        fs::create_dir_all(session.path().join("cache"))?;
        let console_helper = console::write_fence_helper(session.path())?;
        let input = InputHub::new(session.path())?;
        let mut engine = Self {
            config,
            session,
            child: Arc::new(Mutex::new(None)),
            active: Arc::new(AtomicBool::new(false)),
            interrupt_pending: Arc::new(AtomicBool::new(false)),
            interrupt_sent: Arc::new(AtomicBool::new(false)),
            active_request: Arc::new(AtomicU64::new(0)),
            worker_generation: Arc::new(AtomicU64::new(0)),
            transport: None,
            console: ConsoleHub::new(),
            input,
            input_bridge: None,
            user_execution_started: false,
            generation: 0,
            invocation: 0,
            next_id: 1,
            response_prefix: String::new(),
            console_helper,
            version: String::new(),
            capabilities: None,
            prepared_sources: Vec::new(),
            active_sources: BTreeMap::new(),
            next_preparation_id: 1,
            debug_bridge: None,
            debugger: None,
            debug_session: None,
            scope_bridge: None,
            value_bridge: None,
            edit_history_uncertain: false,
        };
        engine.spawn()?;
        if let Some(scope_bridge) = std::env::var_os("MATHLAB_SCOPE_BRIDGE") {
            engine.enable_scope_bridge(Path::new(&scope_bridge))?;
        }
        if let Some(input_bridge) = std::env::var_os("MATHLAB_INPUT_BRIDGE") {
            engine.enable_input_adapter(Path::new(&input_bridge))?;
        }
        if let Some(value_bridge) = std::env::var_os("MATHLAB_VALUE_BRIDGE") {
            engine.enable_value_bridge(Path::new(&value_bridge))?;
        }
        Ok(engine)
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    /// Negotiated ordinary-worker capabilities. Native adapters have separate
    /// version/ABI checks and are not implied by this handshake.
    pub fn capabilities(&self) -> &WorkerCapabilities {
        self.capabilities
            .as_ref()
            .expect("Engine startup validated capabilities")
    }

    pub fn require_capability(&self, name: &str) -> Result<()> {
        self.capabilities().require(name)
    }

    pub fn working_directory(&self) -> &Path {
        &self.config.working_directory
    }

    /// Exact copied transport-worker identity for UI frame filtering. User source
    /// with the same basename must remain navigable.
    pub fn worker_source_path(&self) -> PathBuf {
        let path = self.session.path().join("mathlab_worker.m");
        fs::canonicalize(&path).unwrap_or(path)
    }

    pub fn cancellation_handle(&self) -> CancellationHandle {
        CancellationHandle {
            child: Arc::clone(&self.child),
            active: Arc::clone(&self.active),
            interrupt_pending: Arc::clone(&self.interrupt_pending),
            interrupt_sent: Arc::clone(&self.interrupt_sent),
            active_request: Arc::clone(&self.active_request),
            worker_generation: Arc::clone(&self.worker_generation),
            cancel_directory: self.session.path().to_owned(),
        }
    }

    /// Receive live ordered console events independently of the thread executing
    /// requests. The subscription remains valid across restart; generation IDs
    /// distinguish bytes from the previous process.
    pub fn subscribe_console(&self) -> ConsoleSubscription {
        self.console.subscribe()
    }

    pub fn console_generation(&self) -> u64 {
        self.generation
    }

    pub fn process_id(&self) -> Option<u32> {
        self.child.lock().ok()?.as_ref().map(Child::id)
    }

    /// Whether this engine still owns an open transport. A false result requires
    /// restart; true is not a process-health, idle or request-success guarantee.
    /// This preserves lifecycle status when an ambiguous edit wraps its original
    /// transport error as `UncertainCommit` without exposing string parsing.
    pub fn is_transport_open(&self) -> bool {
        self.transport.is_some()
    }

    /// Enable authentic, independent input replies before running user source.
    /// Previously captured @input handles retain their old builtin, so late
    /// activation is rejected until the numerical process has been restarted.
    pub fn enable_input_adapter(&mut self, bridge_path: &Path) -> Result<InputHandle> {
        let bridge_path = fs::canonicalize(bridge_path)?;
        if !bridge_path.is_file()
            || bridge_path
                .extension()
                .is_none_or(|extension| extension != "oct")
        {
            return Err(Error::Protocol(
                "Input adapter must be a compiled .oct file.".into(),
            ));
        }
        if self.input_bridge.as_ref() == Some(&bridge_path) {
            return Ok(self.input.handle());
        }
        if self.input_bridge.is_some() {
            return Err(Error::Protocol(
                "An input adapter is already loaded. Restart MathLab to change the adapter.".into(),
            ));
        }
        if self.user_execution_started {
            return Err(Error::Protocol("Enable the input adapter before the first user execution, or restart the session before enabling it. Previously captured @input handles cannot be redirected safely.".into()));
        }
        self.install_input_adapter(&bridge_path)?;
        self.input_bridge = Some(bridge_path);
        Ok(self.input.handle())
    }

    pub fn input_handle(&self) -> Option<InputHandle> {
        self.input_bridge.as_ref().map(|_| self.input.handle())
    }

    pub fn input_bridge_path(&self) -> Option<&Path> {
        self.input_bridge.as_deref()
    }

    fn install_input_adapter(&mut self, bridge_path: &Path) -> Result<()> {
        self.request(
            json!({"command":"enable_input_bridge","bridge_path":bridge_path}),
            Some(COMMAND_TIMEOUT),
        )?;
        self.input.set_enabled(true)
    }

    /// Install an optional debugger adapter compiled for this Octave runtime ABI.
    /// A clone can inspect/control pauses while execute blocks on another thread.
    pub fn enable_debugger(&mut self, bridge_path: &Path) -> Result<Debugger> {
        let bridge_path = fs::canonicalize(bridge_path)?;
        if !bridge_path.is_file()
            || bridge_path
                .extension()
                .is_none_or(|extension| extension != "oct")
        {
            return Err(Error::Protocol(
                "Debugger adapter must be a compiled .oct file.".into(),
            ));
        }
        if self.debug_bridge.as_ref() == Some(&bridge_path)
            && let Some(debugger) = self.debugger.as_ref()
        {
            return Ok(debugger.clone());
        }
        if self.debug_bridge.is_some() && self.debugger.is_some() {
            return Err(Error::Protocol(
                "A debugger adapter is already loaded. Restart MathLab to change the adapter."
                    .into(),
            ));
        }
        self.install_debugger(&bridge_path)?;
        self.debug_bridge = Some(bridge_path);
        Ok(self
            .debugger
            .as_ref()
            .expect("installed debugger handle")
            .clone())
    }

    pub fn debugger_handle(&self) -> Option<Debugger> {
        self.debugger.clone()
    }

    /// Enable optional Octave per-file bindings for retained script-local scopes.
    /// The adapter must implement protocol mathlab-scope version 2 for this ABI.
    pub fn enable_scope_bridge(&mut self, bridge_path: &Path) -> Result<()> {
        let bridge_path = fs::canonicalize(bridge_path)?;
        if !bridge_path.is_file()
            || bridge_path
                .extension()
                .is_none_or(|extension| extension != "oct")
        {
            return Err(Error::Protocol(
                "Scope adapter must be a compiled .oct file.".into(),
            ));
        }
        if self.scope_bridge.as_ref() == Some(&bridge_path) {
            return Ok(());
        }
        if self.scope_bridge.is_some() {
            return Err(Error::Protocol(
                "A scope adapter is already loaded. Restart MathLab to change the adapter.".into(),
            ));
        }
        self.install_scope_bridge(&bridge_path)?;
        self.scope_bridge = Some(bridge_path);
        Ok(())
    }

    pub fn scope_bridge_path(&self) -> Option<&Path> {
        self.scope_bridge.as_deref()
    }

    fn install_scope_bridge(&mut self, bridge_path: &Path) -> Result<()> {
        self.request(
            json!({"command":"enable_scope_bridge","bridge_path":bridge_path}),
            Some(COMMAND_TIMEOUT),
        )?;
        Ok(())
    }

    /// Enable optional exact dense-complex component access for this Octave ABI.
    /// The adapter is a narrow native helper, not a Rust numerical evaluator.
    pub fn enable_value_bridge(&mut self, bridge_path: &Path) -> Result<()> {
        let bridge_path = fs::canonicalize(bridge_path)?;
        if !bridge_path.is_file()
            || bridge_path
                .extension()
                .is_none_or(|extension| extension != "oct")
        {
            return Err(Error::Protocol(
                "Value adapter must be a compiled .oct file.".into(),
            ));
        }
        if self.value_bridge.as_ref() == Some(&bridge_path) {
            // A failed restart can retain the configured path after a fresh
            // worker advertised no adapter. Configuration alone is not proof
            // that this generation has successfully loaded the helper.
            return self.require_capability("dense_complex_native_components.v1");
        }
        if self.value_bridge.is_some() {
            return Err(Error::Protocol(
                "A value adapter is already loaded. Start a new engine session to change the adapter."
                    .into(),
            ));
        }
        self.install_value_bridge(&bridge_path)?;
        self.value_bridge = Some(bridge_path);
        Ok(())
    }

    pub fn value_bridge_path(&self) -> Option<&Path> {
        self.value_bridge.as_deref()
    }

    fn retire_value_bridge_capability(&mut self) {
        if let Some(capabilities) = self.capabilities.as_mut() {
            capabilities
                .capabilities
                .retain(|capability| capability != "dense_complex_native_components.v1");
        }
    }

    fn install_value_bridge(&mut self, bridge_path: &Path) -> Result<()> {
        let (ready, _) = self.request(
            json!({"command":"enable_value_bridge","bridge_path":bridge_path}),
            Some(COMMAND_TIMEOUT),
        )?;
        // Enabling the helper changes the ordinary worker's optional capability
        // set. Validate the complete current context/readiness envelope again.
        self.accept_ready(&ready)?;
        self.require_capability("dense_complex_native_components.v1")
    }

    fn install_debugger(&mut self, bridge_path: &Path) -> Result<()> {
        let directory = tempfile::Builder::new()
            .prefix("debugger-")
            .tempdir_in(self.session.path())?;
        self.request(
            json!({"command":"enable_debugger","bridge_path":bridge_path,
                            "debug_directory":directory.path()}),
            Some(COMMAND_TIMEOUT),
        )?;
        self.debugger = Some(Debugger::new(directory.path()));
        self.debug_session = Some(directory);
        for index in 0..self.prepared_sources.len() {
            self.register_debug_source(index)?;
        }
        for (original, current) in &self.active_sources {
            if original == current {
                self.register_direct_debug_source(original)?;
            } else if let Some(index) = self
                .prepared_sources
                .iter()
                .position(|source| &source.materialized.script_path == current)
            {
                self.register_debug_source(index)?;
            }
        }
        Ok(())
    }

    pub fn execute(&mut self, code: &str) -> Result<ExecutionResult> {
        self.execute_inner(code, None)
    }

    /// Run an existing .m script in the base workspace, preserving source stack frames.
    /// Path arguments travel as data instead of being interpolated into source code.
    pub fn run_file(&mut self, path: &Path) -> Result<ExecutionResult> {
        self.run_file_inner(path, None)
    }

    /// Run a .m script with an automation deadline; timeout terminates the session.
    pub fn run_file_with_timeout(
        &mut self,
        path: &Path,
        timeout: Duration,
    ) -> Result<ExecutionResult> {
        self.run_file_inner(path, Some(timeout))
    }

    fn run_file_inner(
        &mut self,
        path: &Path,
        timeout: Option<Duration>,
    ) -> Result<ExecutionResult> {
        let path = self.resolve_session_path(path)?;
        let source = fs::read_to_string(&path)?;
        let request = self.prepare_source_request(&source, &path, false)?;
        self.execution_request(request, timeout)
    }

    /// Execute an unsaved .m editor buffer with its intended original file path.
    /// Forward script-local definitions are prepared, and error frames refer to
    /// the original editor path and line numbers. No editor file is overwritten.
    pub fn run_buffer(&mut self, source: &str, path: &Path) -> Result<ExecutionResult> {
        let path = self.resolve_buffer_path(source, path)?;
        let request = self.prepare_source_request(source, &path, true)?;
        self.execution_request(request, None)
    }

    /// Stage saved source and register original-file debugger mappings. No user
    /// statements run. Debugged scripts use immutable source generations;
    /// ordinary function/class files retain their direct file identities.
    pub fn prepare_debug_file(&mut self, path: &Path) -> Result<PreparedDebugSource> {
        let original_path = self.resolve_session_path(path)?;
        let source = fs::read_to_string(&original_path)?;
        let request = self.prepare_source_request(&source, &original_path, false)?;
        debug_source_descriptor(original_path, &request)
    }

    /// Stage the current unsaved editor text before setting original-file
    /// breakpoints. The source snapshot is immutable and reused by run_buffer.
    pub fn prepare_debug_buffer(
        &mut self,
        source: &str,
        path: &Path,
    ) -> Result<PreparedDebugSource> {
        let original_path = self.resolve_buffer_path(source, path)?;
        let prepared = prepare_script(&original_path, source)
            .map_err(|error| Error::Protocol(error.to_string()))?;
        let kind = prepared
            .source_kind()
            .map_err(|error| Error::Protocol(error.to_string()))?;
        if kind != SourceKind::Script {
            return Err(Error::Protocol(
                "Save function and class files before setting debug breakpoints.".into(),
            ));
        }
        let request = self.prepare_source_request(source, &original_path, true)?;
        debug_source_descriptor(original_path, &request)
    }

    fn resolve_buffer_path(&self, source: &str, path: &Path) -> Result<PathBuf> {
        if source.len() > MAX_CODE_BYTES {
            return Err(Error::LimitExceeded(
                "Editor source exceeds the 8 MiB execution limit.".into(),
            ));
        }
        let candidate = if path.is_absolute() {
            path.to_owned()
        } else {
            self.config.working_directory.join(path)
        };
        let path = if candidate.exists() {
            fs::canonicalize(candidate)?
        } else {
            let filename = candidate
                .file_name()
                .ok_or_else(|| Error::Protocol("Editor path has no filename.".into()))?;
            let parent = candidate
                .parent()
                .ok_or_else(|| Error::Protocol("Editor path has no directory.".into()))?;
            fs::canonicalize(parent)?.join(filename)
        };
        Ok(path)
    }

    fn prepare_source_request(&mut self, source: &str, path: &Path, buffer: bool) -> Result<Value> {
        if let Some(cached) = self.prepared_sources.iter().find(|cached| {
            cached.original_path == path
                && cached.original_source == source
                && (buffer
                    || !cached.local_functions.is_empty()
                    || (self.debugger.is_some() && cached.source_kind == SourceKind::Script))
        }) {
            let request = prepared_request(cached, self.debugger.is_some());
            let current_scope = cached.materialized.script_path.clone();
            let index = self
                .prepared_sources
                .iter()
                .position(|candidate| {
                    candidate.materialized.script_path == cached.materialized.script_path
                })
                .expect("cached source index");
            self.active_sources.insert(path.to_owned(), current_scope);
            self.register_debug_source(index)?;
            return Ok(request);
        }
        let prepared = prepare_script(path, source).map_err(|error| {
            Error::Worker(EngineError {
                message: error.message,
                identifier: "MathLab:SourcePreparation".into(),
                stack: vec![StackFrame {
                    file: path.to_string_lossy().into_owned(),
                    name: path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    line: error.line,
                }],
            })
        })?;
        // Octave's persistent source() can retain a script AST loaded while
        // setting an ordinary breakpoint, even after the saved bytes change.
        // Bind debugged script execution to these exact bytes instead of the
        // mutable filename. Functions/classes keep their normal file identity.
        let debug_script = self.debugger.is_some()
            && prepared
                .source_kind()
                .map_err(|error| Error::Protocol(error.to_string()))?
                == SourceKind::Script;
        if !buffer && !prepared.is_transformed() && !debug_script {
            self.active_sources.insert(path.to_owned(), path.to_owned());
            self.register_direct_debug_source(path)?;
            return Ok(json!({"command":"run_file","path":path}));
        }
        let directory = self
            .session
            .path()
            .join("prepared")
            .join(format!("generation-{}", self.next_preparation_id));
        self.next_preparation_id += 1;
        let materialized = prepared.write_to(&directory)?;
        let snapshot_path = directory.join("original.mlsnapshot");
        fs::write(&snapshot_path, source)?;
        let snapshot_path = fs::canonicalize(snapshot_path)?;
        // A transport-only factory gives the portable backend private handles
        // without injecting calls or temporary variables into the user's script.
        // Native pre-entry preparation bypasses this factory entirely.
        let factory_path = if prepared.is_transformed() {
            let stem = materialized
                .script_path
                .file_stem()
                .expect("generated script stem")
                .to_string_lossy();
            let factory_name = format!("{stem}_factory");
            validate_identifier(&factory_name)?;
            let handles = prepared
                .local_functions
                .iter()
                .map(|function| format!("@{}", function.name))
                .collect::<Vec<_>>()
                .join(", ");
            let factory_path = directory.join(format!("{factory_name}.m"));
            fs::write(
                &factory_path,
                format!(
                    "function handles = {factory_name} ()\n  handles = {{{handles}}};\nendfunction\n"
                ),
            )?;
            Some(fs::canonicalize(factory_path)?)
        } else {
            None
        };
        self.prepared_sources.push(PreparedSource {
            original_path: path.to_owned(),
            original_source: source.to_owned(),
            materialized,
            factory_path,
            snapshot_path,
            source_kind: prepared
                .source_kind()
                .map_err(|error| Error::Protocol(error.to_string()))?,
            local_functions: prepared
                .local_functions
                .iter()
                .map(|function| LocalFunctionLocation {
                    name: function.name.clone(),
                    start_line: function.span.start_line,
                    end_line: function.span.end_line,
                })
                .collect(),
        });
        self.active_sources.insert(
            path.to_owned(),
            self.prepared_sources
                .last()
                .expect("prepared source")
                .materialized
                .script_path
                .clone(),
        );
        self.register_debug_source(self.prepared_sources.len() - 1)?;
        Ok(prepared_request(
            self.prepared_sources.last().expect("prepared source"),
            self.debugger.is_some(),
        ))
    }

    fn register_debug_source(&self, index: usize) -> Result<()> {
        let Some(debugger) = self.debugger.as_ref() else {
            return Ok(());
        };
        let source = &self.prepared_sources[index];
        let mappings = source
            .materialized
            .source_mapping
            .iter()
            .map(|mapping| {
                let location = mapping.local_function.as_ref().and_then(|name| {
                    source
                        .local_functions
                        .iter()
                        .find(|location| &location.name == name)
                });
                SourceMapping {
                    generated_path: mapping.generated_path.clone(),
                    original_path: mapping.original_path.clone(),
                    scope_file: source.materialized.script_path.clone(),
                    snapshot_path: Some(source.snapshot_path.clone()),
                    local_function: mapping.local_function.clone(),
                    start_line: location.map(|location| location.start_line),
                    end_line: location.map(|location| location.end_line),
                }
            })
            .collect();
        debugger
            .register_sources(mappings)
            .map_err(|error| Error::Protocol(error.to_string()))
    }

    fn register_direct_debug_source(&self, path: &Path) -> Result<()> {
        let Some(debugger) = self.debugger.as_ref() else {
            return Ok(());
        };
        debugger
            .register_sources(vec![SourceMapping {
                generated_path: path.to_owned(),
                original_path: path.to_owned(),
                scope_file: path.to_owned(),
                snapshot_path: None,
                local_function: None,
                start_line: None,
                end_line: None,
            }])
            .map_err(|error| Error::Protocol(error.to_string()))
    }

    /// Optional execution deadline for automation. Timeout terminates the process.
    pub fn execute_with_timeout(
        &mut self,
        code: &str,
        timeout: Duration,
    ) -> Result<ExecutionResult> {
        self.execute_inner(code, Some(timeout))
    }

    fn execute_inner(&mut self, code: &str, timeout: Option<Duration>) -> Result<ExecutionResult> {
        if code.len() > MAX_CODE_BYTES {
            return Err(Error::LimitExceeded(
                "Source exceeds the 8 MiB execution limit; run it from a .m file.".into(),
            ));
        }
        self.execution_request(json!({"command": "execute", "code": code}), timeout)
    }

    fn execution_request(
        &mut self,
        request: Value,
        timeout: Option<Duration>,
    ) -> Result<ExecutionResult> {
        self.user_execution_started = true;
        let (value, capture) = self.request(request, timeout)?;
        self.execution_result(value, capture)
    }

    fn execution_result(
        &mut self,
        value: Value,
        capture: ConsoleCapture,
    ) -> Result<ExecutionResult> {
        let mut result: ExecutionResult = serde_json::from_value(value)?;
        result.output = capture.output;
        result.output_truncated = capture.truncated;
        result.output_dropped_bytes = capture.dropped_bytes;
        result.output_decoding_loss = capture.decoding_loss;
        result.console_generation = self.generation;
        result.console_end_sequence = capture.end_sequence;
        if let Some(error) = result.error.as_mut() {
            let worker_path = fs::canonicalize(self.session.path().join("mathlab_worker.m"))?;
            error.stack.retain(|frame| {
                let candidate = PathBuf::from(&frame.file);
                let candidate = fs::canonicalize(&candidate).unwrap_or(candidate);
                candidate != worker_path
                    && !self
                        .prepared_sources
                        .iter()
                        .any(|source| source.factory_path.as_ref() == Some(&candidate))
            });
            for mapping in self
                .prepared_sources
                .iter()
                .flat_map(|cached| &cached.materialized.source_mapping)
            {
                let generated = mapping.generated_path.to_string_lossy();
                if error.message.contains(generated.as_ref()) {
                    // Octave parse errors sometimes provide only the worker frame;
                    // recover the source location from its native diagnostic text.
                    if error.stack.is_empty()
                        && let Some(line) = diagnostic_line(&error.message)
                    {
                        error.stack.push(StackFrame {
                            file: generated.into_owned(),
                            name: mapping.local_function.clone().unwrap_or_else(|| {
                                mapping
                                    .generated_path
                                    .file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned()
                            }),
                            line,
                        });
                    }
                    error.message = error.message.replace(
                        mapping.generated_path.to_string_lossy().as_ref(),
                        mapping.original_path.to_string_lossy().as_ref(),
                    );
                }
            }
            for frame in &mut error.stack {
                let candidate = PathBuf::from(&frame.file);
                let candidate = fs::canonicalize(&candidate).unwrap_or(candidate);
                if let Some(mapping) = self
                    .prepared_sources
                    .iter()
                    .flat_map(|cached| &cached.materialized.source_mapping)
                    .find(|mapping| mapping.generated_path == candidate)
                {
                    frame.file = mapping.original_path.to_string_lossy().into_owned();
                    let stem = mapping
                        .original_path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy();
                    if let Some(local) = &mapping.local_function {
                        frame.name = display_local_frame(&frame.name, local, stem.as_ref());
                    } else {
                        let generated_stem = mapping
                            .generated_path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy();
                        frame.name = display_script_frame(
                            &frame.name,
                            generated_stem.as_ref(),
                            stem.as_ref(),
                        );
                    }
                } else {
                    // Native original-source reflection keeps user error files
                    // original. Unlike paused native frames, a caught .m stack
                    // contains no immutable generation key. Qualify only when
                    // retained source snapshots agree on the enclosing function.
                    if let Some(source) = self.prepared_sources.iter().find(|source| {
                        source.original_path == candidate
                            && !source.local_functions.iter().any(|local| {
                                (local.start_line..=local.end_line).contains(&frame.line)
                            })
                            && source
                                .materialized
                                .script_path
                                .file_stem()
                                .is_some_and(|stem| {
                                    frame.name.contains(stem.to_string_lossy().as_ref())
                                })
                    }) {
                        let generated_stem = source
                            .materialized
                            .script_path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy();
                        let stem = candidate.file_stem().unwrap_or_default().to_string_lossy();
                        frame.name = display_script_frame(
                            &frame.name,
                            generated_stem.as_ref(),
                            stem.as_ref(),
                        );
                        continue;
                    }
                    let mut enclosing = std::collections::BTreeSet::new();
                    for source in &self.prepared_sources {
                        if source.original_path != candidate {
                            continue;
                        }
                        for local in &source.local_functions {
                            if (local.start_line..=local.end_line).contains(&frame.line) {
                                enclosing.insert(local.name.as_str());
                            }
                        }
                    }
                    let raw_outer = frame.name.split('>').next().unwrap_or(&frame.name);
                    let outer = if enclosing.contains(raw_outer) {
                        Some(raw_outer)
                    } else if enclosing.len() == 1 {
                        enclosing.first().copied()
                    } else {
                        None
                    };
                    if let Some(outer) = outer {
                        let stem = candidate.file_stem().unwrap_or_default().to_string_lossy();
                        frame.name = display_local_frame(&frame.name, outer, stem.as_ref());
                    }
                }
            }
        }
        self.config.working_directory = result.working_directory.clone();
        Ok(result)
    }

    pub fn workspace(&mut self) -> Result<Vec<Variable>> {
        let (value, _) = self.request(json!({"command": "workspace"}), Some(COMMAND_TIMEOUT))?;
        Ok(serde_json::from_value(value)?)
    }

    /// Effect-free metadata snapshot for the idle inspector. Tokens survive
    /// declared pure reads and are conservatively revoked before all other jobs,
    /// including jobs which fail or are interrupted after partial mutations.
    pub fn workspace_snapshot(&mut self) -> Result<WorkspaceSnapshot> {
        self.workspace_snapshot_command("workspace_snapshot")
    }

    fn workspace_snapshot_command(&mut self, command: &str) -> Result<WorkspaceSnapshot> {
        self.require_capability("binding_snapshot.v1")?;
        let (value, _) = self.request(json!({"command":command}), Some(COMMAND_TIMEOUT))?;
        self.parse_workspace_snapshot(value)
    }

    fn parse_workspace_snapshot(&self, value: Value) -> Result<WorkspaceSnapshot> {
        let snapshot: WorkspaceSnapshot = serde_json::from_value(value)?;
        self.validate_context(&snapshot.context)?;
        for entry in &snapshot.variables {
            validate_identifier(&entry.variable.name)?;
            if entry.token.name != entry.variable.name || entry.token.context != snapshot.context {
                return Err(Error::Protocol(
                    "Worker returned an inconsistent binding snapshot".into(),
                ));
            }
        }
        Ok(snapshot)
    }

    /// A submitted edit with an ambiguous outcome disables subsequent edits and
    /// history operations until explicit reset or restart. Reads remain usable.
    pub fn has_value_edit_uncertainty(&self) -> bool {
        self.edit_history_uncertain
    }

    /// Explicitly retire all edit history and return fresh binding tokens.
    /// Numerical values are preserved. This does not retry the uncertain edit.
    pub fn reset_value_edit_history(&mut self) -> Result<WorkspaceSnapshot> {
        self.require_capability(value_edit::CAP_ATOMIC_RANGE_EDIT)?;
        self.require_capability("binding_snapshot.v1")?;
        Self::validate_response_limit(MAX_RESPONSE_BYTES)?;
        // Local validation/mailbox failures cannot have reset the worker. In
        // particular they must preserve an already latched ambiguous history.
        let id = self.send_request(json!({"command":"reset_value_edit_history"}), true)?;
        let result = self
            .finish_request(id, Some(COMMAND_TIMEOUT))
            .and_then(|(value, _)| self.parse_workspace_snapshot(value));
        match result {
            Ok(snapshot) => {
                self.edit_history_uncertain = false;
                Ok(snapshot)
            }
            Err(error) => {
                self.edit_history_uncertain = true;
                Err(value_edit::EditError::UncertainCommit(error.to_string()).into())
            }
        }
    }

    /// Commit one same-class rectangle under the original full binding token.
    /// All cells are validated before one root assignment. No automatic retry.
    pub fn write_value_range_checked(
        &mut self,
        request: &value_edit::RangeEditRequest,
    ) -> Result<value_edit::RangeEditReceipt> {
        self.ensure_value_edit_certain()?;
        request.validate_decode()?;
        let mut payload = serde_json::to_value(request)?;
        payload["command"] = json!("write_value_range_checked");
        let expectation = value_edit::ReceiptExpectation {
            operation: value_edit::EditOperation::Write,
            previous: request.path.root.clone(),
            transaction_id: None,
            history_epoch: None,
        };
        self.submit_value_edit(payload, expectation, value_edit::MAX_EDIT_REQUEST_BYTES)
    }

    pub fn undo_value_edit_checked(
        &mut self,
        request: &value_edit::HistoryRequest,
    ) -> Result<value_edit::RangeEditReceipt> {
        self.restore_value_edit(request, value_edit::EditOperation::Undo)
    }

    pub fn redo_value_edit_checked(
        &mut self,
        request: &value_edit::HistoryRequest,
    ) -> Result<value_edit::RangeEditReceipt> {
        self.restore_value_edit(request, value_edit::EditOperation::Redo)
    }

    fn restore_value_edit(
        &mut self,
        request: &value_edit::HistoryRequest,
        operation: value_edit::EditOperation,
    ) -> Result<value_edit::RangeEditReceipt> {
        self.ensure_value_edit_certain()?;
        request.validate()?;
        let mut payload = serde_json::to_value(request)?;
        payload["command"] = json!(match operation {
            value_edit::EditOperation::Undo => "undo_value_edit_checked",
            value_edit::EditOperation::Redo => "redo_value_edit_checked",
            value_edit::EditOperation::Write => unreachable!("private history operation"),
        });
        self.submit_value_edit(
            payload,
            value_edit::ReceiptExpectation {
                operation,
                previous: request.expected.clone(),
                transaction_id: Some(request.transaction_id),
                history_epoch: Some(request.history_epoch),
            },
            value_edit::MAX_HISTORY_REQUEST_BYTES,
        )
    }

    fn ensure_value_edit_certain(&self) -> Result<()> {
        self.require_capability(value_edit::CAP_ATOMIC_RANGE_EDIT)?;
        if self.edit_history_uncertain {
            return Err(value_edit::EditError::UncertainCommit(
                "explicitly reset edit history or restart before another write/undo/redo".into(),
            )
            .into());
        }
        Ok(())
    }

    fn submit_value_edit(
        &mut self,
        payload: Value,
        expectation: value_edit::ReceiptExpectation,
        request_budget: usize,
    ) -> Result<value_edit::RangeEditReceipt> {
        // Public DTO validators bounded every collection before this encoding.
        value_edit::bounded(
            "edit request bytes",
            serde_json::to_vec(&payload)?.len(),
            request_budget,
        )?;
        let response_limit = value_edit::MAX_EDIT_RECEIPT_BYTES + 4096;
        Self::validate_response_limit(response_limit)?;
        // Publication is the uncertainty boundary. An error returned by
        // send_request occurs before the atomic mailbox rename succeeded.
        let id = self.send_request(payload, true)?;
        let (raw, _) = self
            .finish_request_raw(id, Some(COMMAND_TIMEOUT), response_limit)
            .map_err(|error| self.value_edit_failure(error))?;
        let mut settled = expectation.previous.context.clone();
        settled.worker_id.clone_from(&self.response_prefix);
        settled.generation = self.generation;
        settled.invocation = self.invocation;
        settled.revision = settled
            .revision
            .checked_add(1)
            .ok_or_else(|| self.value_edit_failure(value_edit::EditError::Exhausted.into()))?;
        let receipt = value_edit::decode_receipt_json(raw.get().as_bytes(), &expectation, &settled)
            .map_err(|error| self.value_edit_failure(error.into()))?;
        self.validate_context(&receipt.token.context)
            .map_err(|error| self.value_edit_failure(error))?;
        Ok(receipt)
    }

    fn value_edit_failure(&mut self, error: Error) -> Error {
        if let Error::Worker(worker) = &error {
            let id = worker.identifier.as_str();
            // The owned worker tags any post-revocation exception separately.
            // These identifiers therefore represent known precommit rejection.
            if matches!(
                id,
                "MathLab:StaleBinding" | "MathLab:VariableName" | "MathLab:VariableNotFound"
            ) || id.starts_with("MathLab:ValuePage")
                || id.starts_with("MathLab:ValueBridge")
                || (id.starts_with("MathLab:ValueEdit") && id != "MathLab:ValueEditUncertain")
            {
                return error;
            }
        }
        self.edit_history_uncertain = true;
        value_edit::EditError::UncertainCommit(error.to_string()).into()
    }

    /// Read a lossless primitive N-D page through a structured container path.
    /// The root token is checked by the worker in the same owned operation as
    /// traversal. Decode the original JSON only after cancellation settles.
    pub fn read_value_page_checked(&mut self, request: &ValuePageRequest) -> Result<ValuePage> {
        self.read_value_page_checked_with_metrics(request)
            .map(|(page, _)| page)
    }

    /// The same checked read with its actual typed-result byte count and elapsed
    /// request time. Local request validation precedes this timing interval.
    pub fn read_value_page_checked_with_metrics(
        &mut self,
        request: &ValuePageRequest,
    ) -> Result<(ValuePage, ValuePageReadMetrics)> {
        self.require_capability(value_page::CAP_TYPED_ND_PAGE)?;
        request.validate()?;
        let started = Instant::now();
        let mut payload = serde_json::to_value(request)?;
        payload["command"] = json!("read_value_page_checked");
        let (raw, _) = self.request_raw(
            payload,
            Some(COMMAND_TIMEOUT),
            value_page::MAX_PAYLOAD_BYTES + 4096,
        )?;
        let page = value_page::decode_json(raw.get().as_bytes(), request)?;
        self.validate_context(&page.token.context)?;
        Ok((
            page,
            ValuePageReadMetrics {
                result_json_bytes: raw.get().len(),
                elapsed: started.elapsed(),
            },
        ))
    }

    pub fn read_variable_checked(
        &mut self,
        token: &BindingToken,
        row_start: usize,
        column_start: usize,
        rows: usize,
        columns: usize,
    ) -> Result<BoundVariableSlice> {
        self.require_capability("display_page.v1")?;
        validate_identifier(&token.name)?;
        if row_start == 0 || column_start == 0 || rows == 0 || columns == 0 {
            return Err(Error::Protocol(
                "Variable page starts and sizes must be positive.".into(),
            ));
        }
        if rows
            .checked_mul(columns)
            .is_none_or(|count| count > MAX_VARIABLE_CELLS)
        {
            return Err(Error::LimitExceeded(
                "Variable pages are limited to 2000 cells.".into(),
            ));
        }
        let (value, _) = self.request(json!({"command":"read_variable_checked","name":token.name,
            "expected":token,"row_start":row_start,"column_start":column_start,"rows":rows,"columns":columns}), Some(COMMAND_TIMEOUT))?;
        let page: BoundVariableSlice = serde_json::from_value(value)?;
        self.validate_context(&page.token.context)?;
        if &page.token != token || page.slice.name != token.name {
            return Err(Error::Protocol(
                "Worker returned a different binding for the requested page".into(),
            ));
        }
        Ok(page)
    }

    /// Validate a primitive scalar replacement and commit against an exact
    /// snapshot token in one worker operation. No automatic retry after failure.
    pub fn set_variable_element_checked(
        &mut self,
        token: &BindingToken,
        row: usize,
        column: usize,
        literal: &str,
    ) -> Result<MutationReceipt> {
        self.require_capability("checked_scalar_write.v1")?;
        validate_identifier(&token.name)?;
        if row == 0 || column == 0 {
            return Err(Error::Protocol("Variable indices are one-based.".into()));
        }
        let scalar = parse_scalar_literal(literal)?;
        let (value, _) = self.request(
            json!({"command":"set_variable_element_checked","name":token.name,
            "expected":token,"row":row,"column":column,"scalar":scalar}),
            Some(COMMAND_TIMEOUT),
        )?;
        let receipt: MutationReceipt = serde_json::from_value(value)?;
        self.validate_context(&receipt.token.context)?;
        let mut expected = token.clone();
        expected.context.revision = expected
            .context
            .revision
            .checked_add(1)
            .ok_or_else(|| Error::Protocol("Binding revision overflow".into()))?;
        if receipt.token != expected {
            return Err(Error::Protocol(
                "Worker returned an inconsistent mutation receipt; refresh before editing".into(),
            ));
        }
        Ok(receipt)
    }

    fn validate_context(&self, context: &SessionContext) -> Result<()> {
        if context.worker_id != self.response_prefix
            || context.generation != self.generation
            || context.invocation != self.invocation
        {
            return Err(Error::Protocol(
                "Worker returned a context from a different session or invocation".into(),
            ));
        }
        Ok(())
    }

    /// Inspect a bounded page, using one-based row/column offsets. N-D variables
    /// display their first plane; char/cell/struct matrices are read-only previews.
    pub fn read_variable(
        &mut self,
        name: &str,
        row_start: usize,
        column_start: usize,
        rows: usize,
        columns: usize,
    ) -> Result<VariableSlice> {
        validate_identifier(name)?;
        if row_start == 0 || column_start == 0 || rows == 0 || columns == 0 {
            return Err(Error::Protocol(
                "Variable page starts and sizes must be positive.".into(),
            ));
        }
        if rows
            .checked_mul(columns)
            .is_none_or(|count| count > MAX_VARIABLE_CELLS)
        {
            return Err(Error::LimitExceeded(
                "Variable pages are limited to 2000 cells.".into(),
            ));
        }
        let (value, _) = self.request(
            json!({"command":"read_variable","name":name,"row_start":row_start,
                   "column_start":column_start,"rows":rows,"columns":columns}),
            Some(COMMAND_TIMEOUT),
        )?;
        Ok(serde_json::from_value(value)?)
    }

    /// Assign a scalar literal to an existing 2-D numeric/logical element without
    /// evaluating source text. Bounds are checked and class/dimensions are retained.
    pub fn set_variable_element(
        &mut self,
        name: &str,
        row: usize,
        column: usize,
        literal: &str,
    ) -> Result<Vec<Variable>> {
        validate_identifier(name)?;
        if row == 0 || column == 0 {
            return Err(Error::Protocol("Variable indices are one-based.".into()));
        }
        let scalar = parse_scalar_literal(literal)?;
        let (value, _) = self.request(
            json!({"command":"set_variable_element","name":name,"row":row,
                   "column":column,"scalar":scalar}),
            Some(COMMAND_TIMEOUT),
        )?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn set_directory(&mut self, path: &Path) -> Result<()> {
        let path = self.resolve_session_path(path)?;
        let (value, _) = self.request(
            json!({"command": "set_directory", "path": path}),
            Some(COMMAND_TIMEOUT),
        )?;
        self.config.working_directory = serde_json::from_value(value["working_directory"].clone())?;
        Ok(())
    }

    pub fn clear(&mut self) -> Result<()> {
        self.request(json!({"command": "clear"}), Some(COMMAND_TIMEOUT))?;
        Ok(())
    }

    /// Report installed Octave packages without silently loading or installing any.
    pub fn list_packages(&mut self) -> Result<Vec<Package>> {
        let (value, _) =
            self.request(json!({"command": "list_packages"}), Some(COMMAND_TIMEOUT))?;
        Ok(serde_json::from_value(value)?)
    }

    /// Load an already-installed package. Its initialization output streams live
    /// and the same bounded ordered text is returned for non-streaming callers.
    pub fn load_package(&mut self, name: &str) -> Result<String> {
        self.user_execution_started = true;
        let (_, capture) = self.request(
            json!({"command": "load_package", "name": name}),
            Some(COMMAND_TIMEOUT),
        )?;
        Ok(capture.output)
    }

    pub fn restart(&mut self) -> Result<()> {
        self.stop();
        self.debugger.take();
        self.debug_session.take();
        self.prepared_sources.clear();
        self.active_sources.clear();
        let prepared_directory = self.session.path().join("prepared");
        if prepared_directory.is_dir() {
            fs::remove_dir_all(prepared_directory)?;
        }
        self.spawn()?;
        self.edit_history_uncertain = false;
        Ok(())
    }

    fn resolve_session_path(&self, path: &Path) -> Result<PathBuf> {
        let candidate = if path.is_absolute() {
            path.to_owned()
        } else {
            self.config.working_directory.join(path)
        };
        Ok(fs::canonicalize(candidate)?)
    }

    fn spawn(&mut self) -> Result<()> {
        self.retire_value_bridge_capability();
        let _ = fs::remove_file(self.session.path().join("request.json"));
        let _ = fs::remove_file(self.session.path().join("request.tmp"));
        self.generation += 1;
        self.worker_generation
            .store(self.generation, Ordering::Release);
        self.invocation = 1;
        self.user_execution_started = false;
        self.input
            .begin_generation(self.generation, self.invocation)?;
        // Fresh random framing for each worker generation. This is accidental
        // collision protection, not a security boundary against trusted source.
        let nonce_file = tempfile::Builder::new()
            .prefix("nonce-")
            .rand_bytes(24)
            .tempfile_in(self.session.path())?;
        self.response_prefix = nonce_file
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into();
        let session_path = octave_string(&fs::canonicalize(self.session.path())?)?;
        let nonce = octave_string(Path::new(&self.response_prefix))?;
        let helper_path = octave_string(&self.console_helper)?;
        let startup = format!(
            "addpath ({session_path}); mathlab_worker ({session_path}, true, false, '', {nonce}, {}, {helper_path}, {});",
            self.generation, self.invocation
        );
        let pipe = MergedPipe::new()?;
        let child = Command::new(&self.config.executable)
            .args([
                "--quiet",
                "--no-gui",
                "--no-history",
                "--no-init-file",
                "--no-site-file",
                "--persist",
                "--eval",
                &startup,
            ])
            .current_dir(&self.config.working_directory)
            .env("QT_QPA_PLATFORM", "offscreen")
            .env("XDG_CACHE_HOME", self.session.path().join("cache"))
            .stdin(Stdio::piped())
            .stdout(pipe.stdout()?)
            .stderr(pipe.stderr()?)
            .spawn()?;
        *self
            .child
            .lock()
            .map_err(|_| Error::Protocol("Engine process lock poisoned".into()))? = Some(child);
        self.transport = Some(ConsoleTransport::start(
            pipe.into_reader(),
            self.response_prefix.clone(),
            self.generation,
            self.console.clone(),
            self.input.clone(),
        )?);
        let (ready, _) = self.read_response(0, Some(STARTUP_TIMEOUT))?;
        self.accept_ready(&ready)?;
        if let Some(bridge_path) = self.debug_bridge.clone() {
            self.install_debugger(&bridge_path)?;
        }
        if let Some(bridge_path) = self.scope_bridge.clone() {
            self.install_scope_bridge(&bridge_path)?;
        }
        if let Some(bridge_path) = self.input_bridge.clone() {
            self.install_input_adapter(&bridge_path)?;
        }
        if let Some(bridge_path) = self.value_bridge.clone() {
            self.install_value_bridge(&bridge_path)?;
        }
        Ok(())
    }

    fn accept_ready(&mut self, ready: &Value) -> Result<()> {
        let capabilities: WorkerCapabilities = serde_json::from_value(ready["protocol"].clone())?;
        capabilities.validate()?;
        let context: SessionContext = serde_json::from_value(ready["context"].clone())?;
        self.validate_context(&context)?;
        self.capabilities = Some(capabilities);
        self.version = ready
            .get("version")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Protocol("Worker omitted Octave version".into()))?
            .into();
        self.config.working_directory = serde_json::from_value(ready["working_directory"].clone())?;
        Ok(())
    }

    fn send_request(&mut self, mut request: Value, active: bool) -> Result<u64> {
        if !self.is_transport_open() {
            return Err(Error::Protocol(
                "Engine transport is closed; restart the session.".into(),
            ));
        }
        let id = self.next_id;
        self.next_id += 1;
        request["id"] = json!(id);
        let temporary = self.session.path().join("request.tmp");
        let mut file = File::create(&temporary)?;
        serde_json::to_writer(&mut file, &request)?;
        file.write_all(b"\n")?;
        file.flush()?;
        drop(file);
        let _guard = self
            .child
            .lock()
            .map_err(|_| Error::Protocol("Engine process lock poisoned".into()))?;
        if active {
            self.interrupt_pending.store(false, Ordering::Release);
            self.interrupt_sent.store(false, Ordering::Release);
            self.active_request.store(id, Ordering::Release);
            self.active.store(true, Ordering::Release);
        }
        if let Err(error) = fs::rename(temporary, self.session.path().join("request.json")) {
            if active {
                self.active.store(false, Ordering::Release);
            }
            return Err(Error::Io(error));
        }
        Ok(id)
    }

    fn request(
        &mut self,
        request: Value,
        timeout: Option<Duration>,
    ) -> Result<(Value, ConsoleCapture)> {
        let id = self.send_request(request, true)?;
        self.finish_request(id, timeout)
    }

    /// Preserve the exact result JSON for a strict typed consumer. The limit
    /// applies to the complete ordinary response file, including its envelope,
    /// before bytes are read or parsed. Internal recovery readiness and ping
    /// keep the ordinary budget. No decoder runs before cancellation settles.
    fn request_raw(
        &mut self,
        request: Value,
        timeout: Option<Duration>,
        max_response_bytes: usize,
    ) -> Result<(Box<RawValue>, ConsoleCapture)> {
        Self::validate_response_limit(max_response_bytes)?;
        let id = self.send_request(request, true)?;
        self.finish_request_raw(id, timeout, max_response_bytes)
    }

    fn finish_request(
        &mut self,
        id: u64,
        timeout: Option<Duration>,
    ) -> Result<(Value, ConsoleCapture)> {
        let (raw, capture) = self.finish_request_raw(id, timeout, MAX_RESPONSE_BYTES)?;
        let value = serde_json::from_str(raw.get()).inspect_err(|_| {
            let _ = self.terminate_transport();
        })?;
        Ok((value, capture))
    }

    fn finish_request_raw(
        &mut self,
        id: u64,
        timeout: Option<Duration>,
        max_response_bytes: usize,
    ) -> Result<(Box<RawValue>, ConsoleCapture)> {
        let mut response = self.read_response_raw_bounded(id, timeout, max_response_bytes);
        // Normal End and cancellation share the process lock. If a signal was
        // actually sent, prove that the worker has consumed an interrupt or can
        // service a fresh ping before publishing completion.
        if response.is_ok() && self.interrupt_sent.load(Ordering::Acquire) {
            match self.settle_cancellation() {
                Ok(sequence) => {
                    if let Ok((_, capture)) = response.as_mut() {
                        capture.end_sequence = capture.end_sequence.max(sequence);
                    }
                }
                Err(error) => response = Err(error),
            }
        }
        {
            let _guard = self
                .child
                .lock()
                .map_err(|_| Error::Protocol("Engine process lock poisoned".into()))?;
            self.active.store(false, Ordering::Release);
            let _ = fs::remove_file(
                self.session
                    .path()
                    .join(format!("cancel-{}-{id}", self.generation)),
            );
        }
        let (response, capture) = response?;
        Self::response_result(response).map(|value| (value, capture))
    }

    fn settle_cancellation(&mut self) -> Result<u64> {
        loop {
            let id = self.send_request(json!({"command":"ping"}), false)?;
            let (response, capture) = self.read_response_raw(id, Some(COMMAND_TIMEOUT))?;
            if response
                .error
                .as_ref()
                .is_some_and(|error| error.identifier == "MathLab:Interrupted")
            {
                continue;
            }
            let ready = Self::response_value(response).inspect_err(|error| {
                if matches!(error, Error::Json(_)) {
                    let _ = self.terminate_transport();
                }
            })?;
            self.accept_ready(&ready)?;
            return Ok(capture.end_sequence);
        }
    }

    fn response_value(response: Response) -> Result<Value> {
        let raw = Self::response_result(response)?;
        Ok(serde_json::from_str(raw.get())?)
    }

    fn response_result(response: Response) -> Result<Box<RawValue>> {
        if response.ok {
            return Ok(response.result);
        }
        Err(response
            .error
            .map(Error::Worker)
            .unwrap_or_else(|| Error::Protocol("Worker failed without error details".into())))
    }

    fn read_response(
        &mut self,
        expected_id: u64,
        timeout: Option<Duration>,
    ) -> Result<(Value, ConsoleCapture)> {
        let (response, capture) = self.read_response_raw(expected_id, timeout)?;
        Self::response_value(response)
            .inspect_err(|error| {
                if matches!(error, Error::Json(_)) {
                    let _ = self.terminate_transport();
                }
            })
            .map(|value| (value, capture))
    }

    fn deactivate_cancellation(&self, request_id: u64) -> Result<()> {
        let _guard = self
            .child
            .lock()
            .map_err(|_| Error::Protocol("Engine process lock poisoned".into()))?;
        if request_id != 0 && self.active_request.load(Ordering::Acquire) == request_id {
            self.active.store(false, Ordering::Release);
        }
        Ok(())
    }

    fn read_response_raw(
        &mut self,
        expected_id: u64,
        timeout: Option<Duration>,
    ) -> Result<(Response, ConsoleCapture)> {
        self.read_response_raw_bounded(expected_id, timeout, MAX_RESPONSE_BYTES)
    }

    fn read_response_raw_bounded(
        &mut self,
        expected_id: u64,
        timeout: Option<Duration>,
        max_response_bytes: usize,
    ) -> Result<(Response, ConsoleCapture)> {
        Self::validate_response_limit(max_response_bytes)?;
        let start = Instant::now();
        loop {
            if timeout.is_some_and(|limit| start.elapsed() >= limit) {
                self.terminate_transport()?;
                return Err(Error::Timeout);
            }
            let event = self
                .transport
                .as_ref()
                .ok_or_else(|| Error::Protocol("Engine is not running".into()))?
                .recv_timeout(Duration::from_millis(100));
            match event {
                Ok(TransportEvent::Begin {
                    invocation,
                    request_id,
                }) => {
                    if invocation != self.invocation
                        || (request_id != expected_id && request_id != 0)
                    {
                        self.terminate_transport()?;
                        return Err(Error::Protocol(format!(
                            "Worker began invocation {invocation} request {request_id}, expected {} request {expected_id}",
                            self.invocation
                        )));
                    }
                }
                Ok(TransportEvent::Complete {
                    invocation,
                    request_id,
                    capture,
                }) => {
                    if invocation != self.invocation || request_id != expected_id {
                        self.terminate_transport()?;
                        return Err(Error::Protocol(format!(
                            "Unexpected worker completion invocation {invocation} request {request_id}"
                        )));
                    }
                    if request_id != 0 {
                        self.deactivate_cancellation(request_id)?;
                    }
                    let response = self
                        .read_metadata(invocation, request_id, false, max_response_bytes)
                        .inspect_err(|_| {
                            let _ = self.terminate_transport();
                        })?;
                    return Ok((response, capture));
                }
                Ok(TransportEvent::Recovery {
                    invocation,
                    request_id,
                    capture,
                }) => {
                    if invocation != self.invocation {
                        self.terminate_transport()?;
                        return Err(Error::Protocol(
                            "Unexpected worker recovery invocation".into(),
                        ));
                    }
                    self.deactivate_cancellation(request_id)?;
                    // An unsolicited/idle recovery can concern an older job.
                    // Do not apply this request's scoped cap to another ID.
                    let recovery_limit = if request_id == expected_id {
                        max_response_bytes
                    } else {
                        MAX_RESPONSE_BYTES
                    };
                    let interrupted = self
                        .read_metadata(invocation, request_id, true, recovery_limit)
                        .inspect_err(|_| {
                            let _ = self.terminate_transport();
                        })?;
                    // The normal result may have been staged before SIGINT but
                    // its End never reached the reader. Its metadata is immutable.
                    if capture.is_some() {
                        let _ = fs::remove_file(
                            self.session
                                .path()
                                .join(format!("response-{invocation}-{request_id}.json")),
                        );
                    }
                    self.resume_worker()?;
                    let (ready, ready_capture) =
                        self.read_response_raw(0, Some(STARTUP_TIMEOUT))?;
                    if !ready.ok {
                        // Recovery can follow an already-committed edit. A
                        // loader's ValueBridge/ValuePage identifier is therefore
                        // not evidence of precommit rejection of that edit.
                        let details = ready.error.map_or_else(
                            || "without error details".to_owned(),
                            |error| format!("{}: {}", error.identifier, error.message),
                        );
                        let _ = self.terminate_transport();
                        return Err(Error::Protocol(format!(
                            "Recovery worker failed; the session was terminated: {details}"
                        )));
                    }
                    let ready_value: Value =
                        serde_json::from_str(ready.result.get()).inspect_err(|_| {
                            let _ = self.terminate_transport();
                        })?;
                    self.accept_ready(&ready_value)?;
                    if expected_id == 0 {
                        return Ok((ready, ready_capture));
                    }
                    if request_id == expected_id
                        && let Some(mut capture) = capture
                    {
                        capture.end_sequence = capture.end_sequence.max(ready_capture.end_sequence);
                        return Ok((interrupted, capture));
                    }
                }
                Ok(TransportEvent::Failed(message)) => {
                    self.terminate_transport()?;
                    return Err(Error::Protocol(message));
                }
                Ok(TransportEvent::Closed) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.input.close();
                    self.terminate_transport()?;
                    return Err(Error::ProcessExited(format!(
                        "Console pipe closed before a completion fence. {}",
                        self.console.diagnostics(self.generation)
                    )));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let exited = self
                        .child
                        .lock()
                        .map_err(|_| Error::Protocol("Engine process lock poisoned".into()))?
                        .as_mut()
                        .map(|child| child.try_wait())
                        .transpose()?
                        .flatten()
                        .is_some();
                    if exited {
                        self.terminate_transport()?;
                        return Err(Error::ProcessExited(format!(
                            "Worker exited before a completion fence. {}",
                            self.console.diagnostics(self.generation)
                        )));
                    }
                }
            }
        }
    }

    fn validate_response_limit(max_response_bytes: usize) -> Result<()> {
        if max_response_bytes == 0 || max_response_bytes > MAX_RESPONSE_BYTES {
            return Err(Error::Protocol(format!(
                "Response byte limit must be between 1 and {MAX_RESPONSE_BYTES}."
            )));
        }
        Ok(())
    }

    fn read_metadata(
        &self,
        invocation: u64,
        id: u64,
        interrupted: bool,
        max_response_bytes: usize,
    ) -> Result<Response> {
        Self::validate_response_limit(max_response_bytes)?;
        let kind = if interrupted {
            "interruption"
        } else {
            "response"
        };
        let path = self
            .session
            .path()
            .join(format!("{kind}-{invocation}-{id}.json"));
        let file = File::open(&path)?;
        if file.metadata()?.len() > max_response_bytes as u64 {
            return Err(Error::LimitExceeded(format!(
                "Worker metadata exceeded this request's {max_response_bytes}-byte response limit"
            )));
        }
        let mut bytes = Vec::new();
        file.take((max_response_bytes + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > max_response_bytes {
            return Err(Error::LimitExceeded(format!(
                "Worker metadata exceeded this request's {max_response_bytes}-byte response limit"
            )));
        }
        let response: Response = serde_json::from_slice(&bytes)?;
        if response.id != id {
            return Err(Error::Protocol(format!(
                "Worker metadata {} does not match fence {id}",
                response.id
            )));
        }
        fs::remove_file(path)?;
        Ok(response)
    }

    fn resume_worker(&mut self) -> Result<()> {
        self.retire_value_bridge_capability();
        self.invocation += 1;
        self.input
            .begin_generation(self.generation, self.invocation)?;
        let session_path = octave_string(&fs::canonicalize(self.session.path())?)?;
        let enabled = if self.debugger.is_some() {
            "true"
        } else {
            "false"
        };
        let scope_path = self
            .scope_bridge
            .as_deref()
            .map(octave_string)
            .transpose()?
            .unwrap_or_else(|| "''".into());
        let response_prefix = octave_string(Path::new(&self.response_prefix))?;
        let helper_path = octave_string(&self.console_helper)?;
        let input_path = self
            .input_bridge
            .as_deref()
            .map(octave_string)
            .transpose()?
            .unwrap_or_else(|| "''".into());
        let value_path = self
            .value_bridge
            .as_deref()
            .map(octave_string)
            .transpose()?
            .unwrap_or_else(|| "''".into());
        let command = format!(
            "addpath ({session_path}); mathlab_worker ({session_path}, false, {enabled}, {scope_path}, {response_prefix}, {}, {helper_path}, {}, {input_path}, {value_path});\n",
            self.generation, self.invocation
        );
        let mut guard = self
            .child
            .lock()
            .map_err(|_| Error::Protocol("Engine process lock poisoned".into()))?;
        let stdin = guard
            .as_mut()
            .and_then(|child| child.stdin.as_mut())
            .ok_or_else(|| Error::Protocol("Missing engine recovery input".into()))?;
        stdin.write_all(command.as_bytes())?;
        stdin.flush()?;
        Ok(())
    }

    fn terminate_transport(&mut self) -> Result<()> {
        self.retire_value_bridge_capability();
        self.input.close();
        let result = self.cancellation_handle().terminate();
        if let Some(mut transport) = self.transport.take() {
            transport.stop();
        }
        result
    }

    fn stop(&mut self) {
        self.input.close();
        self.active.store(false, Ordering::Release);
        if let Ok(mut guard) = self.child.lock()
            && let Some(mut child) = guard.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(mut transport) = self.transport.take() {
            transport.stop();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

fn prepared_request(source: &PreparedSource, debugger_enabled: bool) -> Value {
    let mut request = json!({"command":"run_file", "path":source.materialized.script_path,
    "source_directory":source.materialized.script_path.parent(),
    "keep_source_directory":!source.materialized.function_paths.is_empty(),
    "local_names":source.local_functions.iter().map(|function| &function.name).collect::<Vec<_>>(),
    "factory_path":source.factory_path,
    "original_path":source.original_path,
    "original_source":source.original_source,
    "source_kind":match source.source_kind {
        SourceKind::Script => "script", SourceKind::Function => "function", SourceKind::Class => "class"
    }});
    if debugger_enabled
        && source.source_kind == SourceKind::Script
        && source.materialized.function_paths.is_empty()
    {
        // Native plain-script entry parses the absolute immutable filename and
        // needs no load-path capsule. Even a temporary addpath/rmpath can rebind
        // unrelated functions, losing their breakpoint or persistent state.
        request
            .as_object_mut()
            .expect("prepared request object")
            .remove("source_directory");
    }
    request
}

fn debug_source_descriptor(original_path: PathBuf, request: &Value) -> Result<PreparedDebugSource> {
    let script_path: PathBuf = serde_json::from_value(request["path"].clone())?;
    Ok(PreparedDebugSource {
        prepared: script_path != original_path,
        original_path,
        script_path,
    })
}

fn diagnostic_line(message: &str) -> Option<usize> {
    let after = message.split_once("near line ")?.1;
    let digits: String = after
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn display_script_frame(name: &str, generated_stem: &str, original_stem: &str) -> String {
    if let Some(position) = name.find(generated_stem) {
        let suffix = &name[position + generated_stem.len()..];
        if suffix.starts_with('>') {
            return format!("{original_stem}{suffix}");
        }
    } else if name.starts_with('@') {
        return format!("{original_stem}>{name}");
    }
    original_stem.into()
}

fn display_local_frame(name: &str, outer: &str, original_stem: &str) -> String {
    if name.is_empty() {
        return format!("{original_stem}>{outer}");
    }
    if name == outer || name.starts_with(&format!("{outer}>")) {
        format!("{original_stem}>{name}")
    } else {
        format!("{original_stem}>{outer}>{name}")
    }
}

fn validate_identifier(name: &str) -> Result<()> {
    let mut bytes = name.bytes();
    if !bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || name.len() > 63
    {
        return Err(Error::Protocol(
            "Expected a variable identifier (up to 63 ASCII characters).".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct ScalarLiteral {
    real: String,
    imaginary: String,
    /// Retain exact integer text for 64-bit integer classes (f64 would round it).
    integer: Option<String>,
}

fn parse_scalar_literal(literal: &str) -> Result<ScalarLiteral> {
    let invalid = || {
        Error::Protocol("Enter a scalar numeric, complex, NaN, Inf, true, or false literal.".into())
    };
    if literal.len() > 512 {
        return Err(invalid());
    }
    let tokens: Vec<_> = literal.split_ascii_whitespace().collect();
    if tokens
        .windows(2)
        .any(|pair| !pair[0].ends_with(['+', '-']) && !pair[1].starts_with(['+', '-']))
    {
        return Err(invalid());
    }
    let compact: String = literal
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect();
    let compact = compact.to_ascii_lowercase();
    let digits = compact.trim_start_matches(['+', '-']);
    let integer = (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| compact.clone());
    let (real, imaginary) = if compact == "true" {
        (1.0, 0.0)
    } else if compact == "false" {
        (0.0, 0.0)
    } else if let Some(body) = compact.strip_suffix(['i', 'j']) {
        let split = body
            .bytes()
            .enumerate()
            .skip(1)
            .rfind(|(index, byte)| {
                (*byte == b'+' || *byte == b'-') && body.as_bytes()[index - 1] != b'e'
            })
            .map(|(index, _)| index);
        let (real, imaginary) = split.map_or(("0", body), |index| body.split_at(index));
        let imaginary = match imaginary {
            "" | "+" => 1.0,
            "-" => -1.0,
            value => value.parse::<f64>().map_err(|_| invalid())?,
        };
        (real.parse::<f64>().map_err(|_| invalid())?, imaginary)
    } else {
        (compact.parse::<f64>().map_err(|_| invalid())?, 0.0)
    };
    let canonical = |value: f64| {
        if value.is_nan() {
            "NaN".into()
        } else if value == f64::INFINITY {
            "Inf".into()
        } else if value == f64::NEG_INFINITY {
            "-Inf".into()
        } else {
            value.to_string()
        }
    };
    Ok(ScalarLiteral {
        real: canonical(real),
        imaginary: canonical(imaginary),
        integer,
    })
}

fn octave_string(path: &Path) -> Result<String> {
    let text = path
        .to_str()
        .ok_or_else(|| Error::Protocol("Octave paths must be valid UTF-8".into()))?;
    Ok(format!("'{}'", text.replace('\'', "''")))
}

/// Discover an installed Octave executable without launching or downloading one.
pub fn discover_octave() -> Option<PathBuf> {
    for variable in ["MATHLAB_OCTAVE", "OCTAVE_EXECUTABLE"] {
        if let Some(path) = std::env::var_os(variable).map(PathBuf::from)
            && is_executable(&path)
        {
            return Some(path);
        }
    }
    let names = if cfg!(windows) {
        vec!["octave-cli.exe", "octave.exe"]
    } else {
        vec!["octave-cli", "octave"]
    };
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            for name in &names {
                let candidate = directory.join(name);
                if is_executable(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    for candidate in [
        "/opt/homebrew/bin/octave-cli",
        "/opt/homebrew/bin/octave",
        "/usr/local/bin/octave-cli",
        "/usr/local/bin/octave",
        "/usr/bin/octave-cli",
        "/usr/bin/octave",
    ] {
        let path = PathBuf::from(candidate);
        if is_executable(&path) {
            return Some(path);
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_response_preserves_duplicate_fields_for_strict_typed_consumers() {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct TypedProbe {
            schema_version: u32,
            real: Vec<String>,
        }
        let result = r#"{"schema_version":1,"schema_version":2,"real":["9007199254741009"]}"#;
        let envelope = format!(r#"{{"id":7,"ok":true,"result":{result}}}"#);
        let response: Response = serde_json::from_str(&envelope).unwrap();
        let raw = Engine::response_result(response).unwrap();
        assert_eq!(raw.get(), result);
        let error = serde_json::from_str::<TypedProbe>(raw.get()).unwrap_err();
        assert!(error.to_string().contains("duplicate field"));
        let valid: TypedProbe =
            serde_json::from_str(r#"{"schema_version":1,"real":["9007199254741009"]}"#).unwrap();
        assert_eq!(valid.schema_version, 1);
        assert_eq!(valid.real, vec!["9007199254741009"]);
        // Legacy consumers still receive their established Value representation.
        let response: Response = serde_json::from_str(&envelope).unwrap();
        assert_eq!(
            Engine::response_value(response).unwrap()["schema_version"],
            2
        );
    }

    #[test]
    fn raw_response_retains_legacy_null_and_worker_error_behavior() {
        for envelope in [
            r#"{"id":1,"ok":true}"#,
            r#"{"id":1,"ok":true,"result":null}"#,
        ] {
            let response: Response = serde_json::from_str(envelope).unwrap();
            assert!(Engine::response_value(response).unwrap().is_null());
        }
        let response: Response = serde_json::from_str(
            r#"{"id":1,"ok":false,"error":{"identifier":"MathLab:Test","message":"expected","stack":[]}}"#,
        ).unwrap();
        assert!(
            matches!(Engine::response_result(response), Err(Error::Worker(EngineError { identifier, .. })) if identifier == "MathLab:Test")
        );
    }

    #[test]
    fn scoped_metadata_limit_is_enforced_before_json_and_does_not_change_the_ordinary_limit() {
        let Some(executable) = discover_octave() else {
            eprintln!("SKIP scoped metadata integration: Octave unavailable");
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let engine = Engine::start(EngineConfig {
            executable,
            working_directory: directory.path().to_owned(),
        })
        .unwrap();
        let path = engine.session.path().join("response-1-999.json");
        File::create(&path).unwrap().set_len(65).unwrap();
        // This sparse file contains invalid JSON. The scoped length error must
        // take precedence over any JSON parser/allocation attempt.
        assert!(matches!(
            engine.read_metadata(1, 999, false, 64),
            Err(Error::LimitExceeded(_))
        ));
        fs::write(&path, b"invalid JSON").unwrap();
        assert!(matches!(
            engine.read_metadata(1, 999, false, 64),
            Err(Error::Json(_))
        ));
        let encoded =
            serde_json::to_vec(&json!({"id":999,"ok":true,"result":{"padding":"x".repeat(128)}}))
                .unwrap();
        fs::write(&path, &encoded).unwrap();
        assert!(matches!(
            engine.read_metadata(1, 999, false, 64),
            Err(Error::LimitExceeded(_))
        ));
        let response = engine.read_metadata(1, 999, false, encoded.len()).unwrap();
        assert_eq!(
            Engine::response_value(response).unwrap()["padding"],
            "x".repeat(128)
        );
        assert!(matches!(
            engine.read_metadata(1, 999, false, 0),
            Err(Error::Protocol(_))
        ));
        assert!(matches!(
            engine.read_metadata(1, 999, false, MAX_RESPONSE_BYTES + 1),
            Err(Error::Protocol(_))
        ));
    }

    #[test]
    fn scoped_raw_requests_preserve_legacy_values_and_reject_invalid_budgets_before_send() {
        let Some(executable) = discover_octave() else {
            eprintln!("SKIP raw response integration: Octave unavailable");
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let mut engine = Engine::start(EngineConfig {
            executable,
            working_directory: directory.path().to_owned(),
        })
        .unwrap();
        let next_id = engine.next_id;
        for invalid_limit in [0, MAX_RESPONSE_BYTES + 1] {
            assert!(matches!(
                engine.request_raw(
                    json!({"command":"execute","code":"budget_must_not_execute=1;"}),
                    Some(COMMAND_TIMEOUT),
                    invalid_limit
                ),
                Err(Error::Protocol(_))
            ));
            assert_eq!(engine.next_id, next_id);
        }
        let (raw, capture) = engine
            .request_raw(json!({"command":"ping"}), Some(COMMAND_TIMEOUT), 16 * 1024)
            .unwrap();
        let ready: Value = serde_json::from_str(raw.get()).unwrap();
        assert_eq!(ready["protocol"]["name"], "mathlab-worker");
        assert!(capture.end_sequence > 0);
        let snapshot = engine.workspace_snapshot().unwrap();
        assert!(
            !snapshot
                .variables
                .iter()
                .any(|entry| entry.variable.name == "budget_must_not_execute")
        );
        let (legacy, _) = engine
            .request(json!({"command":"ping"}), Some(COMMAND_TIMEOUT))
            .unwrap();
        assert_eq!(legacy["version"], engine.version());
    }

    #[cfg(unix)]
    #[test]
    fn scoped_raw_request_uses_the_ordinary_budget_for_recovery_readiness_and_ping() {
        use std::thread;
        let Some(executable) = discover_octave() else {
            eprintln!("SKIP scoped raw SIGINT integration: Octave unavailable");
            return;
        };
        let before =
            "'context', mathlab_binding_context (worker_id, generation, invocation, revision));";
        let after = "'context', mathlab_binding_context (worker_id, generation, invocation, revision), 'padding', builtin('char', builtin('ones', 1, 32768) * 120));";
        let worker = WORKER.replace(before, after);
        assert_ne!(worker, WORKER, "Recovery-ready injection anchor moved");
        let directory = tempfile::tempdir().unwrap();
        let engine = Engine::start_with_worker(
            EngineConfig {
                executable,
                working_directory: directory.path().to_owned(),
            },
            &worker,
        )
        .unwrap();
        let process_id = engine.process_id();
        let subscription = engine.subscribe_console();
        let cancellation = engine.cancellation_handle();
        struct Guard(Option<CancellationHandle>);
        impl Drop for Guard {
            fn drop(&mut self) {
                if let Some(cancellation) = &self.0 {
                    let _ = cancellation.terminate();
                }
            }
        }
        let mut guard = Guard(Some(cancellation.clone()));
        let task = thread::spawn(move || {
            let mut engine = engine;
            let result = engine.request_raw(json!({"command":"execute","code":"scoped_raw_saved=23; fprintf(stdout,'SCOPED_RAW_STOP_READY'); fflush(stdout); while true; end;"}), Some(Duration::from_secs(25)), 16 * 1024);
            (engine, result)
        });
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut output = Vec::new();
        let mut ready = false;
        while Instant::now() < deadline && !ready {
            if let Ok(event) = subscription.recv_timeout(Duration::from_millis(100)) {
                match event.kind {
                    ConsoleEventKind::Output { bytes } => output.extend(bytes),
                    ConsoleEventKind::End | ConsoleEventKind::Disconnected { .. } => break,
                    _ => {}
                }
                ready = output
                    .windows(b"SCOPED_RAW_STOP_READY".len())
                    .any(|window| window == b"SCOPED_RAW_STOP_READY");
            }
        }
        if !ready {
            let _ = cancellation.terminate();
            let _ = task.join();
            panic!("worker did not reach its scoped raw Stop barrier: {output:?}");
        }
        cancellation.interrupt().unwrap();
        let (mut engine, result) = task.join().unwrap();
        guard.0 = None;
        let (raw, capture) = result.unwrap();
        let execution: ExecutionResult = serde_json::from_str(raw.get()).unwrap();
        assert_eq!(execution.error.unwrap().identifier, "MathLab:Interrupted");
        assert_eq!(engine.process_id(), process_id);
        assert!(capture.end_sequence > 0);
        let (ready, _) = engine
            .request(json!({"command":"ping"}), Some(COMMAND_TIMEOUT))
            .unwrap();
        assert_eq!(ready["padding"].as_str().unwrap().len(), 32768);
        assert!(
            engine
                .execute("assert(scoped_raw_saved==23);")
                .unwrap()
                .error
                .is_none()
        );
    }

    #[cfg(unix)]
    #[test]
    fn startup_failure_retains_merged_diagnostics_before_the_first_begin() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("failing-runtime");
        fs::write(
            &executable,
            b"#!/bin/sh\nprintf 'STARTUP_DRIVER_FAILURE\\n' >&2\nexit 7\n",
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let error = Engine::start(EngineConfig {
            executable,
            working_directory: directory.path().to_owned(),
        })
        .err()
        .expect("the deliberately failing runtime unexpectedly started");
        assert!(matches!(error, Error::ProcessExited(_)), "{error}");
        assert!(
            error.to_string().contains("STARTUP_DRIVER_FAILURE"),
            "{error}"
        );
    }

    #[test]
    fn malformed_current_frame_terminates_session_and_restart_recovers() {
        let Some(executable) = discover_octave() else {
            eprintln!("Skipping actual corrupt-frame test: GNU Octave is absent");
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let mut engine = Engine::start(EngineConfig {
            executable,
            working_directory: directory.path().to_owned(),
        })
        .unwrap();
        let original_marker = engine.response_prefix.clone();
        let generation = engine.console_generation();
        let request_id = engine.next_id;
        let error = engine
            .execute(&format!(
                "system('printf ''\\036MATHLAB:{original_marker}:{generation}:X:1:{request_id}\\037''');"
            ))
            .unwrap_err();
        assert!(matches!(error, Error::Protocol(_)), "{error}");
        assert!(engine.execute("must_not_succeed=1;").is_err());
        engine.restart().unwrap();
        assert_ne!(engine.response_prefix, original_marker);
        let result = engine.execute("fresh_value=4;").unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(result.workspace.len(), 1);
        assert_eq!(result.workspace[0].name, "fresh_value");
    }

    #[test]
    fn response_metadata_is_bounded_and_validated_separately_from_console() {
        let Some(executable) = discover_octave() else {
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let engine = Engine::start(EngineConfig {
            executable,
            working_directory: directory.path().to_owned(),
        })
        .unwrap();
        let path = engine.session.path().join("response-1-999.json");
        File::create(&path)
            .unwrap()
            .set_len((MAX_RESPONSE_BYTES + 1) as u64)
            .unwrap();
        assert!(matches!(
            engine.read_metadata(1, 999, false, MAX_RESPONSE_BYTES),
            Err(Error::LimitExceeded(_))
        ));
        fs::write(&path, b"\xff").unwrap();
        assert!(matches!(
            engine.read_metadata(1, 999, false, MAX_RESPONSE_BYTES),
            Err(Error::Json(_))
        ));
        fs::write(
            &path,
            br#"{"id":999,"ok":true,"result":{"metadata":"safe"}}"#,
        )
        .unwrap();
        assert_eq!(
            Engine::response_value(
                engine
                    .read_metadata(1, 999, false, MAX_RESPONSE_BYTES)
                    .unwrap()
            )
            .unwrap()["metadata"],
            "safe"
        );
        assert!(!path.exists());
    }

    #[test]
    fn script_frame_names_hide_generated_paths_and_source_implementation_suffixes() {
        for name in [
            "__generated",
            "/tmp/prepared/__generated.m|source",
            "/tmp/prepared/__generated.mlsource|source",
            "__generated|source",
        ] {
            assert_eq!(
                display_script_frame(name, "__generated", "user_script"),
                "user_script"
            );
        }
        assert_eq!(
            display_script_frame("__generated>@<anonymous>", "__generated", "user_script"),
            "user_script>@<anonymous>"
        );
    }

    #[test]
    fn scalar_literal_parser_accepts_numbers_without_evaluating_expressions() {
        for (input, real, imaginary) in [
            ("1.25", "1.25", "0"),
            ("-Inf", "-Inf", "0"),
            ("NaN", "NaN", "0"),
            ("false", "0", "0"),
            ("1.5 - 2e-3i", "1.5", "-0.002"),
            ("1e-2+3e+4j", "0.01", "30000"),
            ("-i", "0", "-1"),
        ] {
            let scalar = parse_scalar_literal(input).unwrap();
            assert_eq!(
                (scalar.real.as_str(), scalar.imaginary.as_str()),
                (real, imaginary)
            );
        }
        for invalid in [
            "",
            "1 2",
            "1 e2",
            "true false",
            "[1]",
            "1;clear",
            "1+2",
            "sin(1)",
            "1+2+3i",
            "NaN(payload)",
        ] {
            assert!(parse_scalar_literal(invalid).is_err(), "{invalid}");
        }
        assert_eq!(
            parse_scalar_literal("18446744073709551615")
                .unwrap()
                .integer
                .as_deref(),
            Some("18446744073709551615")
        );
        assert!(validate_identifier("A;clear").is_err());
        assert!(validate_identifier("1A").is_err());
        assert!(validate_identifier("valid_name2").is_ok());
    }

    #[test]
    fn quoted_paths_preserve_apostrophes() {
        assert_eq!(
            octave_string(Path::new("/tmp/it's a path")).unwrap(),
            "'/tmp/it''s a path'"
        );
    }

    #[test]
    fn absent_octave_error_decodes_from_empty_array() {
        let result: ExecutionResult = serde_json::from_value(json!({
            "output":"", "error":[], "workspace":[], "figures":[],
            "working_directory":"/tmp", "duration_ms":0, "figure_warnings":[]
        }))
        .unwrap();
        assert!(result.error.is_none());
    }
}
