// SPDX-License-Identifier: GPL-3.0-or-later
//! D10/D11 use accepted native source/value records from 20261016T024523895711Z.
//! All other expectations are original project cases, not native AST, diagnostic,
//! NaN, general IEEE or performance equivalence observations.

use mathlab_array::{ErrorContextV1, MtimesErrorV1, MtimesWorkBudgetV1};
use mathlab_language::arithmetic_expression::*;
use mathlab_language::decimal_expression::{
    DecimalDiagnosticDetailV1, DecimalExpressionErrorV1, evaluate_decimal_source_v1,
};
use mathlab_language::expression::{
    LiteralDiagnosticDetailV1, LiteralExpressionErrorV1, LiteralResourceV1, LiteralSignV1,
    LiteralStageV1,
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
fn two_exact_original_sources_match_accepted_native_value_words() {
    // Sources and words are copied from accepted D10/D11, without conversion.
    let cases: &[(&str, &str, [u64; 2], &[u64])] = &[
        ("D10", "--1", [1, 1], &[0x3ff0000000000000]),
        (
            "D11",
            "-[1,2]",
            [1, 2],
            &[0xbff0000000000000, 0xc000000000000000],
        ),
    ];
    for (id, original, shape, expected) in cases {
        let source: Arc<str> = Arc::from(*original);
        let value = evaluate_arithmetic_source_v1(Arc::clone(&source), budget(0)).unwrap();
        assert!(Arc::ptr_eq(&source, value.source()), "{id}");
        assert_eq!(value.class(), "double", "{id}");
        assert_eq!(value.array().shape(), *shape, "{id}");
        let actual: Vec<_> = value
            .array()
            .as_slice()
            .iter()
            .map(|v| v.to_bits())
            .collect();
        assert_eq!(actual.as_slice(), *expected, "{id}");
        let (parsed, _) = value.into_parts();
        assert!(Arc::ptr_eq(&source, parsed.source()), "{id}");
    }
}

#[test]
fn project_prefix_group_matrix_and_product_precedence_have_fixed_values() {
    let cases: &[(&str, u64, [u64; 2], &[u64])] = &[
        ("-(1+2)*+4", 1, [1, 1], &[0xc028000000000000]),
        ("--(2+3)*-4", 1, [1, 1], &[0xc034000000000000]),
        (
            "1+-[2,3]",
            0,
            [1, 2],
            &[0xbff0000000000000, 0xc000000000000000],
        ),
        (
            "-+[1,-2;3,-4]",
            0,
            [2, 2],
            &[
                0xbff0000000000000,
                0xc008000000000000,
                0x4000000000000000,
                0x4010000000000000,
            ],
        ),
        (
            "++(-[1,2]+[3,4])",
            0,
            [1, 2],
            &[0x4000000000000000, 0x4000000000000000],
        ),
        ("-(-0)", 0, [1, 1], &[0x0000000000000000]),
        ("+(-0)", 0, [1, 1], &[0x8000000000000000]),
        ("--0", 0, [1, 1], &[0x0000000000000000]),
        (
            "-[-0,0]",
            0,
            [1, 2],
            &[0x0000000000000000, 0x8000000000000000],
        ),
        ("-(1e10000)", 0, [1, 1], &[0xfff0000000000000]),
        ("--(1e10000)", 0, [1, 1], &[0x7ff0000000000000]),
    ];
    for (source, terms, shape, expected) in cases {
        let value = evaluate_arithmetic_source_v1(Arc::from(*source), budget(*terms)).unwrap();
        assert_eq!(value.array().shape(), *shape, "{source}");
        let actual: Vec<_> = value
            .array()
            .as_slice()
            .iter()
            .map(|v| v.to_bits())
            .collect();
        assert_eq!(actual.as_slice(), *expected, "{source}");
    }
    for source in ["-[]", "+[]", "--[]", "-([]+1)"] {
        let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap();
        assert_eq!(value.array().shape(), [0, 0]);
        assert!(value.array().as_slice().is_empty());
    }
}

#[test]
fn single_numeric_sign_keeps_signed_shape_and_general_prefixes_keep_actual_spans() {
    let parsed = parse_arithmetic_expression_v1(Arc::from("- 1")).unwrap();
    let ast = parsed.ast();
    assert_eq!(ast.nodes().len(), 2);
    let ArithmeticNodeKindV1::Signed {
        sign: LiteralSignV1::Minus,
        sign_span,
        number,
    } = ast.root().kind
    else {
        panic!("direct sign must stay Signed");
    };
    assert_eq!((sign_span.start, sign_span.end), (0, 1));
    assert!(matches!(
        ast.node(number).unwrap().kind,
        ArithmeticNodeKindV1::Number { .. }
    ));
    let parsed = parse_arithmetic_expression_v1(Arc::from("--1")).unwrap();
    let ast = parsed.ast();
    let ArithmeticNodeKindV1::Unary {
        sign: LiteralSignV1::Minus,
        sign_span,
        operand,
    } = ast.root().kind
    else {
        panic!("outer sign must be Unary");
    };
    assert_eq!((sign_span.start, sign_span.end), (0, 1));
    assert!(matches!(
        ast.node(operand).unwrap().kind,
        ArithmeticNodeKindV1::Signed { .. }
    ));
    let source: Arc<str> = Arc::from("%λ\n-(1+2)");
    let parsed = parse_arithmetic_expression_v1(Arc::clone(&source)).unwrap();
    let ast = parsed.ast();
    let ArithmeticNodeKindV1::Unary {
        sign_span, operand, ..
    } = ast.root().kind
    else {
        panic!("expected general prefix");
    };
    assert_eq!(
        (sign_span.start, sign_span.end, sign_span.start_line),
        (4, 5, 2)
    );
    assert_eq!((ast.root().span.start, ast.root().span.end), (4, 10));
    assert!(matches!(
        ast.node(operand).unwrap().kind,
        ArithmeticNodeKindV1::Group { .. }
    ));
    assert!(Arc::ptr_eq(&source, ast.source()));
}

#[test]
fn literal_and_decimal_facades_keep_their_original_rejections() {
    for source in ["--1", "-[1,2]", "-(1+2)", "++1", "+[]"] {
        let LiteralExpressionErrorV1::Diagnostic(old) =
            evaluate_literal_source_v1(Arc::from(source)).unwrap_err()
        else {
            panic!("expected original literal diagnostic");
        };
        assert_eq!(old.code.as_str(), "UnexpectedToken");
        assert_eq!(old.stage, LiteralStageV1::Parse);
        let DecimalExpressionErrorV1::Diagnostic(old) =
            evaluate_decimal_source_v1(Arc::from(source)).unwrap_err()
        else {
            panic!("expected original decimal diagnostic");
        };
        assert_eq!(old.code.as_str(), "UnexpectedToken");
        assert_eq!(old.stage, LiteralStageV1::Parse);
    }
}

#[test]
fn admission_and_left_first_failure_order_precede_any_pending_unary() {
    assert_eq!(
        error("-([1;2]+[3;4;5])+[1,2;3]").code(),
        Some("MatrixNotAdmitted")
    );
    let source = "-([1;2]+[3;4;5])+-([1,2]*[3,4])";
    let first = error(source);
    assert_eq!(first.span().start, source.find('+').unwrap());
    assert!(
        matches!(first, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::ArrayOperation {
            error: mathlab_array::ArrayErrorV1 { context: ErrorContextV1::AddCompatibility, .. }
        })))
    );
    assert_eq!(error("---1 π").stage(), LiteralStageV1::Lex);
    assert_eq!(error("---[1,2;3]").code(), Some("MatrixNotAdmitted"));
}

#[test]
fn prefix_operations_do_not_consume_a_multiplication_term_budget() {
    let source = "---2*--[1,2,3]";
    let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(3)).unwrap();
    assert_eq!(value.array().as_slice(), &[-2.0, -4.0, -6.0]);
    let failure = evaluate_arithmetic_source_v1(Arc::from(source), budget(2)).unwrap_err();
    assert_eq!(failure.span().start, source.find('*').unwrap());
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
}

#[test]
fn flat_prefix_chains_hit_logical_node_limits_without_recursive_sign_calls() {
    let source = format!("{}1", "-".repeat(1023));
    let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap();
    assert_eq!(value.ast().nodes().len(), 1024);
    assert_eq!(value.array().as_slice()[0].to_bits(), 0xbff0000000000000);
    let rejected = error(&format!("{}1", "-".repeat(1024)));
    assert_eq!(rejected.span().start, 1024);
    assert!(
        matches!(rejected, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::Resource {
            name: LiteralResourceV1::AstNodes, limit: Some(1024), attempted_count: 1025,
        })))
    );
    let rejected = error(&format!("{}1", "-".repeat(4096)));
    assert_eq!(rejected.stage(), LiteralStageV1::Framing);
    assert_eq!(rejected.span().start, 0);
}

#[test]
fn prefixes_preserve_depth_number_limits_and_literal_entry_grammar() {
    let source = format!("-{}1{}", "(".repeat(32), ")".repeat(32));
    let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap();
    assert_eq!(value.array().as_slice(), &[-1.0]);
    let source = format!("-{}1{}", "(".repeat(33), ")".repeat(33));
    let rejected = error(&source);
    assert_eq!(rejected.span().start, 33);
    assert!(
        matches!(rejected, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::Resource {
            name: LiteralResourceV1::OpenDelimiters, limit: Some(32), attempted_count: 33,
        })))
    );
    let source = vec!["-1"; 257].join("+");
    let rejected = error(&source);
    assert_eq!(rejected.span().start, 769);
    assert!(
        matches!(rejected, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::Resource {
            name: LiteralResourceV1::NumberNodes, limit: Some(256), attempted_count: 257,
        })))
    );
    // Matrix entries have not become general expressions or nested arrays.
    for source in ["[--1]", "[-(1)]", "[-[1]]", "1;", "1\n2"] {
        assert!(evaluate_arithmetic_source_v1(Arc::from(source), budget(1)).is_err());
    }
    for source in ["-", "---", "-(", "-)"] {
        assert!(error(source).code().is_some());
    }
}
