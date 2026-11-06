// SPDX-License-Identifier: GPL-3.0-or-later
use mathlab_array::scientific::*;
use std::{cell::Cell,future::Future,task::{Context,Poll,Waker}};
const HEADROOM:usize=64<<20;
#[derive(Default)]
struct Journal{sites:Cell<usize>,reserved:Cell<usize>,checks:Cell<usize>,units:Cell<u64>,largest:Cell<u64>,fail_site:Cell<Option<usize>>,stop_check:Cell<Option<usize>>}
struct Control<'a>(&'a Journal);
impl ScientificControlV1 for Control<'_>{
    fn reserve(&mut self,n:usize)->Result<(),ScientificErrorV1>{let site=self.0.sites.get()+1;self.0.sites.set(site);if self.0.fail_site.get()==Some(site){return Err(ScientificErrorV1::Allocation)}self.0.reserved.set(self.0.reserved.get()+n);Ok(())}
    fn charge(&mut self,n:u64)->Result<(),ScientificErrorV1>{assert!(n<=128);let call=self.0.checks.get()+1;self.0.checks.set(call);self.0.units.set(self.0.units.get()+n);self.0.largest.set(self.0.largest.get().max(n));if self.0.stop_check.get()==Some(call){Err(ScientificErrorV1::Cancelled)}else{Ok(())}}
}
fn drive<F:Future>(future:F)->F::Output{let mut future=std::pin::pin!(future);let mut cx=Context::from_waker(Waker::noop());loop{if let Poll::Ready(result)=future.as_mut().poll(&mut cx){return result}}}
fn csc(shape:(u64,u64),rows:&[u32],columns:&[u32],values:ScientificTripletValuesV1<'_>)->ScientificCscV1{drive(ScientificCscV1::from_triplets_cooperative(ScientificCscShapeV1::new(shape.0,shape.1).unwrap(),rows,columns,values,HEADROOM,&mut Control(&Journal::default()))).unwrap()}
fn dense(shape:&[u64],values:&[f64])->ScientificDenseV1{ScientificDenseV1::from_real(ScientificShapeV1::new(shape).unwrap(),values,&mut Control(&Journal::default())).unwrap()}
fn real_values(a:&ScientificCscV1)->&[f64]{match a.values(){ScientificCscValuesV1::Real(v)=>v,_=>panic!("expected real CSC")}}

#[test]
fn canonical_csc_stable_duplicate_order_zero_pruning_and_borrowed_complete_parts(){
    let rows=[2,0,1,0,0,2,1];let columns=[1,0,1,0,0,1,0];let values=[4.,1e16,5.,1.,-1e16,-4.,7.];
    let a=csc((3,3),&rows,&columns,ScientificTripletValuesV1::Real(&values));
    assert_eq!(a.column_pointers(),[0,1,2,2]);assert_eq!(a.row_indices(),[1,1]);assert_eq!(real_values(&a),[7.,5.]);assert_eq!(a.nnz(),2);assert_eq!(a.nzmax(),2);assert_eq!(a.capacity_bytes(),4*4+2*4+2*8);
    // A different original duplicate order gives a different rounded result;
    // sorting must never reorder equal (column,row) keys.
    let b=csc((1,1),&[0,0,0],&[0,0,0],ScientificTripletValuesV1::Real(&[1e16,-1e16,1.]));assert_eq!(real_values(&b),[1.]);
    assert_eq!(rows,[2,0,1,0,0,2,1]);assert_eq!(values[1].to_bits(),1e16f64.to_bits());
}
#[test]
fn logical_shape_not_dense_numel_and_exact_three_vector_capacity_boundaries(){
    let shape=ScientificCscShapeV1::new(1<<20,1<<20).unwrap();assert_eq!(shape.numel(),1u64<<40);assert!(ScientificCscShapeV1::new((1<<20)+1,1).is_err());
    let exact=ScientificCscShapeV1::new(1<<20,(1<<20)-1).unwrap();assert_eq!(ScientificCscV1::payload_bytes_for(exact,1<<20,false).unwrap(),16<<20);assert_eq!(ScientificCscV1::payload_bytes_for(shape,1<<20,false),Err(ScientificErrorV1::Resource));
    assert_eq!(ScientificCscV1::payload_bytes_for(ScientificCscShapeV1::new(1<<20,3).unwrap(),838860,true).unwrap(),16<<20);assert!(ScientificCscV1::payload_bytes_for(ScientificCscShapeV1::new(1<<20,4).unwrap(),838860,true).is_err());
    let a=csc((32768,32768),&[32767,0],&[32767,0],ScientificTripletValuesV1::Real(&[2.,3.]));assert_eq!(a.shape().numel(),1<<30);assert_eq!(a.nnz(),2);assert_eq!(a.capacity_bytes(),32769*4+2*12);assert_eq!(a.column_pointers().last(),Some(&2));
    assert_eq!(ScientificCscV1::payload_bytes_for(ScientificCscShapeV1::new(32768,32768).unwrap(),223232,false).unwrap(),2809860);
}
#[test]
fn retained_zero_imaginary_demotes_including_empty_actual_min_one_capacity(){
    let values=[Complex64V1{re:2.,im:3.},Complex64V1{re:-2.,im:-3.}];let a=csc((2,2),&[0,0],&[1,1],ScientificTripletValuesV1::Complex(&values));assert!(!a.is_complex());assert!(!a.has_nonfinite_values());assert_eq!(a.nnz(),0);assert_eq!(a.nzmax(),1);assert_eq!(a.column_pointers(),[0,0,0]);assert_eq!(a.capacity_bytes(),24);assert!(real_values(&a).is_empty());
    let b=csc((0,3),&[],&[],ScientificTripletValuesV1::Real(&[]));assert_eq!(b.column_pointers(),[0,0,0,0]);assert_eq!(b.capacity_bytes(),28);assert_eq!(b.shape().numel(),0);assert!(!b.is_complex());assert_eq!(b.nzmax(),1);
    let d=csc((3,0),&[],&[],ScientificTripletValuesV1::ComplexScalar(Complex64V1{re:f64::NAN,im:f64::INFINITY}));assert_eq!(d.column_pointers(),[0]);assert_eq!(d.nnz(),0);assert_eq!(d.nzmax(),1);assert_eq!(d.capacity_bytes(),16);assert!(!d.is_complex());assert!(!d.has_nonfinite_values());
    let input=[Complex64V1{re:2.,im:4.},Complex64V1{re:-2.,im:-4.},Complex64V1{re:3.,im:5.},Complex64V1{re:0.,im:-5.}];let e=csc((1,2),&[0,0,0,0],&[0,0,1,1],ScientificTripletValuesV1::Complex(&input));assert!(!e.is_complex());assert_eq!(e.column_pointers(),[0,0,1]);assert_eq!(real_values(&e),[3.]);
}
#[test]
fn index_and_two_phase_headroom_fail_before_publication(){
    let shape=ScientificCscShapeV1::new(3,4).unwrap();let j=Journal::default();let mut control=Control(&j);
    assert!(matches!(drive(ScientificCscV1::from_triplets_cooperative(shape,&[3],&[0],ScientificTripletValuesV1::RealScalar(1.),HEADROOM,&mut control)),Err(ScientificErrorV1::Index)));assert_eq!(j.sites.get(),0);
    let j=Journal::default();assert!(matches!(drive(ScientificCscV1::from_triplets_cooperative(shape,&[0,1,2],&[0,1,2],ScientificTripletValuesV1::RealScalar(1.),23,&mut Control(&j))),Err(ScientificErrorV1::Resource)));assert_eq!(j.sites.get(),0);
    let j=Journal::default();assert!(matches!(drive(ScientificCscV1::from_triplets_cooperative(shape,&[0,1,2],&[0,1,2],ScientificTripletValuesV1::RealScalar(1.),79,&mut Control(&j))),Err(ScientificErrorV1::Resource)));assert_eq!(j.sites.get(),2); // scratch24 + final56 requires80
    let shape=ScientificCscShapeV1::new(0,0).unwrap();assert_eq!(ScientificCscV1::payload_bytes_for(shape,0,false).unwrap(),16);assert_eq!(ScientificCscV1::payload_bytes_for(shape,0,true).unwrap(),16);
    let j=Journal::default();assert!(matches!(drive(ScientificCscV1::from_triplets_cooperative(shape,&[],&[],ScientificTripletValuesV1::Real(&[]),15,&mut Control(&j))),Err(ScientificErrorV1::Resource)));assert_eq!(j.sites.get(),2);
}
#[test]
fn real_and_complex_sparse_dense_products_multiple_rhs_and_breakdown(){
    let a=csc((3,3),&[0,1,0,1,2,1,2],&[0,0,1,1,1,2,2],ScientificTripletValuesV1::Real(&[4.,-1.,-1.,4.,-1.,-1.,4.]));
    let b=dense(&[3,2],&[1.,2.,3.,-1.,0.,2.]);let out=drive(a.multiply_dense_cooperative(&b,HEADROOM,&mut Control(&Journal::default()))).unwrap();assert_eq!(out.real_words().unwrap(),[2.,4.,10.,-4.,-1.,8.]);assert_eq!(b.real_words().unwrap(),[1.,2.,3.,-1.,0.,2.]);
    let a=csc((2,2),&[0,1,0],&[0,0,1],ScientificTripletValuesV1::Complex(&[Complex64V1{re:1.,im:2.},Complex64V1{re:0.,im:-1.},Complex64V1{re:3.,im:0.}]));let b=dense(&[2,1],&[2.,-1.]);let out=drive(a.multiply_dense_cooperative(&b,HEADROOM,&mut Control(&Journal::default()))).unwrap();assert_eq!(out.complex_words().unwrap(),[Complex64V1{re:-1.,im:4.},Complex64V1{re:0.,im:-2.}]);
    let a=csc((1,1),&[0],&[0],ScientificTripletValuesV1::RealScalar(f64::MAX));assert!(matches!(drive(a.multiply_dense_cooperative(&dense(&[1,1],&[2.]),HEADROOM,&mut Control(&Journal::default()))),Err(ScientificErrorV1::ArithmeticBreakdown)));
    let zero=csc((1,1),&[],&[],ScientificTripletValuesV1::Real(&[]));assert!(matches!(drive(zero.multiply_dense_cooperative(&dense(&[1,1],&[f64::NAN]),HEADROOM,&mut Control(&Journal::default()))),Err(ScientificErrorV1::NonFiniteInput)));
}
#[test]
fn every_real_reservation_fault_preserves_prior_binding_and_recovers(){
    let prior=csc((2,2),&[0],&[1],ScientificTripletValuesV1::RealScalar(7.));let shape=ScientificCscShapeV1::new(3,3).unwrap();
    for site in 1..=5{let j=Journal::default();j.fail_site.set(Some(site));let out=drive(ScientificCscV1::from_triplets_cooperative(shape,&[2,0,1],&[1,0,1],ScientificTripletValuesV1::RealScalar(2.),HEADROOM,&mut Control(&j)));assert!(matches!(out,Err(ScientificErrorV1::Allocation)));assert_eq!(j.sites.get(),site);assert_eq!(real_values(&prior),[7.]);let recovered=csc((1,1),&[0],&[0],ScientificTripletValuesV1::RealScalar(1.));assert_eq!(real_values(&recovered),[1.]);}
    for site in 1..=3{let j=Journal::default();j.fail_site.set(Some(site));assert!(matches!(drive(prior.try_copy_cooperative(HEADROOM,&mut Control(&j))),Err(ScientificErrorV1::Allocation)));assert_eq!(j.sites.get(),site);assert_eq!(real_values(&prior),[7.]);}
    let j=Journal::default();j.fail_site.set(Some(1));assert!(matches!(drive(prior.multiply_dense_cooperative(&dense(&[2,1],&[1.,1.]),HEADROOM,&mut Control(&j))),Err(ScientificErrorV1::Allocation)));assert_eq!(j.sites.get(),1);
}
#[test]
fn every_builder_checkpoint_cancel_and_suspension_drop_keep_prior_binding(){
    let prior=csc((1,1),&[0],&[0],ScientificTripletValuesV1::RealScalar(9.));let rows=[2,1,0,1,2,0,1,0,2];let columns=[2,1,0,2,1,2,0,1,0];let shape=ScientificCscShapeV1::new(3,3).unwrap();let baseline=Journal::default();drive(ScientificCscV1::from_triplets_cooperative(shape,&rows,&columns,ScientificTripletValuesV1::RealScalar(2.),HEADROOM,&mut Control(&baseline))).unwrap();let checks=baseline.checks.get();assert!(checks>8);
    for stop in 1..=checks{let j=Journal::default();j.stop_check.set(Some(stop));assert!(matches!(drive(ScientificCscV1::from_triplets_cooperative(shape,&rows,&columns,ScientificTripletValuesV1::RealScalar(2.),HEADROOM,&mut Control(&j))),Err(ScientificErrorV1::Cancelled)));assert_eq!(j.checks.get(),stop);assert_eq!(real_values(&prior),[9.]);}
    let mut saw_success=false;
    for drop_at in 0..=checks+2{let j=Journal::default();let mut control=Control(&j);{
        let future=ScientificCscV1::from_triplets_cooperative(shape,&rows,&columns,ScientificTripletValuesV1::RealScalar(2.),HEADROOM,&mut control);assert!(std::mem::size_of_val(&future)<=16384);let mut future=std::pin::pin!(future);let mut cx=Context::from_waker(Waker::noop());
        for _ in 0..=drop_at{let before=j.units.get();let state=future.as_mut().poll(&mut cx);assert!(j.units.get()-before<=128);match state{Poll::Pending=>{},Poll::Ready(result)=>{let result=result.unwrap();assert_eq!(result.nnz(),9);saw_success=true;break}}}
    }assert_eq!(real_values(&prior),[9.]);if saw_success{break}}
    assert!(saw_success);
}
#[test]
fn every_matvec_checkpoint_stop_and_drop_leave_inputs_and_borrowed_storage_intact(){
    let a=csc((3,3),&[0,1,2],&[0,1,2],ScientificTripletValuesV1::RealScalar(2.));let rhs=dense(&[3,2],&[1.,2.,3.,4.,5.,6.]);let base=Journal::default();drive(a.multiply_dense_cooperative(&rhs,HEADROOM,&mut Control(&base))).unwrap();assert!(base.checks.get()>3);
    for stop in 1..=base.checks.get(){let j=Journal::default();j.stop_check.set(Some(stop));assert!(matches!(drive(a.multiply_dense_cooperative(&rhs,HEADROOM,&mut Control(&j))),Err(ScientificErrorV1::Cancelled)));assert_eq!(real_values(&a),[2.,2.,2.]);assert_eq!(rhs.real_words().unwrap(),[1.,2.,3.,4.,5.,6.]);}
    let mut success=false;for drop_at in 0..=base.checks.get()+2{let j=Journal::default();let mut control=Control(&j);{let future=a.multiply_dense_cooperative(&rhs,HEADROOM,&mut control);let mut future=std::pin::pin!(future);let mut cx=Context::from_waker(Waker::noop());for _ in 0..=drop_at{let before=j.units.get();let state=future.as_mut().poll(&mut cx);assert!(j.units.get()-before<=128);match state{Poll::Pending=>{},Poll::Ready(out)=>{assert_eq!(out.unwrap().real_words().unwrap(),[2.,4.,6.,8.,10.,12.]);success=true;break}}}}assert_eq!(a.nnz(),3);if success{break}}assert!(success);
}

#[test]
fn real_complex_copy_every_checkpoint_stop_drop_poll_bound_and_recovery(){
    fn assert_complete_independent(a:&ScientificCscV1,b:&ScientificCscV1){
        assert_eq!(a.shape(),b.shape());assert_eq!(a.nnz(),b.nnz());assert_eq!(a.nzmax(),b.nzmax());assert_eq!(a.capacity_bytes(),b.capacity_bytes());assert_eq!(a.column_pointers(),b.column_pointers());assert_eq!(a.row_indices(),b.row_indices());
        assert_ne!(a.column_pointers().as_ptr(),b.column_pointers().as_ptr());assert_ne!(a.row_indices().as_ptr(),b.row_indices().as_ptr());
        match (a.values(),b.values()){
            (ScientificCscValuesV1::Real(x),ScientificCscValuesV1::Real(y))=>{assert_ne!(x.as_ptr(),y.as_ptr());assert_eq!(x.len(),y.len());for (x,y)in x.iter().zip(y){assert_eq!(x.to_bits(),y.to_bits());}},
            (ScientificCscValuesV1::Complex(x),ScientificCscValuesV1::Complex(y))=>{assert_ne!(x.as_ptr(),y.as_ptr());assert_eq!(x.len(),y.len());for (x,y)in x.iter().zip(y){assert_eq!(x.re.to_bits(),y.re.to_bits());assert_eq!(x.im.to_bits(),y.im.to_bits());}},
            _=>panic!("copy changed storage class"),
        }
    }
    // All three buffers cross chunk boundaries. Complex components include a
    // preserved negative zero paired with a nonzero imaginary component.
    let rows=(0..257).map(|i|(i%2) as u32).collect::<Vec<_>>();let columns=(0..257u32).collect::<Vec<_>>();let real=(1..=257).map(|i|-(i as f64)).collect::<Vec<_>>();let complex=(0..257).map(|i|Complex64V1{re:if i==0{-0.}else{i as f64},im:(i+1) as f64}).collect::<Vec<_>>();
    let prior=csc((1,1),&[0],&[0],ScientificTripletValuesV1::RealScalar(19.));
    for source in [csc((2,257),&rows,&columns,ScientificTripletValuesV1::Real(&real)),csc((2,257),&rows,&columns,ScientificTripletValuesV1::Complex(&complex))]{
        let base=Journal::default();let copy=drive(source.try_copy_cooperative(HEADROOM,&mut Control(&base))).unwrap();assert_complete_independent(&source,&copy);assert_eq!(base.sites.get(),3);assert_eq!(base.reserved.get(),copy.capacity_bytes());let checkpoints=base.checks.get();assert!(checkpoints>=9);assert!(base.largest.get()<=128);
        for stop in 1..=checkpoints{
            let j=Journal::default();j.stop_check.set(Some(stop));assert!(matches!(drive(source.try_copy_cooperative(HEADROOM,&mut Control(&j))),Err(ScientificErrorV1::Cancelled)));assert_eq!(j.checks.get(),stop);assert_complete_independent(&source,&copy);assert_eq!(real_values(&prior),[19.]);
            // A new request ledger starts from the unchanged caller-owned value;
            // no assertion treats the reserve hook as a deallocation callback.
            let recovered=drive(source.try_copy_cooperative(HEADROOM,&mut Control(&Journal::default()))).unwrap();assert_complete_independent(&source,&recovered);
        }
        let mut saw_success=false;
        for drop_at in 0..=checkpoints+2{
            let j=Journal::default();let mut control=Control(&j);
            {let future=source.try_copy_cooperative(HEADROOM,&mut control);assert!(std::mem::size_of_val(&future)<=16384);let mut future=std::pin::pin!(future);let mut cx=Context::from_waker(Waker::noop());
                for _ in 0..drop_at{let before=j.units.get();let state=future.as_mut().poll(&mut cx);assert!(j.units.get()-before<=128);match state{Poll::Pending=>{},Poll::Ready(out)=>{assert_complete_independent(&source,&out.unwrap());saw_success=true;break}}}
            }
            assert_complete_independent(&source,&copy);assert_eq!(real_values(&prior),[19.]);let recovered=drive(source.try_copy_cooperative(HEADROOM,&mut Control(&Journal::default()))).unwrap();assert_complete_independent(&source,&recovered);if saw_success{break}
        }
        assert!(saw_success);let expected=match source.values(){ScientificCscValuesV1::Real(v)=>v.iter().map(|x|[x.to_bits(),0]).collect::<Vec<_>>(),ScientificCscValuesV1::Complex(v)=>v.iter().map(|x|[x.re.to_bits(),x.im.to_bits()]).collect::<Vec<_>>()};drop(source);
        let actual=match copy.values(){ScientificCscValuesV1::Real(v)=>v.iter().map(|x|[x.to_bits(),0]).collect::<Vec<_>>(),ScientificCscValuesV1::Complex(v)=>v.iter().map(|x|[x.re.to_bits(),x.im.to_bits()]).collect::<Vec<_>>()};assert_eq!(actual,expected);
    }
}


#[test]
fn selected_native_s01_s04_constructor_order_overflow_nan_and_infinity(){
    // The selected six duplicate permutations from S01, not a universal
    // MATLAB order/bit-equivalence claim.
    let values=[1e16,1.,-1e16,1e16,-1e16,1.,-1e16,1.,1e16,1.,-1e16,1e16,1.,1e16,-1e16,-1e16,1e16,1.];let rows=[0u32;18];let columns=(0..18).map(|i|(i/3) as u32).collect::<Vec<_>>();let a=csc((1,6),&rows,&columns,ScientificTripletValuesV1::Real(&values));assert_eq!(a.column_pointers(),[0,0,1,1,1,1,2]);assert_eq!(real_values(&a),[1.,1.]);assert!(!a.has_nonfinite_values());
    let h=f64::MAX;let u=f64::from_bits(1);let a=csc((1,4),&[0;12],&[0,0,0,1,1,1,2,2,2,3,3,3],ScientificTripletValuesV1::Real(&[h,h,-h,h,-h,h,u,u,-u,u,-u,u]));let words=real_values(&a);assert_eq!(words.len(),4);assert_eq!(words[0],f64::INFINITY);assert_eq!(words[1],h);assert_eq!(words[2].to_bits(),1);assert_eq!(words[3].to_bits(),1);assert!(a.has_nonfinite_values());
    let a=csc((1,2),&[0,0,0],&[0,1,1],ScientificTripletValuesV1::Real(&[f64::NAN,f64::NAN,1.]));assert_eq!(a.nnz(),2);assert!(real_values(&a).iter().all(|v|v.is_nan()));assert!(!a.is_complex());assert!(a.has_nonfinite_values());
    let a=csc((1,4),&[0;5],&[0,1,2,2,3],ScientificTripletValuesV1::Real(&[f64::INFINITY,f64::NEG_INFINITY,f64::INFINITY,f64::NEG_INFINITY,0.]));let words=real_values(&a);assert_eq!(a.column_pointers(),[0,1,2,3,3]);assert_eq!(words[0],f64::INFINITY);assert_eq!(words[1],f64::NEG_INFINITY);assert!(words[2].is_nan());assert!(a.has_nonfinite_values());
    let copy=drive(a.try_copy_cooperative(HEADROOM,&mut Control(&Journal::default()))).unwrap();assert_eq!(copy.capacity_bytes(),a.capacity_bytes());assert!(copy.has_nonfinite_values());assert_eq!(real_values(&copy)[0],f64::INFINITY);assert!(real_values(&copy)[2].is_nan());
    let rhs=dense(&[4,1],&[0.;4]);let j=Journal::default();assert!(matches!(drive(copy.multiply_dense_cooperative(&rhs,HEADROOM,&mut Control(&j))),Err(ScientificErrorV1::NonFiniteInput)));assert_eq!(j.sites.get(),0); // qualification, not native nonfinite-SpMV parity
    let a=csc((1,1),&[0],&[0],ScientificTripletValuesV1::ComplexScalar(Complex64V1{re:0.,im:f64::NAN}));assert!(a.is_complex());assert!(a.has_nonfinite_values());match a.values(){ScientificCscValuesV1::Complex(v)=>assert!(v[0].im.is_nan()),_=>panic!()};
}
#[test]
fn empty_min_one_copy_capacity_fault_stop_drop_and_recovery(){
    for shape in [(0,0),(0,3),(3,0),(2,2)]{
        let input=csc(shape,&[],&[],ScientificTripletValuesV1::Complex(&[]));assert!(!input.is_complex());assert_eq!(input.nnz(),0);assert_eq!(input.nzmax(),1);assert_eq!(input.capacity_bytes(),(shape.1 as usize+1)*4+12);
        let prior=csc((1,1),&[0],&[0],ScientificTripletValuesV1::RealScalar(23.));
        for site in 1..=5{let j=Journal::default();j.fail_site.set(Some(site));assert!(matches!(drive(ScientificCscV1::from_triplets_cooperative(ScientificCscShapeV1::new(shape.0,shape.1).unwrap(),&[],&[],ScientificTripletValuesV1::Complex(&[]),HEADROOM,&mut Control(&j))),Err(ScientificErrorV1::Allocation)));assert_eq!(j.sites.get(),site);assert_eq!(real_values(&prior),[23.]);}
        for site in 1..=3{let j=Journal::default();j.fail_site.set(Some(site));assert!(matches!(drive(input.try_copy_cooperative(HEADROOM,&mut Control(&j))),Err(ScientificErrorV1::Allocation)));assert_eq!(j.sites.get(),site);assert_eq!(input.nzmax(),1);}
        let j=Journal::default();let copy=drive(input.try_copy_cooperative(HEADROOM,&mut Control(&j))).unwrap();assert_eq!(j.reserved.get(),input.capacity_bytes());assert_eq!(j.sites.get(),3);assert_eq!(copy.nzmax(),1);assert_eq!(copy.capacity_bytes(),input.capacity_bytes());assert!(real_values(&copy).is_empty());
        for stop in 1..=j.checks.get(){let stopped=Journal::default();stopped.stop_check.set(Some(stop));assert!(matches!(drive(input.try_copy_cooperative(HEADROOM,&mut Control(&stopped))),Err(ScientificErrorV1::Cancelled)));assert_eq!(stopped.checks.get(),stop);assert_eq!(input.nzmax(),1);}
        let mut saw_success=false;for drop_at in 0..=j.checks.get()+2{let check=Journal::default();let mut control=Control(&check);{let future=input.try_copy_cooperative(HEADROOM,&mut control);let mut future=std::pin::pin!(future);let mut cx=Context::from_waker(Waker::noop());for _ in 0..drop_at{let before=check.units.get();let state=future.as_mut().poll(&mut cx);assert!(check.units.get()-before<=128);match state{Poll::Pending=>{},Poll::Ready(result)=>{let result=result.unwrap();assert_eq!(result.nzmax(),1);assert_eq!(result.capacity_bytes(),input.capacity_bytes());saw_success=true;break}}}}let recovered=drive(input.try_copy_cooperative(HEADROOM,&mut Control(&Journal::default()))).unwrap();assert_eq!(recovered.capacity_bytes(),input.capacity_bytes());assert_eq!(real_values(&prior),[23.]);if saw_success{break}}assert!(saw_success);
    }
}
