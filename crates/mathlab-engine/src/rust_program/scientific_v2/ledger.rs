// SPDX-License-Identifier: GPL-3.0-or-later
//! Stack-only operation reservation ownership; no shared allocator or COW.
use super::*;
#[derive(Debug,Clone,Copy)]
pub(super) enum OperationKind{General,SparseConstructor,SparseProduct}
pub(super) struct OperationScope<'a,'b>{
    stats:&'a mut ScientificProgramStatsV2,live:&'a mut usize,allocations:&'a mut usize,
    options:ScientificProgramOptionsV2,handle:&'a ProgramCancelHandleV1,checkpoint:&'a ProgramCheckpointSink<'b>,
    input_bytes:usize,reserved_bytes:usize,scope_units:u64,committed:bool,kind:OperationKind,fault:Option<usize>,
}
impl<'a,'b> OperationScope<'a,'b>{
    pub(super) fn new(stats:&'a mut ScientificProgramStatsV2,live:&'a mut usize,allocations:&'a mut usize,options:ScientificProgramOptionsV2,handle:&'a ProgramCancelHandleV1,checkpoint:&'a ProgramCheckpointSink<'b>,input_bytes:usize,kind:OperationKind,fault:Option<usize>)->Result<Self,ScientificErrorV1>{
        if input_bytes>*live||*live>PROFILE_LIVE_BYTES||stats.ledger_poisoned{stats.ledger_poisoned=true;return Err(ScientificErrorV1::Resource)}
        Ok(Self{stats,live,allocations,options,handle,checkpoint,input_bytes,reserved_bytes:0,scope_units:0,committed:false,kind,fault})
    }
    pub(super) fn available_bytes(&self)->usize{PROFILE_LIVE_BYTES-*self.live}
    fn poll(&mut self)->Result<(),ScientificErrorV1>{
        (self.checkpoint.borrow_mut())(ProgramCheckpointV1::Task,self.handle);
        if self.options.deadline.is_some_and(|d|Instant::now()>=d){self.handle.cancel_for(self.handle.key,ProgramCancelReasonV1::Deadline);}
        if matches!(self.handle.state.load(Ordering::Acquire),STOP|DEADLINE|PENDING_STOP|PENDING_DEADLINE){return Err(ScientificErrorV1::Cancelled)}
        if self.stats.ledger_poisoned{return Err(ScientificErrorV1::Resource)}Ok(())
    }
    pub(super) fn record_solve(&mut self,diagnostic:Option<ScientificSolveDiagnosticsV1>)->Result<(),ScientificErrorV1>{
        if let Some(d)=diagnostic{self.stats.solves_completed=self.stats.solves_completed.checked_add(1).ok_or(ScientificErrorV1::Resource)?;self.stats.last_solve=Some(d);self.stats.last_solve_complete=true;if let Some(g)=d.growth_factor(){self.stats.maximum_growth=Some(self.stats.maximum_growth.map_or(g,|old|old.max(g)));}}Ok(())
    }
    /// Takes ownership of the candidate until its charge is transferred. On any
    /// invariant/counter error candidate destruction precedes ledger unwind.
    pub(super) fn retain(mut self,output:ScientificProgramValueV2)->Result<ScientificProgramValueV2,ScientificErrorV1>{
        let bytes=output.payload_bytes();let result=(||{
            let owned=self.input_bytes.checked_add(self.reserved_bytes).ok_or_else(||{self.stats.ledger_poisoned=true;ScientificErrorV1::Resource})?;
            if bytes>owned{self.stats.ledger_poisoned=true;return Err(ScientificErrorV1::Resource)}
            let next=self.live.checked_sub(owned).and_then(|n|n.checked_add(bytes)).filter(|n|*n<=PROFILE_LIVE_BYTES).ok_or_else(||{self.stats.ledger_poisoned=true;ScientificErrorV1::Resource})?;
            let operations=self.stats.operations_completed.checked_add(1).ok_or(ScientificErrorV1::Resource)?;
            let constructors=self.stats.sparse_constructors.checked_add(matches!(self.kind,OperationKind::SparseConstructor) as u64).ok_or(ScientificErrorV1::Resource)?;
            let products=self.stats.sparse_products.checked_add(matches!(self.kind,OperationKind::SparseProduct) as u64).ok_or(ScientificErrorV1::Resource)?;
            Ok((next,operations,constructors,products))
        })();
        match result{Ok((next,operations,constructors,products))=>{*self.live=next;self.stats.operations_completed=operations;self.stats.sparse_constructors=constructors;self.stats.sparse_products=products;self.committed=true;Ok(output)},Err(e)=>{drop(output);Err(e)}}
    }
}
impl ScientificControlV1 for OperationScope<'_,'_>{
    fn charge(&mut self,units:u64)->Result<(),ScientificErrorV1>{
        self.poll()?;if units>32768{return Err(ScientificErrorV1::Resource)}
        let total=self.stats.primitive_steps.checked_add(units).filter(|n|*n<=self.options.primitive_fuel).ok_or(ScientificErrorV1::Resource)?;
        let scoped=self.scope_units.checked_add(units).ok_or(ScientificErrorV1::Resource)?;let quantums=scoped/32768-self.scope_units/32768;
        let transitions=self.stats.transitions.checked_add(quantums).filter(|n|*n<=self.options.transition_fuel).ok_or(ScientificErrorV1::Resource)?;
        self.scope_units=scoped;self.stats.primitive_steps=total;self.stats.transitions=transitions;Ok(())
    }
    fn reserve(&mut self,bytes:usize)->Result<(),ScientificErrorV1>{
        self.charge(0)?;if self.options.primitive_fuel==0{return Err(ScientificErrorV1::Resource)}
        let next=self.live.checked_add(bytes).filter(|n|*n<=PROFILE_LIVE_BYTES).ok_or(ScientificErrorV1::Resource)?;
        let reserved=self.reserved_bytes.checked_add(bytes).ok_or(ScientificErrorV1::Resource)?;
        *self.allocations=self.allocations.checked_add(1).ok_or(ScientificErrorV1::Resource)?;
        if self.fault==Some(*self.allocations){return Err(ScientificErrorV1::Allocation)}
        *self.live=next;self.reserved_bytes=reserved;self.stats.peak_live_payload_bytes=self.stats.peak_live_payload_bytes.max(next);self.stats.peak_operation_reserved_bytes=self.stats.peak_operation_reserved_bytes.max(reserved);Ok(())
    }
}
impl Drop for OperationScope<'_,'_>{fn drop(&mut self){if !self.committed{match self.input_bytes.checked_add(self.reserved_bytes).and_then(|owned|self.live.checked_sub(owned)){Some(next)=>*self.live=next,None=>self.stats.ledger_poisoned=true}}}}
