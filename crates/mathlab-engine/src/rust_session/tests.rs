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
    let before = (session.context(), snapshot(session), session.retained_bytes);
    let error = match run(session, source) {
        RustEvalOutcomeV1::Error { error, .. } => error,
        other => panic!("expected error for {source}: {other:?}"),
    };
    assert_eq!((session.context(), snapshot(session), session.retained_bytes), before);
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
    for source in ["A=1; B=2", "A=B=1", "A=1\nB=2", "A(1)=", "A=1,", "A=1;;"] {
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


#[test]
fn rs13_zeros_assignment_ans_and_owned_snapshots() {
    let mut session = RustSessionV1::new().unwrap();
    success(run(&mut session, "n=2;"));
    let assigned = success(run(&mut session, "A=zeros(n,3);"));
    assert_eq!(assigned.value.shape(), [2,3]);
    assert!(assigned.value.as_slice().iter().all(|v| v.to_bits()==0));
    assert!(assigned.display_name.is_none());
    assert!(session.read("ans").unwrap().is_none());
    let snapshot = session.read("A").unwrap().unwrap();
    let sum = success(run(&mut session, "A+1"));
    assert_eq!(sum.value.as_slice(), &[1.0;6]);
    assert_eq!(values(&session, "ans"), [1.0;6]);
    success(run(&mut session, "A=zeros(0,3);"));
    assert_eq!(snapshot.value.shape(), [2,3]);
    assert!(snapshot.value.as_slice().iter().all(|v| v.to_bits()==0));
    assert_eq!(snapshot.context.workspace_revision, 2);
    assert_eq!(session.read("A").unwrap().unwrap().value.shape(), [0,3]);
    let empty = success(run(&mut session, "zeros(3,0)"));
    assert_eq!(empty.value.shape(), [3,0]);
    assert!(empty.value.as_slice().is_empty());
    success(run(&mut session, "A"));
    assert_eq!(session.read("ans").unwrap().unwrap().value.shape(), [3,0]);
    assert_eq!(session.context().workspace_revision, 5);
}

#[test]
fn rs14_zeros_failed_transactions_workspace_limit_and_recovery() {
    let mut session = RustSessionV1::new().unwrap();
    success(run(&mut session, "A=7;"));
    success(run(&mut session, "9"));
    for source in ["A=zeros(17,0);", "B=zeros([1,2],missing);", "B=zeros();", "B=rand(2);"] {
        assert_error_unchanged(&mut session, source);
    }
    session.fail_reservation = Some(ReservationSite::Response);
    assert!(matches!(assert_error_unchanged(&mut session, "B=zeros(2,3);"),
        RustSessionErrorV1::Allocation {..}));
    assert_eq!(success(run(&mut session, "zeros(1)")).value.as_slice(), &[0.0]);
    success(run(&mut session, "zeros=9;"));
    assert!(matches!(assert_error_unchanged(&mut session, "A=zeros(missing);"),
        RustSessionErrorV1::Language(SessionExpressionErrorV1::UnknownBinding {..})));
    assert_eq!(success(run(&mut session, "zeros")).value.as_slice(), &[9.0]);
    session.reset().unwrap();
    for i in 0..64 { success(run(&mut session, &format!("v{i}=zeros(16);"))); }
    assert_eq!(session.workspace().count(), MAX_WORKSPACE_BINDINGS_V1);
    assert_eq!(session.retained_bytes, MAX_WORKSPACE_PAYLOAD_BYTES_V1);
    assert!(matches!(assert_error_unchanged(&mut session, "extra=zeros(0,3);"), RustSessionErrorV1::WorkspaceLimit));
    success(run(&mut session, "v0=zeros(0,3);"));
    assert_eq!(session.retained_bytes, MAX_WORKSPACE_PAYLOAD_BYTES_V1 - 2048);
    assert_eq!(session.read("v0").unwrap().unwrap().value.shape(), [0,3]);
}

#[test]
fn rs15_zeros_stop_deadline_and_prepublication_recovery() {
    for checkpoint in [Checkpoint::Task, Checkpoint::BeforeCommit] {
        let mut session = RustSessionV1::new().unwrap();
        success(run(&mut session, "A=7;"));
        success(run(&mut session, "9"));
        let before = (session.context(), snapshot(&session));
        let mut tasks = 0;
        session.hook = Some(Box::new(move |at, handle| {
            if at == Checkpoint::Task {tasks+=1;}
            if at==checkpoint && (at!=Checkpoint::Task || tasks==7) {handle.request_stop();}
        }));
        assert!(matches!(run(&mut session, "A=zeros(2,3);"),
            RustEvalOutcomeV1::Cancelled { reason: RustCancelReasonV1::UserStop, .. }));
        session.hook = None;
        assert_eq!((session.context(), snapshot(&session)), before);
        assert_eq!(success(run(&mut session, "zeros(1)")).value.as_slice(), &[0.0]);
    }
    let mut session = RustSessionV1::new().unwrap();
    success(run(&mut session, "A=7;"));
    let before = (session.context(), snapshot(&session));
    let request = session.prepare(Arc::from("A=zeros(16);"), Some(Instant::now())).unwrap();
    assert!(matches!(request.execute(), RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::Deadline, ..}));
    assert_eq!((session.context(), snapshot(&session)), before);
    assert_eq!(success(run(&mut session, "zeros(0,3)")).value.shape(), [0,3]);
}


#[test]
fn integer_ranges_session_transactions_cancellation_and_recovery() {
    let mut session = RustSessionV1::new().unwrap();
    success(run(&mut session,"x=7;"));
    success(run(&mut session,"9"));
    assert!(matches!(assert_error_unchanged(&mut session,"x=1:17;"),
        RustSessionErrorV1::Language(SessionExpressionErrorV1::RangeLength {actual:17,limit:16,..})));
    assert_error_unchanged(&mut session,"x=[1,2]:missing:3;");
    assert_eq!(values(&session,"x"),[7.0]);
    assert_eq!(values(&session,"ans"),[9.0]);
    for (checkpoint,stop_at) in [(Checkpoint::Task,7),(Checkpoint::Task,8),(Checkpoint::BeforeCommit,0)] {
        let mut session = RustSessionV1::new().unwrap();
        success(run(&mut session,"x=7;"));
        success(run(&mut session,"9"));
        let before = (session.context(),snapshot(&session));
        let mut tasks = 0;
        session.hook = Some(Box::new(move |at,handle| {
            if at==Checkpoint::Task {tasks+=1;}
            if at==checkpoint && (at!=Checkpoint::Task || tasks==stop_at) {handle.request_stop();}
        }));
        assert!(matches!(run(&mut session,"x=1:16;"),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::UserStop,..}));
        session.hook = None;
        assert_eq!((session.context(),snapshot(&session)),before);
        let value = success(run(&mut session,"x=1:16;")).value;
        assert_eq!(value.shape(),[1,16]);
        assert_eq!(value.as_slice(),(1..=16).map(f64::from).collect::<Vec<_>>());
        assert_eq!(success(run(&mut session,"numel(x)")).value.as_slice(),&[16.0]);
    }
    let before = (session.context(),snapshot(&session));
    let request = session.prepare(Arc::from("x=1:16;"),Some(Instant::now())).unwrap();
    assert!(matches!(request.execute(),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::Deadline,..}));
    assert_eq!((session.context(),snapshot(&session)),before);
    assert_eq!(success(run(&mut session,"1:2")).value.as_slice(),&[1.0,2.0]);
}


#[test]
fn reshape_session_snapshots_failed_transactions_cancellation_and_recovery() {
    let mut session=RustSessionV1::new().unwrap();
    success(run(&mut session,"A=[1,2,3;4,5,6];"));
    success(run(&mut session,"B=7;"));
    success(run(&mut session,"9"));
    let old=session.read("A").unwrap().unwrap();
    let reshaped=success(run(&mut session,"B=reshape(A,3,2);"));
    assert_eq!(reshaped.value.shape(),[3,2]);
    assert_eq!(reshaped.value.as_slice(),&[1.,4.,2.,5.,3.,6.]);
    assert_eq!(values(&session,"ans"),[9.]);
    for source in ["B=reshape(A,1,1);","B=reshape([], -1, 0);","C=reshape(A,1,missing);"] {
        assert_error_unchanged(&mut session,source);
    }
    session.fail_reservation=Some(ReservationSite::Response);
    assert!(matches!(assert_error_unchanged(&mut session,"B=reshape(A,3,2);"),RustSessionErrorV1::Allocation {..}));
    for (checkpoint,stop_at) in [(Checkpoint::Task,9),(Checkpoint::Task,10),(Checkpoint::BeforeCommit,0)] {
        let before=(session.context(),snapshot(&session));
        let mut tasks=0;
        session.hook=Some(Box::new(move |at,handle|{
            if at==Checkpoint::Task{tasks+=1;}
            if at==checkpoint && (at!=Checkpoint::Task || tasks==stop_at){handle.request_stop();}
        }));
        assert!(matches!(run(&mut session,"B=reshape(A,3,2);"),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::UserStop,..}));
        session.hook=None;
        assert_eq!((session.context(),snapshot(&session)),before);
        assert_eq!(success(run(&mut session,"reshape(A,1,6)")).value.shape(),[1,6]);
    }
    let before=(session.context(),snapshot(&session));
    let request=session.prepare(Arc::from("B=reshape(A,3,2);"),Some(Instant::now())).unwrap();
    assert!(matches!(request.execute(),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::Deadline,..}));
    assert_eq!((session.context(),snapshot(&session)),before);
    success(run(&mut session,"A=reshape(A,6,1);"));
    assert_eq!(old.value.shape(),[2,3]);
    assert_eq!(old.value.as_slice(),&[1.,4.,2.,5.,3.,6.]);
    assert_eq!(session.read("A").unwrap().unwrap().value.shape(),[6,1]);
    session.reset().unwrap();
    for i in 0..64 {success(run(&mut session,&format!("v{i}=ones(16);")));}
    assert!(matches!(assert_error_unchanged(&mut session,"extra=reshape(v0,16,16);"),RustSessionErrorV1::WorkspaceLimit));
    assert_eq!(success(run(&mut session,"v0=reshape(v0,16,16);")).value.shape(),[16,16]);
}

#[test]
fn scalar_index_session_bits_ans_suppression_snapshots_and_atomic_errors() {
    let mut session=RustSessionV1::new().unwrap();
    success(run(&mut session,"A=[1,2,3;4,5,6];"));
    success(run(&mut session,"91"));
    let old=session.read("A").unwrap().unwrap();
    let result=success(run(&mut session,"A(2)"));
    assert_eq!(result.value.as_slice(),&[4.]);
    assert_eq!(result.display_name.as_deref(),Some("ans"));
    assert_eq!(result.after.workspace_revision,result.before.workspace_revision+1);
    assert_eq!(values(&session,"ans"),[4.]);
    let suppressed=success(run(&mut session,"A(1,2);"));
    assert!(suppressed.display_name.is_none());
    assert_eq!(values(&session,"ans"),[2.]);
    for source in ["A()","A([],4)","A([1,2;1,2])","A(1,1,1)","A(7)","A(3,1)","A(1,4)","A(-0)","A(1.5)","A(0,missing)"] {
        let retained=session.retained_bytes;
        assert_error_unchanged(&mut session,source);
        assert_eq!(session.retained_bytes,retained);
        assert_eq!(success(run(&mut session,"A(2)")).value.as_slice(),&[4.]);
    }
    let words=[0x8000000000000000,0x7ff8000000000042];
    session.bindings.push(Binding {name:"P".into(),value:SessionValueV1::try_from_col_major([2,1],&words.map(f64::from_bits)).unwrap()});
    session.retained_bytes+=16;
    for (source,word) in [("P(1)",words[0]),("P(2,1)",words[1])] {
        assert_eq!(success(run(&mut session,source)).value.as_slice()[0].to_bits(),word);
        assert_eq!(session.read("ans").unwrap().unwrap().value.as_slice()[0].to_bits(),word);
    }
    success(run(&mut session,"A=0;"));
    assert_eq!(old.value.shape(),[2,3]);
    assert_eq!(old.value.as_slice(),&[1.,4.,2.,5.,3.,6.]);
    assert_eq!(session.read("P").unwrap().unwrap().value.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),words);
}

#[test]
fn scalar_index_session_reservations_quotas_revision_cancel_deadline_recovery() {
    for site in [ReservationSite::Name,ReservationSite::Slot,ReservationSite::Response] {
        let mut session=RustSessionV1::new().unwrap();
        success(run(&mut session,"A=[1;4];"));
        success(run(&mut session,"91"));
        let retained=session.retained_bytes;
        session.fail_reservation=Some(site);
        assert!(matches!(assert_error_unchanged(&mut session,"B=A(2);"),RustSessionErrorV1::Allocation {..}));
        assert_eq!(session.retained_bytes,retained);
        let revision=session.revision;
        assert_eq!(success(run(&mut session,"A(2)")).value.as_slice(),&[4.]);
        assert_eq!(session.revision,revision+1);
    }
    for checkpoint in [Checkpoint::Task,Checkpoint::AfterEvaluation,Checkpoint::BeforeCommit] {
        let mut session=RustSessionV1::new().unwrap();
        success(run(&mut session,"A=[1;4];"));
        success(run(&mut session,"91"));
        let before=(session.context(),snapshot(&session),session.retained_bytes);
        session.hook=Some(Box::new(move|at,handle|if at==checkpoint{handle.request_stop();}));
        assert!(matches!(run(&mut session,"A(2)"),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::UserStop,..}));
        session.hook=None;
        assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
        assert_eq!(success(run(&mut session,"A(2)")).value.as_slice(),&[4.]);
        let before=(session.context(),snapshot(&session),session.retained_bytes);
        let request=session.prepare(Arc::from("A(2)"),Some(Instant::now())).unwrap();
        assert!(matches!(request.execute(),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::Deadline,..}));
        assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
        assert_eq!(success(run(&mut session,"A(2)")).value.as_slice(),&[4.]);
        let revision=session.revision;
        session.revision=u64::MAX;
        assert!(matches!(assert_error_unchanged(&mut session,"A(2)"),RustSessionErrorV1::CounterExhausted));
        session.revision=revision;
        assert_eq!(success(run(&mut session,"A(2)")).value.as_slice(),&[4.]);
    }
    let mut session=RustSessionV1::new().unwrap();
    for i in 0..64 {success(run(&mut session,&format!("v{i}=ones(16);")));}
    assert!(matches!(assert_error_unchanged(&mut session,"v0(1)"),RustSessionErrorV1::WorkspaceLimit));
    assert!(matches!(assert_error_unchanged(&mut session,"extra=v0(1);"),RustSessionErrorV1::WorkspaceLimit));
    assert_eq!(success(run(&mut session,"v0=v0(256);")).value.as_slice(),&[1.]);
    assert_eq!(session.retained_bytes,MAX_WORKSPACE_PAYLOAD_BYTES_V1-2048+8);
}

#[test]
fn vector_index_session_ans_assignment_snapshots_payloads_and_error_atomicity() {
    let mut session=RustSessionV1::new().unwrap();
    success(run(&mut session,"A=[1,2,3;4,5,6];"));success(run(&mut session,"91"));
    let old=session.read("A").unwrap().unwrap();
    let result=success(run(&mut session,"A([2,1])"));
    assert_eq!(result.value.shape(),[1,2]);assert_eq!(result.value.as_slice(),&[4.,1.]);
    assert_eq!(result.display_name.as_deref(),Some("ans"));assert_eq!(result.after.workspace_revision,result.before.workspace_revision+1);
    let suppressed=success(run(&mut session,"A([2;1]);"));
    assert_eq!(suppressed.value.shape(),[2,1]);assert!(suppressed.display_name.is_none());
    assert_eq!(session.read("ans").unwrap().unwrap().value.shape(),[2,1]);
    let assigned=success(run(&mut session,"B=A([2,1]);"));
    assert_eq!(assigned.value.as_slice(),&[4.,1.]);assert_eq!(values(&session,"ans"),[4.,1.]);
    let assigned=success(run(&mut session,"B=A([2,1],[3,1]);"));
    assert_eq!(assigned.value.as_slice(),&[6.,3.,4.,1.]);assert_eq!(values(&session,"ans"),[4.,1.]);
    for source in ["B=A([1,0]);","B=A([2,1],[4,1e309]);","B=A([],4);","B=A([1,2;1,2]);","B=A([0,1],missing);"] {
        assert_error_unchanged(&mut session,source);
        assert_eq!(success(run(&mut session,"A(2)")).value.as_slice(),&[4.]);
    }
    success(run(&mut session,"A=A([2,1]);"));
    assert_eq!(session.read("A").unwrap().unwrap().value.shape(),[1,2]);assert_eq!(values(&session,"A"),[4.,1.]);
    assert_eq!(old.value.shape(),[2,3]);assert_eq!(old.value.as_slice(),&[1.,4.,2.,5.,3.,6.]);
    let words=[0x8000000000000000,0x7ff8000000000042,0x7ff0000000000000,0xfff0000000000000];
    session.bindings.push(Binding {name:"H".into(),value:SessionValueV1::try_from_col_major([2,2],&words.map(f64::from_bits)).unwrap()});session.retained_bytes+=32;
    let old=session.read("H").unwrap().unwrap();
    let expected=[words[1],words[0],words[1],words[3],words[2]];
    let result=success(run(&mut session,"H([2,1,2,4,3])"));
    assert_eq!(result.value.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),expected);
    assert_eq!(session.read("ans").unwrap().unwrap().value.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),expected);
    success(run(&mut session,"H=H([2,1,2,4,3]);"));
    assert_eq!(session.read("H").unwrap().unwrap().value.shape(),[1,5]);
    assert_eq!(old.value.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),words);
    assert_eq!(session.retained_bytes,session.bindings.iter().map(|b|b.value.payload_bytes()).sum::<usize>());
}

#[test]
fn vector_index_session_reservations_cancellation_quotas_revision_and_recovery() {
    for site in [ReservationSite::Name,ReservationSite::Slot,ReservationSite::Response] {
        let mut session=RustSessionV1::new().unwrap();
        success(run(&mut session,"A=[1;4];"));success(run(&mut session,"91"));
        session.fail_reservation=Some(site);
        assert!(matches!(assert_error_unchanged(&mut session,"B=A([2,1]);"),RustSessionErrorV1::Allocation {..}));
        assert_eq!(success(run(&mut session,"A([2,1])")).value.as_slice(),&[4.,1.]);
    }
    for checkpoint in [Checkpoint::Task,Checkpoint::AfterEvaluation,Checkpoint::BeforeCommit] {
        let mut session=RustSessionV1::new().unwrap();
        success(run(&mut session,"A=[1,2,3;4,5,6];"));success(run(&mut session,"91"));
        let before=(session.context(),snapshot(&session),session.retained_bytes);
        session.hook=Some(Box::new(move|at,handle|if at==checkpoint{handle.request_stop();}));
        assert!(matches!(run(&mut session,"A([2,1],[3;1;2])"),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::UserStop,..}));
        session.hook=None;assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
        assert_eq!(success(run(&mut session,"A(2)")).value.as_slice(),&[4.]);
        let before=(session.context(),snapshot(&session),session.retained_bytes);
        let request=session.prepare(Arc::from("A([2,1],[3;1;2])"),Some(Instant::now())).unwrap();
        assert!(matches!(request.execute(),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::Deadline,..}));
        assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
        let revision=session.revision;session.revision=u64::MAX;
        assert!(matches!(assert_error_unchanged(&mut session,"A([2,1])"),RustSessionErrorV1::CounterExhausted));
        session.revision=revision;
        assert_eq!(success(run(&mut session,"A([2,1],[3;1;2])")).value.as_slice(),&[6.,3.,4.,1.,5.,2.]);
    }
    let mut session=RustSessionV1::new().unwrap();
    for i in 0..64 {success(run(&mut session,&format!("v{i}=ones(16);")));}
    assert_eq!(session.retained_bytes,MAX_WORKSPACE_PAYLOAD_BYTES_V1);
    assert!(matches!(assert_error_unchanged(&mut session,"v0([1,1])"),RustSessionErrorV1::WorkspaceLimit));
    assert!(matches!(assert_error_unchanged(&mut session,"extra=v0([1,1]);"),RustSessionErrorV1::WorkspaceLimit));
    assert_eq!(success(run(&mut session,"v0=v0([256,1]);")).value.as_slice(),&[1.,1.]);
    assert_eq!(session.retained_bytes,MAX_WORKSPACE_PAYLOAD_BYTES_V1-2048+16);
}

#[test]
fn matrix_whitespace_session_assignment_bits_indexing_transactions_and_recovery() {
    let mut session=RustSessionV1::new().unwrap();
    let assigned=success(run(&mut session,"A=[1 2 3;4 5 6];"));
    assert_eq!(assigned.value.shape(),[2,3]);assert_eq!(assigned.value.as_slice(),&[1.,4.,2.,5.,3.,6.]);assert!(assigned.display_name.is_none());
    success(run(&mut session,"B=7;"));success(run(&mut session,"91"));
    let old=session.read("A").unwrap().unwrap();
    for source in ["B=[1 +];","B=[1 2;3];","B=[1e -2];","B=[1;];","B=[1 2+];"] {
        assert_error_unchanged(&mut session,source);
        assert_eq!(success(run(&mut session,"A([2 1])")).value.as_slice(),&[4.,1.]);
    }
    session.fail_reservation=Some(ReservationSite::Response);
    assert!(matches!(assert_error_unchanged(&mut session,"B=[1 2;3 4];"),RustSessionErrorV1::Allocation {..}));
    let before=(session.context(),snapshot(&session),session.retained_bytes);
    session.hook=Some(Box::new(|at,handle|if at==Checkpoint::BeforeCommit{handle.request_stop();}));
    assert!(matches!(run(&mut session,"B=[1 2;3 4];"),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::UserStop,..}));
    session.hook=None;assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
    assert_eq!(success(run(&mut session,"A([2 1],[3;1;2])")).value.as_slice(),&[6.,3.,4.,1.,5.,2.]);
    let zeros=success(run(&mut session,"[-0 +0]"));assert_eq!(zeros.value.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),[0x8000000000000000,0]);
    assert_eq!(session.read("ans").unwrap().unwrap().value.as_slice()[0].to_bits(),0x8000000000000000);
    success(run(&mut session,"A=A([2 1]);"));assert_eq!(values(&session,"A"),[4.,1.]);assert_eq!(old.value.as_slice(),&[1.,4.,2.,5.,3.,6.]);
}

#[test]
fn empty_index_session_ans_assignment_suppression_retained_bytes_and_snapshots() {
    let mut session=RustSessionV1::new().unwrap();
    success(run(&mut session,"A=[1 2 3;4 5 6];"));success(run(&mut session,"B=7;"));success(run(&mut session,"91"));
    let old=session.read("A").unwrap().unwrap();let retained=session.retained_bytes;
    let result=success(run(&mut session,"A([])"));
    assert_eq!(result.value.shape(),[0,0]);assert_eq!(result.value.payload_bytes(),0);assert_eq!(result.display_name.as_deref(),Some("ans"));
    assert_eq!(result.after.workspace_revision,result.before.workspace_revision+1);assert_eq!(session.retained_bytes,retained-8);
    let result=success(run(&mut session,"A([],ones(1,16));"));
    assert_eq!(result.value.shape(),[0,16]);assert!(result.display_name.is_none());assert_eq!(session.read("ans").unwrap().unwrap().value.shape(),[0,16]);
    let result=success(run(&mut session,"B=A(ones(16,1),[]);"));
    assert_eq!(result.value.shape(),[16,0]);assert!(result.display_name.is_none());assert_eq!(session.read("B").unwrap().unwrap().value.shape(),[16,0]);
    assert_eq!(session.read("ans").unwrap().unwrap().value.shape(),[0,16]);assert_eq!(session.retained_bytes,48);
    for source in ["B=A([],0);","B=A([],[4,1e309]);","B=A([],missing);","B=A([3,1],[]);","B=A([],ones(1,17));"] {
        assert_error_unchanged(&mut session,source);
        assert_eq!(success(run(&mut session,"A([],[])" )).value.shape(),[0,0]);
    }
    success(run(&mut session,"A=A([]);"));assert_eq!(session.retained_bytes,0);
    assert_eq!(old.value.shape(),[2,3]);assert_eq!(old.value.as_slice(),&[1.,4.,2.,5.,3.,6.]);
    assert!(session.workspace().all(|m|m.shape[0]==0 || m.shape[1]==0));
    success(run(&mut session,"A=[-0,1];"));
    assert_eq!(success(run(&mut session,"A(1)" )).value.as_slice()[0].to_bits(),0x8000000000000000);
    assert_eq!(session.retained_bytes,session.bindings.iter().map(|b|b.value.payload_bytes()).sum::<usize>());
}

#[test]
fn empty_index_session_metadata_reservations_stops_deadline_revision_and_quota_recovery() {
    for site in [ReservationSite::Name,ReservationSite::Slot,ReservationSite::Response] {
        let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=[1;4];"));success(run(&mut session,"91"));
        session.fail_reservation=Some(site);
        assert!(matches!(assert_error_unchanged(&mut session,"B=A([]);"),RustSessionErrorV1::Allocation {..}));
        let revision=session.revision;
        assert_eq!(success(run(&mut session,"B=A([]);" )).value.shape(),[0,0]);assert_eq!(session.revision,revision+1);
        assert_eq!(values(&session,"ans"),[91.]);assert_eq!(session.read("B").unwrap().unwrap().value.payload_bytes(),0);
    }
    for checkpoint in [Checkpoint::Task,Checkpoint::AfterEvaluation,Checkpoint::BeforeCommit] {
        let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=[1 2 3;4 5 6];"));success(run(&mut session,"91"));
        let before=(session.context(),snapshot(&session),session.retained_bytes);
        session.hook=Some(Box::new(move|at,handle|if at==checkpoint{handle.request_stop();}));
        assert!(matches!(run(&mut session,"B=A([],ones(1,16));"),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::UserStop,..}));
        session.hook=None;assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
        let request=session.prepare(Arc::from("B=A([],ones(1,16));"),Some(Instant::now())).unwrap();
        assert!(matches!(request.execute(),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::Deadline,..}));
        assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
        let revision=session.revision;session.revision=u64::MAX;
        assert!(matches!(assert_error_unchanged(&mut session,"A([])"),RustSessionErrorV1::CounterExhausted));session.revision=revision;
        assert_eq!(success(run(&mut session,"A([],ones(1,16))")).value.shape(),[0,16]);assert_eq!(session.revision,revision+1);
    }
    let mut session=RustSessionV1::new().unwrap();
    for i in 0..63 {success(run(&mut session,&format!("v{i}=ones(16);")));}
    success(run(&mut session,"ones(16)"));assert_eq!(session.retained_bytes,MAX_WORKSPACE_PAYLOAD_BYTES_V1);
    assert!(matches!(assert_error_unchanged(&mut session,"extra=v0([]);"),RustSessionErrorV1::WorkspaceLimit));
    assert_eq!(success(run(&mut session,"v0([])")).value.payload_bytes(),0);assert_eq!(session.retained_bytes,MAX_WORKSPACE_PAYLOAD_BYTES_V1-2048);
    success(run(&mut session,"v0=v0([]);"));assert_eq!(session.retained_bytes,MAX_WORKSPACE_PAYLOAD_BYTES_V1-4096);
}

#[test]
fn colon_index_session_values_metadata_assignment_ans_errors_and_recovery() {
    let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=[1 2 3;4 5 6];"));success(run(&mut session,"91"));
    let old=session.read("A").unwrap().unwrap();
    let result=success(run(&mut session,"A(:)"));assert_eq!(result.value.shape(),[6,1]);assert_eq!(result.value.as_slice(),&[1.,4.,2.,5.,3.,6.]);
    assert_eq!(result.display_name.as_deref(),Some("ans"));assert_eq!(result.after.workspace_revision,result.before.workspace_revision+1);
    let result=success(run(&mut session,"B=A(:,[3,1]);"));assert!(result.display_name.is_none());assert_eq!(result.value.as_slice(),&[3.,6.,1.,4.]);
    assert_eq!(session.read("ans").unwrap().unwrap().value.shape(),[6,1]);
    for source in ["B=A(:,4);","B=A([0,1],:);","B=A(:,missing);","B=zeros(:,missing);","B=zeros(:);","B=A(:,:,:);"] {
        assert_error_unchanged(&mut session,source);assert_eq!(success(run(&mut session,"A(:,:)" )).value.shape(),[2,3]);
    }
    success(run(&mut session,"M=ones(16);"));assert_error_unchanged(&mut session,"B=M(:);");
    assert_eq!(success(run(&mut session,"M(:,:)" )).value.payload_bytes(),2048);
    let result=success(run(&mut session,"A=A(:);"));assert_eq!(result.value.shape(),[6,1]);
    assert_eq!(old.value.shape(),[2,3]);assert_eq!(old.value.as_slice(),&[1.,4.,2.,5.,3.,6.]);
    success(run(&mut session,"E=zeros(1,0);"));assert_eq!(success(run(&mut session,"E(:)" )).value.shape(),[0,1]);
    assert_eq!(session.read("ans").unwrap().unwrap().value.payload_bytes(),0);
    let words=[0x8000000000000000,0x7ff8000000000042];
    session.bindings.push(Binding {name:"H".into(),value:SessionValueV1::try_from_col_major([1,2],&words.map(f64::from_bits)).unwrap()});session.retained_bytes+=16;
    assert_eq!(success(run(&mut session,"H(:)" )).value.as_slice().iter().map(|w|w.to_bits()).collect::<Vec<_>>(),words);
    assert_eq!(session.read("ans").unwrap().unwrap().value.shape(),[2,1]);
    assert_eq!(session.retained_bytes,session.bindings.iter().map(|b|b.value.payload_bytes()).sum::<usize>());
}

#[test]
fn colon_index_session_metadata_reservations_cancellation_revision_quotas_and_recovery() {
    for site in [ReservationSite::Name,ReservationSite::Slot,ReservationSite::Response] {
        let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=[1;4];"));success(run(&mut session,"91"));
        session.fail_reservation=Some(site);
        assert!(matches!(assert_error_unchanged(&mut session,"B=A(:);"),RustSessionErrorV1::Allocation {..}));
        assert_eq!(success(run(&mut session,"B=A(:);" )).value.as_slice(),&[1.,4.]);
    }
    for checkpoint in [Checkpoint::Task,Checkpoint::AfterEvaluation,Checkpoint::BeforeCommit] {
        let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=[1 2 3;4 5 6];"));success(run(&mut session,"91"));
        let before=(session.context(),snapshot(&session),session.retained_bytes);
        session.hook=Some(Box::new(move|at,handle|if at==checkpoint{handle.request_stop();}));
        assert!(matches!(run(&mut session,"B=A(:,[3,1]);"),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::UserStop,..}));
        session.hook=None;assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
        let request=session.prepare(Arc::from("A(:,: )"),Some(Instant::now())).unwrap();
        assert!(matches!(request.execute(),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::Deadline,..}));
        assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
        let revision=session.revision;session.revision=u64::MAX;
        assert!(matches!(assert_error_unchanged(&mut session,"A(:)"),RustSessionErrorV1::CounterExhausted));session.revision=revision;
        assert_eq!(success(run(&mut session,"A(:)" )).value.shape(),[6,1]);
    }
    let mut session=RustSessionV1::new().unwrap();for i in 0..64 {success(run(&mut session,&format!("v{i}=ones(16);")));}
    assert!(matches!(assert_error_unchanged(&mut session,"extra=v0(:,:);"),RustSessionErrorV1::WorkspaceLimit));
    assert_error_unchanged(&mut session,"v0(:)");assert_eq!(session.retained_bytes,MAX_WORKSPACE_PAYLOAD_BYTES_V1);
    success(run(&mut session,"v0=v0(1,:);"));assert_eq!(session.retained_bytes,MAX_WORKSPACE_PAYLOAD_BYTES_V1-2048+128);
}

#[test]
fn contextual_end_session_assignment_ans_words_independence_and_error_atomicity() {
    let mut session=RustSessionV1::new().unwrap();
    success(run(&mut session,"A=[1 2 3;4 5 6];"));success(run(&mut session,"91"));
    let old=session.read("A").unwrap().unwrap();
    let result=success(run(&mut session,"A(end,end)"));assert_eq!(result.value.as_slice(),&[6.]);assert_eq!(result.display_name.as_deref(),Some("ans"));assert_eq!(result.after.workspace_revision,result.before.workspace_revision+1);
    let assigned=success(run(&mut session,"B=A(:,end);"));assert!(assigned.display_name.is_none());assert_eq!(assigned.value.as_slice(),&[3.,6.]);
    assert_eq!(values(&session,"ans"),[6.]);
    for source in ["B=A(end+1);","B=A(end,missing);","B=A(end,missing,1);","B=numel(end);","B=[end];","B=A(end-end);"] {
        let retained=session.retained_bytes;assert_error_unchanged(&mut session,source);assert_eq!(session.retained_bytes,retained);
        assert_eq!(success(run(&mut session,"A(reshape(end,1,1))")).value.as_slice(),&[6.]);
    }
    let words=[0x8000000000000000,0x7ff8000000000042];
    session.bindings.push(Binding {name:"H".into(),value:SessionValueV1::try_from_col_major([1,2],&words.map(f64::from_bits)).unwrap()});session.retained_bytes+=16;
    let selected=success(run(&mut session,"H(end:-1:1)"));assert_eq!(selected.value.as_slice().iter().map(|w|w.to_bits()).collect::<Vec<_>>(),[words[1],words[0]]);
    success(run(&mut session,"A=A(end);"));assert_eq!(values(&session,"A"),[6.]);assert_eq!(old.value.as_slice(),&[1.,4.,2.,5.,3.,6.]);
    assert_eq!(selected.value.as_slice()[0].to_bits(),words[1]);
}

#[test]
fn contextual_end_session_reservations_cancel_deadline_revision_and_quotas() {
    for site in [ReservationSite::Name,ReservationSite::Slot,ReservationSite::Response] {
        let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=[1 2 3;4 5 6];"));success(run(&mut session,"91"));session.fail_reservation=Some(site);
        assert!(matches!(assert_error_unchanged(&mut session,"B=A(:,end);"),RustSessionErrorV1::Allocation {..}));
        assert_eq!(success(run(&mut session,"B=A(:,end);")).value.as_slice(),&[3.,6.]);
    }
    for checkpoint in [Checkpoint::Task,Checkpoint::AfterEvaluation,Checkpoint::BeforeCommit] {
        let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=[1 2 3;4 5 6];"));success(run(&mut session,"91"));let before=(session.context(),snapshot(&session),session.retained_bytes);
        session.hook=Some(Box::new(move|at,handle|if at==checkpoint{handle.request_stop();}));
        assert!(matches!(run(&mut session,"B=A(reshape(end,1,1));"),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::UserStop,..}));session.hook=None;
        assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
        let request=session.prepare(Arc::from("A(:,end)"),Some(Instant::now())).unwrap();assert!(matches!(request.execute(),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::Deadline,..}));assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
        let revision=session.revision;session.revision=u64::MAX;assert!(matches!(assert_error_unchanged(&mut session,"A(end)"),RustSessionErrorV1::CounterExhausted));session.revision=revision;assert_eq!(success(run(&mut session,"A(end)")).value.as_slice(),&[6.]);
    }
    let mut session=RustSessionV1::new().unwrap();for i in 0..64 {success(run(&mut session,&format!("v{i}=ones(16);")));}
    assert!(matches!(assert_error_unchanged(&mut session,"extra=v0(end);"),RustSessionErrorV1::WorkspaceLimit));assert!(matches!(assert_error_unchanged(&mut session,"v0(end)"),RustSessionErrorV1::WorkspaceLimit));
    success(run(&mut session,"v0=v0(end);"));assert_eq!(session.retained_bytes,MAX_WORKSPACE_PAYLOAD_BYTES_V1-2048+8);
}

#[test]
fn construction_session_values_bits_ans_assignment_independence_and_atomic_errors() {
    let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=[1;2];"));success(run(&mut session,"91"));let old=session.read("A").unwrap().unwrap();
    let result=success(run(&mut session,"[A A;A A]"));assert_eq!(result.value.shape(),[4,2]);assert_eq!(result.value.as_slice(),&[1.,2.,1.,2.,1.,2.,1.,2.]);assert_eq!(result.display_name.as_deref(),Some("ans"));assert_eq!(result.after.workspace_revision,result.before.workspace_revision+1);
    let assigned=success(run(&mut session,"B=[A,ones(2,1)];"));assert!(assigned.display_name.is_none());assert_eq!(assigned.value.as_slice(),&[1.,2.,1.,1.]);assert_eq!(values(&session,"ans"),[1.,2.,1.,2.,1.,2.,1.,2.]);
    for source in ["B=[A,1];","B=[A,1;missing];","B=[A;missing];","B=[zeros(0,16),zeros(0,1)];","B=[end];","B=[[1,2;3],missing];"] {let retained=session.retained_bytes;assert_error_unchanged(&mut session,source);assert_eq!(session.retained_bytes,retained);assert_eq!(success(run(&mut session,"A([end])")).value.as_slice(),&[2.]);}
    success(run(&mut session,"A=[A;A];"));assert_eq!(values(&session,"A"),[1.,2.,1.,2.]);assert_eq!(old.value.as_slice(),&[1.,2.]);
    success(run(&mut session,"E=[zeros(0,2),zeros(3,0),zeros(0,4)];"));assert_eq!(session.read("E").unwrap().unwrap().value.shape(),[0,0]);
    let words=[0x8000000000000000,0x7ff8000000000042];session.bindings.push(Binding {name:"H".into(),value:SessionValueV1::try_from_col_major([1,2],&words.map(f64::from_bits)).unwrap()});session.retained_bytes+=16;
    let result=success(run(&mut session,"[H;H]"));assert_eq!(result.value.as_slice().iter().map(|w|w.to_bits()).collect::<Vec<_>>(),[words[0],words[0],words[1],words[1]]);
}

#[test]
fn construction_session_reservations_cancellation_deadline_revision_quota_and_recovery() {
    for site in [ReservationSite::Name,ReservationSite::Slot,ReservationSite::Response] {
        let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=[1;2];"));success(run(&mut session,"91"));session.fail_reservation=Some(site);
        assert!(matches!(assert_error_unchanged(&mut session,"B=[A,A];"),RustSessionErrorV1::Allocation {..}));assert_eq!(success(run(&mut session,"B=[A,A];")).value.shape(),[2,2]);
    }
    for checkpoint in [Checkpoint::Task,Checkpoint::AfterEvaluation,Checkpoint::BeforeCommit] {
        let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=[1;2];"));success(run(&mut session,"91"));let before=(session.context(),snapshot(&session),session.retained_bytes);session.hook=Some(Box::new(move|at,handle|if at==checkpoint{handle.request_stop();}));
        assert!(matches!(run(&mut session,"B=[A,A;A,A];"),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::UserStop,..}));session.hook=None;assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
        let request=session.prepare(Arc::from("[A,A]"),Some(Instant::now())).unwrap();assert!(matches!(request.execute(),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::Deadline,..}));assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
        let revision=session.revision;session.revision=u64::MAX;assert!(matches!(assert_error_unchanged(&mut session,"[A,A]"),RustSessionErrorV1::CounterExhausted));session.revision=revision;assert_eq!(success(run(&mut session,"[A,A]")).value.shape(),[2,2]);
    }
    let mut session=RustSessionV1::new().unwrap();for i in 0..64{success(run(&mut session,&format!("v{i}=ones(16);")));}assert!(matches!(assert_error_unchanged(&mut session,"extra=[v0];"),RustSessionErrorV1::WorkspaceLimit));assert!(matches!(assert_error_unchanged(&mut session,"[v0]"),RustSessionErrorV1::WorkspaceLimit));success(run(&mut session,"v0=[v0(1,end)];"));assert_eq!(session.retained_bytes,MAX_WORKSPACE_PAYLOAD_BYTES_V1-2048+8);
}

#[test]
fn indexed_write_session_fixed_geometry_bits_ans_revision_empty_policy_and_atomicity() {
    let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=[1 2 3;4 5 6];"));success(run(&mut session,"91"));let old=session.read("A").unwrap().unwrap();
    let result=success(run(&mut session,"A([2,1],[3,1])=[10,30;20,40]"));assert_eq!(result.value.shape(),[2,3]);assert_eq!(result.value.as_slice(),&[40.,30.,2.,5.,20.,10.]);assert_eq!(result.display_name.as_deref(),Some("A"));assert_eq!(result.after.workspace_revision,result.before.workspace_revision+1);assert_eq!(values(&session,"ans"),[91.]);
    let retained=session.retained_bytes;let result=success(run(&mut session,"A([1,1])=[7,8];"));assert!(result.display_name.is_none());assert_eq!(values(&session,"A"),[8.,30.,2.,5.,20.,10.]);assert_eq!(session.retained_bytes,retained);assert_eq!(old.value.as_slice(),&[1.,4.,2.,5.,3.,6.]);
    success(run(&mut session,"A([2,1])=A([1,2]);"));assert_eq!(values(&session,"A"),[30.,8.,2.,5.,20.,10.]);
    for source in ["New(1)=1;","A(7)=1;","A(0,missing)=[];","A(1)=[];","A(1)=zeros(0,0);","A([1,2],[1,2])=[1,2,3,4];","A(1)=rhs_missing;"] {assert_error_unchanged(&mut session,source);assert_eq!(session.retained_bytes,retained);}
    success(run(&mut session,"E=zeros(0,3);"));success(run(&mut session,"Z=zeros(0,1);"));
    for source in ["E(:,1)=[];","E(:,1)=zeros(0,0);","E(:,1)=([]);"] {assert_error_unchanged(&mut session,source);assert_eq!(session.read("E").unwrap().unwrap().value.shape(),[0,3]);}
    let before=session.context();let retained=session.retained_bytes;let no_op=success(run(&mut session,"E(:,1)=Z;"));assert_eq!(no_op.value.shape(),[0,3]);assert!(no_op.value.as_slice().is_empty());assert_eq!(no_op.after.workspace_revision,before.workspace_revision+1);assert_eq!(session.retained_bytes,retained);assert_eq!(values(&session,"ans"),[91.]);
    let words=[0x8000000000000000,0x7ff8000000000042];session.bindings.push(Binding {name:"H".into(),value:SessionValueV1::try_from_col_major([1,2],&words.map(f64::from_bits)).unwrap()});session.retained_bytes+=16;let old_h=session.read("H").unwrap().unwrap();let result=success(run(&mut session,"H([2,1])=H;"));assert_eq!(result.value.as_slice().iter().map(|w|w.to_bits()).collect::<Vec<_>>(),[words[1],words[0]]);assert_eq!(old_h.value.as_slice().iter().map(|w|w.to_bits()).collect::<Vec<_>>(),words);
}

#[test]
fn indexed_write_session_reservations_phase_cancellation_deadline_quota_and_recovery() {
    for site in [ReservationSite::Name,ReservationSite::Response] {
        let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=[1 2 3;4 5 6];"));success(run(&mut session,"91"));session.fail_reservation=Some(site);assert!(matches!(assert_error_unchanged(&mut session,"A([end;1],end)=A([1,end]);"),RustSessionErrorV1::Allocation {..}));assert_eq!(success(run(&mut session,"A(end)=7;")).value.as_slice()[5],7.);
    }
    for checkpoint in [Checkpoint::Task,Checkpoint::AfterEvaluation,Checkpoint::BeforeCommit] {
        let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=[1 2 3;4 5 6];"));success(run(&mut session,"91"));let before=(session.context(),snapshot(&session),session.retained_bytes);session.hook=Some(Box::new(move|at,handle|if at==checkpoint{handle.request_stop();}));assert!(matches!(run(&mut session,"A([end;1],end)=A([1,end]);"),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::UserStop,..}));session.hook=None;assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
        let request=session.prepare(Arc::from("A(end)=7"),Some(Instant::now())).unwrap();assert!(matches!(request.execute(),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::Deadline,..}));assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);let revision=session.revision;session.revision=u64::MAX;assert!(matches!(assert_error_unchanged(&mut session,"A(end)=7"),RustSessionErrorV1::CounterExhausted));session.revision=revision;assert_eq!(success(run(&mut session,"A(end)=7")).value.as_slice()[5],7.);
    }
    let mut session=RustSessionV1::new().unwrap();for i in 0..64{success(run(&mut session,&format!("v{i}=ones(16);")));}assert_eq!(session.retained_bytes,MAX_WORKSPACE_PAYLOAD_BYTES_V1);let old=session.read("v0").unwrap().unwrap();let before=session.context();let result=success(run(&mut session,"v0(:)=2;"));assert_eq!(result.value.shape(),[16,16]);assert!(result.value.as_slice().iter().all(|w|*w==2.));assert_eq!(result.after.workspace_revision,before.workspace_revision+1);assert_eq!(session.retained_bytes,MAX_WORKSPACE_PAYLOAD_BYTES_V1);assert!(old.value.as_slice().iter().all(|w|*w==1.));assert!(session.read("ans").unwrap().is_none());assert_error_unchanged(&mut session,"v0(:)");assert_eq!(success(run(&mut session,"v0(:,:)=3;")).value.as_slice(),&[3.;256]);
}

#[test]
fn sum_default_session_ans_suppression_revision_bits_and_atomic_recovery() {
    let mut session=RustSessionV1::new().unwrap();
    success(run(&mut session,"A=[1 2 3;4 5 6];"));
    let old=session.read("A").unwrap().unwrap();
    let result=success(run(&mut session,"sum(A)"));
    assert_eq!(result.value.as_slice(), &[5.,7.,9.]);
    assert_eq!(result.display_name.as_deref(), Some("ans"));
    assert_eq!(result.after.workspace_revision,result.before.workspace_revision+1);
    let assigned=success(run(&mut session,"B=sum(A);"));
    assert!(assigned.display_name.is_none()); assert_eq!(values(&session,"ans"),[5.,7.,9.]);
    success(run(&mut session,"A(1,:)=sum(A);"));
    assert_eq!(old.value.as_slice(), &[1.,4.,2.,5.,3.,6.]);
    assert_eq!(values(&session,"B"),[5.,7.,9.]);
    for source in ["B=sum(missing);","B=sum(A,missing);","B=sum(:);","B=sum(zeros(17));"] {
        assert_error_unchanged(&mut session,source);
        assert_eq!(success(run(&mut session,"sum(B)")).value.as_slice(), &[21.]);
    }
    let empty=success(run(&mut session,"sum(zeros(0,0))"));
    assert_eq!(empty.value.shape(),[1,1]);assert_eq!(empty.value.as_slice()[0].to_bits(),0);
    assert_eq!(success(run(&mut session,"sum(zeros(3,0))")).value.shape(),[1,0]);
    assert_eq!(success(run(&mut session,"sum([-0 -0])")).value.as_slice()[0].to_bits(),0x8000000000000000);
    // Intentional private accuracy amendment U27: cancellation is exact before one rounding.
    assert_eq!(success(run(&mut session,"sum([1e308 1e308 -1e308 -1e308])")).value.as_slice()[0].to_bits(),0);
    success(run(&mut session,"sum=[8 9];"));
    assert_eq!(success(run(&mut session,"sum(end)")).value.as_slice(), &[9.]);
}

#[test]
fn sum_default_session_reservations_cancel_deadline_quotas_and_recovery() {
    for site in [ReservationSite::Name,ReservationSite::Slot,ReservationSite::Response] {
        let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=ones(16);"));success(run(&mut session,"91"));
        session.fail_reservation=Some(site);
        assert!(matches!(assert_error_unchanged(&mut session,"B=sum(A);"),RustSessionErrorV1::Allocation {..}));
        assert_eq!(success(run(&mut session,"B=sum(A);" )).value.as_slice(), &[16.;16]);
    }
    for checkpoint in [Checkpoint::Task,Checkpoint::AfterEvaluation,Checkpoint::BeforeCommit] {
        let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=ones(16);"));success(run(&mut session,"91"));
        let before=(session.context(),snapshot(&session),session.retained_bytes);
        session.hook=Some(Box::new(move|at,handle|if at==checkpoint{handle.request_stop();}));
        assert!(matches!(run(&mut session,"B=sum(A);"),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::UserStop,..}));
        session.hook=None;assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
        let request=session.prepare(Arc::from("sum(A)"),Some(Instant::now())).unwrap();
        assert!(matches!(request.execute(),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::Deadline,..}));
        assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
        let revision=session.revision;session.revision=u64::MAX;
        assert!(matches!(assert_error_unchanged(&mut session,"sum(A)"),RustSessionErrorV1::CounterExhausted));session.revision=revision;
        assert_eq!(success(run(&mut session,"sum(A)" )).value.as_slice(), &[16.;16]);
    }
    let mut session=RustSessionV1::new().unwrap();for i in 0..64 {success(run(&mut session,&format!("v{i}=ones(16);")));}
    assert!(matches!(assert_error_unchanged(&mut session,"extra=sum(v0);"),RustSessionErrorV1::WorkspaceLimit));
    success(run(&mut session,"v0=sum(v0);"));assert_eq!(session.retained_bytes,MAX_WORKSPACE_PAYLOAD_BYTES_V1-2048+128);
}


#[test]
fn extrema_default_session_geometry_ans_suppression_words_and_atomic_recovery() {
    for (operation,expected,scalar) in [("min",[1.,2.,3.],1.),("max",[4.,5.,6.],6.)] {
        let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=[1 2 3;4 5 6];"));let old=session.read("A").unwrap().unwrap();
        let result=success(run(&mut session,&format!("{operation}(A)")));assert_eq!(result.value.as_slice(),&expected);assert_eq!(result.display_name.as_deref(),Some("ans"));assert_eq!(result.after.workspace_revision,result.before.workspace_revision+1);
        let assigned=success(run(&mut session,&format!("B={operation}(A);")));assert!(assigned.display_name.is_none());assert_eq!(values(&session,"ans"),expected);
        success(run(&mut session,&format!("A(1,:)={operation}(A);")));assert_eq!(old.value.as_slice(),&[1.,4.,2.,5.,3.,6.]);assert_eq!(values(&session,"B"),expected);
        for source in [format!("B={operation}(missing);"),format!("B={operation}(A,missing);"),format!("B={operation}(:);"),format!("B={operation}(zeros(17));"),format!("A(1)={operation}([]);")] {
            assert_error_unchanged(&mut session,&source);assert_eq!(success(run(&mut session,&format!("{operation}(B)"))).value.as_slice(),&[scalar]);
        }
        assert_eq!(success(run(&mut session,&format!("{operation}([])"))).value.shape(),[0,0]);assert_eq!(success(run(&mut session,&format!("{operation}(zeros(0,3))"))).value.shape(),[0,3]);assert_eq!(success(run(&mut session,&format!("{operation}(zeros(3,0))"))).value.shape(),[1,0]);
        assert_eq!(success(run(&mut session,&format!("{operation}([-0 0])"))).value.as_slice()[0].to_bits(),0x8000000000000000);assert_eq!(success(run(&mut session,&format!("{operation}([0 -0])"))).value.as_slice()[0].to_bits(),0);
        let words=[0xfff8000000000042,0x7ff8000000000abc];session.bindings.push(Binding {name:"P".into(),value:SessionValueV1::try_from_col_major([1,2],&words.map(f64::from_bits)).unwrap()});session.retained_bytes+=16;
        let result=success(run(&mut session,&format!("{operation}(P)")));assert_eq!(result.value.as_slice()[0].to_bits(),words[0]);assert_eq!(session.read("ans").unwrap().unwrap().value.as_slice()[0].to_bits(),words[0]);assert_eq!(session.read("P").unwrap().unwrap().value.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),words);
        success(run(&mut session,&format!("{operation}=[8 9];")));assert_eq!(success(run(&mut session,&format!("{operation}(end)"))).value.as_slice(),&[9.]);
    }
}

#[test]
fn extrema_default_session_reservations_cancel_deadline_quota_counter_and_recovery() {
    for operation in ["min","max"] {
        for site in [ReservationSite::Name,ReservationSite::Slot,ReservationSite::Response] {
            let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=ones(16);"));success(run(&mut session,"91"));session.fail_reservation=Some(site);
            assert!(matches!(assert_error_unchanged(&mut session,&format!("B={operation}(A);")),RustSessionErrorV1::Allocation {..}));assert_eq!(success(run(&mut session,&format!("B={operation}(A);"))).value.as_slice(),&[1.;16]);
        }
        for checkpoint in [Checkpoint::Task,Checkpoint::AfterEvaluation,Checkpoint::BeforeCommit] {
            let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=ones(16);"));success(run(&mut session,"91"));let before=(session.context(),snapshot(&session),session.retained_bytes);
            session.hook=Some(Box::new(move|at,handle|if at==checkpoint{handle.request_stop();}));assert!(matches!(run(&mut session,&format!("B={operation}(A);")),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::UserStop,..}));session.hook=None;assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
            let request=session.prepare(Arc::from(format!("{operation}(A)")),Some(Instant::now())).unwrap();assert!(matches!(request.execute(),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::Deadline,..}));assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
            let revision=session.revision;session.revision=u64::MAX;assert!(matches!(assert_error_unchanged(&mut session,&format!("{operation}(A)")),RustSessionErrorV1::CounterExhausted));session.revision=revision;assert_eq!(success(run(&mut session,&format!("{operation}(A)"))).value.as_slice(),&[1.;16]);
        }
        let mut session=RustSessionV1::new().unwrap();for i in 0..64 {success(run(&mut session,&format!("v{i}=ones(16);")));}assert!(matches!(assert_error_unchanged(&mut session,&format!("extra={operation}(v0);")),RustSessionErrorV1::WorkspaceLimit));success(run(&mut session,&format!("v0={operation}(v0);")));assert_eq!(session.retained_bytes,MAX_WORKSPACE_PAYLOAD_BYTES_V1-2048+128);
    }
}


#[test]
fn exact_mean_prod_session_words_ans_revision_empty_write_and_atomic_recovery() {
    for (operation,expected,reduced) in [("mean",[2.5,3.5,4.5],3.5),("prod",[4.,10.,18.],720.)] {
        let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=[1 2 3;4 5 6];"));let old=session.read("A").unwrap().unwrap();
        let result=success(run(&mut session,&format!("{operation}(A)")));assert_eq!(result.value.as_slice(),&expected);assert_eq!(result.display_name.as_deref(),Some("ans"));assert_eq!(result.after.workspace_revision,result.before.workspace_revision+1);
        assert!(success(run(&mut session,&format!("B={operation}(A);"))).display_name.is_none());assert_eq!(values(&session,"ans"),expected);
        success(run(&mut session,&format!("A(1,:)={operation}(A);")));assert_eq!(old.value.as_slice(),&[1.,4.,2.,5.,3.,6.]);assert_eq!(values(&session,"B"),expected);
        for source in [format!("B={operation}(missing);"),format!("B={operation}(A,missing);"),format!("B={operation}(:);"),format!("B={operation}(zeros(17));"),"A(1)=min([]);".into()] {assert_error_unchanged(&mut session,&source);assert_eq!(success(run(&mut session,&format!("{operation}(B)"))).value.as_slice(),&[reduced]);}
        let empty=success(run(&mut session,&format!("{operation}([])")));assert_eq!(empty.value.shape(),[1,1]);assert_eq!(empty.value.as_slice()[0].to_bits(),if operation=="mean"{0x7ff8000000000000}else{0x3ff0000000000000});assert_eq!(success(run(&mut session,&format!("{operation}(zeros(3,0))"))).value.shape(),[1,0]);
        success(run(&mut session,"E=zeros(0,3);"));let noop=success(run(&mut session,&format!("E(:,1)={operation}([]);")));assert_eq!(noop.value.shape(),[0,3]);assert_eq!(noop.after.workspace_revision,noop.before.workspace_revision+1);
        let words=[0xfff8000000000042,0x7ff8000000000abc];session.bindings.push(Binding {name:"P".into(),value:SessionValueV1::try_from_col_major([1,2],&words.map(f64::from_bits)).unwrap()});session.retained_bytes+=16;
        assert_eq!(success(run(&mut session,&format!("{operation}(P)"))).value.as_slice()[0].to_bits(),words[0]);assert_eq!(session.read("P").unwrap().unwrap().value.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),words);
        success(run(&mut session,&format!("{operation}=[8 9];")));assert_eq!(success(run(&mut session,&format!("{operation}(end)"))).value.as_slice(),&[9.]);
    }
    let mut session=RustSessionV1::new().unwrap();assert_eq!(success(run(&mut session,"mean([9007199254740992 1 -9007199254740992])")).value.as_slice()[0].to_bits(),0x3fd5555555555555);assert_eq!(success(run(&mut session,"mean([1e308 1e308 -1e308 -1e308])")).value.as_slice(),&[0.]);
}

#[test]
fn exact_mean_prod_session_reservations_cancel_deadline_quota_counter_recovery() {
    for operation in ["mean","prod"] {
        for site in [ReservationSite::Name,ReservationSite::Slot,ReservationSite::Response] {
            let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=ones(16);"));success(run(&mut session,"91"));session.fail_reservation=Some(site);assert!(matches!(assert_error_unchanged(&mut session,&format!("B={operation}(A);")),RustSessionErrorV1::Allocation {..}));assert_eq!(success(run(&mut session,&format!("B={operation}(A);"))).value.as_slice(),&[1.;16]);
        }
        for checkpoint in [Checkpoint::Task,Checkpoint::AfterEvaluation,Checkpoint::BeforeCommit] {
            let mut session=RustSessionV1::new().unwrap();success(run(&mut session,"A=ones(16);"));success(run(&mut session,"91"));let before=(session.context(),snapshot(&session),session.retained_bytes);session.hook=Some(Box::new(move|at,handle|if at==checkpoint{handle.request_stop();}));assert!(matches!(run(&mut session,&format!("B={operation}(A);")),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::UserStop,..}));session.hook=None;assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
            let request=session.prepare(Arc::from(format!("{operation}(A)")),Some(Instant::now())).unwrap();assert!(matches!(request.execute(),RustEvalOutcomeV1::Cancelled {reason:RustCancelReasonV1::Deadline,..}));assert_eq!((session.context(),snapshot(&session),session.retained_bytes),before);
            let revision=session.revision;session.revision=u64::MAX;assert!(matches!(assert_error_unchanged(&mut session,&format!("{operation}(A)")),RustSessionErrorV1::CounterExhausted));session.revision=revision;assert_eq!(success(run(&mut session,&format!("{operation}(A)"))).value.as_slice(),&[1.;16]);
        }
        let mut session=RustSessionV1::new().unwrap();for i in 0..64 {success(run(&mut session,&format!("v{i}=ones(16);")));}assert!(matches!(assert_error_unchanged(&mut session,&format!("extra={operation}(v0);")),RustSessionErrorV1::WorkspaceLimit));success(run(&mut session,&format!("v0={operation}(v0);")));assert_eq!(session.retained_bytes,MAX_WORKSPACE_PAYLOAD_BYTES_V1-2048+128);
    }
}

#[test]
fn exact_sum_input_infinities_override_finite_intermediate_range_and_recover() {
    for (words,expected) in [([0x7fefffffffffffff,0x7fefffffffffffff,0xfff0000000000000],0xfff0000000000000),([0xffefffffffffffff,0xffefffffffffffff,0x7ff0000000000000],0x7ff0000000000000)] {
        let mut session=RustSessionV1::new().unwrap();session.bindings.push(Binding {name:"A".into(),value:SessionValueV1::try_from_col_major([1,3],&words.map(f64::from_bits)).unwrap()});session.retained_bytes+=24;
        let before=session.read("A").unwrap().unwrap();let result=success(run(&mut session,"B=sum(A);"));assert_eq!(result.value.as_slice()[0].to_bits(),expected);assert!(result.display_name.is_none());assert_eq!(result.after.workspace_revision,result.before.workspace_revision+1);
        assert_eq!(success(run(&mut session,"sum(A)")).value.as_slice()[0].to_bits(),expected);assert_eq!(values(&session,"ans")[0].to_bits(),expected);assert_error_unchanged(&mut session,"B=sum(missing);");
        assert_eq!(session.read("A").unwrap().unwrap().value.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),words);assert_eq!(before.value.as_slice().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),words);
        assert_eq!(success(run(&mut session,"sum([9007199254740992 1 -9007199254740992])")).value.as_slice(),&[1.]);
    }
}
