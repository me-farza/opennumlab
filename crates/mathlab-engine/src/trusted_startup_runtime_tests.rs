// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicitly selected, already-built Octave 11.3.0/api-v61 native V2 fixture.
//! Hashes identify this harness selection; they do not authenticate a vendor.
//! No discovery, compilation or legacy bridge environment. The included graphics
//! fixture additionally requires an explicitly selected graphics adapter.

use super::*;
use crate::startup_config::QualifiedStartupConfig;
use crate::value_edit::{LeafDescriptor, RangeEditRequest};
use sha2::{Digest, Sha256};
use std::sync::Condvar;
use std::thread;

const CASE_BUDGET: Duration = Duration::from_secs(120);
const EXECUTION_BUDGET: Duration = Duration::from_secs(20);
const ROLES: [(AdapterRole, &str); 4] = [
    (AdapterRole::Input, "MATHLAB_QUALIFIED_TEST_INPUT_ADAPTER"),
    (AdapterRole::Scope, "MATHLAB_QUALIFIED_TEST_SCOPE_ADAPTER"),
    (AdapterRole::Value, "MATHLAB_QUALIFIED_TEST_VALUE_ADAPTER"),
    (AdapterRole::Debug, "MATHLAB_QUALIFIED_TEST_DEBUG_ADAPTER"),
];

fn prerequisite<T, E: fmt::Display>(value: std::result::Result<T, E>, label: &str) -> T {
    value.unwrap_or_else(|error| panic!("qualified runtime prerequisite: {label}: {error}"))
}

fn selected_path(label: &str, directory: bool) -> PathBuf {
    let raw = PathBuf::from(prerequisite(std::env::var(label), label));
    checked_selected_path(raw, label, directory)
}

fn checked_selected_path(raw: PathBuf, label: &str, directory: bool) -> PathBuf {
    assert!(
        raw.is_absolute(),
        "qualified runtime prerequisite: {label} must be absolute"
    );
    let canonical = prerequisite(fs::canonicalize(&raw), label);
    assert_eq!(
        raw, canonical,
        "qualified runtime prerequisite: {label} must already have canonical spelling"
    );
    let metadata = prerequisite(fs::symlink_metadata(&raw), label);
    assert!(
        if directory {
            metadata.is_dir()
        } else {
            metadata.is_file()
        },
        "qualified runtime prerequisite: {label} has the wrong file type"
    );
    raw
}

fn pin(label: &str) -> PinnedFile {
    let path = selected_path(label, false);
    let mut file = prerequisite(File::open(&path), label);
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 16_384];
    loop {
        let count = prerequisite(file.read(&mut buffer), label);
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    PinnedFile {
        path,
        sha256: hash.finalize().into(),
    }
}

fn selected_config() -> TrustedStartupConfig {
    let label = "MATHLAB_QUALIFIED_TEST_RUNTIME_DIRS_JSON";
    let raw = prerequisite(std::env::var(label), label);
    let directories: Vec<String> = prerequisite(serde_json::from_str(&raw), label);
    assert!(
        !directories.is_empty() && directories.len() <= 511,
        "qualified runtime prerequisite: {label} needs 1..=511 explicit runtime directories"
    );
    TrustedStartupConfig {
        installation_root: selected_path("MATHLAB_QUALIFIED_TEST_INSTALLATION_ROOT", true),
        executable: pin("MATHLAB_QUALIFIED_TEST_EXECUTABLE"),
        provider: pin("MATHLAB_QUALIFIED_TEST_PROVIDER"),
        gnuplot: pin("MATHLAB_QUALIFIED_TEST_GNUPLOT"),
        ghostscript: pin("MATHLAB_QUALIFIED_TEST_GHOSTSCRIPT"),
        runtime_directories: directories
            .into_iter()
            .map(|path| checked_selected_path(PathBuf::from(path), label, true))
            .collect(),
        adapters: ROLES
            .into_iter()
            .map(|(role, label)| AdapterConfig {
                role,
                file: pin(label),
            })
            .collect(),
        octave_version: "11.3.0".into(),
        octave_api_version: "api-v61".into(),
    }
}

fn adapter(config: &TrustedStartupConfig, role: AdapterRole) -> &Path {
    &config
        .adapters
        .iter()
        .find(|entry| entry.role == role)
        .unwrap()
        .file
        .path
}

// Wakes early on success/panic, and kills only this Engine's child on timeout.
// The external driver owns the global wall/RSS limits, including prerequisite IO
// and the constructor's own bounded first-Ready wait.
struct RuntimeBudget {
    cancellation: CancellationHandle,
    done: Arc<(Mutex<bool>, Condvar)>,
    expired: Arc<AtomicBool>,
    task: Option<thread::JoinHandle<()>>,
}

impl RuntimeBudget {
    fn new(engine: &Engine, started: Instant) -> Self {
        let remaining = CASE_BUDGET.checked_sub(started.elapsed()).expect(
            "qualified runtime fixture exhausted its 120-second case budget during startup",
        );
        let cancellation = engine.cancellation_handle();
        let done = Arc::new((Mutex::new(false), Condvar::new()));
        let expired = Arc::new(AtomicBool::new(false));
        let watched_done = Arc::clone(&done);
        let watched_expired = Arc::clone(&expired);
        let watched_child = cancellation.clone();
        let task = thread::spawn(move || {
            let (lock, changed) = &*watched_done;
            let (done, _) = changed
                .wait_timeout_while(lock.lock().unwrap(), remaining, |done| !*done)
                .unwrap();
            if !*done {
                watched_expired.store(true, Ordering::Release);
                let _ = watched_child.terminate();
            }
        });
        Self {
            cancellation,
            done,
            expired,
            task: Some(task),
        }
    }
}

impl Drop for RuntimeBudget {
    fn drop(&mut self) {
        // This also bounds cleanup if an assertion fails while Engine is owned
        // by a request thread rather than by the test's stack.
        let _ = self.cancellation.terminate();
        let (lock, changed) = &*self.done;
        if let Ok(mut done) = lock.lock() {
            *done = true;
            changed.notify_all();
        }
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

fn execute(engine: &mut Engine, source: &str) {
    let result = engine
        .execute_with_timeout(source, EXECUTION_BUDGET)
        .unwrap();
    assert!(
        result.error.is_none(),
        "qualified user execution failed: {:?}",
        result.error
    );
    assert!(
        result.figures.is_empty(),
        "core fixture unexpectedly created user graphics"
    );
}

fn window(columns: u32) -> PageWindow {
    PageWindow {
        row_start: 1.into(),
        column_start: 1.into(),
        rows: 1,
        columns,
        higher_indices: vec![],
    }
}

fn root_page(engine: &mut Engine, name: &str, columns: u32) -> ValuePage {
    let snapshot = engine.workspace_snapshot().unwrap();
    engine.validate_context(&snapshot.context).unwrap();
    let token = snapshot
        .variables
        .into_iter()
        .find(|entry| entry.variable.name == name)
        .unwrap_or_else(|| panic!("qualified runtime omitted binding {name}"))
        .token;
    engine
        .read_value_page_checked(&ValuePageRequest {
            path: ValuePath {
                root: token,
                steps: vec![],
            },
            window: window(columns),
        })
        .unwrap()
}

fn assert_real(engine: &mut Engine, name: &str, expected: f64) {
    let page = root_page(engine, name, 1);
    assert_eq!(page.class, NumericClass::Double);
    assert_eq!(page.source_storage, SourceStorage::Dense);
    assert_eq!(
        page.data,
        PageData::Dense {
            real: NumericComponents::Double(vec![expected]),
            imag: None,
        }
    );
}

fn assert_complex(page: &ValuePage, expected_real: &[f64], expected_imag: &[f64]) {
    assert_eq!(page.class, NumericClass::Double);
    assert_eq!(page.source_shape, vec![1.into(), 2.into()]);
    assert_eq!(page.source_storage, SourceStorage::Dense);
    assert!(page.complex);
    let PageData::Dense {
        real: NumericComponents::Double(real),
        imag: Some(NumericComponents::Double(imag)),
    } = &page.data
    else {
        panic!(
            "native complex page lost its typed dense components: {:?}",
            page.data
        );
    };
    assert_eq!(
        real.iter().map(|value| value.to_bits()).collect::<Vec<_>>(),
        expected_real
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        imag.iter().map(|value| value.to_bits()).collect::<Vec<_>>(),
        expected_imag
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>()
    );
}

fn assert_ready(engine: &mut Engine, config: &TrustedStartupConfig) -> SessionContext {
    let (ready, _) = engine
        .request(json!({"command":"ping"}), Some(COMMAND_TIMEOUT))
        .unwrap();
    // These are genuine, unmodified worker bytes. Never construct or substitute
    // a synthetic status/profile to make the selected runtime pass qualification.
    engine.accept_ready(&ready).unwrap();
    let status: qualified_ready::IntrinsicsReady =
        serde_json::from_value(ready["intrinsics"].clone()).unwrap();
    assert_eq!(
        status,
        engine.qualified_ready_profile.as_ref().unwrap().expected
    );
    assert_eq!(status.provider_file, config.provider.path.to_str().unwrap());
    assert_eq!(
        status.worker_file,
        engine.worker_source_path().to_str().unwrap()
    );
    assert_eq!(status.adapters.len(), 4);
    for (role, name) in [
        (AdapterRole::Input, "input"),
        (AdapterRole::Scope, "scope"),
        (AdapterRole::Value, "value"),
        (AdapterRole::Debug, "debug"),
    ] {
        assert_eq!(
            status.adapters[name].path,
            adapter(config, role).to_str().unwrap()
        );
    }
    assert!(engine.is_transport_open());
    engine.require_capability("binding_snapshot.v1").unwrap();
    let context: SessionContext = serde_json::from_value(ready["context"].clone()).unwrap();
    assert_eq!(engine.qualified_ready_context.as_ref(), Some(&context));
    engine.validate_context(&context).unwrap();
    context
}

fn assert_enabled(engine: &Engine, config: &TrustedStartupConfig) {
    assert_eq!(
        engine.input_bridge_path(),
        Some(adapter(config, AdapterRole::Input))
    );
    assert_eq!(
        engine.scope_bridge_path(),
        Some(adapter(config, AdapterRole::Scope))
    );
    assert_eq!(
        engine.value_bridge_path(),
        Some(adapter(config, AdapterRole::Value))
    );
    assert_eq!(
        engine.debug_bridge.as_deref(),
        Some(adapter(config, AdapterRole::Debug))
    );
    assert!(engine.input_handle().is_some());
    assert!(engine.debugger_handle().is_some());
    engine
        .require_capability("dense_complex_native_components.v1")
        .unwrap();
}

fn join_execution(
    task: thread::JoinHandle<(Engine, Result<ExecutionResult>)>,
    cancellation: &CancellationHandle,
) -> (Engine, ExecutionResult) {
    let deadline = Instant::now() + EXECUTION_BUDGET;
    while !task.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    if !task.is_finished() {
        let _ = cancellation.terminate();
        panic!("qualified runtime request did not settle within 20 seconds");
    }
    let (engine, result) = task
        .join()
        .expect("qualified runtime request thread panicked");
    (engine, result.unwrap())
}

#[test]
#[ignore = "requires explicitly selected Octave 11.3/api-v61 and five already-built native modules"]
fn qualified_native_v2_runtime_survives_project_shadows_stop_and_restart() {
    let started = Instant::now();
    let config = selected_config(); // Missing prerequisites fail; never skip-as-pass.
    assert!(trusted_worker::REQUIRED_PRIMITIVES.contains(&"isobject"));
    assert!(trusted_worker::REQUIRED_PRIMITIVES.contains(&"signbit"));
    let project_root = tempfile::Builder::new()
        .prefix("mathlab-qualified-project-")
        .tempdir()
        .unwrap();
    let project = project_root.path().join("résumé's project");
    fs::create_dir(&project).unwrap();
    let project = fs::canonicalize(project).unwrap();
    // Present before project entry and first Ready. A captured path lookup fails
    // loudly, including current genuine-object Ready checks and native loaders.
    for name in [
        "builtin",
        "isobject",
        "signbit",
        "mathlab_intrinsic_bridge",
        "mathlab_input_bridge",
        "mathlab_scope_bridge",
        "mathlab_value_bridge",
        "mathlab_debug_bridge",
        "mathlab_worker",
        "mathlab_scope_register",
    ] {
        fs::write(project.join(format!("{name}.m")), format!(
            "function varargout = {name} (varargin)\n error ('MathLabTest:ProjectShadow', 'owned transport resolved project {name}');\nendfunction\n"
        )).unwrap();
    }
    fs::write(project.join("qualified_user_probe.m"),
        "function value = qualified_user_probe ()\n persistent count = 0;\n count += 1;\n value = count;\nendfunction\n").unwrap();
    let mut engine = Engine::start_qualified(project.clone(), config.clone())
        .expect("qualified runtime prerequisite/startup: selected Octave/native V2 profile failed admission or first Ready");
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = fs::metadata(engine.session.path()).unwrap();
        assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
        assert_eq!(metadata.permissions().mode() & 0o077, 0);
    }
    let budget = RuntimeBudget::new(&engine, started);
    let first = engine
        .qualified_ready_context
        .clone()
        .expect("first qualified Ready missing");
    assert_eq!(engine.working_directory(), project.as_path());
    assert_eq!(
        (first.generation, first.invocation, first.revision),
        (1, 1, 0)
    );
    assert!(engine.input_bridge_path().is_none() && engine.scope_bridge_path().is_none());
    assert!(engine.value_bridge_path().is_none() && engine.debugger_handle().is_none());
    assert!(
        !engine
            .capabilities()
            .supports("dense_complex_native_components.v1")
    );
    assert_ready(&mut engine, &config);
    // Public post-Ready activation; the input API correctly precedes user code.
    engine
        .enable_input_adapter(adapter(&config, AdapterRole::Input))
        .unwrap();
    engine
        .enable_scope_bridge(adapter(&config, AdapterRole::Scope))
        .unwrap();
    engine
        .enable_value_bridge(adapter(&config, AdapterRole::Value))
        .unwrap();
    engine
        .enable_debugger(adapter(&config, AdapterRole::Debug))
        .unwrap();
    assert_enabled(&engine, &config);
    execute(
        &mut engine,
        "qualified_answer=6*7; qualified_first=qualified_user_probe(); qualified_second=qualified_user_probe(); qualified_complex=complex([1,4],[2,-3]); builtin=901; feval=902; whos=903;",
    );
    assert_real(&mut engine, "qualified_answer", 42.);
    assert_real(&mut engine, "qualified_first", 1.);
    assert_real(&mut engine, "qualified_second", 2.);
    let page = root_page(&mut engine, "qualified_complex", 2);
    assert_complex(&page, &[1., 4.], &[2., -3.]);
    let request = RangeEditRequest::from_components(
        ValuePath {
            root: page.token,
            steps: page.path,
        },
        window(2),
        LeafDescriptor {
            class: page.class,
            source_shape: page.source_shape,
            source_storage: page.source_storage,
            complex: page.complex,
        },
        &NumericComponents::Double(vec![7.25, -0.]),
        Some(&NumericComponents::Double(vec![-2., 3.])),
    )
    .unwrap();
    let receipt = engine.write_value_range_checked(&request).unwrap();
    let edited = root_page(&mut engine, "qualified_complex", 2);
    assert_eq!(edited.token, receipt.token);
    assert_complex(&edited, &[7.25, -0.], &[-2., 3.]);
    execute(
        &mut engine,
        "clear functions; qualified_after_clear=qualified_user_probe();",
    );
    assert_real(&mut engine, "qualified_after_clear", 1.);
    // Observe the actual installed Engine's launch/re-entry argument boundary:
    // a cold child's R0 must precede feature installs, while recovery must carry
    // every feature already installed in its surviving numerical process.
    {
        let cold = engine.trusted_worker_arguments(true);
        assert!(cold.announce_ready && !cold.debugger_enabled);
        assert!(
            cold.scope_bridge.is_none()
                && cold.input_bridge.is_none()
                && cold.value_bridge.is_none()
        );
        let recovery = engine.trusted_worker_arguments(false);
        assert!(!recovery.announce_ready && recovery.debugger_enabled);
        assert_eq!(
            recovery.scope_bridge,
            Some(adapter(&config, AdapterRole::Scope))
        );
        assert_eq!(
            recovery.input_bridge,
            Some(adapter(&config, AdapterRole::Input))
        );
        assert_eq!(
            recovery.value_bridge,
            Some(adapter(&config, AdapterRole::Value))
        );
    }
    let before_stop = assert_ready(&mut engine, &config);
    let pid = engine
        .process_id()
        .expect("qualified child missing before Stop");
    let cancellation = engine.cancellation_handle();
    let subscription = engine.subscribe_console();
    const MARKER: &[u8] = b"QUALIFIED_STOP_BODY_RUNNING";
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "qualified_stop_survivor=73; fprintf(1,'QUALIFIED_STOP_BODY_RUNNING'); while true; pause(.005); endwhile;",
            EXECUTION_BUDGET);
        (engine, result)
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut output = Vec::new();
    while !output.windows(MARKER.len()).any(|bytes| bytes == MARKER) {
        assert!(
            Instant::now() < deadline,
            "qualified Stop body did not begin"
        );
        if let Ok(event) = subscription.recv_timeout(Duration::from_millis(50))
            && let ConsoleEventKind::Output { bytes } = event.kind
        {
            output.extend(bytes);
            if output.len() > 4096 {
                output.drain(..output.len() - 4096);
            }
        }
        if task.is_finished() {
            let (engine, result) = task
                .join()
                .expect("qualified Stop task panicked before interruption");
            panic!(
                "qualified Stop body exited before interruption: result={result:?}; console={:?}; process_id={:?}; transport_open={}; watchdog_expired={}",
                String::from_utf8_lossy(&output),
                engine.process_id(),
                engine.is_transport_open(),
                budget.expired.load(Ordering::Acquire),
            );
        }
    }
    cancellation.interrupt().unwrap();
    let (mut engine, interrupted) = join_execution(task, &cancellation);
    assert_eq!(
        interrupted
            .error
            .as_ref()
            .map(|error| error.identifier.as_str()),
        Some("MathLab:Interrupted")
    );
    assert!(
        interrupted
            .output
            .contains(std::str::from_utf8(MARKER).unwrap())
    );
    assert_eq!(
        engine.process_id(),
        Some(pid),
        "Stop replaced the numerical process"
    );
    assert!(engine.validate_context(&before_stop).is_err());
    let recovered = assert_ready(&mut engine, &config);
    assert_eq!(recovered.generation, before_stop.generation);
    assert!(recovered.invocation > before_stop.invocation);
    assert_enabled(&engine, &config);
    assert_real(&mut engine, "qualified_stop_survivor", 73.);
    assert_complex(
        &root_page(&mut engine, "qualified_complex", 2),
        &[7.25, -0.],
        &[-2., 3.],
    );
    // A cold child first accepts its own genuine R0 before restoring all four
    // configured features. Observe that first completion on the public console.
    let restart_console = engine.subscribe_console();
    engine.restart().unwrap();
    assert_ne!(engine.process_id(), Some(pid));
    assert!(engine.validate_context(&recovered).is_err());
    let restarted = assert_ready(&mut engine, &config);
    assert_eq!(
        (restarted.generation, restarted.invocation),
        (recovered.generation + 1, 1)
    );
    let mut first_ready_ends = 0;
    while let Ok(event) = restart_console.try_recv() {
        if event.generation == restarted.generation
            && event.request_id == Some(0)
            && matches!(event.kind, ConsoleEventKind::End)
        {
            first_ready_ends += 1;
        }
    }
    assert_eq!(
        first_ready_ends, 1,
        "cold restart omitted/duplicated its first Ready completion"
    );
    assert_enabled(&engine, &config);
    let input = engine.input_handle().unwrap();
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout("qualified_restarted_input=input('qualified prompt: ', 's'); qualified_restart_probe=qualified_user_probe(); qualified_restart_answer=7*8;", EXECUTION_BUDGET);
        (engine, result)
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "restored qualified input adapter did not request input"
        );
        if let Some(pending) = input.pending().unwrap() {
            assert_eq!(pending.request.key.generation, restarted.generation);
            assert_eq!(pending.request.key.invocation, restarted.invocation);
            assert_eq!(pending.request.mode, InputMode::String);
            input.reply(&pending.request.key, "native-v2-ok").unwrap();
            break;
        }
        assert!(
            !task.is_finished(),
            "restarted input execution ended without requesting input"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let (mut engine, result) = join_execution(task, &cancellation);
    assert!(
        result.error.is_none(),
        "restored input failed: {:?}",
        result.error
    );
    assert_real(&mut engine, "qualified_restart_probe", 1.);
    assert_real(&mut engine, "qualified_restart_answer", 56.);
    // Compare in user code, then inspect a primitive result through the checked API.
    execute(
        &mut engine,
        "qualified_input_ok=double(strcmp(qualified_restarted_input,'native-v2-ok'));",
    );
    assert_real(&mut engine, "qualified_input_ok", 1.);
    assert_ready(&mut engine, &config);
    // Queue normal interpreter exit, then let the owned mailbox worker return.
    // Waiting for a successful child status before Drop exercises provider/debug
    // shutdown callbacks and alias teardown instead of relying on a kill.
    {
        let mut child = engine.child.lock().unwrap();
        let stdin = child.as_mut().unwrap().stdin.as_mut().unwrap();
        stdin.write_all(b"exit(0);\n").unwrap();
        stdin.flush().unwrap();
    }
    engine
        .request(json!({"command":"shutdown"}), Some(EXECUTION_BUDGET))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = engine
            .child
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .try_wait()
            .unwrap();
        if let Some(status) = status {
            assert!(
                status.success(),
                "qualified interpreter's normal shutdown failed: {status}"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "qualified interpreter did not complete normal shutdown"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !budget.expired.load(Ordering::Acquire),
        "qualified runtime exceeded its 120-second case budget"
    );
    assert!(
        started.elapsed() < CASE_BUDGET,
        "qualified runtime exceeded its 120-second case budget"
    );
}

// SPDX-License-Identifier: GPL-3.0-or-later
// Reuse the selected native V2 helpers for the registry lifecycle regression.
// This is one actual runtime behavior case, not a source-text identity assertion.
#[test]
#[ignore = "requires explicitly selected Octave 11.3/api-v61 and five already-built native modules"]
fn qualified_native_v2_registry_reparses_after_clear_and_rejects_foreign_cmdline() {
    let started = Instant::now();
    let config = selected_config(); // Every missing prerequisite is a hard failure.
    let root = tempfile::Builder::new()
        .prefix("mathlab-qualified-registry-")
        .tempdir()
        .unwrap();
    let project = root.path().join("résumé's registry project");
    fs::create_dir(&project).unwrap();
    let project = fs::canonicalize(project).unwrap();
    let project_marker = root.path().join("project-registry-entered");
    let foreign_marker = root.path().join("foreign-cmdline-registry-entered");
    let request_marker = root.path().join("rejected-user-body-entered");
    fs::write(project.join("mathlab_scope_register.m"), format!(
        "function varargout = mathlab_scope_register (varargin)\n fid=fopen({},'w'); if (fid>=0); fclose(fid); endif; error('MathLabTest:ProjectRegistry','project registry executed');\nendfunction\n",
        octave_string(&project_marker).unwrap(),
    )).unwrap();
    fs::write(project.join("registry_lifecycle_probe.m"),
        "function value = registry_lifecycle_probe ()\n persistent count = 0;\n count += 1;\n value = count;\nendfunction\n",
    ).unwrap();
    let mut engine = Engine::start_qualified(project.clone(), config.clone())
        .expect("qualified native V2 registry fixture failed admission or first Ready");
    let budget = RuntimeBudget::new(&engine, started);
    let pid = engine
        .process_id()
        .expect("qualified registry child missing");
    let initial = assert_ready(&mut engine, &config);
    engine
        .enable_scope_bridge(adapter(&config, AdapterRole::Scope))
        .unwrap();
    engine
        .enable_value_bridge(adapter(&config, AdapterRole::Value))
        .unwrap();
    let entry = engine
        .qualified_ready_profile
        .as_ref()
        .unwrap()
        .expected
        .entry_alias
        .clone();
    let registry_path = engine
        .session
        .path()
        .join("registry/mathlab_scope_register.m");
    let admitted_registry = fs::read(&registry_path).unwrap();
    execute(
        &mut engine,
        "registry_initial_one=registry_lifecycle_probe(); registry_initial_two=registry_lifecycle_probe();",
    );
    assert_real(&mut engine, "registry_initial_one", 1.);
    assert_real(&mut engine, "registry_initial_two", 2.);
    for (clear_source, expected_base_presence) in [("clear functions;", 1.), ("clear all;", 0.)] {
        // Seed the genuine unlocked registry through the locked provider's
        // internal actual-object handle. Ordinary project-name lookup is not
        // used. The only temporary owning handle dies before the real clear.
        execute(
            &mut engine,
            &format!(
                "registry_fixture={entry}('intrinsic_bind_registry'); \
             registry_fixture('__configuration__',{{}},false); \
             registry_fixture('__debug_configuration__',{{}},false); \
             registry_fixture('registry-lifecycle-key',{{@registry_lifecycle_probe}}); \
             registry_seed_count=double(numel(registry_fixture('registry-lifecycle-key',{{}},'lookup'))); \
             clear registry_fixture; registry_clear_base_control=23;"
            ),
        );
        assert_real(&mut engine, "registry_seed_count", 1.);
        execute(&mut engine, clear_source);
        // A distinct execute forces the actual scope adapter/provider to
        // rebind before any user code; the project shadow must remain inert.
        execute(
            &mut engine,
            &format!(
                "registry_fixture={entry}('intrinsic_bind_registry'); \
             registry_reset=double(isempty(registry_fixture('registry-lifecycle-key',{{}},'lookup'))); \
             clear registry_fixture; registry_probe_after_clear=registry_lifecycle_probe(); \
             registry_base_presence=double(exist('registry_clear_base_control','var'));"
            ),
        );
        assert_real(&mut engine, "registry_reset", 1.);
        assert_real(&mut engine, "registry_probe_after_clear", 1.);
        assert_real(
            &mut engine,
            "registry_base_presence",
            expected_base_presence,
        );
        let ready = assert_ready(&mut engine, &config);
        assert_eq!(ready.generation, initial.generation);
        assert_eq!(ready.invocation, initial.invocation);
        assert_eq!(engine.process_id(), Some(pid));
        assert!(
            !project_marker.exists(),
            "registry reparse entered the project shadow"
        );
        assert_eq!(fs::read(&registry_path).unwrap(), admitted_registry);
    }
    // Octave eval defines this named function in its command-line table. No
    // ordinary functions()/which() lookup is treated as a slot identity proof.
    // The function body itself must never run, even when the next request fails.
    let foreign_definition = format!(
        "function varargout = mathlab_scope_register (varargin)\n fid=fopen({},'w'); if (fid>=0); fclose(fid); endif; error('MathLabTest:ForeignRegistry','foreign command-line registry executed');\nendfunction\n",
        octave_string(&foreign_marker).unwrap(),
    );
    let definition_bytes = foreign_definition
        .as_bytes()
        .iter()
        .map(|byte| byte.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    execute(
        &mut engine,
        &format!("eval(char([{definition_bytes}])); registry_foreign_defined=1;"),
    );
    assert_real(&mut engine, "registry_foreign_defined", 1.);
    assert!(!project_marker.exists() && !foreign_marker.exists());
    assert_eq!(fs::read(&registry_path).unwrap(), admitted_registry);
    let rejected = engine.execute_with_timeout(
        &format!(
            "fid=fopen({},'w'); if (fid>=0); fclose(fid); endif;",
            octave_string(&request_marker).unwrap(),
        ),
        EXECUTION_BUDGET,
    );
    match rejected {
        Err(Error::Worker(error)) => assert_eq!(error.identifier, "MathLab:IntrinsicIdentity"),
        other => panic!(
            "foreign command-line registry was not rejected with exact identity error: {other:?}"
        ),
    }
    assert!(
        !project_marker.exists(),
        "foreign rejection entered a project registry body"
    );
    assert!(
        !foreign_marker.exists(),
        "foreign rejection executed the replacement body"
    );
    assert!(
        !request_marker.exists(),
        "foreign rejection dispatched the requested user body"
    );
    assert_eq!(fs::read(&registry_path).unwrap(), admitted_registry);
    assert_eq!(engine.process_id(), Some(pid));
    assert!(
        !budget.expired.load(Ordering::Acquire),
        "registry lifecycle fixture exceeded its 120-second budget"
    );
    assert!(started.elapsed() < CASE_BUDGET);
    engine.stop(); // Reap only this fixture's owned child; watchdog remains armed until Drop.
}

fn export_startup_config(
    selected: &TrustedStartupConfig,
    directory: &Path,
    graphics: bool,
) -> QualifiedStartupConfig {
    let pinned = |file: &PinnedFile| {
        json!({"path":file.path,
            "sha256":file.sha256.iter().map(|byte|format!("{byte:02x}")).collect::<String>()})
    };
    let adapters: Vec<_> = selected
        .adapters
        .iter()
        .map(|entry| {
            let role = match entry.role {
                AdapterRole::Input => "input",
                AdapterRole::Scope => "scope",
                AdapterRole::Value => "value",
                AdapterRole::Debug => "debug",
                AdapterRole::Graphics => "graphics",
            };
            json!({"role":role,"file":pinned(&entry.file)})
        })
        .collect();
    let profile = if graphics {
        QUALIFIED_GRAPHICS_STARTUP_PROFILE
    } else {
        QUALIFIED_STARTUP_PROFILE
    };
    let document = json!({"schema_version":2,"profile":profile,"startup":{
        "installation_root":selected.installation_root,
        "executable":pinned(&selected.executable),"provider":pinned(&selected.provider),
        "gnuplot":pinned(&selected.gnuplot),"ghostscript":pinned(&selected.ghostscript),
        "runtime_directories":selected.runtime_directories,"adapters":adapters,
        "octave_version":selected.octave_version,"octave_api_version":selected.octave_api_version}});
    let path = directory.join("selected-export-startup.json");
    fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    QualifiedStartupConfig::load(&path)
        .expect("export fixture requires schema 2 and the exact qualified native basenames")
}

fn export_active(
    engine: &mut Engine,
    selected: &TrustedStartupConfig,
    configured: &QualifiedStartupConfig,
    graphics: bool,
) -> SessionContext {
    let (ready, _) = engine
        .request(json!({"command":"ping"}), Some(EXECUTION_BUDGET))
        .unwrap();
    engine.accept_ready(&ready).unwrap();
    configured.require_active(engine).unwrap();
    assert_enabled(engine, selected);
    let status = engine.qualified_startup_status().unwrap();
    assert_eq!(status.profile, configured.profile());
    assert_eq!(
        status.intrinsics,
        engine.qualified_ready_profile.as_ref().unwrap().expected
    );
    engine.validate_context(&status.context).unwrap();
    let modules = configured.native_modules();
    assert_eq!(modules.len(), if graphics { 6 } else { 5 });
    for file in
        std::iter::once(&selected.provider).chain(selected.adapters.iter().map(|entry| &entry.file))
    {
        let name = file.path.file_name().unwrap().to_str().unwrap();
        assert_eq!(modules[name].path, file.path);
        assert_eq!(
            modules[name].sha256,
            file.sha256
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
    }
    assert_eq!(
        engine
            .capabilities()
            .supports(crate::graphics::CAP_GRAPHICS_REGISTRY),
        graphics
    );
    if graphics {
        engine
            .require_capability(crate::graphics::CAP_GRAPHICS_REGISTRY)
            .unwrap();
        let observed = &status.intrinsics.adapters["graphics"];
        assert_eq!(
            observed.path,
            adapter(selected, AdapterRole::Graphics).to_str().unwrap()
        );
        assert_eq!(
            (&*observed.protocol, &*observed.version),
            ("mathlab-graphics", "1")
        );
    }
    status.context
}

fn assert_export_png(result: &ExecutionResult, stage: &str) {
    eprintln!("qualified PNG export {stage}: {}", result.output);
    assert!(result.error.is_none(), "{stage}: {:?}", result.error);
    assert!(
        result.figure_warnings.is_empty(),
        "{stage}: {:?}",
        result.figure_warnings
    );
    assert!(
        !result.output_truncated && !result.output_decoding_loss,
        "{stage}: lost export evidence"
    );
    assert_eq!(
        result.figures.len(),
        1,
        "{stage}: expected the ordinary plotted figure"
    );
    let path = &result.figures[0].path;
    assert!(
        (33..=32 * 1024 * 1024).contains(&fs::metadata(path).unwrap().len()),
        "{stage}: unreasonable PNG byte size"
    );
    let mut bytes = Vec::new();
    File::open(path)
        .unwrap()
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(
        (33..=32 * 1024 * 1024).contains(&bytes.len()),
        "{stage}: unreasonable PNG payload size"
    );
    assert_eq!(
        &bytes[..8],
        b"\x89PNG\r\n\x1a\n",
        "{stage}: missing PNG signature"
    );
    assert_eq!(
        u32::from_be_bytes(bytes[8..12].try_into().unwrap()),
        13,
        "{stage}: invalid IHDR length"
    );
    assert_eq!(&bytes[12..16], b"IHDR", "{stage}: missing IHDR");
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    assert!(
        (32..=8192).contains(&width)
            && (32..=8192).contains(&height)
            && u64::from(width) * u64::from(height) <= 16 * 1024 * 1024,
        "{stage}: unreasonable PNG dimensions {width}x{height}"
    );
    let mut offset = 8_usize;
    let mut idat_started = false;
    let mut idat_ended = false;
    let mut idat_bytes = 0_usize;
    loop {
        assert!(
            bytes.len() - offset >= 12,
            "{stage}: truncated PNG chunk header/CRC"
        );
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let end = offset
            .checked_add(length)
            .and_then(|end| end.checked_add(12))
            .filter(|end| *end <= bytes.len())
            .expect("PNG chunk extends beyond its complete bounded payload");
        let kind = &bytes[offset + 4..offset + 8];
        if offset != 8 {
            assert_ne!(kind, b"IHDR", "{stage}: duplicate IHDR");
        }
        if kind == b"IDAT" {
            assert!(!idat_ended, "{stage}: nonconsecutive IDAT chunks");
            idat_started = true;
            idat_bytes += length;
        } else if idat_started {
            idat_ended = true;
        }
        if kind == b"IEND" {
            assert_eq!(length, 0, "{stage}: nonempty IEND");
            assert!(idat_bytes > 0, "{stage}: absent/empty IDAT payload");
            assert_eq!(end, bytes.len(), "{stage}: bytes after terminal IEND");
            break;
        }
        offset = end;
    }
    if let Some(root) = std::env::var_os("MATHLAB_QUALIFIED_TEST_PNG_EVIDENCE_DIR") {
        let root = PathBuf::from(root);
        assert!(
            root.is_absolute() && root.is_dir(),
            "invalid PNG evidence directory"
        );
        let session_name = path.parent().unwrap().file_name().unwrap();
        let directory = root.join(session_name);
        fs::create_dir_all(&directory).unwrap();
        let retained = directory.join(format!("{stage}.png"));
        let mut retained_file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&retained)
            .expect("never overwrite retained PNG evidence");
        std::io::Write::write_all(&mut retained_file, &bytes).unwrap();
        retained_file.sync_all().unwrap();
        let retained_bytes = fs::read(&retained).unwrap();
        assert_eq!(retained_bytes, bytes);
        let manifest = json!({
            "schema_version": 1,
            "stage": stage,
            "source_path": path,
            "path": retained,
            "sha256": format!("{:x}", Sha256::digest(&retained_bytes)),
            "bytes": retained_bytes.len(),
            "width": width,
            "height": height,
            "recorded_before_session_cleanup": true,
            "decoded_pixels": false,
        });
        let mut manifest_file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(retained.with_extension("png.json"))
            .expect("never overwrite retained PNG manifest");
        serde_json::to_writer_pretty(&mut manifest_file, &manifest).unwrap();
        manifest_file.sync_all().unwrap();
        eprintln!("QUALIFIED_PNG_RETAINED {}", manifest);
    }
    // Structural completeness only: this does not decode compressed pixels or
    // replace the installed Desktop's actual PNG preview/export journey.
    eprintln!(
        "qualified PNG bytes {stage}: path={}, dimensions={width}x{height}, payload_bytes={}, decoded_pixels=false",
        path.display(),
        bytes.len()
    );
}

fn ordinary_png_export(
    engine: &mut Engine,
    selected: &TrustedStartupConfig,
    stage: &str,
    previous: Option<&Value>,
) -> Value {
    // Make the child's existing current toolkit an explicit user selection once
    // per cold child. Warm recovery and clear must preserve it without reapplying
    // it. Record the raw preselection root so a fallback query cannot hide it.
    let selection = if previous.is_none() {
        "qualified_export_selected_toolkit=qualified_export_current_before; graphics_toolkit(qualified_export_selected_toolkit);"
    } else {
        "assert(qualified_export_existing_figure); assert(strcmp(qualified_export_existing_toolkit,qualified_export_selected_toolkit)); assert(strcmp(qualified_export_root_before,qualified_export_selected_toolkit)); assert(strcmp(qualified_export_current_before,qualified_export_selected_toolkit));"
    };
    let source = format!(
        "assert(!exist('qualified_export_unexpected_continuation','var')); \
         qualified_export_root_before=get(0,'defaultfigure__graphics_toolkit__'); \
         qualified_export_current_before=graphics_toolkit(); \
         qualified_export_existing_figure=exist('qualified_export_figure','var') && isfigure(qualified_export_figure); \
         qualified_export_existing_toolkit=''; \
         if (qualified_export_existing_figure); qualified_export_existing_toolkit=get(qualified_export_figure,'__graphics_toolkit__'); endif; \
         fprintf(1,'QUALIFIED_EXPORT_BEFORE_CLOSE %s\\n',jsonencode(struct('raw_root_before',qualified_export_root_before, \
           'current_before',qualified_export_current_before,'existing_figure',qualified_export_existing_figure, \
           'existing_figure_toolkit',qualified_export_existing_toolkit))); fflush(1); {selection} \
         if (qualified_export_existing_figure); close(qualified_export_figure); endif; \
         assert(strcmp(graphics_toolkit(),qualified_export_selected_toolkit)); \
         assert(strcmp(get(0,'defaultfigure__graphics_toolkit__'),qualified_export_selected_toolkit)); \
         assert(strcmp(getenv('GSC'),{})); \
         qualified_export_figure=figure('visible','off','position',[100,100,360,240]); \
         plot([0 1 2],[0 1 0]); title('QA Core5'); \
         qualified_export_evidence=struct('stage',{},'raw_root_before',qualified_export_root_before, \
           'current_before',qualified_export_current_before,'raw_root_after',get(0,'defaultfigure__graphics_toolkit__'), \
           'existing_figure',qualified_export_existing_figure,'existing_figure_toolkit',qualified_export_existing_toolkit, \
           'current_after',graphics_toolkit(),'figure_toolkit',get(qualified_export_figure,'__graphics_toolkit__'), \
           'selected_toolkit',qualified_export_selected_toolkit,'gnuplot_binary',gnuplot_binary(),'gsc',getenv('GSC'), \
           'temporary_directory',getenv('TMPDIR'),'temporary_name',tempname(),'working_directory',pwd()); \
         fprintf(1,'QUALIFIED_EXPORT_EVIDENCE %s\\n',jsonencode(qualified_export_evidence)); fflush(1);",
        octave_string(&selected.ghostscript.path).unwrap(),
        octave_string(Path::new(stage)).unwrap(),
    );
    let result = engine
        .execute_with_timeout(&source, EXECUTION_BUDGET)
        .unwrap();
    assert_export_png(&result, stage);
    let evidence: Value = serde_json::from_str(
        result
            .output
            .lines()
            .find_map(|line| line.strip_prefix("QUALIFIED_EXPORT_EVIDENCE "))
            .expect("ordinary export omitted its child toolkit/GSC observations"),
    )
    .unwrap();
    assert_eq!(evidence["stage"], stage);
    assert_eq!(evidence["gsc"], json!(selected.ghostscript.path));
    assert_eq!(evidence["gnuplot_binary"], json!(selected.gnuplot.path));
    // The shell-facing spelling must be safe while the files and the working
    // directory remain at the caller's actual selected locations.
    for field in ["temporary_directory", "temporary_name"] {
        assert!(
            evidence[field].as_str().unwrap().bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'+' | b'.')
            }),
            "{stage}: unsafe {field}"
        );
    }
    assert_eq!(
        fs::canonicalize(evidence["temporary_directory"].as_str().unwrap()).unwrap(),
        fs::canonicalize(engine.session.path()).unwrap()
    );
    let temporary_name = Path::new(evidence["temporary_name"].as_str().unwrap());
    assert!(
        temporary_name.is_absolute(),
        "{stage}: relative temporary name"
    );
    assert_eq!(
        fs::canonicalize(temporary_name.parent().unwrap()).unwrap(),
        fs::canonicalize(engine.session.path()).unwrap(),
        "{stage}: tempname escaped the selected session filesystem"
    );
    assert_eq!(
        evidence["working_directory"],
        json!(engine.config.working_directory)
    );
    assert!(!evidence["selected_toolkit"].as_str().unwrap().is_empty());
    for field in ["raw_root_after", "current_after", "figure_toolkit"] {
        assert_eq!(
            evidence[field], evidence["selected_toolkit"],
            "{stage}: {field}"
        );
    }
    if let Some(previous) = previous {
        assert_eq!(
            evidence["existing_figure"], true,
            "{stage}: lost retained figure"
        );
        assert_eq!(
            evidence["existing_figure_toolkit"], previous["figure_toolkit"],
            "{stage}: changed retained figure's toolkit before close"
        );
        assert_eq!(
            evidence["selected_toolkit"], previous["selected_toolkit"],
            "{stage}: changed explicit user toolkit"
        );
        assert_eq!(
            evidence["raw_root_before"], previous["raw_root_after"],
            "{stage}: changed raw root default"
        );
        assert_eq!(
            evidence["current_before"], previous["current_after"],
            "{stage}: changed current toolkit"
        );
    }
    evidence
}

fn stop_export_execution(mut engine: Engine, input_stop: bool) -> Engine {
    let before = engine.qualified_startup_status().unwrap().context;
    let pid = engine.process_id().unwrap();
    let cancellation = engine.cancellation_handle();
    let input = engine.input_handle().unwrap();
    let subscription = engine.subscribe_console();
    const MARKER: &[u8] = b"QUALIFIED_EXPORT_FINITE_STOP_READY";
    let source = if input_stop {
        "clear qualified_export_unexpected_continuation; qualified_export_reply=input('qualified PNG input Stop: ','s'); qualified_export_unexpected_continuation=1;"
    } else {
        "clear qualified_export_unexpected_continuation; fprintf(1,'QUALIFIED_EXPORT_FINITE_STOP_READY'); fflush(1); qualified_export_clock=tic(); while (toc(qualified_export_clock)<12); pause(.005); endwhile; qualified_export_unexpected_continuation=1;"
    };
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(source, EXECUTION_BUDGET);
        (engine, result)
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut output = Vec::new();
    loop {
        while let Ok(event) = subscription.try_recv() {
            if let ConsoleEventKind::Output { bytes } = event.kind {
                output.extend(bytes);
            }
        }
        assert!(
            output.len() <= 64 * 1024,
            "export Stop emitted excessive console output"
        );
        let reached = if input_stop {
            input.pending().unwrap().is_some_and(|pending| {
                assert_eq!(pending.request.key.generation, before.generation);
                assert_eq!(pending.request.key.invocation, before.invocation);
                assert_eq!(pending.request.mode, InputMode::String);
                assert_eq!(pending.phase, InputPhase::Waiting);
                true
            })
        } else {
            output.windows(MARKER.len()).any(|bytes| bytes == MARKER)
        };
        if reached {
            break;
        }
        assert!(
            !task.is_finished(),
            "export Stop source ended before its barrier; console={:?}",
            String::from_utf8_lossy(&output)
        );
        assert!(
            Instant::now() < deadline,
            "export Stop did not reach its barrier; console={:?}",
            String::from_utf8_lossy(&output)
        );
        thread::sleep(Duration::from_millis(5));
    }
    cancellation.interrupt().unwrap();
    let (engine, result) = join_execution(task, &cancellation);
    while let Ok(event) = subscription.try_recv() {
        if let ConsoleEventKind::Output { bytes } = event.kind {
            output.extend(bytes);
        }
    }
    // Preserve warnings, including mlock diagnostics, rather than changing the
    // warning state or treating a surviving PID as proof of graphics recovery.
    eprintln!(
        "qualified PNG Stop input={input_stop}: result_output={:?}; console={:?}",
        result.output,
        String::from_utf8_lossy(&output)
    );
    assert_eq!(
        result.error.as_ref().map(|error| error.identifier.as_str()),
        Some("MathLab:Interrupted")
    );
    assert!(
        result.figure_warnings.is_empty(),
        "{:?}",
        result.figure_warnings
    );
    assert_eq!(
        engine.process_id(),
        Some(pid),
        "Stop replaced the numerical child"
    );
    assert!(engine.validate_context(&before).is_err());
    let after = engine.qualified_startup_status().unwrap().context;
    assert_eq!(after.generation, before.generation);
    assert!(after.invocation > before.invocation);
    engine
}

fn actual_frontend_png_export_lifecycle(graphics: bool) {
    let started = Instant::now();
    let selected = if graphics {
        selected_graphics_config()
    } else {
        selected_config()
    };
    let directory = tempfile::Builder::new()
        .prefix("mathlab-qualified-png-")
        .tempdir()
        .unwrap();
    let project = fs::canonicalize(directory.path()).unwrap();
    let configured = export_startup_config(&selected, &project, graphics);
    let mut engine = configured
        .start(project)
        .expect("selected frontend PNG fixture failed admission/startup");
    let budget = RuntimeBudget::new(&engine, started);
    export_active(&mut engine, &selected, &configured, graphics);
    let mut evidence = ordinary_png_export(&mut engine, &selected, "before-stop", None);
    for (input_stop, stage) in [
        (false, "after-finite-loop-stop"),
        (true, "after-input-stop"),
    ] {
        engine = stop_export_execution(engine, input_stop);
        export_active(&mut engine, &selected, &configured, graphics);
        evidence = ordinary_png_export(&mut engine, &selected, stage, Some(&evidence));
    }
    // A distinct subsequent execution must rebind after the actual clear.
    let cleared = engine
        .execute_with_timeout("clear functions;", EXECUTION_BUDGET)
        .unwrap();
    assert_export_png(&cleared, "clear-functions-command");
    export_active(&mut engine, &selected, &configured, graphics);
    ordinary_png_export(
        &mut engine,
        &selected,
        "after-clear-functions-next-command",
        Some(&evidence),
    );
    let before = engine.qualified_startup_status().unwrap().context;
    let pid = engine.process_id().unwrap();
    configured
        .restart(&mut engine)
        .expect("configuration-aware export restart failed");
    assert_ne!(engine.process_id(), Some(pid));
    let after = export_active(&mut engine, &selected, &configured, graphics);
    assert_eq!(
        (after.generation, after.invocation),
        (before.generation + 1, 1)
    );
    assert!(engine.workspace_snapshot().unwrap().variables.is_empty());
    let final_evidence = ordinary_png_export(
        &mut engine,
        &selected,
        "after-configured-cold-restart",
        None,
    );
    assert!(
        !budget.expired.load(Ordering::Acquire),
        "PNG fixture exhausted its 120-second budget"
    );
    assert!(started.elapsed() < CASE_BUDGET);
    let session = fs::canonicalize(engine.session.path()).unwrap();
    let temporary_directory =
        PathBuf::from(final_evidence["temporary_directory"].as_str().unwrap());
    engine.stop();
    drop(engine);
    assert!(
        !session.exists(),
        "session temporary files survived Engine drop"
    );
    if temporary_directory != session {
        assert!(
            !temporary_directory.parent().unwrap().exists(),
            "temporary alias survived Engine drop"
        );
    }
}

#[test]
#[ignore = "unrun: requires explicit Octave 11.3/api-v61, five native modules, gnuplot and Ghostscript pins; actual PNG/Stop lifecycle"]
fn qualified_frontend_core_v2_png_export_survives_stop_clear_and_restart() {
    actual_frontend_png_export_lifecycle(false);
}

#[test]
#[ignore = "unrun: requires explicit Octave 11.3/api-v61, six native modules, gnuplot and Ghostscript pins; actual PNG/Stop lifecycle"]
fn qualified_frontend_graphics_v2_png_export_survives_stop_clear_and_restart() {
    actual_frontend_png_export_lifecycle(true);
}

fn actual_frontend_glyph_exports(graphics: bool) {
    let started = Instant::now();
    let selected = if graphics {
        selected_graphics_config()
    } else {
        selected_config()
    };
    let profile = if graphics { "graphics6" } else { "core5" };
    let retained = selected_path("MATHLAB_QUALIFIED_TEST_PNG_EVIDENCE_DIR", true);
    assert_eq!(
        fs::read_dir(&retained).unwrap().count(),
        0,
        "fresh PNG evidence root required"
    );
    let directory = tempfile::Builder::new()
        .prefix("mathlab-qualified-glyph-")
        .tempdir()
        .unwrap();
    let project = fs::canonicalize(directory.path()).unwrap();
    let configured = export_startup_config(&selected, &project, graphics);
    let startup_bytes = fs::read(project.join("selected-export-startup.json")).unwrap();
    let retained_startup = retained.join(format!("selected-{profile}-startup.json"));
    let mut startup_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&retained_startup)
        .unwrap();
    std::io::Write::write_all(&mut startup_file, &startup_bytes).unwrap();
    startup_file.sync_all().unwrap();
    let mut engine = configured
        .start(project)
        .expect("glyph fixture startup failed");
    let budget = RuntimeBudget::new(&engine, started);
    export_active(&mut engine, &selected, &configured, graphics);
    let modules: Vec<_> = std::iter::once(&selected.provider)
        .chain(selected.adapters.iter().map(|entry| &entry.file))
        .map(|file| {
            json!({"path":file.path,
            "sha256":file.sha256.iter().map(|b|format!("{b:02x}")).collect::<String>()})
        })
        .collect();
    eprintln!(
        "QUALIFIED_GLYPH_PROFILE {}",
        json!({"profile":profile,
        "pid":engine.process_id(),"runtime":selected.executable.path,
        "startup_config":retained_startup,"startup_sha256":format!("{:x}",Sha256::digest(&startup_bytes)),
        "runtime_sha256":format!("{:x}",Sha256::digest(fs::read(&selected.executable.path).unwrap())),
        "gnuplot":selected.gnuplot.path,"ghostscript":selected.ghostscript.path,"modules":modules})
    );
    let fixtures = [
        (
            "ascii-control",
            r#"close all;
figure('visible','off','position',[100,100,600,400]);
plot([0 1 2],[0 1 0]);
set(gca,'fontsize',12);
title('angle = 45 deg','interpreter','none','fontsize',20);
xlabel('temperature = 20 deg C','interpreter','none','fontsize',16);
"#,
            "angle = 45 deg",
            "temperature = 20 deg C",
            "none",
        ),
        (
            "literal-degree",
            r#"close all;
figure('visible','off','position',[100,100,600,400]);
plot([0 1 2],[0 1 0]);
set(gca,'fontsize',12);
title('angle = 45°','interpreter','none','fontsize',20);
xlabel('temperature = 20°C','interpreter','none','fontsize',16);
"#,
            "angle = 45°",
            "temperature = 20°C",
            "none",
        ),
        (
            "tex-degree",
            r#"close all;
figure('visible','off','position',[100,100,600,400]);
plot([0 1 2],[0 1 0]);
set(gca,'fontsize',12);
title('angle = 45^{\circ}','interpreter','tex','fontsize',20);
xlabel('temperature = 20^{\circ}C','interpreter','tex','fontsize',16);
"#,
            "angle = 45^{\\circ}",
            "temperature = 20^{\\circ}C",
            "tex",
        ),
    ];
    let mut temporary_directory = None;
    for (label, fixture, title, xlabel, interpreter) in fixtures {
        let source = format!(
            r#"{fixture}
qualified_glyph_evidence=struct('profile','{profile}','stage','{label}', ...
  'lc_all',getenv('LC_ALL'),'lang',getenv('LANG'),'lc_ctype',getenv('LC_CTYPE'), ...
  'title_bytes',double(uint8(get(get(gca,'title'),'string'))), ...
  'xlabel_bytes',double(uint8(get(get(gca,'xlabel'),'string'))), ...
  'title_interpreter',get(get(gca,'title'),'interpreter'), ...
  'xlabel_interpreter',get(get(gca,'xlabel'),'interpreter'), ...
  'toolkit',graphics_toolkit(),'figure_toolkit',get(gcf,'__graphics_toolkit__'), ...
  'gnuplot_binary',gnuplot_binary(),'gsc',getenv('GSC'),'tmpdir',getenv('TMPDIR'));
fprintf(1,'QUALIFIED_GLYPH_EVIDENCE %s%c',jsonencode(qualified_glyph_evidence),10); fflush(1);
"#
        );
        eprintln!(
            "QUALIFIED_GLYPH_FIXTURE {}",
            json!({"profile":profile,"stage":label,
            "fixture_sha256":format!("{:x}",Sha256::digest(fixture.as_bytes())),
            "source_sha256":format!("{:x}",Sha256::digest(source.as_bytes())),
            "title_utf8":title.as_bytes(),"xlabel_utf8":xlabel.as_bytes(),"interpreter":interpreter})
        );
        let result = engine
            .execute_with_timeout(&source, EXECUTION_BUDGET)
            .unwrap();
        assert_export_png(&result, &format!("{profile}-{label}"));
        let evidence: Value = serde_json::from_str(
            result
                .output
                .lines()
                .find_map(|line| line.strip_prefix("QUALIFIED_GLYPH_EVIDENCE "))
                .expect("missing glyph property/locale evidence"),
        )
        .unwrap();
        assert_eq!(evidence["profile"], profile);
        assert_eq!(evidence["stage"], label);
        assert_eq!(evidence["title_bytes"], json!(title.as_bytes()));
        assert_eq!(evidence["xlabel_bytes"], json!(xlabel.as_bytes()));
        assert_eq!(evidence["title_interpreter"], interpreter);
        assert_eq!(evidence["xlabel_interpreter"], interpreter);
        assert_eq!(evidence["lc_all"], "C");
        assert_eq!(evidence["toolkit"], "gnuplot");
        assert_eq!(evidence["figure_toolkit"], "gnuplot");
        assert_eq!(evidence["gnuplot_binary"], json!(selected.gnuplot.path));
        assert_eq!(evidence["gsc"], json!(selected.ghostscript.path));
        let observed_tmp = PathBuf::from(evidence["tmpdir"].as_str().unwrap());
        if let Some(previous) = &temporary_directory {
            assert_eq!(previous, &observed_tmp);
        }
        temporary_directory = Some(observed_tmp);
        assert!(!budget.expired.load(Ordering::Acquire));
        assert!(started.elapsed() < CASE_BUDGET);
    }
    let session = fs::canonicalize(engine.session.path()).unwrap();
    engine.stop();
    drop(engine);
    assert!(
        !session.exists(),
        "glyph fixture session survived Engine drop"
    );
    let temporary_directory = temporary_directory.unwrap();
    if temporary_directory != session {
        assert!(
            !temporary_directory.parent().unwrap().exists(),
            "glyph temporary alias survived Engine drop"
        );
    }
    assert!(!budget.expired.load(Ordering::Acquire));
    assert!(started.elapsed() < CASE_BUDGET);
}

#[test]
#[ignore = "requires explicit Core5 runtime/native/tool pins; three retained ASCII/literal-degree/TeX PNGs"]
fn qualified_frontend_core_v2_degree_glyph_exports() {
    actual_frontend_glyph_exports(false);
}

#[test]
#[ignore = "requires explicit Graphics6 runtime/native/tool pins; three retained ASCII/literal-degree/TeX PNGs"]
fn qualified_frontend_graphics_v2_degree_glyph_exports() {
    actual_frontend_glyph_exports(true);
}

include!("trusted_graphics_runtime_tests.rs");
include!("trusted_caller_lock_runtime_tests.rs");
