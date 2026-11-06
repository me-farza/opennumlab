// SPDX-License-Identifier: GPL-3.0-or-later
//! Sparse-left times dense-right, with increasing-column accumulation order.
use super::{Complex64V1 as C,ScientificControlV1,ScientificErrorV1 as E,ScientificDenseV1 as Dense,ScientificShapeV1 as DenseShape,ScientificClassV1};
use super::sparse::{ScientificCscV1,admit_working_set};
impl ScientificCscV1{
    pub fn multiply_dense_output_bytes(&self,right:&Dense)->Result<usize,E>{
        let shape=right.shape();if shape.rank()!=2||shape.axes()[0] as usize!=self.shape.columns(){return Err(E::Shape)}
        if right.class()!=ScientificClassV1::Double{return Err(E::Type)}
        let output=DenseShape::new(&[self.shape.rows() as u64,shape.axes()[1]])?;
        output.len().checked_mul(if self.is_complex()||right.is_complex(){16}else{8}).ok_or(E::Resource)
    }
    /// Nonfinite SpMV is not native-qualified: this finite-only kernel rejects
    /// nonfinite retained CSC values (cached classification) and every dense RHS component,
    /// including components multiplied only by structural zeros. No implicit
    /// densification of the sparse operand, no conjugation, and no publication.
    pub async fn multiply_dense_cooperative(&self,right:&Dense,available_bytes:usize,control:&mut impl ScientificControlV1)->Result<Dense,E>{
        let bytes=self.multiply_dense_output_bytes(right)?;admit_working_set(bytes,available_bytes)?;
        if self.has_nonfinite_values(){return Err(E::NonFiniteInput)}
        for i in 0..right.len(){
            if i%16==0{control.checkpoint(((right.len()-i).min(16)*8) as u64).await?}
            if !right.get(i).unwrap().finite(){return Err(E::NonFiniteInput)}
        }
        let output_shape=DenseShape::new(&[self.shape.rows() as u64,right.shape().axes()[1]])?;
        let mut output=Dense::zeros_cooperative(output_shape,self.is_complex()||right.is_complex(),control).await?;
        for rhs in 0..right.shape().axes()[1] as usize{
            for column in 0..self.shape.columns(){
                control.checkpoint(8).await?;
                let b=right.get(column+rhs*self.shape.columns()).unwrap();let start=self.columns[column] as usize;let end=self.columns[column+1] as usize;
                for slot in start..end{
                    if (slot-start)%8==0{control.checkpoint(((end-slot).min(8)*16) as u64).await?}
                    let index=self.rows[slot] as usize+rhs*self.shape.rows();let a=self.values.get(slot);let prior=output.get(index).unwrap();
                    let z=if output.is_complex(){prior.add(a.mul(b))}else{C::real(prior.re+a.re*b.re)};
                    if !z.finite(){return Err(E::ArithmeticBreakdown)}output.set(index,z)?;
                }
            }
        }
        Ok(output)
    }
}
