// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in single-statement arithmetic profile for an original Rust session.
//! No workspace, backend dispatch or user function lookup lives in this module.
#![forbid(unsafe_code)]

pub(crate) mod exact_reduction;

use crate::SourceSpan;
use crate::arithmetic_expression::ArithmeticExpressionErrorV1;
use crate::decimal_expression::{DecimalExpressionErrorV1, decimal_bits, scan_decimal_arithmetic};
use crate::expression::{
    ExpectedSyntaxV1, LiteralDiagnosticCodeV1, LiteralDiagnosticV1, LiteralExpressionErrorV1,
    LiteralStageV1, LiteralNodeIdV1, MAX_ARRAY_BYTES_V1, MAX_ARRAY_DIMENSION_V1, MAX_ARRAY_ELEMENTS_V1,
};
use crate::expression_core::{
    CallArgument, CoreIr, CoreParsed, CoreParseError, CoreStatement, CoreStatementEffect, CoreTokenKind, admit_core,
    parse_session_core,
};
use crate::literal_evaluation::{evaluate_arena_controlled, leaf_error};
use std::cell::RefCell;
use mathlab_array::{ArrayErrorV1, DenseDouble2d, ErrorCodeV1, ErrorContextV1, LimitsV1, MtimesErrorV1, MtimesWorkBudgetV1};
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
impl From<CoreTokenKind> for SessionTokenV1 {
    fn from(kind: CoreTokenKind) -> Self {
        match kind {
            CoreTokenKind::Number => Self::Number,
            CoreTokenKind::Identifier => Self::Identifier,
            CoreTokenKind::Assign => Self::Assign,
            CoreTokenKind::Colon => Self::Colon,
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
/// Project dimension diagnostics, not MATLAB error identifiers or precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstructorDimensionReasonV1 {
    NonScalar,
    NonFinite,
    Negative,
    Fractional,
    AboveProfileLimit,
}

/// Project scalar index diagnostics, not native error identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexOperandReasonV1 {
    NonScalar,
    NonFinite,
    Fractional,
    NonPositive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegerRangeOperandReasonV1 {
    NonScalar,
    NonFinite,
    Fractional,
    OutsideExactIntegerEnvelope,
}
const INTEGER_RANGE_LIMIT_V1: i128 = 1_i128 << 53;

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
    UnsupportedCall {
        source: Arc<str>,
        span: SourceSpan,
    },
    CallArity {
        source: Arc<str>,
        span: SourceSpan,
        actual: usize,
    },
    IndexArity {
        source: Arc<str>,
        span: SourceSpan,
        actual: usize,
    },
    IndexOperand {
        source: Arc<str>,
        span: SourceSpan,
        argument_index: usize,
        reason: IndexOperandReasonV1,
        shape: [u64; 2],
        value_bits: Option<u64>,
    },
    IndexBounds {
        source: Arc<str>,
        span: SourceSpan,
        argument_index: usize,
        upper_bound: u64,
        shape: [u64; 2],
        value_bits: u64,
    },
    IndexElementOperand {
        source: Arc<str>,
        span: SourceSpan,
        argument_index: usize,
        element_index: usize,
        reason: IndexOperandReasonV1,
        shape: [u64; 2],
        value_bits: u64,
    },
    IndexElementBounds {
        source: Arc<str>,
        span: SourceSpan,
        argument_index: usize,
        element_index: usize,
        upper_bound: u64,
        shape: [u64; 2],
        value_bits: u64,
    },
    ShadowedCallTarget {
        source: Arc<str>,
        span: SourceSpan,
    },
    ConstructorDimension {
        source: Arc<str>,
        span: SourceSpan,
        argument_index: usize,
        reason: ConstructorDimensionReasonV1,
        shape: [u64; 2],
        value_bits: Option<u64>,
    },
    RangeOperand {
        source: Arc<str>,
        span: SourceSpan,
        operand_index: usize,
        reason: IntegerRangeOperandReasonV1,
        shape: [u64;2],
        value_bits: Option<u64>,
    },
    RangeLength {
        source: Arc<str>,
        span: SourceSpan,
        actual: i128,
        limit: usize,
    },
    RangeArithmetic {
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
    UnsupportedEmptyAssignment {
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
            | Self::UnsupportedCall { source, .. }
            | Self::UnsupportedEmptyAssignment { source, .. }
            | Self::CallArity { source, .. }
            | Self::IndexArity { source, .. }
            | Self::IndexOperand { source, .. }
            | Self::IndexBounds { source, .. }
            | Self::IndexElementOperand { source, .. }
            | Self::IndexElementBounds { source, .. }
            | Self::ShadowedCallTarget { source, .. }
            | Self::ConstructorDimension { source, .. }
            | Self::RangeOperand { source, .. }
            | Self::RangeLength { source, .. }
            | Self::RangeArithmetic { source, .. }
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
            | Self::UnsupportedCall { span, .. }
            | Self::UnsupportedEmptyAssignment { span, .. }
            | Self::CallArity { span, .. }
            | Self::IndexArity { span, .. }
            | Self::IndexOperand { span, .. }
            | Self::IndexBounds { span, .. }
            | Self::IndexElementOperand { span, .. }
            | Self::IndexElementBounds { span, .. }
            | Self::ShadowedCallTarget { span, .. }
            | Self::ConstructorDimension { span, .. }
            | Self::RangeOperand { span, .. }
            | Self::RangeLength { span, .. }
            | Self::RangeArithmetic { span, .. }
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
            Self::UnsupportedCall { .. } => Some("UnsupportedCall"),
            Self::UnsupportedEmptyAssignment { .. } => Some("UnsupportedEmptyAssignment"),
            Self::CallArity { .. } => Some("CallArity"),
            Self::IndexArity { .. } => Some("IndexArity"),
            Self::IndexOperand { .. } | Self::IndexElementOperand { .. } => Some("IndexOperand"),
            Self::IndexBounds { .. } | Self::IndexElementBounds { .. } => Some("IndexBounds"),
            Self::ShadowedCallTarget { .. } => Some("ShadowedCallTarget"),
            Self::ConstructorDimension { .. } => Some("ConstructorDimension"),
            Self::RangeOperand { .. } => Some("RangeOperand"),
            Self::RangeLength { .. } => Some("RangeLength"),
            Self::RangeArithmetic { .. } => Some("RangeArithmetic"),
            Self::ValueCopy { .. } => Some("ArrayOperation"),
            Self::Cancelled { .. } => Some("Cancelled"),
        }
    }
}
impl fmt::Display for SessionExpressionErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Self::UnsupportedEmptyAssignment { span, .. } = self {
            return write!(f, "UnsupportedEmptyAssignment: 0-by-0 right-hand sides are unsupported for fixed-shape indexed assignment at bytes {}..{}.", span.start, span.end);
        }
        if let Self::Arithmetic(error) = self {
            return fmt::Display::fmt(error, f);
        }
        if let Self::IndexElementOperand {argument_index,element_index,reason,shape,value_bits,..} = self {
            return write!(f,"IndexOperand argument {argument_index}, element {element_index}: {reason:?}, shape {shape:?}, bits {value_bits:016x} at bytes {}..{}",self.span().start,self.span().end);
        }
        if let Self::IndexElementBounds {argument_index,element_index,upper_bound,shape,value_bits,..} = self {
            return write!(f,"IndexBounds argument {argument_index}, element {element_index}: upper bound {upper_bound}, shape {shape:?}, bits {value_bits:016x} at bytes {}..{}",self.span().start,self.span().end);
        }
        if let Self::RangeOperand {operand_index,reason,shape,value_bits,..} = self {
            return write!(f,"RangeOperand {operand_index}: {reason:?}, shape {shape:?}, bits {value_bits:016x?} at bytes {}..{}",
                self.span().start,self.span().end);
        }
        if let Self::RangeLength {actual,limit,..} = self {
            return write!(f,"RangeLength {actual} exceeds {limit} at bytes {}..{}",self.span().start,self.span().end);
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
struct Selection {
    indices: [[u64; MAX_ARRAY_DIMENSION_V1]; 2],
    counts: [usize; 2],
    full_axes: [bool; 2],
}

// Shared read/write validation: every child has already completed. Keep the
// original per-argument shape/domain/bounds phases and source ordinals.
fn select_indices(parsed: &CoreParsed, source: &Arc<str>, arguments: &[CallArgument],
    values: &[DenseDouble2d], target: &SessionValueV1)
    -> Result<Selection, SessionExpressionErrorV1> {
    let mut indices = [[0_u64; MAX_ARRAY_DIMENSION_V1]; 2];
    let mut counts = [0_usize; 2];
    let shape = target.shape();
    let mut full_axes = [false; 2];
    let mut expressions = values.iter();
    for (index, argument) in arguments.iter().enumerate() {
        let id = match argument {
            CallArgument::FullAxis(_) => {
                full_axes[index] = true;
                counts[index] = if arguments.len() == 1 { target.as_slice().len() } else { shape[index] as usize };
                continue;
            }
            CallArgument::Expression(id) => *id,
        };
        let value = expressions.next().expect("each expression produced one value");
        let span = parsed.node(id).expect("index belongs to parsed arena").span.clone();
        let selector_shape = value.shape();
        let scalar = selector_shape == [1,1];
        let words = value.as_slice();
        if !words.is_empty() && !(selector_shape[0] == 1 || selector_shape[1] == 1) {
            return Err(SessionExpressionErrorV1::IndexOperand {
                source: Arc::clone(source), span, argument_index: index,
                reason: IndexOperandReasonV1::NonScalar, shape: selector_shape, value_bits: None,
            });
        }
        // Complete children succeeded. Per argument: shape, every domain,
        // then every bound; a later invalid word precedes earlier OOB.
        for (element_index, word) in words.iter().enumerate() {
            let reason = if !word.is_finite() { Some(IndexOperandReasonV1::NonFinite) }
                else if word.fract() != 0.0 { Some(IndexOperandReasonV1::Fractional) }
                else if *word <= 0.0 { Some(IndexOperandReasonV1::NonPositive) }
                else { None };
            if let Some(reason) = reason {
                return Err(if scalar {
                    SessionExpressionErrorV1::IndexOperand {
                        source: Arc::clone(source), span, argument_index: index,
                        reason, shape: selector_shape, value_bits: Some(word.to_bits()),
                    }
                } else {
                    SessionExpressionErrorV1::IndexElementOperand {
                        source: Arc::clone(source), span, argument_index: index,
                        element_index, reason, shape: selector_shape, value_bits: word.to_bits(),
                    }
                });
            }
        }
        let upper_bound = if arguments.len() == 1 { target.as_slice().len() as u64 } else { shape[index] };
        for (element_index, word) in words.iter().enumerate() {
            // Bounds precede unsigned conversion, including linear256.
            if *word > upper_bound as f64 {
                return Err(if scalar {
                    SessionExpressionErrorV1::IndexBounds {
                        source: Arc::clone(source), span, argument_index: index,
                        upper_bound, shape: selector_shape, value_bits: word.to_bits(),
                    }
                } else {
                    SessionExpressionErrorV1::IndexElementBounds {
                        source: Arc::clone(source), span, argument_index: index,
                        element_index, upper_bound, shape: selector_shape, value_bits: word.to_bits(),
                    }
                });
            }
            indices[index][element_index] = *word as u64 - 1;
        }
        counts[index] = words.len();
    }
    Ok(Selection { indices, counts, full_axes })
}

/// The owner supplies only immutable binding lookup and request-control polling.
/// A name copy is fallible; no workspace binding is consumed or modified here.
pub fn evaluate_session_expression_v1<'w>(
    ir: SessionExpressionIrV1,
    lookup: impl FnMut(&str) -> Option<&'w SessionValueV1>,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<EvaluatedSessionExpressionV1, SessionExpressionErrorV1> {
    let SessionExpressionIrV1 {
        core: CoreIr { parsed, literals },
        effect,
        suppressed,
    } = ir;
    // The two sequential evaluator callbacks share one immutable binding lookup;
    // each short borrow ends before another task/callback can run.
    let lookup = RefCell::new(lookup);
    let mut resolve = |source: &Arc<str>, span: &SourceSpan, extent: Option<u64>| {
        if &source[span.start..span.end] == "end" {
            let extent = extent.ok_or_else(|| SessionExpressionErrorV1::UnsupportedCall {
                source: Arc::clone(source), span: span.clone(),
            })?;
            return DenseDouble2d::try_from_col_major([1,1], &[extent as f64], OUTPUT_LIMITS)
                .map_err(|error| SessionExpressionErrorV1::from(leaf_error(source, span, error)));
        }
        let value = (lookup.borrow_mut())(&source[span.start..span.end]).ok_or_else(|| {
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
    let mut call = |source: &Arc<str>, callee: &SourceSpan, call_span: &SourceSpan,
                    arguments: &[CallArgument], values: Option<&[DenseDouble2d]>,
                    bound_extents: &mut Option<[u64; 2]>| {
        let name = &source[callee.start..callee.end];
        // Immutable lookup selects indexing before builtin name/arity checks.
        // Completion borrows the original target; only selected words are copied.
        let target = (lookup.borrow_mut())(name);
        let Some(values) = values else {
            if let Some(target) = target {
                if !(1..=2).contains(&arguments.len()) {
                    return Err(SessionExpressionErrorV1::IndexArity {
                        source: Arc::clone(source), span: call_span.clone(), actual: arguments.len(),
                    });
                }
                *bound_extents = Some(if arguments.len() == 1 {
                    [target.as_slice().len() as u64; 2]
                } else {
                    target.shape()
                });
                return Ok(None);
            }
            if !matches!(name, "zeros" | "ones" | "eye" | "size" | "numel" | "reshape" | "sum" | "min" | "max" | "mean" | "prod") {
                return Err(SessionExpressionErrorV1::UnsupportedCall {
                    source: Arc::clone(source), span: callee.clone(),
                });
            }
            let valid_arity = if matches!(name, "size" | "numel" | "sum" | "min" | "max" | "mean" | "prod") {
                arguments.len() == 1
            } else if name == "reshape" {
                arguments.len() == 3
            } else {
                (1..=2).contains(&arguments.len())
            };
            if !valid_arity {
                return Err(SessionExpressionErrorV1::CallArity {
                    source: Arc::clone(source), span: call_span.clone(), actual: arguments.len(),
                });
            }
            return Ok(None);
        };
        if let Some(target) = target {
            let Selection { indices, counts, full_axes } =
                select_indices(&parsed, source, arguments, values, target)?;
            let shape = target.shape();
            let output_shape = if arguments.len() == 2 {
                [counts[0] as u64, counts[1] as u64]
            } else if full_axes[0] {
                [counts[0] as u64, 1]
            } else if (values[0].shape()[0] == 1 || values[0].shape()[1] == 1)
                && shape[0] == 1 && shape[1] != 1 {
                [1, counts[0] as u64]
            } else if (values[0].shape()[0] == 1 || values[0].shape()[1] == 1)
                && shape[1] == 1 && shape[0] != 1 {
                [counts[0] as u64, 1]
            } else {
                // Only empty 1x0/0x1 selectors inherit a vector target's orientation.
                values[0].shape()
            };
            let layout_error = |code| SessionExpressionErrorV1::from(leaf_error(
                source, call_span, ArrayErrorV1 {context: ErrorContextV1::Construct, code},
            ));
            // Check the ordinary output caps before gathering any target payload.
            // Admitted vectors have <=16 words, so public output is at most16x16.
            let elements = output_shape[0].checked_mul(output_shape[1]).ok_or_else(||
                layout_error(ErrorCodeV1::ShapeProductOverflow {rows: output_shape[0], columns: output_shape[1]}))?;
            let bytes = elements.checked_mul(8).ok_or_else(||
                layout_error(ErrorCodeV1::ByteSizeOverflow {elements, element_width_bytes: 8}))?;
            for (axis, actual) in output_shape.into_iter().enumerate() {
                if actual > OUTPUT_LIMITS.max_dimension {
                    return Err(layout_error(ErrorCodeV1::DimensionLimitExceeded {axis, actual, limit: OUTPUT_LIMITS.max_dimension}));
                }
            }
            if elements > OUTPUT_LIMITS.max_elements {
                return Err(layout_error(ErrorCodeV1::ElementLimitExceeded {actual: elements, limit: OUTPUT_LIMITS.max_elements}));
            }
            if bytes > OUTPUT_LIMITS.max_bytes {
                return Err(layout_error(ErrorCodeV1::ByteLimitExceeded {actual: bytes, limit: OUTPUT_LIMITS.max_bytes}));
            }
            let count = usize::try_from(elements).expect("bounded output count");
            let mut payload = [0.0; MAX_ARRAY_ELEMENTS_V1];
            let mut written = 0;
            for column in 0..(if arguments.len() == 1 {1} else {counts[1]}) {
                for row in 0..counts[0] {
                    let row_index = if full_axes[0] { row as u64 } else { indices[0][row] };
                    let offset = if arguments.len() == 1 { Some(row_index) } else {
                        let column_index = if full_axes[1] { column as u64 } else { indices[1][column] };
                        column_index.checked_mul(shape[0]).and_then(|base| base.checked_add(row_index))
                    };
                    let word = offset.and_then(|i| usize::try_from(i).ok())
                        .and_then(|i| target.as_slice().get(i)).expect("bounded column-major gather offset");
                    payload[written] = *word;
                    written += 1;
                }
            }
            debug_assert_eq!(written, count);
            return DenseDouble2d::try_from_col_major(output_shape, &payload[..count], OUTPUT_LIMITS)
                .map(Some)
                .map_err(|error| SessionExpressionErrorV1::from(leaf_error(source, call_span, error)));
        }
        if let Some(span) = arguments.iter().find_map(|argument| match argument {
            CallArgument::FullAxis(span) => Some(span), CallArgument::Expression(_) => None,
        }) {
            // Ordinary builtin contexts reject markers only after all real children.
            return Err(SessionExpressionErrorV1::UnsupportedCall { source: Arc::clone(source), span: span.clone() });
        }
        if matches!(name, "mean" | "prod") {
            debug_assert!(values[0].shape().iter().all(|&axis| axis <= 16) && values[0].as_slice().len() <= 256);
            return exact_reduction::reduce(&values[0], name == "prod").map(Some)
                .map_err(|error| SessionExpressionErrorV1::from(leaf_error(source, call_span, error)));
        }
        if matches!(name, "min" | "max") {
            let argument = &values[0]; // Preflight admits exactly one child.
            let shape = argument.shape();
            let output_shape = if shape[0] == 0 {
                [0, shape[1]]
            } else if shape[0] == 1 {
                [1, if shape[1] == 0 { 0 } else { 1 }]
            } else {
                [1, shape[1]]
            };
            let fibers = (output_shape[0] * output_shape[1]) as usize;
            let terms = if shape[0] == 1 { shape[1] } else { shape[0] } as usize;
            let minimum = name == "min";
            let mut words = [0.0; 16];
            // Nonempty default-axis fibers are contiguous. Seed/classify once,
            // then visit only the remainder: <=256 reads/classifications and
            // <=240 strict comparisons,16 scratch/output words, no stack growth.
            for (fiber, output) in words[..fibers].iter_mut().enumerate() {
                let start = fiber * terms;
                let mut candidate = argument.as_slice()[start];
                let mut seen_numeric = !candidate.is_nan();
                for &word in &argument.as_slice()[start + 1..start + terms] {
                    if word.is_nan() { continue; }
                    if !seen_numeric {
                        candidate = word;
                        seen_numeric = true;
                    } else if if minimum { word < candidate } else { word > candidate } {
                        candidate = word;
                    }
                }
                *output = candidate;
            }
            return DenseDouble2d::try_from_col_major(output_shape, &words[..fibers], OUTPUT_LIMITS)
                .map(Some)
                .map_err(|error| SessionExpressionErrorV1::from(leaf_error(source, call_span, error)));
        }
        if name == "sum" {
            return exact_reduction::sum(&values[0]).map(Some)
                .map_err(|error| SessionExpressionErrorV1::from(leaf_error(source, call_span, error)));
        }
        if matches!(name, "size" | "numel") {
            let argument = &values[0]; // Query preflight admits exactly one child.
            // Session construction bounds extents/count at 16/256, so these
            // integer-to-binary64 conversions are exact. Do not scan payloads
            // or apply constructor dimension validation to query arguments.
            let result = if name == "size" {
                let shape = argument.shape();
                DenseDouble2d::try_from_col_major(
                    [1,2], &[shape[0] as f64, shape[1] as f64], OUTPUT_LIMITS,
                )
            } else {
                DenseDouble2d::try_from_col_major(
                    [1,1], &[argument.as_slice().len() as f64], OUTPUT_LIMITS,
                )
            };
            return result.map(Some)
                .map_err(|error| SessionExpressionErrorV1::from(leaf_error(source, call_span, error)));
        }
        let mut dimensions = [0_u64; 2];
        let dimension_start = usize::from(name == "reshape");
        // All expressions have succeeded. Validate dimensions left to right,
        // before any float-to-integer conversion or result reservation.
        for (index, value) in values.iter().enumerate().skip(dimension_start) {
            let bits = if value.shape() == [1, 1] { Some(value.as_slice()[0].to_bits()) } else { None };
            let reason = match bits.map(f64::from_bits) {
                None => Some(ConstructorDimensionReasonV1::NonScalar),
                Some(v) if !v.is_finite() => Some(ConstructorDimensionReasonV1::NonFinite),
                Some(v) if v.fract() != 0.0 => Some(ConstructorDimensionReasonV1::Fractional),
                Some(v) if name == "reshape" && v < 0.0 => Some(ConstructorDimensionReasonV1::Negative),
                Some(v) => {
                    // Only finite integers reach normalization. Clamp negative
                    // extents before the positive cap and unsigned conversion.
                    let extent = if v < 0.0 { 0.0 } else { v };
                    if extent > MAX_ARRAY_DIMENSION_V1 as f64 {
                        Some(ConstructorDimensionReasonV1::AboveProfileLimit)
                    } else {
                        dimensions[index - dimension_start] = extent as u64;
                        None
                    }
                },
            };
            if let Some(reason) = reason {
                return Err(SessionExpressionErrorV1::ConstructorDimension {
                    source: Arc::clone(source),
                    span: parsed.node(arguments[index].expression().expect("ordinary call has no markers")).expect("argument belongs to parsed arena").span.clone(),
                    argument_index: index, reason, shape: value.shape(), value_bits: bits,
                });
            }
        }
        if values.len() - dimension_start == 1 { dimensions[1] = dimensions[0]; }
        match name {
            "zeros" => DenseDouble2d::try_zeros(dimensions, OUTPUT_LIMITS),
            "ones" => DenseDouble2d::try_ones(dimensions, OUTPUT_LIMITS),
            "eye" => DenseDouble2d::try_eye(dimensions, OUTPUT_LIMITS),
            // Checked import changes only geometry; copy payload words without arithmetic.
            "reshape" => DenseDouble2d::try_from_col_major(dimensions, values[0].as_slice(), OUTPUT_LIMITS),
            _ => unreachable!("dimensioned call completion follows closed preflight and query dispatch"),
        }
            .map(Some)
            .map_err(|error| SessionExpressionErrorV1::from(leaf_error(source, call_span, error)))
    };
    let mut range = |source:&Arc<str>, range_span:&SourceSpan,
                     arguments:&[LiteralNodeIdV1], values:&[DenseDouble2d]| {
        let mut integers = [0_i128;3];
        // Complete operand subtrees have succeeded. Validate every supplied
        // operand in source order before zero-step/direction/count decisions.
        for (index,value) in values.iter().enumerate() {
            let bits = if value.shape()==[1,1] {Some(value.as_slice()[0].to_bits())} else {None};
            let reason = match bits.map(f64::from_bits) {
                None => Some(IntegerRangeOperandReasonV1::NonScalar),
                Some(v) if !v.is_finite() => Some(IntegerRangeOperandReasonV1::NonFinite),
                Some(v) if v.fract()!=0.0 => Some(IntegerRangeOperandReasonV1::Fractional),
                Some(v) if v.abs()>INTEGER_RANGE_LIMIT_V1 as f64 => Some(IntegerRangeOperandReasonV1::OutsideExactIntegerEnvelope),
                Some(_) => None,
            };
            if let Some(reason) = reason {
                return Err(SessionExpressionErrorV1::RangeOperand {
                    source:Arc::clone(source),
                    span:parsed.node(arguments[index]).expect("range operand belongs to arena").span.clone(),
                    operand_index:index,reason,shape:value.shape(),value_bits:bits,
                });
            }
            // Finite integral binary64 within inclusive +/-2^53 converts exactly.
            integers[index] = f64::from_bits(bits.expect("validated scalar")) as i128;
        }
        let (start,step,end) = if values.len()==2 {(integers[0],1,integers[1])}
            else {(integers[0],integers[1],integers[2])};
        let arithmetic = || SessionExpressionErrorV1::RangeArithmetic {
            source:Arc::clone(source),span:range_span.clone(),
        };
        let count = if step==0 || (step>0 && start>end) || (step<0 && start<end) {
            0
        } else {
            let distance = if step>0 {end.checked_sub(start)} else {start.checked_sub(end)}
                .ok_or_else(|| arithmetic())?;
            let stride = step.checked_abs().ok_or_else(|| arithmetic())?;
            distance.checked_div(stride).and_then(|q| q.checked_add(1))
                .ok_or_else(|| arithmetic())?
        };
        if count>MAX_ARRAY_DIMENSION_V1 as i128 {
            return Err(SessionExpressionErrorV1::RangeLength {
                source:Arc::clone(source),span:range_span.clone(),actual:count,limit:MAX_ARRAY_DIMENSION_V1,
            });
        }
        let count = usize::try_from(count).map_err(|_| arithmetic())?;
        let mut payload = [0.0_f64;MAX_ARRAY_DIMENSION_V1];
        if count!=0 {payload[0]=values[0].as_slice()[0];} // Preserve the evaluated first word, including -0.
        for (index,slot) in payload[..count].iter_mut().enumerate().skip(1) {
            let offset = step.checked_mul(index as i128).ok_or_else(|| arithmetic())?;
            let value = start.checked_add(offset).ok_or_else(|| arithmetic())?;
            if !(-INTEGER_RANGE_LIMIT_V1..=INTEGER_RANGE_LIMIT_V1).contains(&value) {
                return Err(arithmetic());
            }
            *slot = value as f64; // Exact integer conversion; computed zero is +0.
        }
        DenseDouble2d::try_from_col_major([1,count as u64],&payload[..count],OUTPUT_LIMITS)
            .map_err(|error| SessionExpressionErrorV1::from(leaf_error(source,range_span,error)))
    };
    // A statement admits only one write root. Freeze this immutable target once
    // after RHS completion; selector calls cannot change the chosen binding.
    let mut write_target: Option<&SessionValueV1> = None;
    let mut write = |source: &Arc<str>, target_span: &SourceSpan, selection_span: &SourceSpan,
        arguments: &[CallArgument], rhs_span: &SourceSpan,
        values: Option<(&DenseDouble2d, &[DenseDouble2d])>, extents: &mut Option<[u64; 2]>| {
        let Some((rhs, values)) = values else {
            let target = (lookup.borrow_mut())(&source[target_span.start..target_span.end])
                .ok_or_else(|| SessionExpressionErrorV1::UnknownBinding {
                    source: Arc::clone(source), span: target_span.clone(),
                })?;
            if !(1..=2).contains(&arguments.len()) {
                return Err(SessionExpressionErrorV1::IndexArity {
                    source: Arc::clone(source), span: selection_span.clone(), actual: arguments.len(),
                });
            }
            *extents = Some(if arguments.len() == 1 {
                [target.as_slice().len() as u64; 2]
            } else { target.shape() });
            write_target = Some(target);
            return Ok(None);
        };
        let target = write_target.take().expect("write preflight froze the existing target");
        let Selection { indices, counts, full_axes } =
            select_indices(&parsed, source, arguments, values, target)?;
        // Even a zero-slot write must never disguise deletion as a successful no-op.
        if rhs.shape() == [0,0] {
            return Err(SessionExpressionErrorV1::UnsupportedEmptyAssignment {
                source: Arc::clone(source), span: rhs_span.clone(),
            });
        }
        let incompatible = |axis, lhs_extent, rhs_extent| SessionExpressionErrorV1::from(leaf_error(
            source, rhs_span, ArrayErrorV1 {context: ErrorContextV1::Construct,
                code: ErrorCodeV1::IncompatibleShape {axis, lhs_extent, rhs_extent}},
        ));
        let slots = if arguments.len() == 1 { counts[0] } else {
            counts[0].checked_mul(counts[1]).expect("bounded two-axis selection has at most 256 slots")
        };
        let scalar = rhs.shape() == [1,1];
        if !scalar {
            if arguments.len() == 1 {
                if rhs.as_slice().len() != slots {
                    return Err(incompatible(0, slots as u64, rhs.as_slice().len() as u64));
                }
            } else {
                let mut expected = [1_u64; 2];
                let mut actual = [1_u64; 2];
                for (i, &extent) in counts.iter().filter(|&&n| n != 1).enumerate() {
                    expected[i] = extent as u64;
                }
                for (i, extent) in rhs.shape().into_iter().filter(|&n| n != 1).enumerate() {
                    actual[i] = extent;
                }
                for axis in 0..2 {
                    if expected[axis] != actual[axis] {
                        return Err(incompatible(axis, expected[axis], actual[axis]));
                    }
                }
            }
        }
        let shape = target.shape();
        let target_words = target.as_slice();
        let mut words = [0.0; MAX_ARRAY_ELEMENTS_V1];
        words[..target_words.len()].copy_from_slice(target_words);
        let mut consumed = 0;
        for column in 0..(if arguments.len() == 1 {1} else {counts[1]}) {
            for row in 0..counts[0] {
                let selected_row = if full_axes[0] {row as u64} else {indices[0][row]};
                let offset = if arguments.len() == 1 {selected_row} else {
                    let selected_column = if full_axes[1] {column as u64} else {indices[1][column]};
                    selected_column.checked_mul(shape[0]).and_then(|n| n.checked_add(selected_row))
                        .expect("validated target offset is bounded")
                };
                words[offset as usize] = rhs.as_slice()[if scalar {0} else {consumed}];
                consumed += 1;
            }
        }
        debug_assert_eq!(consumed, slots);
        DenseDouble2d::try_from_col_major(shape, &words[..target_words.len()], OUTPUT_LIMITS)
            .map(Some).map_err(|error| SessionExpressionErrorV1::from(leaf_error(source, selection_span, error)))
    };
    let array = evaluate_arena_controlled(
        &parsed,
        literals,
        Some(MtimesWorkBudgetV1 { max_terms: 4096 }),
        multiply_error,
        &mut resolve,
        &mut poll,
        &mut call,
        &mut range,
        &mut write,
    )?;
    Ok(EvaluatedSessionExpressionV1 {
        source: Arc::clone(&parsed.source),
        effect,
        suppressed,
        value: SessionValueV1 { array },
    })
}
