// SPDX-License-Identifier: GPL-3.0-or-later
//! Original hand-calculated finite cases. No new MATLAB source oracle is claimed.
use mathlab_array::{ErrorCodeV1, ErrorContextV1, MtimesErrorV1, MtimesWorkBudgetV1};
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
fn error(source: &str, max_terms: u64) -> ArithmeticExpressionErrorV1 {
    evaluate_arithmetic_source_v1(Arc::from(source), budget(max_terms)).unwrap_err()
}

#[test]
fn scalar_precedence_and_grouping_have_fixed_distinct_results() {
    for (source, expected) in [
        ("2+3*4", 14.0),
        ("(2+3)*4", 20.0),
        ("2*3+4*5", 26.0),
        ("2*3*4", 24.0),
        ("2*.5+1e1", 11.0),
        ("1+-2*3", -5.0),
        ("1. * 2", 2.0),
    ] {
        let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(1)).unwrap();
        assert_eq!(value.class(), "double");
        assert_eq!(value.array().shape(), [1, 1]);
        assert_eq!(value.array().as_slice(), &[expected], "{source}");
    }
}

#[test]
fn products_reach_the_owned_column_major_leaf_without_elementwise_broadcasting() {
    let cases: &[(&str, u64, [u64; 2], &[f64])] = &[
        (
            "[1,2,3;4,5,6]*[7,8;9,10;11,12]",
            12,
            [2, 2],
            &[58.0, 139.0, 64.0, 154.0],
        ),
        ("[1,2;0,1]*[1,0;3,1]", 8, [2, 2], &[7.0, 3.0, 2.0, 1.0]),
        ("[1,0;3,1]*[1,2;0,1]", 8, [2, 2], &[1.0, 3.0, 2.0, 7.0]),
        ("[1,2;3,4]*[0,1;1,0]", 8, [2, 2], &[2.0, 4.0, 1.0, 3.0]),
        // B02 supplies a retained numerical operand/result observation; this
        // new complete source expression has not been executed in MATLAB.
        ("[1,2;3,4]*[1,2;3,4]", 8, [2, 2], &[7.0, 15.0, 10.0, 22.0]),
        ("[1,-2,3]*[4;5;-6]", 3, [1, 1], &[-24.0]),
        (
            "[2;-3;4]*[-1,5]",
            6,
            [3, 2],
            &[-2.0, 3.0, -4.0, 10.0, -15.0, 20.0],
        ),
        ("-2*[1,-2;4,3]", 4, [2, 2], &[-2.0, -8.0, 4.0, -6.0]),
        ("[1,-2;4,3]*-2", 4, [2, 2], &[-2.0, -8.0, 4.0, -6.0]),
        ("[.5,1.]*[2;4]", 2, [1, 1], &[5.0]),
        ("([1,2;3,4]+1)*[0,1;1,0]", 8, [2, 2], &[3.0, 5.0, 2.0, 4.0]),
    ];
    for (source, max_terms, shape, expected) in cases {
        let source: Arc<str> = Arc::from(*source);
        let parsed = parse_arithmetic_expression_v1(Arc::clone(&source)).unwrap();
        let ast = format!("{:?}", parsed.ast());
        let ir = admit_arithmetic_expression_v1(parsed).unwrap();
        let value = evaluate_arithmetic_expression_v1(ir, budget(*max_terms)).unwrap();
        assert_eq!(value.array().shape(), *shape, "{source}");
        assert_eq!(value.array().as_slice(), *expected, "{source}");
        assert!(Arc::ptr_eq(&source, value.source()));
        assert_eq!(format!("{:?}", value.ast()), ast);
        let (parsed, array) = value.into_parts();
        drop(source);
        assert_eq!(parsed.ast().root().span.end, parsed.source().len());
        assert_eq!(array.as_slice(), *expected);
    }
}

#[test]
fn product_associativity_is_visible_under_a_per_operation_budget() {
    // Left fold: dot needs 2 terms, then scalar*row needs 2. Right fold
    // would first request a 2-by-2 outer product requiring 4 terms.
    let value = evaluate_arithmetic_source_v1(Arc::from("[1,2]*[3;4]*[5,6]"), budget(2)).unwrap();
    assert_eq!(value.array().shape(), [1, 2]);
    assert_eq!(value.array().as_slice(), &[55.0, 66.0]);
    let rejected = error("[1,2]*([3;4]*[5,6])", 2);
    assert_eq!(rejected.span().start, 12);
    assert!(matches!(
        rejected,
        ArithmeticExpressionErrorV1::Multiplication {
            error: MtimesErrorV1::WorkLimitExceeded {
                required_terms: 4,
                max_terms: 2
            },
            ..
        }
    ));
}

#[test]
fn work_budget_failure_retains_the_actual_star_and_complete_leaf_error() {
    let source: Arc<str> = Arc::from("%λ\n2*[1,2,3]");
    let failure = evaluate_arithmetic_source_v1(Arc::clone(&source), budget(2)).unwrap_err();
    assert_eq!(failure.code(), Some("ArrayOperation"));
    assert_eq!(failure.stage(), LiteralStageV1::Evaluation);
    assert!(Arc::ptr_eq(&source, failure.original_source()));
    assert_eq!(&source[failure.span().start..failure.span().end], "*");
    assert_eq!(failure.span().start, 5);
    assert_eq!(failure.span().start_line, 2);
    assert!(matches!(
        failure,
        ArithmeticExpressionErrorV1::Multiplication {
            error: MtimesErrorV1::WorkLimitExceeded {
                required_terms: 3,
                max_terms: 2
            },
            ..
        }
    ));
    let value = evaluate_arithmetic_source_v1(source, budget(3)).unwrap();
    assert_eq!(value.array().as_slice(), &[2.0, 4.0, 6.0]);
}

#[test]
fn empty_products_use_scalar_exception_and_inner_compatibility() {
    for source in ["[]*2", "2*[]", "[]*[]"] {
        let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap();
        assert_eq!(value.array().shape(), [0, 0]);
        assert!(value.array().as_slice().is_empty());
    }
    assert!(matches!(
        error("[1,2]*[]", 0),
        ArithmeticExpressionErrorV1::Multiplication {
            error: MtimesErrorV1::IncompatibleInnerDimensions {
                lhs_columns: 2,
                rhs_rows: 0
            },
            ..
        }
    ));
}

#[test]
fn complete_admission_and_left_before_right_evaluation_are_shared() {
    let first = error("([1;2]+[3;4;5])*([1,2]*[3,4])", 256);
    assert_eq!(
        &first.original_source()[first.span().start..first.span().end],
        "+"
    );
    assert!(
        matches!(first, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::ArrayOperation {
            error: mathlab_array::ArrayErrorV1 { context: ErrorContextV1::AddCompatibility,
                code: ErrorCodeV1::IncompatibleShape { axis: 0, lhs_extent: 2, rhs_extent: 3 } }
        })))
    );
    assert_eq!(
        error("([1,2]*[3,4])+[1,2;3]", 256).code(),
        Some("MatrixNotAdmitted")
    );
    let source = "[1,2]*([3,4]*[5,6])";
    let failure = error(source, 256);
    assert_eq!(failure.span().start, source.rfind('*').unwrap());
}

#[test]
fn arithmetic_ast_has_neutral_binary_operators_and_preserved_groups() {
    let parsed = parse_arithmetic_expression_v1(Arc::from("2+3*4")).unwrap();
    let ast = parsed.ast();
    let ArithmeticNodeKindV1::Binary {
        operator: ArithmeticOperatorV1::Add,
        operator_span,
        right,
        ..
    } = ast.root().kind
    else {
        panic!("expected Add root");
    };
    assert_eq!(operator_span.start, 1);
    let ArithmeticNodeKindV1::Binary {
        operator: ArithmeticOperatorV1::Multiply,
        operator_span,
        ..
    } = ast.node(right).unwrap().kind
    else {
        panic!("expected product on right");
    };
    assert_eq!(operator_span.start, 3);
    let parsed = parse_arithmetic_expression_v1(Arc::from("(2+3)*4")).unwrap();
    let ast = parsed.ast();
    let ArithmeticNodeKindV1::Binary {
        operator: ArithmeticOperatorV1::Multiply,
        left,
        ..
    } = ast.root().kind
    else {
        panic!("expected product root");
    };
    assert!(matches!(
        ast.node(left).unwrap().kind,
        ArithmeticNodeKindV1::Group { .. }
    ));
}

#[test]
fn old_profiles_still_reject_star_during_complete_lexing() {
    for source in ["2*3", "--1*2", "[1*2]"] {
        let LiteralExpressionErrorV1::Diagnostic(old) =
            evaluate_literal_source_v1(Arc::from(source)).unwrap_err()
        else {
            panic!("expected v1 project diagnostic");
        };
        assert_eq!(old.code.as_str(), "UnsupportedToken");
        assert_eq!(old.stage, LiteralStageV1::Lex);
        let DecimalExpressionErrorV1::Diagnostic(old) =
            evaluate_decimal_source_v1(Arc::from(source)).unwrap_err()
        else {
            panic!("expected decimal-v1 diagnostic");
        };
        assert_eq!(old.code.as_str(), "UnsupportedToken");
        assert_eq!(old.stage, LiteralStageV1::Lex);
    }
}

#[test]
fn syntax_lexing_and_resource_order_do_not_expand_the_other_grammar() {
    for (source, code, start) in [
        ("1**2", "UnexpectedToken", 2),
        ("1*", "ExpectedExpression", 2),
        ("*1", "UnexpectedToken", 0),
        ("[1*2]", "UnexpectedToken", 2),
    ] {
        let failure = error(source, 100);
        assert_eq!(failure.code(), Some(code));
        assert_eq!(failure.span().start, start);
    }
    assert!(matches!(
        error("1**2", 100),
        ArithmeticExpressionErrorV1::Syntax {
            found: Some(ArithmeticTokenKindV1::Star),
            ..
        }
    ));
    assert_eq!(error("--1*2 π", 100).code(), Some("UnsupportedToken"));
    assert_eq!(error("1*\n2", 100).code(), Some("UnsupportedSourceForm"));
    for source in ["[1 2]", "[1,]", "1;", "1\n2"] {
        assert!(evaluate_arithmetic_source_v1(Arc::from(source), budget(256)).is_err());
    }
    let limit = error(&vec!["1"; 257].join("*"), 1);
    assert_eq!(limit.span().start, 512);
    assert!(
        matches!(limit, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::Resource {
            name: LiteralResourceV1::NumberNodes, limit: Some(256), attempted_count: 257,
        })))
    );
    let term = format!("{}1{}", "(".repeat(32), ")".repeat(32));
    let limit = error(&vec![term.as_str(); 31].join("*"), 1);
    assert_eq!(limit.span().start, 1984);
}

#[test]
fn arithmetic_ast_limit_counts_product_nodes_once() {
    let term = format!("{}1{}", "(".repeat(32), ")".repeat(32));
    // Thirty 33-node terms, 29 products, then one product and a 4-node
    // grouped scalar: exactly 1024 nodes. This also checks retained groups.
    let source = format!("{}*(((1)))", vec![term.as_str(); 30].join("*"));
    let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(1)).unwrap();
    assert_eq!(value.ast().nodes().len(), 1024);
    assert_eq!(value.array().as_slice(), &[1.0]);
    let rejected = error(&vec![term.as_str(); 31].join("*"), 1);
    assert!(
        matches!(rejected, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::Resource {
            name: LiteralResourceV1::AstNodes, limit: Some(1024), attempted_count: 1025,
        })))
    );
}
