// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual-worker CP03 checks. Expected fixture values come from the independently
//! authored CP01 oracles, rather than from the typed-page producer or its codec.

use mathlab_engine::{
    BindingToken, DecimalU64, Engine, EngineConfig, Error, NumericClass, NumericComponents,
    PageData, PageWindow, SourceStorage, ValuePage, ValuePageRequest, ValuePath, ValuePathStep,
    WorkspaceSnapshot, discover_octave,
};
use serde::Deserialize;
use std::{fs, path::PathBuf};

fn engine() -> Option<(Engine, tempfile::TempDir)> {
    let Some(executable) = discover_octave() else {
        assert_ne!(
            std::env::var("MATHLAB_REQUIRE_TYPED_PAGE_TESTS").as_deref(),
            Ok("1"),
            "required typed-page integration tests need an installed Octave executable"
        );
        eprintln!("SKIP typed-page integration: no installed Octave executable.");
        return None;
    };
    let directory = tempfile::Builder::new()
        .prefix("mathlab-value-page-test-")
        .tempdir()
        .unwrap();
    let mut engine = Engine::start(EngineConfig {
        executable,
        working_directory: directory.path().to_owned(),
    })
    .expect("installed Octave must start the typed-page worker");
    engine.require_capability("typed_nd_page.v1").unwrap();
    // The original MAT fixture accepts this absolute destination without source
    // changes, even if source execution temporarily enters its own directory.
    let output = directory.path().to_string_lossy().replace('\'', "''");
    execute(
        &mut engine,
        &format!("mathlab_conformance_work_directory='{output}';"),
    );
    Some((engine, directory))
}

fn execute(engine: &mut Engine, source: &str) {
    let result = engine.execute(source).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../compatibility/conformance")
}

fn run_fixture(engine: &mut Engine, case: &str) {
    let result = engine
        .run_file(&fixture_root().join("fixtures").join(format!("{case}.m")))
        .unwrap();
    assert!(result.error.is_none(), "fixture {case}: {:?}", result.error);
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

fn page_request(
    token: BindingToken,
    steps: Vec<ValuePathStep>,
    rows: u32,
    columns: u32,
    higher: &[u64],
) -> ValuePageRequest {
    ValuePageRequest {
        path: ValuePath { root: token, steps },
        window: PageWindow {
            row_start: 1.into(),
            column_start: 1.into(),
            rows,
            columns,
            higher_indices: decimals(higher),
        },
    }
}

fn field_request(
    token: &BindingToken,
    name: &str,
    rows: u32,
    columns: u32,
    higher: &[u64],
) -> ValuePageRequest {
    page_request(token.clone(), vec![field(name)], rows, columns, higher)
}

fn assert_worker_error<T>(result: Result<T, Error>, identifier: &str) {
    match result {
        Err(Error::Worker(error)) => assert_eq!(error.identifier, identifier),
        Err(other) => panic!("expected {identifier}, received {other:?}"),
        Ok(_) => panic!("expected {identifier}, but the request succeeded"),
    }
}

fn dense_real(page: &ValuePage) -> &NumericComponents {
    match &page.data {
        PageData::Dense { real, imag: None } => real,
        other => panic!("expected real dense data, received {other:?}"),
    }
}

#[derive(Deserialize)]
struct OracleFile {
    values: Vec<Oracle>,
}

#[derive(Deserialize)]
struct Oracle {
    name: String,
    class: NumericClass,
    shape: Vec<u64>,
    storage: SourceStorage,
    complex: bool,
    rows: Vec<u32>,
    columns: Vec<u32>,
    real: Vec<String>,
    imag: Vec<String>,
}

fn oracles(case: &str) -> Vec<Oracle> {
    serde_json::from_slice::<OracleFile>(
        &fs::read(fixture_root().join("oracles").join(format!("{case}.json"))).unwrap(),
    )
    .unwrap()
    .values
}

fn oracle(case: &str, name: &str) -> Oracle {
    oracles(case)
        .into_iter()
        .find(|value| value.name == name)
        .unwrap_or_else(|| panic!("oracle {case} did not contain {name}"))
}

fn assert_components(actual: &NumericComponents, expected: &[String]) {
    macro_rules! integers {
        ($values:expr, $ty:ty) => {
            assert_eq!(
                $values,
                &expected
                    .iter()
                    .map(|token| token.parse::<$ty>().unwrap())
                    .collect::<Vec<_>>()
            )
        };
    }
    macro_rules! floats {
        ($values:expr, $ty:ty) => {{
            assert_eq!($values.len(), expected.len());
            for (actual, token) in $values.iter().zip(expected) {
                let wanted = match token.as_str() {
                    "NaN" => <$ty>::NAN,
                    "Inf" => <$ty>::INFINITY,
                    "-Inf" => <$ty>::NEG_INFINITY,
                    _ => token.parse::<$ty>().unwrap(),
                };
                if wanted.is_nan() {
                    assert!(actual.is_nan(), "expected NaN, received {actual}");
                } else {
                    assert_eq!(actual.to_bits(), wanted.to_bits(), "oracle token {token}");
                }
            }
        }};
    }
    match actual {
        NumericComponents::Double(values) => floats!(values, f64),
        NumericComponents::Single(values) => floats!(values, f32),
        NumericComponents::Int8(values) => integers!(values, i8),
        NumericComponents::Uint8(values) => integers!(values, u8),
        NumericComponents::Int16(values) => integers!(values, i16),
        NumericComponents::Uint16(values) => integers!(values, u16),
        NumericComponents::Int32(values) => integers!(values, i32),
        NumericComponents::Uint32(values) => integers!(values, u32),
        NumericComponents::Int64(values) => integers!(values, i64),
        NumericComponents::Uint64(values) => integers!(values, u64),
        NumericComponents::Logical(values) => assert_eq!(
            values,
            &expected
                .iter()
                .map(|token| match token.as_str() {
                    "0" => false,
                    "1" => true,
                    other => panic!("invalid independent logical oracle {other}"),
                })
                .collect::<Vec<_>>()
        ),
    }
}

fn assert_oracle(page: &ValuePage, request: &ValuePageRequest, expected: &Oracle) {
    assert_eq!(page.token, request.path.root);
    assert_eq!(page.path, request.path.steps);
    assert_eq!(page.class, expected.class);
    assert_eq!(page.source_shape, decimals(&expected.shape));
    assert_eq!(page.source_storage, expected.storage);
    assert_eq!(page.complex, expected.complex);
    assert_eq!(page.selection.row_start, request.window.row_start);
    assert_eq!(page.selection.column_start, request.window.column_start);
    assert_eq!(page.selection.higher_indices, request.window.higher_indices);
    let first_row = request.window.row_start.get() - 1;
    let first_column = request.window.column_start.get() - 1;
    let rows = u64::from(request.window.rows).min(expected.shape[0].saturating_sub(first_row));
    let columns =
        u64::from(request.window.columns).min(expected.shape[1].saturating_sub(first_column));
    assert_eq!(u64::from(page.selection.rows), rows);
    assert_eq!(u64::from(page.selection.columns), columns);

    let mut wanted_rows = Vec::new();
    let mut wanted_columns = Vec::new();
    let offsets = match expected.storage {
        SourceStorage::Dense => {
            let mut plane_start = 0;
            let mut stride = expected.shape[0] * expected.shape[1];
            for (axis, index) in request.window.higher_indices.iter().enumerate() {
                plane_start += (index.get() - 1) * stride;
                stride *= expected.shape[axis + 2];
            }
            (0..columns)
                .flat_map(|column| {
                    (0..rows).map(move |row| {
                        (plane_start
                            + first_row
                            + row
                            + (first_column + column) * expected.shape[0])
                            as usize
                    })
                })
                .collect::<Vec<_>>()
        }
        SourceStorage::Sparse => expected
            .rows
            .iter()
            .zip(&expected.columns)
            .enumerate()
            .filter_map(|(offset, (&row, &column))| {
                let row = u64::from(row) - 1;
                let column = u64::from(column) - 1;
                if row >= first_row
                    && row < first_row + rows
                    && column >= first_column
                    && column < first_column + columns
                {
                    wanted_rows.push((row - first_row + 1) as u32);
                    wanted_columns.push((column - first_column + 1) as u32);
                    Some(offset)
                } else {
                    None
                }
            })
            .collect(),
    };
    let wanted_real = offsets
        .iter()
        .map(|&offset| expected.real[offset].clone())
        .collect::<Vec<_>>();
    let wanted_imag = if expected.complex {
        offsets
            .iter()
            .map(|&offset| expected.imag[offset].clone())
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    let (real, imag) = match &page.data {
        PageData::Dense { real, imag } => {
            assert_eq!(expected.storage, SourceStorage::Dense);
            (real, imag)
        }
        PageData::SparseCoo {
            rows,
            columns,
            real,
            imag,
        } => {
            assert_eq!(expected.storage, SourceStorage::Sparse);
            assert_eq!(rows, &wanted_rows);
            assert_eq!(columns, &wanted_columns);
            (real, imag)
        }
    };
    assert_components(real, &wanted_real);
    match (expected.complex, imag) {
        (true, Some(values)) => assert_components(values, &wanted_imag),
        (false, None) => {}
        other => panic!("complex metadata and typed components differ: {other:?}"),
    }
}

#[test]
fn original_nd_fixture_plane_four_and_value_copy_match_independent_oracles() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    run_fixture(&mut engine, "exact_nd_uint64");
    let initial = engine.workspace_snapshot().unwrap();
    let root = binding(&initial, "result");
    for (name, selected) in [("original", 44.0), ("copy", 444.0)] {
        let request = field_request(&root, name, 3, 4, &[4]);
        let page = engine.read_value_page_checked(&request).unwrap();
        assert_oracle(&page, &request, &oracle("exact_nd_uint64", name));
        let NumericComponents::Double(values) = dense_real(&page) else {
            panic!("double fixture lost its class")
        };
        assert_eq!(values[7], selected);
    }
    assert_eq!(
        engine.workspace_snapshot().unwrap().context,
        initial.context
    );
}

#[test]
fn original_integer_fixture_keeps_exact_uint64_and_signed_unsigned_limits() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    run_fixture(&mut engine, "exact_nd_uint64");
    let initial = engine.workspace_snapshot().unwrap();
    let root = binding(&initial, "result");
    for name in ["exact", "unsigned_limits", "signed_limits"] {
        let request = field_request(&root, name, 2, 4, &[]);
        let page = engine.read_value_page_checked(&request).unwrap();
        assert_oracle(&page, &request, &oracle("exact_nd_uint64", name));
        match (name, dense_real(&page)) {
            ("exact", NumericComponents::Uint64(values)) => {
                assert_eq!(values, &[9_007_199_254_741_009]);
            }
            ("unsigned_limits", NumericComponents::Uint64(values)) => {
                assert_eq!(values, &[0, u64::MAX]);
            }
            ("signed_limits", NumericComponents::Int64(values)) => {
                assert_eq!(values, &[i64::MIN, i64::MAX]);
            }
            other => panic!("integer fixture lost its exact type: {other:?}"),
        }
    }
    assert_eq!(
        engine.workspace_snapshot().unwrap().context,
        initial.context
    );
}

#[test]
fn original_complex_single_double_and_dense_logical_values_keep_their_types() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    run_fixture(&mut engine, "complex_sparse");
    let initial = engine.workspace_snapshot().unwrap();
    let root = binding(&initial, "result");
    for name in ["complex_single", "complex_double", "logical"] {
        let request = field_request(&root, name, 3, 4, &[]);
        let page = engine.read_value_page_checked(&request).unwrap();
        assert_oracle(&page, &request, &oracle("complex_sparse", name));
    }
    assert_eq!(
        engine.workspace_snapshot().unwrap().context,
        initial.context
    );
}

#[test]
fn original_sparse_real_complex_logical_windows_use_local_ordered_coordinates() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    run_fixture(&mut engine, "complex_sparse");
    let initial = engine.workspace_snapshot().unwrap();
    let root = binding(&initial, "result");
    for name in ["sparse_real", "sparse_complex", "sparse_logical"] {
        let mut request = field_request(&root, name, 4, 4, &[]);
        request.window.row_start = 2.into();
        request.window.column_start = 2.into();
        let page = engine.read_value_page_checked(&request).unwrap();
        assert_oracle(&page, &request, &oracle("complex_sparse", name));
        let PageData::SparseCoo { rows, columns, .. } = &page.data else {
            panic!("sparse source was densified")
        };
        if name == "sparse_real" {
            assert_eq!(rows, &[2, 1]);
            assert_eq!(columns, &[1, 3]);
        } else {
            assert_eq!(rows, &[2]);
            assert_eq!(columns, &[3]);
        }
    }
    let mut empty = field_request(&root, "sparse_complex", 1, 1, &[]);
    empty.window.column_start = 3.into();
    assert_oracle(
        &engine.read_value_page_checked(&empty).unwrap(),
        &empty,
        &oracle("complex_sparse", "sparse_complex"),
    );
    assert_eq!(
        engine.workspace_snapshot().unwrap().context,
        initial.context
    );
}

#[test]
fn original_mat_roundtrip_payload_and_loaded_values_match_independent_oracles() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    run_fixture(&mut engine, "mat_v7");
    assert!(directory.path().join("conformance-values.mat").is_file());
    let initial = engine.workspace_snapshot().unwrap();
    for root_name in ["payload", "result"] {
        let root = binding(&initial, root_name);
        for expected in oracles("mat_v7") {
            let higher: &[u64] = match expected.name.as_str() {
                "cube" => &[4],
                "empty" => &[2],
                _ => &[],
            };
            let request = field_request(&root, &expected.name, 6, 6, higher);
            assert_oracle(
                &engine.read_value_page_checked(&request).unwrap(),
                &request,
                &expected,
            );
        }
    }
    assert_eq!(
        engine.workspace_snapshot().unwrap().context,
        initial.context
    );
}

#[test]
fn empty_view_axes_keep_shape_while_a_zero_higher_axis_has_no_invented_plane() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    run_fixture(&mut engine, "mat_v7");
    execute(
        &mut engine,
        "empty_columns=zeros(2,0,3); zero_higher=zeros(2,3,0); empty_sparse=sparse([],[],[],0,3);",
    );
    let initial = engine.workspace_snapshot().unwrap();
    let request = field_request(&binding(&initial, "result"), "empty", 3, 4, &[2]);
    assert_oracle(
        &engine.read_value_page_checked(&request).unwrap(),
        &request,
        &oracle("mat_v7", "empty"),
    );
    let request = page_request(binding(&initial, "empty_columns"), vec![], 4, 4, &[3]);
    let page = engine.read_value_page_checked(&request).unwrap();
    assert_eq!(page.source_shape, decimals(&[2, 0, 3]));
    assert_eq!((page.selection.rows, page.selection.columns), (2, 0));
    assert_eq!(dense_real(&page), &NumericComponents::Double(vec![]));
    let request = page_request(binding(&initial, "empty_sparse"), vec![], 4, 4, &[]);
    let page = engine.read_value_page_checked(&request).unwrap();
    assert_eq!(page.source_shape, decimals(&[0, 3]));
    assert_eq!(page.source_storage, SourceStorage::Sparse);
    let PageData::SparseCoo {
        rows,
        columns,
        real,
        imag,
    } = page.data
    else {
        panic!("empty sparse metadata was lost")
    };
    assert!(rows.is_empty() && columns.is_empty() && real.is_empty() && imag.is_none());
    let request = page_request(binding(&initial, "zero_higher"), vec![], 2, 3, &[1]);
    assert_worker_error(
        engine.read_value_page_checked(&request),
        "MathLab:ValuePageUnsupported",
    );
    assert_eq!(
        engine.workspace_snapshot().unwrap().context,
        initial.context
    );
}

#[test]
fn nested_cell_struct_navigation_rejects_objects_without_running_inspection_hooks() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    fs::write(
        directory.path().join("MathLabInspectionTrap.m"),
        include_str!("../../../tests/fixtures/class_support/MathLabInspectionTrap.m"),
    )
    .unwrap();
    execute(
        &mut engine,
        "trap=MathLabInspectionTrap(); leaf=struct(); leaf.values={uint64(9007199254740992)+uint64(17),trap}; nested={leaf}; bounded_struct=struct(); for field_index=1:256; bounded_struct.(sprintf('f%d',field_index))=field_index; end; wide_struct=bounded_struct; wide_struct.extra=257;",
    );
    let initial = engine.workspace_snapshot().unwrap();
    let steps = vec![
        ValuePathStep::Cell {
            indices: decimals(&[1, 1]),
        },
        field("values"),
        ValuePathStep::Cell {
            indices: decimals(&[1, 1]),
        },
    ];
    let request = page_request(binding(&initial, "nested"), steps, 1, 1, &[]);
    let page = engine.read_value_page_checked(&request).unwrap();
    assert_eq!(
        dense_real(&page),
        &NumericComponents::Uint64(vec![9_007_199_254_741_009])
    );
    let boundary = field_request(&binding(&initial, "bounded_struct"), "f256", 1, 1, &[]);
    assert_eq!(
        dense_real(&engine.read_value_page_checked(&boundary).unwrap()),
        &NumericComponents::Double(vec![256.])
    );
    let too_many_fields = field_request(&binding(&initial, "wide_struct"), "f256", 1, 1, &[]);
    assert_worker_error(
        engine.read_value_page_checked(&too_many_fields),
        "MathLab:ValuePageUnsupported",
    );
    let mut object = request.clone();
    object.path.steps[2] = ValuePathStep::Cell {
        indices: decimals(&[1, 2]),
    };
    for rejected in [
        object.clone(),
        {
            let mut property = object;
            property.path.steps.push(field("value"));
            property
        },
        page_request(binding(&initial, "trap"), vec![], 1, 1, &[]),
    ] {
        assert_worker_error(
            engine.read_value_page_checked(&rejected),
            "MathLab:ValuePageUnsupported",
        );
    }
    let after = engine.workspace_snapshot().unwrap();
    assert_eq!(after.context, initial.context);
    assert!(
        !after
            .variables
            .iter()
            .any(|entry| entry.variable.name == "inspection_trap_called")
    );
}

#[test]
fn same_value_rebinding_clear_and_partial_error_revoke_typed_read_tokens() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(&mut engine, "A=[1 2];");
    let old = page_request(
        binding(&engine.workspace_snapshot().unwrap(), "A"),
        vec![],
        1,
        2,
        &[],
    );
    execute(&mut engine, "A=A;");
    assert_worker_error(engine.read_value_page_checked(&old), "MathLab:StaleBinding");
    let next = page_request(
        binding(&engine.workspace_snapshot().unwrap(), "A"),
        vec![],
        1,
        2,
        &[],
    );
    let failure = engine
        .execute("A(1)=77; error('MathLab:TypedPartial','intentional partial assignment');")
        .unwrap();
    assert_eq!(failure.error.unwrap().identifier, "MathLab:TypedPartial");
    assert_worker_error(
        engine.read_value_page_checked(&next),
        "MathLab:StaleBinding",
    );
    let current = page_request(
        binding(&engine.workspace_snapshot().unwrap(), "A"),
        vec![],
        1,
        2,
        &[],
    );
    assert_eq!(
        dense_real(&engine.read_value_page_checked(&current).unwrap()),
        &NumericComponents::Double(vec![77., 2.])
    );
    execute(&mut engine, "clear A;");
    assert_worker_error(
        engine.read_value_page_checked(&current),
        "MathLab:StaleBinding",
    );
}

#[test]
fn identical_values_in_another_worker_or_restart_cannot_reuse_a_root_token() {
    let Some((mut first, _first_directory)) = engine() else {
        return;
    };
    let Some((mut second, _second_directory)) = engine() else {
        return;
    };
    execute(&mut first, "A=42;");
    execute(&mut second, "A=42;");
    let old = page_request(
        binding(&first.workspace_snapshot().unwrap(), "A"),
        vec![],
        1,
        1,
        &[],
    );
    let other = page_request(
        binding(&second.workspace_snapshot().unwrap(), "A"),
        vec![],
        1,
        1,
        &[],
    );
    assert_ne!(
        old.path.root.context.worker_id,
        other.path.root.context.worker_id
    );
    assert_worker_error(second.read_value_page_checked(&old), "MathLab:StaleBinding");
    assert_worker_error(
        first.read_value_page_checked(&other),
        "MathLab:StaleBinding",
    );
    for (engine, request) in [(&mut first, &old), (&mut second, &other)] {
        assert_eq!(
            dense_real(&engine.read_value_page_checked(request).unwrap()),
            &NumericComponents::Double(vec![42.])
        );
    }
    first.restart().unwrap();
    execute(&mut first, "A=42;");
    assert_worker_error(first.read_value_page_checked(&old), "MathLab:StaleBinding");
    let fresh = page_request(
        binding(&first.workspace_snapshot().unwrap(), "A"),
        vec![],
        1,
        1,
        &[],
    );
    assert_ne!(
        fresh.path.root.context.worker_id,
        old.path.root.context.worker_id
    );
    assert!(fresh.path.root.context.generation > old.path.root.context.generation);
    assert_eq!(
        dense_real(&first.read_value_page_checked(&fresh).unwrap()),
        &NumericComponents::Double(vec![42.])
    );
}

#[test]
fn rejected_windows_paths_and_budgets_preserve_the_current_workspace_context() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(&mut engine, "A=reshape(1:24,[2 3 4]);");
    let initial = engine.workspace_snapshot().unwrap();
    let valid = page_request(binding(&initial, "A"), vec![], 2, 3, &[1]);
    let mut rejected = Vec::new();
    for (row, column, rows, columns, higher) in [
        (0, 1, 2, 3, vec![1]),
        (3, 1, 2, 3, vec![1]),
        (1, 4, 2, 3, vec![1]),
        (1, 1, 0, 3, vec![1]),
        (1, 1, 2001, 1, vec![1]),
        (1, 1, 41, 49, vec![1]),
        (1, 1, 2, 3, vec![0]),
        (1, 1, 2, 3, vec![5]),
        (1, 1, 2, 3, vec![]),
        (1, 1, 2, 3, vec![1, 1]),
        (1_u64 << 53, 1, 2, 3, vec![1]),
    ] {
        let mut request = valid.clone();
        request.window.row_start = row.into();
        request.window.column_start = column.into();
        request.window.rows = rows;
        request.window.columns = columns;
        request.window.higher_indices = decimals(&higher);
        rejected.push(request);
    }
    let mut wrong_container = valid.clone();
    wrong_container.path.steps = vec![field("x")];
    rejected.push(wrong_container);
    let mut invalid_identifier = valid.clone();
    invalid_identifier.path.steps = vec![field("x\n")];
    rejected.push(invalid_identifier);
    let mut too_deep = valid.clone();
    too_deep.path.steps = vec![
        ValuePathStep::Cell {
            indices: decimals(&[1, 1])
        };
        17
    ];
    rejected.push(too_deep);
    for request in rejected {
        assert!(
            engine.read_value_page_checked(&request).is_err(),
            "accepted invalid request {request:?}"
        );
        assert_eq!(
            engine.workspace_snapshot().unwrap().context,
            initial.context
        );
        assert_eq!(
            dense_real(&engine.read_value_page_checked(&valid).unwrap()),
            &NumericComponents::Double(vec![1., 2., 3., 4., 5., 6.])
        );
    }
}

#[test]
fn double_single_signed_zero_minimum_subnormal_and_nonfinite_values_round_trip() {
    let Some((mut engine, _directory)) = engine() else {
        return;
    };
    execute(
        &mut engine,
        "D=[typecast(bitshift(uint64(1),63),'double'),typecast(uint64(1),'double'),realmax('double'),NaN,Inf,-Inf]; S=[typecast(bitshift(uint32(1),31),'single'),typecast(uint32(1),'single'),realmax('single'),single(NaN),single(Inf),single(-Inf)];",
    );
    let initial = engine.workspace_snapshot().unwrap();
    for name in ["D", "S"] {
        let request = page_request(binding(&initial, name), vec![], 1, 6, &[]);
        let page = engine.read_value_page_checked(&request).unwrap();
        macro_rules! check {
            ($values:expr, $ty:ty) => {{
                assert_eq!($values.len(), 6);
                assert_eq!($values[0].to_bits(), (-0.0 as $ty).to_bits());
                assert_eq!($values[1].to_bits(), <$ty>::from_bits(1).to_bits());
                assert_eq!($values[2].to_bits(), <$ty>::MAX.to_bits());
                assert!($values[3].is_nan());
                assert_eq!($values[4], <$ty>::INFINITY);
                assert_eq!($values[5], <$ty>::NEG_INFINITY);
            }};
        }
        match (name, dense_real(&page)) {
            ("D", NumericComponents::Double(values)) => check!(values, f64),
            ("S", NumericComponents::Single(values)) => check!(values, f32),
            other => panic!("floating class was lost: {other:?}"),
        }
    }
    assert_eq!(
        engine.workspace_snapshot().unwrap().context,
        initial.context
    );
}

#[test]
fn project_helper_shadowing_cannot_enter_typed_page_resolution_or_publication() {
    let Some((mut engine, directory)) = engine() else {
        return;
    };
    let shadows = directory.path().join("shadowed_helpers");
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
        "sprintf",
        "find",
        "real",
        "imag",
        "jsonencode",
        "__unicode2native__",
    ] {
        fs::write(
            shadows.join(format!("{name}.m")),
            format!("function varargout={name}(varargin)\n builtin('assignin','base','typed_shadow_called','{name}');\n builtin('error','MathLab:TypedShadow','A project helper was invoked.');\nend\n"),
        )
        .unwrap();
    }
    let path = shadows.to_string_lossy().replace('\'', "''");
    execute(
        &mut engine,
        &format!(
            "A=reshape(1:6,[2 3]); Z=sparse([1 2],[1 3],[7 8],2,3); C={{struct('leaf',A)}}; addpath('{path}','-begin');"
        ),
    );
    let initial = engine.workspace_snapshot().unwrap();
    for request in [
        page_request(binding(&initial, "A"), vec![], 2, 3, &[]),
        page_request(
            binding(&initial, "C"),
            vec![
                ValuePathStep::Cell {
                    indices: decimals(&[1, 1]),
                },
                field("leaf"),
            ],
            2,
            3,
            &[],
        ),
    ] {
        assert_eq!(
            dense_real(&engine.read_value_page_checked(&request).unwrap()),
            &NumericComponents::Double(vec![1., 2., 3., 4., 5., 6.])
        );
    }
    let sparse = page_request(binding(&initial, "Z"), vec![], 2, 3, &[]);
    let page = engine.read_value_page_checked(&sparse).unwrap();
    let PageData::SparseCoo {
        rows,
        columns,
        real,
        imag,
    } = page.data
    else {
        panic!("shadowed find forced a sparse fallback")
    };
    assert_eq!(rows, vec![1, 2]);
    assert_eq!(columns, vec![1, 3]);
    assert_eq!(real, NumericComponents::Double(vec![7., 8.]));
    assert!(imag.is_none());
    let after = engine.workspace_snapshot().unwrap();
    assert_eq!(after.context, initial.context);
    assert!(
        !after
            .variables
            .iter()
            .any(|entry| entry.variable.name == "typed_shadow_called")
    );
    // A positive control proves the project function is actually reachable by
    // ordinary user resolution; checked requests must not globally alter paths.
    execute(
        &mut engine,
        "try; find([0 1]); catch caught; if (! builtin('strcmp',caught.identifier,'MathLab:TypedShadow')); builtin('error','MathLab:TypedShadowControl','Wrong control exception.'); endif; end;",
    );
    assert!(
        engine
            .workspace_snapshot()
            .unwrap()
            .variables
            .iter()
            .any(|entry| entry.variable.name == "typed_shadow_called")
    );
}
