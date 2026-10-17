// SPDX-License-Identifier: GPL-3.0-or-later
// Included by the selected native V2 fixture; never discovers or builds modules.

fn caller_lock_capture(event: ConsoleEvent, bytes: &mut Vec<u8>) {
    match event.kind {
        ConsoleEventKind::Output { bytes: next } => {
            bytes.extend(next);
            assert!(
                bytes.len() <= 65_536,
                "caller-lock console exceeded its fixed bound"
            );
        }
        ConsoleEventKind::OutputLimit { dropped_bytes } => {
            panic!("caller-lock console lost {dropped_bytes} bytes");
        }
        ConsoleEventKind::Disconnected { message } => {
            panic!("caller-lock console disconnected: {message}");
        }
        _ => {}
    }
}

fn caller_lock_no_warning(label: &str, bytes: &[u8]) {
    let text = String::from_utf8_lossy(bytes);
    assert!(
        !text.to_ascii_lowercase().contains("warning:")
            && !text.contains("mlock: locking built-in function has no effect"),
        "{label} emitted a warning: {text}"
    );
}

#[test]
#[ignore = "requires explicitly selected Octave 11.3/api-v61 and five native modules; actual finite/input Stop"]
fn qualified_native_v2_caller_lock_recovers_without_warnings() {
    let started = Instant::now();
    let config = selected_config();
    let directory = tempfile::Builder::new()
        .prefix("mathlab-qualified-caller-lock-")
        .tempdir()
        .unwrap();
    let project = fs::canonicalize(directory.path()).unwrap();
    for name in ["mlock", "mislocked"] {
        fs::write(project.join(format!("{name}.m")), format!(
            "function varargout = {name} (varargin)\n error ('MathLabTest:LockShadow', 'private keeper resolved project {name}');\nendfunction\n"
        )).unwrap();
    }
    let mut engine = Engine::start_qualified(project, config.clone())
        .expect("selected caller-lock fixture failed admission or first Ready");
    let budget = RuntimeBudget::new(&engine, started);
    let subscription = engine.subscribe_console();
    let mut output = Vec::new();
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
    let pid = engine.process_id().expect("caller-lock child missing");

    for input_stop in [false, true] {
        let before = assert_ready(&mut engine, &config);
        let input = engine.input_handle().unwrap();
        let cancellation = engine.cancellation_handle();
        let source = if input_stop {
            "clear caller_lock_unexpected; caller_lock_partial=73; caller_lock_reply=input('caller-lock input Stop: ','s'); caller_lock_unexpected=1;"
        } else {
            "clear caller_lock_unexpected; caller_lock_partial=73; for caller_lock_i=1:100000000; if caller_lock_i==1; fprintf(1,'CALLER_LOCK_FINITE_RUNNING'); fflush(1); endif; pause(.005); endfor; caller_lock_unexpected=1;"
        };
        let task = thread::spawn(move || {
            let result = engine.execute_with_timeout(source, EXECUTION_BUDGET);
            (engine, result)
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            while let Ok(event) = subscription.try_recv() {
                caller_lock_capture(event, &mut output);
            }
            let reached = if input_stop {
                input.pending().unwrap().is_some_and(|pending| {
                    assert_eq!(pending.phase, InputPhase::Waiting);
                    assert_eq!(pending.request.mode, InputMode::String);
                    assert_eq!(pending.request.key.generation, before.generation);
                    assert_eq!(pending.request.key.invocation, before.invocation);
                    true
                })
            } else {
                output
                    .windows(b"CALLER_LOCK_FINITE_RUNNING".len())
                    .any(|bytes| bytes == b"CALLER_LOCK_FINITE_RUNNING")
            };
            if reached {
                break;
            }
            assert!(
                !task.is_finished(),
                "caller-lock source ended before its Stop barrier"
            );
            assert!(
                Instant::now() < deadline,
                "caller-lock Stop barrier timed out"
            );
            thread::sleep(Duration::from_millis(5));
        }
        cancellation.interrupt().unwrap();
        let (mut recovered, interrupted) = join_execution(task, &cancellation);
        assert_eq!(
            interrupted
                .error
                .as_ref()
                .map(|error| error.identifier.as_str()),
            Some("MathLab:Interrupted")
        );
        assert!(interrupted.figure_warnings.is_empty());
        caller_lock_no_warning("interrupted result", interrupted.output.as_bytes());
        assert_eq!(
            recovered.process_id(),
            Some(pid),
            "Stop replaced the numerical child"
        );
        assert!(recovered.validate_context(&before).is_err());
        let after = assert_ready(&mut recovered, &config);
        assert_eq!(after.generation, before.generation);
        assert!(after.invocation > before.invocation);
        assert_enabled(&recovered, &config);
        let next = recovered.execute_with_timeout(
            "caller_lock_next=2+2; caller_lock_unexpected_present=double(exist('caller_lock_unexpected','var'));",
            EXECUTION_BUDGET,
        ).unwrap();
        assert!(next.error.is_none());
        caller_lock_no_warning("post-Stop result", next.output.as_bytes());
        assert_real(&mut recovered, "caller_lock_partial", 73.);
        assert_real(&mut recovered, "caller_lock_next", 4.);
        assert_real(&mut recovered, "caller_lock_unexpected_present", 0.);
        while let Ok(event) = subscription.try_recv() {
            caller_lock_capture(event, &mut output);
        }
        caller_lock_no_warning("complete recovery console", &output);
        engine = recovered;
    }
    for source in [
        "clear functions; caller_lock_after_clear=41;",
        "clear all; caller_lock_after_clear=42;",
    ] {
        let result = engine
            .execute_with_timeout(source, EXECUTION_BUDGET)
            .unwrap();
        assert!(result.error.is_none());
        caller_lock_no_warning("clear result", result.output.as_bytes());
        assert_ready(&mut engine, &config);
    }
    while let Ok(event) = subscription.try_recv() {
        caller_lock_capture(event, &mut output);
    }
    caller_lock_no_warning("complete activation/recovery/clear console", &output);
    eprintln!(
        "caller-lock recovery PID={pid}; complete console={:?}",
        String::from_utf8_lossy(&output)
    );
    assert!(!budget.expired.load(Ordering::Acquire));
    assert!(started.elapsed() < CASE_BUDGET);
    engine.stop();
}
