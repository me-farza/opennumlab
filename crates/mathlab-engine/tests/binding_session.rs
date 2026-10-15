// SPDX-License-Identifier: GPL-3.0-or-later
//! Binding revisions describe a particular worker workspace, not a variable's
//! shape or displayed value. These checks exercise the real worker protocol.

use mathlab_engine::{
    BindingToken, Engine, EngineConfig, Error, WorkspaceSnapshot, discover_octave,
};

fn engine() -> Option<(Engine, tempfile::TempDir)> {
    let Some(executable) = discover_octave() else {
        eprintln!("Skipping binding integration: no installed Octave executable.");
        return None;
    };
    let directory = tempfile::Builder::new()
        .prefix("mathlab-binding-test-")
        .tempdir()
        .unwrap();
    let engine = Engine::start(EngineConfig {
        executable,
        working_directory: directory.path().into(),
    })
    .expect("installed Octave must start the binding protocol worker");
    Some((engine, directory))
}

fn execute(engine: &mut Engine, source: &str) {
    let result = engine.execute(source).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
}

fn binding(snapshot: &WorkspaceSnapshot, name: &str) -> BindingToken {
    snapshot
        .variables
        .iter()
        .find(|variable| variable.variable.name == name)
        .unwrap_or_else(|| panic!("snapshot did not contain {name}"))
        .token
        .clone()
}

fn assert_stale<T>(result: Result<T, Error>) {
    match result {
        Err(Error::Worker(error)) => assert_eq!(error.identifier, "MathLab:StaleBinding"),
        Err(other) => panic!("expected stale worker binding, received {other:?}"),
        Ok(_) => panic!("a revoked binding was accepted"),
    }
}

#[test]
fn negotiated_snapshots_and_primitive_reads_preserve_binding_context() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    for capability in [
        "binding_snapshot.v1",
        "display_page.v1",
        "checked_scalar_write.v1",
    ] {
        assert!(engine.capabilities().supports(capability));
        engine.require_capability(capability).unwrap();
    }
    assert!(
        engine
            .require_capability("test-only-unknown-capability.v1")
            .is_err()
    );
    execute(
        &mut engine,
        "A=[1 2;3 4]; L=logical([0 1]); U=intmax('uint64');",
    );
    let initial = engine.workspace_snapshot().unwrap();
    let matrix = binding(&initial, "A");
    let logical = binding(&initial, "L");
    let exact_integer = binding(&initial, "U");
    for variable in &initial.variables {
        assert_eq!(variable.token.context, initial.context);
        assert_eq!(variable.token.name, variable.variable.name);
    }

    for _ in 0..3 {
        let matrix_page = engine.read_variable_checked(&matrix, 1, 1, 2, 2).unwrap();
        assert_eq!(matrix_page.token, matrix);
        assert_eq!(
            matrix_page.slice.cells,
            vec![vec!["1", "2"], vec!["3", "4"]]
        );
        let logical_page = engine.read_variable_checked(&logical, 1, 1, 1, 2).unwrap();
        assert_eq!(logical_page.token, logical);
        assert_eq!(logical_page.slice.cells, vec![vec!["false", "true"]]);
        let integer_page = engine
            .read_variable_checked(&exact_integer, 1, 1, 1, 1)
            .unwrap();
        assert_eq!(integer_page.token, exact_integer);
        assert_eq!(integer_page.slice.cells[0][0], "18446744073709551615");
        let current = engine.workspace_snapshot().unwrap();
        assert_eq!(current.context, initial.context);
        assert_eq!(binding(&current, "A"), matrix);
    }
}

#[test]
fn checked_edit_issues_a_new_token_and_revokes_every_old_workspace_binding() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(&mut engine, "A=[1 2;3 4]; peer=19;");
    let initial = engine.workspace_snapshot().unwrap();
    let old_matrix = binding(&initial, "A");
    let old_peer = binding(&initial, "peer");
    let receipt = engine
        .set_variable_element_checked(&old_matrix, 2, 1, "9")
        .unwrap();
    assert_eq!(receipt.token.name, "A");
    assert_eq!(receipt.token.context.worker_id, initial.context.worker_id);
    assert_eq!(receipt.token.context.generation, initial.context.generation);
    assert_eq!(receipt.token.context.invocation, initial.context.invocation);
    assert!(receipt.token.context.revision > initial.context.revision);
    assert!(
        receipt
            .workspace
            .iter()
            .any(|variable| variable.name == "A")
    );
    assert!(
        receipt
            .workspace
            .iter()
            .any(|variable| variable.name == "peer")
    );

    assert_stale(engine.read_variable_checked(&old_matrix, 1, 1, 2, 2));
    assert_stale(engine.set_variable_element_checked(&old_matrix, 1, 1, "99"));
    assert_stale(engine.read_variable_checked(&old_peer, 1, 1, 1, 1));
    let current = engine.workspace_snapshot().unwrap();
    assert_eq!(current.context, receipt.token.context);
    assert_eq!(binding(&current, "A"), receipt.token);
    let page = engine
        .read_variable_checked(&receipt.token, 1, 1, 2, 2)
        .unwrap();
    assert_eq!(page.slice.cells, vec![vec!["1", "2"], vec!["9", "4"]]);
    assert_eq!(page.token, receipt.token);
}

#[test]
fn same_value_and_same_shape_rebindings_cannot_reuse_an_old_token() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(&mut engine, "A=[1 2;3 4];");
    for (source, expected) in [
        ("A=[1 2;3 4];", vec![vec!["1", "2"], vec!["3", "4"]]),
        ("A=[11 12;13 14];", vec![vec!["11", "12"], vec!["13", "14"]]),
        ("A=A;", vec![vec!["11", "12"], vec!["13", "14"]]),
    ] {
        let before = engine.workspace_snapshot().unwrap();
        let old = binding(&before, "A");
        execute(&mut engine, source);
        assert_stale(engine.read_variable_checked(&old, 1, 1, 2, 2));
        assert_stale(engine.set_variable_element_checked(&old, 1, 1, "99"));
        let after = engine.workspace_snapshot().unwrap();
        assert!(after.context.revision > before.context.revision);
        let fresh = binding(&after, "A");
        let page = engine.read_variable_checked(&fresh, 1, 1, 2, 2).unwrap();
        assert_eq!(page.slice.dimensions, vec![2, 2]);
        assert_eq!(page.slice.cells, expected);
    }
}

#[test]
fn assignment_followed_by_error_revokes_tokens_and_exposes_the_partial_workspace() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(&mut engine, "A=[1 2;3 4];");
    let before = engine.workspace_snapshot().unwrap();
    let old = binding(&before, "A");
    let result = engine
        .execute("A(1,1)=77; partial_saved=19; error('MathLab:BindingTest','intentional');")
        .unwrap();
    assert_eq!(result.error.unwrap().identifier, "MathLab:BindingTest");
    assert_stale(engine.read_variable_checked(&old, 1, 1, 2, 2));
    assert_stale(engine.set_variable_element_checked(&old, 1, 1, "99"));
    let after = engine.workspace_snapshot().unwrap();
    assert!(after.context.revision > before.context.revision);
    let fresh = binding(&after, "A");
    assert_eq!(
        engine
            .read_variable_checked(&fresh, 1, 1, 2, 2)
            .unwrap()
            .slice
            .cells,
        vec![vec!["77", "2"], vec!["3", "4"]]
    );
    let partial = binding(&after, "partial_saved");
    assert_eq!(
        engine
            .read_variable_checked(&partial, 1, 1, 1, 1)
            .unwrap()
            .slice
            .cells[0][0],
        "19"
    );
}

#[test]
fn clear_and_restart_revoke_bindings_even_when_the_variable_is_recreated() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(&mut engine, "A=42;");
    let initial = engine.workspace_snapshot().unwrap();
    let before_clear = binding(&initial, "A");
    engine.clear().unwrap();
    assert_stale(engine.read_variable_checked(&before_clear, 1, 1, 1, 1));
    assert_stale(engine.set_variable_element_checked(&before_clear, 1, 1, "99"));
    assert!(engine.workspace_snapshot().unwrap().variables.is_empty());
    execute(&mut engine, "A=42;");
    assert_stale(engine.read_variable_checked(&before_clear, 1, 1, 1, 1));

    let before_restart = binding(&engine.workspace_snapshot().unwrap(), "A");
    engine.restart().unwrap();
    assert_stale(engine.read_variable_checked(&before_restart, 1, 1, 1, 1));
    assert_stale(engine.set_variable_element_checked(&before_restart, 1, 1, "99"));
    assert!(engine.workspace_snapshot().unwrap().variables.is_empty());
    execute(&mut engine, "A=42;");
    let fresh = binding(&engine.workspace_snapshot().unwrap(), "A");
    assert!(fresh.context.generation > before_restart.context.generation);
    assert_stale(engine.read_variable_checked(&before_restart, 1, 1, 1, 1));
    assert_eq!(
        engine
            .read_variable_checked(&fresh, 1, 1, 1, 1)
            .unwrap()
            .slice
            .cells[0][0],
        "42"
    );
}

#[test]
fn another_worker_cannot_consume_a_token_for_an_identical_variable() {
    let Some((mut first, _first_directory)) = engine() else {
        return;
    };
    let Some((mut second, _second_directory)) = engine() else {
        return;
    };
    execute(&mut first, "A=42;");
    execute(&mut second, "A=42;");
    let first_token = binding(&first.workspace_snapshot().unwrap(), "A");
    let second_token = binding(&second.workspace_snapshot().unwrap(), "A");
    assert_ne!(
        first_token.context.worker_id,
        second_token.context.worker_id
    );
    assert_stale(second.read_variable_checked(&first_token, 1, 1, 1, 1));
    assert_stale(second.set_variable_element_checked(&first_token, 1, 1, "99"));
    assert_stale(first.read_variable_checked(&second_token, 1, 1, 1, 1));
    for (engine, token) in [(&mut first, &first_token), (&mut second, &second_token)] {
        assert_eq!(
            engine
                .read_variable_checked(token, 1, 1, 1, 1)
                .unwrap()
                .slice
                .cells[0][0],
            "42"
        );
        assert_eq!(engine.workspace_snapshot().unwrap().context, token.context);
    }
}

#[test]
fn rejected_checked_edits_preserve_values_class_and_revision() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(&mut engine, "A=[1 2;3 4]; I=int16(7);");
    let initial = engine.workspace_snapshot().unwrap();
    let matrix = binding(&initial, "A");
    let integer = binding(&initial, "I");
    for (token, row, column, literal) in [
        (&matrix, 1, 1, "1; A(1)=999"),
        (&matrix, 1, 1, "sin(0)"),
        (&matrix, 1, 1, "[5 6]"),
        (&matrix, 0, 1, "9"),
        (&matrix, 3, 1, "9"),
        (&matrix, 1, 3, "9"),
        (&integer, 1, 1, "1+2i"),
    ] {
        assert!(
            engine
                .set_variable_element_checked(token, row, column, literal)
                .is_err(),
            "invalid checked edit accepted: {}({row},{column})={literal}",
            token.name
        );
        let after = engine.workspace_snapshot().unwrap();
        assert_eq!(after.context, initial.context);
        assert_eq!(binding(&after, "A"), matrix);
        assert_eq!(binding(&after, "I"), integer);
        let matrix_page = engine.read_variable_checked(&matrix, 1, 1, 2, 2).unwrap();
        assert_eq!(matrix_page.slice.class_name, "double");
        assert_eq!(
            matrix_page.slice.cells,
            vec![vec!["1", "2"], vec!["3", "4"]]
        );
        let integer_page = engine.read_variable_checked(&integer, 1, 1, 1, 1).unwrap();
        assert_eq!(integer_page.slice.class_name, "int16");
        assert_eq!(integer_page.slice.cells[0][0], "7");
    }
}

#[cfg(unix)]
#[test]
fn stop_revokes_old_bindings_and_preserves_the_same_process_partial_workspace() {
    use mathlab_engine::ConsoleEventKind;
    use std::thread;
    use std::time::{Duration, Instant};

    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(&mut engine, "A=19;");
    let old = binding(&engine.workspace_snapshot().unwrap(), "A");
    let process_id = engine.process_id();
    let subscription = engine.subscribe_console();
    let cancellation = engine.cancellation_handle();
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "stop_partial=23; fprintf(stdout,'BINDING_STOP_READY'); fflush(stdout); while true; end;",
            Duration::from_secs(20),
        );
        (engine, result)
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut bytes = Vec::new();
    let mut ready = false;
    while Instant::now() < deadline && !ready {
        if let Ok(event) = subscription.recv_timeout(Duration::from_millis(100)) {
            match event.kind {
                ConsoleEventKind::Output { bytes: fragment } => bytes.extend(fragment),
                ConsoleEventKind::End | ConsoleEventKind::Disconnected { .. } => break,
                _ => {}
            }
            ready = bytes
                .windows(b"BINDING_STOP_READY".len())
                .any(|window| window == b"BINDING_STOP_READY");
        }
    }
    if !ready {
        let _ = cancellation.terminate();
        let _ = task.join();
        panic!("worker did not reach its explicit Stop barrier: {bytes:?}");
    }
    cancellation.interrupt().unwrap();
    let (mut engine, result) = task.join().unwrap();
    let result = result.unwrap();
    assert_eq!(result.error.unwrap().identifier, "MathLab:Interrupted");
    assert_eq!(engine.process_id(), process_id);
    assert_stale(engine.read_variable_checked(&old, 1, 1, 1, 1));
    assert_stale(engine.set_variable_element_checked(&old, 1, 1, "99"));
    let current = engine.workspace_snapshot().unwrap();
    let matrix = binding(&current, "A");
    let partial = binding(&current, "stop_partial");
    for (token, expected) in [(&matrix, "19"), (&partial, "23")] {
        assert_eq!(
            engine
                .read_variable_checked(token, 1, 1, 1, 1)
                .unwrap()
                .slice
                .cells[0][0],
            expected
        );
    }
}

#[test]
fn snapshot_and_checked_object_rejection_do_not_dispatch_user_inspection_methods() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    std::fs::write(
        directory.path().join("MathLabInspectionTrap.m"),
        include_str!("../../../tests/fixtures/class_support/MathLabInspectionTrap.m"),
    )
    .unwrap();
    execute(
        &mut engine,
        "trap=MathLabInspectionTrap(); plain=[1 2]; nested_cell={trap}; nested_struct=struct('value',{trap});",
    );
    let initial = engine.workspace_snapshot().unwrap();
    let trap = binding(&initial, "trap");
    let plain = binding(&initial, "plain");
    let metadata = initial
        .variables
        .iter()
        .find(|variable| variable.variable.name == "trap")
        .unwrap();
    assert_eq!(metadata.variable.class_name, "MathLabInspectionTrap");
    assert!(!metadata.variable.dimensions_known);
    assert!(!metadata.variable.bytes_known);
    assert!(metadata.variable.dimensions.is_empty());
    assert_eq!(metadata.variable.bytes, 0);
    let primitive_metadata = initial
        .variables
        .iter()
        .find(|variable| variable.variable.name == "plain")
        .unwrap();
    assert!(primitive_metadata.variable.dimensions_known);
    assert!(primitive_metadata.variable.bytes_known);
    for name in ["nested_cell", "nested_struct"] {
        let metadata = initial
            .variables
            .iter()
            .find(|variable| variable.variable.name == name)
            .unwrap();
        assert!(metadata.variable.dimensions_known);
        assert_eq!(metadata.variable.dimensions, vec![1, 1]);
        assert!(!metadata.variable.bytes_known);
        assert_eq!(metadata.variable.bytes, 0);
    }
    for _ in 0..2 {
        match engine.read_variable_checked(&trap, 1, 1, 1, 1) {
            Err(Error::Worker(error)) => {
                assert_eq!(error.identifier, "MathLab:VariableType");
            }
            Err(other) => panic!("expected object-type rejection, received {other:?}"),
            Ok(_) => panic!("checked primitive reader accepted an object"),
        }
        let current = engine.workspace_snapshot().unwrap();
        assert_eq!(current.context, initial.context);
        assert_eq!(binding(&current, "trap"), trap);
        assert!(
            !current
                .variables
                .iter()
                .any(|variable| variable.variable.name == "inspection_trap_called")
        );
        assert_eq!(
            engine
                .read_variable_checked(&plain, 1, 1, 1, 2)
                .unwrap()
                .slice
                .cells,
            vec![vec!["1", "2"]]
        );
    }
}

#[test]
fn base_helper_name_collisions_cannot_execute_during_checked_operations() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    std::fs::write(
        directory.path().join("binding_collision_hook.m"),
        "function varargout=binding_collision_hook(varargin)\n builtin('assignin','base','binding_collision_called',true);\n builtin('assignin','base','A',999);\n error('MathLab:BindingCollision','A base helper collision was invoked.');\nend\n",
    )
    .unwrap();
    for builtin_binding in ["@binding_collision_hook", "1"] {
        execute(
            &mut engine,
            &format!(
                "A=[1 2;3 4]; builtin={builtin_binding}; who=@binding_collision_hook; whos=@binding_collision_hook; feval=@binding_collision_hook;"
            ),
        );
        let before = engine.workspace_snapshot().unwrap();
        assert!(
            !before
                .variables
                .iter()
                .any(|variable| variable.variable.name == "binding_collision_called")
        );
        let old = binding(&before, "A");
        let page = engine.read_variable_checked(&old, 1, 1, 2, 2).unwrap();
        assert_eq!(page.slice.cells, vec![vec!["1", "2"], vec!["3", "4"]]);
        assert_eq!(engine.workspace_snapshot().unwrap().context, before.context);
        let receipt = engine
            .set_variable_element_checked(&old, 2, 1, "9")
            .unwrap();
        let after = engine.workspace_snapshot().unwrap();
        assert_eq!(after.context, receipt.token.context);
        assert!(after.context.revision > before.context.revision);
        assert!(
            !after
                .variables
                .iter()
                .any(|variable| variable.variable.name == "binding_collision_called")
        );
        assert_eq!(
            engine
                .read_variable_checked(&receipt.token, 1, 1, 2, 2)
                .unwrap()
                .slice
                .cells,
            vec![vec!["1", "2"], vec!["9", "4"]]
        );
    }
}

#[test]
fn shadowed_global_helpers_cannot_mutate_checked_reads_or_validated_edits() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let shadows = directory.path().join("shadowed_helpers");
    std::fs::create_dir(&shadows).unwrap();
    for name in ["size", "str2double", "repmat", "isequal"] {
        std::fs::write(
            shadows.join(format!("{name}.m")),
            format!(
                "function varargout={name}(varargin)\n builtin('assignin','base','binding_shadow_called','{name}');\n builtin('assignin','base','A',999);\n error('MathLab:BindingShadow','A shadowed {name} helper was invoked.');\nend\n"
            ),
        )
        .unwrap();
    }
    let path = shadows.to_string_lossy().replace('\'', "''");
    execute(
        &mut engine,
        &format!("addpath('{path}','-begin'); A=[1 2;3 4]; I=int16(7);"),
    );
    let before = engine.workspace_snapshot().unwrap();
    let matrix = binding(&before, "A");
    let page = engine.read_variable_checked(&matrix, 1, 1, 2, 2).unwrap();
    assert_eq!(page.slice.cells, vec![vec!["1", "2"], vec!["3", "4"]]);
    assert_eq!(engine.workspace_snapshot().unwrap().context, before.context);
    let receipt = engine
        .set_variable_element_checked(&matrix, 2, 1, "9")
        .unwrap();
    let edited = engine.workspace_snapshot().unwrap();
    assert_eq!(edited.context, receipt.token.context);
    assert!(edited.context.revision > before.context.revision);
    let integer = binding(&edited, "I");
    for (token, row, literal, expected_error) in [
        (&receipt.token, 3, "9", "MathLab:VariableIndex"),
        (&integer, 1, "1+2i", "MathLab:VariableLiteral"),
    ] {
        match engine.set_variable_element_checked(token, row, 1, literal) {
            Err(Error::Worker(error)) => assert_eq!(error.identifier, expected_error),
            Err(other) => panic!("unexpected checked-edit error: {other:?}"),
            Ok(_) => panic!("invalid checked edit was accepted"),
        }
        let current = engine.workspace_snapshot().unwrap();
        assert_eq!(current.context, edited.context);
        assert!(
            !current
                .variables
                .iter()
                .any(|variable| variable.variable.name == "binding_shadow_called")
        );
        assert_eq!(
            engine
                .read_variable_checked(&receipt.token, 1, 1, 2, 2)
                .unwrap()
                .slice
                .cells,
            vec![vec!["1", "2"], vec!["9", "4"]]
        );
        let page = engine.read_variable_checked(&integer, 1, 1, 1, 1).unwrap();
        assert_eq!(page.slice.class_name, "int16");
        assert_eq!(page.slice.cells[0][0], "7");
    }
}

struct PausedBindingGuard(Option<mathlab_engine::CancellationHandle>);

impl Drop for PausedBindingGuard {
    fn drop(&mut self) {
        if let Some(cancellation) = &self.0 {
            let _ = cancellation.terminate();
        }
    }
}

fn paused_evaluation_binding_case(streamed: bool) {
    use mathlab_debugger::ResumeAction;
    use std::path::Path;
    use std::thread;
    use std::time::{Duration, Instant};

    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        eprintln!(
            "Skipping native paused-binding integration (streamed={streamed}): set MATHLAB_DEBUG_BRIDGE to the locally compiled adapter."
        );
        return;
    };
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let function = std::fs::canonicalize(directory.path())
        .unwrap()
        .join("binding_pause.m");
    std::fs::write(
        &function,
        "function y=binding_pause(x)\n local_marker=x;\n y=local_marker+1;\nend\n",
    )
    .unwrap();
    let debugger = engine.enable_debugger(Path::new(&bridge)).unwrap();
    assert_eq!(debugger.set_breakpoint(&function, 2).unwrap(), vec![2]);
    execute(&mut engine, "A=19;");
    let before = engine.workspace_snapshot().unwrap();
    let old = binding(&before, "A");
    let process_id = engine.process_id();
    let mut guard = PausedBindingGuard(Some(engine.cancellation_handle()));
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "outer_before=7; outer_result=binding_pause(5); outer_after=999;",
            Duration::from_secs(25),
        );
        (engine, result)
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let pause = loop {
        let observed = debugger.state().unwrap();
        if let Some(state) = observed.as_ref()
            && state.paused
            && state.file == function
            && state.line == 2
        {
            break state.clone();
        }
        assert!(
            !task.is_finished(),
            "execution completed before its binding-test breakpoint: {observed:?}"
        );
        assert!(
            Instant::now() < deadline,
            "binding-test breakpoint did not become paused: {observed:?}"
        );
        thread::sleep(Duration::from_millis(15));
    };
    if streamed {
        let output = debugger
            .evaluate_output(
                "assignin('base','A',31); assignin('base','paused_partial',23); disp('BINDING_PAUSED_STREAM');",
                pause.revision,
            )
            .unwrap();
        assert!(output.streamed);
        assert!(!output.truncated);
        assert!(output.output.contains("BINDING_PAUSED_STREAM"));
    } else {
        let error = debugger
            .evaluate(
                "assignin('base','A',31); assignin('base','paused_partial',23); error('MathLab:PausedBindingTest','paused-binding-intentional');",
                pause.revision,
            )
            .unwrap_err();
        assert!(
            error.to_string().contains("paused-binding-intentional"),
            "{error}"
        );
    }
    let settled_pause = debugger.state().unwrap().unwrap();
    assert!(settled_pause.paused);
    debugger
        .resume(ResumeAction::Quit, settled_pause.revision)
        .unwrap();
    let (mut engine, result) = task.join().unwrap();
    guard.0 = None;
    let result = result.unwrap();
    assert_eq!(result.error.unwrap().identifier, "MathLab:DebugQuit");
    assert_eq!(engine.process_id(), process_id);
    assert_stale(engine.read_variable_checked(&old, 1, 1, 1, 1));
    assert_stale(engine.set_variable_element_checked(&old, 1, 1, "99"));
    let after = engine.workspace_snapshot().unwrap();
    assert!(after.context.revision > before.context.revision);
    assert!(!after.variables.iter().any(|variable| {
        matches!(
            variable.variable.name.as_str(),
            "outer_after" | "outer_result"
        )
    }));
    for (name, expected) in [("A", "31"), ("paused_partial", "23"), ("outer_before", "7")] {
        let token = binding(&after, name);
        assert_eq!(
            engine
                .read_variable_checked(&token, 1, 1, 1, 1)
                .unwrap()
                .slice
                .cells[0][0],
            expected
        );
    }
    assert_eq!(engine.workspace_snapshot().unwrap().context, after.context);
}

#[test]
fn optional_quiet_paused_evaluation_error_and_quit_preserve_partial_bindings() {
    paused_evaluation_binding_case(false);
}

#[test]
fn optional_streamed_paused_evaluation_and_quit_preserve_partial_bindings() {
    paused_evaluation_binding_case(true);
}
