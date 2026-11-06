// SPDX-License-Identifier: GPL-3.0-or-later
//! New-profile generic real kernels; accepted V1 kernels remain untouched.
//! Separate binary64 operations, no FMA/fast-math or native-bit promise.
use super::{ScientificProgramValueV1 as V,ScientificDenseV1 as Dense,ScientificShapeV1 as Shape,ScientificControlV1,ScientificErrorV1 as E,ScientificLeafOperationV1 as O,ScientificNumericJobV1 as Job,ScientificJobProgressV1,ScientificSolveDiagnosticsV1,ProgramValueClassV1,BinaryOperatorV1 as B,UnaryOperatorV1 as U,Complex64V1 as C,yield_scientific_v1};
fn broadcast_shape(a:&V,b:Option<&V>)->Result<Shape,E>{
    let rank=a.rank().max(b.map_or(2,V::rank));let mut axes=[1u64;8];
    for (i,n) in axes[..rank].iter_mut().enumerate(){let x=a.shape().get(i).copied().unwrap_or(1);let y=b.and_then(|v|v.shape().get(i)).copied().unwrap_or(1);*n=if x==y||y==1{x}else if x==1{y}else{return Err(E::Shape)}}Shape::new(&axes[..rank])
}
fn source_offset(v:&V,out:Shape,mut index:usize)->usize{let own=v.as_dense().shape();let mut offset=0;for axis in 0..out.rank(){let n=out.axis(axis) as usize;let coordinate=if n==0{0}else{let c=index%n;index/=n;c};if own.axis(axis)!=1{offset+=coordinate*own.stride(axis).unwrap_or(0)}}offset}
fn supports_real(op:O,a:&V,b:Option<&V>)->bool{
    if a.is_complex()||a.class()!=ProgramValueClassV1::Double||b.is_some_and(|b|b.is_complex()||b.class()!=ProgramValueClassV1::Double){return false}
    match op{O::Unary(U::Plus|U::Minus)|O::Transpose(_)|O::Real|O::Imag|O::Conjugate|O::NormInf=>true,O::Binary(B::Add|B::Subtract|B::Multiply|B::ElementMultiply|B::ElementDivide|B::ElementLeftDivide)=>true,O::Binary(B::Divide)=>b.is_some_and(|b|b.len()==1),O::Binary(B::LeftDivide)=>a.len()==1,_=>false}
}
async fn all_finite(a:&V,b:Option<&V>,control:&mut impl ScientificControlV1)->Result<bool,E>{
    let mut finite=true;for value in [Some(a),b].into_iter().flatten(){let words=value.as_dense().real_words().ok_or(E::Type)?;for block in words.chunks(64){control.checkpoint((block.len()*2) as u64).await?;for &word in block{finite&=word.is_finite()}}}Ok(finite)
}
async fn real_kernel(op:O,a:&V,b:Option<&V>,control:&mut impl ScientificControlV1)->Result<Dense,E>{
    if matches!(op,O::NormInf){
        if a.rank()!=2{return Err(E::Rank)}let mut out=Dense::zeros_cooperative(Shape::new(&[1,1])?,false,control).await?;let rows=a.shape()[0] as usize;let columns=a.shape()[1] as usize;let words=a.as_dense().real_words().ok_or(E::Type)?;let mut best=0.0f64;
        for row in 0..rows{let mut sum=0.;for first in (0..columns).step_by(32){let end=(first+32).min(columns);control.checkpoint(((end-first)*4) as u64).await?;for column in first..end{sum+=words[row+column*rows].abs();if !sum.is_finite(){return Err(E::ArithmeticBreakdown)}}}control.checkpoint(2).await?;best=best.max(sum)}out.set(0,C::real(best))?;return Ok(out)
    }
    if matches!(op,O::Binary(B::Multiply))&&a.len()!=1&&b.ok_or(E::Shape)?.len()!=1{
        let b=b.unwrap();if a.rank()!=2||b.rank()!=2||a.shape()[1]!=b.shape()[0]{return Err(E::Shape)}let rows=a.shape()[0] as usize;let terms=a.shape()[1] as usize;let columns=b.shape()[1] as usize;let mut out=Dense::zeros_cooperative(Shape::new(&[rows as u64,columns as u64])?,false,control).await?;let left=a.as_dense().real_words().unwrap();let right=b.as_dense().real_words().unwrap();
        for column in 0..columns{control.checkpoint(1).await?;for row in 0..rows{let mut sum=0.;for first in (0..terms).step_by(16){let end=(first+16).min(terms);control.checkpoint(((end-first)*8) as u64).await?;for inner in first..end{let product=left[row+inner*rows]*right[inner+column*terms];sum+=product;if !product.is_finite()||!sum.is_finite(){return Err(E::ArithmeticBreakdown)}}}control.checkpoint(4).await?;out.set(row+column*rows,C::real(sum))?;}}return Ok(out)
    }
    let shape=if matches!(op,O::Transpose(_)){if a.rank()!=2{return Err(E::Rank)}Shape::new(&[a.shape()[1],a.shape()[0]])?}else{broadcast_shape(a,b)?};let mut out=Dense::zeros_cooperative(shape,false,control).await?;
    let transpose=matches!(op,O::Transpose(_));let units=if transpose{4}else{8};let chunk=128/units;
    for first in (0..out.len()).step_by(chunk){let end=(first+chunk).min(out.len());control.checkpoint(((end-first)*units) as u64).await?;for index in first..end{
        let ai=if transpose{let columns=a.shape()[1] as usize;(index/columns)+(index%columns)*a.shape()[0] as usize}else{source_offset(a,shape,index)};let x=a.word(ai).ok_or(E::Index)?;let y=b.map(|b|b.word(source_offset(b,shape,index)).ok_or(E::Index)).transpose()?;
        let z=match op{O::Unary(U::Plus)|O::Transpose(_)|O::Real|O::Conjugate=>x,O::Unary(U::Minus)=>-x,O::Imag=>0.,O::Binary(B::Add)=>x+y.unwrap(),O::Binary(B::Subtract)=>x-y.unwrap(),O::Binary(B::Multiply|B::ElementMultiply)=>x*y.unwrap(),O::Binary(B::Divide|B::ElementDivide)=>{let y=y.unwrap();if y==0.{return Err(E::Singular)}x/y},O::Binary(B::LeftDivide|B::ElementLeftDivide)=>{if x==0.{return Err(E::Singular)}y.unwrap()/x},_=>return Err(E::Unsupported)};
        if !z.is_finite(){return Err(E::ArithmeticBreakdown)}out.set(index,C::real(z))?;
    }}Ok(out)
}
/// Output/scratch admission still uses the original independently checked
/// layout plan. New real paths charge scans2/word, element8/word, transpose4,
/// matrix8/term+4/output+1/column and norm4/term+2/row; preparation charges separately.
/// Real division uses hardware IEEE binary64 division in this new profile.
/// Nonfinite/logical/complex/other operations use the unchanged V1 implementation.
pub async fn evaluate_scientific_dense_v2(op:O,a:V,b:Option<V>,available_bytes:usize,control:&mut impl ScientificControlV1)->Result<(V,Option<ScientificSolveDiagnosticsV1>),E>{
    let real=supports_real(op,&a,b.as_ref())&&all_finite(&a,b.as_ref(),control).await?;
    // Consumed vector storage changes only its shape: no phantom output reserve.
    if real&&matches!(op,O::Transpose(_))&&a.rank()==2&&(a.shape()[0]==1||a.shape()[1]==1){let shape=Shape::new(&[a.shape()[1],a.shape()[0]])?;return Ok((V::from_dense(a.into_dense().reshape(shape)?),None))}
    let extra=Job::additional_bytes(op,&a,b.as_ref())?;if extra>available_bytes.min(64<<20){return Err(E::Resource)}
    if real{return Ok((V::from_dense(real_kernel(op,&a,b.as_ref(),control).await?),None))}
    let mut job=Job::new_cooperative(op,a,b,control).await?;
    loop{yield_scientific_v1().await;let progress=job.step(32768,control)?;yield_scientific_v1().await;if progress==ScientificJobProgressV1::Complete{let diagnostics=job.solve_diagnostics();return Ok((job.into_result()?,diagnostics))}}
}

use super::{ScientificProgramValueV2 as V2,ScientificStorageViewV2,ScientificCscV1,ScientificCscShapeV1,ScientificCscValuesV1,ScientificTripletValuesV1};
fn stored(a:&ScientificCscV1,index:usize)->C{match a.values(){ScientificCscValuesV1::Real(v)=>C::real(v[index]),ScientificCscValuesV1::Complex(v)=>v[index]}}
fn ieee_product(a:C,b:C,left_complex:bool,right_complex:bool)->C{
    match (left_complex,right_complex){(false,false)=>C::real(a.re*b.re),(true,false)=>C{re:a.re*b.re,im:a.im*b.re},(false,true)=>C{re:a.re*b.re,im:a.re*b.im},(true,true)=>a.mul(b)}
}
/// Explicit V2 IEEE-capable route; the accepted finite-checked CSC API stays
/// untouched. Structural zeros are skipped. Component paths avoid phantom
/// imaginary cross products when one operand is represented as real.
pub async fn multiply_sparse_dense_ieee_v2(a:&ScientificCscV1,b:&Dense,available_bytes:usize,control:&mut impl ScientificControlV1)->Result<V2,E>{
    if b.class()!=mathlab_array::scientific::ScientificClassV1::Double{return Err(E::Type)}
    if b.shape().rank()!=2||b.shape().axes()[0] as usize!=a.shape().columns(){return Err(E::Shape)}
    let scalar=a.shape().rows()==1&&a.shape().columns()==1&&b.len()==1;
    if scalar{
        let right=b.get(0).unwrap();
        // No selected evidence for an implicit-zero/nonfinite scalar product.
        if a.nnz()==0&&!right.finite(){return Err(E::Unsupported)}
        let left=if a.nnz()==0{C::ZERO}else{stored(a,0)};
        let right_complex=b.is_complex()&&!(right.im==0.&&!a.is_complex());
        control.checkpoint(16).await?;
        let value=if a.nnz()==0{C::ZERO}else{ieee_product(left,right,a.is_complex(),right_complex)};
        let shape=ScientificCscShapeV1::new(1,1)?;
        return Ok(V2::from_csc(ScientificCscV1::from_triplets_cooperative(shape,&[0],&[0],ScientificTripletValuesV1::ComplexScalar(value),available_bytes,control).await?))
    }
    let shape=Shape::new(&[a.shape().rows() as u64,b.shape().axes()[1]])?;let complex=a.is_complex()||b.is_complex();
    let peak=shape.len().checked_mul(if complex{24}else{8}).ok_or(E::Resource)?;if peak>available_bytes.min(64<<20){return Err(E::Resource)}
    let mut out=Dense::zeros_cooperative(shape,complex,control).await?;let pointers=a.column_pointers();let rows=a.row_indices();let units=if complex{16}else{8};let chunk=128/units;
    for rhs in 0..b.shape().axes()[1] as usize{control.checkpoint(1).await?;for column in 0..a.shape().columns(){
        control.checkpoint(8).await?;let right=b.get(column+rhs*a.shape().columns()).unwrap();let start=pointers[column] as usize;let end=pointers[column+1] as usize;
        for first in (start..end).step_by(chunk){let limit=(first+chunk).min(end);control.checkpoint(((limit-first)*units) as u64).await?;for slot in first..limit{
            let index=rows[slot] as usize+rhs*a.shape().rows();let value=ieee_product(stored(a,slot),right,a.is_complex(),b.is_complex());let prior=out.get(index).unwrap();let value=if complex{prior.add(value)}else{C::real(prior.re+value.re)};out.set(index,value)?;
        }}
    }}
    if complex{
        let mut has_imaginary=false;for first in (0..out.len()).step_by(64){let end=(first+64).min(out.len());control.checkpoint(((end-first)*2) as u64).await?;for i in first..end{has_imaginary|=out.get(i).unwrap().im!=0.;}}
        if !has_imaginary{let mut real=Dense::zeros_cooperative(shape,false,control).await?;for first in (0..out.len()).step_by(64){let end=(first+64).min(out.len());control.checkpoint(((end-first)*2) as u64).await?;for i in first..end{real.set(i,C::real(out.get(i).unwrap().re))?;}}return Ok(V2::from_dense(real))}
    }
    Ok(V2::from_dense(out))
}

fn triplet_vector(value:&V2)->Result<&Dense,E>{let v=value.dense_view()?;if v.class()!=mathlab_array::scientific::ScientificClassV1::Double{return Err(E::Type)}if v.shape().rank()!=2||!(v.len()==0||v.shape().axes()[0]==1||v.shape().axes()[1]==1){return Err(E::Shape)}Ok(v)}
async fn index_buffer(n:usize,control:&mut impl ScientificControlV1)->Result<Vec<u32>,E>{
    let bytes=n.checked_mul(4).filter(|n|*n<=16<<20).ok_or(E::Resource)?;let mut out=Vec::new();control.reserve(bytes)?;out.try_reserve_exact(n).map_err(|_|E::Allocation)?;if out.capacity()!=n{return Err(E::Resource)}Ok(out)
}
/// Scalar i/j/v expand independently. Zero-length nonscalars select count0
/// when all other nonscalars also have length0; unused scalar values are not
/// rejected for NaN/complex components. Typed index conversion stays explicit.
pub async fn construct_scientific_sparse_v2(i:&V2,j:&V2,v:&V2,shape:ScientificCscShapeV1,available_bytes:usize,control:&mut impl ScientificControlV1)->Result<V2,E>{
    let i=triplet_vector(i)?;let j=triplet_vector(j)?;let v=triplet_vector(v)?;let mut count=None;
    for n in [i.len(),j.len(),v.len()]{if n!=1{if count.is_some_and(|old|old!=n){return Err(E::Shape)}count=Some(n)}}let count=count.unwrap_or(1);
    if count>1<<20{return Err(E::Resource)}if count!=0&&(i.is_complex()||j.is_complex()){return Err(E::Type)}
    let adapter=count.checked_mul(8).ok_or(E::Resource)?;if adapter>available_bytes.min(64<<20){return Err(E::Resource)}
    let mut rows=index_buffer(count,control).await?;let mut columns=index_buffer(count,control).await?;
    for first in (0..count).step_by(8){let end=(first+8).min(count);control.checkpoint(((end-first)*16) as u64).await?;for at in first..end{
        let row=i.get(if i.len()==1{0}else{at}).unwrap();let column=j.get(if j.len()==1{0}else{at}).unwrap();
        if row.im!=0.||column.im!=0.||!row.re.is_finite()||!column.re.is_finite()||row.re.fract()!=0.||column.re.fract()!=0.||row.re<1.||column.re<1.||row.re>shape.rows() as f64||column.re>shape.columns() as f64{return Err(E::Index)}
        rows.push(row.re as u32-1);columns.push(column.re as u32-1);
    }}
    let values=if v.is_complex(){if v.len()==1{ScientificTripletValuesV1::ComplexScalar(v.get(0).unwrap())}else{ScientificTripletValuesV1::Complex(v.complex_words().unwrap())}}else if v.len()==1{ScientificTripletValuesV1::RealScalar(v.get(0).unwrap().re)}else{ScientificTripletValuesV1::Real(v.real_words().unwrap())};
    let result=ScientificCscV1::from_triplets_cooperative(shape,&rows,&columns,values,available_bytes-adapter,control).await?;Ok(V2::from_csc(result))
}
/// Validate every retained NaN before an implicit-zero false shortcut. Infinity
/// is not NaN. Complex conditional truth retains the supported V1 restriction.
pub async fn scientific_truth_v2(value:&V2,control:&mut impl ScientificControlV1)->Result<bool,E>{
    if let ScientificStorageViewV2::Csc(v)=value.storage_view(){
        for first in (0..v.nnz()).step_by(16){let end=(first+16).min(v.nnz());control.checkpoint(((end-first)*8) as u64).await?;for at in first..end{let z=stored(v,at);if z.re.is_nan()||z.im.is_nan(){return Err(E::Type)}}}
        if v.is_complex(){return Err(E::Type)}return Ok(v.shape().numel()!=0&&v.nnz() as u64==v.shape().numel())
    }
    let dense=value.dense_view()?;if dense.is_complex(){return Err(E::Type)}let mut yes=!dense.is_empty();
    for first in (0..dense.len()).step_by(128){let end=(first+128).min(dense.len());control.checkpoint((end-first) as u64).await?;for at in first..end{let x=dense.get(at).unwrap().re;if x.is_nan(){return Err(E::Type)}yes&=x!=0.;}}
    Ok(yes)
}
