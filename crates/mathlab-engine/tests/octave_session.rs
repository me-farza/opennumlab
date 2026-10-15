// SPDX-License-Identifier: GPL-3.0-or-later
use mathlab_debugger::{DebugState, Debugger, ResumeAction};
use mathlab_engine::{
    CancellationHandle, ConsoleEventKind, Engine, EngineConfig, Error, ExecutionResult,
    discover_octave,
};
use std::fs;
use std::thread;
use std::time::Duration;

fn engine() -> Option<(Engine, tempfile::TempDir)> {
    let Some(executable) = discover_octave() else {
        eprintln!(
            "Skipping Octave integration test: no installed Octave executable. Set MATHLAB_OCTAVE."
        );
        return None;
    };
    let directory = tempfile::Builder::new()
        .prefix("mathlab-test-")
        .tempdir()
        .unwrap();
    let engine = Engine::start(EngineConfig {
        executable,
        working_directory: directory.path().into(),
    })
    .expect("installed Octave must start the protocol worker");
    Some((engine, directory))
}

#[test]
fn console_streams_non_newline_bytes_before_execution_completes_in_pipe_order() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    let subscription = engine.subscribe_console();
    let generation = engine.console_generation();
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "fprintf(stdout,'PARTIAL'); pause(.5); disp('A1'); system('printf B2'); disp('C3'); fprintf(stderr,'D4');",
            Duration::from_secs(5),
        );
        (engine, result)
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut bytes = Vec::new();
    let mut request_id = None;
    let mut last_sequence = 0;
    let mut saw_partial_before_completion = false;
    let end_sequence = loop {
        let event = subscription.recv_timeout(Duration::from_millis(100));
        if let Ok(event) = event {
            assert_eq!(event.generation, generation);
            assert!(event.sequence > last_sequence);
            last_sequence = event.sequence;
            match event.kind {
                ConsoleEventKind::Begin => request_id = event.request_id,
                ConsoleEventKind::Output { bytes: fragment } => {
                    bytes.extend(fragment);
                    if bytes == b"PARTIAL" {
                        assert!(
                            !task.is_finished(),
                            "partial output arrived only after completion"
                        );
                        saw_partial_before_completion = true;
                    }
                }
                ConsoleEventKind::End => break event.sequence,
                ConsoleEventKind::OutputLimit { .. } => {
                    panic!("small console stream was truncated")
                }
                ConsoleEventKind::InputRequested { .. }
                | ConsoleEventKind::InputFinished { .. } => {
                    panic!("output-only code unexpectedly requested interactive input")
                }
                ConsoleEventKind::Disconnected { message } => {
                    panic!("console disconnected: {message}")
                }
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no complete console stream: {bytes:?}"
        );
    };
    assert!(request_id.is_some());
    assert!(
        saw_partial_before_completion,
        "non-newline output was buffered until completion"
    );
    assert_eq!(bytes, b"PARTIALA1\nB2C3\nD4");
    let (_engine, result) = task.join().unwrap();
    let result = result.unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(result.output.as_bytes(), bytes);
    assert_eq!(result.console_end_sequence, end_sequence);
    assert_eq!(result.console_generation, generation);
    assert!(!result.output_truncated && !result.output_decoding_loss);
}

#[test]
fn console_keeps_raw_invalid_utf8_and_lossy_aggregate_without_losing_the_session() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    let subscription = engine.subscribe_console();
    let result = engine.execute(
        "fwrite(stdout,uint8([240 159])); fflush(stdout); pause(.05); fwrite(stdout,uint8([146 169 255]));"
    ).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(result.output, "💩�");
    assert!(result.output_decoding_loss);
    let mut bytes = Vec::new();
    loop {
        let event = subscription.recv_timeout(Duration::from_secs(1)).unwrap();
        match event.kind {
            ConsoleEventKind::Output { bytes: fragment } => bytes.extend(fragment),
            ConsoleEventKind::End => break,
            _ => {}
        }
    }
    assert_eq!(bytes, [240, 159, 146, 169, 255]);
    assert!(
        engine
            .execute("after_invalid_utf8=17;")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn console_output_limit_and_slow_subscriber_do_not_block_completion_or_later_requests() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    let slow_subscription = engine.subscribe_console();
    let result = engine
        .execute_with_timeout(
            "fwrite(stdout,repmat(uint8(120),1,33*1024*1024)); limited_output_saved=23;",
            Duration::from_secs(10),
        )
        .unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(result.output.len(), 32 * 1024 * 1024);
    assert!(result.output.bytes().all(|byte| byte == b'x'));
    assert!(result.output_truncated);
    assert_eq!(result.output_dropped_bytes, 1024 * 1024);
    assert!(!result.output_decoding_loss);
    let mut saw_delivery_gap = false;
    let mut reached_end = false;
    while let Ok(event) = slow_subscription.try_recv() {
        if let ConsoleEventKind::OutputLimit { dropped_bytes } = event.kind {
            assert!(dropped_bytes > 0);
            saw_delivery_gap = true;
        }
        reached_end |= event.sequence >= result.console_end_sequence;
    }
    assert!(saw_delivery_gap, "slow subscriber loss was hidden");
    assert!(
        reached_end,
        "subscriber could not reach the completion barrier"
    );
    drop(slow_subscription);
    let after = engine
        .execute("assert(limited_output_saved==23); disp('AFTER_LIMIT');")
        .unwrap();
    assert!(after.error.is_none(), "{:?}", after.error);
    assert_eq!(after.output, "AFTER_LIMIT\n");
}

#[cfg(unix)]
#[test]
fn console_reports_background_output_while_engine_owner_is_idle() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    let subscription = engine.subscribe_console();
    let result = engine
        .execute("system('sh -c ''sleep .4; printf IDLE_BACKGROUND'' &'); foreground_value=9;")
        .unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert!(
        !result.output.contains("IDLE_BACKGROUND"),
        "background output was not delayed"
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(4);
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"IDLE_BACKGROUND") {
        if let Ok(event) = subscription.recv_timeout(Duration::from_millis(100))
            && let ConsoleEventKind::Output { bytes: fragment } = event.kind
        {
            assert!(
                event.request_id.is_none(),
                "idle output was assigned to a finished request"
            );
            bytes.extend(fragment);
        }
        assert!(
            std::time::Instant::now() < deadline,
            "background output did not arrive: {bytes:?}"
        );
    }
    assert!(
        engine
            .execute("assert(foreground_value==9);")
            .unwrap()
            .error
            .is_none()
    );
}

#[cfg(unix)]
#[test]
fn console_preserves_emitted_output_on_hard_exit_and_subscription_survives_restart() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    let subscription = engine.subscribe_console();
    let generation = engine.console_generation();
    let error = engine
        .execute_with_timeout(
            "fprintf(stdout,'BEFORE_HARD_EXIT'); fflush(stdout); exit(7);",
            Duration::from_secs(3),
        )
        .unwrap_err();
    assert!(matches!(error, Error::ProcessExited(_)), "{error}");
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let mut bytes = Vec::new();
    while !bytes
        .windows(b"BEFORE_HARD_EXIT".len())
        .any(|window| window == b"BEFORE_HARD_EXIT")
    {
        if let Ok(event) = subscription.recv_timeout(Duration::from_millis(100))
            && let ConsoleEventKind::Output { bytes: fragment } = event.kind
        {
            bytes.extend(fragment);
        }
        assert!(
            std::time::Instant::now() < deadline,
            "lost output before hard exit: {bytes:?}"
        );
    }
    engine.restart().unwrap();
    assert!(engine.console_generation() > generation);
    let result = engine.execute("disp('AFTER_RESTART');").unwrap();
    let mut saw_new_generation = false;
    while let Ok(event) = subscription.try_recv() {
        if event.generation == result.console_generation
            && let ConsoleEventKind::Output { bytes } = event.kind
            && bytes
                .windows(b"AFTER_RESTART".len())
                .any(|window| window == b"AFTER_RESTART")
        {
            saw_new_generation = true;
        }
    }
    assert!(
        saw_new_generation,
        "restart replaced the session console subscription"
    );
}

fn wait_debug_pause(debugger: &Debugger, line: usize, after_revision: u64) -> DebugState {
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        let observed = debugger.state().unwrap();
        if let Some(state) = &observed
            && state.paused
            && state.line == line
            && state.revision > after_revision
        {
            return state.clone();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "debugger did not pause at line {line}; latest state={observed:?}"
        );
        thread::sleep(Duration::from_millis(15));
    }
}

type EngineRequestTask = thread::JoinHandle<(Engine, Result<ExecutionResult, Error>)>;

fn wait_debug_pause_or_finished(
    debugger: &Debugger,
    line: usize,
    after_revision: u64,
    task: &mut Option<EngineRequestTask>,
) -> DebugState {
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        let state = debugger.state().unwrap();
        if let Some(pause) = &state
            && pause.paused
            && pause.line == line
            && pause.revision > after_revision
        {
            return pause.clone();
        }
        if task.as_ref().unwrap().is_finished() {
            let (_engine, result) = task.take().unwrap().join().unwrap();
            panic!(
                "execution completed before expected pause at line {line}: {result:?}; latest state={state:?}"
            );
        }
        assert!(
            std::time::Instant::now() < deadline,
            "debugger did not pause at line {line}; latest state={state:?}"
        );
        thread::sleep(Duration::from_millis(15));
    }
}

// A failed paused-scope assertion must not leave an interpreter waiting forever
// in a detached execution thread. Successful joins explicitly disarm this guard.
struct RunningRequestGuard(Option<CancellationHandle>);

impl RunningRequestGuard {
    fn disarm(&mut self) {
        self.0 = None;
    }
}

impl Drop for RunningRequestGuard {
    fn drop(&mut self) {
        if let Some(handle) = &self.0 {
            let _ = handle.terminate();
        }
    }
}

#[test]
fn optional_debugger_runs_real_scope_step_continue_and_invalidates_restart_handles() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        eprintln!(
            "Skipping native engine debugger integration: set MATHLAB_DEBUG_BRIDGE to the locally compiled adapter"
        );
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let source = fs::canonicalize(directory.path())
        .unwrap()
        .join("debug_function.m");
    fs::write(
        &source,
        "function y=debug_function(x)\n a=x+1;\n y=a*2;\nend\n",
    )
    .unwrap();
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    let clone = debugger.clone();
    assert_eq!(debugger.set_breakpoint(&source, 2).unwrap(), vec![2]);
    let task = thread::spawn(move || {
        let result=engine.execute_with_timeout("saved_before=123; disp('before breakpoint'); debug_result=debug_function(5); disp('after breakpoint');",Duration::from_secs(15));
        (engine, result)
    });
    let state = wait_debug_pause(&clone, 2, 0);
    assert_eq!(state.file, source);
    let variables = clone.workspace(state.revision).unwrap();
    assert!(variables.iter().any(|variable| variable.name == "x"));
    assert!(
        !variables
            .iter()
            .any(|variable| variable.name == "saved_before")
    );
    assert!(
        clone
            .stack(state.revision)
            .unwrap()
            .iter()
            .any(|frame| frame.name == "debug_function" && frame.line == 2)
    );
    assert_eq!(clone.evaluate("disp(x);", state.revision).unwrap(), "5\n");
    clone.resume(ResumeAction::Step, state.revision).unwrap();
    let next = wait_debug_pause(&clone, 3, state.revision);
    assert_eq!(clone.evaluate("disp(a);", next.revision).unwrap(), "6\n");
    clone.resume(ResumeAction::Continue, next.revision).unwrap();
    let (mut engine, result) = task.join().unwrap();
    let result = result.unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert!(
        result.output.contains("before breakpoint") && result.output.contains("after breakpoint"),
        "{}",
        result.output
    );
    debugger.clear_breakpoint(&source, 2).unwrap();
    assert!(
        engine
            .execute("assert(debug_result==12 && saved_before==123);")
            .unwrap()
            .error
            .is_none()
    );
    let old_directory = debugger.directory().to_owned();
    engine.restart().unwrap();
    assert!(!old_directory.exists());
    assert!(debugger.workspace(next.revision).is_err());
    let replacement = engine.debugger_handle().unwrap();
    assert_ne!(replacement.directory(), old_directory);
    replacement.set_breakpoint(&source, 2).unwrap();
    replacement.clear_breakpoint(&source, 2).unwrap();
    assert!(
        engine
            .execute("assert(isempty(whos));")
            .unwrap()
            .error
            .is_none()
    );
    #[cfg(unix)]
    {
        let cancellation = engine.cancellation_handle();
        let task = thread::spawn(move || {
            thread::sleep(Duration::from_millis(250));
            cancellation.interrupt().unwrap();
        });
        let result = engine
            .execute_with_timeout(
                "retained_after_debug=7; while true; end;",
                Duration::from_secs(5),
            )
            .unwrap();
        task.join().unwrap();
        assert_eq!(result.error.unwrap().identifier, "MathLab:Interrupted");
        // The debugger remains installed and its idle mailbox still responds.
        replacement.set_breakpoint(&source, 2).unwrap();
        replacement.clear_breakpoint(&source, 2).unwrap();
        assert!(
            engine
                .execute("assert(retained_after_debug==7);")
                .unwrap()
                .error
                .is_none()
        );
    }
}

#[test]
fn optional_debugger_quit_stops_source_and_inline_execution_without_aborting_octave() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    for source_mode in [true, false] {
        let Some((mut engine, directory)) = engine() else {
            return;
        };
        let root = fs::canonicalize(directory.path()).unwrap();
        let function = root.join("quitting_function.m");
        fs::write(
            &function,
            "function y=quitting_function(x)\n a=x+1;\n y=a*2;\nend\n",
        )
        .unwrap();
        let script = root.join("quitting_script.m");
        fs::write(
            &script,
            "before_quit=12; quitting_result=quitting_function(4); after_quit=999;\n",
        )
        .unwrap();
        let debugger = engine
            .enable_debugger(std::path::Path::new(&bridge))
            .unwrap();
        debugger.set_breakpoint(&function, 2).unwrap();
        let pid = engine.process_id();
        let task = thread::spawn(move || {
            let result = if source_mode {
                engine.run_file_with_timeout(&script, Duration::from_secs(10))
            } else {
                engine.execute_with_timeout(
                    "before_quit=12; quitting_result=quitting_function(4); after_quit=999;",
                    Duration::from_secs(10),
                )
            };
            (engine, result)
        });
        let pause = wait_debug_pause(&debugger, 2, 0);
        debugger.resume(ResumeAction::Quit, pause.revision).unwrap();
        let (mut engine, result) = task.join().unwrap();
        let result = result.unwrap();
        assert_eq!(result.error.unwrap().identifier, "MathLab:DebugQuit");
        assert_eq!(engine.process_id(), pid);
        assert!(engine.execute_with_timeout("assert(before_quit==12); assert(exist('after_quit','var')==0); assert(exist('quitting_result','var')==0); next_command=3;",Duration::from_secs(5)).unwrap().error.is_none());
        debugger.clear_breakpoint(&function, 2).unwrap();
        assert!(
            engine
                .execute("assert(quitting_function(4)==10);")
                .unwrap()
                .error
                .is_none()
        );
    }
}

#[cfg(unix)]
#[test]
fn optional_debugger_sigint_while_paused_retains_process_and_can_quit_cleanly() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let function = fs::canonicalize(directory.path())
        .unwrap()
        .join("paused_interrupt.m");
    fs::write(
        &function,
        "function y=paused_interrupt(x)\n a=x+1;\n y=a*2;\nend\n",
    )
    .unwrap();
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    debugger.set_breakpoint(&function, 2).unwrap();
    let cancellation = engine.cancellation_handle();
    let pid = engine.process_id();
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "saved_paused=27; paused_result=paused_interrupt(2); after_paused=99;",
            Duration::from_secs(10),
        );
        (engine, result)
    });
    wait_debug_pause(&debugger, 2, 0);
    cancellation.interrupt().unwrap();
    thread::sleep(Duration::from_millis(150));
    let state = debugger.state().unwrap().unwrap();
    if state.paused {
        // SIGINT can be handled by Octave's debugger loop without quitting its
        // paused program. The desktop Stop action should issue native Quit here.
        eprintln!("SIGINT while paused left the debugger paused; native Quit cleanly stops it");
        debugger.resume(ResumeAction::Quit, state.revision).unwrap();
    }
    let (mut engine, result) = task.join().unwrap();
    let result = result.unwrap();
    assert!(matches!(
        result.error.unwrap().identifier.as_str(),
        "MathLab:Interrupted" | "MathLab:DebugQuit"
    ));
    assert_eq!(engine.process_id(), pid);
    assert!(
        engine
            .execute("assert(saved_paused==27); assert(exist('after_paused','var')==0);")
            .unwrap()
            .error
            .is_none()
    );
    debugger.clear_breakpoint(&function, 2).unwrap();
}

#[test]
fn optional_debugger_breaks_in_saved_script_outside_current_folder() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let outside = tempfile::tempdir().unwrap();
    let outside_root = fs::canonicalize(outside.path()).unwrap();
    let script = outside_root.join("outside_script.m");
    fs::write(
        &script,
        "outside_before=4;\noutside_second=outside_before+1;\noutside_result=outside_second*2;\n",
    )
    .unwrap();
    let current = fs::canonicalize(directory.path()).unwrap();
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    assert_eq!(debugger.set_breakpoint(&script, 3).unwrap(), vec![3]);
    let run_path = script.clone();
    let task = thread::spawn(move || {
        let result = engine.run_file_with_timeout(&run_path, Duration::from_secs(10));
        (engine, result)
    });
    let pause = wait_debug_pause(&debugger, 3, 0);
    assert_eq!(pause.file, script);
    assert_eq!(
        debugger
            .evaluate("disp(outside_second);", pause.revision)
            .unwrap(),
        "5\n"
    );
    let vars = debugger.workspace(pause.revision).unwrap();
    assert!(
        vars.iter()
            .any(|variable| variable.name == "outside_before")
    );
    assert!(!vars.iter().any(|variable| variable.name == "request"));
    debugger
        .resume(ResumeAction::Continue, pause.revision)
        .unwrap();
    let (mut engine, result) = task.join().unwrap();
    let result = result.unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(fs::canonicalize(result.working_directory).unwrap(), current);
    assert!(
        engine
            .execute("assert(outside_result==10);")
            .unwrap()
            .error
            .is_none()
    );
    debugger.clear_breakpoint(&script, 3).unwrap();
}

#[test]
fn optional_debugger_validates_protocol_and_rejects_hot_adapter_replacement() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let bridge = std::path::PathBuf::from(bridge).canonicalize().unwrap();
    let debugger = engine.enable_debugger(&bridge).unwrap();
    let debugger_directory = debugger.directory().to_owned();
    let query=engine.execute("bridge_info=mathlab_debug_bridge('protocol',0); assert(strcmp(bridge_info.protocol,'mathlab-debugger')); assert(bridge_info.version==4 && bridge_info.streamed_evaluation && bridge_info.input_wait_cancel); clear bridge_info;").unwrap();
    assert!(query.error.is_none(), "{:?}", query.error);
    assert_eq!(
        engine.enable_debugger(&bridge).unwrap().directory(),
        debugger_directory
    );
    let replacement = directory.path().join("replacement_bridge.oct");
    fs::copy(&bridge, &replacement).unwrap();
    let error = engine.enable_debugger(&replacement).unwrap_err();
    assert!(error.to_string().contains("Restart MathLab"), "{error}");
    assert_eq!(
        engine.debugger_handle().unwrap().directory(),
        debugger_directory
    );
    assert!(debugger_directory.is_dir());
    let function = fs::canonicalize(directory.path())
        .unwrap()
        .join("unchanged_adapter.m");
    fs::write(&function, "function y=unchanged_adapter(x)\n y=x+1;\nend\n").unwrap();
    assert_eq!(debugger.set_breakpoint(&function, 2).unwrap(), vec![2]);
    debugger.clear_breakpoint(&function, 2).unwrap();
    assert!(
        engine
            .execute("assert(unchanged_adapter(4)==5);")
            .unwrap()
            .error
            .is_none()
    );
    // Failed replacement preserves the original restart configuration.
    engine.restart().unwrap();
    assert!(!debugger_directory.exists());
    let restored = engine.enable_debugger(&bridge).unwrap();
    assert_eq!(
        restored.directory(),
        engine.debugger_handle().unwrap().directory()
    );
    assert!(
        engine
            .enable_debugger(&replacement)
            .unwrap_err()
            .to_string()
            .contains("Restart MathLab")
    );
    assert_eq!(restored.set_breakpoint(&function, 2).unwrap(), vec![2]);
    restored.clear_breakpoint(&function, 2).unwrap();
}

#[test]
fn optional_prepared_debugger_breaks_in_saved_and_unsaved_script_local_and_nested_sources() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    for saved in [true, false] {
        let Some((mut engine, directory)) = engine() else {
            return;
        };
        let original = fs::canonicalize(directory.path())
            .unwrap()
            .join("prepared_snapshot.m");
        let source = "pre_debug=11;\ninput_value=3;\nscript_result=calculate(input_value);\nstored_handle=@calculate;\nnested_handle=outer(2);\nnested_result=nested_handle(3);\n\nfunction y=calculate(x)\n intermediate=x+1;\n y=intermediate*2;\nend\nfunction h=outer(offset)\n h=@inner;\n function y=inner(x)\n  y=x+offset;\n end\nend\n";
        if saved {
            fs::write(&original, source).unwrap();
        }
        let debugger = engine
            .enable_debugger(std::path::Path::new(&bridge))
            .unwrap();
        let shared = debugger.clone();
        let descriptor = if saved {
            engine.prepare_debug_file(&original)
        } else {
            engine.prepare_debug_buffer(source, &original)
        }
        .unwrap();
        assert!(descriptor.prepared);
        assert!(engine.workspace().unwrap().is_empty());
        assert_eq!(shared.set_breakpoint(&original, 3).unwrap(), vec![3]);
        assert_eq!(shared.set_breakpoint(&original, 9).unwrap(), vec![9]);
        assert_eq!(shared.set_breakpoint(&original, 15).unwrap(), vec![15]);
        let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
        let run_path = original.clone();
        let task = thread::spawn(move || {
            let result = if saved {
                engine.run_file(&run_path)
            } else {
                engine.run_buffer(source, &run_path)
            };
            (engine, result)
        });
        let script_pause = wait_debug_pause(&debugger, 3, 0);
        assert_eq!(script_pause.file, original);
        assert_eq!(script_pause.function_name, "prepared_snapshot");
        let vars = debugger.workspace(script_pause.revision).unwrap();
        assert!(vars.iter().any(|variable| variable.name == "pre_debug"));
        assert!(!vars.iter().any(|variable| variable.name == "request"));
        debugger
            .resume(ResumeAction::Continue, script_pause.revision)
            .unwrap();
        let local_pause = wait_debug_pause(&debugger, 9, script_pause.revision);
        assert_eq!(local_pause.file, original);
        assert_eq!(local_pause.function_name, "prepared_snapshot>calculate");
        assert_eq!(
            debugger.evaluate("disp(x);", local_pause.revision).unwrap(),
            "3\n"
        );
        let stack = debugger.stack(local_pause.revision).unwrap();
        assert!(
            stack.iter().any(|frame| frame.file == original
                && frame.name == "prepared_snapshot>calculate"
                && frame.line == 9),
            "{stack:?}"
        );
        debugger
            .resume(ResumeAction::Continue, local_pause.revision)
            .unwrap();
        let nested_pause = wait_debug_pause(&debugger, 15, local_pause.revision);
        assert_eq!(nested_pause.file, original);
        assert!(
            nested_pause.function_name.contains("inner"),
            "{nested_pause:?}"
        );
        assert_eq!(
            debugger
                .evaluate("disp(offset);", nested_pause.revision)
                .unwrap(),
            "2\n"
        );
        debugger
            .resume(ResumeAction::Continue, nested_pause.revision)
            .unwrap();
        let (mut engine, result) = task.join().unwrap();
        cleanup.disarm();
        let result = result.unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        shared.clear_breakpoint(&original, 3).unwrap();
        shared.clear_breakpoint(&original, 9).unwrap();
        shared.clear_breakpoint(&original, 15).unwrap();
        assert!(
            engine
                .execute("assert(script_result==8 && nested_result==5);")
                .unwrap()
                .error
                .is_none()
        );
        assert_eq!(original.exists(), saved);
    }
}

#[test]
fn optional_prepared_debugger_registers_active_generation_and_maps_retained_old_handles() {
    prepared_debugger_retained_generation_handles(true);
}

#[test]
fn optional_prepared_debugger_maps_retained_handles_without_debugger_warmup_calls() {
    prepared_debugger_retained_generation_handles(false);
}

fn prepared_debugger_retained_generation_handles(warm_up: bool) {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let original = fs::canonicalize(directory.path())
        .unwrap()
        .join("handle_snapshot.m");
    let old_source = if warm_up {
        "latest_value=calc(3);\nlatest_handle=@calc;\nfunction y=calc(x)\n intermediate=x+1;\n y=intermediate*2;\nend\n"
    } else {
        // Neither the script nor the harness calls either local function before
        // installation: its first actual invocation must retain this generation.
        "latest_value=0;\nlatest_handle=@calc;\nfunction y=calc(x)\n intermediate=x+1;\n y=intermediate*2;\nend\n"
    };
    let new_source = old_source.replace("x+1;", "x+10;");
    assert!(
        engine
            .run_buffer(old_source, &original)
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .execute("old_handle=latest_handle;")
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .run_buffer(&new_source, &original)
            .unwrap()
            .error
            .is_none()
    );
    if warm_up {
        let independent = engine
            .execute("assert(old_handle(4)==10 && latest_handle(4)==28 && latest_value==26);")
            .unwrap();
        assert!(independent.error.is_none(), "{:?}", independent.error);
    }
    // Enabling after preparation must replay the selected older generation,
    // rather than accidentally choosing the most recently allocated cache entry.
    let old_descriptor = engine.prepare_debug_buffer(old_source, &original).unwrap();
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    let shared = debugger.clone();
    assert_eq!(shared.set_breakpoint(&original, 5).unwrap(), vec![5]);
    let new_descriptor = engine.prepare_debug_buffer(&new_source, &original).unwrap();
    assert_eq!(shared.set_breakpoint(&original, 5).unwrap(), vec![5]);
    let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
    let mut task = Some(thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "old_result=old_handle(4); new_result=latest_handle(4);",
            Duration::from_secs(15),
        );
        (engine, result)
    }));
    let old_pause = wait_debug_pause_or_finished(&debugger, 5, 0, &mut task);
    assert_eq!(old_pause.file, original);
    assert_eq!(old_pause.function_name, "handle_snapshot>calc");
    assert_ne!(old_pause.runtime_file, original);
    let old_snapshot = shared
        .source_snapshot(&old_pause.runtime_file)
        .unwrap()
        .unwrap();
    assert_eq!(old_snapshot.original_path, original);
    assert_eq!(old_snapshot.source, old_source);
    let intermediate = debugger
        .evaluate("disp(intermediate);", old_pause.revision)
        .unwrap();
    let prior_result = if intermediate != "5\n" {
        debugger.evaluate(r#"if evalin('base','exist("old_result","var")'); disp(evalin('base','old_result')); else disp('unassigned'); end;"#, old_pause.revision).unwrap()
    } else {
        String::new()
    };
    assert_eq!(
        intermediate,
        "5\n",
        "old scope={}, new scope={}, completed old_result={prior_result:?}, raw state={}",
        old_descriptor.script_path.display(),
        new_descriptor.script_path.display(),
        fs::read_to_string(debugger.directory().join("debug-state.json")).unwrap()
    );
    debugger
        .resume(ResumeAction::Continue, old_pause.revision)
        .unwrap();
    let new_pause = wait_debug_pause_or_finished(&debugger, 5, old_pause.revision, &mut task);
    assert_eq!(new_pause.file, original);
    assert_eq!(new_pause.function_name, "handle_snapshot>calc");
    assert_ne!(new_pause.runtime_file, old_pause.runtime_file);
    let new_snapshot = shared
        .source_snapshot(&new_pause.runtime_file)
        .unwrap()
        .unwrap();
    assert_eq!(new_snapshot.original_path, original);
    assert_eq!(new_snapshot.source, new_source);
    assert_eq!(
        debugger
            .evaluate("disp(intermediate);", new_pause.revision)
            .unwrap(),
        "14\n"
    );
    debugger
        .resume(ResumeAction::Continue, new_pause.revision)
        .unwrap();
    let (mut engine, result) = task.take().unwrap().join().unwrap();
    cleanup.disarm();
    let result = result.unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    shared.clear_breakpoint(&original, 5).unwrap();
    engine.prepare_debug_buffer(old_source, &original).unwrap();
    shared.clear_breakpoint(&original, 5).unwrap();
    assert!(
        engine
            .execute("assert(old_result==10 && new_result==28);")
            .unwrap()
            .error
            .is_none()
    );
    assert!(!original.exists());
}

#[test]
fn optional_native_registry_late_activation_preserves_state_and_rejects_global_shadowing() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    const CHILD: &str = "MATHLAB_LATE_REGISTRY_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        if let Some(scope_bridge) = std::env::var_os("MATHLAB_SCOPE_BRIDGE") {
            command.env("MATHLAB_LATE_SCOPE_ADAPTER", scope_bridge);
        }
        let result = command.env_remove("MATHLAB_SCOPE_BRIDGE").env(CHILD, "1").args([
                "optional_native_registry_late_activation_preserves_state_and_rejects_global_shadowing",
                "--exact",
                "--test-threads=1",
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    assert!(engine.scope_bridge_path().is_none());
    let original = fs::canonicalize(directory.path())
        .unwrap()
        .join("late_registry.m");
    let source =
        "late_count=count_local();\nfunction y=count_local()\n persistent n=0; n=n+1; y=n;\nend\n";
    for expected in [1, 2] {
        assert!(
            engine
                .run_buffer(source, &original)
                .unwrap()
                .error
                .is_none()
        );
        assert!(
            engine
                .execute(&format!("assert(late_count=={expected});"))
                .unwrap()
                .error
                .is_none()
        );
    }
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    let result = engine.execute("assert(late_count==3); registry_home=fileparts(which('mathlab_scope_register')); [~,registry_leaf]=fileparts(registry_home); assert(strcmp(registry_leaf,'registry')); assert(!any(strcmp(strsplit(path(),pathsep()),registry_home))); clear registry_home registry_leaf;").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);

    let pid = engine.process_id();
    let cancellation = engine.cancellation_handle();
    let interrupt = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        cancellation.interrupt().unwrap();
    });
    let interrupted = engine
        .execute_with_timeout(
            "native_stop_saved=17; while true; end;",
            Duration::from_secs(5),
        )
        .unwrap();
    interrupt.join().unwrap();
    assert_eq!(interrupted.error.unwrap().identifier, "MathLab:Interrupted");
    assert_eq!(engine.process_id(), pid);
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    let result = engine.execute("assert(late_count==4 && native_stop_saved==17); assert(!any(strcmp(strsplit(path(),pathsep()),fileparts(which('mathlab_scope_register')))));").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    engine.prepare_debug_buffer(source, &original).unwrap();
    assert_eq!(debugger.set_breakpoint(&original, 1).unwrap(), vec![1]);
    let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
    let run_path = original.clone();
    let mut task = Some(thread::spawn(move || {
        let result = engine.run_buffer(source, &run_path);
        (engine, result)
    }));
    let pause = wait_debug_pause_or_finished(&debugger, 1, 0, &mut task);
    debugger
        .resume(ResumeAction::Continue, pause.revision)
        .unwrap();
    let (returned_engine, result) = task.take().unwrap().join().unwrap();
    cleanup.disarm();
    engine = returned_engine;
    assert!(result.unwrap().error.is_none());
    debugger.clear_breakpoint(&original, 1).unwrap();
    assert!(
        engine
            .execute("assert(late_count==5);")
            .unwrap()
            .error
            .is_none()
    );

    let foreign = directory.path().join("mathlab_scope_register.m");
    fs::write(
        &foreign,
        "function mathlab_scope_register(varargin)\n fid=fopen('foreign_registry_executed','w'); fclose(fid); error('Audit:ForeignRegistry','must not execute');\nend\n",
    )
    .unwrap();
    // Newly written files become visible according to Octave's directory cache.
    // Rehash may itself encounter the guard when the entry was already visible.
    let _ = engine.execute("rehash;");
    let error = engine.execute("late_count=999;").unwrap_err();
    assert!(
        matches!(error, Error::Worker(ref value) if value.identifier == "MathLab:ScopeBridgeRegistry")
    );
    assert!(!directory.path().join("foreign_registry_executed").exists());
    fs::remove_file(foreign).unwrap();
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    let result = engine
        .execute("assert(late_count==6); clear functions;")
        .unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    let result = engine.execute("assert(late_count==1);").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    drop(engine);

    if let Some(scope_bridge) = std::env::var_os("MATHLAB_LATE_SCOPE_ADAPTER") {
        let Some((mut scope_engine, scope_directory)) = self::engine() else {
            return;
        };
        assert!(scope_engine.scope_bridge_path().is_none());
        let original = fs::canonicalize(scope_directory.path())
            .unwrap()
            .join("late_scope.m");
        for expected in [1, 2] {
            assert!(
                scope_engine
                    .run_buffer(source, &original)
                    .unwrap()
                    .error
                    .is_none()
            );
            let result = scope_engine
                .execute(&format!("assert(late_count=={expected});"))
                .unwrap();
            assert!(result.error.is_none(), "{:?}", result.error);
        }
        scope_engine
            .enable_scope_bridge(std::path::Path::new(&scope_bridge))
            .unwrap();
        assert!(
            scope_engine
                .run_buffer(source, &original)
                .unwrap()
                .error
                .is_none()
        );
        let result=scope_engine.execute("assert(late_count==3); assert(!any(strcmp(strsplit(path(),pathsep()),fileparts(which('mathlab_scope_register')))));").unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        let result = scope_engine.execute("function mathlab_scope_register(varargin)\n error('Audit:ForeignInMemory','must not execute');\nendfunction\n").unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        let error = scope_engine.execute("late_count=999;").unwrap_err();
        assert!(
            matches!(error, Error::Worker(ref value) if value.identifier == "MathLab:ScopeBridgeRegistry")
        );
    }
}

#[test]
fn optional_prepared_debugger_ast_cache_does_not_prevent_function_and_all_clear() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let original = fs::canonicalize(directory.path())
        .unwrap()
        .join("debug_counter.m");
    let source = "counter_result=counter();\nfunction y=counter()\n persistent n;\n if isempty(n); n=0; end;\n n=n+1;\n y=n;\nend\n";
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    engine.prepare_debug_buffer(source, &original).unwrap();
    assert_eq!(debugger.set_breakpoint(&original, 1).unwrap(), vec![1]);
    let mut revision = 0;
    for (stage, (clear, expected)) in [
        (None, 1),
        (None, 2),
        (Some("clear functions;"), 1),
        (Some("clear all;"), 1),
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(command) = clear {
            assert!(engine.execute(command).unwrap().error.is_none());
        }
        let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
        let run_path = original.clone();
        let mut task = Some(thread::spawn(move || {
            let result = engine.run_buffer(source, &run_path);
            (engine, result)
        }));
        let observed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            wait_debug_pause_or_finished(&debugger, 1, revision, &mut task)
        }));
        let pause = observed.unwrap_or_else(|failure| {
            eprintln!(
                "AST cache pause failure: stage={stage}, clear={clear:?}, prior_revision={revision}"
            );
            std::panic::resume_unwind(failure)
        });
        assert_eq!(pause.file, original);
        revision = pause.revision;
        debugger.resume(ResumeAction::Continue, revision).unwrap();
        let (returned_engine, result) = task.take().unwrap().join().unwrap();
        cleanup.disarm();
        engine = returned_engine;
        let result = result.unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        let checked = engine
            .execute(&format!("assert(counter_result=={expected});"))
            .unwrap();
        assert!(checked.error.is_none(), "{:?}", checked.error);
    }
    debugger.clear_breakpoint(&original, 1).unwrap();
}

#[test]
fn optional_prepared_debugger_uses_unsaved_plain_snapshot_then_restores_saved_source() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let original = fs::canonicalize(directory.path())
        .unwrap()
        .join("plain_snapshot.m");
    let saved = "input_value=10;\nplain_result=input_value+1;\nlast_value=plain_result;\n";
    let unsaved = "input_value=20;\nplain_result=input_value*3;\nlast_value=plain_result;\n";
    fs::write(&original, saved).unwrap();
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    assert!(
        engine
            .prepare_debug_buffer(unsaved, &original)
            .unwrap()
            .prepared
    );
    assert_eq!(debugger.set_breakpoint(&original, 2).unwrap(), vec![2]);
    let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
    let run_path = original.clone();
    let mut task = Some(thread::spawn(move || {
        let result = engine.run_buffer(unsaved, &run_path);
        (engine, result)
    }));
    let pause = wait_debug_pause_or_finished(&debugger, 2, 0, &mut task);
    assert_eq!(pause.file, original);
    assert_eq!(pause.function_name, "plain_snapshot");
    assert_eq!(
        debugger
            .evaluate("disp(input_value);", pause.revision)
            .unwrap(),
        "20\n"
    );
    debugger.resume(ResumeAction::Step, pause.revision).unwrap();
    let stepped = wait_debug_pause(&debugger, 3, pause.revision);
    assert_eq!(
        debugger
            .evaluate("disp(plain_result);", stepped.revision)
            .unwrap(),
        "60\n"
    );
    debugger
        .resume(ResumeAction::Continue, stepped.revision)
        .unwrap();
    let (mut engine, result) = task.take().unwrap().join().unwrap();
    cleanup.disarm();
    assert!(result.unwrap().error.is_none());
    assert_eq!(fs::read_to_string(&original).unwrap(), saved);
    debugger.clear_breakpoint(&original, 2).unwrap();
    assert!(!engine.prepare_debug_file(&original).unwrap().prepared);
    assert_eq!(debugger.set_breakpoint(&original, 2).unwrap(), vec![2]);
    let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
    let run_path = original.clone();
    let mut task = Some(thread::spawn(move || {
        let result = engine.run_file_with_timeout(&run_path, Duration::from_secs(15));
        (engine, result)
    }));
    let pause = wait_debug_pause_or_finished(&debugger, 2, stepped.revision, &mut task);
    assert_eq!(pause.file, original);
    assert_eq!(
        debugger
            .evaluate("disp(input_value);", pause.revision)
            .unwrap(),
        "10\n"
    );
    debugger
        .resume(ResumeAction::Continue, pause.revision)
        .unwrap();
    let (mut engine, result) = task.take().unwrap().join().unwrap();
    cleanup.disarm();
    assert!(result.unwrap().error.is_none());
    debugger.clear_breakpoint(&original, 2).unwrap();
    assert!(
        engine
            .execute("assert(plain_result==11);")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn optional_prepared_debugger_first_pause_can_be_a_forward_local_function() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let original = fs::canonicalize(directory.path())
        .unwrap()
        .join("forward_local.m");
    let source =
        "forward_value=square_local(3);\nfunction y=square_local(x)\n z=x+1;\n y=z*x;\nend\n";
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    engine.prepare_debug_buffer(source, &original).unwrap();
    assert_eq!(debugger.set_breakpoint(&original, 4).unwrap(), vec![4]);
    let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
    let run_path = original.clone();
    let task = thread::spawn(move || {
        let result = engine.run_buffer(source, &run_path);
        (engine, result)
    });
    let pause = wait_debug_pause(&debugger, 4, 0);
    assert_eq!(pause.file, original);
    assert_eq!(pause.function_name, "forward_local>square_local");
    assert_eq!(
        debugger.evaluate("disp([x,z]);", pause.revision).unwrap(),
        "   3   4\n"
    );
    debugger
        .resume(ResumeAction::Continue, pause.revision)
        .unwrap();
    let (mut engine, result) = task.join().unwrap();
    cleanup.disarm();
    assert!(result.unwrap().error.is_none());
    debugger.clear_breakpoint(&original, 4).unwrap();
    assert!(
        engine
            .execute("assert(forward_value==12);")
            .unwrap()
            .error
            .is_none()
    );
    assert!(!original.exists());
}

#[test]
fn optional_prepared_debugger_honors_dbclear_all_and_registry_clear_lifetime() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let original = fs::canonicalize(directory.path())
        .unwrap()
        .join("host_clear_counter.m");
    let source =
        "count_result=counter();\nfunction y=counter()\n persistent n=0;\n n=n+1;\n y=n;\nend\n";
    fs::write(&original, source).unwrap();
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    engine.prepare_debug_file(&original).unwrap();
    debugger.set_breakpoint(&original, 4).unwrap();
    let mut revision = 0;
    for reset in [false, true] {
        if reset {
            debugger.set_breakpoint(&original, 4).unwrap();
            assert!(engine.execute("clear functions;").unwrap().error.is_none());
        }
        let run_path = original.clone();
        let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
        let task = thread::spawn(move || {
            let result = engine.run_file_with_timeout(&run_path, Duration::from_secs(15));
            (engine, result)
        });
        let pause = wait_debug_pause(&debugger, 4, revision);
        revision = pause.revision;
        assert_eq!(debugger.evaluate("disp(n);", revision).unwrap(), "0\n");
        debugger.resume(ResumeAction::Continue, revision).unwrap();
        let (returned, result) = task.join().unwrap();
        cleanup.disarm();
        engine = returned;
        assert!(result.unwrap().error.is_none());
        let status = engine.execute("assert(count_result==1); bp=dbstatus(); assert(!isempty(bp)); clear bp; assert(exist('counter','file')==0); dbclear all; assert(isempty(dbstatus()));").unwrap();
        assert!(status.error.is_none(), "{:?}", status.error);
        let next = engine
            .run_file_with_timeout(&original, Duration::from_secs(3))
            .unwrap();
        assert!(next.error.is_none(), "{:?}", next.error);
        assert!(engine.execute("assert(count_result==2); assert(isempty(dbstatus())); assert(exist('counter','file')==0);").unwrap().error.is_none());
    }
}

#[test]
fn optional_prepared_debugger_host_clear_preserves_ordinary_and_individual_breakpoints() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let root = fs::canonicalize(directory.path()).unwrap();
    let ordinary = root.join("ordinary_unused.m");
    fs::write(&ordinary, "function y=ordinary_unused()\n y=7;\nend\n").unwrap();
    let original = root.join("specific_counter.m");
    fs::write(
        &original,
        "count_result=counter();\nfunction y=counter()\n persistent n=0;\n n=n+1;\n y=n;\nend\n",
    )
    .unwrap();
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    engine.prepare_debug_file(&original).unwrap();
    debugger.set_breakpoint(&original, 4).unwrap();
    debugger.set_breakpoint(&ordinary, 2).unwrap();
    let run_path = original.clone();
    let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
    let task = thread::spawn(move || {
        let result = engine.run_file_with_timeout(&run_path, Duration::from_secs(15));
        (engine, result)
    });
    let pause = wait_debug_pause(&debugger, 4, 0);
    debugger
        .resume(ResumeAction::Continue, pause.revision)
        .unwrap();
    let (returned, result) = task.join().unwrap();
    cleanup.disarm();
    engine = returned;
    assert!(result.unwrap().error.is_none());
    let cleared = engine.execute("dbclear('mathlab_scope_register'); bp=dbstatus(); assert(!isempty(bp)); assert(any(strcmp({bp.name},'ordinary_unused'))); clear bp;").unwrap();
    assert!(cleared.error.is_none(), "{:?}", cleared.error);
    assert!(
        engine
            .run_file_with_timeout(&original, Duration::from_secs(3))
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .execute("assert(count_result==2);")
            .unwrap()
            .error
            .is_none()
    );
    debugger.set_breakpoint(&original, 4).unwrap();
    debugger.clear_breakpoint(&original, 4).unwrap();
    assert!(
        engine
            .run_file_with_timeout(&original, Duration::from_secs(3))
            .unwrap()
            .error
            .is_none()
    );
    assert!(engine.execute("assert(count_result==3); bp=dbstatus(); assert(any(strcmp({bp.name},'ordinary_unused'))); clear bp;").unwrap().error.is_none());
    debugger.clear_breakpoint(&ordinary, 2).unwrap();
    assert!(
        engine
            .execute("assert(isempty(dbstatus()));")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn optional_prepared_debugger_synchronizes_cleared_old_handles_before_new_requests() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let root = fs::canonicalize(directory.path()).unwrap();
    let original = root.join("direct_counter.m");
    fs::write(&original, "saved_handle=@counter; count_result=counter();\nfunction y=counter()\n persistent n=0;\n n=n+1;\n y=n;\nend\n").unwrap();
    let ordinary = root.join("ordinary_after_clear.m");
    fs::write(&ordinary, "function y=ordinary_after_clear()\n y=7;\nend\n").unwrap();
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    engine.prepare_debug_file(&original).unwrap();
    debugger.set_breakpoint(&original, 4).unwrap();
    let run_path = original.clone();
    let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
    let task = thread::spawn(move || {
        let result = engine.run_file_with_timeout(&run_path, Duration::from_secs(15));
        (engine, result)
    });
    let pause = wait_debug_pause(&debugger, 4, 0);
    debugger
        .resume(ResumeAction::Continue, pause.revision)
        .unwrap();
    let (returned, result) = task.join().unwrap();
    cleanup.disarm();
    engine = returned;
    assert!(result.unwrap().error.is_none());
    assert!(engine.execute("dbclear all;").unwrap().error.is_none());
    debugger.set_breakpoint(&ordinary, 2).unwrap();
    // A later direct old-handle call must not stop on an AST mark whose global
    // bookkeeping was removed, even though an ordinary breakpoint enables debug.
    let direct = engine
        .execute_with_timeout(
            "direct_result=saved_handle(); assert(direct_result==2);",
            Duration::from_secs(3),
        )
        .unwrap();
    assert!(direct.error.is_none(), "{:?}", direct.error);
    let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "ordinary_result=ordinary_after_clear();",
            Duration::from_secs(15),
        );
        (engine, result)
    });
    let ordinary_pause = wait_debug_pause(&debugger, 2, pause.revision);
    assert_eq!(ordinary_pause.file, ordinary);
    debugger
        .resume(ResumeAction::Continue, ordinary_pause.revision)
        .unwrap();
    let (mut engine, result) = task.join().unwrap();
    cleanup.disarm();
    assert!(result.unwrap().error.is_none());
    debugger.clear_breakpoint(&ordinary, 2).unwrap();
    assert!(
        engine
            .execute("assert(ordinary_result==7);")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn optional_prepared_handle_quit_preserves_process_workspace_and_next_command() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let original = fs::canonicalize(directory.path())
        .unwrap()
        .join("quit_handle.m");
    let source =
        "prepared_before=12; stored_handle=@calculate;\nfunction y=calculate(x)\n y=x+1;\nend\n";
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    engine.prepare_debug_buffer(source, &original).unwrap();
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    debugger.set_breakpoint(&original, 3).unwrap();
    let pid = engine.process_id();
    let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "before_handle_quit=23; handle_result=stored_handle(4); after_handle_quit=999;",
            Duration::from_secs(15),
        );
        (engine, result)
    });
    let pause = wait_debug_pause(&debugger, 3, 0);
    assert_eq!(pause.file, original);
    debugger.resume(ResumeAction::Quit, pause.revision).unwrap();
    let (mut engine, result) = task.join().unwrap();
    cleanup.disarm();
    assert_eq!(
        result.unwrap().error.unwrap().identifier,
        "MathLab:DebugQuit"
    );
    assert_eq!(engine.process_id(), pid);
    debugger.clear_breakpoint(&original, 3).unwrap();
    let next = engine.execute("assert(prepared_before==12 && before_handle_quit==23); assert(exist('handle_result','var')==0 && exist('after_handle_quit','var')==0); assert(stored_handle(7)==8); after_quit_command=4;").unwrap();
    assert!(next.error.is_none(), "{:?}", next.error);
    assert!(!original.exists());
}

#[test]
fn optional_prepared_debugger_parse_error_preserves_workspace_and_breakpoints() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let original = fs::canonicalize(directory.path())
        .unwrap()
        .join("parse_handle.m");
    let source =
        "parse_retained=17; parse_handle=@calculate;\nfunction y=calculate(x)\n y=x+1;\nend\n";
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    engine.prepare_debug_buffer(source, &original).unwrap();
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    debugger.set_breakpoint(&original, 3).unwrap();
    let pid = engine.process_id();
    let malformed = engine.execute("unclosed_value=[;").unwrap();
    let parse_error = malformed.error.expect("malformed code must fail");
    assert_ne!(parse_error.identifier, "MathLab:DebugQuit");
    assert_eq!(engine.process_id(), pid);
    assert!(
        engine
            .execute("assert(parse_retained==17 && exist('unclosed_value','var')==0);")
            .unwrap()
            .error
            .is_none()
    );
    let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
    let task = thread::spawn(move || {
        let result =
            engine.execute_with_timeout("after_parse=parse_handle(6);", Duration::from_secs(15));
        (engine, result)
    });
    let pause = wait_debug_pause(&debugger, 3, 0);
    assert_eq!(pause.file, original);
    assert_eq!(
        debugger.evaluate("disp(x);", pause.revision).unwrap(),
        "6\n"
    );
    debugger
        .resume(ResumeAction::Continue, pause.revision)
        .unwrap();
    let (mut engine, result) = task.join().unwrap();
    cleanup.disarm();
    assert!(result.unwrap().error.is_none());
    debugger.clear_breakpoint(&original, 3).unwrap();
    assert!(
        engine
            .execute("assert(after_parse==7 && parse_retained==17);")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn persistent_workspace_executes_matrix_code_and_keeps_protocol_variables_private() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    let result = engine
        .execute("A = [2 1; 1 3]; b = [1; 2]; x = A \\ b; disp(x);")
        .unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert!(result.output.contains("0.2000"), "{}", result.output);
    assert_eq!(result.workspace.len(), 3);
    let matrix = result
        .workspace
        .iter()
        .find(|entry| entry.name == "A")
        .unwrap();
    assert_eq!(matrix.dimensions, vec![2, 2]);
    assert_eq!(matrix.class_name, "double");
    let result = engine
        .execute("assert(norm(A*x-b) < 1e-12); y = x(2);")
        .unwrap();
    assert!(result.error.is_none());
    assert!(
        engine
            .workspace()
            .unwrap()
            .iter()
            .any(|entry| entry.name == "y")
    );
    engine.clear().unwrap();
    assert!(engine.workspace().unwrap().is_empty());
}

#[test]
fn subprocess_output_resembling_protocol_frames_stays_console_text() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    for (source, expected_output) in [
        (
            r#"system('printf ''__MATHLAB_JSON__not a response\n'''); marker_value=7;"#,
            "__MATHLAB_JSON__not a response",
        ),
        (
            r#"system('printf ''__MATHLAB_JSON__{"id":3,"ok":true,"result":[]}\n'''); marker_value=7;"#,
            r#"__MATHLAB_JSON__{"id":3,"ok":true,"result":[]}"#,
        ),
        (
            r#"system('printf ''__MATHLAB_JSON__unterminated output'''); marker_value=7;"#,
            "__MATHLAB_JSON__unterminated output",
        ),
    ] {
        let result = engine.execute(source).unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        assert!(result.output.contains(expected_output), "{}", result.output);
        // The real worker response was consumed exactly once; the next command
        // must see both the assignment and the correct response identifier.
        let next = engine
            .execute("assert(marker_value==7); marker_value=8;")
            .unwrap();
        assert!(next.error.is_none(), "{:?}", next.error);
    }
}

#[test]
fn errors_preserve_console_output_and_session_remains_usable() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let result = engine
        .execute("disp('before failure'); error('MathLab:Example', 'deliberate failure');")
        .unwrap();
    assert!(result.output.contains("before failure"));
    let error = result.error.unwrap();
    assert_eq!(error.identifier, "MathLab:Example");
    assert!(error.message.contains("deliberate failure"));
    let result = engine.execute("healthy = 42; disp(healthy);").unwrap();
    assert!(result.error.is_none());
    assert!(result.output.contains("42"));
    let script = directory.path().join("broken_script.m");
    fs::write(
        &script,
        "number = 1;\nerror('MathLab:Script', 'source frame');\n",
    )
    .unwrap();
    let result = engine.run_file(&script).unwrap();
    assert!(
        result
            .error
            .unwrap()
            .stack
            .iter()
            .any(|frame| frame.file.ends_with("broken_script.m") && frame.line == 2)
    );
}

#[test]
fn native_m_file_and_mat_file_roundtrip_with_quoted_directory() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let quoted = directory.path().join("it's a scientific project");
    fs::create_dir(&quoted).unwrap();
    engine.set_directory(&quoted).unwrap();
    fs::write(
        quoted.join("sample.m"),
        "A = [2 1; 1 3]; b = [1; 2]; x = A \\ b;\n",
    )
    .unwrap();
    let result = engine.run_file(std::path::Path::new("sample.m")).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    let result = engine.execute("save('-mat7-binary', 'roundtrip.mat', 'A', 'x'); clear A x; load('roundtrip.mat'); assert(norm(A*x-[1;2]) < 1e-12);").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    let bytes = fs::read(quoted.join("roundtrip.mat")).unwrap();
    assert!(bytes.starts_with(b"MATLAB 5.0 MAT-file"));
    assert_eq!(
        fs::canonicalize(result.working_directory).unwrap(),
        fs::canonicalize(&quoted).unwrap()
    );
    fs::create_dir(quoted.join("nested")).unwrap();
    engine
        .set_directory(std::path::Path::new("nested"))
        .unwrap();
    assert_eq!(
        fs::canonicalize(engine.working_directory()).unwrap(),
        fs::canonicalize(quoted.join("nested")).unwrap()
    );
}

#[test]
fn figures_are_real_png_exports_and_closed_figures_disappear() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    let result = engine.execute("figure('name','Signal'); t=linspace(0,2*pi,30); plot(t,sin(t)); title('Sine wave'); grid on;").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(result.figures.len(), 1, "{:?}", result.figure_warnings);
    assert_eq!(result.figures[0].title, "Signal");
    assert!(
        fs::read(&result.figures[0].path)
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
    let result = engine.execute("close all;").unwrap();
    assert!(result.figures.is_empty());
}

#[test]
fn deadlines_and_hard_termination_allow_a_clean_restart() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let cancel = engine.cancellation_handle();
    let cancellation = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        cancel.terminate().unwrap();
    });
    assert!(engine.execute("pause(30);").is_err());
    cancellation.join().unwrap();
    engine.restart().unwrap();
    assert!(engine.workspace().unwrap().is_empty());
    assert!(engine.execute("restarted = true;").unwrap().error.is_none());
    assert!(matches!(
        engine.execute_with_timeout("pause(30);", Duration::from_millis(200)),
        Err(Error::Timeout)
    ));
    engine.restart().unwrap();
    let long_script = directory.path().join("long_running.m");
    fs::write(&long_script, "pause(30);\n").unwrap();
    assert!(matches!(
        engine.run_file_with_timeout(&long_script, Duration::from_millis(200)),
        Err(Error::Timeout)
    ));
    engine.restart().unwrap();
    assert!(
        engine
            .execute("assert(2 + 2 == 4);")
            .unwrap()
            .error
            .is_none()
    );
}

#[cfg(unix)]
#[test]
fn interruption_preserves_workspace_and_allows_fresh_requests() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    engine.execute("saved_matrix = [1 2; 3 4];").unwrap();
    let pid = engine.process_id();
    let cancel = engine.cancellation_handle();
    let cancellation = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        cancel.interrupt().unwrap();
        cancel.interrupt().unwrap(); // A repeated Stop click must not kill recovery.
    });
    let result = engine
        .execute_with_timeout(
            "fprintf(stdout,'PARTIAL_BEFORE_INTERRUPT'); iteration = 0; while true; iteration = iteration + 1; end;",
            Duration::from_secs(5),
        )
        .unwrap();
    cancellation.join().unwrap();
    assert_eq!(result.output, "PARTIAL_BEFORE_INTERRUPT");
    assert_eq!(result.error.unwrap().identifier, "MathLab:Interrupted");
    assert!(
        result
            .workspace
            .iter()
            .any(|variable| variable.name == "saved_matrix")
    );
    assert!(
        result
            .workspace
            .iter()
            .any(|variable| variable.name == "iteration")
    );
    assert_eq!(engine.process_id(), pid);
    // Metadata and multiple execution requests verify recovery has no stale id=0 replies.
    assert_eq!(
        engine
            .read_variable("saved_matrix", 1, 1, 2, 2)
            .unwrap()
            .cells[1][1],
        "4"
    );
    for index in 0..3 {
        let code = format!("assert(isequal(saved_matrix,[1 2; 3 4])); after_stop={index};");
        assert!(
            engine
                .execute_with_timeout(&code, Duration::from_secs(5))
                .unwrap()
                .error
                .is_none()
        );
    }
    let script = directory.path().join("infinite_script.m");
    fs::write(&script, "script_saved=7; while true; end;\n").unwrap();
    let cancel = engine.cancellation_handle();
    let cancellation = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        cancel.interrupt().unwrap();
    });
    let result = engine
        .run_file_with_timeout(&script, Duration::from_secs(5))
        .unwrap();
    cancellation.join().unwrap();
    assert_eq!(result.error.unwrap().identifier, "MathLab:Interrupted");
    assert!(
        engine
            .execute_with_timeout("assert(script_saved==7);", Duration::from_secs(5))
            .unwrap()
            .error
            .is_none()
    );
    // Interrupting an idle session is harmless.
    engine.cancellation_handle().interrupt().unwrap();
    assert!(
        engine
            .execute_with_timeout("assert(saved_matrix(2,2)==4);", Duration::from_secs(5))
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn variable_pages_are_bounded_and_preserve_shape_and_scalar_values() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    let result = engine.execute("A=reshape(1:60,10,6); S=sparse([1 3],[2 1],[8 9],3,3); L=logical([0 1;1 0]); N=reshape(1:24,3,4,2); E=zeros(0,2); z=[NaN Inf 1+2i];").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    let page = engine.read_variable("A", 8, 5, 30, 12).unwrap();
    assert_eq!(page.dimensions, vec![10, 6]);
    assert_eq!(
        (page.row_start, page.column_start, page.rows, page.columns),
        (8, 5, 3, 2)
    );
    assert_eq!(
        page.cells,
        vec![vec!["48", "58"], vec!["49", "59"], vec!["50", "60"]]
    );
    assert!(page.editable);
    assert_eq!(
        engine.read_variable("S", 1, 1, 3, 3).unwrap().cells[0],
        vec!["0", "8", "0"]
    );
    assert_eq!(
        engine.read_variable("L", 1, 1, 2, 2).unwrap().cells[0],
        vec!["false", "true"]
    );
    let nd = engine.read_variable("N", 1, 1, 3, 4).unwrap();
    assert_eq!(nd.dimensions, vec![3, 4, 2]);
    assert_eq!(nd.cells[2][3], "12");
    assert!(!nd.editable);
    let empty = engine.read_variable("E", 1, 1, 30, 12).unwrap();
    assert_eq!((empty.rows, empty.columns), (0, 2));
    assert!(empty.cells.is_empty());
    assert_eq!(
        engine.read_variable("z", 1, 1, 1, 3).unwrap().cells[0],
        vec!["NaN", "Inf", "1+2i"]
    );
    assert!(matches!(
        engine.read_variable("A", 1, 1, 100, 100),
        Err(Error::LimitExceeded(_))
    ));
    assert!(engine.read_variable("A", 0, 1, 1, 1).is_err());
    assert!(matches!(
        engine.read_variable("A", 11, 1, 1, 1),
        Err(Error::Worker(_))
    ));
    assert!(engine.read_variable("A;clear", 1, 1, 1, 1).is_err());
    assert!(matches!(
        engine.read_variable("pi", 1, 1, 1, 1),
        Err(Error::Worker(_))
    ));
    assert!(
        engine
            .execute("assert(A(10,6)==60);")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn variable_editor_accepts_literals_preserves_class_and_rejects_code() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    engine.execute("A=[1 2;3 4]; F=single([1 2]); I=int16([1 2]); L=logical([0 1]); S=speye(2); N=zeros(2,2,2); C={1,'hello'}; T=struct('a',1); label='hi'; big_i=int64(0); big_u=uint64(0);").unwrap();
    engine
        .set_variable_element("A", 2, 1, "1.5 - 2e-3i")
        .unwrap();
    engine.set_variable_element("A", 1, 2, "NaN").unwrap();
    engine.set_variable_element("A", 2, 2, "-Inf").unwrap();
    engine.set_variable_element("F", 1, 2, "3.25").unwrap();
    engine.set_variable_element("I", 1, 1, "42").unwrap();
    engine.set_variable_element("L", 1, 1, "true").unwrap();
    engine.set_variable_element("S", 2, 1, "7").unwrap();
    engine
        .set_variable_element("big_i", 1, 1, "-9223372036854775808")
        .unwrap();
    engine
        .set_variable_element("big_u", 1, 1, "18446744073709551615")
        .unwrap();
    assert_eq!(
        engine.read_variable("big_i", 1, 1, 1, 1).unwrap().cells[0][0],
        "-9223372036854775808"
    );
    assert_eq!(
        engine.read_variable("big_u", 1, 1, 1, 1).unwrap().cells[0][0],
        "18446744073709551615"
    );
    assert!(
        engine
            .execute("assert(big_i==intmin('int64')); assert(big_u==intmax('uint64'));")
            .unwrap()
            .error
            .is_none()
    );
    let result = engine.execute("assert(A(2,1)==1.5-0.002i); assert(isnan(A(1,2))); assert(A(2,2)==-Inf); assert(isa(F,'single') && F(1,2)==single(3.25)); assert(isa(I,'int16') && I(1)==42); assert(islogical(L) && L(1)); assert(issparse(S) && S(2,1)==7); assert(isequal(size(A),[2 2]));").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    for literal in [
        "clear",
        "1;clear",
        "sin(0)",
        "[1 2]",
        "1+2+3i",
        "system('echo bad')",
    ] {
        assert!(
            engine.set_variable_element("A", 1, 1, literal).is_err(),
            "{literal}"
        );
    }
    assert!(engine.set_variable_element("I", 1, 1, "1+2i").is_err());
    assert!(engine.set_variable_element("A", 3, 1, "9").is_err());
    assert!(engine.set_variable_element("N", 1, 1, "9").is_err());
    assert!(engine.set_variable_element("C", 1, 1, "9").is_err());
    assert!(engine.read_variable("C", 1, 1, 1, 2).unwrap().cells[0][1].contains("hello"));
    assert!(!engine.read_variable("C", 1, 1, 1, 2).unwrap().editable);
    assert!(!engine.read_variable("T", 1, 1, 1, 1).unwrap().editable);
    assert!(!engine.read_variable("label", 1, 1, 1, 2).unwrap().editable);
    assert!(
        engine
            .execute("assert(I(1)==42); assert(A(1,1)==1);")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn unchanged_script_local_functions_run_with_file_scope_and_retained_handles() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let fixture = include_str!("../../../tests/fixtures/script_local_functions.m");
    let script = directory.path().join("script_local_functions.m");
    fs::write(&script, fixture).unwrap();
    let result = engine.run_file(&script).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    let result = engine.execute("assert(local_sum==10 && local_mean==2.5); assert(local_handle([3 7])==10); assert(exist('local_statistics','file')==0); assert(exist('local_total','file')==0);").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
}

#[test]
fn unsaved_buffers_map_script_and_local_error_frames_to_original_source() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let path = fs::canonicalize(directory.path())
        .unwrap()
        .join("unsaved_script.m");
    let source = "buffer_result=twice(4); buffer_handle=@twice;\n\nfunction y=twice(x)\n y=helper(x);\nend\nfunction y=helper(x)\n y=2*x;\nend\n";
    let result = engine.run_buffer(source, &path).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert!(!path.exists(), "running an unsaved buffer must not save it");
    assert!(engine.execute("assert(buffer_result==8); assert(buffer_handle(5)==10); assert(exist('twice','file')==0); assert(exist('helper','file')==0);").unwrap().error.is_none());
    let failing = "error_handle=@failure;\nfailure(2);\n\nfunction y=failure(x)\n y=x;\n error('MathLab:Mapped','failure in local source');\nend\n";
    let result = engine.run_buffer(failing, &path).unwrap();
    let error = result.error.unwrap();
    assert_eq!(error.identifier, "MathLab:Mapped");
    assert!(
        error
            .stack
            .iter()
            .any(|frame| frame.file == path.to_string_lossy()
                && frame.line == 6
                && frame.name == "unsaved_script>failure"),
        "{:?}",
        error.stack
    );
    assert!(
        error
            .stack
            .iter()
            .any(|frame| frame.file == path.to_string_lossy() && frame.line == 2),
        "{:?}",
        error.stack
    );
    // The mapping for an old function handle survives subsequent generated buffers.
    engine.run_buffer("plain_buffer=3;\n", &path).unwrap();
    let error = engine.execute("error_handle(7);").unwrap().error.unwrap();
    assert!(
        error
            .stack
            .iter()
            .any(|frame| frame.file == path.to_string_lossy()
                && frame.line == 6
                && frame.name == "unsaved_script>failure"),
        "{:?}",
        error.stack
    );
    assert!(
        engine
            .execute("assert(buffer_handle(6)==12);")
            .unwrap()
            .error
            .is_none()
    );
    let plain = engine
        .run_buffer(
            "disp('before error');\nerror('MathLab:Buffer','plain source failure');\n",
            &path,
        )
        .unwrap();
    assert!(plain.output.contains("before error"));
    assert!(
        plain
            .error
            .unwrap()
            .stack
            .iter()
            .any(|frame| frame.file == path.to_string_lossy() && frame.line == 2)
    );
}

#[test]
fn debug_source_preparation_is_immutable_and_never_executes_user_statements() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let source =
        "must_not_execute=99; prepared_value=local(2);\nfunction y=local(x)\n y=x*3;\nend\n";
    let original = directory.path().join("prepare_only.m");
    let prepared = engine.prepare_debug_buffer(source, &original).unwrap();
    assert!(prepared.prepared);
    assert_ne!(prepared.script_path, prepared.original_path);
    assert_eq!(prepared.script_path.extension().unwrap(), "mlsource");
    assert!(engine.workspace().unwrap().is_empty());
    assert!(!original.exists());
    let modified = fs::metadata(&prepared.script_path)
        .unwrap()
        .modified()
        .unwrap();
    let repeated = engine.prepare_debug_buffer(source, &original).unwrap();
    assert_eq!(repeated.script_path, prepared.script_path);
    assert_eq!(
        fs::metadata(&repeated.script_path)
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );
    fs::write(&original, source).unwrap();
    assert_eq!(
        engine.prepare_debug_file(&original).unwrap().script_path,
        prepared.script_path
    );
    assert!(engine.workspace().unwrap().is_empty());
    let plain = directory.path().join("plain.m");
    fs::write(&plain, "plain_must_not_run=44;\n").unwrap();
    let staged_plain = engine
        .prepare_debug_buffer("plain_must_not_run=44;\n", &plain)
        .unwrap();
    assert!(staged_plain.prepared);
    let direct = engine.prepare_debug_file(&plain).unwrap();
    assert!(
        !direct.prepared,
        "ordinary saved files remain direct even after preparing their buffer"
    );
    assert_eq!(direct.script_path, direct.original_path);
    let function = directory.path().join("ordinary_function.m");
    fs::write(&function, "function y=ordinary_function(x)\n y=x+1;\nend\n").unwrap();
    assert!(!engine.prepare_debug_file(&function).unwrap().prepared);
    assert!(
        engine
            .prepare_debug_buffer(
                "% comment\nfunction y=f(x)\ny=x;\nend\n",
                &directory.path().join("unsaved_function.m")
            )
            .unwrap_err()
            .to_string()
            .contains("Save function and class")
    );
    assert!(
        engine
            .prepare_debug_buffer(
                "%{\nignored function words\n%}\nclassdef Example\nend\n",
                &directory.path().join("Example.m")
            )
            .unwrap_err()
            .to_string()
            .contains("Save function and class")
    );
    assert!(engine.workspace().unwrap().is_empty());
}

#[test]
fn prepared_scripts_preserve_user_path_changes_and_isolate_same_named_locals() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let user_path = directory.path().join("user_functions");
    fs::create_dir(&user_path).unwrap();
    fs::write(user_path.join("same.m"), "function y=same(x)\ny=99;\nend\n").unwrap();
    let user_path_literal = fs::canonicalize(&user_path)
        .unwrap()
        .to_string_lossy()
        .replace('\'', "''");
    let first_path = directory.path().join("first.m");
    let first = format!(
        "addpath('{user_path_literal}'); first=same(5); first_handle=@same;\nfunction y=same(x)\n y=x*2;\nend\n"
    );
    assert!(
        engine
            .run_buffer(&first, &first_path)
            .unwrap()
            .error
            .is_none()
    );
    let result=engine.execute(&format!("assert(any(strcmp(strsplit(path(),pathsep()),'{user_path_literal}'))); assert(first==10); assert(same(5)==99); assert(first_handle(5)==10);")).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    let second_path = directory.path().join("second.m");
    let second = "second=same(5); second_handle=@same;\nfunction y=same(x)\n y=x*3;\nend\n";
    assert!(
        engine
            .run_buffer(second, &second_path)
            .unwrap()
            .error
            .is_none()
    );
    let result = engine.execute("assert(second==15); assert(first_handle(7)==14); assert(second_handle(7)==21); assert(same(7)==99);").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    let bad = "failure=same(2);\nfunction y=same(x)\n error('MathLab:Cleanup','cleanup');\nend\n";
    assert!(engine.run_buffer(bad, &first_path).unwrap().error.is_some());
    assert!(
        engine
            .execute("assert(same(1)==99);")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn nested_local_errors_keep_original_lines_and_enclosing_function_names() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let original = fs::canonicalize(directory.path())
        .unwrap()
        .join("nested_error.m");
    let source = "h=outer(2);\nnested_value=h(3);\nfunction h=outer(offset)\n h=@inner;\n function y=inner(x)\n  error('MathLab:NestedError','nested failure');\n  y=x+offset;\n end\nend\n";
    let result = engine.run_buffer(source, &original).unwrap();
    let error = result.error.unwrap();
    assert_eq!(error.identifier, "MathLab:NestedError");
    assert!(
        error.stack.iter().any(|frame| {
            std::path::Path::new(&frame.file) == original
                && frame.name == "nested_error>outer>inner"
                && frame.line == 6
        }),
        "{error:?}"
    );
    assert!(
        error.stack.iter().any(|frame| {
            std::path::Path::new(&frame.file) == original
                && frame.name == "nested_error"
                && frame.line == 2
        }),
        "{error:?}"
    );
    assert!(
        engine
            .execute("assert(exist('h','var')==1); stable_after_error=4;")
            .unwrap()
            .error
            .is_none()
    );
    assert!(!original.exists());
}

#[test]
fn cached_local_functions_keep_persistent_state_and_edited_generations_keep_old_handles() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let path = fs::canonicalize(directory.path())
        .unwrap()
        .join("counter_script.m");
    let source = "counter_result=counter(); old_counter=@counter;\nfunction y=counter()\n persistent n;\n if isempty(n); n=0; end;\n n=n+1; y=n;\nend\n";
    assert!(engine.run_buffer(source, &path).unwrap().error.is_none());
    assert!(engine.run_buffer(source, &path).unwrap().error.is_none());
    let result=engine.execute("assert(counter_result==2); assert(old_counter()==3); assert(exist('counter','file')==0); assert(exist('counter_script','file')==0); internal_entries=strsplit(path(),pathsep()); for idx=1:numel(internal_entries); if !isempty(strfind(internal_entries{idx},'generation-')); files=dir(fullfile(internal_entries{idx},'*.m')); assert(numel(files)==1 && !isempty(strfind(files.name,'_factory.m'))); end; end; clear internal_entries idx files;").unwrap();
    assert!(
        result.error.is_none(),
        "{:?}; output={}",
        result.error,
        result.output
    );
    // Saving that same buffer shares the existing generated scope.
    fs::write(&path, source).unwrap();
    assert!(engine.run_file(&path).unwrap().error.is_none());
    assert!(
        engine
            .execute("assert(counter_result==4);")
            .unwrap()
            .error
            .is_none()
    );
    engine.clear().unwrap();
    assert!(engine.run_file(&path).unwrap().error.is_none());
    let result = engine.execute("assert(counter_result==5);").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    let changed = "edited_result=counter(); new_counter=@counter;\nfunction y=counter()\n persistent n;\n if isempty(n); n=100; end;\n n=n+1; y=n;\nend\n";
    assert!(engine.run_buffer(changed, &path).unwrap().error.is_none());
    let result=engine.execute("assert(edited_result==101); assert(new_counter()==102); assert(old_counter()==6); retained_original=old_counter; assert(exist('counter','file')==0);").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert!(engine.run_file(&path).unwrap().error.is_none());
    // Fallback deviation: after another generated scope uses this same local name,
    // Octave's named private lookup rebinds the older script to a fresh function
    // object. Explicit retained handles still use their original persistent state.
    let rebound_code = if engine.scope_bridge_path().is_some() {
        "disp(counter_result); assert(counter_result==7); assert(retained_original()==8); assert(new_counter()==103);"
    } else {
        "assert(counter_result==1); assert(retained_original()==7); assert(new_counter()==103);"
    };
    let rebound = engine.execute(rebound_code).unwrap();
    assert!(
        rebound.error.is_none(),
        "{:?}; output={}",
        rebound.error,
        rebound.output
    );
    assert!(engine.execute("clear all;").unwrap().error.is_none());
    assert!(engine.run_file(&path).unwrap().error.is_none());
    assert!(
        engine
            .execute("assert(counter_result==1);")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn local_persistent_state_survives_workspace_clear_without_user_function_handles() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let path = directory.path().join("bare_counter.m");
    let source = "bare_result=bare();\nfunction y=bare()\n persistent n;\n if isempty(n); n=0; end;\n n=n+1; y=n;\nend\n";
    assert!(engine.run_buffer(source, &path).unwrap().error.is_none());
    assert!(engine.run_buffer(source, &path).unwrap().error.is_none());
    assert!(
        engine
            .execute("assert(bare_result==2);")
            .unwrap()
            .error
            .is_none()
    );
    engine.clear().unwrap();
    let result = engine.run_buffer(source, &path).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(
        result.workspace.len(),
        1,
        "the registry must not add base workspace variables"
    );
    assert!(
        engine
            .execute("assert(bare_result==3);")
            .unwrap()
            .error
            .is_none()
    );
    assert!(engine.execute("clear all;").unwrap().error.is_none());
    assert!(engine.run_buffer(source, &path).unwrap().error.is_none());
    assert!(
        engine
            .execute("assert(bare_result==1);")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn local_scope_registration_preserves_leading_block_comments_and_original_error_lines() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let path = fs::canonicalize(directory.path())
        .unwrap()
        .join("block_header.m");
    let source = "%{\n function y=fake(x); end\n%}\nheader_value=helper(3);\nfunction y=helper(x)\n y=x*2;\nend\n";
    let result = engine.run_buffer(source, &path).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert!(
        engine
            .execute("assert(header_value==6);")
            .unwrap()
            .error
            .is_none()
    );
    let failing = "%{\n function y=fake(x); end\n%}\nheader_value=failure(3);\nfunction y=failure(x)\n y=x; error('MathLab:BlockHeader','expected');\nend\n";
    let error = engine.run_buffer(failing, &path).unwrap().error.unwrap();
    assert!(
        error
            .stack
            .iter()
            .any(|frame| frame.file == path.to_string_lossy()
                && frame.line == 6
                && frame.name == "block_header>failure"),
        "{:?}",
        error.stack
    );
}

#[test]
fn known_deviation_path_mutation_rebinds_named_locals_but_preserves_old_handles() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let path = directory.path().join("path_counter.m");
    let source = "path_count=count_local(); saved_local=@count_local;\nfunction y=count_local()\n persistent n;\n if isempty(n); n=0; end;\n n=n+1; y=n;\nend\n";
    assert!(engine.run_buffer(source, &path).unwrap().error.is_none());
    assert!(engine.run_buffer(source, &path).unwrap().error.is_none());
    let added = directory.path().join("additional_path");
    fs::create_dir(&added).unwrap();
    let literal = added.to_string_lossy().replace('\'', "''");
    assert!(
        engine
            .execute(&format!(
                "assert(path_count==2); preserved_local=saved_local; addpath('{literal}');"
            ))
            .unwrap()
            .error
            .is_none()
    );
    assert!(engine.run_buffer(source, &path).unwrap().error.is_none());
    // Keep the fallback deviation explicit while checking the optional binder's
    // stronger behavior without changing user call names.
    let expected = if engine.scope_bridge_path().is_some() {
        "assert(path_count==3); assert(preserved_local()==4); assert(saved_local()==5);"
    } else {
        "assert(path_count==1); assert(preserved_local()==3); assert(saved_local()==2);"
    };
    let result = engine.execute(expected).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
}

#[test]
fn optional_scope_bridge_keeps_local_state_across_paths_packages_edits_and_clears() {
    let Some(bridge) = std::env::var_os("MATHLAB_SCOPE_BRIDGE") else {
        eprintln!(
            "Skipping optional native scope binding: set MATHLAB_SCOPE_BRIDGE to the compiled adapter"
        );
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let bridge = std::path::PathBuf::from(bridge).canonicalize().unwrap();
    assert_eq!(engine.scope_bridge_path(), Some(bridge.as_path()));
    engine.enable_scope_bridge(&bridge).unwrap();
    let protocol = engine.execute("scope_info=mathlab_scope_bridge('protocol',0); assert(strcmp(scope_info.protocol,'mathlab-scope') && scope_info.version==2 && scope_info.pre_entry && scope_info.original_source_reflection && scope_info.source_entry && scope_info.stable_registry && scope_info.off_path_registry); clear scope_info;").unwrap();
    assert!(protocol.error.is_none(), "{:?}", protocol.error);
    let original = directory.path().join("native_scoped.m");
    let source = "scoped_result=count(); sibling_handle=@sibling; nested_handle=make_nested(3); nested_value=nested_handle(4);\nfunction y=count()\n persistent n;\n if isempty(n); n=0; end;\n n=n+1; y=n;\nend\nfunction y=sibling()\n y=count();\nend\nfunction h=make_nested(offset)\n h=@nested;\n function y=nested(x)\n  offset=offset+1; y=x+offset;\n end\nend\n";
    let first = engine.run_buffer(source, &original).unwrap();
    assert!(first.error.is_none(), "{:?}", first.error);
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .execute("assert(scoped_result==2 && nested_value==8);")
            .unwrap()
            .error
            .is_none()
    );
    let added = directory.path().join("scope_path_mutation");
    fs::create_dir(&added).unwrap();
    assert!(
        engine
            .execute(&format!(
                "addpath('{}');",
                added.to_string_lossy().replace('\'', "''")
            ))
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    let after_path = engine
        .execute("disp(scoped_result); assert(scoped_result==3);")
        .unwrap();
    assert!(
        after_path.error.is_none(),
        "{:?}; output={}",
        after_path.error,
        after_path.output
    );
    if engine
        .list_packages()
        .unwrap()
        .iter()
        .any(|package| package.name == "signal")
    {
        engine.load_package("signal").unwrap();
    } else {
        let another = directory.path().join("another_scope_path");
        fs::create_dir(&another).unwrap();
        assert!(
            engine
                .execute(&format!(
                    "addpath('{}');",
                    another.to_string_lossy().replace('\'', "''")
                ))
                .unwrap()
                .error
                .is_none()
        );
    }
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .execute("assert(scoped_result==4);")
            .unwrap()
            .error
            .is_none()
    );
    engine.clear().unwrap();
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    assert!(engine.execute("assert(scoped_result==5); original_sibling=sibling_handle; original_nested=nested_handle;").unwrap().error.is_none());
    let edited = source.replace("n=0;", "n=100;");
    assert!(
        engine
            .run_buffer(&edited, &original)
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .execute("assert(scoped_result==101);")
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    let result=engine.execute("assert(scoped_result==6); assert(original_sibling()==7); assert(original_nested(4)==9); assert(nested_handle(1)==6); assert(exist('count','file')==0); assert(exist('sibling','file')==0);").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    // Base variables retain MATLAB-style precedence over a scoped function name.
    assert!(engine.execute("count=@()-9;").unwrap().error.is_none());
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .execute("assert(scoped_result==-9); clear count;")
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .execute("assert(scoped_result==8); clear functions;")
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .execute("assert(scoped_result==1); clear all;")
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .execute("assert(scoped_result==1);")
            .unwrap()
            .error
            .is_none()
    );
    let replacement = directory.path().join("replacement_scope.oct");
    fs::copy(&bridge, &replacement).unwrap();
    assert!(
        engine
            .enable_scope_bridge(&replacement)
            .unwrap_err()
            .to_string()
            .contains("Restart MathLab")
    );
    assert_eq!(engine.scope_bridge_path(), Some(bridge.as_path()));
    engine.restart().unwrap();
    assert_eq!(engine.scope_bridge_path(), Some(bridge.as_path()));
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .execute("assert(scoped_result==1);")
            .unwrap()
            .error
            .is_none()
    );
}

#[cfg(unix)]
#[test]
fn prepared_script_interruption_preserves_handles_and_private_scope() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let path = directory.path().join("prepared_infinite.m");
    fs::write(&path,"retained_handle=@twice; script_value=twice(8); while true; end;\nfunction y=twice(x)\n y=x*2;\nend\n").unwrap();
    let cancel = engine.cancellation_handle();
    let cancellation = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        cancel.interrupt().unwrap();
    });
    let result = engine
        .run_file_with_timeout(&path, Duration::from_secs(5))
        .unwrap();
    cancellation.join().unwrap();
    assert_eq!(result.error.unwrap().identifier, "MathLab:Interrupted");
    let result = engine
        .execute_with_timeout(
            "assert(script_value==16); assert(retained_handle(7)==14);",
            Duration::from_secs(5),
        )
        .unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
}

#[test]
fn clear_all_and_multiple_variable_types_do_not_break_worker() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    let result = engine.execute("clear all; n = [NaN Inf]; z = 1+2i; s = struct('field', 1); c = {1,'a'}; sparse_matrix = speye(3); label = 'hello'; f = @sin;").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(result.workspace.len(), 7);
    assert!(
        result
            .workspace
            .iter()
            .any(|entry| entry.name == "z" && entry.preview.contains("2i"))
    );
    assert!(
        result
            .workspace
            .iter()
            .any(|entry| entry.name == "n" && entry.preview.contains("NaN"))
    );
    assert!(
        engine
            .execute("disp(label);")
            .unwrap()
            .output
            .contains("hello")
    );
}

#[test]
fn package_registry_and_missing_package_error_preserve_the_session() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    let installed = engine.list_packages().unwrap();
    assert!(
        installed
            .iter()
            .all(|package| !package.name.is_empty() && !package.version.is_empty())
    );
    assert!(matches!(
        engine.load_package("mathlab_nonexistent_package_12345"),
        Err(Error::Worker(_))
    ));
    assert!(engine.execute("assert(1+1 == 2);").unwrap().error.is_none());
    if installed.iter().any(|package| package.name == "control") {
        engine.load_package("control").unwrap();
        assert!(
            engine
                .list_packages()
                .unwrap()
                .iter()
                .any(|package| package.name == "control" && package.loaded)
        );
    }
}

#[test]
fn pre_entry_registration_does_not_reserve_user_local_or_base_handle_names() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let original = fs::canonicalize(directory.path())
        .unwrap()
        .join("helper_collision.m");
    let source = "collision_value=mathlab_scope_register(5); collision_handle=@mathlab_scope_register;\nfunction y=mathlab_scope_register(x)\n persistent n=0; n=n+1; y=x*3+n;\nend\n";
    let staged = engine.prepare_debug_buffer(source, &original).unwrap();
    assert!(
        !fs::read_to_string(&staged.script_path)
            .unwrap()
            .contains("mathlab_scope_register (")
    );
    let result = engine.run_buffer(source, &original).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(result.workspace.len(), 2);
    assert!(
        engine
            .execute("assert(collision_value==16); assert(collision_handle(2)==8);")
            .unwrap()
            .error
            .is_none()
    );
    fs::write(&original, source).unwrap();
    let result = engine.run_file(&original).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert!(
        engine
            .execute("assert(collision_value==18); mathlab_scope_register=@(x)x+90;")
            .unwrap()
            .error
            .is_none()
    );
    let ordinary = "ordinary_value=double_local(4);\nfunction y=double_local(x)\n y=x*2;\nend\n";
    let other = directory.path().join("base_helper_collision.m");
    let result = engine.run_buffer(ordinary, &other).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    let result = engine.execute("assert(ordinary_value==8); assert(mathlab_scope_register(9)==99); clear -variables mathlab_scope_register;").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    engine.clear().unwrap();
    let result = engine.run_file(&original).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    let expected = if engine.scope_bridge_path().is_some() {
        "disp(collision_value); assert(collision_value==19);"
    } else {
        "assert(collision_value==16);"
    };
    let result = engine.execute(expected).unwrap();
    assert!(
        result.error.is_none(),
        "{:?}; output={}",
        result.error,
        result.output
    );
    assert!(engine.execute("clear functions;").unwrap().error.is_none());
    assert!(engine.run_file(&original).unwrap().error.is_none());
    assert!(
        engine
            .execute("assert(collision_value==16);")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn pre_entry_local_parse_errors_return_execution_errors_and_keep_the_session() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let original = fs::canonicalize(directory.path())
        .unwrap()
        .join("malformed_local.m");
    assert!(
        engine
            .execute("preserved_before_parse=41;")
            .unwrap()
            .error
            .is_none()
    );
    let pid = engine.process_id();
    let source = "must_not_run=99; malformed(3);\nfunction y=malformed(x)\n y=x+;\nend\n";
    let result = engine.run_buffer(source, &original).unwrap();
    let error = result.error.expect("invalid local expression must fail");
    assert!(
        error.message.contains("parse error") || error.message.contains("syntax error"),
        "{error:?}"
    );
    assert!(
        error
            .stack
            .iter()
            .any(|frame| frame.file == original.to_string_lossy() && frame.line == 3),
        "{:?}",
        error.stack
    );
    assert_eq!(engine.process_id(), pid);
    let result = engine.execute("assert(preserved_before_parse==41); assert(!exist('must_not_run','var')); after_parse=42;").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert!(!original.exists());
}

#[test]
fn optional_scope_pre_entry_reflects_original_paths_resources_and_retained_generations() {
    if std::env::var_os("MATHLAB_SCOPE_BRIDGE").is_none() {
        return;
    }
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let root = fs::canonicalize(directory.path()).unwrap();
    fs::write(root.join("resource.txt"), "42\n").unwrap();
    for saved in [false, true] {
        let original = root.join(if saved {
            "reflected_saved.m"
        } else {
            "reflected_unsaved.m"
        });
        let source = "script_seen=mfilename('fullpath'); script_resource=dlmread(fullfile(fileparts(script_seen),'resource.txt')); [seen_path,resource_value,code_value]=read_local(); reflected_handle=@read_local; reflected_nested=make_nested();\nfunction [seen,resource,code]=read_local()\n seen=mfilename('fullpath'); resource=dlmread(fullfile(fileparts(seen),'resource.txt')); code=7;\nend\nfunction h=make_nested()\n h=@inner;\n function [seen,resource]=inner()\n  seen=mfilename('fullpath'); resource=dlmread(fullfile(fileparts(seen),'resource.txt'));\n end\nend\n";
        if saved {
            fs::write(&original, source).unwrap();
        }
        let result = if saved {
            engine.run_file(&original)
        } else {
            engine.run_buffer(source, &original)
        }
        .unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        let path_literal = original.to_string_lossy().replace('\'', "''");
        let stem_literal = original
            .with_extension("")
            .to_string_lossy()
            .replace('\'', "''");
        let result = engine.execute(&format!("assert(strcmp(script_seen,'{stem_literal}') && script_resource==42); assert(strcmp(seen_path,'{stem_literal}') && resource_value==42 && code_value==7); info=functions(reflected_handle); assert(strcmp(info.file,'{path_literal}')); [nested_path,nested_resource]=reflected_nested(); assert(strcmp(nested_path,'{stem_literal}') && nested_resource==42); nested_info=functions(reflected_nested); assert(isempty(nested_info.file)); retained_reflected=reflected_handle; retained_nested=reflected_nested; clear info nested_info; ")).unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        let changed = source.replace("code=7;", "code=101;");
        let result = engine.run_buffer(&changed, &original).unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        let result = engine.execute(&format!("[old_path,old_resource,old_code]=retained_reflected(); [new_path,new_resource,new_code]=reflected_handle(); assert(old_code==7 && new_code==101 && old_resource==42 && new_resource==42); assert(strcmp(old_path,'{stem_literal}') && strcmp(new_path,'{stem_literal}')); [nested_path,nested_resource]=retained_nested(); assert(strcmp(nested_path,'{stem_literal}') && nested_resource==42); ")).unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(original.exists(), saved);
        if saved {
            assert_eq!(fs::read_to_string(&original).unwrap(), source);
        }
    }
}

#[test]
fn reflected_caught_error_frames_preserve_retained_nested_generation_names() {
    if std::env::var_os("MATHLAB_SCOPE_BRIDGE").is_none() {
        return;
    }
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let original = fs::canonicalize(directory.path())
        .unwrap()
        .join("ambiguous_reflection.m");
    let old = "old_nested=outer_old();\nfunction h=outer_old()\n h=@inner;\n function y=inner()\n  error('MathLab:OldNested','old nested failure');\n end\nend\n";
    let new = old
        .replace("old_nested=outer_old()", "new_nested=outer_new()")
        .replace("outer_old", "outer_new");
    assert!(engine.run_buffer(old, &original).unwrap().error.is_none());
    assert!(engine.run_buffer(&new, &original).unwrap().error.is_none());
    let error = engine.execute("old_nested();").unwrap().error.unwrap();
    assert_eq!(error.identifier, "MathLab:OldNested");
    let frame = error
        .stack
        .iter()
        .find(|frame| frame.file == original.to_string_lossy() && frame.line == 5)
        .unwrap();
    assert_eq!(
        frame.name, "ambiguous_reflection>outer_old>inner",
        "native named parentage identifies the old retained outer despite overlapping original line spans"
    );
}

#[test]
fn optional_prepared_debugger_single_original_clear_removes_all_retained_generation_marks() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let root = fs::canonicalize(directory.path()).unwrap();
    let original = root.join("clear_generations.m");
    let old =
        "latest_handle=@calc;\nfunction y=calc(x)\n intermediate=x+1;\n y=intermediate*2;\nend\n";
    let new = old.replace("x+1;", "x+10;");
    assert!(engine.run_buffer(old, &original).unwrap().error.is_none());
    assert!(
        engine
            .execute("old_h=latest_handle;")
            .unwrap()
            .error
            .is_none()
    );
    assert!(engine.run_buffer(&new, &original).unwrap().error.is_none());
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    engine.prepare_debug_buffer(old, &original).unwrap();
    debugger.set_breakpoint(&original, 4).unwrap();
    engine.prepare_debug_buffer(&new, &original).unwrap();
    debugger.set_breakpoint(&original, 4).unwrap();
    let ordinary = root.join("unrelated_after_clear.m");
    fs::write(
        &ordinary,
        "function y=unrelated_after_clear()\n y=71;\nend\n",
    )
    .unwrap();
    debugger.set_breakpoint(&ordinary, 2).unwrap();
    debugger.clear_breakpoint(&original, 4).unwrap();
    let result = engine.execute_with_timeout("old_value=old_h(4); new_value=latest_handle(4); assert(old_value==10 && new_value==28);", Duration::from_secs(3)).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
    let mut task = Some(thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "ordinary_value=unrelated_after_clear();",
            Duration::from_secs(15),
        );
        (engine, result)
    }));
    let pause = wait_debug_pause_or_finished(&debugger, 2, 0, &mut task);
    assert_eq!(pause.file, ordinary);
    debugger
        .resume(ResumeAction::Continue, pause.revision)
        .unwrap();
    let (mut engine, result) = task.take().unwrap().join().unwrap();
    cleanup.disarm();
    assert!(result.unwrap().error.is_none());
    debugger.clear_breakpoint(&ordinary, 2).unwrap();
    assert!(
        engine
            .execute("assert(ordinary_value==71);")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn optional_prepared_debugger_handles_user_registry_name_and_base_variable_collisions() {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let original = fs::canonicalize(directory.path())
        .unwrap()
        .join("debug_collision.m");
    let source = "collision_handle=@mathlab_scope_register;\ncollision_value=0;\nfunction y=mathlab_scope_register(x)\n intermediate=x+1;\n y=intermediate*2;\nend\n";
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .execute("mathlab_scope_register=@(x)x+90;")
            .unwrap()
            .error
            .is_none()
    );
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    debugger.set_breakpoint(&original, 5).unwrap();
    let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
    let mut task = Some(thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "collision_value=collision_handle(4);",
            Duration::from_secs(15),
        );
        (engine, result)
    }));
    let pause = wait_debug_pause_or_finished(&debugger, 5, 0, &mut task);
    assert_eq!(pause.file, original);
    assert_eq!(
        pause.function_name,
        "debug_collision>mathlab_scope_register"
    );
    assert_eq!(
        debugger
            .evaluate("disp(intermediate);", pause.revision)
            .unwrap(),
        "5\n"
    );
    debugger
        .resume(ResumeAction::Continue, pause.revision)
        .unwrap();
    let (mut engine, result) = task.take().unwrap().join().unwrap();
    cleanup.disarm();
    assert!(result.unwrap().error.is_none());
    debugger.clear_breakpoint(&original, 5).unwrap();
    let result = engine
        .execute("assert(collision_value==10); assert(mathlab_scope_register(9)==99);")
        .unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
}

#[test]
fn fallback_factory_identity_guard_rejects_shadowing_without_executing_foreign_code() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    if engine.scope_bridge_path().is_some() {
        return;
    }
    let original = directory.path().join("factory_guard.m");
    let source = "must_not_run=local(2);\nfunction y=local(x)\n y=x*3;\nend\n";
    let staged = engine.prepare_debug_buffer(source, &original).unwrap();
    let factory = format!(
        "{}_factory",
        staged.script_path.file_stem().unwrap().to_string_lossy()
    );
    let shadow = directory.path().join(format!("{factory}.m"));
    fs::write(
        &shadow,
        format!(
            "function h={factory}()\n assignin('base','foreign_executed',true); h={{}};\nend\n"
        ),
    )
    .unwrap();
    let result = engine.run_buffer(source, &original).unwrap();
    assert_eq!(result.error.unwrap().identifier, "MathLab:RuntimeIdentity");
    let result = engine
        .execute("assert(!exist('foreign_executed','var')); assert(!exist('must_not_run','var'));")
        .unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    fs::remove_file(&shadow).unwrap();
    assert!(
        engine
            .execute(&format!("clear {factory}; rehash;"))
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .execute("assert(must_not_run==6);")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn optional_scope_reflection_keeps_immutable_state_after_cache_refresh_and_original_file_edits() {
    if std::env::var_os("MATHLAB_SCOPE_BRIDGE").is_none() {
        return;
    }
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let original = fs::canonicalize(directory.path())
        .unwrap()
        .join("timed_scope.m");
    let source = "timed_result=count_local(); old_timed=@count_local;\nfunction y=count_local()\n persistent n=0; n=n+1; y=n;\nend\n";
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .execute("assert(timed_result==2);")
            .unwrap()
            .error
            .is_none()
    );
    thread::sleep(Duration::from_millis(1200));
    fs::write(&original, source.replace("n=0", "n=100")).unwrap();
    let added = directory.path().join("timed_path");
    fs::create_dir(&added).unwrap();
    assert!(
        engine
            .execute(&format!(
                "addpath('{}');",
                added.to_string_lossy().replace('\'', "''")
            ))
            .unwrap()
            .error
            .is_none()
    );
    let result = engine.run_buffer(source, &original).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    let result = engine
        .execute("disp(timed_result); assert(timed_result==3); assert(old_timed()==4);")
        .unwrap();
    assert!(
        result.error.is_none(),
        "{:?}; output={}",
        result.error,
        result.output
    );
    assert!(engine.run_file(&original).unwrap().error.is_none());
    assert!(
        engine
            .execute("assert(timed_result==101);")
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        engine
            .run_buffer(source, &original)
            .unwrap()
            .error
            .is_none()
    );
    let result = engine
        .execute("disp(timed_result); assert(timed_result==5);")
        .unwrap();
    assert!(
        result.error.is_none(),
        "{:?}; output={}",
        result.error,
        result.output
    );
}

#[test]
fn optional_prepared_debugger_clear_tracks_two_relocations_without_clearing_independent_historical_line()
 {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let original = fs::canonicalize(directory.path())
        .unwrap()
        .join("relocated_counter.m");
    let first = "h1=@counter;\nfunction y=counter()\n persistent n=0;\n n=n+1;\n y=n;\nend\n";
    let second = "h2=@counter;\nfunction y=counter()\n persistent n=100;\n % relocated point\n n=n+1;\n y=n;\nend\n";
    let third = "h3=@counter;\nfunction y=counter()\n persistent n=200;\n independent=1;\n % relocated again\n n=n+1;\n y=n+independent-1;\nend\n";
    for source in [first, second, third] {
        let result = engine.run_buffer(source, &original).unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
    }
    let debugger = engine
        .enable_debugger(std::path::Path::new(&bridge))
        .unwrap();
    engine.prepare_debug_buffer(first, &original).unwrap();
    assert_eq!(debugger.set_breakpoint(&original, 4).unwrap(), vec![4]);
    engine.prepare_debug_buffer(second, &original).unwrap();
    assert_eq!(debugger.set_breakpoint(&original, 4).unwrap(), vec![5]);
    engine.prepare_debug_buffer(third, &original).unwrap();
    assert_eq!(debugger.set_breakpoint(&original, 5).unwrap(), vec![6]);
    assert_eq!(debugger.set_breakpoint(&original, 4).unwrap(), vec![4]);
    debugger.clear_breakpoint(&original, 6).unwrap();
    let result = engine
        .execute_with_timeout(
            "v1=h1(); v2=h2(); assert(v1==1 && v2==101);",
            Duration::from_secs(3),
        )
        .unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    let mut cleanup = RunningRequestGuard(Some(engine.cancellation_handle()));
    let mut task = Some(thread::spawn(move || {
        let result = engine.execute_with_timeout("v3=h3();", Duration::from_secs(15));
        (engine, result)
    }));
    let pause = wait_debug_pause_or_finished(&debugger, 4, 0, &mut task);
    assert_eq!(pause.file, original);
    assert_eq!(pause.function_name, "relocated_counter>counter");
    assert_eq!(
        debugger.evaluate("disp(n);", pause.revision).unwrap(),
        "200\n"
    );
    let snapshot = debugger
        .source_snapshot(&pause.runtime_file)
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.source, third);
    debugger.clear_breakpoint(&original, 4).unwrap();
    debugger
        .resume(ResumeAction::Continue, pause.revision)
        .unwrap();
    let (mut engine, result) = task.take().unwrap().join().unwrap();
    cleanup.disarm();
    assert!(result.unwrap().error.is_none());
    let result = engine
        .execute_with_timeout(
            "assert(v3==201); assert(h1()==2 && h2()==102 && h3()==202);",
            Duration::from_secs(3),
        )
        .unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
}

#[test]
fn optional_native_plain_buffer_entry_uses_original_script_path_and_resources() {
    if std::env::var_os("MATHLAB_SCOPE_BRIDGE").is_none() {
        return;
    }
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let root = fs::canonicalize(directory.path()).unwrap();
    fs::write(root.join("plain_resource.txt"), "73\n").unwrap();
    let original = root.join("unfiled_plain_reflection.m");
    let source = "script_path=mfilename('fullpath'); plain_resource=dlmread(fullfile(fileparts(script_path),'plain_resource.txt')); plain_assignment=7;\n";
    let result = engine.run_buffer(source, &original).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(result.workspace.len(), 3);
    let stem = original
        .with_extension("")
        .to_string_lossy()
        .replace('\'', "''");
    let result = engine
        .execute(&format!(
            "assert(strcmp(script_path,'{stem}') && plain_resource==73 && plain_assignment==7);"
        ))
        .unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert!(!original.exists());
    if let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") {
        engine
            .enable_debugger(std::path::Path::new(&bridge))
            .unwrap();
        let result = engine.run_buffer(source, &original).unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        let result = engine
            .execute(&format!(
                "assert(strcmp(script_path,'{stem}') && plain_resource==73 && plain_assignment==7);"
            ))
            .unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
    }
}
