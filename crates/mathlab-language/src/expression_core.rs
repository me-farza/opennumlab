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
#[derive(Debug)]
pub(crate) enum CoreNodeKind {
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
                CoreNodeKind::Identifier { .. } => {
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
                CoreNodeKind::Matrix { rows } => NodeKindView::Matrix { rows },
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
                    CoreTokenKind::Identifier | CoreTokenKind::Assign => {
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
        let mut left = self.product()?;
        while self.peek().is_some_and(|token| {
            token.kind == CoreTokenKind::Plus
                || (self.arithmetic && token.kind == CoreTokenKind::Minus)
        }) {
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
                Ok(self.store(CoreNode {
                    span: at.clone(),
                    kind: CoreNodeKind::Identifier {
                        identifier_span: at,
                    },
                }))
            }
            Some(CoreTokenKind::Number | CoreTokenKind::Plus | CoreTokenKind::Minus) => {
                self.signed()
            }
            Some(CoreTokenKind::OpenParen) => {
                self.charge(true, false)?;
                let start = self.at();
                self.position += 1;
                let inner = self.expression()?;
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
                while self
                    .peek()
                    .is_some_and(|token| token.kind == CoreTokenKind::Comma)
                {
                    self.position += 1;
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
        session: false,
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
    for token in &tokens {
        if token.kind == CoreTokenKind::Identifier {
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
        session: true,
        nodes,
    };
    let target = match (expression_tokens.first(), expression_tokens.get(1)) {
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
    let root = parser.expression()?;
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

    // Matrices cannot contain matrices under this grammar; their postorder
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
            session: false,
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
}
