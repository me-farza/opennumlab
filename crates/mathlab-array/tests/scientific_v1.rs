// SPDX-License-Identifier: GPL-3.0-or-later
use mathlab_array::scientific::*;
#[derive(Default)]
struct Control{reserved:usize,sites:usize,steps:u64,largest_chunk:u64,fail:Option<usize>,stop_after:Option<u64>}
impl ScientificControlV1 for Control{
    fn charge(&mut self,n:u64)->Result<(),ScientificErrorV1>{self.largest_chunk=self.largest_chunk.max(n);self.steps+=n;if self.stop_after.is_some_and(|s|self.steps>=s){Err(ScientificErrorV1::Cancelled)}else{Ok(())}}
    fn reserve(&mut self,n:usize)->Result<(),ScientificErrorV1>{self.sites+=1;if self.fail==Some(self.sites){return Err(ScientificErrorV1::Allocation)}self.reserved+=n;Ok(())}
}
fn real(shape:&[u64],v:&[f64])->ScientificDenseV1{ScientificDenseV1::from_real(ScientificShapeV1::new(shape).unwrap(),v,&mut Control::default()).unwrap()}
#[test]
fn complex_division_preserves_independent_extreme_components(){
    let huge=f64::from_bits(2046u64<<52);let tiny=f64::MIN_POSITIVE;let sub=f64::from_bits(1);
    let a=Complex64V1{re:huge,im:tiny};
    assert_eq!(a.checked_div(Complex64V1::ONE).unwrap(),a);
    assert_eq!(a.checked_div(Complex64V1{re:0.,im:1.}).unwrap(),Complex64V1{re:tiny,im:-huge});
    let a=Complex64V1{re:huge,im:sub};
    assert_eq!(a.checked_div(Complex64V1::ONE).unwrap().im.to_bits(),1);
    // Non-unit denominator: a tiny denominator component times a huge
    // numerator component yields a representable quotient component.
    let a=Complex64V1{re:0.,im:huge};let b=Complex64V1{re:2.,im:sub};let q=a.checked_div(b).unwrap();
    assert_eq!(q.re.to_bits(),(2f64.powi(-53)).to_bits());
    assert_eq!(q.im.to_bits(),(huge/2.).to_bits());
    assert_eq!(Complex64V1::ONE.checked_div(Complex64V1::ZERO),Err(ScientificErrorV1::Singular));
    assert_eq!(Complex64V1::real(f64::MAX).checked_div(Complex64V1::real(sub)),Err(ScientificErrorV1::ArithmeticBreakdown));
}
#[test]
fn dense_rank_strides_classes_complete_borrow_and_reserve_faults(){
    let shape=ScientificShapeV1::new(&[2,3,2]).unwrap();assert_eq!(shape.offset(&[1,2,1]).unwrap(),11);
    let mut c=Control::default();let a=ScientificDenseV1::zeros(shape,true,&mut c).unwrap();assert_eq!(c.sites,1);assert_eq!(c.reserved,192);assert_eq!(a.capacity_bytes(),192);assert!(a.is_complex());
    match a.read_range(10,2).unwrap(){ScientificReadRangeV1::Complex(words)=>assert_eq!(words.len(),2),_=>panic!()};
    assert!(a.read_range(11,2).is_err());assert!(ScientificShapeV1::new(&[2,2,2,2,2,2,2,2,2]).is_err());assert!(ScientificShapeV1::new(&[1025,1024]).is_err());
    let mut c=Control{fail:Some(1),..Default::default()};assert!(matches!(a.try_copy(&mut c),Err(ScientificErrorV1::Allocation)));assert_eq!(c.steps,0);
    let logical=ScientificDenseV1::zeros_logical(ScientificShapeV1::new(&[0,3]).unwrap(),&mut Control::default()).unwrap();assert_eq!(logical.class(),ScientificClassV1::Logical);assert_eq!(logical.shape().axes(),[0,3]);
}
#[test]
fn solve_pivot_multiple_rhs_resumes_reports_growth_and_keeps_inputs(){
    let a=real(&[2,2],&[0.,1.,1.,1.]);let b=real(&[2,2],&[1.,2.,3.,5.]);let mut c=Control::default();let mut solver=ScientificSolveV1::new(&a,&b,&mut c).unwrap();
    assert_eq!(c.sites,2);assert_eq!(c.reserved,64);let mut chunks=0;
    loop{chunks+=1;match solver.step(128,&mut c).unwrap(){SolveProgressV1::Pending{scalar_steps}=>assert!(scalar_steps<=128),SolveProgressV1::Complete{scalar_steps}=>{assert!(scalar_steps<=128);break}}}
    assert!(chunks>1);let d=solver.diagnostics();assert_eq!(d.initial_metric,1.);assert_eq!(d.maximum_u_metric,1.);assert_eq!(d.growth_factor(),Some(1.));assert!(d.scalar_steps>0);
    let x=solver.into_result().unwrap();assert_eq!(x.real_words().unwrap(),[1.,1.,2.,3.]);assert_eq!(a.real_words().unwrap(),[0.,1.,1.,1.]);assert_eq!(b.real_words().unwrap(),[1.,2.,3.,5.]);assert!(c.largest_chunk<=128);
}
#[test]
fn solve_each_real_reserve_fault_and_stop_leave_originals(){
    let a=real(&[2,2],&[4.,-1.,-1.,4.]);let b=real(&[2,1],&[3.,3.]);
    for fail in 1..=2{let mut c=Control{fail:Some(fail),..Default::default()};assert!(matches!(ScientificSolveV1::new(&a,&b,&mut c),Err(ScientificErrorV1::Allocation)));assert_eq!(c.sites,fail);assert_eq!(a.real_words().unwrap(),[4.,-1.,-1.,4.]);}
    let mut solver=ScientificSolveV1::new(&a,&b,&mut Control::default()).unwrap();let mut c=Control{stop_after:Some(8),..Default::default()};assert_eq!(solver.step(32768,&mut c),Err(ScientificErrorV1::Cancelled));assert!(solver.into_result().is_err());assert_eq!(b.real_words().unwrap(),[3.,3.]);
    let singular=real(&[2,2],&[1.,1.,1.,1.]);let mut solver=ScientificSolveV1::new(&singular,&b,&mut Control::default()).unwrap();let error=loop{match solver.step(32768,&mut Control::default()){Err(e)=>break e,Ok(SolveProgressV1::Pending{..})=>{},Ok(SolveProgressV1::Complete{..})=>panic!()}};assert_eq!(error,ScientificErrorV1::Singular);
}

#[test]
fn canonical_shape_trims_only_trailing_ones_and_reshape_keeps_words(){
    assert_eq!(ScientificShapeV1::new(&[2,3,1,1]).unwrap().axes(),[2,3]);
    assert_eq!(ScientificShapeV1::new(&[2,1,0,1]).unwrap().axes(),[2,1,0]);
    let a=real(&[2,3],&[1.,2.,3.,4.,5.,6.]);let b=a.reshape(ScientificShapeV1::new(&[2,1,3,1]).unwrap()).unwrap();assert_eq!(b.shape().axes(),[2,1,3]);assert_eq!(b.real_words().unwrap(),[1.,2.,3.,4.,5.,6.]);
}

fn complex(shape:&[u64],v:&[(f64,f64)])->ScientificDenseV1{let words=v.iter().map(|&(re,im)|Complex64V1{re,im}).collect::<Vec<_>>();ScientificDenseV1::from_complex(ScientificShapeV1::new(shape).unwrap(),&words,&mut Control::default()).unwrap()}
fn solved(a:&ScientificDenseV1,b:&ScientificDenseV1,quantum:u64)->ScientificDenseV1{let mut solver=ScientificSolveV1::new(a,b,&mut Control::default()).unwrap();loop{if let SolveProgressV1::Complete{..}=solver.step(quantum,&mut Control::default()).unwrap(){return solver.into_result().unwrap()}}}
#[test]
fn genuinely_complex_pivot_multiple_rhs_known_solution_and_chunk_identity(){
    let a=complex(&[2,2],&[(0.,0.),(2.,-1.),(1.,1.),(3.,2.)]);let b=complex(&[2,2],&[(4.,2.),(15.,6.),(3.,-1.),(4.,0.)]);let expected=[(1.,2.),(3.,-1.),(-2.,1.),(1.,-2.)];let reference=solved(&a,&b,128);
    for (i,&(re,im))in expected.iter().enumerate(){let z=reference.get(i).unwrap();assert!((z.re-re).abs()<2f64.powi(-44));assert!((z.im-im).abs()<2f64.powi(-44));}
    for quantum in [256,1024,32768]{let result=solved(&a,&b,quantum);for i in 0..4{assert_eq!(result.get(i).unwrap().re.to_bits(),reference.get(i).unwrap().re.to_bits());assert_eq!(result.get(i).unwrap().im.to_bits(),reference.get(i).unwrap().im.to_bits());}}
}
#[test]
fn solver_near_singular_nonfinite_overflow_and_subnormal_policies(){
    let eps=f64::EPSILON;let a=real(&[2,2],&[1.,1.,1.,1.+eps]);let b=real(&[2,1],&[1.,1.+eps]);assert_eq!(solved(&a,&b,128).real_words().unwrap(),[0.,1.]);
    for word in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY]{let a=real(&[2,2],&[word,0.,0.,1.]);let mut control=Control::default();assert!(matches!(ScientificSolveV1::new(&a,&b,&mut control),Err(ScientificErrorV1::NonFiniteInput)));assert_eq!(control.sites,0);}
    let sub=f64::from_bits(1);let a=real(&[1,1],&[sub]);let b=real(&[1,1],&[sub]);assert_eq!(solved(&a,&b,128).real_words().unwrap(),[1.]);
    let a=real(&[2,2],&[f64::MIN_POSITIVE,0.,0.,2.*f64::MIN_POSITIVE]);let b=real(&[2,1],&[f64::MIN_POSITIVE,4.*f64::MIN_POSITIVE]);assert_eq!(solved(&a,&b,128).real_words().unwrap(),[1.,2.]);
    let a=real(&[1,1],&[1e-308]);let b=real(&[1,1],&[1e308]);let mut s=ScientificSolveV1::new(&a,&b,&mut Control::default()).unwrap();let error=loop{match s.step(32768,&mut Control::default()){Err(e)=>break e,Ok(SolveProgressV1::Pending{..})=>{},_=>panic!()}};assert_eq!(error,ScientificErrorV1::ArithmeticBreakdown);
    let z=Complex64V1{re:-0.,im:0.}.conj();assert_eq!(z.re.to_bits(),(-0f64).to_bits());assert_eq!(z.im.to_bits(),(-0f64).to_bits());
    let d=ScientificSolveDiagnosticsV1{initial_metric:sub,maximum_u_metric:f64::MAX,scalar_steps:1};assert_eq!(d.growth_factor(),Some(f64::INFINITY));
}
#[test]
fn cancellation_at_every_preparation_and_factorization_work_boundary(){
    let a=complex(&[2,2],&[(0.,0.),(2.,-1.),(1.,1.),(3.,2.)]);let b=complex(&[2,2],&[(4.,2.),(15.,6.),(3.,-1.),(4.,0.)]);let mut base=Control::default();let mut solver=ScientificSolveV1::new(&a,&b,&mut base).unwrap();let prep=base.steps;loop{if matches!(solver.step(32768,&mut base).unwrap(),SolveProgressV1::Complete{..}){break}}let total=base.steps;
    for stop in 1..=total{let mut control=Control{stop_after:Some(stop),..Default::default()};match ScientificSolveV1::new(&a,&b,&mut control){Err(e)=>{assert_eq!(e,ScientificErrorV1::Cancelled);assert!(stop<=prep)},Ok(mut solver)=>{let error=loop{match solver.step(128,&mut control){Err(e)=>break e,Ok(SolveProgressV1::Pending{..})=>{},_=>panic!("missingStop{stop}")}};assert_eq!(error,ScientificErrorV1::Cancelled);assert!(solver.into_result().is_err());}}assert_eq!(a.get(1).unwrap(),Complex64V1{re:2.,im:-1.});assert_eq!(b.get(3).unwrap(),Complex64V1{re:4.,im:0.});}
}

#[test]
fn independent_all64_division_cases_and_exact_fraction_residuals(){
    use std::io::Write;
    let cases:&[(&str,[u64;4],[u64;2],u8)]=&[
    ("D01",[0x7fefffffffffffff,0x0000000000000001,0x3ff0000000000000,0x0000000000000000],[0x7fefffffffffffff,0x0000000000000001],0),
    ("D02",[0x7fefffffffffffff,0x0000000000000001,0xbff0000000000000,0x0000000000000000],[0xffefffffffffffff,0x8000000000000001],0),
    ("D03",[0x7fefffffffffffff,0x0000000000000001,0x0000000000000000,0x3ff0000000000000],[0x0000000000000001,0xffefffffffffffff],0),
    ("D04",[0x7fefffffffffffff,0x0000000000000001,0x0000000000000000,0xbff0000000000000],[0x8000000000000001,0x7fefffffffffffff],0),
    ("D05",[0x0000000000000001,0x7fefffffffffffff,0x3ff0000000000000,0x0000000000000000],[0x0000000000000001,0x7fefffffffffffff],0),
    ("D06",[0x0000000000000001,0x7fefffffffffffff,0xbff0000000000000,0x0000000000000000],[0x8000000000000001,0xffefffffffffffff],0),
    ("D07",[0x0000000000000001,0x7fefffffffffffff,0x0000000000000000,0x3ff0000000000000],[0x7fefffffffffffff,0x8000000000000001],0),
    ("D08",[0x0000000000000001,0x7fefffffffffffff,0x0000000000000000,0xbff0000000000000],[0xffefffffffffffff,0x0000000000000001],0),
    ("D09",[0xffefffffffffffff,0x0000000000000001,0x3ff0000000000000,0x0000000000000000],[0xffefffffffffffff,0x0000000000000001],0),
    ("D10",[0xffefffffffffffff,0x0000000000000001,0xbff0000000000000,0x0000000000000000],[0x7fefffffffffffff,0x8000000000000001],0),
    ("D11",[0xffefffffffffffff,0x0000000000000001,0x0000000000000000,0x3ff0000000000000],[0x0000000000000001,0x7fefffffffffffff],0),
    ("D12",[0xffefffffffffffff,0x0000000000000001,0x0000000000000000,0xbff0000000000000],[0x8000000000000001,0xffefffffffffffff],0),
    ("D13",[0x0000000000000001,0xffefffffffffffff,0x3ff0000000000000,0x0000000000000000],[0x0000000000000001,0xffefffffffffffff],0),
    ("D14",[0x0000000000000001,0xffefffffffffffff,0xbff0000000000000,0x0000000000000000],[0x8000000000000001,0x7fefffffffffffff],0),
    ("D15",[0x0000000000000001,0xffefffffffffffff,0x0000000000000000,0x3ff0000000000000],[0xffefffffffffffff,0x8000000000000001],0),
    ("D16",[0x0000000000000001,0xffefffffffffffff,0x0000000000000000,0xbff0000000000000],[0x7fefffffffffffff,0x0000000000000001],0),
    ("D17",[0x7fb0000000000000,0x0030000000000000,0x4000000000000000,0x0000000000000000],[0x7fa0000000000000,0x0020000000000000],0),
    ("D18",[0x7fb0000000000000,0x0030000000000000,0xc000000000000000,0x0000000000000000],[0xffa0000000000000,0x8020000000000000],0),
    ("D19",[0x7fb0000000000000,0x0030000000000000,0x3fe0000000000000,0x0000000000000000],[0x7fc0000000000000,0x0040000000000000],0),
    ("D20",[0x7fb0000000000000,0x0030000000000000,0xbfe0000000000000,0x0000000000000000],[0xffc0000000000000,0x8040000000000000],0),
    ("D21",[0x0030000000000000,0x7fb0000000000000,0x4000000000000000,0x0000000000000000],[0x0020000000000000,0x7fa0000000000000],0),
    ("D22",[0x0030000000000000,0x7fb0000000000000,0xc000000000000000,0x0000000000000000],[0x8020000000000000,0xffa0000000000000],0),
    ("D23",[0x0030000000000000,0x7fb0000000000000,0x3fe0000000000000,0x0000000000000000],[0x0040000000000000,0x7fc0000000000000],0),
    ("D24",[0x0030000000000000,0x7fb0000000000000,0xbfe0000000000000,0x0000000000000000],[0x8040000000000000,0xffc0000000000000],0),
    ("D25",[0x7fefffffffffffff,0x0000000000000001,0x5ff0000000000000,0x0000000000000000],[0x5fefffffffffffff,0x0000000000000000],0),
    ("D26",[0x7fefffffffffffff,0x0000000000000001,0xdff0000000000000,0x0000000000000000],[0xdfefffffffffffff,0x8000000000000000],0),
    ("D27",[0x0000000000000001,0x7fefffffffffffff,0x5ff0000000000000,0x0000000000000000],[0x0000000000000000,0x5fefffffffffffff],0),
    ("D28",[0x0000000000000001,0x7fefffffffffffff,0xdff0000000000000,0x0000000000000000],[0x8000000000000000,0xdfefffffffffffff],0),
    ("D29",[0x5f30000000000000,0x0000000000000001,0x1ff0000000000000,0x0000000000000000],[0x7f30000000000000,0x1cd0000000000000],0),
    ("D30",[0x5f30000000000000,0x0000000000000001,0x9ff0000000000000,0x0000000000000000],[0xff30000000000000,0x9cd0000000000000],0),
    ("D31",[0x0000000000000001,0x5f30000000000000,0x1ff0000000000000,0x0000000000000000],[0x1cd0000000000000,0x7f30000000000000],0),
    ("D32",[0x0000000000000001,0x5f30000000000000,0x9ff0000000000000,0x0000000000000000],[0x9cd0000000000000,0xff30000000000000],0),
    ("D33",[0x0000000000000001,0x0000000000000001,0x0000000000000001,0x0000000000000001],[0x3ff0000000000000,0x0000000000000000],0),
    ("D34",[0x0000000000000001,0x8000000000000001,0x0000000000000001,0x0000000000000001],[0x0000000000000000,0xbff0000000000000],0),
    ("D35",[0x0000000000000002,0x0000000000000001,0x0000000000000001,0x0000000000000000],[0x4000000000000000,0x3ff0000000000000],0),
    ("D36",[0x0000000000000001,0x0000000000000002,0x0000000000000000,0x0000000000000001],[0x4000000000000000,0xbff0000000000000],0),
    ("D37",[0x0000000000000001,0x0000000000000001,0x3ff0000000000000,0x3ff0000000000000],[0x0000000000000001,0x0000000000000000],0),
    ("D38",[0x0000000000000001,0x8000000000000001,0x3ff0000000000000,0x3ff0000000000000],[0x0000000000000000,0x8000000000000001],0),
    ("D39",[0x0000000000000001,0x0000000000000000,0x4000000000000000,0x0000000000000000],[0x0000000000000000,0x0000000000000000],0),
    ("D40",[0x0000000000000003,0x0000000000000000,0x4000000000000000,0x0000000000000000],[0x0000000000000002,0x0000000000000000],0),
    ("D41",[0x7fefffffffffffff,0x7fefffffffffffff,0x7fefffffffffffff,0x7fefffffffffffff],[0x3ff0000000000000,0x0000000000000000],0),
    ("D42",[0x7fefffffffffffff,0xffefffffffffffff,0x7fefffffffffffff,0x7fefffffffffffff],[0x0000000000000000,0xbff0000000000000],0),
    ("D43",[0x3ff0000000000000,0x3ff0000000000000,0x7fefffffffffffff,0x7fefffffffffffff],[0x0004000000000000,0x0000000000000000],1),
    ("D44",[0x0000000000000001,0x0000000000000001,0x7fefffffffffffff,0x7fefffffffffffff],[0x0000000000000000,0x0000000000000000],0),
    ("D45",[0x7fefffffffffffff,0x0000000000000001,0x7fefffffffffffff,0x7fefffffffffffff],[0x3fe0000000000000,0xbfe0000000000000],1),
    ("D46",[0x0000000000000001,0x7fefffffffffffff,0x7fefffffffffffff,0xffefffffffffffff],[0xbfe0000000000000,0x3fe0000000000000],1),
    ("D47",[0x3ff0000000000001,0x3ff0000000000000,0x3ff0000000000000,0x3ff0000000000001],[0x3ff0000000000000,0xbcafffffffffffff],1),
    ("D48",[0x3ff0000000000000,0x3ff0000000000001,0x3ff0000000000001,0x3ff0000000000000],[0x3ff0000000000000,0x3cafffffffffffff],1),
    ("D49",[0x3ff0000000000000,0x3ff0000000000000,0x3ff0000000000000,0x3ff0000000000001],[0x3fefffffffffffff,0xbc9ffffffffffffe],1),
    ("D50",[0x3ff0000000000000,0x3ff0000000000000,0x3ff0000000000000,0x3fefffffffffffff],[0x3ff0000000000000,0x3c90000000000001],1),
    ("D51",[0x7fefffffffffffff,0x7feffffffffffffe,0x7feffffffffffffe,0x7fefffffffffffff],[0x3ff0000000000000,0xbca0000000000001],1),
    ("D52",[0x5f30000000000000,0x5f30000000000001,0x5f30000000000001,0x5f30000000000000],[0x3ff0000000000000,0x3cafffffffffffff],1),
    ("D53",[0x7fefffffffffffff,0x0000000000000001,0x0000000000000001,0x0000000000000000],[0x0000000000000000,0x3ff0000000000000],2),
    ("D54",[0x0000000000000001,0x7fefffffffffffff,0x0000000000000001,0x0000000000000000],[0x3ff0000000000000,0x0000000000000000],2),
    ("D55",[0x7fefffffffffffff,0x7fefffffffffffff,0x0000000000000001,0x0000000000000001],[0x0000000000000000,0x0000000000000000],2),
    ("D56",[0x7fefffffffffffff,0xffefffffffffffff,0x0000000000000001,0x0000000000000001],[0x0000000000000000,0x0000000000000000],2),
    ("D57",[0x7fefffffffffffff,0x0000000000000000,0x3fe0000000000000,0x0000000000000000],[0x0000000000000000,0x0000000000000000],2),
    ("D58",[0x0000000000000000,0x7fefffffffffffff,0x0000000000000000,0x3fe0000000000000],[0x0000000000000000,0x0000000000000000],2),
    ("D59",[0x8000000000000000,0x0000000000000000,0x3ff0000000000000,0x0000000000000000],[0x0000000000000000,0x0000000000000000],0),
    ("D60",[0x0000000000000000,0x8000000000000000,0xbff0000000000000,0x0000000000000000],[0x0000000000000000,0x0000000000000000],0),
    ("D61",[0x8000000000000000,0x8000000000000000,0x0000000000000000,0x3ff0000000000000],[0x0000000000000000,0x0000000000000000],0),
    ("D62",[0x0000000000000000,0x0000000000000000,0x0000000000000000,0xbff0000000000000],[0x0000000000000000,0x0000000000000000],0),
    ("D63",[0x000fffffffffffff,0x0010000000000000,0x4000000000000000,0x0000000000000000],[0x0008000000000000,0x0008000000000000],0),
    ("D64",[0x0010000000000000,0x000fffffffffffff,0x3ff0000000000000,0x3ff0000000000000],[0x0010000000000000,0x8000000000000000],0),
    ];
    assert_eq!(cases.len(),64);
    let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();let path=std::env::temp_dir().join(format!("onl-complex-division-{}-{stamp}.txt",std::process::id()));let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
    for &(id,input,reference,mode) in cases{
        let a=Complex64V1{re:f64::from_bits(input[0]),im:f64::from_bits(input[1])};let b=Complex64V1{re:f64::from_bits(input[2]),im:f64::from_bits(input[3])};
        if mode==2{assert_eq!(a.checked_div(b),Err(ScientificErrorV1::ArithmeticBreakdown),"{id}");writeln!(file,"{id} ArithmeticBreakdown").unwrap();continue}
        let q=a.checked_div(b).unwrap_or_else(|e|panic!("{id}: {e:?}"));assert!(q.finite(),"{id}");
        if mode==0{for (got,bits)in [q.re,q.im].into_iter().zip(reference){if bits&0x7fff_ffff_ffff_ffff==0{assert_eq!(got,0.,"{id} exactzero required")}else{assert_eq!(got.to_bits(),bits,"{id}")}}}
        writeln!(file,"{id} {:016x} {:016x}",q.re.to_bits(),q.im.to_bits()).unwrap();
    }
    file.flush().unwrap();drop(file);
    let oracle=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scientific_oracle.py");let status=std::process::Command::new("/usr/bin/python3").arg(oracle).arg("--division").arg(&path).env_clear().env("PYTHONNOUSERSITE","1").env("PYTHONDONTWRITEBYTECODE","1").status().unwrap();assert!(status.success(),"independentFractionoracle failed; retained {}",path.display());
}

#[test]
fn preparation_future_suspends_before_each_bounded_fill_and_copy_chunk(){
    use std::{cell::Cell,future::Future};
    struct Journal<'a>{steps:&'a Cell<u64>,sites:usize}
    impl ScientificControlV1 for Journal<'_>{fn charge(&mut self,n:u64)->Result<(),ScientificErrorV1>{assert!(n<=128);self.steps.set(self.steps.get()+n);Ok(())}fn reserve(&mut self,_:usize)->Result<(),ScientificErrorV1>{self.sites+=1;Ok(())}}
    let steps=Cell::new(0);let mut control=Journal{steps:&steps,sites:0};let shape=ScientificShapeV1::new(&[1024,1024]).unwrap();let output;
    {let future=ScientificDenseV1::zeros_cooperative(shape,true,&mut control);assert!(std::mem::size_of_val(&future)<=1024*1024);let mut future=std::pin::pin!(future);let mut cx=std::task::Context::from_waker(std::task::Waker::noop());loop{let before=steps.get();let state=future.as_mut().poll(&mut cx);assert!(steps.get()-before<=128);if let std::task::Poll::Ready(result)=state{output=result.unwrap();break}}}
    assert_eq!(steps.get(),2*1024*1024);assert_eq!(control.sites,1);assert_eq!(output.capacity_bytes(),16*1024*1024);
    {let future=output.try_copy_cooperative(&mut control);let mut future=std::pin::pin!(future);let mut cx=std::task::Context::from_waker(std::task::Waker::noop());assert!(future.as_mut().poll(&mut cx).is_pending());}
    assert_eq!(control.sites,2);assert_eq!(output.get(output.len()-1).unwrap(),Complex64V1::ZERO);
}

#[test]
fn independent_twelve_solver_systems_exact_fraction_residual_and_conditioned_forward(){
    use std::io::Write;
    let cases:&[(&str,usize,usize,&[(u64,u64)],&[(u64,u64)],u8)]=&[
    ("S01",2,2,&[(0x0000000000000000,0x0000000000000000),(0x4000000000000000,0xbff0000000000000),(0x3ff0000000000000,0x3ff0000000000000),(0x4008000000000000,0x3fe0000000000000)],&[(0x3ff0000000000000,0x3fe0000000000000),(0x4013800000000000,0xbfd8000000000000),(0x3fe0000000000000,0x3fe0000000000000),(0xbff8000000000000,0x4011000000000000)],0),
    ("S02",2,2,&[(0x4000000000000000,0xbff0000000000000),(0x0000000000000000,0x0000000000000000),(0x4008000000000000,0x3fe0000000000000),(0x3ff0000000000000,0x3ff0000000000000)],&[(0x4013800000000000,0xbfd8000000000000),(0x3ff0000000000000,0x3fe0000000000000),(0xbff8000000000000,0x4011000000000000),(0x3fe0000000000000,0x3fe0000000000000)],0),
    ("S03",2,2,&[(0x3ff0000000000000,0x3ff0000000000000),(0x4008000000000000,0x3fe0000000000000),(0x0000000000000000,0x0000000000000000),(0x4000000000000000,0xbff0000000000000)],&[(0x3ff0000000000000,0x3fe0000000000000),(0x4013800000000000,0xbfd8000000000000),(0x3fe0000000000000,0x3fe0000000000000),(0xbff8000000000000,0x4011000000000000)],0),
    ("S04",4,2,&[(0x4010000000000000,0x3fe0000000000000),(0xbff0000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0xbff0000000000000,0x0000000000000000),(0x4010000000000000,0x3fe0000000000000),(0xbff0000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0xbff0000000000000,0x0000000000000000),(0x4010000000000000,0x3fe0000000000000),(0xbff0000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0xbff0000000000000,0x0000000000000000),(0x4010000000000000,0x3fe0000000000000)],&[(0x3ff4000000000000,0x3fc0000000000000),(0x3ff7000000000000,0x3fe4000000000000),(0x3ffe000000000000,0x3ff0000000000000),(0x400e800000000000,0x3ffe000000000000),(0x4000800000000000,0xbfc0000000000000),(0x4000000000000000,0x3fe0000000000000),(0x4003800000000000,0x3fec000000000000),(0x4012800000000000,0x3ffa000000000000)],0),
    ("S05",3,2,&[(0x0000000000000000,0x0000000000000000),(0x4000000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x4010000000000000,0x3ff0000000000000),(0x3ff0000000000000,0x3ff0000000000000),(0x4008000000000000,0xbff0000000000000),(0x4014000000000000,0x0000000000000000)],&[(0x3fe4000000000000,0x3ffe000000000000),(0x4017800000000000,0x3fec000000000000),(0x4023c00000000000,0x4016800000000000),(0x3ff4000000000000,0x4002000000000000),(0x4020800000000000,0xbfd0000000000000),(0x402d000000000000,0x4014000000000000)],0),
    ("S06",4,2,&[(0x6590000000000000,0x6560000000000000),(0xe570000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0xe570000000000000,0x0000000000000000),(0x6590000000000000,0x6560000000000000),(0xe570000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0xe570000000000000,0x0000000000000000),(0x6590000000000000,0x6560000000000000),(0xe570000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0xe570000000000000,0x0000000000000000),(0x6590000000000000,0x6560000000000000)],&[(0x6574000000000000,0x6540000000000000),(0x6577000000000000,0x6564000000000000),(0x657e000000000000,0x6570000000000000),(0x658e800000000000,0x657e000000000000),(0x6580800000000000,0xe540000000000000),(0x6580000000000000,0x6560000000000000),(0x6583800000000000,0x656c000000000000),(0x6592800000000000,0x657a000000000000)],0),
    ("S07",4,2,&[(0x1a90000000000000,0x1a60000000000000),(0x9a70000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x9a70000000000000,0x0000000000000000),(0x1a90000000000000,0x1a60000000000000),(0x9a70000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x9a70000000000000,0x0000000000000000),(0x1a90000000000000,0x1a60000000000000),(0x9a70000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x9a70000000000000,0x0000000000000000),(0x1a90000000000000,0x1a60000000000000)],&[(0x1a74000000000000,0x1a40000000000000),(0x1a77000000000000,0x1a64000000000000),(0x1a7e000000000000,0x1a70000000000000),(0x1a8e800000000000,0x1a7e000000000000),(0x1a80800000000000,0x9a40000000000000),(0x1a80000000000000,0x1a60000000000000),(0x1a83800000000000,0x1a6c000000000000),(0x1a92800000000000,0x1a7a000000000000)],0),
    ("S08",2,2,&[(0x3ff0000000000000,0x3fd0000000000000),(0x3ff0000000000000,0x3fd0000000000000),(0x3ff0000000000000,0x3fd0000000000000),(0x3ff0000000001000,0x3fd0000000000000)],&[(0x3ffb000000000000,0x3fe6000000000000),(0x3ffb000000000c00,0x3fe5fffffffff800),(0xbffc000000000000,0x3fe4000000000000),(0xbffbfffffffff800,0x3fe4000000000000)],0),
    ("S09",2,1,&[(0x3ff0000000000000,0x3ff0000000000000),(0x4000000000000000,0x4000000000000000),(0x4000000000000000,0xbff0000000000000),(0x4010000000000000,0xc000000000000000)],&[(0xbfd0000000000000,0x4004000000000000),(0xbfe0000000000000,0x4014000000000000)],1),
    ("S10",2,1,&[(0x7ff8000000000001,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x3ff0000000000000,0x0000000000000000)],&[(0x3ff0000000000000,0x0000000000000000),(0x4000000000000000,0x0000000000000000)],2),
    ("S11",2,1,&[(0x3ff0000000000000,0x0000000000000000),(0x0000000000000000,0xfff0000000000000),(0x0000000000000000,0x0000000000000000),(0x3ff0000000000000,0x0000000000000000)],&[(0x3ff0000000000000,0x0000000000000000),(0x4000000000000000,0x0000000000000000)],2),
    ("S12",2,1,&[(0x3ff0000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x0000000000000000,0x0000000000000000),(0x3ff0000000000000,0x0000000000000000)],&[(0x3ff0000000000000,0x0000000000000000),(0x7ff0000000000000,0x0000000000000000)],2),
    ];
    let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();let path=std::env::temp_dir().join(format!("onl-solves-{}-{stamp}.txt",std::process::id()));let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
    for &(id,n,rhs,aw,bw,mode)in cases{
        let aw=aw.iter().map(|&(re,im)|Complex64V1{re:f64::from_bits(re),im:f64::from_bits(im)}).collect::<Vec<_>>();let bw=bw.iter().map(|&(re,im)|Complex64V1{re:f64::from_bits(re),im:f64::from_bits(im)}).collect::<Vec<_>>();
        let a=ScientificDenseV1::from_complex(ScientificShapeV1::new(&[n as u64,n as u64]).unwrap(),&aw,&mut Control::default()).unwrap();let b=ScientificDenseV1::from_complex(ScientificShapeV1::new(&[n as u64,rhs as u64]).unwrap(),&bw,&mut Control::default()).unwrap();
        let result=(||{let mut solver=ScientificSolveV1::new(&a,&b,&mut Control::default())?;loop{if let SolveProgressV1::Complete{..}=solver.step(1024,&mut Control::default())?{return solver.into_result()}}})();
        if mode==1{assert!(matches!(result,Err(ScientificErrorV1::Singular)),"{id}");writeln!(file,"{id} Singular").unwrap();}
        else if mode==2{assert!(matches!(result,Err(ScientificErrorV1::NonFiniteInput)),"{id}");writeln!(file,"{id} NonFiniteInput").unwrap();}
        else{let x=result.unwrap_or_else(|e|panic!("{id}:{e:?}"));write!(file,"{id}").unwrap();for i in 0..x.len(){let z=x.get(i).unwrap();assert!(z.finite(),"{id}");write!(file," {:016x} {:016x}",z.re.to_bits(),z.im.to_bits()).unwrap();}writeln!(file).unwrap();}
    }
    file.flush().unwrap();drop(file);let oracle=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scientific_oracle.py");let status=std::process::Command::new("/usr/bin/python3").arg(oracle).arg("--solver").arg(&path).env_clear().env("PYTHONNOUSERSITE","1").env("PYTHONDONTWRITEBYTECODE","1").status().unwrap();assert!(status.success(),"independentFraction solveroracle; retained {}",path.display());
}
