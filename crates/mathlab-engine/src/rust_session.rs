// SPDX-License-Identifier: GPL-3.0-or-later
//! Original bounded in-process Rust workspace; no Octave process or fallback.
#![forbid(unsafe_code)]

use mathlab_language::session_expression::{
    SessionExpressionErrorV1, SessionStatementEffectV1, SessionValueErrorV1, SessionValueV1,
    admit_session_expression_v1, evaluate_session_expression_v1, parse_session_expression_v1,
};
use std::collections::TryReserveError;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::Instant;

pub const RUST_SESSION_PROFILE_V1: &str = "rust-session-arithmetic-v1";
pub const MAX_WORKSPACE_BINDINGS_V1: usize = 64;
pub const MAX_WORKSPACE_PAYLOAD_BYTES_V1: usize = 131072;
static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);
const RUNNING: u8 = 0;
const CANCEL_USER: u8 = 1;
const CANCEL_DEADLINE: u8 = 2;
const COMMITTED: u8 = 3;
const FAILED: u8 = 4;
const CANCELLED_USER: u8 = 5;
const CANCELLED_DEADLINE: u8 = 6;
const ABANDONED: u8 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RustSessionContextV1 {
    pub session_id: u64,
    pub epoch: u64,
    pub workspace_revision: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RustRequestKeyV1 {
    pub session_id: u64,
    pub epoch: u64,
    pub request_id: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustCancelReasonV1 {
    UserStop,
    Deadline,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustCancelRequestV1 {
    Accepted(RustCancelReasonV1),
    AlreadyAccepted(RustCancelReasonV1),
    TooLate,
    WrongKey,
}

#[derive(Debug, Clone)]
pub struct RustCancelHandleV1 {
    key: RustRequestKeyV1,
    state: Arc<AtomicU8>,
}
impl RustCancelHandleV1 {
    pub fn key(&self) -> RustRequestKeyV1 {
        self.key
    }
    pub fn request_stop(&self) -> RustCancelRequestV1 {
        self.cancel_for(self.key, RustCancelReasonV1::UserStop)
    }
    pub fn cancel_for(
        &self,
        key: RustRequestKeyV1,
        reason: RustCancelReasonV1,
    ) -> RustCancelRequestV1 {
        if key != self.key {
            return RustCancelRequestV1::WrongKey;
        }
        let target = match reason {
            RustCancelReasonV1::UserStop => CANCEL_USER,
            RustCancelReasonV1::Deadline => CANCEL_DEADLINE,
        };
        match self
            .state
            .compare_exchange(RUNNING, target, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => RustCancelRequestV1::Accepted(reason),
            Err(state) => match cancellation_reason(state) {
                Some(reason) => RustCancelRequestV1::AlreadyAccepted(reason),
                None => RustCancelRequestV1::TooLate,
            },
        }
    }
}
fn cancellation_reason(state: u8) -> Option<RustCancelReasonV1> {
    match state {
        CANCEL_USER | CANCELLED_USER => Some(RustCancelReasonV1::UserStop),
        CANCEL_DEADLINE | CANCELLED_DEADLINE => Some(RustCancelReasonV1::Deadline),
        _ => None,
    }
}
fn poll_control(handle: &RustCancelHandleV1, deadline: Option<Instant>) -> bool {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        handle.cancel_for(handle.key, RustCancelReasonV1::Deadline);
    }
    matches!(
        handle.state.load(Ordering::Acquire),
        CANCEL_USER | CANCEL_DEADLINE
    )
}

#[derive(Debug)]
pub enum RustSessionErrorV1 {
    Language(SessionExpressionErrorV1),
    ValueCopy(SessionValueErrorV1),
    Allocation {
        site: &'static str,
        cause: TryReserveError,
    },
    WorkspaceLimit,
    WrongSession,
    StaleEpoch,
    CounterExhausted,
}
impl From<SessionExpressionErrorV1> for RustSessionErrorV1 {
    fn from(error: SessionExpressionErrorV1) -> Self {
        Self::Language(error)
    }
}
impl From<SessionValueErrorV1> for RustSessionErrorV1 {
    fn from(error: SessionValueErrorV1) -> Self {
        Self::ValueCopy(error)
    }
}
impl fmt::Display for RustSessionErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RustSessionErrorV1 {}
#[derive(Debug, PartialEq, Eq)]
pub enum RustBindingEffectV1 {
    None,
    Write(String),
}
#[derive(Debug)]
pub struct RustEvalSuccessV1 {
    pub backend: &'static str,
    pub profile: &'static str,
    pub key: RustRequestKeyV1,
    pub before: RustSessionContextV1,
    pub after: RustSessionContextV1,
    pub effect: RustBindingEffectV1,
    pub value: SessionValueV1,
    pub display_name: Option<String>,
    pub source: Arc<str>,
}
#[derive(Debug)]
pub enum RustEvalOutcomeV1 {
    Success(RustEvalSuccessV1),
    Error {
        key: RustRequestKeyV1,
        context: RustSessionContextV1,
        error: RustSessionErrorV1,
    },
    Cancelled {
        key: RustRequestKeyV1,
        context: RustSessionContextV1,
        reason: RustCancelReasonV1,
    },
}
#[derive(Debug, Clone, Copy)]
pub struct RustBindingMetadataV1<'a> {
    pub name: &'a str,
    pub class: &'static str,
    pub shape: [u64; 2],
    pub context: RustSessionContextV1,
}
/// Independent bounded value with the session context captured at read time.
#[derive(Debug)]
pub struct RustReadSnapshotV1 {
    pub context: RustSessionContextV1,
    pub value: SessionValueV1,
}
struct Binding {
    name: String,
    value: SessionValueV1,
}
pub struct RustSessionV1 {
    session_id: u64,
    epoch: u64,
    revision: u64,
    request_id: u64,
    bindings: Vec<Binding>,
    retained_bytes: usize,
    #[cfg(test)]
    hook: Option<Box<dyn FnMut(Checkpoint, &RustCancelHandleV1)>>,
    #[cfg(test)]
    fail_reservation: Option<ReservationSite>,
}

/// Holding this guard borrows the sole owner. Drop before execute closes the
/// token and releases that borrow; no request queue or stuck pending flag exists.
pub struct PreparedRustRequestV1<'a> {
    owner: &'a mut RustSessionV1,
    handle: RustCancelHandleV1,
    source: Arc<str>,
    deadline: Option<Instant>,
    invoked: bool,
}
impl PreparedRustRequestV1<'_> {
    pub fn cancel_handle(&self) -> RustCancelHandleV1 {
        self.handle.clone()
    }
    pub fn execute(mut self) -> RustEvalOutcomeV1 {
        self.invoked = true;
        self.owner
            .execute_inner(&self.handle, Arc::clone(&self.source), self.deadline)
    }
}
impl Drop for PreparedRustRequestV1<'_> {
    fn drop(&mut self) {
        if !self.invoked {
            self.handle.state.store(ABANDONED, Ordering::Release);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReservationSite {
    Name,
    Slot,
    Response,
}
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Checkpoint {
    BeforeParse,
    AfterParse,
    AfterAdmission,
    Task,
    AfterEvaluation,
    BeforeCommit,
    AfterCommitTransition,
    BeforeError,
}
#[cfg(test)]
fn fire_hook(
    hook: &mut Option<Box<dyn FnMut(Checkpoint, &RustCancelHandleV1)>>,
    at: Checkpoint,
    handle: &RustCancelHandleV1,
) {
    if let Some(hook) = hook {
        hook(at, handle);
    }
}

impl RustSessionV1 {
    pub fn new() -> Result<Self, RustSessionErrorV1> {
        let session_id = NEXT_SESSION_ID
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| RustSessionErrorV1::CounterExhausted)?;
        Ok(Self {
            session_id,
            epoch: 1,
            revision: 0,
            request_id: 0,
            bindings: Vec::new(),
            retained_bytes: 0,
            #[cfg(test)]
            hook: None,
            #[cfg(test)]
            fail_reservation: None,
        })
    }
    pub fn context(&self) -> RustSessionContextV1 {
        RustSessionContextV1 {
            session_id: self.session_id,
            epoch: self.epoch,
            workspace_revision: self.revision,
        }
    }
    pub fn workspace(&self) -> impl Iterator<Item = RustBindingMetadataV1<'_>> {
        let context = self.context();
        self.bindings
            .iter()
            .map(move |binding| RustBindingMetadataV1 {
                name: &binding.name,
                class: binding.value.class(),
                shape: binding.value.shape(),
                context,
            })
    }
    pub fn read(&self, name: &str) -> Result<Option<RustReadSnapshotV1>, RustSessionErrorV1> {
        let context = self.context();
        self.bindings
            .iter()
            .find(|b| b.name == name)
            .map(|b| {
                Ok(RustReadSnapshotV1 {
                    context,
                    value: b.value.try_copy()?,
                })
            })
            .transpose()
    }
    pub fn reset(&mut self) -> Result<RustSessionContextV1, RustSessionErrorV1> {
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or(RustSessionErrorV1::CounterExhausted)?;
        self.bindings.clear();
        self.retained_bytes = 0;
        self.revision = 0;
        self.epoch = epoch;
        Ok(self.context())
    }
    /// Revision is observational here; only session/epoch authorize this owner.
    /// User execution is not a revision-checked value-edit transaction.
    pub fn prepare_at(
        &mut self,
        expected: RustSessionContextV1,
        source: Arc<str>,
        deadline: Option<Instant>,
    ) -> Result<PreparedRustRequestV1<'_>, RustSessionErrorV1> {
        if expected.session_id != self.session_id {
            return Err(RustSessionErrorV1::WrongSession);
        }
        if expected.epoch != self.epoch {
            return Err(RustSessionErrorV1::StaleEpoch);
        }
        let request_id = self
            .request_id
            .checked_add(1)
            .ok_or(RustSessionErrorV1::CounterExhausted)?;
        self.request_id = request_id;
        let key = RustRequestKeyV1 {
            session_id: self.session_id,
            epoch: self.epoch,
            request_id,
        };
        Ok(PreparedRustRequestV1 {
            owner: self,
            handle: RustCancelHandleV1 {
                key,
                state: Arc::new(AtomicU8::new(RUNNING)),
            },
            source,
            deadline,
            invoked: false,
        })
    }
    pub fn prepare(
        &mut self,
        source: Arc<str>,
        deadline: Option<Instant>,
    ) -> Result<PreparedRustRequestV1<'_>, RustSessionErrorV1> {
        self.prepare_at(self.context(), source, deadline)
    }
    fn reserve_site(&mut self, site: ReservationSite) -> Result<(), RustSessionErrorV1> {
        #[cfg(test)]
        if self.fail_reservation == Some(site) {
            self.fail_reservation = None;
            // Capacity overflow is deterministic and never attempts huge allocation.
            let cause = Vec::<u8>::new().try_reserve(usize::MAX).unwrap_err();
            return Err(RustSessionErrorV1::Allocation {
                site: "injected preparation reservation",
                cause,
            });
        }
        let _ = site;
        Ok(())
    }
    fn finish_cancel(&self, handle: &RustCancelHandleV1) -> RustEvalOutcomeV1 {
        let state = handle.state.load(Ordering::Acquire);
        let (reason, terminal) = match state {
            CANCEL_USER => (RustCancelReasonV1::UserStop, CANCELLED_USER),
            CANCEL_DEADLINE => (RustCancelReasonV1::Deadline, CANCELLED_DEADLINE),
            _ => unreachable!("only the owner completes one accepted cancellation"),
        };
        handle.state.store(terminal, Ordering::Release);
        RustEvalOutcomeV1::Cancelled {
            key: handle.key,
            context: self.context(),
            reason,
        }
    }
    fn finish_error(
        &mut self,
        handle: &RustCancelHandleV1,
        deadline: Option<Instant>,
        error: RustSessionErrorV1,
    ) -> RustEvalOutcomeV1 {
        #[cfg(test)]
        fire_hook(&mut self.hook, Checkpoint::BeforeError, handle);
        if poll_control(handle, deadline) {
            return self.finish_cancel(handle);
        }
        match handle
            .state
            .compare_exchange(RUNNING, FAILED, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => RustEvalOutcomeV1::Error {
                key: handle.key,
                context: self.context(),
                error,
            },
            Err(CANCEL_USER | CANCEL_DEADLINE) => self.finish_cancel(handle),
            Err(_) => unreachable!("a consumed request has exactly one terminal owner"),
        }
    }
    fn execute_inner(
        &mut self,
        handle: &RustCancelHandleV1,
        source: Arc<str>,
        deadline: Option<Instant>,
    ) -> RustEvalOutcomeV1 {
        macro_rules! attempt {
            ($expression:expr) => {
                match $expression {
                    Ok(value) => value,
                    Err(error) => return self.finish_error(handle, deadline, error.into()),
                }
            };
        }
        #[cfg(test)]
        fire_hook(&mut self.hook, Checkpoint::BeforeParse, handle);
        if poll_control(handle, deadline) {
            return self.finish_cancel(handle);
        }
        let parsed = attempt!(parse_session_expression_v1(Arc::clone(&source)));
        #[cfg(test)]
        fire_hook(&mut self.hook, Checkpoint::AfterParse, handle);
        if poll_control(handle, deadline) {
            return self.finish_cancel(handle);
        }
        let ir = attempt!(admit_session_expression_v1(parsed));
        #[cfg(test)]
        fire_hook(&mut self.hook, Checkpoint::AfterAdmission, handle);
        if poll_control(handle, deadline) {
            return self.finish_cancel(handle);
        }
        let evaluated = {
            let bindings = &self.bindings;
            #[cfg(test)]
            let hook = &mut self.hook;
            evaluate_session_expression_v1(
                ir,
                |name| bindings.iter().find(|b| b.name == name).map(|b| &b.value),
                || {
                    #[cfg(test)]
                    fire_hook(hook, Checkpoint::Task, handle);
                    poll_control(handle, deadline)
                },
            )
        };
        let evaluated = attempt!(evaluated);
        #[cfg(test)]
        fire_hook(&mut self.hook, Checkpoint::AfterEvaluation, handle);
        if poll_control(handle, deadline) {
            return self.finish_cancel(handle);
        }
        let before = self.context();
        let target = match &evaluated.effect {
            SessionStatementEffectV1::Assignment { target } => {
                Some(&source[target.start..target.end])
            }
            SessionStatementEffectV1::Answer => Some("ans"),
            SessionStatementEffectV1::Display { .. } => None,
        };
        let display = match &evaluated.effect {
            SessionStatementEffectV1::Display { name } => &source[name.start..name.end],
            _ => target.expect("assignment or expression has a target"),
        };
        let index = target.and_then(|name| self.bindings.iter().position(|b| b.name == name));
        if target.is_some() && index.is_none() && self.bindings.len() == MAX_WORKSPACE_BINDINGS_V1 {
            return self.finish_error(handle, deadline, RustSessionErrorV1::WorkspaceLimit);
        }
        let old_bytes = index.map_or(0, |i| self.bindings[i].value.payload_bytes());
        let next_bytes = if target.is_some() {
            match self
                .retained_bytes
                .checked_sub(old_bytes)
                .and_then(|n| n.checked_add(evaluated.value.payload_bytes()))
            {
                Some(n) if n <= MAX_WORKSPACE_PAYLOAD_BYTES_V1 => n,
                _ => {
                    return self.finish_error(handle, deadline, RustSessionErrorV1::WorkspaceLimit);
                }
            }
        } else {
            self.retained_bytes
        };
        let revision = if target.is_some() {
            match self.revision.checked_add(1) {
                Some(n) => n,
                None => {
                    return self.finish_error(
                        handle,
                        deadline,
                        RustSessionErrorV1::CounterExhausted,
                    );
                }
            }
        } else {
            self.revision
        };
        attempt!(self.reserve_site(ReservationSite::Name));
        let staged_name = if index.is_none() {
            target.map(owned_name).transpose()
        } else {
            Ok(None)
        };
        let staged_name = attempt!(staged_name);
        let effect = match target {
            Some(name) => RustBindingEffectV1::Write(attempt!(owned_name(name))),
            None => RustBindingEffectV1::None,
        };
        let display_name = if evaluated.suppressed {
            None
        } else {
            Some(attempt!(owned_name(display)))
        };
        if target.is_some() && index.is_none() {
            attempt!(self.reserve_site(ReservationSite::Slot));
            attempt!(self.bindings.try_reserve(1).map_err(|cause| {
                RustSessionErrorV1::Allocation {
                    site: "binding slot",
                    cause,
                }
            }));
        }
        attempt!(self.reserve_site(ReservationSite::Response));
        let binding_value = if target.is_some() {
            Some(attempt!(evaluated.value.try_copy()))
        } else {
            None
        };
        #[cfg(test)]
        fire_hook(&mut self.hook, Checkpoint::BeforeCommit, handle);
        if poll_control(handle, deadline) {
            return self.finish_cancel(handle);
        }
        match handle
            .state
            .compare_exchange(RUNNING, COMMITTED, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => {}
            Err(CANCEL_USER | CANCEL_DEADLINE) => return self.finish_cancel(handle),
            Err(_) => unreachable!("only the owner commits a consumed request"),
        }
        #[cfg(test)]
        fire_hook(&mut self.hook, Checkpoint::AfterCommitTransition, handle);
        // All fallible work and user-visible decisions precede this publication.
        if let Some(value) = binding_value {
            if let Some(index) = index {
                self.bindings[index].value = value;
            } else {
                self.bindings.push(Binding {
                    name: staged_name.expect("prepared name"),
                    value,
                });
            }
            self.retained_bytes = next_bytes;
            self.revision = revision;
        }
        RustEvalOutcomeV1::Success(RustEvalSuccessV1 {
            backend: "rust",
            profile: RUST_SESSION_PROFILE_V1,
            key: handle.key,
            before,
            after: self.context(),
            effect,
            value: evaluated.value,
            display_name,
            source,
        })
    }
}
fn owned_name(name: &str) -> Result<String, RustSessionErrorV1> {
    let mut result = String::new();
    result
        .try_reserve_exact(name.len())
        .map_err(|cause| RustSessionErrorV1::Allocation {
            site: "name",
            cause,
        })?;
    result.push_str(name);
    Ok(result)
}

#[cfg(test)]
mod tests;
