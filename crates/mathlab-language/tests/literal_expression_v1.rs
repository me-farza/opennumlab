// SPDX-License-Identifier: GPL-3.0-or-later
//! Frozen N01 project-profile predictions, not native MATLAB observations.
//! Fixtures sha256: 3e888f78fa2db8d7400b49dd0a142af2d86e3fb68948c6c5eb0cd9b6f574c598.
//! Every fixture's source-level reference_observation remains not_run.
//! D03's leaf error and S01-S15's evaluated results require the later adapter.

use mathlab_language::SourceSpan;
use mathlab_language::expression::*;
use std::sync::Arc;

fn project_error(source: &str) -> LiteralDiagnosticV1 {
    match lower_literal_expression_v1(Arc::from(source)).unwrap_err() {
        LiteralExpressionErrorV1::Diagnostic(error) => error,
        allocation => panic!("unexpected host allocation failure: {allocation:?}"),
    }
}

fn sp(span: &SourceSpan) -> String {
    format!(
        "{}..{}:L{}..{}",
        span.start, span.end, span.start_line, span.end_line
    )
}

fn snapshot(parsed: &ParsedLiteralExpressionV1, id: LiteralNodeIdV1, records: &mut Vec<String>) {
    let node = parsed.node(id).unwrap();
    let at = sp(&node.span);
    match &node.kind {
        LiteralNodeKindV1::Number { digits_span } => {
            records.push(format!("Number@{at} digits@{}", sp(digits_span)));
        }
        LiteralNodeKindV1::Signed {
            sign,
            sign_span,
            number,
        } => {
            let sign = if *sign == LiteralSignV1::Minus {
                '-'
            } else {
                '+'
            };
            records.push(format!("Signed@{at} sign={sign}@{}", sp(sign_span)));
            snapshot(parsed, *number, records);
        }
        LiteralNodeKindV1::Group { inner } => {
            records.push(format!("Group@{at}"));
            snapshot(parsed, *inner, records);
        }
        LiteralNodeKindV1::Matrix { rows } => {
            let lengths = rows
                .iter()
                .map(|row| row.len().to_string())
                .collect::<Vec<_>>()
                .join(",");
            records.push(format!("Matrix@{at} rows={lengths}"));
            for row in rows {
                for id in row {
                    snapshot(parsed, *id, records);
                }
            }
        }
        LiteralNodeKindV1::Add {
            left,
            operator_span,
            right,
        } => {
            records.push(format!("Add@{at} operator@{}", sp(operator_span)));
            snapshot(parsed, *left, records);
            snapshot(parsed, *right, records);
        }
    }
}

#[test]
fn frozen_positive_sources_parse_and_admit_without_evaluation() {
    let cases = [
        ("S01", "1 + 2"),
        ("S02", "1 + 2 + 3"),
        ("S03", "9007199254740992 + 1 + (-9007199254740992)"),
        ("S04", "9007199254740992 + (1 + (-9007199254740992))"),
        ("S05", "[1,3;2,4]"),
        ("S06", "[1,2,3] + [10;20]"),
        ("S07", "[] + 1"),
        ("S08", "1 + []"),
        ("S09", "-0"),
        ("S10", "-0 + -0"),
        ("S11", "-0 + 0"),
        ("S12", "%λ\n(1 + 2) % exact\n"),
        ("S13", "% c\r\n[1,2]\r\n"),
        ("S14", "+42"),
        ("S15", "[ ]"),
    ];
    for (id, source) in cases {
        let source: Arc<str> = Arc::from(source);
        let ir = lower_literal_expression_v1(Arc::clone(&source))
            .unwrap_or_else(|error| panic!("{id}: {error:?}"));
        assert!(Arc::ptr_eq(&source, ir.source()), "{id}");
        for node in ir.parsed().nodes() {
            assert!(source.is_char_boundary(node.span.start), "{id}");
            assert!(source.is_char_boundary(node.span.end), "{id}");
            assert!(node.span.start < node.span.end, "{id}");
            assert_eq!(node.span.start_line, node.span.end_line, "{id}");
            let _original_slice = &source[node.span.start..node.span.end];
        }
    }
}

#[test]
fn frozen_project_diagnostics_keep_stage_span_and_original_source() {
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
        let error = match lower_literal_expression_v1(Arc::clone(&original)).unwrap_err() {
            LiteralExpressionErrorV1::Diagnostic(error) => error,
            allocation => panic!("{id}: {allocation:?}"),
        };
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
        assert!(Arc::ptr_eq(&original, &error.source), "{id}");
    }
    assert_eq!(
        project_error("[1,2;3]").detail,
        LiteralDiagnosticDetailV1::Matrix(MatrixAdmissionFailureV1::RaggedRows {
            row_lengths: vec![2, 1]
        })
    );
}

#[test]
fn frozen_ast_projections_preserve_groups_signs_operators_and_rows() {
    let cases: &[(&str, &str, &[&str])] = &[
        (
            "S01",
            "1 + 2",
            &[
                "Add@0..5:L1..1 operator@2..3:L1..1",
                "Number@0..1:L1..1 digits@0..1:L1..1",
                "Number@4..5:L1..1 digits@4..5:L1..1",
            ],
        ),
        (
            "S02",
            "1 + 2 + 3",
            &[
                "Add@0..9:L1..1 operator@6..7:L1..1",
                "Add@0..5:L1..1 operator@2..3:L1..1",
                "Number@0..1:L1..1 digits@0..1:L1..1",
                "Number@4..5:L1..1 digits@4..5:L1..1",
                "Number@8..9:L1..1 digits@8..9:L1..1",
            ],
        ),
        (
            "S12",
            "%λ\n(1 + 2) % exact\n",
            &[
                "Group@4..11:L2..2",
                "Add@5..10:L2..2 operator@7..8:L2..2",
                "Number@5..6:L2..2 digits@5..6:L2..2",
                "Number@9..10:L2..2 digits@9..10:L2..2",
            ],
        ),
        (
            "S13",
            "% c\r\n[1,2]\r\n",
            &[
                "Matrix@5..10:L2..2 rows=2",
                "Number@6..7:L2..2 digits@6..7:L2..2",
                "Number@8..9:L2..2 digits@8..9:L2..2",
            ],
        ),
        (
            "S14",
            "+42",
            &[
                "Signed@0..3:L1..1 sign=+@0..1:L1..1",
                "Number@1..3:L1..1 digits@1..3:L1..1",
            ],
        ),
    ];
    for (id, source, expected) in cases {
        let parsed = parse_literal_expression_v1(Arc::from(*source)).unwrap();
        let mut actual = Vec::new();
        snapshot(&parsed, parsed.root_id(), &mut actual);
        let actual: Vec<&str> = actual.iter().map(String::as_str).collect();
        assert_eq!(actual.as_slice(), *expected, "{id}");
    }
}

fn assert_resource(
    error: LiteralDiagnosticV1,
    name: LiteralResourceV1,
    limit: usize,
    attempted_count: usize,
    start: usize,
    end: usize,
) {
    assert_eq!(error.code, LiteralDiagnosticCodeV1::ResourceLimit);
    assert_eq!(
        error.span,
        SourceSpan {
            start,
            end,
            start_line: 1,
            end_line: 1
        }
    );
    assert_eq!(
        error.detail,
        LiteralDiagnosticDetailV1::Resource {
            name,
            limit: Some(limit),
            attempted_count
        }
    );
}

#[test]
fn frozen_resource_witnesses_stop_at_the_rejected_introducing_token() {
    assert_eq!(
        project_error(&" ".repeat(4096)).code,
        LiteralDiagnosticCodeV1::ExpectedExpression
    );
    let maximum_source = format!("1%{}", " ".repeat(4094));
    lower_literal_expression_v1(Arc::from(maximum_source.as_str())).unwrap();
    assert_resource(
        project_error(&" ".repeat(4097)),
        LiteralResourceV1::SourceBytes,
        4096,
        4097,
        0,
        0,
    );
    let depth32 = format!("{}1{}", "(".repeat(32), ")".repeat(32));
    lower_literal_expression_v1(Arc::from(depth32.as_str())).unwrap();
    let depth33 = format!("{}1{}", "(".repeat(33), ")".repeat(33));
    assert_resource(
        project_error(&depth33),
        LiteralResourceV1::OpenDelimiters,
        32,
        33,
        32,
        33,
    );
    let numbers256 = vec!["1"; 256].join("+");
    lower_literal_expression_v1(Arc::from(numbers256.as_str())).unwrap();
    let numbers257 = vec!["1"; 257].join("+");
    assert_resource(
        project_error(&numbers257),
        LiteralResourceV1::NumberNodes,
        256,
        257,
        512,
        513,
    );
    let witness = vec![depth32.as_str(); 31].join("+");
    assert_eq!(witness.len(), 2045);
    assert_resource(
        project_error(&witness),
        LiteralResourceV1::AstNodes,
        1024,
        1025,
        1984,
        1985,
    );
    let exactly_1024 = format!("{}+(((1)))", vec![depth32.as_str(); 30].join("+"));
    let parsed = parse_literal_expression_v1(Arc::from(exactly_1024.as_str())).unwrap();
    assert_eq!(parsed.nodes().len(), 1024);
}

#[test]
fn matrix_lowering_preserves_column_major_bits_and_empty_extents() {
    let ir = lower_literal_expression_v1(Arc::from("[1,3;2,4]")).unwrap();
    assert_eq!(
        ir.literal(ir.parsed().root_id()),
        Some(&LoweredLiteralV1::Matrix {
            shape: [2, 2],
            column_major_bits: vec![
                1.0_f64.to_bits(),
                2.0_f64.to_bits(),
                3.0_f64.to_bits(),
                4.0_f64.to_bits()
            ],
        })
    );
    for source in ["[]", "[ ]"] {
        let ir = lower_literal_expression_v1(Arc::from(source)).unwrap();
        assert_eq!(
            ir.literal(ir.parsed().root_id()),
            Some(&LoweredLiteralV1::Matrix {
                shape: [0, 0],
                column_major_bits: vec![]
            })
        );
    }
    let ir = lower_literal_expression_v1(Arc::from("[-0,+0;-2,3]")).unwrap();
    assert_eq!(
        ir.literal(ir.parsed().root_id()),
        Some(&LoweredLiteralV1::Matrix {
            shape: [2, 2],
            column_major_bits: vec![
                0x8000000000000000,
                (-2.0_f64).to_bits(),
                0,
                3.0_f64.to_bits()
            ],
        })
    );
}

#[test]
fn exact_integer_and_explicit_sign_policy_are_bit_exact_constructor_inputs() {
    for (source, bits) in [
        ("-0", 0x8000000000000000),
        ("+0", 0),
        ("0000000000000000", 0),
        ("9007199254740992", 0x4340000000000000),
        ("-9007199254740992", 0xc340000000000000),
        ("+42", 0x4045000000000000),
    ] {
        let ir = lower_literal_expression_v1(Arc::from(source)).unwrap();
        assert_eq!(
            ir.literal(ir.parsed().root_id()),
            Some(&LoweredLiteralV1::Scalar { bits }),
            "{source}"
        );
    }
    assert_eq!(
        project_error("9007199254740993").detail,
        LiteralDiagnosticDetailV1::Literal(LiteralAdmissionFailureV1::Magnitude {
            actual: 9007199254740993,
            limit: 9007199254740992
        })
    );
    assert_eq!(
        project_error("00000000000000000").detail,
        LiteralDiagnosticDetailV1::Literal(LiteralAdmissionFailureV1::TooManyDigits {
            actual: 17,
            limit: 16
        })
    );
}

#[test]
fn diagnostic_pass_order_cannot_be_replaced_by_early_value_checks() {
    // Whole-source framing wins over a lexical failure on the first code line.
    let error = project_error("@\n2");
    assert_eq!(error.code, LiteralDiagnosticCodeV1::UnsupportedSourceForm);
    assert_eq!(error.span.start, 2);
    // Whole-region lexing wins over a parse error at an earlier token.
    let error = project_error("--1 π");
    assert_eq!(error.code, LiteralDiagnosticCodeV1::UnsupportedToken);
    assert_eq!(&error.source[error.span.start..error.span.end], "π");
    // Complete parsing wins over an earlier inadmissible literal.
    assert_eq!(
        project_error("9007199254740993 +").code,
        LiteralDiagnosticCodeV1::ExpectedExpression
    );
    // Every literal is admitted before any geometry is checked.
    assert_eq!(
        project_error("[1,2;3] + 9007199254740993").code,
        LiteralDiagnosticCodeV1::LiteralNotAdmitted
    );
    let error = project_error("9007199254740993 + 00000000000000000");
    assert_eq!(error.span.start, 0);
    let error = project_error("[1,2;3] + [1;2,3]");
    assert_eq!(error.span.start, 0);
    assert_eq!(error.span.end, 7);
}

#[test]
fn matrix_dimension_boundaries_apply_after_complete_parsing() {
    let row = vec!["1"; 16].join(",");
    let matrix = format!("[{}]", vec![row.as_str(); 16].join(";"));
    let ir = lower_literal_expression_v1(Arc::from(matrix.as_str())).unwrap();
    match ir.literal(ir.parsed().root_id()).unwrap() {
        LoweredLiteralV1::Matrix {
            shape,
            column_major_bits,
        } => {
            assert_eq!(*shape, [16, 16]);
            assert_eq!(column_major_bits.len(), 256);
        }
        scalar => panic!("expected matrix, got {scalar:?}"),
    }
    for (source, axis) in [
        (format!("[{}]", vec!["1"; 17].join(",")), 1),
        (format!("[{}]", vec!["1"; 17].join(";")), 0),
    ] {
        let error = project_error(&source);
        assert_eq!(error.span.end, source.len());
        assert_eq!(
            error.detail,
            LiteralDiagnosticDetailV1::Matrix(MatrixAdmissionFailureV1::Dimension {
                axis,
                actual: 17,
                limit: 16
            })
        );
    }
}

#[test]
fn framing_and_token_boundaries_use_original_utf8_bytes() {
    for source in ["% only", "% λ\n\t\r\n", " \t\n"] {
        let error = project_error(source);
        assert_eq!(error.code, LiteralDiagnosticCodeV1::ExpectedExpression);
        assert_eq!(error.span.start, source.len());
        assert_eq!(
            error.span.start_line,
            source.bytes().filter(|&byte| byte == b'\n').count() + 1
        );
    }
    let error = project_error("\u{feff}1");
    assert_eq!(error.code, LiteralDiagnosticCodeV1::UnsupportedSourceForm);
    assert_eq!(error.span.end, 3);
    for source in [" \t%{anything", "  %}anything"] {
        let error = project_error(source);
        assert_eq!(
            error.detail,
            LiteralDiagnosticDetailV1::SourceForm(UnsupportedSourceFormV1::BlockCommentMarker)
        );
        assert_eq!(error.span.end - error.span.start, 2);
    }
    let error = project_error("1 + alpha_123");
    assert_eq!(error.code, LiteralDiagnosticCodeV1::UnsupportedIdentifier);
    assert_eq!(&error.source[error.span.start..error.span.end], "alpha_123");
    let error = project_error("1 + 🧮");
    assert_eq!(error.span.start, 4);
    assert_eq!(error.span.end, 8);
    let error = project_error(" 1 + \t % trailing\n% λ\n");
    assert_eq!(error.code, LiteralDiagnosticCodeV1::ExpectedExpression);
    assert_eq!(error.span.start, 4);
    assert_eq!(error.span.end, 4);
    assert_eq!(error.span.start_line, 1);
}

#[test]
fn grammar_does_not_silently_expand_beyond_the_profile() {
    for source in [
        "1 - 2", "[1 2]", "-[1,2]", "--1", "[1+2]", "[;]", "[1;]", "(1) 2", "1;", "( )",
    ] {
        assert_eq!(
            project_error(source).code,
            LiteralDiagnosticCodeV1::UnexpectedToken,
            "{source}"
        );
    }
    for source in ["+", "-", "[", "[1,", "[1;", "("] {
        assert_eq!(
            project_error(source).code,
            LiteralDiagnosticCodeV1::ExpectedExpression,
            "{source}"
        );
    }
    for source in ["(1", "[1", "[1,2"] {
        assert_eq!(
            project_error(source).code,
            LiteralDiagnosticCodeV1::UnexpectedEof,
            "{source}"
        );
    }
    for source in ["1++2", "1 + - 2", "[+ 1,- 2]", "( [1] + [] )"] {
        lower_literal_expression_v1(Arc::from(source)).unwrap();
    }
}

#[test]
fn d03_remains_an_unevaluated_owned_leaf_boundary() {
    let ir = lower_literal_expression_v1(Arc::from("[1;2] + [3;4;5]")).unwrap();
    let LiteralNodeKindV1::Add {
        left,
        operator_span,
        right,
    } = &ir.parsed().root().kind
    else {
        panic!("expected Add");
    };
    assert_eq!(operator_span.start, 6);
    assert!(matches!(
        ir.literal(*left),
        Some(LoweredLiteralV1::Matrix { shape: [2, 1], .. })
    ));
    assert!(matches!(
        ir.literal(*right),
        Some(LoweredLiteralV1::Matrix { shape: [3, 1], .. })
    ));
    assert!(ir.literal(ir.parsed().root_id()).is_none());
}
