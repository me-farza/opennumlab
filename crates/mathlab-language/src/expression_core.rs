// SPDX-License-Identifier: GPL-3.0-or-later
//! One private source/AST/parser/admission core for versioned public profiles.
#![forbid(unsafe_code)]
use crate::SourceSpan;
use crate::expression::{
    ExpectedSyntaxV1, LiteralDiagnosticCodeV1, LiteralDiagnosticDetailV1, LiteralDiagnosticV1,
    LiteralExpressionErrorV1, LiteralNodeIdV1, LiteralNodeKindV1, LiteralNodeV1, LiteralResourceV1,
    LiteralSignV1, LiteralStageV1, LiteralTokenKindV1, LoweredLiteralV1, MAX_ARRAY_BYTES_V1,
    MAX_ARRAY_DIMENSION_V1, MAX_ARRAY_ELEMENTS_V1, MAX_AST_NODES_V1, MAX_NUMBER_NODES_V1,
    MAX_OPEN_DELIMITERS_V1, MAX_SOURCE_BYTES_V1, MatrixAdmissionFailureV1,
    ParsedLiteralExpressionV1, UnsupportedSourceFormV1,
};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinaryOperator {
    Add,
    Mtimes,
    Subtract,
    Times,
    RDivide,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreTokenKind {
    Number,
    Identifier,
    Assign,
    Colon,
    Plus,
    Minus,
    Star,
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    Comma,
    Semicolon,
    DotStar,
    DotSlash,
    DotTranspose,
}
#[derive(Debug)]
pub(crate) struct CoreNode {
    pub span: SourceSpan,
    pub kind: CoreNodeKind,
}
// Session call markers have a source span but produce no expression value.
#[derive(Debug)]
pub(crate) enum CallArgument {
    Expression(LiteralNodeIdV1),
    FullAxis(SourceSpan),
}
impl CallArgument {
    pub(crate) fn expression(&self) -> Option<LiteralNodeIdV1> {
        match self { Self::Expression(id) => Some(*id), Self::FullAxis(_) => None }
    }
}
#[derive(Debug)]
pub(crate) enum CoreNodeKind {
    IndexedWrite {
        target_span: SourceSpan,
        selection_span: SourceSpan,
        arguments: Vec<CallArgument>,
        rhs: LiteralNodeIdV1,
    },
    Range {
        start: LiteralNodeIdV1,
        step: Option<LiteralNodeIdV1>,
        end: LiteralNodeIdV1,
        colon_span: SourceSpan,
        second_colon_span: Option<SourceSpan>,
    },
    Call {
        callee_span: SourceSpan,
        arguments: Vec<CallArgument>,
    },
    Identifier {
        identifier_span: SourceSpan,
    },
    Number {
        literal_span: SourceSpan,
    },
    Signed {
        sign: LiteralSignV1,
        sign_span: SourceSpan,
        number: LiteralNodeIdV1,
    },
    Unary {
        sign: LiteralSignV1,
        sign_span: SourceSpan,
        operand: LiteralNodeIdV1,
    },
    Transpose {
        operator_span: SourceSpan,
        operand: LiteralNodeIdV1,
    },
    Group {
        inner: LiteralNodeIdV1,
    },
    Matrix {
        rows: Vec<Vec<LiteralNodeIdV1>>,
    },
    Binary {
        operator: BinaryOperator,
        left: LiteralNodeIdV1,
        operator_span: SourceSpan,
        right: LiteralNodeIdV1,
    },
}
#[derive(Debug)]
pub(crate) struct CoreParsed {
    pub source: Arc<str>,
    pub nodes: Vec<CoreNode>,
    pub root: LiteralNodeIdV1,
}
impl CoreParsed {
    pub fn source(&self) -> &Arc<str> {
        &self.source
    }
    pub fn root_id(&self) -> LiteralNodeIdV1 {
        self.root
    }
    pub fn root(&self) -> &CoreNode {
        &self.nodes[self.root.index()]
    }
    pub fn nodes(&self) -> &[CoreNode] {
        &self.nodes
    }
    pub fn node(&self, id: LiteralNodeIdV1) -> Option<&CoreNode> {
        self.nodes.get(id.index())
    }
    pub fn into_legacy(
        self,
        stage: LiteralStageV1,
    ) -> Result<ParsedLiteralExpressionV1, LiteralExpressionErrorV1> {
        let mut nodes = Vec::new();
        reserve(
            &mut nodes,
            self.nodes.len(),
            &self.source,
            stage,
            &self.root().span,
        )?;
        for node in self.nodes {
            let kind = match node.kind {
                CoreNodeKind::IndexedWrite { .. } | CoreNodeKind::Identifier { .. } | CoreNodeKind::Call { .. } | CoreNodeKind::Range { .. } => {
                    unreachable!("legacy profiles reject identifiers during lexing")
                }
                CoreNodeKind::Number { literal_span } => LiteralNodeKindV1::Number {
                    digits_span: literal_span,
                },
                CoreNodeKind::Signed {
                    sign,
                    sign_span,
                    number,
                } => LiteralNodeKindV1::Signed {
                    sign,
                    sign_span,
                    number,
                },
                CoreNodeKind::Group { inner } => LiteralNodeKindV1::Group { inner },
                CoreNodeKind::Matrix { rows } => LiteralNodeKindV1::Matrix { rows },
                CoreNodeKind::Binary {
                    operator: BinaryOperator::Add,
                    left,
                    operator_span,
                    right,
                } => LiteralNodeKindV1::Add {
                    left,
                    operator_span,
                    right,
                },
                CoreNodeKind::Binary {
                    operator:
                        BinaryOperator::Mtimes
                        | BinaryOperator::Subtract
                        | BinaryOperator::Times
                        | BinaryOperator::RDivide,
                    ..
                }
                | CoreNodeKind::Unary { .. }
                | CoreNodeKind::Transpose { .. } => {
                    unreachable!("restricted profiles never project general arithmetic")
                }
            };
            nodes.push(LiteralNodeV1 {
                span: node.span,
                kind,
            });
        }
        Ok(ParsedLiteralExpressionV1::from_parts(
            self.source,
            nodes,
            self.root,
        ))
    }
}
// Borrowed normalization avoids copying either retained arena between phases.
// Matrix rows and all source spans are borrowed; IDs remain arena-local.
#[derive(Clone, Copy)]
pub(crate) struct NodeView<'a> {
    pub span: &'a SourceSpan,
    pub kind: NodeKindView<'a>,
}
#[derive(Clone, Copy)]
pub(crate) enum NodeKindView<'a> {
    IndexedWrite {
        target_span: &'a SourceSpan,
        selection_span: &'a SourceSpan,
        arguments: &'a [CallArgument],
        rhs: LiteralNodeIdV1,
    },
    Construction { rows: &'a [Vec<LiteralNodeIdV1>] },
    Range {
        start: LiteralNodeIdV1,
        step: Option<LiteralNodeIdV1>,
        end: LiteralNodeIdV1,
    },
    Call {
        callee_span: &'a SourceSpan,
        arguments: &'a [CallArgument],
    },
    Identifier {
        identifier_span: &'a SourceSpan,
    },
    Number {
        literal_span: &'a SourceSpan,
    },
    Signed {
        sign: LiteralSignV1,
        number: LiteralNodeIdV1,
    },
    Unary {
        sign: LiteralSignV1,
        operand: LiteralNodeIdV1,
    },
    Transpose {
        operator_span: &'a SourceSpan,
        operand: LiteralNodeIdV1,
    },
    Group {
        inner: LiteralNodeIdV1,
    },
    Matrix {
        rows: &'a [Vec<LiteralNodeIdV1>],
    },
    Binary {
        operator: BinaryOperator,
        left: LiteralNodeIdV1,
        operator_span: &'a SourceSpan,
        right: LiteralNodeIdV1,
    },
}
pub(crate) trait NodeArena {
    fn source(&self) -> &Arc<str>;
    fn root_id(&self) -> LiteralNodeIdV1;
    fn len(&self) -> usize;
    fn view(&self, id: LiteralNodeIdV1) -> NodeView<'_>;
}
impl NodeArena for CoreParsed {
    fn source(&self) -> &Arc<str> {
        &self.source
    }
    fn root_id(&self) -> LiteralNodeIdV1 {
        self.root
    }
    fn len(&self) -> usize {
        self.nodes.len()
    }
    fn view(&self, id: LiteralNodeIdV1) -> NodeView<'_> {
        let node = &self.nodes[id.index()];
        NodeView {
            span: &node.span,
            kind: match &node.kind {
                CoreNodeKind::Range { start, step, end, .. } => {
                    NodeKindView::Range { start:*start, step:*step, end:*end }
                }
                CoreNodeKind::IndexedWrite { target_span, selection_span, arguments, rhs } => {
                    NodeKindView::IndexedWrite { target_span, selection_span, arguments, rhs: *rhs }
                }
                CoreNodeKind::Call { callee_span, arguments } => {
                    NodeKindView::Call { callee_span, arguments }
                }
                CoreNodeKind::Identifier { identifier_span } => {
                    NodeKindView::Identifier { identifier_span }
                }
                CoreNodeKind::Number { literal_span } => NodeKindView::Number { literal_span },
                CoreNodeKind::Signed { sign, number, .. } => NodeKindView::Signed {
                    sign: *sign,
                    number: *number,
                },
                CoreNodeKind::Unary { sign, operand, .. } => NodeKindView::Unary {
                    sign: *sign,
                    operand: *operand,
                },
                CoreNodeKind::Transpose {
                    operator_span,
                    operand,
                } => NodeKindView::Transpose {
                    operator_span,
                    operand: *operand,
                },
                CoreNodeKind::Group { inner } => NodeKindView::Group { inner: *inner },
                CoreNodeKind::Matrix { rows } => {
                    // Legacy parsers only create numeric rows. Session retains the
                    // same core carrier/IDs and exposes expression rows privately.
                    if rows.iter().flatten().all(|id| matches!(self.nodes[id.index()].kind,
                        CoreNodeKind::Number { .. } | CoreNodeKind::Signed { .. })) {
                        NodeKindView::Matrix { rows }
                    } else { NodeKindView::Construction { rows } }
                }
                CoreNodeKind::Binary {
                    operator,
                    left,
                    operator_span,
                    right,
                } => NodeKindView::Binary {
                    operator: *operator,
                    left: *left,
                    operator_span,
                    right: *right,
                },
            },
        }
    }
}
impl NodeArena for ParsedLiteralExpressionV1 {
    fn source(&self) -> &Arc<str> {
        ParsedLiteralExpressionV1::source(self)
    }
    fn root_id(&self) -> LiteralNodeIdV1 {
        ParsedLiteralExpressionV1::root_id(self)
    }
    fn len(&self) -> usize {
        self.nodes().len()
    }
    fn view(&self, id: LiteralNodeIdV1) -> NodeView<'_> {
        let node = self.node(id).expect("IDs belong to their original arena");
        NodeView {
            span: &node.span,
            kind: match &node.kind {
                LiteralNodeKindV1::Number { digits_span } => NodeKindView::Number {
                    literal_span: digits_span,
                },
                LiteralNodeKindV1::Signed { sign, number, .. } => NodeKindView::Signed {
                    sign: *sign,
                    number: *number,
                },
                LiteralNodeKindV1::Group { inner } => NodeKindView::Group { inner: *inner },
                LiteralNodeKindV1::Matrix { rows } => NodeKindView::Matrix { rows },
                LiteralNodeKindV1::Add {
                    left,
                    operator_span,
                    right,
                } => NodeKindView::Binary {
                    operator: BinaryOperator::Add,
                    left: *left,
                    operator_span,
                    right: *right,
                },
            },
        }
    }
}
#[derive(Debug)]
pub(crate) struct CoreIr {
    pub parsed: CoreParsed,
    pub literals: Vec<Option<LoweredLiteralV1>>,
}
impl CoreIr {
    pub fn source(&self) -> &Arc<str> {
        self.parsed.source()
    }
    pub fn parsed(&self) -> &CoreParsed {
        &self.parsed
    }
}
#[derive(Debug)]
pub(crate) enum CoreParseError {
    Shared(LiteralExpressionErrorV1),
    Syntax {
        source: Arc<str>,
        code: LiteralDiagnosticCodeV1,
        span: SourceSpan,
        expected: ExpectedSyntaxV1,
        found: Option<CoreTokenKind>,
    },
}
impl From<LiteralExpressionErrorV1> for CoreParseError {
    fn from(error: LiteralExpressionErrorV1) -> Self {
        Self::Shared(error)
    }
}
impl From<LiteralDiagnosticV1> for CoreParseError {
    fn from(error: LiteralDiagnosticV1) -> Self {
        Self::Shared(error.into())
    }
}
impl From<CoreParseError> for LiteralExpressionErrorV1 {
    fn from(error: CoreParseError) -> Self {
        match error {
            CoreParseError::Shared(error) => error,
            CoreParseError::Syntax {
                source,
                code,
                span,
                expected,
                found,
            } => {
                let found = found.map(|kind| match kind {
                    CoreTokenKind::Number => LiteralTokenKindV1::Digits,
                    CoreTokenKind::Plus => LiteralTokenKindV1::Plus,
                    CoreTokenKind::Minus => LiteralTokenKindV1::Minus,
                    CoreTokenKind::OpenParen => LiteralTokenKindV1::OpenParen,
                    CoreTokenKind::CloseParen => LiteralTokenKindV1::CloseParen,
                    CoreTokenKind::OpenBracket => LiteralTokenKindV1::OpenBracket,
                    CoreTokenKind::CloseBracket => LiteralTokenKindV1::CloseBracket,
                    CoreTokenKind::Comma => LiteralTokenKindV1::Comma,
                    CoreTokenKind::Semicolon => LiteralTokenKindV1::Semicolon,
                    CoreTokenKind::Identifier | CoreTokenKind::Assign | CoreTokenKind::Colon => {
                        unreachable!("legacy profiles reject session tokens during lexing")
                    }
                    CoreTokenKind::Star
                    | CoreTokenKind::DotStar
                    | CoreTokenKind::DotSlash
                    | CoreTokenKind::DotTranspose => {
                        unreachable!("restricted profiles lexically reject arithmetic operators")
                    }
                });
                LiteralDiagnosticV1 {
                    source,
                    code,
                    span,
                    stage: LiteralStageV1::Parse,
                    detail: LiteralDiagnosticDetailV1::Syntax { expected, found },
                }
                .into()
            }
        }
    }
}

pub(crate) fn diagnostic(
    source: &Arc<str>,
    code: LiteralDiagnosticCodeV1,
    stage: LiteralStageV1,
    span: SourceSpan,
    detail: LiteralDiagnosticDetailV1,
) -> LiteralDiagnosticV1 {
    LiteralDiagnosticV1 {
        source: Arc::clone(source),
        code,
        stage,
        span,
        detail,
    }
}

fn resource(
    source: &Arc<str>,
    stage: LiteralStageV1,
    span: SourceSpan,
    name: LiteralResourceV1,
    limit: Option<usize>,
    attempted_count: usize,
) -> LiteralDiagnosticV1 {
    diagnostic(
        source,
        LiteralDiagnosticCodeV1::ResourceLimit,
        stage,
        span,
        LiteralDiagnosticDetailV1::Resource {
            name,
            limit,
            attempted_count,
        },
    )
}

fn span(start: usize, end: usize, line: usize) -> SourceSpan {
    SourceSpan {
        start,
        end,
        start_line: line,
        end_line: line,
    }
}

fn cover(first: &SourceSpan, last: &SourceSpan) -> SourceSpan {
    SourceSpan {
        start: first.start,
        end: last.end,
        start_line: first.start_line,
        end_line: last.end_line,
    }
}

fn reserve<T>(
    values: &mut Vec<T>,
    additional: usize,
    source: &Arc<str>,
    stage: LiteralStageV1,
    at: &SourceSpan,
) -> Result<(), LiteralExpressionErrorV1> {
    values
        .try_reserve_exact(additional)
        .map_err(|cause| LiteralExpressionErrorV1::Allocation {
            source: Arc::clone(source),
            stage,
            span: at.clone(),
            requested_items: values.len().saturating_add(additional),
            cause,
        })
}

fn push<T>(
    values: &mut Vec<T>,
    value: T,
    source: &Arc<str>,
    stage: LiteralStageV1,
    at: &SourceSpan,
) -> Result<(), LiteralExpressionErrorV1> {
    if values.len() == values.capacity() {
        reserve(values, 1, source, stage, at)?;
    }
    values.push(value);
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct Frame {
    start: usize,
    end: usize,
    line: usize,
}

fn framing_error(
    source: &Arc<str>,
    at: SourceSpan,
    reason: UnsupportedSourceFormV1,
) -> LiteralDiagnosticV1 {
    diagnostic(
        source,
        LiteralDiagnosticCodeV1::UnsupportedSourceForm,
        LiteralStageV1::Framing,
        at,
        LiteralDiagnosticDetailV1::SourceForm(reason),
    )
}

fn frame(source: &Arc<str>) -> Result<Frame, LiteralExpressionErrorV1> {
    if source.len() > MAX_SOURCE_BYTES_V1 {
        return Err(resource(
            source,
            LiteralStageV1::Framing,
            span(0, 0, 1),
            LiteralResourceV1::SourceBytes,
            Some(MAX_SOURCE_BYTES_V1),
            source.len(),
        )
        .into());
    }
    if source.starts_with('\u{feff}') {
        return Err(framing_error(
            source,
            span(0, 3, 1),
            UnsupportedSourceFormV1::ByteOrderMark,
        )
        .into());
    }
    let bytes = source.as_bytes();
    let mut line_start = 0;
    let mut line = 1;
    let mut code = None;
    while line_start < bytes.len() {
        let lf = bytes[line_start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|offset| line_start + offset);
        let physical_end = lf.unwrap_or(bytes.len());
        let content_end =
            if lf.is_some() && physical_end > line_start && bytes[physical_end - 1] == b'\r' {
                physical_end - 1
            } else {
                physical_end
            };
        let first = (line_start..content_end).find(|&i| !matches!(bytes[i], b' ' | b'\t'));
        let lone_cr = (line_start..content_end).find(|&i| bytes[i] == b'\r');

        // Choose the earliest framing violation on this physical line. All
        // framing completes before lexical or parse diagnostics are considered.
        let mut failure = lone_cr.map(|i| (i, i + 1, UnsupportedSourceFormV1::LoneCarriageReturn));
        if let Some(first) = first {
            let marker = bytes[first] == b'%'
                && first + 1 < content_end
                && matches!(bytes[first + 1], b'{' | b'}');
            let candidate = if marker {
                Some((
                    first,
                    first + 2,
                    UnsupportedSourceFormV1::BlockCommentMarker,
                ))
            } else if bytes[first] != b'%' && code.is_some() {
                let width = source[first..]
                    .chars()
                    .next()
                    .expect("nonempty line")
                    .len_utf8();
                Some((
                    first,
                    first + width,
                    UnsupportedSourceFormV1::SecondCodeLine,
                ))
            } else {
                None
            };
            if let Some(candidate) = candidate {
                if failure
                    .as_ref()
                    .is_none_or(|current| candidate.0 < current.0)
                {
                    failure = Some(candidate);
                }
            }
            if let Some((start, end, reason)) = failure {
                return Err(framing_error(source, span(start, end, line), reason).into());
            }
            if bytes[first] != b'%' {
                let comment = bytes[first..content_end]
                    .iter()
                    .position(|&b| b == b'%')
                    .map(|offset| first + offset)
                    .unwrap_or(content_end);
                let mut end = comment;
                while end > first && matches!(bytes[end - 1], b' ' | b'\t') {
                    end -= 1;
                }
                code = Some(Frame {
                    start: first,
                    end,
                    line,
                });
            }
        } else if let Some((start, end, reason)) = failure {
            return Err(framing_error(source, span(start, end, line), reason).into());
        }
        match lf {
            Some(lf) => {
                line_start = lf + 1;
                line += 1;
            }
            None => break,
        }
    }
    code.ok_or_else(|| {
        diagnostic(
            source,
            LiteralDiagnosticCodeV1::ExpectedExpression,
            LiteralStageV1::Framing,
            span(source.len(), source.len(), line),
            LiteralDiagnosticDetailV1::Syntax {
                expected: ExpectedSyntaxV1::Primary,
                found: None,
            },
        )
        .into()
    })
}

#[derive(Debug, Clone, Copy)]
struct Token {
    kind: CoreTokenKind,
    start: usize,
    end: usize,
}

impl Token {
    fn span(self, line: usize) -> SourceSpan {
        span(self.start, self.end, line)
    }
}

pub(crate) fn scan_exact_integer(
    source: &Arc<str>,
    start: usize,
    end: usize,
    _line: usize,
) -> Option<Result<usize, LiteralExpressionErrorV1>> {
    let bytes = source.as_bytes();
    if !bytes[start].is_ascii_digit() {
        return None;
    }
    let mut cursor = start + 1;
    while cursor < end && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }
    Some(Ok(cursor))
}

// The arithmetic profile gives adjacent dot operators priority over a decimal
// suffix dot. Legacy scanners/lexing never enable this boundary.
pub(crate) fn dot_operator_at(bytes: &[u8], start: usize, end: usize) -> Option<CoreTokenKind> {
    if end.saturating_sub(start) < 2 || bytes[start] != b'.' {
        return None;
    }
    match bytes[start + 1] {
        b'*' => Some(CoreTokenKind::DotStar),
        b'/' => Some(CoreTokenKind::DotSlash),
        b'\'' => Some(CoreTokenKind::DotTranspose),
        _ => None,
    }
}

fn lex_with_number_scanner<E>(
    source: &Arc<str>,
    frame: Frame,
    scan_number: fn(&Arc<str>, usize, usize, usize) -> Option<Result<usize, E>>,
    arithmetic: bool,
    session: bool,
) -> Result<Vec<Token>, E>
where
    E: From<LiteralExpressionErrorV1> + From<LiteralDiagnosticV1>,
{
    let mut tokens = Vec::new();
    reserve(
        &mut tokens,
        frame.end - frame.start,
        source,
        LiteralStageV1::Lex,
        &span(frame.start, frame.start, frame.line),
    )?;
    let bytes = source.as_bytes();
    let mut i = frame.start;
    while i < frame.end {
        let start = i;
        if arithmetic {
            if let Some(kind) = dot_operator_at(bytes, i, frame.end) {
                i += 2;
                tokens.push(Token {
                    kind,
                    start,
                    end: i,
                });
                continue;
            }
        }
        if let Some(number) = scan_number(source, i, frame.end, frame.line) {
            i = number?;
            // Each public profile projects its own numeric-token/span terminology.
            tokens.push(Token {
                kind: CoreTokenKind::Number,
                start,
                end: i,
            });
            continue;
        }
        let kind = match bytes[i] {
            b' ' | b'\t' => {
                i += 1;
                continue;
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                i += 1;
                while i < frame.end && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                if session {
                    tokens.push(Token {
                        kind: CoreTokenKind::Identifier,
                        start,
                        end: i,
                    });
                    continue;
                }
                return Err(diagnostic(
                    source,
                    LiteralDiagnosticCodeV1::UnsupportedIdentifier,
                    LiteralStageV1::Lex,
                    span(start, i, frame.line),
                    LiteralDiagnosticDetailV1::Identifier,
                )
                .into());
            }
            byte => {
                let kind = match byte {
                    b'=' if session => CoreTokenKind::Assign,
                    b':' if session => CoreTokenKind::Colon,
                    b'+' => CoreTokenKind::Plus,
                    b'-' => CoreTokenKind::Minus,
                    b'*' if arithmetic => CoreTokenKind::Star,
                    b'(' => CoreTokenKind::OpenParen,
                    b')' => CoreTokenKind::CloseParen,
                    b'[' => CoreTokenKind::OpenBracket,
                    b']' => CoreTokenKind::CloseBracket,
                    b',' => CoreTokenKind::Comma,
                    b';' => CoreTokenKind::Semicolon,
                    _ => {
                        let ch = source[i..]
                            .chars()
                            .next()
                            .expect("nonempty expression region");
                        return Err(diagnostic(
                            source,
                            LiteralDiagnosticCodeV1::UnsupportedToken,
                            LiteralStageV1::Lex,
                            span(i, i + ch.len_utf8(), frame.line),
                            LiteralDiagnosticDetailV1::Character(ch),
                        )
                        .into());
                    }
                };
                i += 1;
                kind
            }
        };
        tokens.push(Token {
            kind,
            start,
            end: i,
        });
    }
    Ok(tokens)
}

struct Parser<'a> {
    source: &'a Arc<str>,
    frame: Frame,
    tokens: &'a [Token],
    position: usize,
    charged_nodes: usize,
    numbers: usize,
    depth: usize,
    arithmetic: bool,
    session: bool,
    bracket_entry: bool,
    nodes: Vec<CoreNode>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.position).copied()
    }

    fn at(&self) -> SourceSpan {
        self.peek()
            .map(|token| token.span(self.frame.line))
            .unwrap_or_else(|| span(self.frame.end, self.frame.end, self.frame.line))
    }

    fn expected(&self, expected: ExpectedSyntaxV1) -> CoreParseError {
        let found = self.peek().map(|token| token.kind);
        let code = if found.is_some() {
            LiteralDiagnosticCodeV1::UnexpectedToken
        } else if matches!(
            expected,
            ExpectedSyntaxV1::Primary | ExpectedSyntaxV1::Number
        ) {
            LiteralDiagnosticCodeV1::ExpectedExpression
        } else {
            LiteralDiagnosticCodeV1::UnexpectedEof
        };
        CoreParseError::Syntax {
            source: Arc::clone(self.source),
            code,
            span: self.at(),
            expected,
            found,
        }
    }

    fn charge(&mut self, opener: bool, number: bool) -> Result<(), CoreParseError> {
        // Check before consuming: opener depth, then every AST node, then Number.
        for (name, actual, limit) in [
            (
                LiteralResourceV1::OpenDelimiters,
                self.depth + usize::from(opener),
                MAX_OPEN_DELIMITERS_V1,
            ),
            (
                LiteralResourceV1::AstNodes,
                self.charged_nodes + 1,
                MAX_AST_NODES_V1,
            ),
            (
                LiteralResourceV1::NumberNodes,
                self.numbers + usize::from(number),
                MAX_NUMBER_NODES_V1,
            ),
        ] {
            if actual > limit {
                return Err(resource(
                    self.source,
                    LiteralStageV1::Parse,
                    self.at(),
                    name,
                    Some(limit),
                    actual,
                )
                .into());
            }
        }
        self.depth += usize::from(opener);
        self.charged_nodes += 1;
        self.numbers += usize::from(number);
        Ok(())
    }

    fn store(&mut self, node: CoreNode) -> LiteralNodeIdV1 {
        // The arena was fallibly reserved for min(source bytes, AST cap).
        // Every node has a distinct introducing ASCII token in the source.
        let id = LiteralNodeIdV1::from_index(self.nodes.len());
        self.nodes.push(node);
        id
    }

    fn expression(&mut self) -> Result<LiteralNodeIdV1, CoreParseError> {
        let start = self.sum_expression()?;
        if !self.session || !self.peek().is_some_and(|token| token.kind==CoreTokenKind::Colon) {
            return Ok(start);
        }
        self.charge(false,false)?;
        let colon_span = self.at();
        self.position += 1;
        let middle = self.sum_expression()?;
        let (step,end,second_colon_span) = if self.peek().is_some_and(|token| token.kind==CoreTokenKind::Colon) {
            self.charge(false,false)?;
            let second = self.at();
            self.position += 1;
            (Some(middle),self.sum_expression()?,Some(second))
        } else {
            (None,middle,None)
        };
        Ok(self.store(CoreNode {
            span:cover(&self.nodes[start.index()].span,&self.nodes[end.index()].span),
            kind:CoreNodeKind::Range {start,step,end,colon_span,second_colon_span},
        }))
    }

    fn ascii_gap_before(&self) -> bool {
        self.position > 0 && self.peek().is_some_and(|next| {
            let gap = &self.source.as_bytes()[self.tokens[self.position - 1].end..next.start];
            !gap.is_empty() && gap.iter().all(|&b| matches!(b, b' ' | b'\t'))
        })
    }

    fn attached_prefix_after_gap(&self) -> bool {
        if !self.ascii_gap_before() { return false; }
        let mut at = self.position;
        while let Some(sign) = self.tokens.get(at).filter(|t|
            matches!(t.kind, CoreTokenKind::Plus | CoreTokenKind::Minus)) {
            let Some(next) = self.tokens.get(at + 1) else { return false; };
            if sign.end != next.start { return false; }
            at += 1;
        }
        self.tokens.get(at).is_some_and(|t| matches!(t.kind,
            CoreTokenKind::Number | CoreTokenKind::Identifier |
            CoreTokenKind::OpenParen | CoreTokenKind::OpenBracket))
    }

    fn sum_expression(&mut self) -> Result<LiteralNodeIdV1, CoreParseError> {
        let mut left = self.product()?;
        while self.peek().is_some_and(|token| {
            token.kind == CoreTokenKind::Plus
                || (self.arithmetic && token.kind == CoreTokenKind::Minus)
        }) {
            if self.bracket_entry && self.attached_prefix_after_gap() { break; }
            self.charge(false, false)?;
            let operator_span = self.at();
            let operator = match self.peek().expect("binary token").kind {
                CoreTokenKind::Plus => BinaryOperator::Add,
                CoreTokenKind::Minus => BinaryOperator::Subtract,
                _ => unreachable!("sum loop accepts plus or arithmetic minus"),
            };
            self.position += 1;
            let right = self.product()?;
            left = self.store(CoreNode {
                span: cover(
                    &self.nodes[left.index()].span,
                    &self.nodes[right.index()].span,
                ),
                kind: CoreNodeKind::Binary {
                    operator,
                    left,
                    operator_span,
                    right,
                },
            });
        }
        Ok(left)
    }

    fn product(&mut self) -> Result<LiteralNodeIdV1, CoreParseError> {
        let mut left = self.prefix()?;
        while self.peek().is_some_and(|token| {
            matches!(
                token.kind,
                CoreTokenKind::Star | CoreTokenKind::DotStar | CoreTokenKind::DotSlash
            )
        }) {
            self.charge(false, false)?;
            let operator_span = self.at();
            let operator = match self.peek().expect("product token").kind {
                CoreTokenKind::Star => BinaryOperator::Mtimes,
                CoreTokenKind::DotStar => BinaryOperator::Times,
                CoreTokenKind::DotSlash => BinaryOperator::RDivide,
                _ => unreachable!("product loop accepts only product operators"),
            };
            self.position += 1;
            let right = self.prefix()?;
            left = self.store(CoreNode {
                span: cover(
                    &self.nodes[left.index()].span,
                    &self.nodes[right.index()].span,
                ),
                kind: CoreNodeKind::Binary {
                    operator,
                    left,
                    operator_span,
                    right,
                },
            });
        }
        Ok(left)
    }

    fn prefix(&mut self) -> Result<LiteralNodeIdV1, CoreParseError> {
        if !self.arithmetic {
            return self.primary();
        }
        let first = self.position;
        while self
            .peek()
            .is_some_and(|token| matches!(token.kind, CoreTokenKind::Plus | CoreTokenKind::Minus))
        {
            // Every prefix is charged at its source token, before its operand.
            self.charge(false, false)?;
            self.position += 1;
        }
        let end = self.position;
        let mut operand = self.postfix()?;
        // Reverse a token range rather than recursing or allocating a sign stack.
        // The innermost direct signed number retains its historical Signed shape.
        for position in (first..end).rev() {
            let token = self.tokens[position];
            let sign = match token.kind {
                CoreTokenKind::Plus => LiteralSignV1::Plus,
                CoreTokenKind::Minus => LiteralSignV1::Minus,
                _ => unreachable!("prefix range contains only signs"),
            };
            let sign_span = token.span(self.frame.line);
            let span = cover(&sign_span, &self.nodes[operand.index()].span);
            let kind = if matches!(
                &self.nodes[operand.index()].kind,
                CoreNodeKind::Number { .. }
            ) {
                CoreNodeKind::Signed {
                    sign,
                    sign_span,
                    number: operand,
                }
            } else {
                CoreNodeKind::Unary {
                    sign,
                    sign_span,
                    operand,
                }
            };
            operand = self.store(CoreNode { span, kind });
        }
        Ok(operand)
    }

    fn postfix(&mut self) -> Result<LiteralNodeIdV1, CoreParseError> {
        let mut operand = self.primary()?;
        // Postfix chains charge one logical node per two-byte operator without
        // growing parser recursion or the open-delimiter depth.
        while self
            .peek()
            .is_some_and(|token| token.kind == CoreTokenKind::DotTranspose)
        {
            self.charge(false, false)?;
            let operator_span = self.at();
            self.position += 1;
            operand = self.store(CoreNode {
                span: cover(&self.nodes[operand.index()].span, &operator_span),
                kind: CoreNodeKind::Transpose {
                    operator_span,
                    operand,
                },
            });
        }
        Ok(operand)
    }

    fn primary(&mut self) -> Result<LiteralNodeIdV1, CoreParseError> {
        match self.peek().map(|token| token.kind) {
            Some(CoreTokenKind::Identifier) if self.session => {
                self.charge(false, false)?;
                let at = self.at();
                self.position += 1;
                if self.peek().is_some_and(|token| token.kind == CoreTokenKind::OpenParen)
                    && !(self.bracket_entry && self.ascii_gap_before()) {
                    self.call(at)
                } else {
                    Ok(self.store(CoreNode {
                        span: at.clone(),
                        kind: CoreNodeKind::Identifier { identifier_span: at },
                    }))
                }
            }
            Some(CoreTokenKind::Number | CoreTokenKind::Plus | CoreTokenKind::Minus) => {
                self.signed()
            }
            Some(CoreTokenKind::OpenParen) => {
                self.charge(true, false)?;
                let start = self.at();
                self.position += 1;
                let outer_boundary = self.bracket_entry;
                self.bracket_entry = false;
                let inner = self.expression()?;
                self.bracket_entry = outer_boundary;
                if !self
                    .peek()
                    .is_some_and(|token| token.kind == CoreTokenKind::CloseParen)
                {
                    return Err(self.expected(ExpectedSyntaxV1::CloseParen).into());
                }
                let end = self.at();
                self.position += 1;
                self.depth -= 1;
                Ok(self.store(CoreNode {
                    span: cover(&start, &end),
                    kind: CoreNodeKind::Group { inner },
                }))
            }
            Some(CoreTokenKind::OpenBracket) => self.matrix(),
            _ => Err(self.expected(ExpectedSyntaxV1::Primary).into()),
        }
    }

    fn call(&mut self, callee_span: SourceSpan) -> Result<LiteralNodeIdV1, CoreParseError> {
        // Charge the call delimiter and each argument in addition to its expression.
        // The caller already charged the callee. Only session primaries reach here.
        self.charge(true, false)?;
        self.position += 1;
        let outer_boundary = self.bracket_entry;
        self.bracket_entry = false;
        let mut arguments = Vec::new();
        if !self.peek().is_some_and(|token| token.kind == CoreTokenKind::CloseParen) {
            loop {
                self.charge(false, false)?;
                let argument = if self.peek().is_some_and(|token| token.kind == CoreTokenKind::Colon)
                    && self.tokens.get(self.position + 1).is_some_and(|token|
                        matches!(token.kind, CoreTokenKind::Comma | CoreTokenKind::CloseParen)) {
                    let span = self.at();
                    self.position += 1;
                    CallArgument::FullAxis(span)
                } else { CallArgument::Expression(self.expression()?) };
                let span = match &argument {
                    CallArgument::Expression(id) => self.nodes[id.index()].span.clone(),
                    CallArgument::FullAxis(span) => span.clone(),
                };
                push(&mut arguments, argument, self.source, LiteralStageV1::Parse, &span)?;
                if !self.peek().is_some_and(|token| token.kind == CoreTokenKind::Comma) {
                    break;
                }
                self.position += 1;
            }
        }
        if !self.peek().is_some_and(|token| token.kind == CoreTokenKind::CloseParen) {
            return Err(self.expected(ExpectedSyntaxV1::CloseParen));
        }
        let end = self.at();
        self.position += 1;
        self.depth -= 1;
        self.bracket_entry = outer_boundary;
        Ok(self.store(CoreNode {
            span: cover(&callee_span, &end),
            kind: CoreNodeKind::Call { callee_span, arguments },
        }))
    }

    fn signed(&mut self) -> Result<LiteralNodeIdV1, CoreParseError> {
        let sign = match self.peek().map(|token| token.kind) {
            Some(CoreTokenKind::Plus) => Some(LiteralSignV1::Plus),
            Some(CoreTokenKind::Minus) => Some(LiteralSignV1::Minus),
            _ => None,
        };
        let sign_span = if sign.is_some() {
            self.charge(false, false)?;
            let at = self.at();
            self.position += 1;
            Some(at)
        } else {
            None
        };
        if !self
            .peek()
            .is_some_and(|token| token.kind == CoreTokenKind::Number)
        {
            return Err(self.expected(ExpectedSyntaxV1::Number).into());
        }
        self.charge(false, true)?;
        let literal_span = self.at();
        self.position += 1;
        let number = self.store(CoreNode {
            span: literal_span.clone(),
            kind: CoreNodeKind::Number { literal_span },
        });
        if let (Some(sign), Some(sign_span)) = (sign, sign_span) {
            Ok(self.store(CoreNode {
                span: cover(&sign_span, &self.nodes[number.index()].span),
                kind: CoreNodeKind::Signed {
                    sign,
                    sign_span,
                    number,
                },
            }))
        } else {
            Ok(number)
        }
    }

    fn matrix(&mut self) -> Result<LiteralNodeIdV1, CoreParseError> {
        if !self.session { return self.numeric_matrix(); }
        self.charge(true, false)?;
        let start = self.at();
        self.position += 1;
        let outer_boundary = self.bracket_entry;
        self.bracket_entry = true;
        let mut rows = Vec::new();
        if !self.peek().is_some_and(|t| t.kind == CoreTokenKind::CloseBracket) {
            loop {
                let mut row = Vec::new();
                loop {
                    let entry = self.expression()?;
                    push(&mut row, entry, self.source, LiteralStageV1::Parse,
                        &self.nodes[entry.index()].span)?;
                    if self.peek().is_some_and(|t| t.kind == CoreTokenKind::Comma) {
                        self.position += 1;
                    } else if !(self.ascii_gap_before() && self.peek().is_some_and(|t|
                        matches!(t.kind, CoreTokenKind::Number | CoreTokenKind::Identifier |
                            CoreTokenKind::OpenParen | CoreTokenKind::OpenBracket |
                            CoreTokenKind::Plus | CoreTokenKind::Minus))) {
                        break;
                    }
                }
                push(&mut rows, row, self.source, LiteralStageV1::Parse, &start)?;
                if self.peek().is_some_and(|t| t.kind == CoreTokenKind::Semicolon) {
                    self.position += 1;
                } else { break; }
            }
        }
        if !self.peek().is_some_and(|t| t.kind == CoreTokenKind::CloseBracket) {
            return Err(self.expected(ExpectedSyntaxV1::MatrixSeparatorOrClose));
        }
        let end = self.at();
        self.position += 1;
        self.depth -= 1;
        self.bracket_entry = outer_boundary;
        // The private view distinguishes expression rows; numeric rows retain
        // their historical core carrier, node IDs, admission and exact bits.
        Ok(self.store(CoreNode {
            span: cover(&start, &end), kind: CoreNodeKind::Matrix { rows },
        }))
    }

    fn numeric_matrix(&mut self) -> Result<LiteralNodeIdV1, CoreParseError> {
        self.charge(true, false)?;
        let start = self.at();
        self.position += 1;
        let mut rows = Vec::new();
        if !self
            .peek()
            .is_some_and(|token| token.kind == CoreTokenKind::CloseBracket)
        {
            loop {
                let mut row = Vec::new();
                let first = self.signed()?;
                push(
                    &mut row,
                    first,
                    self.source,
                    LiteralStageV1::Parse,
                    &self.nodes[first.index()].span,
                )?;
                loop {
                    if self.peek().is_some_and(|token| token.kind == CoreTokenKind::Comma) {
                        self.position += 1;
                    } else {
                        // Only Session numeric matrices gain implicit separators.
                        // signed() already consumed a Number, so its token ends the entry.
                        let Some(next) = self.peek().filter(|_| self.session) else { break };
                        let gap = &self.source.as_bytes()[self.tokens[self.position - 1].end..next.start];
                        let numeric = match next.kind {
                            CoreTokenKind::Number => true,
                            CoreTokenKind::Plus | CoreTokenKind::Minus => self.tokens.get(self.position + 1)
                                .is_some_and(|number| number.kind == CoreTokenKind::Number && next.end == number.start),
                            _ => false,
                        };
                        if gap.is_empty() || !gap.iter().all(|&byte| matches!(byte, b' ' | b'\t')) || !numeric {
                            break;
                        }
                    }
                    let entry = self.signed()?;
                    push(
                        &mut row,
                        entry,
                        self.source,
                        LiteralStageV1::Parse,
                        &self.nodes[entry.index()].span,
                    )?;
                }
                push(&mut rows, row, self.source, LiteralStageV1::Parse, &start)?;
                if self
                    .peek()
                    .is_some_and(|token| token.kind == CoreTokenKind::Semicolon)
                {
                    self.position += 1;
                } else {
                    break;
                }
            }
        }
        if !self
            .peek()
            .is_some_and(|token| token.kind == CoreTokenKind::CloseBracket)
        {
            return Err(self
                .expected(ExpectedSyntaxV1::MatrixSeparatorOrClose)
                .into());
        }
        let end = self.at();
        self.position += 1;
        self.depth -= 1;
        Ok(self.store(CoreNode {
            span: cover(&start, &end),
            kind: CoreNodeKind::Matrix { rows },
        }))
    }
}

// Shared grammar, framing, resources and AST construction. This carrier never
// escapes as a public v1 value when called by a different numeric profile.
pub(crate) fn parse_core<E>(
    source: Arc<str>,
    scan_number: fn(&Arc<str>, usize, usize, usize) -> Option<Result<usize, E>>,
    arithmetic: bool,
) -> Result<CoreParsed, E>
where
    E: From<LiteralExpressionErrorV1> + From<LiteralDiagnosticV1> + From<CoreParseError>,
{
    let frame = frame(&source)?;
    let tokens = lex_with_number_scanner(&source, frame, scan_number, arithmetic, false)?;
    let mut nodes = Vec::new();
    reserve(
        &mut nodes,
        (frame.end - frame.start).min(MAX_AST_NODES_V1),
        &source,
        LiteralStageV1::Parse,
        &span(frame.start, frame.start, frame.line),
    )?;
    let mut parser = Parser {
        source: &source,
        frame,
        tokens: &tokens,
        position: 0,
        charged_nodes: 0,
        numbers: 0,
        depth: 0,
        arithmetic,
        session: false, bracket_entry: false,
        nodes,
    };
    let root = parser.expression()?;
    if parser.peek().is_some() {
        return Err(parser.expected(ExpectedSyntaxV1::EndOfExpression).into());
    }
    let nodes = parser.nodes;
    Ok(CoreParsed {
        source,
        nodes,
        root,
    })
}

/// A session statement retains its real target/name source span separately from
/// the RHS arena. Assignment and its target each charge one logical AST node.
#[derive(Debug, Clone)]
pub(crate) enum CoreStatementEffect {
    Assign(SourceSpan),
    Display(SourceSpan),
    Answer,
}
#[derive(Debug)]
pub(crate) struct CoreStatement {
    pub parsed: CoreParsed,
    pub effect: CoreStatementEffect,
    pub suppressed: bool,
}

// Only Session call-argument leaves defer the reserved lowercase keyword.
// Scan balanced parenthesis ancestry; construction brackets preserve call context.
fn bracket_entry_at(tokens: &[Token], at: usize) -> bool {
    let mut closed = 0_usize;
    for token in tokens[..at].iter().rev() {
        match token.kind {
            CoreTokenKind::CloseParen | CoreTokenKind::CloseBracket => closed += 1,
            CoreTokenKind::OpenParen | CoreTokenKind::OpenBracket if closed != 0 => closed -= 1,
            CoreTokenKind::OpenParen => return false,
            CoreTokenKind::OpenBracket => return true,
            _ => {}
        }
    }
    false
}

fn contextual_end(tokens: &[Token], at: usize) -> bool {
    if tokens.get(at + 1).is_some_and(|t| t.kind == CoreTokenKind::OpenParen) {
        return false; // A keyword never becomes a callee.
    }
    let mut parentheses = 0_usize;
    let mut in_call = false;
    for index in (0..at).rev() {
        match tokens[index].kind {
            CoreTokenKind::CloseParen => parentheses += 1,
            CoreTokenKind::OpenParen if parentheses != 0 => parentheses -= 1,
            CoreTokenKind::OpenParen => {
                in_call |= index > 0 && tokens[index - 1].kind == CoreTokenKind::Identifier
                    && !(tokens[index - 1].end < tokens[index].start && bracket_entry_at(tokens, index));
            }
            _ => {}
        }
    }
    in_call
}

pub(crate) fn parse_session_core<E>(
    source: Arc<str>,
    scan_number: fn(&Arc<str>, usize, usize, usize) -> Option<Result<usize, E>>,
    validate_identifier: fn(&Arc<str>, &SourceSpan) -> Result<(), E>,
) -> Result<CoreStatement, E>
where
    E: From<LiteralExpressionErrorV1> + From<LiteralDiagnosticV1> + From<CoreParseError>,
{
    let frame = frame(&source)?;
    let tokens = lex_with_number_scanner(&source, frame, scan_number, true, true)?;
    for (index, token) in tokens.iter().enumerate() {
        if token.kind == CoreTokenKind::Identifier {
            if &source[token.start..token.end] == "end" && contextual_end(&tokens, index) {
                continue;
            }
            validate_identifier(&source, &token.span(frame.line))?;
        }
    }
    let suppressed = tokens
        .last()
        .is_some_and(|token| token.kind == CoreTokenKind::Semicolon);
    let expression_tokens = if suppressed {
        &tokens[..tokens.len() - 1]
    } else {
        &tokens[..]
    };
    let mut nodes = Vec::new();
    reserve(
        &mut nodes,
        (frame.end - frame.start).min(MAX_AST_NODES_V1),
        &source,
        LiteralStageV1::Parse,
        &span(frame.start, frame.start, frame.line),
    )?;
    let mut parser = Parser {
        source: &source,
        frame,
        tokens: expression_tokens,
        position: 0,
        charged_nodes: 0,
        numbers: 0,
        depth: 0,
        arithmetic: true,
        session: true, bracket_entry: false,
        nodes,
    };
    let mut target = match (expression_tokens.first(), expression_tokens.get(1)) {
        (Some(name), Some(assign))
            if name.kind == CoreTokenKind::Identifier && assign.kind == CoreTokenKind::Assign =>
        {
            parser.charge(false, false)?;
            parser.position = 1;
            parser.charge(false, false)?;
            parser.position = 2;
            Some(name.span(frame.line))
        }
        _ => None,
    };
    let mut root = parser.expression()?;
    if target.is_none() && parser.peek().is_some_and(|token| token.kind == CoreTokenKind::Assign) {
        // Parse once: a direct call header becomes the private write root.
        let direct = matches!(&parser.nodes[root.index()].kind, CoreNodeKind::Call {callee_span,..}
            if expression_tokens.first().is_some_and(|t| t.kind == CoreTokenKind::Identifier
                && t.start == callee_span.start));
        if !direct { return Err(parser.expected(ExpectedSyntaxV1::EndOfExpression).into()); }
        parser.charge(false, false)?;
        parser.position += 1;
        assert_eq!(root.index() + 1, parser.nodes.len());
        let header = parser.nodes.pop().expect("direct call is the last parsed node");
        let CoreNodeKind::Call {callee_span, arguments} = header.kind else { unreachable!() };
        let rhs = parser.expression()?;
        target = Some(callee_span.clone());
        root = parser.store(CoreNode {
            span: cover(&header.span, &parser.nodes[rhs.index()].span),
            kind: CoreNodeKind::IndexedWrite {
                target_span: callee_span, selection_span: header.span, arguments, rhs,
            },
        });
    }
    if parser.peek().is_some() {
        return Err(parser.expected(ExpectedSyntaxV1::EndOfExpression).into());
    }
    let effect = if let Some(target) = target {
        CoreStatementEffect::Assign(target)
    } else if let CoreNodeKind::Identifier { identifier_span } = &parser.nodes[root.index()].kind {
        CoreStatementEffect::Display(identifier_span.clone())
    } else {
        CoreStatementEffect::Answer
    };
    let nodes = parser.nodes;
    Ok(CoreStatement {
        parsed: CoreParsed {
            source,
            nodes,
            root,
        },
        effect,
        suppressed,
    })
}

fn scalar_bits(literals: &[Option<LoweredLiteralV1>], id: LiteralNodeIdV1) -> u64 {
    match &literals[id.index()] {
        Some(LoweredLiteralV1::Scalar { bits }) => *bits,
        _ => unreachable!("parser restricts matrix entries and signs to numeric literals"),
    }
}

pub(crate) fn admit_core<E>(
    parsed: CoreParsed,
    decode_number: fn(&Arc<str>, &SourceSpan) -> Result<u64, E>,
) -> Result<CoreIr, E>
where
    E: From<LiteralExpressionErrorV1> + From<LiteralDiagnosticV1>,
{
    let literals = admit_literals(&parsed, decode_number)?;
    Ok(CoreIr { parsed, literals })
}

pub(crate) fn admit_literals<E>(
    parsed: &impl NodeArena,
    decode_number: fn(&Arc<str>, &SourceSpan) -> Result<u64, E>,
) -> Result<Vec<Option<LoweredLiteralV1>>, E>
where
    E: From<LiteralExpressionErrorV1> + From<LiteralDiagnosticV1>,
{
    let mut literals = Vec::new();
    reserve(
        &mut literals,
        parsed.len(),
        parsed.source(),
        LiteralStageV1::LiteralAdmission,
        parsed.view(parsed.root_id()).span,
    )?;
    literals.resize_with(parsed.len(), || None);

    // Number nodes are stored in source order, even across Group/Binary parents.
    for index in 0..parsed.len() {
        let node = parsed.view(LiteralNodeIdV1::from_index(index));
        if let NodeKindView::Number { literal_span } = node.kind {
            literals[index] = Some(LoweredLiteralV1::Scalar {
                bits: decode_number(parsed.source(), literal_span)?,
            });
        }
    }
    for index in 0..parsed.len() {
        let node = parsed.view(LiteralNodeIdV1::from_index(index));
        if let NodeKindView::Signed { sign, number, .. } = node.kind {
            let bits = scalar_bits(&literals, number)
                ^ if sign == LiteralSignV1::Minus {
                    1_u64 << 63
                } else {
                    0
                };
            literals[index] = Some(LoweredLiteralV1::Scalar { bits });
        }
    }

    // Numeric Matrix nodes cannot contain other matrices; their postorder
    // arena order is therefore also their opening-byte order. Check all geometry
    // before allocating any column-major constructor payload.
    for index in 0..parsed.len() {
        let node = parsed.view(LiteralNodeIdV1::from_index(index));
        if let NodeKindView::Matrix { rows } = node.kind {
            let fail = |reason| {
                diagnostic(
                    parsed.source(),
                    LiteralDiagnosticCodeV1::MatrixNotAdmitted,
                    LiteralStageV1::MatrixAdmission,
                    node.span.clone(),
                    LiteralDiagnosticDetailV1::Matrix(reason),
                )
            };
            let columns = rows.first().map_or(0, Vec::len);
            if rows.iter().any(|row| row.len() != columns) {
                let mut row_lengths = Vec::new();
                reserve(
                    &mut row_lengths,
                    rows.len(),
                    parsed.source(),
                    LiteralStageV1::MatrixAdmission,
                    node.span,
                )?;
                row_lengths.extend(rows.iter().map(Vec::len));
                return Err(fail(MatrixAdmissionFailureV1::RaggedRows { row_lengths }).into());
            }
            for (axis, actual) in [rows.len(), columns].into_iter().enumerate() {
                if actual > MAX_ARRAY_DIMENSION_V1 {
                    return Err(fail(MatrixAdmissionFailureV1::Dimension {
                        axis,
                        actual,
                        limit: MAX_ARRAY_DIMENSION_V1,
                    })
                    .into());
                }
            }
            // After the dimensions above, multiplication is bounded by 256.
            let elements = rows.len() * columns;
            if elements > MAX_ARRAY_ELEMENTS_V1 {
                return Err(fail(MatrixAdmissionFailureV1::Elements {
                    actual: elements,
                    limit: MAX_ARRAY_ELEMENTS_V1,
                })
                .into());
            }
            let bytes = elements * std::mem::size_of::<f64>();
            if bytes > MAX_ARRAY_BYTES_V1 {
                return Err(fail(MatrixAdmissionFailureV1::Bytes {
                    actual: bytes,
                    limit: MAX_ARRAY_BYTES_V1,
                })
                .into());
            }
        }
    }
    for index in 0..parsed.len() {
        let node = parsed.view(LiteralNodeIdV1::from_index(index));
        if let NodeKindView::Matrix { rows } = node.kind {
            let columns = rows.first().map_or(0, Vec::len);
            let mut column_major_bits = Vec::new();
            reserve(
                &mut column_major_bits,
                rows.len() * columns,
                parsed.source(),
                LiteralStageV1::MatrixAdmission,
                node.span,
            )?;
            for column in 0..columns {
                for row in rows {
                    column_major_bits.push(scalar_bits(&literals, row[column]));
                }
            }
            literals[index] = Some(LoweredLiteralV1::Matrix {
                shape: [rows.len() as u64, columns as u64],
                column_major_bits,
            });
        }
    }
    Ok(literals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simultaneous_limit_failures_follow_the_frozen_priority_without_admission() {
        let source: Arc<str> = Arc::from("(");
        let tokens = [Token {
            kind: CoreTokenKind::OpenParen,
            start: 0,
            end: 1,
        }];
        let mut parser = Parser {
            source: &source,
            frame: Frame {
                start: 0,
                end: 1,
                line: 1,
            },
            tokens: &tokens,
            position: 0,
            charged_nodes: 1024,
            numbers: 256,
            depth: 32,
            arithmetic: false,
            session: false, bracket_entry: false,
            nodes: Vec::new(),
        };
        let error: LiteralExpressionErrorV1 = parser.charge(true, true).unwrap_err().into();
        assert!(matches!(
            error,
            LiteralExpressionErrorV1::Diagnostic(LiteralDiagnosticV1 {
                detail: LiteralDiagnosticDetailV1::Resource {
                    name: LiteralResourceV1::OpenDelimiters,
                    attempted_count: 33,
                    ..
                },
                ..
            })
        ));
        assert_eq!(
            (
                parser.position,
                parser.depth,
                parser.charged_nodes,
                parser.numbers
            ),
            (0, 32, 1024, 256)
        );

        parser.depth = 31;
        let error: LiteralExpressionErrorV1 = parser.charge(true, true).unwrap_err().into();
        assert!(matches!(
            error,
            LiteralExpressionErrorV1::Diagnostic(LiteralDiagnosticV1 {
                detail: LiteralDiagnosticDetailV1::Resource {
                    name: LiteralResourceV1::AstNodes,
                    attempted_count: 1025,
                    ..
                },
                ..
            })
        ));
        assert_eq!(
            (
                parser.position,
                parser.depth,
                parser.charged_nodes,
                parser.numbers
            ),
            (0, 31, 1024, 256)
        );

        parser.charged_nodes = 1023;
        let error: LiteralExpressionErrorV1 = parser.charge(false, true).unwrap_err().into();
        assert!(matches!(
            error,
            LiteralExpressionErrorV1::Diagnostic(LiteralDiagnosticV1 {
                detail: LiteralDiagnosticDetailV1::Resource {
                    name: LiteralResourceV1::NumberNodes,
                    attempted_count: 257,
                    ..
                },
                ..
            })
        ));
        assert_eq!(
            (
                parser.position,
                parser.depth,
                parser.charged_nodes,
                parser.numbers
            ),
            (0, 31, 1023, 256)
        );
    }

    #[test]
    fn reservation_failure_is_a_host_error_with_the_original_source() {
        let source: Arc<str> = Arc::from("1");
        let mut values = Vec::<u8>::new();
        let error = reserve(
            &mut values,
            usize::MAX,
            &source,
            LiteralStageV1::Parse,
            &span(0, 1, 1),
        )
        .unwrap_err();
        match error {
            LiteralExpressionErrorV1::Allocation {
                source: original,
                requested_items,
                cause,
                ..
            } => {
                assert!(Arc::ptr_eq(&source, &original));
                assert_eq!(requested_items, usize::MAX);
                let _original_host_error = cause;
            }
            project => panic!("allocation failure became a project diagnostic: {project:?}"),
        }
        assert!(values.is_empty());
    }

    #[test]
    fn session_range_retains_original_colon_and_operand_spans() {
        use crate::session_expression::SessionExpressionErrorV1;
        fn scanner(source:&Arc<str>,start:usize,end:usize,line:usize)
            -> Option<Result<usize,SessionExpressionErrorV1>> {
            scan_exact_integer(source,start,end,line).map(|r| r.map_err(Into::into))
        }
        let source:Arc<str> = Arc::from("% π header\n  (1+1):2:8; % tail\n");
        let statement = parse_session_core::<SessionExpressionErrorV1>(Arc::clone(&source),scanner,|_,_| Ok(())).unwrap();
        assert!(statement.suppressed);
        match &statement.parsed.root().kind {
            CoreNodeKind::Range {start,step:Some(step),end,colon_span,second_colon_span:Some(second)} => {
                for (id,text) in [(*start,"(1+1)"),(*step,"2"),(*end,"8")] {
                    let span = &statement.parsed.node(id).unwrap().span;
                    assert_eq!(&source[span.start..span.end],text);
                    assert_eq!(span.start_line,2);
                }
                let offsets = source.match_indices(':').map(|(i,_)| i).collect::<Vec<_>>();
                assert_eq!((colon_span.start,colon_span.end),(offsets[0],offsets[0]+1));
                assert_eq!((second.start,second.end),(offsets[1],offsets[1]+1));
                assert_eq!((colon_span.start_line,second.start_line),(2,2));
            }
            other => panic!("{other:?}"),
        }
    }
    #[test]
    fn session_matrix_gap_retains_original_nodes_spans_and_charges() {
        use crate::session_expression::SessionExpressionErrorV1;
        fn scanner(source:&Arc<str>,start:usize,end:usize,line:usize)
            -> Option<Result<usize,SessionExpressionErrorV1>> {
            scan_exact_integer(source,start,end,line).map(|r|r.map_err(Into::into))
        }
        for matrix in ["[1  -2;3\t+4]","[1,-2;3,+4]","[1,- 2;3,+ 4]"] {
            let source:Arc<str>=Arc::from(format!("% π header\n  {matrix} % tail"));
            let frame=frame(&source).unwrap();
            let tokens=lex_with_number_scanner(&source,frame,scanner,true,true).unwrap();
            let mut parser=Parser {source:&source,frame,tokens:&tokens,position:0,charged_nodes:0,numbers:0,depth:0,arithmetic:true,session:true, bracket_entry: false,nodes:Vec::with_capacity(7)};
            let root=parser.matrix().unwrap();
            assert_eq!((parser.charged_nodes,parser.numbers,parser.depth,parser.nodes.len()),(7,4,0,7));
            let node=&parser.nodes[root.index()];assert_eq!(&source[node.span.start..node.span.end],matrix);
            assert_eq!(node.span.start_line,2);
            let mut numbers=Vec::new();let mut signs=Vec::new();
            for node in &parser.nodes {
                assert_eq!(node.span.start_line,2);
                match &node.kind {
                    CoreNodeKind::Number {literal_span}=>numbers.push(&source[literal_span.start..literal_span.end]),
                    CoreNodeKind::Signed {sign_span,number,..}=>{
                        signs.push(&source[sign_span.start..sign_span.end]);
                        assert_eq!((node.span.start,node.span.end),(sign_span.start,parser.nodes[number.index()].span.end));
                        assert_eq!(&source[parser.nodes[number.index()].span.start..parser.nodes[number.index()].span.end],if signs.len()==1 {"2"} else {"4"});
                    },
                    CoreNodeKind::Matrix {rows}=>assert_eq!(rows.iter().map(Vec::len).collect::<Vec<_>>(),[2,2]),
                    other=>panic!("unexpected synthetic node: {other:?}"),
                }
            }
            assert_eq!(numbers,["1","2","3","4"]);assert_eq!(signs,["-","+"]);
        }
    }

    #[test]
    fn session_full_axis_argument_spans_nodes_and_charges() {
        use crate::session_expression::SessionExpressionErrorV1;
        fn scanner(source:&Arc<str>,start:usize,end:usize,line:usize)
            -> Option<Result<usize,SessionExpressionErrorV1>> {
            scan_exact_integer(source,start,end,line).map(|r|r.map_err(Into::into))
        }
        for (expression,charged,numbers,nodes,markers) in [
            ("A(end)",4,0,2,0),("A((end))",5,0,3,0),("A(:)",3,0,1,1),("A(:,:)",4,0,1,2),("A(1,:)",5,1,2,1),
            ("A(:,1)",5,1,2,1),("A(1,2)",6,2,3,0),("A(B(:),:)",7,0,2,2),
        ] {
            let source:Arc<str>=Arc::from(format!("% π header\n  {expression} % tail"));
            let frame=frame(&source).unwrap();let tokens=lex_with_number_scanner(&source,frame,scanner,true,true).unwrap();
            let mut parser=Parser {source:&source,frame,tokens:&tokens,position:0,charged_nodes:0,numbers:0,depth:0,arithmetic:true,session:true, bracket_entry: false,nodes:Vec::with_capacity(nodes)};
            let root=parser.expression().unwrap();
            assert_eq!((parser.charged_nodes,parser.numbers,parser.depth,parser.nodes.len()),(charged,numbers,0,nodes),"{expression}");
            assert_eq!(&source[parser.nodes[root.index()].span.start..parser.nodes[root.index()].span.end],expression);
            let mut seen=0;
            for node in &parser.nodes {
                if let CoreNodeKind::Call {arguments,..}=&node.kind {
                    for argument in arguments {
                        if let CallArgument::FullAxis(span)=argument {
                            assert_eq!(&source[span.start..span.end],":");assert_eq!(span.start_line,2);seen+=1;
                        }
                    }
                }
            }
            assert_eq!(seen,markers);
        }
        // A marker consumes the existing argument charge, without a Number charge.
        let source:Arc<str>=Arc::from("A(:)");let frame=frame(&source).unwrap();let tokens=lex_with_number_scanner(&source,frame,scanner,true,true).unwrap();
        let mut parser=Parser {source:&source,frame,tokens:&tokens,position:0,charged_nodes:1021,numbers:256,depth:0,arithmetic:true,session:true, bracket_entry: false,nodes:Vec::with_capacity(1)};
        parser.expression().unwrap();assert_eq!((parser.charged_nodes,parser.numbers),(1024,256));
    }

    #[test]
    fn session_construction_numeric_fastpath_nodes_charges_and_rows() {
        use crate::session_expression::SessionExpressionErrorV1;
        fn scanner(source:&Arc<str>,start:usize,end:usize,line:usize)
            -> Option<Result<usize,SessionExpressionErrorV1>> {
            scan_exact_integer(source,start,end,line).map(|r|r.map_err(Into::into))
        }
        for expression in ["[]","[1 -2;3 +4]","[1,- 2;3,+ 4]","[-0,+0]","[1 2,3;4,5 6]"] {
            let source:Arc<str>=Arc::from(format!("% π original\n  {expression} % tail"));
            let frame=frame(&source).unwrap();let tokens=lex_with_number_scanner(&source,frame,scanner,true,true).unwrap();
            let parse=|new| {
                let mut parser=Parser {source:&source,frame,tokens:&tokens,position:0,charged_nodes:0,numbers:0,depth:0,arithmetic:true,session:true,bracket_entry:false,nodes:Vec::with_capacity(32)};
                let root=if new {parser.matrix()} else {parser.numeric_matrix()}.unwrap();
                assert!(matches!(parser.nodes[root.index()].kind,CoreNodeKind::Matrix {..}));
                (format!("{:?}",parser.nodes),root.index(),parser.charged_nodes,parser.numbers,parser.position,parser.depth)
            };
            assert_eq!(parse(true),parse(false),"{expression}");
        }
        let source:Arc<str>=Arc::from("[A B;C]");let frame=frame(&source).unwrap();let tokens=lex_with_number_scanner(&source,frame,scanner,true,true).unwrap();
        let mut parser=Parser {source:&source,frame,tokens:&tokens,position:0,charged_nodes:0,numbers:0,depth:0,arithmetic:true,session:true,bracket_entry:false,nodes:Vec::with_capacity(4)};
        let root=parser.matrix().unwrap();assert_eq!((parser.nodes.len(),parser.charged_nodes,parser.numbers,parser.depth),(4,4,0,0));
        let parsed=CoreParsed {source:Arc::clone(&source),nodes:parser.nodes,root};
        let NodeKindView::Construction {rows}=parsed.view(root).kind else {panic!("expected private construction view");};assert_eq!(rows.iter().map(Vec::len).collect::<Vec<_>>(),[2,1]);
    }

    #[test]
    fn session_indexed_write_root_postorder_spans_charges_and_projection_isolation() {
        use crate::session_expression::SessionExpressionErrorV1;
        fn scanner(source:&Arc<str>,start:usize,end:usize,line:usize)
            -> Option<Result<usize,SessionExpressionErrorV1>> {
            scan_exact_integer(source,start,end,line).map(|r|r.map_err(Into::into))
        }
        let source:Arc<str>=Arc::from("% π header\n  A(end,:)=([1;2]); % tail");
        let statement=parse_session_core::<SessionExpressionErrorV1>(Arc::clone(&source),scanner,|_,_|Ok(())).unwrap();
        assert!(statement.suppressed);assert!(matches!(statement.effect,CoreStatementEffect::Assign(_)));
        let parsed=statement.parsed;assert_eq!(parsed.nodes.len(),6);assert_eq!(parsed.root.index(),5);
        let CoreNodeKind::IndexedWrite {target_span,selection_span,arguments,rhs}=&parsed.root().kind else {panic!("private write root required");};
        assert_eq!(&source[target_span.start..target_span.end],"A");assert_eq!(&source[selection_span.start..selection_span.end],"A(end,:)");assert_eq!(&source[parsed.root().span.start..parsed.root().span.end],"A(end,:)=([1;2])");assert_eq!(parsed.root().span.start_line,2);
        assert_eq!(arguments[0].expression().unwrap().index(),0);assert!(matches!(arguments[1],CallArgument::FullAxis(_)));assert_eq!(rhs.index(),4);
        assert_eq!(&source[parsed.nodes[rhs.index()].span.start..parsed.nodes[rhs.index()].span.end],"([1;2])");
        assert!(matches!(parsed.view(parsed.root).kind,NodeKindView::IndexedWrite {..}));
        for (signs,valid) in [(1019,true),(1020,false)] {
            let source:Arc<str>=Arc::from(format!("A(:)={}B","-".repeat(signs)));
            let result=parse_session_core::<SessionExpressionErrorV1>(source,scanner,|_,_|Ok(()));assert_eq!(result.is_ok(),valid);
        }
        for count in [256,257] {
            let source:Arc<str>=Arc::from(format!("A(:)={}",vec!["1";count].join("+")));
            assert_eq!(parse_session_core::<SessionExpressionErrorV1>(source,scanner,|_,_|Ok(())).is_ok(),count==256);
        }
    }

}
