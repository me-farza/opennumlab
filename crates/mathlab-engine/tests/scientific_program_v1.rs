// SPDX-License-Identifier: GPL-3.0-or-later
use mathlab_engine::rust_program::{scientific::*,ProgramStatusV1,ProgramCheckpointV1,ProgramCancelReasonV1};
use mathlab_language::program::*;
use std::sync::Arc;
fn module(source:&str)->Arc<ProgramModuleV1>{Arc::new(parse_program_v1(901,Arc::from(source)).unwrap())}
fn run(session:&mut ScientificProgramSessionV1,source:&str)->ScientificProgramOutcomeV1{session.prepare(module(source),Default::default()).unwrap().execute()}
#[test]
fn scientific_nd_complex_functions_atomic_pages_and_borrowed_full_range(){
    let mut s=ScientificProgramSessionV1::new().unwrap();let out=run(&mut s,"A=complex(zeros([2 2 3]),zeros([2 2 3]));A(:,:,2)=f();saved=A;A(1,1,2)=9;z=A(end,end,2);r=real(saved);im=imag(saved);\nfunction y=f\ny=complex([1 2;3 4],[5 6;7 8]);end");assert_eq!(out.status,ProgramStatusV1::Success,"{:?}",out.error);
    let saved=s.binding("saved").unwrap();assert_eq!(saved.shape(),[2,2,3]);assert!(saved.is_complex());assert_eq!(saved.component(4).unwrap(),Complex64V1{re:1.,im:5.});assert_eq!(s.binding("A").unwrap().component(4).unwrap(),Complex64V1::real(9.));
    match saved.read_range(0,12).unwrap(){ScientificReadRangeV1::Complex(words)=>assert_eq!(words.len(),12),_=>panic!()};assert!(s.binding("y").is_none());assert_eq!(s.binding("z").unwrap().component(0).unwrap(),Complex64V1{re:4.,im:8.});
}
#[test]
fn scientific_small_complex_solve_transpose_and_residual(){
    let mut s=ScientificProgramSessionV1::new().unwrap();let out=run(&mut s,"A=complex([2 0;0 3],[1 0;0 -1]);B=complex([1;5],[3;-5]);X=A\\B;R=norm_inf(B-A*X);plain=A.';adjoint=A';");assert_eq!(out.status,ProgramStatusV1::Success,"{:?}",out.error);
    let x=s.binding("X").unwrap();assert!((x.component(0).unwrap().re-1.).abs()<2f64.powi(-48));assert!((x.component(1).unwrap().im+1.).abs()<2f64.powi(-48));assert!(s.binding("R").unwrap().scalar().unwrap()<2f64.powi(-45));assert_eq!(s.binding("plain").unwrap().component(0).unwrap().im,1.);assert_eq!(s.binding("adjoint").unwrap().component(0).unwrap().im,-1.);assert_eq!(out.stats.solves_completed,1);assert!(out.stats.last_solve.is_some());
}
#[test]
fn scientific_stop_fuel_error_and_recovery_keep_prefix(){
    let src="before=7;A=eye(8);B=ones(8,4);X=A\\B;after=1;";
    let mut s=ScientificProgramSessionV1::new().unwrap();let mut fired=false;
    let out=s.prepare(module(src),Default::default()).unwrap().execute_controlled(|at,h|{if at==ProgramCheckpointV1::BeforePublish&&!fired{fired=true;h.request_stop();}});
    assert_eq!(out.status,ProgramStatusV1::Cancelled(ProgramCancelReasonV1::UserStop));assert!(s.binding("before").is_none());
    assert_eq!(run(&mut s,"recovered=1;").status,ProgramStatusV1::Success);
    let out=s.prepare(module(src),ScientificProgramOptionsV1{primitive_fuel:32,..Default::default()}).unwrap().execute();assert_eq!(out.status,ProgramStatusV1::Resource);assert!(s.binding("after").is_none());
    let out=run(&mut s,"prior=9;A=[1 1;1 1];X=A\\[1;1];after=1;");assert_eq!(out.status,ProgramStatusV1::Error);assert_eq!(s.binding("prior").unwrap().scalar().unwrap(),9.);assert!(s.binding("after").is_none());assert_eq!(run(&mut s,"recovered=2;").status,ProgramStatusV1::Success);
    let literal=std::iter::repeat_n("0",256).collect::<Vec<_>>().join(";");let out=run(&mut s,&format!("row_literal=[{literal}];"));assert_eq!(out.status,ProgramStatusV1::Success);assert_eq!(s.binding("row_literal").unwrap().shape(),[256,1]);
    let out=run(&mut s,&format!("row_literal=[{literal};0];"));assert_eq!(out.status,ProgramStatusV1::Resource);assert_eq!(s.binding("row_literal").unwrap().shape(),[256,1]);assert_eq!(run(&mut s,"recovered=3;").status,ProgramStatusV1::Success);
}
#[test]
fn scientific_original_frequency_response_executes_actual_full_shape(){
    let source=include_str!("../../../examples/scientific/frequency_response.m");let mut s=ScientificProgramSessionV1::new().unwrap();let out=run(&mut s,source);assert_eq!(out.status,ProgramStatusV1::Success,"{:?}",out.error);let y=s.binding("Y").unwrap();assert_eq!(y.shape(),[128,32,8]);assert_eq!(y.len(),32768);assert!(y.is_complex());for page in 0..8{assert!((y.component(page*128*32).unwrap().im-(page+1) as f64/8.).abs()<2f64.powi(-32));}assert_eq!(out.stats.solves_completed,8);assert!(s.binding("normalized_residual").unwrap().scalar().unwrap()<2f64.powi(-38));assert!(out.stats.peak_live_payload_bytes<=64<<20);assert!(out.stats.peak_metadata_bytes<=16<<20);assert!(out.stats.primitive_steps<=1_000_000_000);
}

#[test]
fn scientific_canonical_trailing_singletons_and_broadcast_result_shapes(){
    let mut s=ScientificProgramSessionV1::new().unwrap();let out=run(&mut s,"A=zeros([2 3 1]);R=reshape(A,[2 1 3 1]);Z=zeros([2 1 3 1])+zeros([1 4 1 1]);E=zeros([2 1 0 1]);S=size(R);");assert_eq!(out.status,ProgramStatusV1::Success,"{:?}",out.error);assert_eq!(s.binding("A").unwrap().shape(),[2,3]);assert_eq!(s.binding("R").unwrap().shape(),[2,1,3]);assert_eq!(s.binding("Z").unwrap().shape(),[2,4,3]);assert_eq!(s.binding("E").unwrap().shape(),[2,1,0]);assert_eq!(s.binding("S").unwrap().len(),3);
    let out=run(&mut s,"bad=A(:,:,1);");assert_eq!(out.status,ProgramStatusV1::Error);assert_eq!(out.error.unwrap().code,ScientificProgramErrorCodeV1::Arity);assert!(s.binding("bad").is_none());
}
