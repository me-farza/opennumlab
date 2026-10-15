// SPDX-License-Identifier: GPL-3.0-or-later
use mathlab_engine::{
    ConsoleEventKind, Engine, EngineConfig, InputHandle, InputKey, InputOutcome, InputPhase,
    PendingInput, discover_octave,
};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

fn session() -> Option<(Engine, tempfile::TempDir, InputHandle)> {
    if std::env::var_os("MATHLAB_INPUT_BRIDGE").is_none() {
        eprintln!(
            "Skipping native input integration: set MATHLAB_INPUT_BRIDGE to the compiled adapter."
        );
        return None;
    }
    let Some(executable) = discover_octave() else {
        eprintln!("Skipping native input integration: GNU Octave was not found.");
        return None;
    };
    let directory = tempfile::tempdir().unwrap();
    let engine = Engine::start(EngineConfig {
        executable,
        working_directory: directory.path().into(),
    })
    .unwrap();
    let input = engine
        .input_handle()
        .expect("environment-enabled authentic input handle");
    Some((engine, directory, input))
}

fn wait(input: &InputHandle, previous: Option<InputKey>) -> PendingInput {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(pending) = input.pending().unwrap()
            && pending.phase == InputPhase::Waiting
            && Some(pending.request.key) != previous
        {
            return pending;
        }
        assert!(
            Instant::now() < deadline,
            "no authentic input request before deadline"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn optional_input_preserves_caller_scope_unicode_echo_and_ordered_prompt_events() {
    let Some((mut engine, _directory, input)) = session() else {
        return;
    };
    let stream = engine.subscribe_console();
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout("local_seed=40; value=input('Value> '); text=input('Text> ','s'); assert(value==42); assert(strcmp(text,'  λ exact  '));",Duration::from_secs(12));
        (engine, result)
    });
    let first = wait(&input, None);
    assert_eq!(first.request.prompt, "Value> ");
    assert!(first.request.console_sequence > 0);
    input.reply(&first.request.key, "local_seed+2").unwrap();
    let second = wait(&input, Some(first.request.key));
    assert_eq!(second.request.prompt, "Text> ");
    assert!(second.request.key.input_id > first.request.key.input_id);
    assert!(input.reply(&first.request.key, "stale").is_err());
    input.reply(&second.request.key, "  λ exact  ").unwrap();
    let (_engine, result) = task.join().unwrap();
    let result = result.unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(
        result.output.matches("local_seed+2").count(),
        1,
        "{}",
        result.output
    );
    assert_eq!(
        result.output.matches("  λ exact  ").count(),
        1,
        "{}",
        result.output
    );
    assert!(input.pending().unwrap().is_none());
    let mut bytes = Vec::new();
    let mut requested = Vec::new();
    let mut finished = Vec::new();
    while let Ok(event) = stream.try_recv() {
        match event.kind {
            ConsoleEventKind::Output { bytes: fragment } => bytes.extend(fragment),
            ConsoleEventKind::InputRequested { request } => {
                assert!(String::from_utf8_lossy(&bytes).ends_with(&request.prompt));
                assert_eq!(request.console_sequence, event.sequence);
                requested.push(request.key);
            }
            ConsoleEventKind::InputFinished { finished: done } => {
                assert_eq!(done.outcome, InputOutcome::Line);
                finished.push(done.key);
            }
            _ => {}
        }
    }
    assert_eq!(requested, vec![first.request.key, second.request.key]);
    assert_eq!(finished, requested);
    assert_eq!(String::from_utf8(bytes).unwrap(), result.output);
}

#[test]
fn optional_input_nested_expression_consumes_outer_line_before_inner_request() {
    let Some((mut engine, _directory, input)) = session() else {
        return;
    };
    let stream = engine.subscribe_console();
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout("nested_value=input('Outer> '); assert(nested_value==17); builtin_value=builtin('input','Builtin> ','s'); assert(strcmp(builtin_value,'done'));",Duration::from_secs(12));
        (engine, result)
    });
    let outer = wait(&input, None);
    input.reply(&outer.request.key, "input('Inner> ')").unwrap();
    let inner = wait(&input, Some(outer.request.key));
    assert_eq!(inner.request.prompt, "Inner> ");
    input.reply(&inner.request.key, "17").unwrap();
    let builtin = wait(&input, Some(inner.request.key));
    assert_eq!(builtin.request.prompt, "Builtin> ");
    input.reply(&builtin.request.key, "done").unwrap();
    let (_engine, result) = task.join().unwrap();
    let result = result.unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    let mut outer_finished = false;
    let mut saw_inner = false;
    while let Ok(event) = stream.try_recv() {
        match event.kind {
            ConsoleEventKind::InputFinished { finished } if finished.key == outer.request.key => {
                outer_finished = true
            }
            ConsoleEventKind::InputRequested { request } if request.key == inner.request.key => {
                assert!(outer_finished);
                saw_inner = true;
            }
            _ => {}
        }
    }
    assert!(saw_inner);
}

#[cfg(unix)]
#[test]
fn optional_input_stop_and_restart_revoke_old_keys_without_cross_consuming_recovery() {
    let Some((mut engine, _directory, input)) = session() else {
        return;
    };
    let pid = engine.process_id();
    let cancellation = engine.cancellation_handle();
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "saved_before_input=23; answer=input('Stop> '); must_not_run=99;",
            Duration::from_secs(10),
        );
        (engine, result)
    });
    let old = wait(&input, None);
    cancellation.interrupt().unwrap();
    let (mut engine, result) = task.join().unwrap();
    assert_eq!(
        result.unwrap().error.unwrap().identifier,
        "MathLab:Interrupted"
    );
    assert_eq!(engine.process_id(), pid);
    assert!(input.pending().unwrap().is_none());
    assert!(input.reply(&old.request.key, "obsolete").is_err());
    assert!(engine.execute("assert(saved_before_input==23); assert(!exist('must_not_run','var')); healthy_after_input=5;").unwrap().error.is_none());
    engine.restart().unwrap();
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "fresh=input('Fresh> ','s'); assert(strcmp(fresh,'new'));",
            Duration::from_secs(10),
        );
        (engine, result)
    });
    let fresh = wait(&input, Some(old.request.key));
    assert!(fresh.request.key.generation > old.request.key.generation);
    assert!(input.eof(&old.request.key).is_err());
    input.reply(&fresh.request.key, "new").unwrap();
    let (_engine, result) = task.join().unwrap();
    assert!(result.unwrap().error.is_none());
}

#[test]
fn optional_input_clear_and_no_output_calls_keep_the_original_builtin_contract() {
    let Some((mut engine, _directory, input)) = session() else {
        return;
    };
    let task = thread::spawn(move || {
        let result=engine.execute_with_timeout("clear all; input('Discard> ','s'); text=input('After clear> ','s'); assert(ischar(text)&&isempty(text));",Duration::from_secs(10));
        (engine, result)
    });
    let first = wait(&input, None);
    input.reply(&first.request.key, "discarded").unwrap();
    let second = wait(&input, Some(first.request.key));
    input.reply(&second.request.key, "").unwrap();
    let (_engine, result) = task.join().unwrap();
    assert!(result.unwrap().error.is_none());
}

#[test]
fn optional_input_eof_preserves_original_error_and_next_request_remains_usable() {
    let Some((mut engine, _directory, input)) = session() else {
        return;
    };
    for source in [
        "eof_value=input('EOF expression> ');",
        "eof_value=input('EOF string> ','s');",
    ] {
        let stream = engine.subscribe_console();
        let task = thread::spawn(move || {
            let result = engine.execute_with_timeout(source, Duration::from_secs(10));
            (engine, result)
        });
        let requested = wait(&input, None);
        input.eof(&requested.request.key).unwrap();
        let (returned, result) = task.join().unwrap();
        engine = returned;
        assert!(
            result
                .unwrap()
                .error
                .unwrap()
                .message
                .contains("reading user-input failed")
        );
        assert!(input.pending().unwrap().is_none());
        let mut finished = false;
        while let Ok(event) = stream.try_recv() {
            if let ConsoleEventKind::InputFinished { finished: done } = event.kind {
                assert_eq!(done.key, requested.request.key);
                assert_eq!(done.outcome, InputOutcome::Eof);
                finished = true;
            }
        }
        assert!(finished);
        assert!(
            engine
                .execute("healthy_after_eof=6;")
                .unwrap()
                .error
                .is_none()
        );
    }
}

#[test]
fn optional_input_caught_corrupt_reply_finishes_error_before_the_next_read() {
    let Some((mut engine, _directory, input)) = session() else {
        return;
    };
    let input_directory = engine.worker_source_path().parent().unwrap().join("input");
    let stream = engine.subscribe_console();
    let task = thread::spawn(move || {
        let result=engine.execute_with_timeout("try; input('Corrupt> ','s'); catch caught; assert(strcmp(caught.identifier,'MathLab:InputReply')); end; recovered=input('Recovered> ','s'); assert(strcmp(recovered,'healthy'));",Duration::from_secs(12));
        (engine, result)
    });
    let first = wait(&input, None);
    let key = first.request.key;
    let reply = serde_json::json!({"generation":key.generation,"invocation":key.invocation,"request_id":key.request_id,"input_id":key.input_id+1,"text":"foreign","eof":false,"echo":true});
    let path = input_directory.join(format!(
        "input-reply-{}-{}-{}-{}.json",
        key.generation, key.invocation, key.request_id, key.input_id
    ));
    let temporary = path.with_extension("pending");
    std::fs::write(&temporary, serde_json::to_vec(&reply).unwrap()).unwrap();
    std::fs::rename(temporary, path).unwrap();
    let next = wait(&input, Some(key));
    assert_eq!(next.request.prompt, "Recovered> ");
    input.reply(&next.request.key, "healthy").unwrap();
    let (_engine, result) = task.join().unwrap();
    assert!(result.unwrap().error.is_none());
    let mut aborted = false;
    let mut recovered = false;
    while let Ok(event) = stream.try_recv() {
        match event.kind {
            ConsoleEventKind::InputFinished { finished } if finished.key == key => {
                assert_eq!(finished.outcome, InputOutcome::Error);
                aborted = true;
            }
            ConsoleEventKind::InputRequested { request } if request.key == next.request.key => {
                assert!(aborted);
                recovered = true;
            }
            _ => {}
        }
    }
    assert!(recovered);
}

#[test]
fn optional_input_rejects_late_activation_after_old_builtin_handle_capture() {
    const CHILD: &str = "MATHLAB_TEST_UNHOOKED_INPUT";
    if let Some(adapter) = std::env::var_os(CHILD) {
        let executable = discover_octave().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let mut engine = Engine::start(EngineConfig {
            executable,
            working_directory: directory.path().into(),
        })
        .unwrap();
        assert!(engine.input_handle().is_none());
        assert!(engine.execute("old_input=@input;").unwrap().error.is_none());
        let error = engine
            .enable_input_adapter(&PathBuf::from(&adapter))
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("before the first user execution")
        );
        engine.restart().unwrap();
        assert!(engine.enable_input_adapter(&PathBuf::from(adapter)).is_ok());
        return;
    }
    let Some(adapter) = std::env::var_os("MATHLAB_INPUT_BRIDGE") else {
        return;
    };
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "optional_input_rejects_late_activation_after_old_builtin_handle_capture",
            "--nocapture",
        ])
        .env_remove("MATHLAB_INPUT_BRIDGE")
        .env(CHILD, adapter)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
