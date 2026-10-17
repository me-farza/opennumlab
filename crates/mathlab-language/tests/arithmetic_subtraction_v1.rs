// SPDX-License-Identifier: GPL-3.0-or-later
//! Original hand-calculated subtraction cases; no native subtraction source
//! oracle, diagnostic-ID equivalence or broad IEEE/performance parity claimed.

use mathlab_array::{ArrayErrorV1, ErrorCodeV1, ErrorContextV1, MtimesWorkBudgetV1};
use mathlab_language::arithmetic_expression::*;
use mathlab_language::decimal_expression::{
    DecimalDiagnosticDetailV1, DecimalExpressionErrorV1, evaluate_decimal_source_v1,
};
use mathlab_language::expression::{
    LiteralDiagnosticDetailV1, LiteralExpressionErrorV1, LiteralResourceV1, LiteralStageV1,
};
use mathlab_language::literal_evaluation::evaluate_literal_source_v1;
use std::sync::Arc;

fn budget(max_terms: u64) -> MtimesWorkBudgetV1 {
    MtimesWorkBudgetV1 { max_terms }
}
fn error(source: &str) -> ArithmeticExpressionErrorV1 {
    evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap_err()
}

#[test]
fn original_noncommutativity_associativity_prefixes_and_precedence_have_fixed_values() {
    for (source, expected) in [
        ("10-3", 7.0),
        ("3-10", -7.0),
        ("10-3-2", 5.0),
        ("10-(3-2)", 9.0),
        ("20-5+2-3", 14.0),
        ("2+3-4", 1.0),
        ("20-2*3", 14.0),
        ("(20-2)*3", 54.0),
        ("-2-3*4", -14.0),
        ("2*-3-4", -10.0),
        ("1--2", 3.0),
        ("1-+-2", 3.0),
        ("--1--2", 3.0),
        ("-(5-2)", -3.0),
        ("2-(-3)", 5.0),
    ] {
        let source: Arc<str> = Arc::from(source);
        let value = evaluate_arithmetic_source_v1(Arc::clone(&source), budget(1)).unwrap();
        assert!(Arc::ptr_eq(&source, value.source()));
        assert_eq!(value.class(), "double");
        assert_eq!(value.array().shape(), [1, 1]);
        assert_eq!(value.array().as_slice(), &[expected], "{source}");
    }
}

#[test]
fn original_matrix_and_singleton_broadcasting_keep_column_major_order() {
    let cases: &[(&str, [u64; 2], &[f64])] = &[
        ("[1,2;3,4]-[4,3;2,1]", [2, 2], &[-3.0, 1.0, -1.0, 3.0]),
        ("[4,3;2,1]-[1,2;3,4]", [2, 2], &[3.0, -1.0, 1.0, -3.0]),
        (
            "[10,20,30]-[1;2]",
            [2, 3],
            &[9.0, 8.0, 19.0, 18.0, 29.0, 28.0],
        ),
        (
            "[1;2]-[10,20,30]",
            [2, 3],
            &[-9.0, -8.0, -19.0, -18.0, -29.0, -28.0],
        ),
        ("5-[1,2;3,4]", [2, 2], &[4.0, 2.0, 3.0, 1.0]),
        ("[1,2;3,4]-5", [2, 2], &[-4.0, -2.0, -3.0, -1.0]),
        ("-[1,2]- -[3,4]", [1, 2], &[2.0, 2.0]),
    ];
    for (source, shape, expected) in cases {
        let value = evaluate_arithmetic_source_v1(Arc::from(*source), budget(0)).unwrap();
        assert_eq!(value.array().shape(), *shape, "{source}");
        assert_eq!(value.array().as_slice(), *expected, "{source}");
    }
}

#[test]
fn original_signed_zero_subtraction_expectations_compare_words() {
    for (source, expected) in [
        ("0-0", 0x0000000000000000_u64),
        ("0--0", 0x0000000000000000),
        ("-0-0", 0x8000000000000000),
        ("-0--0", 0x0000000000000000),
        ("-(0-0)", 0x8000000000000000),
    ] {
        let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap();
        assert_eq!(value.array().as_slice()[0].to_bits(), expected, "{source}");
    }
    let value = evaluate_arithmetic_source_v1(Arc::from("[0,-0]-[0,0]"), budget(0)).unwrap();
    let words: Vec<_> = value
        .array()
        .as_slice()
        .iter()
        .map(|v| v.to_bits())
        .collect();
    assert_eq!(words, vec![0x0000000000000000_u64, 0x8000000000000000]);
}

#[test]
fn subtraction_ast_preserves_actual_operators_left_fold_and_product_children() {
    let parsed = parse_arithmetic_expression_v1(Arc::from("10-3-2")).unwrap();
    let ast = parsed.ast();
    let ArithmeticNodeKindV1::Binary {
        operator: ArithmeticOperatorV1::Subtract,
        operator_span,
        left,
        ..
    } = ast.root().kind
    else {
        panic!("expected subtraction root");
    };
    assert_eq!(operator_span.start, 4);
    let ArithmeticNodeKindV1::Binary {
        operator: ArithmeticOperatorV1::Subtract,
        operator_span,
        ..
    } = ast.node(left).unwrap().kind
    else {
        panic!("expected left fold");
    };
    assert_eq!(operator_span.start, 2);
    let parsed = parse_arithmetic_expression_v1(Arc::from("2+3*4-5")).unwrap();
    let ast = parsed.ast();
    let ArithmeticNodeKindV1::Binary {
        operator: ArithmeticOperatorV1::Subtract,
        left,
        ..
    } = ast.root().kind
    else {
        panic!("expected subtraction root");
    };
    let ArithmeticNodeKindV1::Binary {
        operator: ArithmeticOperatorV1::Add,
        right,
        ..
    } = ast.node(left).unwrap().kind
    else {
        panic!("expected Add on left");
    };
    assert!(matches!(
        ast.node(right).unwrap().kind,
        ArithmeticNodeKindV1::Binary {
            operator: ArithmeticOperatorV1::Multiply,
            ..
        }
    ));
}

#[test]
fn subtraction_error_retains_original_utf8_crlf_source_span_and_leaf_metadata() {
    let source: Arc<str> = Arc::from("%λ\r\n[1;2]-[3;4;5]");
    let failure = evaluate_arithmetic_source_v1(Arc::clone(&source), budget(0)).unwrap_err();
    assert!(Arc::ptr_eq(&source, failure.original_source()));
    assert_eq!(failure.code(), Some("ArrayOperation"));
    assert_eq!(failure.stage(), LiteralStageV1::Evaluation);
    assert_eq!(
        (
            failure.span().start,
            failure.span().end,
            failure.span().start_line
        ),
        (10, 11, 2)
    );
    assert_eq!(&source[failure.span().start..failure.span().end], "-");
    assert!(
        matches!(failure, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::ArrayOperation {
            error: ArrayErrorV1 { context: ErrorContextV1::SubCompatibility,
                code: ErrorCodeV1::IncompatibleShape { axis: 0, lhs_extent: 2, rhs_extent: 3 } }
        })))
    );
}

#[test]
fn complete_admission_and_left_first_evaluation_precede_pending_subtractions() {
    assert_eq!(
        error("([1;2]-[3;4;5])+[1,2;3]").code(),
        Some("MatrixNotAdmitted")
    );
    let source = "([1;2]-[3;4;5])-([1,2]*[3,4])";
    let failure = error(source);
    assert_eq!(failure.span().start, source.find('-').unwrap());
    assert!(
        matches!(failure, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::ArrayOperation {
            error: ArrayErrorV1 { context: ErrorContextV1::SubCompatibility, .. }
        })))
    );
    let source = "([1;2]+[3;4;5])-([1,2]-[3,4,5])";
    let failure = error(source);
    assert_eq!(failure.span().start, source.find('+').unwrap());
    assert!(
        matches!(failure, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::ArrayOperation {
            error: ArrayErrorV1 { context: ErrorContextV1::AddCompatibility, .. }
        })))
    );
    assert_eq!(error("1-1 π").stage(), LiteralStageV1::Lex);
}

#[test]
fn empty_subtraction_resolves_both_axes_and_does_not_use_mtimes_budget() {
    for source in ["[]-1", "1-[]", "[]-[]", "-[]-+[]"] {
        let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap();
        assert_eq!(value.array().shape(), [0, 0]);
        assert!(value.array().as_slice().is_empty());
    }
    assert!(
        matches!(error("[1,2]-[]"), ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::ArrayOperation {
            error: ArrayErrorV1 { context: ErrorContextV1::SubCompatibility,
                code: ErrorCodeV1::IncompatibleShape { axis: 1, lhs_extent: 2, rhs_extent: 0 } }
        })))
    );
}

#[test]
fn original_literal_and_decimal_facades_still_reject_binary_subtraction() {
    for source in ["1-2", "1--2", "[1,2]-1"] {
        let LiteralExpressionErrorV1::Diagnostic(old) =
            evaluate_literal_source_v1(Arc::from(source)).unwrap_err()
        else {
            panic!("expected literal rejection");
        };
        assert_eq!(old.code.as_str(), "UnexpectedToken");
        assert_eq!(old.stage, LiteralStageV1::Parse);
        let DecimalExpressionErrorV1::Diagnostic(old) =
            evaluate_decimal_source_v1(Arc::from(source)).unwrap_err()
        else {
            panic!("expected decimal rejection");
        };
        assert_eq!(old.code.as_str(), "UnexpectedToken");
        assert_eq!(old.stage, LiteralStageV1::Parse);
    }
}

#[test]
fn subtraction_keeps_logical_node_number_and_broadcast_output_limits() {
    let term = format!("{}1{}", "(".repeat(32), ")".repeat(32));
    let source = format!("{}-(((1)))", vec![term.as_str(); 30].join("-"));
    let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap();
    assert_eq!(value.ast().nodes().len(), 1024);
    assert_eq!(value.array().as_slice(), &[-29.0]);
    let failure = error(&vec!["1"; 257].join("-"));
    assert_eq!(failure.span().start, 512);
    assert!(
        matches!(failure, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::Resource {
            name: LiteralResourceV1::NumberNodes, limit: Some(256), attempted_count: 257,
        })))
    );
    let source = format!(
        "[{}]-[{}]",
        vec!["1"; 16].join(";"),
        vec!["0"; 16].join(",")
    );
    let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap();
    assert_eq!(value.array().shape(), [16, 16]);
    assert_eq!(value.array().as_slice(), &[1.0; 256]);
    assert_eq!(
        error(&format!("[1,2]-[{}]", vec!["1"; 17].join(","))).code(),
        Some("MatrixNotAdmitted")
    );
}
