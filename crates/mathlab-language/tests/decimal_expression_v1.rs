// SPDX-License-Identifier: GPL-3.0-or-later
//! Eighteen fixed source/value observations from accepted native reference batch
//! 20261016T024523895711Z; no runtime is invoked by writing these test sources.
//! Decimal/exponent edge tests below are original project expectations, with no
//! additional MATLAB observation claimed. Existing v1 test files stay unchanged.

use mathlab_language::decimal_expression::*;
use mathlab_language::expression::*;
use mathlab_language::literal_evaluation::evaluate_literal_source_v1;
use std::sync::Arc;

fn error(source: &str) -> DecimalDiagnosticV1 {
    match evaluate_decimal_source_v1(Arc::from(source)).unwrap_err() {
        DecimalExpressionErrorV1::Diagnostic(error) => error,
        host => panic!("unexpected host allocation failure: {host:?}"),
    }
}

#[test]
fn eighteen_exact_original_sources_match_the_frozen_native_records() {
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
        ("D04", "9007199254740993", [1, 1], &[0x4340000000000000]),
        ("D05", "0.5", [1, 1], &[0x3fe0000000000000]),
        ("D18", "00000000000000000", [1, 1], &[0x0000000000000000]),
    ];
    for (id, source, shape, bits) in cases {
        let source: Arc<str> = Arc::from(*source);
        let parsed = parse_decimal_expression_v1(Arc::clone(&source)).unwrap();
        let root_span = (*parsed.ast().root().span).clone();
        let ast = format!("{:?}", parsed.ast());
        let ir = admit_decimal_expression_v1(parsed).unwrap();
        assert!(Arc::ptr_eq(&source, ir.source()), "{id}");
        let value =
            evaluate_decimal_expression_v1(ir).unwrap_or_else(|error| panic!("{id}: {error:?}"));
        assert_eq!(value.class(), "double", "{id}");
        assert_eq!(value.array().shape(), *shape, "{id}");
        let actual: Vec<_> = value
            .array()
            .as_slice()
            .iter()
            .map(|value| value.to_bits())
            .collect();
        assert_eq!(actual.as_slice(), *bits, "{id}");
        assert!(Arc::ptr_eq(&source, value.source()), "{id}");
        assert_eq!(value.root_span(), &root_span, "{id}");
        assert_eq!(format!("{:?}", value.ast()), ast, "{id}");
        let (parsed, array) = value.into_parts();
        assert!(Arc::ptr_eq(&source, parsed.source()), "{id}");
        assert_eq!(array.shape(), *shape, "{id}");
    }
}

#[test]
fn the_three_new_native_targets_remain_rejected_by_the_explicit_legacy_profile() {
    for (source, code) in [
        (
            "9007199254740993",
            LiteralDiagnosticCodeV1::LiteralNotAdmitted,
        ),
        ("0.5", LiteralDiagnosticCodeV1::UnsupportedToken),
        (
            "00000000000000000",
            LiteralDiagnosticCodeV1::LiteralNotAdmitted,
        ),
    ] {
        let LiteralExpressionErrorV1::Diagnostic(rejected) =
            evaluate_literal_source_v1(Arc::from(source)).unwrap_err()
        else {
            panic!("expected project rejection");
        };
        assert_eq!(rejected.code, code);
        evaluate_decimal_source_v1(Arc::from(source)).unwrap();
    }
}

#[test]
fn decimal_significand_and_exponent_forms_have_fixed_project_bit_expectations() {
    for (source, bits) in [
        (".5", 0x3fe0000000000000),
        ("1.", 0x3ff0000000000000),
        ("1.e2", 0x4059000000000000),
        (".5E+1", 0x4014000000000000),
        ("001.2500e-1", 0x3fc0000000000000),
        ("1e2+3", 0x4059c00000000000),
        ("- 0e+99", 0x8000000000000000),
        ("+ .0", 0),
        (
            "1.00000000000000011102230246251565404236316680908203125",
            0x3ff0000000000000,
        ),
        (
            "1.00000000000000011102230246251565404236316680908203126",
            0x3ff0000000000001,
        ),
        ("5e-324", 1),
        ("2.2250738585072014e-308", 0x0010000000000000),
        ("1.7976931348623157e308", 0x7fefffffffffffff),
        ("1e10000", 0x7ff0000000000000),
        ("-1e-10000", 0x8000000000000000),
    ] {
        let value = evaluate_decimal_source_v1(Arc::from(source)).unwrap();
        assert_eq!(value.array().shape(), [1, 1]);
        assert_eq!(value.array().as_slice()[0].to_bits(), bits, "{source}");
    }
}

#[test]
fn malformed_numeric_components_are_lexical_and_source_spanned() {
    for (source, reason, end) in [
        (".", MalformedDecimalV1::MissingSignificandDigits, 1),
        (".e1", MalformedDecimalV1::MissingSignificandDigits, 1),
        ("1e", MalformedDecimalV1::MissingExponentDigits, 2),
        ("1e+", MalformedDecimalV1::MissingExponentDigits, 3),
        ("1e-", MalformedDecimalV1::MissingExponentDigits, 3),
        ("1e+-2", MalformedDecimalV1::MissingExponentDigits, 3),
        ("1..2", MalformedDecimalV1::RepeatedComponent, 4),
        ("1.2.3", MalformedDecimalV1::RepeatedComponent, 5),
        ("1e2e3", MalformedDecimalV1::RepeatedComponent, 5),
    ] {
        let error = error(source);
        assert_eq!(
            error.code,
            DecimalDiagnosticCodeV1::MalformedNumber,
            "{source}"
        );
        assert_eq!(error.stage, LiteralStageV1::Lex, "{source}");
        assert_eq!(error.span.start, 0, "{source}");
        assert_eq!(error.span.end, end, "{source}");
        assert_eq!(
            error.detail,
            DecimalDiagnosticDetailV1::Malformed(reason),
            "{source}"
        );
    }
    for source in ["NaN", "Inf", "0x10", "1D2", "1_000"] {
        assert_eq!(
            error(source).code,
            DecimalDiagnosticCodeV1::Profile(LiteralDiagnosticCodeV1::UnsupportedIdentifier)
        );
    }
}

#[test]
fn token_projection_and_debug_output_never_call_decimal_tokens_digit_only() {
    let source: Arc<str> = Arc::from("%λ\n( - .50e+1 + 2. ) % tail\n");
    let parsed = parse_decimal_expression_v1(Arc::clone(&source)).unwrap();
    assert_eq!(parsed.ast().root().span.start, 4);
    assert_eq!(parsed.ast().root().span.start_line, 2);
    let tokens: Vec<_> = parsed
        .ast()
        .nodes()
        .filter_map(|node| match node.kind {
            DecimalNodeKindV1::Number { literal_span } => {
                Some(&source[literal_span.start..literal_span.end])
            }
            _ => None,
        })
        .collect();
    assert_eq!(tokens, [".50e+1", "2."]);
    assert!(!format!("{parsed:?}").contains("digits_span"));
    let ir = admit_decimal_expression_v1(parsed).unwrap();
    assert!(!format!("{ir:?}").contains("digits_span"));
    let value = evaluate_decimal_expression_v1(ir).unwrap();
    assert!(!format!("{value:?}").contains("digits_span"));
    let syntax = error("[1.5 2.5]");
    assert_eq!(
        syntax.detail,
        DecimalDiagnosticDetailV1::Syntax {
            expected: ExpectedSyntaxV1::MatrixSeparatorOrClose,
            found: Some(DecimalTokenKindV1::Number),
        }
    );
    assert_eq!(&syntax.source[syntax.span.start..syntax.span.end], "2.5");
}

#[test]
fn source_and_numeric_resource_boundaries_have_no_sixteen_digit_decimal_cap() {
    let full_source = "0".repeat(MAX_SOURCE_BYTES_V1);
    let value = evaluate_decimal_source_v1(Arc::from(full_source.as_str())).unwrap();
    assert_eq!(value.array().as_slice()[0].to_bits(), 0);
    let overflow = error(&"0".repeat(MAX_SOURCE_BYTES_V1 + 1));
    assert_eq!(overflow.span.start, 0);
    assert_eq!(overflow.span.end, 0);
    assert_eq!(
        overflow.detail,
        DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::Resource {
            name: LiteralResourceV1::SourceBytes,
            limit: Some(4096),
            attempted_count: 4097,
        })
    );
    let admitted = vec![".5"; 256].join("+");
    assert_eq!(
        evaluate_decimal_source_v1(Arc::from(admitted.as_str()))
            .unwrap()
            .array()
            .as_slice()[0]
            .to_bits(),
        0x4060000000000000
    );
    let rejected = error(&vec![".5"; 257].join("+"));
    assert_eq!(rejected.span.start, 768);
    assert_eq!(
        rejected.detail,
        DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::Resource {
            name: LiteralResourceV1::NumberNodes,
            limit: Some(256),
            attempted_count: 257,
        })
    );
    let term = format!("{}.5{}", "(".repeat(32), ")".repeat(32));
    evaluate_decimal_source_v1(Arc::from(term.as_str())).unwrap();
    let depth = error(&format!("{}.5{}", "(".repeat(33), ")".repeat(33)));
    assert_eq!(depth.span.start, 32);
    let witness = error(&vec![term.as_str(); 31].join("+"));
    assert_eq!(witness.span.start, 2014);
    assert_eq!(
        witness.detail,
        DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::Resource {
            name: LiteralResourceV1::AstNodes,
            limit: Some(1024),
            attempted_count: 1025,
        })
    );
}

#[test]
fn matrix_geometry_and_column_major_lowering_reuse_the_original_policy() {
    let value = evaluate_decimal_source_v1(Arc::from("[1.5,3e0;-2.25,.5]")).unwrap();
    assert_eq!(value.array().shape(), [2, 2]);
    let bits: Vec<_> = value
        .array()
        .as_slice()
        .iter()
        .map(|value| value.to_bits())
        .collect();
    assert_eq!(
        bits,
        [
            0x3ff8000000000000,
            0xc002000000000000,
            0x4008000000000000,
            0x3fe0000000000000
        ]
    );
    let row = vec![".5"; 16].join(",");
    let matrix = format!("[{}]", vec![row.as_str(); 16].join(";"));
    let value = evaluate_decimal_source_v1(Arc::from(matrix.as_str())).unwrap();
    assert_eq!(value.array().shape(), [16, 16]);
    assert_eq!(value.array().as_slice().len(), 256);
    assert_eq!(
        error(&format!("[{}]", vec![".5"; 17].join(","))).code,
        DecimalDiagnosticCodeV1::Profile(LiteralDiagnosticCodeV1::MatrixNotAdmitted)
    );
    assert_eq!(
        error("[.5,1.;2.]").code,
        DecimalDiagnosticCodeV1::Profile(LiteralDiagnosticCodeV1::MatrixNotAdmitted)
    );
}

#[test]
fn framing_complete_lexing_parsing_and_leaf_order_still_apply() {
    assert_eq!(
        error("1e+\n2").code,
        DecimalDiagnosticCodeV1::Profile(LiteralDiagnosticCodeV1::UnsupportedSourceForm)
    );
    assert_eq!(
        error("--1 .e2").code,
        DecimalDiagnosticCodeV1::MalformedNumber
    );
    assert_eq!(
        error("1e10000 +").code,
        DecimalDiagnosticCodeV1::Profile(LiteralDiagnosticCodeV1::ExpectedExpression)
    );
    assert_eq!(
        error("([.5;1.] + [2.;3.;4.]) + [.5,1.;2.]").code,
        DecimalDiagnosticCodeV1::Profile(LiteralDiagnosticCodeV1::MatrixNotAdmitted)
    );
    let source = "([.5;1.] + [2.;3.;4.]) + ([.5,1.] + [2.,3.,4.])";
    let first = error(source);
    assert_eq!(
        first.code,
        DecimalDiagnosticCodeV1::Profile(LiteralDiagnosticCodeV1::ArrayOperation)
    );
    assert_eq!(first.span.start, source.find('+').unwrap());
}

#[test]
fn nine_other_native_valid_sources_remain_explicitly_outside_this_profile() {
    let cases = [
        ("D06", "[1 2]", LiteralDiagnosticCodeV1::UnexpectedToken),
        ("D07", "1;", LiteralDiagnosticCodeV1::UnexpectedToken),
        (
            "D08",
            "1\n2",
            LiteralDiagnosticCodeV1::UnsupportedSourceForm,
        ),
        ("D10", "--1", LiteralDiagnosticCodeV1::UnexpectedToken),
        ("D11", "-[1,2]", LiteralDiagnosticCodeV1::UnexpectedToken),
        ("D12", "[1,]", LiteralDiagnosticCodeV1::UnexpectedToken),
        (
            "D14",
            "%{\n1\n%}",
            LiteralDiagnosticCodeV1::UnsupportedSourceForm,
        ),
        (
            "D15",
            "1\r2",
            LiteralDiagnosticCodeV1::UnsupportedSourceForm,
        ),
        ("D17", "", LiteralDiagnosticCodeV1::ExpectedExpression),
    ];
    for (id, source, code) in cases {
        assert_eq!(
            error(source).code,
            DecimalDiagnosticCodeV1::Profile(code),
            "{id}"
        );
    }
}
