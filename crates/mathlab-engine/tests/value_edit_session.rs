// SPDX-License-Identifier: GPL-3.0-or-later
//! CP03 actual-worker range-edit tests.
//!
//! Expectations are Rust constants/index arithmetic, not values
//! copied from worker preparation, a second codec, or a generated golden.
//! These tests do not measure COW/RSS or prove cancellation inside a commit.

use mathlab_engine::value_edit::{
    CAP_ATOMIC_RANGE_EDIT, EditData, EditOperation, HistoryRequest, LeafDescriptor,
    RangeEditReceipt, RangeEditRequest,
};
use mathlab_engine::{
    BindingToken, DecimalU64, Engine, EngineConfig, NumericClass, NumericComponents, PageData,
    PageWindow, SourceStorage, ValuePage, ValuePageRequest, ValuePath, ValuePathStep,
    WorkspaceSnapshot, discover_octave,
};
use std::{fs, path::PathBuf};

fn engine() -> Option<(Engine, tempfile::TempDir)> {
    let Some(executable) = discover_octave() else {
        assert_ne!(
            std::env::var("MATHLAB_REQUIRE_VALUE_EDIT_TESTS").as_deref(),
            Ok("1"),
            "required value-edit integration tests need an installed Octave executable"
        );
        eprintln!("SKIP value-edit integration: no installed Octave executable.");
        return None;
    };
    let directory = tempfile::Builder::new()
        .prefix("mathlab-value-edit-test-")
        .tempdir()
        .unwrap();
    let engine = Engine::start(EngineConfig {
        executable,
        working_directory: directory.path().to_owned(),
    })
    .expect("installed Octave must start the value-edit worker");
    // An installed runtime lacking the implementation is a failure, not a skip.
    engine.require_capability("typed_nd_page.v1").unwrap();
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
        .unwrap_or_else(|| panic!("snapshot did not contain {name}"))
        .token
        .clone()
}

fn decimals(values: &[u64]) -> Vec<DecimalU64> {
    values.iter().copied().map(DecimalU64::new).collect()
}

fn field(name: &str) -> ValuePathStep {
    ValuePathStep::Field {
        indices: decimals(&[1, 1]),
        name: name.to_owned(),
    }
}

fn cell(row: u64, column: u64) -> ValuePathStep {
    ValuePathStep::Cell {
        indices: decimals(&[row, column]),
    }
}

fn window(row: u64, column: u64, rows: u32, columns: u32, higher: &[u64]) -> PageWindow {
    PageWindow {
        row_start: row.into(),
        column_start: column.into(),
        rows,
        columns,
        higher_indices: decimals(higher),
    }
}

fn read(
    engine: &mut Engine,
    name: &str,
    steps: Vec<ValuePathStep>,
    selected: PageWindow,
) -> ValuePage {
    let snapshot = engine.workspace_snapshot().unwrap();
    engine
        .read_value_page_checked(&ValuePageRequest {
            path: ValuePath {
                root: binding(&snapshot, name),
                steps,
            },
            window: selected,
        })
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
    // A request is anchored to an actual accepted typed page, including its
    // full source descriptor; no source metadata is inferred from the RHS.
    let page = read(engine, name, steps, selected.clone());
    assert_eq!(page.selection.rows, selected.rows);
    assert_eq!(page.selection.columns, selected.columns);
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

fn assert_components(actual: &NumericComponents, expected: &NumericComponents) {
    macro_rules! floats {
        ($actual:expr, $expected:expr) => {{
            assert_eq!($actual.len(), $expected.len());
            for (actual, expected) in $actual.iter().zip($expected) {
                if expected.is_nan() {
                    // Read pages use decimal NaN; no payload claim is possible.
                    assert!(actual.is_nan());
                } else {
                    assert_eq!(actual.to_bits(), expected.to_bits());
                }
            }
        }};
    }
    match (actual, expected) {
        (NumericComponents::Single(actual), NumericComponents::Single(expected)) => {
            floats!(actual, expected)
        }
        (NumericComponents::Double(actual), NumericComponents::Double(expected)) => {
            floats!(actual, expected)
        }
        _ => assert_eq!(actual, expected),
    }
}

fn assert_dense(page: &ValuePage, real: &NumericComponents, imag: Option<&NumericComponents>) {
    assert_eq!(page.source_storage, SourceStorage::Dense);
    assert_eq!(page.complex, imag.is_some());
    let PageData::Dense {
        real: actual,
        imag: actual_imag,
    } = &page.data
    else {
        panic!("dense leaf was not returned as dense native components");
    };
    assert_components(actual, real);
    match (actual_imag, imag) {
        (Some(actual), Some(expected)) => assert_components(actual, expected),
        (None, None) => {}
        _ => panic!("dense imaginary components changed arity"),
    }
}

fn assert_sparse(
    page: &ValuePage,
    rows: &[u32],
    columns: &[u32],
    real: &NumericComponents,
    imag: Option<&NumericComponents>,
) {
    assert_eq!(page.source_storage, SourceStorage::Sparse);
    assert_eq!(page.complex, imag.is_some());
    let PageData::SparseCoo {
        rows: actual_rows,
        columns: actual_columns,
        real: actual,
        imag: actual_imag,
    } = &page.data
    else {
        panic!("sparse leaf was densified");
    };
    assert_eq!(actual_rows, rows);
    assert_eq!(actual_columns, columns);
    assert_components(actual, real);
    match (actual_imag, imag) {
        (Some(actual), Some(expected)) => assert_components(actual, expected),
        (None, None) => {}
        _ => panic!("sparse imaginary components changed arity"),
    }
}

fn assert_advanced(previous: &BindingToken, receipt: &RangeEditReceipt, action: EditOperation) {
    let mut expected = previous.clone();
    expected.context.revision = expected.context.revision.checked_add(1).unwrap();
    assert_eq!(receipt.token, expected);
    assert_eq!(receipt.operation, action);
    assert!(receipt.transaction_id.get() > 0);
    assert!(receipt.history.epoch.get() > 0);
    assert!((1..=32).contains(&receipt.retained_transactions));
    assert!((1..=2 * 1024 * 1024).contains(&receipt.retained_patch_bytes));
}

fn write(engine: &mut Engine, request: &RangeEditRequest) -> RangeEditReceipt {
    let receipt = engine.write_value_range_checked(request).unwrap();
    assert_advanced(&request.path.root, &receipt, EditOperation::Write);
    assert_eq!(
        engine.workspace_snapshot().unwrap().context,
        receipt.token.context
    );
    receipt
}

fn history_request(engine: &mut Engine, receipt: &RangeEditReceipt, undo: bool) -> HistoryRequest {
    let head = if undo {
        receipt.history.undo.as_ref()
    } else {
        receipt.history.redo.as_ref()
    }
    .expect("test expected a current history head");
    let snapshot = engine.workspace_snapshot().unwrap();
    HistoryRequest {
        schema_version: 1,
        expected: binding(&snapshot, &head.root_name),
        history_epoch: receipt.history.epoch,
        transaction_id: head.transaction_id,
    }
}

fn undo(engine: &mut Engine, receipt: &RangeEditReceipt) -> RangeEditReceipt {
    let request = history_request(engine, receipt, true);
    let next = engine.undo_value_edit_checked(&request).unwrap();
    assert_advanced(&request.expected, &next, EditOperation::Undo);
    assert_eq!(next.transaction_id, request.transaction_id);
    assert_eq!(next.history.epoch, request.history_epoch);
    next
}

fn redo(engine: &mut Engine, receipt: &RangeEditReceipt) -> RangeEditReceipt {
    let request = history_request(engine, receipt, false);
    let next = engine.redo_value_edit_checked(&request).unwrap();
    assert_advanced(&request.expected, &next, EditOperation::Redo);
    assert_eq!(next.transaction_id, request.transaction_id);
    assert_eq!(next.history.epoch, request.history_epoch);
    next
}

fn scalar_edit(engine: &mut Engine, name: &str, value: f64) -> RangeEditRequest {
    edit(
        engine,
        name,
        vec![],
        window(1, 1, 1, 1, &[]),
        &NumericComponents::Double(vec![value]),
        None,
    )
}

#[test]
fn dense_rectangle_validates_the_last_component_before_any_commit_and_never_clips() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(&mut engine, "A=reshape(1:20,4,5); peer=19;");
    let accepted = engine.workspace_snapshot().unwrap();
    let request = edit(
        &mut engine,
        "A",
        vec![],
        window(2, 2, 2, 2, &[]),
        &NumericComponents::Double(vec![101., 102., 103., 104.]),
        None,
    );
    let mut malformed = request.clone();
    let EditData::Dense { real, .. } = &mut malformed.data;
    real[3] = "1; A(1)=999".into();
    let mut outside = request.clone();
    outside.window.row_start = 4.into();
    let mut changed_descriptor = request.clone();
    changed_descriptor.expected_leaf.source_shape[1] = 6.into();
    for rejected in [malformed, outside, changed_descriptor] {
        assert!(engine.write_value_range_checked(&rejected).is_err());
        assert_eq!(
            engine.workspace_snapshot().unwrap().context,
            accepted.context
        );
        assert_dense(
            &read(&mut engine, "A", vec![], window(1, 1, 4, 5, &[])),
            &NumericComponents::Double((1..=20).map(f64::from).collect()),
            None,
        );
    }
    let receipt = write(&mut engine, &request);
    let mut expected: Vec<f64> = (1..=20).map(f64::from).collect();
    for (index, value) in [(5, 101.), (6, 102.), (9, 103.), (10, 104.)] {
        expected[index] = value;
    }
    assert_dense(
        &read(&mut engine, "A", vec![], window(1, 1, 4, 5, &[])),
        &NumericComponents::Double(expected.clone()),
        None,
    );
    assert!(engine.write_value_range_checked(&request).is_err());
    assert_dense(
        &read(&mut engine, "peer", vec![], window(1, 1, 1, 1, &[])),
        &NumericComponents::Double(vec![19.]),
        None,
    );
    let undone = undo(&mut engine, &receipt);
    assert_dense(
        &read(&mut engine, "A", vec![], window(1, 1, 4, 5, &[])),
        &NumericComponents::Double((1..=20).map(f64::from).collect()),
        None,
    );
    let _redone = redo(&mut engine, &undone);
    assert_dense(
        &read(&mut engine, "A", vec![], window(1, 1, 4, 5, &[])),
        &NumericComponents::Double(expected),
        None,
    );
}

#[test]
fn every_integer_class_and_logical_keep_exact_bounds_without_saturation() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(
        &mut engine,
        "I8=zeros(2,1,'int8'); U8=zeros(2,1,'uint8'); I16=zeros(2,1,'int16'); U16=zeros(2,1,'uint16'); I32=zeros(2,1,'int32'); U32=zeros(2,1,'uint32'); I64=zeros(2,1,'int64'); U64=zeros(2,1,'uint64'); L=false(2,1);",
    );
    let cases = [
        ("I8", NumericComponents::Int8(vec![i8::MIN, i8::MAX]), "128"),
        ("U8", NumericComponents::Uint8(vec![0, u8::MAX]), "256"),
        (
            "I16",
            NumericComponents::Int16(vec![i16::MIN, i16::MAX]),
            "32768",
        ),
        ("U16", NumericComponents::Uint16(vec![0, u16::MAX]), "65536"),
        (
            "I32",
            NumericComponents::Int32(vec![i32::MIN, i32::MAX]),
            "2147483648",
        ),
        (
            "U32",
            NumericComponents::Uint32(vec![0, u32::MAX]),
            "4294967296",
        ),
        (
            "I64",
            NumericComponents::Int64(vec![i64::MIN, i64::MAX]),
            "9223372036854775808",
        ),
        (
            "U64",
            NumericComponents::Uint64(vec![9_007_199_254_741_009, u64::MAX]),
            "18446744073709551616",
        ),
        ("L", NumericComponents::Logical(vec![false, true]), "2"),
    ];
    for (name, expected, overflow) in cases {
        let before = read(&mut engine, name, vec![], window(1, 1, 2, 1, &[]));
        let request = edit(
            &mut engine,
            name,
            vec![],
            window(1, 1, 2, 1, &[]),
            &expected,
            None,
        );
        let underflow = match name {
            "I8" => "-129",
            "I16" => "-32769",
            "I32" => "-2147483649",
            "I64" => "-9223372036854775809",
            _ => "-1",
        };
        for (index, text) in [(0, underflow), (1, overflow)] {
            let mut bad = request.clone();
            let EditData::Dense { real, .. } = &mut bad.data;
            real[index] = text.into();
            assert!(
                engine.write_value_range_checked(&bad).is_err(),
                "{name} out-of-range component {text} accepted"
            );
            assert_eq!(
                engine.workspace_snapshot().unwrap().context,
                before.token.context
            );
            assert_eq!(
                read(&mut engine, name, vec![], window(1, 1, 2, 1, &[])).data,
                before.data
            );
        }
        let receipt = write(&mut engine, &request);
        let after = read(&mut engine, name, vec![], window(1, 1, 2, 1, &[]));
        assert_eq!(after.class, before.class);
        assert_dense(&after, &expected, None);
        let undone = undo(&mut engine, &receipt);
        assert_eq!(
            read(&mut engine, name, vec![], window(1, 1, 2, 1, &[])).data,
            before.data
        );
        let _redone = redo(&mut engine, &undone);
        assert_dense(
            &read(&mut engine, name, vec![], window(1, 1, 2, 1, &[])),
            &expected,
            None,
        );
    }
}

#[test]
fn native_float_bits_survive_write_undo_redo_without_double_rounding() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(&mut engine, "F=zeros(2,3,'single'); D=zeros(2,3);");
    let cases = [
        (
            "F",
            NumericClass::Single,
            NumericComponents::Single(vec![
                f32::from_bits(0x3f80_0001),
                -0.0,
                f32::from_bits(1),
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NAN,
            ]),
            NumericComponents::Single(vec![0.; 6]),
        ),
        (
            "D",
            NumericClass::Double,
            NumericComponents::Double(vec![
                f64::from_bits(0x3ff0_0000_0000_0001),
                -0.0,
                f64::from_bits(1),
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NAN,
            ]),
            NumericComponents::Double(vec![0.; 6]),
        ),
    ];
    for (name, class, expected, zero) in cases {
        let request = edit(
            &mut engine,
            name,
            vec![],
            window(1, 1, 2, 3, &[]),
            &expected,
            None,
        );
        let receipt = write(&mut engine, &request);
        let after = read(&mut engine, name, vec![], window(1, 1, 2, 3, &[]));
        assert_eq!(after.class, class);
        assert_dense(&after, &expected, None);
        let undone = undo(&mut engine, &receipt);
        assert_dense(
            &read(&mut engine, name, vec![], window(1, 1, 2, 3, &[])),
            &zero,
            None,
        );
        let _redone = redo(&mut engine, &undone);
        assert_dense(
            &read(&mut engine, name, vec![], window(1, 1, 2, 3, &[])),
            &expected,
            None,
        );
    }
}

#[test]
fn complex_single_and_double_preserve_component_class_and_zero_imaginary_state() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(
        &mut engine,
        "F=complex(reshape(single(1:4),2,2),reshape(single(5:8),2,2)); D=complex(reshape(1:4,2,2),reshape(5:8,2,2));",
    );
    let cases = [
        (
            "F",
            NumericClass::Single,
            NumericComponents::Single(vec![-0., 1.25, f32::from_bits(1), f32::INFINITY]),
            NumericComponents::Single(vec![f32::from_bits(1), -0., -2.5, f32::NAN]),
            NumericComponents::Single(vec![0.; 4]),
        ),
        (
            "D",
            NumericClass::Double,
            NumericComponents::Double(vec![-0., 1.25, f64::from_bits(1), f64::INFINITY]),
            NumericComponents::Double(vec![f64::from_bits(1), -0., -2.5, f64::NAN]),
            NumericComponents::Double(vec![0.; 4]),
        ),
    ];
    for (name, class, real, imag, zero) in cases {
        let request = edit(
            &mut engine,
            name,
            vec![],
            window(1, 1, 2, 2, &[]),
            &real,
            Some(&imag),
        );
        let receipt = write(&mut engine, &request);
        let after = read(&mut engine, name, vec![], window(1, 1, 2, 2, &[]));
        assert_eq!(after.class, class);
        assert_dense(&after, &real, Some(&imag));
        let undone = undo(&mut engine, &receipt);
        let _redone = redo(&mut engine, &undone);
        assert_dense(
            &read(&mut engine, name, vec![], window(1, 1, 2, 2, &[])),
            &real,
            Some(&imag),
        );
        // A same-class complex edit containing all-zero imaginary values must
        // preserve complex metadata or reject before commit; this v1 gate
        // requires that preservation to be implemented by the enabled backend.
        let zero_imag = edit(
            &mut engine,
            name,
            vec![],
            window(1, 1, 2, 2, &[]),
            &real,
            Some(&zero),
        );
        let zero_receipt = write(&mut engine, &zero_imag);
        assert_dense(
            &read(&mut engine, name, vec![], window(1, 1, 2, 2, &[])),
            &real,
            Some(&zero),
        );
        let _restored = undo(&mut engine, &zero_receipt);
        assert_dense(
            &read(&mut engine, name, vec![], window(1, 1, 2, 2, &[])),
            &real,
            Some(&imag),
        );
    }
}

#[test]
fn original_nd_fixture_edits_plane_four_with_value_copy_and_other_planes_untouched() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../compatibility/conformance/fixtures/exact_nd_uint64.m");
    let result = engine.run_file(&fixture).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    execute(&mut engine, "fixture_alias=result.original; peer=19;");
    let request = edit(
        &mut engine,
        "result",
        vec![field("original")],
        window(2, 3, 1, 1, &[4]),
        &NumericComponents::Double(vec![444.]),
        None,
    );
    let receipt = write(&mut engine, &request);
    let mut edited: Vec<f64> = (37..=48).map(f64::from).collect();
    edited[7] = 444.;
    for (name, steps, expected) in [
        ("result", vec![field("original")], edited.clone()),
        ("result", vec![field("copy")], edited.clone()),
        ("fixture_alias", vec![], (37..=48).map(f64::from).collect()),
    ] {
        let page = read(&mut engine, name, steps, window(1, 1, 3, 4, &[4]));
        assert_eq!(page.source_shape, decimals(&[3, 4, 5]));
        assert_dense(&page, &NumericComponents::Double(expected), None);
    }
    assert_dense(
        &read(
            &mut engine,
            "result",
            vec![field("original")],
            window(1, 1, 3, 4, &[3]),
        ),
        &NumericComponents::Double((25..=36).map(f64::from).collect()),
        None,
    );
    let _undone = undo(&mut engine, &receipt);
    assert_dense(
        &read(
            &mut engine,
            "result",
            vec![field("original")],
            window(1, 1, 3, 4, &[4]),
        ),
        &NumericComponents::Double((37..=48).map(f64::from).collect()),
        None,
    );
    assert_dense(
        &read(
            &mut engine,
            "result",
            vec![field("copy")],
            window(1, 1, 3, 4, &[4]),
        ),
        &NumericComponents::Double(edited),
        None,
    );
}

#[test]
fn nested_cell_struct_rebuild_preserves_opaque_siblings_aliases_and_peers() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let trap = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/class_support/MathLabInspectionTrap.m");
    fs::copy(trap, directory.path().join("MathLabInspectionTrap.m")).unwrap();
    execute(
        &mut engine,
        "trap=MathLabInspectionTrap(); leaf=struct(); leaf.values={reshape(uint64(1:6),2,3),trap}; nested={leaf}; nested_alias=nested; peer=19;",
    );
    let steps = vec![cell(1, 1), field("values"), cell(1, 1)];
    let request = edit(
        &mut engine,
        "nested",
        steps.clone(),
        window(1, 2, 2, 2, &[]),
        &NumericComponents::Uint64(vec![u64::MAX, 9_007_199_254_741_009, 0, 7]),
        None,
    );
    let receipt = write(&mut engine, &request);
    assert_dense(
        &read(
            &mut engine,
            "nested",
            steps.clone(),
            window(1, 1, 2, 3, &[]),
        ),
        &NumericComponents::Uint64(vec![1, 2, u64::MAX, 9_007_199_254_741_009, 0, 7]),
        None,
    );
    assert_dense(
        &read(
            &mut engine,
            "nested_alias",
            steps.clone(),
            window(1, 1, 2, 3, &[]),
        ),
        &NumericComponents::Uint64(vec![1, 2, 3, 4, 5, 6]),
        None,
    );
    let before_reject = engine.workspace_snapshot().unwrap();
    let mut object_path = request.clone();
    object_path.path.root = binding(&before_reject, "nested");
    object_path.path.steps[2] = cell(1, 2);
    assert!(engine.write_value_range_checked(&object_path).is_err());
    assert_eq!(
        engine.workspace_snapshot().unwrap().context,
        before_reject.context
    );
    let _undone = undo(&mut engine, &receipt);
    assert_dense(
        &read(&mut engine, "nested", steps, window(1, 1, 2, 3, &[])),
        &NumericComponents::Uint64(vec![1, 2, 3, 4, 5, 6]),
        None,
    );
    let final_snapshot = engine.workspace_snapshot().unwrap();
    assert!(
        !final_snapshot
            .variables
            .iter()
            .any(|entry| entry.variable.name == "inspection_trap_called")
    );
    assert_dense(
        &read(&mut engine, "peer", vec![], window(1, 1, 1, 1, &[])),
        &NumericComponents::Double(vec![19.]),
        None,
    );
}

#[test]
fn sparse_rectangle_clears_implicit_zero_positions_and_restores_real_logical_complex_storage() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(
        &mut engine,
        "S=sparse([1,2,4],[1,3,4],[9,8,7],4,4); L=logical(sparse([1,2,4],[1,3,4],[1,1,1],4,4)); C=sparse([1,2,4],[1,3,4],[9+2i,8+3i,7+4i],4,4);",
    );
    let mut double_patch = vec![0.; 9];
    double_patch[5] = 5.;
    let mut logical_patch = vec![false; 9];
    logical_patch[5] = true;
    let mut imaginary_patch = vec![0.; 9];
    imaginary_patch[5] = 6.;
    let cases = [
        (
            "S",
            NumericComponents::Double(double_patch.clone()),
            None,
            NumericComponents::Double(vec![5., 7.]),
            None,
            NumericComponents::Double(vec![9., 8., 7.]),
            None,
        ),
        (
            "L",
            NumericComponents::Logical(logical_patch),
            None,
            NumericComponents::Logical(vec![true, true]),
            None,
            NumericComponents::Logical(vec![true, true, true]),
            None,
        ),
        (
            "C",
            NumericComponents::Double(double_patch),
            Some(NumericComponents::Double(imaginary_patch)),
            NumericComponents::Double(vec![5., 7.]),
            Some(NumericComponents::Double(vec![6., 4.])),
            NumericComponents::Double(vec![9., 8., 7.]),
            Some(NumericComponents::Double(vec![2., 3., 4.])),
        ),
    ];
    for (name, real, imag, after_real, after_imag, original_real, original_imag) in cases {
        let request = edit(
            &mut engine,
            name,
            vec![],
            window(1, 1, 3, 3, &[]),
            &real,
            imag.as_ref(),
        );
        let receipt = write(&mut engine, &request);
        let after = read(&mut engine, name, vec![], window(1, 1, 4, 4, &[]));
        assert_sparse(&after, &[3, 4], &[2, 4], &after_real, after_imag.as_ref());
        let undone = undo(&mut engine, &receipt);
        assert_sparse(
            &read(&mut engine, name, vec![], window(1, 1, 4, 4, &[])),
            &[1, 2, 4],
            &[1, 3, 4],
            &original_real,
            original_imag.as_ref(),
        );
        let _redone = redo(&mut engine, &undone);
        assert_sparse(
            &read(&mut engine, name, vec![], window(1, 1, 4, 4, &[])),
            &[3, 4],
            &[2, 4],
            &after_real,
            after_imag.as_ref(),
        );
    }
}

#[test]
fn global_multi_undo_redo_across_roots_uses_fresh_context_and_exact_head_ids() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(&mut engine, "A=1; B=10;");
    let first_request = scalar_edit(&mut engine, "A", 2.);
    let first = write(&mut engine, &first_request);
    let second_request = scalar_edit(&mut engine, "B", 20.);
    let second = write(&mut engine, &second_request);
    let third_request = scalar_edit(&mut engine, "A", 3.);
    let third = write(&mut engine, &third_request);
    assert!(
        first.transaction_id < second.transaction_id
            && second.transaction_id < third.transaction_id
    );
    assert_eq!(first.history.epoch, third.history.epoch);
    let undo_third = undo(&mut engine, &third);
    assert_eq!(
        undo_third.history.undo.as_ref().unwrap().transaction_id,
        second.transaction_id
    );
    assert_eq!(undo_third.history.undo.as_ref().unwrap().root_name, "B");
    let undo_second = undo(&mut engine, &undo_third);
    assert_eq!(
        undo_second.history.undo.as_ref().unwrap().transaction_id,
        first.transaction_id
    );
    let undo_first = undo(&mut engine, &undo_second);
    assert!(undo_first.history.undo.is_none());
    assert_dense(
        &read(&mut engine, "A", vec![], window(1, 1, 1, 1, &[])),
        &NumericComponents::Double(vec![1.]),
        None,
    );
    assert_dense(
        &read(&mut engine, "B", vec![], window(1, 1, 1, 1, &[])),
        &NumericComponents::Double(vec![10.]),
        None,
    );
    let before_rejections = engine.workspace_snapshot().unwrap();
    let valid_draft = scalar_edit(&mut engine, "A", 9.);
    let mut malformed = valid_draft.clone();
    let EditData::Dense { real, .. } = &mut malformed.data;
    real[0] = "not-ieee-hex".into();
    let mut outside = valid_draft.clone();
    outside.window.rows = 2;
    let mut changed_descriptor = valid_draft;
    changed_descriptor.expected_leaf.source_shape[0] = 2.into();
    for rejected in [malformed, outside, changed_descriptor] {
        assert!(engine.write_value_range_checked(&rejected).is_err());
        assert_eq!(
            engine.workspace_snapshot().unwrap().context,
            before_rejections.context
        );
    }
    // Both client-side malformed/range rejection and genuine-worker descriptor
    // mismatch must preserve the redo chain, which is consumed normally here.
    let redo_first = redo(&mut engine, &undo_first);
    let redo_second = redo(&mut engine, &redo_first);
    let redo_third = redo(&mut engine, &redo_second);
    assert!(redo_third.history.redo.is_none());
    assert_dense(
        &read(&mut engine, "A", vec![], window(1, 1, 1, 1, &[])),
        &NumericComponents::Double(vec![3.]),
        None,
    );
    assert_dense(
        &read(&mut engine, "B", vec![], window(1, 1, 1, 1, &[])),
        &NumericComponents::Double(vec![20.]),
        None,
    );
    let undone = undo(&mut engine, &redo_third);
    let mut obsolete_redo = history_request(&mut engine, &undone, false);
    let branch_request = scalar_edit(&mut engine, "B", 30.);
    let branch = write(&mut engine, &branch_request);
    assert!(branch.history.redo.is_none());
    assert!(branch.transaction_id > third.transaction_id);
    let before = engine.workspace_snapshot().unwrap();
    // A fresh binding prevents this from passing only because of stale context.
    obsolete_redo.expected = binding(&before, "A");
    assert!(engine.redo_value_edit_checked(&obsolete_redo).is_err());
    assert_eq!(engine.workspace_snapshot().unwrap().context, before.context);
}

#[test]
fn retained_history_is_bounded_and_reports_oldest_undo_eviction() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(&mut engine, "A=0;");
    let mut latest = None;
    let mut evicted = 0;
    for value in 1..=40 {
        let request = scalar_edit(&mut engine, "A", f64::from(value));
        let receipt = write(&mut engine, &request);
        evicted += receipt.evicted_undo_entries;
        assert!(receipt.retained_transactions <= 32);
        assert!(receipt.retained_patch_bytes <= 2 * 1024 * 1024);
        latest = Some(receipt);
    }
    let mut current = latest.unwrap();
    assert_eq!(current.retained_transactions, 32);
    assert_eq!(evicted, 8);
    for _ in 0..32 {
        current = undo(&mut engine, &current);
    }
    assert!(current.history.undo.is_none());
    assert_eq!(current.retained_transactions, 32);
    assert_dense(
        &read(&mut engine, "A", vec![], window(1, 1, 1, 1, &[])),
        &NumericComponents::Double(vec![8.]),
        None,
    );
}

#[test]
fn arbitrary_execution_partial_error_and_scalar_editor_invalidate_history_even_with_fresh_binding()
{
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    for (source, expected, fails) in [
        ("A=A;", 2., false),
        (
            "A(1)=9; error('MathLab:EditHistoryTest','intentional');",
            9.,
            true,
        ),
    ] {
        execute(&mut engine, "A=1;");
        let request = scalar_edit(&mut engine, "A", 2.);
        let receipt = write(&mut engine, &request);
        let result = engine.execute(source).unwrap();
        assert_eq!(result.error.is_some(), fails);
        let before = engine.workspace_snapshot().unwrap();
        let stale_history = history_request(&mut engine, &receipt, true);
        assert_eq!(stale_history.expected.context, before.context);
        assert!(engine.undo_value_edit_checked(&stale_history).is_err());
        assert_eq!(engine.workspace_snapshot().unwrap().context, before.context);
        assert_dense(
            &read(&mut engine, "A", vec![], window(1, 1, 1, 1, &[])),
            &NumericComponents::Double(vec![expected]),
            None,
        );
        let new_request = scalar_edit(&mut engine, "A", 3.);
        let next = write(&mut engine, &new_request);
        assert!(next.history.epoch > receipt.history.epoch);
    }
    let request = scalar_edit(&mut engine, "A", 4.);
    let receipt = write(&mut engine, &request);
    let token = binding(&engine.workspace_snapshot().unwrap(), "A");
    engine
        .set_variable_element_checked(&token, 1, 1, "5")
        .unwrap();
    let stale_history = history_request(&mut engine, &receipt, true);
    let before = engine.workspace_snapshot().unwrap();
    assert!(engine.undo_value_edit_checked(&stale_history).is_err());
    assert_eq!(engine.workspace_snapshot().unwrap().context, before.context);
    assert_dense(
        &read(&mut engine, "A", vec![], window(1, 1, 1, 1, &[])),
        &NumericComponents::Double(vec![5.]),
        None,
    );
}

#[test]
fn cross_worker_rebound_and_restart_reject_old_edit_and_history_authority() {
    let Some((mut first, _first_directory)) = engine() else {
        return;
    };
    let Some((mut second, _second_directory)) = engine() else {
        return;
    };
    execute(&mut first, "A=1;");
    execute(&mut second, "A=1;");
    let request = scalar_edit(&mut first, "A", 2.);
    let foreign_before = second.workspace_snapshot().unwrap();
    assert!(second.write_value_range_checked(&request).is_err());
    assert_eq!(
        second.workspace_snapshot().unwrap().context,
        foreign_before.context
    );
    let receipt = write(&mut first, &request);
    let old_history = history_request(&mut first, &receipt, true);
    assert!(second.undo_value_edit_checked(&old_history).is_err());
    assert_eq!(
        second.workspace_snapshot().unwrap().context,
        foreign_before.context
    );
    execute(&mut first, "A=A;");
    assert!(first.write_value_range_checked(&request).is_err());
    assert!(first.undo_value_edit_checked(&old_history).is_err());
    let previous_generation = receipt.token.context.generation;
    first.restart().unwrap();
    first.require_capability(CAP_ATOMIC_RANGE_EDIT).unwrap();
    execute(&mut first, "A=2;");
    let after_restart = first.workspace_snapshot().unwrap();
    assert!(after_restart.context.generation > previous_generation);
    assert!(first.write_value_range_checked(&request).is_err());
    assert!(first.undo_value_edit_checked(&old_history).is_err());
    let mut fresh_binding_old_history = old_history;
    fresh_binding_old_history.expected = binding(&after_restart, "A");
    assert!(
        first
            .undo_value_edit_checked(&fresh_binding_old_history)
            .is_err()
    );
    assert_eq!(
        first.workspace_snapshot().unwrap().context,
        after_restart.context
    );
    assert_dense(
        &read(&mut first, "A", vec![], window(1, 1, 1, 1, &[])),
        &NumericComponents::Double(vec![2.]),
        None,
    );
}

#[test]
fn explicit_history_reset_preserves_values_revokes_revision_and_allows_fresh_edits() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    assert!(!engine.has_value_edit_uncertainty());
    execute(&mut engine, "A=[1,2]; peer=19;");
    let request = edit(
        &mut engine,
        "A",
        vec![],
        window(1, 1, 1, 2, &[]),
        &NumericComponents::Double(vec![11., 12.]),
        None,
    );
    let receipt = write(&mut engine, &request);
    let before = engine.workspace_snapshot().unwrap();
    let mut old_history = history_request(&mut engine, &receipt, true);
    let reset = engine.reset_value_edit_history().unwrap();
    assert_eq!(reset.context.worker_id, before.context.worker_id);
    assert_eq!(reset.context.generation, before.context.generation);
    assert_eq!(reset.context.invocation, before.context.invocation);
    assert_eq!(reset.context.revision, before.context.revision + 1);
    assert!(!engine.has_value_edit_uncertainty());
    assert!(engine.write_value_range_checked(&request).is_err());
    old_history.expected = binding(&reset, "A");
    assert!(engine.undo_value_edit_checked(&old_history).is_err());
    assert_eq!(engine.workspace_snapshot().unwrap().context, reset.context);
    assert_dense(
        &read(&mut engine, "A", vec![], window(1, 1, 1, 2, &[])),
        &NumericComponents::Double(vec![11., 12.]),
        None,
    );
    assert_dense(
        &read(&mut engine, "peer", vec![], window(1, 1, 1, 1, &[])),
        &NumericComponents::Double(vec![19.]),
        None,
    );
    let fresh = edit(
        &mut engine,
        "A",
        vec![],
        window(1, 1, 1, 2, &[]),
        &NumericComponents::Double(vec![21., 22.]),
        None,
    );
    let next = write(&mut engine, &fresh);
    assert!(next.history.epoch > receipt.history.epoch);
    assert_eq!(next.retained_transactions, 1);
    assert!(next.history.redo.is_none());
    let _undone = undo(&mut engine, &next);
    assert_dense(
        &read(&mut engine, "A", vec![], window(1, 1, 1, 2, &[])),
        &NumericComponents::Double(vec![11., 12.]),
        None,
    );
    // This is the public reset contract. A true lost-receipt/uncertain-commit
    // latch test still requires an owner-controlled deterministic fault seam;
    // ordinary Stop of execute() below does not claim to enter a commit window.
}

#[cfg(unix)]
#[test]
fn stop_after_a_causal_output_barrier_invalidates_prior_history_in_the_same_process() {
    use mathlab_engine::ConsoleEventKind;
    use std::{
        thread,
        time::{Duration, Instant},
    };

    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(&mut engine, "A=1;");
    let request = scalar_edit(&mut engine, "A", 2.);
    let receipt = write(&mut engine, &request);
    let process_id = engine.process_id();
    let subscription = engine.subscribe_console();
    let cancellation = engine.cancellation_handle();
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "stop_partial=23; fprintf(stdout,'VALUE_EDIT_STOP_READY'); fflush(stdout); while true; end;",
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
                .windows(b"VALUE_EDIT_STOP_READY".len())
                .any(|window| window == b"VALUE_EDIT_STOP_READY");
        }
    }
    if !ready {
        let _ = cancellation.terminate();
        let _ = task.join();
        panic!("worker did not reach the explicit Stop barrier: {bytes:?}");
    }
    cancellation.interrupt().unwrap();
    let (mut engine, result) = task.join().unwrap();
    let result = result.unwrap();
    assert_eq!(result.error.unwrap().identifier, "MathLab:Interrupted");
    assert_eq!(engine.process_id(), process_id);
    let before = engine.workspace_snapshot().unwrap();
    let obsolete = history_request(&mut engine, &receipt, true);
    assert_eq!(obsolete.expected.context, before.context);
    assert!(engine.undo_value_edit_checked(&obsolete).is_err());
    assert_eq!(engine.workspace_snapshot().unwrap().context, before.context);
    assert_dense(
        &read(&mut engine, "A", vec![], window(1, 1, 1, 1, &[])),
        &NumericComponents::Double(vec![2.]),
        None,
    );
    assert_dense(
        &read(&mut engine, "stop_partial", vec![], window(1, 1, 1, 1, &[])),
        &NumericComponents::Double(vec![23.]),
        None,
    );
    let next_request = scalar_edit(&mut engine, "A", 3.);
    let next = write(&mut engine, &next_request);
    assert!(next.history.epoch > receipt.history.epoch);
}
