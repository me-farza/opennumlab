// SPDX-License-Identifier: GPL-3.0-or-later
//! Original right-looking partial-pivot elimination, column-major multiple RHS.
//! Only private candidates mutate. The caller owns the publication transaction.
use super::{Complex64V1 as C,ScientificDenseV1 as Dense,ScientificErrorV1 as E,ScientificControlV1};
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum SolveProgressV1{Pending{scalar_steps:u64},Complete{scalar_steps:u64}}
#[derive(Debug,Clone,Copy)]
enum Phase{Pivot{row:usize,best:usize,metric:f64},Swap{column:usize,pivot:usize},Factor{row:usize},UpdateA{row:usize,column:usize},UpdateB{row:usize,column:usize},Back{row:usize,column:usize,next:usize},Done}
#[derive(Debug)]
pub struct ScientificSolveV1{a:Dense,b:Dense,n:usize,rhs:usize,k:usize,phase:Phase,multiplier:C,acc:C,work:u64,initial_metric:f64,maximum_u_metric:f64}
#[derive(Debug,Clone,Copy,Default)]
pub struct ScientificSolveDiagnosticsV1{pub initial_metric:f64,pub maximum_u_metric:f64,pub scalar_steps:u64}
impl ScientificSolveDiagnosticsV1{pub fn growth_factor(self)->Option<f64>{(self.initial_metric!=0.).then(||self.maximum_u_metric/self.initial_metric)}}
impl ScientificSolveV1{
    /// Exact candidate payload admission bound; metadata is size_of<Self>() and
    /// vector capacities, exposed by capacity_bytes after successful reservation.
    pub fn candidate_bytes(a:&Dense,b:&Dense)->Result<usize,E>{
        let ash=a.shape();let bsh=b.shape();let n=ash.axes();let m=bsh.axes();
        if n.len()!=2||m.len()!=2||n[0]!=n[1]||m[0]!=n[0]{return Err(E::Shape)}
        if a.class()!=super::ScientificClassV1::Double||b.class()!=super::ScientificClassV1::Double{return Err(E::Type)}
        let unit=if a.is_complex()||b.is_complex(){16}else{8};
        a.len().checked_add(b.len()).and_then(|n|n.checked_mul(unit)).ok_or(E::Resource)
    }
    pub async fn new_cooperative(a:&Dense,b:&Dense,control:&mut impl ScientificControlV1)->Result<Self,E>{
        Self::candidate_bytes(a,b)?;
        let mut initial_metric=0.0f64;
        for (input,value) in [a,b].into_iter().enumerate(){for i in 0..value.len(){if i%16==0{control.checkpoint(((value.len()-i).min(16)*8) as u64).await?}let z=value.get(i).unwrap();if !z.finite(){return Err(E::NonFiniteInput)}if input==0{initial_metric=initial_metric.max(z.pivot_metric())}}}
        let complex=a.is_complex()||b.is_complex();
        let aa=if complex{a.copy_as_complex_cooperative(control).await?}else{a.try_copy_cooperative(control).await?};
        let bb=if complex{b.copy_as_complex_cooperative(control).await?}else{b.try_copy_cooperative(control).await?};
        let n=a.shape().axes()[0] as usize;let rhs=b.shape().axes()[1] as usize;
        Ok(Self{a:aa,b:bb,n,rhs,k:0,phase:if n==0||rhs==0{Phase::Done}else{Phase::Pivot{row:0,best:0,metric:0.}},multiplier:C::ZERO,acc:C::ZERO,work:0,initial_metric,maximum_u_metric:0.})
    }
    pub fn capacity_bytes(&self)->usize{self.a.capacity_bytes()+self.b.capacity_bytes()}
    pub fn scalar_steps(&self)->u64{self.work}
    pub fn diagnostics(&self)->ScientificSolveDiagnosticsV1{ScientificSolveDiagnosticsV1{initial_metric:self.initial_metric,maximum_u_metric:self.maximum_u_metric,scalar_steps:self.work}}
    pub fn step(&mut self,max_steps:u64,control:&mut impl ScientificControlV1)->Result<SolveProgressV1,E>{
        if !(128..=32768).contains(&max_steps){return Err(E::Resource)}let mut spent=0u64;
        loop{
            if matches!(self.phase,Phase::Done){return Ok(SolveProgressV1::Complete{scalar_steps:spent})}
            let cost=match self.phase{Phase::Factor{..}=>128,Phase::Back{next,row,..} if next==row+1=>128,Phase::UpdateA{..}|Phase::UpdateB{..}|Phase::Back{..}=>16,_=>8};
            if spent+cost>max_steps{return Ok(SolveProgressV1::Pending{scalar_steps:spent})}
            control.charge(cost)?;spent+=cost;self.work=self.work.checked_add(cost).ok_or(E::Resource)?;
            match self.phase{
                Phase::Pivot{row,best,metric}=>{
                    if row<self.n{let m=self.a.get(row+self.k*self.n).unwrap().pivot_metric();let take=m>metric;self.phase=Phase::Pivot{row:row+1,best:if take{row}else{best},metric:if take{m}else{metric}};}
                    else{if metric==0.{return Err(E::Singular)}self.phase=Phase::Swap{column:0,pivot:best};}
                }
                Phase::Swap{column,pivot}=>{
                    if column<self.n+self.rhs{
                        let col=if column<self.n{column}else{column-self.n};let v=if column<self.n{&mut self.a}else{&mut self.b};
                        if pivot!=self.k{let x=v.get(self.k+col*self.n).unwrap();let y=v.get(pivot+col*self.n).unwrap();v.set(self.k+col*self.n,y)?;v.set(pivot+col*self.n,x)?;}
                        if column<self.n&&column>=self.k{self.maximum_u_metric=self.maximum_u_metric.max(self.a.get(self.k+column*self.n).unwrap().pivot_metric());}
                        self.phase=Phase::Swap{column:column+1,pivot};
                    }else{self.phase=Phase::Factor{row:self.k+1};}
                }
                Phase::Factor{row}=>{
                    if row==self.n{
                        self.k+=1;
                        self.phase=if self.k<self.n{Phase::Pivot{row:self.k,best:self.k,metric:0.}}else{self.acc=self.b.get(self.n-1).unwrap();Phase::Back{row:self.n-1,column:0,next:self.n}};
                    }else{
                        self.multiplier=self.a.get(row+self.k*self.n).unwrap().checked_div(self.a.get(self.k+self.k*self.n).unwrap())?;
                        self.a.set(row+self.k*self.n,self.multiplier)?;
                        self.phase=Phase::UpdateA{row,column:self.k+1};
                    }
                }
                Phase::UpdateA{row,column}=>{
                    if column==self.n{self.phase=Phase::UpdateB{row,column:0};}
                    else{let z=self.a.get(row+column*self.n).unwrap().sub(self.multiplier.mul(self.a.get(self.k+column*self.n).unwrap()));if !z.finite(){return Err(E::ArithmeticBreakdown)}self.a.set(row+column*self.n,z)?;self.phase=Phase::UpdateA{row,column:column+1};}
                }
                Phase::UpdateB{row,column}=>{
                    if column==self.rhs{self.phase=Phase::Factor{row:row+1};}
                    else{let z=self.b.get(row+column*self.n).unwrap().sub(self.multiplier.mul(self.b.get(self.k+column*self.n).unwrap()));if !z.finite(){return Err(E::ArithmeticBreakdown)}self.b.set(row+column*self.n,z)?;self.phase=Phase::UpdateB{row,column:column+1};}
                }
                Phase::Back{row,column,next}=>{
                    if next>row+1{
                        let j=next-1;self.acc=self.acc.sub(self.a.get(row+j*self.n).unwrap().mul(self.b.get(j+column*self.n).unwrap()));if !self.acc.finite(){return Err(E::ArithmeticBreakdown)}self.phase=Phase::Back{row,column,next:j};
                    }else{
                        let z=self.acc.checked_div(self.a.get(row+row*self.n).unwrap())?;self.b.set(row+column*self.n,z)?;
                        if row>0{self.acc=self.b.get(row-1+column*self.n).unwrap();self.phase=Phase::Back{row:row-1,column,next:self.n};}
                        else if column+1<self.rhs{self.acc=self.b.get(self.n-1+(column+1)*self.n).unwrap();self.phase=Phase::Back{row:self.n-1,column:column+1,next:self.n};}
                        else{self.phase=Phase::Done;}
                    }
                }
                Phase::Done=>unreachable!(),
            }
        }
    }
    pub fn into_result(self)->Result<Dense,E>{if matches!(self.phase,Phase::Done){Ok(self.b)}else{Err(E::Resource)}}
}

impl ScientificSolveV1{pub fn new(a:&Dense,b:&Dense,c:&mut impl ScientificControlV1)->Result<Self,E>{super::drive(Self::new_cooperative(a,b,c))}}
