// SPDX-License-Identifier: GPL-3.0-or-later
//! Real startup negotiation failures must precede any dependent user command.
use super::*;

fn ready_executable(context: &str) -> Option<PathBuf> {
    let Some(executable) = discover_octave() else {
        assert_ne!(
            std::env::var("MATHLAB_REQUIRE_READY_JSON_TESTS").as_deref(),
            Ok("1"),
            "required {context} test needs installed Octave"
        );
        eprintln!("SKIP {context}: Octave unavailable");
        return None;
    };
    Some(executable)
}

#[test]
fn startup_rejects_wrong_protocol_capabilities_limits_and_context() {
    let Some(executable) = ready_executable("ordinary startup negotiation") else {
        return;
    };
    for (before, after) in [
        ("'name', 'mathlab-worker'", "'name', 'foreign-worker'"),
        (
            "'name', 'mathlab-worker', 'version', builtin('uint32', 1)",
            "'name', 'mathlab-worker', 'version', builtin('uint32', 2)",
        ),
        ("'checked_scalar_write.v1', ", ""),
        (
            "'variable_cells', builtin('uint32', 2000)",
            "'variable_cells', builtin('uint32', 1999)",
        ),
        (
            "'response_bytes', builtin('uint32', 33554432)",
            "'response_bytes', 33554432.5",
        ),
        (
            "builtin('sprintf', '%.0f', generation)",
            "builtin('sprintf', '%.0f', generation + 1)",
        ),
    ] {
        let worker = WORKER.replace(before, after);
        assert_ne!(worker, WORKER, "Startup injection anchor moved: {before}");
        let directory = tempfile::tempdir().unwrap();
        let result = Engine::start_with_worker(
            EngineConfig {
                executable: executable.clone(),
                working_directory: directory.path().to_owned(),
            },
            &worker,
        );
        match result {
            Err(Error::Protocol(_)) if after != "'response_bytes', 33554432.5" => {}
            Err(Error::Worker(error)) if after == "'response_bytes', 33554432.5" => {
                assert_eq!(error.identifier, "MathLab:ReadyJsonInteger", "{error:?}");
            }
            Err(error) => panic!("Expected protocol rejection for {before}, received {error}"),
            Ok(_) => panic!("Invalid worker advertised {before} was accepted"),
        }
    }
}

#[test]
fn pure_ping_preserves_tokens_without_calling_a_project_version_function() {
    let Some(executable) = ready_executable("intrinsic version and fieldnames negotiation") else {
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let mut engine = Engine::start(EngineConfig {
        executable,
        working_directory: directory.path().to_owned(),
    })
    .unwrap();
    let version = engine.version().to_owned();
    fs::write(directory.path().join("version.m"),
        "function out = version(varargin)\nassignin('base','version_hook_called',1);\nerror('MathLab:VersionHook','Project version function called.');\nendfunction\n").unwrap();
    fs::write(directory.path().join("fieldnames.m"),
        "function out = fieldnames(varargin)\nbuiltin('assignin','base','fieldnames_hook_called',1);\nbuiltin('error','MathLab:FieldnamesHook','Project fieldnames function called.');\nendfunction\n").unwrap();
    assert!(
        engine
            .execute("rehash; clear version fieldnames; A=5;")
            .unwrap()
            .error
            .is_none()
    );
    let snapshot = engine.workspace_snapshot().unwrap();
    let token = snapshot
        .variables
        .iter()
        .find(|entry| entry.variable.name == "A")
        .unwrap()
        .token
        .clone();
    let (ready, _) = engine
        .request(json!({"command":"ping"}), Some(COMMAND_TIMEOUT))
        .unwrap();
    assert_eq!(ready["version"], version);
    assert_eq!(
        serde_json::from_value::<SessionContext>(ready["context"].clone()).unwrap(),
        snapshot.context
    );
    assert_eq!(
        engine
            .read_variable_checked(&token, 1, 1, 1, 1)
            .unwrap()
            .slice
            .cells[0][0],
        "5"
    );
    let after = engine.workspace_snapshot().unwrap();
    assert_eq!(after.context, snapshot.context);
    for name in ["version_hook_called", "fieldnames_hook_called"] {
        assert!(
            !after
                .variables
                .iter()
                .any(|entry| entry.variable.name == name),
            "pure readiness or inspection invoked project {name}"
        );
    }
    // Genuine __fieldnames__ must bypass the project M-file during Ready
    // encoding without changing ordinary user function resolution.
    let control = engine.execute("try; version(); builtin('error','MathLab:VersionControl','Project version hook did not throw.'); catch caught; assert(strcmp(caught.identifier,'MathLab:VersionHook')); end; try; fieldnames(struct('value',1)); builtin('error','MathLab:FieldnamesControl','Project fieldnames hook did not throw.'); catch caught; assert(strcmp(caught.identifier,'MathLab:FieldnamesHook')); end;").unwrap();
    assert!(control.error.is_none());
    let control_snapshot = engine.workspace_snapshot().unwrap();
    for name in ["version_hook_called", "fieldnames_hook_called"] {
        assert!(
            control_snapshot
                .variables
                .iter()
                .any(|entry| entry.variable.name == name),
            "ordinary user call did not reach project {name}"
        );
    }
}

#[cfg(unix)]
#[test]
fn recovery_ready_producer_failure_closes_transport_without_reentry_or_user_execution() {
    use std::thread;

    let Some(executable) = ready_executable("recovery readiness producer failure") else {
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let reached = directory.path().join("ready-before-begin");
    let reached_temporary = directory.path().join("ready-before-begin.pending");
    let release = directory.path().join("ready-release");
    let invocations = directory.path().join("ready-recovery-invocations");
    let forbidden = directory.path().join("ordinary-source-ran");
    let ready_anchor = "  result = builtin('struct', 'version', builtin('OCTAVE_VERSION'), 'working_directory', builtin('pwd'), 'protocol', protocol, ...";
    let begin_anchor = "        mathlab_begin (request_id, response_prefix, generation, console_helper_path, invocation);";
    assert_eq!(WORKER.matches(ready_anchor).count(), 1);
    assert_eq!(WORKER.matches(begin_anchor).count(), 1);
    let invalid_recovery = format!(
        "  if (invocation>1); ready_test_file=builtin('fopen',{},'a'); builtin('fprintf',ready_test_file,'%u\\n',builtin('uint64',invocation)); builtin('fclose',ready_test_file); protocol.limits.response_bytes=33554432.5; endif;\n{ready_anchor}",
        octave_string(&invocations).unwrap()
    );
    let worker = WORKER.replacen(ready_anchor, &invalid_recovery, 1);
    let gate = format!(
        "        if (builtin('strcmp',request.command,'execute')); ready_test_file=builtin('fopen',{},'w'); builtin('fclose',ready_test_file); builtin('rename',{},{}); while (! builtin('exist',{},'file')); builtin('pause',0.005); endwhile; endif;\n{begin_anchor}",
        octave_string(&reached_temporary).unwrap(),
        octave_string(&reached_temporary).unwrap(),
        octave_string(&reached).unwrap(),
        octave_string(&release).unwrap()
    );
    let worker = worker.replacen(begin_anchor, &gate, 1);
    let mut engine = Engine::start_with_worker(
        EngineConfig {
            executable,
            working_directory: directory.path().to_owned(),
        },
        &worker,
    )
    .unwrap();
    assert_eq!(engine.invocation, 1);
    assert!(engine.is_transport_open());
    assert!(!invocations.exists(), "initial readiness must remain valid");
    let cancellation = engine.cancellation_handle();
    struct Gate {
        cancellation: CancellationHandle,
        release: PathBuf,
    }
    impl Drop for Gate {
        fn drop(&mut self) {
            let _ = fs::write(&self.release, []);
            let _ = self.cancellation.terminate();
        }
    }
    let _cleanup = Gate {
        cancellation: cancellation.clone(),
        release,
    };
    let body = format!(
        "ready_forbidden=builtin('fopen',{},'w'); builtin('fclose',ready_forbidden);",
        octave_string(&forbidden).unwrap()
    );
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(&body, Duration::from_secs(20));
        (engine, result)
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    while !reached.exists() {
        assert!(
            Instant::now() < deadline,
            "request never reached the pre-Begin barrier"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!forbidden.exists());
    cancellation.interrupt().unwrap();
    let deadline = Instant::now() + Duration::from_secs(25);
    while !task.is_finished() {
        assert!(
            Instant::now() < deadline,
            "invalid recovery readiness did not settle"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let (mut engine, result) = task.join().unwrap();
    match result {
        Err(Error::Protocol(message)) => {
            assert!(message.contains("Recovery worker failed"), "{message}");
            assert!(message.contains("MathLab:ReadyJsonInteger:"), "{message}");
            assert!(message.contains("session was terminated"), "{message}");
        }
        Err(error) => panic!("expected producer-specific closed recovery error, got {error:?}"),
        Ok(_) => panic!("invalid recovery readiness returned an execution result"),
    }
    assert!(!engine.is_transport_open());
    assert_eq!(engine.invocation, 2, "producer failure retried recovery");
    assert_eq!(fs::read_to_string(&invocations).unwrap(), "2\n");
    assert!(!forbidden.exists(), "canceled ordinary source was executed");
    let next_id = engine.next_id;
    assert!(matches!(
        engine.execute("must_not_run_after_ready_failure=1;"),
        Err(Error::Protocol(message)) if message.contains("transport is closed")
    ));
    assert_eq!(
        engine.next_id, next_id,
        "closed session published a request"
    );
    assert!(!engine.session.path().join("request.tmp").exists());
    assert!(!engine.session.path().join("request.json").exists());
}
