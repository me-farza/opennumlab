// SPDX-License-Identifier: GPL-3.0-or-later
//! Canonical CSC with a logical shape independent of dense element admission.
//! Selected constructor policies follow reviewed native inputs; capacity growth
//! and nonfinite multiplication remain explicitly qualified, not parity claims.
use super::{Complex64V1 as C,ScientificErrorV1 as E,MAX_ELEMENTS_V1,MAX_VALUE_BYTES_V1};

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct ScientificCscShapeV1{rows:u32,columns:u32}
impl ScientificCscShapeV1{
    pub fn new(rows:u64,columns:u64)->Result<Self,E>{
        if rows>MAX_ELEMENTS_V1 as u64||columns>MAX_ELEMENTS_V1 as u64{return Err(E::Resource)}
        Ok(Self{rows:rows as u32,columns:columns as u32})
    }
    pub fn rows(self)->usize{self.rows as usize}
    pub fn columns(self)->usize{self.columns as usize}
    pub fn numel(self)->u64{u64::from(self.rows)*u64::from(self.columns)}
    pub fn is_empty(self)->bool{self.rows==0||self.columns==0}
}
#[derive(Debug)]
pub(super) enum CscStorage{Real(Vec<f64>),Complex(Vec<C>)}
impl CscStorage{
    pub(super) fn get(&self,index:usize)->C{match self{Self::Real(v)=>C::real(v[index]),Self::Complex(v)=>v[index]}}
    pub(super) fn push(&mut self,z:C)->Result<(),E>{match self{Self::Real(v)=>{if z.im!=0.{return Err(E::Type)}if v.len()==v.capacity(){return Err(E::Resource)}v.push(z.re)},Self::Complex(v)=>{if v.len()==v.capacity(){return Err(E::Resource)}v.push(z)}}Ok(())}
    fn capacity(&self)->usize{match self{Self::Real(v)=>v.capacity(),Self::Complex(v)=>v.capacity()}}
    fn bytes(&self)->usize{self.capacity()*match self{Self::Real(_)=>8,Self::Complex(_)=>16}}
}
#[derive(Debug)]
pub struct ScientificCscV1{
    pub(super) shape:ScientificCscShapeV1,
    pub(super) columns:Vec<u32>,
    pub(super) rows:Vec<u32>,
    pub(super) values:CscStorage,
    pub(super) nonfinite:bool,
}
#[derive(Debug,Clone,Copy)]
pub enum ScientificCscValuesV1<'a>{Real(&'a[f64]),Complex(&'a[C])}
impl ScientificCscV1{
    pub fn shape(&self)->ScientificCscShapeV1{self.shape}
    pub fn nnz(&self)->usize{self.rows.len()}
    /// Actual admitted value-slot capacity is max(nnz,1) in this constructor.
    /// This is not a MATLAB nzmax/growth-equivalence promise.
    pub fn nzmax(&self)->usize{self.values.capacity()}
    pub fn is_complex(&self)->bool{matches!(self.values,CscStorage::Complex(_))}
    /// Derived from retained coalesced values; constructors/copy maintain it.
    pub fn has_nonfinite_values(&self)->bool{self.nonfinite}
    pub fn capacity_bytes(&self)->usize{self.columns.capacity()*4+self.rows.capacity()*4+self.values.bytes()}
    pub fn column_pointers(&self)->&[u32]{&self.columns}
    pub fn row_indices(&self)->&[u32]{&self.rows}
    pub fn values(&self)->ScientificCscValuesV1<'_>{match &self.values{CscStorage::Real(v)=>ScientificCscValuesV1::Real(v),CscStorage::Complex(v)=>ScientificCscValuesV1::Complex(v)}}
    /// Exact final three-vector requested-capacity sum, checked before any of
    /// the final allocations. Each successful vector must expose that capacity.
    pub fn payload_bytes_for(shape:ScientificCscShapeV1,nnz:usize,complex:bool)->Result<usize,E>{
        if nnz>MAX_ELEMENTS_V1||nnz as u64>shape.numel(){return Err(E::Resource)}
        let columns=shape.columns().checked_add(1).and_then(|n|n.checked_mul(4)).ok_or(E::Resource)?;
        let entries=nnz.max(1).checked_mul(if complex&&nnz!=0{20}else{12}).ok_or(E::Resource)?;
        columns.checked_add(entries).filter(|n|*n<=MAX_VALUE_BYTES_V1).ok_or(E::Resource)
    }
    /// Two u32 permutation buffers coexist through canonicalization.
    pub fn triplet_scratch_bytes(input_count:usize)->Result<usize,E>{
        if input_count>MAX_ELEMENTS_V1{return Err(E::Resource)}input_count.checked_mul(8).ok_or(E::Resource)
    }
}
/// Caller supplies remaining host payload headroom after retained inputs.
/// Existing reserve hooks still independently admit every real reservation.
/// The library never raises the selected 64MiB host limit.
pub(super) fn admit_working_set(bytes:usize,available_bytes:usize)->Result<(),E>{
    if bytes>available_bytes.min(64<<20){Err(E::Resource)}else{Ok(())}
}

async fn copy_words<T:Copy>(words:&[T],capacity:usize,units_per_word:usize,control:&mut impl super::ScientificControlV1)->Result<Vec<T>,E>{
    if capacity<words.len(){return Err(E::Resource)}let mut out=super::reserve::<T>(capacity,control)?;
    for chunk in words.chunks(128/units_per_word){control.checkpoint((chunk.len()*units_per_word) as u64).await?;out.extend_from_slice(chunk)}Ok(out)
}
impl ScientificCscV1{
    /// Independent three-vector copy, needed by the later value ownership seam;
    /// no Arc/COW, dense projection or mutation of the borrowed input.
    pub async fn try_copy_cooperative(&self,available_bytes:usize,control:&mut impl super::ScientificControlV1)->Result<Self,E>{
        admit_working_set(self.capacity_bytes(),available_bytes)?;
        let columns=copy_words(&self.columns,self.columns.capacity(),1,control).await?;
        let rows=copy_words(&self.rows,self.rows.capacity(),1,control).await?;
        let values=match &self.values{CscStorage::Real(v)=>CscStorage::Real(copy_words(v,v.capacity(),1,control).await?),CscStorage::Complex(v)=>CscStorage::Complex(copy_words(v,v.capacity(),2,control).await?)};
        let copy=Self{shape:self.shape,columns,rows,values,nonfinite:self.nonfinite};
        if copy.capacity_bytes()!=self.capacity_bytes(){return Err(E::Resource)}Ok(copy)
    }
}
