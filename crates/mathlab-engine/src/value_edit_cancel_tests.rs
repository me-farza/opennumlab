// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual-worker edit cancellation tests, compiled only for Unix test builds.
//! Only the test's private WORKER copy receives builtin-only file gates. No
//! callback is installed and no gate is added to the production worker.

use super::{
    CancellationHandle, Engine, EngineConfig, Error, Result, WORKER, discover_octave, octave_string,
};
use crate::value_edit::{
    CAP_ATOMIC_RANGE_EDIT, EditError, HistoryRequest, LeafDescriptor, RangeEditReceipt,
    RangeEditRequest,
};
use crate::{
    BindingToken, NumericClass, NumericComponents, PageData, PageWindow, SourceStorage,
    ValuePageRequest, ValuePath, WorkspaceSnapshot,
};
use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

const PRECOMMIT: &str = "## MATHLAB_VALUE_EDIT_PRECOMMIT_BOUNDARY";
const POSTASSIGN: &str = "## MATHLAB_VALUE_EDIT_POSTASSIGN_BOUNDARY";
const DISPATCH: &str = "        switch (request.command)";

#[derive(Clone, Copy, Debug)]
enum Boundary {
    BeforeRevision,
    AfterAssignment,
}

#[derive(Clone, Copy, Debug)]
enum Operation {
    Write,
    Undo,
    Redo,
}
impl Operation {
    fn command(self) -> &'static str {
        match self {
            Self::Write => "write_value_range_checked",
            Self::Undo => "undo_value_edit_checked",
            Self::Redo => "redo_value_edit_checked",
        }
    }
}

/// Dropping on a failed assertion releases any file wait and kills only this
/// test-owned session, including a request thread whose watchdog expired.
struct Gate {
    monitor: PathBuf,
    attempts: PathBuf,
    reached: PathBuf,
    release: PathBuf,
    cancellation: CancellationHandle,
    _directory: TempDir,
}
impl Drop for Gate {
    fn drop(&mut self) {
        let _ = fs::write(&self.release, []);
        let _ = self.cancellation.terminate();
    }
}

fn replace_once(source: String, anchor: &str, replacement: &str) -> String {
    assert_eq!(
        source.matches(anchor).count(),
        1,
        "range-edit injection anchor moved or became ambiguous: {anchor}"
    );
    source.replacen(anchor, replacement, 1)
}

fn with_gate(boundary: Boundary, operation: Operation) -> Option<(Engine, Gate)> {
    let Some(executable) = discover_octave() else {
        assert_ne!(
            std::env::var("MATHLAB_REQUIRE_VALUE_EDIT_TESTS").as_deref(),
            Ok("1"),
            "required value-edit boundary tests need an installed Octave"
        );
        eprintln!("SKIP value-edit boundary test: no installed Octave executable.");
        return None;
    };
    let directory = tempfile::Builder::new()
        .prefix("mathlab-edit-cancel-")
        .tempdir()
        .unwrap();
    let monitor = directory.path().join("monitor-dispatch");
    let attempts = directory.path().join("dispatch-attempts");
    let reached = directory.path().join("boundary-reached");
    let release = directory.path().join("boundary-release");
    let condition = format!(
        "builtin('exist',{},'file') && builtin('strcmp',request.command,'{}')",
        octave_string(&monitor).unwrap(),
        operation.command()
    );
    // Observe every edit dispatch, including a rejected stale-token retry or
    // a supposedly client-blocked operation while uncertainty is latched.
    // Monitoring starts only after all setup writes/undo have completed.
    let observe = format!(
        "builtin('exist',{},'file') && builtin('any',builtin('strcmp',request.command,{{'write_value_range_checked','undo_value_edit_checked','redo_value_edit_checked'}}))",
        octave_string(&monitor).unwrap()
    );
    let dispatch_observer = format!(
        "        if ({observe}); edit_test_file=builtin('fopen',{},'a'); builtin('fprintf',edit_test_file,'attempt\\n'); builtin('fclose',edit_test_file); endif\n{DISPATCH}",
        octave_string(&attempts).unwrap()
    );
    let mut worker = replace_once(WORKER.to_owned(), DISPATCH, &dispatch_observer);
    let gate = format!(
        "if ({condition}); edit_test_file=builtin('fopen',{},'w'); builtin('fclose',edit_test_file); while (! builtin('exist',{},'file')); builtin('pause',0.005); endwhile; endif",
        octave_string(&reached).unwrap(),
        octave_string(&release).unwrap()
    );
    let anchor = match boundary {
        Boundary::BeforeRevision => PRECOMMIT,
        Boundary::AfterAssignment => POSTASSIGN,
    };
    worker = replace_once(worker, anchor, &format!("{anchor}\n            {gate}"));
    let engine = Engine::start_with_worker(
        EngineConfig {
            executable,
            working_directory: directory.path().to_owned(),
        },
        &worker,
    )
    .expect("installed worker must start for cancellation injection");
    engine.require_capability(CAP_ATOMIC_RANGE_EDIT).unwrap();
    let gate = Gate {
        monitor,
        attempts,
        reached,
        release,
        cancellation: engine.cancellation_handle(),
        _directory: directory,
    };
    Some((engine, gate))
}

fn execute(engine: &mut Engine, source: &str) {
    let result = engine.execute(source).unwrap();
    assert!(
        result.error.is_none(),
        "setup/request failed: {:?}",
        result.error
    );
}

fn binding(snapshot: &WorkspaceSnapshot, name: &str) -> BindingToken {
    snapshot
        .variables
        .iter()
        .find(|entry| entry.variable.name == name)
        .unwrap_or_else(|| panic!("missing test binding {name}"))
        .token
        .clone()
}

fn edit(
    token: BindingToken,
    rows: u32,
    columns: u32,
    row_start: u64,
    column_start: u64,
    values: &[u64],
) -> RangeEditRequest {
    RangeEditRequest::from_components(
        ValuePath {
            root: token,
            steps: vec![],
        },
        PageWindow {
            row_start: row_start.into(),
            column_start: column_start.into(),
            rows,
            columns,
            higher_indices: vec![],
        },
        LeafDescriptor {
            class: NumericClass::Uint64,
            source_shape: vec![3.into(), 4.into()],
            source_storage: SourceStorage::Dense,
            complex: false,
        },
        &NumericComponents::Uint64(values.to_vec()),
        None,
    )
    .unwrap()
}

fn history_request(receipt: &RangeEditReceipt, redo: bool) -> HistoryRequest {
    let head = if redo {
        &receipt.history.redo
    } else {
        &receipt.history.undo
    };
    let head = head
        .as_ref()
        .expect("setup must provide the targeted history head");
    HistoryRequest {
        schema_version: 1,
        expected: receipt.token.clone(),
        history_epoch: receipt.history.epoch,
        transaction_id: head.transaction_id,
    }
}

enum Call {
    Write(RangeEditRequest),
    Undo(HistoryRequest),
    Redo(HistoryRequest),
}
impl Call {
    fn expected(&self) -> &BindingToken {
        match self {
            Self::Write(request) => &request.path.root,
            Self::Undo(request) | Self::Redo(request) => &request.expected,
        }
    }
    fn run(self, engine: &mut Engine) -> Result<RangeEditReceipt> {
        match self {
            Self::Write(request) => engine.write_value_range_checked(&request),
            Self::Undo(request) => engine.undo_value_edit_checked(&request),
            Self::Redo(request) => engine.redo_value_edit_checked(&request),
        }
    }
}

struct Scenario {
    call: Call,
    before: Vec<u64>,
    after: Vec<u64>,
    history_id: crate::DecimalU64,
    history_epoch: crate::DecimalU64,
}
fn scenario(engine: &mut Engine, operation: Operation) -> Scenario {
    execute(
        engine,
        "A=uint64(reshape(1:12,3,4)); untouched_sentinel=uint64(55);",
    );
    let first = binding(&engine.workspace_snapshot().unwrap(), "A");
    let original = (1..=12).collect::<Vec<u64>>();
    // Independent column-major oracle: rows2:3, columns2:3 replace offsets
    // 4,5,7,8 (zero-based), leaving every other element untouched.
    let mut changed = original.clone();
    for (offset, value) in [(4, 101), (5, 102), (7, 103), (8, 104)] {
        changed[offset] = value;
    }
    match operation {
        Operation::Write => {
            // A prior accepted transaction makes invalidation observable even
            // when the tested write is interrupted before changing any value.
            let seed = engine
                .write_value_range_checked(&edit(first, 1, 1, 1, 1, &[202]))
                .unwrap();
            let id = seed.history.undo.as_ref().unwrap().transaction_id;
            let mut before = original;
            before[0] = 202;
            let mut after = changed;
            after[0] = 202;
            Scenario {
                call: Call::Write(edit(seed.token, 2, 2, 2, 2, &[101, 102, 103, 104])),
                before,
                after,
                history_id: id,
                history_epoch: seed.history.epoch,
            }
        }
        Operation::Undo | Operation::Redo => {
            let accepted = engine
                .write_value_range_checked(&edit(first, 2, 2, 2, 2, &[101, 102, 103, 104]))
                .unwrap();
            let id = accepted.history.undo.as_ref().unwrap().transaction_id;
            let epoch = accepted.history.epoch;
            if matches!(operation, Operation::Undo) {
                Scenario {
                    call: Call::Undo(history_request(&accepted, false)),
                    before: changed,
                    after: original,
                    history_id: id,
                    history_epoch: epoch,
                }
            } else {
                let undone = engine
                    .undo_value_edit_checked(&history_request(&accepted, false))
                    .unwrap();
                Scenario {
                    call: Call::Redo(history_request(&undone, true)),
                    before: original,
                    after: changed,
                    history_id: id,
                    history_epoch: epoch,
                }
            }
        }
    }
}

fn wait_reached(gate: &Gate) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !gate.reached.exists() {
        assert!(
            Instant::now() < deadline,
            "range edit did not reach its exact injected boundary"
        );
        thread::sleep(Duration::from_millis(5));
    }
}
fn join_request(task: thread::JoinHandle<(Engine, Result<RangeEditReceipt>)>) -> (Engine, Error) {
    let deadline = Instant::now() + Duration::from_secs(25);
    while !task.is_finished() {
        assert!(
            Instant::now() < deadline,
            "SIGINT did not settle the range edit promptly"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let (engine, result) = task.join().expect("range-edit request thread panicked");
    let error = result.expect_err("an interrupted edit must not publish an accepted receipt");
    assert!(
        matches!(&error, Error::ValueEdit(EditError::UncertainCommit(_))),
        "expected explicit uncertain commit, got {error:?}"
    );
    (engine, error)
}
fn assert_stale<T>(result: Result<T>) {
    match result {
        Err(Error::Worker(error)) => assert_eq!(error.identifier, "MathLab:StaleBinding"),
        Err(other) => panic!("expected stale binding, got {other:?}"),
        Ok(_) => panic!("pre-interruption binding was accepted"),
    }
}
fn assert_uncertain<T>(result: Result<T>) {
    match result {
        Err(Error::ValueEdit(EditError::UncertainCommit(_))) => {}
        Err(other) => panic!("expected the explicit local uncertainty latch, got {other:?}"),
        Ok(_) => panic!("an edit was accepted before explicit history reset"),
    }
}
fn assert_history_invalid<T>(result: Result<T>) {
    match result {
        Err(Error::Worker(error)) => assert!(
            matches!(
                error.identifier.as_str(),
                "MathLab:ValueEditHistoryConflict" | "MathLab:StaleBinding"
            ),
            "unexpected history rejection: {error:?}"
        ),
        Err(other) => panic!("expected a nonmutating history rejection, got {other:?}"),
        Ok(_) => panic!("pre-interruption history survived recovery"),
    }
}

fn cancellation_case(boundary: Boundary, operation: Operation) {
    let Some((mut engine, gate)) = with_gate(boundary, operation) else {
        return;
    };
    let scenario = scenario(&mut engine, operation);
    let old = scenario.call.expected().clone();
    let old_context = old.context.clone();
    let pid = engine.process_id();
    fs::write(&gate.monitor, []).unwrap();
    let task = thread::spawn(move || {
        let result = scenario.call.run(&mut engine);
        (engine, result)
    });
    wait_reached(&gate);
    gate.cancellation
        .interrupt()
        .expect("send actual SIGINT to the test worker");
    let (mut engine, _) = join_request(task);
    fs::write(&gate.release, []).unwrap();
    assert_eq!(
        engine.process_id(),
        pid,
        "soft Stop replaced the worker process"
    );
    assert!(
        engine.invocation > old_context.invocation,
        "SIGINT must settle through a new recovery invocation"
    );

    let fresh = engine.workspace_snapshot().unwrap();
    assert_eq!(fresh.context.worker_id, old_context.worker_id);
    assert_eq!(fresh.context.generation, old_context.generation);
    assert!(fresh.context.invocation > old_context.invocation);
    let current = binding(&fresh, "A");
    let mut page = ValuePageRequest {
        path: ValuePath {
            root: current.clone(),
            steps: vec![],
        },
        window: PageWindow {
            row_start: 1.into(),
            column_start: 1.into(),
            rows: 3,
            columns: 4,
            higher_indices: vec![],
        },
    };
    let actual = engine.read_value_page_checked(&page).unwrap();
    assert_eq!(actual.class, NumericClass::Uint64);
    assert_eq!(actual.source_storage, SourceStorage::Dense);
    assert!(!actual.complex);
    assert_eq!(actual.source_shape, vec![3.into(), 4.into()]);
    let PageData::Dense {
        real: NumericComponents::Uint64(values),
        imag: None,
    } = actual.data
    else {
        panic!("the full root changed class/storage/complex state")
    };
    let wanted = match boundary {
        Boundary::BeforeRevision => scenario.before,
        Boundary::AfterAssignment => scenario.after,
    };
    assert_eq!(
        values, wanted,
        "a canceled range left a partially replaced root"
    );
    page.path.root = old.clone();
    assert_stale(engine.read_value_page_checked(&page));

    // Fresh pure reads work, but no edit/history operation may dispatch while
    // the client cannot distinguish an uncommitted/committed lost receipt.
    let old_history = HistoryRequest {
        schema_version: 1,
        expected: old,
        history_epoch: scenario.history_epoch,
        transaction_id: scenario.history_id,
    };
    let fresh_history = HistoryRequest {
        expected: current.clone(),
        ..old_history.clone()
    };
    assert_uncertain(engine.write_value_range_checked(&edit(current, 1, 1, 1, 1, &[303])));
    assert_uncertain(engine.undo_value_edit_checked(&fresh_history));
    assert_uncertain(engine.redo_value_edit_checked(&fresh_history));
    assert_eq!(
        engine.workspace_snapshot().unwrap().context,
        fresh.context,
        "locally blocked edits advanced the settled binding context"
    );
    fs::remove_file(&gate.monitor).unwrap();
    assert_eq!(
        fs::read_to_string(&gate.attempts).unwrap().lines().count(),
        1,
        "an uncertain numerical operation was retried or a latched edit dispatched"
    );

    // Only an explicit reset clears the latch. It changes context/history, not
    // numerical values, and must not restart the preserved worker process.
    let reset = engine.reset_value_edit_history().unwrap();
    assert_eq!(reset.context.worker_id, fresh.context.worker_id);
    assert_eq!(reset.context.generation, fresh.context.generation);
    assert_eq!(reset.context.invocation, fresh.context.invocation);
    assert_eq!(reset.context.revision, fresh.context.revision + 1);
    let reset_token = binding(&reset, "A");
    page.path.root = reset_token.clone();
    let unchanged = engine.read_value_page_checked(&page).unwrap();
    assert!(
        matches!(unchanged.data, PageData::Dense { real: NumericComponents::Uint64(values), imag: None } if values == wanted)
    );

    // Old tokens remain revoked. Substituting the reset's fresh binding must
    // not revive either stack from the interrupted invocation. Probe without
    // arbitrary evaluation, which would itself clear history and hide a bug.
    assert_history_invalid(engine.undo_value_edit_checked(&old_history));
    let reset_history = HistoryRequest {
        expected: reset_token.clone(),
        ..old_history
    };
    assert_history_invalid(engine.undo_value_edit_checked(&reset_history));
    assert_history_invalid(engine.redo_value_edit_checked(&reset_history));
    assert_eq!(engine.workspace_snapshot().unwrap().context, reset.context);
    let accepted = engine
        .write_value_range_checked(&edit(reset_token, 1, 1, 1, 1, &[303]))
        .unwrap();
    page.path.root = accepted.token;
    let final_page = engine.read_value_page_checked(&page).unwrap();
    let mut after_fresh_edit = wanted;
    after_fresh_edit[0] = 303;
    assert!(
        matches!(final_page.data, PageData::Dense { real: NumericComponents::Uint64(values), imag: None } if values == after_fresh_edit)
    );
    execute(
        &mut engine,
        "assert(untouched_sentinel==uint64(55)); usable_after_edit_stop=71;",
    );
    assert_eq!(engine.process_id(), pid);
}

#[test]
fn sigint_before_write_revision_keeps_old_root_and_revokes_history() {
    cancellation_case(Boundary::BeforeRevision, Operation::Write);
}
#[test]
fn sigint_after_write_assignment_keeps_new_root_and_reports_uncertain() {
    cancellation_case(Boundary::AfterAssignment, Operation::Write);
}
#[test]
fn sigint_before_undo_revision_keeps_current_root_and_revokes_history() {
    cancellation_case(Boundary::BeforeRevision, Operation::Undo);
}
#[test]
fn sigint_after_undo_assignment_keeps_restored_root_and_reports_uncertain() {
    cancellation_case(Boundary::AfterAssignment, Operation::Undo);
}
#[test]
fn sigint_before_redo_revision_keeps_current_root_and_revokes_history() {
    cancellation_case(Boundary::BeforeRevision, Operation::Redo);
}
#[test]
fn sigint_after_redo_assignment_keeps_redone_root_and_reports_uncertain() {
    cancellation_case(Boundary::AfterAssignment, Operation::Redo);
}
