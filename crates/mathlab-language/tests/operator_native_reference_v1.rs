// SPDX-License-Identifier: GPL-3.0-or-later
//! Selected capture-backed comparisons for the bounded real-double profile.
//! The fixture provenance retains six prediction matches, three observer
//! controls and six unasserted observations. These tests do not change those
//! historical verdicts, admit the controls, or promise general MATLAB parity.

#[path = "fixtures/operator_native_reference_v1.rs"]
mod native;

use mathlab_array::{ArrayErrorV1, ErrorCodeV1, ErrorContextV1, MtimesWorkBudgetV1};
use mathlab_language::arithmetic_expression::{
    ArithmeticExpressionErrorV1, evaluate_arithmetic_source_v1,
};
use mathlab_language::decimal_expression::{DecimalDiagnosticDetailV1, DecimalExpressionErrorV1};
use mathlab_language::expression::{LiteralDiagnosticDetailV1, LiteralStageV1};
use native::NativeExpectedV1;
use std::sync::Arc;

#[test]
fn accepted_operator_records_compare_class_shape_and_selected_payload_policy() {
    for case in native::CASES {
        if matches!(case.expected, NativeExpectedV1::ShapeError { .. }) {
            continue;
        }
        let source: Arc<str> = Arc::from(case.source);
        let value =
            evaluate_arithmetic_source_v1(Arc::clone(&source), MtimesWorkBudgetV1 { max_terms: 0 })
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.id));
        assert!(Arc::ptr_eq(&source, value.source()), "{}", case.id);
        assert!(Arc::ptr_eq(&source, value.ast().source()), "{}", case.id);
        match case.expected {
            NativeExpectedV1::Exact {
                class,
                shape,
                words,
            } => {
                assert_eq!(value.class(), class, "{}", case.id);
                assert_eq!(value.array().shape(), shape, "{}", case.id);
                let actual: Vec<_> = value
                    .array()
                    .as_slice()
                    .iter()
                    .map(|v| v.to_bits())
                    .collect();
                assert_eq!(actual.as_slice(), words, "{}", case.id);
            }
            NativeExpectedV1::Nan {
                class,
                shape,
                count,
            } => {
                assert_eq!(value.class(), class, "{}", case.id);
                assert_eq!(value.array().shape(), shape, "{}", case.id);
                assert_eq!(value.array().as_slice().len(), count, "{}", case.id);
                assert!(
                    value.array().as_slice().iter().all(|v| v.is_nan()),
                    "{}",
                    case.id
                );
            }
            NativeExpectedV1::ShapeError { .. } => unreachable!(),
        }
    }
}

#[test]
fn native_shape_failure_keeps_project_diagnostic_and_original_utf8_span() {
    let case = native::case("O04");
    let NativeExpectedV1::ShapeError {
        native_identifier,
        native_line,
    } = case.expected
    else {
        panic!("O04 must retain its observed native failure metadata");
    };
    // The native record supplies this identifier and frame line, but no column
    // or byte-span oracle. The Rust category and bytes below are project policy.
    assert_eq!(native_identifier, "MATLAB:sizeDimensionsMustMatch");
    let source: Arc<str> = Arc::from(case.source);
    let failure =
        evaluate_arithmetic_source_v1(Arc::clone(&source), MtimesWorkBudgetV1 { max_terms: 0 })
            .unwrap_err();
    assert!(Arc::ptr_eq(&source, failure.original_source()));
    assert_eq!(failure.stage(), LiteralStageV1::Evaluation);
    assert_eq!(failure.code(), Some("ArrayOperation"));
    assert_eq!((failure.span().start, failure.span().end), (10, 12));
    assert_eq!(failure.span().start_line, native_line);
    assert_eq!(&source[failure.span().start..failure.span().end], "./");
    assert!(matches!(
        failure,
        ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(
            LiteralDiagnosticDetailV1::ArrayOperation {
                error: ArrayErrorV1 {
                    context: ErrorContextV1::RDivideCompatibility,
                    code: ErrorCodeV1::IncompatibleShape {
                        axis: 0, lhs_extent: 2, rhs_extent: 3,
                    },
                },
            }
        ))
    ));
}
