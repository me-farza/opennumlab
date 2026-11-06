// SPDX-License-Identifier: GPL-3.0-or-later
//! Scientific arithmetic plans. No reuse of the legacy fixed-fiber accumulator.
use super::{BinaryOperatorV1 as B,UnaryOperatorV1 as U,ScientificProgramValueV1 as V,ScientificErrorV1 as E,ScientificShapeV1 as Shape,Complex64V1 as C,ProgramValueClassV1,ScientificControlV1};
use mathlab_array::scientific::ScientificSolveDiagnosticsV1;
use mathlab_array::scientific::{ScientificDenseV1 as Dense,ScientificSolveV1,SolveProgressV1};
#[derive(Debug,Clone,Copy)]
pub enum ScientificLeafOperationV1{Unary(U),Binary(B),Transpose(bool),Real,Imag,Conjugate,Complex,NormInf}
#[derive(Debug)]
enum JobState{
    Element{op:ScientificLeafOperationV1,a:V,b:Option<V>,out:Dense,next:usize},
    Multiply{a:V,b:V,out:Dense,row:usize,column:usize,inner:usize,sum:C},
    Solve{a:V,b:V,solver:ScientificSolveV1},
    Norm{a:V,out:Dense,row:usize,column:usize,sum:f64,best:f64},
    Complete(Option<V>,Option<ScientificSolveDiagnosticsV1>),
}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum ScientificJobProgressV1{Pending,Complete}
fn axes(a:&V,b:Option<&V>)->Result<Shape,E>{
    let rank=a.rank().max(b.map_or(2,V::rank));let mut dims=[1u64;8];
    for (i,n) in dims[..rank].iter_mut().enumerate(){let x=a.shape().get(i).copied().unwrap_or(1);let y=b.and_then(|b|b.shape().get(i)).copied().unwrap_or(1);*n=if x==y{x}else if x==1{y}else if y==1{x}else{return Err(E::Shape)}}
    Shape::new(&dims[..rank])
}
fn broadcast(v:&V,out:Shape,mut index:usize)->usize{
    let own=v.as_dense().shape();let mut offset=0;
    for axis in 0..out.rank(){let n=out.axis(axis) as usize;let coordinate=if n==0{0}else{let j=index%n;index/=n;j};if own.axis(axis)!=1{offset+=coordinate*own.stride(axis).unwrap_or(0)}}offset
}
impl JobState{
    /// Result+scratch capacity that must be admitted BEFORE new() allocates.
    pub fn additional_bytes(op:ScientificLeafOperationV1,a:&V,b:Option<&V>)->Result<usize,E>{
        if matches!(op,ScientificLeafOperationV1::Binary(B::LeftDivide))&&a.len()!=1{return ScientificSolveV1::candidate_bytes(a.as_dense(),b.ok_or(E::Shape)?.as_dense())}
        if matches!(op,ScientificLeafOperationV1::Binary(B::Divide))&&b.ok_or(E::Shape)?.len()!=1{return Err(E::Unsupported)}
        let shape=match op{ScientificLeafOperationV1::Binary(B::Multiply) if a.len()!=1&&b.ok_or(E::Shape)?.len()!=1=>{let b=b.unwrap();if a.rank()!=2||b.rank()!=2||a.shape()[1]!=b.shape()[0]{return Err(E::Shape)}Shape::new(&[a.shape()[0],b.shape()[1]])?},ScientificLeafOperationV1::Transpose(_)=>{if a.rank()!=2{return Err(E::Rank)}Shape::new(&[a.shape()[1],a.shape()[0]])?},ScientificLeafOperationV1::NormInf=>Shape::new(&[1,1])?,_=>axes(a,b)?};
        let logical=(a.class()==ProgramValueClassV1::Logical&&matches!(op,ScientificLeafOperationV1::Transpose(_)))||matches!(op,ScientificLeafOperationV1::Unary(U::Not)|ScientificLeafOperationV1::Binary(B::Equal|B::NotEqual|B::Less|B::LessEqual|B::Greater|B::GreaterEqual|B::And|B::Or));
        let complex=match op{ScientificLeafOperationV1::Real|ScientificLeafOperationV1::Imag|ScientificLeafOperationV1::NormInf=>false,ScientificLeafOperationV1::Complex=>true,_=>a.is_complex()||b.is_some_and(V::is_complex)};
        shape.len().checked_mul(if logical{1}else if complex{16}else{8}).ok_or(E::Resource)
    }
    pub async fn new_cooperative(op:ScientificLeafOperationV1,a:V,b:Option<V>,control:&mut impl ScientificControlV1)->Result<Self,E>{
        Self::additional_bytes(op,&a,b.as_ref())?;
        let complex_input=a.is_complex()||b.as_ref().is_some_and(V::is_complex);
        if complex_input&&matches!(op,ScientificLeafOperationV1::Complex|ScientificLeafOperationV1::Unary(U::Not)|ScientificLeafOperationV1::Binary(B::Less|B::LessEqual|B::Greater|B::GreaterEqual|B::And|B::Or)){return Err(E::Type)}
        if b.as_ref().is_some_and(V::is_complex)&&matches!(op,ScientificLeafOperationV1::Binary(B::Power|B::ElementPower)){return Err(E::Type)}
        if a.class()!=ProgramValueClassV1::Double&& !matches!(op,ScientificLeafOperationV1::Unary(U::Not)|ScientificLeafOperationV1::Transpose(_)|ScientificLeafOperationV1::Binary(B::And|B::Or|B::Equal|B::NotEqual)){return Err(E::Type)}
        if b.as_ref().is_some_and(|b|b.class()!=ProgramValueClassV1::Double)&&!matches!(op,ScientificLeafOperationV1::Binary(B::And|B::Or|B::Equal|B::NotEqual)){return Err(E::Type)}
        if matches!(op,ScientificLeafOperationV1::Binary(B::LeftDivide))&&a.len()!=1{let b=b.ok_or(E::Shape)?;let solver=ScientificSolveV1::new_cooperative(a.as_dense(),b.as_dense(),control).await?;return Ok(Self::Solve{a,b,solver})}
        if matches!(op,ScientificLeafOperationV1::NormInf){if a.rank()!=2{return Err(E::Rank)}let out=Dense::zeros_cooperative(Shape::new(&[1,1])?,false,control).await?;return Ok(Self::Norm{a,out,row:0,column:0,sum:0.,best:0.})}
        if matches!(op,ScientificLeafOperationV1::Binary(B::Multiply))&&a.len()!=1&&b.as_ref().ok_or(E::Shape)?.len()!=1{
            let b=b.unwrap();let shape=Shape::new(&[a.shape()[0],b.shape()[1]])?;let out=Dense::zeros_cooperative(shape,a.is_complex()||b.is_complex(),control).await?;return Ok(Self::Multiply{a,b,out,row:0,column:0,inner:0,sum:C::ZERO})
        }
        let shape=if matches!(op,ScientificLeafOperationV1::Transpose(_)){Shape::new(&[a.shape()[1],a.shape()[0]])?}else{axes(&a,b.as_ref())?};
        let complex=match op{ScientificLeafOperationV1::Real|ScientificLeafOperationV1::Imag=>false,ScientificLeafOperationV1::Complex=>true,_=>a.is_complex()||b.as_ref().is_some_and(V::is_complex)};
        let logical=(a.class()==ProgramValueClassV1::Logical&&matches!(op,ScientificLeafOperationV1::Transpose(_)))||matches!(op,ScientificLeafOperationV1::Unary(U::Not)|ScientificLeafOperationV1::Binary(B::Equal|B::NotEqual|B::Less|B::LessEqual|B::Greater|B::GreaterEqual|B::And|B::Or));
        let out=if logical{Dense::zeros_logical_cooperative(shape,control).await?}else{Dense::zeros_cooperative(shape,complex,control).await?};Ok(Self::Element{op,a,b,out,next:0})
    }
    pub fn held_capacity_bytes(&self)->usize{match self{Self::Element{a,b,out,..}=>a.payload_bytes()+b.as_ref().map_or(0,V::payload_bytes)+out.capacity_bytes(),Self::Multiply{a,b,out,..}=>a.payload_bytes()+b.payload_bytes()+out.capacity_bytes(),Self::Solve{a,b,solver}=>a.payload_bytes()+b.payload_bytes()+solver.capacity_bytes(),Self::Norm{a,out,..}=>a.payload_bytes()+out.capacity_bytes(),Self::Complete(v,_)=>v.as_ref().map_or(0,V::payload_bytes)}}
    pub fn step(&mut self,max:u64,control:&mut impl ScientificControlV1)->Result<ScientificJobProgressV1,E>{
        if !(512..=32768).contains(&max){return Err(E::Resource)}let mut spent=0u64;
        loop{
            if matches!(self,Self::Complete(..)){return Ok(ScientificJobProgressV1::Complete)}
            let reserve=if matches!(self,Self::Element{op:ScientificLeafOperationV1::Binary(B::Power|B::ElementPower|B::Divide|B::ElementDivide|B::LeftDivide|B::ElementLeftDivide),..}){512}else{64};
            if spent+reserve>max{return Ok(ScientificJobProgressV1::Pending)}control.charge(64)?;spent+=64;
            match self{
                Self::Element{op,a,b,out,next}=>{
                    if *next==out.len(){let done=std::mem::replace(self,Self::Complete(None,None));if let Self::Element{out,..}=done{*self=Self::Complete(Some(V::from_dense(out)),None)}continue}
                    let i=*next;let shape=out.shape();let ai=if matches!(op,ScientificLeafOperationV1::Transpose(_)){let rows=a.shape()[0] as usize;let cols=a.shape()[1] as usize;(i/cols)+(i%cols)*rows}else{broadcast(a,shape,i)};
                    let av=a.component(ai).ok_or(E::Index)?;let bv=b.as_ref().map(|b|b.component(broadcast(b,shape,i)).ok_or(E::Index)).transpose()?;
                    let z=leaf(*op,av,bv,&mut StepMeter{control:&mut *control,spent:&mut spent})?;out.set(i,z)?;*next+=1;
                }
                Self::Multiply{a,b,out,row,column,inner,sum}=>{
                    let rows=a.shape()[0] as usize;let cols=b.shape()[1] as usize;let terms=a.shape()[1] as usize;
                    if *column==cols||rows==0{let done=std::mem::replace(self,Self::Complete(None,None));if let Self::Multiply{out,..}=done{*self=Self::Complete(Some(V::from_dense(out)),None)}continue}
                    if *inner<terms{let p=a.component(*row+*inner*rows).unwrap().mul(b.component(*inner+*column*terms).unwrap());*sum=sum.add(p);if !sum.finite(){return Err(E::ArithmeticBreakdown)}*inner+=1;}
                    else{out.set(*row+*column*rows,*sum)?;*sum=C::ZERO;*inner=0;*row+=1;if *row==rows{*row=0;*column+=1;}}
                }
                Self::Solve{solver,..}=>{
                    let available=max-spent; if available<128{return Ok(ScientificJobProgressV1::Pending)}
                    let progress=solver.step(available,control)?;
                    if matches!(progress,SolveProgressV1::Complete{..}){let done=std::mem::replace(self,Self::Complete(None,None));if let Self::Solve{solver,..}=done{let diagnostics=solver.diagnostics();*self=Self::Complete(Some(V::from_dense(solver.into_result()?)),Some(diagnostics))}continue}
                    return Ok(ScientificJobProgressV1::Pending)
                }
                Self::Norm{a,out,row,column,sum,best}=>{
                    let rows=a.shape()[0] as usize;let cols=a.shape()[1] as usize;
                    if *row==rows{out.set(0,C::real(*best))?;let done=std::mem::replace(self,Self::Complete(None,None));if let Self::Norm{out,..}=done{*self=Self::Complete(Some(V::from_dense(out)),None)}continue}
                    if *column==cols{*best=best.max(*sum);*sum=0.;*column=0;*row+=1;}
                    else{let z=a.component(*row+*column*rows).unwrap();*sum+=z.re.abs()+z.im.abs();if !sum.is_finite(){return Err(E::ArithmeticBreakdown)}*column+=1;}
                }
                Self::Complete(..)=>unreachable!(),
            }
        }
    }
    pub fn solve_diagnostics(&self)->Option<ScientificSolveDiagnosticsV1>{match self{Self::Solve{solver,..}=>Some(solver.diagnostics()),Self::Complete(_,d)=>*d,_=>None}}
    pub fn into_result(self)->Result<V,E>{if let Self::Complete(Some(v),_)=self{Ok(v)}else{Err(E::Resource)}}
}
fn leaf(op:ScientificLeafOperationV1,a:C,b:Option<C>,control:&mut impl ScientificControlV1)->Result<C,E>{
    let real=|x:C|if x.im==0.{Ok(x.re)}else{Err(E::Type)};
    let truth=|x:C|->Result<bool,E>{let x=real(x)?;if x.is_nan(){Err(E::Type)}else{Ok(x!=0.)}};
    let z=match op{
        ScientificLeafOperationV1::Unary(U::Plus)=>a,
        ScientificLeafOperationV1::Unary(U::Minus)=>C{re:-a.re,im:-a.im},
        ScientificLeafOperationV1::Unary(U::Not)=>C::real((!truth(a)?) as u8 as f64),
        ScientificLeafOperationV1::Transpose(conjugate)=>if conjugate{a.conj()}else{a},
        ScientificLeafOperationV1::Conjugate=>a.conj(),ScientificLeafOperationV1::Real=>C::real(a.re),ScientificLeafOperationV1::Imag=>C::real(a.im),
        ScientificLeafOperationV1::Complex=>C{re:real(a)?,im:real(b.ok_or(E::Shape)?)?},
        ScientificLeafOperationV1::Binary(op)=>{let b=b.ok_or(E::Shape)?;match op{
            B::Add=>a.add(b),B::Subtract=>a.sub(b),B::Multiply|B::ElementMultiply=>a.mul(b),B::Divide|B::ElementDivide=>{control.charge(128)?;a.checked_div(b)?},B::LeftDivide|B::ElementLeftDivide=>{control.charge(128)?;b.checked_div(a)?},
            B::Power|B::ElementPower=>{let exponent=real(b)?;if !exponent.is_finite()||exponent.fract()!=0.||exponent.abs()>1024.{return Err(E::Unsupported)}let mut base=if exponent<0.{control.charge(128)?;C::ONE.checked_div(a)?}else{a};let mut n=exponent.abs() as u32;let mut z=C::ONE;while n!=0{if n&1!=0{control.charge(16)?;z=z.mul(base)}n>>=1;if n!=0{control.charge(16)?;base=base.mul(base)}}z},
            B::Equal=>C::real((a==b) as u8 as f64),B::NotEqual=>C::real((a!=b) as u8 as f64),B::Less=>C::real((real(a)?<real(b)?) as u8 as f64),B::LessEqual=>C::real((real(a)?<=real(b)?) as u8 as f64),B::Greater=>C::real((real(a)?>real(b)?) as u8 as f64),B::GreaterEqual=>C::real((real(a)?>=real(b)?) as u8 as f64),B::And=>C::real((truth(a)?&truth(b)?) as u8 as f64),B::Or=>C::real((truth(a)?|truth(b)?) as u8 as f64),_=>return Err(E::Unsupported),
        }},
        ScientificLeafOperationV1::NormInf=>return Err(E::Unsupported),
    };
    if !z.finite(){return Err(E::ArithmeticBreakdown)}Ok(z)
}

struct StepMeter<'a,T:ScientificControlV1>{control:&'a mut T,spent:&'a mut u64}
impl<T:ScientificControlV1> ScientificControlV1 for StepMeter<'_,T>{
    fn charge(&mut self,n:u64)->Result<(),E>{self.control.charge(n)?;*self.spent+=n;Ok(())}
    fn reserve(&mut self,n:usize)->Result<(),E>{self.control.reserve(n)}
}

/// Opaque job: partial candidate storage never escapes through public variants.
#[derive(Debug)]
pub struct ScientificNumericJobV1{state:JobState}
impl ScientificNumericJobV1{
    pub fn additional_bytes(op:ScientificLeafOperationV1,a:&V,b:Option<&V>)->Result<usize,E>{JobState::additional_bytes(op,a,b)}
    pub async fn new_cooperative(op:ScientificLeafOperationV1,a:V,b:Option<V>,control:&mut impl ScientificControlV1)->Result<Self,E>{Ok(Self{state:JobState::new_cooperative(op,a,b,control).await?})}
    pub fn held_capacity_bytes(&self)->usize{self.state.held_capacity_bytes()}
    pub fn step(&mut self,max:u64,control:&mut impl ScientificControlV1)->Result<ScientificJobProgressV1,E>{self.state.step(max,control)}
    pub fn solve_diagnostics(&self)->Option<ScientificSolveDiagnosticsV1>{self.state.solve_diagnostics()}
    pub fn into_result(self)->Result<V,E>{self.state.into_result()}
}
