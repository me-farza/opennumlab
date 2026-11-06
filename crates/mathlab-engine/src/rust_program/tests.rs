// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn module(s: &str) -> Arc<ProgramModuleV1> {
    Arc::new(parse_program_v1(81, Arc::from(s)).unwrap())
}
#[test]
fn allocation_preflights_preserve_prefix_and_recover() {
    let mut failures = 0;
    for fail in 1..128 {
        let mut s = ProgramSessionV1::new().unwrap();
        s.fail_allocation = Some(fail);
        let out = s
            .prepare(
                module("a=1\nb=f(a)\nc=3\nfunction y=f(x)\ny=x+1\nend\n"),
                Default::default(),
            )
            .unwrap()
            .execute();
        if out.status == ProgramStatusV1::Success {
            assert!(failures > 4);
            break;
        }
        failures += 1;
        assert_eq!(out.status, ProgramStatusV1::Resource);
        assert_eq!(out.error.unwrap().code, ProgramErrorCodeV1::Allocation);
        let expected = [("a", 1.), ("y", 2.), ("b", 2.), ("c", 3.)];
        assert!(out.events.len() <= 4);
        for (e, (name, value)) in out.events.iter().zip(expected) {
            assert_eq!(e.name.as_deref(), Some(name));
            assert_eq!(e.value.scalar().unwrap(), value);
        }
        let revision = out.final_context.revision;
        assert!(revision <= 3);
        for (name, rev, expected) in [("a", 1, 1.), ("b", 2, 2.), ("c", 3, 3.)] {
            let v = s.read_binding(name).unwrap();
            assert_eq!(v.is_some(), revision >= rev);
            if let Some(v) = v {
                assert_eq!(v.scalar().unwrap(), expected);
            }
        }
        assert!(s.read_binding("x").unwrap().is_none());
        assert!(s.read_binding("y").unwrap().is_none());
        s.fail_allocation = None;
        assert_eq!(
            s.prepare(module("recovered=1;"), Default::default())
                .unwrap()
                .execute()
                .status,
            ProgramStatusV1::Success
        );
    }
    assert!(failures > 4);
    let mut s = ProgramSessionV1::new().unwrap();
    s.fail_allocation = Some(0);
    assert!(matches!(
        s.prepare(module("x=1;"), Default::default()),
        Err(ProgramErrorCodeV1::Allocation)
    ));
}
#[test]
fn stop_before_execute_and_abandoned_handle() {
    let mut s = ProgramSessionV1::new().unwrap();
    let request = s.prepare(module("x=1;"), Default::default()).unwrap();
    let h = request.cancel_handle();
    assert_eq!(
        h.request_stop(),
        ProgramCancelResultV1::Accepted(ProgramCancelReasonV1::UserStop)
    );
    let out = request.execute();
    assert_eq!(
        out.status,
        ProgramStatusV1::Cancelled(ProgramCancelReasonV1::UserStop)
    );
    assert_eq!(
        h.cancel_for(h.key(), ProgramCancelReasonV1::Deadline),
        ProgramCancelResultV1::AlreadyAccepted(ProgramCancelReasonV1::UserStop)
    );
    let request = s.prepare(module("x=1;"), Default::default()).unwrap();
    let h = request.cancel_handle();
    h.request_stop();
    drop(request);
    assert_eq!(
        h.request_stop(),
        ProgramCancelResultV1::AlreadyAccepted(ProgramCancelReasonV1::UserStop)
    );
    let request = s.prepare(module("x=1;"), Default::default()).unwrap();
    let abandoned = request.cancel_handle();
    drop(request);
    assert_eq!(abandoned.request_stop(), ProgramCancelResultV1::TooLate);
}
#[test]
fn every_task_stop_injection_has_exact_prefix_and_recovery() {
    let mut baseline = ProgramSessionV1::new().unwrap();
    let src = "a=1; for k=1:3,a=a+k;end\n";
    let out = baseline
        .prepare(module(src), Default::default())
        .unwrap()
        .execute();
    assert_eq!(out.status, ProgramStatusV1::Success);
    let expected = [
        (None, None, 0),
        (Some(1.), None, 1),
        (Some(1.), Some(1.), 1),
        (Some(2.), Some(1.), 2),
        (Some(2.), Some(2.), 2),
        (Some(4.), Some(2.), 3),
        (Some(4.), Some(3.), 3),
        (Some(7.), Some(3.), 4),
    ];
    for stop in 1..=out.stats.transitions {
        let mut s = ProgramSessionV1::new().unwrap();
        let mut count = 0;
        let out = s
            .prepare(module(src), Default::default())
            .unwrap()
            .execute_controlled(|at, h| {
                if at == ProgramCheckpointV1::Task {
                    count += 1;
                    if count == stop {
                        h.request_stop();
                    }
                }
            });
        assert_eq!(
            out.status,
            ProgramStatusV1::Cancelled(ProgramCancelReasonV1::UserStop)
        );
        let (a, k, leaves) = expected[out.final_context.revision as usize];
        assert_eq!(s.read_binding("a").unwrap().map(|v| v.scalar().unwrap()), a);
        assert_eq!(s.read_binding("k").unwrap().map(|v| v.scalar().unwrap()), k);
        assert_eq!(out.completed_leaf_statement_count, leaves);
        assert!(out.events.is_empty());
        assert_eq!(
            s.prepare(module("ok=1;"), Default::default())
                .unwrap()
                .execute()
                .status,
            ProgramStatusV1::Success
        );
    }
}
#[test]
fn terminal_cancellation_races_and_first_reason() {
    for (cp, status) in [
        (
            ProgramCheckpointV1::BeforeTerminal,
            ProgramStatusV1::Cancelled(ProgramCancelReasonV1::Deadline),
        ),
        (ProgramCheckpointV1::AfterTerminal, ProgramStatusV1::Success),
    ] {
        let mut s = ProgramSessionV1::new().unwrap();
        let out = s
            .prepare(module("x=1;"), Default::default())
            .unwrap()
            .execute_controlled(|at, h| {
                if at == cp {
                    let result = h.cancel_for(h.key(), ProgramCancelReasonV1::Deadline);
                    assert_eq!(
                        result,
                        if cp == ProgramCheckpointV1::BeforeTerminal {
                            ProgramCancelResultV1::Accepted(ProgramCancelReasonV1::Deadline)
                        } else {
                            ProgramCancelResultV1::TooLate
                        }
                    );
                }
            });
        assert_eq!(out.status, status);
        assert_eq!(out.final_context.revision, 1);
    }
    let mut s = ProgramSessionV1::new().unwrap();
    let mut fired = false;
    let out = s
        .prepare(module("x=1;y=2;"), Default::default())
        .unwrap()
        .execute_controlled(|at, h| {
            if at == ProgramCheckpointV1::AfterBarrier && !fired {
                fired = true;
                assert_eq!(
                    h.cancel_for(h.key(), ProgramCancelReasonV1::Deadline),
                    ProgramCancelResultV1::Accepted(ProgramCancelReasonV1::Deadline)
                );
                assert_eq!(
                    h.request_stop(),
                    ProgramCancelResultV1::AlreadyAccepted(ProgramCancelReasonV1::Deadline)
                );
            }
        });
    assert_eq!(
        out.status,
        ProgramStatusV1::Cancelled(ProgramCancelReasonV1::Deadline)
    );
    assert_eq!(out.final_context.revision, 1);
}
#[test]
fn duplicate_preparation_faults_preserve_caller_prefix_and_unique_slots() {
    let source = "x=9;[x,x]=pair();\nfunction [a,b]=pair\nentered=1\na=11;b=22;\nend\n";
    let mut failed = 0;
    for fail in 1..128 {
        let mut s = ProgramSessionV1::new().unwrap();
        s.fail_allocation = Some(fail);
        let out = s
            .prepare(module(source), Default::default())
            .unwrap()
            .execute();
        if out.status == ProgramStatusV1::Success {
            assert_eq!(s.read_binding("x").unwrap().unwrap().scalar().unwrap(), 22.);
            assert_eq!(s.bindings().len(), 1);
            break;
        }
        failed += 1;
        assert_eq!(out.status, ProgramStatusV1::Resource);
        assert!(out.error.unwrap().code.is_allocation());
        let revision = out.final_context.revision;
        assert!(revision <= 1);
        let value = s.read_binding("x").unwrap();
        assert_eq!(value.is_some(), revision == 1);
        if let Some(value) = value {
            assert_eq!(value.scalar().unwrap(), 9.);
        }
        assert!(out.events.len() <= 1);
        for event in out.events {
            assert_eq!(event.name.as_deref(), Some("entered"));
            assert_eq!(event.value.scalar().unwrap(), 1.);
        }
        s.fail_allocation = None;
        assert_eq!(
            s.prepare(module("recovered=3;"), Default::default())
                .unwrap()
                .execute()
                .status,
            ProgramStatusV1::Success
        );
    }
    assert!(failed > 4);
}
#[test]
fn duplicate_publication_stop_has_only_whole_transaction_states() {
    for (point, published) in [
        (ProgramCheckpointV1::BeforePublish, false),
        (ProgramCheckpointV1::AfterBarrier, true),
        (ProgramCheckpointV1::AfterPublish, true),
    ] {
        let mut s = ProgramSessionV1::new().unwrap();
        let mut count = 0;
        let out = s
            .prepare(
                module("[x,x]=pair()\nfunction [a,b]=pair\na=11;b=22;\nend\n"),
                Default::default(),
            )
            .unwrap()
            .execute_controlled(|at, handle| {
                if at == point {
                    count += 1;
                    if count == 3 {
                        handle.request_stop();
                    }
                }
            });
        assert_eq!(
            out.status,
            ProgramStatusV1::Cancelled(ProgramCancelReasonV1::UserStop)
        );
        assert_eq!(out.final_context.revision, published as u64);
        assert_eq!(s.bindings().len(), published as usize);
        if published {
            assert_eq!(s.read_binding("x").unwrap().unwrap().scalar().unwrap(), 22.);
            assert_eq!(out.events.len(), 2);
            assert_eq!(out.events[0].value.scalar().unwrap(), 11.);
            assert_eq!(out.events[1].value.scalar().unwrap(), 22.);
            assert_eq!(out.events[0].sequence, out.events[1].sequence);
        } else {
            assert!(out.events.is_empty());
        }
    }
}

#[test]
fn empty_for_initialization_allocation_failures_preserve_atomic_prefix() {
    let source = "prefix=9\nfor k=zeros(3,0)\nwrong=1;\nend\nafter=11\n";
    let mut failures = 0;
    let mut saw_success = false;
    for fail in 1..128 {
        let mut s = ProgramSessionV1::new().unwrap();
        s.prepare(module("k=7;"), Default::default()).unwrap().execute();
        s.fail_allocation = Some(fail);
        let out = s.prepare(module(source), Default::default()).unwrap().execute();
        if out.status == ProgramStatusV1::Success {
            assert_eq!(out.final_context.revision, 4);
            saw_success = true;
            break;
        }
        failures += 1;
        assert_eq!(out.status, ProgramStatusV1::Resource);
        let error = out.error.unwrap();
        assert_eq!(error.code, ProgramErrorCodeV1::Allocation);
        assert!(error.location.span.end <= source.len());
        let revision = out.final_context.revision;
        assert!((1..=4).contains(&revision));
        let k = s.read_binding("k").unwrap().unwrap();
        if revision < 3 { assert_eq!(k.scalar().unwrap(), 7.); }
        else { assert_eq!(k.shape(), [0, 0]); }
        assert_eq!(s.read_binding("prefix").unwrap().is_some(), revision >= 2);
        assert_eq!(s.read_binding("after").unwrap().is_some(), revision >= 4);
        assert!(s.read_binding("wrong").unwrap().is_none());
        let expected: &[(&str, f64)] = if revision < 2 { &[] } else if revision < 4 { &[("prefix", 9.)] } else { &[("prefix", 9.), ("after", 11.)] };
        assert_eq!(out.events.len(), expected.len());
        for (event, &(name, value)) in out.events.iter().zip(expected) {
            assert_eq!(event.name.as_deref(), Some(name));
            assert_eq!(event.value.scalar().unwrap(), value);
        }
        s.fail_allocation = None;
        assert_eq!(s.prepare(module("recovered=1;"), Default::default()).unwrap().execute().status, ProgramStatusV1::Success);
    }
    assert!(failures > 4);
    assert!(saw_success);
}
#[test]
fn empty_for_initialization_stop_barrier_has_exact_prefix() {
    for (point, published) in [
        (ProgramCheckpointV1::BeforePublish, false),
        (ProgramCheckpointV1::AfterBarrier, true),
        (ProgramCheckpointV1::AfterPublish, true),
    ] {
        let mut s = ProgramSessionV1::new().unwrap();
        s.prepare(module("k=7;"), Default::default()).unwrap().execute();
        let mut count = 0;
        let out = s.prepare(module("prefix=9\nfor k=zeros(3,0)\nwrong=1;\nend\nafter=11\n"), Default::default()).unwrap().execute_controlled(|at, handle| {
            if at == point {
                count += 1;
                if count == 2 { handle.request_stop(); }
            }
        });
        assert_eq!(out.status, ProgramStatusV1::Cancelled(ProgramCancelReasonV1::UserStop));
        assert_eq!(out.final_context.revision, if published { 3 } else { 2 });
        let k = s.read_binding("k").unwrap().unwrap();
        if published { assert_eq!(k.shape(), [0, 0]); }
        else { assert_eq!(k.scalar().unwrap(), 7.); }
        assert!(s.read_binding("wrong").unwrap().is_none());
        assert!(s.read_binding("after").unwrap().is_none());
        assert_eq!(out.events.len(), 1);
        assert_eq!(out.events[0].name.as_deref(), Some("prefix"));
        assert_eq!(out.events[0].value.scalar().unwrap(), 9.);
        assert_eq!(s.prepare(module("recovered=1;"), Default::default()).unwrap().execute().status, ProgramStatusV1::Success);
    }
}
