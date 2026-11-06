// SPDX-License-Identifier: GPL-3.0-or-later
use super::{
    lexer::{Kind as K, Token},
    *,
};
use ProgramNodeKindV1 as N;

// Concrete adapters keep the legacy arena element and reservation formulas intact.
// The grammar is monomorphized over storage, never a second expression parser.
trait SyntaxNodes {
    const MEMBERS: bool;
    fn len(&self) -> usize;
    fn span(&self, id: NodeIdV1) -> ProgramSpanV1;
    fn common(&self, id: NodeIdV1) -> Option<N>;
    fn member(&self, _id: NodeIdV1) -> Option<(NodeIdV1, NodeIdV1)> {
        None
    }
    fn push_common(&mut self, kind: N, span: ProgramSpanV1);
    fn push_member(&mut self, receiver: NodeIdV1, name: NodeIdV1, span: ProgramSpanV1);
    fn set_next(&mut self, tail: NodeIdV1, next: NodeIdV1);
}
struct LegacyNodes(Vec<ProgramNodeV1>);
impl SyntaxNodes for LegacyNodes {
    const MEMBERS: bool = false;
    fn len(&self) -> usize {
        self.0.len()
    }
    fn span(&self, id: NodeIdV1) -> ProgramSpanV1 {
        self.0[id.index()].span
    }
    fn common(&self, id: NodeIdV1) -> Option<N> {
        Some(self.0[id.index()].kind)
    }
    fn push_common(&mut self, kind: N, span: ProgramSpanV1) {
        self.0.push(ProgramNodeV1 { kind, span });
    }
    fn push_member(&mut self, _: NodeIdV1, _: NodeIdV1, _: ProgramSpanV1) {
        unreachable!()
    }
    fn set_next(&mut self, tail: NodeIdV1, id: NodeIdV1) {
        if let N::List { next, .. } = &mut self.0[tail.index()].kind {
            *next = Some(id);
        }
    }
}
include!("class_frontend.rs");

#[derive(Default, Clone, Copy)]
struct List {
    head: Option<NodeIdV1>,
    tail: Option<NodeIdV1>,
}
struct Parser<'a, 'b, B> {
    source: Arc<str>,
    tokens: Vec<Token>,
    pos: usize,
    nodes: B,
    functions: Vec<NodeIdV1>,
    budget: &'a mut Budget<'b>,
    depth: usize,
    loops: usize,
    end_context: usize,
}
pub(super) fn parse(
    source: Arc<str>,
    tokens: Vec<Token>,
    budget: &mut Budget<'_>,
) -> ParseResult<ProgramModuleV1> {
    let mut nodes = Vec::new();
    let mut functions = Vec::new();
    let span = tokens[0].span;
    budget.reserve(&mut nodes, MAX_PROGRAM_NODES_V1, span)?;
    budget.reserve(&mut functions, MAX_PROGRAM_FUNCTIONS_V1, span)?;
    let mut p = Parser {
        source,
        tokens,
        pos: 0,
        nodes: LegacyNodes(nodes),
        functions,
        budget,
        depth: 0,
        loops: 0,
        end_context: 0,
    };
    let script = p.block(true)?;
    if p.k() != K::Eof {
        return Err(p.err(ProgramParseErrorCodeV1::Syntax));
    }
    p.budget.stats.nodes = p.nodes.len();
    let stats = p.budget.stats;
    Ok(ProgramModuleV1 {
        source_id: p.budget.source_id,
        source: p.source,
        nodes: p.nodes.0,
        functions: p.functions,
        script,
        stats,
    })
}
impl<B: SyntaxNodes> Parser<'_, '_, B> {
    fn t(&self) -> Token {
        self.tokens[self.pos]
    }
    fn k(&self) -> K {
        self.t().kind
    }
    fn word(&self, w: &str) -> bool {
        self.k() == K::Name && &self.source[self.t().span.start..self.t().span.end] == w
    }
    fn err(&self, c: ProgramParseErrorCodeV1) -> ProgramParseErrorV1 {
        self.budget.error(self.t().span, c)
    }
    fn bump(&mut self) -> ParseResult<Token> {
        let t = self.t();
        self.budget.check(t.span)?;
        if t.kind != K::Eof {
            self.pos += 1
        }
        Ok(t)
    }
    fn need(&mut self, k: K) -> ParseResult<Token> {
        if self.k() != k {
            return Err(self.err(ProgramParseErrorCodeV1::Syntax));
        }
        self.bump()
    }
    fn node(&mut self, kind: N, span: ProgramSpanV1) -> ParseResult<NodeIdV1> {
        self.budget.check(span)?;
        self.budget
            .count(self.nodes.len(), MAX_PROGRAM_NODES_V1, span)?;
        let id = NodeIdV1(self.nodes.len() as u32);
        self.nodes.push_common(kind, span);
        Ok(id)
    }
    fn span(&self, id: NodeIdV1) -> ProgramSpanV1 {
        self.nodes.span(id)
    }
    fn kind(&self, id: NodeIdV1) -> Option<N> {
        self.nodes.common(id)
    }
    fn append(&mut self, list: &mut List, item: NodeIdV1) -> ParseResult<()> {
        let id = self.node(N::List { item, next: None }, self.span(item))?;
        if let Some(t) = list.tail {
            self.nodes.set_next(t, id);
        } else {
            list.head = Some(id)
        }
        list.tail = Some(id);
        Ok(())
    }
    fn enter(&mut self) -> ParseResult<()> {
        self.budget.check(self.t().span)?;
        self.depth = self
            .budget
            .count(self.depth, MAX_PROGRAM_NESTING_V1, self.t().span)?;
        self.budget.stats.max_nesting = self.budget.stats.max_nesting.max(self.depth);
        Ok(())
    }
    fn count_statement(&mut self) -> ParseResult<()> {
        self.budget.check(self.t().span)?;
        self.budget.stats.statements = self.budget.count(
            self.budget.stats.statements,
            MAX_PROGRAM_STATEMENTS_V1,
            self.t().span,
        )?;
        Ok(())
    }
    fn separators(&mut self) -> ParseResult<()> {
        while matches!(self.k(), K::Newline | K::Semi | K::Comma) {
            self.bump()?;
        }
        Ok(())
    }
    fn header(&mut self) -> ParseResult<()> {
        if !matches!(self.k(), K::Newline | K::Semi | K::Comma) {
            return Err(self.err(ProgramParseErrorCodeV1::Syntax));
        }
        self.separators()
    }
    fn soft(&mut self) -> ParseResult<()> {
        while self.k() == K::Newline {
            self.bump()?;
        }
        Ok(())
    }
    fn stop(&self) -> bool {
        self.k() == K::Eof || self.word("end") || self.word("else") || self.word("elseif")
    }
    fn block(&mut self, top: bool) -> ParseResult<NodeIdV1> {
        if !top {
            self.enter()?;
        }
        let start = self.t().span;
        let mut list = List::default();
        self.separators()?;
        while !self.stop() {
            if self.word("function") {
                if !top {
                    return Err(self.err(ProgramParseErrorCodeV1::UnsupportedFeature));
                }
                self.function()?;
            } else {
                let stmt = self.statement()?;
                self.append(&mut list, stmt)?;
            }
            if self.k() != K::Eof && !matches!(self.k(), K::Newline | K::Semi | K::Comma) {
                return Err(self.err(if self.k() == K::Name && !self.reserved() {
                    ProgramParseErrorCodeV1::UnsupportedFeature
                } else {
                    ProgramParseErrorCodeV1::Syntax
                }));
            }
            self.separators()?;
        }
        let end = self.t().span;
        let id = self.node(
            N::Block {
                statements: list.head,
            },
            start.through(end),
        )?;
        if !top {
            self.depth -= 1;
        }
        Ok(id)
    }
    fn end(&mut self) -> ParseResult<Token> {
        if !self.word("end") {
            return Err(self.err(ProgramParseErrorCodeV1::Syntax));
        }
        self.bump()
    }
    fn name(&mut self) -> ParseResult<NodeIdV1> {
        if self.k() != K::Name || self.reserved() {
            return Err(self.err(ProgramParseErrorCodeV1::Syntax));
        }
        let t = self.bump()?;
        self.node(N::Name, t.span)
    }
    fn reserved(&self) -> bool {
        [
            "if",
            "else",
            "elseif",
            "end",
            "for",
            "while",
            "function",
            "break",
            "continue",
            "return",
            "classdef",
            "global",
            "persistent",
            "try",
            "catch",
            "parfor",
            "spmd",
            "switch",
            "case",
            "otherwise",
        ]
        .iter()
        .any(|w| self.word(w))
    }
    fn same(&mut self, a: NodeIdV1, b: NodeIdV1) -> ParseResult<bool> {
        let a = self.span(a);
        let b = self.span(b);
        if a.end - a.start != b.end - b.start {
            return Ok(false);
        }
        let mut i = 0;
        while i < a.end - a.start {
            self.budget.check(a)?;
            let n = (a.end - a.start - i).min(128);
            if self.source.as_bytes()[a.start + i..a.start + i + n]
                != self.source.as_bytes()[b.start + i..b.start + i + n]
            {
                return Ok(false);
            }
            i += n
        }
        Ok(true)
    }
    fn unique(&mut self, list: Option<NodeIdV1>, name: NodeIdV1) -> ParseResult<()> {
        let mut cur = list;
        while let Some(id) = cur {
            if let Some(N::List { item, next }) = self.kind(id) {
                if self.same(item, name)? {
                    return Err(self
                        .budget
                        .error(self.span(name), ProgramParseErrorCodeV1::DuplicateName));
                }
                cur = next
            } else {
                unreachable!()
            }
        }
        Ok(())
    }
    fn names(&mut self, close: K, whitespace: bool) -> ParseResult<Option<NodeIdV1>> {
        let mut list = List::default();
        self.soft()?;
        if self.k() == close {
            self.bump()?;
            return Ok(None);
        }
        loop {
            let name = self.name()?;
            self.unique(list.head, name)?;
            self.append(&mut list, name)?;
            self.soft()?;
            if self.k() == close {
                self.bump()?;
                break;
            }
            if self.k() == K::Comma {
                self.bump()?;
                self.soft()?;
            } else if !(whitespace && self.k() == K::Name && self.t().space) {
                return Err(self.err(ProgramParseErrorCodeV1::Syntax));
            }
        }
        Ok(list.head)
    }
    fn function(&mut self) -> ParseResult<()> {
        let function = self.function_node(true)?;
        self.functions.push(function);
        Ok(())
    }
    fn function_node(&mut self, free: bool) -> ParseResult<NodeIdV1> {
        self.count_statement()?;
        let start = self.bump()?.span;
        self.budget.stats.functions =
            self.budget
                .count(self.budget.stats.functions, MAX_PROGRAM_FUNCTIONS_V1, start)?;
        let mut outputs = None;
        let first = if self.k() == K::LBracket {
            self.bump()?;
            outputs = self.names(K::RBracket, true)?;
            self.need(K::Assign)?;
            self.name()?
        } else {
            let n = self.name()?;
            if self.k() == K::Assign {
                self.bump()?;
                let mut l = List::default();
                self.append(&mut l, n)?;
                outputs = l.head;
                self.name()?
            } else {
                n
            }
        };
        for i in 0..if free { self.functions.len() } else { 0 } {
            if let Some(N::Function { name, .. }) = self.kind(self.functions[i]) {
                if self.same(name, first)? {
                    return Err(self
                        .budget
                        .error(self.span(first), ProgramParseErrorCodeV1::DuplicateName));
                }
            }
        }
        let inputs = if self.k() == K::LParen {
            self.bump()?;
            self.names(K::RParen, false)?
        } else {
            None
        };
        if B::MEMBERS && self.k() == K::Dot {
            return Err(self.err(ProgramParseErrorCodeV1::UnsupportedFeature));
        }
        self.header()?;
        let body = self.block(false)?;
        let end = self.end()?.span;
        let function = self.node(
            N::Function {
                name: first,
                inputs,
                outputs,
                body,
            },
            start.through(end),
        )?;
        Ok(function)
    }
    fn statement(&mut self) -> ParseResult<NodeIdV1> {
        self.count_statement()?;
        let start = self.t().span;
        if self.word("if") {
            self.bump()?;
            let mut branches = List::default();
            let mut otherwise = None;
            loop {
                let condition = self.expr(0, false, false)?;
                self.header()?;
                let body = self.block(false)?;
                let branch = self.node(
                    N::IfBranch { condition, body },
                    self.span(condition).through(self.span(body)),
                )?;
                self.append(&mut branches, branch)?;
                if self.word("elseif") {
                    self.count_statement()?;
                    self.bump()?;
                } else {
                    break;
                }
            }
            if self.word("else") {
                self.count_statement()?;
                self.bump()?;
                self.header()?;
                otherwise = Some(self.block(false)?)
            }
            let end = self.end()?.span;
            return self.node(
                N::If {
                    branches: branches.head,
                    otherwise,
                },
                start.through(end),
            );
        }
        if self.word("for") || self.word("while") {
            let is_for = self.word("for");
            self.bump()?;
            let variable = if is_for {
                let v = self.name()?;
                self.need(K::Assign)?;
                Some(v)
            } else {
                None
            };
            let value = self.expr(0, false, false)?;
            self.header()?;
            self.loops += 1;
            let body = self.block(false)?;
            self.loops -= 1;
            let end = self.end()?.span;
            return self.node(
                if let Some(variable) = variable {
                    N::For {
                        variable,
                        iterable: value,
                        body,
                    }
                } else {
                    N::While {
                        condition: value,
                        body,
                    }
                },
                start.through(end),
            );
        }
        if self.word("break") || self.word("continue") || self.word("return") {
            let kind = if self.word("break") {
                N::Break
            } else if self.word("continue") {
                N::Continue
            } else {
                N::Return
            };
            if kind != N::Return && self.loops == 0 {
                return Err(self.err(ProgramParseErrorCodeV1::InvalidControlFlow));
            }
            self.bump()?;
            return self.node(kind, start);
        }
        if self.reserved() {
            return Err(self.err(ProgramParseErrorCodeV1::UnsupportedFeature));
        }
        let expression = self.expr(0, false, false)?;
        let kind = if self.k() == K::Assign {
            self.bump()?;
            let targets = self.targets(expression)?;
            let value = self.expr(0, false, false)?;
            N::Assignment {
                targets,
                value,
                suppressed: self.k() == K::Semi,
            }
        } else {
            N::ExpressionStatement {
                expression,
                suppressed: self.k() == K::Semi,
            }
        };
        let end = self.tokens[self.pos.saturating_sub(1)].span;
        self.node(kind, start.through(end))
    }
    fn assignable(&mut self, mut id: NodeIdV1) -> ParseResult<bool> {
        loop {
            self.budget.check(self.span(id))?;
            match self.kind(id) {
                Some(N::Name) => return Ok(true),
                Some(N::Apply { callee, .. }) => id = callee,
                None => match self.nodes.member(id) {
                    Some((receiver, _)) => id = receiver,
                    None => return Ok(false),
                },
                _ => return Ok(false),
            }
        }
    }
    fn targets(&mut self, id: NodeIdV1) -> ParseResult<AssignmentTargetsV1> {
        let valid = if B::MEMBERS {
            self.assignable(id)?
        } else {
            match self.kind(id) {
                Some(N::Name) => true,
                Some(N::Apply { callee, .. }) => matches!(self.kind(callee), Some(N::Name)),
                _ => false,
            }
        };
        if valid {
            return Ok(AssignmentTargetsV1::Single(id));
        }
        if let Some(N::Matrix {
            rows: Some(rowlist),
        }) = self.kind(id)
        {
            if let Some(N::List {
                item: row,
                next: None,
            }) = self.kind(rowlist)
            {
                if let Some(N::Row { items: Some(items) }) = self.kind(row) {
                    let mut cur = Some(items);
                    while let Some(i) = cur {
                        if let Some(N::List { item, next }) = self.kind(i) {
                            if !matches!(self.kind(item), Some(N::Name)) {
                                return Err(self.budget.error(
                                    self.span(item),
                                    ProgramParseErrorCodeV1::InvalidAssignment,
                                ));
                            }
                            cur = next
                        }
                    }
                    return Ok(AssignmentTargetsV1::Multiple(items));
                }
            }
        }
        Err(self
            .budget
            .error(self.span(id), ProgramParseErrorCodeV1::InvalidAssignment))
    }
    fn expr(&mut self, min: u8, soft: bool, matrix: bool) -> ParseResult<NodeIdV1> {
        self.budget.check(self.t().span)?;
        if soft {
            self.soft()?
        }
        let t = self.t();
        let mut left = match t.kind {
            K::Number => {
                self.bump()?;
                self.budget.stats.numeric_literals = self.budget.count(
                    self.budget.stats.numeric_literals,
                    MAX_PROGRAM_NUMBERS_V1,
                    t.span,
                )?;
                self.budget.check(t.span)?;
                let value = self.source[t.span.start..t.span.end]
                    .parse::<f64>()
                    .map_err(|_| self.budget.error(t.span, ProgramParseErrorCodeV1::Syntax))?;
                self.budget.check(t.span)?;
                self.node(
                    N::Number {
                        bits: value.to_bits(),
                    },
                    t.span,
                )?
            }
            K::Name => {
                if self.word("end") && self.end_context > 0 {
                    self.bump()?;
                    self.node(N::End, t.span)?
                } else {
                    self.name()?
                }
            }
            K::Plus | K::Minus | K::Not => {
                self.enter()?;
                self.bump()?;
                let input = self.expr(min.max(10), soft, matrix)?;
                self.depth -= 1;
                let operator = match t.kind {
                    K::Plus => UnaryOperatorV1::Plus,
                    K::Minus => UnaryOperatorV1::Minus,
                    _ => UnaryOperatorV1::Not,
                };
                self.node(
                    N::Unary { operator, input },
                    t.span.through(self.span(input)),
                )?
            }
            K::LParen => {
                self.enter()?;
                self.bump()?;
                let value = self.expr(0, true, false)?;
                self.soft()?;
                let end = self.need(K::RParen)?.span;
                self.depth -= 1;
                self.node(N::Group { value }, t.span.through(end))?
            }
            K::LBracket => {
                self.enter()?;
                let value = self.matrix()?;
                self.depth -= 1;
                value
            }
            K::Quote | K::DotQuote => {
                return Err(self.err(ProgramParseErrorCodeV1::UnsupportedFeature));
            }
            _ => return Err(self.err(ProgramParseErrorCodeV1::Syntax)),
        };
        let mut power = false;
        let mut not_power = false;
        loop {
            if soft {
                self.soft()?
            }
            let next = self.t();
            if B::MEMBERS && next.kind == K::Dot {
                self.bump()?;
                let name = self.name()?;
                let span = self.span(left).through(self.span(name));
                self.budget.check(span)?;
                self.budget
                    .count(self.nodes.len(), MAX_PROGRAM_NODES_V1, span)?;
                let id = NodeIdV1(self.nodes.len() as u32);
                self.nodes.push_member(left, name, span);
                left = id;
                continue;
            }
            if next.kind == K::LParen && !(matrix && next.space) {
                self.bump()?;
                self.enter()?;
                self.end_context += 1;
                let mut args = List::default();
                self.soft()?;
                if self.k() != K::RParen {
                    loop {
                        let value = if self.k() == K::Colon {
                            let c = self.bump()?;
                            self.node(N::FullAxis, c.span)?
                        } else {
                            self.expr(0, true, false)?
                        };
                        self.append(&mut args, value)?;
                        self.soft()?;
                        if self.k() != K::Comma {
                            break;
                        }
                        self.bump()?;
                    }
                }
                let end = self.need(K::RParen)?.span;
                self.end_context -= 1;
                self.depth -= 1;
                left = self.node(
                    N::Apply {
                        callee: left,
                        arguments: args.head,
                    },
                    self.span(left).through(end),
                )?;
                continue;
            }
            if matches!(next.kind, K::Quote | K::DotQuote) {
                if next.space {
                    return Err(self.err(ProgramParseErrorCodeV1::UnsupportedFeature));
                }
                if 11 < min {
                    break;
                }
                self.bump()?;
                left = self.node(
                    N::Transpose {
                        input: left,
                        conjugate: next.kind == K::Quote,
                    },
                    self.span(left).through(next.span),
                )?;
                continue;
            }
            if matrix
                && next.space
                && matches!(next.kind, K::Plus | K::Minus)
                && self
                    .tokens
                    .get(self.pos + 1)
                    .is_some_and(|n| !n.space && n.kind != K::Newline)
            {
                break;
            }
            if next.kind == K::Colon {
                if 6 < min {
                    break;
                }
                self.bump()?;
                let middle = self.expr(7, soft, matrix)?;
                let (step, stop) = if self.k() == K::Colon {
                    self.bump()?;
                    (Some(middle), self.expr(7, soft, matrix)?)
                } else {
                    (None, middle)
                };
                if self.k() == K::Colon {
                    return Err(self.err(ProgramParseErrorCodeV1::Syntax));
                }
                left = self.node(
                    N::Range {
                        start: left,
                        step,
                        stop,
                    },
                    self.span(left).through(self.span(stop)),
                )?;
                continue;
            }
            let Some((precedence, operator)) = binary(next.kind) else {
                break;
            };
            if precedence < min {
                break;
            }
            if precedence == 11 && not_power {
                return Err(self.err(ProgramParseErrorCodeV1::UnsupportedFeature));
            }
            self.bump()?;
            let signed_not = self.k() == K::Not;
            if precedence == 11 && power && signed_not {
                return Err(self.err(ProgramParseErrorCodeV1::UnsupportedFeature));
            }
            let right = self.expr(
                if precedence == 11 && signed_not {
                    10
                } else {
                    precedence + 1
                },
                soft,
                matrix,
            )?;
            if precedence == 11 {
                // Prefix signs must not hide a mixed tilde/power chain.
                // Groups deliberately end this bounded unary-spine walk.
                let mut prefix = right;
                let mut right_not = false;
                while let Some(N::Unary { operator, input }) = self.kind(prefix) {
                    self.budget.check(self.span(prefix))?;
                    right_not |= operator == UnaryOperatorV1::Not;
                    prefix = input;
                }
                if right_not
                    && (power
                        || matches!(
                            self.kind(prefix),
                            Some(N::Binary {
                                operator: BinaryOperatorV1::Power | BinaryOperatorV1::ElementPower,
                                ..
                            })
                        ))
                {
                    return Err(self.budget.error(
                        self.span(right),
                        ProgramParseErrorCodeV1::UnsupportedFeature,
                    ));
                }
                power = true;
                not_power = right_not;
            }
            left = self.node(
                N::Binary {
                    operator,
                    left,
                    right,
                },
                self.span(left).through(self.span(right)),
            )?;
        }
        Ok(left)
    }
    fn matrix(&mut self) -> ParseResult<NodeIdV1> {
        let start = self.need(K::LBracket)?.span;
        let mut rows = List::default();
        if self.k() == K::RBracket {
            let end = self.bump()?.span;
            return self.node(N::Matrix { rows: None }, start.through(end));
        }
        let mut items = List::default();
        loop {
            let value = self.expr(0, false, true)?;
            self.append(&mut items, value)?;
            match self.k() {
                K::RBracket => {
                    let row = self.node(
                        N::Row { items: items.head },
                        self.span(items.head.unwrap()).through(self.span(value)),
                    )?;
                    self.append(&mut rows, row)?;
                    let end = self.bump()?.span;
                    return self.node(N::Matrix { rows: rows.head }, start.through(end));
                }
                K::Semi | K::Newline => {
                    let row = self.node(
                        N::Row { items: items.head },
                        self.span(items.head.unwrap()).through(self.span(value)),
                    )?;
                    self.append(&mut rows, row)?;
                    items = List::default();
                    self.bump()?;
                    if self.k() == K::RBracket {
                        return Err(self.err(ProgramParseErrorCodeV1::Syntax));
                    }
                }
                K::Comma => {
                    self.bump()?;
                }
                _ if self.t().space
                    && matches!(
                        self.k(),
                        K::Name | K::Number | K::LParen | K::LBracket | K::Plus | K::Minus | K::Not
                    ) => {}
                _ => return Err(self.err(ProgramParseErrorCodeV1::Syntax)),
            }
        }
    }
}
fn binary(k: K) -> Option<(u8, BinaryOperatorV1)> {
    use BinaryOperatorV1 as B;
    Some(match k {
        K::OrOr => (1, B::ShortCircuitOr),
        K::AndAnd => (2, B::ShortCircuitAnd),
        K::Or => (3, B::Or),
        K::And => (4, B::And),
        K::Lt => (5, B::Less),
        K::Le => (5, B::LessEqual),
        K::Gt => (5, B::Greater),
        K::Ge => (5, B::GreaterEqual),
        K::EqEq => (5, B::Equal),
        K::Ne => (5, B::NotEqual),
        K::Plus => (7, B::Add),
        K::Minus => (7, B::Subtract),
        K::Star => (8, B::Multiply),
        K::Slash => (8, B::Divide),
        K::Backslash => (8, B::LeftDivide),
        K::DotStar => (8, B::ElementMultiply),
        K::DotSlash => (8, B::ElementDivide),
        K::DotBackslash => (8, B::ElementLeftDivide),
        K::Power => (11, B::Power),
        K::DotPower => (11, B::ElementPower),
        _ => return None,
    })
}
