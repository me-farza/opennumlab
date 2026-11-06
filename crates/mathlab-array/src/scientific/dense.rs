// SPDX-License-Identifier: GPL-3.0-or-later
use super::{Complex64V1 as C,ScientificErrorV1 as E,MAX_RANK_V1,MAX_ELEMENTS_V1,MAX_VALUE_BYTES_V1,reserve,ScientificControlV1};
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum ScientificClassV1 { Double, Logical }
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct ScientificShapeV1 { axes:[u64;MAX_RANK_V1], strides:[usize;MAX_RANK_V1], rank:u8, len:usize }
impl ScientificShapeV1 {
    pub fn new(axes:&[u64])->Result<Self,E> {
        if !(2..=MAX_RANK_V1).contains(&axes.len()){return Err(E::Rank)}
        let mut rank=axes.len();while rank>2&&axes[rank-1]==1{rank-=1;}
        let axes=&axes[..rank];
        let mut out=Self{axes:[1;MAX_RANK_V1],strides:[0;MAX_RANK_V1],rank:axes.len() as u8,len:1};
        for (i,&n) in axes.iter().enumerate(){
            if n>MAX_ELEMENTS_V1 as u64{return Err(E::Resource)}
            out.axes[i]=n;out.strides[i]=out.len;
            out.len=out.len.checked_mul(n as usize).ok_or(E::Resource)?;
        }
        if out.len>MAX_ELEMENTS_V1{return Err(E::Resource)} Ok(out)
    }
    pub fn axes(&self)->&[u64]{&self.axes[..self.rank as usize]}
    pub fn rank(&self)->usize{self.rank as usize}
    pub fn len(&self)->usize{self.len}
    pub fn is_empty(&self)->bool{self.len==0}
    pub fn stride(&self,axis:usize)->Option<usize>{(axis<self.rank()).then(||self.strides[axis])}
    pub fn axis(&self,axis:usize)->u64{self.axes.get(axis).copied().unwrap_or(1)}
    pub fn offset(&self,indices:&[usize])->Result<usize,E>{
        if indices.len()!=self.rank(){return Err(E::Rank)} let mut n=0usize;
        for (axis,&i) in indices.iter().enumerate(){if i>=self.axes[axis] as usize{return Err(E::Index)}n=n.checked_add(i.checked_mul(self.strides[axis]).ok_or(E::Resource)?).ok_or(E::Resource)?;}Ok(n)
    }
}
#[derive(Debug)]
enum Storage{Real(Vec<f64>),Complex(Vec<C>),Logical(Vec<u8>)}
#[derive(Debug)]
pub struct ScientificDenseV1{shape:ScientificShapeV1,data:Storage}
impl ScientificDenseV1 {
    pub fn shape(&self)->ScientificShapeV1{self.shape}
    pub fn shape_ref(&self)->&ScientificShapeV1{&self.shape}
    pub fn len(&self)->usize{self.shape.len()}
    pub fn is_empty(&self)->bool{self.len()==0}
    pub fn class(&self)->ScientificClassV1{match self.data{Storage::Logical(_)=>ScientificClassV1::Logical,_=>ScientificClassV1::Double}}
    pub fn is_complex(&self)->bool{matches!(self.data,Storage::Complex(_))}
    pub fn capacity_bytes(&self)->usize{match &self.data{Storage::Real(v)=>v.capacity()*8,Storage::Complex(v)=>v.capacity()*16,Storage::Logical(v)=>v.capacity()}}
    pub fn real_words(&self)->Option<&[f64]>{if let Storage::Real(v)=&self.data{Some(v)}else{None}}
    pub fn complex_words(&self)->Option<&[C]>{if let Storage::Complex(v)=&self.data{Some(v)}else{None}}
    pub fn logical_words(&self)->Option<&[u8]>{if let Storage::Logical(v)=&self.data{Some(v)}else{None}}
    /// Complete borrowed, canonical linear range; no copy, clipping or projection.
    pub fn read_range(&self,start:usize,count:usize)->Result<ScientificReadRangeV1<'_>,E>{
        let end=start.checked_add(count).filter(|n|*n<=self.len()).ok_or(E::Index)?;
        Ok(match &self.data{Storage::Real(v)=>ScientificReadRangeV1::Real(&v[start..end]),Storage::Complex(v)=>ScientificReadRangeV1::Complex(&v[start..end]),Storage::Logical(v)=>ScientificReadRangeV1::Logical(&v[start..end])})
    }
    pub fn get(&self,i:usize)->Option<C>{match &self.data{Storage::Real(v)=>v.get(i).map(|&x|C::real(x)),Storage::Complex(v)=>v.get(i).copied(),Storage::Logical(v)=>v.get(i).map(|&x|C::real(x as f64))}}
    pub fn scalar_real(&self)->Result<f64,E>{if self.len()!=1||self.is_complex(){return Err(E::Type)}Ok(self.get(0).unwrap().re)}
    pub async fn from_real_cooperative(shape:ScientificShapeV1,words:&[f64],control:&mut impl ScientificControlV1)->Result<Self,E>{
        if shape.len()!=words.len(){return Err(E::Shape)} let mut v=reserve(words.len(),control)?;
        for block in words.chunks(128){control.checkpoint(block.len() as u64).await?;v.extend_from_slice(block)}Ok(Self{shape,data:Storage::Real(v)})
    }
    pub async fn from_complex_cooperative(shape:ScientificShapeV1,words:&[C],control:&mut impl ScientificControlV1)->Result<Self,E>{
        if shape.len()!=words.len(){return Err(E::Shape)}let mut v=reserve(words.len(),control)?;
        for block in words.chunks(64){control.checkpoint((block.len()*2) as u64).await?;v.extend_from_slice(block)}Ok(Self{shape,data:Storage::Complex(v)})
    }
    pub async fn from_logical_cooperative(shape:ScientificShapeV1,words:&[u8],control:&mut impl ScientificControlV1)->Result<Self,E>{
        if shape.len()!=words.len(){return Err(E::Shape)}let mut v=reserve(words.len(),control)?;
        for block in words.chunks(64){control.checkpoint((block.len()*2) as u64).await?;if block.iter().any(|x|*x>1){return Err(E::Type)}v.extend_from_slice(block)}Ok(Self{shape,data:Storage::Logical(v)})
    }
    pub async fn try_copy_cooperative(&self,control:&mut impl ScientificControlV1)->Result<Self,E>{match &self.data{Storage::Real(v)=>Self::from_real_cooperative(self.shape,v,control).await,Storage::Complex(v)=>Self::from_complex_cooperative(self.shape,v,control).await,Storage::Logical(v)=>Self::from_logical_cooperative(self.shape,v,control).await}}
    pub fn reshape(mut self,shape:ScientificShapeV1)->Result<Self,E>{if self.len()!=shape.len(){return Err(E::Shape)}self.shape=shape;Ok(self)}
    pub async fn zeros_cooperative(shape:ScientificShapeV1,complex:bool,control:&mut impl ScientificControlV1)->Result<Self,E>{
        let bytes=shape.len().checked_mul(if complex{16}else{8}).ok_or(E::Resource)?;if bytes>MAX_VALUE_BYTES_V1{return Err(E::Resource)}
        if complex{let mut v=reserve(shape.len(),control)?;while v.len()<shape.len(){let n=(shape.len()-v.len()).min(64);control.checkpoint((n*2) as u64).await?;v.resize(v.len()+n,C::ZERO)}Ok(Self{shape,data:Storage::Complex(v)})}
        else{let mut v=reserve(shape.len(),control)?;while v.len()<shape.len(){let n=(shape.len()-v.len()).min(128);control.checkpoint(n as u64).await?;v.resize(v.len()+n,0.)}Ok(Self{shape,data:Storage::Real(v)})}
    }
    pub async fn zeros_logical_cooperative(shape:ScientificShapeV1,control:&mut impl ScientificControlV1)->Result<Self,E>{
        let mut v=reserve(shape.len(),control)?;while v.len()<shape.len(){let n=(shape.len()-v.len()).min(128);control.checkpoint(n as u64).await?;v.resize(v.len()+n,0)}Ok(Self{shape,data:Storage::Logical(v)})
    }
    /// Candidate-only mutable access; caller publication must occur after completion.
    pub fn set(&mut self,i:usize,z:C)->Result<(),E>{match &mut self.data{Storage::Real(v)=>{if z.im!=0.{return Err(E::Type)}*v.get_mut(i).ok_or(E::Index)?=z.re},Storage::Complex(v)=>*v.get_mut(i).ok_or(E::Index)?=z,Storage::Logical(v)=>{if z.im!=0.||!(z.re==0.||z.re==1.){return Err(E::Type)}*v.get_mut(i).ok_or(E::Index)?=z.re as u8}}Ok(())}
    pub async fn copy_as_complex_cooperative(&self,control:&mut impl ScientificControlV1)->Result<Self,E>{
        if self.class()!=ScientificClassV1::Double{return Err(E::Type)}
        let mut v=reserve(self.len(),control)?;
        for i in 0..self.len(){if i%64==0{control.checkpoint(((self.len()-i).min(64)*2) as u64).await?}v.push(self.get(i).unwrap())}
        Ok(Self{shape:self.shape,data:Storage::Complex(v)})
    }
    pub async fn into_complex_cooperative(self,control:&mut impl ScientificControlV1)->Result<Self,E>{
        if self.is_complex(){return Ok(self)}if self.class()!=ScientificClassV1::Double{return Err(E::Type)}
        let mut v=reserve(self.len(),control)?;for i in 0..self.len(){if i%64==0{control.checkpoint((self.len()-i).min(64) as u64*2).await?}v.push(self.get(i).unwrap())}Ok(Self{shape:self.shape,data:Storage::Complex(v)})
    }
}
#[derive(Debug,Clone,Copy)]
pub enum ScientificReadRangeV1<'a>{Real(&'a[f64]),Complex(&'a[C]),Logical(&'a[u8])}

// Synchronous Rust convenience APIs use precisely the same preparation state
// machines; the program core awaits their cooperative counterparts directly.
impl ScientificDenseV1{
    pub fn from_real(s:ScientificShapeV1,w:&[f64],c:&mut impl ScientificControlV1)->Result<Self,E>{super::drive(Self::from_real_cooperative(s,w,c))}
    pub fn from_complex(s:ScientificShapeV1,w:&[C],c:&mut impl ScientificControlV1)->Result<Self,E>{super::drive(Self::from_complex_cooperative(s,w,c))}
    pub fn from_logical(s:ScientificShapeV1,w:&[u8],c:&mut impl ScientificControlV1)->Result<Self,E>{super::drive(Self::from_logical_cooperative(s,w,c))}
    pub fn try_copy(&self,c:&mut impl ScientificControlV1)->Result<Self,E>{super::drive(self.try_copy_cooperative(c))}
    pub fn zeros(s:ScientificShapeV1,b:bool,c:&mut impl ScientificControlV1)->Result<Self,E>{super::drive(Self::zeros_cooperative(s,b,c))}
    pub fn zeros_logical(s:ScientificShapeV1,c:&mut impl ScientificControlV1)->Result<Self,E>{super::drive(Self::zeros_logical_cooperative(s,c))}
    pub fn copy_as_complex(&self,c:&mut impl ScientificControlV1)->Result<Self,E>{super::drive(self.copy_as_complex_cooperative(c))}
    pub fn into_complex(self,c:&mut impl ScientificControlV1)->Result<Self,E>{super::drive(self.into_complex_cooperative(c))}
}
