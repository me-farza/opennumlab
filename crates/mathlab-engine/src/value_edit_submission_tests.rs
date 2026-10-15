// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual-worker publication-boundary tests. Local mailbox failures must not be
//! described as possible commits. Only the final test alters a private WORKER
//! copy, after a genuine reset has already run, to lose its snapshot authority.

use super::{Engine, EngineConfig, Error, Result, WORKER, discover_octave, octave_string};
use crate::value_edit::{
    CAP_ATOMIC_RANGE_EDIT, EditError, HistoryRequest, LeafDescriptor, RangeEditReceipt,
    RangeEditRequest,
};
use crate::{
    BindingToken, NumericClass, NumericComponents, PageData, PageWindow, SourceStorage,
    ValuePageRequest, ValuePath, WorkspaceSnapshot,
};
use std::{fs, sync::atomic::Ordering};

fn session(worker: &str) -> Option<(Engine, tempfile::TempDir)> {
    let Some(executable) = discover_octave() else {
        assert_ne!(
            std::env::var("MATHLAB_REQUIRE_VALUE_EDIT_TESTS").as_deref(),
            Ok("1"),
            "required edit submission tests need an installed Octave"
        );
        eprintln!("SKIP edit submission: no installed Octave executable.");
        return None;
    };
    let directory = tempfile::Builder::new()
        .prefix("mathlab-edit-submission-")
        .tempdir()
        .unwrap();
    let engine = Engine::start_with_worker(
        EngineConfig {
            executable,
            working_directory: directory.path().to_owned(),
        },
        worker,
    )
    .unwrap();
    engine.require_capability(CAP_ATOMIC_RANGE_EDIT).unwrap();
    Some((engine, directory))
}

fn binding(snapshot: &WorkspaceSnapshot) -> BindingToken {
    snapshot
        .variables
        .iter()
        .find(|entry| entry.variable.name == "A")
        .unwrap()
        .token
        .clone()
}

fn edit(token: BindingToken, value: u64) -> RangeEditRequest {
    RangeEditRequest::from_components(
        ValuePath {
            root: token,
            steps: vec![],
        },
        PageWindow {
            row_start: 1.into(),
            column_start: 1.into(),
            rows: 1,
            columns: 1,
            higher_indices: vec![],
        },
        LeafDescriptor {
            class: NumericClass::Uint64,
            source_shape: vec![2.into(), 1.into()],
            source_storage: SourceStorage::Dense,
            complex: false,
        },
        &NumericComponents::Uint64(vec![value]),
        None,
    )
    .unwrap()
}

fn history(receipt: &RangeEditReceipt, redo: bool) -> HistoryRequest {
    let head = if redo {
        receipt.history.redo.as_ref()
    } else {
        receipt.history.undo.as_ref()
    }
    .unwrap();
    HistoryRequest {
        schema_version: 1,
        expected: receipt.token.clone(),
        history_epoch: receipt.history.epoch,
        transaction_id: head.transaction_id,
    }
}

fn prepare(engine: &mut Engine) -> (RangeEditRequest, HistoryRequest, HistoryRequest) {
    let initialized = engine.execute("A=uint64([1;2]); peer=uint64(19);").unwrap();
    assert!(initialized.error.is_none(), "{:?}", initialized.error);
    let token = binding(&engine.workspace_snapshot().unwrap());
    let first = engine.write_value_range_checked(&edit(token, 17)).unwrap();
    let second = engine
        .write_value_range_checked(&edit(first.token, 23))
        .unwrap();
    let undone = engine
        .undo_value_edit_checked(&history(&second, false))
        .unwrap();
    // Both heads are genuine and usable in the SAME current context: the first
    // write remains undoable and the second write is available for redo.
    let write = edit(undone.token.clone(), 29);
    let undo = history(&undone, false);
    let redo = history(&undone, true);
    write.validate_decode().unwrap();
    undo.validate().unwrap();
    redo.validate().unwrap();
    (write, undo, redo)
}

fn assert_known_protocol<T>(result: Result<T>) {
    match result {
        Err(Error::Protocol(message)) => {
            assert!(message.contains("transport is closed"), "{message}")
        }
        Err(other) => panic!("expected known pre-publication Protocol, got {other:?}"),
        Ok(_) => panic!("closed transport accepted a public operation"),
    }
}

fn assert_known_io<T>(result: Result<T>) {
    match result {
        Err(Error::Io(_)) => {}
        Err(other) => panic!("expected known pre-publication Io, got {other:?}"),
        Ok(_) => panic!("request.tmp directory unexpectedly accepted a request file"),
    }
}

fn assert_not_published(engine: &Engine, next_id: Option<u64>, previous_latch: bool) {
    if let Some(expected) = next_id {
        assert_eq!(
            engine.next_id, expected,
            "closed transport consumed a request ID"
        );
    }
    assert!(!engine.session.path().join("request.json").exists());
    assert!(!engine.active.load(Ordering::Acquire));
    assert_eq!(engine.has_value_edit_uncertainty(), previous_latch);
}

fn assert_values(engine: &mut Engine, snapshot: &WorkspaceSnapshot, first: u64) {
    let page = engine
        .read_value_page_checked(&ValuePageRequest {
            path: ValuePath {
                root: binding(snapshot),
                steps: vec![],
            },
            window: PageWindow {
                row_start: 1.into(),
                column_start: 1.into(),
                rows: 2,
                columns: 1,
                higher_indices: vec![],
            },
        })
        .unwrap();
    assert_eq!(
        page.data,
        PageData::Dense {
            real: NumericComponents::Uint64(vec![first, 2]),
            imag: None,
        }
    );
    assert_eq!(
        engine.workspace_snapshot().unwrap().context,
        snapshot.context
    );
}

#[test]
fn closed_transport_rejects_public_edits_and_reset_before_publication_without_new_uncertainty() {
    let Some((mut engine, _directory)) = session(WORKER) else {
        return;
    };
    let (write, undo, redo) = prepare(&mut engine);
    engine.terminate_transport().unwrap();
    assert!(!engine.is_transport_open());
    let next_id = engine.next_id;
    assert_known_protocol(engine.write_value_range_checked(&write));
    assert_not_published(&engine, Some(next_id), false);
    assert_known_protocol(engine.undo_value_edit_checked(&undo));
    assert_not_published(&engine, Some(next_id), false);
    assert_known_protocol(engine.redo_value_edit_checked(&redo));
    assert_not_published(&engine, Some(next_id), false);
    assert_known_protocol(engine.reset_value_edit_history());
    assert_not_published(&engine, Some(next_id), false);
    assert!(!engine.session.path().join("request.tmp").exists());
    // A failed local reset must also preserve a PREEXISTING latch. Submitted
    // loss/cancellation tests establish that latch; this models its prior state.
    engine.edit_history_uncertain = true;
    assert_known_protocol(engine.reset_value_edit_history());
    assert_not_published(&engine, Some(next_id), true);
}

#[test]
fn request_file_creation_failure_is_known_and_allows_recovery_without_reset() {
    let Some((mut engine, _directory)) = session(WORKER) else {
        return;
    };
    let (write, undo, redo) = prepare(&mut engine);
    let before = engine.workspace_snapshot().unwrap();
    let pid = engine.process_id();
    let temporary = engine.session.path().join("request.tmp");
    // A directory makes File::create fail deterministically, independently of
    // effective-user permissions. It cannot become the atomic request mailbox.
    fs::create_dir(&temporary).unwrap();
    assert_known_io(engine.write_value_range_checked(&write));
    assert_not_published(&engine, None, false);
    assert_known_io(engine.undo_value_edit_checked(&undo));
    assert_not_published(&engine, None, false);
    assert_known_io(engine.redo_value_edit_checked(&redo));
    assert_not_published(&engine, None, false);
    assert_known_io(engine.reset_value_edit_history());
    assert_not_published(&engine, None, false);
    assert!(temporary.is_dir());
    fs::remove_dir(&temporary).unwrap();
    let after = engine.workspace_snapshot().unwrap();
    assert_eq!(after.context, before.context);
    assert_values(&mut engine, &after, 17);
    // Use the SAME request authority. A local publication failure did not
    // invalidate it and does not require a reset or implicit retry.
    let accepted = engine.write_value_range_checked(&write).unwrap();
    assert_eq!(accepted.token.context.revision, before.context.revision + 1);
    let after_write = engine.workspace_snapshot().unwrap();
    assert_values(&mut engine, &after_write, 29);
    assert_eq!(engine.process_id(), pid);
}

#[test]
fn local_reset_failure_preserves_prior_uncertainty_until_a_valid_submitted_reset() {
    let Some((mut engine, _directory)) = session(WORKER) else {
        return;
    };
    let _ = prepare(&mut engine);
    let before = engine.workspace_snapshot().unwrap();
    engine.edit_history_uncertain = true;
    let temporary = engine.session.path().join("request.tmp");
    fs::create_dir(&temporary).unwrap();
    assert_known_io(engine.reset_value_edit_history());
    assert_not_published(&engine, None, true);
    fs::remove_dir(&temporary).unwrap();
    let reset = engine.reset_value_edit_history().unwrap();
    assert!(!engine.has_value_edit_uncertainty());
    assert_eq!(reset.context.revision, before.context.revision + 1);
    assert_values(&mut engine, &reset, 17);
    let accepted = engine
        .write_value_range_checked(&edit(binding(&reset), 31))
        .unwrap();
    assert_eq!(accepted.token.context.revision, reset.context.revision + 1);
}

#[test]
fn submitted_reset_with_invalid_snapshot_stays_uncertain_until_an_explicit_valid_reset() {
    let fault_directory = tempfile::tempdir().unwrap();
    let corrupt = fault_directory.path().join("corrupt-reset-response");
    fs::write(&corrupt, []).unwrap();
    let anchor =
        "            result = builtin('struct', 'context', context, 'variables', {bound});";
    assert_eq!(WORKER.matches(anchor).count(), 1);
    let worker = WORKER.replacen(
        anchor,
        &format!(
            "{anchor}\n            if (builtin('strcmp',request.command,'reset_value_edit_history') && builtin('exist',{},'file')); result.variables=builtin('uint64',17); endif;",
            octave_string(&corrupt).unwrap()
        ),
        1,
    );
    let Some((mut engine, _directory)) = session(&worker) else {
        return;
    };
    let (write, _undo, _redo) = prepare(&mut engine);
    let before = engine.workspace_snapshot().unwrap();
    let next_id = engine.next_id;
    assert!(matches!(
        engine.reset_value_edit_history(),
        Err(Error::ValueEdit(EditError::UncertainCommit(_)))
    ));
    assert!(engine.next_id > next_id, "reset was genuinely submitted");
    assert!(engine.has_value_edit_uncertainty());
    assert!(engine.is_transport_open());
    let after = engine.workspace_snapshot().unwrap();
    assert_eq!(after.context.revision, before.context.revision + 1);
    assert_values(&mut engine, &after, 17);
    let next_id = engine.next_id;
    assert!(matches!(
        engine.write_value_range_checked(&write),
        Err(Error::ValueEdit(EditError::UncertainCommit(_)))
    ));
    assert_eq!(
        engine.next_id, next_id,
        "ambiguous reset must not trigger a retry"
    );
    fs::remove_file(&corrupt).unwrap();
    let reset = engine.reset_value_edit_history().unwrap();
    assert_eq!(reset.context.revision, after.context.revision + 1);
    assert!(!engine.has_value_edit_uncertainty());
    assert_values(&mut engine, &reset, 17);
}
