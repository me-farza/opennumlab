// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use mathlab_language::arithmetic_expression::{
    ArithmeticExpressionErrorV1, parse_arithmetic_expression_v1,
};
use mathlab_language::expression::LiteralStageV1;

fn run(session: &mut RustSessionV1, code: &str) -> RustEvalOutcomeV1 {
    session.prepare(Arc::from(code), None).unwrap().execute()
}
fn success(outcome: RustEvalOutcomeV1) -> RustEvalSuccessV1 {
    match outcome {
        RustEvalOutcomeV1::Success(value) => value,
        other => panic!("expected success: {other:?}"),
    }
}
fn values(session: &RustSessionV1, name: &str) -> Vec<f64> {
    session
        .read(name)
        .unwrap()
        .unwrap()
        .value
        .as_slice()
        .to_vec()
}
fn snapshot(session: &RustSessionV1) -> Vec<(String, [u64; 2], Vec<u64>)> {
    session
        .workspace()
        .map(|m| {
            (
                m.name.to_owned(),
                m.shape,
                session
                    .read(m.name)
                    .unwrap()
                    .unwrap()
                    .value
                    .as_slice()
                    .iter()
                    .map(|n| n.to_bits())
                    .collect(),
            )
        })
        .collect()
}
fn assert_error_unchanged(session: &mut RustSessionV1, source: &str) -> RustSessionErrorV1 {
    let before = (session.context(), snapshot(session));
    let error = match run(session, source) {
        RustEvalOutcomeV1::Error { error, .. } => error,
        other => panic!("expected error for {source}: {other:?}"),
    };
    assert_eq!((session.context(), snapshot(session)), before);
    error
}

#[test]
fn rs01_opt_in_profile_preserves_stateless_rejection() {
    for source in ["A", "A=1"] {
        let error = parse_arithmetic_expression_v1(Arc::from(source)).unwrap_err();
        assert_eq!(error.code(), Some("UnsupportedIdentifier"));
    }
    let mut session = RustSessionV1::new().unwrap();
    let result = success(run(&mut session, "1+2"));
    assert_eq!(result.backend, "rust");
    assert_eq!(result.profile, RUST_SESSION_PROFILE_V1);
    assert_eq!(result.value.as_slice(), &[3.0]);
    assert_error_unchanged(&mut session, "sin(0)");
}

#[test]
fn rs02_sequential_case_sensitive_matrix_bindings() {
    let mut session = RustSessionV1::new().unwrap();
    for (source, revision) in [("A=[1,2;3,4];", 1), ("a=5;", 2), ("B=A+1;", 3)] {
        assert_eq!(
            success(run(&mut session, source)).after.workspace_revision,
            revision
        );
    }
    assert_eq!(values(&session, "A"), [1.0, 3.0, 2.0, 4.0]);
    assert_eq!(values(&session, "a"), [5.0]);
    let b = session.read("B").unwrap().unwrap();
    assert_eq!(b.value.shape(), [2, 2]);
    assert_eq!(b.value.as_slice(), &[2.0, 4.0, 3.0, 5.0]);
    assert_eq!(session.workspace().count(), 3);
}

#[test]
fn rs03_ans_assignment_and_bare_display_are_distinct() {
    let mut session = RustSessionV1::new().unwrap();
    assert!(matches!(
        assert_error_unchanged(&mut session, "ans"),
        RustSessionErrorV1::Language(SessionExpressionErrorV1::UnknownBinding { .. })
    ));
    assert_eq!(
        success(run(&mut session, "2+2")).display_name.as_deref(),
        Some("ans")
    );
    success(run(&mut session, "result=4+4"));
    let result = success(run(&mut session, "result"));
    assert_eq!(result.value.as_slice(), &[8.0]);
    assert_eq!(result.display_name.as_deref(), Some("result"));
    assert_eq!(result.before, result.after);
    assert_eq!(result.after.workspace_revision, 2);
    assert_eq!(result.effect, RustBindingEffectV1::None);
    assert_eq!(success(run(&mut session, "ans")).value.as_slice(), &[4.0]);
    let result = success(run(&mut session, "ans=9"));
    assert_eq!(result.after.workspace_revision, 3);
    assert_eq!(values(&session, "ans"), [9.0]);
}

#[test]
fn rs04_semicolon_suppresses_display_without_suppressing_commit() {
    let mut session = RustSessionV1::new().unwrap();
    let result = success(run(&mut session, "6+1;"));
    assert!(result.display_name.is_none());
    assert_eq!(values(&session, "ans"), [7.0]);
    assert!(
        success(run(&mut session, "A=[1,2;3,4];"))
            .display_name
            .is_none()
    );
    for source in ["A=1; B=2", "A=B=1", "A=1\nB=2", "A(1)=5", "A=1,", "A=1;;"] {
        assert_error_unchanged(&mut session, source);
    }
}

#[test]
fn rs05_binding_copies_and_exported_snapshots_are_independent() {
    let mut session = RustSessionV1::new().unwrap();
    success(run(&mut session, "A=[1,2];"));
    success(run(&mut session, "B=A;"));
    let old = session.read("A").unwrap().unwrap();
    let context = session.context();
    assert_eq!(old.context, context);
    let response = success(run(&mut session, "A=A+1;"));
    assert_eq!(old.value.as_slice(), &[1.0, 2.0]);
    assert_eq!(values(&session, "B"), [1.0, 2.0]);
    assert_eq!(old.context, context);
    assert_eq!(response.before, context);
    assert_eq!(
        response.after.workspace_revision,
        context.workspace_revision + 1
    );
    assert_eq!(response.value.as_slice(), &[2.0, 3.0]);
    drop(response);
    assert_eq!(values(&session, "A"), [2.0, 3.0]);
}

#[test]
fn rs06_shape_layout_signed_zero_empty_and_nonfinite_values() {
    let mut session = RustSessionV1::new().unwrap();
    success(run(&mut session, "A=[-0,1;2,3];"));
    let a = session.read("A").unwrap().unwrap();
    assert_eq!(a.value.shape(), [2, 2]);
    assert_eq!(a.value.as_slice()[0].to_bits(), (-0.0_f64).to_bits());
    let transpose = success(run(&mut session, "A.'"));
    assert_eq!(
        transpose.value.as_slice()[0].to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(&transpose.value.as_slice()[1..], &[1.0, 2.0, 3.0]);
    let empty = success(run(&mut session, "E=[];"));
    assert_eq!(empty.value.shape(), [0, 0]);
    assert!(empty.value.as_slice().is_empty());
    assert!(success(run(&mut session, "1./0")).value.as_slice()[0].is_infinite());
    assert!(success(run(&mut session, "0./0")).value.as_slice()[0].is_nan());
}

#[test]
fn rs07_errors_keep_original_spans_and_allow_next_command() {
    let mut session = RustSessionV1::new().unwrap();
    success(run(&mut session, "A=3;"));
    success(run(&mut session, "2+2"));
    match assert_error_unchanged(&mut session, "B=missing+1;") {
        RustSessionErrorV1::Language(error) => {
            assert_eq!(error.code(), Some("UnknownBinding"));
            assert_eq!((error.span().start, error.span().end), (2, 9));
            assert_eq!(error.original_source().as_ref(), "B=missing+1;");
        }
        other => panic!("unexpected {other:?}"),
    }
    for source in ["if=1", "_A=1", "A="] {
        assert_error_unchanged(&mut session, source);
    }
    let long_name = format!("{}=1", "a".repeat(2049));
    assert!(matches!(
        assert_error_unchanged(&mut session, &long_name),
        RustSessionErrorV1::Language(SessionExpressionErrorV1::InvalidIdentifier { .. })
    ));
    assert_eq!(success(run(&mut session, "2+3")).value.as_slice(), &[5.0]);
}

#[test]
fn rs08_operator_error_preserves_target_ans_and_revision() {
    let mut session = RustSessionV1::new().unwrap();
    success(run(&mut session, "A=[1,2];"));
    success(run(&mut session, "4"));
    let source = "A=[1,2]*[1,2]";
    match assert_error_unchanged(&mut session, source) {
        RustSessionErrorV1::Language(SessionExpressionErrorV1::Arithmetic(
            error @ ArithmeticExpressionErrorV1::Multiplication { .. },
        )) => {
            assert_eq!(error.stage(), LiteralStageV1::Evaluation);
            assert_eq!(error.code(), Some("ArrayOperation"));
            assert_eq!(error.span().start, source.find('*').unwrap());
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_error_unchanged(&mut session, &" ".repeat(4097));
    assert_error_unchanged(
        &mut session,
        "A=[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17]",
    );
}

#[test]
fn rs09_quota_replacement_and_prepublication_reservation_failures() {
    let mut session = RustSessionV1::new().unwrap();
    for n in 0..64 {
        success(run(&mut session, &format!("v{n}=1;")));
    }
    assert_eq!(session.workspace().count(), 64);
    assert!(matches!(
        assert_error_unchanged(&mut session, "extra=2;"),
        RustSessionErrorV1::WorkspaceLimit
    ));
    success(run(&mut session, "v0=[1,2];"));
    assert_eq!(values(&session, "v0"), [1.0, 2.0]);
    for site in [
        ReservationSite::Name,
        ReservationSite::Slot,
        ReservationSite::Response,
    ] {
        let mut session = RustSessionV1::new().unwrap();
        success(run(&mut session, "A=7;"));
        success(run(&mut session, "3"));
        session.fail_reservation = Some(site);
        assert!(matches!(
            assert_error_unchanged(&mut session, "B=A+1;"),
            RustSessionErrorV1::Allocation { .. }
        ));
        assert_eq!(success(run(&mut session, "2+3")).value.as_slice(), &[5.0]);
    }
}

#[test]
fn rs10_checkpoint_cancellation_deadline_and_recovery() {
    for checkpoint in [
        Checkpoint::BeforeParse,
        Checkpoint::AfterParse,
        Checkpoint::AfterAdmission,
        Checkpoint::Task,
        Checkpoint::AfterEvaluation,
        Checkpoint::BeforeCommit,
    ] {
        let mut session = RustSessionV1::new().unwrap();
        success(run(&mut session, "A=[1,2];"));
        success(run(&mut session, "9"));
        let before = (session.context(), snapshot(&session));
        let mut tasks = 0;
        session.hook = Some(Box::new(move |at, handle| {
            if at == Checkpoint::Task {
                tasks += 1;
            }
            if at == checkpoint && (at != Checkpoint::Task || tasks == 3) {
                handle.request_stop();
            }
        }));
        assert!(matches!(
            run(&mut session, "B=(A+1)*[1;1];"),
            RustEvalOutcomeV1::Cancelled {
                reason: RustCancelReasonV1::UserStop,
                ..
            }
        ));
        session.hook = None;
        assert_eq!((session.context(), snapshot(&session)), before);
        assert_eq!(success(run(&mut session, "2+3")).value.as_slice(), &[5.0]);
    }
    let mut session = RustSessionV1::new().unwrap();
    let request = session
        .prepare(Arc::from("bad syntax"), Some(Instant::now()))
        .unwrap();
    assert!(matches!(
        request.execute(),
        RustEvalOutcomeV1::Cancelled {
            reason: RustCancelReasonV1::Deadline,
            ..
        }
    ));
    assert_eq!(session.context().workspace_revision, 0);
}

#[test]
fn rs11_terminal_races_reason_publication_and_stale_stop() {
    use std::sync::Barrier;
    for (first, second) in [
        (RustCancelReasonV1::UserStop, RustCancelReasonV1::Deadline),
        (RustCancelReasonV1::Deadline, RustCancelReasonV1::UserStop),
    ] {
        let mut session = RustSessionV1::new().unwrap();
        let request = session.prepare(Arc::from("2+3"), None).unwrap();
        let handle = request.cancel_handle();
        assert_eq!(
            handle.cancel_for(handle.key(), first),
            RustCancelRequestV1::Accepted(first)
        );
        assert_eq!(
            handle.cancel_for(handle.key(), second),
            RustCancelRequestV1::AlreadyAccepted(first)
        );
        assert!(
            matches!(request.execute(),RustEvalOutcomeV1::Cancelled { reason, .. } if reason == first)
        );
        assert_eq!(
            handle.request_stop(),
            RustCancelRequestV1::AlreadyAccepted(first)
        );
    }
    let mut session = RustSessionV1::new().unwrap();
    let commit_barrier = Arc::new(Barrier::new(2));
    let hook_barrier = Arc::clone(&commit_barrier);
    session.hook = Some(Box::new(move |at, _| {
        if at == Checkpoint::BeforeCommit {
            hook_barrier.wait();
            hook_barrier.wait();
        }
    }));
    let request = session.prepare(Arc::from("7"), None).unwrap();
    let cancel = request.cancel_handle();
    let cancel_barrier = Arc::clone(&commit_barrier);
    let stop = std::thread::spawn(move || {
        cancel_barrier.wait();
        let result = cancel.request_stop();
        cancel_barrier.wait();
        result
    });
    assert!(matches!(
        request.execute(),
        RustEvalOutcomeV1::Cancelled { .. }
    ));
    assert_eq!(
        stop.join().unwrap(),
        RustCancelRequestV1::Accepted(RustCancelReasonV1::UserStop)
    );
    assert_eq!(session.context().workspace_revision, 0);
    session.hook = None;
    let mut session = RustSessionV1::new().unwrap();
    let before = session.context();
    let commit_barrier = Arc::new(Barrier::new(2));
    let hook_barrier = Arc::clone(&commit_barrier);
    session.hook = Some(Box::new(move |at, _| {
        if at == Checkpoint::AfterCommitTransition {
            hook_barrier.wait();
            hook_barrier.wait();
        }
    }));
    let request = session.prepare(Arc::from("1"), None).unwrap();
    let old = request.cancel_handle();
    let cancel = old.clone();
    let cancel_barrier = Arc::clone(&commit_barrier);
    let stop = std::thread::spawn(move || {
        cancel_barrier.wait();
        let result = cancel.request_stop();
        cancel_barrier.wait();
        result
    });
    let committed = success(request.execute());
    assert_eq!(stop.join().unwrap(), RustCancelRequestV1::TooLate);
    assert_eq!(committed.before, before);
    assert_eq!(committed.after, session.context());
    assert_eq!(
        committed.after.workspace_revision,
        before.workspace_revision + 1
    );
    assert_eq!(
        committed.effect,
        RustBindingEffectV1::Write("ans".to_owned())
    );
    assert_eq!(committed.value.as_slice(), &[1.0]);
    assert_eq!(values(&session, "ans"), [1.0]);
    session.hook = None;
    assert_eq!(old.request_stop(), RustCancelRequestV1::TooLate);
    let next = session.prepare(Arc::from("2+3"), None).unwrap();
    let fresh = next.cancel_handle();
    assert_eq!(
        old.cancel_for(fresh.key(), RustCancelReasonV1::UserStop),
        RustCancelRequestV1::WrongKey
    );
    success(next.execute());
    session.hook = Some(Box::new(|at, handle| {
        if at == Checkpoint::BeforeError {
            handle.request_stop();
        }
    }));
    assert!(matches!(
        run(&mut session, "missing"),
        RustEvalOutcomeV1::Cancelled { .. }
    ));
    session.hook = None;
    let request = session.prepare(Arc::from("missing"), None).unwrap();
    let failed = request.cancel_handle();
    assert!(matches!(request.execute(), RustEvalOutcomeV1::Error { .. }));
    assert_eq!(failed.request_stop(), RustCancelRequestV1::TooLate);
    // A real pair of concurrent CAS operations publishes exactly one first reason.
    for _ in 0..8 {
        let request = session.prepare(Arc::from("5"), None).unwrap();
        let handle = request.cancel_handle();
        let barrier = Arc::new(Barrier::new(3));
        let a = handle.clone();
        let ba = Arc::clone(&barrier);
        let one = std::thread::spawn(move || {
            ba.wait();
            a.cancel_for(a.key(), RustCancelReasonV1::UserStop)
        });
        let b = handle.clone();
        let bb = Arc::clone(&barrier);
        let two = std::thread::spawn(move || {
            bb.wait();
            b.cancel_for(b.key(), RustCancelReasonV1::Deadline)
        });
        barrier.wait();
        let results = [one.join().unwrap(), two.join().unwrap()];
        let winner: Vec<_> = results
            .iter()
            .filter_map(|r| {
                if let RustCancelRequestV1::Accepted(reason) = r {
                    Some(*reason)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(winner.len(), 1);
        assert!(
            matches!(request.execute(),RustEvalOutcomeV1::Cancelled { reason, .. } if reason == winner[0])
        );
    }
}

#[test]
fn rs12_owner_epoch_preparation_and_counter_lifetimes() {
    let mut a = RustSessionV1::new().unwrap();
    let mut b = RustSessionV1::new().unwrap();
    assert_ne!(a.context().session_id, b.context().session_id);
    assert_eq!(a.context().epoch, b.context().epoch);
    assert!(matches!(
        a.prepare_at(b.context(), Arc::from("1"), None),
        Err(RustSessionErrorV1::WrongSession)
    ));
    let request = a.prepare(Arc::from("A=9"), None).unwrap();
    let abandoned = request.cancel_handle();
    let other = b.prepare(Arc::from("2"), None).unwrap();
    let other_handle = other.cancel_handle();
    assert_eq!(abandoned.key().request_id, other_handle.key().request_id);
    assert_eq!(
        abandoned.cancel_for(other_handle.key(), RustCancelReasonV1::UserStop),
        RustCancelRequestV1::WrongKey
    );
    success(other.execute());
    drop(request);
    assert_eq!(abandoned.request_stop(), RustCancelRequestV1::TooLate);
    success(run(&mut a, "A=3;"));
    let old = a.read("A").unwrap().unwrap();
    let context = a.context();
    assert_eq!(old.context, context);
    let request = a.prepare(Arc::from("1"), None).unwrap();
    let closed = request.cancel_handle();
    success(request.execute());
    let reset = a.reset().unwrap();
    assert_ne!(context.epoch, reset.epoch);
    assert_eq!(reset.workspace_revision, 0);
    assert_eq!(a.workspace().count(), 0);
    assert_eq!(b.workspace().count(), 1);
    assert_eq!(values(&b, "ans"), [2.0]);
    assert_eq!(old.value.as_slice(), &[3.0]);
    assert_eq!(old.context, context);
    assert_eq!(closed.request_stop(), RustCancelRequestV1::TooLate);
    assert!(matches!(
        a.prepare_at(context, Arc::from("1"), None),
        Err(RustSessionErrorV1::StaleEpoch)
    ));
    a.revision = u64::MAX;
    assert!(matches!(
        assert_error_unchanged(&mut a, "A=1"),
        RustSessionErrorV1::CounterExhausted
    ));
    a.request_id = u64::MAX;
    assert!(matches!(
        a.prepare(Arc::from("1"), None),
        Err(RustSessionErrorV1::CounterExhausted)
    ));
    a.epoch = u64::MAX;
    let before = a.context();
    assert!(matches!(
        a.reset(),
        Err(RustSessionErrorV1::CounterExhausted)
    ));
    assert_eq!(a.context(), before);
}
