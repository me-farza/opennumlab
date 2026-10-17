// SPDX-License-Identifier: GPL-3.0-or-later
//! Real-worker path-admission and session-state regressions.
//! Requires a genuine Octave runtime; no fallback worker is used.
#![cfg(unix)]

use mathlab_engine::value_edit::{HistoryRequest, LeafDescriptor, RangeEditRequest};
use mathlab_engine::{
    BindingToken, ConsoleEventKind, ConsoleSubscription, Engine, EngineConfig, Error,
    NumericComponents, PageData, PageWindow, ValuePage, ValuePageRequest, ValuePath,
    WorkspaceSnapshot, discover_octave,
};
use std::time::Duration;
use std::{ffi::OsString, fs, os::unix::ffi::OsStringExt, path::Path, path::PathBuf};

fn session() -> (Engine, tempfile::TempDir) {
    // This explicit qualification target must fail rather than report a pass
    // when its genuine runtime prerequisite is absent.
    let executable = discover_octave().expect("wire-path qualification requires real Octave");
    let directory = tempfile::Builder::new()
        .prefix("mathlab wire path λ ")
        .tempdir()
        .unwrap();
    let engine = Engine::start(EngineConfig {
        executable,
        working_directory: directory.path().to_owned(),
    })
    .expect("the real Engine worker must start");
    (engine, directory)
}

fn execute(engine: &mut Engine, code: &str) {
    let result = engine.execute(code).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
}

fn token(snapshot: &WorkspaceSnapshot, name: &str) -> BindingToken {
    snapshot
        .variables
        .iter()
        .find(|binding| binding.variable.name == name)
        .unwrap_or_else(|| panic!("missing actual workspace binding {name}"))
        .token
        .clone()
}

fn window() -> PageWindow {
    PageWindow {
        row_start: 1.into(),
        column_start: 1.into(),
        rows: 1,
        columns: 2,
        higher_indices: Vec::new(),
    }
}

fn read(engine: &mut Engine, binding: BindingToken) -> ValuePage {
    engine
        .read_value_page_checked(&ValuePageRequest {
            path: ValuePath {
                root: binding,
                steps: Vec::new(),
            },
            window: window(),
        })
        .unwrap()
}

fn assert_dense(page: &ValuePage, expected: [f64; 2]) {
    match &page.data {
        PageData::Dense {
            real: NumericComponents::Double(values),
            imag: None,
        } => {
            assert_eq!(values.len(), 2);
            for (actual, expected) in values.iter().zip(expected) {
                assert_eq!(actual.to_bits(), expected.to_bits());
            }
        }
        other => panic!("expected the actual real-double row, got {other:?}"),
    }
}

fn assert_encoding_error<T>(result: Result<T, Error>, role: &'static str, path: &Path) {
    match result {
        Err(Error::NonUtf8Path {
            role: actual_role,
            path: actual_path,
        }) => {
            assert_eq!(actual_role, role);
            assert_eq!(actual_path.as_path(), path);
        }
        Err(other) => panic!("expected a typed path error, got {other:?}"),
        Ok(_) => panic!("a non-UTF-8 wire identity was accepted"),
    }
}

fn invalid_path() -> PathBuf {
    // In-memory identity only. No invalid filename is created on macOS/APFS.
    PathBuf::from(OsString::from_vec(b"/not-created/invalid-\xff.m".to_vec()))
}

fn only_completed_request(subscription: &ConsoleSubscription) -> u64 {
    // Engine's synchronous finish waits for the matching console End before
    // returning. This fixture has no background output or interactive input.
    let mut begins = Vec::new();
    let mut ends = Vec::new();
    while let Ok(event) = subscription.try_recv() {
        match event.kind {
            ConsoleEventKind::Begin => begins.push(event.request_id),
            ConsoleEventKind::End => ends.push(event.request_id),
            other => panic!("unexpected event during quiet snapshot: {other:?}"),
        }
    }
    assert_eq!(begins.len(), 1, "unexpected request submission count");
    assert_eq!(ends.len(), 1, "unexpected completed request count");
    assert_eq!(begins, ends);
    ends[0].expect("ordinary snapshot has an exact request identity")
}

#[test]
fn rejected_public_paths_preserve_bindings_cached_generation_and_edit_history_without_dispatch() {
    let (mut engine, directory) = session();
    let original = directory.path().join("source_selection.m");
    let source = "selection_marker=path_local();\nfunction y=path_local()\ny=37;\nend\n";
    let staged = engine.prepare_debug_buffer(source, &original).unwrap();
    execute(&mut engine, "A=[1 2]; untouched_marker=37;");
    let initial = engine.workspace_snapshot().unwrap();
    let page = read(&mut engine, token(&initial, "A"));
    assert_dense(&page, [1.0, 2.0]);
    let request = RangeEditRequest::from_components(
        ValuePath {
            root: page.token,
            steps: page.path,
        },
        window(),
        LeafDescriptor {
            class: page.class,
            source_shape: page.source_shape,
            source_storage: page.source_storage,
            complex: page.complex,
        },
        &NumericComponents::Double(vec![9.0, 2.0]),
        None,
    )
    .unwrap();
    let written = engine.write_value_range_checked(&request).unwrap();
    assert!(written.history.undo.is_some());
    let subscription = engine.subscribe_console();
    let stable = engine.workspace_snapshot().unwrap();
    let stable_request_id = only_completed_request(&subscription);
    assert_eq!(token(&stable, "A"), written.token);
    let pid = engine.process_id();
    let working_directory = engine.working_directory().to_owned();
    let adapter_paths = (
        engine.input_bridge_path().map(Path::to_owned),
        engine.scope_bridge_path().map(Path::to_owned),
        engine.value_bridge_path().map(Path::to_owned),
    );
    let debugger_enabled = engine.debugger_handle().is_some();
    let invalid = invalid_path();

    assert_encoding_error(engine.run_file(&invalid), "session path", &invalid);
    assert_encoding_error(
        engine.run_file_with_timeout(&invalid, Duration::from_secs(5)),
        "session path",
        &invalid,
    );
    assert_encoding_error(
        engine.prepare_debug_file(&invalid),
        "session path",
        &invalid,
    );
    assert_encoding_error(engine.set_directory(&invalid), "session path", &invalid);
    assert_encoding_error(
        engine.run_buffer("error('must not run');", &invalid),
        "source path",
        &invalid,
    );
    assert_encoding_error(
        engine.prepare_debug_buffer(source, &invalid),
        "source path",
        &invalid,
    );
    assert_encoding_error(
        engine.enable_input_adapter(&invalid),
        "input adapter path",
        &invalid,
    );
    assert_encoding_error(
        engine.enable_debugger(&invalid),
        "debugger adapter path",
        &invalid,
    );
    assert_encoding_error(
        engine.enable_scope_bridge(&invalid),
        "scope adapter path",
        &invalid,
    );
    assert_encoding_error(
        engine.enable_value_bridge(&invalid),
        "value adapter path",
        &invalid,
    );

    let after = engine.workspace_snapshot().unwrap();
    assert_eq!(
        only_completed_request(&subscription),
        stable_request_id.checked_add(1).unwrap(),
        "rejected paths must neither submit a request nor consume a request identity"
    );
    assert_eq!(
        after.context, stable.context,
        "rejection must precede effectful dispatch"
    );
    assert_eq!(token(&after, "A"), written.token);
    assert_eq!(
        token(&after, "untouched_marker"),
        token(&stable, "untouched_marker")
    );
    assert_eq!(engine.process_id(), pid);
    assert_eq!(engine.working_directory(), working_directory);
    assert_eq!(
        (
            engine.input_bridge_path().map(Path::to_owned),
            engine.scope_bridge_path().map(Path::to_owned),
            engine.value_bridge_path().map(Path::to_owned)
        ),
        adapter_paths
    );
    assert_eq!(engine.debugger_handle().is_some(), debugger_enabled);
    assert!(!engine.has_value_edit_uncertainty());
    assert_dense(&read(&mut engine, written.token.clone()), [9.0, 2.0]);

    // Re-preparation can rewrite active routing. These public assertions prove
    // cached generation reuse; immediate map preservation needs private coverage.
    let selected = engine.prepare_debug_buffer(source, &original).unwrap();
    assert_eq!(selected.original_path, staged.original_path);
    assert_eq!(selected.script_path, staged.script_path);
    assert_eq!(selected.prepared, staged.prepared);
    let undo = engine
        .undo_value_edit_checked(&HistoryRequest {
            schema_version: 1,
            expected: written.token,
            history_epoch: written.history.epoch,
            transaction_id: written.transaction_id,
        })
        .unwrap();
    assert_dense(&read(&mut engine, undo.token.clone()), [1.0, 2.0]);
    let redo = engine
        .redo_value_edit_checked(&HistoryRequest {
            schema_version: 1,
            expected: undo.token,
            history_epoch: undo.history.epoch,
            transaction_id: undo.transaction_id,
        })
        .unwrap();
    assert_dense(&read(&mut engine, redo.token), [9.0, 2.0]);
}

#[test]
fn valid_unicode_apostrophe_paths_execute_saved_and_dirty_source_and_change_directory() {
    let (mut engine, directory) = session();
    let saved = directory.path().join("it's λ saved.m");
    fs::write(&saved, "A=[41 42];\n").unwrap();
    let saved_before = fs::read(&saved).unwrap();
    let result = engine.run_file(&saved).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    let snapshot = engine.workspace_snapshot().unwrap();
    assert_dense(&read(&mut engine, token(&snapshot, "A")), [41.0, 42.0]);
    let staged = engine.prepare_debug_file(&saved).unwrap();
    assert_eq!(staged.original_path, fs::canonicalize(&saved).unwrap());

    let result = engine.run_buffer("A=[73 74];\n", &saved).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(
        fs::read(&saved).unwrap(),
        saved_before,
        "dirty buffer must not overwrite saved source"
    );
    let snapshot = engine.workspace_snapshot().unwrap();
    assert_dense(&read(&mut engine, token(&snapshot, "A")), [73.0, 74.0]);
    let folder = directory.path().join("it's λ folder");
    fs::create_dir(&folder).unwrap();
    engine.set_directory(&folder).unwrap();
    assert_eq!(
        engine.working_directory(),
        fs::canonicalize(&folder).unwrap()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn utf8_aliases_to_non_utf8_targets_are_rejected_before_live_state_changes() {
    use std::os::unix::fs::symlink;
    let (mut engine, directory) = session();
    execute(&mut engine, "A=[5 6];");
    let subscription = engine.subscribe_console();
    let baseline = engine.workspace_snapshot().unwrap();
    let baseline_request_id = only_completed_request(&subscription);
    let target = directory
        .path()
        .join(OsString::from_vec(b"target-\xff.m".to_vec()));
    fs::write(&target, "error('must not execute alias target');\n").unwrap();
    let alias = directory.path().join("valid-alias.m");
    symlink(&target, &alias).unwrap();
    let canonical_target = fs::canonicalize(&target).unwrap();
    assert_encoding_error(
        engine.run_file(&alias),
        "resolved session path",
        &canonical_target,
    );
    assert_encoding_error(
        engine.prepare_debug_file(&alias),
        "resolved session path",
        &canonical_target,
    );
    assert_encoding_error(
        engine.run_buffer("A=[99 99];", &alias),
        "resolved source path",
        &canonical_target,
    );
    assert_encoding_error(
        engine.prepare_debug_buffer("A=[99 99];", &alias),
        "resolved source path",
        &canonical_target,
    );

    let adapter_alias = directory.path().join("valid-alias.oct");
    symlink(&target, &adapter_alias).unwrap();
    assert_encoding_error(
        engine.enable_input_adapter(&adapter_alias),
        "input adapter path",
        &canonical_target,
    );
    assert_encoding_error(
        engine.enable_debugger(&adapter_alias),
        "debugger adapter path",
        &canonical_target,
    );
    assert_encoding_error(
        engine.enable_scope_bridge(&adapter_alias),
        "scope adapter path",
        &canonical_target,
    );
    assert_encoding_error(
        engine.enable_value_bridge(&adapter_alias),
        "value adapter path",
        &canonical_target,
    );
    let invalid_folder = directory
        .path()
        .join(OsString::from_vec(b"folder-\xff".to_vec()));
    fs::create_dir(&invalid_folder).unwrap();
    let folder_alias = directory.path().join("valid-directory-alias");
    symlink(&invalid_folder, &folder_alias).unwrap();
    assert_encoding_error(
        engine.set_directory(&folder_alias),
        "resolved session path",
        &fs::canonicalize(&invalid_folder).unwrap(),
    );
    let after = engine.workspace_snapshot().unwrap();
    assert_eq!(
        only_completed_request(&subscription),
        baseline_request_id.checked_add(1).unwrap(),
        "canonical path rejection must precede all request dispatch and ID consumption"
    );
    assert_eq!(after.context, baseline.context);
    assert_eq!(token(&after, "A"), token(&baseline, "A"));
    assert_dense(&read(&mut engine, token(&after, "A")), [5.0, 6.0]);
}
