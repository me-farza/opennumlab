// SPDX-License-Identifier: GPL-3.0-or-later
//! Public-API class semantics; the canonical original reference files are inputs,
//! not implementation-derived expectations. Fault injection remains unit-only.
use mathlab_engine::rust_program::{class::*,ProgramStatusV1,ProgramCheckpointV1,ProgramCancelReasonV1,ProgramCancelResultV1};
use mathlab_language::program::{ClassSourceV1,ClassProgramGraphV1,ClassParseErrorCodeV1,parse_class_graph_controlled_v1,ScientificCscValuesV1};
use std::{sync::Arc,future::Future,time::Instant};
type Session=ClassProgramSessionV1;
type Outcome=ClassProgramOutcomeV1;
fn sources(entry:&str,modules:&[&str])->Vec<ClassSourceV1>{std::iter::once(entry).chain(modules.iter().copied()).enumerate().map(|(i,text)|ClassSourceV1{source_id:(i+1)as u64,text:Arc::from(text)}).collect()}
fn graph(entry:&str,modules:&[&str])->Arc<ClassProgramGraphV1>{Arc::new(parse_class_graph_controlled_v1(sources(entry,modules),||false).unwrap())}
fn run(s:&mut Session,entry:&str,modules:&[&str])->Outcome{s.prepare(graph(entry,modules),Default::default()).unwrap().execute()}
fn ok(o:&Outcome){assert_eq!(o.status,ProgramStatusV1::Success,"{:?}",o.error);assert!(o.error.is_none());assert!(!o.stats.ledger_poisoned);assert!(o.stats.peak_live_payload_bytes<=64<<20);assert!(o.stats.peak_metadata_bytes<=16<<20);assert!(o.stats.heap_payload_bytes<=64<<20);assert!(o.stats.heap_metadata_bytes<=16<<20);}
fn real(s:&Session,name:&str,expected:&[f64]){let v=s.binding(name).unwrap().scientific().unwrap();let d=v.dense_view().unwrap();assert!(!d.is_complex());assert_eq!(d.len(),expected.len());for(i,w)in expected.iter().enumerate(){assert_eq!(d.get(i).unwrap().re,*w,"{name}[{i}]");}}
fn id(s:&Session,name:&str)->ClassObjectIdV1{s.binding(name).unwrap().object().unwrap()}
fn valid(s:&Session,name:&str)->bool{match s.view(s.binding(name).unwrap()).unwrap(){ClassValueViewV1::Object{metadata,..}=>metadata.is_valid(),_=>panic!("object expected")}}
const J01:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J01/J01.m");
const JV01:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J01/Jv01.m");
const J02:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J02/J02.m");
const JH02:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J02/Jh02.m");
const J03:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J03/J03.m");
const JG03:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J03/Jg03.m");
const JW03:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J03/Jw03.m");
const J04:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J04/J04.m");
const JH04:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J04/Jh04.m");
const J05:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J05/J05.m");
const JV05:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J05/Jv05.m");
const J06:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J06/J06.m");
const JB06:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J06/Jb06.m");
const JD06:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J06/Jd06.m");
const K01:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K01/K01.m");
const KC01:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K01/Kc01.m");
const K02:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K02/K02.m");
const KM02:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K02/Km02.m");
const K03:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K03/K03.m");
const KS03:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K03/Ks03.m");
const K04:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K04/K04.m");
const KG04:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K04/Kg04.m");
const KW04:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K04/Kw04.m");

#[test]
fn all_six_original_native_foundation_graphs_and_invalid_handle_prefix(){
 for(entry,modules,name,expected)in [(J01,vec![JV01],"pf_after",vec![4.,9.]),(J02,vec![JH02],"pf_after",vec![9.,9.]),(J03,vec![JG03,JW03],"pf_after",vec![9.,9.,9.]),(J05,vec![JV05],"pf_after",vec![4.,12.,9.]),(J06,vec![JB06,JD06],"pf_dispatch",vec![4.,8.])]{let mut s=Session::new().unwrap();let o=run(&mut s,entry,&modules);ok(&o);real(&s,name,&expected);real(&s,"pf_done",&[1.]);if entry==J01{assert_ne!(id(&s,"a"),id(&s,"b"));}if entry==J02{assert_eq!(id(&s,"a"),id(&s,"b"));}}
 let mut s=Session::new().unwrap();let o=run(&mut s,J04,&[JH04]);assert_eq!(o.status,ProgramStatusV1::Error);let e=o.error.unwrap();assert_eq!(e.code,ClassProgramErrorCodeV1::Value(ClassErrorV1::InvalidHandle));assert_eq!(e.location.source_id,1);assert_eq!(e.location.span.start_line,7);real(&s,"pf_before_valid",&[1.,1.]);real(&s,"pf_after_valid",&[0.,0.]);real(&s,"pf_mutation_reached",&[1.]);real(&s,"pf_done",&[0.]);assert!(s.binding("pf_mutation_completed").is_none());assert_eq!(id(&s,"a"),id(&s,"b"));assert!(!valid(&s,"a"));assert_eq!(o.stats.handle_deletions,1);
}

#[test]
fn native_constructor_method_visible_counts_and_numeric_class_shadow(){
 for(entry,modules)in [(K01,vec![KC01]),(K02,vec![KM02])]{let mut s=Session::new().unwrap();ok(&run(&mut s,entry,&modules));if entry==K01{real(&s,"pf_counts",&[0.,1.,1.,1.]);real(&s,"pf_bare",&[1.,0.,11.]);}else{real(&s,"pf_two",&[4.,9.,2.,2.]);real(&s,"pf_one",&[4.,12.,2.,1.]);real(&s,"pf_bare",&[17.,2.,0.]);}}
 let mut s=Session::new().unwrap();ok(&run(&mut s,"a=Ks03(3);pf_constructed=a.Reading;pf_reading=localShadowNumericK03();\nfunction value=localShadowNumericK03()\nKs03=[7 8];value=Ks03(2);\nend\n",&[KS03]));real(&s,"pf_constructed",&[3.]);real(&s,"pf_reading",&[8.]);
 // The exact original asks for char class(selected), outside this value API.
 let mut original=Session::new().unwrap();let o=run(&mut original,K03,&[KS03]);assert_eq!(o.status,ProgramStatusV1::Error);assert_eq!(o.error.unwrap().code,ClassProgramErrorCodeV1::UnsupportedFeature);
 // Persistent instrumentation is not a supported derivative of default sharing.
 let e=parse_class_graph_controlled_v1(sources(K04,&[KG04,KW04]),||false).unwrap_err();assert_eq!(e.code,ClassParseErrorCodeV1::Unsupported);assert_eq!(e.source_id,2);
}

const GAUGE:&str="classdef ApiGauge < handle\nproperties\nReading=4\nend\nend\n";
const WRAPPER:&str="classdef ApiWrapper\nproperties\nGauge=ApiGauge\nBuilt\nend\nmethods\nfunction obj=ApiWrapper()\nobj.Built=ApiGauge;\nend\nend\nend\n";
#[test]
fn handle_valued_defaults_shared_but_constructor_handles_distinct(){
 let mut s=Session::new().unwrap();ok(&run(&mut s,"a=ApiWrapper;b=ApiWrapper;a.Gauge.Reading=9;a.Built.Reading=12;pf=[a.Gauge.Reading b.Gauge.Reading a.Built.Reading b.Built.Reading];g=a.Gauge;h=b.Gauge;u=a.Built;v=b.Built;",&[GAUGE,WRAPPER]));real(&s,"pf",&[9.,9.,12.,4.]);assert_ne!(id(&s,"a"),id(&s,"b"));assert_eq!(id(&s,"g"),id(&s,"h"));assert_ne!(id(&s,"u"),id(&s,"v"));
 // Once-per-class-session defaults remain the same across later entry requests.
 ok(&run(&mut s,"c=ApiWrapper;later=c.Gauge.Reading;fresh=c.Built.Reading;",&[GAUGE,WRAPPER]));real(&s,"later",&[9.]);real(&s,"fresh",&[4.]);
}

#[test]
fn nested_value_copy_isolates_dense_csc_and_retains_shared_handle(){
 let box_class="classdef ApiBox\nproperties\nDense\nSparse\nGauge\nend\nmethods\nfunction obj=ApiBox(h)\nobj.Dense=[1 2];obj.Sparse=sparse([1 2],[1 2],[2 3],2,2);obj.Gauge=h;\nend\nfunction obj=changed(obj)\nobj.Dense=[9 8];obj.Sparse=sparse(1,1,7,2,2);obj.Gauge.Reading=12;\nend\nend\nend\n";
 let outer="classdef ApiOuter\nproperties\nInner\nend\nmethods\nfunction obj=ApiOuter(h)\nobj.Inner=ApiBox(h);\nend\nend\nend\n";
 let mut s=Session::new().unwrap();ok(&run(&mut s,"h=ApiGauge;a=ApiOuter(h);b=a;v=b.Inner;v=v.changed();b.Inner=v;old=a.Inner;new=b.Inner;da=old.Dense;db=new.Dense;A=old.Sparse;B=new.Sparse;Y=A*[1;1];readings=[old.Gauge.Reading new.Gauge.Reading h.Reading];",&[GAUGE,box_class,outer]));real(&s,"da",&[1.,2.]);real(&s,"db",&[9.,8.]);real(&s,"Y",&[2.,3.]);real(&s,"readings",&[12.,12.,12.]);assert_ne!(id(&s,"a"),id(&s,"b"));assert_ne!(id(&s,"old"),id(&s,"new"));let a=s.binding("A").unwrap().scientific().unwrap().csc_view().unwrap();assert_eq!(a.column_pointers(),[0,1,2]);assert_eq!(a.row_indices(),[0,1]);match a.values(){ScientificCscValuesV1::Real(v)=>assert_eq!(v,[2.,3.]),_=>panic!("real CSC expected")};assert_eq!(s.binding("B").unwrap().scientific().unwrap().stored_len(),1);
}

#[test]
fn cross_session_same_slot_tokens_reset_epoch_and_schema_refusal(){
 let mut a=Session::new().unwrap();let mut b=Session::new().unwrap();ok(&run(&mut a,"h=ApiGauge;",&[GAUGE]));ok(&run(&mut b,"h=ApiGauge;",&[GAUGE]));let ta=id(&a,"h");let tb=id(&b,"h");assert_eq!(ta.slot(),tb.slot());assert_eq!(ta.generation(),tb.generation());assert_ne!(ta,tb);let foreign=ClassValueV1::Object(ta);assert!(matches!(b.view(&foreign),Err(ClassErrorV1::InvalidHandle)));
 let before=a.context();let changed=GAUGE.replace("Reading=4","Reading=8");let o=run(&mut a,"unreached=1;",&[&changed]);assert_eq!(o.status,ProgramStatusV1::Error);assert_eq!(o.error.unwrap().code,ClassProgramErrorCodeV1::UnsupportedFeature);assert_eq!(a.context().revision,before.revision);assert!(a.binding("unreached").is_none());assert_eq!(id(&a,"h"),ta);
 a.reset().unwrap();assert!(matches!(a.view(&foreign),Err(ClassErrorV1::InvalidHandle)));assert!(a.context().epoch>before.epoch);ok(&run(&mut a,"h=ApiGauge;pf=h.Reading;",&[&changed]));real(&a,"pf",&[8.]);assert_ne!(id(&a,"h"),ta);
 let mut s=Session::new().unwrap();ok(&run(&mut s,"a=ApiWrapper;",&[GAUGE,WRAPPER]));let before=s.context();let o=run(&mut s,"unreached=1;",&[WRAPPER,GAUGE]);assert_eq!(o.status,ProgramStatusV1::Error);assert_eq!(o.error.unwrap().code,ClassProgramErrorCodeV1::UnsupportedFeature);assert_eq!(s.context().revision,before.revision);assert!(s.binding("unreached").is_none());
}

#[test]
fn immutable_object_snapshots_survive_delete_reset_and_session_drop(){
 let mut s=Session::new().unwrap();let o=run(&mut s,"h=ApiGauge;alias=h;h\ndelete(h);v=isvalid(alias);",&[GAUGE]);ok(&o);let token=id(&s,"h");real(&s,"v",&[0.]);assert!(!valid(&s,"h"));let snapshot=o.events.iter().find_map(|e|match &e.value{ClassEventValueV1::Object(v)=>Some(v.clone()),_=>None}).unwrap();assert_eq!(snapshot.slot(),token.slot());assert_eq!(snapshot.generation(),token.generation());s.reset().unwrap();drop(s);drop(o);assert_eq!(snapshot.class_name(),"ApiGauge");assert_eq!(snapshot.kind(),ClassObjectKindV1::Handle);assert!(snapshot.is_valid());
}

#[test]
fn published_handle_effects_survive_failed_constructors_and_methods(){
 for delete in [false,true]{
  let action=if delete{"delete(h);"}else{"h.Reading=9;"};let failing=format!("classdef ApiFailed\nproperties\nMarker=4\nend\nmethods\nfunction obj=ApiFailed(h)\nobj.Marker=8;\n{action}\nobj.Missing=1;\nend\nend\nend\n");let mut s=Session::new().unwrap();let o=run(&mut s,"h=ApiGauge;before=h.Reading;result=ApiFailed(h);after=1;",&[GAUGE,&failing]);assert_eq!(o.status,ProgramStatusV1::Error);let e=o.error.unwrap();assert_eq!(e.code,ClassProgramErrorCodeV1::Value(ClassErrorV1::UnknownProperty));assert_eq!(e.location.source_id,3);assert_eq!(e.location.span.start_line,9);assert!(s.binding("result").is_none());assert!(s.binding("after").is_none());real(&s,"before",&[4.]);assert_eq!(o.final_context.revision,3);assert_eq!(o.committed_effect_sequence,3);
  if delete{assert!(!valid(&s,"h"));assert_eq!(o.stats.handle_deletions,1);}else{assert!(valid(&s,"h"));assert_eq!(o.stats.handle_mutations,1);ok(&run(&mut s,"observed=h.Reading;",&[GAUGE,&failing]));real(&s,"observed",&[9.]);}
 }
 let method="classdef ApiMethod\nproperties\nMarker=4\nend\nmethods\nfunction obj=fail(obj,h)\nobj.Marker=8;h.Reading=12;obj.Missing=1;\nend\nend\nend\n";let mut s=Session::new().unwrap();let o=run(&mut s,"h=ApiGauge;a=ApiMethod;result=a.fail(h);after=1;",&[GAUGE,method]);assert_eq!(o.status,ProgramStatusV1::Error);assert_eq!(o.error.unwrap().code,ClassProgramErrorCodeV1::Value(ClassErrorV1::UnknownProperty));assert!(s.binding("result").is_none());assert!(s.binding("after").is_none());ok(&run(&mut s,"original=a.Marker;shared=h.Reading;",&[GAUGE,method]));real(&s,"original",&[4.]);real(&s,"shared",&[12.]);
}

#[test]
fn published_cycles_refused_without_changing_handle_property(){
 let node="classdef ApiNode < handle\nproperties\nNext=0\nend\nend\n";let mut s=Session::new().unwrap();let o=run(&mut s,"h=ApiNode;h.Next=h;unreached=1;",&[node]);assert_eq!(o.status,ProgramStatusV1::Error);assert_eq!(o.error.unwrap().code,ClassProgramErrorCodeV1::Value(ClassErrorV1::CyclicReference));assert!(s.binding("unreached").is_none());ok(&run(&mut s,"still=h.Next;",&[node]));real(&s,"still",&[0.]);
}

#[test]
fn handle_publication_stop_barriers_and_fuel_deadline_recovery(){
 for (point,changed)in [(ProgramCheckpointV1::BeforePublish,false),(ProgramCheckpointV1::AfterBarrier,true),(ProgramCheckpointV1::AfterPublish,true)]{let mut s=Session::new().unwrap();ok(&run(&mut s,"h=ApiGauge;",&[GAUGE]));let mut fired=false;let o=s.prepare(graph("h.Reading=9;after=1;",&[GAUGE]),Default::default()).unwrap().execute_controlled(|at,h|{if at==point&&!fired{fired=true;h.request_stop();}});assert!(fired);assert_eq!(o.status,ProgramStatusV1::Cancelled(ProgramCancelReasonV1::UserStop));assert!(s.binding("after").is_none());ok(&run(&mut s,"observed=h.Reading;",&[GAUGE]));real(&s,"observed",&[if changed{9.}else{4.}]);}
 for options in [ClassProgramOptionsV1{transition_fuel:1,..Default::default()},ClassProgramOptionsV1{primitive_fuel:1,..Default::default()},ClassProgramOptionsV1{deadline:Some(Instant::now()),..Default::default()}]{let mut s=Session::new().unwrap();ok(&run(&mut s,"seed=17;",&[GAUGE]));let o=s.prepare(graph("h=ApiGauge;h.Reading=9;last=1;",&[GAUGE]),options).unwrap().execute();assert!(matches!(o.status,ProgramStatusV1::Resource|ProgramStatusV1::Cancelled(ProgramCancelReasonV1::Deadline)));assert!(s.binding("last").is_none());real(&s,"seed",&[17.]);ok(&run(&mut s,"recovered=1;",&[GAUGE]));}
}

const PHASES:&str="h=ApiGauge;v=ApiWrapper;h.Reading=9;v.Gauge.Reading=12;last=1;";
// Each ordinary helper owns exactly one real API future; the test orchestration
// frame therefore holds no Future slots during synchronous seed/recovery calls.
#[inline(never)]
fn drive_tiny_class_baseline(session:&mut Session,module:Arc<ClassProgramGraphV1>)->usize{
 let request=session.prepare(module,Default::default()).unwrap();let mut future=std::pin::pin!(request.execute_cooperatively(|_,_|{}));let mut cx=std::task::Context::from_waker(std::task::Waker::noop());let mut polls=0;
 loop{polls+=1;assert!(polls<=4096,"bounded tiny-program poll catalog");if let std::task::Poll::Ready(outcome)=future.as_mut().poll(&mut cx){ok(&outcome);return polls}}
}
#[inline(never)]
fn stop_or_drop_tiny_class_after(session:&mut Session,module:Arc<ClassProgramGraphV1>,at:usize,stop:bool)->mathlab_engine::rust_program::ProgramCancelHandleV1{
 let request=session.prepare(module,Default::default()).unwrap();let handle=request.cancel_handle();let mut future=std::pin::pin!(request.execute_cooperatively(|_,_|{}));let mut cx=std::task::Context::from_waker(std::task::Waker::noop());for _ in 0..at{assert!(future.as_mut().poll(&mut cx).is_pending());}
 if stop{assert!(matches!(handle.request_stop(),ProgramCancelResultV1::Accepted(_)));let mut remaining=0;loop{remaining+=1;assert!(remaining<=4096);if let std::task::Poll::Ready(outcome)=future.as_mut().poll(&mut cx){assert_eq!(outcome.status,ProgramStatusV1::Cancelled(ProgramCancelReasonV1::UserStop));assert!(!outcome.stats.ledger_poisoned);break}}}handle
}
#[test]
fn each_tiny_class_continuation_stop_drop_preserves_roots_and_recovers(){
 let module=graph(PHASES,&[GAUGE,WRAPPER]);let mut base=Session::new().unwrap();ok(&run(&mut base,"seed=17;",&[GAUGE,WRAPPER]));let polls=drive_tiny_class_baseline(&mut base,module.clone());assert!(polls>1);
 for at in 0..polls{for stop in [false,true]{let mut session=Session::new().unwrap();ok(&run(&mut session,"seed=17;",&[GAUGE,WRAPPER]));let handle=stop_or_drop_tiny_class_after(&mut session,module.clone(),at,stop);
  assert_eq!(handle.request_stop(),if stop{ProgramCancelResultV1::AlreadyAccepted(ProgramCancelReasonV1::UserStop)}else{ProgramCancelResultV1::TooLate});real(&session,"seed",&[17.]);if session.binding("h").is_some(){assert!(valid(&session,"h"));}if session.binding("v").is_some(){assert!(valid(&session,"v"));}ok(&run(&mut session,"recovered=1;",&[GAUGE,WRAPPER]));real(&session,"recovered",&[1.]);
 }}println!("CLASS_PUBLIC_DROP_FIXTURE all_stop_drop_prefix_recovery_passed boundaries={polls}");
}

#[test]
fn abandoned_request_stale_cancel_and_object_gc_replacement_recovery(){
 let mut s=Session::new().unwrap();let before=s.context();let handle;{let request=s.prepare(graph("unreached=1;",&[GAUGE]),Default::default()).unwrap();handle=request.cancel_handle();}assert_eq!(handle.request_stop(),ProgramCancelResultV1::TooLate);assert_eq!(s.context().revision,before.revision);assert!(s.binding("unreached").is_none());
 // More than the declared1024 object slots; completed overwrites must permit GC.
 ok(&run(&mut s,"garbage=ApiGauge;",&[GAUGE]));let old_garbage=ClassValueV1::Object(id(&s,"garbage"));let o=run(&mut s,"survivor=ApiGauge;alias=survivor;for k=1:1200;garbage=ApiGauge;end;survivor.Reading=9;observed=alias.Reading;",&[GAUGE]);assert!(matches!(s.view(&old_garbage),Err(ClassErrorV1::InvalidHandle)));ok(&o);real(&s,"observed",&[9.]);assert_eq!(id(&s,"survivor"),id(&s,"alias"));assert!(o.stats.heap_reclaimed_objects>0);assert!(o.stats.heap_object_slots<=1024);assert!(o.stats.heap_live_objects<=1024);ok(&run(&mut s,"recovered=1;",&[GAUGE]));
}


#[test]
fn renamed_value_handle_and_inheritance_graphs_have_same_observable_semantics(){
 let rename=|s:&str|s.replace("Jv01","RenamedValue").replace("Jh02","RenamedHandle").replace("Jb06","RenamedBase").replace("Jd06","RenamedChild");
 for(entry,modules,name,expected)in [(J01,vec![JV01],"pf_after",vec![4.,9.]),(J02,vec![JH02],"pf_after",vec![9.,9.]),(J06,vec![JB06,JD06],"pf_dispatch",vec![4.,8.])]{let text=rename(entry);let owned:Vec<String>=modules.iter().map(|m|rename(m)).collect();let refs:Vec<&str>=owned.iter().map(String::as_str).collect();let mut s=Session::new().unwrap();ok(&run(&mut s,&text,&refs));real(&s,name,&expected);}
}

#[test]
fn class_tall_sc06_multiple_rhs_and_square_lu_keep_shapes_and_completed_prefix(){
 let mut s=Session::new().unwrap();let o=run(&mut s,"A=[1,0;0,1;1,1];B=[1,2;2,4;4,8];X=A\\B;R=A*X-B;",&[]);ok(&o);let x=s.binding("X").unwrap().scientific().unwrap().dense_view().unwrap();assert_eq!(x.shape().axes(),[2,2]);for(i,v)in [4./3.,7./3.,8./3.,14./3.].into_iter().enumerate(){assert!((x.get(i).unwrap().re-v).abs()<1e-12);}
 let o=run(&mut s,"square=[4,1;1,3]\\[1;2];",&[]);ok(&o);let x=s.binding("square").unwrap().scientific().unwrap();assert!((x.dense_component(0).unwrap().re-1./11.).abs()<1e-12);assert!((x.dense_component(1).unwrap().re-7./11.).abs()<1e-12);
 let o=run(&mut s,"before_rank=17;bad=[1,2;2,4;3,6]\\[1;2;3];unreached=1;",&[]);assert_eq!(o.status,ProgramStatusV1::Error);assert!(s.binding("bad").is_none());assert!(s.binding("unreached").is_none());real(&s,"before_rank",&[17.]);ok(&run(&mut s,"recovered=31;",&[]));real(&s,"recovered",&[31.]);
}
#[test]
fn class_tall_complex_two_rhs_uses_conjugation_and_never_relabels_wide_as_minimum_norm(){
 let mut s=Session::new().unwrap();let o=run(&mut s,"A=complex([1,3;0,1;1,0;2,-1],[1,-1;0,2;-1,1;1,1]);B=complex([5,-1.25;4,0;3,0.75;0,-4.75],[-3,1.25;3,1.25;2,3.5;6,3.25]);X=A\\B;",&[]);ok(&o);let x=s.binding("X").unwrap().scientific().unwrap();for(i,(re,im))in [(1.,1.),(2.,-1.),(-1.,2.),(0.5,0.25)].into_iter().enumerate(){let z=x.dense_component(i).unwrap();assert!((z.re-re).abs()<1e-12&&(z.im-im).abs()<1e-12);}
 let o=run(&mut s,"wide=[1,1]\\2;",&[]);assert_eq!(o.status,ProgramStatusV1::Error);assert!(s.binding("wide").is_none());
}
