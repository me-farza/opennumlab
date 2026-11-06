// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use super::ProgramNodeKindV1 as N;
use super::ProgramNumericOperationV1 as O;
use std::{
    cell::Cell,
    mem::{size_of, take},
};
const LIVE: usize = super::PROFILE_LIVE_BYTES;
const META: usize = 16_777_216;
type R<T> = Result<T, ProgramErrorCodeV1>;
struct Frame {
    bindings: Vec<Binding>,
    function: Option<NodeIdV1>,
    outputs: Option<NodeIdV1>,
    requested: usize,
    actual: usize,
    task_base: usize,
    value_base: usize,
    site: NodeIdV1,
}
declare_machine_modes!{
    Function(NodeIdV1),
    Builtin(NodeIdV1),
    Index(NodeIdV1),
    Write {
        target: NodeIdV1,
        rhs: NodeIdV1,
        suppressed: bool,
    },
}

declare_machine_tasks!{
    Block(Option<NodeIdV1>),
    Exec(NodeIdV1),
    Eval(NodeIdV1, usize, Option<u64>),
    Unary(NodeIdV1, O),
    Binary(NodeIdV1, BinaryOperatorV1),
    Short(NodeIdV1, BinaryOperatorV1, NodeIdV1, Option<u64>),
    ShortEnd(NodeIdV1),
    Range(NodeIdV1, bool),
    Matrix(NodeIdV1, usize),
    Args {
        site: NodeIdV1,
        next: Option<NodeIdV1>,
        index: usize,
        count: usize,
        base: usize,
        requested: usize,
        mode: Mode,
        full: ProgramAxes,
    },
    Leaf {
        node: NodeIdV1,
        base: usize,
    },
    Indexed {
        node: NodeIdV1,
        target: NodeIdV1,
        base: usize,
        count: usize,
        full: ProgramAxes,
        suppressed: bool,
    },
    If {
        branches: Option<NodeIdV1>,
        otherwise: Option<NodeIdV1>,
    },
    IfChecked {
        condition: NodeIdV1,
        body: NodeIdV1,
        next: Option<NodeIdV1>,
        otherwise: Option<NodeIdV1>,
    },
    While(NodeIdV1, NodeIdV1),
    WhileChecked(NodeIdV1, NodeIdV1),
    ForReady(NodeIdV1, NodeIdV1),
    ForNext {
        variable: NodeIdV1,
        body: NodeIdV1,
        frozen: ProgramValueV1,
        column: usize,
    },
    Return,
}

struct Machine<'a, 'b> {
    owner: &'a mut ProgramSessionV1,
    module: Arc<ProgramModuleV1>,
    options: ProgramExecutionOptionsV1,
    handle: &'b ProgramCancelHandleV1,
    checkpoint: super::ProgramCheckpointSink<'b>,
    frames: Vec<Frame>,
    tasks: Vec<Task>,
    values: Vec<ProgramValueV1>,
    events: Vec<ProgramEventV1>,
    stats: ProgramExecutionStatsV1,
    live: usize,
    metadata: usize,
    sequence: u64,
    completed: u64,
    current: NodeIdV1,
    allocations: usize,
    name_steps: Cell<u64>,
    name_error: Cell<Option<ProgramErrorCodeV1>>,
}
fn reserve<T>(n: usize) -> R<Vec<T>> {
    let mut v = Vec::new();
    v.try_reserve_exact(n)
        .map_err(|_| ProgramErrorCodeV1::Allocation)?;
    if v.capacity() > n {
        return Err(ProgramErrorCodeV1::Resource);
    }
    Ok(v)
}
fn reserve_at<T>(n: usize, site: usize, fault: Option<usize>) -> R<Vec<T>> {
    if fault == Some(site) {
        Err(ProgramErrorCodeV1::Allocation)
    } else {
        reserve(n)
    }
}
pub(super) async fn execute_async(
    owner: &mut ProgramSessionV1,
    module: Arc<ProgramModuleV1>,
    options: ProgramExecutionOptionsV1,
    handle: &ProgramCancelHandleV1,
    checkpoint: &mut dyn FnMut(ProgramCheckpointV1, &ProgramCancelHandleV1),
) -> ProgramOutcomeV1 {
    let start = owner.context();
    let origin = module.script();
    #[cfg(test)]
    let initial_fault = owner.fail_allocation;
    #[cfg(not(test))]
    let initial_fault = None;
    let initial: R<_> = (|| {
        let requested = 64usize
            + 33 * size_of::<Frame>()
            + 4096 * size_of::<Task>()
            + 4096 * size_of::<ProgramValueV1>()
            + 1024 * size_of::<ProgramEventV1>()
            + 64 * size_of::<Binding>()
            + 64 * 2048
            + profile_extra_metadata!(owner);
        if requested > META {
            return Err(ProgramErrorCodeV1::Resource);
        }
        Ok((
            reserve_at::<Frame>(33, 1, initial_fault)?,
            reserve_at::<Task>(4096, 2, initial_fault)?,
            reserve_at::<ProgramValueV1>(4096, 3, initial_fault)?,
            reserve_at::<ProgramEventV1>(1024, 4, initial_fault)?,
        ))
    })();
    let (mut frames, tasks, values, events) = match initial {
        Ok(v) => v,
        Err(code) => {
            let _ =
                handle
                    .state
                    .compare_exchange(RUNNING, FAILED, Ordering::AcqRel, Ordering::Acquire);
            return ProgramOutcomeV1 {
                profile: PROGRAM_PROFILE_V1,
                key: handle.key,
                start,
                final_context: start,
                committed_effect_sequence: 0,
                completed_leaf_statement_count: 0,
                stats: ProgramExecutionStatsV1::default(),
                events: Vec::new(),
                status: match handle.state.load(Ordering::Acquire) {
                    STOP => ProgramStatusV1::Cancelled(ProgramCancelReasonV1::UserStop),
                    DEADLINE => ProgramStatusV1::Cancelled(ProgramCancelReasonV1::Deadline),
                    _ => ProgramStatusV1::Resource,
                },
                error: Some(ProgramErrorV1 {
                    code,
                    location: initial_profile_location!(module,origin),
                    call_stack: [None; 32],
                    call_depth: 0,
                }),
            };
        }
    };
    let bindings = take(&mut owner.bindings);
    frames.push(Frame {
        bindings,
        function: None,
        outputs: None,
        requested: 0,
        actual: 0,
        task_base: 0,
        value_base: 0,
        site: origin,
    });
    let metadata = profile_extra_metadata!(owner) + 64
        + frames.capacity() * size_of::<Frame>()
        + tasks.capacity() * size_of::<Task>()
        + values.capacity() * size_of::<ProgramValueV1>()
        + events.capacity() * size_of::<ProgramEventV1>()
        + frames[0].bindings.capacity() * size_of::<Binding>()
        + frames[0]
            .bindings
            .iter()
            .map(|b| b.name.capacity())
            .sum::<usize>();
    let live = profile_extra_live!(owner) + frames[0]
        .bindings
        .iter()
        .map(|b| b.value.payload_bytes())
        .sum::<usize>();
    let mut m = Machine {
        owner,
        module,
        options,
        handle,
        checkpoint: super::checkpoint_sink(checkpoint),
        frames,
        tasks,
        values,
        events,
        stats: ProgramExecutionStatsV1 {
            peak_live_payload_bytes: live,
            peak_metadata_bytes: metadata,
            ..Default::default()
        },
        live,
        metadata,
        sequence: 0,
        completed: 0,
        current: origin,
        allocations: 4,
        name_steps: Cell::new(0),
        name_error: Cell::new(None),
    };
    let result = async {
        m.admit().await?;
        profile_start!(m);
        m.push(Task::Exec(origin))?;
        while let Some(task) = m.tasks.pop() {
            m.tick()?;
            profile_dispatch_task!(m,task);
        }
        m.poll()?;
        Ok(())
    }.await;
    let pending = m.poll().err();
    let code = pending.or(result.err());
    m.notify(ProgramCheckpointV1::BeforeTerminal);
    let mut stack = [None; 32];
    for (i, f) in m.frames.iter().skip(1).enumerate() {
        stack[i] = Some(m.location(f.site));
    }
    let error = code.map(|code| ProgramErrorV1 {
        code,
        location: m.location(m.current),
        call_stack: stack,
        call_depth: m.frames.len() - 1,
    });
    let desired = if code.is_none() { DONE } else { FAILED };
    let _ = handle
        .state
        .compare_exchange(RUNNING, desired, Ordering::AcqRel, Ordering::Acquire);
    m.notify(ProgramCheckpointV1::AfterTerminal);
    let state = handle.state.load(Ordering::Acquire);
    let status = match state {
        STOP | PENDING_STOP => ProgramStatusV1::Cancelled(ProgramCancelReasonV1::UserStop),
        DEADLINE | PENDING_DEADLINE => ProgramStatusV1::Cancelled(ProgramCancelReasonV1::Deadline),
        _ if code.is_some_and(|c| c == ProgramErrorCodeV1::Resource || c.is_allocation()) => {
            ProgramStatusV1::Resource
        }
        _ if code.is_some() => ProgramStatusV1::Error,
        _ => ProgramStatusV1::Success,
    };
    profile_finalize_stats!(m);
    m.owner.bindings = take(&mut m.frames[0].bindings);
    ProgramOutcomeV1 {
        profile: PROGRAM_PROFILE_V1,
        key: handle.key,
        start,
        final_context: m.owner.context(),
        committed_effect_sequence: m.sequence,
        completed_leaf_statement_count: m.completed,
        stats: m.stats,
        events: take(&mut m.events),
        status,
        error,
    }
}
impl Machine<'_, '_> {
    install_machine_syntax!();
    fn frame(&self) -> &Frame {
        self.frames.last().unwrap()
    }
    async fn names_equal(&self, a: &str, b: &str) -> bool {
        if a.len() != b.len() {
            return false;
        }
        for (a, b) in a.as_bytes().chunks(128).zip(b.as_bytes().chunks(128)) {
            self.name_yield().await;
            let n = self.name_steps.get().checked_add(a.len() as u64);
            if n.is_none_or(|n| {
                self.stats
                    .primitive_steps
                    .checked_add(n)
                    .is_none_or(|sum| sum > self.options.primitive_fuel)
            }) {
                self.name_error.set(Some(ProgramErrorCodeV1::Resource));
                return false;
            }
            self.name_steps.set(n.unwrap());
            if self.options.deadline.is_some_and(|d| Instant::now() >= d) {
                self.handle
                    .cancel_for(self.handle.key, ProgramCancelReasonV1::Deadline);
            }
            if matches!(self.handle.state.load(Ordering::Acquire), STOP | DEADLINE) {
                self.name_error.set(Some(ProgramErrorCodeV1::Cancelled));
                return false;
            }
            if a != b {
                return false;
            }
        }
        true
    }
    fn metadata_room(&self, bytes: usize) -> R<()> {
        if self.metadata.checked_add(bytes).is_none_or(|n| n > META) {
            Err(ProgramErrorCodeV1::Resource)
        } else {
            Ok(())
        }
    }
    async fn binding_index(&self,name:&str)->Option<usize>{
        for (index,binding) in self.frame().bindings.iter().enumerate(){if self.names_equal(&binding.name,name).await{return Some(index)}}None
    }
    async fn lookup(&self,name:&str)->Option<&ProgramValueV1>{self.binding_index(name).await.map(|index|&self.frame().bindings[index].value)}

    fn list(&self, id: NodeIdV1) -> (NodeIdV1, Option<NodeIdV1>) {
        let N::List { item, next } = self.kind(id) else {
            unreachable!()
        };
        (item, next)
    }
    async fn count(&mut self, mut list: Option<NodeIdV1>) -> R<usize> {
        let mut n = 0;
        while let Some(id) = list {
            self.poll()?;
            n += 1;
            if n > 64 {
                return Err(ProgramErrorCodeV1::Resource);
            }
            list = self.list(id).1;
        }
        Ok(n)
    }
    async fn admit(&mut self) -> R<()> {
        for i in 0..self.module.nodes().len() {
            self.poll()?;
            let n = self.module.nodes()[i];
            let Some(admitted_kind)=profile_admit_kind!(n) else{continue};
            if matches!(admitted_kind, N::Name) && n.span.end - n.span.start > 2048 {
                return Err(ProgramErrorCodeV1::Resource);
            }
            if let N::Function {
                inputs, outputs, ..
            } = admitted_kind
            {
                self.count(inputs).await?;
                self.count(outputs).await?;
                for mut list in [inputs, outputs] {
                    while let Some(id) = list {
                        let (item, next) = self.list(id);
                        if matches!(self.text(item), "varargin" | "varargout") {
                            self.current = item;
                            return Err(ProgramErrorCodeV1::UnsupportedFeature);
                        }
                        list = next;
                    }
                }
            }
        }
        Ok(())
    }

    fn poll(&mut self) -> R<()> {
        self.primitive_checkpoint();
        self.stats.primitive_steps = self
            .stats
            .primitive_steps
            .checked_add(self.name_steps.replace(0))
            .filter(|n| *n <= self.options.primitive_fuel)
            .ok_or(ProgramErrorCodeV1::Resource)?;
        if let Some(code) = self.name_error.take() {
            return Err(code);
        }
        if self.options.deadline.is_some_and(|d| Instant::now() >= d) {
            self.handle
                .cancel_for(self.handle.key, ProgramCancelReasonV1::Deadline);
        }
        if matches!(
            self.handle.state.load(Ordering::Acquire),
            STOP | DEADLINE | PENDING_STOP | PENDING_DEADLINE
        ) {
            return Err(ProgramErrorCodeV1::Cancelled);
        }
        Ok(())
    }
    fn tick(&mut self) -> R<()> {
        self.notify(ProgramCheckpointV1::Task);
        self.poll()?;
        self.stats.transitions = self
            .stats
            .transitions
            .checked_add(1)
            .filter(|n| *n <= self.options.transition_fuel)
            .ok_or(ProgramErrorCodeV1::Resource)?;
        Ok(())
    }

    fn allocation(&mut self) -> R<()> {
        self.allocations += 1;
        #[cfg(test)]
        if self.owner.fail_allocation == Some(self.allocations) {
            return Err(ProgramErrorCodeV1::Allocation);
        }
        Ok(())
    }
    fn room(&mut self, bytes: usize) -> R<()> {
        self.poll()?;
        if self.live.checked_add(bytes).is_none_or(|n| n > LIVE) {
            return Err(ProgramErrorCodeV1::Resource);
        }
        Ok(())
    }
    fn meta(&mut self, bytes: usize) -> R<()> {
        self.metadata = self
            .metadata
            .checked_add(bytes)
            .filter(|n| *n <= META)
            .ok_or(ProgramErrorCodeV1::Resource)?;
        self.stats.peak_metadata_bytes = self.stats.peak_metadata_bytes.max(self.metadata);
        Ok(())
    }
    fn release(&mut self, v: ProgramValueV1) {
        profile_release_value!(self,v);
    }




    fn push(&mut self, t: Task) -> R<()> {
        if self.tasks.len() == 4096 {
            return Err(ProgramErrorCodeV1::Resource);
        }
        self.tasks.push(t);
        Ok(())
    }
    fn value(&mut self, v: ProgramValueV1) -> R<()> {
        if self.values.len() == 4096 {
            return Err(ProgramErrorCodeV1::Resource);
        }
        self.values.push(v);
        Ok(())
    }
    fn pop(&mut self) -> R<ProgramValueV1> {
        self.values.pop().ok_or(ProgramErrorCodeV1::Arity)
    }

    async fn named_copy(&mut self,name:NodeIdV1)->R<ProgramValueV1>{let index=self.binding_index(self.text(name)).await.ok_or(ProgramErrorCodeV1::UnknownBinding)?;self.copy_binding(index).await}


    async fn function(&self,name:&str)->Option<NodeIdV1>{for &id in self.module.functions(){if let N::Function{name:n,..}=self.kind(id){if profile_function_visible!(self,id)&&self.names_equal(self.text(n),name).await{return Some(id)}}}None}


    async fn owned_name(&mut self, name: &str) -> R<String> {
        self.work(name.len() as u64).await?;
        self.allocation()?;
        profile_metadata_admission!(self,name.len());
        let mut s = String::new();
        s.try_reserve_exact(name.len())
            .map_err(|_| ProgramErrorCodeV1::Allocation)?;
        if s.capacity() > name.len() {
            return Err(ProgramErrorCodeV1::Resource);
        }
        self.meta(s.capacity())?;
        for chunk in name.as_bytes().chunks(128) {
            self.poll()?;
            s.push_str(std::str::from_utf8(chunk).unwrap());
        }
        Ok(s)
    }
    async fn name_copy(&mut self, name: Name) -> R<String> {
        match name {
            Name::Ans => self.owned_name("ans").await,
            Name::Node(id) => {
                let span = self.module.node(id).unwrap().span;
                self.work((span.end - span.start) as u64).await?;
                self.allocation()?;
                profile_metadata_admission!(self,span.end - span.start);
                let mut s = String::new();
                s.try_reserve_exact(span.end - span.start)
                    .map_err(|_| ProgramErrorCodeV1::Allocation)?;
                if s.capacity() > span.end - span.start {
                    return Err(ProgramErrorCodeV1::Resource);
                }
                self.meta(s.capacity())?;
                let mut offset = span.start;
                while offset < span.end {
                    self.poll()?;
                    let end = (offset + 128).min(span.end);
                    s.push_str(profile_name_slice!(self,id,offset,end));
                    offset = end;
                }
                Ok(s)
            }
        }
    }
    fn name_text(&self, name: Name) -> &str {
        match name {
            Name::Ans => "ans",
            Name::Node(id) => self.text(id),
        }
    }
    async fn publish(
        &mut self,
        names: &[Name],
        mut incoming: [Option<ProgramValueV1>; 64],
        display: bool,
        suppressed: bool,
    ) -> R<()> {
        let _profile_incoming_roots=profile_register_incoming!(self,&incoming);
        if names.len() > 64 {
            return Err(ProgramErrorCodeV1::Resource);
        }
        let mut writes: [Option<Write>; 64] = std::array::from_fn(|_| None);
        let mut displays: [Option<ProgramEventV1>; 64] = std::array::from_fn(|_| None);
        let mut new = 0;
        for (i, &name) in names.iter().enumerate() {
            self.poll()?;
            let value = incoming[i].take().ok_or(ProgramErrorCodeV1::Arity)?;
            // Earlier occurrences may have staged a new slot that is not yet
            // visible in bindings. Reuse it so aliases never create duplicates.
            let mut slot = None;
            for (j, previous) in names[..i].iter().enumerate() {
                if self.names_equal(self.name_text(*previous), self.name_text(name)).await {
                    slot = Some(writes[j].as_ref().unwrap().index);
                    break;
                }
            }
            if slot.is_none() {
                slot = self.binding_index(self.name_text(name)).await;
            }
            let owned = if slot.is_none() {
                new += 1;
                Some(self.name_copy(name).await?)
            } else {
                None
            };
            let index = slot.unwrap_or_else(|| self.frame().bindings.len() + new - 1);
            if self.frame().bindings.len() + new > 64 {
                return Err(ProgramErrorCodeV1::Resource);
            }
            if display && !suppressed {
                if self.events.len() + i >= 1024 {
                    return Err(ProgramErrorCodeV1::Resource);
                }
                let snapshot = profile_event_copy!(self,&value).await?;
                let label = self.name_copy(name).await?;
                displays[i] = Some(ProgramEventV1 {
                    sequence: 0,
                    location: self.location(self.current),
                    function: self.frame().function,
                    name: Some(label),
                    value: snapshot,
                });
            }
            writes[i] = Some(Write {
                index,
                name: owned,
                value,
            });
        }
        let external = self.frames.len() == 1 || display && !suppressed;
        let sequence = if external {
            self.sequence
                .checked_add(1)
                .ok_or(ProgramErrorCodeV1::CounterExhausted)?
        } else {
            self.sequence
        };
        let revision = if self.frames.len() == 1 && !names.is_empty() {
            self.owner
                .revision
                .checked_add(1)
                .ok_or(ProgramErrorCodeV1::CounterExhausted)?
        } else {
            self.owner.revision
        };
        let prepared_profile_publication=profile_prepare_publication!(self,&writes,external);
        self.notify(ProgramCheckpointV1::BeforePublish);
        self.poll()?;
        self.handle
            .state
            .compare_exchange(RUNNING, PUBLISHING, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ProgramErrorCodeV1::Cancelled)?;
        self.notify(ProgramCheckpointV1::AfterBarrier);
        // All capacities, snapshots, names and counters were prepared before the CAS.
        profile_commit_publication!(self,prepared_profile_publication);
        for i in 0..names.len() {
            let write = writes[i].take().unwrap();
            if let Some(name) = write.name {
                self.frames.last_mut().unwrap().bindings.push(Binding {
                    name,
                    value: write.value,
                });
            } else {
                let old = std::mem::replace(
                    &mut self.frames.last_mut().unwrap().bindings[write.index].value,
                    write.value,
                );
                self.release(old);
            }
            if let Some(mut event) = displays[i].take() {
                event.sequence = sequence;
                self.events.push(event);
            }
        }
        self.sequence = sequence;
        self.owner.revision = revision;
        match self.handle.state.compare_exchange(
            PUBLISHING,
            RUNNING,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {}
            Err(PENDING_STOP) => self.handle.state.store(STOP, Ordering::Release),
            Err(PENDING_DEADLINE) => self.handle.state.store(DEADLINE, Ordering::Release),
            Err(_) => unreachable!(),
        }
        self.notify(ProgramCheckpointV1::AfterPublish);
        Ok(())
    }
    async fn display(
        &mut self,
        value: ProgramValueV1,
        name: Option<NodeIdV1>,
        suppressed: bool,
    ) -> R<()> {
        if suppressed {
            self.release(value);
            return Ok(());
        }
        if self.events.len() == 1024 {
            return Err(ProgramErrorCodeV1::Resource);
        }
        let label = match name {
            Some(id) => Some(self.name_copy(Name::Node(id)).await?),
            None => None,
        };
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or(ProgramErrorCodeV1::CounterExhausted)?;
        let event = ProgramEventV1 {
            sequence,
            location: self.location(self.current),
            function: self.frame().function,
            name: label,
            value:profile_event_move!(self,value),
        };
        self.notify(ProgramCheckpointV1::BeforePublish);
        self.poll()?;
        self.handle
            .state
            .compare_exchange(RUNNING, PUBLISHING, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ProgramErrorCodeV1::Cancelled)?;
        self.notify(ProgramCheckpointV1::AfterBarrier);
        self.events.push(event);
        self.sequence = sequence;
        match self.handle.state.compare_exchange(
            PUBLISHING,
            RUNNING,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {}
            Err(PENDING_STOP) => self.handle.state.store(STOP, Ordering::Release),
            Err(PENDING_DEADLINE) => self.handle.state.store(DEADLINE, Ordering::Release),
            _ => unreachable!(),
        }
        self.notify(ProgramCheckpointV1::AfterPublish);
        Ok(())
    }
    async fn one(&mut self, name: Name, value: ProgramValueV1, display: bool, suppressed: bool) -> R<()> {
        let mut values = std::array::from_fn(|_| None);
        values[0] = Some(value);
        self.publish(&[name], values, display, suppressed).await
    }
    fn complete(&mut self) -> R<()> {
        self.completed = self
            .completed
            .checked_add(1)
            .ok_or(ProgramErrorCodeV1::CounterExhausted)?;
        Ok(())
    }
    fn discard_tasks(&mut self, base: usize) {
        while self.tasks.len() > base {
            if let Some(Task::ForNext { frozen, .. }) = self.tasks.pop() {
                self.release(frozen);
            }
        }
    }
    fn discard_values(&mut self, base: usize) {
        while self.values.len() > base {
            let v = self.values.pop().unwrap();
            self.release(v);
        }
    }
    async fn return_frame(&mut self) -> R<()> {
        profile_before_return!(self);
        if self.frames.len() == 1 {
            self.discard_tasks(0);
            self.discard_values(0);
            return Ok(());
        }
        let caller_requested = self.frame().requested;
        // A bare call exposes nargout0 but may still supply its assigned first
        // output to ans. Missing implicit outputs are not explicitly requested.
        let delivered = if caller_requested == 0 {
            if let Some(list) = self.frame().outputs {
                let (name, _) = self.list(list);
                usize::from(self.lookup(self.text(name)).await.is_some())
            } else {
                0
            }
        } else {
            caller_requested
        };
        self.poll()?;
        let mut next = self.frame().outputs;
        let mut results: [Option<ProgramValueV1>; 64] = std::array::from_fn(|_| None);
        // Only the object profile needs owned return leases. Legacy expands the
        // original non-enumerated copy loop, with a genuinely zero-sized guard.
        let _profile_result_roots=profile_copy_return_values!(self,results,next,delivered);
        let base = self.frame().task_base;
        let vbase = self.frame().value_base;
        self.discard_tasks(base);
        self.discard_values(vbase);
        let frame = self.frames.pop().unwrap();
        self.metadata -= frame.bindings.capacity() * size_of::<Binding>();
        for b in frame.bindings {
            self.metadata -= b.name.capacity();
            self.release(b.value);
        }
        for v in results.iter_mut().take(delivered) {
            self.value(v.take().unwrap())?;
        }
        Ok(())
    }
    async fn step(&mut self, task: Task) -> R<()> {
        #[allow(unreachable_patterns)]
        profile_match_task!(self,task;
            Task::Block(list) => {
                if let Some(id) = list {
                    let (item, next) = self.list(id);
                    self.push(Task::Block(next))?;
                    self.push(Task::Exec(item))?;
                }
            }
            Task::Exec(id) => {
                self.current = id;
                profile_before_exec!(self,id);
                match self.kind(id) {
                    N::Block { statements } => self.push(Task::Block(statements))?,
                    N::ExpressionStatement { expression, .. } => {
                        let callee = match self.kind(expression) {
                            N::Apply { callee, .. } if matches!(self.kind(callee), N::Name) => {
                                Some(callee)
                            }
                            N::Name => Some(expression),
                            _ => None,
                        };
                        let requested = if let Some(callee) = callee {
                            if self.lookup(self.text(callee)).await.is_none() {
                                if self.function(self.text(callee)).await.is_some() {
                                    0
                                } else {
                                    1
                                }
                            } else {
                                1
                            }
                        } else {
                            1
                        };
                        let base = self.values.len();
                        self.push(Task::Leaf { node: id, base })?;
                        self.push(Task::Eval(expression, requested, None))?;
                    }
                    N::Assignment {
                        targets,
                        value,
                        suppressed,
                    } => {
                        let base = self.values.len();
                        match targets {
                            AssignmentTargetsV1::Single(target) => {
                                if let N::Apply { callee, arguments } = self.kind(target) {
                                    if !matches!(self.kind(callee), N::Name) {
                                        return Err(ProgramErrorCodeV1::UnsupportedFeature);
                                    }
                                    self.lookup(self.text(callee)).await
                                        .ok_or(ProgramErrorCodeV1::UnknownBinding)?;
                                    let count = self.count(arguments).await?;
                                    if !(1..=super::PROFILE_INDEX_AXES).contains(&count) {
                                        return Err(ProgramErrorCodeV1::Arity);
                                    }
                                    self.push(Task::Args {
                                        site: id,
                                        next: arguments,
                                        index: 0,
                                        count,
                                        base,
                                        requested: 1,
                                        mode: Mode::Write {
                                            target: callee,
                                            rhs: value,
                                            suppressed,
                                        },
                                        full: ProgramAxes::default(),
                                    })?;
                                } else {
                                    self.push(Task::Leaf { node: id, base })?;
                                    self.push(Task::Eval(value, 1, None))?;
                                }
                            }
                            AssignmentTargetsV1::Multiple(list) => {
                                let count = self.count(Some(list)).await?;
                                self.push(Task::Leaf { node: id, base })?;
                                self.push(Task::Eval(value, count, None))?;
                            }
                        }
                    }
                    N::If {
                        branches,
                        otherwise,
                    } => self.push(Task::If {
                        branches,
                        otherwise,
                    })?,
                    N::While { condition, body } => self.push(Task::While(condition, body))?,
                    N::For {
                        variable,
                        iterable,
                        body,
                    } => {
                        self.push(Task::ForReady(variable, body))?;
                        self.push(Task::Eval(iterable, 1, None))?;
                    }
                    N::Break | N::Continue => {
                        let base = self.frame().task_base;
                        let position = (base..self.tasks.len())
                            .rev()
                            .find(|&i| {
                                matches!(self.tasks[i], Task::ForNext { .. } | Task::While(..))
                            })
                            .ok_or(ProgramErrorCodeV1::UnsupportedFeature)?;
                        self.discard_tasks(if matches!(self.kind(id), N::Break) {
                            position
                        } else {
                            position + 1
                        });
                        self.complete()?;
                    }
                    N::Return => {
                        self.complete()?;
                        self.return_frame().await?;
                    }
                    _ => return Err(ProgramErrorCodeV1::UnsupportedFeature),
                }
            }
            Task::Eval(id, requested, extent) => {
                self.current = id;
                profile_before_eval!(self,id,requested,extent);
                if requested != 1 && !matches!(self.kind(id), N::Apply { .. } | N::Name) {
                    return Err(ProgramErrorCodeV1::Arity);
                }
                match self.kind(id) {
                    N::Number { bits } => {
                        let v = self.number(f64::from_bits(bits)).await?;
                        self.value(v)?;
                    }
                    N::Name => {
                        let name = self.text(id);
                        if self.lookup(name).await.is_some() {
                            let v = self.named_copy(id).await?;
                            self.value(v)?;
                        } else if let Some(function) = self.function(name).await {
                            self.enter_call(function, self.values.len(), 0, requested, id).await?;
                        } else if matches!(name, "nargin" | "nargout") && self.frames.len() > 1 {
                            let n = if name == "nargin" {
                                self.frame().actual
                            } else {
                                self.frame().requested
                            };
                            let v = self.number(n as f64).await?;
                            self.value(v)?;
                        } else {
                            return Err(ProgramErrorCodeV1::UnknownBinding);
                        }
                    }
                    N::End => {
                        let n = extent.ok_or(ProgramErrorCodeV1::Index)?;
                        let v = self.number(n as f64).await?;
                        self.value(v)?;
                    }
                    N::Group { value } => self.push(Task::Eval(value, requested, extent))?,
                    N::Unary { operator, input } => {
                        self.push(Task::Unary(id, O::Unary(operator)))?;
                        self.push(Task::Eval(input, 1, extent))?;
                    }
                    N::Transpose { input, conjugate } => {
                        self.push(Task::Unary(id, self.transpose_operation(conjugate)))?;
                        self.push(Task::Eval(input, 1, extent))?;
                    }
                    N::Binary {
                        operator,
                        left,
                        right,
                    } => {
                        if matches!(
                            operator,
                            BinaryOperatorV1::ShortCircuitAnd | BinaryOperatorV1::ShortCircuitOr
                        ) {
                            self.push(Task::Short(id, operator, right, extent))?;
                            self.push(Task::Eval(left, 1, extent))?;
                        } else {
                            self.push(Task::Binary(id, operator))?;
                            self.push(Task::Eval(right, 1, extent))?;
                            self.push(Task::Eval(left, 1, extent))?;
                        }
                    }
                    N::Range { start, step, stop } => {
                        self.push(Task::Range(id, step.is_some()))?;
                        self.push(Task::Eval(stop, 1, extent))?;
                        if let Some(step) = step {
                            self.push(Task::Eval(step, 1, extent))?;
                        }
                        self.push(Task::Eval(start, 1, extent))?;
                    }
                    N::Matrix { rows } => {
                        let base = self.values.len();
                        self.push(Task::Matrix(id, base))?;
                        let mut ids = [None; 256];
                        let mut count = 0;
                        let mut row = rows;
                        while let Some(r) = row {
                            let (item, next) = self.list(r);
                            row = next;
                            let N::Row { items } = self.kind(item) else {
                                unreachable!()
                            };
                            let mut list = items;
                            while let Some(i) = list {
                                self.poll()?;
                                if count == 256 {
                                    return Err(ProgramErrorCodeV1::Resource);
                                }
                                let (item, next) = self.list(i);
                                ids[count] = Some(item);
                                count += 1;
                                list = next;
                            }
                        }
                        for id in ids[..count].iter().rev() {
                            self.push(Task::Eval(id.unwrap(), 1, extent))?;
                        }
                    }
                    N::Apply { callee, arguments } => {
                        if !matches!(self.kind(callee), N::Name) {
                            return Err(ProgramErrorCodeV1::UnsupportedFeature);
                        }
                        let count = self.count(arguments).await?;
                        let name = self.text(callee);
                        let mode = if self.lookup(name).await.is_some() {
                            if !(1..=super::PROFILE_INDEX_AXES).contains(&count) || requested != 1 {
                                return Err(ProgramErrorCodeV1::Arity);
                            }
                            Mode::Index(callee)
                        } else if let Some(f) = self.function(name).await {
                            self.check_call(f, count, requested).await?;
                            Mode::Function(f)
                        } else {
                            if requested != 1 {
                                return Err(ProgramErrorCodeV1::Arity);
                            }
                            Mode::Builtin(callee)
                        };
                        self.push(Task::Args {
                            site: id,
                            next: arguments,
                            index: 0,
                            count,
                            base: self.values.len(),
                            requested,
                            mode,
                            full: ProgramAxes::default(),
                        })?;
                    }
                    _ => return Err(ProgramErrorCodeV1::UnsupportedFeature),
                }
            }
            Task::Unary(id, op) => {
                self.current = id;
                let a = self.pop()?;
                self.finish_numeric(id, op, a, None).await?;
            }
            Task::Binary(id, op) => {
                self.current = id;
                let b = self.pop()?;
                let a = self.pop()?;
                self.finish_numeric(id, O::Binary(op), a, Some(b)).await?;
            }
            Task::Short(id, op, right, extent) => {
                self.current = id;
                let a = self.pop()?;
                let truth = Self::scalar(&a)? != 0.0;
                self.release(a);
                if (op == BinaryOperatorV1::ShortCircuitAnd && !truth)
                    || (op == BinaryOperatorV1::ShortCircuitOr && truth)
                {
                    let v = self.boolean(truth).await?;
                    self.value(v)?;
                } else {
                    self.push(Task::ShortEnd(id))?;
                    self.push(Task::Eval(right, 1, extent))?;
                }
            }
            Task::ShortEnd(id) => {
                self.current = id;
                let a = self.pop()?;
                let truth = Self::scalar(&a)? != 0.0;
                self.release(a);
                let v = self.boolean(truth).await?;
                self.value(v)?;
            }
            Task::Range(id, has_step) => self.range(id, has_step).await?,
            Task::Matrix(id, base) => {
                self.current = id;
                self.matrix(id, base).await?;
            }
            Task::Args {
                site,
                next,
                index,
                count,
                base,
                requested,
                mode,
                mut full,
            } => {
                self.current = site;
                if let Some(list) = next {
                    let (arg, tail) = self.list(list);
                    let extent = match mode {
                        Mode::Index(target) | Mode::Write { target, .. } => {
                            let value = self
                                .lookup(self.text(target)).await
                                .ok_or(ProgramErrorCodeV1::UnknownBinding)?;
                            Some(self.index_extent(value, count, index)?)
                        }
                        _ => None,
                    };
                    self.push(Task::Args {
                        site,
                        next: tail,
                        index: index + 1,
                        count,
                        base,
                        requested,
                        mode,
                        full: {
                            if self.is_full_axis(arg) && index < super::PROFILE_INDEX_AXES {
                                full[index] = true;
                            }
                            full
                        },
                    })?;
                    if self.is_full_axis(arg) {
                        let v = self.full_axis_value(extent.ok_or(ProgramErrorCodeV1::Index)?).await?;
                        self.value(v)?;
                    } else {
                        self.push(Task::Eval(arg, 1, extent))?;
                    }
                } else {
                    #[allow(unreachable_patterns)]
                    profile_match_mode!(self,mode,base,count,requested,site;
                        Mode::Function(f) => self.enter_call(f, base, count, requested, site).await?,
                        Mode::Builtin(name) => {
                            profile_builtin!(self,site,name,base,count,requested).await?;
                        }
                        Mode::Index(target) => {
                            let v = self.gather(target, base, count, full).await?;
                            self.discard_values(base);
                            self.value(v)?;
                        }
                        Mode::Write {
                            target,
                            rhs,
                            suppressed,
                        } => {
                            self.push(Task::Indexed {
                                node: site,
                                target,
                                base,
                                count,
                                full,
                                suppressed,
                            })?;
                            self.push(Task::Eval(rhs, 1, None))?;
                        }
                    );
                }
            }
            Task::Leaf { node, base } => {
                profile_leaf_task!(self,node,base);
            }
            Task::Indexed {
                node,
                target,
                base,
                count,
                full,
                suppressed,
            } => {
                self.current = node;
                let rhs = self.pop()?;
                let replacement = self.replace_index(target, base, count, full, &rhs).await?;
                self.release(rhs);
                self.discard_values(base);
                self.one(Name::Node(target), replacement, true, suppressed).await?;
                self.complete()?;
            }
            Task::If {
                branches,
                otherwise,
            } => {
                if let Some(id) = branches {
                    let (branch, next) = self.list(id);
                    let N::IfBranch { condition, body } = self.kind(branch) else {
                        unreachable!()
                    };
                    self.push(Task::IfChecked {
                        condition,
                        body,
                        next,
                        otherwise,
                    })?;
                    self.push(Task::Eval(condition, 1, None))?;
                } else if let Some(body) = otherwise {
                    self.push(Task::Exec(body))?;
                }
            }
            Task::IfChecked {
                condition,
                body,
                next,
                otherwise,
            } => {
                self.current = condition;
                let v = self.pop()?;
                let yes = self.truth(&v).await?;
                self.release(v);
                self.push(if yes {
                    Task::Exec(body)
                } else {
                    Task::If {
                        branches: next,
                        otherwise,
                    }
                })?;
            }
            Task::While(condition, body) => {
                self.push(Task::WhileChecked(condition, body))?;
                self.push(Task::Eval(condition, 1, None))?;
            }
            Task::WhileChecked(condition, body) => {
                self.current = condition;
                let v = self.pop()?;
                let yes = self.truth(&v).await?;
                self.release(v);
                if yes {
                    self.push(Task::While(condition, body))?;
                    self.push(Task::Exec(body))?;
                }
            }
            Task::ForReady(variable, body) => {
                let frozen = self.pop()?;
                if self.loop_columns(&frozen)? == 0 {
                    // Empty column iteration still initializes the loop
                    // variable, through the same atomic publication barrier.
                    let empty = self.double([0, 0], &[]).await?;
                    self.one(Name::Node(variable), empty, false, true).await?;
                    self.release(frozen);
                    return Ok(());
                }
                self.push(Task::ForNext {
                    variable,
                    body,
                    frozen,
                    column: 0,
                })?;
            }
            Task::ForNext { variable, body, frozen, column } => self.for_next(variable, body, frozen, column).await?,
            Task::Return => self.return_frame().await?,
        );
        Ok(())
    }
    async fn check_call(&mut self, id: NodeIdV1, actual: usize, requested: usize) -> R<()> {
        let N::Function {
            inputs, outputs, ..
        } = self.kind(id)
        else {
            unreachable!()
        };
        if actual > self.count(inputs).await? || requested > self.count(outputs).await? {
            return Err(ProgramErrorCodeV1::Arity);
        }
        Ok(())
    }
    async fn enter_call(
        &mut self,
        id: NodeIdV1,
        base: usize,
        actual: usize,
        requested: usize,
        site: NodeIdV1,
    ) -> R<()> {
        self.work(1).await?;
        self.check_call(id, actual, requested).await?;
        if self.frames.len() > 32 {
            return Err(ProgramErrorCodeV1::Resource);
        }
        let N::Function {
            inputs,
            outputs,
            body,
            ..
        } = self.kind(id)
        else {
            unreachable!()
        };
        self.allocation()?;
        profile_metadata_admission!(self,64 * size_of::<Binding>());
        let mut bindings = reserve::<Binding>(64)?;
        self.meta(bindings.capacity() * size_of::<Binding>())?;
        let mut args: [Option<ProgramValueV1>; 64] = std::array::from_fn(|_| None);
        for i in (0..actual).rev() {
            args[i] = Some(self.pop()?);
        }
        let _profile_call_arg_roots=profile_register_incoming!(self,&args);
        if self.values.len() != base {
            return Err(ProgramErrorCodeV1::Arity);
        }
        let mut input = inputs;
        for arg in args.iter_mut().take(actual) {
            let (name, next) = self.list(input.ok_or(ProgramErrorCodeV1::Arity)?);
            input = next;
            let name = self.name_copy(Name::Node(name)).await?;
            bindings.push(Binding {
                name,
                value: arg.take().unwrap(),
            });
        }
        let task_base = self.tasks.len();
        self.frames.push(Frame {
            bindings,
            function: Some(id),
            outputs,
            requested,
            actual,
            task_base,
            value_base: base,
            site,
        });
        self.push(Task::Return)?;
        self.push(Task::Exec(body))?;
        Ok(())
    }
    async fn finish_leaf(&mut self, id: NodeIdV1, base: usize) -> R<()> {
        match self.kind(id) {
            N::Assignment {
                targets,
                suppressed,
                ..
            } => {
                let mut names = [Name::Ans; 64];
                let count = match targets {
                    AssignmentTargetsV1::Single(target) => {
                        names[0] = Name::Node(target);
                        1
                    }
                    AssignmentTargetsV1::Multiple(list) => {
                        let mut next = Some(list);
                        let mut n = 0;
                        while let Some(id) = next {
                            if n == 64 {
                                return Err(ProgramErrorCodeV1::Resource);
                            }
                            let (item, tail) = self.list(id);
                            next = tail;
                            names[n] = Name::Node(item);
                            n += 1;
                        }
                        n
                    }
                };
                if self.values.len() != base + count {
                    return Err(ProgramErrorCodeV1::Arity);
                }
                let mut values = std::array::from_fn(|_| None);
                for i in (0..count).rev() {
                    values[i] = Some(self.pop()?);
                }
                self.publish(&names[..count], values, true, suppressed).await
            }
            N::ExpressionStatement {
                expression,
                suppressed,
            } => {
                if self.values.len() == base {
                    return Ok(());
                }
                if self.values.len() != base + 1 {
                    return Err(ProgramErrorCodeV1::Arity);
                }
                let value = self.pop()?;
                if matches!(self.kind(expression), N::Name)
                    && self.lookup(self.text(expression)).await.is_some()
                {
                    self.display(value, Some(expression), suppressed).await
                } else {
                    self.one(Name::Ans, value, true, suppressed).await
                }
            }
            _ => Err(ProgramErrorCodeV1::UnsupportedFeature),
        }
    }





}
#[derive(Clone, Copy)]
enum Name {
    Node(NodeIdV1),
    Ans,
}
struct Write {
    index: usize,
    name: Option<String>,
    value: ProgramValueV1,
}


// Installed into this exact private scope: no public polymorphism or enum widening.
super::install_value_profile!();

// Restores every completed caller publication if a cooperative future is dropped
// while a preparation helper owns unpublished candidate buffers.
impl Drop for Machine<'_, '_>{fn drop(&mut self){profile_on_drop!(self);if let Some(frame)=self.frames.first_mut(){if frame.bindings.capacity()!=0{self.owner.bindings=take(&mut frame.bindings);}}let _=self.handle.state.compare_exchange(RUNNING,ABANDONED,Ordering::AcqRel,Ordering::Acquire);}}
fn drive<F:std::future::Future>(future:F)->F::Output{let mut future=std::pin::pin!(future);let mut cx=std::task::Context::from_waker(std::task::Waker::noop());loop{if let std::task::Poll::Ready(value)=future.as_mut().poll(&mut cx){return value}}}
pub(super) fn execute(owner:&mut ProgramSessionV1,module:Arc<ProgramModuleV1>,options:ProgramExecutionOptionsV1,handle:&ProgramCancelHandleV1,checkpoint:&mut dyn FnMut(ProgramCheckpointV1,&ProgramCancelHandleV1))->ProgramOutcomeV1{drive(execute_async(owner,module,options,handle,checkpoint))}
