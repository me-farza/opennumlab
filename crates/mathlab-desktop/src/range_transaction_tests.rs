// SPDX-License-Identifier: GPL-3.0-or-later
//! Controller state regressions. No numerical runtime or rendered UI.

use super::*;
use crate::lifecycle_tests::{harness, Harness};
use crate::range_paste::{parse_range_paste, PasteLimits, PasteTarget};
use crate::range_transactions::{
    TransactionCommand, TransactionEnvelope, TransactionFailure, TransactionOutcome,
    TransactionRefresh,
};
use mathlab_engine::value_edit::{EditOperation, HistoryHead, HistoryHeads, RangeEditReceipt};
use mathlab_engine::{
    BindingToken, BoundVariable, NumericClass, NumericComponents, PageData, PageSelection,
    SessionContext, SourceStorage, ValuePage, ValuePath, WorkspaceSnapshot,
};

fn metadata(name: &str) -> Variable {
    Variable {
        name: name.into(),
        class_name: "double".into(),
        dimensions: vec![2, 2],
        dimensions_known: true,
        bytes: 32,
        bytes_known: true,
        preview: String::new(),
    }
}

fn page(revision: u64) -> ValuePage {
    ValuePage {
        token: BindingToken {
            name: "A".into(),
            context: SessionContext {
                worker_id: "desktop-range-controller".into(),
                generation: 1,
                invocation: 1,
                revision,
            },
        },
        path: vec![],
        class: NumericClass::Double,
        source_shape: vec![2.into(), 2.into()],
        source_storage: SourceStorage::Dense,
        complex: false,
        selection: PageSelection {
            row_start: 1.into(),
            column_start: 1.into(),
            rows: 2,
            columns: 2,
            higher_indices: vec![],
        },
        data: PageData::Dense {
            real: NumericComponents::Double(vec![1., 2., 3., 4.]),
            imag: None,
        },
    }
}

fn seed(test: &mut Harness, revision: u64, request_id: u64) -> ValuePage {
    let page = page(revision);
    if test.app.inspected.is_none() {
        test.app.inspected = Some(VariableEditor::new(metadata("A")));
    }
    test.app.inspector_request = request_id;
    test.app
        .inspected
        .as_mut()
        .unwrap()
        .begin_read(request_id, None);
    test.events
        .send(Event::VariableRead {
            request_id,
            result: Ok(variable_editor::VariablePage::Typed {
                page: Box::new(page.clone()),
                metadata: Some(metadata("A")),
            }),
        })
        .unwrap();
    test.app.poll_events(&test.ctx);
    assert!(!test.app.inspected.as_ref().unwrap().stale);
    page
}

fn pasted(page: &ValuePage) -> range_paste::ParsedRangePaste {
    parse_range_paste(
        "99\t3\n2\t4",
        &PasteTarget {
            path: ValuePath {
                root: page.token.clone(),
                steps: page.path.clone(),
            },
            class: page.class,
            source_shape: page.source_shape.clone(),
            source_storage: page.source_storage,
            complex: page.complex,
            selection: page.selection.clone(),
        },
        PasteLimits::default(),
    )
    .unwrap()
}

fn begun(test: &mut Harness, page: &ValuePage) -> TransactionEnvelope {
    test.app.submit_range_paste(pasted(page)).unwrap();
    match test.requests.recv().unwrap() {
        Request::RangeTransaction(envelope) => *envelope,
        _ => panic!("expected actor transaction"),
    }
}

fn committed(envelope: &TransactionEnvelope) -> TransactionOutcome {
    let mut after = page(envelope.page.path.root.context.revision + 1);
    after.token.context = envelope.page.path.root.context.clone();
    after.token.context.revision += 1;
    after.data = PageData::Dense {
        real: NumericComponents::Double(vec![99., 2., 3., 4.]),
        imag: None,
    };
    TransactionOutcome::Applied {
        receipt: Box::new(RangeEditReceipt {
            schema_version: 1,
            operation: EditOperation::Write,
            transaction_id: 1.into(),
            token: after.token.clone(),
            history: HistoryHeads {
                epoch: 4_294_967_297.into(),
                undo: Some(HistoryHead {
                    transaction_id: 1.into(),
                    root_name: "A".into(),
                }),
                redo: None,
            },
            retained_patch_bytes: 64,
            retained_transactions: 1,
            evicted_undo_entries: 0,
        }),
        refresh: Ok(Box::new(TransactionRefresh {
            snapshot: WorkspaceSnapshot {
                context: after.token.context.clone(),
                variables: vec![BoundVariable {
                    token: after.token.clone(),
                    variable: metadata("A"),
                }],
            },
            page: after,
        })),
    }
}

fn finish(test: &mut Harness, envelope: TransactionEnvelope, outcome: TransactionOutcome) {
    test.events
        .send(Event::RangeTransactionFinished {
            envelope: Box::new(envelope),
            outcome: Box::new(outcome),
        })
        .unwrap();
    test.app.poll_events(&test.ctx);
}

#[test]
fn view_switch_settles_committed_receipt_without_applying_old_page_to_new_variable() {
    let mut test = harness();
    let before = seed(&mut test, 7, 1);
    let envelope = begun(&mut test, &before);
    test.app.open_variable(metadata("B")); // Read remains blocked while transaction owns the actor.
    assert!(test.requests.try_recv().is_err());
    let outcome = committed(&envelope);
    finish(&mut test, envelope, outcome);
    let editor = test.app.inspected.as_ref().unwrap();
    assert_eq!(editor.variable.name, "B");
    assert!(editor.slice.is_none() && editor.token.is_none());
    assert!(!test.app.busy);
    assert_eq!(
        test.app
            .range_edit_state()
            .history()
            .unwrap()
            .undo
            .as_ref()
            .unwrap()
            .root_name,
        "A"
    );
    assert!(test.app.range_edit_state().accepted_page().is_none());
    assert!(
        test.requests.try_recv().is_err(),
        "no automatic read of the superseded page"
    );
}

#[test]
fn changed_view_draft_is_released_but_never_replaced_by_old_transaction_page() {
    let mut test = harness();
    let before = seed(&mut test, 7, 1);
    let envelope = begun(&mut test, &before);
    test.app.inspected.as_mut().unwrap().row_start = 2;
    let outcome = committed(&envelope);
    finish(&mut test, envelope, outcome);
    let editor = test.app.inspected.as_ref().unwrap();
    assert_eq!(editor.row_start, 2);
    assert!(!editor.pending && editor.stale && editor.token.is_none());
    assert_eq!(editor.slice.as_ref().unwrap().cells[0][0], "1");
    assert!(
        test.app.range_edit_state().history().is_some(),
        "the numerical receipt remains settled"
    );
}

#[test]
fn changed_selector_draft_cannot_submit_old_page_paste_history_or_reset() {
    let mut test = harness();
    let before = seed(&mut test, 7, 1);
    let envelope = begun(&mut test, &before);
    let outcome = committed(&envelope);
    finish(&mut test, envelope, outcome);
    let accepted = test
        .app
        .inspected
        .as_ref()
        .unwrap()
        .typed_page
        .as_ref()
        .unwrap()
        .clone();
    test.app.inspected.as_mut().unwrap().row_start = 2;
    assert!(test.app.submit_range_paste(pasted(&accepted)).is_err());
    assert!(test.app.undo_range_edit().is_err());
    assert!(test.app.reset_range_edit_history().is_err());
    assert!(!test.app.range_edit_state().pending());
    assert!(
        test.requests.try_recv().is_err(),
        "draft changes require an explicit accepted page read"
    );
}

#[test]
fn closed_actor_channel_releases_pending_and_reports_known_failed_send_offline() {
    let mut test = harness();
    let before = seed(&mut test, 7, 1);
    drop(test.requests);
    let error = test.app.submit_range_paste(pasted(&before)).unwrap_err();
    assert!(error.contains("before accepting"));
    assert!(!test.app.range_edit_state().pending());
    assert!(test.app.range_edit_state().history().is_none());
    assert!(test.app.range_edit_state().accepted_page().is_none());
    assert!(test.app.range_edit_state().connection_lost());
    assert!(test.app.range_edit_state().writes_blocked());
    assert!(test
        .app
        .range_edit_state()
        .quarantine()
        .is_some_and(|reason| reason.contains("connection closed")));
    assert!(!test.app.busy && !test.app.connected);
    assert!(!test.app.inspected.as_ref().unwrap().pending);
}

#[test]
fn uncertainty_survives_explicit_read_and_does_not_dispatch_another_mutation() {
    let mut test = harness();
    let before = seed(&mut test, 7, 1);
    let envelope = begun(&mut test, &before);
    finish(
        &mut test,
        envelope,
        TransactionOutcome::Failed(TransactionFailure {
            message: "unknown commit".into(),
            uncertain: true,
            disconnected: false,
        }),
    );
    test.app.read_variable();
    let read_id = match test.requests.recv().unwrap() {
        Request::ReadVariable {
            request_id,
            token: None,
            ..
        } => request_id,
        _ => panic!("explicit fresh read must remain usable"),
    };
    let fresh = seed(&mut test, 9, read_id);
    assert!(test.app.range_edit_state().writes_blocked());
    assert!(test.app.submit_range_paste(pasted(&fresh)).is_err());
    assert!(test.app.undo_range_edit().is_err());
    assert!(test.requests.try_recv().is_err());
}

#[test]
fn known_connected_rejection_requires_explicit_read_before_preserved_undo_or_redo() {
    for operation in [EditOperation::Undo, EditOperation::Redo] {
        let mut test = harness();
        let before = seed(&mut test, 7, 1);
        let write = begun(&mut test, &before);
        let outcome = committed(&write);
        finish(&mut test, write, outcome);
        if operation == EditOperation::Redo {
            test.app.undo_range_edit().unwrap();
            let undo = match test.requests.recv().unwrap() {
                Request::RangeTransaction(envelope)
                    if matches!(&envelope.command, TransactionCommand::Undo(_)) =>
                {
                    *envelope
                }
                _ => panic!("expected explicit undo"),
            };
            let mut outcome = committed(&undo);
            if let TransactionOutcome::Applied {
                receipt,
                refresh: Ok(refresh),
            } = &mut outcome
            {
                receipt.operation = EditOperation::Undo;
                receipt.history.redo = receipt.history.undo.take();
                refresh.page.data = PageData::Dense {
                    real: NumericComponents::Double(vec![1., 2., 3., 4.]),
                    imag: None,
                };
            }
            finish(&mut test, undo, outcome);
        }
        let current = test
            .app
            .inspected
            .as_ref()
            .unwrap()
            .typed_page
            .as_ref()
            .unwrap()
            .clone();
        let heads = test.app.range_edit_state().history().unwrap().clone();
        let rejected = begun(&mut test, &current);
        finish(
            &mut test,
            rejected,
            TransactionOutcome::Failed(TransactionFailure::rejected(
                "request.tmp creation failed before publication",
            )),
        );
        assert!(test.app.connected && !test.app.busy);
        assert!(!test.app.range_edit_state().pending());
        assert!(test.app.range_edit_state().quarantine().is_none());
        assert!(test.app.range_edit_state().accepted_page().is_none());
        assert_eq!(test.app.range_edit_state().history(), Some(&heads));
        let editor = test.app.inspected.as_ref().unwrap();
        assert!(editor.stale && editor.token.is_none() && !editor.pending);
        assert!(test.requests.try_recv().is_err(), "no automatic retry/read");
        assert!(test.app.submit_range_paste(pasted(&current)).is_err());
        assert!(test.app.undo_range_edit().is_err());
        assert!(test.app.redo_range_edit().is_err());
        assert!(test.requests.try_recv().is_err());

        test.app.read_variable();
        let request_id = match test.requests.recv().unwrap() {
            Request::ReadVariable {
                request_id,
                token: None,
                ..
            } => request_id,
            _ => panic!("rejected transaction must require an explicit fresh read"),
        };
        test.events
            .send(Event::VariableRead {
                request_id,
                result: Ok(variable_editor::VariablePage::Typed {
                    page: Box::new(current.clone()),
                    metadata: Some(metadata("A")),
                }),
            })
            .unwrap();
        test.app.poll_events(&test.ctx);
        assert_eq!(test.app.range_edit_state().history(), Some(&heads));
        assert!(!test.app.inspected.as_ref().unwrap().stale);
        assert!(
            test.requests.try_recv().is_err(),
            "accepting a fresh page must not replay the rejected paste"
        );
        match operation {
            EditOperation::Undo => test.app.undo_range_edit().unwrap(),
            EditOperation::Redo => test.app.redo_range_edit().unwrap(),
            EditOperation::Write => unreachable!(),
        }
        let recovered = match test.requests.recv().unwrap() {
            Request::RangeTransaction(envelope) => *envelope,
            _ => panic!("expected explicitly requested history operation"),
        };
        let (request, head) = match &recovered.command {
            TransactionCommand::Undo(request) if operation == EditOperation::Undo => {
                (request, heads.undo.as_ref().unwrap())
            }
            TransactionCommand::Redo(request) if operation == EditOperation::Redo => {
                (request, heads.redo.as_ref().unwrap())
            }
            _ => panic!("wrong recovered history operation"),
        };
        assert_eq!(request.expected, current.token);
        assert_eq!(request.history_epoch, heads.epoch);
        assert_eq!(request.transaction_id, head.transaction_id);
        assert!(test.requests.try_recv().is_err());
    }
}

#[test]
fn ordinary_execution_retires_successful_transaction_heads_and_page_token() {
    let mut test = harness();
    let before = seed(&mut test, 7, 1);
    let envelope = begun(&mut test, &before);
    let outcome = committed(&envelope);
    finish(&mut test, envelope, outcome);
    assert!(test.app.range_edit_state().history().is_some());
    assert_eq!(
        test.app
            .inspected
            .as_ref()
            .unwrap()
            .slice
            .as_ref()
            .unwrap()
            .cells[0][0],
        "99"
    );
    test.app
        .execute("A=1;".into(), "ordinary evaluation".into());
    assert!(matches!(test.requests.recv().unwrap(), Request::Execute(_)));
    assert!(test.app.range_edit_state().history().is_none());
    assert!(test.app.inspected.as_ref().unwrap().token.is_none());
}

#[test]
fn stale_preview_reply_does_not_retire_newer_history_or_current_accepted_page() {
    let mut test = harness();
    let before = seed(&mut test, 7, 1);
    let envelope = begun(&mut test, &before);
    let stale_id = envelope.page.view_id;
    let outcome = committed(&envelope);
    finish(&mut test, envelope, outcome);
    let history = test.app.range_edit_state().history().cloned();
    let accepted = test.app.range_edit_state().accepted_page().cloned();
    let slice = mathlab_engine::VariableSlice {
        name: "earlier_selection".into(),
        class_name: "cell".into(),
        dimensions: vec![1, 1],
        row_start: 1,
        column_start: 1,
        rows: 1,
        columns: 1,
        cells: vec![vec!["'stale preview'".into()]],
        editable: false,
    };
    test.events
        .send(Event::VariableRead {
            request_id: stale_id,
            result: Ok(variable_editor::VariablePage::Preview(slice)),
        })
        .unwrap();
    test.app.poll_events(&test.ctx);
    assert_eq!(test.app.range_edit_state().history(), history.as_ref());
    assert_eq!(
        test.app.range_edit_state().accepted_page(),
        accepted.as_ref()
    );
    assert_eq!(
        test.app
            .inspected
            .as_ref()
            .unwrap()
            .slice
            .as_ref()
            .unwrap()
            .cells[0][0],
        "99"
    );
}

#[test]
fn disconnect_before_nominally_successful_receipt_reports_commit_without_reviving_view() {
    let mut test = harness();
    let before = seed(&mut test, 7, 1);
    let envelope = begun(&mut test, &before);
    let pending_id = envelope.request_id;
    test.events
        .send(Event::Disconnected(
            "synthetic console EOF before actor completion".into(),
        ))
        .unwrap();
    test.app.poll_events(&test.ctx);
    assert!(!test.app.connected && !test.app.busy);
    assert!(
        test.app.range_edit_state().pending(),
        "the exact envelope remains available for authoritative settlement"
    );
    let outcome = committed(&envelope); // Deliberately carries an apparently valid successful refresh.
    finish(&mut test, envelope, outcome);
    assert!(!test.app.connected && !test.app.busy);
    assert!(!test.app.range_edit_state().pending());
    assert!(test.app.range_edit_state().connection_lost());
    assert!(test.app.range_edit_state().history().is_none());
    assert!(test.app.range_edit_state().accepted_page().is_none());
    let editor = test.app.inspected.as_ref().unwrap();
    assert!(!editor.pending && editor.stale && editor.token.is_none());
    assert_eq!(
        editor.slice.as_ref().unwrap().cells[0][0],
        "1",
        "offline refresh must not replace visible values"
    );
    assert!(
        test.app
            .console
            .iter()
            .any(|entry| entry.text.contains("The edit committed")),
        "the valid receipt is not forgotten as an unknown commit"
    );
    let status = test.app.status.clone();
    test.events
        .send(Event::VariableRead {
            request_id: pending_id,
            result: Ok(variable_editor::VariablePage::Typed {
                page: Box::new(page(8)),
                metadata: Some(metadata("A")),
            }),
        })
        .unwrap();
    test.app.poll_events(&test.ctx);
    test.app.read_variable();
    assert!(test.app.submit_range_paste(pasted(&page(8))).is_err());
    assert!(test.app.reset_range_edit_history().is_err());
    assert_eq!(
        test.app.status, status,
        "late reads cannot hide the latest offline commit report"
    );
    assert!(test.requests.try_recv().is_err());
}
