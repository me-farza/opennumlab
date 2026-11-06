// SPDX-License-Identifier: GPL-3.0-or-later
// Private source specialization, not a public framework. The Legacy expansion
// uses its original concrete value/task/frame types and original leaf bodies.
macro_rules! install_legacy {
()=>{
struct Selection {
    indices: [[usize; 256]; 2],
    counts: [usize; 2],
    shape: [u64; 2],
}
impl Machine<'_ ,'_>{
    async fn work(&mut self, n: u64) -> R<()> {
        self.poll()?;
        self.stats.primitive_steps = self
            .stats
            .primitive_steps
            .checked_add(n)
            .filter(|n| *n <= self.options.primitive_fuel)
            .ok_or(ProgramErrorCodeV1::Resource)?;
        Ok(())
    }
    async fn name_yield(&self){}
    fn primitive_checkpoint(&mut self){}
    fn notify(&mut self,at:ProgramCheckpointV1){(self.checkpoint)(at,self.handle);}

    fn tracked(&mut self, v: ProgramValueV1) -> R<ProgramValueV1> {
        self.room(v.payload_bytes())?;
        self.live += v.payload_bytes();
        self.stats.peak_live_payload_bytes = self.stats.peak_live_payload_bytes.max(self.live);
        Ok(v)
    }

    async fn truth(&mut self, v: &ProgramValueV1) -> R<bool> {
        self.work(v.len() as u64).await?;
        if v.is_empty() {
            return Ok(false);
        }
        let mut yes = true;
        for i in 0..v.len() {
            if i % 128 == 0 {
                self.poll()?
            }
            let word = v.word(i).unwrap();
            // A NaN cannot be converted to a condition. Scan the complete
            // nonempty value even after a zero (qualified array policy).
            if word.is_nan() {
                return Err(ProgramErrorCodeV1::Type);
            }
            yes &= word != 0.0;
        }
        Ok(yes)
    }

    async fn copy(&mut self, v: &ProgramValueV1) -> R<ProgramValueV1> {
        self.room(v.payload_bytes())?;
        self.work(v.len() as u64).await?;
        self.allocation()?;
        let out = v.try_copy().map_err(ProgramErrorCodeV1::Value)?;
        self.tracked(out)
    }

    async fn number(&mut self, n: f64) -> R<ProgramValueV1> {
        self.double([1, 1], &[n]).await
    }

    async fn double(&mut self, shape: [u64; 2], words: &[f64]) -> R<ProgramValueV1> {
        self.room(words.len() * 8)?;
        self.work(words.len() as u64).await?;
        self.allocation()?;
        let v = ProgramValueV1::try_double(shape, words).map_err(ProgramErrorCodeV1::Value)?;
        self.tracked(v)
    }

    async fn boolean(&mut self, b: bool) -> R<ProgramValueV1> {
        self.room(1)?;
        self.allocation()?;
        let v =
            ProgramValueV1::try_logical([1, 1], &[b as u8]).map_err(ProgramErrorCodeV1::Value)?;
        self.tracked(v)
    }

    fn scalar(v: &ProgramValueV1) -> R<f64> {
        v.scalar().map_err(ProgramErrorCodeV1::Value)
    }

    async fn numeric(
        &mut self,
        op: O,
        a: ProgramValueV1,
        b: Option<ProgramValueV1>,
    ) -> R<ProgramValueV1> {
        let (shape, class) = program_numeric_output_layout_v1(op, &a, b.as_ref())
            .map_err(ProgramErrorCodeV1::Value)?;
        self.room(
            (shape[0] * shape[1]) as usize
                * if class == ProgramValueClassV1::Double {
                    8
                } else {
                    1
                },
        )?;
        self.work(32768).await?;
        self.allocation()?;
        let handle = self.handle;
        let deadline = self.options.deadline;
        let v = evaluate_program_numeric_controlled_v1(op, &a, b.as_ref(), || {
            if deadline.is_some_and(|d| Instant::now() >= d) {
                handle.cancel_for(handle.key, ProgramCancelReasonV1::Deadline);
            }
            matches!(handle.state.load(Ordering::Acquire), STOP | DEADLINE)
        })
        .map_err(ProgramErrorCodeV1::Value)?;
        let v = self.tracked(v)?;
        self.release(a);
        if let Some(b) = b {
            self.release(b)
        }
        Ok(v)
    }

    async fn matrix(&mut self, id: NodeIdV1, base: usize) -> R<()> {
        let N::Matrix { rows } = self.kind(id) else {
            unreachable!()
        };
        if rows.is_none() {
            let v = self.double([0, 0], &[]).await?;
            return self.value(v);
        }
        let mut nextrow = rows;
        let mut index = base;
        let mut outrows = 0usize;
        let mut outcols = None;
        let class = self
            .values
            .get(base)
            .ok_or(ProgramErrorCodeV1::Shape)?
            .class();
        let mut words = [0.0; 256];
        let mut bools = [0u8; 256];
        // First derive the full rectangular layout without allocating output payload.
        while let Some(row) = nextrow {
            self.poll()?;
            let (item, next) = self.list(row);
            nextrow = next;
            let N::Row { items } = self.kind(item) else {
                unreachable!()
            };
            let mut next = items;
            let mut rows = None;
            let mut cols = 0;
            while let Some(list) = next {
                let (_, tail) = self.list(list);
                next = tail;
                let v = self.values.get(index).ok_or(ProgramErrorCodeV1::Shape)?;
                if v.class() != class {
                    return Err(ProgramErrorCodeV1::Type);
                }
                let shape = v.shape();
                if rows.is_some_and(|r| r != shape[0] as usize) {
                    return Err(ProgramErrorCodeV1::Shape);
                }
                rows = Some(shape[0] as usize);
                cols += shape[1] as usize;
                index += 1;
            }
            if outcols.is_some_and(|c| c != cols) {
                return Err(ProgramErrorCodeV1::Shape);
            }
            outcols = Some(cols);
            outrows += rows.unwrap_or(0);
        }
        let outcols = outcols.unwrap_or(0);
        if outrows > 16 || outcols > 16 {
            return Err(ProgramErrorCodeV1::Resource);
        }
        let mut nextrow = rows;
        let mut index = base;
        let mut rowoffset = 0;
        while let Some(row) = nextrow {
            let (item, next) = self.list(row);
            nextrow = next;
            let N::Row { items } = self.kind(item) else {
                unreachable!()
            };
            let mut next = items;
            let mut coloffset = 0;
            let mut height = 0;
            while let Some(list) = next {
                let (_, tail) = self.list(list);
                next = tail;
                let shape = self.values[index].shape();
                height = shape[0] as usize;
                self.work((shape[0] * shape[1]) as u64).await?;
                for col in 0..shape[1] as usize {
                    for row in 0..height {
                        self.work(1).await?;
                        let offset = rowoffset + row + (coloffset + col) * outrows;
                        let v = self.values[index].word(row + col * height).unwrap();
                        words[offset] = v;
                        bools[offset] = v as u8;
                    }
                }
                coloffset += shape[1] as usize;
                index += 1;
            }
            rowoffset += height;
        }
        let n = outrows * outcols;
        let value = if class == ProgramValueClassV1::Double {
            self.double([outrows as u64, outcols as u64], &words[..n]).await?
        } else {
            self.room(n)?;
            self.allocation()?;
            let v = ProgramValueV1::try_logical([outrows as u64, outcols as u64], &bools[..n])
                .map_err(ProgramErrorCodeV1::Value)?;
            self.tracked(v)?
        };
        self.discard_values(base);
        self.value(value)
    }

    async fn builtin(&mut self, id: NodeIdV1, base: usize, count: usize) -> R<ProgramValueV1> {
        let mut args: [Option<ProgramValueV1>; 64] = std::array::from_fn(|_| None);
        for i in (0..count).rev() {
            args[i] = Some(self.pop()?);
        }
        if self.values.len() != base {
            return Err(ProgramErrorCodeV1::Arity);
        }
        let name = self.text(id);
        let output = match name {
            "sum" | "mean" | "prod" => {
                if count != 1 {
                    return Err(ProgramErrorCodeV1::Arity);
                }
                let op = match name {
                    "sum" => O::Sum,
                    "mean" => O::Mean,
                    _ => O::Prod,
                };
                return self.numeric(op, args[0].take().unwrap(), None).await;
            }
            "nargin" | "nargout" => {
                if count != 0 || self.frames.len() == 1 {
                    return Err(ProgramErrorCodeV1::Arity);
                }
                self.number(if name == "nargin" {
                    self.frame().actual
                } else {
                    self.frame().requested
                } as f64).await?
            }
            "numel" => {
                if count != 1 {
                    return Err(ProgramErrorCodeV1::Arity);
                }
                self.number(args[0].as_ref().unwrap().len() as f64).await?
            }
            "size" => {
                if count != 1 {
                    return Err(ProgramErrorCodeV1::Arity);
                }
                let shape = args[0].as_ref().unwrap().shape();
                self.double([1, 2], &[shape[0] as f64, shape[1] as f64]).await?
            }
            "zeros" | "ones" | "eye" | "reshape" => {
                let reshape = name == "reshape";
                if (reshape && count != 3) || (!reshape && !(1..=2).contains(&count)) {
                    return Err(ProgramErrorCodeV1::Arity);
                }
                let offset = reshape as usize;
                let dimension = |v: &ProgramValueV1| -> R<u64> {
                    if v.class() != ProgramValueClassV1::Double {
                        return Err(ProgramErrorCodeV1::Type);
                    }
                    let x = Self::scalar(v)?;
                    if !x.is_finite() || x.fract() != 0.0 || !(0.0..=16.0).contains(&x) {
                        return Err(ProgramErrorCodeV1::Shape);
                    }
                    Ok(x as u64)
                };
                let rows = dimension(args[offset].as_ref().unwrap())?;
                let cols = if !reshape && count == 1 {
                    rows
                } else {
                    dimension(args[offset + 1].as_ref().unwrap())?
                };
                let n = (rows * cols) as usize;
                let mut words = [0.0; 256];
                let mut bools = [0u8; 256];
                if reshape {
                    let source = args[0].as_ref().unwrap();
                    if source.len() != n {
                        return Err(ProgramErrorCodeV1::Shape);
                    }
                    for i in 0..n {
                        self.work(1).await?;
                        words[i] = source.word(i).unwrap();
                        bools[i] = words[i] as u8;
                    }
                    if source.class() == ProgramValueClassV1::Logical {
                        self.room(n)?;
                        self.allocation()?;
                        let v = ProgramValueV1::try_logical([rows, cols], &bools[..n])
                            .map_err(ProgramErrorCodeV1::Value)?;
                        self.tracked(v)?
                    } else {
                        self.double([rows, cols], &words[..n]).await?
                    }
                } else {
                    if name == "ones" {
                        words[..n].fill(1.0)
                    } else if name == "eye" {
                        for i in 0..rows.min(cols) as usize {
                            words[i + i * rows as usize] = 1.0;
                        }
                    }
                    self.double([rows, cols], &words[..n]).await?
                }
            }
            _ => return Err(ProgramErrorCodeV1::UnsupportedFeature),
        };
        for arg in args.into_iter().take(count).flatten() {
            self.release(arg);
        }
        Ok(output)
    }

    async fn selection(
        &mut self,
        target: NodeIdV1,
        base: usize,
        count: usize,
        full: [bool; 2],
    ) -> R<Selection> {
        let value = self
            .lookup(self.text(target)).await
            .ok_or(ProgramErrorCodeV1::UnknownBinding)?;
        let shape = value.shape();
        let len = value.len();
        let mut selection = Selection {
            indices: [[0; 256]; 2],
            counts: [0; 2],
            shape: [0; 2],
        };
        for axis in 0..count {
            self.work(256).await?;
            let extent = if count == 1 {
                len
            } else {
                shape[axis] as usize
            };
            let arg = self
                .values
                .get(base + axis)
                .ok_or(ProgramErrorCodeV1::Index)?;
            let ash = arg.shape();
            if arg.class() != ProgramValueClassV1::Double
                || !(ash[0] == 1 || ash[1] == 1 || ash == [0, 0])
                || arg.len() > 16
            {
                return Err(ProgramErrorCodeV1::Index);
            }
            selection.counts[axis] = arg.len();
            for i in 0..arg.len() {
                let word = arg.word(i).unwrap();
                if !word.is_finite() || word.fract() != 0.0 || word < 1.0 || word > extent as f64 {
                    return Err(ProgramErrorCodeV1::Index);
                }
                selection.indices[axis][i] = word as usize - 1;
            }
        }
        selection.shape = if count == 2 {
            [selection.counts[0] as u64, selection.counts[1] as u64]
        } else if full[0] {
            [selection.counts[0] as u64, 1]
        } else if (self.values[base].shape()[0] == 1 || self.values[base].shape()[1] == 1)
            && shape[0] == 1
            && shape[1] != 1
        {
            [1, selection.counts[0] as u64]
        } else if (self.values[base].shape()[0] == 1 || self.values[base].shape()[1] == 1)
            && shape[1] == 1
            && shape[0] != 1
        {
            [selection.counts[0] as u64, 1]
        } else {
            self.values[base].shape()
        };
        Ok(selection)
    }

    async fn gather(
        &mut self,
        target: NodeIdV1,
        base: usize,
        count: usize,
        full: [bool; 2],
    ) -> R<ProgramValueV1> {
        let target_slot = self.binding_index(self.text(target)).await
            .ok_or(ProgramErrorCodeV1::UnknownBinding)?;

        let s = self.selection(target, base, count, full).await?;
        let rows = (&self.frame().bindings[target_slot].value).shape()[0] as usize;
        let n = (s.shape[0] * s.shape[1]) as usize;
        let mut words = [0.0; 256];
        let mut bools = [0u8; 256];
        let class = (&self.frame().bindings[target_slot].value).class();
        self.work(n as u64).await?;
        for i in 0..n {
            self.work(1).await?;
            let offset = if count == 1 {
                s.indices[0][i]
            } else {
                s.indices[0][i % s.counts[0]] + s.indices[1][i / s.counts[0]] * rows
            };
            words[i] = (&self.frame().bindings[target_slot].value)
                .word(offset)
                .ok_or(ProgramErrorCodeV1::Index)?;
            bools[i] = words[i] as u8;
        }
        if class == ProgramValueClassV1::Double {
            self.double(s.shape, &words[..n]).await
        } else {
            self.room(n)?;
            self.allocation()?;
            let v = ProgramValueV1::try_logical(s.shape, &bools[..n])
                .map_err(ProgramErrorCodeV1::Value)?;
            self.tracked(v)
        }
    }

    async fn replace_index(
        &mut self,
        target: NodeIdV1,
        base: usize,
        count: usize,
        full: [bool; 2],
        rhs: &ProgramValueV1,
    ) -> R<ProgramValueV1> {
        let target_slot = self.binding_index(self.text(target)).await
            .ok_or(ProgramErrorCodeV1::UnknownBinding)?;

        let selection = self.selection(target, base, count, full).await?;
        let n = (selection.shape[0] * selection.shape[1]) as usize;
        if rhs.shape() == [0, 0] {
            return Err(ProgramErrorCodeV1::UnsupportedFeature);
        }
        let shape = (&self.frame().bindings[target_slot].value).shape();
        let len = (&self.frame().bindings[target_slot].value).len();
        let class = (&self.frame().bindings[target_slot].value).class();
        if class != rhs.class() {
            return Err(ProgramErrorCodeV1::Type);
        }
        if rhs.shape() != [1, 1] {
            if count == 1 {
                if rhs.len() != n {
                    return Err(ProgramErrorCodeV1::Shape);
                }
            } else {
                let mut expected = [1u64; 2];
                let mut actual = [1u64; 2];
                for (i, &n) in selection.counts.iter().filter(|&&n| n != 1).enumerate() {
                    expected[i] = n as u64;
                }
                for (i, n) in rhs.shape().into_iter().filter(|&n| n != 1).enumerate() {
                    actual[i] = n;
                }
                if expected != actual {
                    return Err(ProgramErrorCodeV1::Shape);
                }
            }
        }
        let mut words = [0.0; 256];
        let mut bools = [0u8; 256];
        self.work((len + n) as u64).await?;
        for (i, w) in words[..len].iter_mut().enumerate() {
            self.work(1).await?;
            *w = (&self.frame().bindings[target_slot].value).word(i).unwrap();
        }
        for i in 0..n {
            self.work(1).await?;
            let offset = if count == 1 {
                selection.indices[0][i]
            } else {
                selection.indices[0][i % selection.counts[0]]
                    + selection.indices[1][i / selection.counts[0]] * shape[0] as usize
            };
            words[offset] = rhs.word(if rhs.len() == 1 { 0 } else { i }).unwrap();
        }
        if class == ProgramValueClassV1::Double {
            self.double(shape, &words[..len]).await
        } else {
            for i in 0..len {
                bools[i] = words[i] as u8;
            }
            self.room(len)?;
            self.allocation()?;
            let v = ProgramValueV1::try_logical(shape, &bools[..len])
                .map_err(ProgramErrorCodeV1::Value)?;
            self.tracked(v)
        }
    }

    fn transpose_operation(&self, _: bool) -> O { O::Transpose }
    async fn finish_numeric(&mut self, _: NodeIdV1, op: O, a: ProgramValueV1, b: Option<ProgramValueV1>) -> R<()> {
        let v=self.numeric(op,a,b).await?;self.value(v)
    }
    async fn finish_builtin(&mut self, _: NodeIdV1, name: NodeIdV1, base: usize, count: usize) -> R<()> {
        let v=self.builtin(name,base,count).await?;self.value(v)
    }
    fn index_extent(&self,value:&ProgramValueV1,count:usize,index:usize)->R<u64>{Ok(if count==1{value.len() as u64}else{value.shape()[index]})}
    fn loop_columns(&self,value:&ProgramValueV1)->R<u64>{Ok(value.shape()[1])}
    async fn full_axis_value(&mut self, extent:u64)->R<ProgramValueV1>{
        let n=extent as usize;if n>16{return Err(ProgramErrorCodeV1::Resource)}
        let mut words=[0.;16];for(i,w)in words[..n].iter_mut().enumerate(){*w=(i+1) as f64;}self.double([1,n as u64],&words[..n]).await
    }
    async fn copy_binding(&mut self,index:usize)->R<ProgramValueV1>{
        let bytes=self.frame().bindings[index].value.payload_bytes();let n=self.frame().bindings[index].value.len();
        self.room(bytes)?;self.work(n as u64).await?;self.allocation()?;
        let v=self.frame().bindings[index].value.try_copy().map_err(ProgramErrorCodeV1::Value)?;self.tracked(v)
    }

    async fn range(&mut self,id:NodeIdV1,has_step:bool)->R<()>{
                self.current = id;
                let stop = self.pop()?;
                let step = if has_step { Some(self.pop()?) } else { None };
                let start = self.pop()?;
                let a = Self::scalar(&start)?;
                let b = if let Some(step) = &step {
                    Self::scalar(step)?
                } else {
                    1.0
                };
                let z = Self::scalar(&stop)?;
                if ![a, b, z].iter().all(|v| {
                    v.is_finite() && v.fract() == 0.0 && v.abs() <= 9_007_199_254_740_991.0
                }) || b == 0.0
                {
                    return Err(ProgramErrorCodeV1::UnsupportedFeature);
                }
                if start.class() != ProgramValueClassV1::Double
                    || stop.class() != ProgramValueClassV1::Double
                    || step
                        .as_ref()
                        .is_some_and(|v| v.class() != ProgramValueClassV1::Double)
                {
                    return Err(ProgramErrorCodeV1::Type);
                }
                let ai = a as i128;
                let bi = b as i128;
                let zi = z as i128;
                let count = if (bi > 0 && zi < ai) || (bi < 0 && zi > ai) {
                    0
                } else {
                    ((zi - ai) / bi + 1) as usize
                };
                if count > 16 {
                    return Err(ProgramErrorCodeV1::Resource);
                }
                let mut words = [0.0; 16];
                for (i, w) in words[..count].iter_mut().enumerate() {
                    *w = (ai + bi * i as i128) as f64;
                }
                let v = self.double([1, count as u64], &words[..count]).await?;
                self.release(start);
                self.release(stop);
                if let Some(step) = step {
                    self.release(step)
                }
                self.value(v)?;
            
Ok(())
}

    async fn for_next(&mut self,variable:NodeIdV1,body:NodeIdV1,frozen:ProgramValueV1,column:usize)->R<()>{
                if column < frozen.shape()[1] as usize {
                    let rows = frozen.shape()[0] as usize;
                    let mut words = [0.0; 16];
                    let mut bools = [0u8; 16];
                    for i in 0..rows {
                        words[i] = frozen.word(column * rows + i).unwrap();
                        bools[i] = words[i] as u8;
                    }
                    let value = if frozen.class() == ProgramValueClassV1::Double {
                        self.double([rows as u64, 1], &words[..rows]).await?
                    } else {
                        self.room(rows)?;
                        self.allocation()?;
                        let value = ProgramValueV1::try_logical([rows as u64, 1], &bools[..rows])
                            .map_err(ProgramErrorCodeV1::Value)?;
                        self.tracked(value)?
                    };
                    self.one(Name::Node(variable), value, false, true).await?;
                    self.push(Task::ForNext {
                        variable,
                        body,
                        frozen,
                        column: column + 1,
                    })?;
                    self.push(Task::Exec(body))?;
                } else {
                    self.release(frozen);
                }
            
Ok(())
}

}
#[cfg(test)]
mod boundaries {
    use super::*;
    #[test]
    fn task_value_and_metadata_structural_bounds() {
        let mut owner = ProgramSessionV1::new().unwrap();
        let module = Arc::new(parse_program_v1(1, Arc::from("")).unwrap());
        let handle = ProgramCancelHandleV1 {
            key: ProgramRequestKeyV1 {
                session_id: 1,
                epoch: 1,
                request_id: 1,
            },
            state: Arc::new(AtomicU8::new(RUNNING)),
        };
        let mut checkpoint = |_, _: &ProgramCancelHandleV1| {};
        let mut m = Machine {
            owner: &mut owner,
            module: module.clone(),
            options: Default::default(),
            handle: &handle,
            checkpoint: &mut checkpoint,
            frames: reserve(33).unwrap(),
            tasks: reserve(4096).unwrap(),
            values: reserve(4096).unwrap(),
            events: reserve(1024).unwrap(),
            stats: Default::default(),
            live: 0,
            metadata: 64,
            sequence: 0,
            completed: 0,
            current: module.script(),
            allocations: 4,
            name_steps: Cell::new(0),
            name_error: Cell::new(None),
        };
        for _ in 0..4096 {
            m.push(Task::Block(None)).unwrap();
            m.value(ProgramValueV1::try_double([0, 0], &[]).unwrap())
                .unwrap();
        }
        assert!(matches!(
            m.push(Task::Block(None)),
            Err(ProgramErrorCodeV1::Resource)
        ));
        assert!(matches!(
            m.value(ProgramValueV1::try_double([0, 0], &[]).unwrap()),
            Err(ProgramErrorCodeV1::Resource)
        ));
        m.metadata = META - 1;
        assert!(m.metadata_room(1).is_ok());
        assert!(matches!(
            m.metadata_room(2),
            Err(ProgramErrorCodeV1::Resource)
        ));
        m.live = LIVE - 8;
        assert!(m.room(8).is_ok());
        assert!(matches!(m.room(9), Err(ProgramErrorCodeV1::Resource)));
        // All visible capacities + maximal resident names + one staged publication.
        let bound = 64
            + 33 * size_of::<Frame>()
            + 4096 * size_of::<Task>()
            + 4096 * size_of::<ProgramValueV1>()
            + 1024 * size_of::<ProgramEventV1>()
            + 34 * 64 * size_of::<Binding>()
            + (33 * 64 + 1024 + 128) * 2048;
        assert!(bound < META);
    }
}

#[cfg(test)]
mod original_c355_machine {
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use ProgramNodeKindV1 as N;
use ProgramNumericOperationV1 as O;
use std::{
    cell::Cell,
    mem::{size_of, take},
};
const LIVE: usize = 1_048_576;
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
#[derive(Clone, Copy)]
enum Mode {
    Function(NodeIdV1),
    Builtin(NodeIdV1),
    Index(NodeIdV1),
    Write {
        target: NodeIdV1,
        rhs: NodeIdV1,
        suppressed: bool,
    },
}
enum Task {
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
        full: [bool; 2],
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
        full: [bool; 2],
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
    checkpoint: &'b mut dyn FnMut(ProgramCheckpointV1, &ProgramCancelHandleV1),
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
pub(super) fn execute(
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
            + 64 * 2048;
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
                    location: ProgramLocationV1 {
                        source_id: module.source_id(),
                        span: module.node(origin).unwrap().span,
                    },
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
    let metadata = 64
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
    let live = frames[0]
        .bindings
        .iter()
        .map(|b| b.value.payload_bytes())
        .sum();
    let mut m = Machine {
        owner,
        module,
        options,
        handle,
        checkpoint,
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
    let result = (|| {
        m.admit()?;
        m.push(Task::Exec(origin))?;
        while let Some(task) = m.tasks.pop() {
            m.tick()?;
            m.step(task)?;
        }
        m.poll()?;
        Ok(())
    })();
    let pending = m.poll().err();
    let code = pending.or(result.err());
    (m.checkpoint)(ProgramCheckpointV1::BeforeTerminal, handle);
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
    (m.checkpoint)(ProgramCheckpointV1::AfterTerminal, handle);
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
    fn location(&self, id: NodeIdV1) -> ProgramLocationV1 {
        ProgramLocationV1 {
            source_id: self.module.source_id(),
            span: self.module.node(id).unwrap().span,
        }
    }
    fn kind(&self, id: NodeIdV1) -> N {
        self.module.node(id).unwrap().kind
    }
    fn text(&self, id: NodeIdV1) -> &str {
        self.module
            .source_text(self.module.node(id).unwrap().span)
            .unwrap()
    }
    fn frame(&self) -> &Frame {
        self.frames.last().unwrap()
    }
    fn names_equal(&self, a: &str, b: &str) -> bool {
        if a.len() != b.len() {
            return false;
        }
        for (a, b) in a.as_bytes().chunks(128).zip(b.as_bytes().chunks(128)) {
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
    fn lookup(&self, name: &str) -> Option<&ProgramValueV1> {
        self.frame()
            .bindings
            .iter()
            .find(|b| self.names_equal(&b.name, name))
            .map(|b| &b.value)
    }
    fn list(&self, id: NodeIdV1) -> (NodeIdV1, Option<NodeIdV1>) {
        let N::List { item, next } = self.kind(id) else {
            unreachable!()
        };
        (item, next)
    }
    fn count(&mut self, mut list: Option<NodeIdV1>) -> R<usize> {
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
    fn admit(&mut self) -> R<()> {
        for i in 0..self.module.nodes().len() {
            self.poll()?;
            let n = self.module.nodes()[i];
            if matches!(n.kind, N::Name) && n.span.end - n.span.start > 2048 {
                return Err(ProgramErrorCodeV1::Resource);
            }
            if let N::Function {
                inputs, outputs, ..
            } = n.kind
            {
                self.count(inputs)?;
                self.count(outputs)?;
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
    fn truth(&mut self, v: &ProgramValueV1) -> R<bool> {
        self.work(v.len() as u64)?;
        if v.is_empty() {
            return Ok(false);
        }
        let mut yes = true;
        for i in 0..v.len() {
            if i % 128 == 0 {
                self.poll()?
            }
            let word = v.word(i).unwrap();
            // A NaN cannot be converted to a condition. Scan the complete
            // nonempty value even after a zero (qualified array policy).
            if word.is_nan() {
                return Err(ProgramErrorCodeV1::Type);
            }
            yes &= word != 0.0;
        }
        Ok(yes)
    }
    fn poll(&mut self) -> R<()> {
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
        (self.checkpoint)(ProgramCheckpointV1::Task, self.handle);
        self.poll()?;
        self.stats.transitions = self
            .stats
            .transitions
            .checked_add(1)
            .filter(|n| *n <= self.options.transition_fuel)
            .ok_or(ProgramErrorCodeV1::Resource)?;
        Ok(())
    }
    fn work(&mut self, n: u64) -> R<()> {
        self.poll()?;
        self.stats.primitive_steps = self
            .stats
            .primitive_steps
            .checked_add(n)
            .filter(|n| *n <= self.options.primitive_fuel)
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
    fn tracked(&mut self, v: ProgramValueV1) -> R<ProgramValueV1> {
        self.room(v.payload_bytes())?;
        self.live += v.payload_bytes();
        self.stats.peak_live_payload_bytes = self.stats.peak_live_payload_bytes.max(self.live);
        Ok(v)
    }
    fn release(&mut self, v: ProgramValueV1) {
        self.live -= v.payload_bytes();
    }
    fn copy(&mut self, v: &ProgramValueV1) -> R<ProgramValueV1> {
        self.room(v.payload_bytes())?;
        self.work(v.len() as u64)?;
        self.allocation()?;
        let out = v.try_copy().map_err(ProgramErrorCodeV1::Value)?;
        self.tracked(out)
    }
    fn number(&mut self, n: f64) -> R<ProgramValueV1> {
        self.double([1, 1], &[n])
    }
    fn double(&mut self, shape: [u64; 2], words: &[f64]) -> R<ProgramValueV1> {
        self.room(words.len() * 8)?;
        self.work(words.len() as u64)?;
        self.allocation()?;
        let v = ProgramValueV1::try_double(shape, words).map_err(ProgramErrorCodeV1::Value)?;
        self.tracked(v)
    }
    fn boolean(&mut self, b: bool) -> R<ProgramValueV1> {
        self.room(1)?;
        self.allocation()?;
        let v =
            ProgramValueV1::try_logical([1, 1], &[b as u8]).map_err(ProgramErrorCodeV1::Value)?;
        self.tracked(v)
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
    fn scalar(v: &ProgramValueV1) -> R<f64> {
        v.scalar().map_err(ProgramErrorCodeV1::Value)
    }
    fn named_copy(&mut self, name: NodeIdV1) -> R<ProgramValueV1> {
        let index = self
            .frame()
            .bindings
            .iter()
            .position(|b| self.names_equal(&b.name, self.text(name)))
            .ok_or(ProgramErrorCodeV1::UnknownBinding)?;
        let bytes = self.frame().bindings[index].value.payload_bytes();
        let n = self.frame().bindings[index].value.len();
        self.room(bytes)?;
        self.work(n as u64)?;
        self.allocation()?;
        let v = self.frame().bindings[index]
            .value
            .try_copy()
            .map_err(ProgramErrorCodeV1::Value)?;
        self.tracked(v)
    }
    fn function(&self, name: &str) -> Option<NodeIdV1> {
        self.module
            .functions()
            .iter()
            .copied()
            .find(|id| matches!(self.kind(*id),N::Function{name:n,..} if self.names_equal(self.text(n),name)))
    }
    fn numeric(
        &mut self,
        op: O,
        a: ProgramValueV1,
        b: Option<ProgramValueV1>,
    ) -> R<ProgramValueV1> {
        let (shape, class) = program_numeric_output_layout_v1(op, &a, b.as_ref())
            .map_err(ProgramErrorCodeV1::Value)?;
        self.room(
            (shape[0] * shape[1]) as usize
                * if class == ProgramValueClassV1::Double {
                    8
                } else {
                    1
                },
        )?;
        self.work(32768)?;
        self.allocation()?;
        let handle = self.handle;
        let deadline = self.options.deadline;
        let v = evaluate_program_numeric_controlled_v1(op, &a, b.as_ref(), || {
            if deadline.is_some_and(|d| Instant::now() >= d) {
                handle.cancel_for(handle.key, ProgramCancelReasonV1::Deadline);
            }
            matches!(handle.state.load(Ordering::Acquire), STOP | DEADLINE)
        })
        .map_err(ProgramErrorCodeV1::Value)?;
        let v = self.tracked(v)?;
        self.release(a);
        if let Some(b) = b {
            self.release(b)
        }
        Ok(v)
    }
    fn owned_name(&mut self, name: &str) -> R<String> {
        self.work(name.len() as u64)?;
        self.allocation()?;
        self.metadata_room(name.len())?;
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
    fn name_copy(&mut self, name: Name) -> R<String> {
        match name {
            Name::Ans => self.owned_name("ans"),
            Name::Node(id) => {
                let span = self.module.node(id).unwrap().span;
                self.work((span.end - span.start) as u64)?;
                self.allocation()?;
                self.metadata_room(span.end - span.start)?;
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
                    s.push_str(&self.module.source()[offset..end]);
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
    fn publish(
        &mut self,
        names: &[Name],
        mut incoming: [Option<ProgramValueV1>; 64],
        display: bool,
        suppressed: bool,
    ) -> R<()> {
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
                if self.names_equal(self.name_text(*previous), self.name_text(name)) {
                    slot = Some(writes[j].as_ref().unwrap().index);
                    break;
                }
            }
            if slot.is_none() {
                slot = self
                    .frame()
                    .bindings
                    .iter()
                    .position(|b| self.names_equal(&b.name, self.name_text(name)));
            }
            let owned = if slot.is_none() {
                new += 1;
                Some(self.name_copy(name)?)
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
                let snapshot = self.copy(&value)?;
                let label = self.name_copy(name)?;
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
        (self.checkpoint)(ProgramCheckpointV1::BeforePublish, self.handle);
        self.poll()?;
        self.handle
            .state
            .compare_exchange(RUNNING, PUBLISHING, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ProgramErrorCodeV1::Cancelled)?;
        (self.checkpoint)(ProgramCheckpointV1::AfterBarrier, self.handle);
        // All capacities, snapshots, names and counters were prepared before the CAS.
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
        (self.checkpoint)(ProgramCheckpointV1::AfterPublish, self.handle);
        Ok(())
    }
    fn display(
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
            Some(id) => Some(self.name_copy(Name::Node(id))?),
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
            value,
        };
        (self.checkpoint)(ProgramCheckpointV1::BeforePublish, self.handle);
        self.poll()?;
        self.handle
            .state
            .compare_exchange(RUNNING, PUBLISHING, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ProgramErrorCodeV1::Cancelled)?;
        (self.checkpoint)(ProgramCheckpointV1::AfterBarrier, self.handle);
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
        (self.checkpoint)(ProgramCheckpointV1::AfterPublish, self.handle);
        Ok(())
    }
    fn one(&mut self, name: Name, value: ProgramValueV1, display: bool, suppressed: bool) -> R<()> {
        let mut values = std::array::from_fn(|_| None);
        values[0] = Some(value);
        self.publish(&[name], values, display, suppressed)
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
    fn return_frame(&mut self) -> R<()> {
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
                usize::from(self.lookup(self.text(name)).is_some())
            } else {
                0
            }
        } else {
            caller_requested
        };
        self.poll()?;
        let mut next = self.frame().outputs;
        let mut results: [Option<ProgramValueV1>; 64] = std::array::from_fn(|_| None);
        for slot in results.iter_mut().take(delivered) {
            let (id, tail) = self.list(next.ok_or(ProgramErrorCodeV1::Arity)?);
            next = tail;
            let source = self.text(id);
            let binding = self
                .frame()
                .bindings
                .iter()
                .position(|b| self.names_equal(&b.name, source))
                .ok_or(ProgramErrorCodeV1::UnassignedOutput)?;
            let bytes = self.frame().bindings[binding].value.payload_bytes();
            self.room(bytes)?;
            self.work(self.frame().bindings[binding].value.len() as u64)?;
            self.allocation()?;
            let v = self.frame().bindings[binding]
                .value
                .try_copy()
                .map_err(ProgramErrorCodeV1::Value)?;
            *slot = Some(self.tracked(v)?);
        }
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
    fn step(&mut self, task: Task) -> R<()> {
        match task {
            Task::Block(list) => {
                if let Some(id) = list {
                    let (item, next) = self.list(id);
                    self.push(Task::Block(next))?;
                    self.push(Task::Exec(item))?;
                }
            }
            Task::Exec(id) => {
                self.current = id;
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
                            if self.lookup(self.text(callee)).is_none() {
                                if self.function(self.text(callee)).is_some() {
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
                                    self.lookup(self.text(callee))
                                        .ok_or(ProgramErrorCodeV1::UnknownBinding)?;
                                    let count = self.count(arguments)?;
                                    if !(1..=2).contains(&count) {
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
                                        full: [false; 2],
                                    })?;
                                } else {
                                    self.push(Task::Leaf { node: id, base })?;
                                    self.push(Task::Eval(value, 1, None))?;
                                }
                            }
                            AssignmentTargetsV1::Multiple(list) => {
                                let count = self.count(Some(list))?;
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
                        self.return_frame()?;
                    }
                    _ => return Err(ProgramErrorCodeV1::UnsupportedFeature),
                }
            }
            Task::Eval(id, requested, extent) => {
                self.current = id;
                if requested != 1 && !matches!(self.kind(id), N::Apply { .. } | N::Name) {
                    return Err(ProgramErrorCodeV1::Arity);
                }
                match self.kind(id) {
                    N::Number { bits } => {
                        let v = self.number(f64::from_bits(bits))?;
                        self.value(v)?;
                    }
                    N::Name => {
                        let name = self.text(id);
                        if self.lookup(name).is_some() {
                            let v = self.named_copy(id)?;
                            self.value(v)?;
                        } else if let Some(function) = self.function(name) {
                            self.enter_call(function, self.values.len(), 0, requested, id)?;
                        } else if matches!(name, "nargin" | "nargout") && self.frames.len() > 1 {
                            let n = if name == "nargin" {
                                self.frame().actual
                            } else {
                                self.frame().requested
                            };
                            let v = self.number(n as f64)?;
                            self.value(v)?;
                        } else {
                            return Err(ProgramErrorCodeV1::UnknownBinding);
                        }
                    }
                    N::End => {
                        let n = extent.ok_or(ProgramErrorCodeV1::Index)?;
                        let v = self.number(n as f64)?;
                        self.value(v)?;
                    }
                    N::Group { value } => self.push(Task::Eval(value, requested, extent))?,
                    N::Unary { operator, input } => {
                        self.push(Task::Unary(id, O::Unary(operator)))?;
                        self.push(Task::Eval(input, 1, extent))?;
                    }
                    N::Transpose { input, .. } => {
                        self.push(Task::Unary(id, O::Transpose))?;
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
                        let count = self.count(arguments)?;
                        let name = self.text(callee);
                        let mode = if self.lookup(name).is_some() {
                            if !(1..=2).contains(&count) || requested != 1 {
                                return Err(ProgramErrorCodeV1::Arity);
                            }
                            Mode::Index(callee)
                        } else if let Some(f) = self.function(name) {
                            self.check_call(f, count, requested)?;
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
                            full: [false; 2],
                        })?;
                    }
                    _ => return Err(ProgramErrorCodeV1::UnsupportedFeature),
                }
            }
            Task::Unary(id, op) => {
                self.current = id;
                let a = self.pop()?;
                let v = self.numeric(op, a, None)?;
                self.value(v)?;
            }
            Task::Binary(id, op) => {
                self.current = id;
                let b = self.pop()?;
                let a = self.pop()?;
                let v = self.numeric(O::Binary(op), a, Some(b))?;
                self.value(v)?;
            }
            Task::Short(id, op, right, extent) => {
                self.current = id;
                let a = self.pop()?;
                let truth = Self::scalar(&a)? != 0.0;
                self.release(a);
                if (op == BinaryOperatorV1::ShortCircuitAnd && !truth)
                    || (op == BinaryOperatorV1::ShortCircuitOr && truth)
                {
                    let v = self.boolean(truth)?;
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
                let v = self.boolean(truth)?;
                self.value(v)?;
            }
            Task::Range(id, has_step) => {
                self.current = id;
                let stop = self.pop()?;
                let step = if has_step { Some(self.pop()?) } else { None };
                let start = self.pop()?;
                let a = Self::scalar(&start)?;
                let b = if let Some(step) = &step {
                    Self::scalar(step)?
                } else {
                    1.0
                };
                let z = Self::scalar(&stop)?;
                if ![a, b, z].iter().all(|v| {
                    v.is_finite() && v.fract() == 0.0 && v.abs() <= 9_007_199_254_740_991.0
                }) || b == 0.0
                {
                    return Err(ProgramErrorCodeV1::UnsupportedFeature);
                }
                if start.class() != ProgramValueClassV1::Double
                    || stop.class() != ProgramValueClassV1::Double
                    || step
                        .as_ref()
                        .is_some_and(|v| v.class() != ProgramValueClassV1::Double)
                {
                    return Err(ProgramErrorCodeV1::Type);
                }
                let ai = a as i128;
                let bi = b as i128;
                let zi = z as i128;
                let count = if (bi > 0 && zi < ai) || (bi < 0 && zi > ai) {
                    0
                } else {
                    ((zi - ai) / bi + 1) as usize
                };
                if count > 16 {
                    return Err(ProgramErrorCodeV1::Resource);
                }
                let mut words = [0.0; 16];
                for (i, w) in words[..count].iter_mut().enumerate() {
                    *w = (ai + bi * i as i128) as f64;
                }
                let v = self.double([1, count as u64], &words[..count])?;
                self.release(start);
                self.release(stop);
                if let Some(step) = step {
                    self.release(step)
                }
                self.value(v)?;
            }
            Task::Matrix(id, base) => {
                self.current = id;
                self.matrix(id, base)?;
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
                                .lookup(self.text(target))
                                .ok_or(ProgramErrorCodeV1::UnknownBinding)?;
                            Some(if count == 1 {
                                value.len() as u64
                            } else {
                                value.shape()[index]
                            })
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
                            if matches!(self.kind(arg), N::FullAxis) && index < 2 {
                                full[index] = true;
                            }
                            full
                        },
                    })?;
                    if matches!(self.kind(arg), N::FullAxis) {
                        let n = extent.ok_or(ProgramErrorCodeV1::Index)? as usize;
                        if n > 16 {
                            return Err(ProgramErrorCodeV1::Resource);
                        }
                        let mut words = [0.0; 16];
                        for (i, w) in words[..n].iter_mut().enumerate() {
                            *w = (i + 1) as f64;
                        }
                        let v = self.double([1, n as u64], &words[..n])?;
                        self.value(v)?;
                    } else {
                        self.push(Task::Eval(arg, 1, extent))?;
                    }
                } else {
                    match mode {
                        Mode::Function(f) => self.enter_call(f, base, count, requested, site)?,
                        Mode::Builtin(name) => {
                            let v = self.builtin(name, base, count)?;
                            self.value(v)?;
                        }
                        Mode::Index(target) => {
                            let v = self.gather(target, base, count, full)?;
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
                    }
                }
            }
            Task::Leaf { node, base } => {
                self.current = node;
                self.finish_leaf(node, base)?;
                self.complete()?;
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
                let replacement = self.replace_index(target, base, count, full, &rhs)?;
                self.release(rhs);
                self.discard_values(base);
                self.one(Name::Node(target), replacement, true, suppressed)?;
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
                let yes = self.truth(&v)?;
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
                let yes = self.truth(&v)?;
                self.release(v);
                if yes {
                    self.push(Task::While(condition, body))?;
                    self.push(Task::Exec(body))?;
                }
            }
            Task::ForReady(variable, body) => {
                let frozen = self.pop()?;
                if frozen.shape()[1] == 0 {
                    // Empty column iteration still initializes the loop
                    // variable, through the same atomic publication barrier.
                    let empty = self.double([0, 0], &[])?;
                    self.one(Name::Node(variable), empty, false, true)?;
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
            Task::ForNext {
                variable,
                body,
                frozen,
                column,
            } => {
                if column < frozen.shape()[1] as usize {
                    let rows = frozen.shape()[0] as usize;
                    let mut words = [0.0; 16];
                    let mut bools = [0u8; 16];
                    for i in 0..rows {
                        words[i] = frozen.word(column * rows + i).unwrap();
                        bools[i] = words[i] as u8;
                    }
                    let value = if frozen.class() == ProgramValueClassV1::Double {
                        self.double([rows as u64, 1], &words[..rows])?
                    } else {
                        self.room(rows)?;
                        self.allocation()?;
                        let value = ProgramValueV1::try_logical([rows as u64, 1], &bools[..rows])
                            .map_err(ProgramErrorCodeV1::Value)?;
                        self.tracked(value)?
                    };
                    self.one(Name::Node(variable), value, false, true)?;
                    self.push(Task::ForNext {
                        variable,
                        body,
                        frozen,
                        column: column + 1,
                    })?;
                    self.push(Task::Exec(body))?;
                } else {
                    self.release(frozen);
                }
            }
            Task::Return => self.return_frame()?,
        }
        Ok(())
    }
    fn check_call(&mut self, id: NodeIdV1, actual: usize, requested: usize) -> R<()> {
        let N::Function {
            inputs, outputs, ..
        } = self.kind(id)
        else {
            unreachable!()
        };
        if actual > self.count(inputs)? || requested > self.count(outputs)? {
            return Err(ProgramErrorCodeV1::Arity);
        }
        Ok(())
    }
    fn enter_call(
        &mut self,
        id: NodeIdV1,
        base: usize,
        actual: usize,
        requested: usize,
        site: NodeIdV1,
    ) -> R<()> {
        self.work(1)?;
        self.check_call(id, actual, requested)?;
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
        self.metadata_room(64 * size_of::<Binding>())?;
        let mut bindings = reserve::<Binding>(64)?;
        self.meta(bindings.capacity() * size_of::<Binding>())?;
        let mut args: [Option<ProgramValueV1>; 64] = std::array::from_fn(|_| None);
        for i in (0..actual).rev() {
            args[i] = Some(self.pop()?);
        }
        if self.values.len() != base {
            return Err(ProgramErrorCodeV1::Arity);
        }
        let mut input = inputs;
        for arg in args.iter_mut().take(actual) {
            let (name, next) = self.list(input.ok_or(ProgramErrorCodeV1::Arity)?);
            input = next;
            let name = self.name_copy(Name::Node(name))?;
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
    fn finish_leaf(&mut self, id: NodeIdV1, base: usize) -> R<()> {
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
                self.publish(&names[..count], values, true, suppressed)
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
                    && self.lookup(self.text(expression)).is_some()
                {
                    self.display(value, Some(expression), suppressed)
                } else {
                    self.one(Name::Ans, value, true, suppressed)
                }
            }
            _ => Err(ProgramErrorCodeV1::UnsupportedFeature),
        }
    }
    fn matrix(&mut self, id: NodeIdV1, base: usize) -> R<()> {
        let N::Matrix { rows } = self.kind(id) else {
            unreachable!()
        };
        if rows.is_none() {
            let v = self.double([0, 0], &[])?;
            return self.value(v);
        }
        let mut nextrow = rows;
        let mut index = base;
        let mut outrows = 0usize;
        let mut outcols = None;
        let class = self
            .values
            .get(base)
            .ok_or(ProgramErrorCodeV1::Shape)?
            .class();
        let mut words = [0.0; 256];
        let mut bools = [0u8; 256];
        // First derive the full rectangular layout without allocating output payload.
        while let Some(row) = nextrow {
            self.poll()?;
            let (item, next) = self.list(row);
            nextrow = next;
            let N::Row { items } = self.kind(item) else {
                unreachable!()
            };
            let mut next = items;
            let mut rows = None;
            let mut cols = 0;
            while let Some(list) = next {
                let (_, tail) = self.list(list);
                next = tail;
                let v = self.values.get(index).ok_or(ProgramErrorCodeV1::Shape)?;
                if v.class() != class {
                    return Err(ProgramErrorCodeV1::Type);
                }
                let shape = v.shape();
                if rows.is_some_and(|r| r != shape[0] as usize) {
                    return Err(ProgramErrorCodeV1::Shape);
                }
                rows = Some(shape[0] as usize);
                cols += shape[1] as usize;
                index += 1;
            }
            if outcols.is_some_and(|c| c != cols) {
                return Err(ProgramErrorCodeV1::Shape);
            }
            outcols = Some(cols);
            outrows += rows.unwrap_or(0);
        }
        let outcols = outcols.unwrap_or(0);
        if outrows > 16 || outcols > 16 {
            return Err(ProgramErrorCodeV1::Resource);
        }
        let mut nextrow = rows;
        let mut index = base;
        let mut rowoffset = 0;
        while let Some(row) = nextrow {
            let (item, next) = self.list(row);
            nextrow = next;
            let N::Row { items } = self.kind(item) else {
                unreachable!()
            };
            let mut next = items;
            let mut coloffset = 0;
            let mut height = 0;
            while let Some(list) = next {
                let (_, tail) = self.list(list);
                next = tail;
                let shape = self.values[index].shape();
                height = shape[0] as usize;
                self.work((shape[0] * shape[1]) as u64)?;
                for col in 0..shape[1] as usize {
                    for row in 0..height {
                        self.work(1)?;
                        let offset = rowoffset + row + (coloffset + col) * outrows;
                        let v = self.values[index].word(row + col * height).unwrap();
                        words[offset] = v;
                        bools[offset] = v as u8;
                    }
                }
                coloffset += shape[1] as usize;
                index += 1;
            }
            rowoffset += height;
        }
        let n = outrows * outcols;
        let value = if class == ProgramValueClassV1::Double {
            self.double([outrows as u64, outcols as u64], &words[..n])?
        } else {
            self.room(n)?;
            self.allocation()?;
            let v = ProgramValueV1::try_logical([outrows as u64, outcols as u64], &bools[..n])
                .map_err(ProgramErrorCodeV1::Value)?;
            self.tracked(v)?
        };
        self.discard_values(base);
        self.value(value)
    }
    fn builtin(&mut self, id: NodeIdV1, base: usize, count: usize) -> R<ProgramValueV1> {
        let mut args: [Option<ProgramValueV1>; 64] = std::array::from_fn(|_| None);
        for i in (0..count).rev() {
            args[i] = Some(self.pop()?);
        }
        if self.values.len() != base {
            return Err(ProgramErrorCodeV1::Arity);
        }
        let name = self.text(id);
        let output = match name {
            "sum" | "mean" | "prod" => {
                if count != 1 {
                    return Err(ProgramErrorCodeV1::Arity);
                }
                let op = match name {
                    "sum" => O::Sum,
                    "mean" => O::Mean,
                    _ => O::Prod,
                };
                return self.numeric(op, args[0].take().unwrap(), None);
            }
            "nargin" | "nargout" => {
                if count != 0 || self.frames.len() == 1 {
                    return Err(ProgramErrorCodeV1::Arity);
                }
                self.number(if name == "nargin" {
                    self.frame().actual
                } else {
                    self.frame().requested
                } as f64)?
            }
            "numel" => {
                if count != 1 {
                    return Err(ProgramErrorCodeV1::Arity);
                }
                self.number(args[0].as_ref().unwrap().len() as f64)?
            }
            "size" => {
                if count != 1 {
                    return Err(ProgramErrorCodeV1::Arity);
                }
                let shape = args[0].as_ref().unwrap().shape();
                self.double([1, 2], &[shape[0] as f64, shape[1] as f64])?
            }
            "zeros" | "ones" | "eye" | "reshape" => {
                let reshape = name == "reshape";
                if (reshape && count != 3) || (!reshape && !(1..=2).contains(&count)) {
                    return Err(ProgramErrorCodeV1::Arity);
                }
                let offset = reshape as usize;
                let dimension = |v: &ProgramValueV1| -> R<u64> {
                    if v.class() != ProgramValueClassV1::Double {
                        return Err(ProgramErrorCodeV1::Type);
                    }
                    let x = Self::scalar(v)?;
                    if !x.is_finite() || x.fract() != 0.0 || !(0.0..=16.0).contains(&x) {
                        return Err(ProgramErrorCodeV1::Shape);
                    }
                    Ok(x as u64)
                };
                let rows = dimension(args[offset].as_ref().unwrap())?;
                let cols = if !reshape && count == 1 {
                    rows
                } else {
                    dimension(args[offset + 1].as_ref().unwrap())?
                };
                let n = (rows * cols) as usize;
                let mut words = [0.0; 256];
                let mut bools = [0u8; 256];
                if reshape {
                    let source = args[0].as_ref().unwrap();
                    if source.len() != n {
                        return Err(ProgramErrorCodeV1::Shape);
                    }
                    for i in 0..n {
                        self.work(1)?;
                        words[i] = source.word(i).unwrap();
                        bools[i] = words[i] as u8;
                    }
                    if source.class() == ProgramValueClassV1::Logical {
                        self.room(n)?;
                        self.allocation()?;
                        let v = ProgramValueV1::try_logical([rows, cols], &bools[..n])
                            .map_err(ProgramErrorCodeV1::Value)?;
                        self.tracked(v)?
                    } else {
                        self.double([rows, cols], &words[..n])?
                    }
                } else {
                    if name == "ones" {
                        words[..n].fill(1.0)
                    } else if name == "eye" {
                        for i in 0..rows.min(cols) as usize {
                            words[i + i * rows as usize] = 1.0;
                        }
                    }
                    self.double([rows, cols], &words[..n])?
                }
            }
            _ => return Err(ProgramErrorCodeV1::UnsupportedFeature),
        };
        for arg in args.into_iter().take(count).flatten() {
            self.release(arg);
        }
        Ok(output)
    }
    fn selection(
        &mut self,
        target: NodeIdV1,
        base: usize,
        count: usize,
        full: [bool; 2],
    ) -> R<Selection> {
        let value = self
            .lookup(self.text(target))
            .ok_or(ProgramErrorCodeV1::UnknownBinding)?;
        let shape = value.shape();
        let len = value.len();
        let mut selection = Selection {
            indices: [[0; 256]; 2],
            counts: [0; 2],
            shape: [0; 2],
        };
        for axis in 0..count {
            self.work(256)?;
            let extent = if count == 1 {
                len
            } else {
                shape[axis] as usize
            };
            let arg = self
                .values
                .get(base + axis)
                .ok_or(ProgramErrorCodeV1::Index)?;
            let ash = arg.shape();
            if arg.class() != ProgramValueClassV1::Double
                || !(ash[0] == 1 || ash[1] == 1 || ash == [0, 0])
                || arg.len() > 16
            {
                return Err(ProgramErrorCodeV1::Index);
            }
            selection.counts[axis] = arg.len();
            for i in 0..arg.len() {
                let word = arg.word(i).unwrap();
                if !word.is_finite() || word.fract() != 0.0 || word < 1.0 || word > extent as f64 {
                    return Err(ProgramErrorCodeV1::Index);
                }
                selection.indices[axis][i] = word as usize - 1;
            }
        }
        selection.shape = if count == 2 {
            [selection.counts[0] as u64, selection.counts[1] as u64]
        } else if full[0] {
            [selection.counts[0] as u64, 1]
        } else if (self.values[base].shape()[0] == 1 || self.values[base].shape()[1] == 1)
            && shape[0] == 1
            && shape[1] != 1
        {
            [1, selection.counts[0] as u64]
        } else if (self.values[base].shape()[0] == 1 || self.values[base].shape()[1] == 1)
            && shape[1] == 1
            && shape[0] != 1
        {
            [selection.counts[0] as u64, 1]
        } else {
            self.values[base].shape()
        };
        Ok(selection)
    }
    fn gather(
        &mut self,
        target: NodeIdV1,
        base: usize,
        count: usize,
        full: [bool; 2],
    ) -> R<ProgramValueV1> {
        let target_slot = self
            .frame()
            .bindings
            .iter()
            .position(|b| self.names_equal(&b.name, self.text(target)))
            .ok_or(ProgramErrorCodeV1::UnknownBinding)?;

        let s = self.selection(target, base, count, full)?;
        let rows = (&self.frame().bindings[target_slot].value).shape()[0] as usize;
        let n = (s.shape[0] * s.shape[1]) as usize;
        let mut words = [0.0; 256];
        let mut bools = [0u8; 256];
        let class = (&self.frame().bindings[target_slot].value).class();
        self.work(n as u64)?;
        for i in 0..n {
            self.work(1)?;
            let offset = if count == 1 {
                s.indices[0][i]
            } else {
                s.indices[0][i % s.counts[0]] + s.indices[1][i / s.counts[0]] * rows
            };
            words[i] = (&self.frame().bindings[target_slot].value)
                .word(offset)
                .ok_or(ProgramErrorCodeV1::Index)?;
            bools[i] = words[i] as u8;
        }
        if class == ProgramValueClassV1::Double {
            self.double(s.shape, &words[..n])
        } else {
            self.room(n)?;
            self.allocation()?;
            let v = ProgramValueV1::try_logical(s.shape, &bools[..n])
                .map_err(ProgramErrorCodeV1::Value)?;
            self.tracked(v)
        }
    }
    fn replace_index(
        &mut self,
        target: NodeIdV1,
        base: usize,
        count: usize,
        full: [bool; 2],
        rhs: &ProgramValueV1,
    ) -> R<ProgramValueV1> {
        let target_slot = self
            .frame()
            .bindings
            .iter()
            .position(|b| self.names_equal(&b.name, self.text(target)))
            .ok_or(ProgramErrorCodeV1::UnknownBinding)?;

        let selection = self.selection(target, base, count, full)?;
        let n = (selection.shape[0] * selection.shape[1]) as usize;
        if rhs.shape() == [0, 0] {
            return Err(ProgramErrorCodeV1::UnsupportedFeature);
        }
        let shape = (&self.frame().bindings[target_slot].value).shape();
        let len = (&self.frame().bindings[target_slot].value).len();
        let class = (&self.frame().bindings[target_slot].value).class();
        if class != rhs.class() {
            return Err(ProgramErrorCodeV1::Type);
        }
        if rhs.shape() != [1, 1] {
            if count == 1 {
                if rhs.len() != n {
                    return Err(ProgramErrorCodeV1::Shape);
                }
            } else {
                let mut expected = [1u64; 2];
                let mut actual = [1u64; 2];
                for (i, &n) in selection.counts.iter().filter(|&&n| n != 1).enumerate() {
                    expected[i] = n as u64;
                }
                for (i, n) in rhs.shape().into_iter().filter(|&n| n != 1).enumerate() {
                    actual[i] = n;
                }
                if expected != actual {
                    return Err(ProgramErrorCodeV1::Shape);
                }
            }
        }
        let mut words = [0.0; 256];
        let mut bools = [0u8; 256];
        self.work((len + n) as u64)?;
        for (i, w) in words[..len].iter_mut().enumerate() {
            self.work(1)?;
            *w = (&self.frame().bindings[target_slot].value).word(i).unwrap();
        }
        for i in 0..n {
            self.work(1)?;
            let offset = if count == 1 {
                selection.indices[0][i]
            } else {
                selection.indices[0][i % selection.counts[0]]
                    + selection.indices[1][i / selection.counts[0]] * shape[0] as usize
            };
            words[offset] = rhs.word(if rhs.len() == 1 { 0 } else { i }).unwrap();
        }
        if class == ProgramValueClassV1::Double {
            self.double(shape, &words[..len])
        } else {
            for i in 0..len {
                bools[i] = words[i] as u8;
            }
            self.room(len)?;
            self.allocation()?;
            let v = ProgramValueV1::try_logical(shape, &bools[..len])
                .map_err(ProgramErrorCodeV1::Value)?;
            self.tracked(v)
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
struct Selection {
    indices: [[usize; 256]; 2],
    counts: [usize; 2],
    shape: [u64; 2],
}
#[cfg(test)]
mod boundaries {
    use super::*;
    #[test]
    fn task_value_and_metadata_structural_bounds() {
        let mut owner = ProgramSessionV1::new().unwrap();
        let module = Arc::new(parse_program_v1(1, Arc::from("")).unwrap());
        let handle = ProgramCancelHandleV1 {
            key: ProgramRequestKeyV1 {
                session_id: 1,
                epoch: 1,
                request_id: 1,
            },
            state: Arc::new(AtomicU8::new(RUNNING)),
        };
        let mut checkpoint = |_, _: &ProgramCancelHandleV1| {};
        let mut m = Machine {
            owner: &mut owner,
            module: module.clone(),
            options: Default::default(),
            handle: &handle,
            checkpoint: &mut checkpoint,
            frames: reserve(33).unwrap(),
            tasks: reserve(4096).unwrap(),
            values: reserve(4096).unwrap(),
            events: reserve(1024).unwrap(),
            stats: Default::default(),
            live: 0,
            metadata: 64,
            sequence: 0,
            completed: 0,
            current: module.script(),
            allocations: 4,
            name_steps: Cell::new(0),
            name_error: Cell::new(None),
        };
        for _ in 0..4096 {
            m.push(Task::Block(None)).unwrap();
            m.value(ProgramValueV1::try_double([0, 0], &[]).unwrap())
                .unwrap();
        }
        assert!(matches!(
            m.push(Task::Block(None)),
            Err(ProgramErrorCodeV1::Resource)
        ));
        assert!(matches!(
            m.value(ProgramValueV1::try_double([0, 0], &[]).unwrap()),
            Err(ProgramErrorCodeV1::Resource)
        ));
        m.metadata = META - 1;
        assert!(m.metadata_room(1).is_ok());
        assert!(matches!(
            m.metadata_room(2),
            Err(ProgramErrorCodeV1::Resource)
        ));
        m.live = LIVE - 8;
        assert!(m.room(8).is_ok());
        assert!(matches!(m.room(9), Err(ProgramErrorCodeV1::Resource)));
        // All visible capacities + maximal resident names + one staged publication.
        let bound = 64
            + 33 * size_of::<Frame>()
            + 4096 * size_of::<Task>()
            + 4096 * size_of::<ProgramValueV1>()
            + 1024 * size_of::<ProgramEventV1>()
            + 34 * 64 * size_of::<Binding>()
            + (33 * 64 + 1024 + 128) * 2048;
        assert!(bound < META);
    }
}

    pub(super) fn layout_witness()->[usize;20]{
        use std::mem::{size_of,align_of,offset_of};
        [size_of::<Frame>(),align_of::<Frame>(),size_of::<Task>(),align_of::<Task>(),size_of::<Binding>(),align_of::<Binding>(),size_of::<ProgramValueV1>(),align_of::<ProgramValueV1>(),size_of::<ProgramEventV1>(),align_of::<ProgramEventV1>(),size_of::<Selection>(),align_of::<Selection>(),offset_of!(Frame,bindings),offset_of!(Frame,function),offset_of!(Frame,outputs),offset_of!(Frame,requested),offset_of!(Frame,actual),offset_of!(Frame,task_base),offset_of!(Frame,value_base),offset_of!(Frame,site)]
    }
    pub(super) fn initial_reservation_witness()->usize{64+33*size_of::<Frame>()+4096*size_of::<Task>()+4096*size_of::<ProgramValueV1>()+1024*size_of::<ProgramEventV1>()+64*size_of::<Binding>()+64*2048}
}
#[cfg(test)]
mod specialization_compatibility {
    use super::*;
    fn trace(original:bool,source:&str,fault:Option<usize>,stop:Option<(ProgramCheckpointV1,usize)>,fuel:u64)->String{
        let module=Arc::new(parse_program_v1(771,Arc::from(source)).unwrap());let mut session=ProgramSessionV1::new().unwrap();session.fail_allocation=fault;
        let options=ProgramExecutionOptionsV1{primitive_fuel:fuel,..Default::default()};
        let mut request=match session.prepare(module,options){Ok(request)=>request,Err(e)=>return format!("prepare:{e:?}")};request.invoked=true;
        let mut at_count=0;let mut checkpoint=|at,handle:&ProgramCancelHandleV1|{if let Some((point,n))=stop{if at==point{at_count+=1;if at_count==n{handle.request_stop();}}}};
        let outcome=if original{original_c355_machine::execute(request.owner,Arc::clone(&request.module),request.options,&request.handle,&mut checkpoint)}else{execute(request.owner,Arc::clone(&request.module),request.options,&request.handle,&mut checkpoint)};
        drop(request);
        let words=|v:&ProgramValueV1|format!("{:?}:{:?}:{:?}",v.class(),v.shape(),(0..v.len()).map(|i|v.word(i).unwrap().to_bits()).collect::<Vec<_>>());
        let bindings=session.bindings().map(|b|format!("{}={}",b.name,words(b.value))).collect::<Vec<_>>();
        let events=outcome.events.iter().map(|e|format!("{}:{:?}:{:?}:{}",e.sequence,e.location,e.name,words(&e.value))).collect::<Vec<_>>();
        format!("{:?}|{:?}|{}|{}|{}|{:?}|{:?}|{}|{}|{}|{}",outcome.status,outcome.error,outcome.final_context.revision,outcome.committed_effect_sequence,outcome.completed_leaf_statement_count,bindings,events,outcome.stats.transitions,outcome.stats.primitive_steps,outcome.stats.peak_live_payload_bytes,outcome.stats.peak_metadata_bytes)
    }
    const SOURCE:&str="a=1;[x,x]=pair();A=[1 2;3 4];A(1,1)=x;for k=1:2,a=a+k;end\nfunction [u,v]=pair\nu=11;v=22;end\n";
    #[test]
    fn legacy_layout_and_initial_reservation_are_exact_c355(){
        use std::mem::{size_of,align_of,offset_of};
        let current=[size_of::<Frame>(),align_of::<Frame>(),size_of::<Task>(),align_of::<Task>(),size_of::<Binding>(),align_of::<Binding>(),size_of::<ProgramValueV1>(),align_of::<ProgramValueV1>(),size_of::<ProgramEventV1>(),align_of::<ProgramEventV1>(),size_of::<Selection>(),align_of::<Selection>(),offset_of!(Frame,bindings),offset_of!(Frame,function),offset_of!(Frame,outputs),offset_of!(Frame,requested),offset_of!(Frame,actual),offset_of!(Frame,task_base),offset_of!(Frame,value_base),offset_of!(Frame,site)];
        assert_eq!(current,original_c355_machine::layout_witness());
        assert_eq!(64+33*size_of::<Frame>()+4096*size_of::<Task>()+4096*size_of::<ProgramValueV1>()+1024*size_of::<ProgramEventV1>()+64*size_of::<Binding>()+64*2048,original_c355_machine::initial_reservation_witness());
    }
    #[test]
    fn legacy_each_allocation_fault_boundary_matches_frozen_original(){
        let mut saw_success=false;let mut saw_failure=false;
        for fail in 0..128{let current=trace(false,SOURCE,Some(fail),None,100_000_000);let original=trace(true,SOURCE,Some(fail),None,100_000_000);assert_eq!(current,original,"fault{fail}");saw_success|=current.starts_with("Success|");saw_failure|=!current.starts_with("Success|");}
        assert!(saw_success&&saw_failure);
    }
    #[test]
    fn legacy_stop_fuel_and_effect_prefix_match_frozen_original(){
        for n in 1..=64{assert_eq!(trace(false,SOURCE,None,Some((ProgramCheckpointV1::Task,n)),100_000_000),trace(true,SOURCE,None,Some((ProgramCheckpointV1::Task,n)),100_000_000),"task{n}");}
        for point in [ProgramCheckpointV1::BeforePublish,ProgramCheckpointV1::AfterBarrier,ProgramCheckpointV1::AfterPublish,ProgramCheckpointV1::BeforeTerminal,ProgramCheckpointV1::AfterTerminal]{for n in 1..=3{assert_eq!(trace(false,SOURCE,None,Some((point,n)),100_000_000),trace(true,SOURCE,None,Some((point,n)),100_000_000));}}
        for fuel in [0,1,2,8,16,32,64,100,1000,10000,100000]{assert_eq!(trace(false,SOURCE,None,None,fuel),trace(true,SOURCE,None,None,fuel),"fuel{fuel}");}
    }
}

};
}
pub(super) use install_legacy;
