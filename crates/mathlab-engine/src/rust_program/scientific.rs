// SPDX-License-Identifier: GPL-3.0-or-later
//! Scientific-v1 API; same located AST, control machine and keyed cancellation.
use super::*;
use mathlab_language::program::{ScientificProgramValueV1,ScientificErrorV1,ScientificShapeV1,Complex64V1,ScientificLeafOperationV1,ScientificNumericJobV1,ScientificJobProgressV1,ScientificControlV1};
use mathlab_language::program::ScientificDenseV1;
const PROGRAM_PROFILE_V1:&str="scientific-v1";
const PROFILE_LIVE_BYTES:usize=64<<20;
const PROFILE_INDEX_AXES:usize=8;
type ProgramAxes=[bool;8];
type ProgramValueV1=ScientificProgramValueV1;
type ProgramErrorCodeV1=ScientificProgramErrorCodeV1;
type ProgramErrorV1=ScientificProgramErrorV1;
type ProgramExecutionOptionsV1=ScientificProgramOptionsV1;
type ProgramEventV1=ScientificProgramEventV1;
type ProgramOutcomeV1=ScientificProgramOutcomeV1;
type ProgramSessionV1=ScientificProgramSessionV1;
#[derive(Debug)]
enum ProgramNumericOperationV1{Unary(UnaryOperatorV1),Binary(BinaryOperatorV1),Transpose(bool),Leaf(ScientificLeafOperationV1),Resume(ScientificNumericJobV1)}


#[derive(Debug,Default,Clone,Copy)]
pub struct ScientificProgramStatsV1{pub transitions:u64,pub primitive_steps:u64,pub peak_live_payload_bytes:usize,pub peak_metadata_bytes:usize,pub solves_completed:u64,pub last_solve:Option<ScientificSolveDiagnosticsV1>,pub last_solve_complete:bool,pub maximum_growth:Option<f64>}
type ProgramExecutionStatsV1=ScientificProgramStatsV1;
struct ScientificAllocationControl<'a,'b>{stats:&'a mut ScientificProgramStatsV1,live:&'a mut usize,allocations:&'a mut usize,options:ScientificProgramOptionsV1,handle:&'a ProgramCancelHandleV1,checkpoint:&'a std::cell::RefCell<&'b mut dyn FnMut(ProgramCheckpointV1,&ProgramCancelHandleV1)>,fault:Option<usize>}
impl ScientificControlV1 for ScientificAllocationControl<'_,'_>{
    fn charge(&mut self,n:u64)->Result<(),ScientificErrorV1>{
        (self.checkpoint.borrow_mut())(ProgramCheckpointV1::Task,self.handle);
        if self.options.deadline.is_some_and(|d|Instant::now()>=d){self.handle.cancel_for(self.handle.key,ProgramCancelReasonV1::Deadline);}
        if matches!(self.handle.state.load(Ordering::Acquire),STOP|DEADLINE|PENDING_STOP|PENDING_DEADLINE){return Err(ScientificErrorV1::Cancelled)}
        self.stats.primitive_steps=self.stats.primitive_steps.checked_add(n).filter(|x|*x<=self.options.primitive_fuel).ok_or(ScientificErrorV1::Resource)?;Ok(())
    }
    fn reserve(&mut self,bytes:usize)->Result<(),ScientificErrorV1>{
        self.charge(0)?;let next=self.live.checked_add(bytes).filter(|x|*x<=PROFILE_LIVE_BYTES).ok_or(ScientificErrorV1::Resource)?;
        *self.allocations=self.allocations.checked_add(1).ok_or(ScientificErrorV1::Resource)?;if self.fault==Some(*self.allocations){return Err(ScientificErrorV1::Allocation)}
        *self.live=next;self.stats.peak_live_payload_bytes=self.stats.peak_live_payload_bytes.max(next);Ok(())
    }
}
macro_rules! scientific_control{($m:expr)=>{ScientificAllocationControl{stats:&mut $m.stats,live:&mut $m.live,allocations:&mut $m.allocations,options:$m.options,handle:$m.handle,checkpoint:&$m.checkpoint,fault:{#[cfg(test)]{ $m.owner.fail_allocation } #[cfg(not(test))]{None}}}};}

type ProgramCheckpointSink<'a>=std::cell::RefCell<&'a mut dyn FnMut(ProgramCheckpointV1,&ProgramCancelHandleV1)>;
fn checkpoint_sink<'a>(callback:&'a mut dyn FnMut(ProgramCheckpointV1,&ProgramCancelHandleV1))->ProgramCheckpointSink<'a>{std::cell::RefCell::new(callback)}
macro_rules! install_scientific {()=>{
// Expanded into the same private machine scope as the Legacy implementation.
struct Selection { counts:[usize;8], shape:ScientificShapeV1, target:ScientificShapeV1, rank:usize, base:usize, full:ProgramAxes, linear:bool }
impl Machine<'_, '_> {
    fn primitive_checkpoint(&mut self){self.notify(ProgramCheckpointV1::Task);}
    fn notify(&self,at:ProgramCheckpointV1){(self.checkpoint.borrow_mut())(at,self.handle);}
    async fn name_yield(&self){yield_scientific_v1().await;self.notify(ProgramCheckpointV1::Task);}
    async fn work(&mut self,mut n:u64)->R<()>{self.poll()?;while n!=0{let part=n.min(128);ScientificControlV1::checkpoint(&mut scientific_control!(self),part).await.map_err(Self::science_error)?;n-=part;}Ok(())}

    fn science_error(e:ScientificErrorV1)->ProgramErrorCodeV1{match e{ScientificErrorV1::Allocation=>ProgramErrorCodeV1::Allocation,ScientificErrorV1::Resource=>ProgramErrorCodeV1::Resource,ScientificErrorV1::Cancelled=>ProgramErrorCodeV1::Cancelled,ScientificErrorV1::Shape|ScientificErrorV1::Rank=>ProgramErrorCodeV1::Shape,ScientificErrorV1::Type=>ProgramErrorCodeV1::Type,ScientificErrorV1::Index=>ProgramErrorCodeV1::Index,ScientificErrorV1::Unsupported=>ProgramErrorCodeV1::UnsupportedFeature,_=>ProgramErrorCodeV1::Value(e)}}
    fn tracked(&mut self,v:ProgramValueV1)->R<ProgramValueV1>{self.poll()?;Ok(v)}
    async fn copy(&mut self,v:&ProgramValueV1)->R<ProgramValueV1>{self.poll()?;self.room(v.payload_bytes())?;let out=v.try_copy_controlled(&mut scientific_control!(self)).await.map_err(Self::science_error)?;self.tracked(out)}
    async fn copy_binding(&mut self,index:usize)->R<ProgramValueV1>{
        self.poll()?;let frame=self.frames.len()-1;let bytes=self.frames[frame].bindings[index].value.payload_bytes();self.room(bytes)?;
        let mut control=scientific_control!(self);
        let v=self.frames[frame].bindings[index].value.try_copy_controlled(&mut control).await.map_err(Self::science_error)?;self.tracked(v)
    }
    async fn double(&mut self,shape:[u64;2],words:&[f64])->R<ProgramValueV1>{
        self.room(words.len().checked_mul(8).ok_or(ProgramErrorCodeV1::Resource)?)?;
        let shape=ScientificShapeV1::new(&shape).map_err(Self::science_error)?;
        let out=ScientificDenseV1::from_real_cooperative(shape,words,&mut scientific_control!(self)).await.map_err(Self::science_error)?;self.tracked(ProgramValueV1::from_dense(out))
    }
    async fn number(&mut self,n:f64)->R<ProgramValueV1>{self.double([1,1],&[n]).await}
    async fn boolean(&mut self,b:bool)->R<ProgramValueV1>{self.room(1)?;let out=ScientificDenseV1::from_logical_cooperative(ScientificShapeV1::new(&[1,1]).unwrap(),&[b as u8],&mut scientific_control!(self)).await.map_err(Self::science_error)?;self.tracked(ProgramValueV1::from_dense(out))}
    fn scalar(v:&ProgramValueV1)->R<f64>{v.scalar().map_err(Self::science_error)}
    async fn truth(&mut self,v:&ProgramValueV1)->R<bool>{
        if v.is_complex(){return Err(ProgramErrorCodeV1::Type)}if v.is_empty(){return Ok(false)}let mut yes=true;
        for i in 0..v.len(){if i%128==0{self.work((v.len()-i).min(128) as u64).await?;}let word=v.word(i).unwrap();if word.is_nan(){return Err(ProgramErrorCodeV1::Type)}yes&=word!=0.;}Ok(yes)
    }
    fn transpose_operation(&self,conjugate:bool)->O{O::Transpose(conjugate)}
    async fn finish_numeric(&mut self,id:NodeIdV1,op:O,a:ProgramValueV1,b:Option<ProgramValueV1>)->R<()>{
        self.poll()?;
        match op{
            O::Resume(mut job)=>{
                yield_scientific_v1().await;
                let before=job.held_capacity_bytes();let attempted=job.step(32768,&mut scientific_control!(self));
                yield_scientific_v1().await;
                if let Some(d)=job.solve_diagnostics(){self.stats.last_solve=Some(d);self.stats.last_solve_complete=false;if let Some(g)=d.growth_factor(){self.stats.maximum_growth=Some(self.stats.maximum_growth.map_or(g,|old|old.max(g)));}}
                let progress=attempted.map_err(Self::science_error)?;
                let after=job.held_capacity_bytes();self.live=self.live.checked_sub(before-after).ok_or(ProgramErrorCodeV1::Resource)?;
                if progress==ScientificJobProgressV1::Complete{if let Some(d)=job.solve_diagnostics(){self.stats.solves_completed=self.stats.solves_completed.checked_add(1).ok_or(ProgramErrorCodeV1::CounterExhausted)?;self.stats.last_solve=Some(d);self.stats.last_solve_complete=true;if let Some(g)=d.growth_factor(){self.stats.maximum_growth=Some(self.stats.maximum_growth.map_or(g,|old|old.max(g)));}}self.release(a);let result=job.into_result().map_err(Self::science_error)?;self.value(result)}
                else{self.value(a)?;self.push(Task::Unary(id,O::Resume(job)))}
            }
            op=>{
                let leaf=match op{O::Unary(op)=>ScientificLeafOperationV1::Unary(op),O::Binary(op)=>ScientificLeafOperationV1::Binary(op),O::Transpose(conjugate)=>ScientificLeafOperationV1::Transpose(conjugate),O::Leaf(op)=>op,O::Resume(_)=>unreachable!()};
                self.work(32).await?;let extra=ScientificNumericJobV1::additional_bytes(leaf,&a,b.as_ref()).map_err(Self::science_error)?;self.room(extra)?;
                
                let inputs=a.payload_bytes()+b.as_ref().map_or(0,ProgramValueV1::payload_bytes);
                let job=ScientificNumericJobV1::new_cooperative(leaf,a,b,&mut scientific_control!(self)).await.map_err(Self::science_error)?;
                debug_assert!(job.held_capacity_bytes()>=inputs);
                let placeholder=self.double([0,0],&[]).await?;self.value(placeholder)?;self.push(Task::Unary(id,O::Resume(job)))
            }
        }
    }
    async fn full_axis_value(&mut self,_:u64)->R<ProgramValueV1>{self.double([0,0],&[]).await}
    fn index_extent(&self,value:&ProgramValueV1,count:usize,index:usize)->R<u64>{if count==1{Ok(value.len() as u64)}else if count==value.rank(){Ok(value.shape()[index])}else{Err(ProgramErrorCodeV1::Arity)}}
    fn loop_columns(&self,value:&ProgramValueV1)->R<u64>{if value.rank()!=2{Err(ProgramErrorCodeV1::UnsupportedFeature)}else{Ok(value.shape()[1])}}
    async fn range(&mut self,id:NodeIdV1,has_step:bool)->R<()>{
        self.current=id;let stop=self.pop()?;let step=if has_step{Some(self.pop()?)}else{None};let start=self.pop()?;
        if start.class()!=ProgramValueClassV1::Double||stop.class()!=ProgramValueClassV1::Double||step.as_ref().is_some_and(|s|s.class()!=ProgramValueClassV1::Double){return Err(ProgramErrorCodeV1::Type)}
        let a=Self::scalar(&start)?;let b=step.as_ref().map(Self::scalar).transpose()?.unwrap_or(1.);let z=Self::scalar(&stop)?;
        if ![a,b,z].iter().all(|x|x.is_finite()&&x.fract()==0.&&x.abs()<=9_007_199_254_740_991.)||b==0.{return Err(ProgramErrorCodeV1::UnsupportedFeature)}
        let (a,b,z)=(a as i128,b as i128,z as i128);let count=if b>0&&z<a||b<0&&z>a{0}else{usize::try_from((z-a)/b+1).map_err(|_|ProgramErrorCodeV1::Resource)?};
        let shape=ScientificShapeV1::new(&[1,count as u64]).map_err(Self::science_error)?;self.room(count.checked_mul(8).ok_or(ProgramErrorCodeV1::Resource)?)?;
        let mut out=ScientificDenseV1::zeros_cooperative(shape,false,&mut scientific_control!(self)).await.map_err(Self::science_error)?;
        for i in 0..count{self.work(1).await?;out.set(i,Complex64V1::real((a+b*i as i128) as f64)).map_err(Self::science_error)?;}
        self.release(start);self.release(stop);if let Some(step)=step{self.release(step)}let v=self.tracked(ProgramValueV1::from_dense(out))?;self.value(v)
    }
    async fn for_next(&mut self,variable:NodeIdV1,body:NodeIdV1,frozen:ProgramValueV1,column:usize)->R<()>{
        if column<self.loop_columns(&frozen)? as usize{
            let rows=frozen.shape()[0] as usize;let shape=ScientificShapeV1::new(&[rows as u64,1]).map_err(Self::science_error)?;
            let bytes=rows.checked_mul(if frozen.is_complex(){16}else if frozen.class()==ProgramValueClassV1::Logical{1}else{8}).ok_or(ProgramErrorCodeV1::Resource)?;self.room(bytes)?;
            let mut candidate=if frozen.class()==ProgramValueClassV1::Logical{ScientificDenseV1::zeros_logical_cooperative(shape,&mut scientific_control!(self)).await}else{ScientificDenseV1::zeros_cooperative(shape,frozen.is_complex(),&mut scientific_control!(self)).await}.map_err(Self::science_error)?;
            for row in 0..rows{self.work(2).await?;candidate.set(row,frozen.component(row+column*rows).unwrap()).map_err(Self::science_error)?;}
            let v=self.tracked(ProgramValueV1::from_dense(candidate))?;self.one(Name::Node(variable),v,false,true).await?;self.push(Task::ForNext{variable,body,frozen,column:column+1})?;self.push(Task::Exec(body))
        }else{self.release(frozen);Ok(())}
    }
    async fn dimensions(&mut self,args:&[Option<ProgramValueV1>;64],offset:usize,count:usize)->R<ScientificShapeV1>{
        if count<=offset{return Err(ProgramErrorCodeV1::Arity)}
        let mut axes=[1u64;8];let n=if count-offset==1&&args[offset].as_ref().unwrap().len()!=1{let v=args[offset].as_ref().unwrap();if v.rank()!=2||v.shape()[0]!=1||v.len()>8{return Err(ProgramErrorCodeV1::Shape)}v.len()}else{count-offset};
        if !(2..=8).contains(&n){if n==1{let v=args[offset].as_ref().unwrap();if v.class()!=ProgramValueClassV1::Double||v.is_complex(){return Err(ProgramErrorCodeV1::Type)}let x=Self::scalar(v)?;if !x.is_finite()||x.fract()!=0.||x<0.{return Err(ProgramErrorCodeV1::Shape)}return ScientificShapeV1::new(&[x as u64,x as u64]).map_err(Self::science_error)}return Err(ProgramErrorCodeV1::Shape)}
        for i in 0..n{self.work(1).await?;let v=if count-offset==1{args[offset].as_ref().unwrap()}else{args[offset+i].as_ref().unwrap()};if v.class()!=ProgramValueClassV1::Double||v.is_complex(){return Err(ProgramErrorCodeV1::Type)}let x=if count-offset==1{v.word(i).unwrap()}else{Self::scalar(v)?};if !x.is_finite()||x.fract()!=0.||x<0.{return Err(ProgramErrorCodeV1::Shape)}axes[i]=x as u64;}
        ScientificShapeV1::new(&axes[..n]).map_err(Self::science_error)
    }
    async fn finish_builtin(&mut self,site:NodeIdV1,id:NodeIdV1,base:usize,count:usize)->R<()>{
        let mut args:[Option<ProgramValueV1>;64]=std::array::from_fn(|_|None);for i in (0..count).rev(){args[i]=Some(self.pop()?)}if self.values.len()!=base{return Err(ProgramErrorCodeV1::Arity)}
        let source_module=Arc::clone(&self.module);let name=source_module.source_text(source_module.node(id).unwrap().span).unwrap();
        let operation=match name{"real"=>Some((ScientificLeafOperationV1::Real,1)),"imag"=>Some((ScientificLeafOperationV1::Imag,1)),"conj"=>Some((ScientificLeafOperationV1::Conjugate,1)),"complex"=>Some((ScientificLeafOperationV1::Complex,2)),"norm_inf"=>Some((ScientificLeafOperationV1::NormInf,1)),_=>None};
        if let Some((op,n))=operation{if count!=n{return Err(ProgramErrorCodeV1::Arity)}let a=args[0].take().unwrap();let b=if n==2{args[1].take()}else{None};return self.finish_numeric(site,O::Leaf(op),a,b).await}
        let output=match name{
            "nargin"|"nargout"=>{if count!=0||self.frames.len()==1{return Err(ProgramErrorCodeV1::Arity)}self.number(if name=="nargin"{self.frame().actual}else{self.frame().requested} as f64).await?},
            "numel"=>{if count!=1{return Err(ProgramErrorCodeV1::Arity)}self.number(args[0].as_ref().unwrap().len() as f64).await?},
            "size"=>{if count!=1{return Err(ProgramErrorCodeV1::Arity)}let mut words=[0.;8];let shape=args[0].as_ref().unwrap().shape();for(i,&n)in shape.iter().enumerate(){words[i]=n as f64;}self.double([1,shape.len() as u64],&words[..shape.len()]).await?},
            "zeros"|"ones"|"eye"|"reshape"=>{
                let reshape=name=="reshape";let shape=self.dimensions(&args,reshape as usize,count).await?;
                if name=="eye"&&shape.rank()!=2{return Err(ProgramErrorCodeV1::Shape)}
                let logical=reshape&&args[0].as_ref().unwrap().class()==ProgramValueClassV1::Logical;let complex=reshape&&args[0].as_ref().unwrap().is_complex();
                let bytes=shape.len().checked_mul(if logical{1}else if complex{16}else{8}).ok_or(ProgramErrorCodeV1::Resource)?;self.room(bytes)?;
                let mut out=if logical{ScientificDenseV1::zeros_logical_cooperative(shape,&mut scientific_control!(self)).await}else{ScientificDenseV1::zeros_cooperative(shape,complex,&mut scientific_control!(self)).await}.map_err(Self::science_error)?;
                if reshape{let value=args[0].as_ref().unwrap();if value.len()!=shape.len(){return Err(ProgramErrorCodeV1::Shape)}for i in 0..out.len(){self.work(2).await?;out.set(i,value.component(i).unwrap()).map_err(Self::science_error)?;}}
                else if name=="ones"{for i in 0..out.len(){self.work(1).await?;out.set(i,Complex64V1::ONE).map_err(Self::science_error)?;}}
                else if name=="eye"{for i in 0..shape.axis(0).min(shape.axis(1)) as usize{self.work(1).await?;out.set(i+i*shape.axis(0) as usize,Complex64V1::ONE).map_err(Self::science_error)?;}}
                self.tracked(ProgramValueV1::from_dense(out))?
            },
            _=>return Err(ProgramErrorCodeV1::UnsupportedFeature),
        };
        for value in args.into_iter().take(count).flatten(){self.release(value)}self.value(output)
    }
    async fn selection(&mut self,target:NodeIdV1,base:usize,count:usize,full:ProgramAxes)->R<Selection>{
        let target_shape=self.lookup(self.text(target)).await.ok_or(ProgramErrorCodeV1::UnknownBinding)?.as_dense().shape();
        if count!=1&&count!=target_shape.rank(){return Err(ProgramErrorCodeV1::Arity)}let mut counts=[1usize;8];
        for axis in 0..count{
            let extent=if count==1{target_shape.len()}else{target_shape.axis(axis) as usize};
            if full[axis]{counts[axis]=extent;continue}
            let arg=self.values.get(base+axis).ok_or(ProgramErrorCodeV1::Index)?;
            if arg.class()!=ProgramValueClassV1::Double||arg.is_complex()||arg.rank()!=2||!(arg.shape()[0]==1||arg.shape()[1]==1||arg.shape()==[0,0]){return Err(ProgramErrorCodeV1::Index)}
            counts[axis]=arg.len();
            for i in 0..counts[axis]{let x=self.values[base+axis].word(i).unwrap();self.work(1).await?;if !x.is_finite()||x.fract()!=0.||x<1.||x>extent as f64{return Err(ProgramErrorCodeV1::Index)}}
        }
        let shape=if count==1{
            if full[0]{ScientificShapeV1::new(&[counts[0] as u64,1])}
            else{let arg=self.values[base].shape();if target_shape.rank()==2&&target_shape.axis(0)==1&&target_shape.axis(1)!=1{ScientificShapeV1::new(&[1,counts[0] as u64])}else if target_shape.rank()==2&&target_shape.axis(1)==1&&target_shape.axis(0)!=1{ScientificShapeV1::new(&[counts[0] as u64,1])}else{ScientificShapeV1::new(arg)}}
        }else{let mut axes=[1u64;8];for i in 0..count{axes[i]=counts[i] as u64;}ScientificShapeV1::new(&axes[..count])}.map_err(Self::science_error)?;
        Ok(Selection{counts,shape,target:target_shape,rank:count,base,full,linear:count==1})
    }
    async fn selected_offset(&mut self,s:&Selection,mut linear:usize)->R<usize>{
        let mut offset=0usize;
        for axis in 0..s.rank{let coordinate=linear%s.counts[axis];linear/=s.counts[axis];let index=if s.full[axis]{coordinate}else{self.values[s.base+axis].word(coordinate).unwrap() as usize-1};self.work(1).await?;
            offset=offset.checked_add(index.checked_mul(if s.linear{1}else{s.target.stride(axis).unwrap()}).ok_or(ProgramErrorCodeV1::Resource)?).ok_or(ProgramErrorCodeV1::Resource)?;}
        Ok(offset)
    }
    async fn gather(&mut self,target:NodeIdV1,base:usize,count:usize,full:ProgramAxes)->R<ProgramValueV1>{
        let s=self.selection(target,base,count,full).await?;let index=self.binding_index(self.text(target)).await.ok_or(ProgramErrorCodeV1::UnknownBinding)?;
        let value=&self.frame().bindings[index].value;let complex=value.is_complex();let logical=value.class()==ProgramValueClassV1::Logical;let bytes=s.shape.len().checked_mul(if logical{1}else if complex{16}else{8}).ok_or(ProgramErrorCodeV1::Resource)?;
        self.room(bytes)?;let mut out=if logical{ScientificDenseV1::zeros_logical_cooperative(s.shape,&mut scientific_control!(self)).await}else{ScientificDenseV1::zeros_cooperative(s.shape,complex,&mut scientific_control!(self)).await}.map_err(Self::science_error)?;
        for i in 0..s.shape.len(){let j=self.selected_offset(&s,i).await?;let z=self.frame().bindings[index].value.component(j).unwrap();self.work(2).await?;out.set(i,z).map_err(Self::science_error)?;}
        self.tracked(ProgramValueV1::from_dense(out))
    }
    async fn replace_index(&mut self,target:NodeIdV1,base:usize,count:usize,full:ProgramAxes,rhs:&ProgramValueV1)->R<ProgramValueV1>{
        let s=self.selection(target,base,count,full).await?;if rhs.is_empty(){return Err(ProgramErrorCodeV1::UnsupportedFeature)}
        if rhs.len()!=1{for axis in 0..8{if rhs.shape().get(axis).copied().unwrap_or(1)!=s.shape.axis(axis){return Err(ProgramErrorCodeV1::Shape)}}}
        let frame=self.frames.len()-1;let index=self.binding_index(self.text(target)).await.ok_or(ProgramErrorCodeV1::UnknownBinding)?;
        let original=&self.frames[frame].bindings[index].value;if original.class()!=rhs.class(){return Err(ProgramErrorCodeV1::Type)}
        let promote=!s.shape.is_empty()&&rhs.is_complex()&&!original.is_complex();let bytes=if promote{original.len().checked_mul(16).ok_or(ProgramErrorCodeV1::Resource)?}else{original.payload_bytes()};self.room(bytes)?;
        let mut control=scientific_control!(self);let source=self.frames[frame].bindings[index].value.as_dense();
        let mut out=if promote{source.copy_as_complex_cooperative(&mut control).await}else{source.try_copy_cooperative(&mut control).await}.map_err(Self::science_error)?;
        for i in 0..s.shape.len(){let index=self.selected_offset(&s,i).await?;let z=rhs.component(if rhs.len()==1{0}else{i}).unwrap();self.work(2).await?;out.set(index,z).map_err(Self::science_error)?;}
        self.tracked(ProgramValueV1::from_dense(out))
    }
    async fn matrix(&mut self,id:NodeIdV1,base:usize)->R<()>{
        let N::Matrix{rows}=self.kind(id)else{unreachable!()};let mut rows=rows;let mut info=[(0usize,0usize,0usize,0usize);256];let mut row_count=0;let mut value_index=base;let mut total_rows=0usize;let mut columns=None;let mut logical=true;let mut class=None;let mut complex=false;
        while let Some(list)=rows{
            self.poll()?;let(row,next)=self.list(list);rows=next;let N::Row{items}=self.kind(row)else{unreachable!()};let start=value_index;let mut count=0;let mut height=None;let mut width=0usize;let mut items=items;
            while let Some(list)=items{self.work(1).await?;let(_,next)=self.list(list);items=next;let value=self.values.get(value_index).ok_or(ProgramErrorCodeV1::Shape)?;if value.rank()!=2{return Err(ProgramErrorCodeV1::Shape)}let axes=value.shape();
                if axes!=[0,0]{if height.is_some_and(|n|n!=axes[0] as usize){return Err(ProgramErrorCodeV1::Shape)}height=Some(axes[0] as usize);width=width.checked_add(axes[1] as usize).ok_or(ProgramErrorCodeV1::Resource)?;if class.is_some_and(|c|c!=value.class()){return Err(ProgramErrorCodeV1::Type)}class=Some(value.class());logical=value.class()==ProgramValueClassV1::Logical;complex|=value.is_complex();}
                value_index+=1;count+=1;
            }
            let height=height.unwrap_or(0);if height!=0||width!=0{if columns.is_some_and(|n|n!=width){return Err(ProgramErrorCodeV1::Shape)}columns=Some(width);}
            if row_count==256{return Err(ProgramErrorCodeV1::Resource)}info[row_count]=(start,count,height,width);row_count+=1;total_rows=total_rows.checked_add(height).ok_or(ProgramErrorCodeV1::Resource)?;
        }
        let columns=columns.unwrap_or(0);let shape=ScientificShapeV1::new(&[total_rows as u64,columns as u64]).map_err(Self::science_error)?;if value_index!=self.values.len(){return Err(ProgramErrorCodeV1::Shape)}
        if class.is_none(){logical=false;}let bytes=shape.len().checked_mul(if logical{1}else if complex{16}else{8}).ok_or(ProgramErrorCodeV1::Resource)?;self.room(bytes)?;
        let mut out=if logical{ScientificDenseV1::zeros_logical_cooperative(shape,&mut scientific_control!(self)).await}else{ScientificDenseV1::zeros_cooperative(shape,complex,&mut scientific_control!(self)).await}.map_err(Self::science_error)?;let mut top=0usize;
        for &(start,count,height,_)in &info[..row_count]{let mut left=0usize;for index in start..start+count{let axes=self.values[index].shape();let width=axes[1] as usize;for col in 0..width{for row in 0..height{let z=self.values[index].component(row+col*height).unwrap();self.work(2).await?;out.set((top+row)+(left+col)*total_rows,z).map_err(Self::science_error)?;}}left+=width;}top+=height;}
        self.discard_values(base);let v=self.tracked(ProgramValueV1::from_dense(out))?;self.value(v)
    }
}

};}
use install_scientific as install_value_profile;
#[path="machine.rs"] mod machine;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScientificProgramErrorCodeV1 {
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
impl ScientificProgramErrorCodeV1{pub fn is_allocation(&self)->bool{matches!(self,Self::Allocation|Self::Value(ScientificErrorV1::Allocation))}}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScientificProgramErrorV1 {
    pub code: ScientificProgramErrorCodeV1,
    pub location: ProgramLocationV1,
    pub call_stack: [Option<ProgramLocationV1>; 32],
    pub call_depth: usize,
}
#[derive(Debug, Clone, Copy)]
pub struct ScientificProgramOptionsV1 {
    pub deadline: Option<Instant>,
    pub transition_fuel: u64,
    pub primitive_fuel: u64,
}
impl Default for ScientificProgramOptionsV1 {
    fn default() -> Self {
        Self {
            deadline: None,
            transition_fuel: 5_000_000,
            primitive_fuel: 1_000_000_000,
        }
    }
}
#[derive(Debug)]
pub struct ScientificProgramEventV1 {
    pub sequence: u64,
    pub location: ProgramLocationV1,
    pub function: Option<NodeIdV1>,
    pub name: Option<String>,
    pub value: ScientificProgramValueV1,
}
#[derive(Debug)]
pub struct ScientificProgramOutcomeV1 {
    pub profile: &'static str,
    pub key: ProgramRequestKeyV1,
    pub start: ProgramContextV1,
    pub final_context: ProgramContextV1,
    pub committed_effect_sequence: u64,
    pub completed_leaf_statement_count: u64,
    pub stats: ProgramExecutionStatsV1,
    pub events: Vec<ScientificProgramEventV1>,
    pub status: ProgramStatusV1,
    pub error: Option<ScientificProgramErrorV1>,
}
struct Binding {
    name: String,
    value: ScientificProgramValueV1,
}
pub struct ScientificProgramSessionV1 {
    session_id: u64,
    epoch: u64,
    request_id: u64,
    revision: u64,
    bindings: Vec<Binding>,
    #[cfg(test)]
    fail_allocation: Option<usize>,
}
/// Borrowed read-only workspace entry; no mutable storage or value copy escapes.
pub struct ScientificProgramBindingViewV1<'a> {
    pub name: &'a str,
    pub value: &'a ScientificProgramValueV1,
}
impl ScientificProgramSessionV1 {
    pub fn new() -> Result<Self, ScientificProgramErrorCodeV1> {
        let session_id = NEXT_ID
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| ScientificProgramErrorCodeV1::CounterExhausted)?;
        let mut bindings = Vec::new();
        bindings
            .try_reserve_exact(64)
            .map_err(|_| ScientificProgramErrorCodeV1::Allocation)?;
        if bindings.capacity() > 64 {
            return Err(ScientificProgramErrorCodeV1::Resource);
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
    pub fn bindings(&self) -> impl ExactSizeIterator<Item = ScientificProgramBindingViewV1<'_>> {
        self.bindings.iter().map(|b| ScientificProgramBindingViewV1 {
            name: &b.name,
            value: &b.value,
        })
    }
    pub fn binding(&self,name:&str)->Option<&ScientificProgramValueV1>{self.bindings.iter().find(|b|b.name==name).map(|b|&b.value)}
    pub fn reset(&mut self) -> Result<(), ScientificProgramErrorCodeV1> {
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or(ScientificProgramErrorCodeV1::CounterExhausted)?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(ScientificProgramErrorCodeV1::CounterExhausted)?;
        self.bindings.clear();
        self.epoch = epoch;
        self.revision = revision;
        Ok(())
    }
    pub fn prepare(
        &mut self,
        module: Arc<ProgramModuleV1>,
        options: ScientificProgramOptionsV1,
    ) -> Result<PreparedScientificProgramRequestV1<'_>, ScientificProgramErrorCodeV1> {
        if options.transition_fuel > 5_000_000 || options.primitive_fuel > 1_000_000_000 {
            return Err(ScientificProgramErrorCodeV1::Resource);
        }
        let request_id = self
            .request_id
            .checked_add(1)
            .ok_or(ScientificProgramErrorCodeV1::CounterExhausted)?;
        #[cfg(test)]
        if self.fail_allocation == Some(0) {
            return Err(ScientificProgramErrorCodeV1::Allocation);
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
        Ok(PreparedScientificProgramRequestV1 {
            owner: self,
            module,
            options,
            handle,
            invoked: false,
        })
    }
}
pub struct PreparedScientificProgramRequestV1<'a> {
    owner: &'a mut ScientificProgramSessionV1,
    module: Arc<ProgramModuleV1>,
    options: ScientificProgramOptionsV1,
    handle: ProgramCancelHandleV1,
    invoked: bool,
}
impl PreparedScientificProgramRequestV1<'_> {
    pub fn cancel_handle(&self) -> ProgramCancelHandleV1 {
        self.handle.clone()
    }
    /// Allocation-free cooperative execution. Dropping a suspended future
    /// abandons its key and restores completed caller publications.
    pub async fn execute_cooperatively(mut self,mut checkpoint:impl FnMut(ProgramCheckpointV1,&ProgramCancelHandleV1))->ScientificProgramOutcomeV1{
        self.invoked=true;
        machine::execute_async(self.owner,Arc::clone(&self.module),self.options,&self.handle,&mut checkpoint).await
    }
    pub fn execute(self) -> ScientificProgramOutcomeV1 {
        self.execute_controlled(|_, _| {})
    }
    pub fn execute_controlled(
        mut self,
        mut checkpoint: impl FnMut(ProgramCheckpointV1, &ProgramCancelHandleV1),
    ) -> ScientificProgramOutcomeV1 {
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
impl Drop for PreparedScientificProgramRequestV1<'_> {
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
mod resource_tests{
    use super::*;
    fn module(s:&str)->Arc<ProgramModuleV1>{Arc::new(parse_program_v1(902,Arc::from(s)).unwrap())}
    #[test]
    fn every_scientific_allocation_site_preserves_exact_prefix_and_recovers(){
        let src="a=7\nA=eye(2);B=complex([1;2],[3;4]);X=A\\B;after=1;";let mut success=false;let mut failures=0;
        for fail in 1..256{let mut s=ScientificProgramSessionV1::new().unwrap();s.fail_allocation=Some(fail);let out=s.prepare(module(src),Default::default()).unwrap().execute();
            if out.status==ProgramStatusV1::Success{success=true;break}failures+=1;assert_eq!(out.status,ProgramStatusV1::Resource);assert_eq!(out.error.unwrap().code,ScientificProgramErrorCodeV1::Allocation);let revision=out.final_context.revision;assert!(revision<=5);
            for(i,name)in ["a","A","B","X","after"].into_iter().enumerate(){assert_eq!(s.binding(name).is_some(),revision>i as u64)}
            assert_eq!(out.events.len(),(revision>=1) as usize);if revision>=1{assert_eq!(out.events[0].value.scalar().unwrap(),7.);}
            if let Some(value)=s.binding("X"){assert_eq!(value.component(0).unwrap(),Complex64V1{re:1.,im:3.});assert_eq!(value.component(1).unwrap(),Complex64V1{re:2.,im:4.});}
            s.fail_allocation=None;assert_eq!(s.prepare(module("recovered=1;"),Default::default()).unwrap().execute().status,ProgramStatusV1::Success);
        }assert!(success&&failures>4);
    }
    #[test]
    fn scientific_page_assignment_stop_barrier_publishes_whole_candidate(){
        let src="Y=complex(zeros([2 2 2]),zeros([2 2 2]));saved=Y;Y(:,:,2)=complex(ones(2),ones(2));after=1;";
        for(point,published)in [(ProgramCheckpointV1::BeforePublish,false),(ProgramCheckpointV1::AfterBarrier,true),(ProgramCheckpointV1::AfterPublish,true)]{let mut s=ScientificProgramSessionV1::new().unwrap();let mut count=0;let out=s.prepare(module(src),Default::default()).unwrap().execute_controlled(|at,h|{if at==point{count+=1;if count==3{h.request_stop();}}});assert_eq!(out.status,ProgramStatusV1::Cancelled(ProgramCancelReasonV1::UserStop));assert_eq!(out.final_context.revision,if published{3}else{2});assert!(s.binding("after").is_none());let y=s.binding("Y").unwrap();for i in 0..8{let expected=if published&&i>=4{Complex64V1::ONE.add(Complex64V1{re:0.,im:1.})}else{Complex64V1::ZERO};assert_eq!(y.component(i).unwrap(),expected);}assert!(out.events.is_empty());}
    }
}

#[cfg(test)]
mod cooperative_preparation_tests{
    use super::*;
    use std::future::Future;
    fn module(s:&str)->Arc<ProgramModuleV1>{Arc::new(parse_program_v1(903,Arc::from(s)).unwrap())}
    const SOURCE:&str="before=7;A=complex(eye(4),zeros(4));B=complex(ones(4,2),ones(4,2));X=A\\B;Y=complex(zeros([4 2 2]),zeros([4 2 2]));Y(:,:,2)=X;last=1;";
    fn prefix(session:&ScientificProgramSessionV1){
        assert_eq!(session.binding("seed").unwrap().scalar().unwrap(),17.);
        if let Some(x)=session.binding("X"){assert_eq!(x.shape(),[4,2]);for i in 0..8{assert_eq!(x.component(i).unwrap(),Complex64V1{re:1.,im:1.});}}
        if let Some(y)=session.binding("Y"){let page=y.component(8).unwrap();assert!(page==Complex64V1::ZERO||page==Complex64V1{re:1.,im:1.});for i in 8..16{assert_eq!(y.component(i).unwrap(),page)}}
    }
    #[test]
    fn every_preparation_suspension_drop_restores_prefix_and_releases_budget(){
        let mut baseline=ScientificProgramSessionV1::new().unwrap();baseline.prepare(module("seed=17;"),Default::default()).unwrap().execute();let request=baseline.prepare(module(SOURCE),Default::default()).unwrap();let future=request.execute_cooperatively(|_,_|{});let bytes=std::mem::size_of_val(&future);assert!(bytes<=1024*1024,"suspendedfuture{bytes}");
        let mut future=std::pin::pin!(future);let mut context=std::task::Context::from_waker(std::task::Waker::noop());let mut polls=0;
        loop{polls+=1;if let std::task::Poll::Ready(out)=future.as_mut().poll(&mut context){assert_eq!(out.status,ProgramStatusV1::Success,"{:?}",out.error);break}}
        assert!(polls>32);
        for suspend in 0..polls-1{
            let mut s=ScientificProgramSessionV1::new().unwrap();s.prepare(module("seed=17;"),Default::default()).unwrap().execute();let handle;
            {let request=s.prepare(module(SOURCE),Default::default()).unwrap();handle=request.cancel_handle();let future=request.execute_cooperatively(|_,_|{});let mut future=std::pin::pin!(future);for _ in 0..suspend{assert!(future.as_mut().poll(&mut context).is_pending());}}
            assert_eq!(handle.request_stop(),ProgramCancelResultV1::TooLate);prefix(&s);
            let live=s.bindings().map(|b|b.value.payload_bytes()).sum::<usize>();let out=s.prepare(module("recovered=1;"),Default::default()).unwrap().execute();assert_eq!(out.status,ProgramStatusV1::Success);assert_eq!(out.stats.peak_live_payload_bytes,live+8);
        }
    }
    #[test]
    fn every_preparation_suspension_stop_keeps_first_reason_and_prefix(){
        let mut baseline=ScientificProgramSessionV1::new().unwrap();baseline.prepare(module("seed=17;"),Default::default()).unwrap().execute();let mut total=0;
        {let request=baseline.prepare(module(SOURCE),Default::default()).unwrap();let future=request.execute_cooperatively(|_,_|{});let mut future=std::pin::pin!(future);let mut cx=std::task::Context::from_waker(std::task::Waker::noop());loop{total+=1;if future.as_mut().poll(&mut cx).is_ready(){break}}}
        for suspend in 0..total-1{
            let mut s=ScientificProgramSessionV1::new().unwrap();s.prepare(module("seed=17;"),Default::default()).unwrap().execute();let request=s.prepare(module(SOURCE),Default::default()).unwrap();let handle=request.cancel_handle();
            let out={let future=request.execute_cooperatively(|_,_|{});let mut future=std::pin::pin!(future);let mut context=std::task::Context::from_waker(std::task::Waker::noop());
                for _ in 0..suspend{assert!(future.as_mut().poll(&mut context).is_pending());}
                assert!(matches!(handle.request_stop(),ProgramCancelResultV1::Accepted(ProgramCancelReasonV1::UserStop)));
                loop{if let std::task::Poll::Ready(out)=future.as_mut().poll(&mut context){break out}}
            };
            assert_eq!(out.status,ProgramStatusV1::Cancelled(ProgramCancelReasonV1::UserStop));assert_eq!(handle.cancel_for(handle.key(),ProgramCancelReasonV1::Deadline),ProgramCancelResultV1::AlreadyAccepted(ProgramCancelReasonV1::UserStop));prefix(&s);
        }
    }
}

#[cfg(test)]
mod empty_future_drop_test{
    use super::*;use std::future::Future;
    #[test]
    fn dropping_initial_preparation_restores_even_empty_reserved_binding_pool(){
        let mut s=ScientificProgramSessionV1::new().unwrap();
        {let module=Arc::new(parse_program_v1(904,Arc::from("X=ones(256,256);")).unwrap());let request=s.prepare(module,Default::default()).unwrap();let future=request.execute_cooperatively(|_,_|{});let mut future=std::pin::pin!(future);let mut cx=std::task::Context::from_waker(std::task::Waker::noop());assert!(future.as_mut().poll(&mut cx).is_pending());}
        assert_eq!(s.bindings.capacity(),64);assert!(s.bindings.is_empty());let out=s.prepare(Arc::new(parse_program_v1(904,Arc::from("recovered=1;")).unwrap()),Default::default()).unwrap().execute();assert_eq!(out.status,ProgramStatusV1::Success);assert_eq!(s.bindings.capacity(),64);
    }
}
