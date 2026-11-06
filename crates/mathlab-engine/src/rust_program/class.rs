// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit class profile; shared located syntax/control, typed object heap.
use super::*;
use mathlab_language::program::{ClassSourceV1,ClassProgramGraphV1,ClassNodeKindV1,ScientificProgramValueV2,ScientificStorageViewV2,ScientificDenseV1,ScientificShapeV1,ScientificCscV1,ScientificCscShapeV1,ScientificCscValuesV1,ScientificTripletValuesV1,ScientificErrorV1,ScientificControlV1,ScientificLeafOperationV1,ScientificSolveDiagnosticsV1,Complex64V1,evaluate_scientific_dense_v2,yield_scientific_v1};
mod value;mod heap;mod operations;
use operations::TemporaryMetadata;
pub use value::*;
const PROGRAM_PROFILE_V1:&str="class-v1";const PROFILE_LIVE_BYTES:usize=64<<20;const PROFILE_INDEX_AXES:usize=8;
type ProgramAxes=[bool;8];type ProgramValueV1=ClassValueV1;type ProgramModuleV1=ClassProgramGraphV1;
type ProgramErrorCodeV1=ClassProgramErrorCodeV1;type ProgramErrorV1=ClassProgramErrorV1;type ProgramExecutionOptionsV1=ClassProgramOptionsV1;type ProgramExecutionStatsV1=ClassProgramStatsV1;type ProgramEventV1=ClassProgramEventV1;type ProgramOutcomeV1=ClassProgramOutcomeV1;type ProgramSessionV1=ClassProgramSessionV1;
// Exact accepted V2 numeric provider/ledger bodies are compiled into this
// specialization; no duplicate arithmetic or alternate AST interpreter.
#[path="scientific_v2/operations.rs"]mod numeric_operations;
#[path="scientific_v2/ledger.rs"]mod numeric_ledger;
type ScientificProgramOptionsV2=ClassProgramOptionsV1;type ScientificProgramStatsV2=ClassProgramStatsV1;
#[derive(Debug)]enum ProgramNumericOperationV1{Unary(UnaryOperatorV1),Binary(BinaryOperatorV1),Transpose(bool),Leaf(ScientificLeafOperationV1)}
#[derive(Debug,Default,Clone,Copy)]pub struct ClassProgramStatsV1{pub transitions:u64,pub primitive_steps:u64,pub peak_live_payload_bytes:usize,pub peak_metadata_bytes:usize,pub solves_completed:u64,pub last_solve:Option<ScientificSolveDiagnosticsV1>,pub last_solve_complete:bool,pub maximum_growth:Option<f64>,pub operations_completed:u64,pub peak_operation_reserved_bytes:usize,pub sparse_constructors:u64,pub sparse_products:u64,pub ledger_poisoned:bool,pub heap_object_slots:usize,pub heap_live_objects:usize,pub heap_payload_bytes:usize,pub heap_metadata_bytes:usize,pub heap_reclaimed_objects:u64,pub handle_mutations:u64,pub handle_deletions:u64,pub class_heap_steps:u64}
type ProgramCheckpointSink<'a>=std::cell::RefCell<&'a mut dyn FnMut(ProgramCheckpointV1,&ProgramCancelHandleV1)>;
fn checkpoint_sink<'a>(callback:&'a mut dyn FnMut(ProgramCheckpointV1,&ProgramCancelHandleV1))->ProgramCheckpointSink<'a>{std::cell::RefCell::new(callback)}
#[derive(Debug)]pub enum ClassEventValueV1{Scientific(ScientificProgramValueV2),Object(ClassObjectSnapshotV1)}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassProgramErrorCodeV1 {
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
    Value(ClassErrorV1),
}
impl ClassProgramErrorCodeV1{pub fn is_allocation(&self)->bool{matches!(self,Self::Allocation|Self::Value(ClassErrorV1::Allocation))}}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassProgramErrorV1 {
    pub code: ClassProgramErrorCodeV1,
    pub location: ProgramLocationV1,
    pub call_stack: [Option<ProgramLocationV1>; 32],
    pub call_depth: usize,
}
#[derive(Debug, Clone, Copy)]
pub struct ClassProgramOptionsV1 {
    pub deadline: Option<Instant>,
    pub transition_fuel: u64,
    pub primitive_fuel: u64,
}
impl Default for ClassProgramOptionsV1 {
    fn default() -> Self {
        Self {
            deadline: None,
            transition_fuel: 5_000_000,
            primitive_fuel: 1_000_000_000,
        }
    }
}
#[derive(Debug)]
pub struct ClassProgramEventV1 {
    pub sequence: u64,
    pub location: ProgramLocationV1,
    pub function: Option<NodeIdV1>,
    pub name: Option<String>,
    pub value: ClassEventValueV1,
}
#[derive(Debug)]
pub struct ClassProgramOutcomeV1 {
    pub profile: &'static str,
    pub key: ProgramRequestKeyV1,
    pub start: ProgramContextV1,
    pub final_context: ProgramContextV1,
    pub committed_effect_sequence: u64,
    pub completed_leaf_statement_count: u64,
    pub stats: ProgramExecutionStatsV1,
    pub events: Vec<ClassProgramEventV1>,
    pub status: ProgramStatusV1,
    pub error: Option<ClassProgramErrorV1>,
}
struct Binding {
    name: String,
    value: ClassValueV1,
}
pub struct ClassProgramSessionV1 {
    session_id: u64,
    epoch: u64,
    request_id: u64,
    revision: u64,
    bindings: Vec<Binding>,
    heap:heap::HeapStorage,
    roots:Arc<heap::PrivateRoots>,
    heap_poison:std::cell::Cell<bool>,
    class_schema:Vec<ClassSourceV1>,
    schema_initialized:bool,
    graph:Option<Arc<ClassProgramGraphV1>>,
    defaults:[Option<Vec<Option<ClassValueV1>>>;8],
    defaults_metadata:usize,
    defaults_payload_bytes:usize,
    default_building:[Option<Vec<Option<ClassValueV1>>>;8],
    default_state:[u8;8],
    default_eval_depth:usize,
    copy_work:Vec<(ClassObjectIdV1,ClassObjectIdV1,usize)>,
    #[cfg(test)]
    fail_allocation: Option<usize>,
    #[cfg(test)] test_numeric_poison:bool,
    #[cfg(test)] test_release_underflow:bool,
    #[cfg(test)] test_force_metadata_gc:bool,
    #[cfg(test)] test_metadata_gc_runs:u64,
}
/// Borrowed read-only workspace entry; no mutable storage or value copy escapes.
pub struct ClassProgramBindingViewV1<'a> {
    pub name: &'a str,
    pub value: &'a ClassValueV1,
}
impl ClassProgramSessionV1 {
    pub fn new() -> Result<Self, ClassProgramErrorCodeV1> { Self::new_with_reservation_fault(None) }
    // Logical fault points model preflight, not recovery from std Arc global OOM.
    fn new_with_reservation_fault(fault:Option<usize>) -> Result<Self,ClassProgramErrorCodeV1> {
        let registry_bytes=std::mem::size_of::<heap::PrivateRoots>().checked_add(64).ok_or(ClassProgramErrorCodeV1::Resource)?;
        let initial_bytes=64usize.checked_mul(std::mem::size_of::<Binding>()).and_then(|n|n.checked_add(registry_bytes)).filter(|n|*n<=16<<20).ok_or(ClassProgramErrorCodeV1::Resource)?;
        let _=initial_bytes;
        if fault==Some(1){return Err(ClassProgramErrorCodeV1::Allocation)}
        let session_id = NEXT_ID
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| ClassProgramErrorCodeV1::CounterExhausted)?;
        let mut bindings = Vec::new();
        bindings
            .try_reserve_exact(64)
            .map_err(|_| ClassProgramErrorCodeV1::Allocation)?;
        if bindings.capacity() > 64 {
            return Err(ClassProgramErrorCodeV1::Resource);
        }
        if fault==Some(2){return Err(ClassProgramErrorCodeV1::Allocation)}
        Ok(Self {
            session_id,
            epoch: 1,
            request_id: 0,
            revision: 0,
            bindings,
            heap:heap::HeapStorage::empty(session_id,1),roots:Arc::new(heap::PrivateRoots::new()),heap_poison:std::cell::Cell::new(false),class_schema:Vec::new(),schema_initialized:false,graph:None,defaults:std::array::from_fn(|_|None),defaults_metadata:0,defaults_payload_bytes:0,default_building:std::array::from_fn(|_|None),default_state:[0;8],default_eval_depth:0,copy_work:Vec::new(),
            #[cfg(test)]
            fail_allocation: None,
            #[cfg(test)] test_numeric_poison:false,
            #[cfg(test)] test_release_underflow:false,
            #[cfg(test)] test_force_metadata_gc:false,
            #[cfg(test)] test_metadata_gc_runs:0,
        })
    }
    fn extra_live(&self)->usize{self.heap.payload_bytes+self.defaults_payload_bytes}
    fn extra_metadata(&self)->usize{self.heap.metadata_bytes+self.defaults_metadata+self.class_schema.capacity()*std::mem::size_of::<ClassSourceV1>()+std::mem::size_of::<heap::PrivateRoots>()+64+self.copy_work.capacity()*std::mem::size_of::<(ClassObjectIdV1,ClassObjectIdV1,usize)>()}
    pub fn context(&self) -> ProgramContextV1 {
        ProgramContextV1 {
            session_id: self.session_id,
            epoch: self.epoch,
            revision: self.revision,
        }
    }
    pub fn bindings(&self) -> impl ExactSizeIterator<Item = ClassProgramBindingViewV1<'_>> {
        self.bindings.iter().map(|b| ClassProgramBindingViewV1 {
            name: &b.name,
            value: &b.value,
        })
    }
    pub fn binding(&self,name:&str)->Option<&ClassValueV1>{self.bindings.iter().find(|b|b.name==name).map(|b|&b.value)}
    pub fn view<'a>(&'a self,value:&'a ClassValueV1)->Result<ClassValueViewV1<'a>,ClassErrorV1>{match value{ClassValueV1::Scientific(v)=>Ok(ClassValueViewV1::Scientific(v)),ClassValueV1::Object(id)=>{let record=self.heap.record(*id)?;let graph=self.graph.as_ref().ok_or(ClassErrorV1::UnknownClass)?;let class_name=graph.text(graph.classes()[record.class_index].name).ok_or(ClassErrorV1::UnknownClass)?;Ok(ClassValueViewV1::Object{class_name,metadata:ClassObjectMetadataV1{class_index:record.class_index,object:*id,valid:record.valid}})}}}
    pub fn reset(&mut self) -> Result<(), ClassProgramErrorCodeV1> {
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or(ClassProgramErrorCodeV1::CounterExhausted)?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(ClassProgramErrorCodeV1::CounterExhausted)?;
        self.bindings.clear();
        self.defaults=std::array::from_fn(|_|None);self.heap=heap::HeapStorage::empty(self.session_id,epoch);self.roots=Arc::new(heap::PrivateRoots::new());self.heap_poison.set(false);self.class_schema=Vec::new();self.schema_initialized=false;self.graph=None;self.defaults_metadata=0;self.defaults_payload_bytes=0;self.default_building=std::array::from_fn(|_|None);self.default_state=[0;8];self.default_eval_depth=0;self.copy_work=Vec::new();
        self.epoch = epoch;
        self.revision = revision;
        Ok(())
    }
    pub fn prepare(
        &mut self,
        module: Arc<ProgramModuleV1>,
        options: ClassProgramOptionsV1,
    ) -> Result<PreparedClassProgramRequestV1<'_>, ClassProgramErrorCodeV1> {
        if self.heap_poison.get()||self.heap.poisoned.get(){return Err(ClassProgramErrorCodeV1::Resource)}
        if options.transition_fuel > 5_000_000 || options.primitive_fuel > 1_000_000_000 {
            return Err(ClassProgramErrorCodeV1::Resource);
        }
        let request_id = self
            .request_id
            .checked_add(1)
            .ok_or(ClassProgramErrorCodeV1::CounterExhausted)?;
        #[cfg(test)]
        if self.fail_allocation == Some(0) {
            return Err(ClassProgramErrorCodeV1::Allocation);
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
        Ok(PreparedClassProgramRequestV1 {
            owner: self,
            module,
            options,
            handle,
            invoked: false,
        })
    }
}
pub struct PreparedClassProgramRequestV1<'a> {
    owner: &'a mut ClassProgramSessionV1,
    module: Arc<ProgramModuleV1>,
    options: ClassProgramOptionsV1,
    handle: ProgramCancelHandleV1,
    invoked: bool,
}
impl PreparedClassProgramRequestV1<'_> {
    pub fn cancel_handle(&self) -> ProgramCancelHandleV1 {
        self.handle.clone()
    }
    /// Allocation-free cooperative execution. Dropping a suspended future
    /// abandons its key and restores completed caller publications.
    pub async fn execute_cooperatively(mut self,mut checkpoint:impl FnMut(ProgramCheckpointV1,&ProgramCancelHandleV1))->ClassProgramOutcomeV1{
        self.invoked=true;
        machine::execute_async(self.owner,Arc::clone(&self.module),self.options,&self.handle,&mut checkpoint).await
    }
    pub fn execute(self) -> ClassProgramOutcomeV1 {
        self.execute_controlled(|_, _| {})
    }
    pub fn execute_controlled(
        mut self,
        mut checkpoint: impl FnMut(ProgramCheckpointV1, &ProgramCancelHandleV1),
    ) -> ClassProgramOutcomeV1 {
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
impl Drop for PreparedClassProgramRequestV1<'_> {
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


/// Heap reservations become attached session storage before a suspension. Numeric
/// temporaries use the unchanged scoped V2 ledger. Fatal-request unwind may keep
/// a conservative transient reservation until its counter is discarded; the
/// next request recomputes from actual session heap/binding ownership.
struct ClassHeapBudget<'a,'b>{stats:&'a mut ClassProgramStatsV1,live:&'a mut usize,metadata:&'a mut usize,allocations:&'a mut usize,options:ClassProgramOptionsV1,handle:&'a ProgramCancelHandleV1,checkpoint:&'a ProgramCheckpointSink<'b>,poison:&'a std::cell::Cell<bool>,fault:Option<usize>}
impl ClassHeapBudget<'_,'_>{
 fn allocation(&mut self)->Result<(),ClassErrorV1>{*self.allocations=self.allocations.checked_add(1).ok_or(ClassErrorV1::Resource)?;if self.fault==Some(*self.allocations){return Err(ClassErrorV1::Allocation)}Ok(())}
 fn fail(&mut self)->ClassErrorV1{self.stats.ledger_poisoned=true;self.poison.set(true);ClassErrorV1::Resource}
}
impl ScientificControlV1 for ClassHeapBudget<'_,'_>{
 fn charge(&mut self,n:u64)->Result<(),ScientificErrorV1>{
  (self.checkpoint.borrow_mut())(ProgramCheckpointV1::Task,self.handle);if self.options.deadline.is_some_and(|d|Instant::now()>=d){self.handle.cancel_for(self.handle.key,ProgramCancelReasonV1::Deadline);}
  if matches!(self.handle.state.load(Ordering::Acquire),STOP|DEADLINE|PENDING_STOP|PENDING_DEADLINE){return Err(ScientificErrorV1::Cancelled)}if self.poison.get(){return Err(ScientificErrorV1::Resource)}if n>32768{return Err(ScientificErrorV1::Resource)}
  let next=self.stats.primitive_steps.checked_add(n).filter(|n|*n<=self.options.primitive_fuel).ok_or(ScientificErrorV1::Resource)?;let units=self.stats.class_heap_steps.checked_add(n).ok_or(ScientificErrorV1::Resource)?;let ticks=units/32768-self.stats.class_heap_steps/32768;let transitions=self.stats.transitions.checked_add(ticks).filter(|n|*n<=self.options.transition_fuel).ok_or(ScientificErrorV1::Resource)?;self.stats.class_heap_steps=units;self.stats.primitive_steps=next;self.stats.transitions=transitions;Ok(())
 }
 fn reserve(&mut self,bytes:usize)->Result<(),ScientificErrorV1>{self.charge(0)?;let next=self.live.checked_add(bytes).filter(|n|*n<=PROFILE_LIVE_BYTES).ok_or(ScientificErrorV1::Resource)?;self.allocation().map_err(|e|if e==ClassErrorV1::Allocation{ScientificErrorV1::Allocation}else{ScientificErrorV1::Resource})?;*self.live=next;self.stats.peak_live_payload_bytes=self.stats.peak_live_payload_bytes.max(next);Ok(())}
}
impl heap::ClassHeapControl for ClassHeapBudget<'_,'_>{
 fn poison(&mut self){self.stats.ledger_poisoned=true;self.poison.set(true);}
 fn reserve_metadata(&mut self,bytes:usize)->Result<(),ClassErrorV1>{self.charge(0)?;let next=self.metadata.checked_add(bytes).filter(|n|*n<=16<<20).ok_or(ClassErrorV1::Resource)?;self.allocation()?;*self.metadata=next;self.stats.peak_metadata_bytes=self.stats.peak_metadata_bytes.max(next);Ok(())}
 fn release_metadata(&mut self,bytes:usize)->Result<(),ClassErrorV1>{match self.metadata.checked_sub(bytes){Some(next)=>{*self.metadata=next;Ok(())},None=>Err(self.fail())}}
 fn release_payload(&mut self,bytes:usize)->Result<(),ClassErrorV1>{match self.live.checked_sub(bytes){Some(next)=>{*self.live=next;Ok(())},None=>Err(self.fail())}}
}

#[derive(Debug,Clone,Copy)]enum ClassMode{Constructor(usize),Method(NodeIdV1)}
#[derive(Debug)]enum ClassTask{
 Member{site:NodeIdV1,name:NodeIdV1},
 MemberLeaf{site:NodeIdV1,base:usize,suppressed:bool},
 MethodReceiver{site:NodeIdV1,name:NodeIdV1,args:Option<NodeIdV1>,requested:usize,base:usize},
 Construct{site:NodeIdV1,class:usize,base:usize,count:usize,requested:usize},
 ConstructorSeed{site:NodeIdV1,receiver:ClassObjectIdV1,output:NodeIdV1},
 Defaults{site:NodeIdV1,class:usize},
 DefaultNext{site:NodeIdV1,class:usize,property:usize},
 DefaultStored{site:NodeIdV1,class:usize,property:usize,base:usize},
 PropertySet{site:NodeIdV1,target:NodeIdV1,suppressed:bool},
}
macro_rules! declare_machine_tasks{($($variants:tt)*)=>{enum Task{$($variants)* Class(ClassTask),}}}
macro_rules! declare_machine_modes{($($variants:tt)*)=>{#[derive(Clone,Copy)]enum Mode{$($variants)* Class(ClassMode),}}}
macro_rules! install_machine_syntax{()=>{
 fn location(&self,id:NodeIdV1)->ProgramLocationV1{ProgramLocationV1{source_id:self.module.source_id_for(id).expect("located graph node"),span:self.module.node(id).unwrap().span}}
 fn kind(&self,id:NodeIdV1)->N{self.module.common_kind(id).expect("class extensions handled before common dispatch")}
 fn text(&self,id:NodeIdV1)->&str{self.module.text(id).expect("located graph text")}
 fn is_full_axis(&self,id:NodeIdV1)->bool{matches!(self.module.common_kind(id),Some(N::FullAxis))}
};}
macro_rules! initial_profile_location{($module:expr,$id:expr)=>{ProgramLocationV1{source_id:$module.source_id_for($id).unwrap(),span:$module.node($id).unwrap().span}}}
macro_rules! profile_admit_kind{($node:expr)=>{match $node.kind{ClassNodeKindV1::Common(kind)=>Some(kind),_=>None}}}
macro_rules! profile_name_slice{($m:expr,$id:expr,$start:expr,$end:expr)=>{&$m.module.sources()[$m.module.node($id).unwrap().source_slot as usize].text[$start..$end]}}
macro_rules! profile_function_visible{($m:expr,$id:expr)=>{$m.module.node($m.current).is_some_and(|n|n.source_slot==0)}}
macro_rules! profile_extra_live{($owner:expr)=>{$owner.extra_live()}}
macro_rules! profile_extra_metadata{($owner:expr)=>{$owner.extra_metadata()}}
macro_rules! profile_start{($m:expr)=>{$m.class_start().await?;}}
macro_rules! profile_before_exec{($m:expr,$id:expr)=>{if $m.class_exec($id).await?{return Ok(())}}}
macro_rules! profile_before_eval{($m:expr,$id:expr,$requested:expr,$extent:expr)=>{if $m.class_eval($id,$requested,$extent).await?{return Ok(())}}}
macro_rules! profile_match_task{($m:expr,$task:expr;$($arms:tt)*)=>{match $task{$($arms)* other=>profile_extra_task!($m,other)?,}}}
macro_rules! profile_match_mode{($m:expr,$mode:expr,$base:expr,$count:expr,$requested:expr,$site:expr;$($arms:tt)*)=>{match $mode{$($arms)* other=>profile_extra_mode!($m,other,$base,$count,$requested,$site)?,}}}
macro_rules! profile_extra_task{($m:expr,$task:expr)=>{{let _=$task;Err::<(),ProgramErrorCodeV1>(ProgramErrorCodeV1::UnsupportedFeature)}}}
macro_rules! profile_extra_mode{($m:expr,$mode:expr,$base:expr,$count:expr,$requested:expr,$site:expr)=>{{match $mode{Mode::Class(ClassMode::Constructor(class))=>$m.push(Task::Class(ClassTask::Construct{site:$site,class,base:$base,count:$count,requested:$requested})),Mode::Class(ClassMode::Method(function))=>$m.enter_call(function,$base,$count,$requested,$site).await,_=>Err(ProgramErrorCodeV1::UnsupportedFeature)}}}}
macro_rules! profile_before_return{($m:expr)=>{$m.class_validate_return().await?;}}
// Class continuations are dispatched at the same run-loop boundary as common
// tasks. Do not nest their generated poll frames inside Machine::step.
macro_rules! profile_dispatch_task{($m:expr,$task:expr)=>{{match $task{Task::Class(task)=>$m.class_step(task).await?,Task::Leaf{node,base}=>{$m.current=node;$m.finish_leaf(node,base).await?;$m.complete()?;},task=>$m.step(task).await?}}}}
// Dispatched above: avoid embedding the publication future and its generated
// native frame inside common step for the class specialization.
macro_rules! profile_leaf_task{($m:expr,$node:expr,$base:expr)=>{{return Err(ProgramErrorCodeV1::UnsupportedFeature);}}}
macro_rules! profile_metadata_admission{($m:expr,$bytes:expr)=>{{$m.class_metadata_room($bytes).await?;}}}
macro_rules! profile_release_value{($m:expr,$value:expr)=>{{$m.class_release($value);}}}
macro_rules! profile_copy_return_values{($m:expr,$results:ident,$next:ident,$delivered:expr)=>{{
 let mut roots:[Option<_>;64]=std::array::from_fn(|_|None);
 for(index,slot)in $results.iter_mut().take($delivered).enumerate(){
  let(id,tail)=$m.list($next.ok_or(ProgramErrorCodeV1::Arity)?);$next=tail;
  let source=$m.text(id);let binding=$m.binding_index(source).await.ok_or(ProgramErrorCodeV1::UnassignedOutput)?;
  *slot=Some($m.copy_binding(binding).await?);roots[index]=Some(profile_root_value!($m,slot.as_ref().unwrap()));
 }
 roots
}}}
macro_rules! profile_root_value{($m:expr,$value:expr)=>{match $value{ClassValueV1::Object(id)=>Some($m.owner.roots.hold(*id).map_err(Self::class_error)?),ClassValueV1::Scientific(_)=>None}}}
macro_rules! profile_register_incoming{($m:expr,$incoming:expr)=>{$m.class_hold_incoming($incoming)?}}
macro_rules! profile_builtin{($m:expr,$site:expr,$id:expr,$base:expr,$count:expr,$requested:expr)=>{$m.class_builtin($site,$id,$base,$count,$requested)}}
macro_rules! profile_event_copy{($m:expr,$value:expr)=>{$m.class_event_copy($value)}}
macro_rules! profile_event_move{($m:expr,$value:expr)=>{$m.class_event_move($value)?}}
macro_rules! profile_prepare_publication{($m:expr,$writes:expr,$external:expr)=>{{$m.class_prepare_writes($writes).await?}}}
macro_rules! profile_commit_publication{($m:expr,$prepared:expr)=>{$m.owner.heap.commit_publication($prepared)}}
macro_rules! profile_finalize_stats{($m:expr)=>{{$m.class_finalize_stats();}}}
macro_rules! profile_on_drop{($m:expr)=>{{$m.class_abandon_defaults();}}}
macro_rules! class_heap_budget{($m:expr)=>{ClassHeapBudget{stats:&mut $m.stats,live:&mut $m.live,metadata:&mut $m.metadata,allocations:&mut $m.allocations,options:$m.options,handle:$m.handle,checkpoint:&$m.checkpoint,poison:&$m.owner.heap_poison,fault:{#[cfg(test)]{$m.owner.fail_allocation}#[cfg(not(test))]{None}}}}}
macro_rules! class_numeric_scope{($m:expr,$inputs:expr,$kind:expr)=>{numeric_ledger::OperationScope::new(&mut $m.stats,&mut $m.live,&mut $m.allocations,$m.options,$m.handle,&$m.checkpoint,$inputs,$kind,{#[cfg(test)]{$m.owner.fail_allocation}#[cfg(not(test))]{None}})}}
use operations::install_class_runtime as install_value_profile;
#[path="machine.rs"]mod machine;

#[cfg(test)]
mod class_vm_tests {
 use super::*;
 use mathlab_language::program::parse_class_graph_controlled_v1;
 const GAUGE:&str="classdef VmGauge < handle\nproperties\nReading=4\nend\nend\n";
 const BOX:&str="classdef VmBox\nproperties\nDense=[1 2 3]\nGauge\nend\nmethods\nfunction obj=VmBox(h)\nobj.Gauge=h;\nend\nfunction obj=changed(obj)\nobj.Dense=[8 9 10];obj.Gauge.Reading=12;\nend\nend\nend\n";
 fn graph(entry:&str,modules:&[&str])->Arc<ClassProgramGraphV1>{Arc::new(parse_class_graph_controlled_v1(std::iter::once(entry).chain(modules.iter().copied()).enumerate().map(|(i,text)|ClassSourceV1{source_id:(i+1)as u64,text:Arc::from(text)}).collect(),||false).unwrap())}
 fn run(s:&mut ClassProgramSessionV1,entry:&str,modules:&[&str])->ClassProgramOutcomeV1{s.prepare(graph(entry,modules),Default::default()).unwrap().execute()}
 fn success(o:&ClassProgramOutcomeV1){assert_eq!(o.status,ProgramStatusV1::Success,"{:?}",o.error);assert!(!o.stats.ledger_poisoned);assert!(o.stats.peak_metadata_bytes<=16<<20);assert!(o.stats.peak_live_payload_bytes<=64<<20)}
 fn scalar(s:&ClassProgramSessionV1,name:&str)->f64{s.binding(name).unwrap().scientific().unwrap().dense_view().unwrap().get(0).unwrap().re}
 fn audit(s:&ClassProgramSessionV1){
  s.heap.audit_cached_ownership();assert!(!s.roots.copy_is_active());assert!(s.copy_work.is_empty(),"suspended copy workspace cleared on request drop/terminal");assert!(matches!(s.copy_work.capacity(),0|1024));
  let mut payload=0usize;let mut metadata=0usize;
  for values in s.defaults.iter().chain(s.default_building.iter()).flatten(){metadata+=values.capacity()*std::mem::size_of::<Option<ClassValueV1>>();for value in values.iter().flatten(){payload+=value.direct_payload_bytes();}}
  assert_eq!(payload,s.defaults_payload_bytes,"all retained default payload counted once");assert_eq!(metadata,s.defaults_metadata);
  assert!(!s.heap.poisoned.get());assert!(!s.heap_poison.get());
 }
 #[test]
 fn class_initial_registry_preflight_and_logical_faults_are_bounded(){
  for fault in [1,2]{assert!(matches!(ClassProgramSessionV1::new_with_reservation_fault(Some(fault)),Err(ClassProgramErrorCodeV1::Allocation)));}
  let s=ClassProgramSessionV1::new().unwrap();assert_eq!(s.extra_metadata(),std::mem::size_of::<heap::PrivateRoots>()+64);assert!(s.extra_metadata()<=4096);assert_eq!(Arc::strong_count(&s.roots),1);
  let lease=s.roots.begin_copy().unwrap();assert!(s.roots.begin_copy().is_err(),"copy workspace must never be reentered");drop(lease);assert!(!s.roots.copy_is_active());let lease=s.roots.begin_copy().unwrap();drop(lease);
  // Arc<RefCell<...>> intentionally makes the session thread-affine. The
  // measured payload plus logical64B header is not an Arc ABI/RSS guarantee.
 }
 #[test]
 fn class_allocation_fault_sweep_has_first_success_exact_caller_prefix_and_recovery(){
  let entry="h=VmGauge;b=VmBox(h);c=b;d=c.changed();last=d.Dense;done=1;";let module=graph(entry,&[GAUGE,BOX]);
  let mut reference=ClassProgramSessionV1::new().unwrap();success(&run(&mut reference,"seed=17;",&[GAUGE,BOX]));let base=reference.context();let full=reference.prepare(module.clone(),Default::default()).unwrap().execute();success(&full);
  let mut failures=0;let mut saw_success=false;
  for fault in 0..512{let mut s=ClassProgramSessionV1::new().unwrap();success(&run(&mut s,"seed=17;",&[GAUGE,BOX]));s.fail_allocation=Some(fault);
   let outcome=match s.prepare(module.clone(),Default::default()){Ok(request)=>Some(request.execute()),Err(code)=>{assert_eq!(fault,0);assert!(code.is_allocation());None}};
   s.fail_allocation=None;assert_eq!(scalar(&s,"seed"),17.);
   if let Some(o)=outcome {if o.status==ProgramStatusV1::Success{assert_eq!(o.final_context.revision-base.revision,full.final_context.revision-base.revision);success(&o);saw_success=true;}else{failures+=1;let e=o.error.expect("located fault");assert!(e.code.is_allocation(),"fault{fault}: {e:?}");assert!((1..=3).contains(&e.location.source_id));assert!(s.binding("done").is_none());assert!(o.final_context.revision>=base.revision);assert!(o.final_context.revision<=full.final_context.revision);assert!(!o.stats.ledger_poisoned);for event in &o.events{assert!(event.sequence<=o.committed_effect_sequence)}}}else{failures+=1;assert_eq!(s.context().revision,base.revision)}
   let revision=s.context().revision-base.revision;
   for(name,threshold)in[("h",1),("b",2),("c",3),("d",5),("last",6),("done",7)]{assert_eq!(s.binding(name).is_some(),revision>=threshold,"fault{fault} exact publication prefix {name},revision{revision}");}
   if let Some(h)=s.binding("h"){let token=h.object().unwrap();let reading=s.heap.property(token,0).unwrap().scientific().unwrap().dense_view().unwrap().get(0).unwrap().re;assert_eq!(reading,if revision>=4{12.}else{4.});}
   audit(&s);let prior=s.context();let recovery=run(&mut s,"recovered=23;",&[GAUGE,BOX]);success(&recovery);assert_eq!(recovery.start,prior);assert_eq!(scalar(&s,"recovered"),23.);audit(&s);
   if saw_success{break}
  }
  assert!(failures>=12,"initial vectors, names, defaults, heap, numeric and copy reserves exercised");assert!(saw_success,"sweep must reach genuine success");
 }
 #[test]
 fn class_session_rejects_poison_before_any_request_and_reset_restarts_domain(){
  for heap_poison in [false,true]{let mut s=ClassProgramSessionV1::new().unwrap();success(&run(&mut s,"h=VmGauge;",&[GAUGE]));let token=s.binding("h").unwrap().object().unwrap();let prior=s.context();let requests=s.request_id;if heap_poison{s.heap.poisoned.set(true)}else{s.heap_poison.set(true)}
   assert!(matches!(s.prepare(graph("unreached=1;",&[GAUGE]),Default::default()),Err(ClassProgramErrorCodeV1::Resource)));assert_eq!(s.context(),prior);assert_eq!(s.request_id,requests);assert!(s.binding("unreached").is_none());s.reset().unwrap();assert!(matches!(s.view(&ClassValueV1::Object(token)),Err(ClassErrorV1::InvalidHandle)));success(&run(&mut s,"fresh=1;",&[GAUGE]));audit(&s);
  }
 }
 #[test]
 fn class_revision_exhaustion_prepares_without_mutating_shared_handle(){
  let mut s=ClassProgramSessionV1::new().unwrap();success(&run(&mut s,"h=VmGauge;",&[GAUGE]));let token=s.binding("h").unwrap().object().unwrap();s.revision=u64::MAX;
  let o=run(&mut s,"h.Reading=9;unreached=1;",&[GAUGE]);assert_eq!(o.error.unwrap().code,ClassProgramErrorCodeV1::CounterExhausted);assert_eq!(s.revision,u64::MAX);assert!(s.binding("unreached").is_none());assert_eq!(s.heap.property(token,0).unwrap().scientific().unwrap().dense_view().unwrap().get(0).unwrap().re,4.);audit(&s);
  let before=s.context();assert_eq!(s.reset(),Err(ClassProgramErrorCodeV1::CounterExhausted));assert_eq!(s.context(),before);
 }
 #[test]
 fn class_multioutput_return_copies_stay_rooted_during_gc_pressure(){
  // Both independent returned value objects contain nested values and a shared
  // published handle. A later output copy crosses the768-record GC threshold.
  let boxdef="classdef VmTree\nproperties\nLeft\nRight\nGauge\nend\nend\n";
  let entry="h=VmGauge;leaf=VmTree;leaf.Gauge=h;tree=leaf;for k=1:6\nnext=VmTree;next.Left=tree;next.Right=tree;next.Gauge=h;tree=next;end\n[a,b]=pair(tree);x=a.Left.Gauge;y=b.Right.Gauge;x.Reading=29;observed=[x.Reading y.Reading h.Reading];function [a,b]=pair(t)\na=t;b=t;\nend\n";
  let mut s=ClassProgramSessionV1::new().unwrap();let o=run(&mut s,entry,&[GAUGE,boxdef]);success(&o);assert!(o.stats.heap_reclaimed_objects>0,"pressure must actually collect");let words=s.binding("observed").unwrap().scientific().unwrap().dense_view().unwrap();assert_eq!(words.len(),3);for i in 0..3{assert_eq!(words.get(i).unwrap().re,29.)}assert_ne!(s.binding("a").unwrap().object().unwrap(),s.binding("b").unwrap().object().unwrap());audit(&s);
 }

 #[test]
 fn class_numeric_invariant_poison_persists_on_terminal_and_every_reached_drop(){
  let module=graph("unreached=1;",&[]);let mut reference=ClassProgramSessionV1::new().unwrap();reference.test_numeric_poison=true;
  let(outcome,polls)=drive_measured_class_fixture(&mut reference,module.clone(),256);assert_eq!(outcome.status,ProgramStatusV1::Resource);assert!(outcome.stats.ledger_poisoned);
  assert!(reference.heap_poison.get());assert!(matches!(reference.prepare(module.clone(),Default::default()),Err(ClassProgramErrorCodeV1::Resource)));
  let mut reached_drop=false;
  for at in 0..polls{let mut session=ClassProgramSessionV1::new().unwrap();session.test_numeric_poison=true;drop_class_fixture_after(&mut session,module.clone(),at);
   if !session.test_numeric_poison{reached_drop=true;assert!(session.heap_poison.get(),"actual numeric ledger failure must persist after child future drop");assert!(matches!(session.prepare(module.clone(),Default::default()),Err(ClassProgramErrorCodeV1::Resource)));session.reset().unwrap();success(&run(&mut session,"fresh=1;",&[]));}
  }
  assert!(reached_drop,"must include suspended child after actual ledger invariant failure");println!("CLASS_POISON_DROP_FIXTURE terminal_and_reached_drop_refusal_reset_passed boundaries={polls}");
 }
 #[test]
 fn class_release_underflow_poison_preserves_completed_binding_and_property_barriers(){
  for property in [false,true]{let mut s=ClassProgramSessionV1::new().unwrap();success(&run(&mut s,"h=VmGauge;a=4;",&[GAUGE]));let before=s.context();s.test_release_underflow=true;let o=run(&mut s,if property{"h.Reading=9;unreached=1;"}else{"a=9;unreached=1;"},&[GAUGE]);assert_eq!(o.status,ProgramStatusV1::Resource);assert!(o.stats.ledger_poisoned);assert!(s.heap_poison.get());assert_eq!(s.context().revision,before.revision+1);assert!(s.binding("unreached").is_none());
   if property{let id=s.binding("h").unwrap().object().unwrap();assert_eq!(s.heap.property(id,0).unwrap().scientific().unwrap().dense_view().unwrap().get(0).unwrap().re,9.);}else{assert_eq!(scalar(&s,"a"),9.);}
   assert!(matches!(s.prepare(graph("refused=1;",&[GAUGE]),Default::default()),Err(ClassProgramErrorCodeV1::Resource)));s.reset().unwrap();success(&run(&mut s,"fresh=1;",&[GAUGE]));
  }
 }
 #[test]
 fn class_private_unsuppressed_constructor_display_is_separate_from_shared_mutation(){
  for semicolon in [true,false]{let ending=if semicolon{";"}else{""};let failing=format!("classdef VmFailed < handle\nproperties\nMarker=4\nend\nmethods\nfunction obj=VmFailed(h)\nobj.Marker=8{ending}\nh.Reading=9;\nobj.Missing=1;\nend\nend\nend\n");let mut s=ClassProgramSessionV1::new().unwrap();let o=run(&mut s,"h=VmGauge;before=h.Reading;result=VmFailed(h);unreached=1;",&[GAUGE,&failing]);assert_eq!(o.status,ProgramStatusV1::Error);assert_eq!(o.error.unwrap().code,ClassProgramErrorCodeV1::Value(ClassErrorV1::UnknownProperty));assert_eq!(o.final_context.revision,3);assert_eq!(o.committed_effect_sequence,3+u64::from(!semicolon));assert_eq!(o.stats.handle_mutations,1,"only committed published handle mutation counted");assert_eq!(o.events.len(),usize::from(!semicolon));if !semicolon{match &o.events[0].value{ClassEventValueV1::Scientific(v)=>assert_eq!(v.dense_view().unwrap().get(0).unwrap().re,8.),_=>panic!("property value snapshot")}}
   assert!(s.binding("result").is_none());assert!(s.binding("unreached").is_none());let id=s.binding("h").unwrap().object().unwrap();assert_eq!(s.heap.property(id,0).unwrap().scientific().unwrap().dense_view().unwrap().get(0).unwrap().re,9.);audit(&s);
  }
 }

 #[test]
 fn class_initial_metadata_structural_max_includes_full_heap_before_gc(){
  let bound=machine::class_initial_structural_metadata_bound();assert!(bound<=16<<20,"actual typed initial admission must fit even at heap1024*64/default512 maxima: {bound}");
 }

 #[test]
 fn class_call_args_and_method_receiver_survive_forced_metadata_gc(){
  let mut s=ClassProgramSessionV1::new().unwrap();success(&run(&mut s,"h=VmGauge;a=VmBox(h);",&[GAUGE,BOX]));s.test_force_metadata_gc=true;
  let o=run(&mut s,"[b,c]=ordinary(a,a);d=b.changed();observed=[b.Gauge.Reading c.Gauge.Reading h.Reading];kept=a.Dense;changed=d.Dense;function [x,y]=ordinary(p,q)\nx=p;y=q;\nend\n",&[GAUGE,BOX]);success(&o);assert!(s.test_metadata_gc_runs>=6,"force GC at argument-name/frame metadata admission, not only numeric copying");
  let observed=s.binding("observed").unwrap().scientific().unwrap().dense_view().unwrap();for i in 0..3{assert_eq!(observed.get(i).unwrap().re,12.)}assert_eq!(scalar(&s,"kept"),1.);assert_eq!(scalar(&s,"changed"),8.);assert_ne!(s.binding("a").unwrap().object().unwrap(),s.binding("b").unwrap().object().unwrap());audit(&s);
  let definition="classdef delete\nproperties\nValue=17\nend\nend\n";let mut named=ClassProgramSessionV1::new().unwrap();success(&run(&mut named,"a=delete();b=delete;observed=a.Value;bare=b.Value;delete=[4 8];selected=delete(2);",&[definition]));assert_eq!(scalar(&named,"observed"),17.);assert_eq!(scalar(&named,"bare"),17.);assert_eq!(scalar(&named,"selected"),8.);audit(&named);
  let mut local=ClassProgramSessionV1::new().unwrap();success(&run(&mut local,"selected=delete(2);function y=delete(x)\ny=x+3;\nend\n",&[]));assert_eq!(scalar(&local,"selected"),5.);audit(&local);
 }
 // Separate ordinary call frames keep the baseline future, dropped future and
 // synchronous recovery future from all reserving slots in one test prologue.
 // No thread/stack-size change: each helper owns exactly one real API future.
 #[inline(never)]
 fn drive_measured_class_fixture(session:&mut ClassProgramSessionV1,module:Arc<ClassProgramGraphV1>,max_polls:usize)->(ClassProgramOutcomeV1,usize){
  use std::future::Future;
  let request=session.prepare(module,Default::default()).unwrap();let callback=|_:ProgramCheckpointV1,_:&ProgramCancelHandleV1|{};assert_eq!(std::mem::size_of_val(&callback),0);let factory=||request.execute_cooperatively(callback);
  fn layout<F:std::future::Future>(_:&impl FnOnce()->F)->(usize,usize){(std::mem::size_of::<F>(),std::mem::align_of::<F>())}
  let(bytes,alignment)=layout(&factory);assert!(bytes<=1<<20,"engine-owned class future exceeds1MiB: {bytes}");assert!(alignment<=64,"class future alignment exceeds conservative stack padding bound");
  println!("CLASS_FUTURE_LAYOUT_V1 bytes={bytes} alignment={alignment} bound_bytes=1048576 callback_bytes=0 caller_capture_excluded=true");
  let mut future=std::pin::pin!(factory());assert_eq!(std::mem::size_of_val(future.as_ref().get_ref()),bytes);let mut cx=std::task::Context::from_waker(std::task::Waker::noop());let mut polls=0;
  loop{polls+=1;assert!(polls<=max_polls);if let std::task::Poll::Ready(outcome)=future.as_mut().poll(&mut cx){return(outcome,polls)}}
 }
 #[inline(never)]
 fn drop_class_fixture_after(session:&mut ClassProgramSessionV1,module:Arc<ClassProgramGraphV1>,at:usize){
  use std::future::Future;
  let request=session.prepare(module,Default::default()).unwrap();let mut future=std::pin::pin!(request.execute_cooperatively(|_,_|{}));let mut cx=std::task::Context::from_waker(std::task::Waker::noop());for _ in 0..at{assert!(future.as_mut().poll(&mut cx).is_pending());}
 }
 #[test]
 fn class_every_private_default_copy_return_drop_recomputes_actual_heap_caches(){
  let module=graph("h=VmGauge;a=VmBox(h);b=a;c=b.changed();d=identity(c);function out=identity(input)\nout=input;\nend\n",&[GAUGE,BOX]);let mut reference=ClassProgramSessionV1::new().unwrap();let(outcome,polls)=drive_measured_class_fixture(&mut reference,module.clone(),2048);success(&outcome);audit(&reference);assert!(polls>1);println!("CLASS_DROP_FIXTURE baseline_ready polls={polls}");
  for at in 0..polls{let mut session=ClassProgramSessionV1::new().unwrap();drop_class_fixture_after(&mut session,module.clone(),at);audit(&session);let before=session.context();let outcome=run(&mut session,"recovered=31;",&[GAUGE,BOX]);success(&outcome);assert_eq!(outcome.start,before);assert_eq!(scalar(&session,"recovered"),31.);audit(&session);}println!("CLASS_DROP_FIXTURE all_pending_drop_recovery_passed boundaries={polls}");
 }
}
