// SPDX-License-Identifier: GPL-3.0-or-later
//! Storage operations only; AST scheduling/publication remain in machine.rs.
use super::*;
type V=ScientificProgramValueV2;type E=ScientificErrorV1;type ResultV<T>=Result<T,E>;
pub(super) fn input_bytes(values:&[Option<V>;64],count:usize)->ResultV<usize>{let mut bytes=0usize;for value in values[..count].iter().flatten(){bytes=bytes.checked_add(value.payload_bytes()).ok_or(E::Resource)?}Ok(bytes)}
pub(super) async fn number(n:f64,control:&mut impl ScientificControlV1)->ResultV<V>{Ok(V::from_dense(ScientificDenseV1::from_real_cooperative(ScientificShapeV1::new(&[1,1])?,&[n],control).await?))}
pub(super) async fn boolean(n:bool,control:&mut impl ScientificControlV1)->ResultV<V>{Ok(V::from_dense(ScientificDenseV1::from_logical_cooperative(ScientificShapeV1::new(&[1,1])?,&[n as u8],control).await?))}
pub(super) fn valid_arity(name:&str,count:usize)->bool{match name{"nargin"|"nargout"|"nan"|"inf"=>count==0,"numel"|"size"|"issparse"|"nnz"|"nzmax"|"real"|"imag"|"conj"|"norm_inf"=>count==1,"complex"=>count==2,"sparse"=>count==5,"zeros"|"ones"=> (1..=8).contains(&count),"eye"=>(1..=2).contains(&count),"reshape"=>(2..=9).contains(&count),_=>true}}
pub(super) async fn numeric(op:ProgramNumericOperationV1,a:V,b:Option<V>,available:usize,control:&mut impl ScientificControlV1)->ResultV<(V,Option<ScientificSolveDiagnosticsV1>)>{
    if a.is_sparse()||b.as_ref().is_some_and(V::is_sparse){
        if matches!(op,ProgramNumericOperationV1::Binary(BinaryOperatorV1::Multiply))&&a.is_sparse()&&!b.as_ref().ok_or(E::Shape)?.is_sparse(){let output=mathlab_language::program::multiply_sparse_dense_ieee_v2(a.csc_view()?,b.as_ref().unwrap().dense_view()?,available,control).await?;return Ok((output,None))}
        return Err(E::Unsupported)
    }
    let op=match op{ProgramNumericOperationV1::Unary(u)=>ScientificLeafOperationV1::Unary(u),ProgramNumericOperationV1::Binary(b)=>ScientificLeafOperationV1::Binary(b),ProgramNumericOperationV1::Transpose(c)=>ScientificLeafOperationV1::Transpose(c),ProgramNumericOperationV1::Leaf(l)=>l};
    let (result,diagnostics)=evaluate_scientific_dense_v2(op,a.into_dense_v1()?,b.map(V::into_dense_v1).transpose()?,available,control).await?;Ok((V::from_dense(result.into_dense()),diagnostics))
}
fn dimension(value:&V)->ResultV<u64>{if value.is_sparse()||value.class()!=ProgramValueClassV1::Double||value.is_complex(){return Err(E::Type)}let n=value.scalar()?;if !n.is_finite()||n.fract()!=0.||n<0.||n>1_048_576.{return Err(E::Shape)}Ok(n as u64)}
async fn dimensions(args:&[Option<V>;64],offset:usize,count:usize,control:&mut impl ScientificControlV1)->ResultV<ScientificShapeV1>{
    let mut axes=[1u64;8];let arg=args[offset].as_ref().ok_or(E::Shape)?;let dense=arg.dense_view()?;
    if count-offset==1&&dense.len()==1{let n=dimension(arg)?;return ScientificShapeV1::new(&[n,n])}
    let n=if count-offset==1{if dense.shape().rank()!=2||dense.shape().axes()[0]!=1||dense.len()>8{return Err(E::Shape)}dense.len()}else{count-offset};if !(2..=8).contains(&n){return Err(E::Shape)}
    for i in 0..n{control.checkpoint(1).await?;let x=if count-offset==1{if arg.class()!=ProgramValueClassV1::Double||arg.is_complex(){return Err(E::Type)}dense.get(i).unwrap().re}else{dimension(args[offset+i].as_ref().unwrap())? as f64};if !x.is_finite()||x.fract()!=0.||x<0.||x>1_048_576.{return Err(E::Shape)}axes[i]=x as u64;}ScientificShapeV1::new(&axes[..n])
}
fn require_headroom(bytes:usize,available:usize)->ResultV<()>{if bytes>available.min(64<<20){Err(E::Resource)}else{Ok(())}}
fn broadcast(a:&ScientificDenseV1,b:&ScientificDenseV1)->ResultV<ScientificShapeV1>{let rank=a.shape().rank().max(b.shape().rank());let mut axes=[1u64;8];for(i,n)in axes[..rank].iter_mut().enumerate(){let x=a.shape().axis(i);let y=b.shape().axis(i);*n=if x==y||y==1{x}else if x==1{y}else{return Err(E::Shape)}}ScientificShapeV1::new(&axes[..rank])}
fn offset(value:&ScientificDenseV1,shape:ScientificShapeV1,mut index:usize)->usize{let mut at=0;for axis in 0..shape.rank(){let extent=shape.axis(axis) as usize;let coordinate=if extent==0{0}else{let n=index%extent;index/=extent;n};if value.shape().axis(axis)!=1{at+=coordinate*value.shape().stride(axis).unwrap_or(0)}}at}
async fn complex(a:&V,b:&V,available:usize,control:&mut impl ScientificControlV1)->ResultV<V>{
    let a=a.dense_view()?;let b=b.dense_view()?;if a.class()!=mathlab_language::program::ScientificClassV1::Double||b.class()!=mathlab_language::program::ScientificClassV1::Double||a.is_complex()||b.is_complex(){return Err(E::Type)}let shape=broadcast(a,b)?;require_headroom(shape.len().checked_mul(16).ok_or(E::Resource)?,available)?;let mut out=ScientificDenseV1::zeros_cooperative(shape,true,control).await?;
    for first in (0..out.len()).step_by(16){let end=(first+16).min(out.len());control.checkpoint(((end-first)*8) as u64).await?;for i in first..end{out.set(i,Complex64V1{re:a.get(offset(a,shape,i)).unwrap().re,im:b.get(offset(b,shape,i)).unwrap().re})?}}Ok(V::from_dense(out))
}
pub(super) async fn builtin(name:&str,mut args:[Option<V>;64],count:usize,actual:usize,requested:usize,in_function:bool,available:usize,control:&mut impl ScientificControlV1)->ResultV<V>{
    control.checkpoint(count as u64).await?;
    let leaf=match name{"real"=>Some(ScientificLeafOperationV1::Real),"imag"=>Some(ScientificLeafOperationV1::Imag),"conj"=>Some(ScientificLeafOperationV1::Conjugate),"norm_inf"=>Some(ScientificLeafOperationV1::NormInf),_=>None};
    if let Some(leaf)=leaf{return numeric(ProgramNumericOperationV1::Leaf(leaf),args[0].take().unwrap(),None,available,control).await.map(|v|v.0)}
    match name{
        "nargin"|"nargout"=>{if !in_function{return Err(E::Shape)}number(if name=="nargin"{actual}else{requested} as f64,control).await},
        "nan"=>number(f64::NAN,control).await,"inf"=>number(f64::INFINITY,control).await,
        "complex"=>complex(args[0].as_ref().unwrap(),args[1].as_ref().unwrap(),available,control).await,
        "numel"=>number(args[0].as_ref().unwrap().logical_numel() as f64,control).await,
        "size"=>{let axes=args[0].as_ref().unwrap().axes();let mut words=[0.;8];for(i,&n)in axes.iter().enumerate(){words[i]=n as f64;}Ok(V::from_dense(ScientificDenseV1::from_real_cooperative(ScientificShapeV1::new(&[1,axes.len() as u64])?,&words[..axes.len()],control).await?))},
        "issparse"=>boolean(args[0].as_ref().unwrap().is_sparse(),control).await,
        "nzmax"=>number(args[0].as_ref().unwrap().csc_view()?.nzmax() as f64,control).await,
        "nnz"=>{let value=args[0].as_ref().unwrap();if value.is_sparse(){return number(value.csc_view()?.nnz() as f64,control).await}let dense=value.dense_view()?;let mut n=0u64;for first in (0..dense.len()).step_by(32){let end=(first+32).min(dense.len());control.checkpoint(((end-first)*4) as u64).await?;for i in first..end{let z=dense.get(i).unwrap();n+=(z.re!=0.||z.im!=0.) as u64}}number(n as f64,control).await},
        "sparse"=>{let shape=ScientificCscShapeV1::new(dimension(args[3].as_ref().unwrap())?,dimension(args[4].as_ref().unwrap())?)?;mathlab_language::program::construct_scientific_sparse_v2(args[0].as_ref().unwrap(),args[1].as_ref().unwrap(),args[2].as_ref().unwrap(),shape,available,control).await},
        "zeros"|"ones"|"eye"|"reshape"=>{
            let reshape=name=="reshape";let shape=dimensions(&args,reshape as usize,count,control).await?;
            if reshape{let input=args[0].take().unwrap().into_dense_v1()?.into_dense();return Ok(V::from_dense(input.reshape(shape)?))}
            if name=="eye"&&shape.rank()!=2{return Err(E::Shape)}require_headroom(shape.len().checked_mul(8).ok_or(E::Resource)?,available)?;let mut out=ScientificDenseV1::zeros_cooperative(shape,false,control).await?;
            if name=="ones"{for first in (0..out.len()).step_by(128){let end=(first+128).min(out.len());control.checkpoint((end-first) as u64).await?;for i in first..end{out.set(i,Complex64V1::ONE)?}}}else if name=="eye"{let n=shape.axis(0).min(shape.axis(1)) as usize;for first in (0..n).step_by(128){let end=(first+128).min(n);control.checkpoint((end-first) as u64).await?;for i in first..end{out.set(i+i*shape.axis(0) as usize,Complex64V1::ONE)?}}}Ok(V::from_dense(out))
        },
        _=>Err(E::Unsupported),
    }
}
pub(super) async fn range(a:V,step:Option<V>,z:V,available:usize,control:&mut impl ScientificControlV1)->ResultV<V>{
    for v in [Some(&a),step.as_ref(),Some(&z)].into_iter().flatten(){if v.is_sparse()||v.class()!=ProgramValueClassV1::Double||v.is_complex(){return Err(E::Type)}}
    let start=a.scalar()?;let increment=step.as_ref().map(V::scalar).transpose()?.unwrap_or(1.);let stop=z.scalar()?;
    if ![start,increment,stop].iter().all(|x|x.is_finite()&&x.fract()==0.&&x.abs()<=9_007_199_254_740_991.)||increment==0.{return Err(E::Unsupported)}
    let(a,b,z)=(start as i128,increment as i128,stop as i128);let n=if b>0&&z<a||b<0&&z>a{0}else{usize::try_from((z-a)/b+1).map_err(|_|E::Resource)?};let shape=ScientificShapeV1::new(&[1,n as u64])?;require_headroom(n.checked_mul(8).ok_or(E::Resource)?,available)?;let mut out=ScientificDenseV1::zeros_cooperative(shape,false,control).await?;
    for first in (0..n).step_by(128){let end=(first+128).min(n);control.checkpoint((end-first) as u64).await?;for i in first..end{out.set(i,Complex64V1::real((a+b*i as i128) as f64))?}}Ok(V::from_dense(out))
}
// Selectors are borrowed from the machine stack, never copied or densified.
struct Selection{counts:[usize;8],shape:ScientificShapeV1,target:ScientificShapeV1,rank:usize,full:ProgramAxes,linear:bool}
async fn selection(target:&V,args:&[V],full:ProgramAxes,control:&mut impl ScientificControlV1)->ResultV<Selection>{
    let target=target.dense_view()?.shape();let count=args.len();if count!=1&&count!=target.rank(){return Err(E::Shape)}let mut counts=[1usize;8];
    for axis in 0..count{let extent=if count==1{target.len()}else{target.axis(axis) as usize};if full[axis]{counts[axis]=extent;continue}let arg=args[axis].dense_view()?;let shape=arg.shape();if args[axis].class()!=ProgramValueClassV1::Double||arg.is_complex()||shape.rank()!=2||!(shape.axis(0)==1||shape.axis(1)==1||shape.axes()==[0,0]){return Err(E::Index)}counts[axis]=arg.len();
        for first in (0..arg.len()).step_by(128){let end=(first+128).min(arg.len());control.checkpoint((end-first) as u64).await?;for i in first..end{let x=arg.get(i).unwrap().re;if !x.is_finite()||x.fract()!=0.||x<1.||x>extent as f64{return Err(E::Index)}}}
    }
    let shape=if count==1{if full[0]{ScientificShapeV1::new(&[counts[0] as u64,1])}else if target.rank()==2&&target.axis(0)==1&&target.axis(1)!=1{ScientificShapeV1::new(&[1,counts[0] as u64])}else if target.rank()==2&&target.axis(1)==1&&target.axis(0)!=1{ScientificShapeV1::new(&[counts[0] as u64,1])}else{Ok(args[0].dense_view()?.shape())}}else{let mut axes=[1u64;8];for i in 0..count{axes[i]=counts[i] as u64}ScientificShapeV1::new(&axes[..count])}?;
    Ok(Selection{counts,shape,target,rank:count,full,linear:count==1})
}
fn selected(s:&Selection,args:&[V],mut n:usize)->usize{let mut at=0;for axis in 0..s.rank{let coordinate=n%s.counts[axis];n/=s.counts[axis];let i=if s.full[axis]{coordinate}else{args[axis].dense_component(coordinate).unwrap().re as usize-1};at+=i*if s.linear{1}else{s.target.stride(axis).unwrap()}}at}
async fn zeros(shape:ScientificShapeV1,logical:bool,complex:bool,available:usize,control:&mut impl ScientificControlV1)->ResultV<ScientificDenseV1>{require_headroom(shape.len().checked_mul(if logical{1}else if complex{16}else{8}).ok_or(E::Resource)?,available)?;if logical{ScientificDenseV1::zeros_logical_cooperative(shape,control).await}else{ScientificDenseV1::zeros_cooperative(shape,complex,control).await}}
pub(super) async fn gather(target:&V,args:&[V],full:ProgramAxes,available:usize,control:&mut impl ScientificControlV1)->ResultV<V>{
    let s=selection(target,args,full,control).await?;let source=target.dense_view()?;let mut out=zeros(s.shape,target.class()==ProgramValueClassV1::Logical,target.is_complex(),available,control).await?;
    let cost=(s.rank+2) as u64;let chunk=(128/cost) as usize;for first in (0..out.len()).step_by(chunk){let end=(first+chunk).min(out.len());control.checkpoint((end-first) as u64*cost).await?;for i in first..end{out.set(i,source.get(selected(&s,args,i)).unwrap())?}}Ok(V::from_dense(out))
}
pub(super) async fn replace(target:&V,args:&[V],full:ProgramAxes,rhs:&V,available:usize,control:&mut impl ScientificControlV1)->ResultV<V>{
    let s=selection(target,args,full,control).await?;let source=target.dense_view()?;let rhs_dense=rhs.dense_view()?;if rhs.is_empty(){return Err(E::Unsupported)}if target.class()!=rhs.class(){return Err(E::Type)}if rhs_dense.len()!=1{for axis in 0..8{if rhs_dense.shape().axis(axis)!=s.shape.axis(axis){return Err(E::Shape)}}}
    let promote=!s.shape.is_empty()&&rhs.is_complex()&&!target.is_complex();let bytes=if promote{source.len().checked_mul(16).ok_or(E::Resource)?}else{target.payload_bytes()};require_headroom(bytes,available)?;let mut out=if promote{source.copy_as_complex_cooperative(control).await?}else{source.try_copy_cooperative(control).await?};let cost=(s.rank+2) as u64;let chunk=(128/cost) as usize;
    for first in (0..s.shape.len()).step_by(chunk){let end=(first+chunk).min(s.shape.len());control.checkpoint((end-first) as u64*cost).await?;for i in first..end{out.set(selected(&s,args,i),rhs_dense.get(if rhs_dense.len()==1{0}else{i}).unwrap())?}}Ok(V::from_dense(out))
}
pub(super) async fn column(frozen:&V,column:usize,available:usize,control:&mut impl ScientificControlV1)->ResultV<V>{let source=frozen.dense_view()?;if source.shape().rank()!=2{return Err(E::Unsupported)}let rows=source.shape().axis(0) as usize;let mut out=zeros(ScientificShapeV1::new(&[rows as u64,1])?,frozen.class()==ProgramValueClassV1::Logical,frozen.is_complex(),available,control).await?;for first in (0..rows).step_by(64){let end=(first+64).min(rows);control.checkpoint(((end-first)*2) as u64).await?;for i in first..end{out.set(i,source.get(i+column*rows).ok_or(E::Index)?)?}}Ok(V::from_dense(out))}
pub(super) async fn matrix(values:&[V],info:&[(usize,usize,usize,usize)],total_rows:usize,columns:usize,logical:bool,complex:bool,available:usize,control:&mut impl ScientificControlV1)->ResultV<V>{
    let mut out=zeros(ScientificShapeV1::new(&[total_rows as u64,columns as u64])?,logical,complex,available,control).await?;let mut top=0;
    for &(start,count,height,_)in info{let mut left=0;for value in &values[start..start+count]{let source=value.dense_view()?;let width=source.shape().axis(1) as usize;control.checkpoint(1).await?;if height==0{left+=width;continue}for col in 0..width{for first in (0..height).step_by(64){let end=(first+64).min(height);control.checkpoint(((end-first)*2) as u64).await?;for row in first..end{out.set((top+row)+(left+col)*total_rows,source.get(row+col*height).unwrap())?}}}left+=width}top+=height}Ok(V::from_dense(out))
}
