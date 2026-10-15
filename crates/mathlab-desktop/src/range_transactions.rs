// SPDX-License-Identifier: GPL-3.0-or-later
//! Frontend transaction ownership. No widget, clipboard access or engine calls.
//! Retains bounded identities/requests, never source arrays or undo value copies.

use crate::range_paste::ParsedRangePaste;
use mathlab_engine::value_edit::{
    EditOperation, HistoryHeads, HistoryRequest, LeafDescriptor, RangeEditReceipt,
    RangeEditRequest, ReceiptExpectation, EDIT_SCHEMA_VERSION,
};
use mathlab_engine::{PageWindow, SessionContext, ValuePage, ValuePath, WorkspaceSnapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageIdentity {
    pub view_id: u64,
    pub path: ValuePath,
    pub window: PageWindow,
    pub leaf: LeafDescriptor,
}

impl PageIdentity {
    pub fn accepted(view_id: u64, page: &ValuePage) -> Self {
        Self {
            view_id,
            path: ValuePath {
                root: page.token.clone(),
                steps: page.path.clone(),
            },
            window: PageWindow {
                row_start: page.selection.row_start,
                column_start: page.selection.column_start,
                rows: page.selection.rows,
                columns: page.selection.columns,
                higher_indices: page.selection.higher_indices.clone(),
            },
            leaf: LeafDescriptor {
                class: page.class,
                source_shape: page.source_shape.clone(),
                source_storage: page.source_storage,
                complex: page.complex,
            },
        }
    }

    fn includes(&self, window: &PageWindow) -> bool {
        fn contains(start: u64, count: u32, other: u64, length: u32) -> bool {
            count > 0
                && length > 0
                && other >= start
                && other
                    .checked_add(u64::from(length))
                    .zip(start.checked_add(u64::from(count)))
                    .is_some_and(|(end, limit)| end <= limit)
        }
        self.window.higher_indices == window.higher_indices
            && contains(
                self.window.row_start.get(),
                self.window.rows,
                window.row_start.get(),
                window.rows,
            )
            && contains(
                self.window.column_start.get(),
                self.window.columns,
                window.column_start.get(),
                window.columns,
            )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionCommand {
    Write(RangeEditRequest),
    Undo(HistoryRequest),
    Redo(HistoryRequest),
    Reset { expected: SessionContext },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionEnvelope {
    pub request_id: u64,
    pub page: PageIdentity,
    pub command: TransactionCommand,
}

impl TransactionEnvelope {
    /// Worker rechecks this independently before any numerical operation.
    pub fn validate(&self) -> Result<(), String> {
        let matches = match &self.command {
            TransactionCommand::Write(request) => {
                request
                    .validate_decode()
                    .map_err(|error| error.to_string())?;
                request.path == self.page.path
                    && request.expected_leaf == self.page.leaf
                    && self.page.includes(&request.window)
            }
            TransactionCommand::Undo(request) | TransactionCommand::Redo(request) => {
                request.validate().map_err(|error| error.to_string())?;
                request.expected == self.page.path.root
            }
            TransactionCommand::Reset { expected } => expected == &self.page.path.root.context,
        };
        if matches {
            Ok(())
        } else {
            Err("The transaction no longer matches its accepted page, binding or selection.".into())
        }
    }

    fn expectation(&self) -> Option<ReceiptExpectation> {
        let (operation, transaction_id, history_epoch) = match &self.command {
            TransactionCommand::Write(_) => (EditOperation::Write, None, None),
            TransactionCommand::Undo(request) => (
                EditOperation::Undo,
                Some(request.transaction_id),
                Some(request.history_epoch),
            ),
            TransactionCommand::Redo(request) => (
                EditOperation::Redo,
                Some(request.transaction_id),
                Some(request.history_epoch),
            ),
            TransactionCommand::Reset { .. } => return None,
        };
        Some(ReceiptExpectation {
            operation,
            previous: self.page.path.root.clone(),
            transaction_id,
            history_epoch,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionFailure {
    pub message: String,
    pub uncertain: bool,
    pub disconnected: bool,
}

impl TransactionFailure {
    pub fn rejected(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            uncertain: false,
            disconnected: false,
        }
    }
}

pub struct TransactionRefresh {
    pub snapshot: WorkspaceSnapshot,
    pub page: ValuePage,
}

pub enum TransactionOutcome {
    /// A validated receipt remains a successful commit even if its pure refresh
    /// fails. Never turn a read failure into an invitation to repeat the write.
    Applied {
        receipt: Box<RangeEditReceipt>,
        refresh: Result<Box<TransactionRefresh>, TransactionFailure>,
    },
    Reset(WorkspaceSnapshot),
    Failed(TransactionFailure),
}

#[derive(Default)]
pub struct RangeTransactions {
    accepted: Option<PageIdentity>,
    context: Option<SessionContext>,
    history: Option<HistoryHeads>,
    pending: Option<TransactionEnvelope>,
    quarantine: Option<String>,
    connection_lost: bool,
}

impl RangeTransactions {
    pub fn accepted_page(&self) -> Option<&PageIdentity> {
        self.accepted.as_ref()
    }
    pub fn history(&self) -> Option<&HistoryHeads> {
        self.history.as_ref()
    }
    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn quarantine(&self) -> Option<&str> {
        self.quarantine.as_deref()
    }
    pub fn writes_blocked(&self) -> bool {
        self.connection_lost || self.pending() || self.quarantine.is_some()
    }

    pub fn connection_lost(&self) -> bool {
        self.connection_lost
    }

    /// Call only after the existing inspector accepted a complete typed page.
    /// Pure refresh cannot clear uncertainty or manufacture missing history.
    pub fn observe_page(&mut self, view_id: u64, page: &ValuePage) {
        if self.connection_lost || self.pending() {
            return;
        }
        if self
            .context
            .as_ref()
            .is_some_and(|context| context != &page.token.context)
        {
            self.history = None;
        }
        self.context = Some(page.token.context.clone());
        self.accepted = Some(PageIdentity::accepted(view_id, page));
    }

    pub fn clear_page(&mut self) {
        self.accepted = None;
    }

    /// Ordinary execution (including failed/Stop jobs), legacy preview, scalar
    /// mutation or scope evaluation conservatively retires history. Explicit
    /// restart also abandons any old pending envelope before a new session.
    /// A refresh alone never clears a previously uncertain commit.
    pub fn invalidate_context(&mut self) {
        self.accepted = None;
        self.context = None;
        self.history = None;
        if self.pending.take().is_some() {
            self.quarantine = Some("The session changed while an edit was outstanding. Reset edit history or restart before another mutation.".into());
        }
    }

    /// Console EOF can precede the actor's valid edit receipt. Preserve only its
    /// exact pending envelope for settlement; numerical pages/history stay dead.
    /// A reset or a late successful read cannot revive the lost connection.
    pub fn disconnected(&mut self) {
        self.connection_lost = true;
        self.accepted = None;
        self.context = None;
        self.history = None;
        self.quarantine.get_or_insert_with(|| {
            "The connection closed. Restart before another variable mutation or history reset."
                .into()
        });
    }

    /// Only a successful new session or a validated explicit reset clears this.
    pub fn restarted(&mut self) {
        *self = Self::default();
    }

    fn available(&self, allow_quarantine: bool) -> Result<PageIdentity, String> {
        if self.connection_lost {
            return Err("The connection closed. Restart before another transaction.".into());
        }
        if self.pending() {
            return Err("An edit transaction is already pending.".into());
        }
        if !allow_quarantine {
            if let Some(message) = &self.quarantine {
                return Err(message.clone());
            }
        }
        self.accepted
            .clone()
            .ok_or_else(|| "Refresh and accept a typed page before this operation.".into())
    }

    fn begin(
        &mut self,
        request_id: u64,
        page: PageIdentity,
        command: TransactionCommand,
    ) -> Result<TransactionEnvelope, String> {
        let envelope = TransactionEnvelope {
            request_id,
            page,
            command,
        };
        envelope.validate()?;
        self.pending = Some(envelope.clone());
        Ok(envelope)
    }

    pub fn begin_paste(
        &mut self,
        request_id: u64,
        paste: ParsedRangePaste,
    ) -> Result<TransactionEnvelope, String> {
        let page = self.available(false)?;
        let request = paste
            .into_edit_request()
            .map_err(|error| error.to_string())?;
        self.begin(request_id, page, TransactionCommand::Write(request))
    }

    pub fn begin_history(
        &mut self,
        request_id: u64,
        operation: EditOperation,
    ) -> Result<TransactionEnvelope, String> {
        let page = self.available(false)?;
        let heads = self
            .history
            .as_ref()
            .ok_or_else(|| "No accepted edit history is available.".to_owned())?;
        let head = match operation {
            EditOperation::Undo => &heads.undo,
            EditOperation::Redo => &heads.redo,
            EditOperation::Write => return Err("Use a validated paste request for a write.".into()),
        }
        .as_ref()
        .ok_or_else(|| "No matching history entry is available.".to_owned())?;
        if head.root_name != page.path.root.name {
            return Err("Open and refresh the history entry's root variable first.".into());
        }
        let request = HistoryRequest {
            schema_version: EDIT_SCHEMA_VERSION,
            expected: page.path.root.clone(),
            history_epoch: heads.epoch,
            transaction_id: head.transaction_id,
        };
        self.begin(
            request_id,
            page,
            match operation {
                EditOperation::Undo => TransactionCommand::Undo(request),
                EditOperation::Redo => TransactionCommand::Redo(request),
                EditOperation::Write => unreachable!(),
            },
        )
    }

    /// The page must have been freshly read if uncertainty changed its context.
    /// The actor checks the context once more before resetting numerical history.
    pub fn begin_reset(&mut self, request_id: u64) -> Result<TransactionEnvelope, String> {
        let page = self.available(true)?;
        let expected = page.path.root.context.clone();
        self.begin(request_id, page, TransactionCommand::Reset { expected })
    }

    /// Exact echo comparison includes request payload, path, plane, view and ID.
    /// A late reply cannot release another operation or change visible values.
    pub fn finish(
        &mut self,
        envelope: &TransactionEnvelope,
        mut outcome: TransactionOutcome,
    ) -> Option<TransactionOutcome> {
        if self.pending.as_ref() != Some(envelope) {
            return None;
        }
        self.pending = None;
        self.accepted = None;
        let valid = match &mut outcome {
            TransactionOutcome::Applied { receipt, refresh } => {
                let validation = envelope
                    .expectation()
                    .ok_or_else(|| "Reset cannot accept an edit receipt.".to_owned())
                    .and_then(|expectation| {
                        let mut settled = expectation.previous.context.clone();
                        settled.revision = settled
                            .revision
                            .checked_add(1)
                            .ok_or_else(|| "Revision counter exhausted.".to_owned())?;
                        receipt
                            .validate(&expectation, &settled)
                            .map_err(|error| error.to_string())
                    });
                if validation.is_ok() {
                    if self.connection_lost {
                        // Even an apparently successful late refresh describes
                        // an offline generation, never a newly accepted page.
                        *refresh = Err(TransactionFailure {
                            message: "The connection closed before the committed edit could be displayed. Restart before reading or editing values.".into(),
                            uncertain: false,
                            disconnected: true,
                        });
                    } else {
                        self.context = Some(receipt.token.context.clone());
                        self.history = Some(receipt.history.clone());
                    }
                    if let Ok(refreshed) = refresh {
                        if !refresh_matches(envelope, receipt, refreshed) {
                            *refresh = Err(TransactionFailure::rejected(
                                "The committed edit's refresh does not match its exact receipt, page or binding. Refresh explicitly; do not repeat the edit.",
                            ));
                        }
                    }
                }
                validation
            }
            TransactionOutcome::Reset(snapshot) => match &envelope.command {
                TransactionCommand::Reset { expected }
                    if next_context(expected).as_ref() == Some(&snapshot.context)
                        && snapshot_consistent(snapshot) =>
                {
                    if !self.connection_lost {
                        self.context = Some(snapshot.context.clone());
                        self.history = None;
                        self.quarantine = None;
                    }
                    Ok(())
                }
                _ => {
                    Err("Reset returned an unexpected session context or binding snapshot.".into())
                }
            },
            TransactionOutcome::Failed(_) => Ok(()),
        };
        if let Err(message) = valid {
            outcome = TransactionOutcome::Failed(TransactionFailure {
                message,
                uncertain: true,
                disconnected: false,
            });
        }
        if self.connection_lost {
            if matches!(&outcome, TransactionOutcome::Reset(_)) {
                outcome = TransactionOutcome::Failed(TransactionFailure {
                    message: "History reset completed, but the connection closed. Restart before reading or editing values.".into(),
                    uncertain: false,
                    disconnected: true,
                });
            }
            if let TransactionOutcome::Failed(failure) = &mut outcome {
                failure.disconnected = true;
            }
        }
        if let TransactionOutcome::Failed(failure) = &outcome {
            if failure.uncertain || failure.disconnected {
                self.context = None;
                self.history = None;
            }
            // A known connected rejection did not retire worker history. The
            // page stays invalid until an explicit read; observe_page then
            // preserves these heads only if the complete context still matches.
            if failure.uncertain {
                self.quarantine = Some(failure.message.clone());
            }
        }
        Some(outcome)
    }
}

fn next_context(context: &SessionContext) -> Option<SessionContext> {
    let mut next = context.clone();
    next.revision = next.revision.checked_add(1)?;
    Some(next)
}

fn snapshot_consistent(snapshot: &WorkspaceSnapshot) -> bool {
    snapshot.variables.iter().all(|binding| {
        binding.token.context == snapshot.context && binding.token.name == binding.variable.name
    })
}

fn refresh_matches(
    envelope: &TransactionEnvelope,
    receipt: &RangeEditReceipt,
    refresh: &TransactionRefresh,
) -> bool {
    let mut expected = envelope.page.clone();
    expected.path.root = receipt.token.clone();
    refresh.snapshot.context == receipt.token.context
        && snapshot_consistent(&refresh.snapshot)
        && refresh
            .snapshot
            .variables
            .iter()
            .any(|binding| binding.token == receipt.token)
        && PageIdentity::accepted(expected.view_id, &refresh.page) == expected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::range_paste::PasteTarget;
    use mathlab_engine::value_edit::HistoryHead;
    use mathlab_engine::{
        BindingToken, BoundVariable, NumericClass, NumericComponents, PageData, PageSelection,
        SourceStorage, Variable,
    };

    fn sample_page(revision: u64, plane: u64) -> ValuePage {
        ValuePage {
            token: BindingToken {
                name: "A".into(),
                context: SessionContext {
                    worker_id: "range-test-worker".into(),
                    generation: 3,
                    invocation: 1,
                    revision,
                },
            },
            path: vec![],
            class: NumericClass::Uint64,
            source_shape: vec![3.into(), 4.into(), 5.into()],
            source_storage: SourceStorage::Dense,
            complex: false,
            selection: PageSelection {
                row_start: 1.into(),
                column_start: 1.into(),
                rows: 1,
                columns: 2,
                higher_indices: vec![plane.into()],
            },
            data: PageData::Dense {
                real: NumericComponents::Uint64(vec![u64::MAX, 9_007_199_254_740_993]),
                imag: None,
            },
        }
    }

    fn paste(page: &ValuePage) -> ParsedRangePaste {
        ParsedRangePaste {
            target: PasteTarget {
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
            real: NumericComponents::Uint64(vec![99, u64::MAX]),
            imag: None,
        }
    }

    fn snapshot(page: &ValuePage) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            context: page.token.context.clone(),
            variables: vec![BoundVariable {
                token: page.token.clone(),
                variable: Variable {
                    name: "A".into(),
                    class_name: "uint64".into(),
                    dimensions: vec![3, 4, 5],
                    dimensions_known: true,
                    bytes: 480,
                    bytes_known: true,
                    preview: String::new(),
                },
            }],
        }
    }

    fn applied(envelope: &TransactionEnvelope) -> TransactionOutcome {
        let expected = envelope.expectation().unwrap();
        let mut after = sample_page(expected.previous.context.revision + 1, 4);
        after.token.context = next_context(&expected.previous.context).unwrap();
        let linked = HistoryHead {
            transaction_id: expected.transaction_id.unwrap_or(7.into()),
            root_name: "A".into(),
        };
        let history = HistoryHeads {
            epoch: expected.history_epoch.unwrap_or(4_294_967_297.into()),
            undo: (expected.operation != EditOperation::Undo).then_some(linked.clone()),
            redo: (expected.operation == EditOperation::Undo).then_some(linked),
        };
        TransactionOutcome::Applied {
            receipt: Box::new(RangeEditReceipt {
                schema_version: EDIT_SCHEMA_VERSION,
                operation: expected.operation,
                transaction_id: expected.transaction_id.unwrap_or(7.into()),
                token: after.token.clone(),
                history,
                retained_patch_bytes: 32,
                retained_transactions: 1,
                evicted_undo_entries: 0,
            }),
            refresh: Ok(Box::new(TransactionRefresh {
                snapshot: snapshot(&after),
                page: after,
            })),
        }
    }

    fn ready() -> (RangeTransactions, ValuePage) {
        let page = sample_page(17, 4);
        let mut state = RangeTransactions::default();
        state.observe_page(11, &page);
        (state, page)
    }

    fn ready_history(operation: EditOperation) -> (RangeTransactions, ValuePage) {
        let (mut state, page) = ready();
        let write = state.begin_paste(12, paste(&page)).unwrap();
        let Some(TransactionOutcome::Applied {
            refresh: Ok(refresh),
            ..
        }) = state.finish(&write, applied(&write))
        else {
            panic!("expected write");
        };
        let mut page = refresh.page;
        state.observe_page(12, &page);
        if operation == EditOperation::Redo {
            let undo = state.begin_history(13, EditOperation::Undo).unwrap();
            let Some(TransactionOutcome::Applied {
                refresh: Ok(refresh),
                ..
            }) = state.finish(&undo, applied(&undo))
            else {
                panic!("expected undo");
            };
            page = refresh.page;
            state.observe_page(13, &page);
        }
        (state, page)
    }

    #[test]
    fn paste_requires_original_token_plane_and_rectangle_within_accepted_page() {
        let (mut state, page) = ready();
        let mut wrong = paste(&page);
        wrong.target.path.root.context.revision += 1;
        assert!(state.begin_paste(12, wrong).is_err());
        let mut wrong = paste(&page);
        wrong.target.selection.higher_indices[0] = 2.into();
        assert!(state.begin_paste(12, wrong).is_err());
        let mut wrong = paste(&page);
        wrong.target.selection.row_start = 2.into();
        assert!(state.begin_paste(12, wrong).is_err());
        assert!(!state.pending());
        let request = state.begin_paste(12, paste(&page)).unwrap();
        let TransactionCommand::Write(request) = request.command else {
            panic!("expected write");
        };
        assert_eq!(request.path.root, page.token);
        assert_eq!(request.window.higher_indices, vec![4.into()]);
        assert!(
            matches!(request.data, mathlab_engine::value_edit::EditData::Dense { real, .. } if real == ["99", "18446744073709551615"])
        );
    }

    #[test]
    fn altered_payload_or_old_view_reply_cannot_complete_current_request() {
        let (mut state, page) = ready();
        let command = state.begin_paste(12, paste(&page)).unwrap();
        let mut altered = command.clone();
        altered.page.view_id += 1;
        assert!(state.finish(&altered, applied(&command)).is_none());
        assert!(state.pending());
        let mut altered = command.clone();
        let TransactionCommand::Write(request) = &mut altered.command else {
            unreachable!();
        };
        request.window.column_start = 2.into();
        assert!(state.finish(&altered, applied(&command)).is_none());
        assert!(state.pending());
        assert!(state.finish(&command, applied(&command)).is_some());
        assert!(!state.pending());
        assert!(state.finish(&command, applied(&command)).is_none());
    }

    #[test]
    fn accepted_receipt_refresh_drives_exact_undo_and_redo_heads() {
        let (mut state, page) = ready();
        let command = state.begin_paste(12, paste(&page)).unwrap();
        let Some(TransactionOutcome::Applied {
            refresh: Ok(refresh),
            ..
        }) = state.finish(&command, applied(&command))
        else {
            panic!("expected commit");
        };
        assert!(
            state.accepted_page().is_none(),
            "values need inspector acceptance"
        );
        state.observe_page(12, &refresh.page);
        let undo = state.begin_history(13, EditOperation::Undo).unwrap();
        let TransactionCommand::Undo(request) = &undo.command else {
            unreachable!();
        };
        assert_eq!(request.expected, refresh.page.token);
        assert_eq!(request.transaction_id.get(), 7);
        let Some(TransactionOutcome::Applied {
            refresh: Ok(refresh),
            ..
        }) = state.finish(&undo, applied(&undo))
        else {
            panic!("expected undo");
        };
        state.observe_page(13, &refresh.page);
        assert!(state.history().unwrap().undo.is_none());
        let redo = state.begin_history(14, EditOperation::Redo).unwrap();
        let TransactionCommand::Redo(request) = &redo.command else {
            unreachable!();
        };
        assert_eq!(request.expected.context.revision, 19);
        assert_eq!(request.history_epoch.get(), 4_294_967_297);
        assert!(state.finish(&redo, applied(&redo)).is_some());
    }

    #[test]
    fn committed_refresh_failure_preserves_history_without_authorizing_retry() {
        let (mut state, page) = ready();
        let command = state.begin_paste(12, paste(&page)).unwrap();
        let TransactionOutcome::Applied { receipt, .. } = applied(&command) else {
            unreachable!();
        };
        let outcome = TransactionOutcome::Applied {
            receipt,
            refresh: Err(TransactionFailure::rejected("refresh failed")),
        };
        assert!(matches!(
            state.finish(&command, outcome),
            Some(TransactionOutcome::Applied {
                refresh: Err(_),
                ..
            })
        ));
        assert!(state.history().unwrap().undo.is_some());
        assert!(state.quarantine().is_none());
        assert!(state.begin_paste(13, paste(&page)).is_err());
        assert!(state.begin_history(13, EditOperation::Undo).is_err());
        state.observe_page(13, &sample_page(18, 4));
        assert!(state.begin_history(14, EditOperation::Undo).is_ok());
    }

    #[test]
    fn known_connected_rejection_recovers_undo_and_redo_only_after_same_context_read() {
        for operation in [EditOperation::Undo, EditOperation::Redo] {
            let (mut state, page) = ready_history(operation);
            let heads = state.history().unwrap().clone();
            let rejected = state.begin_paste(14, paste(&page)).unwrap();
            assert!(matches!(
                state.finish(
                    &rejected,
                    TransactionOutcome::Failed(TransactionFailure::rejected(
                        "request.tmp creation failed before publication"
                    ))
                ),
                Some(TransactionOutcome::Failed(TransactionFailure {
                    uncertain: false,
                    disconnected: false,
                    ..
                }))
            ));
            assert!(!state.pending() && !state.connection_lost());
            assert!(state.quarantine().is_none());
            assert!(state.accepted_page().is_none());
            assert_eq!(state.context.as_ref(), Some(&page.token.context));
            assert_eq!(state.history(), Some(&heads));
            assert!(state.begin_paste(15, paste(&page)).is_err());
            assert!(state.begin_history(15, operation).is_err());
            assert!(!state.pending(), "rejection must not retry automatically");

            state.observe_page(16, &page);
            assert_eq!(state.history(), Some(&heads));
            let recovery = state.begin_history(17, operation).unwrap();
            let expected_head = match operation {
                EditOperation::Undo => heads.undo.as_ref().unwrap(),
                EditOperation::Redo => heads.redo.as_ref().unwrap(),
                EditOperation::Write => unreachable!(),
            };
            let request = match &recovery.command {
                TransactionCommand::Undo(request) if operation == EditOperation::Undo => request,
                TransactionCommand::Redo(request) if operation == EditOperation::Redo => request,
                _ => panic!("wrong recovered history operation"),
            };
            assert_eq!(request.expected, page.token);
            assert_eq!(request.history_epoch, heads.epoch);
            assert_eq!(request.transaction_id, expected_head.transaction_id);
        }
    }

    #[test]
    fn changed_context_read_retires_preserved_heads_after_known_connected_rejection() {
        for operation in [EditOperation::Undo, EditOperation::Redo] {
            for changed_component in 0..4 {
                let (mut state, page) = ready_history(operation);
                let heads = state.history().unwrap().clone();
                let rejected = state.begin_paste(14, paste(&page)).unwrap();
                state.finish(
                    &rejected,
                    TransactionOutcome::Failed(TransactionFailure::rejected(
                        "known precommit rejection",
                    )),
                );
                assert_eq!(state.history(), Some(&heads));
                let mut fresh = page.clone();
                match changed_component {
                    0 => fresh.token.context.revision += 1,
                    1 => fresh.token.context.invocation += 1,
                    2 => fresh.token.context.generation += 1,
                    3 => fresh.token.context.worker_id.push_str("-new"),
                    _ => unreachable!(),
                }
                state.observe_page(16, &fresh);
                assert_eq!(state.context.as_ref(), Some(&fresh.token.context));
                assert!(state.accepted_page().is_some());
                assert!(state.history().is_none());
                assert!(state.quarantine().is_none());
                assert!(state.begin_history(17, EditOperation::Undo).is_err());
                assert!(state.begin_history(17, EditOperation::Redo).is_err());
                assert!(!state.pending());
                assert!(state.begin_paste(18, paste(&fresh)).is_ok());
            }
        }
    }

    #[test]
    fn malformed_refresh_stays_stale_and_invalid_receipt_quarantines() {
        let (mut state, page) = ready();
        let command = state.begin_paste(12, paste(&page)).unwrap();
        let mut outcome = applied(&command);
        if let TransactionOutcome::Applied {
            refresh: Ok(refresh),
            ..
        } = &mut outcome
        {
            refresh.page.selection.higher_indices[0] = 2.into();
        }
        assert!(matches!(
            state.finish(&command, outcome),
            Some(TransactionOutcome::Applied {
                refresh: Err(_),
                ..
            })
        ));
        assert!(state.accepted_page().is_none());
        state.observe_page(13, &sample_page(18, 4));
        let command = state.begin_history(14, EditOperation::Undo).unwrap();
        let mut outcome = applied(&command);
        if let TransactionOutcome::Applied { receipt, .. } = &mut outcome {
            receipt.transaction_id = 99.into();
        }
        assert!(matches!(
            state.finish(&command, outcome),
            Some(TransactionOutcome::Failed(TransactionFailure {
                uncertain: true,
                ..
            }))
        ));
        assert!(state.quarantine().is_some());
        assert!(state.history().is_none());
    }

    #[test]
    fn uncertainty_survives_reads_and_only_valid_explicit_reset_or_restart_clears_it() {
        let (mut state, page) = ready();
        let command = state.begin_paste(12, paste(&page)).unwrap();
        state.finish(
            &command,
            TransactionOutcome::Failed(TransactionFailure {
                message: "commit outcome unknown".into(),
                uncertain: true,
                disconnected: false,
            }),
        );
        assert!(
            state.begin_reset(13).is_err(),
            "a stale page cannot authorize reset"
        );
        let fresh = sample_page(21, 4);
        state.observe_page(13, &fresh);
        assert!(state.writes_blocked());
        assert!(state.begin_paste(14, paste(&fresh)).is_err());
        let reset = state.begin_reset(14).unwrap();
        let after = sample_page(22, 4);
        assert!(matches!(
            state.finish(&reset, TransactionOutcome::Reset(snapshot(&after))),
            Some(TransactionOutcome::Reset(_))
        ));
        assert!(state.quarantine().is_none());
        assert!(state.history().is_none());
        assert!(state.accepted_page().is_none());
        state.restarted();
        assert!(state.quarantine().is_none());
    }

    #[test]
    fn ordinary_mutation_invalidates_history_and_late_outstanding_completion() {
        let (mut state, page) = ready();
        let command = state.begin_paste(12, paste(&page)).unwrap();
        state.invalidate_context();
        assert!(state.finish(&command, applied(&command)).is_none());
        assert!(
            state.quarantine().is_some(),
            "abandoned outstanding mutation needs explicit reconciliation"
        );
        state.restarted();
        state.observe_page(11, &page);
        let command = state.begin_paste(12, paste(&page)).unwrap();
        state.finish(&command, applied(&command));
        assert!(state.history().is_some());
        state.observe_page(20, &sample_page(99, 4));
        assert!(
            state.history().is_none(),
            "an unrelated new revision retires receipt heads"
        );
    }

    #[test]
    fn disconnected_pending_edit_settles_receipt_without_reviving_history_or_page() {
        let (mut state, page) = ready();
        let command = state.begin_paste(12, paste(&page)).unwrap();
        state.disconnected();
        assert!(state.pending() && state.connection_lost());
        let mut wrong_echo = command.clone();
        wrong_echo.page.view_id += 1;
        assert!(state.finish(&wrong_echo, applied(&command)).is_none());
        assert!(state.pending());
        let nominal_success = applied(&command);
        let expected_token = match &nominal_success {
            TransactionOutcome::Applied {
                receipt,
                refresh: Ok(_),
                ..
            } => receipt.token.clone(),
            _ => unreachable!(),
        };
        let Some(TransactionOutcome::Applied {
            receipt,
            refresh: Err(failure),
        }) = state.finish(&command, nominal_success)
        else {
            panic!("a valid offline receipt must remain a known commit");
        };
        assert_eq!(receipt.token, expected_token);
        assert!(failure.disconnected && !failure.uncertain);
        assert!(!state.pending() && state.connection_lost() && state.writes_blocked());
        assert!(state.accepted_page().is_none() && state.history().is_none());
        assert!(state.quarantine().is_some());
        state.observe_page(13, &sample_page(18, 4));
        assert!(
            state.accepted_page().is_none(),
            "a late read cannot reopen the session"
        );
        assert!(state.begin_reset(14).is_err());
        assert!(state.begin_paste(14, paste(&sample_page(18, 4))).is_err());
        assert!(state.finish(&command, applied(&command)).is_none());
    }

    #[test]
    fn disconnect_keeps_late_reset_offline_and_restart_rejects_obsolete_reply() {
        let (mut state, _) = ready();
        let reset = state.begin_reset(12).unwrap();
        state.disconnected();
        let after = sample_page(18, 4);
        assert!(matches!(
            state.finish(&reset, TransactionOutcome::Reset(snapshot(&after))),
            Some(TransactionOutcome::Failed(TransactionFailure {
                uncertain: false,
                disconnected: true,
                ..
            }))
        ));
        assert!(state.connection_lost() && state.quarantine().is_some());
        assert!(state.history().is_none() && state.accepted_page().is_none());
        state.restarted();
        let mut fresh = sample_page(0, 4);
        fresh.token.context.generation += 1;
        state.observe_page(20, &fresh);
        let current = state.begin_paste(21, paste(&fresh)).unwrap();
        assert!(state
            .finish(&reset, TransactionOutcome::Reset(snapshot(&after)))
            .is_none());
        assert!(state.pending() && !state.connection_lost());
        assert!(matches!(
            state.finish(&current, applied(&current)),
            Some(TransactionOutcome::Applied { .. })
        ));
        assert!(!state.connection_lost() && state.quarantine().is_none());
    }
}
