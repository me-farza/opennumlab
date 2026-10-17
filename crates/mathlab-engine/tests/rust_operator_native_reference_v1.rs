// SPDX-License-Identifier: GPL-3.0-or-later
//! Selected native values exercise the existing opt-in Rust session route.
//! Publication and recovery assertions are project contracts, not native atomicity.

use mathlab_engine::rust_session::{
    RUST_SESSION_PROFILE_V1, RustBindingEffectV1, RustEvalOutcomeV1, RustEvalSuccessV1,
    RustSessionErrorV1, RustSessionV1,
};
use mathlab_language::arithmetic_expression::ArithmeticExpressionErrorV1;
use mathlab_language::decimal_expression::{DecimalDiagnosticDetailV1, DecimalExpressionErrorV1};
use mathlab_language::expression::{LiteralDiagnosticDetailV1, LiteralStageV1};
use mathlab_language::session_expression::{SessionExpressionErrorV1, SessionValueV1};
use std::sync::Arc;

#[path = "../../mathlab-language/tests/fixtures/operator_native_reference_v1.rs"]
mod native_reference;
use native_reference::{NativeCaseV1, NativeExpectedV1, case};

fn words(values: &[f64]) -> Vec<u64> {
    values.iter().map(|value| value.to_bits()).collect()
}

fn run(session: &mut RustSessionV1, source: &str) -> RustEvalSuccessV1 {
    match session.prepare(Arc::from(source), None).unwrap().execute() {
        RustEvalOutcomeV1::Success(value) => value,
        other => panic!("expected success for {source}: {other:?}"),
    }
}

fn assert_native_exact(value: &SessionValueV1, native: &NativeCaseV1) {
    let NativeExpectedV1::Exact {
        class,
        shape,
        words: expected,
    } = &native.expected
    else {
        panic!("expected exact native fixture {}", native.id);
    };
    assert_eq!(value.class(), *class, "{}", native.id);
    assert_eq!(value.shape(), *shape, "{}", native.id);
    assert_eq!(
        words(value.as_slice()).as_slice(),
        *expected,
        "{}",
        native.id
    );
}

fn snapshot(session: &RustSessionV1) -> Vec<(String, &'static str, [u64; 2], Vec<u64>)> {
    let mut published: Vec<_> = session
        .workspace()
        .map(|metadata| {
            let read = session.read(metadata.name).unwrap().unwrap();
            assert_eq!(read.context, metadata.context);
            assert_eq!(read.context, session.context());
            assert_eq!(read.value.class(), metadata.class);
            assert_eq!(read.value.shape(), metadata.shape);
            (
                metadata.name.to_owned(),
                metadata.class,
                metadata.shape,
                words(read.value.as_slice()),
            )
        })
        .collect();
    published.sort_by(|left, right| left.0.cmp(&right.0));
    published
}

#[test]
fn selected_broadcast_and_nonfinite_records_publish_through_rust_session() {
    let mut session = RustSessionV1::new().unwrap();
    for id in ["F02", "O02"] {
        let native = case(id);
        let before = session.context();
        let response = run(&mut session, native.source);
        assert_eq!(response.backend, "rust");
        assert_eq!(response.profile, RUST_SESSION_PROFILE_V1);
        assert_eq!(response.source.as_ref(), native.source);
        assert_eq!(response.before, before);
        assert_eq!(response.after, session.context());
        assert_eq!(response.after.session_id, before.session_id);
        assert_eq!(response.after.epoch, before.epoch);
        assert_eq!(
            response.after.workspace_revision,
            before.workspace_revision + 1
        );
        assert_eq!(
            response.effect,
            RustBindingEffectV1::Write("ans".to_owned())
        );
        assert_eq!(response.display_name.as_deref(), Some("ans"));
        assert_native_exact(&response.value, native);
        let published = session.read("ans").unwrap().unwrap();
        assert_eq!(published.context, response.after);
        assert_native_exact(&published.value, native);
    }
}

#[test]
fn selected_shape_failure_retains_publication_and_allows_next_command() {
    let mut session = RustSessionV1::new().unwrap();
    run(&mut session, "keep=[-0,7];");
    let seed = case("F02");
    assert_native_exact(&run(&mut session, seed.source).value, seed);
    let keep = words(session.read("keep").unwrap().unwrap().value.as_slice());
    let before = session.context();
    let published = snapshot(&session);
    let native = case("O04");
    assert!(matches!(
        &native.expected,
        NativeExpectedV1::ShapeError { native_line: 2, .. }
    ));
    let source: Arc<str> = Arc::from(native.source);
    let error = match session
        .prepare(Arc::clone(&source), None)
        .unwrap()
        .execute()
    {
        RustEvalOutcomeV1::Error { context, error, .. } => {
            assert_eq!(context, before);
            error
        }
        other => panic!("expected selected shape failure: {other:?}"),
    };
    assert_eq!(session.context(), before);
    assert_eq!(snapshot(&session), published);
    let RustSessionErrorV1::Language(error) = error else {
        panic!("expected language diagnostic");
    };
    assert_eq!(error.stage(), LiteralStageV1::Evaluation);
    assert_eq!(error.code(), Some("ArrayOperation"));
    assert!(Arc::ptr_eq(error.original_source(), &source));
    assert_eq!(
        (
            error.span().start,
            error.span().end,
            error.span().start_line,
            error.span().end_line,
        ),
        (10, 12, 2, 2)
    );
    assert_eq!(&source[error.span().start..error.span().end], "./");
    match error {
        SessionExpressionErrorV1::Arithmetic(ArithmeticExpressionErrorV1::Shared(
            DecimalExpressionErrorV1::Diagnostic(diagnostic),
        )) => match diagnostic.detail {
            DecimalDiagnosticDetailV1::Profile(LiteralDiagnosticDetailV1::ArrayOperation {
                error: leaf,
            }) => {
                assert_eq!(leaf.context.as_str(), "rdivide.compatibility");
                assert_eq!(leaf.code.as_str(), "IncompatibleShape");
            }
            other => panic!("expected RDivideCompatibility leaf: {other:?}"),
        },
        other => panic!("expected structured ArrayOperation diagnostic: {other:?}"),
    }
    let next = case("O02");
    let recovered = run(&mut session, next.source);
    assert_eq!(recovered.before, before);
    assert_eq!(recovered.after, session.context());
    assert_eq!(
        recovered.after.workspace_revision,
        before.workspace_revision + 1
    );
    assert_native_exact(&recovered.value, next);
    assert_native_exact(&session.read("ans").unwrap().unwrap().value, next);
    assert_eq!(
        words(session.read("keep").unwrap().unwrap().value.as_slice()),
        keep
    );
}
