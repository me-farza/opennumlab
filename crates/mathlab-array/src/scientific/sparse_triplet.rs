// SPDX-License-Identifier: GPL-3.0-or-later
//! Stable, cooperative triplet canonicalization; no std sorting or dense image.
use super::{Complex64V1 as C,ScientificControlV1,ScientificErrorV1 as E,reserve};
use super::sparse::{ScientificCscV1,ScientificCscShapeV1,CscStorage,admit_working_set};

#[derive(Debug,Clone,Copy)]
pub enum ScientificTripletValuesV1<'a>{Real(&'a[f64]),Complex(&'a[C]),RealScalar(f64),ComplexScalar(C)}
impl ScientificTripletValuesV1<'_>{
    fn validate_length(self,n:usize)->Result<(),E>{match self{Self::Real(v) if v.len()!=n=>Err(E::Shape),Self::Complex(v) if v.len()!=n=>Err(E::Shape),_=>Ok(())}}
    fn get(self,i:usize)->C{match self{Self::Real(v)=>C::real(v[i]),Self::Complex(v)=>v[i],Self::RealScalar(v)=>C::real(v),Self::ComplexScalar(v)=>v}}
}
/// Selected S01-S04 direction: separate binary64 additions in original order,
/// retaining generated NaN/Inf. No universal native-bit/summation-order claim.
fn input_order_duplicate_add(a:C,b:C)->C{a.add(b)}
#[derive(Debug,Default,Clone,Copy,PartialEq,Eq)]
struct CoalescedSummary{nnz:usize,complex:bool,nonfinite:bool}
fn emit(out:&mut Option<&mut ScientificCscV1>,row:u32,column:u32,z:C,summary:&mut CoalescedSummary)->Result<(),E>{
    if z.re==0.&&z.im==0.{return Ok(())}
    summary.nnz=summary.nnz.checked_add(1).ok_or(E::Resource)?;
    summary.complex|=z.im!=0.;summary.nonfinite|=!z.finite();
    if let Some(out)=out.as_deref_mut(){
        if out.rows.len()==out.rows.capacity(){return Err(E::Resource)}
        out.values.push(z)?;out.rows.push(row);
        out.columns[column as usize+1]=out.columns[column as usize+1].checked_add(1).ok_or(E::Resource)?;
    }
    Ok(())
}
async fn coalesce(order:&[u32],rows:&[u32],columns:&[u32],values:ScientificTripletValuesV1<'_>,mut out:Option<&mut ScientificCscV1>,control:&mut impl ScientificControlV1)->Result<CoalescedSummary,E>{
    let mut current:Option<(u32,u32,C)>=None;let mut summary=CoalescedSummary::default();
    for i in 0..order.len(){
        if i%4==0{control.checkpoint(((order.len()-i).min(4)*32) as u64).await?}
        let at=order[i] as usize;let row=rows[at];let column=columns[at];let z=values.get(at);
        match current{
            Some((r,c,acc)) if (r,c)==(row,column)=>current=Some((r,c,input_order_duplicate_add(acc,z))),
            Some((r,c,acc))=>{emit(&mut out,r,c,acc,&mut summary)?;current=Some((row,column,z));},
            None=>current=Some((row,column,z)),
        }
    }
    if let Some((r,c,acc))=current{emit(&mut out,r,c,acc,&mut summary)?}Ok(summary)
}
impl ScientificCscV1{
    /// Library indices are ZERO-BASED u32; a script adapter must separately
    /// validate finite exact positive integer indices before converting them.
    /// `available_bytes` is host payload headroom after retained inputs.
    /// Phase1 preflights both permutation buffers before reserving either;
    /// phase2 preflights their sum plus the exact final CSC before its reserves.
    /// The host reconciles released scratch after success (exactly 8*n bytes),
    /// or discards/reconciles its request ledger on error/drop. reserve() is an
    /// admission/fault hook, not a general allocator lifetime notification.
    pub async fn from_triplets_cooperative(shape:ScientificCscShapeV1,rows:&[u32],columns:&[u32],values:ScientificTripletValuesV1<'_>,available_bytes:usize,control:&mut impl ScientificControlV1)->Result<Self,E>{
        let n=rows.len();if columns.len()!=n{return Err(E::Shape)}values.validate_length(n)?;
        let scratch_bytes=Self::triplet_scratch_bytes(n)?;admit_working_set(scratch_bytes,available_bytes)?;
        for i in 0..n{
            if i%16==0{control.checkpoint(((n-i).min(16)*8) as u64).await?}
            if rows[i] as usize>=shape.rows()||columns[i] as usize>=shape.columns(){return Err(E::Index)}
        }
        let mut order=reserve::<u32>(n,control)?;let mut scratch=reserve::<u32>(n,control)?;
        while order.len()<n{let count=(n-order.len()).min(128);control.checkpoint(count as u64).await?;let start=order.len();for i in start..start+count{order.push(i as u32)}}
        while scratch.len()<n{let count=(n-scratch.len()).min(128);control.checkpoint(count as u64).await?;scratch.resize(scratch.len()+count,0)}
        let mut width=1usize;
        while width<n{
            let mut start=0usize;
            while start<n{
                let middle=(start+width).min(n);let end=(middle+width).min(n);let mut left=start;let mut right=middle;let mut dest=start;
                while dest<end{
                    let limit=(dest+16).min(end);control.checkpoint(((limit-dest)*8) as u64).await?;
                    while dest<limit{
                        let take_left=right==end||(left<middle&&{
                            let a=order[left] as usize;let b=order[right] as usize;
                            (columns[a],rows[a])<=(columns[b],rows[b])
                        });
                        scratch[dest]=if take_left{let value=order[left];left+=1;value}else{let value=order[right];right+=1;value};dest+=1;
                    }
                }
                start=end;
            }
            std::mem::swap(&mut order,&mut scratch);width=width.checked_mul(2).ok_or(E::Resource)?;
        }
        let summary=coalesce(&order,rows,columns,values,None,control).await?;let nnz=summary.nnz;let slots=nnz.max(1);
        let payload=Self::payload_bytes_for(shape,nnz,summary.complex)?;
        admit_working_set(scratch_bytes.checked_add(payload).ok_or(E::Resource)?,available_bytes)?;
        let pointers=reserve::<u32>(shape.columns()+1,control)?;let indices=reserve::<u32>(slots,control)?;
        let data=if summary.complex{CscStorage::Complex(reserve::<C>(slots,control)?)}else{CscStorage::Real(reserve::<f64>(slots,control)?)};
        let mut result=Self{shape,columns:pointers,rows:indices,values:data,nonfinite:summary.nonfinite};
        while result.columns.len()<shape.columns()+1{let count=(shape.columns()+1-result.columns.len()).min(128);control.checkpoint(count as u64).await?;result.columns.resize(result.columns.len()+count,0)}
        let filled=coalesce(&order,rows,columns,values,Some(&mut result),control).await?;if filled!=summary{return Err(E::Resource)}
        for column in 0..shape.columns(){
            if column%32==0{control.checkpoint(((shape.columns()-column).min(32)*4) as u64).await?}
            result.columns[column+1]=result.columns[column+1].checked_add(result.columns[column]).ok_or(E::Resource)?;
        }
        if result.capacity_bytes()!=payload{return Err(E::Resource)}Ok(result)
    }
}
