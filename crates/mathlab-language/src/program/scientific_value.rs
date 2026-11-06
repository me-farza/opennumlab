// SPDX-License-Identifier: GPL-3.0-or-later
//! Additive scientific value API. Legacy ProgramValueV1 representation unchanged.
use super::ProgramValueClassV1;
pub use mathlab_array::scientific::{Complex64V1,ScientificDenseV1,ScientificShapeV1,ScientificErrorV1,ScientificReadRangeV1,ScientificControlV1,ScientificSolveDiagnosticsV1,yield_scientific_v1};
#[derive(Debug)]
pub struct ScientificProgramValueV1 { pub(crate) dense:ScientificDenseV1 }
impl ScientificProgramValueV1 {
    pub fn from_dense(dense:ScientificDenseV1)->Self{Self{dense}}
    pub fn as_dense(&self)->&ScientificDenseV1{&self.dense}
    pub fn into_dense(self)->ScientificDenseV1{self.dense}
    pub fn shape(&self)->&[u64]{self.dense.shape_ref().axes()}
    pub fn rank(&self)->usize{self.shape().len()}
    pub fn len(&self)->usize{self.dense.len()}
    pub fn is_empty(&self)->bool{self.dense.is_empty()}
    pub fn is_complex(&self)->bool{self.dense.is_complex()}
    pub fn class(&self)->ProgramValueClassV1{match self.dense.class(){mathlab_array::scientific::ScientificClassV1::Double=>ProgramValueClassV1::Double,mathlab_array::scientific::ScientificClassV1::Logical=>ProgramValueClassV1::Logical}}
    pub fn payload_bytes(&self)->usize{self.dense.capacity_bytes()}
    pub fn word(&self,i:usize)->Option<f64>{if self.is_complex(){None}else{self.dense.get(i).map(|z|z.re)}}
    pub fn component(&self,i:usize)->Option<Complex64V1>{self.dense.get(i)}
    pub fn scalar(&self)->Result<f64,ScientificErrorV1>{self.dense.scalar_real()}
    pub fn read_range(&self,start:usize,count:usize)->Result<ScientificReadRangeV1<'_>,ScientificErrorV1>{self.dense.read_range(start,count)}
    pub async fn try_copy_controlled(&self,control:&mut impl ScientificControlV1)->Result<Self,ScientificErrorV1>{Ok(Self::from_dense(self.dense.try_copy_cooperative(control).await?))}
}
