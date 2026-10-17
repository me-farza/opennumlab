// SPDX-License-Identifier: GPL-3.0-or-later
//! Only O13's exact `1.'` source/value is retained native evidence. Other cases
//! are project expectations for this real-double, nonconjugating subset.

use mathlab_array::{ErrorContextV1, MtimesWorkBudgetV1};
use mathlab_language::arithmetic_expression::*;
use mathlab_language::decimal_expression::{
    DecimalDiagnosticDetailV1, DecimalExpressionErrorV1, evaluate_decimal_source_v1,
};
use mathlab_language::expression::{
    LiteralDiagnosticDetailV1, LiteralResourceV1, LiteralSignV1, LiteralStageV1,
};
use mathlab_language::literal_evaluation::evaluate_literal_source_v1;
use std::sync::Arc;

fn budget(max_terms: u64) -> MtimesWorkBudgetV1 {
    MtimesWorkBudgetV1 { max_terms }
}

#[test]
fn exact_o13_source_retains_native_class_shape_word_and_original_arc() {
    // 20261016T035041036492Z records/O13/record.json; no float conversion.
    let source: Arc<str> = Arc::from("1.'");
    let value = evaluate_arithmetic_source_v1(Arc::clone(&source), budget(0)).unwrap();
    assert!(Arc::ptr_eq(&source, value.source()));
    assert_eq!(value.class(), "double");
    assert_eq!(value.array().shape(), [1, 1]);
    assert_eq!(value.array().as_slice().len(), 1);
    assert_eq!(value.array().as_slice()[0].to_bits(), 0x3ff0000000000000);
    let (parsed, array) = value.into_parts();
    assert!(Arc::ptr_eq(&source, parsed.source()));
    assert_eq!(array.as_slice()[0].to_bits(), 0x3ff0000000000000);
}

#[test]
fn project_real_transposes_keep_column_major_order_and_dimensions() {
    let cases: &[(&str, [u64; 2], &[f64])] = &[
        ("[1,2,3;4,5,6].'", [3, 2], &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
        ("[1,2;3,4].'", [2, 2], &[1.0, 2.0, 3.0, 4.0]),
        ("[1,2,3].'", [3, 1], &[1.0, 2.0, 3.0]),
        ("[1;2;3].'", [1, 3], &[1.0, 2.0, 3.0]),
        ("[1,2,3;4,5,6].'.'", [2, 3], &[1.0, 4.0, 2.0, 5.0, 3.0, 6.0]),
    ];
    for (source, shape, expected) in cases {
        let value = evaluate_arithmetic_source_v1(Arc::from(*source), budget(0)).unwrap();
        assert_eq!(value.array().shape(), *shape, "{source}");
        assert_eq!(value.array().as_slice(), *expected, "{source}");
    }
    for source in ["[].'", "(-[]).'.'", "([]+1).'"] {
        let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap();
        assert_eq!(value.array().shape(), [0, 0]);
        assert!(value.array().as_slice().is_empty());
    }
}

#[test]
fn project_postfix_binds_above_prefix_and_product_without_charging_mtimes() {
    let parsed = parse_arithmetic_expression_v1(Arc::from("-[1,2].'")).unwrap();
    let ast = parsed.ast();
    let ArithmeticNodeKindV1::Unary {
        sign: LiteralSignV1::Minus,
        operand,
        ..
    } = ast.root().kind
    else {
        panic!("expected prefix root");
    };
    let ArithmeticNodeKindV1::Transpose {
        operator_span,
        operand,
    } = ast.node(operand).unwrap().kind
    else {
        panic!("expected postfix inside prefix");
    };
    assert_eq!((operator_span.start, operator_span.end), (6, 8));
    assert!(matches!(
        ast.node(operand).unwrap().kind,
        ArithmeticNodeKindV1::Matrix { .. }
    ));
    let value = evaluate_arithmetic_source_v1(Arc::from("-[1,2].'"), budget(0)).unwrap();
    assert_eq!(value.array().shape(), [2, 1]);
    assert_eq!(value.array().as_slice(), &[-1.0, -2.0]);
    let value = evaluate_arithmetic_source_v1(Arc::from("[1,2].'*[3,4]"), budget(4)).unwrap();
    assert_eq!(value.array().shape(), [2, 2]);
    assert_eq!(value.array().as_slice(), &[3.0, 6.0, 4.0, 8.0]);
    let value = evaluate_arithmetic_source_v1(Arc::from("[1,2;3,4].'*[5;6]"), budget(4)).unwrap();
    assert_eq!(value.array().shape(), [2, 1]);
    assert_eq!(value.array().as_slice(), &[23.0, 34.0]);
}

#[test]
fn project_numeric_boundaries_and_transpose_preserve_zero_and_infinity_words() {
    for source in ["1.'", "1..'", "1.0.'", ".5.'", "1.e2.'", "1 .'"] {
        let parsed = parse_arithmetic_expression_v1(Arc::from(source)).unwrap();
        let ast = parsed.ast();
        let ArithmeticNodeKindV1::Transpose {
            operator_span,
            operand,
        } = ast.root().kind
        else {
            panic!("expected transpose: {source}");
        };
        assert_eq!(&source[operator_span.start..operator_span.end], ".'");
        assert!(matches!(
            ast.node(operand).unwrap().kind,
            ArithmeticNodeKindV1::Number { .. }
        ));
    }
    // `1..'` is token composition in this project, not a native observation.
    let value =
        evaluate_arithmetic_source_v1(Arc::from("[-0,0;1e10000,-1e10000].'"), budget(0)).unwrap();
    assert_eq!(value.array().shape(), [2, 2]);
    let expected = [
        0x8000000000000000_u64,
        0,
        0x7ff0000000000000,
        0xfff0000000000000,
    ];
    for (value, bits) in value.array().as_slice().iter().zip(expected) {
        assert_eq!(value.to_bits(), bits);
    }
}

#[test]
fn project_operand_errors_and_complete_admission_precede_pending_transpose() {
    let source: Arc<str> = Arc::from("%λ\r\n([1;2]+[3;4;5]).'");
    let failure = evaluate_arithmetic_source_v1(Arc::clone(&source), budget(0)).unwrap_err();
    assert!(Arc::ptr_eq(&source, failure.original_source()));
    assert_eq!(failure.span().start, source.find('+').unwrap());
    assert_eq!(failure.stage(), LiteralStageV1::Evaluation);
    assert!(
        matches!(failure, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::ArrayOperation {
            error: mathlab_array::ArrayErrorV1 { context: ErrorContextV1::AddCompatibility, .. }
        })))
    );
    let error = evaluate_arithmetic_source_v1(Arc::from("([1;2]+[3;4;5]).'+[1,2;3]"), budget(0))
        .unwrap_err();
    assert_eq!(error.code(), Some("MatrixNotAdmitted"));
    assert_eq!(
        evaluate_arithmetic_source_v1(Arc::from("1.' π"), budget(0))
            .unwrap_err()
            .stage(),
        LiteralStageV1::Lex
    );
}

#[test]
fn project_flat_postfix_chains_charge_nodes_without_recursive_parser_calls() {
    let source = format!("1{}", ".'".repeat(1023));
    let value = evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).unwrap();
    assert_eq!(value.ast().nodes().len(), 1024);
    assert_eq!(value.array().as_slice()[0].to_bits(), 0x3ff0000000000000);
    let error =
        evaluate_arithmetic_source_v1(Arc::from(format!("1{}", ".'".repeat(1024))), budget(0))
            .unwrap_err();
    assert_eq!(error.span().start, 2047);
    assert!(
        matches!(error, ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error))
        if matches!(error.detail, DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::Resource {
            name: LiteralResourceV1::AstNodes, limit: Some(1024), attempted_count: 1025,
        })))
    );
}

#[test]
fn project_legacy_profiles_and_unsupported_quote_forms_remain_excluded() {
    for source in ["1.'", "[1,2].'", "1..'"] {
        assert!(evaluate_literal_source_v1(Arc::from(source)).is_err());
        assert!(evaluate_decimal_source_v1(Arc::from(source)).is_err());
    }
    for source in ["1'", "1.'+", "1. '1'", "[1.';2]", "1D2", "1/2"] {
        assert!(
            evaluate_arithmetic_source_v1(Arc::from(source), budget(0)).is_err(),
            "{source}"
        );
    }
}
