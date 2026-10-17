// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in single-statement arithmetic profile for an original Rust session.
//! No workspace, backend dispatch or user function lookup lives in this module.
#![forbid(unsafe_code)]

use crate::SourceSpan;
use crate::arithmetic_expression::ArithmeticExpressionErrorV1;
use crate::decimal_expression::{DecimalExpressionErrorV1, decimal_bits, scan_decimal_arithmetic};
use crate::expression::{
    ExpectedSyntaxV1, LiteralDiagnosticCodeV1, LiteralDiagnosticV1, LiteralExpressionErrorV1,
    LiteralStageV1, MAX_ARRAY_BYTES_V1, MAX_ARRAY_DIMENSION_V1, MAX_ARRAY_ELEMENTS_V1,
};
use crate::expression_core::{
    CoreIr, CoreParseError, CoreStatement, CoreStatementEffect, CoreTokenKind, admit_core,
    parse_session_core,
};
use crate::literal_evaluation::evaluate_arena_controlled;
use mathlab_array::{ArrayErrorV1, DenseDouble2d, LimitsV1, MtimesErrorV1, MtimesWorkBudgetV1};
use std::fmt;
use std::sync::Arc;

pub const MAX_IDENTIFIER_BYTES_V1: usize = 2048;
const OUTPUT_LIMITS: LimitsV1 = LimitsV1 {
    max_dimension: MAX_ARRAY_DIMENSION_V1 as u64,
    max_elements: MAX_ARRAY_ELEMENTS_V1 as u64,
    max_bytes: MAX_ARRAY_BYTES_V1 as u64,
};
const KEYWORDS: [&str; 20] = [
    "break",
    "case",
    "catch",
    "classdef",
    "continue",
    "else",
    "elseif",
    "end",
    "for",
    "function",
    "global",
    "if",
    "otherwise",
    "parfor",
    "persistent",
    "return",
    "spmd",
    "switch",
    "try",
    "while",
];

/// Independent owned value. Mutable storage never crosses the public boundary.
#[derive(Debug)]
pub struct SessionValueV1 {
    array: DenseDouble2d,
}
#[derive(Debug)]
pub struct SessionValueErrorV1 {
    pub cause: ArrayErrorV1,
}
impl SessionValueV1 {
    pub fn try_from_col_major(shape: [u64; 2], data: &[f64]) -> Result<Self, SessionValueErrorV1> {
        DenseDouble2d::try_from_col_major(shape, data, OUTPUT_LIMITS)
            .map(|array| Self { array })
            .map_err(|cause| SessionValueErrorV1 { cause })
    }
    pub fn try_copy(&self) -> Result<Self, SessionValueErrorV1> {
        Self::try_from_col_major(self.shape(), self.as_slice())
    }
    pub fn shape(&self) -> [u64; 2] {
        self.array.shape()
    }
    pub fn as_slice(&self) -> &[f64] {
        self.array.as_slice()
    }
    pub const fn class(&self) -> &'static str {
        "double"
    }
    pub fn payload_bytes(&self) -> usize {
        self.as_slice().len() * 8
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatementEffectV1 {
    Assignment { target: SourceSpan },
    Display { name: SourceSpan },
    Answer,
}
impl From<CoreStatementEffect> for SessionStatementEffectV1 {
    fn from(effect: CoreStatementEffect) -> Self {
        match effect {
            CoreStatementEffect::Assign(target) => Self::Assignment { target },
            CoreStatementEffect::Display(name) => Self::Display { name },
            CoreStatementEffect::Answer => Self::Answer,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTokenV1 {
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
impl From<CoreTokenKind> for SessionTokenV1 {
    fn from(kind: CoreTokenKind) -> Self {
        match kind {
            CoreTokenKind::Number => Self::Number,
            CoreTokenKind::Identifier => Self::Identifier,
            CoreTokenKind::Assign => Self::Assign,
            CoreTokenKind::Plus => Self::Plus,
            CoreTokenKind::Minus => Self::Minus,
            CoreTokenKind::Star => Self::Star,
            CoreTokenKind::OpenParen => Self::OpenParen,
            CoreTokenKind::CloseParen => Self::CloseParen,
            CoreTokenKind::OpenBracket => Self::OpenBracket,
            CoreTokenKind::CloseBracket => Self::CloseBracket,
            CoreTokenKind::Comma => Self::Comma,
            CoreTokenKind::Semicolon => Self::Semicolon,
            CoreTokenKind::DotStar => Self::DotStar,
            CoreTokenKind::DotSlash => Self::DotSlash,
            CoreTokenKind::DotTranspose => Self::DotTranspose,
        }
    }
}
#[derive(Debug)]
pub enum SessionExpressionErrorV1 {
    Arithmetic(ArithmeticExpressionErrorV1),
    Syntax {
        source: Arc<str>,
        code: LiteralDiagnosticCodeV1,
        span: SourceSpan,
        expected: ExpectedSyntaxV1,
        found: Option<SessionTokenV1>,
    },
    InvalidIdentifier {
        source: Arc<str>,
        span: SourceSpan,
    },
    UnknownBinding {
        source: Arc<str>,
        span: SourceSpan,
    },
    ValueCopy {
        source: Arc<str>,
        span: SourceSpan,
        cause: SessionValueErrorV1,
    },
    Cancelled {
        source: Arc<str>,
        span: SourceSpan,
    },
}
impl From<LiteralExpressionErrorV1> for SessionExpressionErrorV1 {
    fn from(error: LiteralExpressionErrorV1) -> Self {
        Self::Arithmetic(error.into())
    }
}
impl From<LiteralDiagnosticV1> for SessionExpressionErrorV1 {
    fn from(error: LiteralDiagnosticV1) -> Self {
        Self::Arithmetic(error.into())
    }
}
impl From<DecimalExpressionErrorV1> for SessionExpressionErrorV1 {
    fn from(error: DecimalExpressionErrorV1) -> Self {
        Self::Arithmetic(error.into())
    }
}
impl From<CoreParseError> for SessionExpressionErrorV1 {
    fn from(error: CoreParseError) -> Self {
        match error {
            CoreParseError::Shared(error) => error.into(),
            CoreParseError::Syntax {
                source,
                code,
                span,
                expected,
                found,
            } => Self::Syntax {
                source,
                code,
                span,
                expected,
                found: found.map(Into::into),
            },
        }
    }
}
impl SessionExpressionErrorV1 {
    pub fn original_source(&self) -> &Arc<str> {
        match self {
            Self::Arithmetic(e) => e.original_source(),
            Self::Syntax { source, .. }
            | Self::InvalidIdentifier { source, .. }
            | Self::UnknownBinding { source, .. }
            | Self::ValueCopy { source, .. }
            | Self::Cancelled { source, .. } => source,
        }
    }
    pub fn span(&self) -> &SourceSpan {
        match self {
            Self::Arithmetic(e) => e.span(),
            Self::Syntax { span, .. }
            | Self::InvalidIdentifier { span, .. }
            | Self::UnknownBinding { span, .. }
            | Self::ValueCopy { span, .. }
            | Self::Cancelled { span, .. } => span,
        }
    }
    pub fn stage(&self) -> LiteralStageV1 {
        match self {
            Self::Arithmetic(e) => e.stage(),
            Self::Syntax { .. } => LiteralStageV1::Parse,
            Self::InvalidIdentifier { .. } => LiteralStageV1::Lex,
            _ => LiteralStageV1::Evaluation,
        }
    }
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Self::Arithmetic(e) => e.code(),
            Self::Syntax { code, .. } => Some(code.as_str()),
            Self::InvalidIdentifier { .. } => Some("InvalidIdentifier"),
            Self::UnknownBinding { .. } => Some("UnknownBinding"),
            Self::ValueCopy { .. } => Some("ArrayOperation"),
            Self::Cancelled { .. } => Some("Cancelled"),
        }
    }
}
impl fmt::Display for SessionExpressionErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Self::Arithmetic(error) = self {
            return fmt::Display::fmt(error, f);
        }
        write!(
            f,
            "{} at bytes {}..{}",
            self.code().unwrap_or("Allocation"),
            self.span().start,
            self.span().end
        )
    }
}
impl std::error::Error for SessionExpressionErrorV1 {}

#[derive(Debug)]
pub struct ParsedSessionExpressionV1 {
    statement: CoreStatement,
}
#[derive(Debug)]
pub struct SessionExpressionIrV1 {
    core: CoreIr,
    effect: SessionStatementEffectV1,
    suppressed: bool,
}
#[derive(Debug)]
pub struct EvaluatedSessionExpressionV1 {
    pub source: Arc<str>,
    pub effect: SessionStatementEffectV1,
    pub suppressed: bool,
    pub value: SessionValueV1,
}
fn scanner(
    source: &Arc<str>,
    start: usize,
    end: usize,
    line: usize,
) -> Option<Result<usize, SessionExpressionErrorV1>> {
    scan_decimal_arithmetic(source, start, end, line).map(|r| r.map_err(Into::into))
}
fn validate_identifier(
    source: &Arc<str>,
    span: &SourceSpan,
) -> Result<(), SessionExpressionErrorV1> {
    let name = &source[span.start..span.end];
    if name.len() > MAX_IDENTIFIER_BYTES_V1
        || !name.as_bytes()[0].is_ascii_alphabetic()
        || KEYWORDS.contains(&name)
    {
        return Err(SessionExpressionErrorV1::InvalidIdentifier {
            source: Arc::clone(source),
            span: span.clone(),
        });
    }
    Ok(())
}
pub fn parse_session_expression_v1(
    source: Arc<str>,
) -> Result<ParsedSessionExpressionV1, SessionExpressionErrorV1> {
    Ok(ParsedSessionExpressionV1 {
        statement: parse_session_core(source, scanner, validate_identifier)?,
    })
}
pub fn admit_session_expression_v1(
    parsed: ParsedSessionExpressionV1,
) -> Result<SessionExpressionIrV1, SessionExpressionErrorV1> {
    let CoreStatement {
        parsed,
        effect,
        suppressed,
    } = parsed.statement;
    let core = admit_core::<SessionExpressionErrorV1>(parsed, |source, span| {
        decimal_bits(source, span).map_err(Into::into)
    })?;
    Ok(SessionExpressionIrV1 {
        core,
        effect: effect.into(),
        suppressed,
    })
}
/// The owner supplies only immutable binding lookup and request-control polling.
/// A name copy is fallible; no workspace binding is consumed or modified here.
pub fn evaluate_session_expression_v1<'w>(
    ir: SessionExpressionIrV1,
    mut lookup: impl FnMut(&str) -> Option<&'w SessionValueV1>,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<EvaluatedSessionExpressionV1, SessionExpressionErrorV1> {
    let SessionExpressionIrV1 {
        core: CoreIr { parsed, literals },
        effect,
        suppressed,
    } = ir;
    let mut resolve = |source: &Arc<str>, span: &SourceSpan| {
        let value = lookup(&source[span.start..span.end]).ok_or_else(|| {
            SessionExpressionErrorV1::UnknownBinding {
                source: Arc::clone(source),
                span: span.clone(),
            }
        })?;
        value
            .try_copy()
            .map(|v| v.array)
            .map_err(|cause| SessionExpressionErrorV1::ValueCopy {
                source: Arc::clone(source),
                span: span.clone(),
                cause,
            })
    };
    let mut poll = |source: &Arc<str>, span: &SourceSpan| {
        if is_cancelled() {
            Err(SessionExpressionErrorV1::Cancelled {
                source: Arc::clone(source),
                span: span.clone(),
            })
        } else {
            Ok(())
        }
    };
    let multiply_error = |source: &Arc<str>, span: &SourceSpan, error: MtimesErrorV1| {
        SessionExpressionErrorV1::Arithmetic(ArithmeticExpressionErrorV1::Multiplication {
            source: Arc::clone(source),
            span: span.clone(),
            error,
        })
    };
    let array = evaluate_arena_controlled(
        &parsed,
        literals,
        Some(MtimesWorkBudgetV1 { max_terms: 4096 }),
        multiply_error,
        &mut resolve,
        &mut poll,
    )?;
    Ok(EvaluatedSessionExpressionV1 {
        source: Arc::clone(&parsed.source),
        effect,
        suppressed,
        value: SessionValueV1 { array },
    })
}
