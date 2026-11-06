// SPDX-License-Identifier: GPL-3.0-or-later
use mathlab_language::program::*;
use std::sync::Arc;
fn sources(texts: &[&str]) -> Vec<ClassSourceV1> {
    texts
        .iter()
        .enumerate()
        .map(|(i, &text)| ClassSourceV1 {
            source_id: 101 + i as u64,
            text: Arc::from(text),
        })
        .collect()
}
fn graph(texts: &[&str]) -> ClassProgramGraphV1 {
    parse_class_graph_v1(sources(texts)).unwrap()
}
fn failure(texts: &[&str], code: ClassParseErrorCodeV1) -> ClassParseErrorV1 {
    let e = parse_class_graph_v1(sources(texts)).unwrap_err();
    assert_eq!(e.code, code);
    e
}
#[test]
fn original_foundation_source_graphs_are_admitted_without_rewriting() {
    macro_rules! fixture {
        ($case:literal, $file:literal) => {
            include_str!(concat!(
                "../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/",
                $case,
                "/",
                $file,
                ".m"
            ))
        };
    }
    let cases: &[&[&str]] = &[
        &[fixture!("J01", "J01"), fixture!("J01", "Jv01")],
        &[fixture!("J02", "J02"), fixture!("J02", "Jh02")],
        &[
            fixture!("J03", "J03"),
            fixture!("J03", "Jg03"),
            fixture!("J03", "Jw03"),
        ],
        &[fixture!("J04", "J04"), fixture!("J04", "Jh04")],
        &[fixture!("J05", "J05"), fixture!("J05", "Jv05")],
        &[
            fixture!("J06", "J06"),
            fixture!("J06", "Jb06"),
            fixture!("J06", "Jd06"),
        ],
    ];
    for texts in cases {
        let g = graph(texts);
        assert_eq!(g.sources().len(), texts.len());
        assert_eq!(g.classes().len(), texts.len() - 1);
        assert!(g.functions().is_empty());
        for n in g.nodes() {
            let text = &texts[n.source_slot as usize][n.span.start..n.span.end];
            assert_eq!(
                n.span.start_line,
                1 + texts[n.source_slot as usize][..n.span.start]
                    .bytes()
                    .filter(|&b| b == b'\n')
                    .count()
            );
            if let ClassNodeKindV1::Member { name, .. } = n.kind {
                assert!(!g.text(name).unwrap().contains('.'));
            }
            assert!(
                !text.is_empty()
                    || matches!(
                        n.kind,
                        ClassNodeKindV1::Common(ProgramNodeKindV1::Block { .. })
                    )
            );
        }
    }
}
#[test]
fn accepted_call_sources_keep_visible_constructor_parameters_and_local_functions() {
    macro_rules! fixture {
        ($case:literal, $file:literal) => {
            include_str!(concat!(
                "../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/",
                $case,
                "/",
                $file,
                ".m"
            ))
        };
    }
    for texts in [
        [fixture!("K01", "K01"), fixture!("K01", "Kc01")],
        [fixture!("K02", "K02"), fixture!("K02", "Km02")],
        [fixture!("K03", "K03"), fixture!("K03", "Ks03")],
    ] {
        let g = graph(&texts);
        assert_eq!(g.functions().len(), 1);
        let constructor = g.constructor(0).unwrap();
        let ProgramNodeKindV1::Function {
            inputs: Some(list),
            outputs: Some(output),
            ..
        } = g.common_kind(constructor.function).unwrap()
        else {
            panic!()
        };
        let ProgramNodeKindV1::List { item, next: None } = g.common_kind(list).unwrap() else {
            panic!()
        };
        assert_eq!(g.text(item), Some("reading"));
        assert!(matches!(
            g.common_kind(output),
            Some(ProgramNodeKindV1::List { next: None, .. })
        ));
        assert!(!g.functions().contains(&constructor.function));
    }
    let e = failure(
        &[
            fixture!("K04", "K04"),
            fixture!("K04", "Kg04"),
            fixture!("K04", "Kw04"),
        ],
        ClassParseErrorCodeV1::Unsupported,
    );
    assert_eq!(e.source_id, 102);
    assert_eq!(
        &fixture!("K04", "Kg04")[e.span.start..e.span.end],
        "persistent"
    );
}
#[test]
fn inheritance_flattens_properties_and_preserves_declaring_class_and_lookup() {
    let g = graph(&[
        "x=Leaf; y=x.value();\n",
        "classdef Leaf < Middle\nproperties\nQ = [1 2; 3 4]\nend\nmethods\nfunction r=value(obj)\nr=obj.Q;\nend\nend\nend\n",
        "classdef Root < handle\nproperties\nP = sparse([1 0;0 2])\nend\nmethods\nfunction r=value(obj)\nr=obj.P;\nend\nfunction r=inherited(obj)\nr=obj.P;\nend\nend\nend\n",
        "classdef Middle < Root\nend\n",
    ]);
    assert_eq!(g.classes()[0].base, Some(2));
    assert!(g.classes().iter().all(|c| c.is_handle));
    assert_eq!(g.classes()[0].properties.len(), 2);
    let p = g.inherited_property(0, 0).unwrap();
    assert_eq!(p.declaring_class, 1);
    assert_eq!(g.source_id_for(p.name), Some(103));
    assert!(matches!(
        g.common_kind(p.initial.unwrap()),
        Some(ProgramNodeKindV1::Apply { .. })
    ));
    assert_eq!(g.property_index(0, "P"), Some(0));
    assert_eq!(g.property_index(0, "Q"), Some(1));
    assert_eq!(g.method(0, "value").unwrap().declaring_class, 0);
    assert_eq!(g.method(0, "inherited").unwrap().declaring_class, 1);
    assert_eq!(g.class_index("Leaf"), Some(0));
    assert!(g.constructor(0).is_none());
}
#[test]
fn default_expressions_and_member_chains_share_the_ordinary_grammar() {
    let g = graph(&[
        "a=Box; a.Child.Data(1)=3; x=a.Child.measure([1 2].^2)';\n",
        "classdef Box\nproperties\nChild = ChildClass(2+3*4)\nData = [1 .5; 3 4].^2\nend\nend\n",
    ]);
    assert!(
        g.nodes()
            .iter()
            .filter(|n| matches!(n.kind, ClassNodeKindV1::Member { .. }))
            .count()
            >= 4
    );
    let p = &g.classes()[0].properties[0];
    assert!(matches!(
        g.common_kind(p.initial.unwrap()),
        Some(ProgramNodeKindV1::Apply { .. })
    ));
    assert!(g.nodes().iter().any(|n| matches!(
        n.kind,
        ClassNodeKindV1::Common(ProgramNodeKindV1::Binary {
            operator: BinaryOperatorV1::Multiply,
            ..
        })
    )));
}
#[test]
fn duplicate_names_bases_and_inherited_collisions_are_preflight_errors() {
    use ClassParseErrorCodeV1::*;
    failure(&["", "classdef A\nend", "classdef A\nend"], DuplicateName);
    let e = failure(&["", "classdef A < Missing\nend"], UnknownBase);
    assert_eq!(e.source_id, 102);
    assert_eq!(e.span.start, "classdef A < ".len());
    failure(
        &["", "classdef A < B\nend", "classdef B < A\nend"],
        InheritanceCycle,
    );
    failure(&["", "classdef A < A\nend"], InheritanceCycle);
    failure(
        &["", "classdef A\nproperties\nX\nX\nend\nend"],
        DuplicateName,
    );
    failure(
        &[
            "",
            "classdef A\nproperties\nX\nend\nend",
            "classdef B < A\nproperties\nX\nend\nend",
        ],
        DuplicateName,
    );
    failure(&["", "classdef A\nmethods\nfunction x=f(obj)\nx=1;\nend\nfunction x=f(obj)\nx=2;\nend\nend\nend"], DuplicateName);
    failure(
        &[
            "",
            "classdef A\nproperties\nf\nend\nmethods\nfunction x=f(obj)\nx=1;\nend\nend\nend",
        ],
        DuplicateName,
    );
}
#[test]
fn class_names_may_be_variables_but_free_function_ambiguity_is_rejected() {
    graph(&["A=[7 8];x=A(2);\n", "classdef A\nend"]);
    failure(
        &["function y=A(x)\ny=x;\nend\n", "classdef A\nend"],
        ClassParseErrorCodeV1::DuplicateName,
    );
    for script in [
        "f=1;",
        "f(1)=2;",
        "[f,x]=pair();",
        "for f=1:2\nend",
        "if 0\nf=2;\nend",
    ] {
        let text = format!("{script}\nfunction y=f(x)\ny=x;\nend\n");
        failure(&[&text], ClassParseErrorCodeV1::DuplicateName);
    }
    graph(&["function y=f(x)\ny=x;\nend\nfunction y=other\nf=2;y=f;\nend\n"]);
}
#[test]
fn unsupported_features_are_explicit_and_source_local() {
    for module in [
        "classdef (Abstract) A\nend",
        "classdef A < B & C\nend",
        "classdef A\nproperties (Dependent)\nX\nend\nend",
        "classdef A\nproperties\nX double\nend\nend",
        "classdef A\nmethods (Static)\nend\nend",
        "classdef A\nevents\nChanged\nend\nend",
        "classdef A\nmethods\nfunction obj=A\npersistent count\nend\nend\nend",
        "classdef A\nmethods\nfunction f\nend\nend\nend",
        "classdef A\nmethods\nfunction A\nend\nend\nend",
        "classdef A\nend\nclassdef B\nend",
    ] {
        let e = failure(&["", module], ClassParseErrorCodeV1::Unsupported);
        assert_eq!(e.source_id, 102);
        assert!(e.span.end <= module.len());
    }
    failure(&["classdef A\nend"], ClassParseErrorCodeV1::Unsupported);
}
#[test]
fn source_admission_bounds_and_immutable_identity() {
    let mut duplicate = sources(&["", "classdef A\nend"]);
    duplicate[1].source_id = duplicate[0].source_id;
    assert_eq!(
        parse_class_graph_v1(duplicate).unwrap_err().code,
        ClassParseErrorCodeV1::DuplicateName
    );
    assert_eq!(
        parse_class_graph_v1(Vec::new()).unwrap_err().code,
        ClassParseErrorCodeV1::Resource
    );
    let large = " ".repeat(MAX_PROGRAM_SOURCE_BYTES_V1 + 1);
    failure(&[&large], ClassParseErrorCodeV1::Resource);
    let excessive: Vec<_> = (0..MAX_CLASS_SOURCES_V1 + 1)
        .map(|i| ClassSourceV1 {
            source_id: i as u64,
            text: Arc::from(""),
        })
        .collect();
    assert_eq!(
        parse_class_graph_v1(excessive).unwrap_err().code,
        ClassParseErrorCodeV1::Resource
    );
    let input = sources(&["x=1;\n"]);
    let held = input[0].text.clone();
    let g = parse_class_graph_v1(input).unwrap();
    assert!(Arc::ptr_eq(&held, &g.sources()[0].text));
    assert_eq!(g.sources()[0].source_id, 101);
}
#[test]
fn cancellation_at_every_completed_parse_poll_returns_no_graph() {
    let texts = [
        "a=Child; a.P=2;\n",
        "classdef Child < Base\nend\n",
        "classdef Base\nproperties\nP=1+2\nend\nend\n",
    ];
    let mut polls = 0;
    let g = parse_class_graph_controlled_v1(sources(&texts), || {
        polls += 1;
        false
    })
    .unwrap();
    assert_eq!(g.stats().polls, polls);
    for stop in 1..=polls {
        let mut at = 0;
        let e = parse_class_graph_controlled_v1(sources(&texts), || {
            at += 1;
            at == stop
        })
        .unwrap_err();
        assert_eq!(e.code, ClassParseErrorCodeV1::Cancelled);
        assert_eq!(at, stop);
        assert!(texts[(e.source_id - 101) as usize]
            .get(e.span.start..e.span.end)
            .is_some());
    }
}
#[test]
fn ordinary_ast_and_grammar_statistics_remain_the_same() {
    for text in [
        "x=[1 .5;3 4].^2; y=x'; z=x.';\n",
        "x=0;for k=1:2:9\nif k>3\nx=x+k;\nelse\nx=x-1;\nend\nend\n",
        "y=local(3);\nfunction y=local(x)\ny=-(x^2)+1;\nend\n",
    ] {
        let old = parse_program_v1(101, Arc::from(text)).unwrap();
        let new = graph(&[text]);
        assert_eq!(old.nodes().len(), new.nodes().len());
        for (a, b) in old.nodes().iter().zip(new.nodes()) {
            assert_eq!(a.span, b.span);
            let ClassNodeKindV1::Common(kind) = b.kind else {
                panic!()
            };
            assert_eq!(a.kind, kind);
        }
        let a = old.stats();
        let b = new.stats();
        assert_eq!(
            (
                a.source_bytes,
                a.tokens,
                a.nodes,
                a.statements,
                a.numeric_literals,
                a.functions,
                a.max_nesting
            ),
            (
                b.source_bytes,
                b.tokens,
                b.nodes,
                b.statements,
                b.numeric_literals,
                b.functions,
                b.max_nesting
            )
        );
    }
    let error = parse_program_v1(101, Arc::from("a.b=1;")).unwrap_err();
    assert_eq!(error.code, ProgramParseErrorCodeV1::UnsupportedFeature);
    assert_eq!(error.span.start, 1);
}

#[test]
fn exact_property_and_aggregate_source_caps_are_enforced() {
    let module = |name: &str, n: usize| {
        let mut text = format!("classdef {name}\nproperties\n");
        for i in 0..n {
            text.push_str(&format!("P{i}=1\n"));
        }
        text.push_str("end\nend\n");
        text
    };
    let at = module("At", MAX_CLASS_PROPERTIES_V1);
    assert_eq!(
        graph(&["", &at]).classes()[0].properties.len(),
        MAX_CLASS_PROPERTIES_V1
    );
    let over = module("Over", MAX_CLASS_PROPERTIES_V1 + 1);
    failure(&["", &over], ClassParseErrorCodeV1::Resource);
    failure(
        &[
            "",
            &at,
            "classdef Child < At\nproperties\nExtra\nend\nend\n",
        ],
        ClassParseErrorCodeV1::Resource,
    );
    let mut full = String::from("classdef Full\nend\n");
    full.push_str(&" ".repeat(MAX_PROGRAM_SOURCE_BYTES_V1 - full.len()));
    let too_many_bytes = [full.as_str(); 5];
    failure(&too_many_bytes, ClassParseErrorCodeV1::Resource);
    let entry = "x=0;\n".repeat(MAX_PROGRAM_STATEMENTS_V1 + 1);
    failure(&[&entry], ClassParseErrorCodeV1::Resource);
}
#[test]
fn member_locations_are_local_to_the_declaring_method_source() {
    let module =
        "classdef Located\nmethods\nfunction obj=Located(x)\nobj.Value=x;\nend\nend\nend\n";
    let g = graph(&["v=Located(3);\n", module]);
    let member = g
        .nodes()
        .iter()
        .find_map(|n| {
            if let ClassNodeKindV1::Member { receiver, name } = n.kind {
                if g.text(name) == Some("Value") {
                    return Some((n, receiver, name));
                }
            }
            None
        })
        .unwrap();
    assert_eq!(member.0.source_slot, 1);
    assert_eq!(member.0.span.start_line, 4);
    assert_eq!(&module[member.0.span.start..member.0.span.end], "obj.Value");
    assert_eq!(g.source_id_for(member.1), Some(102));
    assert_eq!(g.source_id_for(member.2), Some(102));
    assert_eq!(g.text(member.1), Some("obj"));
}

#[test]
fn user_lifecycle_and_operator_hooks_are_outside_class_v1() {
    for hook in [
        "delete", "isvalid", "subsref", "subsasgn", "plus", "mtimes", "eq", "ne", "saveobj",
    ] {
        let module = format!("classdef A\nmethods\nfunction x={hook}(obj)\nx=1;\nend\nend\nend\n");
        let e = failure(&["", &module], ClassParseErrorCodeV1::Unsupported);
        assert_eq!(&module[e.span.start..e.span.end], hook);
    }
    failure(
        &[
            "",
            "classdef A\nmethods\nfunction x=get.Value(obj)\nx=1;\nend\nend\nend\n",
        ],
        ClassParseErrorCodeV1::Unsupported,
    );
}
