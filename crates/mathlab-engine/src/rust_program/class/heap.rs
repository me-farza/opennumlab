// SPDX-License-Identifier: GPL-3.0-or-later
//! Flat session heap. Private roots precede suspension; garbage stays charged.
use super::value::*;
use mathlab_language::program::ScientificControlV1;
use std::{cell::RefCell,mem::size_of};
pub(super) const OBJECTS:usize=1024;
pub(super) const PROPERTIES:usize=64;
type R<T>=Result<T,ClassErrorV1>;
pub(super) trait ClassHeapControl:ScientificControlV1{
 fn reserve_metadata(&mut self,bytes:usize)->R<()>;
 fn release_metadata(&mut self,bytes:usize)->R<()>;
 fn release_payload(&mut self,bytes:usize)->R<()>;
 fn poison(&mut self);
}
#[derive(Debug)]
pub(super) struct Record{pub class_index:usize,pub kind:ClassObjectKindV1,pub valid:bool,pub properties:Vec<Option<ClassValueV1>>}
#[derive(Debug)]
struct Slot{generation:u64,record:Option<Record>}
/// Separate from HeapStorage so an owned child future may mutate storage while
/// an owned root lease keeps this bounded bookkeeping registry alive. The Arc
/// contains no object arena/payload; object storage remains the flat session Vec.
#[derive(Debug)]
pub(super) struct PrivateRoots{counts:RefCell<[u16;OBJECTS]>,copy_active:std::cell::Cell<bool>}
impl PrivateRoots{
 pub fn new()->Self{Self{counts:RefCell::new([0;OBJECTS]),copy_active:std::cell::Cell::new(false)}}
 pub fn begin_copy(self:&std::sync::Arc<Self>)->R<CopyLease>{if self.copy_active.replace(true){return Err(ClassErrorV1::Resource)}Ok(CopyLease{roots:std::sync::Arc::clone(self)})}
 #[cfg(test)] pub fn copy_is_active(&self)->bool{self.copy_active.get()}
 pub fn hold(self:&std::sync::Arc<Self>,id:ClassObjectIdV1)->R<RootedObject>{let mut counts=self.counts.borrow_mut();let n=counts.get_mut(id.slot()).ok_or(ClassErrorV1::Resource)?;*n=n.checked_add(1).ok_or(ClassErrorV1::Resource)?;Ok(RootedObject{id,roots:std::sync::Arc::clone(self)})}
 fn contains(&self,slot:usize)->bool{self.counts.borrow()[slot]!=0}
}
pub(super) struct CopyLease{roots:std::sync::Arc<PrivateRoots>}
impl Drop for CopyLease{fn drop(&mut self){assert!(self.roots.copy_active.replace(false),"class copy lease balance");}}
pub(super) struct RootedObject{pub id:ClassObjectIdV1,roots:std::sync::Arc<PrivateRoots>}
impl Drop for RootedObject{fn drop(&mut self){let mut counts=self.roots.counts.borrow_mut();let n=&mut counts[self.id.slot()];assert!(*n>0,"private root balance");*n-=1;}}
#[derive(Debug,Clone,Copy)]
pub(super) struct Publication{pub published:[u64;OBJECTS/64],expected_epoch:u64,next_epoch:u64}
#[derive(Debug)]
pub(super) struct PropertyCommit{publication:Publication,next_payload_bytes:usize}
#[derive(Debug)]
pub(super) struct HeapStorage{owner:u64,domain_epoch:u64,mutation_epoch:u64,pub poisoned:std::cell::Cell<bool>,live_count:usize,slots:Vec<Slot>,published:[u64;OBJECTS/64],pub payload_bytes:usize,pub metadata_bytes:usize}
impl HeapStorage{
 pub fn empty(owner:u64,domain_epoch:u64)->Self{Self{owner,domain_epoch,mutation_epoch:0,poisoned:std::cell::Cell::new(false),live_count:0,slots:Vec::new(),published:[0;OBJECTS/64],payload_bytes:0,metadata_bytes:0}}
 pub fn object_slots(&self)->usize{self.slots.len()}
 pub fn live_objects(&self)->usize{self.live_count}
 fn next_epoch(&self)->R<u64>{self.mutation_epoch.checked_add(1).ok_or_else(||{self.poisoned.set(true);ClassErrorV1::CounterExhausted})}
 pub fn new(owner:u64,domain_epoch:u64,control:&mut impl ClassHeapControl)->R<Self>{let bytes=OBJECTS.checked_mul(size_of::<Slot>()).ok_or(ClassErrorV1::Resource)?;control.reserve_metadata(bytes)?;let mut slots=Vec::new();if slots.try_reserve_exact(OBJECTS).is_err(){control.release_metadata(bytes)?;return Err(ClassErrorV1::Allocation)}if slots.capacity()!=OBJECTS{control.release_metadata(bytes)?;return Err(ClassErrorV1::Resource)}Ok(Self{owner,domain_epoch,mutation_epoch:0,poisoned:std::cell::Cell::new(false),live_count:0,slots,published:[0;OBJECTS/64],payload_bytes:0,metadata_bytes:bytes})}
 fn id(&self,slot:usize)->R<ClassObjectIdV1>{let s=self.slots.get(slot).ok_or(ClassErrorV1::InvalidHandle)?;let r=s.record.as_ref().ok_or(ClassErrorV1::InvalidHandle)?;Ok(ClassObjectIdV1{owner:self.owner,epoch:self.domain_epoch,slot:slot as u16,generation:s.generation,kind:r.kind})}
 pub fn record(&self,id:ClassObjectIdV1)->R<&Record>{if id.owner!=self.owner||id.epoch!=self.domain_epoch{return Err(ClassErrorV1::InvalidHandle)}let s=self.slots.get(id.slot()).ok_or(ClassErrorV1::InvalidHandle)?;if s.generation!=id.generation{return Err(ClassErrorV1::InvalidHandle)}let r=s.record.as_ref().ok_or(ClassErrorV1::InvalidHandle)?;if r.kind!=id.kind{return Err(ClassErrorV1::InvalidHandle)}Ok(r)}
 fn record_mut(&mut self,id:ClassObjectIdV1)->R<&mut Record>{if id.owner!=self.owner||id.epoch!=self.domain_epoch{return Err(ClassErrorV1::InvalidHandle)}let s=self.slots.get_mut(id.slot()).ok_or(ClassErrorV1::InvalidHandle)?;if s.generation!=id.generation{return Err(ClassErrorV1::InvalidHandle)}let r=s.record.as_mut().ok_or(ClassErrorV1::InvalidHandle)?;if r.kind!=id.kind{return Err(ClassErrorV1::InvalidHandle)}Ok(r)}
 pub fn is_valid(&self,id:ClassObjectIdV1)->bool{self.record(id).is_ok_and(|r|r.valid)}
 pub fn is_published(&self,id:ClassObjectIdV1)->R<bool>{self.record(id)?;Ok(self.published[id.slot()/64]&(1u64<<(id.slot()%64))!=0)}
 pub async fn allocate(&mut self,roots:&std::sync::Arc<PrivateRoots>,class_index:usize,kind:ClassObjectKindV1,properties:usize,control:&mut impl ClassHeapControl)->R<RootedObject>{
  if properties>PROPERTIES||self.slots.capacity()!=OBJECTS{return Err(ClassErrorV1::Resource)}let mut vacant=None;
  for first in (0..self.slots.len()).step_by(128){let end=(first+128).min(self.slots.len());control.checkpoint((end-first) as u64).await?;for i in first..end{if self.slots[i].record.is_none(){vacant=Some(i);break}}if vacant.is_some(){break}}
  let slot=vacant.unwrap_or(self.slots.len());if slot==OBJECTS{return Err(ClassErrorV1::Resource)}let generation=if slot==self.slots.len(){1}else{self.slots[slot].generation.checked_add(1).ok_or(ClassErrorV1::CounterExhausted)?};let bytes=properties.checked_mul(size_of::<Option<ClassValueV1>>()).ok_or(ClassErrorV1::Resource)?;control.reserve_metadata(bytes)?;
  let mut values=Vec::new();if values.try_reserve_exact(properties).is_err(){control.release_metadata(bytes)?;return Err(ClassErrorV1::Allocation)}if values.capacity()!=properties{control.release_metadata(bytes)?;return Err(ClassErrorV1::Resource)}
  let next_epoch=self.next_epoch()?;let next_live_count=self.live_count.checked_add(1).ok_or(ClassErrorV1::Resource)?;
  let record=Record{class_index,kind,valid:true,properties:values};if slot==self.slots.len(){self.slots.push(Slot{generation,record:Some(record)})}else{self.slots[slot]=Slot{generation,record:Some(record)}}self.metadata_bytes=self.metadata_bytes.checked_add(bytes).ok_or(ClassErrorV1::Resource)?;
  self.mutation_epoch=next_epoch;self.live_count=next_live_count;let id=self.id(slot)?;let root=roots.hold(id)?; // registered BEFORE the first fill suspension
  let next_epoch=self.next_epoch()?;control.checkpoint(properties as u64).await?;self.record_mut(id)?.properties.resize_with(properties,||None);self.mutation_epoch=next_epoch;Ok(root)
 }
 /// Prepare all reachable handle publication bits and reject a new reference
 /// cycle. This copy is128 stack bytes; commit replaces it without a graph walk.
 pub async fn prepare_publication(&self,value:&ClassValueV1,cycle_owner:Option<ClassObjectIdV1>,publish:bool,control:&mut impl ClassHeapControl)->R<Publication>{
  let mut bitmap=self.published;let mut seen=[0u64;OBJECTS/64];let mut todo=[0u16;OBJECTS];let mut count=0usize;
  if let ClassValueV1::Object(id)=value{self.record(*id)?;todo[0]=id.slot;count=1;seen[id.slot()/64]|=1u64<<(id.slot()%64)}
  while count!=0{count-=1;let slot=todo[count] as usize;let id=self.id(slot)?;if cycle_owner==Some(id){return Err(ClassErrorV1::CyclicReference)}let record=self.record(id)?;control.checkpoint((record.properties.len()+1) as u64).await?;
   if publish&&record.kind==ClassObjectKindV1::Handle{bitmap[slot/64]|=1u64<<(slot%64)}
   if record.valid{for value in record.properties.iter().flatten(){if let ClassValueV1::Object(child)=value{self.record(*child)?;let bit=1u64<<(child.slot()%64);if seen[child.slot()/64]&bit==0{seen[child.slot()/64]|=bit;todo[count]=child.slot;count+=1;}}}}
  }Ok(Publication{published:bitmap,expected_epoch:self.mutation_epoch,next_epoch:self.next_epoch()?})
 }
 pub fn property(&self,id:ClassObjectIdV1,slot:usize)->R<&ClassValueV1>{let r=self.record(id)?;if !r.valid{return Err(ClassErrorV1::InvalidHandle)}r.properties.get(slot).and_then(Option::as_ref).ok_or(ClassErrorV1::UnknownProperty)}
 /// Caller validates ID/property/counters before its barrier. Returns the old
 /// value to an already prepared garbage owner; no allocation or await here.
 pub async fn prepare_property(&self,id:ClassObjectIdV1,slot:usize,value:&ClassValueV1,control:&mut impl ClassHeapControl)->R<PropertyCommit>{self.prepare_property_external(id,slot,value,false,control).await}
 pub async fn prepare_property_external(&self,id:ClassObjectIdV1,slot:usize,value:&ClassValueV1,publish_external:bool,control:&mut impl ClassHeapControl)->R<PropertyCommit>{
  let record=self.record(id)?;if !record.valid{return Err(ClassErrorV1::InvalidHandle)}let old=record.properties.get(slot).ok_or(ClassErrorV1::UnknownProperty)?.as_ref().map_or(0,ClassValueV1::direct_payload_bytes);let old_removed=self.payload_bytes.checked_sub(old).ok_or_else(||{self.poisoned.set(true);ClassErrorV1::Resource})?;let next_payload_bytes=old_removed.checked_add(value.direct_payload_bytes()).filter(|n|*n<=64<<20).ok_or(ClassErrorV1::Resource)?;
  let publish=publish_external||(record.kind==ClassObjectKindV1::Handle&&self.is_published(id)?);let publication=self.prepare_publication(value,Some(id),publish,control).await?;Ok(PropertyCommit{publication,next_payload_bytes})
 }
 pub fn validate_publication(&self,prepared:&Publication)->R<()>{if prepared.expected_epoch!=self.mutation_epoch{self.poisoned.set(true);return Err(ClassErrorV1::Resource)}Ok(())}
 pub fn validate_property(&self,id:ClassObjectIdV1,slot:usize,prepared:&PropertyCommit)->R<()>{self.validate_publication(&prepared.publication)?;let r=self.record(id)?;if !r.valid{return Err(ClassErrorV1::InvalidHandle)}if slot>=r.properties.len(){return Err(ClassErrorV1::UnknownProperty)}Ok(())}
 pub fn commit_property(&mut self,id:ClassObjectIdV1,slot:usize,value:ClassValueV1,prepared:PropertyCommit)->Option<ClassValueV1>{assert_eq!(self.mutation_epoch,prepared.publication.expected_epoch,"exclusive prepared property operation");let r=self.record_mut(id).expect("prepared object");assert!(r.valid);let old=r.properties[slot].replace(value);self.published=prepared.publication.published;self.payload_bytes=prepared.next_payload_bytes;self.mutation_epoch=prepared.publication.next_epoch;old}
 pub fn commit_publication(&mut self,prepared:Publication){assert_eq!(self.mutation_epoch,prepared.expected_epoch,"exclusive prepared publication");self.published=prepared.published;self.mutation_epoch=prepared.next_epoch;}
 pub fn empty_publication(&self)->R<Publication>{Ok(Publication{published:self.published,expected_epoch:self.mutation_epoch,next_epoch:self.next_epoch()?})}
 pub fn prepare_delete(&self,id:ClassObjectIdV1)->R<Publication>{let r=self.record(id)?;if !r.valid{return Err(ClassErrorV1::InvalidHandle)}if r.kind!=ClassObjectKindV1::Handle{return Err(ClassErrorV1::Type)}Ok(Publication{published:self.published,expected_epoch:self.mutation_epoch,next_epoch:self.next_epoch()?})}
 pub fn commit_delete(&mut self,id:ClassObjectIdV1,prepared:Publication){assert_eq!(self.mutation_epoch,prepared.expected_epoch,"exclusive prepared delete");self.mutation_epoch=prepared.next_epoch;let r=self.record_mut(id).expect("prepared object");assert_eq!(r.kind,ClassObjectKindV1::Handle);r.valid=false;}
 pub async fn collect(&mut self,roots:&[ClassObjectIdV1],private:&PrivateRoots,control:&mut impl ClassHeapControl)->R<usize>{
  let mut bits=[0u64;OBJECTS/64];for block in roots.chunks(128){control.checkpoint(block.len() as u64).await?;for &id in block{self.record(id)?;bits[id.slot()/64]|=1u64<<(id.slot()%64)}}self.collect_bitmap(bits,private,control).await
 }
 pub async fn collect_bitmap(&mut self,mut marked:[u64;OBJECTS/64],private:&PrivateRoots,control:&mut impl ClassHeapControl)->R<usize>{
  let mut todo=[0u16;OBJECTS];let mut count=0usize;
  for first in (0..self.slots.len()).step_by(128){let end=(first+128).min(self.slots.len());control.checkpoint((end-first) as u64).await?;for slot in first..end{if marked[slot/64]&(1u64<<(slot%64))!=0{self.id(slot)?;todo[count]=slot as u16;count+=1;}}}
  for first in (0..self.slots.len()).step_by(128){let end=(first+128).min(self.slots.len());control.checkpoint((end-first) as u64).await?;for slot in first..end{if private.contains(slot)&&marked[slot/64]&(1u64<<(slot%64))==0{self.id(slot)?;marked[slot/64]|=1u64<<(slot%64);todo[count]=slot as u16;count+=1;}}}
  while count!=0{count-=1;let id=self.id(todo[count] as usize)?;let r=self.record(id)?;control.checkpoint((r.properties.len()+1) as u64).await?;if r.valid{for v in r.properties.iter().flatten(){if let ClassValueV1::Object(child)=v{self.record(*child)?;let bit=1u64<<(child.slot()%64);if marked[child.slot()/64]&bit==0{marked[child.slot()/64]|=bit;todo[count]=child.slot;count+=1}}}}}
  let mut freed=0;
  for slot in 0..self.slots.len(){control.checkpoint(1).await?;let marked=marked[slot/64]&(1u64<<(slot%64))!=0;let Some(r)=self.slots[slot].record.as_ref()else{continue};if marked&&r.valid{continue}let length=r.properties.len();
   for index in 0..length{control.checkpoint(1).await?;let next_epoch=self.next_epoch()?;let value=self.slots[slot].record.as_mut().unwrap().properties[index].take();self.mutation_epoch=next_epoch;if let Some(value)=value{let bytes=value.direct_payload_bytes();drop(value);self.payload_bytes=self.payload_bytes.checked_sub(bytes).ok_or_else(||{self.poisoned.set(true);control.poison();ClassErrorV1::Resource})?;if let Err(error)=control.release_payload(bytes){self.poisoned.set(true);control.poison();return Err(error)}}}
   if !marked{let next_epoch=self.next_epoch()?;let r=self.slots[slot].record.take().unwrap();self.mutation_epoch=next_epoch;let bytes=r.properties.capacity()*size_of::<Option<ClassValueV1>>();drop(r);self.metadata_bytes=self.metadata_bytes.checked_sub(bytes).ok_or_else(||{self.poisoned.set(true);control.poison();ClassErrorV1::Resource})?;if let Err(error)=control.release_metadata(bytes){self.poisoned.set(true);control.poison();return Err(error)};self.published[slot/64]&=!(1u64<<(slot%64));self.live_count=self.live_count.checked_sub(1).ok_or_else(||{self.poisoned.set(true);control.poison();ClassErrorV1::Resource})?;freed+=1;}
  }Ok(freed)
 }
}

#[cfg(test)]
#[path="heap_tests.rs"]
mod tests;

#[cfg(test)] pub(super) fn maximum_metadata_bytes()->usize{OBJECTS*std::mem::size_of::<Slot>()+OBJECTS*PROPERTIES*std::mem::size_of::<Option<ClassValueV1>>()}

#[cfg(test)] impl HeapStorage{
 pub(super) fn audit_cached_ownership(&self){
  let mut metadata=self.slots.capacity()*size_of::<Slot>();let mut payload=0usize;let mut live=0usize;
  for slot in &self.slots{if let Some(record)=&slot.record{live+=1;metadata+=record.properties.capacity()*size_of::<Option<ClassValueV1>>();for value in record.properties.iter().flatten(){payload+=value.direct_payload_bytes();}}}
  assert_eq!(self.metadata_bytes,metadata,"actual retained slot/property capacities");assert_eq!(self.payload_bytes,payload,"actual retained numeric property ownership");assert_eq!(self.live_count,live,"all attached records including private garbage");
 }
}
