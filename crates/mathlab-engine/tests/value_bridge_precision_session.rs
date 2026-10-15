// SPDX-License-Identifier: GPL-3.0-or-later
//! Public Engine precision tests for the optional dense-complex value adapter.
//! No ordinary execute occurs between a journey's write/undo/redo actions.
//! Final bit oracles deliberately run AFTER its last history action.

use mathlab_engine::value_edit::{
    EditOperation, HistoryRequest, LeafDescriptor, RangeEditReceipt, RangeEditRequest,
};
use mathlab_engine::{
    BindingToken, DecimalU64, Engine, EngineConfig, Error, NumericClass, NumericComponents,
    PageData, PageWindow, SourceStorage, ValuePage, ValuePageRequest, ValuePath, ValuePathStep,
    WorkspaceSnapshot, discover_octave,
};
use std::{fs, path::PathBuf, process::Command};

const CAPABILITY: &str = "dense_complex_native_components.v1";
const MISSING_CHILD: &str = "MATHLAB_VALUE_BRIDGE_PRECISION_MISSING_CHILD";
const MISSING_TEST: &str =
    "forced_complex_roots_and_nested_values_keep_state_and_missing_adapter_rejects";
const MISSING_DONE: &str = "VALUE_BRIDGE_PRECISION_MISSING_PROFILE_COMPLETED";

#[derive(Clone, Copy, Debug)]
enum Precision {
    Double,
    Single,
}
const PRECISIONS: [Precision; 2] = [Precision::Double, Precision::Single];

impl Precision {
    fn name(self) -> &'static str {
        match self {
            Self::Double => "double",
            Self::Single => "single",
        }
    }
    fn class(self) -> NumericClass {
        match self {
            Self::Double => NumericClass::Double,
            Self::Single => NumericClass::Single,
        }
    }
    fn integer(self) -> &'static str {
        match self {
            Self::Double => "uint64",
            Self::Single => "uint32",
        }
    }
    fn zero(self) -> u64 {
        0
    }
    fn negative_zero(self) -> u64 {
        match self {
            Self::Double => 0x8000_0000_0000_0000,
            Self::Single => 0x8000_0000,
        }
    }
    fn one(self) -> u64 {
        match self {
            Self::Double => 0x3ff0_0000_0000_0000,
            Self::Single => 0x3f80_0000,
        }
    }
    fn minus_one(self) -> u64 {
        match self {
            Self::Double => 0xbff0_0000_0000_0000,
            Self::Single => 0xbf80_0000,
        }
    }
    fn positive_infinity(self) -> u64 {
        match self {
            Self::Double => 0x7ff0_0000_0000_0000,
            Self::Single => 0x7f80_0000,
        }
    }
    fn negative_infinity(self) -> u64 {
        match self {
            Self::Double => 0xfff0_0000_0000_0000,
            Self::Single => 0xff80_0000,
        }
    }
    fn nan_a(self) -> u64 {
        match self {
            Self::Double => 0x7ff8_0000_0000_0042,
            Self::Single => 0x7fc0_0042,
        }
    }
    fn nan_b(self) -> u64 {
        match self {
            Self::Double => 0xfff8_0000_0000_0099,
            Self::Single => 0xffc0_0099,
        }
    }
    fn components(self, bits: &[u64]) -> NumericComponents {
        match self {
            Self::Double => {
                NumericComponents::Double(bits.iter().copied().map(f64::from_bits).collect())
            }
            Self::Single => NumericComponents::Single(
                bits.iter()
                    .map(|&word| f32::from_bits(u32::try_from(word).unwrap()))
                    .collect(),
            ),
        }
    }
    fn finite(self, values: &[f64]) -> NumericComponents {
        match self {
            Self::Double => NumericComponents::Double(values.to_vec()),
            Self::Single => {
                NumericComponents::Single(values.iter().map(|&value| value as f32).collect())
            }
        }
    }
    // Independent ORACLE transport: decimal uint32 halves, not the production
    // IEEE-hex encoder or an edit request's generated data.
    fn octave_words(self, bits: &[u64]) -> String {
        let numbers = |values: Vec<u64>| {
            values
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        };
        match self {
            Self::Double => format!(
                "builtin('bitor',builtin('bitshift',builtin('uint64',[{}]),32),builtin('uint64',[{}]))",
                numbers(bits.iter().map(|word| word >> 32).collect()),
                numbers(bits.iter().map(|word| word & 0xffff_ffff).collect())
            ),
            Self::Single => format!("builtin('uint32',[{}])", numbers(bits.to_vec())),
        }
    }
}

fn session(native_required: bool) -> Option<(Engine, tempfile::TempDir)> {
    let bridge = std::env::var_os("MATHLAB_VALUE_BRIDGE").map(PathBuf::from);
    if native_required && bridge.is_none() {
        assert_ne!(
            std::env::var("MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS").as_deref(),
            Ok("1"),
            "required precision tests need MATHLAB_VALUE_BRIDGE to a matching .oct"
        );
        eprintln!("SKIP value bridge precision: no MATHLAB_VALUE_BRIDGE configured.");
        return None;
    }
    let Some(executable) = discover_octave() else {
        assert_ne!(
            std::env::var("MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS").as_deref(),
            Ok("1"),
            "required precision tests need an installed Octave"
        );
        eprintln!("SKIP value bridge precision: no installed Octave.");
        return None;
    };
    let directory = tempfile::Builder::new()
        .prefix("mathlab-value-precision-")
        .tempdir()
        .unwrap();
    let mut engine = Engine::start(EngineConfig {
        executable,
        working_directory: directory.path().to_owned(),
    })
    .expect("configured Engine/native adapters must complete startup");
    if native_required {
        let path =
            fs::canonicalize(bridge.unwrap()).expect("configured precision adapter must exist");
        engine.enable_value_bridge(&path).unwrap();
        assert_eq!(engine.value_bridge_path(), Some(path.as_path()));
        engine.require_capability(CAPABILITY).unwrap();
    }
    engine.require_capability("typed_nd_page.v1").unwrap();
    engine.require_capability("atomic_range_edit.v1").unwrap();
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
        .unwrap_or_else(|| panic!("missing test root {name}"))
        .token
        .clone()
}
fn field(name: &str) -> ValuePathStep {
    ValuePathStep::Field {
        indices: vec![1.into(), 1.into()],
        name: name.into(),
    }
}
fn cell() -> ValuePathStep {
    ValuePathStep::Cell {
        indices: vec![1.into(), 1.into()],
    }
}
fn window(row: u64, column: u64, rows: u32, columns: u32, higher: &[u64]) -> PageWindow {
    PageWindow {
        row_start: row.into(),
        column_start: column.into(),
        rows,
        columns,
        higher_indices: higher.iter().copied().map(DecimalU64::new).collect(),
    }
}
fn read(
    engine: &mut Engine,
    name: &str,
    steps: Vec<ValuePathStep>,
    selected: PageWindow,
) -> ValuePage {
    let snapshot = engine.workspace_snapshot().unwrap();
    let request = ValuePageRequest {
        path: ValuePath {
            root: binding(&snapshot, name),
            steps,
        },
        window: selected,
    };
    let page = engine.read_value_page_checked(&request).unwrap();
    assert_eq!(page.token, request.path.root);
    assert_eq!(page.path, request.path.steps);
    assert_eq!(
        engine.workspace_snapshot().unwrap().context,
        snapshot.context,
        "pure precise page revoked context/history"
    );
    page
}
fn components_equal(actual: &NumericComponents, expected: &NumericComponents) {
    macro_rules! floating {
        ($actual:expr,$expected:expr) => {{
            assert_eq!($actual.len(), $expected.len());
            for (actual, expected) in $actual.iter().zip($expected) {
                if expected.is_nan() {
                    // Decimal page wire intentionally carries NaN category only.
                    assert!(actual.is_nan());
                } else {
                    assert_eq!(actual.to_bits(), expected.to_bits());
                }
            }
        }};
    }
    match (actual, expected) {
        (NumericComponents::Double(a), NumericComponents::Double(e)) => floating!(a, e),
        (NumericComponents::Single(a), NumericComponents::Single(e)) => floating!(a, e),
        _ => panic!("wrong primitive component precision"),
    }
}
fn assert_complex(
    page: &ValuePage,
    precision: Precision,
    shape: &[u64],
    real: &NumericComponents,
    imag: &NumericComponents,
) {
    assert_eq!(page.class, precision.class());
    assert!(page.complex, "genuine complex leaf narrowed");
    assert_eq!(page.source_storage, SourceStorage::Dense);
    assert_eq!(
        page.source_shape,
        shape
            .iter()
            .copied()
            .map(DecimalU64::new)
            .collect::<Vec<_>>()
    );
    let PageData::Dense {
        real: actual_real,
        imag: Some(actual_imag),
    } = &page.data
    else {
        panic!("complex dense page lost component pair");
    };
    components_equal(actual_real, real);
    components_equal(actual_imag, imag);
}
fn edit(
    engine: &mut Engine,
    name: &str,
    steps: Vec<ValuePathStep>,
    selected: PageWindow,
    real: &NumericComponents,
    imag: &NumericComponents,
) -> RangeEditRequest {
    let page = read(engine, name, steps, selected.clone());
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
        Some(imag),
    )
    .unwrap()
}
fn assert_receipt(before: &BindingToken, receipt: &RangeEditReceipt, operation: EditOperation) {
    let mut expected = before.clone();
    expected.context.revision += 1;
    assert_eq!(receipt.token, expected);
    assert_eq!(receipt.operation, operation);
    assert!((1..=32).contains(&receipt.retained_transactions));
    assert!((1..=2 * 1024 * 1024).contains(&receipt.retained_patch_bytes));
}
fn history(receipt: &RangeEditReceipt, redo: bool) -> HistoryRequest {
    let head = if redo {
        receipt.history.redo.as_ref()
    } else {
        receipt.history.undo.as_ref()
    }
    .unwrap();
    assert_eq!(head.root_name, receipt.token.name);
    HistoryRequest {
        schema_version: 1,
        expected: receipt.token.clone(),
        history_epoch: receipt.history.epoch,
        transaction_id: head.transaction_id,
    }
}
fn undo(engine: &mut Engine, receipt: &RangeEditReceipt) -> RangeEditReceipt {
    let request = history(receipt, false);
    let result = engine.undo_value_edit_checked(&request).unwrap();
    assert_receipt(&request.expected, &result, EditOperation::Undo);
    assert_eq!(result.transaction_id, receipt.transaction_id);
    assert_eq!(result.history.epoch, receipt.history.epoch);
    assert_eq!(result.retained_patch_bytes, receipt.retained_patch_bytes);
    result
}
fn redo(engine: &mut Engine, receipt: &RangeEditReceipt) -> RangeEditReceipt {
    let request = history(receipt, true);
    let result = engine.redo_value_edit_checked(&request).unwrap();
    assert_receipt(&request.expected, &result, EditOperation::Redo);
    assert_eq!(result.transaction_id, receipt.transaction_id);
    assert_eq!(result.history.epoch, receipt.history.epoch);
    assert_eq!(result.retained_patch_bytes, receipt.retained_patch_bytes);
    result
}

// Only use this before a journey starts, or AFTER its final history action.
// Extract the WHOLE leaf's real/imag arrays before flattening/typecast. The
// expected words are literals/uint32 halves, never production-encoded data.
fn bit_oracle(
    engine: &mut Engine,
    expression: &str,
    precision: Precision,
    shape: &[u64],
    re: &[u64],
    im: &[u64],
) {
    assert_eq!(re.len(), im.len());
    let shape = shape
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    execute(
        engine,
        &format!(
            "precision_value={expression}; if (!builtin('strcmp',builtin('class',precision_value),'{ty}') || builtin('isreal',precision_value) || builtin('ndims',precision_value)!=2 || builtin('any',builtin('size',precision_value)!=[{shape}])) ; builtin('error','MathLab:PrecisionOracle','Wrong class/shape/complex state for {expression}.'); endif; precision_real=builtin('real',precision_value); precision_imag=builtin('imag',precision_value); precision_actual=builtin('typecast',precision_real(:),'{integer}'); precision_expected={real}; if (builtin('numel',precision_actual)!=builtin('numel',precision_expected) || builtin('any',precision_actual(:)!=precision_expected(:))); builtin('error','MathLab:PrecisionOracle','Wrong real component bits for {expression}.'); endif; precision_actual=builtin('typecast',precision_imag(:),'{integer}'); precision_expected={imag}; if (builtin('numel',precision_actual)!=builtin('numel',precision_expected) || builtin('any',precision_actual(:)!=precision_expected(:))); builtin('error','MathLab:PrecisionOracle','Wrong imaginary component bits for {expression}.'); endif;",
            ty = precision.name(),
            integer = precision.integer(),
            real = precision.octave_words(re),
            imag = precision.octave_words(im)
        ),
    );
}

#[test]
fn dense_complex_singleton_all_real_and_nd_plane_pages_preserve_imaginary_negative_zero() {
    let Some((mut engine, _directory)) = session(true) else {
        return;
    };
    for precision in PRECISIONS {
        let ty = precision.name();
        let nz = precision.octave_words(&[precision.negative_zero()]);
        execute(
            &mut engine,
            &format!(
                "precision_nz=builtin('typecast',{nz},'{ty}'); precision_im=builtin('{ty}',[0;0;5]); precision_im(1)=precision_nz; precision_im(2)=precision_nz; R=struct('leaf',builtin('complex',builtin('{ty}',[2;4;6]),precision_im)); precision_nd_im=builtin('zeros',[3 4 5],'{ty}'); precision_nd_im(1)=builtin('{ty}',5); precision_nd_im(:,:,4)=precision_nz; N=builtin('complex',builtin('reshape',builtin('{ty}',1:60),[3 4 5]),precision_nd_im);"
            ),
        );
        let page = read(
            &mut engine,
            "R",
            vec![field("leaf")],
            window(1, 1, 2, 1, &[]),
        );
        assert_complex(
            &page,
            precision,
            &[3, 1],
            &precision.finite(&[2., 4.]),
            &precision.components(&[precision.negative_zero(); 2]),
        );
        let singleton = read(
            &mut engine,
            "R",
            vec![field("leaf")],
            window(1, 1, 1, 1, &[]),
        );
        assert_complex(
            &singleton,
            precision,
            &[3, 1],
            &precision.finite(&[2.]),
            &precision.components(&[precision.negative_zero()]),
        );
        let plane = read(&mut engine, "N", vec![], window(2, 3, 2, 2, &[4]));
        assert_complex(
            &plane,
            precision,
            &[3, 4, 5],
            &precision.finite(&[44., 45., 47., 48.]),
            &precision.components(&[precision.negative_zero(); 4]),
        );
        assert_eq!(plane.selection.higher_indices, vec![4.into()]);
        assert_eq!((plane.selection.rows, plane.selection.columns), (2, 2));
    }
}

struct JourneyBits {
    old_re: Vec<u64>,
    old_im: Vec<u64>,
    rhs_re: Vec<u64>,
    rhs_im: Vec<u64>,
    new_re: Vec<u64>,
    new_im: Vec<u64>,
}
fn journey_bits(p: Precision) -> JourneyBits {
    let z = p.zero();
    let nz = p.negative_zero();
    let one = p.one();
    let minus = p.minus_one();
    let a = p.nan_a();
    let b = p.nan_b();
    let inf = p.positive_infinity();
    let ninf = p.negative_infinity();
    // Selected one-based offsets5,6,8,9 in3x4 include OLD NaN payloads/±0.
    // Complete final arrays are independent literal bitword expectations.
    JourneyBits {
        old_re: vec![a, nz, z, one, a, minus, b, z, nz, inf, ninf, z],
        old_im: vec![nz, b, z, one, z, nz, a, b, minus, nz, inf, ninf],
        rhs_re: vec![nz, b, z, one],
        rhs_im: vec![z, nz, a, minus],
        new_re: vec![a, nz, z, one, nz, b, b, z, one, inf, ninf, z],
        new_im: vec![nz, b, z, one, z, nz, a, a, minus, nz, inf, ninf],
    }
}
fn initialize_journey(engine: &mut Engine, p: Precision, b: &JourneyBits) {
    execute(
        engine,
        &format!(
            "Z=builtin('complex',builtin('reshape',builtin('typecast',{},'{}'),3,4),builtin('reshape',builtin('typecast',{},'{}'),3,4)); Alias=Z;",
            p.octave_words(&b.old_re),
            p.name(),
            p.octave_words(&b.old_im),
            p.name()
        ),
    );
    // Constructor qualification happens before the first numerical edit.
    bit_oracle(engine, "Z", p, &[3, 4], &b.old_re, &b.old_im);
}

#[test]
fn independent_undo_and_redo_journeys_preserve_nan_payloads_signed_zeros_and_aliases() {
    let Some((mut engine, _directory)) = session(true) else {
        return;
    };
    let pid = engine.process_id();
    for precision in PRECISIONS {
        let bits = journey_bits(precision);
        for with_redo in [false, true] {
            // Independent setup AFTER the prior journey's final oracle. Any
            // ordinary eval here deliberately begins a fresh history epoch.
            initialize_journey(&mut engine, precision, &bits);
            let selected = window(2, 2, 2, 2, &[]);
            let request = edit(
                &mut engine,
                "Z",
                vec![],
                selected.clone(),
                &precision.components(&bits.rhs_re),
                &precision.components(&bits.rhs_im),
            );
            let written = engine.write_value_range_checked(&request).unwrap();
            assert_receipt(&request.path.root, &written, EditOperation::Write);
            // Pure page sentinel checks catch a no-op write/undo/redo. They do
            // not claim to compare NaN payloads on the decimal read wire.
            let changed = read(&mut engine, "Z", vec![], selected.clone());
            assert_complex(
                &changed,
                precision,
                &[3, 4],
                &precision.components(&bits.rhs_re),
                &precision.components(&bits.rhs_im),
            );
            let undone = undo(&mut engine, &written);
            let old = read(&mut engine, "Z", vec![], selected.clone());
            assert_complex(
                &old,
                precision,
                &[3, 4],
                &precision.components(&[
                    precision.nan_a(),
                    precision.minus_one(),
                    precision.zero(),
                    precision.negative_zero(),
                ]),
                &precision.components(&[
                    precision.zero(),
                    precision.negative_zero(),
                    precision.nan_b(),
                    precision.minus_one(),
                ]),
            );
            let last = if with_redo {
                let redone = redo(&mut engine, &undone);
                let changed = read(&mut engine, "Z", vec![], selected);
                assert_complex(
                    &changed,
                    precision,
                    &[3, 4],
                    &precision.components(&bits.rhs_re),
                    &precision.components(&bits.rhs_im),
                );
                redone
            } else {
                undone
            };
            assert_eq!(
                engine.workspace_snapshot().unwrap().context,
                last.token.context
            );
            let (expected_re, expected_im) = if with_redo {
                (&bits.new_re, &bits.new_im)
            } else {
                (&bits.old_re, &bits.old_im)
            };
            // LAST history action is complete. Ordinary exact-bit assertions
            // may now revoke history without masking an undo/redo failure.
            bit_oracle(
                &mut engine,
                "Z",
                precision,
                &[3, 4],
                expected_re,
                expected_im,
            );
            bit_oracle(
                &mut engine,
                "Alias",
                precision,
                &[3, 4],
                &bits.old_re,
                &bits.old_im,
            );
            assert_eq!(engine.process_id(), pid);
        }
    }
}

#[test]
fn original_imaginary_negative_zero_prefixes_survive_undo_and_redo_exactly() {
    let Some((mut engine, _directory)) = session(true) else {
        return;
    };
    let pid = engine.process_id();
    for precision in PRECISIONS {
        // Independent literal IEEE words: 2,4,6,5,11,13,-3,7. The original
        // imaginary prefix [-0,-0] is all-real even though the 3x1 leaf is
        // genuinely complex. Ordinary complex singleton/prefix indexing can
        // narrow it and lose precisely the OLD signs needed by Undo.
        let words: [u64; 8] = match precision {
            Precision::Double => [
                0x4000_0000_0000_0000,
                0x4010_0000_0000_0000,
                0x4018_0000_0000_0000,
                0x4014_0000_0000_0000,
                0x4026_0000_0000_0000,
                0x402a_0000_0000_0000,
                0xc008_0000_0000_0000,
                0x401c_0000_0000_0000,
            ],
            Precision::Single => [
                0x4000_0000,
                0x4080_0000,
                0x40c0_0000,
                0x40a0_0000,
                0x4130_0000,
                0x4150_0000,
                0xc040_0000,
                0x40e0_0000,
            ],
        };
        let [two, four, six, five, eleven, thirteen, minus_three, seven] = words;
        let old_re = [two, four, six];
        let old_im = [precision.negative_zero(), precision.negative_zero(), five];
        let rhs_re = [eleven, thirteen];
        let rhs_im = [minus_three, seven];
        for rows in [1_u32, 2] {
            let count = rows as usize;
            let new_re = if rows == 1 {
                [eleven, four, six]
            } else {
                [eleven, thirteen, six]
            };
            let new_im = if rows == 1 {
                [minus_three, precision.negative_zero(), five]
            } else {
                [minus_three, seven, five]
            };
            for with_redo in [false, true] {
                // Fresh fixture/history for each independent write->undo and
                // write->undo->redo journey; no eval between its history actions.
                execute(
                    &mut engine,
                    &format!(
                        "Z=builtin('complex',builtin('reshape',builtin('typecast',{},'{}'),3,1),builtin('reshape',builtin('typecast',{},'{}'),3,1)); Alias=Z; precision_peer=builtin('uint64',19);",
                        precision.octave_words(&old_re),
                        precision.name(),
                        precision.octave_words(&old_im),
                        precision.name()
                    ),
                );
                bit_oracle(&mut engine, "Z", precision, &[3, 1], &old_re, &old_im);
                let request = edit(
                    &mut engine,
                    "Z",
                    vec![],
                    window(1, 1, rows, 1, &[]),
                    &precision.components(&rhs_re[..count]),
                    &precision.components(&rhs_im[..count]),
                );
                let written = engine.write_value_range_checked(&request).unwrap();
                assert_receipt(&request.path.root, &written, EditOperation::Write);
                assert_complex(
                    &read(&mut engine, "Z", vec![], window(1, 1, 3, 1, &[])),
                    precision,
                    &[3, 1],
                    &precision.components(&new_re),
                    &precision.components(&new_im),
                );
                let undone = undo(&mut engine, &written);
                assert_complex(
                    &read(&mut engine, "Z", vec![], window(1, 1, 3, 1, &[])),
                    precision,
                    &[3, 1],
                    &precision.components(&old_re),
                    &precision.components(&old_im),
                );
                let last = if with_redo {
                    let redone = redo(&mut engine, &undone);
                    assert_complex(
                        &read(&mut engine, "Z", vec![], window(1, 1, 3, 1, &[])),
                        precision,
                        &[3, 1],
                        &precision.components(&new_re),
                        &precision.components(&new_im),
                    );
                    redone
                } else {
                    undone
                };
                assert_eq!(
                    engine.workspace_snapshot().unwrap().context,
                    last.token.context,
                    "pure full-leaf checks invalidated the history journey"
                );
                let (expected_re, expected_im) = if with_redo {
                    (&new_re, &new_im)
                } else {
                    (&old_re, &old_im)
                };
                // Only after the LAST action may ordinary typecast-bit oracles
                // inspect Undo or Redo. Full arrays include untouched peers;
                // the independent alias must retain the entire original leaf.
                bit_oracle(
                    &mut engine,
                    "Z",
                    precision,
                    &[3, 1],
                    expected_re,
                    expected_im,
                );
                bit_oracle(&mut engine, "Alias", precision, &[3, 1], &old_re, &old_im);
                execute(
                    &mut engine,
                    "if (precision_peer!=builtin('uint64',19)); builtin('error','MathLab:PrecisionOracle','Prefix history changed independent peer.'); endif;",
                );
                assert_eq!(engine.process_id(), pid);
            }
        }
    }
}

#[test]
fn nested_original_imaginary_negative_zero_prefixes_survive_undo_and_redo_exactly() {
    let Some((mut engine, directory)) = session(true) else {
        return;
    };
    fs::write(
        directory.path().join("MathLabInspectionTrap.m"),
        include_str!("../../../tests/fixtures/class_support/MathLabInspectionTrap.m"),
    )
    .unwrap();
    let pid = engine.process_id();
    let steps = vec![field("cell"), cell(), field("leaf")];
    for precision in PRECISIONS {
        // Independent literal IEEE words for the same original [-0,-0,5]
        // imaginary leaf as the root regression, now behind BOTH struct and
        // cell ancestors. No bit expectation comes from the edit encoder.
        let words: [u64; 8] = match precision {
            Precision::Double => [
                0x4000_0000_0000_0000,
                0x4010_0000_0000_0000,
                0x4018_0000_0000_0000,
                0x4014_0000_0000_0000,
                0x4026_0000_0000_0000,
                0x402a_0000_0000_0000,
                0xc008_0000_0000_0000,
                0x401c_0000_0000_0000,
            ],
            Precision::Single => [
                0x4000_0000,
                0x4080_0000,
                0x40c0_0000,
                0x40a0_0000,
                0x4130_0000,
                0x4150_0000,
                0xc040_0000,
                0x40e0_0000,
            ],
        };
        let [two, four, six, five, eleven, thirteen, minus_three, seven] = words;
        let old_re = [two, four, six];
        let old_im = [precision.negative_zero(), precision.negative_zero(), five];
        let rhs_re = [eleven, thirteen];
        let rhs_im = [minus_three, seven];
        for rows in [1_u32, 2] {
            let count = rows as usize;
            let new_re = if rows == 1 {
                [eleven, four, six]
            } else {
                [eleven, thirteen, six]
            };
            let new_im = if rows == 1 {
                [minus_three, precision.negative_zero(), five]
            } else {
                [minus_three, seven, five]
            };
            for with_redo in [false, true] {
                // Independent Undo-only and Undo->Redo histories. Aliases at
                // every ancestor level must retain the original leaf; opaque
                // siblings must survive without invoking user inspection.
                execute(
                    &mut engine,
                    &format!(
                        "R=builtin('struct'); R.cell=builtin('cell',1,2); R.cell{{1,1}}=builtin('struct'); R.cell{{1,1}}.leaf=builtin('complex',builtin('reshape',builtin('typecast',{},'{}'),3,1),builtin('reshape',builtin('typecast',{},'{}'),3,1)); R.cell{{1,1}}.peer=builtin('uint64',23); R.cell{{1,1}}.opaque=MathLabInspectionTrap(); R.cell{{1,2}}=builtin('uint64',29); R.peer=builtin('uint64',19); R.opaque=MathLabInspectionTrap(); Ralias=R; CellAlias=R.cell; LeafAlias=R.cell{{1,1}}.leaf; precision_peer=builtin('uint64',31);",
                        precision.octave_words(&old_re),
                        precision.name(),
                        precision.octave_words(&old_im),
                        precision.name()
                    ),
                );
                bit_oracle(
                    &mut engine,
                    "R.cell{1,1}.leaf",
                    precision,
                    &[3, 1],
                    &old_re,
                    &old_im,
                );
                let request = edit(
                    &mut engine,
                    "R",
                    steps.clone(),
                    window(1, 1, rows, 1, &[]),
                    &precision.components(&rhs_re[..count]),
                    &precision.components(&rhs_im[..count]),
                );
                let written = engine.write_value_range_checked(&request).unwrap();
                assert_receipt(&request.path.root, &written, EditOperation::Write);
                assert_complex(
                    &read(&mut engine, "R", steps.clone(), window(1, 1, 3, 1, &[])),
                    precision,
                    &[3, 1],
                    &precision.components(&new_re),
                    &precision.components(&new_im),
                );
                let undone = undo(&mut engine, &written);
                assert_complex(
                    &read(&mut engine, "R", steps.clone(), window(1, 1, 3, 1, &[])),
                    precision,
                    &[3, 1],
                    &precision.components(&old_re),
                    &precision.components(&old_im),
                );
                let last = if with_redo {
                    let redone = redo(&mut engine, &undone);
                    assert_complex(
                        &read(&mut engine, "R", steps.clone(), window(1, 1, 3, 1, &[])),
                        precision,
                        &[3, 1],
                        &precision.components(&new_re),
                        &precision.components(&new_im),
                    );
                    redone
                } else {
                    undone
                };
                assert_eq!(
                    engine.workspace_snapshot().unwrap().context,
                    last.token.context,
                    "pure nested full-leaf checks invalidated the history journey"
                );
                let (expected_re, expected_im) = if with_redo {
                    (&new_re, &new_im)
                } else {
                    (&old_re, &old_im)
                };
                // The LAST history action is complete. Whole-leaf bit oracles
                // independently check the restored signs and untouched rows.
                bit_oracle(
                    &mut engine,
                    "R.cell{1,1}.leaf",
                    precision,
                    &[3, 1],
                    expected_re,
                    expected_im,
                );
                for alias in ["Ralias.cell{1,1}.leaf", "CellAlias{1,1}.leaf", "LeafAlias"] {
                    bit_oracle(&mut engine, alias, precision, &[3, 1], &old_re, &old_im);
                }
                execute(
                    &mut engine,
                    "if (R.peer!=builtin('uint64',19) || Ralias.peer!=builtin('uint64',19) || R.cell{1,1}.peer!=builtin('uint64',23) || Ralias.cell{1,1}.peer!=builtin('uint64',23) || CellAlias{1,1}.peer!=builtin('uint64',23) || R.cell{1,2}!=builtin('uint64',29) || Ralias.cell{1,2}!=builtin('uint64',29) || CellAlias{1,2}!=builtin('uint64',29) || precision_peer!=builtin('uint64',31)); builtin('error','MathLab:PrecisionOracle','Nested prefix history changed a peer.'); endif; if (!builtin('strcmp',builtin('class',R.opaque),'MathLabInspectionTrap') || !builtin('strcmp',builtin('class',R.cell{1,1}.opaque),'MathLabInspectionTrap') || !builtin('strcmp',builtin('class',Ralias.opaque),'MathLabInspectionTrap') || !builtin('strcmp',builtin('class',Ralias.cell{1,1}.opaque),'MathLabInspectionTrap') || !builtin('strcmp',builtin('class',CellAlias{1,1}.opaque),'MathLabInspectionTrap')); builtin('error','MathLab:PrecisionOracle','Nested prefix history lost an opaque sibling.'); endif; if (builtin('exist','inspection_trap_called','var')!=0); builtin('error','MathLab:PrecisionOracle','Nested prefix history invoked an opaque sibling.'); endif;",
                );
                assert_eq!(engine.process_id(), pid);
            }
        }
    }
}

fn assert_worker_error<T>(result: Result<T, Error>, identifier: &str) {
    match result {
        Err(Error::Worker(error)) => assert_eq!(error.identifier, identifier, "{}", error.message),
        Err(other) => panic!("expected {identifier}, got {other:?}"),
        Ok(_) => panic!("expected {identifier}, accepted request"),
    }
}
fn missing_adapter_profile() {
    assert!(std::env::var_os("MATHLAB_VALUE_BRIDGE").is_none());
    let Some((mut engine, _directory)) = session(false) else {
        return;
    };
    assert!(engine.value_bridge_path().is_none());
    assert!(!engine.capabilities().supports(CAPABILITY));
    for precision in PRECISIONS {
        let ty = precision.name();
        let nz = precision.octave_words(&[precision.negative_zero()]);
        execute(
            &mut engine,
            &format!(
                "precision_nz=builtin('typecast',{nz},'{ty}'); precision_im=builtin('{ty}',[0;5]); precision_im(1)=precision_nz; Z=builtin('complex',builtin('{ty}',[2;4]),precision_im); precision_peer=builtin('uint64',19);"
            ),
        );
        let snapshot = engine.workspace_snapshot().unwrap();
        let token = binding(&snapshot, "Z");
        let selected = window(1, 1, 1, 1, &[]);
        let path = ValuePath {
            root: token.clone(),
            steps: vec![],
        };
        assert_worker_error(
            engine.read_value_page_checked(&ValuePageRequest {
                path: path.clone(),
                window: selected.clone(),
            }),
            "MathLab:ValuePagePrecisionUnavailable",
        );
        // Construct from known fixture metadata: the unavailable complex read
        // must not be used to bootstrap this negative edit test.
        let request = RangeEditRequest::from_components(
            path,
            selected,
            LeafDescriptor {
                class: precision.class(),
                source_shape: vec![2.into(), 1.into()],
                source_storage: SourceStorage::Dense,
                complex: true,
            },
            &precision.components(&[precision.negative_zero()]),
            Some(&precision.components(&[precision.negative_zero()])),
        )
        .unwrap();
        assert_worker_error(
            engine.write_value_range_checked(&request),
            "MathLab:ValueEditPrecisionUnavailable",
        );
        assert!(!engine.has_value_edit_uncertainty());
        let after = engine.workspace_snapshot().unwrap();
        assert_eq!(after.context, snapshot.context);
        assert_eq!(binding(&after, "Z"), token);
        // Only AFTER unchanged-token checks may ordinary evaluation inspect
        // base values. Finite values and -0 must remain exactly as initialized.
        execute(
            &mut engine,
            &format!(
                "precision_actual_re=builtin('real',Z); precision_actual_im=builtin('imag',Z); precision_expected_re=builtin('{ty}',[2;4]); precision_expected_im=builtin('{ty}',[0;5]); precision_expected_im(1)=precision_nz; precision_a=builtin('typecast',precision_actual_re(:),'{}'); precision_b=builtin('typecast',precision_expected_re(:),'{}'); if (builtin('any',precision_a(:)!=precision_b(:))); builtin('error','MathLab:PrecisionOracle','Missing-adapter rejection changed real values.'); endif; precision_a=builtin('typecast',precision_actual_im(:),'{}'); precision_b=builtin('typecast',precision_expected_im(:),'{}'); if (builtin('any',precision_a(:)!=precision_b(:))); builtin('error','MathLab:PrecisionOracle','Missing-adapter rejection changed imaginary values.'); endif; if (precision_peer!=builtin('uint64',19)); builtin('error','MathLab:PrecisionOracle','Missing-adapter rejection changed peer.'); endif;",
                precision.integer(),
                precision.integer(),
                precision.integer(),
                precision.integer()
            ),
        );
    }
    // Parent verifies this exact marker, so --exact matching zero tests cannot
    // be mistaken for successful execution of the negative profile.
    println!("{MISSING_DONE}");
}

#[test]
fn forced_complex_roots_and_nested_values_keep_state_and_missing_adapter_rejects() {
    if std::env::var(MISSING_CHILD).as_deref() == Ok("1") {
        missing_adapter_profile();
        return;
    }
    let child = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg(MISSING_TEST)
        .arg("--nocapture")
        .env(MISSING_CHILD, "1")
        .env_remove("MATHLAB_VALUE_BRIDGE")
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "missing-adapter profile failed: stdout={} stderr={}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    if discover_octave().is_some() {
        assert!(
            String::from_utf8_lossy(&child.stdout).contains(MISSING_DONE),
            "missing-adapter child did not run its profile: {}",
            String::from_utf8_lossy(&child.stdout)
        );
    }
    let Some((mut engine, _directory)) = session(true) else {
        return;
    };
    for precision in PRECISIONS {
        let ty = precision.name();
        let nz = precision.negative_zero();
        let z = precision.zero();
        execute(
            &mut engine,
            &format!(
                "Z=builtin('complex',builtin('{ty}',[2;4]),builtin('{ty}',[5;7])); Alias=Z; S=builtin('complex',builtin('{ty}',2),builtin('{ty}',5)); R=struct(); R.branch={{struct('leaf',builtin('complex',builtin('{ty}',[3;6]),builtin('{ty}',[8;9])))}}; R.peer=builtin('uint64',19); Ralias=R;"
            ),
        );
        let real = precision.components(&[nz, z]);
        let imag = precision.components(&[nz, z]);
        let full = edit(
            &mut engine,
            "Z",
            vec![],
            window(1, 1, 2, 1, &[]),
            &real,
            &imag,
        );
        let receipt = engine.write_value_range_checked(&full).unwrap();
        assert_receipt(&full.path.root, &receipt, EditOperation::Write);
        assert_complex(
            &read(&mut engine, "Z", vec![], window(1, 1, 2, 1, &[])),
            precision,
            &[2, 1],
            &real,
            &imag,
        );
        let scalar_re = precision.components(&[nz]);
        let scalar_im = precision.components(&[nz]);
        let scalar = edit(
            &mut engine,
            "S",
            vec![],
            window(1, 1, 1, 1, &[]),
            &scalar_re,
            &scalar_im,
        );
        let receipt = engine.write_value_range_checked(&scalar).unwrap();
        assert_receipt(&scalar.path.root, &receipt, EditOperation::Write);
        assert_complex(
            &read(&mut engine, "S", vec![], window(1, 1, 1, 1, &[])),
            precision,
            &[1, 1],
            &scalar_re,
            &scalar_im,
        );
        let steps = vec![field("branch"), cell(), field("leaf")];
        let nested = edit(
            &mut engine,
            "R",
            steps.clone(),
            window(1, 1, 2, 1, &[]),
            &real,
            &imag,
        );
        let written = engine.write_value_range_checked(&nested).unwrap();
        assert_receipt(&nested.path.root, &written, EditOperation::Write);
        assert_complex(
            &read(&mut engine, "R", steps.clone(), window(1, 1, 2, 1, &[])),
            precision,
            &[2, 1],
            &real,
            &imag,
        );
        let undone = undo(&mut engine, &written);
        let redone = redo(&mut engine, &undone);
        assert_eq!(
            engine.workspace_snapshot().unwrap().context,
            redone.token.context
        );
        assert_complex(
            &read(&mut engine, "R", steps.clone(), window(1, 1, 2, 1, &[])),
            precision,
            &[2, 1],
            &real,
            &imag,
        );
        assert_complex(
            &read(&mut engine, "Ralias", steps, window(1, 1, 2, 1, &[])),
            precision,
            &[2, 1],
            &precision.finite(&[3., 6.]),
            &precision.finite(&[8., 9.]),
        );
        assert_complex(
            &read(&mut engine, "Alias", vec![], window(1, 1, 2, 1, &[])),
            precision,
            &[2, 1],
            &precision.finite(&[2., 4.]),
            &precision.finite(&[5., 7.]),
        );
        // Final history action complete: base assignment/container extraction
        // must retain genuine complex state and exact all-zero component bits.
        bit_oracle(&mut engine, "Z", precision, &[2, 1], &[nz, z], &[nz, z]);
        bit_oracle(&mut engine, "S", precision, &[1, 1], &[nz], &[nz]);
        bit_oracle(
            &mut engine,
            "R.branch{1,1}.leaf",
            precision,
            &[2, 1],
            &[nz, z],
            &[nz, z],
        );
        execute(
            &mut engine,
            "if (R.peer!=builtin('uint64',19) || Ralias.peer!=builtin('uint64',19)); builtin('error','MathLab:PrecisionOracle','Nested replacement changed peer.'); endif;",
        );
    }
}
