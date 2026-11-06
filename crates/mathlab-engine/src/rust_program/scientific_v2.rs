// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit scientific-v2 specialization of the SAME located AST machine.
use super::*;
use mathlab_language::program::{ScientificProgramValueV2,ScientificStorageViewV2,ScientificDenseV1,ScientificShapeV1,ScientificCscV1,ScientificCscShapeV1,ScientificCscValuesV1,ScientificTripletValuesV1,ScientificErrorV1,ScientificControlV1,ScientificLeafOperationV1,ScientificSolveDiagnosticsV1,Complex64V1,evaluate_scientific_dense_v2,yield_scientific_v1};
const PROGRAM_PROFILE_V1:&str="scientific-v2";
const PROFILE_LIVE_BYTES:usize=64<<20;
const PROFILE_INDEX_AXES:usize=8;
type ProgramAxes=[bool;8];
type ProgramValueV1=ScientificProgramValueV2;
type ProgramErrorCodeV1=ScientificProgramErrorCodeV2;
type ProgramErrorV1=ScientificProgramErrorV2;
type ProgramExecutionOptionsV1=ScientificProgramOptionsV2;
type ProgramExecutionStatsV1=ScientificProgramStatsV2;
type ProgramEventV1=ScientificProgramEventV2;
type ProgramOutcomeV1=ScientificProgramOutcomeV2;
type ProgramSessionV1=ScientificProgramSessionV2;
#[derive(Debug)]
enum ProgramNumericOperationV1{Unary(UnaryOperatorV1),Binary(BinaryOperatorV1),Transpose(bool),Leaf(ScientificLeafOperationV1)}
#[derive(Debug,Default,Clone,Copy)]
pub struct ScientificProgramStatsV2{
    pub transitions:u64,pub primitive_steps:u64,pub peak_live_payload_bytes:usize,pub peak_metadata_bytes:usize,
    pub solves_completed:u64,pub last_solve:Option<ScientificSolveDiagnosticsV1>,pub last_solve_complete:bool,pub maximum_growth:Option<f64>,
    pub operations_completed:u64,pub peak_operation_reserved_bytes:usize,pub sparse_constructors:u64,pub sparse_products:u64,pub ledger_poisoned:bool,
}
type ProgramCheckpointSink<'a>=std::cell::RefCell<&'a mut dyn FnMut(ProgramCheckpointV1,&ProgramCancelHandleV1)>;
fn checkpoint_sink<'a>(callback:&'a mut dyn FnMut(ProgramCheckpointV1,&ProgramCancelHandleV1))->ProgramCheckpointSink<'a>{std::cell::RefCell::new(callback)}
mod ledger;
mod operations;
use ledger::{OperationScope,OperationKind};
// The guard is declared BEFORE the child future which owns consumed operands.
// Rust drops a suspended child before this guard, including cancellation/unwind.
macro_rules! operation_scope{($m:expr,$inputs:expr,$kind:expr)=>{OperationScope::new(&mut $m.stats,&mut $m.live,&mut $m.allocations,$m.options,$m.handle,&$m.checkpoint,$inputs,$kind,{#[cfg(test)]{$m.owner.fail_allocation}#[cfg(not(test))]{None}})}}
macro_rules! install_scientific_v2{()=>{
impl Machine<'_, '_>{
    fn primitive_checkpoint(&mut self){self.notify(ProgramCheckpointV1::Task)}
    fn notify(&self,at:ProgramCheckpointV1){(self.checkpoint.borrow_mut())(at,self.handle)}
    async fn name_yield(&self){yield_scientific_v1().await;self.notify(ProgramCheckpointV1::Task)}
    fn science_error(e:ScientificErrorV1)->ProgramErrorCodeV1{match e{ScientificErrorV1::Allocation=>ProgramErrorCodeV1::Allocation,ScientificErrorV1::Resource=>ProgramErrorCodeV1::Resource,ScientificErrorV1::Cancelled=>ProgramErrorCodeV1::Cancelled,ScientificErrorV1::Shape|ScientificErrorV1::Rank=>ProgramErrorCodeV1::Shape,ScientificErrorV1::Type=>ProgramErrorCodeV1::Type,ScientificErrorV1::Index=>ProgramErrorCodeV1::Index,ScientificErrorV1::Unsupported=>ProgramErrorCodeV1::UnsupportedFeature,_=>ProgramErrorCodeV1::Value(e)}}
    async fn work(&mut self,mut n:u64)->R<()>{self.poll()?;let mut scope=operation_scope!(self,0,OperationKind::General).map_err(Self::science_error)?;while n!=0{let part=n.min(128);scope.checkpoint(part).await.map_err(Self::science_error)?;n-=part}Ok(())}
    fn tracked(&mut self,v:ProgramValueV1)->R<ProgramValueV1>{self.poll()?;Ok(v)}
    async fn copy(&mut self,v:&ProgramValueV1)->R<ProgramValueV1>{self.poll()?;let mut scope=operation_scope!(self,0,OperationKind::General).map_err(Self::science_error)?;let available=scope.available_bytes();let candidate=v.try_copy_cooperative(available,&mut scope).await.map_err(Self::science_error)?;scope.retain(candidate).map_err(Self::science_error)}
    async fn copy_binding(&mut self,index:usize)->R<ProgramValueV1>{self.poll()?;let frame=self.frames.len()-1;let mut scope=operation_scope!(self,0,OperationKind::General).map_err(Self::science_error)?;let available=scope.available_bytes();let candidate=self.frames[frame].bindings[index].value.try_copy_cooperative(available,&mut scope).await.map_err(Self::science_error)?;scope.retain(candidate).map_err(Self::science_error)}
    async fn double(&mut self,shape:[u64;2],words:&[f64])->R<ProgramValueV1>{let shape=ScientificShapeV1::new(&shape).map_err(Self::science_error)?;let mut scope=operation_scope!(self,0,OperationKind::General).map_err(Self::science_error)?;let candidate=ScientificDenseV1::from_real_cooperative(shape,words,&mut scope).await.map_err(Self::science_error)?;scope.retain(ProgramValueV1::from_dense(candidate)).map_err(Self::science_error)}
    async fn number(&mut self,n:f64)->R<ProgramValueV1>{self.double([1,1],&[n]).await}
    async fn boolean(&mut self,b:bool)->R<ProgramValueV1>{let mut scope=operation_scope!(self,0,OperationKind::General).map_err(Self::science_error)?;let candidate=operations::boolean(b,&mut scope).await.map_err(Self::science_error)?;scope.retain(candidate).map_err(Self::science_error)}
    fn scalar(v:&ProgramValueV1)->R<f64>{v.scalar().map_err(Self::science_error)}
    async fn truth(&mut self,v:&ProgramValueV1)->R<bool>{let mut scope=operation_scope!(self,0,OperationKind::General).map_err(Self::science_error)?;mathlab_language::program::scientific_truth_v2(v,&mut scope).await.map_err(Self::science_error)}
    fn transpose_operation(&self,c:bool)->O{O::Transpose(c)}
    async fn finish_numeric(&mut self,_:NodeIdV1,op:O,a:ProgramValueV1,b:Option<ProgramValueV1>)->R<()>{
        let inputs=a.payload_bytes().checked_add(b.as_ref().map_or(0,ProgramValueV1::payload_bytes)).ok_or(ProgramErrorCodeV1::Resource)?;
        let kind=if a.is_sparse()&&matches!(op,O::Binary(BinaryOperatorV1::Multiply)){OperationKind::SparseProduct}else{OperationKind::General};
        let mut scope=operation_scope!(self,inputs,kind).map_err(Self::science_error)?;let available=scope.available_bytes();
        let(candidate,diagnostic)=operations::numeric(op,a,b,available,&mut scope).await.map_err(Self::science_error)?;
        scope.record_solve(diagnostic).map_err(Self::science_error)?;let candidate=scope.retain(candidate).map_err(Self::science_error)?;self.value(candidate)
    }
    async fn full_axis_value(&mut self,_:u64)->R<ProgramValueV1>{self.double([0,0],&[]).await}
    fn index_extent(&self,value:&ProgramValueV1,count:usize,index:usize)->R<u64>{if value.is_sparse(){return Err(ProgramErrorCodeV1::UnsupportedFeature)}if count==1{Ok(value.logical_numel())}else if count==value.rank(){Ok(value.axes()[index])}else{Err(ProgramErrorCodeV1::Arity)}}
    fn loop_columns(&self,value:&ProgramValueV1)->R<u64>{if value.is_sparse()||value.rank()!=2{Err(ProgramErrorCodeV1::UnsupportedFeature)}else{Ok(value.axes()[1])}}
    async fn range(&mut self,id:NodeIdV1,has_step:bool)->R<()>{self.current=id;let stop=self.pop()?;let step=if has_step{Some(self.pop()?)}else{None};let start=self.pop()?;let inputs=start.payload_bytes().checked_add(stop.payload_bytes()).and_then(|n|n.checked_add(step.as_ref().map_or(0,ProgramValueV1::payload_bytes))).ok_or(ProgramErrorCodeV1::Resource)?;let mut scope=operation_scope!(self,inputs,OperationKind::General).map_err(Self::science_error)?;let available=scope.available_bytes();let candidate=operations::range(start,step,stop,available,&mut scope).await.map_err(Self::science_error)?;let candidate=scope.retain(candidate).map_err(Self::science_error)?;self.value(candidate)}
    async fn for_next(&mut self,variable:NodeIdV1,body:NodeIdV1,frozen:ProgramValueV1,column:usize)->R<()>{
        if column>=self.loop_columns(&frozen)? as usize{self.release(frozen);return Ok(())}
        let mut scope=operation_scope!(self,0,OperationKind::General).map_err(Self::science_error)?;let available=scope.available_bytes();let candidate=operations::column(&frozen,column,available,&mut scope).await.map_err(Self::science_error)?;let candidate=scope.retain(candidate).map_err(Self::science_error)?;
        self.one(Name::Node(variable),candidate,false,true).await?;self.push(Task::ForNext{variable,body,frozen,column:column+1})?;self.push(Task::Exec(body))
    }
    async fn finish_builtin(&mut self,_site:NodeIdV1,id:NodeIdV1,base:usize,count:usize)->R<()>{
        let module=Arc::clone(&self.module);let name=module.source_text(module.node(id).unwrap().span).unwrap();if !operations::valid_arity(name,count){return Err(ProgramErrorCodeV1::Arity)}
        let mut args:[Option<ProgramValueV1>;64]=std::array::from_fn(|_|None);for i in (0..count).rev(){args[i]=Some(self.pop()?)}if self.values.len()!=base{return Err(ProgramErrorCodeV1::Arity)}
        let inputs=operations::input_bytes(&args,count).map_err(Self::science_error)?;let actual=self.frame().actual;let requested=self.frame().requested;let in_function=self.frames.len()>1;let kind=if name=="sparse"{OperationKind::SparseConstructor}else{OperationKind::General};
        let mut scope=operation_scope!(self,inputs,kind).map_err(Self::science_error)?;let available=scope.available_bytes();let candidate=operations::builtin(name,args,count,actual,requested,in_function,available,&mut scope).await.map_err(Self::science_error)?;let candidate=scope.retain(candidate).map_err(Self::science_error)?;self.value(candidate)
    }
    async fn gather(&mut self,target:NodeIdV1,base:usize,count:usize,full:ProgramAxes)->R<ProgramValueV1>{
        let index=self.binding_index(self.text(target)).await.ok_or(ProgramErrorCodeV1::UnknownBinding)?;let frame=self.frames.len()-1;
        let mut scope=operation_scope!(self,0,OperationKind::General).map_err(Self::science_error)?;let available=scope.available_bytes();let candidate=operations::gather(&self.frames[frame].bindings[index].value,&self.values[base..base+count],full,available,&mut scope).await.map_err(Self::science_error)?;scope.retain(candidate).map_err(Self::science_error)
    }
    async fn replace_index(&mut self,target:NodeIdV1,base:usize,count:usize,full:ProgramAxes,rhs:&ProgramValueV1)->R<ProgramValueV1>{
        let index=self.binding_index(self.text(target)).await.ok_or(ProgramErrorCodeV1::UnknownBinding)?;let frame=self.frames.len()-1;
        let mut scope=operation_scope!(self,0,OperationKind::General).map_err(Self::science_error)?;let available=scope.available_bytes();let candidate=operations::replace(&self.frames[frame].bindings[index].value,&self.values[base..base+count],full,rhs,available,&mut scope).await.map_err(Self::science_error)?;scope.retain(candidate).map_err(Self::science_error)
    }
    async fn matrix(&mut self,id:NodeIdV1,base:usize)->R<()>{
        let N::Matrix{rows}=self.kind(id)else{unreachable!()};let mut rows=rows;let mut info=[(0usize,0usize,0usize,0usize);256];let mut row_count=0;let mut value_index=base;let mut total_rows=0usize;let mut columns=None;let mut logical=true;let mut class=None;let mut complex=false;
        while let Some(list)=rows{self.poll()?;let(row,next)=self.list(list);rows=next;let N::Row{items}=self.kind(row)else{unreachable!()};let start=value_index-base;let mut count=0;let mut height=None;let mut width=0usize;let mut items=items;
            while let Some(list)=items{self.work(1).await?;let(_,next)=self.list(list);items=next;let value=self.values.get(value_index).ok_or(ProgramErrorCodeV1::Shape)?;let dense=value.dense_view().map_err(Self::science_error)?;let shape=dense.shape();if shape.rank()!=2{return Err(ProgramErrorCodeV1::Shape)}let axes=shape.axes();
                if axes!=[0,0]{if height.is_some_and(|n|n!=axes[0] as usize){return Err(ProgramErrorCodeV1::Shape)}height=Some(axes[0] as usize);width=width.checked_add(axes[1] as usize).ok_or(ProgramErrorCodeV1::Resource)?;if class.is_some_and(|c|c!=value.class()){return Err(ProgramErrorCodeV1::Type)}class=Some(value.class());logical=value.class()==ProgramValueClassV1::Logical;complex|=value.is_complex();}value_index+=1;count+=1;
            }
            let height=height.unwrap_or(0);if height!=0||width!=0{if columns.is_some_and(|n|n!=width){return Err(ProgramErrorCodeV1::Shape)}columns=Some(width)}if row_count==256{return Err(ProgramErrorCodeV1::Resource)}info[row_count]=(start,count,height,width);row_count+=1;total_rows=total_rows.checked_add(height).ok_or(ProgramErrorCodeV1::Resource)?;
        }
        if value_index!=self.values.len(){return Err(ProgramErrorCodeV1::Shape)}if class.is_none(){logical=false}
        let mut scope=operation_scope!(self,0,OperationKind::General).map_err(Self::science_error)?;let available=scope.available_bytes();let candidate=operations::matrix(&self.values[base..],&info[..row_count],total_rows,columns.unwrap_or(0),logical,complex,available,&mut scope).await.map_err(Self::science_error)?;let candidate=scope.retain(candidate).map_err(Self::science_error)?;self.discard_values(base);self.value(candidate)
    }
}
};}
use install_scientific_v2 as install_value_profile;
#[path="machine.rs"]mod machine;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScientificProgramErrorCodeV2 {
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
    Value(ScientificErrorV1),
}
impl ScientificProgramErrorCodeV2{pub fn is_allocation(&self)->bool{matches!(self,Self::Allocation|Self::Value(ScientificErrorV1::Allocation))}}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScientificProgramErrorV2 {
    pub code: ScientificProgramErrorCodeV2,
    pub location: ProgramLocationV1,
    pub call_stack: [Option<ProgramLocationV1>; 32],
    pub call_depth: usize,
}
#[derive(Debug, Clone, Copy)]
pub struct ScientificProgramOptionsV2 {
    pub deadline: Option<Instant>,
    pub transition_fuel: u64,
    pub primitive_fuel: u64,
}
impl Default for ScientificProgramOptionsV2 {
    fn default() -> Self {
        Self {
            deadline: None,
            transition_fuel: 5_000_000,
            primitive_fuel: 1_000_000_000,
        }
    }
}
#[derive(Debug)]
pub struct ScientificProgramEventV2 {
    pub sequence: u64,
    pub location: ProgramLocationV1,
    pub function: Option<NodeIdV1>,
    pub name: Option<String>,
    pub value: ScientificProgramValueV2,
}
#[derive(Debug)]
pub struct ScientificProgramOutcomeV2 {
    pub profile: &'static str,
    pub key: ProgramRequestKeyV1,
    pub start: ProgramContextV1,
    pub final_context: ProgramContextV1,
    pub committed_effect_sequence: u64,
    pub completed_leaf_statement_count: u64,
    pub stats: ProgramExecutionStatsV1,
    pub events: Vec<ScientificProgramEventV2>,
    pub status: ProgramStatusV1,
    pub error: Option<ScientificProgramErrorV2>,
}
struct Binding {
    name: String,
    value: ScientificProgramValueV2,
}
pub struct ScientificProgramSessionV2 {
    session_id: u64,
    epoch: u64,
    request_id: u64,
    revision: u64,
    bindings: Vec<Binding>,
    #[cfg(test)]
    fail_allocation: Option<usize>,
}
/// Borrowed read-only workspace entry; no mutable storage or value copy escapes.
pub struct ScientificProgramBindingViewV2<'a> {
    pub name: &'a str,
    pub value: &'a ScientificProgramValueV2,
}
impl ScientificProgramSessionV2 {
    pub fn new() -> Result<Self, ScientificProgramErrorCodeV2> {
        let session_id = NEXT_ID
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| ScientificProgramErrorCodeV2::CounterExhausted)?;
        let mut bindings = Vec::new();
        bindings
            .try_reserve_exact(64)
            .map_err(|_| ScientificProgramErrorCodeV2::Allocation)?;
        if bindings.capacity() > 64 {
            return Err(ScientificProgramErrorCodeV2::Resource);
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
    pub fn bindings(&self) -> impl ExactSizeIterator<Item = ScientificProgramBindingViewV2<'_>> {
        self.bindings.iter().map(|b| ScientificProgramBindingViewV2 {
            name: &b.name,
            value: &b.value,
        })
    }
    pub fn binding(&self,name:&str)->Option<&ScientificProgramValueV2>{self.bindings.iter().find(|b|b.name==name).map(|b|&b.value)}
    pub fn reset(&mut self) -> Result<(), ScientificProgramErrorCodeV2> {
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or(ScientificProgramErrorCodeV2::CounterExhausted)?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(ScientificProgramErrorCodeV2::CounterExhausted)?;
        self.bindings.clear();
        self.epoch = epoch;
        self.revision = revision;
        Ok(())
    }
    pub fn prepare(
        &mut self,
        module: Arc<ProgramModuleV1>,
        options: ScientificProgramOptionsV2,
    ) -> Result<PreparedScientificProgramRequestV2<'_>, ScientificProgramErrorCodeV2> {
        if options.transition_fuel > 5_000_000 || options.primitive_fuel > 1_000_000_000 {
            return Err(ScientificProgramErrorCodeV2::Resource);
        }
        let request_id = self
            .request_id
            .checked_add(1)
            .ok_or(ScientificProgramErrorCodeV2::CounterExhausted)?;
        #[cfg(test)]
        if self.fail_allocation == Some(0) {
            return Err(ScientificProgramErrorCodeV2::Allocation);
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
        Ok(PreparedScientificProgramRequestV2 {
            owner: self,
            module,
            options,
            handle,
            invoked: false,
        })
    }
}
pub struct PreparedScientificProgramRequestV2<'a> {
    owner: &'a mut ScientificProgramSessionV2,
    module: Arc<ProgramModuleV1>,
    options: ScientificProgramOptionsV2,
    handle: ProgramCancelHandleV1,
    invoked: bool,
}
impl PreparedScientificProgramRequestV2<'_> {
    pub fn cancel_handle(&self) -> ProgramCancelHandleV1 {
        self.handle.clone()
    }
    /// Allocation-free cooperative execution. Dropping a suspended future
    /// abandons its key and restores completed caller publications.
    pub async fn execute_cooperatively(mut self,mut checkpoint:impl FnMut(ProgramCheckpointV1,&ProgramCancelHandleV1))->ScientificProgramOutcomeV2{
        self.invoked=true;
        machine::execute_async(self.owner,Arc::clone(&self.module),self.options,&self.handle,&mut checkpoint).await
    }
    pub fn execute(self) -> ScientificProgramOutcomeV2 {
        self.execute_controlled(|_, _| {})
    }
    pub fn execute_controlled(
        mut self,
        mut checkpoint: impl FnMut(ProgramCheckpointV1, &ProgramCancelHandleV1),
    ) -> ScientificProgramOutcomeV2 {
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
impl Drop for PreparedScientificProgramRequestV2<'_> {
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


#[cfg(test)]
mod resource_tests_v2{
    use super::*;
    fn module(s:&str)->Arc<ProgramModuleV1>{Arc::new(parse_program_v1(2102,Arc::from(s)).unwrap())}
    #[test]
    fn every_sparse_adapter_kernel_copy_and_display_allocation_fault_keeps_exact_prefix(){
        let src="a=7\ni=[2;1;2];j=[2;1;2];v=[3;4;5];A=sparse(i,j,v,2,2);saved=A;Y=A*[1;2];A=sparse(1,1,9,2,2);after=1;";
        let mut success=false;let mut failures=0;
        for fail in 1..512{let mut s=ScientificProgramSessionV2::new().unwrap();s.fail_allocation=Some(fail);let out=s.prepare(module(src),Default::default()).unwrap().execute();
            if out.status==ProgramStatusV1::Success{assert!(!out.stats.ledger_poisoned);success=true;break}failures+=1;assert_eq!(out.status,ProgramStatusV1::Resource,"fault{fail}: {:?}",out.error);assert_eq!(out.error.unwrap().code,ScientificProgramErrorCodeV2::Allocation);assert!(!out.stats.ledger_poisoned);let revision=out.final_context.revision;assert!(revision<=9);
            for(i,name)in ["a","i","j","v","A","saved","Y"].into_iter().enumerate(){assert_eq!(s.binding(name).is_some(),revision>i as u64,"fault{fail} {name}")}
            assert_eq!(s.binding("after").is_some(),revision==9);assert_eq!(out.events.len(),(revision>=1) as usize);if revision>=1{assert_eq!(out.events[0].value.scalar().unwrap(),7.)}
            if let Some(a)=s.binding("A"){let a=a.csc_view().unwrap();if revision>=8{assert_eq!(a.row_indices(),[0]);match a.values(){ScientificCscValuesV1::Real(v)=>assert_eq!(v,[9.]),_=>panic!()}}else{assert_eq!(a.row_indices(),[0,1]);match a.values(){ScientificCscValuesV1::Real(v)=>assert_eq!(v,[4.,8.]),_=>panic!()}}}
            if let Some(saved)=s.binding("saved"){assert_eq!(saved.csc_view().unwrap().row_indices(),[0,1])}if let Some(y)=s.binding("Y"){assert_eq!(y.dense_component(0).unwrap().re,4.);assert_eq!(y.dense_component(1).unwrap().re,16.)}
            let live=s.bindings().map(|b|b.value.payload_bytes()).sum::<usize>();s.fail_allocation=None;let recovery=s.prepare(module("recovered=1;"),Default::default()).unwrap().execute();assert_eq!(recovery.status,ProgramStatusV1::Success);assert_eq!(recovery.stats.peak_live_payload_bytes,live+8);assert!(!recovery.stats.ledger_poisoned);
        }assert!(success&&failures>20,"must reach success after every prior actual allocation attempt");
    }
    #[test]
    fn control_arc_preflight_fault_is_resource_and_first_stop_survives(){let mut s=ScientificProgramSessionV2::new().unwrap();s.fail_allocation=Some(0);assert!(matches!(s.prepare(module("x=1;"),Default::default()),Err(ScientificProgramErrorCodeV2::Allocation)));s.fail_allocation=None;let request=s.prepare(module("x=sparse(1,1,9,2,2);"),Default::default()).unwrap();let handle=request.cancel_handle();assert!(matches!(handle.request_stop(),ProgramCancelResultV1::Accepted(_)));assert_eq!(request.execute().status,ProgramStatusV1::Cancelled(ProgramCancelReasonV1::UserStop));assert!(s.binding("x").is_none());}
}
