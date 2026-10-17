// SPDX-License-Identifier: GPL-3.0-or-later
use mathlab_language::arithmetic_expression::parse_arithmetic_expression_v1;
use mathlab_language::decimal_expression::parse_decimal_expression_v1;
use mathlab_language::expression::parse_literal_expression_v1;
use mathlab_language::session_expression::*;
use std::sync::Arc;

fn lower(source: &str) -> Result<SessionExpressionIrV1, SessionExpressionErrorV1> {
    admit_session_expression_v1(parse_session_expression_v1(Arc::from(source))?)
}

#[test]
fn original_source_spans_survive_assignment_comments_and_framing() {
    let source = "% original comment\n  A=missing+1; % tail\n";
    let error =
        evaluate_session_expression_v1(lower(source).unwrap(), |_| None, || false).unwrap_err();
    let start = source.find("missing").unwrap();
    assert_eq!(error.code(), Some("UnknownBinding"));
    assert_eq!((error.span().start, error.span().end), (start, start + 7));
    assert_eq!(error.span().start_line, 2);
    assert_eq!(error.original_source().as_ref(), source);
    for rejected in [
        "A=1;B=2",
        "A=1\nB=2",
        "A(1)=2",
        "A=1,",
        "A=1;;",
        "A=[B]",
        "",
        "% comment",
    ] {
        assert!(lower(rejected).is_err(), "{rejected}");
    }
}

#[test]
fn session_only_tokens_do_not_escape_into_existing_profiles() {
    for source in ["A", "A=1", "_a=1", "if=1"] {
        assert!(parse_literal_expression_v1(Arc::from(source)).is_err());
        assert!(parse_decimal_expression_v1(Arc::from(source)).is_err());
        assert_eq!(
            parse_arithmetic_expression_v1(Arc::from(source))
                .unwrap_err()
                .code(),
            Some("UnsupportedIdentifier")
        );
    }
    let assigned =
        evaluate_session_expression_v1(lower("A=1;").unwrap(), |_| None, || false).unwrap();
    assert!(assigned.suppressed);
    assert!(matches!(
        assigned.effect,
        SessionStatementEffectV1::Assignment { .. }
    ));
    let a = SessionValueV1::try_from_col_major([1, 1], &[3.0]).unwrap();
    let bare = evaluate_session_expression_v1(lower("A").unwrap(), |_| Some(&a), || false).unwrap();
    assert!(matches!(
        bare.effect,
        SessionStatementEffectV1::Display { .. }
    ));
    let grouped =
        evaluate_session_expression_v1(lower("(A)").unwrap(), |_| Some(&a), || false).unwrap();
    assert!(matches!(grouped.effect, SessionStatementEffectV1::Answer));
}

#[test]
fn identifier_statement_and_array_resource_caps_are_admitted_before_lookup() {
    let name = "a".repeat(2048);
    assert!(lower(&format!("{name}=1")).is_ok());
    assert!(matches!(
        lower(&format!("{}=1", "a".repeat(2049))),
        Err(SessionExpressionErrorV1::InvalidIdentifier { .. })
    ));
    for keyword in ["if", "while", "function", "end", "classdef"] {
        assert!(matches!(
            lower(&format!("{keyword}=1")),
            Err(SessionExpressionErrorV1::InvalidIdentifier { .. })
        ));
    }
    assert!(lower(&" ".repeat(4097)).is_err());
    assert!(lower(&format!("{}A", "A+".repeat(512))).is_err());
    assert!(lower("A=[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17]").is_err());
    assert!(lower(&format!("{}1", "(".repeat(33))).is_err());
}

#[test]
fn resolver_owns_copies_and_task_checkpoints_can_cancel() {
    let a = SessionValueV1::try_from_col_major([1, 2], &[1.0, 2.0]).unwrap();
    let negated =
        evaluate_session_expression_v1(lower("-A").unwrap(), |_| Some(&a), || false).unwrap();
    assert_eq!(a.as_slice(), &[1.0, 2.0]);
    assert_eq!(negated.value.as_slice(), &[-1.0, -2.0]);
    let mut names = Vec::new();
    let sum = evaluate_session_expression_v1(
        lower("A+A").unwrap(),
        |name| {
            names.push(name.to_owned());
            Some(&a)
        },
        || false,
    )
    .unwrap();
    assert_eq!(names, ["A", "A"]);
    assert_eq!(sum.value.as_slice(), &[2.0, 4.0]);
    let mut polls = 0;
    let error = evaluate_session_expression_v1(
        lower("A+A").unwrap(),
        |_| Some(&a),
        || {
            polls += 1;
            polls == 4
        },
    )
    .unwrap_err();
    assert!(matches!(error, SessionExpressionErrorV1::Cancelled { .. }));
    assert_eq!(polls, 4);
    assert_eq!(a.as_slice(), &[1.0, 2.0]);
    // The eighth checkpoint follows the actual Add kernel for this small tree.
    let mut polls = 0;
    let error = evaluate_session_expression_v1(
        lower("1+2").unwrap(),
        |_| None,
        || {
            polls += 1;
            polls == 8
        },
    )
    .unwrap_err();
    assert!(matches!(error, SessionExpressionErrorV1::Cancelled { .. }));
    assert_eq!(polls, 8);
}
