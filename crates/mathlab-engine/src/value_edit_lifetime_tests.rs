// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual-worker regressions for opaque sibling ownership and edit temporaries.
//! Includes an original handle fixture and private worker copies for fault gates.

use super::{Engine, EngineConfig, WORKER, discover_octave, octave_string};
use crate::value_edit::{CAP_ATOMIC_RANGE_EDIT, LeafDescriptor, RangeEditRequest};
use crate::{
    BindingToken, NumericClass, NumericComponents, PageData, PageWindow, SourceStorage,
    ValuePageRequest, ValuePath, ValuePathStep, WorkspaceSnapshot,
};
use std::{fs, path::PathBuf};

const PEER_SOURCE: &str =
    include_str!("../../../tests/fixtures/class_support/MathLabEditLifetimePeer.m");

fn engine_with_worker(worker: &str) -> Option<(Engine, tempfile::TempDir, PathBuf)> {
    let Some(executable) = discover_octave() else {
        assert_ne!(
            std::env::var("MATHLAB_REQUIRE_VALUE_EDIT_TESTS").as_deref(),
            Ok("1"),
            "required lifetime test needs an installed Octave"
        );
        eprintln!("SKIP value-edit lifetime regression: no installed Octave.");
        return None;
    };
    let directory = tempfile::Builder::new()
        .prefix("mathlab-edit-lifetime-")
        .tempdir()
        .unwrap();
    fs::write(
        directory.path().join("MathLabEditLifetimePeer.m"),
        PEER_SOURCE,
    )
    .unwrap();
    let marker = directory.path().join("peer-deleted");
    let engine = Engine::start_with_worker(
        EngineConfig {
            executable,
            working_directory: directory.path().to_owned(),
        },
        worker,
    )
    .unwrap();
    engine.require_capability(CAP_ATOMIC_RANGE_EDIT).unwrap();
    Some((engine, directory, marker))
}

fn execute(engine: &mut Engine, source: &str) {
    let result = engine.execute(source).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
}

fn binding(snapshot: &WorkspaceSnapshot, name: &str) -> BindingToken {
    snapshot
        .variables
        .iter()
        .find(|entry| entry.variable.name == name)
        .unwrap_or_else(|| panic!("missing {name}"))
        .token
        .clone()
}

fn leaf_path(token: BindingToken) -> ValuePath {
    ValuePath {
        root: token,
        steps: vec![ValuePathStep::Field {
            indices: vec![1.into(), 1.into()],
            name: "leaf".to_owned(),
        }],
    }
}

fn selected() -> PageWindow {
    PageWindow {
        row_start: 1.into(),
        column_start: 1.into(),
        rows: 1,
        columns: 1,
        higher_indices: vec![],
    }
}

fn nested_edit(engine: &mut Engine) -> RangeEditRequest {
    let token = binding(&engine.workspace_snapshot().unwrap(), "R");
    // Both this snapshot and typed read must preserve the binding/history.
    let page = engine
        .read_value_page_checked(&ValuePageRequest {
            path: leaf_path(token),
            window: selected(),
        })
        .unwrap();
    assert_eq!(page.class, NumericClass::Uint64);
    RangeEditRequest::from_components(
        leaf_path(page.token),
        selected(),
        LeafDescriptor {
            class: page.class,
            source_shape: page.source_shape,
            source_storage: page.source_storage,
            complex: page.complex,
        },
        &NumericComponents::Uint64(vec![101]),
        None,
    )
    .unwrap()
}

fn assert_leaf(engine: &mut Engine, expected: u64) {
    let snapshot = engine.workspace_snapshot().unwrap();
    let page = engine
        .read_value_page_checked(&ValuePageRequest {
            path: leaf_path(binding(&snapshot, "R")),
            window: selected(),
        })
        .unwrap();
    assert_eq!(page.source_storage, SourceStorage::Dense);
    assert_eq!(page.source_shape, vec![2.into(), 3.into()]);
    assert_eq!(
        page.data,
        PageData::Dense {
            real: NumericComponents::Uint64(vec![expected]),
            imag: None,
        }
    );
    assert_eq!(
        engine.workspace_snapshot().unwrap().context,
        snapshot.context
    );
}

fn make_root(engine: &mut Engine, marker: &std::path::Path, mode: &str) {
    let marker = octave_string(marker).unwrap();
    execute(
        engine,
        &format!(
            "R=struct('leaf',builtin('uint64',builtin('reshape',1:6,2,3)),'peer',MathLabEditLifetimePeer({marker},'{mode}')); A=builtin('uint64',7);"
        ),
    );
}

#[test]
fn finished_nested_edit_releases_opaque_sibling_before_user_clear() {
    let Some((mut engine, _directory, marker)) = engine_with_worker(WORKER) else {
        return;
    };
    let pid = engine.process_id();
    make_root(&mut engine, &marker, "marker");
    let request = nested_edit(&mut engine);
    let receipt = engine.write_value_range_checked(&request).unwrap();
    assert!(
        !marker.exists(),
        "peer was deleted while still owned in base"
    );
    assert_leaf(&mut engine, 101);
    assert_eq!(
        binding(&engine.workspace_snapshot().unwrap(), "R"),
        receipt.token,
        "pure inspection revoked the accepted binding"
    );

    // This ordinary operation is the only remaining owner release. No later
    // edit, worker restart, function clear or destructor invocation is used.
    let marker_literal = octave_string(&marker).unwrap();
    execute(
        &mut engine,
        &format!(
            "clear R; if (builtin('exist',{marker_literal},'file') != 2); builtin('error','MathLab:LifetimeNotReleased','Peer was not released inside the explicit base clear.'); endif;"
        ),
    );
    assert!(
        marker.exists(),
        "completed edit retained a hidden root: peer delete did not run at clear R"
    );
    assert_eq!(fs::read_to_string(&marker).unwrap(), "deleted\n");
    assert!(
        engine
            .workspace_snapshot()
            .unwrap()
            .variables
            .iter()
            .all(|entry| entry.variable.name != "R")
    );
    assert_eq!(engine.process_id(), pid);
}

fn history_request(
    receipt: &crate::value_edit::RangeEditReceipt,
    redo: bool,
) -> crate::value_edit::HistoryRequest {
    let head = if redo {
        &receipt.history.redo
    } else {
        &receipt.history.undo
    }
    .as_ref()
    .unwrap();
    crate::value_edit::HistoryRequest {
        schema_version: 1,
        expected: receipt.token.clone(),
        history_epoch: receipt.history.epoch,
        transaction_id: head.transaction_id,
    }
}

fn root_a_edit(engine: &mut Engine, value: u64) -> RangeEditRequest {
    let token = binding(&engine.workspace_snapshot().unwrap(), "A");
    let page = engine
        .read_value_page_checked(&ValuePageRequest {
            path: ValuePath {
                root: token,
                steps: vec![],
            },
            window: selected(),
        })
        .unwrap();
    RangeEditRequest::from_components(
        ValuePath {
            root: page.token,
            steps: vec![],
        },
        selected(),
        LeafDescriptor {
            class: page.class,
            source_shape: page.source_shape,
            source_storage: page.source_storage,
            complex: page.complex,
        },
        &NumericComponents::Uint64(vec![value]),
        None,
    )
    .unwrap()
}

fn assert_deleted_once(marker: &std::path::Path) {
    assert!(marker.exists(), "no synchronous last-owner delete marker");
    assert_eq!(fs::read_to_string(marker).unwrap(), "deleted\n");
}

#[test]
fn history_moves_release_root_while_base_alias_preserves_peer() {
    let Some((mut engine, _directory, marker)) = engine_with_worker(WORKER) else {
        return;
    };
    let pid = engine.process_id();
    make_root(&mut engine, &marker, "assignin");
    execute(&mut engine, "Ralias=R;");
    let request = nested_edit(&mut engine);
    let written = engine.write_value_range_checked(&request).unwrap();
    let undone = engine
        .undo_value_edit_checked(&history_request(&written, false))
        .unwrap();
    let redone = engine
        .redo_value_edit_checked(&history_request(&undone, true))
        .unwrap();
    assert_eq!(written.transaction_id, redone.transaction_id);
    assert_leaf(&mut engine, 101);
    assert!(!marker.exists());

    // A real base alias must retain the peer, independently of worker temps.
    execute(&mut engine, "clear R;");
    assert!(
        !marker.exists(),
        "cleanup destroyed a base-owned opaque peer"
    );
    clear_and_require_delete(&mut engine, &marker, "Ralias");
    // These mutations are allowed user effects during ordinary clear, whose
    // dispatch already revoked tokens/history. They must never be delayed into
    // the next checked edit before its own commit/revocation boundary.
    execute(
        &mut engine,
        "assert(A==builtin('uint64',900)); assert(edit_lifetime_called); clear edit_lifetime_called; A=builtin('uint64',7);",
    );
    let fresh = root_a_edit(&mut engine, 111);
    let written = engine.write_value_range_checked(&fresh).unwrap();
    let undone = engine
        .undo_value_edit_checked(&history_request(&written, false))
        .unwrap();
    engine
        .redo_value_edit_checked(&history_request(&undone, true))
        .unwrap();
    assert!(
        engine
            .workspace_snapshot()
            .unwrap()
            .variables
            .iter()
            .all(|entry| entry.variable.name != "edit_lifetime_called")
    );
    assert_deleted_once(&marker);
    assert_eq!(engine.process_id(), pid);
}

#[test]
fn variable_only_cleanup_preserves_base_collisions_and_unrelated_function_scope() {
    let Some((mut engine, directory, _marker)) = engine_with_worker(WORKER) else {
        return;
    };
    fs::write(
        directory.path().join("candidate_history.m"),
        "function value=candidate_history()\n persistent count=0; count+=1; value=count;\nend\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("clear.m"),
        "function varargout=clear(varargin)\n builtin('assignin','base','lifetime_clear_shadow_called',true); builtin('error','MathLab:LifetimeShadow','Project clear called.');\nend\n",
    )
    .unwrap();
    let path = octave_string(directory.path()).unwrap();
    execute(
        &mut engine,
        &format!(
            "addpath({path},'-begin'); A=builtin('uint64',7); replacement=71; patch=72; edit_receipt=73; edit_request=74; edit_token=75; operation=76; binding_request=77; next_revision=78; next_context=79; next_token=80; context=81; request=82; request_line=83; result=84; caught=85; execution_error=86; ans=87; lifetime_counter_handle=@candidate_history; lifetime_counter_first=lifetime_counter_handle(); assert(lifetime_counter_first==1);"
        ),
    );
    // Repeated absent transport locals must not cause clear's function fallback.
    engine.workspace_snapshot().unwrap();
    engine.workspace_snapshot().unwrap();
    let edit = root_a_edit(&mut engine, 111);
    let written = engine.write_value_range_checked(&edit).unwrap();
    let undone = engine
        .undo_value_edit_checked(&history_request(&written, false))
        .unwrap();
    engine
        .redo_value_edit_checked(&history_request(&undone, true))
        .unwrap();
    assert!(
        engine
            .workspace_snapshot()
            .unwrap()
            .variables
            .iter()
            .all(|entry| entry.variable.name != "lifetime_clear_shadow_called")
    );
    execute(
        &mut engine,
        "assert(replacement==71 && patch==72 && edit_receipt==73 && edit_request==74 && edit_token==75 && operation==76 && binding_request==77 && next_revision==78 && next_context==79 && next_token==80 && context==81 && request==82 && request_line==83 && result==84 && caught==85 && execution_error==86 && ans==87); lifetime_counter_named=candidate_history(); lifetime_counter_retained=lifetime_counter_handle(); assert(lifetime_counter_named==2 && lifetime_counter_retained==3);",
    );
    // Positive control: the project function remains reachable through normal
    // resolution; protected cleanup must bypass it, never remove or replace it.
    execute(
        &mut engine,
        "try; clear(); catch control_error; assert(builtin('strcmp',control_error.identifier,'MathLab:LifetimeShadow')); end; assert(lifetime_clear_shadow_called);",
    );
}

#[derive(Clone, Copy)]
enum ErrorBoundary {
    BeforeRevision,
    AfterAssignment,
}

fn worker_with_error(boundary: ErrorBoundary) -> String {
    let anchor = match boundary {
        ErrorBoundary::BeforeRevision => "## MATHLAB_VALUE_EDIT_PRECOMMIT_BOUNDARY",
        ErrorBoundary::AfterAssignment => "## MATHLAB_VALUE_EDIT_POSTASSIGN_BOUNDARY",
    };
    assert_eq!(WORKER.matches(anchor).count(), 1);
    WORKER.replacen(
        anchor,
        &format!(
            "{anchor}\n            if (builtin('strcmp',edit_token.name,'R')); builtin('error','MathLab:ValueEditPayload','Private exact-boundary rejection.'); endif"
        ),
        1,
    )
}

fn clear_and_require_delete(engine: &mut Engine, marker: &std::path::Path, name: &str) {
    assert!(matches!(name, "R" | "Ralias"));
    let quoted = octave_string(marker).unwrap();
    execute(
        engine,
        &format!(
            "clear {name}; if (builtin('exist',{quoted},'file')!=2); builtin('error','MathLab:LifetimeNotReleased','Delete must occur inside the final user clear.'); endif;"
        ),
    );
    assert_deleted_once(marker);
}

fn error_lifetime_case(boundary: ErrorBoundary) {
    let worker = worker_with_error(boundary);
    let Some((mut engine, _directory, marker)) = engine_with_worker(&worker) else {
        return;
    };
    let pid = engine.process_id();
    make_root(&mut engine, &marker, "assignin");
    let request = nested_edit(&mut engine);
    let before = request.path.root.context.clone();
    let error = engine.write_value_range_checked(&request).unwrap_err();
    let expected = match boundary {
        ErrorBoundary::BeforeRevision => {
            assert!(
                matches!(error, crate::Error::Worker(ref value) if value.identifier=="MathLab:ValueEditPayload")
            );
            assert!(!engine.has_value_edit_uncertainty());
            1
        }
        ErrorBoundary::AfterAssignment => {
            assert!(matches!(
                error,
                crate::Error::ValueEdit(crate::value_edit::EditError::UncertainCommit(_))
            ));
            assert!(engine.has_value_edit_uncertainty());
            // A local latch check does not send a request or release temporaries.
            assert!(matches!(
                engine.write_value_range_checked(&request),
                Err(crate::Error::ValueEdit(
                    crate::value_edit::EditError::UncertainCommit(_)
                ))
            ));
            101
        }
    };
    assert!(
        !marker.exists(),
        "error cleanup destroyed a base-owned peer"
    );
    let quoted = octave_string(&marker).unwrap();
    // This is the IMMEDIATE next worker request. Assert the all-old/all-new
    // value, then require deletion in the source before dispatch cleanup runs.
    execute(
        &mut engine,
        &format!(
            "if (R.leaf(1)!=builtin('uint64',{expected})); builtin('error','MathLab:LifetimeValue','Wrong committed root.'); endif; clear R; if (builtin('exist',{quoted},'file')!=2); builtin('error','MathLab:LifetimeNotReleased','Delete was delayed beyond clear R.'); endif;"
        ),
    );
    assert_deleted_once(&marker);
    let after = engine.workspace_snapshot().unwrap().context;
    assert_eq!(after.worker_id, before.worker_id);
    assert_eq!(after.generation, before.generation);
    assert_eq!(after.invocation, before.invocation);
    let expected_advance = match boundary {
        ErrorBoundary::BeforeRevision => 1, // only the ordinary clear execution
        ErrorBoundary::AfterAssignment => 2, // edit commit + ordinary execution
    };
    assert_eq!(after.revision, before.revision + expected_advance);
    execute(
        &mut engine,
        "assert(edit_lifetime_called); assert(A==builtin('uint64',900)); A=builtin('uint64',7);",
    );
    if matches!(boundary, ErrorBoundary::AfterAssignment) {
        engine.reset_value_edit_history().unwrap();
    }
    let usable = root_a_edit(&mut engine, 111);
    engine.write_value_range_checked(&usable).unwrap();
    assert_eq!(engine.process_id(), pid);
}

#[test]
fn precommit_candidate_rejection_releases_hidden_root_without_revocation() {
    error_lifetime_case(ErrorBoundary::BeforeRevision);
}

#[test]
fn postassign_uncertainty_releases_hidden_root_before_next_user_clear() {
    error_lifetime_case(ErrorBoundary::AfterAssignment);
}

struct LifetimeGate {
    release: PathBuf,
    cancellation: crate::CancellationHandle,
}
impl Drop for LifetimeGate {
    fn drop(&mut self) {
        let _ = fs::write(&self.release, []);
        let _ = self.cancellation.terminate();
    }
}

fn sigint_lifetime_case(boundary: ErrorBoundary) {
    use std::{
        thread,
        time::{Duration, Instant},
    };
    let gate_directory = tempfile::Builder::new()
        .prefix("mathlab-edit-lifetime-gate-")
        .tempdir()
        .unwrap();
    let reached = gate_directory.path().join("reached");
    let release = gate_directory.path().join("release");
    let anchor = match boundary {
        ErrorBoundary::BeforeRevision => "## MATHLAB_VALUE_EDIT_PRECOMMIT_BOUNDARY",
        ErrorBoundary::AfterAssignment => "## MATHLAB_VALUE_EDIT_POSTASSIGN_BOUNDARY",
    };
    assert_eq!(WORKER.matches(anchor).count(), 1);
    let worker = WORKER.replacen(
        anchor,
        &format!(
            "{anchor}\n            if (builtin('strcmp',edit_token.name,'R')); lifetime_test_file=builtin('fopen',{},'w'); builtin('fclose',lifetime_test_file); while (! builtin('exist',{},'file')); builtin('pause',0.005); endwhile; endif",
            octave_string(&reached).unwrap(),
            octave_string(&release).unwrap()
        ),
        1,
    );
    let Some((mut engine, _directory, marker)) = engine_with_worker(&worker) else {
        return;
    };
    let pid = engine.process_id();
    make_root(&mut engine, &marker, "assignin");
    let request = nested_edit(&mut engine);
    let old = request.path.root.clone();
    let guard = LifetimeGate {
        release,
        cancellation: engine.cancellation_handle(),
    };
    let task = thread::spawn(move || {
        let result = engine.write_value_range_checked(&request);
        (engine, result)
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    while !reached.exists() {
        assert!(Instant::now() < deadline, "worker missed lifetime gate");
        thread::sleep(Duration::from_millis(5));
    }
    guard.cancellation.interrupt().unwrap();
    let deadline = Instant::now() + Duration::from_secs(25);
    while !task.is_finished() {
        assert!(Instant::now() < deadline, "lifetime SIGINT did not settle");
        thread::sleep(Duration::from_millis(5));
    }
    let (mut engine, result) = task.join().unwrap();
    assert!(matches!(
        result,
        Err(crate::Error::ValueEdit(
            crate::value_edit::EditError::UncertainCommit(_)
        ))
    ));
    assert!(engine.has_value_edit_uncertainty());
    assert_eq!(engine.process_id(), pid);
    let after = engine.workspace_snapshot().unwrap();
    assert_eq!(after.context.generation, old.context.generation);
    assert!(after.context.invocation > old.context.invocation);
    assert_leaf(
        &mut engine,
        match boundary {
            ErrorBoundary::BeforeRevision => 1,
            ErrorBoundary::AfterAssignment => 101,
        },
    );
    assert!(
        matches!(engine.read_value_page_checked(&ValuePageRequest { path:leaf_path(old), window:selected() }), Err(crate::Error::Worker(ref error)) if error.identifier=="MathLab:StaleBinding")
    );
    let blocked = root_a_edit(&mut engine, 111);
    assert!(matches!(
        engine.write_value_range_checked(&blocked),
        Err(crate::Error::ValueEdit(
            crate::value_edit::EditError::UncertainCommit(_)
        ))
    ));
    engine.reset_value_edit_history().unwrap();
    assert!(
        !marker.exists(),
        "SIGINT cleanup destroyed a base-owned peer"
    );
    clear_and_require_delete(&mut engine, &marker, "R");
    execute(
        &mut engine,
        "assert(edit_lifetime_called); assert(A==builtin('uint64',900)); A=builtin('uint64',7);",
    );
    let usable = root_a_edit(&mut engine, 111);
    engine.write_value_range_checked(&usable).unwrap();
    assert_eq!(engine.process_id(), pid);
    // Guard terminates only this now-idle, test-owned session on normal exit;
    // on failed watchdog/assertion it also releases the private file barrier.
    drop(guard);
}

#[test]
fn sigint_precommit_releases_candidates_without_deleting_base_peer() {
    sigint_lifetime_case(ErrorBoundary::BeforeRevision);
}

#[test]
fn sigint_postassign_releases_candidates_without_deleting_base_peer() {
    sigint_lifetime_case(ErrorBoundary::AfterAssignment);
}

#[test]
fn immediate_next_user_clear_proves_write_candidate_was_already_released() {
    let Some((mut engine, _directory, marker)) = engine_with_worker(WORKER) else {
        return;
    };
    make_root(&mut engine, &marker, "assignin");
    let request = nested_edit(&mut engine);
    engine.write_value_range_checked(&request).unwrap();
    assert!(!marker.exists());
    // No snapshot, page read, ping, ordinary eval or next edit can mask lifetime.
    clear_and_require_delete(&mut engine, &marker, "R");
    execute(
        &mut engine,
        "assert(edit_lifetime_called); assert(A==builtin('uint64',900));",
    );
}

#[test]
fn precommit_rejection_preserves_prior_history_independently_of_lifetime_probe() {
    let worker = worker_with_error(ErrorBoundary::BeforeRevision);
    let Some((mut engine, _directory, marker)) = engine_with_worker(&worker) else {
        return;
    };
    make_root(&mut engine, &marker, "marker");
    let prior = root_a_edit(&mut engine, 17);
    let accepted = engine.write_value_range_checked(&prior).unwrap();
    let rejected = nested_edit(&mut engine);
    let before = rejected.path.root.context.clone();
    assert!(
        matches!(engine.write_value_range_checked(&rejected), Err(crate::Error::Worker(ref error)) if error.identifier=="MathLab:ValueEditPayload")
    );
    assert_eq!(engine.workspace_snapshot().unwrap().context, before);
    let undone = engine
        .undo_value_edit_checked(&history_request(&accepted, false))
        .unwrap();
    assert_eq!(undone.transaction_id, accepted.transaction_id);
    assert_eq!(undone.history.epoch, accepted.history.epoch);
    assert_eq!(undone.retained_patch_bytes, accepted.retained_patch_bytes);
    let current = root_a_edit(&mut engine, 7);
    // Do not commit this inspection draft: its expected leaf is the actually
    // restored A, independently checked through the underlying typed page.
    let page = engine
        .read_value_page_checked(&ValuePageRequest {
            path: current.path,
            window: selected(),
        })
        .unwrap();
    assert_eq!(
        page.data,
        PageData::Dense {
            real: NumericComponents::Uint64(vec![7]),
            imag: None
        }
    );
    assert!(!marker.exists());
    clear_and_require_delete(&mut engine, &marker, "R");
}
