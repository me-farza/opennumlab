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
        "A(1)=",
        "A=1,",
        "A=1;;",
        "A=[B;]",
        "",
        "% comment",
    ] {
        assert!(lower(rejected).is_err(), "{rejected}");
    }
}

#[test]
fn session_only_tokens_do_not_escape_into_existing_profiles() {
    for source in ["A", "A=1", "_a=1", "if=1", "zeros(2)", "zeros (2,3)", "zeros()", "ones(2)", "ones (2,3)", "ones()", "eye(2)", "eye (2,3)", "eye()", "size(7)", "numel([])", "size", "numel", "reshape(1,1,1)", "A(1)", "A(1,2)"] {
        assert!(parse_literal_expression_v1(Arc::from(source)).is_err());
        assert!(parse_decimal_expression_v1(Arc::from(source)).is_err());
        assert_eq!(
            parse_arithmetic_expression_v1(Arc::from(source))
                .unwrap_err()
                .code(),
            Some("UnsupportedIdentifier")
        );
    }
    for source in ["1:3","1:2:5"] {
        assert_eq!(SessionExpressionErrorV1::from(parse_literal_expression_v1(Arc::from(source)).unwrap_err()).code(),Some("UnsupportedToken"));
        assert_eq!(SessionExpressionErrorV1::from(parse_decimal_expression_v1(Arc::from(source)).unwrap_err()).code(),Some("UnsupportedToken"));
        assert_eq!(parse_arithmetic_expression_v1(Arc::from(source)).unwrap_err().code(),Some("UnsupportedToken"));
    }
    // Session calls retain the same original UTF-8 framing and resource limits.
    let source = "% π header\n  A=zeros (missing,3); % tail\n";
    let error = evaluate_session_expression_v1(lower(source).unwrap(), |_| None, || false).unwrap_err();
    let start = source.find("missing").unwrap();
    assert_eq!(error.code(), Some("UnknownBinding"));
    assert_eq!((error.span().start, error.span().end, error.span().start_line), (start, start + 7, 2));
    assert_eq!(error.original_source().as_ref(), source);
    for source in ["zeros(2,)", "zeros(,2)", "zeros(2;3)", "zeros(2)(1)",
                   "zeros(2)=", "zeros(2,'single')", "[a,b]=zeros(2)", "zeros([n;])", "ones(2,'single')", "ones(2)(1)", "eye(2,'single')", "eye(2)(1)", "size([1,2])(1)", "[m,n]=size(1)"] {
        assert!(lower(source).is_err(), "{source}");
    }
    for source in ["ones(2,'single')", "eye(2,'single')"] {
        let error = lower(source).unwrap_err();
        assert_eq!(error.code(), Some("UnsupportedToken"));
        assert_eq!(&source[error.span().start..error.span().end], "'");
    }
    assert!(lower(&format!("{}1{}", "zeros(".repeat(32), ")".repeat(32))).is_ok());
    assert!(lower(&format!("{}1{}", "zeros(".repeat(33), ")".repeat(33))).is_err());
    assert!(lower(&format!("zeros({})", vec!["1"; 257].join(","))).is_err());
    // Calls and argument slots count toward the existing AST admission cap.
    assert!(lower(&vec!["zeros(1)"; 205].join("+")).is_ok());
    let calls = vec!["zeros(1)"; 206].join("+");
    assert!(calls.len() < 4096);
    assert!(lower(&calls).is_err());
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


#[test]
fn zeros_values_empty_axes_and_expression_composition() {
    let n = SessionValueV1::try_from_col_major([1, 1], &[2.0]).unwrap();
    for (source, shape) in [
        ("zeros(1)", [1,1]), ("zeros(2)", [2,2]), ("zeros(2,3)", [2,3]),
        ("zeros(0)", [0,0]), ("zeros(0,3)", [0,3]), ("zeros(3,0)", [3,0]),
        ("zeros(-0,2)", [0,2]), ("zeros(2.0,3e0)", [2,3]),
        ("zeros(-1,2)", [0,2]), ("zeros(-1)", [0,0]), ("zeros(2,-3)", [2,0]),
        ("zeros(-2,-3)", [0,0]), ("zeros(-1e300,2)", [0,2]),
        ("zeros(1+1,3)", [2,3]), ("zeros(n,1)", [2,1]),
        ("zeros(zeros(1),2)", [0,2]), ("zeros(2,0)*zeros(0,3)", [2,3]),
        ("-2*zeros(5,0)", [5,0]), ("zeros(2,3).'", [3,2]),
        ("zeros(16)", [16,16]), ("zeros([2],[3])", [2,3]),
        ("zeros(ones(1),2)", [1,2]),
        ("+(zeros (2,3))", [2,3]),
    ] {
        let result = evaluate_session_expression_v1(lower(source).unwrap(),
            |name| if name == "n" {Some(&n)} else {None}, || false).unwrap();
        assert_eq!(result.value.shape(), shape, "{source}");
        assert_eq!(result.value.as_slice().len(), (shape[0]*shape[1]) as usize);
        assert!(result.value.as_slice().iter().all(|v| v.to_bits() == 0), "{source}");
    }
    for (source, shape) in [
        ("ones(1)", [1,1]), ("ones(2,3)", [2,3]), ("ones(0)", [0,0]),
        ("ones(0,3)", [0,3]), ("ones(3,0)", [3,0]), ("ones(-2,3)", [0,3]),
        ("ones(16)", [16,16]), ("ones(zeros(1),2)", [0,2]), ("ones(n,3).'", [3,2]),
    ] {
        let result = evaluate_session_expression_v1(lower(source).unwrap(),
            |name| if name=="n" {Some(&n)} else {None}, || false).unwrap();
        assert_eq!(result.value.class(), "double");
        assert_eq!(result.value.shape(), shape, "{source}");
        assert_eq!(result.value.as_slice().len(), (shape[0]*shape[1]) as usize);
        assert!(result.value.as_slice().iter().all(|v| v.to_bits()==0x3ff0000000000000), "{source}");
    }
    let one = 0x3ff0000000000000_u64;
    for (source, shape, expected) in [
        ("eye(1)",[1,1],vec![one]), ("eye(2)",[2,2],vec![one,0,0,one]),
        ("eye(2,3)",[2,3],vec![one,0,0,one,0,0]),
        ("eye(3,2)",[3,2],vec![one,0,0,0,one,0]),
        ("eye(0)",[0,0],vec![]), ("eye(0,3)",[0,3],vec![]), ("eye(3,0)",[3,0],vec![]),
        ("eye(-2,3)",[0,3],vec![]), ("eye(3,-2)",[3,0],vec![]), ("eye(-2)",[0,0],vec![]),
        ("eye(ones(1)+1)",[2,2],vec![one,0,0,one]),
    ] {
        let result = evaluate_session_expression_v1(lower(source).unwrap(), |_| None, || false).unwrap();
        assert_eq!(result.value.class(), "double");
        assert_eq!(result.value.shape(),shape,"{source}");
        assert_eq!(result.value.as_slice().iter().map(|v| v.to_bits()).collect::<Vec<_>>(),expected,"{source}");
    }
    let maximum = evaluate_session_expression_v1(lower("eye(16)").unwrap(), |_| None, || false).unwrap().value;
    assert_eq!(maximum.shape(),[16,16]);
    assert_eq!(maximum.as_slice().len(),256);
    assert!(maximum.as_slice().iter().all(|v| [0,one].contains(&v.to_bits())));
    assert_eq!(maximum.as_slice().iter().enumerate().filter_map(|(i,v)| (v.to_bits()==one).then_some(i)).collect::<Vec<_>>(),
        [0_usize,17,34,51,68,85,102,119,136,153,170,187,204,221,238,255]);
    let identity_product = evaluate_session_expression_v1(lower("eye(2,3)*[1;2;3]").unwrap(), |_| None, || false).unwrap().value;
    assert_eq!(identity_product.shape(),[2,1]);
    assert_eq!(identity_product.as_slice(),&[1.0,2.0]);
    let ones_sum = evaluate_session_expression_v1(lower("ones(2,3)+[1,2,3;4,5,6]").unwrap(), |_| None, || false).unwrap();
    assert_eq!(ones_sum.value.shape(), [2,3]);
    assert_eq!(ones_sum.value.as_slice(), &[2.0,5.0,3.0,6.0,4.0,7.0]);
    let product = evaluate_session_expression_v1(lower("ones(2,3)*ones(3,2)").unwrap(), |_| None, || false).unwrap();
    assert_eq!(product.value.shape(), [2,2]);
    assert_eq!(product.value.as_slice().iter().map(|v| v.to_bits()).collect::<Vec<_>>(), [0x4008000000000000_u64;4]);
    let sum = evaluate_session_expression_v1(lower("zeros(3,2)+[1,2;3,4;5,6]").unwrap(), |_| None, || false).unwrap();
    assert_eq!(sum.value.as_slice(), &[1.0,3.0,5.0,2.0,4.0,6.0]);
    let negated = evaluate_session_expression_v1(lower("-zeros(1)").unwrap(), |_| None, || false).unwrap();
    assert_eq!(negated.value.as_slice()[0].to_bits(), 1_u64 << 63);
    assert_eq!(n.as_slice(), &[2.0]);
}

#[test]
fn zeros_dimension_errors_retain_reason_shape_bits_and_argument_span() {
    use ConstructorDimensionReasonV1::*;
    for (source, subject, reason, index, shape, bits) in [
        ("zeros([2,3])", "[2,3]", NonScalar, 0, [1,2], None),
        ("zeros([])", "[]", NonScalar, 0, [0,0], None),
        ("ones([2,3])", "[2,3]", NonScalar, 0, [1,2], None),
        ("eye([2,3])", "[2,3]", NonScalar, 0, [1,2], None),
        ("eye(-1.5)", "-1.5", Fractional, 0, [1,1], Some((-1.5_f64).to_bits())),
        ("eye(1./0)", "1./0", NonFinite, 0, [1,1], Some(f64::INFINITY.to_bits())),
        ("eye(-1,17)", "17", AboveProfileLimit, 1, [1,1], Some(17.0_f64.to_bits())),
        ("eye(17,0)", "17", AboveProfileLimit, 0, [1,1], Some(17.0_f64.to_bits())),
        ("ones(-1.5)", "-1.5", Fractional, 0, [1,1], Some((-1.5_f64).to_bits())),
        ("ones(1./0)", "1./0", NonFinite, 0, [1,1], Some(f64::INFINITY.to_bits())),
        ("ones(-1,17)", "17", AboveProfileLimit, 1, [1,1], Some(17.0_f64.to_bits())),
        ("ones(17,0)", "17", AboveProfileLimit, 0, [1,1], Some(17.0_f64.to_bits())),
        ("zeros(-1.5,2)", "-1.5", Fractional, 0, [1,1], Some((-1.5_f64).to_bits())),
        ("zeros(-1,17)", "17", AboveProfileLimit, 1, [1,1], Some(17.0_f64.to_bits())),
        ("zeros(17,-1)", "17", AboveProfileLimit, 0, [1,1], Some(17.0_f64.to_bits())),
        ("zeros(-1./0,2)", "-1./0", NonFinite, 0, [1,1], Some(f64::NEG_INFINITY.to_bits())),
        ("zeros(1.5)", "1.5", Fractional, 0, [1,1], Some(1.5_f64.to_bits())),
        ("zeros(1./0)", "1./0", NonFinite, 0, [1,1], Some(f64::INFINITY.to_bits())),
        ("zeros(17,0)", "17", AboveProfileLimit, 0, [1,1], Some(17.0_f64.to_bits())),
        ("zeros(0,17)", "17", AboveProfileLimit, 1, [1,1], Some(17.0_f64.to_bits())),
        ("zeros(1e300)", "1e300", AboveProfileLimit, 0, [1,1], Some(1e300_f64.to_bits())),
        ("zeros(zeros(0,3),2)", "zeros(0,3)", NonScalar, 0, [0,3], None),
    ] {
        let error = evaluate_session_expression_v1(lower(source).unwrap(), |_| None, || false).unwrap_err();
        assert_eq!(error.code(), Some("ConstructorDimension"));
        assert_eq!(&source[error.span().start..error.span().end], subject);
        assert_eq!(error.original_source().as_ref(), source);
        assert_eq!(error.stage(), mathlab_language::expression::LiteralStageV1::Evaluation);
        match error {
            SessionExpressionErrorV1::ConstructorDimension { reason: r, argument_index, shape: s, value_bits, .. } => {
                assert_eq!((r, argument_index, s, value_bits), (reason, index, shape, bits));
            }
            other => panic!("{other:?}"),
        }
    }
    for source in ["zeros(0./0)", "ones(0./0)", "eye(0./0)"] {
        let error = evaluate_session_expression_v1(lower(source).unwrap(), |_| None, || false).unwrap_err();
        assert_eq!(error.code(), Some("ConstructorDimension"));
        assert_eq!(&source[error.span().start..error.span().end], "0./0");
        match error {
            SessionExpressionErrorV1::ConstructorDimension {reason: NonFinite, value_bits: Some(bits), ..} => assert!(f64::from_bits(bits).is_nan()),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn zeros_unsupported_arity_and_shadowing_are_explicit() {
    for (source, code, subject) in [
        ("rand(2)", "UnsupportedCall", "rand"), ("sin(2)", "UnsupportedCall", "sin"),
        ("foo(missing)", "UnsupportedCall", "foo"), ("ZEROS(2)", "UnsupportedCall", "ZEROS"),
        ("zeros()", "CallArity", "zeros()"),
        ("eye()", "CallArity", "eye()"), ("eye(1,2,3)", "CallArity", "eye(1,2,3)"),
        ("ones()", "CallArity", "ones()"), ("ones(1,2,3)", "CallArity", "ones(1,2,3)"), ("zeros(1,2,3)", "CallArity", "zeros(1,2,3)"),
        ("zeros(2,3,1)", "CallArity", "zeros(2,3,1)"),
    ] {
        let error = evaluate_session_expression_v1(lower(source).unwrap(), |_| None, || false).unwrap_err();
        assert_eq!(error.code(), Some(code));
        assert_eq!(&source[error.span().start..error.span().end], subject);
    }
    let binding = SessionValueV1::try_from_col_major([1,1], &[9.0]).unwrap();
    let error = evaluate_session_expression_v1(lower("zeros(missing)").unwrap(),
        |name| if name=="zeros" {Some(&binding)} else {None}, || false).unwrap_err();
    assert_eq!(error.code(), Some("UnknownBinding"));
    assert_eq!((error.span().start, error.span().end), (6,13));
    let bare = evaluate_session_expression_v1(lower("zeros").unwrap(), |_| Some(&binding), || false).unwrap();
    assert_eq!(bare.value.as_slice(), &[9.0]);
    assert!(matches!(bare.effect, SessionStatementEffectV1::Display {..}));
    assert!(matches!(evaluate_session_expression_v1(lower("zeros=3").unwrap(), |_| None, || false).unwrap().effect,
        SessionStatementEffectV1::Assignment {..}));
    let error = evaluate_session_expression_v1(lower("ones(missing)").unwrap(),
        |name| if name=="ones" {Some(&binding)} else {None}, || false).unwrap_err();
    assert_eq!(error.code(), Some("UnknownBinding"));
    assert_eq!((error.span().start, error.span().end), (5,12));
    let bare = evaluate_session_expression_v1(lower("ones").unwrap(), |_| Some(&binding), || false).unwrap();
    assert_eq!(bare.value.as_slice(), &[9.0]);
    assert!(matches!(bare.effect, SessionStatementEffectV1::Display {..}));
    assert!(matches!(evaluate_session_expression_v1(lower("ones=3").unwrap(), |_| None, || false).unwrap().effect,
        SessionStatementEffectV1::Assignment {..}));
    let error = evaluate_session_expression_v1(lower("eye(missing)").unwrap(),
        |name| if name=="eye" {Some(&binding)} else {None}, || false).unwrap_err();
    assert_eq!(error.code(),Some("UnknownBinding"));
    assert_eq!((error.span().start,error.span().end),(4,11));
    let bare = evaluate_session_expression_v1(lower("eye").unwrap(), |_| Some(&binding), || false).unwrap();
    assert_eq!(bare.value.as_slice(),&[9.0]);
    assert!(matches!(bare.effect,SessionStatementEffectV1::Display {..}));
    assert!(matches!(evaluate_session_expression_v1(lower("eye=3").unwrap(), |_| None, || false).unwrap().effect,
        SessionStatementEffectV1::Assignment {..}));
    let unbound = evaluate_session_expression_v1(lower("eye").unwrap(), |_| None, || false).unwrap_err();
    assert_eq!(unbound.code(),Some("UnknownBinding"));
    for (shadowed, callee, word) in [
        ("zeros","ones",0x3ff0000000000000_u64), ("ones","zeros",0_u64),
        ("zeros","eye",0x3ff0000000000000), ("ones","eye",0x3ff0000000000000),
        ("eye","zeros",0), ("eye","ones",0x3ff0000000000000),
    ] {
        let value = evaluate_session_expression_v1(lower(&format!("{callee}(1)")).unwrap(),
            |name| if name==shadowed {Some(&binding)} else {None}, || false).unwrap();
        assert_eq!(value.value.as_slice()[0].to_bits(), word);
    }
}

#[test]
fn zeros_arguments_evaluate_before_dimensions_in_source_order() {
    let n = SessionValueV1::try_from_col_major([1,1], &[2.0]).unwrap();
    let m = SessionValueV1::try_from_col_major([1,1], &[3.0]).unwrap();
    for callee in ["zeros", "ones", "eye"] {
        let mut names = Vec::new();
        let result = evaluate_session_expression_v1(lower(&format!("{callee}(n,m)")).unwrap(), |name| {
            names.push(name.to_owned());
            match name {"n"=>Some(&n), "m"=>Some(&m), _=>None}
        }, || false).unwrap();
        assert_eq!(names, [callee, "n", "m", callee]);
        assert_eq!(result.value.shape(), [2,3]);
    }
    for (source, expected_names) in [
        ("zeros([1,2],missing)", vec!["zeros", "missing"]),
        ("zeros(-1,missing)", vec!["zeros", "missing"]),
        ("zeros(missing,other)", vec!["zeros", "missing"]),
        ("ones(missing,2)", vec!["ones", "missing"]),
        ("eye(missing,2)", vec!["eye", "missing"]),
        ("eye([1,2],missing)", vec!["eye", "missing"]),
        ("ones([1,2],missing)", vec!["ones", "missing"]),
    ] {
        let mut names = Vec::new();
        let error = evaluate_session_expression_v1(lower(source).unwrap(), |name| {names.push(name.to_owned()); None}, || false).unwrap_err();
        assert_eq!(names, expected_names);
        assert_eq!(error.code(), Some("UnknownBinding"));
        assert_eq!(&source[error.span().start..error.span().end], "missing");
    }
    let error = evaluate_session_expression_v1(lower("zeros(-1,1.5)").unwrap(), |_| None, || false).unwrap_err();
    assert_eq!(&error.original_source()[error.span().start..error.span().end], "1.5");
    assert!(matches!(error, SessionExpressionErrorV1::ConstructorDimension {
        argument_index:1, reason:ConstructorDimensionReasonV1::Fractional, value_bits:Some(bits), ..
    } if bits==1.5_f64.to_bits()));
    // Admission errors in arguments precede even unsupported-callee dispatch.
    assert!(lower("foo([1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17])").is_err());
}

#[test]
fn zeros_cancellation_checkpoints_drop_candidates_and_allow_recovery() {
    let n = SessionValueV1::try_from_col_major([1,1], &[2.0]).unwrap();
    // Before dispatch/arguments/construction, then after either fill constructor.
    for callee in ["zeros", "ones", "eye"] {
        for stop_at in [1,3,7,8] {
            let mut polls = 0;
            let error = evaluate_session_expression_v1(lower(&format!("{callee}(n,3)")).unwrap(),
                |name| if name=="n" {Some(&n)} else {None},
                || {polls+=1; polls==stop_at}).unwrap_err();
            assert!(matches!(error, SessionExpressionErrorV1::Cancelled {..}));
            assert_eq!(polls, stop_at);
            assert_eq!(n.as_slice(), &[2.0]);
        }
    }
    assert_eq!(evaluate_session_expression_v1(lower("zeros(1)").unwrap(), |_| None, || false).unwrap().value.as_slice(), &[0.0]);
    assert_eq!(evaluate_session_expression_v1(lower("ones(1)").unwrap(), |_| None, || false).unwrap().value.as_slice()[0].to_bits(), 0x3ff0000000000000);
    assert_eq!(evaluate_session_expression_v1(lower("eye(1)").unwrap(), |_| None, || false).unwrap().value.as_slice()[0].to_bits(), 0x3ff0000000000000);
}


#[test]
fn shape_queries_exact_rows_counts_empty_geometry_and_composition() {
    let one = 0x3ff0000000000000_u64;
    let two = 0x4000000000000000_u64;
    let three = 0x4008000000000000_u64;
    for (source,shape,words) in [
        ("size(7)",[1,2],vec![one,one]), ("numel(0./0)",[1,1],vec![one]),
        ("size([1,2,3])",[1,2],vec![one,three]), ("size([1;2;3])",[1,2],vec![three,one]),
        ("size([1,2,3;4,5,6])",[1,2],vec![two,three]),
        ("numel([1,2,3;4,5,6])",[1,1],vec![0x4018000000000000]),
        ("size([])",[1,2],vec![0,0]), ("size(zeros(0,3))",[1,2],vec![0,three]),
        ("size(zeros(3,0))",[1,2],vec![three,0]), ("numel(zeros(0,16))",[1,1],vec![0]),
        ("numel(ones(16,16))",[1,1],vec![0x4070000000000000]),
        ("numel(size(eye(2,3)))",[1,1],vec![two]),
        ("size(ones(16))",[1,2],vec![0x4030000000000000,0x4030000000000000]),
        ("size(1./0)",[1,2],vec![one,one]), ("numel(-0)",[1,1],vec![one]),
    ] {
        let result = evaluate_session_expression_v1(lower(source).unwrap(), |_| None, || false).unwrap().value;
        assert_eq!(result.class(),"double");
        assert_eq!(result.shape(),shape,"{source}");
        assert_eq!(result.as_slice().iter().map(|v| v.to_bits()).collect::<Vec<_>>(),words,"{source}");
    }
    let input = SessionValueV1::try_from_col_major([1,3],&[-0.0,f64::INFINITY,f64::from_bits(0x7ff8000000000042)]).unwrap();
    let before = input.as_slice().iter().map(|v| v.to_bits()).collect::<Vec<_>>();
    for (source,words) in [("size(A)",vec![one,three]),("numel(A)",vec![three])] {
        let value = evaluate_session_expression_v1(lower(source).unwrap(), |name| if name=="A" {Some(&input)} else {None}, || false).unwrap().value;
        assert_eq!(value.as_slice().iter().map(|v| v.to_bits()).collect::<Vec<_>>(),words);
    }
    assert_eq!(input.as_slice().iter().map(|v| v.to_bits()).collect::<Vec<_>>(),before);
    let constructed = evaluate_session_expression_v1(lower("zeros(numel([1,2]))").unwrap(), |_| None, || false).unwrap().value;
    assert_eq!(constructed.shape(),[2,2]);
    assert!(constructed.as_slice().iter().all(|v| v.to_bits()==0));
}

#[test]
fn shape_queries_arity_shadowing_argument_errors_and_dimension_boundary() {
    let binding = SessionValueV1::try_from_col_major([1,1],&[9.0]).unwrap();
    for (source,actual,span) in [("size()",0,(0,6)),("numel()",0,(0,7)),
        ("size(missing,2)",2,(0,15)),("numel(missing,2)",2,(0,16))] {
        let mut lookups = 0;
        let error = evaluate_session_expression_v1(lower(source).unwrap(), |_| {lookups+=1;None}, || false).unwrap_err();
        assert_eq!(lookups,1);
        assert_eq!(error.code(),Some("CallArity"));
        assert_eq!((error.span().start,error.span().end),span);
        assert!(matches!(error,SessionExpressionErrorV1::CallArity {actual:n,..} if n==actual));
    }
    for callee in ["size","numel"] {
        let source = format!("{callee}(missing)");
        let mut names = Vec::new();
        let error = evaluate_session_expression_v1(lower(&source).unwrap(), |name| {names.push(name.to_owned());None}, || false).unwrap_err();
        assert_eq!(names,[callee,"missing"]);
        assert_eq!(error.code(),Some("UnknownBinding"));
        assert_eq!((error.span().start,error.span().end),(callee.len()+1,callee.len()+8));
        let shadow = evaluate_session_expression_v1(lower(&source).unwrap(), |name| if name==callee {Some(&binding)} else {None}, || false).unwrap_err();
        assert_eq!(shadow.code(),Some("UnknownBinding"));
        assert_eq!((shadow.span().start,shadow.span().end),(callee.len()+1,callee.len()+8));
        let bare = evaluate_session_expression_v1(lower(callee).unwrap(), |_| Some(&binding), || false).unwrap();
        assert_eq!(bare.value.as_slice(),&[9.0]);
        assert!(matches!(bare.effect,SessionStatementEffectV1::Display {..}));
        assert!(matches!(evaluate_session_expression_v1(lower(&format!("{callee}=3")).unwrap(), |_| None, || false).unwrap().effect,
            SessionStatementEffectV1::Assignment {..}));
    }
    for (shadowed,callee,shape) in [("size","numel",[1,1]),("numel","size",[1,2])] {
        let value = evaluate_session_expression_v1(lower(&format!("{callee}(7)")).unwrap(), |name| if name==shadowed {Some(&binding)} else {None}, || false).unwrap().value;
        assert_eq!(value.shape(),shape);
        assert!(value.as_slice().iter().all(|v| v.to_bits()==0x3ff0000000000000));
    }
    let error = evaluate_session_expression_v1(lower("numel(zeros(17))").unwrap(), |_| None, || false).unwrap_err();
    assert_eq!(error.code(),Some("ConstructorDimension"));
    assert_eq!((error.span().start,error.span().end),(12,14));
    assert!(matches!(error,SessionExpressionErrorV1::ConstructorDimension {
        reason:ConstructorDimensionReasonV1::AboveProfileLimit,argument_index:0,shape:[1,1],value_bits:Some(0x4031000000000000),..
    }));
    let source = "zeros(size(7))";
    let error = evaluate_session_expression_v1(lower(source).unwrap(), |_| None, || false).unwrap_err();
    assert_eq!(&source[error.span().start..error.span().end],"size(7)");
    assert!(matches!(error,SessionExpressionErrorV1::ConstructorDimension {
        reason:ConstructorDimensionReasonV1::NonScalar,shape:[1,2],value_bits:None,..
    }));
    let error = evaluate_session_expression_v1(lower("zeros(numel(ones(16)))").unwrap(), |_| None, || false).unwrap_err();
    assert!(matches!(error,SessionExpressionErrorV1::ConstructorDimension {
        reason:ConstructorDimensionReasonV1::AboveProfileLimit,value_bits:Some(0x4070000000000000),..
    }));
}

#[test]
fn shape_queries_cancellation_checkpoints_preserve_inputs_and_recover() {
    let input = SessionValueV1::try_from_col_major([1,3],&[-0.0,f64::INFINITY,f64::NAN]).unwrap();
    let before = input.as_slice().iter().map(|v| v.to_bits()).collect::<Vec<_>>();
    for callee in ["size","numel"] {
        for stop_at in [1,3,5,6] {
            let mut polls = 0;
            let error = evaluate_session_expression_v1(lower(&format!("{callee}(A)")).unwrap(),
                |name| if name=="A" {Some(&input)} else {None}, || {polls+=1;polls==stop_at}).unwrap_err();
            assert!(matches!(error,SessionExpressionErrorV1::Cancelled {..}));
            assert_eq!(polls,stop_at);
            assert_eq!(input.as_slice().iter().map(|v| v.to_bits()).collect::<Vec<_>>(),before);
        }
        let value = evaluate_session_expression_v1(lower(&format!("{callee}(A)")).unwrap(),
            |name| if name=="A" {Some(&input)} else {None}, || false).unwrap().value;
        assert_eq!(value.shape(),if callee=="size" {[1,2]} else {[1,1]});
    }
}


#[test]
fn integer_ranges_selected_native_words_precedence_empty_and_exact_envelope() {
    // Selected native outputs; provenance is pinned in this slice's evidence index.
    let cases:&[(&str,[u64;2],&[u64])] = &[
        ("1:4",[1,4],&[0x3ff0000000000000_u64,0x4000000000000000_u64,0x4008000000000000_u64,0x4010000000000000_u64]),
        ("5:-2:0",[1,3],&[0x4014000000000000_u64,0x4008000000000000_u64,0x3ff0000000000000_u64]),
        ("1+1:2:2*4",[1,4],&[0x4000000000000000_u64,0x4010000000000000_u64,0x4018000000000000_u64,0x4020000000000000_u64]),
        ("1:3.'",[1,3],&[0x3ff0000000000000_u64,0x4000000000000000_u64,0x4008000000000000_u64]),
        ("(1:3).'",[3,1],&[0x3ff0000000000000_u64,0x4000000000000000_u64,0x4008000000000000_u64]),
        ("1:16",[1,16],&[0x3ff0000000000000_u64,0x4000000000000000_u64,0x4008000000000000_u64,0x4010000000000000_u64,0x4014000000000000_u64,0x4018000000000000_u64,0x401c000000000000_u64,0x4020000000000000_u64,0x4022000000000000_u64,0x4024000000000000_u64,0x4026000000000000_u64,0x4028000000000000_u64,0x402a000000000000_u64,0x402c000000000000_u64,0x402e000000000000_u64,0x4030000000000000_u64]),
        ("2:-3:2",[1,1],&[0x4000000000000000_u64]),
        ("5:1",[1,0],&[]),
        ("1:0:3",[1,0],&[]),
        ("1:-1:3",[1,0],&[]),
        ("(1:0:3).'",[0,1],&[]),
        ("-0",[1,1],&[0x8000000000000000_u64]),
        ("0",[1,1],&[0x0000000000000000_u64]),
        ("-0:1:0",[1,1],&[0x8000000000000000_u64]),
        ("0:-1:-0",[1,1],&[0x0000000000000000_u64]),
        ("-0:1:1",[1,2],&[0x8000000000000000_u64,0x3ff0000000000000_u64]),
        ("1:-1:-0",[1,2],&[0x3ff0000000000000_u64,0x0000000000000000_u64]),
        ("1:-0:3",[1,0],&[]),
        ("(-0):0:(-0)",[1,0],&[]),
        ("9007199254740991:1:9007199254740992",[1,2],&[0x433fffffffffffff_u64,0x4340000000000000_u64]),
        ("-9007199254740991:-1:-9007199254740992",[1,2],&[0xc33fffffffffffff_u64,0xc340000000000000_u64]),
        ("-3:0",[1,4],&[0xc008000000000000_u64,0xc000000000000000_u64,0xbff0000000000000_u64,0x0000000000000000_u64]),
        ("-2:2",[1,5],&[0xc000000000000000_u64,0xbff0000000000000_u64,0x0000000000000000_u64,0x3ff0000000000000_u64,0x4000000000000000_u64]),
        ("(5:1).'",[0,1],&[]),
    ];
    for &(source,shape,words) in cases {
        let value = evaluate_session_expression_v1(lower(source).unwrap(), |_| None, || false).unwrap().value;
        assert_eq!(value.class(),"double");
        assert_eq!(value.shape(),shape,"{source}");
        assert_eq!(value.as_slice().iter().map(|v| v.to_bits()).collect::<Vec<_>>(),words,"{source}");
    }
    // Wider intermediate offset, but all operands/results remain in the exact envelope.
    let value = evaluate_session_expression_v1(lower("-9007199254740992:9007199254740992:9007199254740992").unwrap(), |_| None, || false).unwrap().value;
    assert_eq!(value.shape(),[1,3]);
    assert_eq!(value.as_slice().iter().map(|v| v.to_bits()).collect::<Vec<_>>(),[0xc340000000000000_u64,0,0x4340000000000000]);
    let count = evaluate_session_expression_v1(lower("numel(1:4)").unwrap(), |_| None, || false).unwrap().value;
    assert_eq!(count.as_slice(),&[4.0]);
    let shape = evaluate_session_expression_v1(lower("size(5:1)").unwrap(), |_| None, || false).unwrap().value;
    assert_eq!(shape.as_slice().iter().map(|v| v.to_bits()).collect::<Vec<_>>(),[0x3ff0000000000000_u64,0]);
    let composed = evaluate_session_expression_v1(lower("ones(numel(1:3),1)").unwrap(), |_| None, || false).unwrap().value;
    assert_eq!(composed.shape(),[3,1]);
    assert_eq!(composed.as_slice(),&[1.0;3]);
}

#[test]
fn integer_ranges_grammar_operand_diagnostics_and_checked_cardinality() {
    for source in [":","1:","1::3","1:2:3:4","[1:]","A((:))"] {
        assert!(lower(source).is_err(),"{source}");
    }
    let error = lower("1:2:3:4").unwrap_err();
    assert_eq!(error.code(),Some("UnexpectedToken"));
    assert_eq!((error.span().start,error.span().end),(5,6));
    assert!(matches!(error,SessionExpressionErrorV1::Syntax {found:Some(SessionTokenV1::Colon),..}));
    let error = evaluate_session_expression_v1(lower("colon(1,3)").unwrap(), |_| None, || false).unwrap_err();
    assert_eq!(error.code(),Some("UnsupportedCall"));
    assert_eq!((error.span().start,error.span().end),(0,5));
    use IntegerRangeOperandReasonV1::*;
    for (source,subject,index,reason,shape,bits) in [
        ("[1,2]:3","[1,2]",0,NonScalar,[1,2],None),
        ("[]:0","[]",0,NonScalar,[0,0],None),
        ("1:0.5:3","0.5",1,Fractional,[1,1],Some(0.5_f64.to_bits())),
        ("1:1:17.5","17.5",2,Fractional,[1,1],Some(17.5_f64.to_bits())),
        ("(1./0):1:3","(1./0)",0,NonFinite,[1,1],Some(f64::INFINITY.to_bits())),
        ("1:0:(1./0)","(1./0)",2,NonFinite,[1,1],Some(f64::INFINITY.to_bits())),
        ("9007199254740994:1","9007199254740994",0,OutsideExactIntegerEnvelope,[1,1],Some(9007199254740994.0_f64.to_bits())),
        ("1:-9007199254740994:0","-9007199254740994",1,OutsideExactIntegerEnvelope,[1,1],Some((-9007199254740994.0_f64).to_bits())),
        ("1e308:1:1e308","1e308",0,OutsideExactIntegerEnvelope,[1,1],Some(1e308_f64.to_bits())),
    ] {
        let error = evaluate_session_expression_v1(lower(source).unwrap(), |_| None, || false).unwrap_err();
        assert_eq!(error.code(),Some("RangeOperand"));
        assert_eq!(&source[error.span().start..error.span().end],subject);
        assert_eq!(error.original_source().as_ref(),source);
        match error {
            SessionExpressionErrorV1::RangeOperand {operand_index,reason:r,shape:s,value_bits,..} =>
                assert_eq!((operand_index,r,s,value_bits),(index,reason,shape,bits)),
            other => panic!("{other:?}"),
        }
    }
    for (source,count) in [("1:17",17_i128),("-9007199254740992:9007199254740992",18014398509481985)] {
        let error = evaluate_session_expression_v1(lower(source).unwrap(), |_| None, || false).unwrap_err();
        assert_eq!(error.code(),Some("RangeLength"));
        assert_eq!((error.span().start,error.span().end),(0,source.len()));
        assert!(matches!(error,SessionExpressionErrorV1::RangeLength {actual,limit:16,..} if actual==count));
    }
    // Both colon operators are charged, beyond the three numeric leaves.
    assert!(lower(&format!("1:1:{}1","-".repeat(1019))).is_ok());
    assert!(lower(&format!("1:1:{}1","-".repeat(1020))).is_err());
}

#[test]
fn integer_ranges_expression_order_validation_and_cancellation_recovery() {
    let a = SessionValueV1::try_from_col_major([1,1],&[-1.0]).unwrap();
    let b = SessionValueV1::try_from_col_major([1,1],&[1.0]).unwrap();
    let c = SessionValueV1::try_from_col_major([1,1],&[1.0]).unwrap();
    let mut names = Vec::new();
    let value = evaluate_session_expression_v1(lower("A:B:C").unwrap(), |name| {
        names.push(name.to_owned());match name {"A"=>Some(&a),"B"=>Some(&b),"C"=>Some(&c),_=>None}
    }, || false).unwrap().value;
    assert_eq!(names,["A","B","C"]);
    assert_eq!(value.as_slice().iter().map(|v| v.to_bits()).collect::<Vec<_>>(),[(-1.0_f64).to_bits(),0,1.0_f64.to_bits()]);
    for source in ["[1,2]:missing:3","1:0:missing"] {
        let mut names = Vec::new();
        let error = evaluate_session_expression_v1(lower(source).unwrap(), |name| {names.push(name.to_owned());None}, || false).unwrap_err();
        assert_eq!(names,["missing"]);
        assert_eq!(error.code(),Some("UnknownBinding"));
        assert_eq!(&source[error.span().start..error.span().end],"missing");
    }
    let source = "% π\n  x=[1,2]:missing:3; % tail\n";
    let error = evaluate_session_expression_v1(lower(source).unwrap(), |_| None, || false).unwrap_err();
    assert_eq!(error.original_source().as_ref(),source);
    assert_eq!(error.span().start,source.find("missing").unwrap());
    assert_eq!(error.span().start_line,2);
    for (source,stops) in [("1:16",[1,3,7,8]),("1:1:16",[1,3,9,10])] {
        for stop_at in stops {
            let mut polls = 0;
            let error = evaluate_session_expression_v1(lower(source).unwrap(), |_| None,
                || {polls+=1;polls==stop_at}).unwrap_err();
            assert!(matches!(error,SessionExpressionErrorV1::Cancelled {..}));
            assert_eq!(polls,stop_at);
        }
    }
    let value = evaluate_session_expression_v1(lower("1:3").unwrap(), |_| None, || false).unwrap().value;
    assert_eq!(value.as_slice(),&[1.0,2.0,3.0]);
}


#[test]
fn reshape_values_column_major_empty_geometry_and_independent_payload_bits() {
    for (source,shape,data) in [
        ("reshape([1,2,3;4,5,6],3,2)",[3,2],vec![1.,4.,2.,5.,3.,6.]),
        ("reshape([1;2;3;4],1,4)",[1,4],vec![1.,2.,3.,4.]),
        ("reshape(7,1,1)",[1,1],vec![7.]),
        ("reshape([-0,0],2,1)",[2,1],vec![-0.,0.]),
        ("reshape([],0,3)",[0,3],vec![]),
        ("reshape(zeros(3,0),4,0)",[4,0],vec![]),
        ("reshape(zeros(0,3),0,0)",[0,0],vec![]),
        ("reshape([], -0, 3)",[0,3],vec![]),
        ("reshape([], 3, -0)",[3,0],vec![]),
        ("reshape(ones(16,16),16,16)",[16,16],vec![1.;256]),
        ("numel(reshape([1,2;3,4],1,4))",[1,1],vec![4.]),
        ("reshape(1:6,3,2)",[3,2],vec![1.,2.,3.,4.,5.,6.]),
        ("reshape(1e309,1,1)",[1,1],vec![f64::INFINITY]),
        ("reshape(-1e309,1,1)",[1,1],vec![f64::NEG_INFINITY]),
    ] {
        let value = evaluate_session_expression_v1(lower(source).unwrap(), |_| None, || false).unwrap().value;
        assert_eq!(value.class(),"double");
        assert_eq!(value.shape(),shape,"{source}");
        assert_eq!(value.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
            data.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),"{source}");
    }
    // Host-seeded custom NaN verifies Rust copying, not native custom-payload parity.
    let words = [0x8000000000000000,0x7ff0000000000000,0xfff0000000000000,0x7ff8000000000042];
    let input = SessionValueV1::try_from_col_major([1,4],&words.map(f64::from_bits)).unwrap();
    let value = evaluate_session_expression_v1(lower("reshape(A,2,2)").unwrap(),
        |name| if name=="A" {Some(&input)} else {None},||false).unwrap().value;
    assert_eq!(value.shape(),[2,2]);
    assert_ne!(value.as_slice().as_ptr(),input.as_slice().as_ptr());
    for array in [&input,&value] {
        assert_eq!(array.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),words);
    }
    assert_eq!(input.shape(),[1,4]);
}

#[test]
fn reshape_dimension_errors_arity_shadowing_and_original_storage_cause() {
    use ConstructorDimensionReasonV1::*;
    for (source,subject,reason,index,shape,bits) in [
        ("reshape([], -1, 0)","-1",Negative,1,[1,1],Some((-1.0_f64).to_bits())),
        ("reshape([], -1.5, 0)","-1.5",Fractional,1,[1,1],Some((-1.5_f64).to_bits())),
        ("reshape(1,1.5,1)","1.5",Fractional,1,[1,1],Some(1.5_f64.to_bits())),
        ("reshape(1,1,1e309)","1e309",NonFinite,2,[1,1],Some(f64::INFINITY.to_bits())),
        ("reshape([],[],0)","[]",NonScalar,1,[0,0],None),
        ("reshape(zeros(0,3),0,17)","17",AboveProfileLimit,2,[1,1],Some(17.0_f64.to_bits())),
        ("reshape([],0,-1)","-1",Negative,2,[1,1],Some((-1.0_f64).to_bits())),
        ("reshape([],17,0)","17",AboveProfileLimit,1,[1,1],Some(17.0_f64.to_bits())),
    ] {
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|_|None,||false).unwrap_err();
        assert_eq!(error.code(),Some("ConstructorDimension"));
        assert_eq!(error.original_source().as_ref(),source);
        assert_eq!(error.stage(),mathlab_language::expression::LiteralStageV1::Evaluation);
        assert_eq!(&source[error.span().start..error.span().end],subject);
        assert!(matches!(error,SessionExpressionErrorV1::ConstructorDimension {
            argument_index:i,reason:r,shape:s,value_bits:b,..} if (i,r,s,b)==(index,reason,shape,bits)));
    }
    let source="reshape([1,2],1,1)";
    let error=evaluate_session_expression_v1(lower(source).unwrap(),|_|None,||false).unwrap_err();
    assert_eq!(error.code(),Some("ArrayOperation"));
    assert_eq!((error.span().start,error.span().end),(0,source.len()));
    match error {
        SessionExpressionErrorV1::Arithmetic(mathlab_language::arithmetic_expression::ArithmeticExpressionErrorV1::Shared(
            mathlab_language::decimal_expression::DecimalExpressionErrorV1::Diagnostic(d))) => {
            assert!(matches!(d.detail,mathlab_language::decimal_expression::DecimalDiagnosticDetailV1::Profile(
                mathlab_language::expression::LiteralDiagnosticDetailV1::ArrayOperation {
                    error:mathlab_array::ArrayErrorV1 {context:mathlab_array::ErrorContextV1::Construct,
                        code:mathlab_array::ErrorCodeV1::StorageLengthMismatch {expected_elements:1,actual_elements:2}}
                })));
        },
        other=>panic!("{other:?}"),
    }
    let binding=SessionValueV1::try_from_col_major([1,1],&[9.]).unwrap();
    for (source,actual) in [("reshape()",0),("reshape(missing)",1),("reshape(missing,1)",2),("reshape(missing,1,1,1)",4)] {
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|_|None,||false).unwrap_err();
        assert_eq!((error.span().start,error.span().end),(0,source.len()));
        assert!(matches!(error,SessionExpressionErrorV1::CallArity {actual:n,..} if n==actual));
    }
    let error=evaluate_session_expression_v1(lower("reshape(missing,1,1)").unwrap(),|name| {
        assert_eq!(name,"reshape");Some(&binding)
    },||false).unwrap_err();
    assert_eq!(error.code(),Some("IndexArity"));
    assert_eq!((error.span().start,error.span().end),(0,20));
    let bare=evaluate_session_expression_v1(lower("reshape").unwrap(),|_|Some(&binding),||false).unwrap();
    assert!(matches!(bare.effect,SessionStatementEffectV1::Display {..}));
    assert_eq!(bare.value.as_slice(),&[9.]);
    assert!(matches!(evaluate_session_expression_v1(lower("reshape=3;").unwrap(),|_|None,||false).unwrap().effect,
        SessionStatementEffectV1::Assignment {..}));
}

#[test]
fn reshape_expression_order_cancellation_and_recovery() {
    for (source,names,span) in [
        ("reshape(1,-1,missing)",vec!["reshape","missing"],(13,20)),
        ("reshape(missing,other,another)",vec!["reshape","missing"],(8,15)),
    ] {
        let mut seen=Vec::new();
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|name|{seen.push(name.to_owned());None},||false).unwrap_err();
        assert_eq!(error.code(),Some("UnknownBinding"));
        assert_eq!((error.span().start,error.span().end),span);
        assert_eq!(seen,names);
    }
    let input=SessionValueV1::try_from_col_major([1,4],&[1.,2.,3.,4.]).unwrap();
    let m=SessionValueV1::try_from_col_major([1,1],&[2.]).unwrap();
    let mut seen=Vec::new();
    let value=evaluate_session_expression_v1(lower("reshape(A,m,n)").unwrap(),|name|{
        seen.push(name.to_owned());match name {"A"=>Some(&input),"m"|"n"=>Some(&m),_=>None}
    },||false).unwrap().value;
    assert_eq!(seen,["reshape","A","m","n","reshape"]);
    assert_eq!(value.shape(),[2,2]);
    for stop_at in [1,3,5,7,9,10] {
        let mut polls=0;
        let error=evaluate_session_expression_v1(lower("reshape(A,2,2)").unwrap(),
            |name|if name=="A" {Some(&input)} else {None},||{polls+=1;polls==stop_at}).unwrap_err();
        assert!(matches!(error,SessionExpressionErrorV1::Cancelled {..}));
        assert_eq!(polls,stop_at);
        assert_eq!(input.shape(),[1,4]);
        assert_eq!(input.as_slice(),&[1.,2.,3.,4.]);
    }
    assert_eq!(evaluate_session_expression_v1(lower("reshape(A,4,1)").unwrap(),
        |name|if name=="A"{Some(&input)}else{None},||false).unwrap().value.shape(),[4,1]);
}

#[test]
fn scalar_index_column_major_composition_independent_words_and_linear_256() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    for (source,expected) in [("A(1+1)",4.),("A([1],2)",2.),("A(2:2)",4.),
        ("A(2,3)",6.),("A(A(1)+1)",4.),("A(2)+A(1,2)",6.)] {
        let value=evaluate_session_expression_v1(lower(source).unwrap(),|name|if name=="A"{Some(&a)}else{None},||false).unwrap().value;
        assert_eq!(value.shape(),[1,1]);
        assert_eq!(value.as_slice(),&[expected]);
        assert_ne!(value.as_slice().as_ptr(),a.as_slice().as_ptr());
    }
    let full=SessionValueV1::try_from_col_major([16,16],&(1..=256).map(f64::from).collect::<Vec<_>>()).unwrap();
    assert_eq!(evaluate_session_expression_v1(lower("A(256)").unwrap(),|_|Some(&full),||false).unwrap().value.as_slice(),&[256.]);
    for (source,words) in [("A(1)",[0x8000000000000000,0x3ff0000000000000]),
        ("A(2,1)",[0x3ff0000000000000,0x7ff8000000000042])] {
        let input=SessionValueV1::try_from_col_major([2,1],&words.map(f64::from_bits)).unwrap();
        let selected=evaluate_session_expression_v1(lower(source).unwrap(),|_|Some(&input),||false).unwrap().value;
        assert_eq!(selected.as_slice()[0].to_bits(),if source=="A(1)"{words[0]}else{words[1]});
        assert_eq!(input.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),words);
    }
}

#[test]
fn scalar_index_domain_bounds_original_spans_and_native_order_delta() {
    use IndexOperandReasonV1::*;
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    for (expr,reason,shape,bits) in [("0",NonPositive,[1,1],Some(0)),
        ("-0",NonPositive,[1,1],Some(0x8000000000000000)),("-1",NonPositive,[1,1],Some((-1_f64).to_bits())),
        ("1.5",Fractional,[1,1],Some(1.5_f64.to_bits())),("[1,2;1,2]",NonScalar,[2,2],None)] {
        let source=format!("% π\n  A(1,{expr}); % tail");
        let error=evaluate_session_expression_v1(lower(&source).unwrap(),|name|if name=="A"{Some(&a)}else{None},||false).unwrap_err();
        assert_eq!(error.original_source().as_ref(),source);
        assert_eq!(error.code(),Some("IndexOperand"));
        assert_eq!(&source[error.span().start..error.span().end],expr);
        assert_eq!(error.span().start_line,2);
        assert!(matches!(error,SessionExpressionErrorV1::IndexOperand {argument_index:1,reason:r,shape:s,value_bits:b,..} if (r,s,b)==(reason,shape,bits)));
    }
    for word in [f64::INFINITY.to_bits(),0x7ff8000000000042] {
        let i=SessionValueV1::try_from_col_major([1,1],&[f64::from_bits(word)]).unwrap();
        let eval=|source:&str|evaluate_session_expression_v1(lower(source).unwrap(),|name|match name{"A"=>Some(&a),"I"=>Some(&i),_=>None},||false).unwrap_err();
        assert!(matches!(eval("A(I)"),SessionExpressionErrorV1::IndexOperand {argument_index:0,reason:NonFinite,value_bits:Some(bits),..} if bits==word));
        // I33 delta: first row bounds precede the second index's nonfinite domain.
        assert!(matches!(eval("A(7,I)"),SessionExpressionErrorV1::IndexBounds {argument_index:0,upper_bound:2,..}));
        assert!(matches!(eval("A(I,7)"),SessionExpressionErrorV1::IndexOperand {argument_index:0,reason:NonFinite,..}));
    }
    let empty=SessionValueV1::try_from_col_major([0,3],&[]).unwrap();
    for (source,index,bound,expr) in [("A(7)",0,6,"7"),("A(3,1)",0,2,"3"),("A(1,4)",1,3,"4"),
        ("A(1e300)",0,6,"1e300"),("E(1)",0,0,"1"),("E(1,1)",0,0,"1")] {
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|name|match name{"A"=>Some(&a),"E"=>Some(&empty),_=>None},||false).unwrap_err();
        assert_eq!(&source[error.span().start..error.span().end],expr);
        assert!(matches!(error,SessionExpressionErrorV1::IndexBounds {argument_index:i,upper_bound:b,shape:[1,1],value_bits,..} if i==index && b==bound && f64::from_bits(value_bits)>b as f64));
    }
}

#[test]
fn scalar_index_binding_first_all_builtin_arities_and_no_fallback() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    for name in ["A","zeros","ones","eye","size","numel","reshape","sin"] {
        for (args,expected) in [("2",4.),("1,2",2.)] {
            let source=format!("{name}({args})");
            let value=evaluate_session_expression_v1(lower(&source).unwrap(),|n|if n==name{Some(&a)}else{None},||false).unwrap().value;
            assert_eq!(value.as_slice(),&[expected]);
        }
        for (args,count) in [("",0),("missing,1,1",3)] {
            let source=format!("{name}({args})");
            let mut seen=Vec::new();
            let error=evaluate_session_expression_v1(lower(&source).unwrap(),|n|{seen.push(n.to_owned());if n==name{Some(&a)}else{None}},||false).unwrap_err();
            assert_eq!(seen,[name]);
            assert_eq!((error.span().start,error.span().end),(0,source.len()));
            assert!(matches!(error,SessionExpressionErrorV1::IndexArity {actual,..} if actual==count));
        }
        let source=format!("{name}(0,missing)");
        let error=evaluate_session_expression_v1(lower(&source).unwrap(),|n|if n==name{Some(&a)}else{None},||false).unwrap_err();
        assert_eq!(error.code(),Some("UnknownBinding"));
        assert_eq!(&source[error.span().start..error.span().end],"missing");
        let source=format!("{name}(7)");
        assert_eq!(evaluate_session_expression_v1(lower(&source).unwrap(),|n|if n==name{Some(&a)}else{None},||false).unwrap_err().code(),Some("IndexBounds"));
    }
    for source in ["A(:+1)","A(1)=","(A)(1)","A(1)(1)"] {assert!(lower(source).is_err(),"{source}");}
}

#[test]
fn scalar_index_complete_expression_order_cancellation_and_recovery() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    let i=SessionValueV1::try_from_col_major([1,1],&[1.]).unwrap();
    let mut seen=Vec::new();
    evaluate_session_expression_v1(lower("A(i,j)").unwrap(),|name|{seen.push(name.to_owned());match name{"A"=>Some(&a),"i"|"j"=>Some(&i),_=>None}},||false).unwrap();
    assert_eq!(seen,["A","i","j","A"]);
    for (source,subject) in [("A(0,missing)","missing"),("A(missing,other)","missing"),("A(7,missing)","missing")] {
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|name|if name=="A"{Some(&a)}else{None},||false).unwrap_err();
        assert_eq!(error.code(),Some("UnknownBinding"));
        assert_eq!(&source[error.span().start..error.span().end],subject);
    }
    let mut total=0;
    evaluate_session_expression_v1(lower("A(1+1,2)").unwrap(),|name|if name=="A"{Some(&a)}else{None},||{total+=1;false}).unwrap();
    for stop_at in 1..=total {
        let mut polls=0;
        assert!(matches!(evaluate_session_expression_v1(lower("A(1+1,2)").unwrap(),|name|if name=="A"{Some(&a)}else{None},||{polls+=1;polls==stop_at}),Err(SessionExpressionErrorV1::Cancelled {..})));
        assert_eq!(a.as_slice(),&[1.,4.,2.,5.,3.,6.]);
    }
    assert_eq!(evaluate_session_expression_v1(lower("A(2)").unwrap(),|name|if name=="A"{Some(&a)}else{None},||false).unwrap().value.as_slice(),&[4.]);
}

#[test]
fn vector_index_linear_cartesian_orientation_duplicates_and_expression_values() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    let vr=SessionValueV1::try_from_col_major([1,4],&[10.,20.,30.,40.]).unwrap();
    let vc=SessionValueV1::try_from_col_major([4,1],&[10.,20.,30.,40.]).unwrap();
    let scalar=SessionValueV1::try_from_col_major([1,1],&[9.]).unwrap();
    for (source,shape,words) in [
        ("A([1,2])",[1,2],vec![1.,4.]),("A([2;1;2])",[3,1],vec![4.,1.,4.]),
        ("A([6,1,3])",[1,3],vec![6.,1.,2.]),("A(2,[3;1;2])",[1,3],vec![6.,4.,5.]),
        ("A([2,1],2)",[2,1],vec![5.,2.]),("VR(1,[4;1])",[1,2],vec![40.,10.]),
        ("VC([4,1],1)",[2,1],vec![40.,10.]),("A([2,1],[2;2;1])",[2,3],vec![5.,2.,5.,2.,4.,1.]),
        ("A([1,1]+[1,2])",[1,2],vec![4.,2.]),("S([1,1])",[1,2],vec![9.,9.]),
        ("S([1;1])",[2,1],vec![9.,9.]),("A(1:2,3:-1:1)",[2,3],vec![3.,6.,2.,5.,1.,4.]),
    ] {
        let value=evaluate_session_expression_v1(lower(source).unwrap(),|name|match name{"A"=>Some(&a),"VR"=>Some(&vr),"VC"=>Some(&vc),"S"=>Some(&scalar),_=>None},||false).unwrap().value;
        assert_eq!(value.shape(),shape,"{source}");assert_eq!(value.as_slice(),words,"{source}");
    }
    for selector in ["[4,1,2]","[4;1;2]"] {
        for (name,target,shape) in [("VR",&vr,[1,3]),("VC",&vc,[3,1])] {
            let source=format!("{name}({selector})");
            let value=evaluate_session_expression_v1(lower(&source).unwrap(),|n|if n==name{Some(target)}else{None},||false).unwrap().value;
            assert_eq!(value.shape(),shape);assert_eq!(value.as_slice(),&[40.,10.,20.]);
        }
    }
    for rows in ["[2,1]","[2;1]"] {for columns in ["[3,1]","[3;1]"] {
        let source=format!("A({rows},{columns})");
        let value=evaluate_session_expression_v1(lower(&source).unwrap(),|name|if name=="A"{Some(&a)}else{None},||false).unwrap().value;
        assert_eq!(value.shape(),[2,2]);assert_eq!(value.as_slice(),&[6.,3.,4.,1.]);
    }}
}

#[test]
fn vector_index_element_metadata_domain_phases_argument_order_and_rejections() {
    use IndexOperandReasonV1::*;
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    for (word,reason) in [(0,NonPositive),(0x8000000000000000,NonPositive),((-1_f64).to_bits(),NonPositive),
        (1.5_f64.to_bits(),Fractional),((-1.5_f64).to_bits(),Fractional),(f64::INFINITY.to_bits(),NonFinite),(0x7ff8000000000042,NonFinite)] {
        for shape in [[1,2],[2,1]] {
            let p=SessionValueV1::try_from_col_major(shape,&[1.,f64::from_bits(word)]).unwrap();
            let source="% π original\n  B=A(1,P); % tail";
            let error=evaluate_session_expression_v1(lower(source).unwrap(),|name|match name{"A"=>Some(&a),"P"=>Some(&p),_=>None},||false).unwrap_err();
            assert_eq!(error.code(),Some("IndexOperand"));assert_eq!(error.original_source().as_ref(),source);
            assert_eq!(&source[error.span().start..error.span().end],"P");assert_eq!(error.span().start_line,2);
            assert!(matches!(error,SessionExpressionErrorV1::IndexElementOperand {argument_index:1,element_index:1,reason:r,shape:s,value_bits,..} if r==reason && s==shape && value_bits==word));
        }
    }
    for (source,arg,element,reason,bits) in [
        ("A([7,1e309])",0,1,NonFinite,f64::INFINITY.to_bits()),
        ("A([1e309,7])",0,0,NonFinite,f64::INFINITY.to_bits()),
        ("A([1;7;0])",0,2,NonPositive,0),
        ("A([2,1],[4,1e309])",1,1,NonFinite,f64::INFINITY.to_bits()),
    ] {
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|name|if name=="A"{Some(&a)}else{None},||false).unwrap_err();
        assert!(matches!(error,SessionExpressionErrorV1::IndexElementOperand {argument_index:i,element_index:e,reason:r,value_bits,..} if (i,e,r,value_bits)==(arg,element,reason,bits)),"{source}");
    }
    for (source,arg,element,bound,word) in [("A([3,1],[1e309,1])",0,0,2,3.),
        ("A([2,1],[4,5])",1,0,3,4.),("A([1,7,8])",0,1,6,7.),
        ("A([1e300,1])",0,0,6,1e300),("A([3,1],[])",0,0,2,3.)] {
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|name|if name=="A"{Some(&a)}else{None},||false).unwrap_err();
        assert_eq!(error.code(),Some("IndexBounds"));
        assert!(matches!(error,SessionExpressionErrorV1::IndexElementBounds {argument_index:i,element_index:e,upper_bound:b,value_bits,..} if (i,e,b)==(arg,element,bound) && f64::from_bits(value_bits)==word),"{source}");
    }
    for (source,arg,shape) in [("A([1,2;1,2])",0,[2,2])] {
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|name|if name=="A"{Some(&a)}else{None},||false).unwrap_err();
        assert!(matches!(error,SessionExpressionErrorV1::IndexOperand {argument_index:i,reason:NonScalar,shape:s,value_bits:None,..} if (i,s)==(arg,shape)));
    }
    let empty=SessionValueV1::try_from_col_major([0,3],&[]).unwrap();
    assert!(matches!(evaluate_session_expression_v1(lower("E([1,1])").unwrap(),|name|if name=="E"{Some(&empty)}else{None},||false),Err(SessionExpressionErrorV1::IndexElementBounds {argument_index:0,element_index:0,upper_bound:0,..})));
    assert_eq!(evaluate_session_expression_v1(lower("E([])").unwrap(),|name|if name=="E"{Some(&empty)}else{None},||false).unwrap().value.shape(),[0,0]);
    assert!(matches!(evaluate_session_expression_v1(lower("A(7,1e309)").unwrap(),|name|if name=="A"{Some(&a)}else{None},||false),Err(SessionExpressionErrorV1::IndexBounds {argument_index:0,upper_bound:2,..})));
}

#[test]
fn vector_index_selected_word_bits_independence_and_maximum_output_geometry() {
    let words=[0x8000000000000000,0x7ff8000000000042,0x7ff0000000000000,0xfff0000000000000];
    let b=SessionValueV1::try_from_col_major([2,2],&words.map(f64::from_bits)).unwrap();
    for (source,shape,expected) in [("B([2,1,2,4,3])",[1,5],vec![words[1],words[0],words[1],words[3],words[2]]),
        ("B([2,1],[2,1,2])",[2,3],vec![words[3],words[2],words[1],words[0],words[3],words[2]])] {
        let value=evaluate_session_expression_v1(lower(source).unwrap(),|name|if name=="B"{Some(&b)}else{None},||false).unwrap().value;
        assert_eq!(value.shape(),shape);assert_eq!(value.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),expected);
        assert_ne!(value.as_slice().as_ptr(),b.as_slice().as_ptr());
    }
    assert_eq!(b.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),words);
    let m=SessionValueV1::try_from_col_major([16,16],&(1..=256).map(f64::from).collect::<Vec<_>>()).unwrap();
    let scalar=SessionValueV1::try_from_col_major([1,1],&[9.]).unwrap();
    for (source,shape) in [("M([256,1])",[1,2]),("M([256;1])",[2,1])] {
        let value=evaluate_session_expression_v1(lower(source).unwrap(),|name|if name=="M"{Some(&m)}else{None},||false).unwrap().value;
        assert_eq!(value.shape(),shape);assert_eq!(value.as_slice(),&[256.,1.]);
    }
    let reverse=evaluate_session_expression_v1(lower("M(16:-1:1,16:-1:1)").unwrap(),|name|if name=="M"{Some(&m)}else{None},||false).unwrap().value;
    assert_eq!(reverse.shape(),[16,16]);assert_eq!(reverse.payload_bytes(),2048);
    assert_eq!(reverse.as_slice(),(1..=256).rev().map(f64::from).collect::<Vec<_>>());
    let maximum=evaluate_session_expression_v1(lower("S(ones(1,16),ones(1,16))").unwrap(),|name|if name=="S"{Some(&scalar)}else{None},||false).unwrap().value;
    assert_eq!(maximum.shape(),[16,16]);assert_eq!(maximum.payload_bytes(),2048);assert_eq!(maximum.as_slice(),&[9.;256]);
    let error=evaluate_session_expression_v1(lower("M(ones(1,17))").unwrap(),|name|if name=="M"{Some(&m)}else{None},||false).unwrap_err();
    assert!(matches!(error,SessionExpressionErrorV1::ConstructorDimension {argument_index:1,reason:ConstructorDimensionReasonV1::AboveProfileLimit,..}));
}

#[test]
fn vector_index_binding_first_complete_children_polling_and_recovery() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    for name in ["A","zeros","ones","eye","size","numel","reshape","sin"] {
        let source=format!("{name}([2,1],[3,1])");
        let value=evaluate_session_expression_v1(lower(&source).unwrap(),|n|if n==name{Some(&a)}else{None},||false).unwrap().value;
        assert_eq!(value.shape(),[2,2]);assert_eq!(value.as_slice(),&[6.,3.,4.,1.]);
        let source=format!("{name}([7,1])");
        assert!(matches!(evaluate_session_expression_v1(lower(&source).unwrap(),|n|if n==name{Some(&a)}else{None},||false),Err(SessionExpressionErrorV1::IndexElementBounds {argument_index:0,element_index:0,upper_bound:6,..})));
    }
    for (source,code,subject) in [("A([0,1],missing)","UnknownBinding","missing"),
        ("A(missing,other_missing)","UnknownBinding","missing"),("A(missing,missing,missing)","IndexArity","A(missing,missing,missing)"),
        ("zeros([2,1])","ConstructorDimension","[2,1]"),("unbound([1,2])","UnsupportedCall","unbound")] {
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|name|if name=="A"{Some(&a)}else{None},||false).unwrap_err();
        assert_eq!(error.code(),Some(code));assert_eq!(&source[error.span().start..error.span().end],subject);
    }
    for source in ["A([1 +])","A((:),1)","A(1)=","([1,2])(1)","A(1)(1)"] {assert!(lower(source).is_err(),"{source}");}
    let source="A([2,1],[3;1;2])";
    let mut total=0;
    evaluate_session_expression_v1(lower(source).unwrap(),|name|if name=="A"{Some(&a)}else{None},||{total+=1;false}).unwrap();
    for stop_at in 1..=total {
        let mut polls=0;
        assert!(matches!(evaluate_session_expression_v1(lower(source).unwrap(),|name|if name=="A"{Some(&a)}else{None},||{polls+=1;polls==stop_at}),Err(SessionExpressionErrorV1::Cancelled {..})));
        assert_eq!(a.as_slice(),&[1.,4.,2.,5.,3.,6.]);
    }
    assert_eq!(evaluate_session_expression_v1(lower("A(2)").unwrap(),|name|if name=="A"{Some(&a)}else{None},||false).unwrap().value.as_slice(),&[4.]);
}

#[test]
fn matrix_whitespace_numeric_geometry_bits_sign_boundaries_and_index_consumers() {
    for (source,shape,words) in [
        ("[1 2]",[1,2],vec![1.,2.]),("[1 2;3 4]",[2,2],vec![1.,3.,2.,4.]),
        ("[1\t2;3\t4]",[2,2],vec![1.,3.,2.,4.]),("[1  2]",[1,2],vec![1.,2.]),
        ("[1 \t 2]",[1,2],vec![1.,2.]),(" \t[ \t1,\t2 \t]\t ",[1,2],vec![1.,2.]),
        ("[ \t1 2 \t]",[1,2],vec![1.,2.]),("[1,2 3;4 5,6]",[2,3],vec![1.,4.,2.,5.,3.,6.]),
        ("[1 , 2 ; 3 , 4]",[2,2],vec![1.,3.,2.,4.]),("[1 -2]",[1,2],vec![1.,-2.]),
        ("[1 +2]",[1,2],vec![1.,2.]),("[1\t-2]",[1,2],vec![1.,-2.]),("[1 -2e-3]",[1,2],vec![1.,-0.002]),
        ("[1,-2]",[1,2],vec![1.,-2.]),("[1,- 2]",[1,2],vec![1.,-2.]),("[1,+ 2]",[1,2],vec![1.,2.]),
        ("[1;- 2]",[2,1],vec![1.,-2.]),("[- 2]",[1,1],vec![-2.]),("[-2 3]",[1,2],vec![-2.,3.]),
        ("[- 2 3]",[1,2],vec![-2.,3.]),("[1e-2 3E+4]",[1,2],vec![0.01,30000.]),
        ("[1e-2,3E+4]",[1,2],vec![0.01,30000.]),("[.5 1.]",[1,2],vec![0.5,1.]),
        ("[1.2 .3]",[1,2],vec![1.2,0.3]),("[12]",[1,1],vec![12.]),("[]",[0,0],vec![]),("[ \t]",[0,0],vec![]),
        ("[-0 +0,- 0]",[1,3],vec![-0.0,0.0,-0.0]),("[1e309 -1e309]",[1,2],vec![f64::INFINITY,f64::NEG_INFINITY]),
    ] {
        let result=evaluate_session_expression_v1(lower(source).unwrap(),|_|None,||false).unwrap().value;
        assert_eq!(result.shape(),shape,"{source}");
        assert_eq!(result.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),words.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),"{source}");
    }
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    let value=evaluate_session_expression_v1(lower("A([2 1],[3;1;2])").unwrap(),|name|if name=="A"{Some(&a)}else{None},||false).unwrap().value;
    assert_eq!(value.shape(),[2,3]);assert_eq!(value.as_slice(),&[6.,3.,4.,1.,5.,2.]);
}

#[test]
fn matrix_whitespace_preserves_rejections_failure_order_and_public_profile_isolation() {
    for source in ["[1e -2]","[1e- 2]","[1.2.3]","[;]","[1;]","[1,]","[1(2)]","[1[2]]","[1 2i]",
        "[1,,2]","[1 ; ;2]","[,1]","[1\n2]","[1\r2]","[1\u{a0}2]","[1 % gap\n2]","[1 ...\n2]",
        "[1 +]"] {
        assert!(lower(source).is_err(),"{source}");
    }
    for source in ["[1 2]","[1\t2]","[1 -2]","[1 2;3 4]"] {
        assert!(parse_literal_expression_v1(Arc::from(source)).is_err());
        assert!(parse_decimal_expression_v1(Arc::from(source)).is_err());
        assert!(parse_arithmetic_expression_v1(Arc::from(source)).is_err());
    }
    for source in ["[1,2]","[1,- 2]"] {
        assert!(parse_literal_expression_v1(Arc::from(source)).is_ok());
        assert!(parse_decimal_expression_v1(Arc::from(source)).is_ok());
        assert!(parse_arithmetic_expression_v1(Arc::from(source)).is_ok());
    }
    for source in ["[1 2;3]","[1,2;3]"] {
        let error=lower(source).unwrap_err();assert_eq!(error.code(),Some("MatrixNotAdmitted"));
        assert_eq!(error.stage(),mathlab_language::expression::LiteralStageV1::MatrixAdmission);
        assert_eq!((error.span().start,error.span().end),(0,source.len()));
    }
    let source="% π header\n  A=[1 +]; % tail";
    let error=lower(source).unwrap_err();assert_eq!(error.code(),Some("UnexpectedToken"));
    assert_eq!(error.original_source().as_ref(),source);assert_eq!(error.span().start_line,2);
    assert_eq!(&source[error.span().start..error.span().end],"]");
    for (source,code) in [("[1 2;3] + 1e","MalformedNumber"),("[1,2;3] + 1e","MalformedNumber")] {
        assert_eq!(lower(source).unwrap_err().code(),Some(code));
    }
}

#[test]
fn matrix_whitespace_resource_caps_charge_boundaries_and_original_failure_spans() {
    use mathlab_language::expression::{LiteralDiagnosticDetailV1,LiteralResourceV1,LiteralStageV1};
    use mathlab_language::decimal_expression::{DecimalDiagnosticDetailV1,DecimalExpressionErrorV1};
    use mathlab_language::arithmetic_expression::ArithmeticExpressionErrorV1;
    fn resource(error:SessionExpressionErrorV1)->(LiteralResourceV1,Option<usize>,usize,usize,usize,LiteralStageV1) {
        let at=(error.span().start,error.span().end,error.stage());
        match error {
            SessionExpressionErrorV1::Arithmetic(ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(d)))=>match d.detail {
                DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::Resource {name,limit,attempted_count})=>(name,limit,attempted_count,at.0,at.1,at.2),
                other=>panic!("{other:?}"),
            },other=>panic!("{other:?}"),
        }
    }
    for separator in [" ",","] {
        let row=vec!["1";16].join(separator);let matrix=format!("[{}]",vec![row.as_str();16].join(";"));
        let value=evaluate_session_expression_v1(lower(&matrix).unwrap(),|_|None,||false).unwrap().value;
        assert_eq!(value.shape(),[16,16]);assert_eq!(value.payload_bytes(),2048);
        let extra=format!("{matrix}+1");let failure=resource(lower(&extra).unwrap_err());
        assert_eq!((failure.0,failure.1,failure.2),(LiteralResourceV1::NumberNodes,Some(256),257));
        assert_eq!((failure.3,failure.4),(extra.len()-1,extra.len()));
        for oversized in [format!("[{}]",vec!["1";17].join(separator)),format!("[{}]",vec!["1";17].join(";"))] {
            let error=lower(&oversized).unwrap_err();assert_eq!(error.code(),Some("MatrixNotAdmitted"));assert_eq!(error.stage(),LiteralStageV1::MatrixAdmission);
        }
        let term=format!("{}[1{separator}2]{}","(".repeat(31),")".repeat(31));
        let prefix=vec![term.as_str();29].join("+");
        let exactly=format!("{prefix}+{}[1{separator}2]{}","(".repeat(6),")".repeat(6));assert!(lower(&exactly).is_ok());
        let too_many=format!("{prefix}+{}[1{separator}2]{}","(".repeat(7),")".repeat(7));
        let failure=resource(lower(&too_many).unwrap_err());assert_eq!((failure.0,failure.1,failure.2),(LiteralResourceV1::AstNodes,Some(1024),1025));
        assert_eq!((failure.3,failure.4),(too_many.rfind('2').unwrap(),too_many.rfind('2').unwrap()+1));
        let signed=format!("{prefix}+{}[1{separator}-2]{}","(".repeat(5),")".repeat(5));assert!(lower(&signed).is_ok());
        let depth=format!("{}[1{separator}2]{}","(".repeat(32),")".repeat(32));let failure=resource(lower(&depth).unwrap_err());
        assert_eq!((failure.0,failure.1,failure.2),(LiteralResourceV1::OpenDelimiters,Some(32),33));assert_eq!((failure.3,failure.4),(32,33));
        let bounded=format!("[1{separator}2]%{}"," ".repeat(4090));assert_eq!(bounded.len(),4096);assert!(lower(&bounded).is_ok());
        let failure=resource(lower(&(bounded+" ")).unwrap_err());assert_eq!((failure.0,failure.1,failure.2),(LiteralResourceV1::SourceBytes,Some(4096),4097));
    }
}

#[test]
fn empty_index_linear_native_shape_table_and_extent_sixteen_boundaries() {
    // Literal expected cells transcribe the accepted native table, including prior matrix cells.
    let selectors=[[0,0],[1,0],[0,1],[0,3],[3,0]];
    for (target_shape,expected) in [
        ([1,1],[[0,0],[1,0],[0,1],[0,3],[3,0]]),
        ([1,3],[[0,0],[1,0],[1,0],[0,3],[3,0]]),
        ([3,1],[[0,0],[0,1],[0,1],[0,3],[3,0]]),
        ([2,3],[[0,0],[1,0],[0,1],[0,3],[3,0]]),
        ([0,0],[[0,0],[1,0],[0,1],[0,3],[3,0]]),
        ([1,0],[[0,0],[1,0],[1,0],[0,3],[3,0]]),
        ([0,1],[[0,0],[0,1],[0,1],[0,3],[3,0]]),
        ([0,3],[[0,0],[1,0],[0,1],[0,3],[3,0]]),
        ([3,0],[[0,0],[1,0],[0,1],[0,3],[3,0]]),
    ] {
        let target=SessionValueV1::try_from_col_major(target_shape,&vec![f64::from_bits(0x7ff8000000000042);(target_shape[0]*target_shape[1]) as usize]).unwrap();
        for (selector_shape,shape) in selectors.into_iter().zip(expected) {
            let selector=SessionValueV1::try_from_col_major(selector_shape,&[]).unwrap();
            for source in ["A(P)".to_owned(),format!("A(zeros({},{}))",selector_shape[0],selector_shape[1])] {
                let value=evaluate_session_expression_v1(lower(&source).unwrap(),|name|match name{"A"=>Some(&target),"P"=>Some(&selector),_=>None},||false).unwrap().value;
                assert_eq!(value.shape(),shape,"target={target_shape:?},selector={selector_shape:?},{source}");
                assert!(value.as_slice().is_empty());assert_eq!(value.payload_bytes(),0);
            }
        }
        assert!(target.as_slice().iter().all(|word|word.to_bits()==0x7ff8000000000042));
    }
    // These are Rust boundary obligations, not extra native measurements.
    for (target_shape,expected) in [
        ([1,16],[[0,0],[1,0],[1,0],[0,16],[16,0]]),
        ([16,1],[[0,0],[0,1],[0,1],[0,16],[16,0]]),
        ([0,16],[[0,0],[1,0],[0,1],[0,16],[16,0]]),
        ([16,0],[[0,0],[1,0],[0,1],[0,16],[16,0]]),
    ] {
        let target=SessionValueV1::try_from_col_major(target_shape,&vec![-0.;(target_shape[0]*target_shape[1]) as usize]).unwrap();
        for (selector_shape,shape) in [[0,0],[1,0],[0,1],[0,16],[16,0]].into_iter().zip(expected) {
            let selector=SessionValueV1::try_from_col_major(selector_shape,&[]).unwrap();
            let value=evaluate_session_expression_v1(lower("A(P)").unwrap(),|name|match name{"A"=>Some(&target),"P"=>Some(&selector),_=>None},||false).unwrap().value;
            assert_eq!(value.shape(),shape);assert_eq!(value.payload_bytes(),0);
        }
    }
}

#[test]
fn empty_index_cartesian_counts_complete_children_and_other_argument_diagnostics() {
    use IndexOperandReasonV1::*;
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    let e03=SessionValueV1::try_from_col_major([0,3],&[]).unwrap();
    let e30=SessionValueV1::try_from_col_major([3,0],&[]).unwrap();
    let e00=SessionValueV1::try_from_col_major([0,0],&[]).unwrap();
    let m=SessionValueV1::try_from_col_major([2,2],&[1.;4]).unwrap();
    for (source,shape) in [("A(zeros(0,3),[1,2])",[0,2]),("A([2,1],zeros(3,0))",[2,0]),
        ("A(zeros(0,3),zeros(3,0))",[0,0]),("A([],[])",[0,0]),
        ("E03([],[3,1])",[0,2]),("E30([3,1],[])",[2,0]),("E00([],[])",[0,0]),
        ("A([],ones(1,16))",[0,16]),("A(ones(16,1),[])",[16,0]),
        ("reshape(A([],[]),0,16)",[0,16]),("size(A([],ones(1,16)))",[1,2])] {
        let value=evaluate_session_expression_v1(lower(source).unwrap(),|name|match name{"A"=>Some(&a),"E03"=>Some(&e03),"E30"=>Some(&e30),"E00"=>Some(&e00),_=>None},||false).unwrap().value;
        assert_eq!(value.shape(),shape,"{source}");
        if source.starts_with("size") {assert_eq!(value.as_slice(),&[0.,16.]);} else {assert_eq!(value.payload_bytes(),0);}
    }
    for (expression,arg,element,reason,subject) in [
        ("A([],0)",1,None,NonPositive,"0"),("A(0,[])",0,None,NonPositive,"0"),
        ("A([],[4,1e309])",1,Some(1),NonFinite,"[4,1e309]"),
        ("A([3,1e309],[])",0,Some(1),NonFinite,"[3,1e309]"),
        ("A([],M)",1,None,NonScalar,"M"),("A(M,[])",0,None,NonScalar,"M")] {
        let source=format!("% π original\n  B={expression}; % tail");
        let error=evaluate_session_expression_v1(lower(&source).unwrap(),|name|match name{"A"=>Some(&a),"M"=>Some(&m),_=>None},||false).unwrap_err();
        assert_eq!(error.original_source().as_ref(),source);assert_eq!(error.span().start_line,2);assert_eq!(&source[error.span().start..error.span().end],subject);
        match error {
            SessionExpressionErrorV1::IndexOperand {argument_index,reason:r,..}=>assert_eq!((argument_index,None,r),(arg,element,reason)),
            SessionExpressionErrorV1::IndexElementOperand {argument_index,element_index,reason:r,..}=>assert_eq!((argument_index,Some(element_index),r),(arg,element,reason)),
            other=>panic!("{expression}: {other:?}"),
        }
    }
    for (source,arg,element,bound,subject) in [
        ("A([],[4,1])",1,Some(0),3,"[4,1]"),("A([3,1],[])",0,Some(0),2,"[3,1]"),
        ("E03(1,[])",0,None,0,"1"),("E30([],1)",1,None,0,"1"),("E00([],1)",1,None,0,"1"),
        ("E00(1,[])",0,None,0,"1"),("A(7,[])",0,None,2,"7")] {
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|name|match name{"A"=>Some(&a),"E03"=>Some(&e03),"E30"=>Some(&e30),"E00"=>Some(&e00),_=>None},||false).unwrap_err();
        assert_eq!(&source[error.span().start..error.span().end],subject);
        match error {
            SessionExpressionErrorV1::IndexBounds {argument_index,upper_bound,..}=>assert_eq!((argument_index,None,upper_bound),(arg,element,bound)),
            SessionExpressionErrorV1::IndexElementBounds {argument_index,element_index,upper_bound,..}=>assert_eq!((argument_index,Some(element_index),upper_bound),(arg,element,bound)),
            other=>panic!("{source}: {other:?}"),
        }
    }
    for source in ["A([],missing)","A(0,missing)","A([],missing+other)","A(missing,[])"] {
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|name|if name=="A"{Some(&a)}else{None},||false).unwrap_err();
        assert_eq!(error.code(),Some("UnknownBinding"));assert_eq!(&source[error.span().start..error.span().end],"missing");
    }
}

#[test]
fn empty_index_binding_arity_caps_polling_and_recovery() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    for name in ["A","zeros","ones","eye","size","numel","reshape","sin"] {
        let source=format!("{name}([])");
        assert_eq!(evaluate_session_expression_v1(lower(&source).unwrap(),|n|if n==name{Some(&a)}else{None},||false).unwrap().value.shape(),[0,0]);
        let source=format!("{name}([],missing,[])");
        assert!(matches!(evaluate_session_expression_v1(lower(&source).unwrap(),|n|if n==name{Some(&a)}else{None},||false),Err(SessionExpressionErrorV1::IndexArity {actual:3,..})));
    }
    for source in ["A(zeros(0,17))","A(zeros(17,0))","A([],ones(1,17))","A(ones(17,1),[])"] {
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||false).unwrap_err();
        assert!(matches!(error,SessionExpressionErrorV1::ConstructorDimension {reason:ConstructorDimensionReasonV1::AboveProfileLimit,..}));
    }
    for source in ["A([])","A([],ones(1,16))","A(zeros(16,0))"] {
        let mut total=0;
        evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||{total+=1;false}).unwrap();
        for stop_at in 1..=total {
            let mut polls=0;
            assert!(matches!(evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||{polls+=1;polls==stop_at}),Err(SessionExpressionErrorV1::Cancelled {..})));
            assert_eq!(a.as_slice(),&[1.,4.,2.,5.,3.,6.]);
        }
        assert_eq!(evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||false).unwrap().value.payload_bytes(),0);
    }
    assert_eq!(evaluate_session_expression_v1(lower("A(2)").unwrap(),|n|if n=="A"{Some(&a)}else{None},||false).unwrap().value.as_slice(),&[4.]);
}

#[test]
fn colon_index_native_table_values_empty_shapes_order_ranges_and_word_independence() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    let r=SessionValueV1::try_from_col_major([1,3],&[10.,20.,30.]).unwrap();
    let c=SessionValueV1::try_from_col_major([3,1],&[10.,20.,30.]).unwrap();
    let scalar=SessionValueV1::try_from_col_major([1,1],&[9.]).unwrap();
    for (source,shape,expected) in [
        ("A(:)",[6,1],vec![1.,4.,2.,5.,3.,6.]),("A(:,:)",[2,3],vec![1.,4.,2.,5.,3.,6.]),
        ("R(:)",[3,1],vec![10.,20.,30.]),("C(:)",[3,1],vec![10.,20.,30.]),("S(:)",[1,1],vec![9.]),
        ("A(:,[3,1])",[2,2],vec![3.,6.,1.,4.]),("A([2,1],:)",[2,3],vec![4.,1.,5.,2.,6.,3.]),
        ("A(:,3:-1:1)",[2,3],vec![3.,6.,2.,5.,1.,4.]),("A(2:-1:1,:)",[2,3],vec![4.,1.,5.,2.,6.,3.]),
        ("A(:,[])",[2,0],vec![]),("A([],:)",[0,3],vec![]),("A(:,zeros(0,3))",[2,0],vec![]),
        ("A(zeros(3,0),:)",[0,3],vec![]),
    ] {
        let value=evaluate_session_expression_v1(lower(source).unwrap(),|name|match name{"A"=>Some(&a),"R"=>Some(&r),"C"=>Some(&c),"S"=>Some(&scalar),_=>None},||false).unwrap().value;
        assert_eq!(value.shape(),shape,"{source}");assert_eq!(value.as_slice(),expected,"{source}");
    }
    for target_shape in [[0,0],[1,0],[0,1],[0,3],[3,0],[0,16],[16,0]] {
        let empty=SessionValueV1::try_from_col_major(target_shape,&[]).unwrap();
        for (source,expected) in [("E(:)",[0,1]),("E(:,:)",target_shape)] {
            let value=evaluate_session_expression_v1(lower(source).unwrap(),|name|if name=="E"{Some(&empty)}else{None},||false).unwrap().value;
            assert_eq!(value.shape(),expected);assert_eq!(value.payload_bytes(),0);
        }
    }
    let bits=[0x8000000000000000,0x7ff8000000000042,0x7ff0000000000000,0xfff0000000000000];
    let host=SessionValueV1::try_from_col_major([2,2],&bits.map(f64::from_bits)).unwrap();
    for (source,shape,expected) in [("H(:)",[4,1],bits.to_vec()),("H(:,:)",[2,2],bits.to_vec()),
        ("H([2,1],:)",[2,2],vec![bits[1],bits[0],bits[3],bits[2]])] {
        let value=evaluate_session_expression_v1(lower(source).unwrap(),|name|if name=="H"{Some(&host)}else{None},||false).unwrap().value;
        assert_eq!(value.shape(),shape);assert_eq!(value.as_slice().iter().map(|word|word.to_bits()).collect::<Vec<_>>(),expected);
        assert_ne!(value.as_slice().as_ptr(),host.as_slice().as_ptr());
    }
    assert_eq!(host.as_slice().iter().map(|word|word.to_bits()).collect::<Vec<_>>(),bits);
}

#[test]
fn colon_index_marker_stack_nested_composition_binding_and_context_policy() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    let b=SessionValueV1::try_from_col_major([1,2],&[3.,1.]).unwrap();
    for (source,shape,expected) in [
        ("10+A(1,1)",[1,1],vec![11.]),("10+A(:)",[6,1],vec![11.,14.,12.,15.,13.,16.]),
        ("10+A(:,:)",[2,3],vec![11.,14.,12.,15.,13.,16.]),
        ("10+A(:,B(:))",[2,2],vec![13.,16.,11.,14.]),
        ("10+A(B(:),:)",[2,3],vec![14.,11.,15.,12.,16.,13.]),
        ("numel(A(:))",[1,1],vec![6.]),("size(A(:))",[1,2],vec![6.,1.]),
        ("reshape(A(:),3,2)",[3,2],vec![1.,4.,2.,5.,3.,6.]),
        ("A(:).'+[1 1 1 1 1 1]",[1,6],vec![2.,5.,3.,6.,4.,7.]),
    ] {
        // B uses [2,1] in the row-selector case, retaining independent expected values.
        let row=SessionValueV1::try_from_col_major([1,2],&[2.,1.]).unwrap();
        let value=evaluate_session_expression_v1(lower(source).unwrap(),|name|match name{"A"=>Some(&a),"B"=>Some(if source.contains("A(B") {&row} else {&b}),_=>None},||false).unwrap().value;
        assert_eq!(value.shape(),shape,"{source}");assert_eq!(value.as_slice(),expected,"{source}");
    }
    for name in ["A","zeros","ones","eye","size","numel","reshape","sin"] {
        let source=format!("{name}(:)");
        assert_eq!(evaluate_session_expression_v1(lower(&source).unwrap(),|n|if n==name{Some(&a)}else{None},||false).unwrap().value.shape(),[6,1]);
        let source=format!("{name}(:,missing,:)");
        assert!(matches!(evaluate_session_expression_v1(lower(&source).unwrap(),|n|if n==name{Some(&a)}else{None},||false),Err(SessionExpressionErrorV1::IndexArity {actual:3,..})));
    }
    for source in ["zeros(:)","ones(:,2)","eye(2,:)","size(:)","numel(:)","reshape([],0,:)"] {
        let framed=format!("% π original\n  {source}; % tail");
        let error=evaluate_session_expression_v1(lower(&framed).unwrap(),|_|None,||false).unwrap_err();
        assert_eq!(error.code(),Some("UnsupportedCall"));assert_eq!(&framed[error.span().start..error.span().end],":");assert_eq!(error.span().start_line,2);
    }
    for source in ["zeros(:,missing)","ones(missing,:)","reshape(:,missing,other)"] {
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|_|None,||false).unwrap_err();
        assert_eq!(error.code(),Some("UnknownBinding"));assert_eq!(&source[error.span().start..error.span().end],"missing");
    }
    for (source,code,subject) in [("numel(:,missing)","CallArity","numel(:,missing)"),
        ("unknown(:,missing)","UnsupportedCall","unknown"),("A(:,missing)","UnknownBinding","missing"),
        ("A(missing,:)","UnknownBinding","missing")] {
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||false).unwrap_err();
        assert_eq!(error.code(),Some(code));assert_eq!(&source[error.span().start..error.span().end],subject);
    }
}

#[test]
fn colon_index_other_argument_ordinals_domains_bounds_caps_and_cancellation() {
    use IndexOperandReasonV1::*;
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    for (source,arg,element,reason) in [("A(:,[4,1e309])",1,Some(1),NonFinite),("A([3,1e309],:)",0,Some(1),NonFinite),
        ("A(:,0)",1,None,NonPositive),("A([0,1],:)",0,Some(0),NonPositive),("A(:,[1,2;1,2])",1,None,NonScalar)] {
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||false).unwrap_err();
        match error {
            SessionExpressionErrorV1::IndexOperand {argument_index,reason:r,..}=>assert_eq!((argument_index,None,r),(arg,element,reason)),
            SessionExpressionErrorV1::IndexElementOperand {argument_index,element_index,reason:r,..}=>assert_eq!((argument_index,Some(element_index),r),(arg,element,reason)),
            other=>panic!("{source}: {other:?}"),
        }
    }
    for (target_shape,source,arg,bound) in [([0,3],"A(:,4)",1,3),([3,0],"A(4,:)",0,3),
        ([0,0],"A(:,1)",1,0),([0,0],"A(1,:)",0,0)] {
        let empty=SessionValueV1::try_from_col_major(target_shape,&[]).unwrap();
        let error=evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&empty)}else{None},||false).unwrap_err();
        assert!(matches!(error,SessionExpressionErrorV1::IndexBounds {argument_index,upper_bound,..} if (argument_index,upper_bound)==(arg,bound)));
    }
    for shape in [[4,4],[1,16]] {
        let target=SessionValueV1::try_from_col_major(shape,&(1..=16).map(f64::from).collect::<Vec<_>>()).unwrap();
        let value=evaluate_session_expression_v1(lower("M(:)").unwrap(),|n|if n=="M"{Some(&target)}else{None},||false).unwrap().value;
        assert_eq!(value.shape(),[16,1]);assert_eq!(value.as_slice(),&(1..=16).map(f64::from).collect::<Vec<_>>());
    }
    for (shape,count) in [([2,9],18),([16,16],256)] {
        let target=SessionValueV1::try_from_col_major(shape,&vec![f64::from_bits(0x7ff8000000000042);count]).unwrap();
        let error=evaluate_session_expression_v1(lower("M(:)").unwrap(),|n|if n=="M"{Some(&target)}else{None},||false).unwrap_err();
        assert_eq!(error.code(),Some("ArrayOperation"));
        match error {
            SessionExpressionErrorV1::Arithmetic(mathlab_language::arithmetic_expression::ArithmeticExpressionErrorV1::Shared(
                mathlab_language::decimal_expression::DecimalExpressionErrorV1::Diagnostic(d)))=>match d.detail {
                mathlab_language::decimal_expression::DecimalDiagnosticDetailV1::Profile(
                    mathlab_language::expression::LiteralDiagnosticDetailV1::ArrayOperation {error})=>
                    assert_eq!(error.code,mathlab_array::ErrorCodeV1::DimensionLimitExceeded {axis:0,actual:count as u64,limit:16}),
                other=>panic!("{other:?}"),
            },other=>panic!("{other:?}"),
        }
        let value=evaluate_session_expression_v1(lower("M(:,:)").unwrap(),|n|if n=="M"{Some(&target)}else{None},||false).unwrap().value;
        assert_eq!(value.shape(),shape);assert_eq!(value.as_slice().len(),count);assert!(value.as_slice().iter().all(|w|w.to_bits()==0x7ff8000000000042));
    }
    for source in ["A(:)","A(:,[3,1])","A([2,1],:)","A(:,:)","A(:,[])"] {
        let mut total=0;evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||{total+=1;false}).unwrap();
        for stop_at in 1..=total {let mut polls=0;assert!(matches!(evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||{polls+=1;polls==stop_at}),Err(SessionExpressionErrorV1::Cancelled {..})));}
        assert_eq!(a.as_slice(),&[1.,4.,2.,5.,3.,6.]);
    }
}

#[test]
fn colon_index_bare_argument_grammar_non_session_profiles_and_ast_limit() {
    use mathlab_language::expression::parse_literal_expression_v1;
    use mathlab_language::decimal_expression::parse_decimal_expression_v1;
    use mathlab_language::arithmetic_expression::parse_arithmetic_expression_v1;
    for source in [":","(:)","A((:))","A(:+1)","A([:])","A(:,)","A(,:)","A(:)=","(A)(:)","A(1)(:)"] {assert!(lower(source).is_err(),"{source}");}
    for source in ["A(:)","A(:,1)","A(1,:)","A(:,:)"] {
        assert!(parse_literal_expression_v1(Arc::from(source)).is_err());assert!(parse_decimal_expression_v1(Arc::from(source)).is_err());assert!(parse_arithmetic_expression_v1(Arc::from(source)).is_err());
    }
    let terms=std::iter::repeat_n("A(:)",256).collect::<Vec<_>>().join("+");
    assert!(lower(&format!("+{terms}")).is_ok());
    let error=lower(&format!("++{terms}")).unwrap_err();assert_eq!(error.code(),Some("ResourceLimit"));
}

#[test]
fn contextual_end_linear_axes_colons_ranges_and_selected_bits() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    for (source,shape,words) in [
        ("A(end)",[1,1],vec![6.]),("A((end)-1)",[1,1],vec![3.]),
        ("A(end,end)",[1,1],vec![6.]),("A(end,:)",[1,3],vec![4.,5.,6.]),
        ("A(:,end)",[2,1],vec![3.,6.]),("A(1:end-1,end)",[1,1],vec![3.]),
        ("A(end:-2:1)",[1,3],vec![6.,5.,4.]),("A(end+1:end)",[1,0],vec![]),
    ] {
        let value=evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||false).unwrap().value;
        assert_eq!(value.shape(),shape,"{source}");assert_eq!(value.as_slice(),words,"{source}");
    }
    let words=[0x8000000000000000,0x7ff8000000000042];
    let h=SessionValueV1::try_from_col_major([1,2],&words.map(f64::from_bits)).unwrap();
    let value=evaluate_session_expression_v1(lower("H(end:-1:1)").unwrap(),|_|Some(&h),||false).unwrap().value;
    assert_eq!(value.as_slice().iter().map(|w|w.to_bits()).collect::<Vec<_>>(),[words[1],words[0]]);
    drop(h);assert_eq!(value.as_slice()[0].to_bits(),words[1]);
}

#[test]
fn contextual_end_ordinary_inheritance_nested_rebinding_and_depth() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    let b=SessionValueV1::try_from_col_major([1,3],&[1.,2.,3.]).unwrap();
    for (source,expected) in [("A(reshape(end,1,1))",6.),("A(reshape(reshape(end,1,1),1,1))",6.),
        ("A(numel(ones(1,end)))",6.),("A(B(end))",2.),("A(B(end-1))",4.),("A(B(end-1),end)",6.),
        ("A(reshape(B(end),1,1))",2.),("A(B(1)+end-1)",6.)] {
        let value=evaluate_session_expression_v1(lower(source).unwrap(),|n|match n{"A"=>Some(&a),"B"=>Some(&b),_=>None},||false).unwrap().value;
        assert_eq!(value.as_slice(),&[expected],"{source}");
    }
    let source=format!("A({}end{})","(".repeat(30),")".repeat(30));
    assert_eq!(evaluate_session_expression_v1(lower(&source).unwrap(),|_|Some(&a),||false).unwrap().value.as_slice(),&[6.]);
    let e=evaluate_session_expression_v1(lower("B(A(end))").unwrap(),|n|match n{"A"=>Some(&a),"B"=>Some(&b),_=>None},||false).unwrap_err();
    assert_eq!(e.code(),Some("IndexBounds"));
}

#[test]
fn contextual_end_keyword_isolation_preflight_and_malformed_diagnostics() {
    for source in ["end","(end)","end=1","end(1)","A(end())","A(1)+end"] {
        let error=lower(source).unwrap_err();assert_eq!(error.code(),Some("InvalidIdentifier"),"{source}");assert_eq!(error.stage(),mathlab_language::expression::LiteralStageV1::Lex);
        assert_eq!(&source[error.span().start..error.span().end],"end");
    }
    for source in ["A(end+)","A(end,)"] {let e=lower(source).unwrap_err();assert_ne!(e.code(),Some("InvalidIdentifier"));assert_eq!(e.stage(),mathlab_language::expression::LiteralStageV1::Parse);}
    let source="A(end,for)";let e=lower(source).unwrap_err();assert_eq!(e.code(),Some("InvalidIdentifier"));assert_eq!(&source[e.span().start..e.span().end],"for");
    for source in ["End","end1","endless"] {assert_eq!(evaluate_session_expression_v1(lower(source).unwrap(),|_|None,||false).unwrap_err().code(),Some("UnknownBinding"));}
    let a=SessionValueV1::try_from_col_major([1,2],&[1.,2.]).unwrap();
    for (source,code) in [("A(end,missing,1)","IndexArity"),("A(end,missing)","UnknownBinding"),("A(end*0,missing)","UnknownBinding"),("sin(end)","UnsupportedCall"),("numel(end,missing)","CallArity"),("numel(end)","UnsupportedCall")] {
        assert_eq!(evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||false).unwrap_err().code(),Some(code),"{source}");
    }
    for source in ["A(end)","numel(end)","A(1:end)"] {assert!(parse_literal_expression_v1(Arc::from(source)).is_err());assert!(parse_decimal_expression_v1(Arc::from(source)).is_err());assert!(parse_arithmetic_expression_v1(Arc::from(source)).is_err());}
}

#[test]
fn contextual_end_empty_axes_domain_bounds_and_caps() {
    for shape in [[0,0],[0,1],[1,0],[0,3],[3,0]] {
        let e=SessionValueV1::try_from_col_major(shape,&[]).unwrap();
        for source in ["E(end)","E(end:end)"] {assert_eq!(evaluate_session_expression_v1(lower(source).unwrap(),|_|Some(&e),||false).unwrap_err().code(),Some("IndexOperand"));}
        for source in ["E(1:end)","E(end+1:end)"] {assert!(evaluate_session_expression_v1(lower(source).unwrap(),|_|Some(&e),||false).unwrap().value.as_slice().is_empty());}
    }
    let e=SessionValueV1::try_from_col_major([0,3],&[]).unwrap();
    assert_eq!(evaluate_session_expression_v1(lower("E(:,end)").unwrap(),|_|Some(&e),||false).unwrap().value.shape(),[0,1]);
    assert_eq!(evaluate_session_expression_v1(lower("E(end,:)").unwrap(),|_|Some(&e),||false).unwrap_err().code(),Some("IndexOperand"));
    assert_eq!(evaluate_session_expression_v1(lower("E(:,end+1)").unwrap(),|_|Some(&e),||false).unwrap_err().code(),Some("IndexBounds"));
    let e=SessionValueV1::try_from_col_major([3,0],&[]).unwrap();
    assert_eq!(evaluate_session_expression_v1(lower("E(end,:)").unwrap(),|_|Some(&e),||false).unwrap().value.shape(),[1,0]);
    let a=SessionValueV1::try_from_col_major([16,16],&[1.;256]).unwrap();
    for source in ["A(end)","A(1:16,end)","A(:,end)"] {assert!(evaluate_session_expression_v1(lower(source).unwrap(),|_|Some(&a),||false).is_ok());}
    for source in ["A(1:end)","A(end:-1:1)"] {assert_eq!(evaluate_session_expression_v1(lower(source).unwrap(),|_|Some(&a),||false).unwrap_err().code(),Some("RangeLength"));}
    for (source,code) in [("A(end+1)","IndexBounds"),("A(end-end)","IndexOperand"),("A(end.*1e309)","IndexOperand"),("A(end-0.5)","IndexOperand")] {assert_eq!(evaluate_session_expression_v1(lower(source).unwrap(),|_|Some(&a),||false).unwrap_err().code(),Some(code),"{source}");}
}

#[test]
fn contextual_end_source_spans_polling_names_and_recovery() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    let source="% π header\n  B=A(end+1); % tail";
    let e=evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||false).unwrap_err();
    assert_eq!(e.code(),Some("IndexBounds"));assert_eq!(&source[e.span().start..e.span().end],"end+1");assert_eq!(e.span().start_line,2);assert_eq!(e.original_source().as_ref(),source);
    for name in ["zeros","ones","eye","size","numel","reshape","sin"] {let source=format!("{name}(end)");assert_eq!(evaluate_session_expression_v1(lower(&source).unwrap(),|n|if n==name{Some(&a)}else{None},||false).unwrap().value.as_slice(),&[6.]);}
    let mut polls=0;evaluate_session_expression_v1(lower("A(reshape(end-1,1,1))").unwrap(),|n|if n=="A"{Some(&a)}else{None},||{polls+=1;false}).unwrap();
    for stop in 1..=polls {let mut seen=0;assert_eq!(evaluate_session_expression_v1(lower("A(reshape(end-1,1,1))").unwrap(),|n|if n=="A"{Some(&a)}else{None},||{seen+=1;seen==stop}).unwrap_err().code(),Some("Cancelled"));}
    assert_eq!(evaluate_session_expression_v1(lower("A(end)").unwrap(),|_|Some(&a),||false).unwrap().value.as_slice(),&[6.]);
}

#[test]
fn construction_full_operands_column_major_nested_geometry_and_composition() {
    let a=SessionValueV1::try_from_col_major([2,1],&[1.,2.]).unwrap();
    let b=SessionValueV1::try_from_col_major([2,2],&[3.,4.,5.,6.]).unwrap();
    for (source,shape,words) in [
        ("[A B]",[2,3],vec![1.,2.,3.,4.,5.,6.]),
        ("[A;A]",[4,1],vec![1.,2.,1.,2.]),
        ("[A,B;A,B]",[4,3],vec![1.,2.,1.,2.,3.,4.,3.,4.,5.,6.,5.,6.]),
        ("[[A B];[A B]]",[4,3],vec![1.,2.,1.,2.,3.,4.,3.,4.,5.,6.,5.,6.]),
        ("[ones(1,2);1,2]",[2,2],vec![1.,1.,1.,2.]),
        ("[1,2;ones(1,2)]",[2,2],vec![1.,1.,2.,1.]),
        ("[1:3 4+1]",[1,4],vec![1.,2.,3.,5.]),
        ("[A.' -A.']",[1,4],vec![1.,2.,-1.,-2.]),
        ("[ones(1,2).*2,ones(1,2)./2]",[1,4],vec![2.,2.,0.5,0.5]),
        ("[1+2,3*4]",[1,2],vec![3.,12.]),
        ("[ones(2,1),ones(2,2);ones(1,3)]",[3,3],vec![1.;9]),
    ] {
        let value=evaluate_session_expression_v1(lower(source).unwrap(),|n|match n{"A"=>Some(&a),"B"=>Some(&b),_=>None},||false).unwrap().value;
        assert_eq!(value.shape(),shape,"{source}");assert_eq!(value.as_slice(),words,"{source}");
    }
    for source in ["[A 1]","[A;B]","[ones(1,2);ones(1,1)]"] {
        let e=evaluate_session_expression_v1(lower(source).unwrap(),|n|match n{"A"=>Some(&a),"B"=>Some(&b),_=>None},||false).unwrap_err();
        assert_eq!(e.code(),Some("ArrayOperation"));
    }
    for source in ["horzcat(A,A)","vertcat(A,A)"] {assert_eq!(evaluate_session_expression_v1(lower(source).unwrap(),|_|None,||false).unwrap_err().code(),Some("UnsupportedCall"));}
}

#[test]
fn construction_gap_attached_sign_call_group_and_profile_boundaries() {
    let a=SessionValueV1::try_from_col_major([1,1],&[2.]).unwrap();
    for (source,expected) in [
        ("[1 -2]",vec![1.,-2.]),("[1 - 2]",vec![-1.]),("[1-2]",vec![-1.]),
        ("[1 +2]",vec![1.,2.]),("[1 + 2]",vec![3.]),("[1+2]",vec![3.]),
        ("[1 --2]",vec![1.,2.]),("[1,--2]",vec![1.,2.]),("[-+2]",vec![-2.]),
        ("[A -A]",vec![2.,-2.]),("[A - A]",vec![0.]),("[A --A]",vec![2.,2.]),
        ("[(A - A) A]",vec![0.,2.]),("[reshape(A - A,1,1) A]",vec![0.,2.]),
        ("[A +(A)]",vec![2.,2.]),("[numel(ones (1)) 2]",vec![1.,2.]),
    ] {
        let v=evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||false).unwrap().value;
        assert_eq!(v.as_slice(),expected,"{source}");
    }
    let a=SessionValueV1::try_from_col_major([1,2],&[7.,8.]).unwrap();
    for (source,expected) in [("[A (1)]",vec![7.,8.,1.]),("[(A) (1)]",vec![7.,8.,1.]),("[A(1)]",vec![7.]),("[numel(A (1))]",vec![1.])] {
        assert_eq!(evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||false).unwrap().value.as_slice(),expected,"{source}");
    }
    for source in ["[A]","[1+2]","[[1],[2]]","[1:2]","A([end])"] {
        assert!(parse_literal_expression_v1(Arc::from(source)).is_err());assert!(parse_decimal_expression_v1(Arc::from(source)).is_err());assert!(parse_arithmetic_expression_v1(Arc::from(source)).is_err());
    }
    for source in ["[;]","[1;]","[1,]","[,1]","[1,,2]","[1(2)]","[1[2]]","[1\n2]","[1 % gap\n2]","[1 ...\n2]","[A](1)","[A](1)=2"] {assert!(lower(source).is_err(),"{source}");}
}

#[test]
fn construction_whole_operand_empty_geometry_flat_nested_and_actual_caps() {
    for (source,shape) in [
        ("[zeros(0,2),zeros(0,3)]",[0,5]),
        ("[zeros(2,0);zeros(3,0)]",[5,0]),
        ("[zeros(2,1),zeros(0,3)]",[2,1]),
        ("[zeros(2,1),zeros(3,0)]",[2,1]),
        ("[zeros(1,2);zeros(3,0)]",[1,2]),
        ("[zeros(1,2);zeros(0,3)]",[1,2]),
        ("[zeros(0,2);zeros(0,3)]",[0,3]),
        ("[zeros(2,0),zeros(3,0)]",[3,0]),
        ("[zeros(0,2),zeros(3,0)]",[0,0]),
        ("[zeros(0,2);zeros(3,0)]",[0,0]),
        ("[[zeros(0,2),zeros(0,3)];zeros(0,4)]",[0,5]),
        ("[zeros(0,0),zeros(0,3)]",[0,3]),
        ("[zeros(3,0);zeros(0,0)]",[3,0]),
        ("[zeros(0,3);zeros(0,2)]",[0,3]),
        ("[zeros(3,0),zeros(2,0)]",[3,0]),
        ("[zeros(0,2),zeros(3,0),zeros(0,4)]",[0,0]),
        ("[[zeros(0,2),zeros(3,0)],zeros(0,4)]",[0,4]),
        ("[zeros(0,2);zeros(3,0);zeros(0,4)]",[0,0]),
        ("[[zeros(0,2);zeros(3,0)];zeros(0,4)]",[0,4]),
        ("[zeros(3,0),zeros(0,2)]",[0,0]),
        ("[zeros(3,0);zeros(0,2)]",[0,0]),
        ("[zeros(0,2);zeros(3,0);zeros(4,0)]",[0,0]),
        ("[[zeros(0,2);zeros(3,0)];zeros(4,0)]",[4,0]),
        ("[[],zeros(0,3)]",[0,3]),
        ("[zeros(3,0);[]]",[3,0]),
        ("[ones(2,1),zeros(0,3),zeros(3,0)]",[2,1]),
        ("[ones(1,2);zeros(3,0);zeros(0,3)]",[1,2]),
        ("[zeros(0,16),zeros(0,16),zeros(1,0)]",[0,0]),
        ("[zeros(16,0);zeros(16,0);zeros(0,1)]",[0,0]),
        ("[ones(16,8),ones(16,8)]",[16,16]),
    ] {
        let v=evaluate_session_expression_v1(lower(source).unwrap(),|_|None,||false).unwrap().value;
        assert_eq!(v.shape(),shape,"{source}");assert_eq!(v.as_slice().len(),(shape[0]*shape[1])as usize);
    }
    for source in ["[zeros(0,16),zeros(0,1)]","[zeros(16,0);zeros(1,0)]","[ones(1,16),1]","[[zeros(0,16),zeros(0,1)],zeros(1,0)]"] {
        let e=evaluate_session_expression_v1(lower(source).unwrap(),|_|None,||false).unwrap_err();assert_eq!(e.code(),Some("ArrayOperation"),"{source}");
    }
}

#[test]
fn construction_row_scheduler_preflight_admission_and_original_bracket_spans() {
    for (source,code,subject) in [
        ("[ones(2,1),ones(1,1),missing]","UnknownBinding","missing"),
        ("[ones(2,1),ones(1,1);missing,1]","ArrayOperation","[ones(2,1),ones(1,1);missing,1]"),
        ("[ones(1,2);ones(1,1);missing]","UnknownBinding","missing"),
        ("[missing;ones(1,2);ones(1,1)]","UnknownBinding","missing"),
        ("[[1,2;3],missing]","MatrixNotAdmitted","[1,2;3]"),
        ("[missing,ones(1,17)]","UnknownBinding","missing"),
        ("[ones(1,17),missing]","ConstructorDimension","17"),
        ("unknown([missing])","UnsupportedCall","unknown"),
    ] {
        let source=format!("% π original\n  B={source}; % tail");
        let e=match lower(&source) {Err(e)=>e,Ok(ir)=>evaluate_session_expression_v1(ir,|_|None,||false).unwrap_err()};
        assert_eq!(e.code(),Some(code),"{source}");assert_eq!(&source[e.span().start..e.span().end],subject,"{source}");assert_eq!(e.span().start_line,2);assert_eq!(e.original_source().as_ref(),source);
    }
    for source in ["[ones(2,1),ones(1,1)]","[ones(1,2);ones(1,1)]"] {let e=evaluate_session_expression_v1(lower(source).unwrap(),|_|None,||false).unwrap_err();assert_eq!(e.code(),Some("ArrayOperation"));}
}

#[test]
fn construction_matrix_context_end_rebinding_inheritance_and_keyword_isolation() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    let b=SessionValueV1::try_from_col_major([1,2],&[1.,2.]).unwrap();
    for (source,shape,words) in [
        ("A([end])",[1,1],vec![6.]),("A([1,end])",[1,2],vec![1.,6.]),
        ("A([[end-1],end])",[1,2],vec![3.,6.]),("[A(end)]",[1,1],vec![6.]),
        ("A([reshape(end,1,1)])",[1,1],vec![6.]),("A([B(end),end])",[1,2],vec![4.,6.]),
        ("A([end;1],[end])",[2,1],vec![6.,3.]),("A([numel(end)])",[1,1],vec![1.]),
    ] {
        let v=evaluate_session_expression_v1(lower(source).unwrap(),|n|match n{"A"=>Some(&a),"B"=>Some(&b),_=>None},||false).unwrap().value;assert_eq!(v.shape(),shape,"{source}");assert_eq!(v.as_slice(),words,"{source}");
    }
    for source in ["[end]","[[end]]","[A (end)]","[end(1)]","[End end]","[A(end),end]"] {assert_eq!(lower(source).unwrap_err().code(),Some("InvalidIdentifier"),"{source}");}
    assert_eq!(evaluate_session_expression_v1(lower("numel([end])").unwrap(),|_|None,||false).unwrap_err().code(),Some("UnsupportedCall"));
}

#[test]
fn construction_stack_values_quotas_word_independence_and_task_cancellation() {
    let words=[0x8000000000000000,0x7ff8000000000042,0x7ff0000000000000];
    let a=SessionValueV1::try_from_col_major([1,3],&words.map(f64::from_bits)).unwrap();
    let v=evaluate_session_expression_v1(lower("[A,A;A,A]").unwrap(),|_|Some(&a),||false).unwrap().value;
    let expected=words.iter().flat_map(|w|[*w,*w]).cycle().take(12).collect::<Vec<_>>();assert_eq!(v.as_slice().iter().map(|w|w.to_bits()).collect::<Vec<_>>(),expected);drop(a);assert_eq!(v.as_slice()[2].to_bits(),words[1]);
    let source="[ones(2,1),zeros(2,1);ones(1,2)]";let mut polls=0;evaluate_session_expression_v1(lower(source).unwrap(),|_|None,||{polls+=1;false}).unwrap();
    for stop in 1..=polls {let mut seen=0;assert_eq!(evaluate_session_expression_v1(lower(source).unwrap(),|_|None,||{seen+=1;seen==stop}).unwrap_err().code(),Some("Cancelled"));}
    let source=format!("{}ones(1){}","[".repeat(31),"]".repeat(31));assert_eq!(evaluate_session_expression_v1(lower(&source).unwrap(),|_|None,||false).unwrap().value.as_slice(),&[1.]);
    let source=format!("[{}]",vec!["ones(0,0)";100].join(";"));assert_eq!(evaluate_session_expression_v1(lower(&source).unwrap(),|_|None,||false).unwrap().value.shape(),[0,0]);
    let empty=SessionValueV1::try_from_col_major([0,0],&[]).unwrap();
    for separator in [",",";"] {
        let source=format!("[{}]",vec!["E";1023].join(separator));
        assert_eq!(evaluate_session_expression_v1(lower(&source).unwrap(),|n|if n=="E"{Some(&empty)}else{None},||false).unwrap().value.shape(),[0,0]);
        let over=format!("[{}]",vec!["E";1024].join(separator));assert_eq!(lower(&over).unwrap_err().code(),Some("ResourceLimit"));
    }
    let source=format!("[missing,{}]",vec!["1";257].join(","));assert_eq!(lower(&source).unwrap_err().code(),Some("ResourceLimit"));
}

#[test]
fn indexed_write_linear_scalar_vectors_aliases_full_target_and_independence() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    for (source,expected) in [
        ("A(2)=9",vec![1.,9.,2.,5.,3.,6.]),
        ("A([6,2,1])=[7;8;9]",vec![9.,8.,2.,5.,3.,7.]),
        ("A([1,1,2])=[7,8,9]",vec![8.,9.,2.,5.,3.,6.]),
        ("A([2,1])=A([1,2])",vec![4.,1.,2.,5.,3.,6.]),
        ("A(1:2:end)=0",vec![0.,4.,0.,5.,0.,6.]),
        ("A(:)=[11,12;13,14;15,16]",vec![11.,13.,15.,12.,14.,16.]),
        ("A([])=3",vec![1.,4.,2.,5.,3.,6.]),
        ("A([])=zeros(0,1)",vec![1.,4.,2.,5.,3.,6.]),
        ("A(end)=A(1)",vec![1.,4.,2.,5.,3.,1.]),
        ("A(reshape(end,1,1))=0",vec![1.,4.,2.,5.,3.,0.]),
    ] {
        let result=evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||false).unwrap();
        assert_eq!(result.value.shape(),[2,3],"{source}");assert_eq!(result.value.as_slice(),expected,"{source}");
        match result.effect {SessionStatementEffectV1::Assignment {target}=>assert_eq!(&source[target.start..target.end],"A"),other=>panic!("{other:?}")};
        assert_eq!(a.as_slice(),&[1.,4.,2.,5.,3.,6.]);
    }
    let words=[0x8000000000000000,0x7ff8000000000042];let h=SessionValueV1::try_from_col_major([1,2],&words.map(f64::from_bits)).unwrap();
    let v=evaluate_session_expression_v1(lower("H([2,1])=H").unwrap(),|_|Some(&h),||false).unwrap().value;
    assert_eq!(v.as_slice().iter().map(|w|w.to_bits()).collect::<Vec<_>>(),[words[1],words[0]]);drop(h);assert_eq!(v.as_slice()[0].to_bits(),words[1]);
}

#[test]
fn indexed_write_cartesian_orientation_duplicates_end_and_full_256_slots() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    let b=SessionValueV1::try_from_col_major([1,2],&[1.,2.]).unwrap();
    for (source,expected) in [
        ("A([2,1],[3,1])=[10,30;20,40]",vec![40.,30.,2.,5.,20.,10.]),
        ("A([1,1],[1,1])=[10,30;20,40]",vec![40.,4.,2.,5.,3.,6.]),
        ("A(1,:)=[7;8;9]",vec![7.,4.,8.,5.,9.,6.]),
        ("A(:,end)=[7,8]",vec![1.,4.,2.,5.,7.,8.]),
        ("A(:,end)=0",vec![1.,4.,2.,5.,0.,0.]),
        ("A(B(end),end)=9",vec![1.,4.,2.,5.,3.,9.]),
        ("A([end;1],end)=A([1,end])",vec![1.,4.,2.,5.,6.,1.]),
    ] {
        let v=evaluate_session_expression_v1(lower(source).unwrap(),|n|match n{"A"=>Some(&a),"B"=>Some(&b),_=>None},||false).unwrap().value;assert_eq!(v.shape(),[2,3]);assert_eq!(v.as_slice(),expected,"{source}");
    }
    for source in ["A([1,2],[1,2])=[1,2,3,4]","A([1,2],[1,2])=[1;2;3;4]","A([1,2])=[1,2,3]"] {
        assert_eq!(evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||false).unwrap_err().code(),Some("ArrayOperation"),"{source}");
    }
    let big=SessionValueV1::try_from_col_major([16,16],&[1.;256]).unwrap();
    for source in ["M(:)=2","M(:,:)=ones(16).*3","M(256)=4"] {
        let v=evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="M"{Some(&big)}else{None},||false).unwrap().value;assert_eq!(v.shape(),[16,16]);assert_eq!(v.as_slice().len(),256);
        if source=="M(256)=4" {assert_eq!(v.as_slice()[255],4.);assert_eq!(v.as_slice()[0],1.);}else {assert!(v.as_slice().iter().all(|w|*w==if source=="M(:)=2"{2.}else{3.}));}
    }
    for (source,code) in [("M(:)","ArrayOperation"),("M(1:end)=2","RangeLength"),("M(257)=2","IndexBounds")] {assert_eq!(evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="M"{Some(&big)}else{None},||false).unwrap_err().code(),Some(code));}
}

#[test]
fn indexed_write_empty_rhs_truthful_variant_full_span_and_ordinary_empty_noop() {
    use mathlab_language::expression::LiteralStageV1;
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    let e=SessionValueV1::try_from_col_major([0,3],&[]).unwrap();let z=SessionValueV1::try_from_col_major([0,0],&[]).unwrap();let c=SessionValueV1::try_from_col_major([0,1],&[]).unwrap();
    for (lhs,rhs) in [("A(1)","[]"),("A([])","zeros(0,0)"),("A(1)","Z"),("A(1)","((Z))"),("E(:,1)","[]"),("E(:,1)","[zeros(0,2),zeros(3,0)]")] {
        let source=format!("% π header\n  {lhs} = {rhs}; % tail");let error=evaluate_session_expression_v1(lower(&source).unwrap(),|n|match n{"A"=>Some(&a),"E"=>Some(&e),"Z"=>Some(&z),_=>None},||false).unwrap_err();
        assert!(matches!(error,SessionExpressionErrorV1::UnsupportedEmptyAssignment {..}));assert_eq!(error.code(),Some("UnsupportedEmptyAssignment"));assert_eq!(error.stage(),LiteralStageV1::Evaluation);assert_eq!(error.original_source().as_ref(),source);assert_eq!(&source[error.span().start..error.span().end],rhs);assert_eq!(error.span().start_line,2);assert!(std::error::Error::source(&error).is_none());
        assert_eq!(error.to_string(),format!("UnsupportedEmptyAssignment: 0-by-0 right-hand sides are unsupported for fixed-shape indexed assignment at bytes {}..{}.",error.span().start,error.span().end));
    }
    for source in ["E(:,1)=C","E(:,1)=zeros(0,1)","E(:,1)=(C)","E(:)=zeros(1,0)","E(:,1)=7"] {let v=evaluate_session_expression_v1(lower(source).unwrap(),|n|match n{"E"=>Some(&e),"C"=>Some(&c),_=>None},||false).unwrap().value;assert_eq!(v.shape(),[0,3]);assert!(v.as_slice().is_empty());}
    for (source,code) in [("A(0)=[]","IndexOperand"),("A(7)=[]","IndexBounds"),("A(0,missing)=[]","UnknownBinding"),("E(:,4)=[]","IndexBounds"),("E(:,1)=zeros(0,2)","ArrayOperation")] {assert_eq!(evaluate_session_expression_v1(lower(source).unwrap(),|n|match n{"A"=>Some(&a),"E"=>Some(&e),_=>None},||false).unwrap_err().code(),Some(code),"{source}");}
}

#[test]
fn indexed_write_rhs_first_frozen_target_selector_phases_and_global_admission() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    for (source,code,subject) in [
        ("Missing(missing)=rhs_missing","UnknownBinding","rhs_missing"),
        ("Missing(missing)=1","UnknownBinding","Missing"),
        ("A(missing,missing,missing)=rhs_missing","UnknownBinding","rhs_missing"),
        ("A(missing,missing,missing)=1","IndexArity","A(missing,missing,missing)"),
        ("A(0,missing)=1","UnknownBinding","missing"),
        ("A(3,0)=1","IndexBounds","3"),
        ("A(0,4)=1","IndexOperand","0"),
        ("A(end)=numel(end)","UnsupportedCall","end"),
        ("A(missing)=[1,2;3]","MatrixNotAdmitted","[1,2;3]"),
    ] {
        let error=match lower(source){Err(e)=>e,Ok(ir)=>evaluate_session_expression_v1(ir,|n|if n=="A"{Some(&a)}else{None},||false).unwrap_err()};assert_eq!(error.code(),Some(code),"{source}");assert_eq!(&source[error.span().start..error.span().end],subject,"{source}");
    }
    let other=SessionValueV1::try_from_col_major([1,1],&[99.]).unwrap();let mut reads=0;
    let v=evaluate_session_expression_v1(lower("A(end)=7").unwrap(),|name|if name=="A"{reads+=1;Some(if reads==1{&a}else{&other})}else{None},||false).unwrap().value;assert_eq!(reads,1);assert_eq!(v.shape(),[2,3]);assert_eq!(v.as_slice()[5],7.);
    for source in ["A(1)=1e","A(for)=1","A(1)=end"] {assert!(lower(source).is_err());}
    for source in ["A([1,2;1,2])=1","A(zeros(1,17))=1"] {assert!(evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||false).is_err());}
}

#[test]
fn indexed_write_poll_phases_ast_capacity_recovery_and_non_session_isolation() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();let source="A([end;1],end)=A([1,end])";let mut polls=0;
    evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||{polls+=1;false}).unwrap();
    for stop in 1..=polls {let mut seen=0;assert_eq!(evaluate_session_expression_v1(lower(source).unwrap(),|n|if n=="A"{Some(&a)}else{None},||{seen+=1;seen==stop}).unwrap_err().code(),Some("Cancelled"));assert_eq!(a.as_slice(),&[1.,4.,2.,5.,3.,6.]);}
    let e=SessionValueV1::try_from_col_major([0,0],&[]).unwrap();let source=format!("A([{}])=1",vec!["E";1000].join(","));let v=evaluate_session_expression_v1(lower(&source).unwrap(),|n|match n{"A"=>Some(&a),"E"=>Some(&e),_=>None},||false).unwrap().value;assert_eq!(v.as_slice(),a.as_slice());
    let source=format!("A(1)=[{}]",vec!["E";1000].join(","));assert_eq!(evaluate_session_expression_v1(lower(&source).unwrap(),|n|match n{"A"=>Some(&a),"E"=>Some(&e),_=>None},||false).unwrap_err().code(),Some("UnsupportedEmptyAssignment"));
    let source=format!("A(1)=[{}]",vec!["1";257].join(","));assert_eq!(lower(&source).unwrap_err().code(),Some("ResourceLimit"));
    for source in ["A(1)=2","A(:)=2","A([end])=[]"] {assert!(parse_literal_expression_v1(Arc::from(source)).is_err());assert!(parse_decimal_expression_v1(Arc::from(source)).is_err());assert!(parse_arithmetic_expression_v1(Arc::from(source)).is_err());}
    for source in ["(A)(1)=2","(A(1))=2","A(1)(1)=2","A(1)+1=2","A= A(1)=2","A(1)=A(1)=2","[A](1)=2"] {assert!(lower(source).is_err(),"{source}");}
    assert_eq!(evaluate_session_expression_v1(lower("A(1)=2").unwrap(),|n|if n=="A"{Some(&a)}else{None},||false).unwrap().value.as_slice()[0],2.);
}

#[test]
fn sum_default_exact_rn_retained_policy_corpus_and_owned_words() {
    let mut counts=[0;3];
    for line in include_str!("fixtures/sum_words_v1.txt").lines().filter(|line|!line.starts_with('#') && !line.is_empty()) {
        let f:Vec<_>=line.split_whitespace().collect();assert_eq!(f.len(),8);
        let decode=|field:&str| ->Vec<u64> {if field=="-"{Vec::new()}else{field.split(',').map(|w|u64::from_str_radix(w,16).unwrap()).collect()}};
        let input=decode(f[3]);let expected=decode(f[6]);let words:Vec<_>=input.iter().copied().map(f64::from_bits).collect();
        let a=SessionValueV1::try_from_col_major([f[1].parse().unwrap(),f[2].parse().unwrap()],&words).unwrap();
        let result=evaluate_session_expression_v1(lower("sum(A)").unwrap(),|n|(n=="A").then_some(&a),||false).unwrap().value;
        assert_eq!(result.shape(),[f[4].parse().unwrap(),f[5].parse().unwrap()],"{}",f[0]);assert_eq!(result.as_slice().len(),expected.len());
        for (actual,expected) in result.as_slice().iter().zip(expected) {if f[7]=="nan" {assert!(actual.is_nan(),"{}",f[0]);}else{assert_eq!(actual.to_bits(),expected,"{}",f[0]);}}
        assert_eq!(a.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),input);if !input.is_empty() && !result.as_slice().is_empty(){assert_ne!(a.as_slice().as_ptr(),result.as_slice().as_ptr());}
        counts[match f[0].as_bytes()[0] {b'N'=>0,b'T'=>1,b'R'=>2,_=>panic!("unknown fixture") }]+=1;
    }
    assert_eq!(counts,[43,43,320]);
}

#[test]
fn sum_default_composition_shadowing_preflight_markers_and_original_spans() {
    let a = SessionValueV1::try_from_col_major([2,3], &[1.,4.,2.,5.,3.,6.]).unwrap();
    for (source, shape, expected) in [
        ("sum(A)", [1,3], vec![5.,7.,9.]),
        ("sum(sum(A))", [1,1], vec![21.]),
        ("sum([A;A])", [1,3], vec![10.,14.,18.]),
        ("sum(A(:,end))", [1,1], vec![9.]),
        ("A(sum(end))", [1,1], vec![6.]),
        ("reshape(sum(A),3,1)", [3,1], vec![5.,7.,9.]),
        ("A(1,:)=sum(A)", [2,3], vec![5.,4.,7.,5.,9.,6.]),
    ] {
        let value = evaluate_session_expression_v1(lower(source).unwrap(), |n| (n=="A").then_some(&a), ||false).unwrap().value;
        assert_eq!(value.shape(), shape, "{source}"); assert_eq!(value.as_slice(), expected, "{source}");
    }
    for (source, code, fragment) in [
        ("sum(missing,missing)", "CallArity", "sum(missing,missing)"),
        ("sum()", "CallArity", "sum()"),
        ("Sum(missing)", "UnsupportedCall", "Sum"),
        ("sum(missing)", "UnknownBinding", "missing"),
        ("sum(:)", "UnsupportedCall", ":"),
        ("sum(end)", "UnsupportedCall", "end"),
    ] {
        let error = evaluate_session_expression_v1(lower(source).unwrap(), |_|None, ||false).unwrap_err();
        assert_eq!(error.code(), Some(code), "{source}"); assert_eq!(&source[error.span().start..error.span().end], fragment, "{source}");
    }
    let shadow = SessionValueV1::try_from_col_major([1,2], &[8.,9.]).unwrap();
    for (source, expected) in [("sum", vec![8.,9.]), ("sum(end)", vec![9.]), ("sum(1,2)", vec![9.])] {
        let value = evaluate_session_expression_v1(lower(source).unwrap(), |n| (n=="sum").then_some(&shadow), ||false).unwrap().value;
        assert_eq!(value.as_slice(), expected);
    }
    assert_eq!(evaluate_session_expression_v1(lower("sum").unwrap(), |_|None, ||false).unwrap_err().code(), Some("UnknownBinding"));
    for source in ["sum(ones(2))", "sum(A)"] {
        assert!(parse_literal_expression_v1(Arc::from(source)).is_err());
        assert!(parse_decimal_expression_v1(Arc::from(source)).is_err());
        assert!(parse_arithmetic_expression_v1(Arc::from(source)).is_err());
    }
}

#[test]
fn sum_default_resource_admission_polls_and_output_recovery() {
    let a = SessionValueV1::try_from_col_major([16,16], &[1.;256]).unwrap();
    let mut polls=0;
    let output=evaluate_session_expression_v1(lower("sum(A)").unwrap(), |n|(n=="A").then_some(&a), ||{polls+=1;false}).unwrap().value;
    assert_eq!(output.shape(), [1,16]); assert_eq!(output.as_slice(), &[16.;16]);
    for stop in 1..=polls {
        let mut seen=0;
        assert_eq!(evaluate_session_expression_v1(lower("sum(A)").unwrap(), |n|(n=="A").then_some(&a), ||{seen+=1;seen==stop}).unwrap_err().code(), Some("Cancelled"));
        assert_eq!(a.as_slice(), &[1.;256]);
    }
    for source in [format!("sum([{}])",vec!["1";257].join(",")), "sum(zeros(17))".into()] {
        let result=lower(&source).and_then(|ir|evaluate_session_expression_v1(ir, |_|None, ||false)); assert!(result.is_err());
    }
    let value=evaluate_session_expression_v1(lower("sum(A)").unwrap(), |n|(n=="A").then_some(&a), ||false).unwrap().value;
    assert_eq!(value.as_slice(), &[16.;16]);
}

#[test]
fn extrema_default_native_translated_geometry_finite_words_and_classification() {
    // Frozen independent exact-word/value/classification oracles. Native-only
    // division/setup scripts are typed seeds, not promoted Session grammar.
    // None promises NaN classification only; distinct NaN policy is separate.
    let cases: &[(&str, &str, [u64;2], &[u64], [u64;2], &[Option<u64>])] = &[
        ("M01", "min", [1, 1], &[0x401c000000000000], [1, 1], &[Some(0x401c000000000000)]),
        ("X01", "max", [1, 1], &[0x401c000000000000], [1, 1], &[Some(0x401c000000000000)]),
        ("M02", "min", [1, 1], &[0x8000000000000000], [1, 1], &[Some(0x8000000000000000)]),
        ("X02", "max", [1, 1], &[0x8000000000000000], [1, 1], &[Some(0x8000000000000000)]),
        ("M03", "min", [1, 4], &[0xc008000000000000, 0xc000000000000000, 0xc000000000000000, 0xc01c000000000000], [1, 1], &[Some(0xc01c000000000000)]),
        ("X03", "max", [1, 4], &[0xc008000000000000, 0xc000000000000000, 0xc000000000000000, 0xc01c000000000000], [1, 1], &[Some(0xc000000000000000)]),
        ("M04", "min", [4, 1], &[0xc008000000000000, 0xc000000000000000, 0xc000000000000000, 0xc01c000000000000], [1, 1], &[Some(0xc01c000000000000)]),
        ("X04", "max", [4, 1], &[0xc008000000000000, 0xc000000000000000, 0xc000000000000000, 0xc01c000000000000], [1, 1], &[Some(0xc000000000000000)]),
        ("M05", "min", [2, 3], &[0x4008000000000000, 0x3ff0000000000000, 0xc000000000000000, 0x401c000000000000, 0x4010000000000000, 0xc014000000000000], [1, 3], &[Some(0x3ff0000000000000), Some(0xc000000000000000), Some(0xc014000000000000)]),
        ("X05", "max", [2, 3], &[0x4008000000000000, 0x3ff0000000000000, 0xc000000000000000, 0x401c000000000000, 0x4010000000000000, 0xc014000000000000], [1, 3], &[Some(0x4008000000000000), Some(0x401c000000000000), Some(0x4010000000000000)]),
        ("M06", "min", [16, 16], &[0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000], [1, 16], &[Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000)]),
        ("X06", "max", [16, 16], &[0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000], [1, 16], &[Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000), Some(0x3ff0000000000000)]),
        ("M07", "min", [0, 0], &[], [0, 0], &[]),
        ("X07", "max", [0, 0], &[], [0, 0], &[]),
        ("M08", "min", [0, 1], &[], [0, 1], &[]),
        ("X08", "max", [0, 1], &[], [0, 1], &[]),
        ("M09", "min", [1, 0], &[], [1, 0], &[]),
        ("X09", "max", [1, 0], &[], [1, 0], &[]),
        ("M10", "min", [0, 3], &[], [0, 3], &[]),
        ("X10", "max", [0, 3], &[], [0, 3], &[]),
        ("M11", "min", [3, 0], &[], [1, 0], &[]),
        ("X11", "max", [3, 0], &[], [1, 0], &[]),
        ("M12", "min", [1, 2], &[0x8000000000000000, 0x0000000000000000], [1, 1], &[Some(0x8000000000000000)]),
        ("X12", "max", [1, 2], &[0x8000000000000000, 0x0000000000000000], [1, 1], &[Some(0x8000000000000000)]),
        ("M13", "min", [1, 2], &[0x0000000000000000, 0x8000000000000000], [1, 1], &[Some(0x0000000000000000)]),
        ("X13", "max", [1, 2], &[0x0000000000000000, 0x8000000000000000], [1, 1], &[Some(0x0000000000000000)]),
        ("M14", "min", [1, 2], &[0x8000000000000000, 0x8000000000000000], [1, 1], &[Some(0x8000000000000000)]),
        ("X14", "max", [1, 2], &[0x8000000000000000, 0x8000000000000000], [1, 1], &[Some(0x8000000000000000)]),
        ("M15", "min", [1, 3], &[0x7ff8000000000000, 0x4008000000000000, 0xc000000000000000], [1, 1], &[Some(0xc000000000000000)]),
        ("X15", "max", [1, 3], &[0x7ff8000000000000, 0x4008000000000000, 0xc000000000000000], [1, 1], &[Some(0x4008000000000000)]),
        ("M16", "min", [1, 3], &[0x4008000000000000, 0x7ff8000000000000, 0xc000000000000000], [1, 1], &[Some(0xc000000000000000)]),
        ("X16", "max", [1, 3], &[0x4008000000000000, 0x7ff8000000000000, 0xc000000000000000], [1, 1], &[Some(0x4008000000000000)]),
        ("M17", "min", [1, 2], &[0x7ff8000000000000, 0x7ff8000000000000], [1, 1], &[None]),
        ("X17", "max", [1, 2], &[0x7ff8000000000000, 0x7ff8000000000000], [1, 1], &[None]),
        ("M18", "min", [2, 3], &[0x7ff8000000000000, 0x4000000000000000, 0x3ff0000000000000, 0x7ff8000000000000, 0x7ff8000000000000, 0x7ff8000000000000], [1, 3], &[Some(0x4000000000000000), Some(0x3ff0000000000000), None]),
        ("X18", "max", [2, 3], &[0x7ff8000000000000, 0x4000000000000000, 0x3ff0000000000000, 0x7ff8000000000000, 0x7ff8000000000000, 0x7ff8000000000000], [1, 3], &[Some(0x4000000000000000), Some(0x3ff0000000000000), None]),
        ("M19", "min", [1, 3], &[0xfff0000000000000, 0x4008000000000000, 0x7ff0000000000000], [1, 1], &[Some(0xfff0000000000000)]),
        ("X19", "max", [1, 3], &[0xfff0000000000000, 0x4008000000000000, 0x7ff0000000000000], [1, 1], &[Some(0x7ff0000000000000)]),
        ("M20", "min", [1, 3], &[0x0000000000000001, 0x0000000000000000, 0x8000000000000001], [1, 1], &[Some(0x8000000000000001)]),
        ("X20", "max", [1, 3], &[0x0000000000000001, 0x0000000000000000, 0x8000000000000001], [1, 1], &[Some(0x0000000000000001)]),
        ("M21", "min", [1, 16], &[0x4340000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x433fffffffffffff], [1, 1], &[Some(0x3ff0000000000000)]),
        ("X21", "max", [1, 16], &[0x4340000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x3ff0000000000000, 0x433fffffffffffff], [1, 1], &[Some(0x4340000000000000)]),
    ];
    for (id, operation, shape, input, output_shape, expected) in cases {
        let data: Vec<f64>=input.iter().copied().map(f64::from_bits).collect();
        let a=SessionValueV1::try_from_col_major(*shape,&data).unwrap();
        let value=evaluate_session_expression_v1(lower(&format!("{operation}(A)")).unwrap(),|name|(name=="A").then_some(&a),||false).unwrap().value;
        assert_eq!(value.shape(),*output_shape,"{id}");assert_eq!(value.as_slice().len(),expected.len(),"{id}");
        for (actual,expected) in value.as_slice().iter().zip(*expected) {
            match expected {Some(word)=>assert_eq!(actual.to_bits(),*word,"{id}"),None=>assert!(actual.is_nan(),"{id}")}
        }
        assert_eq!(a.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),*input,"{id}");
        if !value.as_slice().is_empty() {assert_ne!(value.as_slice().as_ptr(),a.as_slice().as_ptr());}
    }
}

#[test]
fn extrema_default_typed_first_numeric_zero_ties_nan_payload_and_scalar_identity() {
    // Manual selected-input indices/exact rational ordering in POLICY_ORACLE;
    // these exact NaN words specify project policy, not native predictions.
    let cases: &[(&str,&str,&[u64],u64)] = &[
        ("T01-min","min",&[0x8000000000000000],0x8000000000000000),
        ("T01-max","max",&[0x8000000000000000],0x8000000000000000),
        ("T02-min","min",&[0xfff8000000000042],0xfff8000000000042),
        ("T02-max","max",&[0xfff8000000000042],0xfff8000000000042),
        ("T03-min","min",&[0x7ff8000000000001, 0xfff8000000000042],0x7ff8000000000001),
        ("T03-max","max",&[0x7ff8000000000001, 0xfff8000000000042],0x7ff8000000000001),
        ("T04-min","min",&[0xfff8000000000042, 0x7ff8000000000001],0xfff8000000000042),
        ("T04-max","max",&[0xfff8000000000042, 0x7ff8000000000001],0xfff8000000000042),
        ("T05-min","min",&[0xfff8000000000042, 0x7ff8000000000abc, 0x7ff8000000000001],0xfff8000000000042),
        ("T05-max","max",&[0xfff8000000000042, 0x7ff8000000000abc, 0x7ff8000000000001],0xfff8000000000042),
        ("T06-min","min",&[0x7ff8000000000001, 0x8000000000000000, 0x0000000000000000],0x8000000000000000),
        ("T06-max","max",&[0x7ff8000000000001, 0x8000000000000000, 0x0000000000000000],0x8000000000000000),
        ("T07-min","min",&[0xfff8000000000042, 0x0000000000000000, 0x8000000000000000],0x0000000000000000),
        ("T07-max","max",&[0xfff8000000000042, 0x0000000000000000, 0x8000000000000000],0x0000000000000000),
        ("T08-min","min",&[0x8000000000000000, 0x7ff8000000000001, 0x0000000000000000],0x8000000000000000),
        ("T08-max","max",&[0x8000000000000000, 0x7ff8000000000001, 0x0000000000000000],0x8000000000000000),
        ("T09-min","min",&[0x7ff8000000000001, 0xc000000000000000, 0x3ff0000000000000, 0xfff8000000000042],0xc000000000000000),
        ("T09-max","max",&[0x7ff8000000000001, 0xc000000000000000, 0x3ff0000000000000, 0xfff8000000000042],0x3ff0000000000000),
        ("T10-min","min",&[0x3ff0000000000000, 0xfff8000000000042, 0xc000000000000000, 0x7ff8000000000001],0xc000000000000000),
        ("T10-max","max",&[0x3ff0000000000000, 0xfff8000000000042, 0xc000000000000000, 0x7ff8000000000001],0x3ff0000000000000),
        ("T11-min","min",&[0xfff0000000000000, 0x7ff8000000000abc, 0x7ff0000000000000],0xfff0000000000000),
        ("T11-max","max",&[0xfff0000000000000, 0x7ff8000000000abc, 0x7ff0000000000000],0x7ff0000000000000),
        ("T12-min","min",&[0x4000000000000000, 0xfff8000000000042, 0x4000000000000000],0x4000000000000000),
        ("T12-max","max",&[0x4000000000000000, 0xfff8000000000042, 0x4000000000000000],0x4000000000000000),
    ];
    for (id,operation,input,expected) in cases {
        let data: Vec<f64>=input.iter().copied().map(f64::from_bits).collect();
        for shape in [[1,data.len() as u64],[data.len() as u64,1]] {
            let a=SessionValueV1::try_from_col_major(shape,&data).unwrap();
            let value=evaluate_session_expression_v1(lower(&format!("{operation}(A)")).unwrap(),|name|(name=="A").then_some(&a),||false).unwrap().value;
            assert_eq!(value.shape(),[1,1]);assert_eq!(value.as_slice()[0].to_bits(),*expected,"{id}");
            assert_eq!(a.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),*input);assert_ne!(value.as_slice().as_ptr(),a.as_slice().as_ptr());
        }
    }
    let words=[0xfff8000000000042,0x7ff8000000000abc,0,0x8000000000000000];
    let a=SessionValueV1::try_from_col_major([2,2],&words.map(f64::from_bits)).unwrap();
    for operation in ["min","max"] {
        let value=evaluate_session_expression_v1(lower(&format!("{operation}(A)")).unwrap(),|name|(name=="A").then_some(&a),||false).unwrap().value;
        assert_eq!(value.shape(),[1,2]);assert_eq!(value.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),[words[0],0]);
    }
}

#[test]
fn extrema_default_composition_shadowing_preflight_marker_end_spans_and_empty_write_policy() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    for (source,shape,expected) in [
        ("min(A)",[1,3],vec![1.,2.,3.]),("max(A)",[1,3],vec![4.,5.,6.]),
        ("min(max(A))",[1,1],vec![4.]),("max(min(A))",[1,1],vec![3.]),
        ("min([A;A])",[1,3],vec![1.,2.,3.]),("max(A(:,end))",[1,1],vec![6.]),
        ("A(min(end))",[1,1],vec![6.]),("A(max(end))",[1,1],vec![6.]),
        ("reshape(min(A),3,1)",[3,1],vec![1.,2.,3.]),
        ("A(1,:)=max(A)",[2,3],vec![4.,4.,5.,5.,6.,6.]),
    ] {
        let value=evaluate_session_expression_v1(lower(source).unwrap(),|n|(n=="A").then_some(&a),||false).unwrap().value;
        assert_eq!(value.shape(),shape,"{source}");assert_eq!(value.as_slice(),expected,"{source}");
    }
    for operation in ["min","max"] {
        for (argument,code,fragment) in [("missing,missing","CallArity",None),("","CallArity",None),("missing","UnknownBinding",Some("missing")),(":","UnsupportedCall",Some(":")),("end","UnsupportedCall",Some("end"))] {
            let source=format!("{operation}({argument})");let error=evaluate_session_expression_v1(lower(&source).unwrap(),|_|None,||false).unwrap_err();
            assert_eq!(error.code(),Some(code));assert_eq!(&source[error.span().start..error.span().end],fragment.unwrap_or(&source));
        }
        let shadow=SessionValueV1::try_from_col_major([1,2],&[8.,9.]).unwrap();
        for (source,expected) in [(operation.to_string(),vec![8.,9.]),(format!("{operation}(end)"),vec![9.]),(format!("{operation}(1,2)"),vec![9.])] {
            let value=evaluate_session_expression_v1(lower(&source).unwrap(),|n|(n==operation).then_some(&shadow),||false).unwrap().value;assert_eq!(value.as_slice(),expected);
        }
        assert_eq!(evaluate_session_expression_v1(lower(operation).unwrap(),|_|None,||false).unwrap_err().code(),Some("UnknownBinding"));
        let source=format!("A(1)={operation}([])");let error=evaluate_session_expression_v1(lower(&source).unwrap(),|n|(n=="A").then_some(&a),||false).unwrap_err();assert_eq!(error.code(),Some("UnsupportedEmptyAssignment"));assert_eq!(&source[error.span().start..error.span().end],format!("{operation}([])"));
        for source in [format!("{operation}(A)"),format!("{operation}(ones(2))")] {
            assert!(parse_literal_expression_v1(Arc::from(source.as_str())).is_err());assert!(parse_decimal_expression_v1(Arc::from(source.as_str())).is_err());assert!(parse_arithmetic_expression_v1(Arc::from(source.as_str())).is_err());
        }
    }
    assert_eq!(evaluate_session_expression_v1(lower("Min(missing)").unwrap(),|_|None,||false).unwrap_err().code(),Some("UnsupportedCall"));
}

#[test]
fn extrema_default_cap_boundaries_task_polls_and_recovery() {
    let a=SessionValueV1::try_from_col_major([16,16],&[1.;256]).unwrap();
    for operation in ["min","max"] {
        let source=format!("{operation}(A)");let mut polls=0;
        let value=evaluate_session_expression_v1(lower(&source).unwrap(),|n|(n=="A").then_some(&a),||{polls+=1;false}).unwrap().value;assert_eq!(value.shape(),[1,16]);assert_eq!(value.as_slice(),&[1.;16]);
        for stop in 1..=polls {let mut seen=0;assert_eq!(evaluate_session_expression_v1(lower(&source).unwrap(),|n|(n=="A").then_some(&a),||{seen+=1;seen==stop}).unwrap_err().code(),Some("Cancelled"));assert_eq!(a.as_slice(),&[1.;256]);}
        let global=format!("{operation}([{}])",vec!["1";257].join(","));assert_eq!(lower(&global).unwrap_err().code(),Some("ResourceLimit"));
        assert!(evaluate_session_expression_v1(lower(&format!("{operation}(zeros(17))")).unwrap(),|_|None,||false).is_err());
        assert_eq!(evaluate_session_expression_v1(lower(&source).unwrap(),|n|(n=="A").then_some(&a),||false).unwrap().value.as_slice(),&[1.;16]);
        assert!(lower(&format!("{operation}({})",vec!["1";257].join(","))).is_err());
    }
}


#[test]
fn exact_mean_prod_independent_word_fixture_and_owned_outputs() {
    let mut counts=[0;3];
    for line in include_str!("fixtures/mean_prod_words_v1.txt").lines().filter(|line|!line.starts_with('#') && !line.is_empty()) {
        let fields:Vec<_>=line.split_whitespace().collect();assert_eq!(fields.len(),8);
        let id=fields[0];let operation=fields[1];assert!(matches!(operation,"mean"|"prod"));
        let shape=[fields[2].parse().unwrap(),fields[3].parse().unwrap()];
        let decode=|field:&str| ->Vec<u64> {if field=="-" {Vec::new()} else {field.split(',').map(|word|u64::from_str_radix(word,16).unwrap()).collect()}};
        let input=decode(fields[4]);let expected=decode(fields[7]);let output_shape=[fields[5].parse().unwrap(),fields[6].parse().unwrap()];
        let data:Vec<f64>=input.iter().copied().map(f64::from_bits).collect();let a=SessionValueV1::try_from_col_major(shape,&data).unwrap();
        let result=evaluate_session_expression_v1(lower(&format!("{operation}(A)")).unwrap(),|name|(name=="A").then_some(&a),||false).unwrap().value;
        assert_eq!(result.shape(),output_shape,"{id}");assert_eq!(result.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"{id}");assert_eq!(a.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),input,"{id}");
        if !input.is_empty() && !expected.is_empty() {assert_ne!(a.as_slice().as_ptr(),result.as_slice().as_ptr());}
        counts[match id.as_bytes()[0] {b'N'=>0,b'T'=>1,b'R'=>2,_=>panic!("unknownfixture") }]+=1;
    }
    assert_eq!(counts,[42,44,320]);
}

#[test]
fn exact_mean_prod_composition_preflight_marker_end_shadowing_and_prior_policies() {
    let a=SessionValueV1::try_from_col_major([2,3],&[1.,4.,2.,5.,3.,6.]).unwrap();
    for (source,shape,expected) in [
        ("mean(A)",[1,3],vec![2.5,3.5,4.5]),("prod(A)",[1,3],vec![4.,10.,18.]),
        ("mean([A;A])",[1,3],vec![2.5,3.5,4.5]),("prod([A;A])",[1,3],vec![16.,100.,324.]),
        ("A(mean(end))",[1,1],vec![6.]),("A(prod(end))",[1,1],vec![6.]),
        ("reshape(mean(A),3,1)",[3,1],vec![2.5,3.5,4.5]),("prod(A(:,end))",[1,1],vec![18.]),
        ("A(1,:)=mean(A)",[2,3],vec![2.5,4.,3.5,5.,4.5,6.]),
    ] {let value=evaluate_session_expression_v1(lower(source).unwrap(),|n|(n=="A").then_some(&a),||false).unwrap().value;assert_eq!(value.shape(),shape,"{source}");assert_eq!(value.as_slice(),expected,"{source}");}
    for operation in ["mean","prod"] {
        for (args,code,fragment) in [("missing,missing","CallArity",None),("","CallArity",None),("missing","UnknownBinding",Some("missing")),(":","UnsupportedCall",Some(":")),("end","UnsupportedCall",Some("end"))] {
            let source=format!("{operation}({args})");let error=evaluate_session_expression_v1(lower(&source).unwrap(),|_|None,||false).unwrap_err();assert_eq!(error.code(),Some(code));assert_eq!(&source[error.span().start..error.span().end],fragment.unwrap_or(&source));
        }
        let bound=SessionValueV1::try_from_col_major([1,2],&[8.,9.]).unwrap();
        for (source,expected) in [(operation.to_string(),vec![8.,9.]),(format!("{operation}(end)"),vec![9.]),(format!("{operation}(1,2)"),vec![9.])] {assert_eq!(evaluate_session_expression_v1(lower(&source).unwrap(),|n|(n==operation).then_some(&bound),||false).unwrap().value.as_slice(),expected);}
        assert_eq!(evaluate_session_expression_v1(lower(operation).unwrap(),|_|None,||false).unwrap_err().code(),Some("UnknownBinding"));
        for source in [format!("{operation}(A)"),format!("{operation}(ones(2))")] {assert!(parse_literal_expression_v1(Arc::from(source.as_str())).is_err());assert!(parse_decimal_expression_v1(Arc::from(source.as_str())).is_err());assert!(parse_arithmetic_expression_v1(Arc::from(source.as_str())).is_err());}
    }
    for (source,word) in [("mean([9007199254740992 1 -9007199254740992])",0x3fd5555555555555),("sum([9007199254740992 1 -9007199254740992])",0x3ff0000000000000),("mean([1e308 1e308 -1e308 -1e308])",0),("sum([1e308 1e308 -1e308 -1e308])",0),("min([-0 0])",0x8000000000000000)] {assert_eq!(evaluate_session_expression_v1(lower(source).unwrap(),|_|None,||false).unwrap().value.as_slice()[0].to_bits(),word);}
    assert_eq!(evaluate_session_expression_v1(lower("A(1)=min([])").unwrap(),|n|(n=="A").then_some(&a),||false).unwrap_err().code(),Some("UnsupportedEmptyAssignment"));
}

#[test]
fn exact_mean_prod_caps_new_quantum_polls_and_recovery() {
    let a=SessionValueV1::try_from_col_major([16,16],&[1.;256]).unwrap();
    for operation in ["mean","prod"] {
        let source=format!("{operation}(A)");let mut polls=0;
        let value=evaluate_session_expression_v1(lower(&source).unwrap(),|n|(n=="A").then_some(&a),||{polls+=1;false}).unwrap().value;assert_eq!(value.shape(),[1,16]);assert_eq!(value.as_slice(),&[1.;16]);
        for stop in 1..=polls {let mut seen=0;assert_eq!(evaluate_session_expression_v1(lower(&source).unwrap(),|n|(n=="A").then_some(&a),||{seen+=1;seen==stop}).unwrap_err().code(),Some("Cancelled"));assert_eq!(a.as_slice(),&[1.;256]);}
        assert!(evaluate_session_expression_v1(lower(&format!("{operation}(zeros(17))")).unwrap(),|_|None,||false).is_err());
        assert_eq!(lower(&format!("{operation}([{}])",vec!["1";257].join(","))).unwrap_err().code(),Some("ResourceLimit"));
        assert!(lower(&format!("{operation}({})",vec!["1";257].join(","))).is_err());
        assert_eq!(evaluate_session_expression_v1(lower(&source).unwrap(),|n|(n=="A").then_some(&a),||false).unwrap().value.as_slice(),&[1.;16]);
    }
}
