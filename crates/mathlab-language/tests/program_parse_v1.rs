// SPDX-License-Identifier: GPL-3.0-or-later
use mathlab_language::program::*;
use std::sync::Arc;
fn parse(s: &str) -> ProgramModuleV1 {
    parse_program_v1(91, Arc::from(s)).unwrap()
}
fn refused(s: &str, code: ProgramParseErrorCodeV1) {
    let e = parse_program_v1(91, Arc::from(s)).unwrap_err();
    assert_eq!(e.source_id, 91);
    assert_eq!(e.code, code, "{s}: {e}");
    assert!(e.span.start <= e.span.end && e.span.end <= s.len());
}
fn expression(m: &ProgramModuleV1) -> NodeIdV1 {
    let ProgramNodeKindV1::Block {
        statements: Some(list),
    } = m.node(m.script()).unwrap().kind
    else {
        panic!()
    };
    let ProgramNodeKindV1::List { item, .. } = m.node(list).unwrap().kind else {
        panic!()
    };
    let ProgramNodeKindV1::ExpressionStatement { expression, .. } = m.node(item).unwrap().kind
    else {
        panic!()
    };
    expression
}
fn binary(m: &ProgramModuleV1, id: NodeIdV1, op: BinaryOperatorV1) -> (NodeIdV1, NodeIdV1) {
    let ProgramNodeKindV1::Binary {
        operator,
        left,
        right,
    } = m.node(id).unwrap().kind
    else {
        panic!("{:?}", m.node(id))
    };
    assert_eq!(operator, op);
    (left, right)
}
#[test]
fn pa_whole_file_functions_and_control() {
    assert_scientific_fixtures();
    let s = "a=f(3);\nfunction [y z]=f(x)\ny=0; z=x;\nfor k=1:2:x\nwhile y<x\nif k>2\nbreak;\nelseif k==2\ncontinue;\nelse\ny=y+1;\nend\nend\nend\nreturn;\nend\nb=g(a);\nfunction g\nreturn\nend\n";
    let m = parse(s);
    assert_eq!(m.functions().len(), 2);
    assert_eq!(m.source(), s);
    assert_eq!(m.source_id(), 91);
    assert!(
        m.nodes()
            .iter()
            .any(|n| matches!(n.kind, ProgramNodeKindV1::If { .. }))
    );
    for n in m.nodes() {
        assert!(m.source_text(n.span).is_some());
    }
    assert!(m.node(m.script()).is_some());
}

// Complete original frontend fixtures derived from STAGED_PLAN P-A/P-B/P-C.
// Their numerical expectations belong to the separately gated interpreter.
const SCIENTIFIC_A: &str = "A=[1,2;3,4]; caller=A; result=sum_squares(4);\nfunction total=sum_squares(n)\ntotal=0;\nfor k=1:n\ntotal=total+k*k;\nend\nreturn;\nend\nafter=result+A(1,1);\n";
const SCIENTIFIC_B: &str = "x0=1; a=2; x=x0; iterations=0;\nwhile iterations<8 && x>0\nnext=newton_step(x,a); iterations=iterations+1;\nif next==x || ~(next>0)\nbreak;\nelseif iterations<0\ncontinue;\nelse\nx=next;\nend\nend\nfunction y=newton_step(x,a)\ny=(x+a./x)./2;\nreturn;\nend\nanswer=x+newton_step(x,a);\n";
const SCIENTIFIC_C: &str = "A=[4,-1;-1,4]; b=[3;3]; x=[0;0]; caller=x;\nfor k=1:8\n[x,residual_sq]=jacobi(A,b,x);\nfor row=1:2\nhistory(row,k)=x(row);\nend\nend\nfunction [next,residual_sq]=jacobi(A,b,x)\nnext=[(3+x(2))./4;(3+x(1))./4];\nresidual_sq=sum((b-A*next).*(b-A*next));\nreturn;\nend\nfinal=x;\n";

fn children(m: &ProgramModuleV1, mut list: Option<NodeIdV1>) -> Vec<NodeIdV1> {
    let mut out = Vec::new();
    while let Some(id) = list {
        let ProgramNodeKindV1::List { item, next } = m.node(id).unwrap().kind else {
            panic!()
        };
        out.push(item);
        list = next;
    }
    out
}
fn block_items(m: &ProgramModuleV1, id: NodeIdV1) -> Vec<NodeIdV1> {
    let ProgramNodeKindV1::Block { statements } = m.node(id).unwrap().kind else {
        panic!()
    };
    children(m, statements)
}
fn text(m: &ProgramModuleV1, id: NodeIdV1) -> &str {
    m.source_text(m.node(id).unwrap().span).unwrap()
}
fn names(m: &ProgramModuleV1, list: Option<NodeIdV1>) -> Vec<&str> {
    children(m, list)
        .into_iter()
        .map(|id| {
            assert!(matches!(m.node(id).unwrap().kind, ProgramNodeKindV1::Name));
            text(m, id)
        })
        .collect()
}
fn call(m: &ProgramModuleV1, id: NodeIdV1, name: &str, args: &[&str]) {
    let ProgramNodeKindV1::Apply { callee, arguments } = m.node(id).unwrap().kind else {
        panic!()
    };
    assert_eq!(text(m, callee), name);
    assert_eq!(
        children(m, arguments)
            .into_iter()
            .map(|id| text(m, id))
            .collect::<Vec<_>>(),
        args
    );
}
fn assignment(m: &ProgramModuleV1, id: NodeIdV1, name: &str) -> NodeIdV1 {
    let ProgramNodeKindV1::Assignment {
        targets: AssignmentTargetsV1::Single(target),
        value,
        ..
    } = m.node(id).unwrap().kind
    else {
        panic!()
    };
    assert_eq!(text(m, target), name);
    value
}
fn scientific_connections() {
    use BinaryOperatorV1 as B;
    use ProgramNodeKindV1 as N;
    let m = parse(SCIENTIFIC_A);
    let script = block_items(&m, m.script());
    call(
        &m,
        assignment(&m, script[2], "result"),
        "sum_squares",
        &["4"],
    );
    let N::Function {
        name,
        inputs,
        outputs,
        body,
    } = m.node(m.functions()[0]).unwrap().kind
    else {
        panic!()
    };
    assert_eq!(text(&m, name), "sum_squares");
    assert_eq!(names(&m, inputs), ["n"]);
    assert_eq!(names(&m, outputs), ["total"]);
    let body = block_items(&m, body);
    assert_eq!(text(&m, assignment(&m, body[0], "total")), "0");
    let N::For {
        variable,
        iterable,
        body,
    } = m.node(body[1]).unwrap().kind
    else {
        panic!()
    };
    assert_eq!(text(&m, variable), "k");
    let N::Range {
        start,
        step: None,
        stop,
    } = m.node(iterable).unwrap().kind
    else {
        panic!()
    };
    assert_eq!((text(&m, start), text(&m, stop)), ("1", "n"));
    let inner = block_items(&m, body);
    let (l, r) = binary(&m, assignment(&m, inner[0], "total"), B::Add);
    assert_eq!(text(&m, l), "total");
    let (l, r) = binary(&m, r, B::Multiply);
    assert_eq!((text(&m, l), text(&m, r)), ("k", "k"));
    let m = parse(SCIENTIFIC_B);
    let script = block_items(&m, m.script());
    let N::While { condition, body } = m.node(script[4]).unwrap().kind else {
        panic!()
    };
    let (l, r) = binary(&m, condition, B::ShortCircuitAnd);
    let (a, b) = binary(&m, l, B::Less);
    assert_eq!((text(&m, a), text(&m, b)), ("iterations", "8"));
    let (a, b) = binary(&m, r, B::Greater);
    assert_eq!((text(&m, a), text(&m, b)), ("x", "0"));
    let inner = block_items(&m, body);
    call(
        &m,
        assignment(&m, inner[0], "next"),
        "newton_step",
        &["x", "a"],
    );
    let N::If {
        branches,
        otherwise: Some(otherwise),
    } = m.node(inner[2]).unwrap().kind
    else {
        panic!()
    };
    let branch = children(&m, branches);
    assert_eq!(branch.len(), 2);
    let N::IfBranch { condition, body } = m.node(branch[0]).unwrap().kind else {
        panic!()
    };
    let (l, _) = binary(&m, condition, B::ShortCircuitOr);
    let (a, b) = binary(&m, l, B::Equal);
    assert_eq!((text(&m, a), text(&m, b)), ("next", "x"));
    assert!(matches!(
        m.node(block_items(&m, body)[0]).unwrap().kind,
        N::Break
    ));
    assert_eq!(
        text(&m, assignment(&m, block_items(&m, otherwise)[0], "x")),
        "next"
    );
    let N::Function {
        name,
        inputs,
        outputs,
        body,
    } = m.node(m.functions()[0]).unwrap().kind
    else {
        panic!()
    };
    assert_eq!(text(&m, name), "newton_step");
    assert_eq!(names(&m, inputs), ["x", "a"]);
    assert_eq!(names(&m, outputs), ["y"]);
    let value = assignment(&m, block_items(&m, body)[0], "y");
    let (l, r) = binary(&m, value, B::ElementDivide);
    assert_eq!(text(&m, r), "2");
    let N::Group { value } = m.node(l).unwrap().kind else {
        panic!()
    };
    let (l, r) = binary(&m, value, B::Add);
    assert_eq!(text(&m, l), "x");
    let (l, r) = binary(&m, r, B::ElementDivide);
    assert_eq!((text(&m, l), text(&m, r)), ("a", "x"));
    let m = parse(SCIENTIFIC_C);
    let script = block_items(&m, m.script());
    let N::For {
        variable,
        iterable,
        body,
    } = m.node(script[4]).unwrap().kind
    else {
        panic!()
    };
    assert_eq!(text(&m, variable), "k");
    assert_eq!(text(&m, iterable), "1:8");
    let outer = block_items(&m, body);
    let N::Assignment {
        targets: AssignmentTargetsV1::Multiple(targets),
        value,
        ..
    } = m.node(outer[0]).unwrap().kind
    else {
        panic!()
    };
    assert_eq!(names(&m, Some(targets)), ["x", "residual_sq"]);
    call(&m, value, "jacobi", &["A", "b", "x"]);
    let N::For {
        variable,
        iterable,
        body,
    } = m.node(outer[1]).unwrap().kind
    else {
        panic!()
    };
    assert_eq!(text(&m, variable), "row");
    assert_eq!(text(&m, iterable), "1:2");
    let N::Assignment {
        targets: AssignmentTargetsV1::Single(target),
        value,
        ..
    } = m.node(block_items(&m, body)[0]).unwrap().kind
    else {
        panic!()
    };
    call(&m, target, "history", &["row", "k"]);
    call(&m, value, "x", &["row"]);
    let N::Function {
        name,
        inputs,
        outputs,
        body,
    } = m.node(m.functions()[0]).unwrap().kind
    else {
        panic!()
    };
    assert_eq!(text(&m, name), "jacobi");
    assert_eq!(names(&m, inputs), ["A", "b", "x"]);
    assert_eq!(names(&m, outputs), ["next", "residual_sq"]);
    let statements = block_items(&m, body);
    let next = assignment(&m, statements[0], "next");
    let N::Matrix { rows } = m.node(next).unwrap().kind else {
        panic!()
    };
    let rows = children(&m, rows);
    assert_eq!(rows.len(), 2);
    for (row, index) in rows.into_iter().zip(["2", "1"]) {
        let N::Row { items } = m.node(row).unwrap().kind else {
            panic!()
        };
        let items = children(&m, items);
        assert_eq!(items.len(), 1);
        let (l, r) = binary(&m, items[0], B::ElementDivide);
        assert_eq!(text(&m, r), "4");
        let N::Group { value } = m.node(l).unwrap().kind else {
            panic!()
        };
        let (l, r) = binary(&m, value, B::Add);
        assert_eq!(text(&m, l), "3");
        call(&m, r, "x", &[index]);
    }
    let value = assignment(&m, statements[1], "residual_sq");
    let N::Apply { callee, arguments } = m.node(value).unwrap().kind else {
        panic!()
    };
    assert_eq!(text(&m, callee), "sum");
    let args = children(&m, arguments);
    assert_eq!(args.len(), 1);
    let (l, r) = binary(&m, args[0], B::ElementMultiply);
    assert_eq!((text(&m, l), text(&m, r)), ("(b-A*next)", "(b-A*next)"));
}

fn assert_scientific_fixtures() {
    scientific_connections();
    for (source, fors, whiles, multiple) in [
        (SCIENTIFIC_A, 1, 0, 0),
        (SCIENTIFIC_B, 0, 1, 0),
        (SCIENTIFIC_C, 2, 0, 1),
    ] {
        let m = parse(source);
        assert_eq!(m.functions().len(), 1);
        assert_eq!(
            m.nodes()
                .iter()
                .filter(|n| matches!(n.kind, ProgramNodeKindV1::For { .. }))
                .count(),
            fors
        );
        assert_eq!(
            m.nodes()
                .iter()
                .filter(|n| matches!(n.kind, ProgramNodeKindV1::While { .. }))
                .count(),
            whiles
        );
        assert_eq!(
            m.nodes()
                .iter()
                .filter(|n| matches!(
                    n.kind,
                    ProgramNodeKindV1::Assignment {
                        targets: AssignmentTargetsV1::Multiple(_),
                        ..
                    }
                ))
                .count(),
            multiple
        );
        let ProgramNodeKindV1::Block {
            statements: Some(script),
        } = m.node(m.script()).unwrap().kind
        else {
            panic!()
        };
        let mut cursor = Some(script);
        let mut count = 0;
        while let Some(id) = cursor {
            let ProgramNodeKindV1::List { item, next } = m.node(id).unwrap().kind else {
                panic!()
            };
            assert!(!matches!(
                m.node(item).unwrap().kind,
                ProgramNodeKindV1::Function { .. }
            ));
            count += 1;
            cursor = next;
        }
        assert!(count >= 4);
        assert!(
            m.nodes()
                .iter()
                .any(|n| matches!(n.kind, ProgramNodeKindV1::Return))
        );
    }
    let m = parse(SCIENTIFIC_C);
    assert!(m.nodes().iter().any(|n|matches!(n.kind,ProgramNodeKindV1::Assignment{targets:AssignmentTargetsV1::Single(id),..} if matches!(m.node(id).unwrap().kind,ProgramNodeKindV1::Apply{..}))));
}

#[test]
fn pa_precedence() {
    let m = parse("2^3'");
    let ProgramNodeKindV1::Transpose {
        input,
        conjugate: true,
    } = m.node(expression(&m)).unwrap().kind
    else {
        panic!()
    };
    binary(&m, input, BinaryOperatorV1::Power);
    let m = parse("2'^3");
    let (left, _) = binary(&m, expression(&m), BinaryOperatorV1::Power);
    assert!(matches!(
        m.node(left).unwrap().kind,
        ProgramNodeKindV1::Transpose {
            conjugate: true,
            ..
        }
    ));
    let m = parse("A.'");
    assert!(matches!(
        m.node(expression(&m)).unwrap().kind,
        ProgramNodeKindV1::Transpose {
            conjugate: false,
            ..
        }
    ));

    use BinaryOperatorV1 as B;
    let m = parse("2^3^2");
    let (l, _) = binary(&m, expression(&m), B::Power);
    binary(&m, l, B::Power);
    let m = parse("-2^2");
    let ProgramNodeKindV1::Unary { input, .. } = m.node(expression(&m)).unwrap().kind else {
        panic!()
    };
    binary(&m, input, B::Power);
    let m = parse("a||b&&c|d&e==f: g+h*i");
    let (_, r) = binary(&m, expression(&m), B::ShortCircuitOr);
    let (_, r) = binary(&m, r, B::ShortCircuitAnd);
    let (_, r) = binary(&m, r, B::Or);
    let (_, r) = binary(&m, r, B::And);
    binary(&m, r, B::Equal);
    for s in ["2^-2", "a.*b./c.\\d", "a~=b<=c>=d", "A' + B.'", "2^3'"] {
        parse(s);
    }
}
#[test]
fn pa_matrices_calls_ranges_end() {
    for (source, count) in [
        ("[A (1)]", 2),
        ("[A(1)]", 1),
        ("[A -B]", 2),
        ("[A - B]", 1),
        ("[A --B]", 2),
        ("[f(A, (1))]", 1),
    ] {
        let m = parse(source);
        let ProgramNodeKindV1::Matrix { rows: Some(rows) } = m.node(expression(&m)).unwrap().kind
        else {
            panic!()
        };
        let ProgramNodeKindV1::List {
            item: row,
            next: None,
        } = m.node(rows).unwrap().kind
        else {
            panic!()
        };
        let ProgramNodeKindV1::Row { items } = m.node(row).unwrap().kind else {
            panic!()
        };
        let mut cursor = items;
        let mut actual = 0;
        while let Some(id) = cursor {
            let ProgramNodeKindV1::List { next, .. } = m.node(id).unwrap().kind else {
                panic!()
            };
            actual += 1;
            cursor = next;
        }
        assert_eq!(actual, count, "{source}");
    }

    let m =
        parse("[A (1), B(1,end-1); A -B A - B A --B];\n[a,b]=f(A(:,2:end));\nA(1:2:end)=b;\n[]\n");
    assert_eq!(
        m.nodes()
            .iter()
            .filter(|n| matches!(n.kind, ProgramNodeKindV1::End))
            .count(),
        3
    );
    assert!(
        m.nodes()
            .iter()
            .any(|n| matches!(n.kind, ProgramNodeKindV1::FullAxis))
    );
    assert!(m.nodes().iter().any(|n| matches!(
        n.kind,
        ProgramNodeKindV1::Assignment {
            targets: AssignmentTargetsV1::Multiple(_),
            ..
        }
    )));
    let m = parse("A(1:2:9)");
    assert!(
        m.nodes()
            .iter()
            .any(|n| matches!(n.kind, ProgramNodeKindV1::Range { step: Some(_), .. }))
    );
}
#[test]
fn pa_lexical_locations() {
    let s = "\u{feff}% α comment\r\n%{\r\n%{\n%}\n%}\rA=1 ... ignored @ ' \r\n +2;\rA(end)\n";
    let m = parse(s);
    let n = m
        .nodes()
        .iter()
        .find(|n| m.source_text(n.span) == Some("A") && n.span.start_line == 6)
        .unwrap();
    assert_eq!(n.span.end_line, 6);
    assert!(
        m.nodes()
            .iter()
            .any(|n| matches!(n.kind, ProgramNodeKindV1::End))
    );
    for s in [".5", "1.e2", "1e-2", "1.*2", "1./2", "1.^2", "1.\\2"] {
        parse(s);
    }
}
#[test]
fn pa_deterministic_immutable() {
    let source: Arc<str> = Arc::from("x=f(1);function y=f(x),y=x+1;end\n");
    let a = parse_program_v1(7, source.clone()).unwrap();
    let b = parse_program_v1(7, source.clone()).unwrap();
    assert_eq!(a.nodes(), b.nodes());
    assert_eq!(a.stats(), b.stats());
    assert_eq!(a.source().as_ptr(), source.as_ptr());
    assert!(
        a.source_text(ProgramSpanV1 {
            start: usize::MAX,
            end: 0,
            start_line: 0,
            end_line: 0
        })
        .is_none()
    );
}
#[test]
fn pb_unsupported() {
    use ProgramParseErrorCodeV1::UnsupportedFeature as U;
    for s in [
        "classdef C\nend",
        "global a",
        "persistent a",
        "try\ncatch\nend",
        "parfor a=1\nend",
        "spmd\nend",
        "@f",
        "'abc'",
        "\"abc\"",
        "A{1}",
        "A.field",
        "disp x",
        "[A 'x']",
        "function f\nfunction g\nend\nend",
    ] {
        refused(s, U);
    }
}
#[test]
fn pb_invalid_syntax() {
    let error = parse_program_v1(44, Arc::from("% α\r\nx=@f")).unwrap_err();
    assert_eq!(error.source_id, 44);
    assert_eq!(error.span.start_line, 2);
    assert_eq!(error.span.start, 8);
    assert_eq!(error.span.end, 9);

    use ProgramParseErrorCodeV1::Syntax as S;
    for s in [
        "(",
        "[1,]",
        "[1;]",
        "[;1]",
        "f(,)",
        "f(1,)",
        "1+",
        "1e+",
        "if a\nx=1\n",
        "for 1=2\nend",
        "while\nend",
        "return 1",
        "end",
        "else",
        "a end",
        "1:2:3:4",
        "%{\n",
        "function f(a,)\nend",
    ] {
        refused(s, S);
    }
    refused("(a)=1", ProgramParseErrorCodeV1::InvalidAssignment);
    refused("[a,1]=f()", ProgramParseErrorCodeV1::InvalidAssignment);
}
#[test]
fn pb_duplicates_and_jumps() {
    use ProgramParseErrorCodeV1::*;
    for s in [
        "function f(a,a)\nend",
        "function [a a]=f\nend",
        "function f\nend\nfunction f\nend",
    ] {
        refused(s, DuplicateName);
    }
    for s in ["break", "continue", "function f\nbreak\nend"] {
        refused(s, InvalidControlFlow);
    }
    parse("return\nfunction y=f(y)\nreturn\nend\n");
}
#[test]
fn pb_bounded_mutations() {
    let base = b"if x>1\na=f(x,2);\nelse\na=[1 2];\nend\n";
    for i in 0..base.len() {
        for byte in [b'(', b']', b'\'', b'@', b'\n', b';'] {
            let mut s = base.to_vec();
            s[i] = byte;
            let source = String::from_utf8(s).unwrap();
            let _ = parse_program_v1(1, Arc::from(source));
        }
    }
}
#[test]
fn pc_source_and_statistics() {
    use ProgramParseErrorCodeV1::ResourceLimit as R;
    let s = format!("%{}", "a".repeat(65535));
    let m = parse(&s);
    assert_eq!(m.stats().source_bytes, 65536);
    refused(&(s + "a"), R);
    let m = parse("x=1;\n");
    assert_eq!(m.stats().nodes, m.nodes().len());
    assert_eq!(m.stats().numeric_literals, 1);
    assert_eq!(m.stats().statements, 1);
    assert!(m.stats().tokens <= m.source().len() + 1);
    assert!(m.stats().reserved_bytes > 4096 * std::mem::size_of::<ProgramNodeV1>());
}
#[test]
fn pc_cap_boundaries() {
    use ProgramParseErrorCodeV1::ResourceLimit as R;
    let s = "x;".repeat(1024);
    assert_eq!(parse(&s).stats().statements, 1024);
    refused(&(s + "x;"), R);
    let s = std::iter::repeat_n("1", 1024).collect::<Vec<_>>().join("+");
    assert_eq!(parse(&s).stats().numeric_literals, 1024);
    refused(&(s + "+1"), R);
    let s = (0..32)
        .map(|i| format!("function f{i}\nend\n"))
        .collect::<String>();
    assert_eq!(parse(&s).functions().len(), 32);
    refused(&(s + "function f32\nend\n"), R);
    let s = format!("{}x{}", "(".repeat(32), ")".repeat(32));
    assert_eq!(parse(&s).stats().max_nesting, 32);
    refused(&format!("({s})"), R);
    let s = format!("{}x;", "(x);".repeat(1023));
    assert_eq!(parse(&s).stats().nodes, 4096);
    refused(&format!("{}(x);", "(x);".repeat(1023)), R);
    let s = format!("{}% comment\n{}", "%{\n".repeat(32), "%}\n".repeat(32));
    parse(&s);
    refused(&format!("%{{\n{s}%}}\n"), R);
}
#[test]
fn pc_cancellation_every_poll() {
    let s: Arc<str> = Arc::from("a=f(1);\nfunction y=f(x)\nif x>0\ny=x^2;\nelse\ny=0;\nend\nend\n");
    let m = parse_program_v1(12, s.clone()).unwrap();
    for cancel in 1..=m.stats().polls {
        let mut polls = 0;
        let e = parse_program_controlled_v1(12, s.clone(), || {
            polls += 1;
            polls == cancel
        })
        .unwrap_err();
        assert_eq!(e.code, ProgramParseErrorCodeV1::Cancelled);
        assert_eq!(polls, cancel);
    }
    let mut polls = 0;
    parse_program_controlled_v1(12, s, || {
        polls += 1;
        false
    })
    .unwrap();
    assert_eq!(polls, m.stats().polls);
}

#[test]
fn pa_signed_exponent_chains_preserve_left_fold_unary_and_spans() {
    use BinaryOperatorV1::Power;
    let m = parse("2^-2^3");
    let root = expression(&m);
    let (left, right) = binary(&m, root, Power);
    let (_, signed) = binary(&m, left, Power);
    assert_eq!(m.source_text(m.node(root).unwrap().span), Some("2^-2^3"));
    assert_eq!(m.source_text(m.node(left).unwrap().span), Some("2^-2"));
    assert_eq!(m.source_text(m.node(signed).unwrap().span), Some("-2"));
    assert_eq!(m.source_text(m.node(right).unwrap().span), Some("3"));
    assert!(matches!(m.node(signed).unwrap().kind, ProgramNodeKindV1::Unary { operator: UnaryOperatorV1::Minus, .. }));
    for source in ["2^3^-2", "2^-2^-3", "2^+2^3"] {
        let m = parse(source);
        let (left, _) = binary(&m, expression(&m), Power);
        binary(&m, left, Power);
    }
    let m = parse("-2^-2^3");
    let ProgramNodeKindV1::Unary { input, .. } = m.node(expression(&m)).unwrap().kind else { panic!() };
    let (left, _) = binary(&m, input, Power);
    binary(&m, left, Power);
    let m = parse("2^-(2^3)");
    let (_, right) = binary(&m, expression(&m), Power);
    let ProgramNodeKindV1::Unary { input, .. } = m.node(right).unwrap().kind else { panic!() };
    let ProgramNodeKindV1::Group { value } = m.node(input).unwrap().kind else { panic!() };
    binary(&m, value, Power);
    let m = parse("2^-2^3'");
    let ProgramNodeKindV1::Transpose { input, .. } = m.node(expression(&m)).unwrap().kind else { panic!() };
    let (left, right) = binary(&m, input, Power);
    binary(&m, left, Power);
    assert_eq!(m.source_text(m.node(right).unwrap().span), Some("3"));
    for source in ["2^~2^3", "2^2^~3", "2^-~2^3", "2^+~2^3", "2^2^-~3", "2^~+2^3", "2^~-2^3"] { refused(source, ProgramParseErrorCodeV1::UnsupportedFeature); }
    let m = parse("2.^-2.^3");
    let (left, _) = binary(&m, expression(&m), BinaryOperatorV1::ElementPower);
    binary(&m, left, BinaryOperatorV1::ElementPower);
    refused("[~,kept]=many()", ProgramParseErrorCodeV1::Syntax);
    for source in ["2^-", "2^-^3", "2^-2^"] { refused(source, ProgramParseErrorCodeV1::Syntax); }
}
