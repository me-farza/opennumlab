// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in located-AST execution. No native process, source reparsing or fallback.
#![forbid(unsafe_code)]
// Static private extension points. Existing profiles expand the original
// Task/Mode definitions and source/value operations without widened enums.
macro_rules! declare_machine_tasks{($($variants:tt)*)=>{enum Task{$($variants)*}}}
macro_rules! declare_machine_modes{($($variants:tt)*)=>{#[derive(Clone,Copy)]enum Mode{$($variants)*}}}
macro_rules! install_machine_syntax{()=>{
 fn location(&self,id:NodeIdV1)->ProgramLocationV1{ProgramLocationV1{source_id:self.module.source_id(),span:self.module.node(id).unwrap().span}}
 fn kind(&self,id:NodeIdV1)->N{self.module.node(id).unwrap().kind}
 fn text(&self,id:NodeIdV1)->&str{self.module.source_text(self.module.node(id).unwrap().span).unwrap()}
 fn is_full_axis(&self,id:NodeIdV1)->bool{matches!(self.kind(id),N::FullAxis)}
};}
macro_rules! initial_profile_location{($module:expr,$id:expr)=>{ProgramLocationV1{source_id:$module.source_id(),span:$module.node($id).unwrap().span}}}
macro_rules! profile_extra_live{($owner:expr)=>{0usize}}
macro_rules! profile_extra_metadata{($owner:expr)=>{0usize}}
macro_rules! profile_admit_kind{($node:expr)=>{Some($node.kind)}}
macro_rules! profile_name_slice{($m:expr,$id:expr,$start:expr,$end:expr)=>{&$m.module.source()[$start..$end]}}
macro_rules! profile_function_visible{($m:expr,$id:expr)=>{true}}
macro_rules! profile_before_return{($m:expr)=>{}}
macro_rules! profile_leaf_task{($m:expr,$node:expr,$base:expr)=>{$m.current=$node;$m.finish_leaf($node,$base).await?;$m.complete()?;}}
macro_rules! profile_dispatch_task{($m:expr,$task:expr)=>{$m.step($task).await?;}}
macro_rules! profile_metadata_admission{($m:expr,$bytes:expr)=>{{$m.metadata_room($bytes)?;}}}
macro_rules! profile_release_value{($m:expr,$value:expr)=>{{$m.live-=$value.payload_bytes();}}}
// Legacy copies require no object leases or enumeration state across await.
macro_rules! profile_copy_return_values{($m:expr,$results:ident,$next:ident,$delivered:expr)=>{{
 for slot in $results.iter_mut().take($delivered){
  let(id,tail)=$m.list($next.ok_or(ProgramErrorCodeV1::Arity)?);$next=tail;
  let source=$m.text(id);let binding=$m.binding_index(source).await.ok_or(ProgramErrorCodeV1::UnassignedOutput)?;
  *slot=Some($m.copy_binding(binding).await?);
 }
}}}
macro_rules! profile_root_value {($m:expr,$value:expr)=>{()}}
macro_rules! profile_register_incoming{($m:expr,$incoming:expr)=>{()}}
macro_rules! profile_builtin{($m:expr,$site:expr,$id:expr,$base:expr,$count:expr,$requested:expr)=>{$m.finish_builtin($site,$id,$base,$count)}}
macro_rules! profile_start{($m:expr)=>{}}
macro_rules! profile_before_exec{($m:expr,$id:expr)=>{}}
macro_rules! profile_before_eval{($m:expr,$id:expr,$requested:expr,$extent:expr)=>{}}
// Diagnostic static exhaustive matches: old variants have no extension arm.
macro_rules! profile_match_task{($m:expr,$task:expr;$($arms:tt)*)=>{match $task{$($arms)*}}}
macro_rules! profile_match_mode{($m:expr,$mode:expr,$base:expr,$count:expr,$requested:expr,$site:expr;$($arms:tt)*)=>{match $mode{$($arms)*}}}
macro_rules! profile_extra_task{($m:expr,$task:expr)=>{Err::<(),ProgramErrorCodeV1>(ProgramErrorCodeV1::UnsupportedFeature)}}
macro_rules! profile_extra_mode{($m:expr,$mode:expr,$base:expr,$count:expr,$requested:expr,$site:expr)=>{Err::<(),ProgramErrorCodeV1>(ProgramErrorCodeV1::UnsupportedFeature)}}
macro_rules! profile_event_copy{($m:expr,$v:expr)=>{$m.copy($v)}}
macro_rules! profile_event_move{($m:expr,$v:expr)=>{$v}}
macro_rules! profile_prepare_publication{($m:expr,$writes:expr,$external:expr)=>{()}}
macro_rules! profile_commit_publication{($m:expr,$prepared:expr)=>{{let _=$prepared;}}}
macro_rules! profile_finalize_stats{($m:expr)=>{{}}}
macro_rules! profile_on_drop{($m:expr)=>{{}}}
mod profile;
use profile::install_legacy as install_value_profile;
type ProgramAxes = [bool; 2];
const PROFILE_LIVE_BYTES: usize = 1_048_576;
const PROFILE_INDEX_AXES: usize = 2;
pub mod scientific;
pub mod scientific_v2;
pub mod class;
mod machine;
#[cfg(test)]
mod tests;
use mathlab_language::program::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
    time::Instant,
};
pub const PROGRAM_PROFILE_V1: &str = "rust-program-execution-v1";
const RUNNING: u8 = 0;
const PUBLISHING: u8 = 1;
const STOP: u8 = 2;
const DEADLINE: u8 = 3;
const PENDING_STOP: u8 = 4;
const PENDING_DEADLINE: u8 = 5;
const DONE: u8 = 6;
const FAILED: u8 = 7;
const ABANDONED: u8 = 8;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramRequestKeyV1 {
    pub session_id: u64,
    pub epoch: u64,
    pub request_id: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramContextV1 {
    pub session_id: u64,
    pub epoch: u64,
    pub revision: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramCancelReasonV1 {
    UserStop,
    Deadline,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramCancelResultV1 {
    Accepted(ProgramCancelReasonV1),
    AlreadyAccepted(ProgramCancelReasonV1),
    TooLate,
    WrongKey,
}
#[derive(Debug, Clone)]
pub struct ProgramCancelHandleV1 {
    key: ProgramRequestKeyV1,
    state: Arc<AtomicU8>,
}
impl ProgramCancelHandleV1 {
    pub fn key(&self) -> ProgramRequestKeyV1 {
        self.key
    }
    pub fn request_stop(&self) -> ProgramCancelResultV1 {
        self.cancel_for(self.key, ProgramCancelReasonV1::UserStop)
    }
    pub fn cancel_for(
        &self,
        key: ProgramRequestKeyV1,
        reason: ProgramCancelReasonV1,
    ) -> ProgramCancelResultV1 {
        if key != self.key {
            return ProgramCancelResultV1::WrongKey;
        }
        let mut old = self.state.load(Ordering::Acquire);
        loop {
            let target = match old {
                RUNNING => {
                    if reason == ProgramCancelReasonV1::UserStop {
                        STOP
                    } else {
                        DEADLINE
                    }
                }
                PUBLISHING => {
                    if reason == ProgramCancelReasonV1::UserStop {
                        PENDING_STOP
                    } else {
                        PENDING_DEADLINE
                    }
                }
                STOP | PENDING_STOP => {
                    return ProgramCancelResultV1::AlreadyAccepted(ProgramCancelReasonV1::UserStop);
                }
                DEADLINE | PENDING_DEADLINE => {
                    return ProgramCancelResultV1::AlreadyAccepted(ProgramCancelReasonV1::Deadline);
                }
                _ => return ProgramCancelResultV1::TooLate,
            };
            match self
                .state
                .compare_exchange_weak(old, target, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return ProgramCancelResultV1::Accepted(reason),
                Err(now) => old = now,
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramErrorCodeV1 {
    UnknownBinding,
    Arity,
    UnassignedOutput,
    Type,
    Shape,
    Index,
    UnsupportedFeature,
    Allocation,
    Resource,
    CounterExhausted,
    Cancelled,
    Value(ProgramValueErrorV1),
}
impl ProgramErrorCodeV1 {
    pub fn is_allocation(&self) -> bool {
        matches!(self, Self::Allocation) || matches!(self,Self::Value(v) if v.is_allocation())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramLocationV1 {
    pub source_id: u64,
    pub span: ProgramSpanV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramErrorV1 {
    pub code: ProgramErrorCodeV1,
    pub location: ProgramLocationV1,
    pub call_stack: [Option<ProgramLocationV1>; 32],
    pub call_depth: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramCheckpointV1 {
    Task,
    BeforePublish,
    AfterBarrier,
    AfterPublish,
    BeforeTerminal,
    AfterTerminal,
}
#[derive(Debug, Clone, Copy)]
pub struct ProgramExecutionOptionsV1 {
    pub deadline: Option<Instant>,
    pub transition_fuel: u64,
    pub primitive_fuel: u64,
}
impl Default for ProgramExecutionOptionsV1 {
    fn default() -> Self {
        Self {
            deadline: None,
            transition_fuel: 1_000_000,
            primitive_fuel: 100_000_000,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramStatusV1 {
    Success,
    Error,
    Cancelled(ProgramCancelReasonV1),
    Resource,
}
#[derive(Debug, Default, Clone, Copy)]
pub struct ProgramExecutionStatsV1 {
    pub transitions: u64,
    pub primitive_steps: u64,
    pub peak_live_payload_bytes: usize,
    pub peak_metadata_bytes: usize,
}
#[derive(Debug)]
pub struct ProgramEventV1 {
    pub sequence: u64,
    pub location: ProgramLocationV1,
    pub function: Option<NodeIdV1>,
    pub name: Option<String>,
    pub value: ProgramValueV1,
}
#[derive(Debug)]
pub struct ProgramOutcomeV1 {
    pub profile: &'static str,
    pub key: ProgramRequestKeyV1,
    pub start: ProgramContextV1,
    pub final_context: ProgramContextV1,
    pub committed_effect_sequence: u64,
    pub completed_leaf_statement_count: u64,
    pub stats: ProgramExecutionStatsV1,
    pub events: Vec<ProgramEventV1>,
    pub status: ProgramStatusV1,
    pub error: Option<ProgramErrorV1>,
}
struct Binding {
    name: String,
    value: ProgramValueV1,
}
pub struct ProgramSessionV1 {
    session_id: u64,
    epoch: u64,
    request_id: u64,
    revision: u64,
    bindings: Vec<Binding>,
    #[cfg(test)]
    fail_allocation: Option<usize>,
}
/// Borrowed read-only workspace entry; no mutable storage or value copy escapes.
pub struct ProgramBindingViewV1<'a> {
    pub name: &'a str,
    pub value: &'a ProgramValueV1,
}
impl ProgramSessionV1 {
    pub fn new() -> Result<Self, ProgramErrorCodeV1> {
        let session_id = NEXT_ID
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| ProgramErrorCodeV1::CounterExhausted)?;
        let mut bindings = Vec::new();
        bindings
            .try_reserve_exact(64)
            .map_err(|_| ProgramErrorCodeV1::Allocation)?;
        if bindings.capacity() > 64 {
            return Err(ProgramErrorCodeV1::Resource);
        }
        Ok(Self {
            session_id,
            epoch: 1,
            request_id: 0,
            revision: 0,
            bindings,
            #[cfg(test)]
            fail_allocation: None,
        })
    }
    pub fn context(&self) -> ProgramContextV1 {
        ProgramContextV1 {
            session_id: self.session_id,
            epoch: self.epoch,
            revision: self.revision,
        }
    }
    pub fn bindings(&self) -> impl ExactSizeIterator<Item = ProgramBindingViewV1<'_>> {
        self.bindings.iter().map(|b| ProgramBindingViewV1 {
            name: &b.name,
            value: &b.value,
        })
    }
    pub fn read_binding(&self, name: &str) -> Result<Option<ProgramValueV1>, ProgramValueErrorV1> {
        self.bindings
            .iter()
            .find(|b| b.name == name)
            .map(|b| b.value.try_copy())
            .transpose()
    }
    pub fn reset(&mut self) -> Result<(), ProgramErrorCodeV1> {
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or(ProgramErrorCodeV1::CounterExhausted)?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(ProgramErrorCodeV1::CounterExhausted)?;
        self.bindings.clear();
        self.epoch = epoch;
        self.revision = revision;
        Ok(())
    }
    pub fn prepare(
        &mut self,
        module: Arc<ProgramModuleV1>,
        options: ProgramExecutionOptionsV1,
    ) -> Result<PreparedProgramRequestV1<'_>, ProgramErrorCodeV1> {
        if options.transition_fuel > 1_000_000 || options.primitive_fuel > 100_000_000 {
            return Err(ProgramErrorCodeV1::Resource);
        }
        let request_id = self
            .request_id
            .checked_add(1)
            .ok_or(ProgramErrorCodeV1::CounterExhausted)?;
        #[cfg(test)]
        if self.fail_allocation == Some(0) {
            return Err(ProgramErrorCodeV1::Allocation);
        }
        // The single stable-std Arc allocation is qualified separately from fallible payload storage.
        self.request_id = request_id;
        let handle = ProgramCancelHandleV1 {
            key: ProgramRequestKeyV1 {
                session_id: self.session_id,
                epoch: self.epoch,
                request_id,
            },
            state: Arc::new(AtomicU8::new(RUNNING)),
        };
        Ok(PreparedProgramRequestV1 {
            owner: self,
            module,
            options,
            handle,
            invoked: false,
        })
    }
}
pub struct PreparedProgramRequestV1<'a> {
    owner: &'a mut ProgramSessionV1,
    module: Arc<ProgramModuleV1>,
    options: ProgramExecutionOptionsV1,
    handle: ProgramCancelHandleV1,
    invoked: bool,
}
impl PreparedProgramRequestV1<'_> {
    pub fn cancel_handle(&self) -> ProgramCancelHandleV1 {
        self.handle.clone()
    }
    pub fn execute(self) -> ProgramOutcomeV1 {
        self.execute_controlled(|_, _| {})
    }
    pub fn execute_controlled(
        mut self,
        mut checkpoint: impl FnMut(ProgramCheckpointV1, &ProgramCancelHandleV1),
    ) -> ProgramOutcomeV1 {
        self.invoked = true;
        machine::execute(
            self.owner,
            Arc::clone(&self.module),
            self.options,
            &self.handle,
            &mut checkpoint,
        )
    }
}
impl Drop for PreparedProgramRequestV1<'_> {
    fn drop(&mut self) {
        if !self.invoked {
            let _ = self.handle.state.compare_exchange(
                RUNNING,
                ABANDONED,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
    }
}

type ProgramCheckpointSink<'a>=&'a mut dyn FnMut(ProgramCheckpointV1,&ProgramCancelHandleV1);
fn checkpoint_sink<'a>(callback:ProgramCheckpointSink<'a>)->ProgramCheckpointSink<'a>{callback}
