// SPDX-License-Identifier: GPL-3.0-or-later
//! Separate typed storage boundary. ScientificProgramValueV1/as_dense unchanged.
use super::{ProgramValueClassV1,ScientificProgramValueV1 as V1,ScientificDenseV1 as Dense,ScientificErrorV1 as E,ScientificControlV1,Complex64V1 as C};
pub use mathlab_array::scientific::{ScientificClassV1,ScientificCscV1,ScientificCscShapeV1,ScientificCscValuesV1,ScientificTripletValuesV1};
#[derive(Debug)]
enum Storage{Dense(Dense),Csc(ScientificCscV1)}
#[derive(Debug)]
pub struct ScientificProgramValueV2{storage:Storage,payload_bytes:usize}
#[derive(Debug,Clone,Copy)]
pub enum ScientificStorageViewV2<'a>{Dense(&'a Dense),Csc(&'a ScientificCscV1)}
#[derive(Debug,Clone,Copy)]
pub enum ScientificAxesV2<'a>{Dense(&'a[u64]),Csc([u64;2])}
impl std::ops::Deref for ScientificAxesV2<'_>{type Target=[u64];fn deref(&self)->&[u64]{match self{Self::Dense(v)=>v,Self::Csc(v)=>v}}}
impl ScientificProgramValueV2{
    pub fn from_dense(value:Dense)->Self{let payload_bytes=value.capacity_bytes();Self{storage:Storage::Dense(value),payload_bytes}}
    pub fn from_csc(value:ScientificCscV1)->Self{let payload_bytes=value.capacity_bytes();Self{storage:Storage::Csc(value),payload_bytes}}
    pub fn storage_view(&self)->ScientificStorageViewV2<'_>{match &self.storage{Storage::Dense(v)=>ScientificStorageViewV2::Dense(v),Storage::Csc(v)=>ScientificStorageViewV2::Csc(v)}}
    pub fn dense_view(&self)->Result<&Dense,E>{match &self.storage{Storage::Dense(v)=>Ok(v),Storage::Csc(_)=>Err(E::Unsupported)}}
    pub fn csc_view(&self)->Result<&ScientificCscV1,E>{match &self.storage{Storage::Csc(v)=>Ok(v),Storage::Dense(_)=>Err(E::Unsupported)}}
    pub fn is_sparse(&self)->bool{matches!(self.storage,Storage::Csc(_))}
    pub fn class(&self)->ProgramValueClassV1{match self.storage_view(){ScientificStorageViewV2::Dense(v)=>match v.class(){mathlab_array::scientific::ScientificClassV1::Double=>ProgramValueClassV1::Double,_=>ProgramValueClassV1::Logical},ScientificStorageViewV2::Csc(_)=>ProgramValueClassV1::Double}}
    pub fn is_complex(&self)->bool{match self.storage_view(){ScientificStorageViewV2::Dense(v)=>v.is_complex(),ScientificStorageViewV2::Csc(v)=>v.is_complex()}}
    pub fn axes(&self)->ScientificAxesV2<'_>{match self.storage_view(){ScientificStorageViewV2::Dense(v)=>ScientificAxesV2::Dense(v.shape_ref().axes()),ScientificStorageViewV2::Csc(v)=>ScientificAxesV2::Csc([v.shape().rows() as u64,v.shape().columns() as u64])}}
    pub fn rank(&self)->usize{self.axes().len()}
    pub fn logical_numel(&self)->u64{match self.storage_view(){ScientificStorageViewV2::Dense(v)=>v.len() as u64,ScientificStorageViewV2::Csc(v)=>v.shape().numel()}}
    pub fn is_empty(&self)->bool{self.logical_numel()==0}
    pub fn stored_len(&self)->usize{match self.storage_view(){ScientificStorageViewV2::Dense(v)=>v.len(),ScientificStorageViewV2::Csc(v)=>v.nnz()}}
    pub fn payload_bytes(&self)->usize{self.payload_bytes}
    pub fn scalar(&self)->Result<f64,E>{
        let z=match self.storage_view(){ScientificStorageViewV2::Dense(v)=>{if v.len()!=1{return Err(E::Shape)}if v.is_complex(){return Err(E::Type)}v.get(0).unwrap()},ScientificStorageViewV2::Csc(v)=>{if v.shape().numel()!=1{return Err(E::Shape)}if v.is_complex(){return Err(E::Type)}if v.nnz()==0{C::ZERO}else{match v.values(){ScientificCscValuesV1::Real(x)=>C::real(x[0]),_=>return Err(E::Type)}}}};
        if z.re.is_nan(){return Err(E::Type)}Ok(z.re)
    }
    pub fn dense_component(&self,index:usize)->Result<C,E>{self.dense_view()?.get(index).ok_or(E::Index)}
    pub fn into_dense_v1(self)->Result<V1,E>{match self.storage{Storage::Dense(v)=>Ok(V1::from_dense(v)),Storage::Csc(_)=>Err(E::Unsupported)}}
    pub async fn try_copy_cooperative(&self,available_bytes:usize,control:&mut impl ScientificControlV1)->Result<Self,E>{
        if self.payload_bytes>available_bytes.min(64<<20){return Err(E::Resource)}
        match self.storage_view(){ScientificStorageViewV2::Dense(v)=>Ok(Self::from_dense(v.try_copy_cooperative(control).await?)),ScientificStorageViewV2::Csc(v)=>Ok(Self::from_csc(v.try_copy_cooperative(available_bytes,control).await?))}
    }
}
