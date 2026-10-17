// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit opt-in decimal-expression profile, sharing the v1 expression core.
//!
//! Number = (Digits ["." [Digits]] | "." Digits) [Exponent], where
//! Exponent = ("e" | "E") ["+" | "-"] Digits and Digits is nonempty ASCII.
//! Unary signs remain separate AST nodes; no general unary or statement syntax
//! is added. Numeric conversion uses Rust's correctly rounded f64 FromStr.
//! Overflow/underflow follow that converter (infinity/zero); spelled inf/NaN,
//! hexadecimal and MATLAB D-exponents are outside this profile.

#![forbid(unsafe_code)]

use crate::SourceSpan;
use crate::expression::{
    ExpectedSyntaxV1, LiteralDiagnosticCodeV1, LiteralDiagnosticDetailV1, LiteralDiagnosticV1,
    LiteralExpressionErrorV1, LiteralNodeIdV1, LiteralSignV1, LiteralStageV1, LiteralTokenKindV1,
};
use crate::expression_core::{
    BinaryOperator, CoreIr, CoreNode, CoreNodeKind, CoreParseError, CoreParsed, admit_core,
    dot_operator_at, parse_core,
};
use crate::literal_evaluation::{evaluate_core, no_mtimes};
use mathlab_array::DenseDouble2d;
use std::fmt;
use std::sync::Arc;

/// Grammar-neutral projection; a Number span covers its entire decimal token.
#[derive(Debug, Clone, Copy)]
pub struct DecimalNodeV1<'a> {
    pub span: &'a SourceSpan,
    pub kind: DecimalNodeKindV1<'a>,
}

#[derive(Debug, Clone, Copy)]
pub enum DecimalNodeKindV1<'a> {
    Number {
        literal_span: &'a SourceSpan,
    },
    Signed {
        sign: LiteralSignV1,
        sign_span: &'a SourceSpan,
        number: LiteralNodeIdV1,
    },
    Group {
        inner: LiteralNodeIdV1,
    },
    Matrix {
        rows: &'a [Vec<LiteralNodeIdV1>],
    },
    Add {
        left: LiteralNodeIdV1,
        operator_span: &'a SourceSpan,
        right: LiteralNodeIdV1,
    },
}

/// Immutable borrowed AST view. IDs refer only to the owning source snapshot.
#[derive(Clone, Copy)]
pub struct DecimalAstV1<'a> {
    core: &'a CoreParsed,
}

impl<'a> DecimalAstV1<'a> {
    pub fn source(self) -> &'a Arc<str> {
        self.core.source()
    }
    pub fn root_id(self) -> LiteralNodeIdV1 {
        self.core.root_id()
    }
    pub fn root(self) -> DecimalNodeV1<'a> {
        self.node(self.root_id()).expect("valid root")
    }
    pub fn node(self, id: LiteralNodeIdV1) -> Option<DecimalNodeV1<'a>> {
        self.core.node(id).map(decimal_node)
    }
    pub fn nodes(self) -> impl ExactSizeIterator<Item = DecimalNodeV1<'a>> + 'a {
        self.core.nodes().iter().map(decimal_node)
    }
}

fn decimal_node(node: &CoreNode) -> DecimalNodeV1<'_> {
    DecimalNodeV1 {
        span: &node.span,
        kind: match &node.kind {
            CoreNodeKind::Identifier { .. } => {
                unreachable!("decimal-v1 rejects identifiers during lexing")
            }
            CoreNodeKind::Number { literal_span } => DecimalNodeKindV1::Number { literal_span },
            CoreNodeKind::Signed {
                sign,
                sign_span,
                number,
            } => DecimalNodeKindV1::Signed {
                sign: *sign,
                sign_span,
                number: *number,
            },
            CoreNodeKind::Group { inner } => DecimalNodeKindV1::Group { inner: *inner },
            CoreNodeKind::Matrix { rows } => DecimalNodeKindV1::Matrix { rows },
            CoreNodeKind::Binary {
                operator: BinaryOperator::Add,
                left,
                operator_span,
                right,
            } => DecimalNodeKindV1::Add {
                left: *left,
                operator_span,
                right: *right,
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
                unreachable!("decimal-v1 excludes general arithmetic")
            }
        },
    }
}

impl fmt::Debug for DecimalAstV1<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.nodes()).finish()
    }
}

// The historical core carriers are deliberately private: no decimal source can
// be retrieved as a public digit-only v1 Parsed/IR/Value through this API.
pub struct ParsedDecimalExpressionV1 {
    core: CoreParsed,
}
impl ParsedDecimalExpressionV1 {
    pub fn source(&self) -> &Arc<str> {
        self.core.source()
    }
    pub fn ast(&self) -> DecimalAstV1<'_> {
        DecimalAstV1 { core: &self.core }
    }
}

pub struct DecimalExpressionIrV1 {
    core: CoreIr,
}
impl DecimalExpressionIrV1 {
    pub fn source(&self) -> &Arc<str> {
        self.core.source()
    }
    pub fn ast(&self) -> DecimalAstV1<'_> {
        DecimalAstV1 {
            core: self.core.parsed(),
        }
    }
}

impl fmt::Debug for ParsedDecimalExpressionV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedDecimalExpressionV1")
            .field("source", self.source())
            .field("ast", &self.ast())
            .finish()
    }
}
impl fmt::Debug for DecimalExpressionIrV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecimalExpressionIrV1")
            .field("source", self.source())
            .field("ast", &self.ast())
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct DecimalValueV1 {
    parsed: ParsedDecimalExpressionV1,
    array: DenseDouble2d,
}
impl DecimalValueV1 {
    pub fn source(&self) -> &Arc<str> {
        self.parsed.source()
    }
    pub fn ast(&self) -> DecimalAstV1<'_> {
        self.parsed.ast()
    }
    pub fn root_span(&self) -> &SourceSpan {
        &self.parsed.core.root().span
    }
    pub const fn class(&self) -> &'static str {
        "double"
    }
    pub fn array(&self) -> &DenseDouble2d {
        &self.array
    }
    pub fn into_parts(self) -> (ParsedDecimalExpressionV1, DenseDouble2d) {
        (self.parsed, self.array)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecimalDiagnosticCodeV1 {
    Profile(LiteralDiagnosticCodeV1),
    MalformedNumber,
    ConversionRejected,
}
impl DecimalDiagnosticCodeV1 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Profile(code) => code.as_str(),
            Self::MalformedNumber => "MalformedNumber",
            Self::ConversionRejected => "ConversionRejected",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecimalTokenKindV1 {
    Number,
    Plus,
    Minus,
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    Comma,
    Semicolon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MalformedDecimalV1 {
    MissingSignificandDigits,
    MissingExponentDigits,
    RepeatedComponent,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecimalDiagnosticDetailV1 {
    /// Shared non-syntax resource/framing/identifier/matrix/leaf metadata.
    Profile(LiteralDiagnosticDetailV1),
    Syntax {
        expected: ExpectedSyntaxV1,
        found: Option<DecimalTokenKindV1>,
    },
    Malformed(MalformedDecimalV1),
    ConversionRejected(std::num::ParseFloatError),
}

#[derive(Debug, PartialEq, Eq)]
pub struct DecimalDiagnosticV1 {
    pub source: Arc<str>,
    pub code: DecimalDiagnosticCodeV1,
    pub stage: LiteralStageV1,
    pub span: SourceSpan,
    pub detail: DecimalDiagnosticDetailV1,
}

#[derive(Debug)]
pub enum DecimalExpressionErrorV1 {
    Diagnostic(DecimalDiagnosticV1),
    Allocation {
        source: Arc<str>,
        stage: LiteralStageV1,
        span: SourceSpan,
        requested_items: usize,
        cause: std::collections::TryReserveError,
    },
}

impl From<LiteralDiagnosticV1> for DecimalExpressionErrorV1 {
    fn from(error: LiteralDiagnosticV1) -> Self {
        let detail = match error.detail {
            LiteralDiagnosticDetailV1::Syntax { expected, found } => {
                DecimalDiagnosticDetailV1::Syntax {
                    expected,
                    found: found.map(|kind| match kind {
                        LiteralTokenKindV1::Digits => DecimalTokenKindV1::Number,
                        LiteralTokenKindV1::Plus => DecimalTokenKindV1::Plus,
                        LiteralTokenKindV1::Minus => DecimalTokenKindV1::Minus,
                        LiteralTokenKindV1::OpenParen => DecimalTokenKindV1::OpenParen,
                        LiteralTokenKindV1::CloseParen => DecimalTokenKindV1::CloseParen,
                        LiteralTokenKindV1::OpenBracket => DecimalTokenKindV1::OpenBracket,
                        LiteralTokenKindV1::CloseBracket => DecimalTokenKindV1::CloseBracket,
                        LiteralTokenKindV1::Comma => DecimalTokenKindV1::Comma,
                        LiteralTokenKindV1::Semicolon => DecimalTokenKindV1::Semicolon,
                    }),
                }
            }
            shared => DecimalDiagnosticDetailV1::Profile(shared),
        };
        Self::Diagnostic(DecimalDiagnosticV1 {
            source: error.source,
            code: DecimalDiagnosticCodeV1::Profile(error.code),
            stage: error.stage,
            span: error.span,
            detail,
        })
    }
}

impl From<LiteralExpressionErrorV1> for DecimalExpressionErrorV1 {
    fn from(error: LiteralExpressionErrorV1) -> Self {
        match error {
            LiteralExpressionErrorV1::Diagnostic(error) => error.into(),
            LiteralExpressionErrorV1::Allocation {
                source,
                stage,
                span,
                requested_items,
                cause,
            } => Self::Allocation {
                source,
                stage,
                span,
                requested_items,
                cause,
            },
        }
    }
}

impl fmt::Display for DecimalExpressionErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Diagnostic(error) => write!(
                f,
                "{} at bytes {}..{} (line {})",
                error.code.as_str(),
                error.span.start,
                error.span.end,
                error.span.start_line
            ),
            Self::Allocation {
                requested_items,
                cause,
                ..
            } => write!(
                f,
                "decimal-expression host allocation of {requested_items} items failed: {cause}"
            ),
        }
    }
}
impl std::error::Error for DecimalExpressionErrorV1 {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Allocation { cause, .. } => Some(cause),
            Self::Diagnostic(DecimalDiagnosticV1 {
                detail: DecimalDiagnosticDetailV1::ConversionRejected(error),
                ..
            }) => Some(error),
            _ => None,
        }
    }
}

fn malformed(
    source: &Arc<str>,
    start: usize,
    end: usize,
    line: usize,
    reason: MalformedDecimalV1,
) -> DecimalExpressionErrorV1 {
    DecimalExpressionErrorV1::Diagnostic(DecimalDiagnosticV1 {
        source: Arc::clone(source),
        code: DecimalDiagnosticCodeV1::MalformedNumber,
        stage: LiteralStageV1::Lex,
        span: SourceSpan {
            start,
            end,
            start_line: line,
            end_line: line,
        },
        detail: DecimalDiagnosticDetailV1::Malformed(reason),
    })
}

pub(crate) fn scan_decimal(
    source: &Arc<str>,
    start: usize,
    end: usize,
    line: usize,
) -> Option<Result<usize, DecimalExpressionErrorV1>> {
    scan_decimal_token(source, start, end, line, false)
}

pub(crate) fn scan_decimal_arithmetic(
    source: &Arc<str>,
    start: usize,
    end: usize,
    line: usize,
) -> Option<Result<usize, DecimalExpressionErrorV1>> {
    scan_decimal_token(source, start, end, line, true)
}

fn scan_decimal_token(
    source: &Arc<str>,
    start: usize,
    end: usize,
    line: usize,
    dot_operators: bool,
) -> Option<Result<usize, DecimalExpressionErrorV1>> {
    let bytes = source.as_bytes();
    if (dot_operators && dot_operator_at(bytes, start, end).is_some())
        || (!bytes[start].is_ascii_digit() && bytes[start] != b'.')
    {
        return None;
    }
    let mut cursor = start;
    while cursor < end && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }
    let integer_digits = cursor - start;
    if cursor < end
        && bytes[cursor] == b'.'
        && !(dot_operators && dot_operator_at(bytes, cursor, end).is_some())
    {
        cursor += 1;
        let fraction_start = cursor;
        while cursor < end && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if integer_digits == 0 && fraction_start == cursor {
            return Some(Err(malformed(
                source,
                start,
                cursor,
                line,
                MalformedDecimalV1::MissingSignificandDigits,
            )));
        }
    }
    if cursor < end && matches!(bytes[cursor], b'e' | b'E') {
        cursor += 1;
        if cursor < end && matches!(bytes[cursor], b'+' | b'-') {
            cursor += 1;
        }
        let exponent_start = cursor;
        while cursor < end && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if exponent_start == cursor {
            return Some(Err(malformed(
                source,
                start,
                cursor,
                line,
                MalformedDecimalV1::MissingExponentDigits,
            )));
        }
    }
    if cursor < end
        && matches!(bytes[cursor], b'.' | b'e' | b'E')
        && !(dot_operators && dot_operator_at(bytes, cursor, end).is_some())
    {
        // Consume the repeated numeric component, without swallowing binary +
        // or a sign starting a later primary. All indices stay ASCII boundaries.
        while cursor < end
            && (bytes[cursor].is_ascii_digit() || matches!(bytes[cursor], b'.' | b'e' | b'E'))
        {
            cursor += 1;
        }
        return Some(Err(malformed(
            source,
            start,
            cursor,
            line,
            MalformedDecimalV1::RepeatedComponent,
        )));
    }
    Some(Ok(cursor))
}

pub(crate) fn decimal_bits(
    source: &Arc<str>,
    literal_span: &SourceSpan,
) -> Result<u64, DecimalExpressionErrorV1> {
    source[literal_span.start..literal_span.end]
        .parse::<f64>()
        .map(f64::to_bits)
        .map_err(|error| {
            DecimalExpressionErrorV1::Diagnostic(DecimalDiagnosticV1 {
                source: Arc::clone(source),
                code: DecimalDiagnosticCodeV1::ConversionRejected,
                stage: LiteralStageV1::LiteralAdmission,
                span: literal_span.clone(),
                detail: DecimalDiagnosticDetailV1::ConversionRejected(error),
            })
        })
}

pub fn parse_decimal_expression_v1(
    source: Arc<str>,
) -> Result<ParsedDecimalExpressionV1, DecimalExpressionErrorV1> {
    Ok(ParsedDecimalExpressionV1 {
        core: parse_core(source, scan_decimal, false)?,
    })
}

pub fn admit_decimal_expression_v1(
    parsed: ParsedDecimalExpressionV1,
) -> Result<DecimalExpressionIrV1, DecimalExpressionErrorV1> {
    Ok(DecimalExpressionIrV1 {
        core: admit_core(parsed.core, decimal_bits)?,
    })
}

pub fn lower_decimal_expression_v1(
    source: Arc<str>,
) -> Result<DecimalExpressionIrV1, DecimalExpressionErrorV1> {
    admit_decimal_expression_v1(parse_decimal_expression_v1(source)?)
}

pub fn evaluate_decimal_expression_v1(
    ir: DecimalExpressionIrV1,
) -> Result<DecimalValueV1, DecimalExpressionErrorV1> {
    let (parsed, array) = evaluate_core(ir.core, None, no_mtimes::<DecimalExpressionErrorV1>)?;
    Ok(DecimalValueV1 {
        parsed: ParsedDecimalExpressionV1 { core: parsed },
        array,
    })
}

pub fn evaluate_decimal_source_v1(
    source: Arc<str>,
) -> Result<DecimalValueV1, DecimalExpressionErrorV1> {
    evaluate_decimal_expression_v1(lower_decimal_expression_v1(source)?)
}

impl From<CoreParseError> for DecimalExpressionErrorV1 {
    fn from(error: CoreParseError) -> Self {
        let restricted: LiteralExpressionErrorV1 = error.into();
        restricted.into()
    }
}
