use crate::range_transactions::{
    TransactionCommand, TransactionEnvelope, TransactionFailure, TransactionOutcome,
    TransactionRefresh,
};
use crate::variable_editor::VariablePage;
use mathlab_engine::startup_config::QualifiedStartupConfig;
use mathlab_engine::{
    BindingToken, CancellationHandle, DecimalU64, Engine, EngineConfig, ExecutionResult,
    MutationReceipt, Package, PageWindow, ValuePageRequest, ValuePath, discover_octave,
};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

pub enum Request {
    RangeTransaction(Box<TransactionEnvelope>),
    Start {
        executable: Option<PathBuf>,
        startup_config: Option<PathBuf>,
        packaged_startup: bool,
        directory: PathBuf,
    },
    Execute(String),
    RunFile(PathBuf),
    RunBuffer {
        source: String,
        path: PathBuf,
    },
    Packages,
    LoadPackage(String),
    Directory(PathBuf),
    Restart,
    EnableDebugger(PathBuf),
    PrepareDebug {
        request_id: u64,
        document_id: Option<usize>,
        path: PathBuf,
        source: Option<String>,
    },
    ReadVariable {
        request_id: u64,
        name: String,
        token: Option<BindingToken>,
        row_start: usize,
        column_start: usize,
        higher_indices: Vec<u64>,
    },
    WriteVariable {
        request_id: u64,
        token: BindingToken,
        row: usize,
        column: usize,
        literal: String,
    },
}

pub enum Event {
    RangeTransactionFinished {
        envelope: Box<TransactionEnvelope>,
        outcome: Box<TransactionOutcome>,
    },
    ConsoleConnected {
        subscription: mathlab_engine::ConsoleSubscription,
        generation: u64,
    },
    Ready {
        executable: PathBuf,
        qualified_profile: Option<String>,
        debugger: Option<mathlab_debugger::Debugger>,
        cancel: CancellationHandle,
        internal_worker: PathBuf,
        input: Option<mathlab_engine::InputHandle>,
    },
    Executed(ExecutionResult),
    Directory(PathBuf),
    Restarted {
        qualified_profile: Option<String>,
        debugger: Option<mathlab_debugger::Debugger>,
        internal_worker: PathBuf,
        input: Option<mathlab_engine::InputHandle>,
    },
    DebuggerReady(mathlab_debugger::Debugger),
    DebuggerUnavailable {
        error: String,
        debugger: Option<mathlab_debugger::Debugger>,
    },
    DebugSourcePrepared {
        request_id: u64,
        document_id: Option<usize>,
        result: Result<mathlab_engine::PreparedDebugSource, mathlab_engine::Error>,
    },
    Packages {
        packages: Vec<Package>,
        output: String,
    },
    Failed(String),
    SourceFailed(mathlab_engine::EngineError),
    Disconnected(String),
    Unavailable(String),
    VariableRead {
        request_id: u64,
        result: Result<VariablePage, String>,
    },
    VariableChanged {
        request_id: u64,
        result: Result<MutationReceipt, String>,
    },
}

pub fn spawn(ctx: eframe::egui::Context) -> (Sender<Request>, Receiver<Event>) {
    spawn_with_executable(
        ctx,
        std::env::current_exe().map_err(|error| error.to_string()),
    )
}

fn spawn_with_executable(
    ctx: eframe::egui::Context,
    running_executable: Result<PathBuf, String>,
) -> (Sender<Request>, Receiver<Event>) {
    // Freeze the physical process anchor once; retries must not retarget an
    // executable alias that has changed after this worker was created.
    let running_executable =
        running_executable.and_then(|path| path.canonicalize().map_err(|error| error.to_string()));
    let (requests_tx, requests_rx) = channel();
    let (events_tx, events_rx) = channel();
    std::thread::Builder::new()
        .name("mathlab-engine".into())
        .spawn(move || {
            let mut engine: Option<Engine> = None;
            let mut qualified_selection = false;
            let mut selected_config: Option<QualifiedStartupConfig> = None;
            while let Ok(request) = requests_rx.recv() {
                let event = match request {
                    Request::RangeTransaction(envelope) => {
                        let outcome = match engine.as_mut() {
                            Some(engine) => run_range_transaction(engine, &envelope),
                            None => TransactionOutcome::Failed(TransactionFailure {
                                message: "The engine is not connected.".into(),
                                uncertain: false,
                                disconnected: true,
                            }),
                        };
                        Event::RangeTransactionFinished { envelope, outcome: Box::new(outcome) }
                    }
                    Request::Start { executable, startup_config, packaged_startup, directory } => {
                        engine = None;
                        selected_config = None;
                        qualified_selection = startup_config.is_some() || packaged_startup;
                        match select_startup(executable, startup_config, packaged_startup, &running_executable, discover_octave)
                            .and_then(|selection| start_engine(selection, directory))
                        {
                            Ok((started, executable, config)) => {
                                let qualified_profile = accepted_profile(&started);
                                let debugger = started.debugger_handle();
                                let cancel = started.cancellation_handle();
                                let internal_worker = started.worker_source_path();
                                let _ = events_tx.send(Event::ConsoleConnected {
                                    subscription: started.subscribe_console(),
                                    generation: started.console_generation(),
                                });
                                let input = started.input_handle();
                                selected_config = config;
                                engine = Some(started);
                                Event::Ready { executable, qualified_profile, debugger, cancel, internal_worker, input }
                            }
                            Err(error) => Event::Unavailable(error),
                        }
                    }
                    Request::Execute(code) => match engine.as_mut() {
                        Some(engine) => match engine.execute(&code) {
                            Ok(result) => Event::Executed(result),
                            Err(error) => failure(error),
                        },
                        None => Event::Failed("The engine is not connected.".into()),
                    },
                    Request::RunFile(path) => match engine.as_mut() {
                        Some(engine) => match engine.run_file(&path) {
                            Ok(result) => Event::Executed(result),
                            Err(error) => failure(error),
                        },
                        None => Event::Failed("The engine is not connected.".into()),
                    },
                    Request::RunBuffer { source, path } => match engine.as_mut() {
                        Some(engine) => match engine.run_buffer(&source, &path) {
                            Ok(result) => Event::Executed(result),
                            Err(error) => failure(error),
                        },
                        None => Event::Failed("The engine is not connected.".into()),
                    },
                    Request::Packages => match engine.as_mut() {
                        Some(engine) => match engine.list_packages() {
                            Ok(packages) => Event::Packages { packages, output: String::new() },
                            Err(error) => failure(error),
                        },
                        None => Event::Failed("The engine is not connected.".into()),
                    },
                    Request::LoadPackage(name) => match engine.as_mut() {
                        Some(engine) => match engine.load_package(&name) {
                            Ok(output) => match engine.list_packages() {
                                Ok(packages) => Event::Packages { packages, output },
                                Err(error) => failure(error),
                            },
                            Err(error) => failure(error),
                        },
                        None => Event::Failed("The engine is not connected.".into()),
                    },
                    Request::Directory(directory) => match engine.as_mut() {
                        Some(engine) => match engine.set_directory(&directory) {
                            Ok(()) => Event::Directory(directory),
                            Err(error) => failure(error),
                        },
                        None => Event::Directory(directory),
                    },
                    Request::Restart => match engine.as_mut() {
                        Some(engine) => match restart_engine(engine, selected_config.as_ref()) {
                            Ok(()) => {
                                let qualified_profile = accepted_profile(engine);
                                if qualified_selection && qualified_profile.is_none() {
                                    let _ = engine.cancellation_handle().terminate();
                                    Event::Unavailable("Qualified restart did not retain accepted startup status.".into())
                                } else {
                                    Event::Restarted { qualified_profile, debugger: engine.debugger_handle(), internal_worker: engine.worker_source_path(), input: engine.input_handle() }
                                }
                            }
                            Err(error) => Event::Unavailable(error.to_string()),
                        },
                        None if qualified_selection => Event::Unavailable("The qualified engine is unavailable. Retry the startup configuration in Engine Settings.".into()),
                        None => Event::Unavailable("Choose an Octave executable to start the engine.".into()),
                    },
                    Request::EnableDebugger(_) if qualified_selection => Event::DebuggerUnavailable {
                        error: "The startup configuration owns the debugger adapter; runtime overrides are disabled.".into(),
                        debugger: engine.as_ref().and_then(Engine::debugger_handle),
                    },
                    Request::EnableDebugger(path) => match engine.as_mut() {
                        Some(engine) => match engine.enable_debugger(&path) {
                            Ok(debugger) => Event::DebuggerReady(debugger),
                            Err(error) if disconnected(&error) => failure(error),
                            Err(error) => Event::DebuggerUnavailable { error: error.to_string(), debugger: engine.debugger_handle() },
                        },
                        None => Event::DebuggerUnavailable { error: "Connect GNU Octave before enabling the debugger.".into(), debugger: None },
                    },
                    Request::PrepareDebug { request_id, document_id, path, source } => match engine.as_mut() {
                        Some(engine) => {
                            let result = if let Some(source) = source { engine.prepare_debug_buffer(&source, &path) }
                                else { engine.prepare_debug_file(&path) };
                            match result {
                                Err(error) if disconnected(&error) => failure(error),
                                result => Event::DebugSourcePrepared { request_id, document_id, result },
                            }
                        }
                        None => Event::Disconnected("The engine is not connected.".into()),
                    },
                    Request::ReadVariable { request_id, name, token, row_start, column_start, higher_indices } => {
                        match engine.as_mut() {
                            Some(engine) => match read_variable_page(engine, &name, token, row_start, column_start, higher_indices) {
                                Ok(slice) => Event::VariableRead { request_id, result: Ok(slice) },
                                Err(error) if disconnected(&error) => failure(error),
                                Err(error) => Event::VariableRead { request_id, result: Err(error.to_string()) },
                            },
                            None => Event::Disconnected("The engine is not connected.".into()),
                        }
                    }
                    Request::WriteVariable { request_id, token, row, column, literal } => {
                        match engine.as_mut() {
                            Some(engine) => match engine.set_variable_element_checked(&token, row, column, &literal) {
                                Ok(receipt) => Event::VariableChanged { request_id, result: Ok(receipt) },
                                Err(error) if disconnected(&error) => failure(error),
                                Err(error) => Event::VariableChanged { request_id, result: Err(error.to_string()) },
                            },
                            None => Event::Disconnected("The engine is not connected.".into()),
                        }
                    }
                };
                if events_tx.send(event).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        })
        .expect("could not create engine worker thread");
    (requests_tx, events_rx)
}

#[derive(Debug, PartialEq, Eq)]
enum StartupSelection {
    Qualified(PathBuf),
    Packaged(PathBuf),
    Legacy(PathBuf),
}

fn select_startup(
    executable: Option<PathBuf>,
    startup_config: Option<PathBuf>,
    packaged_startup: bool,
    running_executable: &Result<PathBuf, String>,
    discover: impl FnOnce() -> Option<PathBuf>,
) -> Result<StartupSelection, String> {
    if packaged_startup {
        if startup_config.is_some() || executable.is_some() {
            return Err(
                "Packaged startup cannot be combined with a startup configuration or Octave executable override."
                    .into(),
            );
        }
        return installed_contents(running_executable).map(StartupSelection::Packaged);
    }
    if let Some(path) = startup_config {
        if executable.is_some() {
            return Err(
                "A startup configuration cannot be combined with an Octave executable override."
                    .into(),
            );
        }
        return Ok(StartupSelection::Qualified(path));
    }
    executable.or_else(discover).map(StartupSelection::Legacy).ok_or_else(|| {
        "GNU Octave was not found. Install Octave, then choose its executable in Engine Settings.".into()
    })
}

// The captured physical executable is the sole bundle anchor. Revalidate its
// spelling without retargeting it; cwd, environment and recovery cannot select it.
fn installed_contents(running_executable: &Result<PathBuf, String>) -> Result<PathBuf, String> {
    let executable = running_executable.as_ref().map_err(Clone::clone)?;
    let resolved = executable
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if resolved.as_os_str() != executable.as_os_str() {
        return Err("The running executable's physical installed context changed.".into());
    }
    let macos = executable
        .parent()
        .ok_or("Installed executable has no MacOS directory")?;
    let contents = macos
        .parent()
        .ok_or("Installed executable has no Contents directory")?;
    if !executable.is_file()
        || executable.file_name() != Some(std::ffi::OsStr::new("mathlab-desktop"))
        || macos.file_name() != Some(std::ffi::OsStr::new("MacOS"))
        || contents.file_name() != Some(std::ffi::OsStr::new("Contents"))
    {
        return Err(
            "Packaged startup requires the running Contents/MacOS/mathlab-desktop executable."
                .into(),
        );
    }
    Ok(contents.to_owned())
}

fn start_engine(
    selection: StartupSelection,
    directory: PathBuf,
) -> Result<(Engine, PathBuf, Option<QualifiedStartupConfig>), String> {
    match selection {
        selection @ (StartupSelection::Qualified(_) | StartupSelection::Packaged(_)) => {
            let config = match selection {
                StartupSelection::Qualified(path) => QualifiedStartupConfig::load(&path),
                StartupSelection::Packaged(contents) => {
                    QualifiedStartupConfig::load_installed(&contents)
                }
                StartupSelection::Legacy(_) => unreachable!(),
            }
            .map_err(|error| error.to_string())?;
            let executable = config.executable().to_owned();
            let engine = config.start(directory).map_err(|error| error.to_string())?;
            Ok((engine, executable, Some(config)))
        }
        StartupSelection::Legacy(executable) => {
            let engine = Engine::start(EngineConfig {
                executable: executable.clone(),
                working_directory: directory,
            })
            .map_err(|error| error.to_string())?;
            Ok((engine, executable, None))
        }
    }
}

fn restart_engine(
    engine: &mut Engine,
    config: Option<&QualifiedStartupConfig>,
) -> mathlab_engine::Result<()> {
    match config {
        Some(config) => config.restart(engine),
        None => engine.restart(),
    }
}

fn accepted_profile(engine: &Engine) -> Option<String> {
    engine
        .qualified_startup_status()
        .map(|status| status.profile.to_owned())
}

/// Commit and refresh are separate outcomes: a successful receipt is never
/// replaced by a follow-up read error, and a failed edit is never retried.
fn run_range_transaction(
    engine: &mut Engine,
    envelope: &TransactionEnvelope,
) -> TransactionOutcome {
    if let Err(message) = envelope.validate() {
        return TransactionOutcome::Failed(TransactionFailure::rejected(message));
    }
    if let TransactionCommand::Reset { expected } = &envelope.command {
        let snapshot = match engine.workspace_snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => return TransactionOutcome::Failed(range_failure(engine, error)),
        };
        if &snapshot.context != expected {
            return TransactionOutcome::Failed(TransactionFailure::rejected(
                "The session changed before the explicit history reset. Refresh before resetting again.",
            ));
        }
        return match engine.reset_value_edit_history() {
            Ok(snapshot) => TransactionOutcome::Reset(snapshot),
            Err(error) => TransactionOutcome::Failed(range_failure(engine, error)),
        };
    }
    let result = match &envelope.command {
        TransactionCommand::Write(request) => engine.write_value_range_checked(request),
        TransactionCommand::Undo(request) => engine.undo_value_edit_checked(request),
        TransactionCommand::Redo(request) => engine.redo_value_edit_checked(request),
        TransactionCommand::Reset { .. } => unreachable!(),
    };
    let receipt = match result {
        Ok(receipt) => receipt,
        Err(error) => return TransactionOutcome::Failed(range_failure(engine, error)),
    };
    let refresh = (|| {
        let snapshot = engine.workspace_snapshot()?;
        let mut path = envelope.page.path.clone();
        path.root = receipt.token.clone();
        let page = engine.read_value_page_checked(&ValuePageRequest {
            path,
            window: envelope.page.window.clone(),
        })?;
        Ok::<_, mathlab_engine::Error>(Box::new(TransactionRefresh { snapshot, page }))
    })()
    .map_err(|error| range_failure(engine, error));
    TransactionOutcome::Applied {
        receipt: Box::new(receipt),
        refresh,
    }
}

fn range_failure(engine: &Engine, error: mathlab_engine::Error) -> TransactionFailure {
    TransactionFailure {
        uncertain: engine.has_value_edit_uncertainty()
            || matches!(
                &error,
                mathlab_engine::Error::ValueEdit(
                    mathlab_engine::value_edit::EditError::UncertainCommit(_)
                )
            ),
        disconnected: !engine.is_transport_open() || disconnected(&error),
        message: error.to_string(),
    }
}

fn read_variable_page(
    engine: &mut Engine,
    name: &str,
    token: Option<BindingToken>,
    row_start: usize,
    column_start: usize,
    higher_indices: Vec<u64>,
) -> mathlab_engine::Result<VariablePage> {
    let (token, primitive, metadata) = if let Some(token) = token {
        if token.name != name {
            return Err(mathlab_engine::Error::Protocol(
                "The variable selection changed. Refresh before inspecting it.".into(),
            ));
        }
        (token, true, None)
    } else {
        let binding = engine
            .workspace_snapshot()?
            .variables
            .into_iter()
            .find(|binding| binding.variable.name == name)
            .ok_or_else(|| {
                mathlab_engine::Error::Protocol(format!(
                    "Variable {name} is no longer in the workspace."
                ))
            })?;
        let primitive = matches!(
            binding.variable.class_name.as_str(),
            "double"
                | "single"
                | "int8"
                | "uint8"
                | "int16"
                | "uint16"
                | "int32"
                | "uint32"
                | "int64"
                | "uint64"
                | "logical"
        );
        (binding.token, primitive, Some(binding.variable))
    };
    let result = (|| {
        if primitive {
            if engine.capabilities().supports("typed_nd_page.v1") {
                let row_start = u64::try_from(row_start).map_err(|_| {
                    mathlab_engine::Error::Protocol(
                        "Page start exceeds the wire index range.".into(),
                    )
                })?;
                let column_start = u64::try_from(column_start).map_err(|_| {
                    mathlab_engine::Error::Protocol(
                        "Page start exceeds the wire index range.".into(),
                    )
                })?;
                let request = ValuePageRequest {
                    path: ValuePath {
                        root: token,
                        steps: Vec::new(),
                    },
                    window: PageWindow {
                        row_start: DecimalU64::new(row_start),
                        column_start: DecimalU64::new(column_start),
                        rows: crate::variable_editor::PAGE_ROWS as u32,
                        columns: crate::variable_editor::PAGE_COLUMNS as u32,
                        higher_indices: higher_indices.into_iter().map(DecimalU64::new).collect(),
                    },
                };
                // A typed failure never accepts values. A fresh snapshot may
                // correct only the next explicit read's metadata/selector draft;
                // never retry the token or downgrade to a first-plane display.
                return engine
                    .read_value_page_checked(&request)
                    .map(|page| VariablePage::Typed {
                        page: Box::new(page),
                        metadata: metadata.clone(),
                    });
            }
            if higher_indices.iter().any(|&index| index != 1) {
                engine.require_capability("typed_nd_page.v1")?;
            }
            // A checked failure is terminal for this read. Never reacquire a token
            // or fall back to an unconditional preview after a stale response.
            engine
                .read_variable_checked(
                    &token,
                    row_start,
                    column_start,
                    crate::variable_editor::PAGE_ROWS,
                    crate::variable_editor::PAGE_COLUMNS,
                )
                .map(VariablePage::Checked)
        } else {
            // Only fresh metadata can choose the existing read-only container path.
            // Legacy previews can have effects, so they never carry an edit token.
            engine
                .read_variable(
                    name,
                    row_start,
                    column_start,
                    crate::variable_editor::PAGE_ROWS,
                    crate::variable_editor::PAGE_COLUMNS,
                )
                .map(VariablePage::Preview)
        }
    })();
    match result {
        Err(error) if !disconnected(&error) => match metadata {
            Some(variable) => Ok(VariablePage::MetadataOnly {
                variable,
                diagnostic: error.to_string(),
            }),
            None => Err(error),
        },
        result => result,
    }
}

fn failure(error: mathlab_engine::Error) -> Event {
    if let mathlab_engine::Error::Worker(error) = error {
        Event::SourceFailed(error)
    } else if disconnected(&error) {
        Event::Disconnected(error.to_string())
    } else {
        Event::Failed(error.to_string())
    }
}

fn disconnected(error: &mathlab_engine::Error) -> bool {
    match error {
        mathlab_engine::Error::ProcessExited(_) | mathlab_engine::Error::Timeout => true,
        mathlab_engine::Error::Io(error) => matches!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::NotConnected
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_startup_selection_bypasses_discovery_and_rejects_overrides() {
        let path = PathBuf::from("qualified.json");
        assert_eq!(
            select_startup(
                None,
                Some(path.clone()),
                false,
                &Err("unused".into()),
                || panic!("qualified selection must not discover")
            ),
            Ok(StartupSelection::Qualified(path.clone())),
        );
        assert!(
            select_startup(
                Some("octave".into()),
                Some(path),
                false,
                &Err("unused".into()),
                || panic!("conflicting selection must not discover")
            )
            .is_err()
        );
        assert_eq!(
            select_startup(
                Some("selected-octave".into()),
                None,
                false,
                &Err("unused".into()),
                || panic!("explicit runtime must not discover")
            ),
            Ok(StartupSelection::Legacy("selected-octave".into())),
        );
        assert_eq!(
            select_startup(None, None, false, &Err("unused".into()), || Some(
                "discovered-octave".into()
            )),
            Ok(StartupSelection::Legacy("discovered-octave".into())),
        );
    }

    #[test]
    fn unavailable_explicit_config_fails_before_any_legacy_start() {
        let directory = tempfile::tempdir().unwrap();
        let selection = select_startup(
            None,
            Some(directory.path().join("missing.json")),
            false,
            &Err("unused".into()),
            || panic!("invalid qualified configuration must not discover a fallback"),
        )
        .unwrap();
        assert!(start_engine(selection, directory.path().to_owned()).is_err());
    }

    fn installed_test_executable(base: &std::path::Path) -> PathBuf {
        let executable = base.join("Moved λ space.app/Contents/MacOS/mathlab-desktop");
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::write(&executable, b"test context anchor; never executed").unwrap();
        executable.canonicalize().unwrap()
    }

    #[test]
    fn packaged_selection_uses_only_physical_executable_context_and_rejects_conflicts() {
        let directory = tempfile::tempdir().unwrap();
        let executable = installed_test_executable(directory.path());
        let running = Ok(executable.clone());
        let selected = select_startup(None, None, true, &running, || {
            panic!("no packaged discovery")
        })
        .unwrap();
        assert_eq!(
            selected,
            StartupSelection::Packaged(executable.parent().unwrap().parent().unwrap().to_owned())
        );
        for (runtime, config) in [
            (Some("override".into()), None),
            (None, Some("seed.json".into())),
        ] {
            assert!(
                select_startup(runtime, config, true, &running, || panic!(
                    "no conflict discovery"
                ))
                .is_err()
            );
        }
        let wrong_layout = directory.path().join("mathlab-desktop");
        std::fs::write(&wrong_layout, b"wrong installed layout").unwrap();
        for context in [
            Err("unavailable current executable".into()),
            Ok(directory.path().join("missing")),
            Ok(wrong_layout.canonicalize().unwrap()),
        ] {
            assert!(
                select_startup(None, None, true, &context, || panic!("no context fallback"))
                    .is_err()
            );
        }
        #[cfg(unix)]
        {
            let alias = directory.path().join("desktop-alias");
            std::os::unix::fs::symlink(&executable, &alias).unwrap();
            assert_eq!(
                installed_contents(&Ok(alias.canonicalize().unwrap())).unwrap(),
                executable.parent().unwrap().parent().unwrap()
            );
        }
    }

    #[test]
    fn missing_packaged_configuration_keeps_worker_qualified_on_retry_and_restart() {
        use std::time::Duration;
        let directory = tempfile::tempdir().unwrap();
        let executable = installed_test_executable(directory.path());
        let (requests, events) =
            spawn_with_executable(eframe::egui::Context::default(), Ok(executable));
        for _ in 0..2 {
            requests
                .send(Request::Start {
                    executable: None,
                    startup_config: None,
                    packaged_startup: true,
                    directory: directory.path().into(),
                })
                .unwrap();
            assert!(matches!(
                events.recv_timeout(Duration::from_secs(3)).unwrap(),
                Event::Unavailable(_)
            ));
            requests.send(Request::Restart).unwrap();
            match events.recv_timeout(Duration::from_secs(3)).unwrap() {
                Event::Unavailable(error) => assert!(error.contains("qualified engine")),
                _ => panic!("missing packaged startup must remain qualified"),
            }
            requests
                .send(Request::EnableDebugger("override.oct".into()))
                .unwrap();
            assert!(matches!(
                events.recv_timeout(Duration::from_secs(3)).unwrap(),
                Event::DebuggerUnavailable { debugger: None, .. }
            ));
        }
    }

    // These ignored tests exercise the actual worker and admitted external
    // runtime, with a deliberately injected test executable context. They do
    // not prove the production current_exe route or a normal/relocated UI launch.
    struct PackagedRuntimeFixture {
        _directory: tempfile::TempDir,
        executable: PathBuf,
        contents: PathBuf,
        old_native: PathBuf,
    }

    impl PackagedRuntimeFixture {
        fn new() -> Self {
            Self::from_seed("MATHLAB_QUALIFIED_STARTUP_CONFIG")
        }

        fn from_seed(variable: &str) -> Self {
            let seed = std::env::var_os(variable)
                .unwrap_or_else(|| panic!("required packaged worker test needs {variable}"));
            let config = QualifiedStartupConfig::load(std::path::Path::new(&seed)).unwrap();
            let mut document: serde_json::Value =
                serde_json::from_slice(&std::fs::read(seed).unwrap()).unwrap();
            let directory = tempfile::tempdir().unwrap();
            let executable = installed_test_executable(directory.path());
            let contents = executable.parent().unwrap().parent().unwrap().to_owned();
            let native = contents.join("Resources/native");
            let old_native = directory
                .path()
                .canonicalize()
                .unwrap()
                .join("Original.app/Contents/Resources/native");
            std::fs::create_dir_all(&native).unwrap();
            std::fs::create_dir_all(&old_native).unwrap();
            for (basename, module) in config.native_modules() {
                std::fs::copy(&module.path, native.join(&basename)).unwrap();
                std::fs::copy(&module.path, old_native.join(basename)).unwrap();
            }
            document["startup"]["provider"]["path"] =
                serde_json::json!(old_native.join("mathlab_intrinsic_bridge.oct"));
            for adapter in document["startup"]["adapters"].as_array_mut().unwrap() {
                let basename = std::path::Path::new(adapter["file"]["path"].as_str().unwrap())
                    .file_name()
                    .unwrap()
                    .to_owned();
                adapter["file"]["path"] = serde_json::json!(old_native.join(basename));
            }
            std::fs::write(
                contents.join("Resources/qualified-core-v2.json"),
                serde_json::to_vec(&document).unwrap(),
            )
            .unwrap();
            Self {
                _directory: directory,
                executable,
                contents,
                old_native,
            }
        }

        fn start(&self, requests: &Sender<Request>) {
            requests
                .send(Request::Start {
                    executable: None,
                    startup_config: None,
                    packaged_startup: true,
                    directory: self._directory.path().into(),
                })
                .unwrap();
        }
    }

    fn next_startup_event(events: &Receiver<Event>) -> Event {
        loop {
            match events
                .recv_timeout(std::time::Duration::from_secs(30))
                .unwrap()
            {
                Event::ConsoleConnected { .. } => {}
                event => return event,
            }
        }
    }

    struct PackagedChildGuard(CancellationHandle);

    impl Drop for PackagedChildGuard {
        fn drop(&mut self) {
            let _ = self.0.terminate();
        }
    }

    fn require_packaged_ready(events: &Receiver<Event>) -> PackagedChildGuard {
        match next_startup_event(events) {
            Event::Ready {
                qualified_profile,
                debugger,
                input,
                cancel,
                ..
            } => {
                let guard = PackagedChildGuard(cancel);
                assert_eq!(qualified_profile.as_deref(), Some("qualified-core-v2"));
                assert!(debugger.is_some());
                assert!(input.is_some());
                guard
            }
            Event::Unavailable(error) => panic!("packaged startup failed: {error}"),
            _ => panic!("expected accepted packaged Ready"),
        }
    }

    #[test]
    #[ignore = "UNRUN: requires admitted native runtime; injected executable context is not UI proof"]
    fn packaged_worker_accepts_current_modules_with_original_present_then_absent_and_restarts() {
        let fixture = PackagedRuntimeFixture::new();
        let (requests, events) = spawn_with_executable(
            eframe::egui::Context::default(),
            Ok(fixture.executable.clone()),
        );
        fixture.start(&requests);
        let first = require_packaged_ready(&events);
        std::fs::remove_dir_all(&fixture.old_native).unwrap();
        requests.send(Request::Restart).unwrap();
        match next_startup_event(&events) {
            Event::Restarted {
                qualified_profile,
                debugger,
                input,
                ..
            } => {
                assert_eq!(qualified_profile.as_deref(), Some("qualified-core-v2"));
                assert!(debugger.is_some());
                assert!(input.is_some());
            }
            _ => panic!("packaged restart must retain current native modules"),
        }
        first.0.terminate().unwrap();
        fixture.start(&requests);
        let current = require_packaged_ready(&events);
        current.0.terminate().unwrap();
    }

    #[test]
    #[ignore = "UNRUN: requires admitted native runtime; injected executable context is not UI proof"]
    fn packaged_worker_current_tamper_fails_with_valid_original_and_retry_recovers_in_place() {
        let fixture = PackagedRuntimeFixture::new();
        let selected = fixture
            .contents
            .join("Resources/native/mathlab_intrinsic_bridge.oct");
        let good = std::fs::read(&selected).unwrap();
        std::fs::write(&selected, b"tampered installed provider").unwrap();
        let (requests, events) = spawn_with_executable(
            eframe::egui::Context::default(),
            Ok(fixture.executable.clone()),
        );
        fixture.start(&requests);
        match next_startup_event(&events) {
            Event::Unavailable(error) => assert!(error.contains("SHA-256"), "{error}"),
            Event::Ready { cancel, .. } => {
                let _ = cancel.terminate();
                panic!("valid old bundle must not hide current tamper");
            }
            _ => panic!("expected packaged admission failure"),
        }
        requests.send(Request::Restart).unwrap();
        assert!(matches!(next_startup_event(&events), Event::Unavailable(_)));
        requests
            .send(Request::EnableDebugger("override.oct".into()))
            .unwrap();
        assert!(matches!(
            next_startup_event(&events),
            Event::DebuggerUnavailable { .. }
        ));
        std::fs::write(selected, good).unwrap();
        fixture.start(&requests);
        let current = require_packaged_ready(&events);
        current.0.terminate().unwrap();
    }

    #[test]
    #[ignore = "UNRUN: requires MATHLAB_QUALIFIED_GRAPHICS_STARTUP_CONFIG; injected context is not UI proof"]
    fn packaged_graphics_selection_activates_registry_and_restores_it_on_frontend_restart() {
        let fixture =
            PackagedRuntimeFixture::from_seed("MATHLAB_QUALIFIED_GRAPHICS_STARTUP_CONFIG");
        let (mut engine, _, selected) = start_engine(
            StartupSelection::Packaged(fixture.contents.clone()),
            fixture._directory.path().canonicalize().unwrap(),
        )
        .unwrap();
        let config = selected.expect("packaged selection must retain its configuration");
        assert_eq!(
            config.profile(),
            mathlab_engine::QUALIFIED_GRAPHICS_STARTUP_PROFILE
        );
        assert_eq!(
            accepted_profile(&engine).as_deref(),
            Some(mathlab_engine::QUALIFIED_GRAPHICS_STARTUP_PROFILE)
        );
        config.require_active(&engine).unwrap();
        assert!(engine.graphics_snapshot().unwrap().nodes.is_empty());
        let result = engine.execute("desktop_gf=figure('visible','off'); desktop_ga=axes('parent',desktop_gf); desktop_gl=line('parent',desktop_ga,'xdata',[1,2,3],'ydata',[10,20,30]);").unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        let old_scene = engine.graphics_snapshot().unwrap();
        assert_eq!(old_scene.nodes.len(), 3);
        let old = old_scene
            .nodes
            .iter()
            .find(|node| {
                matches!(
                    node.properties,
                    mathlab_engine::graphics::GraphicsProperties::Line { .. }
                )
            })
            .unwrap()
            .token
            .clone();
        let pid = engine.process_id();
        std::fs::remove_dir_all(&fixture.old_native).unwrap();
        restart_engine(&mut engine, Some(&config)).unwrap();
        assert_ne!(engine.process_id(), pid);
        config.require_active(&engine).unwrap();
        assert!(engine.graphics_snapshot().unwrap().nodes.is_empty());
        assert!(engine.graphics_query(&old, 1, 2).is_err());
        // Replace the current file's inode, preserving any loaded native image.
        // A configured restart must fail closed rather than lose graphics and
        // announce the selected profile as an active frontend session.
        let replacement = fixture.contents.join("Resources/native/replacement.oct");
        std::fs::write(&replacement, b"tampered current graphics adapter").unwrap();
        std::fs::rename(
            replacement,
            fixture
                .contents
                .join("Resources/native/mathlab_graphics_bridge.oct"),
        )
        .unwrap();
        assert!(restart_engine(&mut engine, Some(&config)).is_err());
        assert!(accepted_profile(&engine).is_none());
        assert!(engine.graphics_snapshot().is_err());
    }

    #[test]
    #[ignore = "requires MATHLAB_QUALIFIED_STARTUP_CONFIG with the admitted native runtime"]
    fn qualified_worker_start_and_restart_retain_profile_and_native_adapters() {
        use std::time::Duration;

        let path = std::env::var_os("MATHLAB_QUALIFIED_STARTUP_CONFIG").expect(
            "required qualified desktop integration needs MATHLAB_QUALIFIED_STARTUP_CONFIG",
        );
        let config = QualifiedStartupConfig::load(std::path::Path::new(&path)).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let (requests, events) = spawn(eframe::egui::Context::default());
        requests
            .send(Request::Start {
                executable: None,
                startup_config: Some(path.into()),
                packaged_startup: false,
                directory: directory.path().to_owned(),
            })
            .unwrap();
        let next = || loop {
            match events.recv_timeout(Duration::from_secs(30)).unwrap() {
                Event::ConsoleConnected { .. } => {}
                event => break event,
            }
        };
        let cancel = match next() {
            Event::Ready {
                executable,
                qualified_profile,
                debugger,
                input,
                cancel,
                ..
            } => {
                assert_eq!(executable.as_path(), config.executable());
                assert_eq!(qualified_profile.as_deref(), Some("qualified-core-v2"));
                assert!(
                    debugger.is_some(),
                    "the configured debugger must be enabled before Ready"
                );
                assert!(
                    input.is_some(),
                    "the configured input adapter must be enabled before Ready"
                );
                cancel
            }
            _ => panic!("expected qualified Ready after adapter activation"),
        };
        struct TerminateOnDrop(CancellationHandle);
        impl Drop for TerminateOnDrop {
            fn drop(&mut self) {
                let _ = self.0.terminate();
            }
        }
        let _terminate = TerminateOnDrop(cancel);
        requests
            .send(Request::EnableDebugger(
                directory.path().join("override.oct"),
            ))
            .unwrap();
        assert!(matches!(
            next(),
            Event::DebuggerUnavailable {
                debugger: Some(_),
                ..
            }
        ));
        requests
            .send(Request::Execute("qualified_before_restart=42;".into()))
            .unwrap();
        match next() {
            Event::Executed(result) => assert!(result.error.is_none(), "{:?}", result.error),
            _ => panic!("expected qualified execution"),
        }
        requests
            .send(Request::ReadVariable {
                request_id: 1,
                name: "qualified_before_restart".into(),
                token: None,
                row_start: 1,
                column_start: 1,
                higher_indices: Vec::new(),
            })
            .unwrap();
        assert!(matches!(
            next(),
            Event::VariableRead {
                result: Ok(VariablePage::Typed { .. }),
                ..
            }
        ));
        requests.send(Request::Restart).unwrap();
        match next() {
            Event::Restarted {
                qualified_profile,
                debugger,
                input,
                ..
            } => {
                assert_eq!(qualified_profile.as_deref(), Some("qualified-core-v2"));
                assert!(debugger.is_some());
                assert!(input.is_some());
            }
            _ => panic!("expected qualified restart with restored native adapters"),
        }
        requests
            .send(Request::Execute(
                "assert(exist('qualified_before_restart', 'var') == 0); qualified_after_restart=7;"
                    .into(),
            ))
            .unwrap();
        match next() {
            Event::Executed(result) => assert!(result.error.is_none(), "{:?}", result.error),
            _ => panic!("expected cleared workspace after qualified restart"),
        }
        requests
            .send(Request::ReadVariable {
                request_id: 2,
                name: "qualified_after_restart".into(),
                token: None,
                row_start: 1,
                column_start: 1,
                higher_indices: Vec::new(),
            })
            .unwrap();
        assert!(matches!(
            next(),
            Event::VariableRead {
                result: Ok(VariablePage::Typed { .. }),
                ..
            }
        ));
    }

    #[test]
    #[ignore = "requires MATHLAB_QUALIFIED_STARTUP_CONFIG with the admitted native runtime"]
    fn qualified_worker_retry_after_unavailable_config_preserves_selection() {
        use std::time::Duration;

        let admitted = std::env::var_os("MATHLAB_QUALIFIED_STARTUP_CONFIG").expect(
            "required qualified desktop integration needs MATHLAB_QUALIFIED_STARTUP_CONFIG",
        );
        let admitted = std::path::Path::new(&admitted);
        let config = QualifiedStartupConfig::load(admitted).unwrap();
        let config_bytes = std::fs::read(admitted).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let selected = directory.path().join("retry-qualified.json");
        let (requests, events) = spawn(eframe::egui::Context::default());
        let next = || loop {
            match events.recv_timeout(Duration::from_secs(30)).unwrap() {
                Event::ConsoleConnected { .. } => {}
                event => break event,
            }
        };
        requests
            .send(Request::Start {
                executable: None,
                startup_config: Some(selected.clone()),
                packaged_startup: false,
                directory: directory.path().to_owned(),
            })
            .unwrap();
        match next() {
            Event::Unavailable(error) => assert!(!error.is_empty()),
            Event::Ready { cancel, .. } => {
                let _ = cancel.terminate();
                panic!("missing qualified configuration must not start a legacy fallback");
            }
            _ => panic!("expected unavailable selected configuration"),
        }
        requests.send(Request::Restart).unwrap();
        match next() {
            Event::Unavailable(error) => assert!(error.contains("qualified engine")),
            _ => panic!("unavailable qualified selection must remain selected on restart"),
        }

        std::fs::write(&selected, &config_bytes).unwrap();
        requests
            .send(Request::Start {
                executable: None,
                startup_config: Some(selected),
                packaged_startup: false,
                directory: directory.path().to_owned(),
            })
            .unwrap();
        let (executable, profile, debugger, input, cancel) = match next() {
            Event::Ready {
                executable,
                qualified_profile,
                debugger,
                input,
                cancel,
                ..
            } => (executable, qualified_profile, debugger, input, cancel),
            _ => panic!("expected qualified Ready after restoring the selected configuration"),
        };
        struct TerminateOnDrop(CancellationHandle);
        impl Drop for TerminateOnDrop {
            fn drop(&mut self) {
                let _ = self.0.terminate();
            }
        }
        let _terminate = TerminateOnDrop(cancel);
        assert_eq!(executable.as_path(), config.executable());
        assert_eq!(profile.as_deref(), Some("qualified-core-v2"));
        assert!(debugger.is_some());
        assert!(input.is_some());
        requests
            .send(Request::EnableDebugger(
                directory.path().join("override.oct"),
            ))
            .unwrap();
        assert!(matches!(
            next(),
            Event::DebuggerUnavailable {
                debugger: Some(_),
                ..
            }
        ));
        requests
            .send(Request::Execute("qualified_after_retry=uint64(42);".into()))
            .unwrap();
        match next() {
            Event::Executed(result) => assert!(result.error.is_none(), "{:?}", result.error),
            _ => panic!("expected execution in the retried qualified session"),
        }
        requests
            .send(Request::ReadVariable {
                request_id: 1,
                name: "qualified_after_retry".into(),
                token: None,
                row_start: 1,
                column_start: 1,
                higher_indices: Vec::new(),
            })
            .unwrap();
        let token = match next() {
            Event::VariableRead {
                request_id: 1,
                result: Ok(VariablePage::Typed { page, .. }),
            } => page.token,
            _ => panic!("expected a typed binding after qualified retry"),
        };
        requests
            .send(Request::ReadVariable {
                request_id: 2,
                name: "qualified_after_retry".into(),
                token: Some(token.clone()),
                row_start: 1,
                column_start: 1,
                higher_indices: Vec::new(),
            })
            .unwrap();
        match next() {
            Event::VariableRead {
                request_id: 2,
                result: Ok(VariablePage::Typed { page, .. }),
            } => {
                assert_eq!(page.token, token);
                assert_eq!(page.class, mathlab_engine::NumericClass::Uint64);
                match page.data {
                    mathlab_engine::PageData::Dense {
                        real: mathlab_engine::NumericComponents::Uint64(values),
                        imag: None,
                    } => assert_eq!(values, vec![42]),
                    _ => panic!("expected the exact uint64 value in a checked typed page"),
                }
            }
            _ => panic!("expected a checked typed read in the retried qualified session"),
        }
    }

    #[test]
    fn lost_engine_transport_disconnects_instead_of_leaving_the_editor_enabled() {
        assert!(disconnected(&mathlab_engine::Error::Timeout));
        assert!(disconnected(&mathlab_engine::Error::Io(
            std::io::Error::from(std::io::ErrorKind::BrokenPipe)
        )));
        assert!(!disconnected(&mathlab_engine::Error::Io(
            std::io::Error::from(std::io::ErrorKind::PermissionDenied)
        )));
        assert!(!disconnected(&mathlab_engine::Error::Protocol(
            "Invalid literal".into()
        )));
    }

    #[test]
    fn queued_variable_write_rejects_the_original_page_after_same_name_rebinding() {
        use mathlab_engine::ConsoleEventKind;
        use std::time::{Duration, Instant};

        let Some(executable) = discover_octave() else {
            eprintln!("Skipping checked desktop binding integration: GNU Octave was not found.");
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let (requests, events) = spawn(eframe::egui::Context::default());
        requests
            .send(Request::Start {
                executable: Some(executable),
                startup_config: None,
                packaged_startup: false,
                directory: directory.path().into(),
            })
            .unwrap();
        let mut console = None;
        let cancel = loop {
            match events.recv_timeout(Duration::from_secs(30)).unwrap() {
                Event::ConsoleConnected { subscription, .. } => console = Some(subscription),
                Event::Ready { cancel, .. } => break cancel,
                Event::Unavailable(error) => panic!("desktop worker startup failed: {error}"),
                _ => panic!("unexpected startup event"),
            }
        };
        struct TerminateOnDrop(CancellationHandle);
        impl Drop for TerminateOnDrop {
            fn drop(&mut self) {
                let _ = self.0.terminate();
            }
        }
        let _terminate = TerminateOnDrop(cancel);
        requests
            .send(Request::Execute("A=1; C={'still readable'};".into()))
            .unwrap();
        match events.recv_timeout(Duration::from_secs(10)).unwrap() {
            Event::Executed(result) => assert!(result.error.is_none()),
            _ => panic!("expected initial execution"),
        }
        requests
            .send(Request::ReadVariable {
                request_id: 1,
                name: "A".into(),
                token: None,
                row_start: 1,
                column_start: 1,
                higher_indices: Vec::new(),
            })
            .unwrap();
        let original = match events.recv_timeout(Duration::from_secs(10)).unwrap() {
            Event::VariableRead {
                result: Ok(VariablePage::Typed { page, .. }),
                ..
            } => {
                assert!(matches!(&page.data, mathlab_engine::PageData::Dense {
                    real: mathlab_engine::NumericComponents::Double(values), imag: None
                } if values.as_slice() == [1.0]));
                page.token
            }
            Event::VariableRead {
                result: Ok(VariablePage::Checked(page)),
                ..
            } => {
                assert_eq!(page.slice.cells[0][0], "1");
                page.token
            }
            _ => panic!("expected original checked page"),
        };
        requests.send(Request::Execute(
            "A=2; fprintf('BINDING_REPLACED'); while (!exist('release-binding-test','file')); pause(.01); endwhile;".into()
        )).unwrap();
        requests
            .send(Request::WriteVariable {
                request_id: 2,
                token: original,
                row: 1,
                column: 1,
                literal: "99".into(),
            })
            .unwrap();
        let console = console.unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut output = Vec::new();
        while !output
            .windows(b"BINDING_REPLACED".len())
            .any(|part| part == b"BINDING_REPLACED")
        {
            let event = console
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            if let ConsoleEventKind::Output { bytes } = event.kind {
                output.extend(bytes);
            }
        }
        assert!(
            events.try_recv().is_err(),
            "the source must still be blocked before release"
        );
        std::fs::write(directory.path().join("release-binding-test"), b"release").unwrap();
        match events.recv_timeout(Duration::from_secs(10)).unwrap() {
            Event::Executed(result) => assert!(result.error.is_none()),
            _ => panic!("expected source completion before queued edit"),
        }
        match events.recv_timeout(Duration::from_secs(10)).unwrap() {
            Event::VariableChanged {
                request_id: 2,
                result: Err(error),
            } => {
                assert!(error.contains("binding changed"), "{error}");
            }
            _ => panic!("a queued old-page edit must be rejected"),
        }
        requests
            .send(Request::ReadVariable {
                request_id: 3,
                name: "A".into(),
                token: None,
                row_start: 1,
                column_start: 1,
                higher_indices: Vec::new(),
            })
            .unwrap();
        match events.recv_timeout(Duration::from_secs(10)).unwrap() {
            Event::VariableRead {
                result: Ok(VariablePage::Typed { page, .. }),
                ..
            } => {
                assert!(
                    matches!(&page.data, mathlab_engine::PageData::Dense {
                    real: mathlab_engine::NumericComponents::Double(values), imag: None
                } if values.as_slice() == [2.0]),
                    "stale write changed the replacement"
                );
            }
            Event::VariableRead {
                result: Ok(VariablePage::Checked(page)),
                ..
            } => {
                assert_eq!(
                    page.slice.cells[0][0], "2",
                    "stale write changed the replacement"
                );
            }
            _ => panic!("expected a fresh checked page"),
        }
        requests
            .send(Request::ReadVariable {
                request_id: 4,
                name: "C".into(),
                token: None,
                row_start: 1,
                column_start: 1,
                higher_indices: Vec::new(),
            })
            .unwrap();
        match events.recv_timeout(Duration::from_secs(10)).unwrap() {
            Event::VariableRead {
                result: Ok(VariablePage::Preview(slice)),
                ..
            } => {
                assert!(!slice.editable);
                assert_eq!(slice.cells[0][0], "'still readable'");
            }
            _ => panic!("existing cell previews must stay read-only and available"),
        }
    }

    #[test]
    fn desktop_worker_typed_nd_route_preserves_plane_context_and_rejects_rebinding() {
        use mathlab_engine::{NumericClass, NumericComponents, PageData, SourceStorage, ValuePage};
        use std::time::Duration;

        let Some(executable) = discover_octave() else {
            assert_ne!(
                std::env::var("MATHLAB_REQUIRE_TYPED_PAGE_TESTS").as_deref(),
                Ok("1"),
                "required desktop typed-page integration needs an installed Octave executable"
            );
            eprintln!("Skipping desktop typed N-D integration: GNU Octave was not found.");
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let (requests, events) = spawn(eframe::egui::Context::default());
        requests
            .send(Request::Start {
                executable: Some(executable),
                startup_config: None,
                packaged_startup: false,
                directory: directory.path().into(),
            })
            .unwrap();
        let cancel = loop {
            match events.recv_timeout(Duration::from_secs(30)).unwrap() {
                Event::ConsoleConnected { .. } => {}
                Event::Ready { cancel, .. } => break cancel,
                Event::Unavailable(error) => panic!("desktop worker startup failed: {error}"),
                _ => panic!("unexpected desktop worker startup event"),
            }
        };
        struct TerminateOnDrop(CancellationHandle);
        impl Drop for TerminateOnDrop {
            fn drop(&mut self) {
                let _ = self.0.terminate();
            }
        }
        let _terminate = TerminateOnDrop(cancel);

        let execute = |source: &str| {
            requests.send(Request::Execute(source.into())).unwrap();
            match events.recv_timeout(Duration::from_secs(10)).unwrap() {
                Event::Executed(result) => assert!(result.error.is_none(), "{:?}", result.error),
                _ => panic!("expected source completion"),
            }
        };
        let read = |request_id, token, plane| {
            requests
                .send(Request::ReadVariable {
                    request_id,
                    name: "A".into(),
                    token,
                    row_start: 1,
                    column_start: 1,
                    higher_indices: vec![plane],
                })
                .unwrap();
        };
        let typed_reply = |expected_id| match events.recv_timeout(Duration::from_secs(10)).unwrap()
        {
            Event::VariableRead {
                request_id,
                result: Ok(VariablePage::Typed { page, .. }),
            } => {
                assert_eq!(request_id, expected_id);
                *page
            }
            Event::VariableRead {
                result: Err(error), ..
            } => panic!("typed desktop read failed: {error}"),
            _ => panic!("expected a typed page, never a checked/preview downgrade"),
        };
        let assert_plane = |page: &ValuePage, plane: u64, first: i32| {
            assert_eq!(page.token.name, "A");
            assert!(page.path.is_empty());
            assert_eq!(page.class, NumericClass::Double);
            assert_eq!(page.source_storage, SourceStorage::Dense);
            assert!(!page.complex);
            assert_eq!(
                page.source_shape
                    .iter()
                    .map(|extent| extent.get())
                    .collect::<Vec<_>>(),
                [3, 4, 5]
            );
            assert_eq!(page.selection.row_start.get(), 1);
            assert_eq!(page.selection.column_start.get(), 1);
            assert_eq!((page.selection.rows, page.selection.columns), (3, 4));
            assert_eq!(
                page.selection
                    .higher_indices
                    .iter()
                    .map(|index| index.get())
                    .collect::<Vec<_>>(),
                [plane]
            );
            let expected: Vec<f64> = (first..first + 12).map(f64::from).collect();
            match &page.data {
                PageData::Dense {
                    real: NumericComponents::Double(values),
                    imag: None,
                } => {
                    assert_eq!(values, &expected, "dense payload must stay column-major");
                    assert_eq!(values[1 + 2 * 3], f64::from(first + 7));
                }
                _ => panic!("expected an exact real double dense plane"),
            }
        };

        execute("A=reshape(1:60,[3,4,5]);");
        read(1, None, 4);
        let fourth = typed_reply(1);
        assert_plane(&fourth, 4, 37);
        let original = fourth.token.clone();
        read(2, Some(original.clone()), 2);
        let second = typed_reply(2);
        assert_plane(&second, 2, 13);
        assert_eq!(
            second.token, original,
            "pure plane reads must preserve the complete binding context"
        );

        execute("A=reshape(101:160,[3,4,5]);");
        read(3, Some(original.clone()), 4);
        match events.recv_timeout(Duration::from_secs(10)).unwrap() {
            Event::VariableRead {
                request_id: 3,
                result: Err(error),
            } => assert!(error.contains("binding changed"), "{error}"),
            _ => panic!("a stale typed read must fail without refresh or preview fallback"),
        }
        read(4, None, 4);
        let replacement = typed_reply(4);
        assert_plane(&replacement, 4, 137);
        assert_ne!(replacement.token.context, original.context);

        execute("A=reshape(201:212,[3,4]);");
        read(5, None, 4);
        match events.recv_timeout(Duration::from_secs(10)).unwrap() {
            Event::VariableRead {
                request_id: 5,
                result:
                    Ok(VariablePage::MetadataOnly {
                        variable,
                        diagnostic,
                    }),
            } => {
                assert_eq!(variable.name, "A");
                assert_eq!(variable.dimensions, [3, 4]);
                assert!(variable.dimensions_known);
                assert!(!diagnostic.is_empty());
            }
            _ => panic!("an obsolete no-token selector must yield fresh metadata without a page"),
        }
        assert!(
            events.try_recv().is_err(),
            "metadata recovery must not enqueue an automatic reread"
        );
        requests
            .send(Request::ReadVariable {
                request_id: 6,
                name: "A".into(),
                token: None,
                row_start: 1,
                column_start: 1,
                higher_indices: Vec::new(),
            })
            .unwrap();
        let corrected = typed_reply(6);
        assert_eq!(
            corrected
                .source_shape
                .iter()
                .map(|extent| extent.get())
                .collect::<Vec<_>>(),
            [3, 4]
        );
        assert!(corrected.selection.higher_indices.is_empty());
        assert_eq!(
            (corrected.selection.rows, corrected.selection.columns),
            (3, 4)
        );
        assert_ne!(corrected.token.context, replacement.token.context);
        assert!(matches!(&corrected.data, PageData::Dense {
            real: NumericComponents::Double(values), imag: None
        } if values == &(201..=212).map(f64::from).collect::<Vec<_>>()));
    }

    #[test]
    fn desktop_worker_exposes_non_newline_output_before_the_request_completes() {
        use mathlab_engine::ConsoleEventKind;
        use std::time::{Duration, Instant};
        let Some(executable) = discover_octave() else {
            eprintln!("Skipping desktop worker streaming integration: GNU Octave was not found.");
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let (requests, events) = spawn(eframe::egui::Context::default());
        requests
            .send(Request::Start {
                executable: Some(executable),
                startup_config: None,
                packaged_startup: false,
                directory: directory.path().into(),
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut subscription = None;
        let cancel = loop {
            match events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap()
            {
                Event::ConsoleConnected {
                    subscription: connected,
                    ..
                } => subscription = Some(connected),
                Event::Ready { cancel, .. } => break cancel,
                Event::Unavailable(error) => panic!("desktop worker startup failed: {error}"),
                _ => panic!("unexpected desktop worker startup event"),
            }
        };
        struct TerminateOnDrop(CancellationHandle);
        impl Drop for TerminateOnDrop {
            fn drop(&mut self) {
                let _ = self.0.terminate();
            }
        }
        let _terminate = TerminateOnDrop(cancel);
        let subscription = subscription.expect("console subscription must precede Ready");
        requests.send(Request::Execute(
            "fprintf('STREAM_BEFORE_WAIT'); while (!exist('release-console-test', 'file')); pause(.01); endwhile; fprintf('STREAM_AFTER_WAIT');".into()
        )).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut bytes = Vec::new();
        while !bytes
            .windows(b"STREAM_BEFORE_WAIT".len())
            .any(|part| part == b"STREAM_BEFORE_WAIT")
        {
            let event = subscription
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            if let ConsoleEventKind::Output { bytes: chunk } = event.kind {
                bytes.extend_from_slice(&chunk);
            }
        }
        assert!(
            matches!(events.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)),
            "completion must not arrive while source still waits for the test gate"
        );
        std::fs::write(directory.path().join("release-console-test"), b"continue").unwrap();
        match events.recv_timeout(Duration::from_secs(10)).unwrap() {
            Event::Executed(result) => {
                assert!(result.error.is_none());
                assert!(
                    result
                        .output
                        .contains("STREAM_BEFORE_WAITSTREAM_AFTER_WAIT")
                );
                assert!(result.console_end_sequence > 0);
            }
            _ => panic!("expected completed execution after releasing the test gate"),
        }
    }

    struct InputRuntime {
        requests: Sender<Request>,
        events: Receiver<Event>,
        input: mathlab_engine::InputHandle,
        cancel: CancellationHandle,
        directory: tempfile::TempDir,
    }

    impl Drop for InputRuntime {
        fn drop(&mut self) {
            let _ = self.cancel.terminate();
        }
    }

    fn input_runtime() -> Option<InputRuntime> {
        use std::time::Duration;
        if std::env::var_os("MATHLAB_INPUT_BRIDGE").is_none() {
            eprintln!(
                "Skipping desktop input integration: set MATHLAB_INPUT_BRIDGE to the locally compiled adapter."
            );
            return None;
        }
        let Some(executable) = discover_octave() else {
            eprintln!("Skipping desktop input integration: GNU Octave was not found.");
            return None;
        };
        let directory = tempfile::tempdir().unwrap();
        let (requests, events) = spawn(eframe::egui::Context::default());
        requests
            .send(Request::Start {
                executable: Some(executable),
                startup_config: None,
                packaged_startup: false,
                directory: directory.path().into(),
            })
            .unwrap();
        loop {
            match events.recv_timeout(Duration::from_secs(30)).unwrap() {
                Event::ConsoleConnected { .. } => {}
                Event::Ready { cancel, input, .. } => {
                    return Some(InputRuntime {
                        requests,
                        events,
                        input: input.expect(
                            "input adapter was requested but no handle was handed to the desktop",
                        ),
                        cancel,
                        directory,
                    });
                }
                Event::Unavailable(error) => panic!("input worker startup failed: {error}"),
                _ => panic!("unexpected input worker startup event"),
            }
        }
    }

    fn wait_input(runtime: &InputRuntime, prompt: &str) -> mathlab_engine::InputRequest {
        use std::time::{Duration, Instant};
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(pending) = runtime.input.pending().unwrap() {
                if matches!(pending.phase, mathlab_engine::InputPhase::Waiting)
                    && pending.request.prompt.contains(prompt)
                {
                    return pending.request;
                }
            }
            assert!(
                Instant::now() < deadline,
                "input request {prompt} did not arrive"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn completed(runtime: &InputRuntime) -> ExecutionResult {
        match runtime
            .events
            .recv_timeout(std::time::Duration::from_secs(15))
            .unwrap()
        {
            Event::Executed(result) => result,
            Event::SourceFailed(error) => panic!("execution failed outside result: {error:?}"),
            Event::Failed(error) | Event::Disconnected(error) => {
                panic!("execution failed: {error}")
            }
            _ => panic!("unexpected execution event"),
        }
    }

    #[test]
    fn input_replies_and_eof_bypass_the_busy_worker_and_stop_preserves_the_session() {
        let Some(runtime) = input_runtime() else {
            return;
        };
        runtime.requests.send(Request::Execute(
            "prefix=7; text=input('STRING_PROMPT> ','s'); empty=input('EMPTY_PROMPT> ','s'); number=input('EXPR_PROMPT> '); assert(strcmp(text,'  input λ  ')); assert(isempty(empty)); assert(number==prefix+2);".into()
        )).unwrap();
        let first = wait_input(&runtime, "STRING_PROMPT>");
        assert!(matches!(first.mode, mathlab_engine::InputMode::String));
        assert!(matches!(
            runtime.events.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        runtime.input.reply(&first.key, "  input λ  ").unwrap();
        let second = wait_input(&runtime, "EMPTY_PROMPT>");
        runtime.input.reply(&second.key, "").unwrap();
        let expression = wait_input(&runtime, "EXPR_PROMPT>");
        assert!(matches!(
            expression.mode,
            mathlab_engine::InputMode::Expression
        ));
        runtime.input.reply(&expression.key, "prefix + 2").unwrap();
        let result = completed(&runtime);
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(
            result.output.matches("  input λ  ").count(),
            1,
            "native echo must appear once"
        );
        assert!(runtime.input.reply(&first.key, "stale").is_err());
        runtime
            .requests
            .send(Request::Execute(
                "eof_value=input('EOF_PROMPT> ','s');".into(),
            ))
            .unwrap();
        let eof = wait_input(&runtime, "EOF_PROMPT>");
        runtime.input.eof(&eof.key).unwrap();
        let result = completed(&runtime);
        assert!(
            result.error.is_some(),
            "EOF should retain the native input() error behavior"
        );
        assert!(runtime.input.pending().unwrap().is_none());
        runtime
            .requests
            .send(Request::Execute(
                "input_before_stop=11; input('STOP_PROMPT> ','s');".into(),
            ))
            .unwrap();
        wait_input(&runtime, "STOP_PROMPT>");
        runtime.cancel.interrupt().unwrap();
        let result = completed(&runtime);
        assert_eq!(
            result.error.as_ref().map(|error| error.identifier.as_str()),
            Some("MathLab:Interrupted")
        );
        assert!(runtime.input.pending().unwrap().is_none());
        runtime
            .requests
            .send(Request::Execute(
                "assert(prefix==7 && input_before_stop==11); survived_input_stop=true;".into(),
            ))
            .unwrap();
        assert!(completed(&runtime).error.is_none());
    }

    #[test]
    fn paused_scope_input_is_answered_without_queuing_another_engine_request() {
        use std::time::{Duration, Instant};
        let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
            eprintln!("Skipping paused input integration: set MATHLAB_DEBUG_BRIDGE.");
            return;
        };
        let Some(runtime) = input_runtime() else {
            return;
        };
        let path = runtime.directory.path().join("desktop_paused_input.m");
        std::fs::write(&path, "function value=desktop_paused_input(x)\n local_value=x+1;\n value=local_value*2;\nend\n").unwrap();
        runtime
            .requests
            .send(Request::EnableDebugger(bridge.into()))
            .unwrap();
        let debugger = match runtime
            .events
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
        {
            Event::DebuggerReady(debugger) => debugger,
            Event::DebuggerUnavailable { error, .. } => panic!("debugger enable failed: {error}"),
            _ => panic!("unexpected debugger event"),
        };
        assert_eq!(debugger.set_breakpoint(&path, 3).unwrap(), vec![3]);
        runtime
            .requests
            .send(Request::Execute(
                "paused_result=desktop_paused_input(5);".into(),
            ))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let state = loop {
            if let Some(state) = debugger.state().unwrap().filter(|state| state.paused) {
                break state;
            }
            assert!(Instant::now() < deadline, "source did not pause");
            std::thread::sleep(Duration::from_millis(10));
        };
        let evaluator = debugger.clone();
        let revision = state.revision;
        let evaluation = std::thread::spawn(move || {
            evaluator.evaluate_output(
                "reply_value=input('PAUSED_INPUT> '); disp(reply_value);",
                revision,
            )
        });
        let request = wait_input(&runtime, "PAUSED_INPUT>");
        runtime
            .input
            .reply(&request.key, "local_value + 3")
            .unwrap();
        let output = evaluation.join().unwrap().unwrap();
        assert!(
            output.output.contains('9'),
            "expression input lost its paused caller scope: {output:?}"
        );
        assert!(
            output.streamed,
            "interactive evaluation should use the streamed protocol"
        );
        debugger
            .resume(mathlab_debugger::ResumeAction::Continue, revision)
            .unwrap();
        let result = completed(&runtime);
        assert!(result.error.is_none());
        assert!(
            result.output.contains('9'),
            "streamed evaluation output did not reach the live engine console"
        );
    }

    #[test]
    fn app_stop_interrupts_a_paused_evaluation_after_input_has_already_been_consumed() {
        use std::time::{Duration, Instant};
        let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
            eprintln!("Skipping paused input Stop integration: set MATHLAB_DEBUG_BRIDGE.");
            return;
        };
        let Some(runtime) = input_runtime() else {
            return;
        };
        let source = runtime.directory.path().join("desktop_stop_input.m");
        std::fs::write(
            &source,
            "function value=desktop_stop_input(x)\n local_value=x+1;\n value=local_value*2;\nend\n",
        )
        .unwrap();
        std::fs::write(runtime.directory.path().join("desktop_input_long.m"),
            "function value=desktop_input_long(x)\n fid=fopen('entered-long-input-expression','w'); fprintf(fid,'%g',x); fclose(fid);\n while(true); pause(.01); endwhile\n value=x;\nend\n").unwrap();
        runtime
            .requests
            .send(Request::EnableDebugger(bridge.into()))
            .unwrap();
        let debugger = match runtime
            .events
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
        {
            Event::DebuggerReady(debugger) => debugger,
            Event::DebuggerUnavailable { error, .. } => panic!("debugger enable failed: {error}"),
            _ => panic!("unexpected debugger event"),
        };
        assert_eq!(debugger.set_breakpoint(&source, 3).unwrap(), vec![3]);
        runtime
            .requests
            .send(Request::Execute(
                "stop_base=19; paused_result=desktop_stop_input(5);".into(),
            ))
            .unwrap();
        let mut test = crate::lifecycle_tests::harness();
        // Source navigation lays out the paused line, as the first app frame
        // does in production. Initialize egui's configured fonts first.
        let _ = test.ctx.run(crate::egui::RawInput::default(), |_| {});
        test.app.cancellation = Some(runtime.cancel.clone());
        test.app.busy = true;
        test.app.debugger = Some(crate::debugging::DebuggerPanel::new(
            debugger,
            test.ctx.clone(),
        ));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            test.app.debugger_updates(&test.ctx);
            if test
                .app
                .debugger
                .as_ref()
                .is_some_and(|panel| panel.paused())
            {
                break;
            }
            assert!(Instant::now() < deadline, "source did not pause");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            test.app
                .debugger
                .as_mut()
                .unwrap()
                .evaluate_source("reply_value=input('PAUSED_LONG_INPUT> ');".into())
                .is_some()
        );
        let request = wait_input(&runtime, "PAUSED_LONG_INPUT>");
        runtime
            .input
            .reply(&request.key, "desktop_input_long(local_value)")
            .unwrap();
        let marker = runtime
            .directory
            .path()
            .join("entered-long-input-expression");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !marker.exists() || runtime.input.pending().unwrap().is_some() {
            assert!(
                Instant::now() < deadline,
                "the entered expression did not begin after input consumption"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "6");
        assert!(!test.app.input.active());
        assert!(test.app.debugger.as_ref().unwrap().evaluation_pending());
        test.app.stop();
        assert_eq!(test.app.status, "Interrupting…");
        let result = completed(&runtime);
        assert_eq!(
            result.error.as_ref().map(|error| error.identifier.as_str()),
            Some("MathLab:Interrupted")
        );
        test.events.send(Event::Executed(result)).unwrap();
        test.app.poll_events(&test.ctx);
        let deadline = Instant::now() + Duration::from_secs(3);
        while test.app.debugger.as_ref().unwrap().evaluation_pending() {
            test.app.debugger_updates(&test.ctx);
            assert!(
                Instant::now() < deadline,
                "interrupted evaluation did not publish its terminal debugger response"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!test.app.busy);
        assert!(test.app.connected);
        assert!(
            test.requests.try_recv().is_err(),
            "Stop must not submit a normal source request"
        );
        runtime
            .requests
            .send(Request::Execute(
                "assert(stop_base==19); survived_paused_input_stop=true;".into(),
            ))
            .unwrap();
        assert!(completed(&runtime).error.is_none());
    }

    /// Authored, unrun real-worker actor journey. Integer-only fixtures retain
    /// portable coverage when the optional native complex adapter is absent.
    #[test]
    fn desktop_range_actor_routes_exact_nd_paste_history_and_stale_reset_without_retry() {
        use crate::range_paste::{PasteLimits, PasteTarget, parse_range_paste};
        use crate::range_transactions::RangeTransactions;
        use mathlab_engine::value_edit::EditOperation;
        use mathlab_engine::{NumericComponents, PageData, ValuePage};
        use std::time::Duration;

        let Some(executable) = discover_octave() else {
            assert_ne!(
                std::env::var("MATHLAB_REQUIRE_DESKTOP_RANGE_TESTS").as_deref(),
                Ok("1"),
                "required desktop range actor test needs installed Octave"
            );
            eprintln!("SKIP desktop range actor test: no installed Octave.");
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let (requests, events) = spawn(eframe::egui::Context::default());
        requests
            .send(Request::Start {
                executable: Some(executable),
                startup_config: None,
                packaged_startup: false,
                directory: directory.path().into(),
            })
            .unwrap();
        let cancel = loop {
            match events.recv_timeout(Duration::from_secs(30)).unwrap() {
                Event::ConsoleConnected { .. } => {}
                Event::Ready { cancel, .. } => break cancel,
                Event::Unavailable(error) => panic!("worker startup: {error}"),
                _ => panic!("unexpected startup event"),
            }
        };
        struct TerminateOnDrop(CancellationHandle);
        impl Drop for TerminateOnDrop {
            fn drop(&mut self) {
                let _ = self.0.terminate();
            }
        }
        let _terminate = TerminateOnDrop(cancel);
        let read = |request_id| {
            requests
                .send(Request::ReadVariable {
                    request_id,
                    name: "A".into(),
                    token: None,
                    row_start: 1,
                    column_start: 1,
                    higher_indices: vec![4],
                })
                .unwrap();
            match events.recv_timeout(Duration::from_secs(15)).unwrap() {
                Event::VariableRead {
                    request_id: actual,
                    result: Ok(VariablePage::Typed { page, .. }),
                } => {
                    assert_eq!(actual, request_id);
                    *page
                }
                _ => panic!("expected typed N-D page"),
            }
        };
        let operation = |envelope: &TransactionEnvelope| {
            requests
                .send(Request::RangeTransaction(Box::new(envelope.clone())))
                .unwrap();
            match events.recv_timeout(Duration::from_secs(20)).unwrap() {
                Event::RangeTransactionFinished {
                    envelope: echoed,
                    outcome,
                } => {
                    assert_eq!(echoed.as_ref(), envelope);
                    *outcome
                }
                _ => panic!("expected range actor response"),
            }
        };
        let values = |page: &ValuePage| match &page.data {
            PageData::Dense {
                real: NumericComponents::Uint64(values),
                imag: None,
            } => values.clone(),
            _ => panic!("expected exact uint64 components"),
        };
        requests
            .send(Request::Execute("A=reshape(uint64(1:60),[3,4,5]);".into()))
            .unwrap();
        match events.recv_timeout(Duration::from_secs(15)).unwrap() {
            Event::Executed(result) => assert!(result.error.is_none()),
            _ => panic!("expected initialization"),
        }
        let before = read(1);
        assert_eq!(values(&before), (37..=48).collect::<Vec<_>>());
        let mut state = RangeTransactions::default();
        state.observe_page(1, &before);
        let target = |page: &ValuePage| {
            let mut selection = page.selection.clone();
            selection.rows = 2;
            selection.columns = 2;
            PasteTarget {
                path: ValuePath {
                    root: page.token.clone(),
                    steps: page.path.clone(),
                },
                class: page.class,
                source_shape: page.source_shape.clone(),
                source_storage: page.source_storage,
                complex: page.complex,
                selection,
            }
        };
        let pasted = parse_range_paste(
            "9007199254740993\t18446744073709551615\n7\t8",
            &target(&before),
            PasteLimits::default(),
        )
        .unwrap();
        let command = state.begin_paste(2, pasted).unwrap();
        let Some(TransactionOutcome::Applied {
            receipt,
            refresh: Ok(refresh),
        }) = state.finish(&command, operation(&command))
        else {
            panic!("paste failed");
        };
        assert_eq!(
            receipt.token.context.revision,
            before.token.context.revision + 1
        );
        let data = values(&refresh.page);
        assert_eq!(
            [data[0], data[1], data[3], data[4]],
            [9_007_199_254_740_993, 7, u64::MAX, 8]
        );
        assert_eq!(refresh.page.selection.higher_indices, vec![4.into()]);
        state.observe_page(2, &refresh.page);
        let undo = state.begin_history(3, EditOperation::Undo).unwrap();
        let Some(TransactionOutcome::Applied {
            refresh: Ok(refresh),
            ..
        }) = state.finish(&undo, operation(&undo))
        else {
            panic!("undo failed");
        };
        assert_eq!(values(&refresh.page), values(&before));
        state.observe_page(3, &refresh.page);
        let redo = state.begin_history(4, EditOperation::Redo).unwrap();
        let Some(TransactionOutcome::Applied {
            refresh: Ok(refresh),
            ..
        }) = state.finish(&redo, operation(&redo))
        else {
            panic!("redo failed");
        };
        let data = values(&refresh.page);
        assert_eq!(
            [data[0], data[1], data[3], data[4]],
            [9_007_199_254_740_993, 7, u64::MAX, 8]
        );
        state.observe_page(4, &refresh.page);
        let stale_page = refresh.page;
        let parsed =
            parse_range_paste("1\t2\n3\t4", &target(&stale_page), PasteLimits::default()).unwrap();
        let stale = state.begin_paste(5, parsed).unwrap();
        // Deliberately queue an external replacement before the old-token write.
        requests
            .send(Request::Execute("A=zeros(3,4,5,'uint64');".into()))
            .unwrap();
        requests
            .send(Request::RangeTransaction(Box::new(stale.clone())))
            .unwrap();
        match events.recv_timeout(Duration::from_secs(15)).unwrap() {
            Event::Executed(result) => assert!(result.error.is_none()),
            _ => panic!("expected replacement before stale edit"),
        }
        let failed = match events.recv_timeout(Duration::from_secs(15)).unwrap() {
            Event::RangeTransactionFinished { envelope, outcome } => {
                assert_eq!(*envelope, stale);
                let TransactionOutcome::Failed(failure) = *outcome else {
                    panic!("old-token edit unexpectedly succeeded");
                };
                assert!(!failure.uncertain);
                failure
            }
            _ => panic!("old-token edit must fail without a preview or refreshed retry"),
        };
        assert!(failed.message.contains("binding"));
        state.finish(&stale, TransactionOutcome::Failed(failed));
        assert!(
            events.try_recv().is_err(),
            "no automatic edit retry/refresh after rejection"
        );
        let fresh = read(6);
        assert!(values(&fresh).iter().all(|value| *value == 0));
        let stale_reset = TransactionEnvelope {
            request_id: 7,
            page: crate::range_transactions::PageIdentity::accepted(4, &stale_page),
            command: TransactionCommand::Reset {
                expected: stale_page.token.context.clone(),
            },
        };
        assert!(matches!(
            operation(&stale_reset),
            TransactionOutcome::Failed(TransactionFailure {
                uncertain: false,
                ..
            })
        ));
        let after_rejection = read(8);
        assert_eq!(
            after_rejection.token.context, fresh.token.context,
            "stale reset must not revoke current bindings/history"
        );
        state.observe_page(8, &after_rejection);
        let reset = state.begin_reset(9).unwrap();
        let Some(TransactionOutcome::Reset(snapshot)) = state.finish(&reset, operation(&reset))
        else {
            panic!("explicit fresh reset failed");
        };
        assert_eq!(snapshot.context.revision, fresh.token.context.revision + 1);
        assert!(state.history().is_none());
        let current = read(10);
        state.observe_page(10, &current);
        let parsed =
            parse_range_paste("1\t2\n3\t4", &target(&current), PasteLimits::default()).unwrap();
        let dead_transport = state.begin_paste(11, parsed).unwrap();
        _terminate.0.terminate().unwrap();
        let outcome = operation(&dead_transport);
        assert!(
            matches!(
                &outcome,
                TransactionOutcome::Failed(TransactionFailure {
                    uncertain: true,
                    disconnected: true,
                    ..
                })
            ),
            "wrapped edit transport failure must retain typed closed-session state"
        );
        state.finish(&dead_transport, outcome);
        assert!(state.quarantine().is_some());
        assert!(events.try_recv().is_err(), "no retry after dead transport");
    }
}
