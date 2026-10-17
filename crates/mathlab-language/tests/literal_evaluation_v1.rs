// SPDX-License-Identifier: GPL-3.0-or-later
//! All 33 frozen N01 project predictions. Native source observations are not_run.
//! Fixture sha256: 3e888f78fa2db8d7400b49dd0a142af2d86e3fb68948c6c5eb0cd9b6f574c598.
//! These tests exercise the original parser/IR/leaf composition, not Engine admission.

use mathlab_array::{ArrayErrorV1, ErrorCodeV1, ErrorContextV1};
use mathlab_language::SourceSpan;
use mathlab_language::expression::*;
use mathlab_language::literal_evaluation::*;
use std::sync::Arc;

fn diagnostic(source: &str) -> LiteralDiagnosticV1 {
    match evaluate_literal_source_v1(Arc::from(source)).unwrap_err() {
        LiteralExpressionErrorV1::Diagnostic(error) => error,
        host => panic!("unexpected host allocation failure: {host:?}"),
    }
}

#[test]
fn all_fifteen_frozen_values_match_class_shape_and_binary64_bits() {
    let cases: &[(&str, &str, [u64; 2], &[u64])] = &[
        ("S01", "1 + 2", [1, 1], &[0x4008000000000000]),
        ("S02", "1 + 2 + 3", [1, 1], &[0x4018000000000000]),
        (
            "S03",
            "9007199254740992 + 1 + (-9007199254740992)",
            [1, 1],
            &[0x0000000000000000],
        ),
        (
            "S04",
            "9007199254740992 + (1 + (-9007199254740992))",
            [1, 1],
            &[0x3ff0000000000000],
        ),
        (
            "S05",
            "[1,3;2,4]",
            [2, 2],
            &[
                0x3ff0000000000000,
                0x4000000000000000,
                0x4008000000000000,
                0x4010000000000000,
            ],
        ),
        (
            "S06",
            "[1,2,3] + [10;20]",
            [2, 3],
            &[
                0x4026000000000000,
                0x4035000000000000,
                0x4028000000000000,
                0x4036000000000000,
                0x402a000000000000,
                0x4037000000000000,
            ],
        ),
        ("S07", "[] + 1", [0, 0], &[]),
        ("S08", "1 + []", [0, 0], &[]),
        ("S09", "-0", [1, 1], &[0x8000000000000000]),
        ("S10", "-0 + -0", [1, 1], &[0x8000000000000000]),
        ("S11", "-0 + 0", [1, 1], &[0x0000000000000000]),
        (
            "S12",
            "%λ\n(1 + 2) % exact\n",
            [1, 1],
            &[0x4008000000000000],
        ),
        (
            "S13",
            "% c\r\n[1,2]\r\n",
            [1, 2],
            &[0x3ff0000000000000, 0x4000000000000000],
        ),
        ("S14", "+42", [1, 1], &[0x4045000000000000]),
        ("S15", "[ ]", [0, 0], &[]),
    ];
    for (id, source, shape, expected_bits) in cases {
        let source: Arc<str> = Arc::from(*source);
        let parsed = parse_literal_expression_v1(Arc::clone(&source)).unwrap();
        let original_root_span = parsed.root().span.clone();
        let original_nodes = format!("{:?}", parsed.nodes());
        let ir = admit_literal_expression_v1(parsed).unwrap();
        let value =
            evaluate_literal_expression_v1(ir).unwrap_or_else(|error| panic!("{id}: {error:?}"));
        assert_eq!(value.class(), "double", "{id}");
        assert_eq!(value.array().shape(), *shape, "{id}");
        let bits: Vec<_> = value
            .array()
            .as_slice()
            .iter()
            .map(|number| number.to_bits())
            .collect();
        assert_eq!(bits.as_slice(), *expected_bits, "{id}");
        assert!(Arc::ptr_eq(value.source(), &source), "{id}");
        assert_eq!(value.root_span(), &original_root_span, "{id}");
        assert_eq!(
            format!("{:?}", value.parsed().nodes()),
            original_nodes,
            "{id}"
        );
        let (parsed, array) = value.into_parts();
        assert!(Arc::ptr_eq(parsed.source(), &source), "{id}");
        assert_eq!(array.shape(), *shape, "{id}");
    }
}

#[test]
fn all_eighteen_frozen_diagnostics_keep_original_stage_span_and_source() {
    let cases = [
        (
            "D01",
            "1 +",
            LiteralDiagnosticCodeV1::ExpectedExpression,
            LiteralStageV1::Parse,
            [3, 3, 1, 1],
            "",
        ),
        (
            "D02",
            "[1,2;3]",
            LiteralDiagnosticCodeV1::MatrixNotAdmitted,
            LiteralStageV1::MatrixAdmission,
            [0, 7, 1, 1],
            "[1,2;3]",
        ),
        (
            "D03",
            "[1;2] + [3;4;5]",
            LiteralDiagnosticCodeV1::ArrayOperation,
            LiteralStageV1::Evaluation,
            [6, 7, 1, 1],
            "+",
        ),
        (
            "D04",
            "9007199254740993",
            LiteralDiagnosticCodeV1::LiteralNotAdmitted,
            LiteralStageV1::LiteralAdmission,
            [0, 16, 1, 1],
            "9007199254740993",
        ),
        (
            "D05",
            "0.5",
            LiteralDiagnosticCodeV1::UnsupportedToken,
            LiteralStageV1::Lex,
            [1, 2, 1, 1],
            ".",
        ),
        (
            "D06",
            "[1 2]",
            LiteralDiagnosticCodeV1::UnexpectedToken,
            LiteralStageV1::Parse,
            [3, 4, 1, 1],
            "2",
        ),
        (
            "D07",
            "1;",
            LiteralDiagnosticCodeV1::UnexpectedToken,
            LiteralStageV1::Parse,
            [1, 2, 1, 1],
            ";",
        ),
        (
            "D08",
            "1\n2",
            LiteralDiagnosticCodeV1::UnsupportedSourceForm,
            LiteralStageV1::Framing,
            [2, 3, 2, 2],
            "2",
        ),
        (
            "D09",
            "x",
            LiteralDiagnosticCodeV1::UnsupportedIdentifier,
            LiteralStageV1::Lex,
            [0, 1, 1, 1],
            "x",
        ),
        (
            "D10",
            "--1",
            LiteralDiagnosticCodeV1::UnexpectedToken,
            LiteralStageV1::Parse,
            [1, 2, 1, 1],
            "-",
        ),
        (
            "D11",
            "-[1,2]",
            LiteralDiagnosticCodeV1::UnexpectedToken,
            LiteralStageV1::Parse,
            [1, 2, 1, 1],
            "[",
        ),
        (
            "D12",
            "[1,]",
            LiteralDiagnosticCodeV1::UnexpectedToken,
            LiteralStageV1::Parse,
            [3, 4, 1, 1],
            "]",
        ),
        (
            "D13",
            "(1 + 2",
            LiteralDiagnosticCodeV1::UnexpectedEof,
            LiteralStageV1::Parse,
            [6, 6, 1, 1],
            "",
        ),
        (
            "D14",
            "%{\n1\n%}",
            LiteralDiagnosticCodeV1::UnsupportedSourceForm,
            LiteralStageV1::Framing,
            [0, 2, 1, 1],
            "%{",
        ),
        (
            "D15",
            "1\r2",
            LiteralDiagnosticCodeV1::UnsupportedSourceForm,
            LiteralStageV1::Framing,
            [1, 2, 1, 1],
            "\r",
        ),
        (
            "D16",
            "1 + π",
            LiteralDiagnosticCodeV1::UnsupportedToken,
            LiteralStageV1::Lex,
            [4, 6, 1, 1],
            "π",
        ),
        (
            "D17",
            "",
            LiteralDiagnosticCodeV1::ExpectedExpression,
            LiteralStageV1::Framing,
            [0, 0, 1, 1],
            "",
        ),
        (
            "D18",
            "00000000000000000",
            LiteralDiagnosticCodeV1::LiteralNotAdmitted,
            LiteralStageV1::LiteralAdmission,
            [0, 17, 1, 1],
            "00000000000000000",
        ),
    ];
    for (id, source, code, stage, at, slice) in cases {
        let original: Arc<str> = Arc::from(source);
        let error = match evaluate_literal_source_v1(Arc::clone(&original)).unwrap_err() {
            LiteralExpressionErrorV1::Diagnostic(error) => error,
            host => panic!("{id}: {host:?}"),
        };
        assert!(Arc::ptr_eq(&error.source, &original), "{id}");
        assert_eq!(error.code, code, "{id}");
        assert_eq!(error.stage, stage, "{id}");
        assert_eq!(
            error.span,
            SourceSpan {
                start: at[0],
                end: at[1],
                start_line: at[2],
                end_line: at[3]
            },
            "{id}"
        );
        assert_eq!(
            &error.source[error.span.start..error.span.end],
            slice,
            "{id}"
        );
    }
    assert_eq!(
        diagnostic("[1;2] + [3;4;5]").detail,
        LiteralDiagnosticDetailV1::ArrayOperation {
            error: ArrayErrorV1 {
                context: ErrorContextV1::AddCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis: 0,
                    lhs_extent: 2,
                    rhs_extent: 3
                },
            }
        }
    );
}

#[test]
fn complete_semantic_admission_precedes_every_leaf_operation() {
    // The left addition would fail, but every literal and matrix is checked first.
    let error = diagnostic("([1;2] + [3;4;5]) + 9007199254740993");
    assert_eq!(error.code, LiteralDiagnosticCodeV1::LiteralNotAdmitted);
    assert_eq!(
        &error.source[error.span.start..error.span.end],
        "9007199254740993"
    );
    let error = diagnostic("([1;2] + [3;4;5]) + [1,2;3]");
    assert_eq!(error.code, LiteralDiagnosticCodeV1::MatrixNotAdmitted);
    assert_eq!(&error.source[error.span.start..error.span.end], "[1,2;3]");
}

fn assert_shape_error(
    source: &str,
    operator_offset: usize,
    axis: usize,
    lhs_extent: u64,
    rhs_extent: u64,
) {
    let error = diagnostic(source);
    assert_eq!(error.code, LiteralDiagnosticCodeV1::ArrayOperation);
    assert_eq!(error.stage, LiteralStageV1::Evaluation);
    assert_eq!(error.span.start, operator_offset);
    assert_eq!(error.span.end, operator_offset + 1);
    assert_eq!(&error.source[error.span.start..error.span.end], "+");
    assert_eq!(
        error.detail,
        LiteralDiagnosticDetailV1::ArrayOperation {
            error: ArrayErrorV1 {
                context: ErrorContextV1::AddCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis,
                    lhs_extent,
                    rhs_extent
                },
            }
        }
    );
}

#[test]
fn evaluation_reports_the_complete_left_subtree_before_the_right_subtree() {
    let both_fail = "([1;2] + [3;4;5]) + ([1,2] + [3,4,5])";
    assert_shape_error(both_fail, both_fail.find('+').unwrap(), 0, 2, 3);
    let right_fails = "([1;2] + [3;4]) + ([1,2] + [3,4,5])";
    assert_shape_error(right_fails, right_fails.rfind('+').unwrap(), 1, 2, 3);
    let inside_right = "[] + ([1;2] + [3;4;5])";
    assert_shape_error(inside_right, inside_right.rfind('+').unwrap(), 0, 2, 3);
}

#[test]
fn the_leaf_resolves_row_before_column_and_compatibility_before_empty_result() {
    let both_axes_fail = "[1,2;3,4] + [5,6,7;8,9,10;11,12,13]";
    assert_shape_error(both_axes_fail, both_axes_fail.find('+').unwrap(), 0, 2, 3);
    for (source, axis, lhs, rhs) in [
        ("[1;2] + []", 0, 2, 0),
        ("[1,2] + []", 1, 2, 0),
        ("[] + [1,2]", 1, 0, 2),
    ] {
        assert_shape_error(source, source.find('+').unwrap(), axis, lhs, rhs);
    }
    for source in ["[] + []", "([] + (1 + 2))", "((1)) + []"] {
        let value = evaluate_literal_source_v1(Arc::from(source)).unwrap();
        assert_eq!(value.array().shape(), [0, 0]);
        assert!(value.array().as_slice().is_empty());
    }
}

#[test]
fn maximum_admitted_constructor_and_broadcast_outputs_reach_all_payload_bounds() {
    let row = vec!["1"; 16].join(",");
    let column = vec!["1"; 16].join(";");
    let maximum_matrix = format!("[{}]", vec![row.as_str(); 16].join(";"));
    let maximum_broadcast = format!("[{row}] + [{column}]");
    for (source, bits) in [
        (maximum_matrix, 1.0_f64.to_bits()),
        (maximum_broadcast, 2.0_f64.to_bits()),
    ] {
        let value = evaluate_literal_source_v1(Arc::from(source.as_str())).unwrap();
        assert_eq!(value.array().shape(), [16, 16]);
        assert_eq!(value.array().as_slice().len(), 256);
        assert_eq!(
            value.array().as_slice().len() * std::mem::size_of::<f64>(),
            2048
        );
        assert!(
            value
                .array()
                .as_slice()
                .iter()
                .all(|number| number.to_bits() == bits)
        );
    }
}

#[test]
fn grouping_and_signs_remain_in_the_result_ast_with_original_utf8_offsets() {
    let source: Arc<str> = Arc::from("% λ\r\n ( - 0 ) + [ + 2 ] % tail\r\n");
    let value = evaluate_literal_source_v1(Arc::clone(&source)).unwrap();
    assert!(Arc::ptr_eq(value.source(), &source));
    assert_eq!(value.root_span().start_line, 2);
    assert_eq!(value.root_span().end_line, 2);
    assert_eq!(
        &source[value.root_span().start..value.root_span().end],
        "( - 0 ) + [ + 2 ]"
    );
    let nodes = value.parsed().nodes();
    let group = nodes
        .iter()
        .find(|node| matches!(&node.kind, LiteralNodeKindV1::Group { .. }))
        .unwrap();
    assert_eq!(&source[group.span.start..group.span.end], "( - 0 )");
    let signs: Vec<_> = nodes
        .iter()
        .filter_map(|node| match &node.kind {
            LiteralNodeKindV1::Signed { sign_span, .. } => {
                Some(&source[sign_span.start..sign_span.end])
            }
            _ => None,
        })
        .collect();
    assert_eq!(signs, ["-", "+"]);
    assert_eq!(value.array().shape(), [1, 1]);
    assert_eq!(value.array().as_slice()[0].to_bits(), 2.0_f64.to_bits());
}

#[test]
fn source_and_array_remain_independently_owned_after_ir_and_value_are_consumed() {
    let source: Arc<str> = Arc::from("[1,3;2,4] + 1");
    let value = evaluate_literal_source_v1(Arc::clone(&source)).unwrap();
    let (parsed, array) = value.into_parts();
    drop(source);
    assert_eq!(&**parsed.source(), "[1,3;2,4] + 1");
    assert_eq!(array.shape(), [2, 2]);
    let bits: Vec<_> = array
        .as_slice()
        .iter()
        .map(|number| number.to_bits())
        .collect();
    assert_eq!(
        bits,
        [
            2.0_f64.to_bits(),
            3.0_f64.to_bits(),
            4.0_f64.to_bits(),
            5.0_f64.to_bits()
        ]
    );
}
