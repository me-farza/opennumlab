// SPDX-License-Identifier: GPL-3.0-or-later
// Included in parser.rs so class declarations use the same private Pratt/block grammar.
struct ClassNodes {
    nodes: Vec<ClassNodeV1>,
    source_slot: u8,
}
impl SyntaxNodes for ClassNodes {
    const MEMBERS: bool = true;
    fn len(&self) -> usize {
        self.nodes.len()
    }
    fn span(&self, id: NodeIdV1) -> ProgramSpanV1 {
        self.nodes[id.index()].span
    }
    fn common(&self, id: NodeIdV1) -> Option<N> {
        match self.nodes[id.index()].kind {
            ClassNodeKindV1::Common(n) => Some(n),
            _ => None,
        }
    }
    fn member(&self, id: NodeIdV1) -> Option<(NodeIdV1, NodeIdV1)> {
        match self.nodes[id.index()].kind {
            ClassNodeKindV1::Member { receiver, name } => Some((receiver, name)),
            _ => None,
        }
    }
    fn push_common(&mut self, kind: N, span: ProgramSpanV1) {
        self.nodes.push(ClassNodeV1 {
            source_slot: self.source_slot,
            span,
            kind: ClassNodeKindV1::Common(kind),
        });
    }
    fn push_member(&mut self, receiver: NodeIdV1, name: NodeIdV1, span: ProgramSpanV1) {
        self.nodes.push(ClassNodeV1 {
            source_slot: self.source_slot,
            span,
            kind: ClassNodeKindV1::Member { receiver, name },
        });
    }
    fn set_next(&mut self, tail: NodeIdV1, id: NodeIdV1) {
        if let ClassNodeKindV1::Common(N::List { next, .. }) = &mut self.nodes[tail.index()].kind {
            *next = Some(id);
        }
    }
}
fn class_error(e: ProgramParseErrorV1) -> ClassParseErrorV1 {
    let code = match e.code {
        ProgramParseErrorCodeV1::UnsupportedFeature => ClassParseErrorCodeV1::Unsupported,
        ProgramParseErrorCodeV1::DuplicateName => ClassParseErrorCodeV1::DuplicateName,
        ProgramParseErrorCodeV1::ResourceLimit => ClassParseErrorCodeV1::Resource,
        ProgramParseErrorCodeV1::AllocationFailure => ClassParseErrorCodeV1::Allocation,
        ProgramParseErrorCodeV1::Cancelled => ClassParseErrorCodeV1::Cancelled,
        _ => ClassParseErrorCodeV1::Syntax,
    };
    ClassParseErrorV1 {
        source_id: e.source_id,
        span: e.span,
        code,
    }
}
pub fn parse_class_graph_v1(
    sources: Vec<ClassSourceV1>,
) -> Result<ClassProgramGraphV1, ClassParseErrorV1> {
    parse_class_graph_controlled_v1(sources, || false)
}
pub fn parse_class_graph_controlled_v1(
    sources: Vec<ClassSourceV1>,
    mut poll: impl FnMut() -> bool,
) -> Result<ClassProgramGraphV1, ClassParseErrorV1> {
    let origin = ProgramSpanV1 {
        start: 0,
        end: 0,
        start_line: 1,
        end_line: 1,
    };
    let fail = |source_id, code| ClassParseErrorV1 {
        source_id,
        span: origin,
        code,
    };
    let first = sources.first().map_or(0, |s| s.source_id);
    if sources.is_empty() || sources.len() > MAX_CLASS_SOURCES_V1 {
        return Err(fail(first, ClassParseErrorCodeV1::Resource));
    }
    let mut budget = Budget {
        source_id: first,
        stats: ProgramParseStatsV1::default(),
        poll: &mut poll,
    };
    budget.check(origin).map_err(class_error)?;
    for (i, source) in sources.iter().enumerate() {
        budget.source_id = source.source_id;
        budget.check(origin).map_err(class_error)?;
        if sources[..i].iter().any(|s| s.source_id == source.source_id) {
            return Err(fail(source.source_id, ClassParseErrorCodeV1::DuplicateName));
        }
        budget.stats.source_bytes = budget
            .stats
            .source_bytes
            .checked_add(source.text.len())
            .filter(|&n| n <= MAX_CLASS_SOURCE_BYTES_V1)
            .ok_or_else(|| fail(source.source_id, ClassParseErrorCodeV1::Resource))?;
        if source.text.len() > MAX_PROGRAM_SOURCE_BYTES_V1 {
            return Err(fail(source.source_id, ClassParseErrorCodeV1::Resource));
        }
    }
    budget.source_id = first;
    let mut nodes = Vec::new();
    let mut functions = Vec::new();
    let mut classes = Vec::new();
    budget
        .reserve(&mut nodes, MAX_PROGRAM_NODES_V1, origin)
        .map_err(class_error)?;
    budget
        .reserve(&mut functions, MAX_PROGRAM_FUNCTIONS_V1, origin)
        .map_err(class_error)?;
    budget
        .reserve(&mut classes, MAX_CLASSES_V1, origin)
        .map_err(class_error)?;
    let mut script = None;
    for (slot, source) in sources.iter().enumerate() {
        budget.source_id = source.source_id;
        let previous_tokens = budget.stats.tokens;
        let tokens = lexer::lex_class(&source.text, &mut budget).map_err(class_error)?;
        budget.stats.tokens = previous_tokens
            .checked_add(budget.stats.tokens)
            .ok_or_else(|| fail(source.source_id, ClassParseErrorCodeV1::Resource))?;
        let mut p = Parser {
            source: source.text.clone(),
            tokens,
            pos: 0,
            nodes: ClassNodes {
                nodes,
                source_slot: slot as u8,
            },
            functions,
            budget: &mut budget,
            depth: 0,
            loops: 0,
            end_context: 0,
        };
        if slot == 0 {
            script = Some(p.block(true).map_err(class_error)?);
        } else {
            p.separators().map_err(class_error)?;
            classes.push(p.class_definition(classes.len()).map_err(class_error)?);
            p.separators().map_err(class_error)?;
        }
        if p.k() != K::Eof {
            return Err(class_error(
                p.err(ProgramParseErrorCodeV1::UnsupportedFeature),
            ));
        }
        nodes = p.nodes.nodes;
        functions = p.functions;
    }
    budget.stats.nodes = nodes.len();
    let mut graph = ClassProgramGraphV1 {
        sources,
        nodes,
        functions,
        script: script.unwrap(),
        classes,
        stats: budget.stats,
    };
    resolve_classes(&mut graph, &mut budget)?;
    validate_class_script_names(&graph, graph.script, &mut budget).map_err(class_error)?;
    graph.stats = budget.stats;
    Ok(graph)
}
impl Parser<'_, '_, ClassNodes> {
    fn class_definition(&mut self, index: usize) -> ParseResult<ClassDefinitionV1> {
        if !self.word("classdef") {
            return Err(self.err(ProgramParseErrorCodeV1::Syntax));
        }
        self.count_statement()?;
        self.bump()?;
        if self.k() == K::LParen {
            return Err(self.err(ProgramParseErrorCodeV1::UnsupportedFeature));
        }
        let name = self.name()?;
        let base_name = if self.k() == K::Lt {
            self.bump()?;
            Some(self.name()?)
        } else {
            None
        };
        if !matches!(self.k(), K::Newline | K::Semi | K::Comma) {
            return Err(self.err(ProgramParseErrorCodeV1::UnsupportedFeature));
        }
        self.header()?;
        let mut properties = Vec::new();
        let mut methods = Vec::new();
        let name_span = self.span(name);
        self.budget
            .reserve(&mut properties, MAX_CLASS_PROPERTIES_V1, name_span)?;
        self.budget
            .reserve(&mut methods, MAX_PROGRAM_FUNCTIONS_V1, name_span)?;
        while !self.word("end") {
            self.budget.check(self.t().span)?;
            if self.word("properties") {
                self.bump()?;
                if self.k() == K::LParen {
                    return Err(self.err(ProgramParseErrorCodeV1::UnsupportedFeature));
                }
                self.header()?;
                while !self.word("end") {
                    self.budget
                        .count(properties.len(), MAX_CLASS_PROPERTIES_V1, self.t().span)?;
                    let property = self.name()?;
                    for previous in &properties {
                        let previous: &ClassPropertyV1 = previous;
                        if self.same(previous.name, property)? {
                            return Err(self.budget.error(
                                self.span(property),
                                ProgramParseErrorCodeV1::DuplicateName,
                            ));
                        }
                    }
                    let initial = if self.k() == K::Assign {
                        self.bump()?;
                        Some(self.expr(0, false, false)?)
                    } else {
                        None
                    };
                    if !matches!(self.k(), K::Newline | K::Semi | K::Comma) {
                        return Err(self.err(ProgramParseErrorCodeV1::UnsupportedFeature));
                    }
                    properties.push(ClassPropertyV1 {
                        name: property,
                        initial,
                        declaring_class: index,
                    });
                    self.header()?;
                }
                self.end()?;
                self.header()?;
            } else if self.word("methods") {
                self.bump()?;
                if self.k() == K::LParen {
                    return Err(self.err(ProgramParseErrorCodeV1::UnsupportedFeature));
                }
                self.header()?;
                while !self.word("end") {
                    if !self.word("function") {
                        return Err(self.err(ProgramParseErrorCodeV1::UnsupportedFeature));
                    }
                    let function = self.function_node(false)?;
                    let Some(N::Function {
                        name: method,
                        inputs,
                        outputs,
                        ..
                    }) = self.kind(function)
                    else {
                        unreachable!()
                    };
                    let constructor = self.same(name, method)?;
                    // Hooks require runtime dispatch/finalizer semantics outside class-v1.
                    let method_text = &self.source[self.span(method).start..self.span(method).end];
                    if !constructor
                        && [
                            "delete",
                            "isvalid",
                            "subsref",
                            "subsasgn",
                            "subsindex",
                            "plus",
                            "minus",
                            "times",
                            "mtimes",
                            "rdivide",
                            "mrdivide",
                            "ldivide",
                            "mldivide",
                            "power",
                            "mpower",
                            "eq",
                            "ne",
                            "lt",
                            "le",
                            "gt",
                            "ge",
                            "and",
                            "or",
                            "not",
                            "uminus",
                            "uplus",
                            "transpose",
                            "ctranspose",
                            "horzcat",
                            "vertcat",
                            "cat",
                            "numel",
                            "size",
                            "length",
                            "isempty",
                            "disp",
                            "display",
                            "class",
                            "isa",
                            "isequal",
                            "isequaln",
                            "saveobj",
                            "loadobj",
                        ]
                        .contains(&method_text)
                    {
                        return Err(self.budget.error(
                            self.span(method),
                            ProgramParseErrorCodeV1::UnsupportedFeature,
                        ));
                    }
                    // Constructor receiver is its ordinary single output; it is seeded by the engine.
                    if constructor {
                        if !outputs.is_some_and(|list| {
                            matches!(self.kind(list), Some(N::List { next: None, .. }))
                        }) {
                            return Err(self.budget.error(
                                self.span(method),
                                ProgramParseErrorCodeV1::UnsupportedFeature,
                            ));
                        }
                    } else if inputs.is_none() {
                        return Err(self.budget.error(
                            self.span(method),
                            ProgramParseErrorCodeV1::UnsupportedFeature,
                        ));
                    }
                    for previous in &methods {
                        let previous: &ClassMethodV1 = previous;
                        if self.same(previous.name, method)? {
                            return Err(self
                                .budget
                                .error(self.span(method), ProgramParseErrorCodeV1::DuplicateName));
                        }
                    }
                    methods.push(ClassMethodV1 {
                        name: method,
                        function,
                        declaring_class: index,
                        constructor,
                    });
                    if !matches!(self.k(), K::Newline | K::Semi | K::Comma) {
                        return Err(self.err(ProgramParseErrorCodeV1::Syntax));
                    }
                    self.separators()?;
                }
                self.end()?;
                self.header()?;
            } else {
                return Err(self.err(ProgramParseErrorCodeV1::UnsupportedFeature));
            }
        }
        self.end()?;
        Ok(ClassDefinitionV1 {
            name,
            base_name,
            base: None,
            is_handle: false,
            properties,
            methods,
        })
    }
}
fn graph_error(
    graph: &ClassProgramGraphV1,
    id: NodeIdV1,
    code: ClassParseErrorCodeV1,
) -> ClassParseErrorV1 {
    ClassParseErrorV1 {
        source_id: graph.source_id_for(id).unwrap(),
        span: graph.node(id).unwrap().span,
        code,
    }
}
fn graph_poll(
    graph: &ClassProgramGraphV1,
    id: NodeIdV1,
    budget: &mut Budget<'_>,
) -> Result<(), ClassParseErrorV1> {
    budget.source_id = graph.source_id_for(id).unwrap();
    budget
        .check(graph.node(id).unwrap().span)
        .map_err(class_error)
}
fn graph_same(
    graph: &ClassProgramGraphV1,
    a: NodeIdV1,
    b: NodeIdV1,
    budget: &mut Budget<'_>,
) -> Result<bool, ClassParseErrorV1> {
    let a_text = graph.text(a).unwrap().as_bytes();
    let b_text = graph.text(b).unwrap().as_bytes();
    if a_text.len() != b_text.len() {
        return Ok(false);
    }
    for offset in (0..a_text.len()).step_by(128) {
        graph_poll(graph, a, budget)?;
        let end = (offset + 128).min(a_text.len());
        if a_text[offset..end] != b_text[offset..end] {
            return Ok(false);
        }
    }
    Ok(true)
}
fn resolve_classes(
    graph: &mut ClassProgramGraphV1,
    budget: &mut Budget<'_>,
) -> Result<(), ClassParseErrorV1> {
    for i in 0..graph.classes.len() {
        let name = graph.classes[i].name;
        graph_poll(graph, name, budget)?;
        if graph.text(name) == Some("handle") {
            return Err(graph_error(graph, name, ClassParseErrorCodeV1::Unsupported));
        }
        for previous in &graph.classes[..i] {
            if graph_same(graph, previous.name, name, budget)? {
                return Err(graph_error(
                    graph,
                    name,
                    ClassParseErrorCodeV1::DuplicateName,
                ));
            }
        }
        for &function in &graph.functions {
            graph_poll(graph, function, budget)?;
            if let Some(N::Function { name: other, .. }) = graph.common_kind(function) {
                if graph_same(graph, name, other, budget)? {
                    return Err(graph_error(
                        graph,
                        other,
                        ClassParseErrorCodeV1::DuplicateName,
                    ));
                }
            }
        }
        if let Some(base_name) = graph.classes[i].base_name {
            if graph.text(base_name) == Some("handle") {
                graph.classes[i].is_handle = true;
            } else {
                let mut found = None;
                for (index, definition) in graph.classes.iter().enumerate() {
                    if graph_same(graph, definition.name, base_name, budget)? {
                        found = Some(index);
                        break;
                    }
                }
                graph.classes[i].base = Some(found.ok_or_else(|| {
                    graph_error(graph, base_name, ClassParseErrorCodeV1::UnknownBase)
                })?);
            }
        }
    }
    // Fixed-size marks bound inheritance traversal and avoid recursive source graphs.
    let mut done = [false; MAX_CLASSES_V1];
    let mut remaining = graph.classes.len();
    while remaining > 0 {
        let mut progress = false;
        for i in 0..graph.classes.len() {
            let name = graph.classes[i].name;
            graph_poll(graph, name, budget)?;
            if done[i] {
                continue;
            }
            let base = graph.classes[i].base;
            if base.is_some_and(|b| !done[b]) {
                continue;
            }
            if let Some(base) = base {
                graph.classes[i].is_handle = graph.classes[base].is_handle;
                let count = graph.classes[base]
                    .properties
                    .len()
                    .checked_add(graph.classes[i].properties.len())
                    .filter(|&n| n <= MAX_CLASS_PROPERTIES_V1)
                    .ok_or_else(|| graph_error(graph, name, ClassParseErrorCodeV1::Resource))?;
                for property in &graph.classes[i].properties {
                    for inherited in &graph.classes[base].properties {
                        graph_poll(graph, property.name, budget)?;
                        if graph_same(graph, property.name, inherited.name, budget)? {
                            return Err(graph_error(
                                graph,
                                property.name,
                                ClassParseErrorCodeV1::DuplicateName,
                            ));
                        }
                    }
                }
                let mut flattened = Vec::new();
                budget.source_id = graph.source_id_for(name).unwrap();
                budget
                    .reserve(&mut flattened, count, graph.node(name).unwrap().span)
                    .map_err(class_error)?;
                for property in graph.classes[base]
                    .properties
                    .iter()
                    .chain(graph.classes[i].properties.iter())
                {
                    graph_poll(graph, property.name, budget)?;
                    flattened.push(*property);
                }
                graph.classes[i].properties = flattened;
            }
            // Field/method collisions are an explicit class-v1 admission limit.
            for property in &graph.classes[i].properties {
                let mut owner = Some(i);
                while let Some(c) = owner {
                    graph_poll(graph, property.name, budget)?;
                    for method in &graph.classes[c].methods {
                        if graph_same(graph, method.name, property.name, budget)? {
                            return Err(graph_error(
                                graph,
                                property.name,
                                ClassParseErrorCodeV1::DuplicateName,
                            ));
                        }
                    }
                    owner = graph.classes[c].base;
                }
            }
            done[i] = true;
            remaining -= 1;
            progress = true;
        }
        if !progress {
            let i = done[..graph.classes.len()]
                .iter()
                .position(|&d| !d)
                .unwrap();
            let id = graph.classes[i].base_name.unwrap_or(graph.classes[i].name);
            return Err(graph_error(
                graph,
                id,
                ClassParseErrorCodeV1::InheritanceCycle,
            ));
        }
    }
    Ok(())
}
// Same script/function collision rule as the legacy frontend, with member writes
// rooted at their binding. Class names intentionally do not enter this namespace.
fn validate_class_script_names(
    graph: &ClassProgramGraphV1,
    id: NodeIdV1,
    budget: &mut Budget<'_>,
) -> ParseResult<()> {
    budget.source_id = graph.source_id_for(id).unwrap();
    budget.check(graph.node(id).unwrap().span)?;
    match graph.common_kind(id) {
        Some(N::Block { mut statements }) => {
            while let Some(list) = statements {
                budget.check(graph.node(list).unwrap().span)?;
                let Some(N::List { item, next }) = graph.common_kind(list) else {
                    unreachable!()
                };
                statements = next;
                validate_class_script_names(graph, item, budget)?;
            }
        }
        Some(N::Assignment { targets, .. }) => match targets {
            AssignmentTargetsV1::Single(target) => {
                validate_class_script_write(graph, target, budget)?
            }
            AssignmentTargetsV1::Multiple(list) => {
                let mut cursor = Some(list);
                while let Some(list) = cursor {
                    budget.check(graph.node(list).unwrap().span)?;
                    let Some(N::List { item, next }) = graph.common_kind(list) else {
                        unreachable!()
                    };
                    cursor = next;
                    validate_class_script_write(graph, item, budget)?;
                }
            }
        },
        Some(N::For { variable, body, .. }) => {
            validate_class_script_write(graph, variable, budget)?;
            validate_class_script_names(graph, body, budget)?;
        }
        Some(N::While { body, .. }) => validate_class_script_names(graph, body, budget)?,
        Some(N::If {
            mut branches,
            otherwise,
        }) => {
            while let Some(list) = branches {
                budget.check(graph.node(list).unwrap().span)?;
                let Some(N::List { item, next }) = graph.common_kind(list) else {
                    unreachable!()
                };
                branches = next;
                let Some(N::IfBranch { body, .. }) = graph.common_kind(item) else {
                    unreachable!()
                };
                validate_class_script_names(graph, body, budget)?;
            }
            if let Some(body) = otherwise {
                validate_class_script_names(graph, body, budget)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn validate_class_script_write(
    graph: &ClassProgramGraphV1,
    mut id: NodeIdV1,
    budget: &mut Budget<'_>,
) -> ParseResult<()> {
    loop {
        budget.check(graph.node(id).unwrap().span)?;
        if let Some((receiver, _)) = graph.member(id) {
            id = receiver;
        } else if let Some(N::Apply { callee, .. }) = graph.common_kind(id) {
            id = callee;
        } else {
            break;
        }
    }
    let span = graph.node(id).unwrap().span;
    for &function in &graph.functions {
        budget.check(span)?;
        let Some(N::Function { name, .. }) = graph.common_kind(function) else {
            unreachable!()
        };
        let other = graph.node(name).unwrap().span;
        if span.end - span.start != other.end - other.start {
            continue;
        }
        let a = graph.text(id).unwrap().as_bytes();
        let b = graph.text(name).unwrap().as_bytes();
        let mut equal = true;
        for offset in (0..a.len()).step_by(128) {
            budget.check(span)?;
            let end = (offset + 128).min(a.len());
            if a[offset..end] != b[offset..end] {
                equal = false;
                break;
            }
        }
        if equal {
            return Err(budget.error(span, ProgramParseErrorCodeV1::DuplicateName));
        }
    }
    Ok(())
}
