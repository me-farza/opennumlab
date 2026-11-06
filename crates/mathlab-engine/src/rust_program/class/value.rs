// SPDX-License-Identifier: GPL-3.0-or-later
//! Typed class values; no scalar surrogate, recursive host object, or COW.
use mathlab_language::program::{ScientificProgramValueV2,ScientificErrorV1,ClassProgramGraphV1};
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum ClassObjectKindV1{Value,Handle}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct ClassObjectIdV1{pub(super) owner:u64,pub(super) epoch:u64,pub(super) slot:u16,pub(super) generation:u64,pub(super) kind:ClassObjectKindV1}
impl ClassObjectIdV1{pub fn kind(self)->ClassObjectKindV1{self.kind}pub fn slot(self)->usize{self.slot as usize}pub fn generation(self)->u64{self.generation}}
#[derive(Debug)]
pub enum ClassValueV1{Scientific(ScientificProgramValueV2),Object(ClassObjectIdV1)}
impl ClassValueV1{
 /// Directly owned numeric bytes only. Object cells/buffers are charged ONCE
 /// in the session heap, including deferred garbage, not once per alias.
 pub fn payload_bytes(&self)->usize{self.direct_payload_bytes()}
 pub fn direct_payload_bytes(&self)->usize{match self{Self::Scientific(v)=>v.payload_bytes(),Self::Object(_)=>0}}
 pub fn scientific(&self)->Result<&ScientificProgramValueV2,ClassErrorV1>{match self{Self::Scientific(v)=>Ok(v),Self::Object(_)=>Err(ClassErrorV1::Type)}}
 pub fn object(&self)->Result<ClassObjectIdV1,ClassErrorV1>{match self{Self::Object(id)=>Ok(*id),Self::Scientific(_)=>Err(ClassErrorV1::Type)}}
}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum ClassErrorV1{Type,Shape,Arity,UnknownClass,UnknownProperty,UnknownMethod,InvalidHandle,CyclicReference,Unsupported,Allocation,Resource,Cancelled,CounterExhausted,Scientific(ScientificErrorV1)}
impl From<ScientificErrorV1> for ClassErrorV1{fn from(e:ScientificErrorV1)->Self{match e{ScientificErrorV1::Allocation=>Self::Allocation,ScientificErrorV1::Resource=>Self::Resource,ScientificErrorV1::Cancelled=>Self::Cancelled,_=>Self::Scientific(e)}}}
/// Immutable event metadata remains meaningful after deletion or session drop.
/// Live access is available only through a session-borrowed view.
#[derive(Debug,Clone,Copy)]
pub struct ClassObjectMetadataV1{pub class_index:usize,pub(super) object:ClassObjectIdV1,pub(super) valid:bool}
impl ClassObjectMetadataV1{pub fn kind(self)->ClassObjectKindV1{self.object.kind()}pub fn slot(self)->usize{self.object.slot()}pub fn generation(self)->u64{self.object.generation()}pub fn is_valid(self)->bool{self.valid}}
#[derive(Debug,Clone)]
pub struct ClassObjectSnapshotV1{pub(super) graph:std::sync::Arc<ClassProgramGraphV1>,pub(super) metadata:ClassObjectMetadataV1}
impl ClassObjectSnapshotV1{pub fn class_name(&self)->&str{self.graph.text(self.graph.classes()[self.metadata.class_index].name).unwrap()}pub fn kind(&self)->ClassObjectKindV1{self.metadata.kind()}pub fn slot(&self)->usize{self.metadata.slot()}pub fn generation(&self)->u64{self.metadata.generation()}pub fn is_valid(&self)->bool{self.metadata.is_valid()}}
pub enum ClassValueViewV1<'a>{Scientific(&'a ScientificProgramValueV2),Object{class_name:&'a str,metadata:ClassObjectMetadataV1}}
