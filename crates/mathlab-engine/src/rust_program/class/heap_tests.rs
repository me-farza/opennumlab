// SPDX-License-Identifier: GPL-3.0-or-later
//! Heap-only unit witnesses. Synthetic admission/release faults are not an
//! allocator/RSS/global-OOM oracle; the real cooperative checkpoint is retained.
use super::*;
use mathlab_language::program::{ScientificControlV1,ScientificErrorV1,ScientificDenseV1,ScientificShapeV1,ScientificProgramValueV2};
use std::{future::Future,sync::Arc,task::{Context,Poll,Waker}};

#[derive(Default)]
struct Control{metadata:usize,payload:usize,reserves:usize,charges:usize,fail_reserve:Option<usize>,stop_charge:Option<usize>,fail_metadata_release:bool,fail_payload_release:bool,poisoned:bool}
impl Control{
 fn reserve_site(&mut self)->R<()>{self.reserves+=1;if self.fail_reserve==Some(self.reserves){Err(ClassErrorV1::Allocation)}else{Ok(())}}
}
impl ScientificControlV1 for Control{
 fn charge(&mut self,units:u64)->Result<(),ScientificErrorV1>{assert!(units<=128);self.charges+=1;if self.stop_charge==Some(self.charges){Err(ScientificErrorV1::Cancelled)}else{Ok(())}}
 fn reserve(&mut self,bytes:usize)->Result<(),ScientificErrorV1>{self.reserve_site().map_err(|_|ScientificErrorV1::Allocation)?;self.payload=self.payload.checked_add(bytes).ok_or(ScientificErrorV1::Resource)?;Ok(())}
}
impl ClassHeapControl for Control{
 fn reserve_metadata(&mut self,bytes:usize)->R<()>{self.reserve_site()?;self.metadata=self.metadata.checked_add(bytes).ok_or(ClassErrorV1::Resource)?;Ok(())}
 fn release_metadata(&mut self,bytes:usize)->R<()>{if self.fail_metadata_release{return Err(ClassErrorV1::Resource)}self.metadata=self.metadata.checked_sub(bytes).ok_or(ClassErrorV1::Resource)?;Ok(())}
 fn release_payload(&mut self,bytes:usize)->R<()>{if self.fail_payload_release{return Err(ClassErrorV1::Resource)}self.payload=self.payload.checked_sub(bytes).ok_or(ClassErrorV1::Resource)?;Ok(())}
 fn poison(&mut self){self.poisoned=true;}
}
fn drive<F:Future>(f:F)->F::Output{let mut f=std::pin::pin!(f);let mut cx=Context::from_waker(Waker::noop());for _ in 0..8192{if let Poll::Ready(v)=f.as_mut().poll(&mut cx){return v}}panic!("bounded tiny heap future");}
fn numeric(c:&mut Control,words:&[f64])->ClassValueV1{let shape=ScientificShapeV1::new(&[1,words.len() as u64]).unwrap();let d=drive(ScientificDenseV1::from_real_cooperative(shape,words,c)).unwrap();ClassValueV1::Scientific(ScientificProgramValueV2::from_dense(d))}
fn put(h:&mut HeapStorage,id:ClassObjectIdV1,slot:usize,v:ClassValueV1,c:&mut Control){let prepared=drive(h.prepare_property(id,slot,&v,c)).unwrap();h.validate_property(id,slot,&prepared).unwrap();let old=h.commit_property(id,slot,v,prepared);if let Some(old)=old{let bytes=old.direct_payload_bytes();drop(old);c.release_payload(bytes).unwrap();}}
fn accounting(h:&HeapStorage,c:&Control){let records=h.slots.iter().filter_map(|s|s.record.as_ref());let mut metadata=h.slots.capacity()*size_of::<Slot>();let mut payload=0;let mut live=0;for r in records{live+=1;metadata+=r.properties.capacity()*size_of::<Option<ClassValueV1>>();payload+=r.properties.iter().flatten().map(ClassValueV1::direct_payload_bytes).sum::<usize>();}assert_eq!(h.live_objects(),live);assert_eq!(h.metadata_bytes,metadata);assert_eq!(h.payload_bytes,payload);assert_eq!(c.metadata,metadata);assert_eq!(c.payload,payload);assert!(!h.poisoned.get());assert!(!c.poisoned);}
fn object(h:&mut HeapStorage,roots:&Arc<PrivateRoots>,kind:ClassObjectKindV1,properties:usize,c:&mut Control)->RootedObject{drive(h.allocate(roots,0,kind,properties,c)).unwrap()}

#[test]
fn new_empty_and_each_actual_heap_reserve_refusal_preserve_ledger(){
 let roots=Arc::new(PrivateRoots::new());let mut c=Control::default();let mut empty=HeapStorage::empty(1,1);assert!(matches!(drive(empty.allocate(&roots,0,ClassObjectKindV1::Value,1,&mut c)),Err(ClassErrorV1::Resource)));assert_eq!(c.reserves,0);
 let mut fail=Control{fail_reserve:Some(1),..Default::default()};assert!(matches!(HeapStorage::new(1,1,&mut fail),Err(ClassErrorV1::Allocation)));assert_eq!(fail.metadata,0);
 let mut c=Control::default();let mut h=HeapStorage::new(1,1,&mut c).unwrap();assert_eq!(c.reserves,1);assert_eq!(h.slots.capacity(),OBJECTS);accounting(&h,&c);c.fail_reserve=Some(2);assert!(matches!(drive(h.allocate(&roots,0,ClassObjectKindV1::Value,3,&mut c)),Err(ClassErrorV1::Allocation)));assert_eq!(h.object_slots(),0);accounting(&h,&c);c.fail_reserve=None;let lease=object(&mut h,&roots,ClassObjectKindV1::Value,3,&mut c);assert_eq!(h.record(lease.id).unwrap().properties.len(),3);assert_eq!(c.reserves,3);accounting(&h,&c);
 // These are exactly the heap's slot and property Vec admission sites. Numeric
 // payload reserves are exercised separately through the actual provider.
}

#[test]
fn allocate_registers_private_root_before_first_fill_suspension_and_drop_defers_garbage(){
 let roots=Arc::new(PrivateRoots::new());let mut c=Control::default();let mut h=HeapStorage::new(3,4,&mut c).unwrap();
 {let f=h.allocate(&roots,0,ClassObjectKindV1::Value,4,&mut c);let mut f=std::pin::pin!(f);let mut cx=Context::from_waker(Waker::noop());assert!(f.as_mut().poll(&mut cx).is_pending());assert_eq!(roots.counts.borrow()[0],1);}
 assert_eq!(roots.counts.borrow()[0],0);assert_eq!(h.live_objects(),1);let r=h.record(h.id(0).unwrap()).unwrap();assert_eq!(r.properties.len(),0);assert_eq!(r.properties.capacity(),4);accounting(&h,&c);let charged=c.metadata;assert!(charged>OBJECTS*size_of::<Slot>());assert_eq!(drive(h.collect(&[],&roots,&mut c)).unwrap(),1);assert_eq!(h.live_objects(),0);assert_eq!(c.metadata,OBJECTS*size_of::<Slot>());accounting(&h,&c);let lease=object(&mut h,&roots,ClassObjectKindV1::Value,1,&mut c);assert_eq!(lease.id.slot(),0);assert_eq!(lease.id.generation(),2);
}

#[test]
fn allocate_fill_stop_releases_lease_and_partial_record_recovers(){
 let roots=Arc::new(PrivateRoots::new());let mut c=Control{stop_charge:Some(1),..Default::default()};let mut h=HeapStorage::new(1,1,&mut c).unwrap();assert!(matches!(drive(h.allocate(&roots,0,ClassObjectKindV1::Handle,3,&mut c)),Err(ClassErrorV1::Cancelled)));assert_eq!(roots.counts.borrow()[0],0);accounting(&h,&c);c.stop_charge=None;assert_eq!(drive(h.collect(&[],&roots,&mut c)).unwrap(),1);accounting(&h,&c);let _lease=object(&mut h,&roots,ClassObjectKindV1::Handle,1,&mut c);accounting(&h,&c);
}

#[test]
fn property_payload_replacement_charges_actual_capacity_once_not_per_alias(){
 let roots=Arc::new(PrivateRoots::new());let mut c=Control::default();let mut h=HeapStorage::new(1,1,&mut c).unwrap();let lease=object(&mut h,&roots,ClassObjectKindV1::Handle,2,&mut c);let alias=roots.hold(lease.id).unwrap();let value=numeric(&mut c,&[1.,2.,3.]);let bytes=value.direct_payload_bytes();put(&mut h,lease.id,0,value,&mut c);accounting(&h,&c);assert_eq!(h.payload_bytes,bytes);assert_eq!(roots.counts.borrow()[lease.id.slot()],2);let replacement=numeric(&mut c,&[9.]);put(&mut h,lease.id,0,replacement,&mut c);accounting(&h,&c);assert_eq!(h.payload_bytes,8);assert_eq!(h.property(alias.id,0).unwrap().scientific().unwrap().scalar().unwrap(),9.);drop(alias);drop(lease);assert_eq!(drive(h.collect(&[],&roots,&mut c)).unwrap(),1);assert_eq!(c.payload,0);accounting(&h,&c);
}

fn fixture()->(HeapStorage,Arc<PrivateRoots>,Control,RootedObject,ClassObjectIdV1,ClassObjectIdV1){
 let roots=Arc::new(PrivateRoots::new());let mut c=Control::default();let mut h=HeapStorage::new(7,8,&mut c).unwrap();let root=object(&mut h,&roots,ClassObjectKindV1::Handle,2,&mut c);let child=object(&mut h,&roots,ClassObjectKindV1::Value,2,&mut c);let orphan=object(&mut h,&roots,ClassObjectKindV1::Value,2,&mut c);let x=numeric(&mut c,&[1.,2.]);put(&mut h,child.id,0,x,&mut c);let y=numeric(&mut c,&[8.,9.,10.]);put(&mut h,orphan.id,0,y,&mut c);put(&mut h,root.id,0,ClassValueV1::Object(child.id),&mut c);let child_id=child.id;let orphan_id=orphan.id;drop(child);drop(orphan);c.charges=0;(h,roots,c,root,child_id,orphan_id)
}
fn protected(h:&HeapStorage,root:ClassObjectIdV1,child:ClassObjectIdV1){assert!(h.is_valid(root));assert!(h.is_valid(child));assert_eq!(h.property(root,0).unwrap().object().unwrap(),child);assert_eq!(h.property(child,0).unwrap().scientific().unwrap().dense_component(1).unwrap().re,2.);}

#[test]
fn private_lease_and_transitive_child_survive_gc_orphan_reclaimed(){
 let(mut h,roots,mut c,root,child,orphan)=fixture();assert_eq!(drive(h.collect(&[],&roots,&mut c)).unwrap(),1);protected(&h,root.id,child);assert!(matches!(h.record(orphan),Err(ClassErrorV1::InvalidHandle)));accounting(&h,&c);assert_eq!(h.live_objects(),2);drop(root);assert_eq!(drive(h.collect(&[],&roots,&mut c)).unwrap(),2);accounting(&h,&c);assert_eq!(h.payload_bytes,0);
}

#[test]
fn every_gc_checkpoint_stop_keeps_protected_graph_and_exact_partial_charges_then_recovers(){
 let(mut h,roots,mut c,root,_,_)=fixture();drive(h.collect(&[root.id],&roots,&mut c)).unwrap();let checkpoints=c.charges;assert!(checkpoints>8);
 for at in 1..=checkpoints{let(mut h,roots,mut c,root,child,_)=fixture();c.stop_charge=Some(at);let result=drive(h.collect(&[root.id],&roots,&mut c));assert!(matches!(result,Err(ClassErrorV1::Cancelled)),"GC checkpoint{at}");protected(&h,root.id,child);accounting(&h,&c);c.stop_charge=None;drive(h.collect(&[root.id],&roots,&mut c)).unwrap();protected(&h,root.id,child);accounting(&h,&c);assert_eq!(h.live_objects(),2);}
}

#[test]
fn every_gc_pending_poll_drop_retains_exact_partial_state_and_fresh_collection_recovers(){
 let(mut h,roots,mut c,root,_,_)=fixture();let mut pending=0;{let root_ids=[root.id];let f=h.collect(&root_ids,&roots,&mut c);let mut f=std::pin::pin!(f);let mut cx=Context::from_waker(Waker::noop());loop{match f.as_mut().poll(&mut cx){Poll::Pending=>{pending+=1;assert!(pending<128)},Poll::Ready(r)=>{r.unwrap();break}}}}
 for at in 0..=pending{let(mut h,roots,mut c,root,child,_)=fixture();{let root_ids=[root.id];let f=h.collect(&root_ids,&roots,&mut c);let mut f=std::pin::pin!(f);let mut cx=Context::from_waker(Waker::noop());for _ in 0..at{assert!(f.as_mut().poll(&mut cx).is_pending());}}protected(&h,root.id,child);accounting(&h,&c);drive(h.collect(&[],&roots,&mut c)).unwrap();protected(&h,root.id,child);accounting(&h,&c);assert_eq!(h.live_objects(),2);}
}

#[test]
fn owner_epoch_kind_generation_and_reclaimed_slot_tokens_rejected(){
 let roots=Arc::new(PrivateRoots::new());let mut c=Control::default();let mut h=HeapStorage::new(11,12,&mut c).unwrap();let root=object(&mut h,&roots,ClassObjectKindV1::Handle,1,&mut c);let token=root.id;
 for bad in [ClassObjectIdV1{owner:10,..token},ClassObjectIdV1{epoch:11,..token},ClassObjectIdV1{kind:ClassObjectKindV1::Value,..token},ClassObjectIdV1{generation:2,..token}]{assert!(matches!(h.record(bad),Err(ClassErrorV1::InvalidHandle)));}
 drop(root);drive(h.collect(&[],&roots,&mut c)).unwrap();let new=object(&mut h,&roots,ClassObjectKindV1::Handle,1,&mut c);assert_eq!(new.id.slot(),token.slot());assert!(new.id.generation()>token.generation());assert!(matches!(h.record(token),Err(ClassErrorV1::InvalidHandle)));accounting(&h,&c);
}

#[test]
fn recursive_publication_and_property_cycle_refusal_are_prepared_atomically(){
 let roots=Arc::new(PrivateRoots::new());let mut c=Control::default();let mut h=HeapStorage::new(1,1,&mut c).unwrap();let parent=object(&mut h,&roots,ClassObjectKindV1::Value,1,&mut c);let child=object(&mut h,&roots,ClassObjectKindV1::Handle,1,&mut c);put(&mut h,parent.id,0,ClassValueV1::Object(child.id),&mut c);let plan=drive(h.prepare_publication(&ClassValueV1::Object(parent.id),None,true,&mut c)).unwrap();assert!(!h.is_published(child.id).unwrap());h.validate_publication(&plan).unwrap();h.commit_publication(plan);assert!(h.is_published(child.id).unwrap());assert!(!h.is_published(parent.id).unwrap());let epoch=h.mutation_epoch;assert!(matches!(drive(h.prepare_property(child.id,0,&ClassValueV1::Object(parent.id),&mut c)),Err(ClassErrorV1::CyclicReference)));assert_eq!(h.mutation_epoch,epoch);assert!(matches!(h.property(child.id,0),Err(ClassErrorV1::UnknownProperty)));accounting(&h,&c);
}

#[test]
fn stale_prepared_property_publication_and_delete_reject_and_poison_before_commit(){
 for operation in 0..3{let roots=Arc::new(PrivateRoots::new());let mut c=Control::default();let mut h=HeapStorage::new(1,1,&mut c).unwrap();let root=object(&mut h,&roots,ClassObjectKindV1::Handle,1,&mut c);let value=ClassValueV1::Object(root.id);
  if operation==0{let candidate=numeric(&mut c,&[1.]);let bytes=candidate.direct_payload_bytes();let plan=drive(h.prepare_property(root.id,0,&candidate,&mut c)).unwrap();let bump=h.empty_publication().unwrap();h.commit_publication(bump);assert_eq!(h.validate_property(root.id,0,&plan),Err(ClassErrorV1::Resource));drop(candidate);c.release_payload(bytes).unwrap();}
  else{let plan=if operation==1{drive(h.prepare_publication(&value,None,true,&mut c)).unwrap()}else{h.prepare_delete(root.id).unwrap()};let bump=h.empty_publication().unwrap();h.commit_publication(bump);assert_eq!(h.validate_publication(&plan),Err(ClassErrorV1::Resource));}
  assert!(h.poisoned.get());assert!(h.is_valid(root.id));assert!(matches!(h.property(root.id,0),Err(ClassErrorV1::UnknownProperty)));
 }
}

#[test]
fn deletion_retains_reachable_tombstone_releases_payload_and_does_not_follow_dead_edges(){
 let(mut h,roots,mut c,root,child,_)=fixture();let plan=h.prepare_delete(root.id).unwrap();h.validate_publication(&plan).unwrap();h.commit_delete(root.id,plan);assert!(!h.is_valid(root.id));assert!(matches!(h.property(root.id,0),Err(ClassErrorV1::InvalidHandle)));drive(h.collect(&[root.id],&roots,&mut c)).unwrap();assert!(h.record(root.id).is_ok());assert!(matches!(h.record(child),Err(ClassErrorV1::InvalidHandle)));assert_eq!(h.live_objects(),1);assert_eq!(h.payload_bytes,0);accounting(&h,&c);assert_eq!(h.prepare_delete(root.id).unwrap_err(),ClassErrorV1::InvalidHandle);drop(root);assert_eq!(drive(h.collect(&[],&roots,&mut c)).unwrap(),1);accounting(&h,&c);
}

#[test]
fn release_failures_and_corrupt_cached_payload_poison_persistently(){
 for payload_failure in [false,true]{let(mut h,roots,mut c,root,_,_)=fixture();if payload_failure{c.fail_payload_release=true}else{c.fail_metadata_release=true}assert!(matches!(drive(h.collect(&[],&roots,&mut c)),Err(ClassErrorV1::Resource)));assert!(h.poisoned.get());assert!(c.poisoned);drop(root);}
 let roots=Arc::new(PrivateRoots::new());let mut c=Control::default();let mut h=HeapStorage::new(1,1,&mut c).unwrap();let root=object(&mut h,&roots,ClassObjectKindV1::Value,1,&mut c);let x=numeric(&mut c,&[4.]);put(&mut h,root.id,0,x,&mut c);h.payload_bytes=0;drop(root);assert!(matches!(drive(h.collect(&[],&roots,&mut c)),Err(ClassErrorV1::Resource)));assert!(h.poisoned.get());assert!(c.poisoned);
}

#[test]
fn lease_counter_and_mutation_generation_exhaustion_never_wrap(){
 let roots=Arc::new(PrivateRoots::new());let mut c=Control::default();let mut h=HeapStorage::new(1,1,&mut c).unwrap();let root=object(&mut h,&roots,ClassObjectKindV1::Handle,0,&mut c);roots.counts.borrow_mut()[root.id.slot()]=u16::MAX;assert!(matches!(roots.hold(root.id),Err(ClassErrorV1::Resource)));roots.counts.borrow_mut()[root.id.slot()]=1;h.mutation_epoch=u64::MAX;assert!(matches!(h.empty_publication(),Err(ClassErrorV1::CounterExhausted)));assert!(h.poisoned.get());drop(root);
 let roots=Arc::new(PrivateRoots::new());let mut c=Control::default();let mut h=HeapStorage::new(1,1,&mut c).unwrap();let root=object(&mut h,&roots,ClassObjectKindV1::Value,0,&mut c);let slot=root.id.slot();drop(root);drive(h.collect(&[],&roots,&mut c)).unwrap();h.slots[slot].generation=u64::MAX;assert!(matches!(drive(h.allocate(&roots,0,ClassObjectKindV1::Value,0,&mut c)),Err(ClassErrorV1::CounterExhausted)));assert_eq!(h.live_objects(),0);accounting(&h,&c);
}

#[test]
fn allocation_scan_and_fill_each_checkpoint_stop_or_drop_keeps_prior_roots_and_recovers(){
 for stop in [false,true]{for at in 1..=2{let(mut h,roots,mut c,root,child,_)=fixture();if stop{c.stop_charge=Some(at);assert!(matches!(drive(h.allocate(&roots,0,ClassObjectKindV1::Value,2,&mut c)),Err(ClassErrorV1::Cancelled)));c.stop_charge=None;}else{let f=h.allocate(&roots,0,ClassObjectKindV1::Value,2,&mut c);let mut f=std::pin::pin!(f);let mut cx=Context::from_waker(Waker::noop());for _ in 0..at{assert!(f.as_mut().poll(&mut cx).is_pending());}}
  protected(&h,root.id,child);accounting(&h,&c);drive(h.collect(&[],&roots,&mut c)).unwrap();protected(&h,root.id,child);accounting(&h,&c);let next=object(&mut h,&roots,ClassObjectKindV1::Value,2,&mut c);assert!(h.is_valid(next.id));accounting(&h,&c);
 }}
}

#[test]
fn corrupt_cached_metadata_underflow_is_terminal_poison_not_uncharged_success(){
 let roots=Arc::new(PrivateRoots::new());let mut c=Control::default();let mut h=HeapStorage::new(1,1,&mut c).unwrap();let root=object(&mut h,&roots,ClassObjectKindV1::Value,1,&mut c);drop(root);h.metadata_bytes=0;assert!(matches!(drive(h.collect(&[],&roots,&mut c)),Err(ClassErrorV1::Resource)));assert!(h.poisoned.get());assert!(c.poisoned);assert_eq!(h.metadata_bytes,0);
 // Do not resume a poisoned heap: the owning session must reject further work.
 // Its fail-closed gate is tested by the VM unit owner, not invented here.
}
