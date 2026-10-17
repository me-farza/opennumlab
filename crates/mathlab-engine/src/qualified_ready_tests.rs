// SPDX-License-Identifier: GPL-3.0-or-later
//! Ready validation and status fixtures. Synthetic profile selection proves only
//! the Rust lifecycle boundary when run, never provider or runtime trust.

use super::*;
use qualified_ready::IntrinsicsReady;

fn status() -> Value {
    json!({
        "protocol":"mathlab-intrinsics", "version":"1", "phase":"ready",
        "provider_file":"/private/tmp/ready-test/provider.oct",
        "worker_file":"/private/tmp/ready-test/mathlab_worker.m",
        "registry_file":"/private/tmp/ready-test/registry/mathlab_scope_register.m",
        "entry_alias":"__mlentry_ready_test", "octave_version":"11.3.0",
        "octave_api_version":"api-v61", "initial_path_admitted":true,
        "shutdown_alias":"__mlshutdown_ready_test", "shutdown_registered":true,
        "adapters":{"value":{"path":"/private/tmp/ready-test/value.oct",
            "protocol":"mathlab-value", "version":"1"}}
    })
}

fn profile() -> QualifiedReadyProfile {
    QualifiedReadyProfile {
        expected: serde_json::from_value::<IntrinsicsReady>(status()).unwrap(),
    }
}

fn ready(engine: &Engine) -> Value {
    json!({
        "protocol":{"name":"mathlab-worker", "version":1,
            "capabilities":["binding_snapshot.v1", "checked_scalar_write.v1",
                "display_page.v1", "atomic_range_edit.v1", "dense_complex_native_components.v1"],
            "limits":{"variable_cells":2000,"response_bytes":33554432}},
        "context":{"worker_id":engine.response_prefix,
            "generation":engine.generation.to_string(),
            "invocation":engine.invocation.to_string(), "revision":"9"},
        "version":"11.3.0", "working_directory":engine.session.path(),
        "intrinsics":status()
    })
}

#[test]
fn qualified_status_serializes_profile_context_and_original_wire_identities() {
    let context = SessionContext {
        worker_id: "status-worker".into(),
        generation: u64::MAX,
        invocation: 3,
        revision: 9,
    };
    let snapshot = profile().accepted_status(context.clone());
    assert_eq!(snapshot.profile, "qualified-core-v2");
    assert_eq!(snapshot.context, context);
    assert_eq!(snapshot.intrinsics.version, "1");
    assert_eq!(snapshot.intrinsics.adapters["value"].version, "1");
    assert_eq!(
        serde_json::to_value(snapshot).unwrap(),
        json!({
            "profile": "qualified-core-v2",
            "context": {
                "worker_id": "status-worker",
                "generation": "18446744073709551615",
                "invocation": "3",
                "revision": "9"
            },
            "intrinsics": status()
        })
    );
}

#[test]
fn qualified_status_requires_current_accepted_context_and_live_transport() {
    let mut engine = state_fixture();
    assert!(engine.qualified_startup_status().is_none());
    let accepted = ready(&engine);
    engine.accept_ready(&accepted).unwrap();
    let snapshot = engine.qualified_startup_status().unwrap();
    assert_eq!(
        snapshot.context,
        engine.qualified_ready_context.clone().unwrap()
    );
    assert_eq!(snapshot.intrinsics, profile().expected);

    engine.generation += 1;
    assert!(engine.qualified_startup_status().is_none());
    engine.generation -= 1;
    engine.invocation += 1;
    assert!(engine.qualified_startup_status().is_none());
    engine.invocation -= 1;
    assert!(engine.qualified_startup_status().is_some());
    engine.transport = None;
    assert!(engine.qualified_startup_status().is_none());

    let mut rejected = state_fixture();
    let mut rejected_ready = ready(&rejected);
    rejected.accept_ready(&rejected_ready).unwrap();
    rejected_ready["intrinsics"]["phase"] = json!("binding");
    assert!(rejected.accept_ready(&rejected_ready).is_err());
    assert!(rejected.qualified_startup_status().is_none());

    let mut legacy = state_fixture();
    legacy.qualified_ready_profile = None;
    let legacy_ready = ready(&legacy);
    legacy.accept_ready(&legacy_ready).unwrap();
    assert!(legacy.qualified_startup_status().is_none());
}

#[test]
fn graphics_carrier_ready_uses_the_shared_identity_and_close_boundary() {
    let mut engine = state_fixture();
    let graphics = json!({"path":"/private/tmp/ready-test/graphics.oct", "protocol":"mathlab-graphics", "version":"1"});
    let mut expected = status();
    expected["adapters"]["graphics"] = graphics.clone();
    engine.qualified_ready_profile = Some(QualifiedReadyProfile {
        expected: serde_json::from_value(expected).unwrap(),
    });
    let mut response = ready(&engine);
    response["intrinsics"]["adapters"]["graphics"] = graphics;
    response["protocol"]["capabilities"]
        .as_array_mut()
        .unwrap()
        .push(json!("graphics_registry.v1"));
    engine.edit_history_uncertain = true;
    engine.accept_ready(&response).unwrap();
    engine.require_capability("graphics_registry.v1").unwrap();
    engine.require_capability("atomic_range_edit.v1").unwrap();
    assert!(engine.has_value_edit_uncertainty());
    response["intrinsics"]["adapters"]["graphics"]["path"] =
        json!("/private/tmp/ready-test/other.oct");
    assert!(engine.accept_ready(&response).is_err());
    assert!(!engine.is_transport_open());
    assert!(engine.has_value_edit_uncertainty());
}

#[test]
#[cfg(unix)]
fn graphics_enable_rejects_unadmitted_or_different_role_path_before_request_publication() {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;

    // Admission is real, but these are inert files; no native module or worker
    // runs. A valid existing alternate path must fail before request publication.
    fn pinned(path: PathBuf, executable: bool) -> PinnedFile {
        let bytes = b"inert graphics activation fixture";
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(
            &path,
            fs::Permissions::from_mode(if executable { 0o700 } else { 0o600 }),
        )
        .unwrap();
        PinnedFile {
            path,
            sha256: Sha256::digest(bytes).into(),
        }
    }

    for selected_graphics in [false, true] {
        let mut engine = state_fixture();
        let session = fs::canonicalize(engine.session.path()).unwrap();
        fs::set_permissions(&session, fs::Permissions::from_mode(0o700)).unwrap();
        let root = session.join("installation");
        let runtime = root.join("m");
        fs::create_dir_all(&runtime).unwrap();
        fs::create_dir(session.join("registry")).unwrap();
        pinned(session.join("mathlab_worker.m"), false);
        pinned(session.join("registry/mathlab_scope_register.m"), false);
        pinned(session.join("console-fence"), true);
        let selected = pinned(session.join("selected-graphics.oct"), false);
        let other = pinned(session.join("other-graphics.oct"), false);
        let config = TrustedStartupConfig {
            installation_root: root.clone(),
            executable: pinned(root.join("octave"), true),
            provider: pinned(session.join("provider.oct"), false),
            gnuplot: pinned(session.join("gnuplot"), true),
            ghostscript: pinned(session.join("gs"), true),
            runtime_directories: vec![runtime],
            adapters: if selected_graphics {
                vec![AdapterConfig {
                    role: AdapterRole::Graphics,
                    file: selected,
                }]
            } else {
                vec![]
            },
            octave_version: "11.3.0".into(),
            octave_api_version: "api-v61".into(),
        };
        let prepared = config.admit().unwrap().prepare(&session).unwrap();
        engine.qualified_ready_profile = Some(prepared.ready_profile());
        engine.trusted_startup = Some(prepared);
        engine.edit_history_uncertain = true;
        let next_id = engine.next_id;
        let error = engine.enable_graphics_service(&other.path).unwrap_err();
        let expected = if selected_graphics {
            "Enabled adapter differs from the admitted role identity"
        } else {
            "Enabled adapter was not admitted"
        };
        assert!(matches!(error, Error::Protocol(message) if message == expected));
        assert_eq!(engine.next_id, next_id);
        assert!(!session.join("request.json").exists());
        assert!(!session.join("request.tmp").exists());
        assert!(engine.is_transport_open());
        assert!(engine.has_value_edit_uncertainty());
    }
}

fn state_fixture() -> Engine {
    let session = tempfile::tempdir().unwrap();
    let input = InputHub::new(session.path()).unwrap();
    input.set_enabled(true).unwrap();
    let console = ConsoleHub::new();
    // An inert reader keeps this a Rust state fixture; no numerical worker or
    // fake protocol producer runs. is_transport_open is intentionally structural.
    let transport = ConsoleTransport::start(
        File::open("/dev/null").unwrap(),
        "test-worker".into(),
        7,
        console.clone(),
        input.clone(),
    )
    .unwrap();
    Engine {
        config: EngineConfig {
            executable: "/bin/false".into(),
            working_directory: session.path().to_owned(),
        },
        session,
        child: Arc::new(Mutex::new(None)),
        active: Arc::new(AtomicBool::new(false)),
        interrupt_pending: Arc::new(AtomicBool::new(false)),
        interrupt_sent: Arc::new(AtomicBool::new(false)),
        active_request: Arc::new(AtomicU64::new(0)),
        worker_generation: Arc::new(AtomicU64::new(7)),
        transport: Some(transport),
        console,
        input,
        input_bridge: None,
        user_execution_started: false,
        generation: 7,
        invocation: 3,
        next_id: 1,
        response_prefix: "test-worker".into(),
        console_helper: PathBuf::new(),
        version: String::new(),
        capabilities: None,
        trusted_startup: None,
        qualified_ready_profile: Some(profile()),
        qualified_ready_context: None,
        prepared_sources: Vec::new(),
        active_sources: BTreeMap::new(),
        next_preparation_id: 1,
        debug_bridge: None,
        debugger: None,
        debug_session: None,
        scope_bridge: None,
        value_bridge: None,
        edit_history_uncertain: false,
    }
}

fn assert_retired(engine: &mut Engine, context: &SessionContext, prior_latch: bool) {
    assert!(!engine.is_transport_open());
    assert!(engine.capabilities().capabilities.is_empty());
    assert!(engine.require_capability("binding_snapshot.v1").is_err());
    assert!(engine.qualified_ready_context.is_none());
    assert!(
        engine.qualified_ready_profile.is_some(),
        "must not downgrade to legacy"
    );
    assert!(engine.validate_context(context).is_err());
    assert!(engine.input.handle().pending().is_err());
    assert!(!engine.active.load(Ordering::Acquire));
    assert_eq!(engine.has_value_edit_uncertainty(), prior_latch);
    let next_id = engine.next_id;
    assert!(
        engine
            .send_request(json!({"command":"ping"}), false)
            .is_err()
    );
    assert_eq!(engine.next_id, next_id);
    assert!(!engine.session.path().join("request.json").exists());
    assert!(!engine.session.path().join("request.tmp").exists());
}

#[test]
fn qualified_ready_accepts_complete_exact_status_and_rejects_every_late_field_before_commit() {
    let valid_fixture = state_fixture();
    let original = ready(&valid_fixture);
    drop(valid_fixture);
    let mut variants = Vec::new();
    for path in [
        "/protocol/version",
        "/context/generation",
        "/version",
        "/working_directory",
        "/intrinsics",
        "/intrinsics/version",
        "/intrinsics/phase",
        "/intrinsics/provider_file",
        "/intrinsics/worker_file",
        "/intrinsics/registry_file",
        "/intrinsics/entry_alias",
        "/intrinsics/octave_api_version",
        "/intrinsics/octave_version",
        "/intrinsics/initial_path_admitted",
        "/intrinsics/shutdown_alias",
        "/intrinsics/shutdown_registered",
        "/intrinsics/adapters/value/path",
        "/intrinsics/adapters/value/protocol",
        "/intrinsics/adapters/value/version",
    ] {
        let mut invalid = original.clone();
        *invalid.pointer_mut(path).unwrap() = Value::Null;
        variants.push(invalid);
    }
    let mut numeric_status_version = original.clone();
    numeric_status_version["intrinsics"]["version"] = json!(1);
    variants.push(numeric_status_version);
    let mut extra_role = original.clone();
    extra_role["intrinsics"]["adapters"]["foreign"] =
        json!({"path":"/tmp/a.oct","protocol":"foreign","version":"1"});
    variants.push(extra_role);
    let mut omitted_role = original.clone();
    omitted_role["intrinsics"]["adapters"] = json!({});
    variants.push(omitted_role);
    let mut extra_status_field = original.clone();
    extra_status_field["intrinsics"]["callable"] = json!("must not enter JSON");
    variants.push(extra_status_field);
    let mut foreign_identity = original;
    foreign_identity["intrinsics"]["provider_file"] = json!("/tmp/foreign/provider.oct");
    variants.push(foreign_identity);

    for mut invalid in variants {
        let mut engine = state_fixture();
        let accepted = ready(&engine);
        engine.accept_ready(&accepted).unwrap();
        let context = engine.qualified_ready_context.clone().unwrap();
        engine.require_capability("binding_snapshot.v1").unwrap();
        engine.edit_history_uncertain = true;
        let cwd = engine.working_directory().to_owned();
        // Give the malformed payload a new cwd so a late status failure would
        // expose an early mutation. Preserve its deliberate invalid cwd case.
        if !invalid["working_directory"].is_null() {
            invalid["working_directory"] = json!("/private/tmp/uninstalled-ready-cwd");
        }
        let error = engine.accept_ready(&invalid).unwrap_err();
        assert!(matches!(error, Error::Protocol(_)), "{error:?}");
        assert_eq!(engine.version(), "11.3.0");
        assert_eq!(engine.working_directory(), cwd);
        assert_eq!(engine.capabilities().name, "mathlab-worker");
        assert_retired(&mut engine, &context, true);
        assert!(matches!(
            engine.ensure_value_edit_certain(),
            Err(Error::ValueEdit(value_edit::EditError::UncertainCommit(_)))
        ));
        assert!(
            engine.accept_ready(&accepted).is_err(),
            "closed Ready must not revive authority"
        );
        assert!(engine.capabilities().capabilities.is_empty());
    }
}

#[test]
fn initial_qualified_rejection_has_inert_nonpanicking_diagnostics_and_no_new_edit_ambiguity() {
    let mut engine = state_fixture();
    let mut invalid = ready(&engine);
    invalid.as_object_mut().unwrap().remove("intrinsics");
    assert!(engine.accept_ready(&invalid).is_err());
    assert!(engine.capabilities().capabilities.is_empty());
    assert_eq!(engine.version(), "");
    let context = serde_json::from_value(invalid["context"].clone()).unwrap();
    assert_retired(&mut engine, &context, false);
}

#[test]
fn qualified_close_waits_for_only_its_owned_child() {
    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn idle_child() -> Child {
        Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }
    let mut unrelated = ChildGuard(idle_child());
    let mut engine = state_fixture();
    let child = idle_child();
    let pid = child.id();
    *engine.child.lock().unwrap() = Some(child);
    let mut invalid = ready(&engine);
    invalid["intrinsics"]["phase"] = json!("binding");
    assert!(engine.accept_ready(&invalid).is_err());
    assert!(engine.child.lock().unwrap().is_none());
    let mut status = 0;
    assert_eq!(
        unsafe { libc::waitpid(pid as libc::pid_t, &mut status, libc::WNOHANG) },
        -1
    );
    assert_eq!(
        io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
    assert!(
        unrelated.0.try_wait().unwrap().is_none(),
        "closed a foreign child"
    );
}

#[test]
fn legacy_ready_keeps_its_existing_optional_status_behavior_and_stages_before_commit() {
    let mut engine = state_fixture();
    engine.qualified_ready_profile = None;
    let mut legacy = ready(&engine);
    legacy.as_object_mut().unwrap().remove("intrinsics");
    engine.accept_ready(&legacy).unwrap();
    let old = engine.capabilities().clone();
    let mut invalid = legacy;
    invalid["version"] = Value::Null;
    invalid["protocol"]["capabilities"] = json!([
        "binding_snapshot.v1",
        "checked_scalar_write.v1",
        "display_page.v1"
    ]);
    assert!(engine.accept_ready(&invalid).is_err());
    assert_eq!(engine.capabilities(), &old);
    assert!(engine.is_transport_open());
}

fn actual_executable() -> Option<PathBuf> {
    let executable = discover_octave();
    if executable.is_none() {
        assert_ne!(
            std::env::var("MATHLAB_REQUIRE_QUALIFIED_READY_TESTS").as_deref(),
            Ok("1"),
            "required qualified Ready lifecycle fixture needs installed Octave"
        );
        eprintln!("SKIP qualified Ready lifecycle: installed Octave unavailable");
    }
    executable
}

// Select only the Rust validator, with explicitly synthetic identity metadata.
// The unmodified launcher/provider is NOT qualified by this test arrangement.
fn select_synthetic_profile(engine: &mut Engine) {
    let (mut ready, _) = engine
        .request(json!({"command":"ping"}), Some(COMMAND_TIMEOUT))
        .unwrap();
    let mut expected = profile();
    expected.expected.octave_version = engine.version().to_owned();
    let mut identity = status();
    identity["octave_version"] = json!(engine.version());
    ready["intrinsics"] = identity;
    engine.qualified_ready_profile = Some(expected);
    engine.accept_ready(&ready).unwrap();
}

#[test]
fn actual_startup_missing_qualified_status_closes_without_an_accepted_context() {
    let Some(executable) = actual_executable() else {
        return;
    };
    let mut engine = state_fixture();
    engine.config.executable = executable;
    engine.console_helper = console::write_fence_helper(engine.session.path()).unwrap();
    fs::create_dir(engine.session.path().join("registry")).unwrap();
    fs::create_dir(engine.session.path().join("cache")).unwrap();
    fs::write(
        engine
            .session
            .path()
            .join("registry/mathlab_scope_register.m"),
        SCOPE_REGISTRY,
    )
    .unwrap();
    fs::write(
        engine.session.path().join("mathlab_worker.m"),
        format!("{WORKER}\n{VALUE_PAGE_WORKER}\n{VALUE_EDIT_WORKER}\n{READY_JSON_WORKER}"),
    )
    .unwrap();
    let error = engine.spawn().unwrap_err();
    assert!(
        matches!(error, Error::Protocol(message) if message.contains("Qualified Ready rejected"))
    );
    assert!(engine.process_id().is_none());
    assert!(!engine.is_transport_open());
    assert!(engine.qualified_ready_context.is_none());
    assert!(engine.capabilities().capabilities.is_empty());
    assert!(!engine.has_value_edit_uncertainty());
}

#[test]
fn actual_settlement_ping_and_value_enable_reject_successful_unqualified_ready_without_retry() {
    let Some(executable) = actual_executable() else {
        return;
    };
    // The private enable fixture returns a Ready without loading any adapter.
    // This targets accept_ready at that caller; it is not native loader proof.
    let anchor = "            installed_value_bridge = mathlab_value_bridge_open (request.bridge_path);\n            value_bridge = installed_value_bridge;\n            value_bridge_path = request.bridge_path;";
    assert_eq!(WORKER.matches(anchor).count(), 1);
    let worker = WORKER.replacen(anchor, "            value_bridge = [];", 1);
    for enable in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut engine = Engine::start_with_worker(
            EngineConfig {
                executable: executable.clone(),
                working_directory: directory.path().to_owned(),
            },
            &worker,
        )
        .unwrap();
        select_synthetic_profile(&mut engine);
        let context = engine.qualified_ready_context.clone().unwrap();
        let next_id = engine.next_id;
        let error = if enable {
            engine.install_value_bridge(Path::new("/private/tmp/synthetic-never-loaded.oct"))
        } else {
            engine.settle_cancellation().map(|_| ())
        }
        .unwrap_err();
        assert!(
            matches!(error, Error::Protocol(message) if message.contains("Qualified Ready rejected"))
        );
        assert_eq!(
            engine.next_id,
            next_id + 1,
            "Ready rejection retried the request"
        );
        assert!(engine.process_id().is_none());
        assert_retired(&mut engine, &context, false);
    }
}

#[test]
fn actual_postassign_edit_recovery_ready_rejection_remains_uncertain_and_does_not_reenter() {
    use std::thread;
    use value_edit::{EditError, LeafDescriptor, RangeEditRequest};

    let Some(executable) = actual_executable() else {
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let reached = directory.path().join("qualified-ready-postassign");
    let pending = directory.path().join("qualified-ready-postassign.pending");
    let release = directory.path().join("qualified-ready-release");
    let invocations = directory.path().join("qualified-ready-invocations");
    let assign_anchor = "## MATHLAB_VALUE_EDIT_POSTASSIGN_BOUNDARY";
    let ready_anchor = "  result = builtin('struct', 'version', builtin('OCTAVE_VERSION'), 'working_directory', builtin('pwd'), 'protocol', protocol, ...";
    assert_eq!(WORKER.matches(assign_anchor).count(), 1);
    assert_eq!(WORKER.matches(ready_anchor).count(), 1);
    let gate = format!(
        "{assign_anchor}\n            qr_file=builtin('fopen',{},'w'); builtin('fprintf',qr_file,'%u',builtin('evalin','base','A')); builtin('fclose',qr_file); builtin('rename',{},{}); while (! builtin('exist',{},'file')); builtin('pause',0.005); endwhile;",
        octave_string(&pending).unwrap(),
        octave_string(&pending).unwrap(),
        octave_string(&reached).unwrap(),
        octave_string(&release).unwrap()
    );
    let count = format!(
        "  if (invocation>1); qr_file=builtin('fopen',{},'a'); builtin('fprintf',qr_file,'%u\\n',builtin('uint64',invocation)); builtin('fclose',qr_file); endif;\n{ready_anchor}",
        octave_string(&invocations).unwrap()
    );
    let worker = WORKER
        .replacen(assign_anchor, &gate, 1)
        .replacen(ready_anchor, &count, 1);
    let mut engine = Engine::start_with_worker(
        EngineConfig {
            executable,
            working_directory: directory.path().to_owned(),
        },
        &worker,
    )
    .unwrap();
    assert!(engine.execute("A=uint64(1);").unwrap().error.is_none());
    select_synthetic_profile(&mut engine);
    let token = engine
        .workspace_snapshot()
        .unwrap()
        .variables
        .into_iter()
        .find(|binding| binding.variable.name == "A")
        .unwrap()
        .token;
    let old_context = token.context.clone();
    let request = RangeEditRequest::from_components(
        ValuePath {
            root: token,
            steps: vec![],
        },
        PageWindow {
            row_start: 1.into(),
            column_start: 1.into(),
            rows: 1,
            columns: 1,
            higher_indices: vec![],
        },
        LeafDescriptor {
            class: NumericClass::Uint64,
            source_shape: vec![1.into(), 1.into()],
            source_storage: SourceStorage::Dense,
            complex: false,
        },
        &NumericComponents::Uint64(vec![99]),
        None,
    )
    .unwrap();
    let retry = request.clone();
    let cancellation = engine.cancellation_handle();
    struct Release {
        cancellation: CancellationHandle,
        path: PathBuf,
    }
    impl Drop for Release {
        fn drop(&mut self) {
            let _ = fs::write(&self.path, []);
            let _ = self.cancellation.terminate();
        }
    }
    let _cleanup = Release {
        cancellation: cancellation.clone(),
        path: release,
    };
    let task = thread::spawn(move || {
        let result = engine.write_value_range_checked(&request);
        (engine, result)
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    while !reached.exists() {
        assert!(
            Instant::now() < deadline,
            "edit did not reach postassignment gate"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(fs::read_to_string(&reached).unwrap(), "99");
    cancellation.interrupt().unwrap();
    let deadline = Instant::now() + Duration::from_secs(25);
    while !task.is_finished() {
        assert!(
            Instant::now() < deadline,
            "qualified Ready rejection did not settle"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let (mut engine, result) = task.join().unwrap();
    assert!(matches!(result,
        Err(Error::ValueEdit(EditError::UncertainCommit(message))) if message.contains("Qualified Ready rejected")));
    assert_eq!(engine.invocation, 2);
    assert_eq!(fs::read_to_string(invocations).unwrap(), "2\n");
    assert!(engine.process_id().is_none());
    assert_retired(&mut engine, &old_context, true);
    let next_id = engine.next_id;
    assert!(matches!(
        engine.write_value_range_checked(&retry),
        Err(Error::ValueEdit(EditError::UncertainCommit(_)))
    ));
    assert_eq!(engine.next_id, next_id, "uncertain edit was resubmitted");
    assert!(engine.reset_value_edit_history().is_err());
    assert!(engine.has_value_edit_uncertainty());
    // The selected profile survives a restart failure. A legacy worker cannot
    // silently reopen it, and the failed restart cannot clear the edit latch.
    assert!(engine.restart().is_err());
    assert!(engine.has_value_edit_uncertainty());
    assert!(engine.qualified_ready_profile.is_some());
    assert!(engine.capabilities().capabilities.is_empty());
    assert!(!engine.is_transport_open());
}
