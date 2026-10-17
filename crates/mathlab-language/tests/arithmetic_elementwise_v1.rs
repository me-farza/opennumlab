// SPDX-License-Identifier: GPL-3.0-or-later
//! O11/O12 reuse accepted selected native observations from 20261016T035041036492Z.
//! Other expectations are original project cases, including 1..*2 composition.
//! No broad native, NaN-payload, IEEE, diagnostic-ID or performance claim.

use mathlab_array::{ArrayErrorV1, ErrorCodeV1, ErrorContextV1, MtimesErrorV1, MtimesWorkBudgetV1};
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
fn two_exact_native_sources_retain_class_shape_words_and_original_arc() {
    // Native record output.data_hex, never a Rust/Python-derived expectation.
    for (id, original, expected) in [
        ("O11", "1.*2", 0x4000000000000000_u64),
        ("O12", "1./2", 0x3fe0000000000000),
    ] {
        let source: Arc<str> = Arc::from(original);
        let value = evaluate_arithmetic_source_v1(Arc::clone(&source), budget(0)).unwrap();
        assert!(Arc::ptr_eq(&source, value.source()), "{id}");
        assert!(Arc::ptr_eq(&source, value.ast().source()), "{id}");
        assert_eq!(value.source().as_ref(), original, "{id}");
        assert_eq!(value.class(), "double", "{id}");
        assert_eq!(value.array().shape(), [1, 1], "{id}");
        assert_eq!(value.array().as_slice().len(), 1, "{id}");
        assert_eq!(value.array().as_slice()[0].to_bits(), expected, "{id}");
        let (parsed, _) = value.into_parts();
        assert!(Arc::ptr_eq(&source, parsed.source()), "{id}");
    }
}

#[test]
fn decimal_and_dot_operator_boundaries_have_exact_distinct_spans() {
    for (source, number_end, operator_start, operator_end, operator) in [
        ("1.*2", 1, 1, 3, ArithmeticOperatorV1::ElementwiseMultiply),
        ("1./2", 1, 1, 3, ArithmeticOperatorV1::ElementwiseDivide),
        (".5.*2", 2, 2, 4, ArithmeticOperatorV1::ElementwiseMultiply),
        ("1.5.*2", 3, 3, 5, ArithmeticOperatorV1::ElementwiseMultiply),
        ("1.e2./2", 4, 4, 6, ArithmeticOperatorV1::ElementwiseDivide),
        // Original project token composition; this spelling is not native-observed.
        ("1..*2", 2, 2, 4, ArithmeticOperatorV1::ElementwiseMultiply),
        ("1. *2", 2, 3, 4, ArithmeticOperatorV1::Multiply),
    ] {
        let parsed = parse_arithmetic_expression_v1(Arc::from(source)).unwrap();
        let ast = parsed.ast();
        let ArithmeticNodeKindV1::Binary {
            operator: actual,
            left,
            operator_span,
            ..
        } = ast.root().kind
        else {
            panic!("expected binary root: {source}");
        };
        assert_eq!(actual, operator, "{source}");
        assert_eq!(
            (operator_span.start, operator_span.end),
            (operator_start, operator_end),
            "{source}"
        );
        let ArithmeticNodeKindV1::Number { literal_span } = ast.node(left).unwrap().kind else {
            panic!("expected original numeric token: {source}");
        };
        assert_eq!(
            (literal_span.start, literal_span.end),
            (0, number_end),
            "{source}"
        );
    }
    for (source, expected) in [("1.", 1.0), ("1.e2", 100.0)] {
        let parsed = parse_arithmetic_expression_v1(Arc::from(source)).unwrap();
        let ArithmeticNodeKindV1::Number { literal_span } = parsed.ast().root().kind else {
            panic!("decimal spelling changed");
        };
        assert_eq!((literal_span.start, literal_span.end), (0, source.len()));
        let old = evaluate_decimal_source_v1(Arc::from(source)).unwrap();
        assert_eq!(old.array().as_slice(), &[expected]);
        assert!(evaluate_literal_source_v1(Arc::from(source)).is_err());
    }
}

#[test]
fn original_elementwise_values_and_shared_product_precedence_are_fixed() {
    for (source, expected) in [
        ("8./2.*2", 8.0),
        ("8.*2./4", 4.0),
        ("2+8./4.*3", 8.0),
        ("(2+8)./4.*3", 7.5),
        ("-8./-2", 4.0),
        (".5.*2", 1.0),
        ("1.5.*2", 3.0),
        ("1.e2./2", 50.0),
        ("1..*2", 2.0),
    ] {
        let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap();
        assert_eq!(value.array().as_slice(), &[expected], "{source}");
    }
    let value = evaluate_arithmetic_source_v1(Arc::from("[1,2].*[3,4]*[5;6]"), budget(2)).unwrap();
    assert_eq!(value.array().shape(), [1, 1]);
    assert_eq!(value.array().as_slice(), &[63.0]);
    let value = evaluate_arithmetic_source_v1(Arc::from("[1,2]*[3;4].*[5,6]"), budget(2)).unwrap();
    assert_eq!(value.array().shape(), [1, 2]);
    assert_eq!(value.array().as_slice(), &[55.0, 66.0]);
    let value = evaluate_arithmetic_source_v1(Arc::from("2.*[1,2,3]"), budget(0)).unwrap();
    assert_eq!(value.array().as_slice(), &[2.0, 4.0, 6.0]);
    assert!(matches!(
        error("2*[1,2,3]"),
        ArithmeticExpressionErrorV1::Multiplication {
            error: MtimesErrorV1::WorkLimitExceeded {
                required_terms: 3,
                max_terms: 0
            },
            ..
        }
    ));
}

#[test]
fn original_matrix_and_singleton_semantics_are_column_major() {
    let cases: &[(&str, [u64; 2], &[f64])] = &[
        ("[1,2].*[3;4]", [2, 2], &[3.0, 4.0, 6.0, 8.0]),
        ("[2;4]./[2,4,8]", [2, 3], &[1.0, 2.0, 0.5, 1.0, 0.25, 0.5]),
        ("[2,4,8]./[2;4]", [2, 3], &[1.0, 0.5, 2.0, 1.0, 4.0, 2.0]),
        ("8./[2,4]", [1, 2], &[4.0, 2.0]),
        ("[2,4]./8", [1, 2], &[0.25, 0.5]),
        ("[1,2;3,4].*[4,3;2,1]", [2, 2], &[4.0, 6.0, 6.0, 4.0]),
        ("[1,2;3,4]./[2,4;4,8]", [2, 2], &[0.5, 0.75, 0.5, 0.5]),
        ("[.5,1.5].*[2,3]", [1, 2], &[1.0, 4.5]),
    ];
    for (source, shape, expected) in cases {
        let value = evaluate_arithmetic_source_v1(Arc::from(*source), budget(0)).unwrap();
        assert_eq!(value.array().shape(), *shape, "{source}");
        assert_eq!(value.array().as_slice(), *expected, "{source}");
    }
}

#[test]
fn original_zero_and_nonfinite_policy_preserves_selected_words_or_nan_category() {
    let cases: &[(&str, &[u64])] = &[
        (
            "[0,-0,0,-0].*[2,2,-2,-2]",
            &[
                0x0000000000000000,
                0x8000000000000000,
                0x8000000000000000,
                0x0000000000000000,
            ],
        ),
        (
            "[0,-0,0,-0]./[2,2,-2,-2]",
            &[
                0x0000000000000000,
                0x8000000000000000,
                0x8000000000000000,
                0x0000000000000000,
            ],
        ),
        (
            "[1,-1,1,-1]./[0,0,-0,-0]",
            &[
                0x7ff0000000000000,
                0xfff0000000000000,
                0xfff0000000000000,
                0x7ff0000000000000,
            ],
        ),
        (
            "[1e10000,-1e10000].*[2,-2]",
            &[0x7ff0000000000000, 0x7ff0000000000000],
        ),
        ("1./-1e10000", &[0x8000000000000000]),
    ];
    for (source, expected) in cases {
        let value = evaluate_arithmetic_source_v1(Arc::from(*source), budget(0)).unwrap();
        let words: Vec<_> = value
            .array()
            .as_slice()
            .iter()
            .map(|v| v.to_bits())
            .collect();
        assert_eq!(words.as_slice(), *expected, "{source}");
    }
    for source in ["[0,1e10000].*[1e10000,0]", "[0,1e10000]./[0,1e10000]"] {
        let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap();
        assert!(value.array().as_slice().iter().all(|v| v.is_nan()));
    }
}

#[test]
fn leaf_failures_attach_two_character_original_operator_spans() {
    let source: Arc<str> = Arc::from("%λ\r\n[1;2]./[3;4;5]");
    let failure = evaluate_arithmetic_source_v1(Arc::clone(&source), budget(0)).unwrap_err();
    assert!(Arc::ptr_eq(&source, failure.original_source()));
    assert_eq!(failure.stage(), LiteralStageV1::Evaluation);
    assert_eq!(failure.code(), Some("ArrayOperation"));
    assert_eq!(
        (
            failure.span().start,
            failure.span().end,
            failure.span().start_line
        ),
        (10, 12, 2)
    );
    assert_eq!(&source[failure.span().start..failure.span().end], "./");
    assert!(
        matches!(failure, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::ArrayOperation {
            error: ArrayErrorV1 { context: ErrorContextV1::RDivideCompatibility,
                code: ErrorCodeV1::IncompatibleShape { axis: 0, lhs_extent: 2, rhs_extent: 3 } }
        })))
    );
}

#[test]
fn admission_full_lexing_and_left_first_errors_keep_the_shared_order() {
    assert_eq!(
        error("([1;2]./[3;4;5])+[1,2;3]").code(),
        Some("MatrixNotAdmitted")
    );
    let source = "([1;2].*[3;4;5])-([1,2]*[3,4])";
    let failure = error(source);
    assert_eq!(failure.span().start, source.find(".*").unwrap());
    assert!(
        matches!(failure, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::ArrayOperation {
            error: ArrayErrorV1 { context: ErrorContextV1::TimesCompatibility, .. }
        })))
    );
    let source = "([1;2]+[3;4;5])./([1,2].*[3,4,5])";
    let failure = error(source);
    assert_eq!(failure.span().start, source.find('+').unwrap());
    assert_eq!(error("1./0 π").stage(), LiteralStageV1::Lex);
}

#[test]
fn empty_dot_operations_validate_both_axes_without_a_mtimes_budget() {
    for source in ["[].*1", "1.*[]", "[]./1", "1./[]", "[].*[]", "[]./[]"] {
        let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap();
        assert_eq!(value.array().shape(), [0, 0]);
        assert!(value.array().as_slice().is_empty());
    }
    for (source, context) in [
        ("[1,2].*[]", ErrorContextV1::TimesCompatibility),
        ("[1,2]./[]", ErrorContextV1::RDivideCompatibility),
    ] {
        let failure = error(source);
        let ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error)) =
            failure
        else {
            panic!("expected leaf diagnostic");
        };
        assert!(
            matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::ArrayOperation {
            error: ArrayErrorV1 { context: actual, code: ErrorCodeV1::IncompatibleShape { axis: 1, lhs_extent: 2, rhs_extent: 0 } }
        }) if actual == context)
        );
    }
}

#[test]
fn old_facades_and_unrelated_operators_keep_their_exact_exclusions() {
    for source in ["1.*2", "1./2"] {
        let LiteralExpressionErrorV1::Diagnostic(old) =
            evaluate_literal_source_v1(Arc::from(source)).unwrap_err()
        else {
            panic!("expected literal rejection");
        };
        assert_eq!(old.code.as_str(), "UnsupportedToken");
        assert_eq!(old.stage, LiteralStageV1::Lex);
        let DecimalExpressionErrorV1::Diagnostic(old) =
            evaluate_decimal_source_v1(Arc::from(source)).unwrap_err()
        else {
            panic!("expected decimal rejection");
        };
        assert_eq!(old.code.as_str(), "UnsupportedToken");
        assert_eq!(old.stage, LiteralStageV1::Lex);
    }
    for source in [
        "1/2", "[1,2]'", "1D2", "1. /2", ". *2", "1..2", "1e.*2", "1.2.3.*4",
    ] {
        assert!(
            evaluate_arithmetic_source_v1(Arc::from(source), budget(1)).is_err(),
            "{source}"
        );
    }
    assert!(
        matches!(error("1.*.*2"), ArithmeticExpressionErrorV1::Syntax {
        found: Some(ArithmeticTokenKindV1::DotStar), span, .. } if (span.start, span.end) == (3, 5))
    );
    assert!(
        matches!(error("1././2"), ArithmeticExpressionErrorV1::Syntax {
        found: Some(ArithmeticTokenKindV1::DotSlash), span, .. } if (span.start, span.end) == (3, 5))
    );
    assert_eq!(error("1./").code(), Some("ExpectedExpression"));
}

#[test]
fn two_byte_operators_charge_one_ast_node_and_keep_numeric_array_bounds() {
    let term = format!("{}1{}", "(".repeat(32), ")".repeat(32));
    let source = format!("{}.*(((1)))", vec![term.as_str(); 30].join(".*"));
    let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap();
    assert_eq!(value.ast().nodes().len(), 1024);
    assert_eq!(value.array().as_slice(), &[1.0]);
    let failure = error(&vec![term.as_str(); 31].join(".*"));
    assert_eq!(failure.span().start, 2014);
    assert!(
        matches!(failure, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::Resource {
            name: LiteralResourceV1::AstNodes, limit: Some(1024), attempted_count: 1025,
        })))
    );
    let failure = error(&vec!["1"; 257].join("./"));
    assert_eq!(failure.span().start, 768);
    assert!(
        matches!(failure, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::Resource {
            name: LiteralResourceV1::NumberNodes, limit: Some(256), attempted_count: 257,
        })))
    );
    for (operator, expected) in [(".*", 8.0), ("./", 0.5)] {
        let source = format!(
            "[{}]{operator}[{}]",
            vec!["2"; 16].join(";"),
            vec!["4"; 16].join(",")
        );
        let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap();
        assert_eq!(value.array().shape(), [16, 16]);
        assert!(value.array().as_slice().iter().all(|v| *v == expected));
    }
}
