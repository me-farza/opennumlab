// SPDX-License-Identifier: GPL-3.0-or-later
use mathlab_engine::rust_program::*;
use mathlab_language::program::*;
use std::{sync::Arc, time::Instant};
fn module(s: &str) -> Arc<ProgramModuleV1> {
    Arc::new(parse_program_v1(501, Arc::from(s)).unwrap())
}
fn run(session: &mut ProgramSessionV1, s: &str) -> ProgramOutcomeV1 {
    session
        .prepare(module(s), Default::default())
        .unwrap()
        .execute()
}
fn success(session: &mut ProgramSessionV1, s: &str) -> ProgramOutcomeV1 {
    let out = run(session, s);
    assert_eq!(out.status, ProgramStatusV1::Success, "{:?}", out.error);
    out
}
fn value(session: &ProgramSessionV1, name: &str) -> ProgramValueV1 {
    session.read_binding(name).unwrap().unwrap()
}
fn scalar(session: &ProgramSessionV1, name: &str) -> f64 {
    value(session, name).scalar().unwrap()
}
#[test]
fn ra_scientific_a() {
    let mut s = ProgramSessionV1::new().unwrap();
    let out = success(
        &mut s,
        "A=[1,2;3,4]; caller=A; result=sum_squares(4);\nfunction total=sum_squares(n)\ntotal=0;\nfor k=1:n\ntotal=total+k*k;\nend\nreturn;\nend\nafter=result+A(1,1);\n",
    );
    assert_eq!(scalar(&s, "result"), 30.0);
    assert_eq!(scalar(&s, "after"), 31.0);
    assert_eq!(value(&s, "caller").doubles().unwrap(), [1., 3., 2., 4.]);
    for name in ["n", "k", "total"] {
        assert!(s.read_binding(name).unwrap().is_none());
    }
    assert_eq!(out.profile, PROGRAM_PROFILE_V1);
    assert_eq!(out.final_context.revision, 4);
}
#[test]
fn ra_scientific_b() {
    let mut s = ProgramSessionV1::new().unwrap();
    success(
        &mut s,
        "x0=1; a=2; x=x0; iterations=0;\nwhile iterations<8 && x>0\nnext=newton_step(x,a); iterations=iterations+1;\nif next==x || ~(next>0)\nbreak;\nelseif iterations<0\ncontinue;\nelse\nx=next;\nend\nend\nfunction y=newton_step(x,a)\ny=(x+a./x)./2;\nreturn;\nend\nanswer=x+newton_step(x,a);\n",
    );
    assert_eq!(scalar(&s, "x").to_bits(), 0x3ff6a09e667f3bcc);
    assert!(scalar(&s, "iterations") <= 8.);
    assert!((scalar(&s, "x") - 2f64.sqrt()).abs() < 2f64.powi(-50));
    assert!(s.read_binding("y").unwrap().is_none());
}
#[test]
fn ra_scientific_c() {
    let mut s = ProgramSessionV1::new().unwrap();
    success(
        &mut s,
        "A=[4,-1;-1,4]; b=[3;3]; x=[0;0]; caller=x; history=zeros(2,8);\nfor k=1:8\n[x,residual_sq]=jacobi(A,b,x);\nfor row=1:2\nhistory(row,k)=x(row);\nend\nend\nfunction [next,residual_sq]=jacobi(A,b,x)\nnext=[(3+x(2))./4;(3+x(1))./4];\nresidual_sq=sum((b-A*next).*(b-A*next));\nreturn;\nend\nfinal=x;\n",
    );
    assert_eq!(
        value(&s, "x")
            .doubles()
            .unwrap()
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>(),
        [0x3fefffe000000000; 2]
    );
    assert_eq!(scalar(&s, "residual_sq").to_bits(), 0x3e32000000000000);
    assert_eq!(value(&s, "caller").doubles().unwrap(), [0., 0.]);
    assert_eq!(value(&s, "history").shape(), [2, 8]);
    assert_eq!(
        value(&s, "history").word(15).unwrap().to_bits(),
        0x3fefffe000000000
    );
    assert!(s.read_binding("next").unwrap().is_none());
}
#[test]
fn rb_frames_arity_outputs_and_recursive_machine() {
    let mut s = ProgramSessionV1::new().unwrap();
    success(
        &mut s,
        "x=9; [a,b]=pair(x); z=unused(); depth=recur(31);\nfunction [y,z]=pair(x)\nx=3;y=x;z=4;\nend\nfunction y=unused(a)\ny=7;\nend\nfunction y=recur(n)\nif n>0\ny=recur(n-1);\nelse\ny=0;\nend\nend\n",
    );
    assert_eq!(scalar(&s, "x"), 9.);
    assert_eq!(scalar(&s, "a"), 3.);
    assert_eq!(scalar(&s, "b"), 4.);
    assert_eq!(scalar(&s, "z"), 7.);
    assert_eq!(scalar(&s, "depth"), 0.);
    let out = run(
        &mut s,
        "prior=1; bad=f(); after=3;\nfunction y=f(x)\ny=x;\nend\n",
    );
    assert_eq!(out.status, ProgramStatusV1::Error);
    let e = out.error.unwrap();
    assert_eq!(e.code, ProgramErrorCodeV1::UnknownBinding);
    assert_eq!(e.call_depth, 1);
    assert_eq!(e.location.source_id, 501);
    assert!(s.read_binding("bad").unwrap().is_none());
    assert_eq!(scalar(&s, "prior"), 1.);
    success(&mut s, "recovered=prior+1;");
    assert_eq!(scalar(&s, "recovered"), 2.);
    for src in [
        "a=f(1,2);\nfunction y=f(x)\ny=x;\nend\n",
        "[a,b]=f();\nfunction y=f\ny=1;\nend\n",
    ] {
        assert_eq!(
            run(&mut s, src).error.unwrap().code,
            ProgramErrorCodeV1::Arity
        );
    }
    let out = run(&mut s, "[new_a,new_b]=f();\nfunction [a,b]=f\na=1;\nend\n");
    assert_eq!(
        out.error.unwrap().code,
        ProgramErrorCodeV1::UnassignedOutput
    );
    assert!(s.read_binding("new_a").unwrap().is_none());
}
#[test]
fn rb_columns_empty_axes_frozen_and_jumps() {
    let mut s = ProgramSessionV1::new().unwrap();
    success(
        &mut s,
        "A=[1 2;3 4]; total=0;\nfor col=A\ntotal=total+sum(col); A(1,1)=99;\nend\ncount=0;\nfor empty=zeros(0,3)\ncount=count+1;\nend\nold=9;\nfor old=zeros(3,0)\nold=0;\nend\nn=0;\nfor k=1:3\nfor j=1:3\nif j==2\ncontinue;\nend\nn=n+1;\nif k==2\nbreak;\nend\nend\nend\n",
    );
    assert_eq!(scalar(&s, "total"), 10.);
    assert_eq!(value(&s, "col").doubles().unwrap(), [2., 4.]);
    assert_eq!(scalar(&s, "count"), 3.);
    assert_eq!(value(&s, "empty").shape(), [0, 1]);
    assert_eq!(value(&s, "old").shape(), [0, 0]);
    assert_eq!(scalar(&s, "n"), 5.);
}
#[test]
fn rb_logicals_shortcircuit_transpose_and_expansion() {
    let mut s = ProgramSessionV1::new().unwrap();
    success(
        &mut s,
        "skip=0 && missing; yes=1 || missing; mask=[1 2 3;4 5 6]>2; turned=mask'; back=turned'; expanded=[1;2]<[1 2 3]; empty=zeros(0,1)<[1 2]; eager=(1&2)|(0&3);\n",
    );
    assert_eq!(value(&s, "skip").class(), ProgramValueClassV1::Logical);
    assert_eq!(value(&s, "skip").logicals().unwrap(), [0]);
    assert_eq!(value(&s, "yes").logicals().unwrap(), [1]);
    assert_eq!(value(&s, "turned").shape(), [3, 2]);
    assert_eq!(value(&s, "back").logicals(), value(&s, "mask").logicals());
    assert_eq!(
        value(&s, "expanded").logicals().unwrap(),
        [0, 0, 1, 0, 1, 1]
    );
    assert_eq!(value(&s, "empty").shape(), [0, 2]);
    assert_eq!(
        run(&mut s, "bad=1 & missing;").status,
        ProgramStatusV1::Error
    );
    assert_eq!(run(&mut s, "bad=[1 2]&&1;").status, ProgramStatusV1::Error);
}
#[test]
fn rb_numeric_seams_indexing_and_power_range() {
    let mut s = ProgramSessionV1::new().unwrap();
    success(
        &mut s,
        "p=1e155^-2; a=[1 2;3 4]; saved=a; a(1,2)=9; selected=a(:,end); s=sum([1e16 1 -1e16]); m=mean([1 2 3]); pr=prod([2 3 4]); powers=[2;3].^[2 3];\n",
    );
    assert_eq!(scalar(&s, "p").to_bits(), 0x000012688b70e62b);
    assert_eq!(scalar(&s, "s"), 1.);
    assert_eq!(scalar(&s, "m"), 2.);
    assert_eq!(scalar(&s, "pr"), 24.);
    assert_eq!(value(&s, "saved").doubles().unwrap(), [1., 3., 2., 4.]);
    assert_eq!(value(&s, "selected").doubles().unwrap(), [9., 4.]);
    assert_eq!(value(&s, "powers").doubles().unwrap(), [4., 9., 8., 27.]);
    let out = run(&mut s, "a(0)=1;");
    assert_eq!(out.status, ProgramStatusV1::Error);
    assert_eq!(value(&s, "a").doubles().unwrap(), [1., 3., 9., 4.]);
}
#[test]
fn rc_prefix_events_multioutput_failure_and_recovery() {
    let mut s = ProgramSessionV1::new().unwrap();
    let out = run(
        &mut s,
        "prior=1; [a,b]=bad(); after=3;\nfunction [a,b]=bad\nlocal=7\na=1; b=missing;\nend\n",
    );
    assert_eq!(out.status, ProgramStatusV1::Error);
    assert_eq!(out.events.len(), 1);
    assert_eq!(out.events[0].name.as_deref(), Some("local"));
    assert_eq!(out.events[0].value.scalar().unwrap(), 7.);
    assert_eq!(out.final_context.revision, 1);
    assert_eq!(out.committed_effect_sequence, 2);
    for name in ["a", "b", "after", "local"] {
        assert!(s.read_binding(name).unwrap().is_none());
    }
    success(&mut s, "recovered=prior+1;");
    assert_eq!(scalar(&s, "recovered"), 2.);
}
#[test]
fn rc_stop_barrier_key_lifecycle_and_recovery() {
    for (cp, expected) in [
        (ProgramCheckpointV1::BeforePublish, 0),
        (ProgramCheckpointV1::AfterBarrier, 1),
        (ProgramCheckpointV1::AfterPublish, 1),
    ] {
        let mut s = ProgramSessionV1::new().unwrap();
        let request = s.prepare(module("x=1; y=2;"), Default::default()).unwrap();
        let handle = request.cancel_handle();
        let mut fired = false;
        let out = request.execute_controlled(|at, h| {
            if at == cp && !fired {
                fired = true;
                assert_eq!(
                    h.request_stop(),
                    ProgramCancelResultV1::Accepted(ProgramCancelReasonV1::UserStop)
                );
            }
        });
        assert_eq!(
            out.status,
            ProgramStatusV1::Cancelled(ProgramCancelReasonV1::UserStop)
        );
        assert_eq!(out.final_context.revision, expected);
        assert_eq!(s.read_binding("x").unwrap().is_some(), expected == 1);
        assert!(s.read_binding("y").unwrap().is_none());
        let mut wrong = handle.key();
        wrong.request_id += 1;
        assert_eq!(
            handle.cancel_for(wrong, ProgramCancelReasonV1::UserStop),
            ProgramCancelResultV1::WrongKey
        );
        success(&mut s, "recovered=3;");
        assert_eq!(scalar(&s, "recovered"), 3.);
    }
}
#[test]
fn rc_deadline_fuel_unsupported_and_recovery() {
    let mut s = ProgramSessionV1::new().unwrap();
    let out = s
        .prepare(
            module("x=1;"),
            ProgramExecutionOptionsV1 {
                deadline: Some(Instant::now()),
                ..Default::default()
            },
        )
        .unwrap()
        .execute();
    assert_eq!(
        out.status,
        ProgramStatusV1::Cancelled(ProgramCancelReasonV1::Deadline)
    );
    let out = s
        .prepare(
            module("prior=1; while 1,prior=prior+1;end\n"),
            ProgramExecutionOptionsV1 {
                primitive_fuel: 10,
                ..Default::default()
            },
        )
        .unwrap()
        .execute();
    assert_eq!(out.status, ProgramStatusV1::Resource);
    assert_eq!(scalar(&s, "prior"), 1.);
    for source in ["function f(varargin)\nend\n", "function varargout=f\nend\n"] {
        assert_eq!(
            run(&mut s, source).error.unwrap().code,
            ProgramErrorCodeV1::UnsupportedFeature
        );
    }
    success(&mut s, "recovered=2;");
}
#[test]
fn rb_outputs_truth_returns_and_exact_integer_ranges() {
    let mut s = ProgramSessionV1::new().unwrap();
    success(
        &mut s,
        "ans=99; f(); one=pair(); counts=counted(4); r=-9007199254740991:9007199254740991:9007199254740990; hits=0; if [1 2],hits=hits+1;end\nif [],hits=9;end\nfor mask=[1 2]>0,logical_col=mask;end\nreturn; never=1;\nfunction f\nreturn;\nend\nfunction [a,b]=pair\na=3;\nend\nfunction y=counted(x,unused)\ny=nargin+nargout;return;y=0;\nend\n",
    );
    assert_eq!(scalar(&s, "ans"), 99.);
    assert_eq!(scalar(&s, "one"), 3.);
    assert_eq!(scalar(&s, "counts"), 2.);
    assert_eq!(value(&s, "r").doubles().unwrap(), [-9007199254740991., 0.]);
    assert_eq!(scalar(&s, "hits"), 1.);
    assert_eq!(
        value(&s, "logical_col").class(),
        ProgramValueClassV1::Logical
    );
    assert!(s.read_binding("never").unwrap().is_none());
    let out = success(
        &mut s,
        "[duplicate,duplicate]=show();\nfunction [a,b]=show\na=1\nb=2\nend\n",
    );
    assert_eq!(scalar(&s, "duplicate"), 2.);
    assert_eq!(out.events.len(), 2);
}
#[test]
fn rc_binding_event_livepayload_and_transition_boundaries() {
    let mut s = ProgramSessionV1::new().unwrap();
    let source = (0..65).map(|i| format!("v{i}=1;\n")).collect::<String>();
    let out = run(&mut s, &source);
    assert_eq!(out.status, ProgramStatusV1::Resource);
    assert_eq!(out.final_context.revision, 64);
    assert!(s.read_binding("v63").unwrap().is_some());
    assert!(s.read_binding("v64").unwrap().is_none());
    let mut s = ProgramSessionV1::new().unwrap();
    let out = run(&mut s, "k=0;while k<1025,k=k+1;1\nend\n");
    assert_eq!(out.status, ProgramStatusV1::Resource);
    assert_eq!(out.events.len(), 1024);
    assert_eq!(scalar(&s, "k"), 1025.);
    let mut s = ProgramSessionV1::new().unwrap();
    let out = run(&mut s, "A=ones(16); k=0;while k<600,k=k+1;A\nend\n");
    assert_eq!(out.status, ProgramStatusV1::Resource);
    assert!(out.events.len() < 600);
    assert!(out.stats.peak_live_payload_bytes <= 1048576);
    assert!(out.stats.peak_live_payload_bytes > 1040000);
    let mut s = ProgramSessionV1::new().unwrap();
    let out = s
        .prepare(
            module("x=1;while 1,end\n"),
            ProgramExecutionOptionsV1 {
                transition_fuel: 20,
                ..Default::default()
            },
        )
        .unwrap()
        .execute();
    assert_eq!(out.status, ProgramStatusV1::Resource);
    assert_eq!(scalar(&s, "x"), 1.);
    assert_eq!(out.stats.transitions, 20);
    success(&mut s, "ok=1;");
    let mut s = ProgramSessionV1::new().unwrap();
    let out = run(
        &mut s,
        "x=f(32);\nfunction y=f(n)\nif n>0,y=f(n-1);else,y=0;end\nend\n",
    );
    assert_eq!(out.status, ProgramStatusV1::Resource);
    assert_eq!(out.error.unwrap().call_depth, 32);
}
#[test]
fn rb_long_names_are_charged_and_empty_selectors_keep_v1_geometry() {
    let mut s = ProgramSessionV1::new().unwrap();
    success(
        &mut s,
        "A=[1 2;3 4]; old=A; empty=A([]); row=A([],1:2); col=A(1:2,[]); A([],1:2)=7;\n",
    );
    assert_eq!(value(&s, "empty").shape(), [0, 0]);
    assert_eq!(value(&s, "row").shape(), [0, 2]);
    assert_eq!(value(&s, "col").shape(), [2, 0]);
    assert_eq!(value(&s, "A").doubles(), value(&s, "old").doubles());
    let revision = s.context().revision;
    assert_eq!(
        run(&mut s, "A([])=[];").error.unwrap().code,
        ProgramErrorCodeV1::UnsupportedFeature
    );
    assert_eq!(s.context().revision, revision);
    let name = format!("f{}", "a".repeat(2047));
    let source = format!("prior=1; result={name}();\nfunction y={name}\ny=1\nend\n");
    let mut s = ProgramSessionV1::new().unwrap();
    let out = s
        .prepare(
            module(&source),
            ProgramExecutionOptionsV1 {
                primitive_fuel: 100,
                ..Default::default()
            },
        )
        .unwrap()
        .execute();
    assert_eq!(out.status, ProgramStatusV1::Resource);
    assert_eq!(out.final_context.revision, 1);
    assert!(out.events.is_empty());
    assert!(s.read_binding("result").unwrap().is_none());
    let source = format!("[{name},{name}]=show();\nfunction [a,b]=show\na=1\nb=2\nend\n");
    let out = success(&mut s, &source);
    assert_eq!(scalar(&s, &name), 2.);
    assert_eq!(out.events.len(), 2);
}
#[test]
fn rb_local_count_names_shadow_builtins_consistently() {
    let mut s = ProgramSessionV1::new().unwrap();
    success(
        &mut s,
        "bare=caller(); applied=nargin(); other=nargout(); bound=boundcaller();\nfunction y=caller\ny=nargin+nargout;\nend\nfunction y=nargin\ny=41;\nend\nfunction y=nargout\ny=42;\nend\nfunction y=boundcaller\nnargin=5;nargout=6;y=nargin+nargout+nargin(1);\nend\n",
    );
    assert_eq!(scalar(&s, "bare"), 83.);
    assert_eq!(scalar(&s, "applied"), 41.);
    assert_eq!(scalar(&s, "other"), 42.);
    assert_eq!(scalar(&s, "bound"), 16.);
}
#[test]
fn rb_native_f05_bare_count_and_implicit_ans() {
    let mut s = ProgramSessionV1::new().unwrap();
    let out = success(
        &mut s,
        "pf_a=1;\n2;\npf_saved_ans=ans;\npf_out=pf_count();\npf_count()\nfunction y=pf_count()\ny=nargout;\nend\n",
    );
    assert_eq!(scalar(&s, "pf_a"), 1.);
    assert_eq!(scalar(&s, "pf_saved_ans"), 2.);
    assert_eq!(scalar(&s, "pf_out"), 1.);
    assert_eq!(scalar(&s, "ans").to_bits(), 0);
    assert_eq!(out.final_context.revision, 5);
    assert_eq!(out.events.len(), 1);
    assert_eq!(out.events[0].name.as_deref(), Some("ans"));
    assert_eq!(out.events[0].value.scalar().unwrap(), 0.);
    assert!(s.read_binding("y").unwrap().is_none());
}
#[test]
fn rb_bare_unset_and_multioutput_are_qualified_project_policy() {
    let mut s = ProgramSessionV1::new().unwrap();
    success(
        &mut s,
        "ans=9;none();keep=ans;many();captured=ans;zero();after_zero=ans;\nfunction y=none\nreturn;\nend\nfunction [a,b]=many\na=nargout;\nend\nfunction zero\nreturn;\nend\n",
    );
    assert_eq!(scalar(&s, "keep"), 9.);
    assert_eq!(scalar(&s, "captured"), 0.);
    assert_eq!(scalar(&s, "after_zero"), 0.);
    let out = run(&mut s, "x=none();\nfunction y=none\nreturn;\nend\n");
    assert_eq!(
        out.error.unwrap().code,
        ProgramErrorCodeV1::UnassignedOutput
    );
}
#[test]
fn rb_native_f15_rightmost_duplicate_slot_and_value_isolation() {
    let mut s = ProgramSessionV1::new().unwrap();
    let out = success(
        &mut s,
        "pf_a=1;\n[pf_a,pf_a]=pf_pair();\nfunction [a,b]=pf_pair()\na=11;\nb=22;\nend\n",
    );
    assert_eq!(scalar(&s, "pf_a").to_bits(), 0x4036000000000000);
    assert_eq!(out.final_context.revision, 2);
    assert_eq!(s.bindings().len(), 1);
    let out = success(
        &mut s,
        "[x,y,x]=three();\nfunction [a,b,c]=three\nentered=1\na=11;b=2;c=22;\nend\n",
    );
    assert_eq!(scalar(&s, "x"), 22.);
    assert_eq!(scalar(&s, "y"), 2.);
    assert_eq!(out.final_context.revision - out.start.revision, 1);
    assert_eq!(out.events.len(), 1);
    assert_eq!(out.events[0].name.as_deref(), Some("entered"));
    assert_eq!(s.bindings().filter(|v| v.name == "x").count(), 1);
    success(
        &mut s,
        "[same,same]=arrays();snapshot=same;same(1)=9;\nfunction [a,b]=arrays\na=[1 2];b=[3 4];\nend\n",
    );
    assert_eq!(value(&s, "same").doubles().unwrap(), [9., 4.]);
    assert_eq!(value(&s, "snapshot").doubles().unwrap(), [3., 4.]);
}
#[test]
fn rc_duplicate_quota_and_failed_rhs_keep_atomic_prefix() {
    let mut s = ProgramSessionV1::new().unwrap();
    let mut source = (0..63).map(|i| format!("v{i}=0;\n")).collect::<String>();
    source.push_str("[last,last]=pair();\nfunction [a,b]=pair\na=11;b=22;\nend\n");
    success(&mut s, &source);
    assert_eq!(s.bindings().len(), 64);
    assert_eq!(scalar(&s, "last"), 22.);
    success(
        &mut s,
        "[v0,v0]=pair();\nfunction [a,b]=pair\na=11;b=22;\nend\n",
    );
    assert_eq!(s.bindings().len(), 64);
    assert_eq!(scalar(&s, "v0"), 22.);
    let mut s = ProgramSessionV1::new().unwrap();
    let out = run(
        &mut s,
        "x=9;[x,x]=bad();\nfunction [a,b]=bad\nentered=1\na=11;\nend\n",
    );
    assert_eq!(
        out.error.unwrap().code,
        ProgramErrorCodeV1::UnassignedOutput
    );
    assert_eq!(out.final_context.revision, 1);
    assert_eq!(scalar(&s, "x"), 9.);
    assert_eq!(out.events.len(), 1);
    assert_eq!(out.events[0].name.as_deref(), Some("entered"));
}
#[test]
fn rb_f16_rejects_before_any_script_effect_and_preserves_scopes() {
    let mut s = ProgramSessionV1::new().unwrap();
    success(&mut s, "existing=5;");
    let before = s.context();
    let source = "pf_forward=sum(1);\nfunction y=sum(x)\ny=x+100;\nend\npf_late=sum(2);\nsum=[7,8];\npf_index=sum(2);\n";
    let error = parse_program_v1(9, Arc::from(source)).unwrap_err();
    assert_eq!(error.code, ProgramParseErrorCodeV1::DuplicateName);
    assert_eq!(error.span.start_line, 6);
    assert_eq!(s.context(), before);
    assert!(s.read_binding("pf_forward").unwrap().is_none());
    success(
        &mut s,
        "pf_forward=sum(1);\nfunction y=sum(x)\ny=x+100;\nend\npf_late=sum(2);\n",
    );
    assert_eq!(scalar(&s, "pf_forward"), 101.);
    assert_eq!(scalar(&s, "pf_late"), 102.);
    success(
        &mut s,
        "v=caller();\nfunction y=caller\nsum=[7 8];y=sum(2);\nend\nfunction y=sum(x)\ny=x+100;\nend\n",
    );
    assert_eq!(scalar(&s, "v"), 8.);
}

#[test]
fn rb_native_g24_signed_power_chain_and_grouping() {
    let mut s = ProgramSessionV1::new().unwrap();
    success(&mut s, "pf_unparenthesized=2^-2^3;pf_left=(2^-2)^3;pf_grouped=2^-(2^3); leading=-2^2; signed_after=2^3^-2; transposed=2^-2^3';element=2.^-2.^3;");
    for name in ["pf_unparenthesized", "pf_left", "signed_after", "transposed", "element"] {
        assert_eq!(scalar(&s, name).to_bits(), 0x3f90000000000000);
    }
    assert_eq!(scalar(&s, "pf_grouped").to_bits(), 0x3f70000000000000);
    assert_eq!(scalar(&s, "leading"), -4.);
}
#[test]
fn rb_derived_nan_conditions_fail_at_condition_and_keep_prefix() {
    for (control, end) in [("if", "else\nwrong=2;\nend"), ("while", "end")] {
        let source = format!("before=7;\n{control} 0./0\nwrong=1;\n{end}\nafter=9;");
        let mut s = ProgramSessionV1::new().unwrap();
        let out = run(&mut s, &source);
        assert_eq!(out.status, ProgramStatusV1::Error);
        let error = out.error.unwrap();
        assert_eq!(error.code, ProgramErrorCodeV1::Type);
        assert_eq!(&source[error.location.span.start..error.location.span.end], "0./0");
        assert_eq!(error.location.span.start_line, 2);
        assert_eq!(out.final_context.revision, 1);
        assert_eq!(out.completed_leaf_statement_count, 1);
        assert_eq!(scalar(&s, "before"), 7.);
        assert!(s.read_binding("wrong").unwrap().is_none());
        assert!(s.read_binding("after").unwrap().is_none());
        success(&mut s, "recovered=1;");
    }
    // Full-array NaN detection, including after zero, is explicit project policy.
    let mut s = ProgramSessionV1::new().unwrap();
    assert_eq!(run(&mut s, "if [0 0./0]\nx=1;\nend").error.unwrap().code, ProgramErrorCodeV1::Type);
    success(&mut s, "if [1 2]\nx=1;\nend\nif []\nx=9;\nend");
    assert_eq!(scalar(&s, "x"), 1.);
}
#[test]
fn rb_native_g07_g08_zero_axis_loop_bindings() {
    let mut s = ProgramSessionV1::new().unwrap();
    let out = success(&mut s, "k=7;count=0;\nfor k=zeros(3,0)\ncount=count+1;\nend\nfor fresh=zeros(0,0)\ncount=count+1;\nend\nfor column=zeros(0,3)\ncount=count+1;\nend");
    assert_eq!(value(&s, "k").shape(), [0, 0]);
    assert_eq!(value(&s, "fresh").shape(), [0, 0]);
    assert_eq!(value(&s, "column").shape(), [0, 1]);
    assert_eq!(scalar(&s, "count"), 3.);
    assert!(out.events.is_empty());
    assert_eq!(out.final_context.revision, 10);
    let out = run(&mut s, "saved=8;for saved=missing\nwrong=1;end");
    assert_eq!(out.error.unwrap().code, ProgramErrorCodeV1::UnknownBinding);
    assert_eq!(scalar(&s, "saved"), 8.);
}
#[test]
fn rb_native_g16_g18_supported_bare_and_output_counts() {
    let mut s = ProgramSessionV1::new().unwrap();
    let out = success(&mut s, "pf_before=16;first_unset();pf_after=7;\nfunction [a,b]=first_unset()\nb=22;seen=nargout\nend");
    assert!(s.read_binding("ans").unwrap().is_none());
    assert_eq!(out.events.len(), 1);
    assert_eq!(out.events[0].value.scalar().unwrap(), 0.);
    let out = success(&mut s, "many();pf_one=many();[pf_a,pf_b]=many();pf_after=18;\nfunction [a,b]=many()\nseen=nargout\na=11;b=22;end");
    assert_eq!(out.events.iter().map(|e| e.value.scalar().unwrap()).collect::<Vec<_>>(), [0., 1., 2.]);
    for name in ["ans", "pf_one", "pf_a"] { assert_eq!(scalar(&s, name), 11.); }
    assert_eq!(scalar(&s, "pf_b"), 22.);
}
