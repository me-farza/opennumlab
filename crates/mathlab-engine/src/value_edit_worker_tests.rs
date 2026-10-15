// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual-worker tests bypassing the public edit codec where stated.
//! Private child module alongside the engine's other actual-worker unit tests.
//! No custom worker text, production hook or numerical callback is injected.

use super::{COMMAND_TIMEOUT, Engine, EngineConfig, Error, Result, discover_octave};
use crate::value_edit::{
    CAP_ATOMIC_RANGE_EDIT, HistoryRequest, LeafDescriptor, RangeEditReceipt, RangeEditRequest,
};
use crate::{
    BindingToken, NumericClass, NumericComponents, PageData, PageWindow, SourceStorage, ValuePage,
    ValuePageRequest, ValuePath, ValuePathStep, WorkspaceSnapshot,
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

fn engine() -> Option<(Engine, tempfile::TempDir)> {
    let Some(executable) = discover_octave() else {
        assert_ne!(
            std::env::var("MATHLAB_REQUIRE_VALUE_EDIT_TESTS").as_deref(),
            Ok("1"),
            "required worker edit tests need an installed Octave"
        );
        eprintln!("SKIP worker edit validation: no installed Octave executable.");
        return None;
    };
    let directory = tempfile::Builder::new()
        .prefix("mathlab-edit-worker-")
        .tempdir()
        .unwrap();
    let engine = Engine::start(EngineConfig {
        executable,
        working_directory: directory.path().to_owned(),
    })
    .unwrap();
    engine.require_capability(CAP_ATOMIC_RANGE_EDIT).unwrap();
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
        .find(|entry| entry.variable.name == name)
        .unwrap_or_else(|| panic!("missing {name}"))
        .token
        .clone()
}
fn window(rows: u32, columns: u32) -> PageWindow {
    PageWindow {
        row_start: 1.into(),
        column_start: 1.into(),
        rows,
        columns,
        higher_indices: vec![],
    }
}
fn read(
    engine: &mut Engine,
    name: &str,
    steps: Vec<ValuePathStep>,
    selected: PageWindow,
) -> ValuePage {
    let token = binding(&engine.workspace_snapshot().unwrap(), name);
    engine
        .read_value_page_checked(&ValuePageRequest {
            path: ValuePath { root: token, steps },
            window: selected,
        })
        .unwrap()
}
fn request_from_page(
    page: ValuePage,
    selected: PageWindow,
    real: &NumericComponents,
    imag: Option<&NumericComponents>,
) -> RangeEditRequest {
    RangeEditRequest::from_components(
        ValuePath {
            root: page.token,
            steps: page.path,
        },
        selected,
        LeafDescriptor {
            class: page.class,
            source_shape: page.source_shape,
            source_storage: page.source_storage,
            complex: page.complex,
        },
        real,
        imag,
    )
    .unwrap()
}
fn edit(
    engine: &mut Engine,
    name: &str,
    steps: Vec<ValuePathStep>,
    selected: PageWindow,
    real: &NumericComponents,
    imag: Option<&NumericComponents>,
) -> RangeEditRequest {
    let page = read(engine, name, steps, selected.clone());
    request_from_page(page, selected, real, imag)
}
fn raw_write(request: &RangeEditRequest) -> Value {
    let mut raw = serde_json::to_value(request).unwrap();
    raw["command"] = json!("write_value_range_checked");
    raw
}
fn assert_worker_error<T>(result: Result<T>, identifier: &str) {
    match result {
        Err(Error::Worker(error)) => assert_eq!(error.identifier, identifier, "{}", error.message),
        Err(other) => panic!("expected worker {identifier}, got {other:?}"),
        Ok(_) => panic!("malformed direct worker command was accepted"),
    }
}
fn history(receipt: &RangeEditReceipt, undo: bool) -> HistoryRequest {
    let head = if undo {
        &receipt.history.undo
    } else {
        &receipt.history.redo
    };
    let head = head.as_ref().unwrap();
    assert_eq!(head.root_name, receipt.token.name);
    HistoryRequest {
        schema_version: 1,
        expected: receipt.token.clone(),
        history_epoch: receipt.history.epoch,
        transaction_id: head.transaction_id,
    }
}
fn assert_dense(page: ValuePage, real: NumericComponents, imag: Option<NumericComponents>) {
    assert_eq!(page.source_storage, SourceStorage::Dense);
    assert_eq!(page.complex, imag.is_some());
    assert_eq!(page.data, PageData::Dense { real, imag });
}
fn assert_u64_root(engine: &mut Engine, expected: &[u64]) {
    let page = read(engine, "A", vec![], window(2, 3));
    assert_eq!(page.class, NumericClass::Uint64);
    assert_eq!(page.source_shape, vec![2.into(), 3.into()]);
    assert_dense(page, NumericComponents::Uint64(expected.to_vec()), None);
}
fn seed(engine: &mut Engine) -> RangeEditReceipt {
    let request = edit(
        engine,
        "A",
        vec![],
        window(1, 1),
        &NumericComponents::Uint64(vec![17]),
        None,
    );
    engine.write_value_range_checked(&request).unwrap()
}
fn assert_prior_history_works(engine: &mut Engine, seed: &RangeEditReceipt) {
    let undo = engine
        .undo_value_edit_checked(&history(seed, true))
        .unwrap();
    assert_eq!(undo.transaction_id, seed.transaction_id);
    assert_eq!(undo.history.epoch, seed.history.epoch);
    assert_eq!(undo.retained_transactions, seed.retained_transactions);
    assert_eq!(undo.retained_patch_bytes, seed.retained_patch_bytes);
    assert_u64_root(engine, &[1, 2, 3, 4, 5, 6]);
    let redo = engine
        .redo_value_edit_checked(&history(&undo, false))
        .unwrap();
    assert_eq!(redo.transaction_id, seed.transaction_id);
    assert_eq!(redo.history.epoch, seed.history.epoch);
    assert_eq!(redo.retained_transactions, seed.retained_transactions);
    assert_eq!(redo.retained_patch_bytes, seed.retained_patch_bytes);
    assert_u64_root(engine, &[17, 2, 3, 4, 5, 6]);
}

#[test]
fn worker_rejects_last_uint64_component_before_values_revision_or_history_change() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(&mut engine, "A=uint64(reshape(1:6,2,3)); peer=19;");
    let accepted = seed(&mut engine);
    let before = engine.workspace_snapshot().unwrap();
    let mut selected = window(2, 2);
    selected.column_start = 2.into();
    let valid = edit(
        &mut engine,
        "A",
        vec![],
        selected,
        &NumericComponents::Uint64(vec![101, 102, 103, u64::MAX]),
        None,
    );
    let mut raw = raw_write(&valid);
    raw["data"]["real"][3] = json!("18446744073709551616");
    // Deliberately bypass RangeEditRequest::validate_decode, submit_value_edit
    // and its local rejection/latch policy: this assertion is the worker's own.
    assert_worker_error(
        engine.request(raw, Some(COMMAND_TIMEOUT)),
        "MathLab:ValueEditPayload",
    );
    assert!(!engine.has_value_edit_uncertainty());
    assert_eq!(engine.workspace_snapshot().unwrap().context, before.context);
    assert_u64_root(&mut engine, &[17, 2, 3, 4, 5, 6]);
    assert_prior_history_works(&mut engine, &accepted);
}

#[test]
fn worker_rejects_malformed_history_authority_without_consuming_its_valid_head() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(&mut engine, "A=uint64(reshape(1:6,2,3));");
    let accepted = seed(&mut engine);
    let before = engine.workspace_snapshot().unwrap();
    let mut original = serde_json::to_value(history(&accepted, true)).unwrap();
    original["command"] = json!("undo_value_edit_checked");
    let mut wrong_id = original.clone();
    wrong_id["transaction_id"] = json!("999999");
    let mut wrong_epoch = original.clone();
    wrong_epoch["history_epoch"] = json!("01");
    let mut wrong_type = original.clone();
    wrong_type["transaction_id"] = json!(1);
    let mut extra_field = original;
    extra_field["evaluate"] = json!("A(:)=999;");
    for (payload, identifier) in [
        (wrong_id, "MathLab:ValueEditHistoryConflict"),
        (wrong_epoch, "MathLab:ValueEditHistoryConflict"),
        (wrong_type, "MathLab:ValueEditHistoryConflict"),
        (extra_field, "MathLab:ValueEditSchema"),
    ] {
        assert_worker_error(engine.request(payload, Some(COMMAND_TIMEOUT)), identifier);
        assert_eq!(engine.workspace_snapshot().unwrap().context, before.context);
        assert_u64_root(&mut engine, &[17, 2, 3, 4, 5, 6]);
    }
    assert!(!engine.has_value_edit_uncertainty());
    assert_prior_history_works(&mut engine, &accepted);
}

#[test]
fn worker_native_construction_and_path_budgets_reject_before_any_commit() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    // One nonzero with 1,048,576 columns: about8MiB of column indices on the
    // selected 64-bit runtime, rather than a large dense numerical allocation.
    execute(
        &mut engine,
        "A=uint64(reshape(1:6,2,3)); S=builtin('sparse',1,1,7,1,1048576);",
    );
    let accepted = seed(&mut engine);
    let before = engine.workspace_snapshot().unwrap();
    let sparse = edit(
        &mut engine,
        "S",
        vec![],
        window(1, 1),
        &NumericComponents::Double(vec![77.]),
        None,
    );
    assert_eq!(
        sparse.expected_leaf.source_shape,
        vec![1.into(), 1048576.into()]
    );
    assert_worker_error(
        engine.request(raw_write(&sparse), Some(COMMAND_TIMEOUT)),
        "MathLab:ValueEditLimit",
    );
    let request = edit(
        &mut engine,
        "A",
        vec![],
        window(1, 1),
        &NumericComponents::Uint64(vec![88]),
        None,
    );
    let mut too_deep = raw_write(&request);
    too_deep["path"]["steps"] = json!(vec![json!({"kind":"cell","indices":["1","1"]}); 17]);
    assert_worker_error(
        engine.request(too_deep, Some(COMMAND_TIMEOUT)),
        "MathLab:ValuePageLimit",
    );
    assert_eq!(engine.workspace_snapshot().unwrap().context, before.context);
    assert_u64_root(&mut engine, &[17, 2, 3, 4, 5, 6]);
    let original_sparse = read(&mut engine, "S", vec![], window(1, 1));
    assert_eq!(original_sparse.source_shape, vec![1.into(), 1048576.into()]);
    assert_eq!(
        original_sparse.data,
        PageData::SparseCoo {
            rows: vec![1],
            columns: vec![1],
            real: NumericComponents::Double(vec![7.]),
            imag: None
        }
    );
    assert!(!engine.has_value_edit_uncertainty());
    assert_prior_history_works(&mut engine, &accepted);
}

fn nested_steps() -> Vec<ValuePathStep> {
    vec![
        ValuePathStep::Field {
            indices: vec![1.into(), 1.into()],
            name: "payload".into(),
        },
        ValuePathStep::Cell {
            indices: vec![1.into(), 1.into()],
        },
    ]
}
fn assert_no_shadow_marker(engine: &mut Engine) {
    assert!(
        !engine
            .workspace_snapshot()
            .unwrap()
            .variables
            .iter()
            .any(|entry| entry.variable.name == "edit_shadow_called"),
        "a project helper ran inside a checked operation"
    );
}
fn roundtrip(engine: &mut Engine, request: &RangeEditRequest) -> RangeEditReceipt {
    let written = engine.write_value_range_checked(request).unwrap();
    assert_no_shadow_marker(engine);
    let undone = engine
        .undo_value_edit_checked(&history(&written, true))
        .unwrap();
    assert_eq!(undone.transaction_id, written.transaction_id);
    assert_no_shadow_marker(engine);
    let redone = engine
        .redo_value_edit_checked(&history(&undone, false))
        .unwrap();
    assert_eq!(redone.transaction_id, written.transaction_id);
    assert_no_shadow_marker(engine);
    redone
}

#[test]
fn project_shadow_helpers_cannot_run_in_nested_integer_complex_or_sparse_edit_history() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let shadows: PathBuf = directory.path().join("project-shadows");
    fs::create_dir(&shadows).unwrap();
    for name in [
        "size",
        "ndims",
        "numfields",
        "class",
        "isnumeric",
        "iscell",
        "isstruct",
        "str2double",
        "repmat",
        "isequal",
        "sprintf",
        "zeros",
        "complex",
        "typecast",
        "bitshift",
        "bitor",
        "find",
        "sparse",
        "real",
        "imag",
        "jsonencode",
        "__fieldnames__",
        "__unicode2native__",
        "mathlab_prepare_value_edit",
        "mathlab_edit_components",
        "mathlab_edit_integer",
        "mathlab_edit_float_bits",
        "mathlab_edit_history_accept_write",
        "mathlab_edit_history_restore_request",
        "mathlab_edit_history_accept_restore",
    ] {
        fs::write(shadows.join(format!("{name}.m")), format!(
            "function varargout={name}(varargin)\n builtin('assignin','base','edit_shadow_called','{name}');\n builtin('assignin','base','R',999);\n builtin('error','MathLab:ValueEditShadow','A project helper was called.');\nend\n"
        )).unwrap();
    }
    let path = shadows.to_string_lossy().replace('\'', "''");
    execute(
        &mut engine,
        &format!(
            "A=uint64(reshape(1:6,2,3)); R=struct('payload',{{{{A}}}},'peer',19); Ralias=R; F=complex(single([1 2;3 4]),single([5 6;7 8])); S=sparse([1 2],[1 3],[9 8],2,3); addpath('{path}','-begin');"
        ),
    );
    assert_no_shadow_marker(&mut engine);
    let mut selected = window(2, 2);
    selected.column_start = 2.into();
    let nested = edit(
        &mut engine,
        "R",
        nested_steps(),
        selected,
        &NumericComponents::Uint64(vec![101, 102, 103, 104]),
        None,
    );
    roundtrip(&mut engine, &nested);
    assert_dense(
        read(&mut engine, "R", nested_steps(), window(2, 3)),
        NumericComponents::Uint64(vec![1, 2, 101, 102, 103, 104]),
        None,
    );
    assert_dense(
        read(&mut engine, "Ralias", nested_steps(), window(2, 3)),
        NumericComponents::Uint64(vec![1, 2, 3, 4, 5, 6]),
        None,
    );
    assert_u64_root(&mut engine, &[1, 2, 3, 4, 5, 6]);

    let complex = edit(
        &mut engine,
        "F",
        vec![],
        window(1, 2),
        &NumericComponents::Single(vec![7.25, 8.5]),
        Some(&NumericComponents::Single(vec![-2., 3.])),
    );
    roundtrip(&mut engine, &complex);
    assert_dense(
        read(&mut engine, "F", vec![], window(2, 2)),
        NumericComponents::Single(vec![7.25, 3., 8.5, 4.]),
        Some(NumericComponents::Single(vec![-2., 7., 3., 8.])),
    );

    let sparse = edit(
        &mut engine,
        "S",
        vec![],
        window(2, 3),
        &NumericComponents::Double(vec![0., 20., 10., 0., 0., 30.]),
        None,
    );
    roundtrip(&mut engine, &sparse);
    let page = read(&mut engine, "S", vec![], window(2, 3));
    assert_eq!(page.source_storage, SourceStorage::Sparse);
    assert_eq!(
        page.data,
        PageData::SparseCoo {
            rows: vec![2, 1, 2],
            columns: vec![1, 2, 3],
            real: NumericComponents::Double(vec![20., 10., 30.]),
            imag: None
        }
    );
    assert_no_shadow_marker(&mut engine);

    // Positive controls use ordinary project resolution. The checked path must
    // neither invoke these functions nor remove/replace them on the user path.
    execute(
        &mut engine,
        "try; str2double('1'); catch caught; if (! builtin('strcmp',caught.identifier,'MathLab:ValueEditShadow')); builtin('error','MathLab:ValueEditShadowControl','Wrong control exception.'); endif; end; if (! builtin('strcmp',edit_shadow_called,'str2double')); builtin('error','MathLab:ValueEditShadowControl','Global shadow was not reachable.'); endif;",
    );
    execute(
        &mut engine,
        "try; mathlab_prepare_value_edit(); catch caught; if (! builtin('strcmp',caught.identifier,'MathLab:ValueEditShadow')); builtin('error','MathLab:ValueEditShadowControl','Wrong private-name control exception.'); endif; end; if (! builtin('strcmp',edit_shadow_called,'mathlab_prepare_value_edit')); builtin('error','MathLab:ValueEditShadowControl','Project private-name shadow was not reachable.'); endif;",
    );
}
