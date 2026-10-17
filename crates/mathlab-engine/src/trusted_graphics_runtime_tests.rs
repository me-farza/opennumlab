// SPDX-License-Identifier: GPL-3.0-or-later
// Included in trusted_startup_runtime_tests; no discovery or skip-as-pass path.

pub(super) fn selected_graphics_config() -> TrustedStartupConfig {
    let mut config = selected_config();
    config.adapters.push(AdapterConfig {
        role: AdapterRole::Graphics,
        file: pin("MATHLAB_QUALIFIED_TEST_GRAPHICS_ADAPTER"),
    });
    config
}

fn graphics_ready(engine: &mut Engine, config: &TrustedStartupConfig) -> SessionContext {
    let (ready, _) = engine
        .request(json!({"command":"ping"}), Some(COMMAND_TIMEOUT))
        .unwrap();
    engine.accept_ready(&ready).unwrap();
    let accepted = engine
        .qualified_startup_status()
        .expect("missing accepted graphics Ready");
    assert_eq!(accepted.profile, "qualified-graphics-v2");
    assert_eq!(
        accepted.intrinsics,
        engine.qualified_ready_profile.as_ref().unwrap().expected
    );
    assert_eq!(accepted.intrinsics.adapters.len(), 5);
    assert_eq!(
        accepted.intrinsics.provider_file,
        config.provider.path.to_str().unwrap()
    );
    assert_eq!(
        accepted.intrinsics.worker_file,
        engine.worker_source_path().to_str().unwrap()
    );
    for (role, name) in [
        (AdapterRole::Input, "input"),
        (AdapterRole::Scope, "scope"),
        (AdapterRole::Value, "value"),
        (AdapterRole::Debug, "debug"),
        (AdapterRole::Graphics, "graphics"),
    ] {
        assert_eq!(
            accepted.intrinsics.adapters[name].path,
            adapter(config, role).to_str().unwrap()
        );
    }
    engine.validate_context(&accepted.context).unwrap();
    accepted.context
}

fn graphics_execute(engine: &mut Engine, source: &str) {
    let result = engine
        .execute_with_timeout(source, EXECUTION_BUDGET)
        .unwrap();
    assert!(
        result.error.is_none(),
        "graphics user body failed: {:?}",
        result.error
    );
}

fn graphics_line(engine: &mut Engine, width: f64) -> crate::graphics::GraphicsToken {
    use crate::graphics::GraphicsProperties;
    let snapshot = engine.graphics_snapshot().unwrap();
    assert_eq!(
        snapshot.nodes.len(),
        3,
        "expected actual figure/axes/line tree: {snapshot:?}"
    );
    assert!(snapshot.unsupported.is_empty(), "{snapshot:?}");
    let lines: Vec<_> = snapshot
        .nodes
        .iter()
        .filter(|node| matches!(node.properties, GraphicsProperties::Line { .. }))
        .collect();
    assert_eq!(lines.len(), 1);
    let token = lines[0].token.clone();
    let query = engine.graphics_query(&token, 2, 2).unwrap();
    assert!(
        matches!(query.node.properties, GraphicsProperties::Line {line_width, ..} if line_width == width)
    );
    let samples = query.samples.expect("actual line query omitted samples");
    assert_eq!(samples.start.get(), 2);
    assert_eq!(samples.total_count.get(), 4);
    assert_eq!(samples.original_indices, vec![2.into(), 3.into()]);
    assert_eq!(samples.x, vec![2., 3.]);
    assert_eq!(samples.y, vec![20., 30.]);
    token
}

fn stale_graphics<T>(result: Result<T>) {
    match result {
        Err(Error::Worker(error)) => assert_eq!(error.identifier, "MathLab:StaleGraphics"),
        Err(other) => panic!("expected stale graphics identity, got {other:?}"),
        Ok(_) => panic!("old graphics identity was accepted"),
    }
}

fn graphics_normal_exit(engine: &mut Engine, queued_commands: &str) {
    {
        let mut child = engine.child.lock().unwrap();
        let stdin = child.as_mut().unwrap().stdin.as_mut().unwrap();
        // The commands execute only after the mailbox worker returns. No
        // protocol requests may follow its final shutdown completion.
        stdin.write_all(queued_commands.as_bytes()).unwrap();
        stdin.flush().unwrap();
    }
    engine
        .request(json!({"command":"shutdown"}), Some(EXECUTION_BUDGET))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = engine
            .child
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .try_wait()
            .unwrap()
        {
            assert!(
                status.success(),
                "graphics interpreter normal exit failed: {status}"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "graphics interpreter did not exit normally"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
#[ignore = "requires explicitly selected Octave 11.3/api-v61 provider, four core adapters and graphics adapter (six native modules)"]
fn qualified_native_v2_graphics_survives_stop_reentry_and_cold_restart() {
    let started = Instant::now();
    let config = selected_graphics_config();
    let directory = tempfile::Builder::new()
        .prefix("mathlab-qualified-graphics-")
        .tempdir()
        .unwrap();
    let project = fs::canonicalize(directory.path()).unwrap();
    fs::write(
        project.join("qualified_graphics_callback.m"),
        r#"function qualified_graphics_callback (source, event)
  if (! builtin('strcmp', event.stimulus, 'programmatic'))
    builtin('error', 'MathLabTest:Stimulus', 'Expected actual programmatic dispatch.');
  endif
  effect = builtin('evalin', 'base', 'gfx_effect');
  builtin('assignin', 'base', 'gfx_effect', effect + 1);
  builtin('set', source, 'linewidth', effect + 2.5);
  if (builtin('evalin', 'base', 'gfx_loop'))
    builtin('fprintf', 1, 'QUALIFIED_GRAPHICS_CALLBACK_RUNNING');
    builtin('fflush', 1);
    while (1 == 1); builtin('pause', .005); endwhile
  endif
endfunction
"#,
    )
    .unwrap();
    let mut engine = Engine::start_qualified(project, config.clone())
        .expect("selected graphics V2 first Ready failed");
    let budget = RuntimeBudget::new(&engine, started);
    let first = graphics_ready(&mut engine, &config);
    assert_eq!((first.generation, first.invocation), (1, 1));
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
    engine
        .enable_graphics_service(adapter(&config, AdapterRole::Graphics))
        .unwrap();
    assert_enabled(&engine, &config);
    engine
        .require_capability(crate::graphics::CAP_GRAPHICS_REGISTRY)
        .unwrap();
    assert!(engine.graphics_snapshot().unwrap().nodes.is_empty());
    const SCENE: &str = "gfx_effect=0; gfx_loop=0; gfx_f=figure('visible','off'); gfx_a=axes('parent',gfx_f); gfx_l=line('parent',gfx_a,'xdata',[1,2,3,4],'ydata',[10,20,30,40],'linewidth',1.5,'buttondownfcn',@qualified_graphics_callback);";
    graphics_execute(
        &mut engine,
        "qualified_graphics_survivor=73; qualified_graphics_complex=complex([1,4],[2,-3]);",
    );
    graphics_execute(&mut engine, SCENE);
    let initial_line = graphics_line(&mut engine, 1.5);
    let ticket = engine.graphics_enqueue_callback(&initial_line).unwrap();
    let positive = engine.graphics_dispatch_callback(&ticket).unwrap();
    assert!(
        positive.error.is_none(),
        "normal callback failed: {:?}",
        positive.error
    );
    assert_real(&mut engine, "gfx_effect", 1.);
    graphics_line(&mut engine, 2.5);
    graphics_execute(&mut engine, "gfx_loop=1;");
    let token = graphics_line(&mut engine, 2.5);
    let ticket = engine.graphics_enqueue_callback(&token).unwrap();
    let pending = engine.graphics_enqueue_callback(&token).unwrap();
    let before_stop = graphics_ready(&mut engine, &config);
    let pid = engine
        .process_id()
        .expect("graphics child missing before Stop");
    let cancellation = engine.cancellation_handle();
    let subscription = engine.subscribe_console();
    let mut running = Some(thread::spawn(move || {
        let result = engine.graphics_dispatch_callback(&ticket);
        (engine, result)
    }));
    const MARKER: &[u8] = b"QUALIFIED_GRAPHICS_CALLBACK_RUNNING";
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut output = Vec::new();
    while !output.windows(MARKER.len()).any(|bytes| bytes == MARKER) {
        if let Ok(event) = subscription.recv_timeout(Duration::from_millis(50))
            && let ConsoleEventKind::Output { bytes } = event.kind
        {
            output.extend(bytes);
            if output.len() > 4096 {
                output.drain(..output.len() - 4096);
            }
        }
        if running.as_ref().unwrap().is_finished() {
            let (engine, result) = running
                .take()
                .unwrap()
                .join()
                .expect("graphics callback thread panicked");
            panic!(
                "graphics callback ended before Stop: {result:?}; console={:?}; pid={:?}; transport={}; watchdog={}",
                String::from_utf8_lossy(&output),
                engine.process_id(),
                engine.is_transport_open(),
                budget.expired.load(Ordering::Acquire)
            );
        }
        assert!(
            Instant::now() < deadline,
            "actual graphics callback did not begin; console={:?}",
            String::from_utf8_lossy(&output)
        );
    }
    cancellation.interrupt().unwrap();
    let (mut engine, interrupted) = join_execution(running.take().unwrap(), &cancellation);
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
        "Stop replaced numerical process"
    );
    assert!(engine.validate_context(&before_stop).is_err());
    let recovered = graphics_ready(&mut engine, &config);
    assert_eq!(recovered.generation, before_stop.generation);
    assert!(recovered.invocation > before_stop.invocation);
    stale_graphics(engine.graphics_query(&token, 2, 2));
    stale_graphics(engine.graphics_dispatch_callback(&pending));
    assert_enabled(&engine, &config);
    engine
        .require_capability(crate::graphics::CAP_GRAPHICS_REGISTRY)
        .unwrap();
    assert_real(&mut engine, "qualified_graphics_survivor", 73.);
    assert_real(&mut engine, "gfx_effect", 2.);
    assert_complex(
        &root_page(&mut engine, "qualified_graphics_complex", 2),
        &[1., 4.],
        &[2., -3.],
    );
    let fresh = graphics_line(&mut engine, 3.5);
    assert_eq!(fresh.context.worker_id, token.context.worker_id);
    assert_eq!(fresh.context.generation, token.context.generation);
    assert!(
        fresh.context.invocation > token.context.invocation,
        "graphics native ledger epoch was not replaced"
    );
    // Actual input traverses the retained input/debug adapters; execution and
    // checked metadata/complex reads above also exercise debug/scope/value paths.
    let input = engine.input_handle().unwrap();
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout("gfx_input=input('graphics recovery: ','s'); gfx_input_ok=double(strcmp(gfx_input,'graphics-ok')); gfx_loop=0;", EXECUTION_BUDGET);
        (engine, result)
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(pending) = input.pending().unwrap() {
            assert_eq!(pending.request.key.generation, recovered.generation);
            assert_eq!(pending.request.key.invocation, recovered.invocation);
            input.reply(&pending.request.key, "graphics-ok").unwrap();
            break;
        }
        if task.is_finished() {
            let (_, result) = task.join().unwrap();
            panic!("recovered graphics/core input ended without request: {result:?}");
        }
        assert!(
            Instant::now() < deadline,
            "recovered graphics/core input never requested input"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let (mut engine, result) = join_execution(task, &cancellation);
    assert!(
        result.error.is_none(),
        "recovered input failed: {:?}",
        result.error
    );
    assert_real(&mut engine, "gfx_input_ok", 1.);
    let fresh = graphics_line(&mut engine, 3.5);
    let ticket = engine.graphics_enqueue_callback(&fresh).unwrap();
    let result = engine.graphics_dispatch_callback(&ticket).unwrap();
    assert!(
        result.error.is_none(),
        "recovered callback failed: {:?}",
        result.error
    );
    assert_real(&mut engine, "gfx_effect", 3.);
    let before_restart = graphics_line(&mut engine, 4.5);
    engine.restart().unwrap();
    assert_ne!(engine.process_id(), Some(pid));
    assert!(engine.validate_context(&recovered).is_err());
    let restarted = graphics_ready(&mut engine, &config);
    assert_eq!(
        (restarted.generation, restarted.invocation),
        (recovered.generation + 1, 1)
    );
    assert_enabled(&engine, &config);
    assert!(
        !engine
            .capabilities()
            .supports(crate::graphics::CAP_GRAPHICS_REGISTRY),
        "cold restart must require explicit graphics activation"
    );
    engine
        .enable_graphics_service(adapter(&config, AdapterRole::Graphics))
        .unwrap();
    assert!(engine.graphics_snapshot().unwrap().nodes.is_empty());
    stale_graphics(engine.graphics_query(&before_restart, 2, 2));
    graphics_execute(&mut engine, SCENE);
    let cold = graphics_line(&mut engine, 1.5);
    assert_eq!(cold.context.generation.get(), restarted.generation);
    assert_eq!(cold.context.invocation.get(), 1);
    let ticket = engine.graphics_enqueue_callback(&cold).unwrap();
    let result = engine.graphics_dispatch_callback(&ticket).unwrap();
    assert!(
        result.error.is_none(),
        "cold callback failed: {:?}",
        result.error
    );
    let live_line = graphics_line(&mut engine, 2.5);
    // Keep the original scene alive, but also retain native ledger entries for
    // objects that have already been deleted before interpreter shutdown.
    graphics_execute(
        &mut engine,
        "gfx_deleted_f=figure('visible','off'); gfx_deleted_a=axes('parent',gfx_deleted_f); gfx_deleted_l=line('parent',gfx_deleted_a,'xdata',[1,2,3,4],'ydata',[10,20,30,40]);",
    );
    let captured = engine.graphics_snapshot().unwrap();
    assert_eq!(
        captured.nodes.len(),
        6,
        "both scenes must be captured: {captured:?}"
    );
    assert!(captured.unsupported.is_empty(), "{captured:?}");
    let deleted_line = captured
        .nodes
        .iter()
        .find(|node| {
            matches!(
                node.properties,
                crate::graphics::GraphicsProperties::Line { .. }
            ) && node.token.object_id != live_line.object_id
        })
        .expect("second scene line was not captured by the native ledger")
        .token
        .clone();
    graphics_execute(&mut engine, "delete(gfx_deleted_f);");
    let retained_live_line = graphics_line(&mut engine, 2.5);
    assert_eq!(retained_live_line.object_id, live_line.object_id);
    assert_eq!(retained_live_line.lifetime, live_line.lifetime);
    stale_graphics(engine.graphics_query(&deleted_line, 2, 2));
    graphics_ready(&mut engine, &config);
    // Exercise genuine registered atexit cleanup with both live and deleted
    // ledger references. Never pre-clean through the explicit shutdown alias.
    graphics_normal_exit(&mut engine, "exit(0);\n");
    assert!(!budget.expired.load(Ordering::Acquire));
    assert!(started.elapsed() < CASE_BUDGET);
}

#[test]
#[ignore = "requires explicitly selected Octave 11.3/api-v61 provider, four core adapters and graphics adapter (six native modules)"]
fn qualified_native_v2_graphics_shutdown_alias_is_idempotent() {
    let started = Instant::now();
    let config = selected_graphics_config();
    let directory = tempfile::Builder::new()
        .prefix("mathlab-qualified-graphics-idempotence-")
        .tempdir()
        .unwrap();
    let project = fs::canonicalize(directory.path()).unwrap();
    let mut engine = Engine::start_qualified(project, config.clone())
        .expect("selected graphics V2 idempotence first Ready failed");
    let budget = RuntimeBudget::new(&engine, started);
    graphics_ready(&mut engine, &config);
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
    engine
        .enable_graphics_service(adapter(&config, AdapterRole::Graphics))
        .unwrap();
    assert_enabled(&engine, &config);
    graphics_execute(
        &mut engine,
        "gfx_f=figure('visible','off'); gfx_a=axes('parent',gfx_f); gfx_l=line('parent',gfx_a,'xdata',[1,2,3,4],'ydata',[10,20,30,40],'linewidth',1.5);",
    );
    graphics_line(&mut engine, 1.5);
    graphics_ready(&mut engine, &config);
    let shutdown_alias = engine
        .qualified_startup_status()
        .unwrap()
        .intrinsics
        .shutdown_alias;
    // This separate process deliberately exercises explicit cleanup twice;
    // its later atexit invocation tests the already-cleaned no-op path.
    graphics_normal_exit(
        &mut engine,
        &format!("{shutdown_alias}();\n{shutdown_alias}();\nexit(0);\n"),
    );
    assert!(!budget.expired.load(Ordering::Acquire));
    assert!(started.elapsed() < CASE_BUDGET);
}
