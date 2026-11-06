// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in decimal arithmetic with `*`/`.*`/`./` above binary `+`/`-`.
//! All profiles reuse one private source/parser/admission/evaluation core.
//! Binary operators associate left to right; prefix signs bind tighter, with
//! real nonconjugating postfix `.'` above prefixes.
//! Matrix-literal entries retain their single-sign numeric grammar.
//! Matrix `*` uses the reviewed rank-2/scalar leaf with a caller-selected budget.
//! Dot operators use direct elementwise leaves; only `*` spends that term budget.
//! No aggregate work budget or general MATLAB source parity is claimed.
#![forbid(unsafe_code)]

use crate::SourceSpan;
use crate::decimal_expression::{DecimalExpressionErrorV1, decimal_bits, scan_decimal_arithmetic};
use crate::expression::{
    ExpectedSyntaxV1, LiteralDiagnosticCodeV1, LiteralDiagnosticV1, LiteralExpressionErrorV1,
    LiteralNodeIdV1, LiteralSignV1, LiteralStageV1,
};
use crate::expression_core::{
    BinaryOperator, CoreIr, CoreNode, CoreNodeKind, CoreParseError, CoreParsed, CoreTokenKind,
    admit_core, parse_core,
};
use crate::literal_evaluation::evaluate_core;
use mathlab_array::{DenseDouble2d, MtimesErrorV1, MtimesWorkBudgetV1};
use std::fmt;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithmeticOperatorV1 {
    Add,
    Multiply,
    Subtract,
    ElementwiseMultiply,
    ElementwiseDivide,
}

#[derive(Debug, Clone, Copy)]
pub struct ArithmeticNodeV1<'a> {
    pub span: &'a SourceSpan,
    pub kind: ArithmeticNodeKindV1<'a>,
}

#[derive(Debug, Clone, Copy)]
pub enum ArithmeticNodeKindV1<'a> {
    Number {
        literal_span: &'a SourceSpan,
    },
    Signed {
        sign: LiteralSignV1,
        sign_span: &'a SourceSpan,
        number: LiteralNodeIdV1,
    },
    Unary {
        sign: LiteralSignV1,
        sign_span: &'a SourceSpan,
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
        operator: ArithmeticOperatorV1,
        left: LiteralNodeIdV1,
        operator_span: &'a SourceSpan,
        right: LiteralNodeIdV1,
    },
}

#[derive(Clone, Copy)]
pub struct ArithmeticAstV1<'a> {
    core: &'a CoreParsed,
}
impl<'a> ArithmeticAstV1<'a> {
    pub fn source(self) -> &'a Arc<str> {
        self.core.source()
    }
    pub fn root_id(self) -> LiteralNodeIdV1 {
        self.core.root_id()
    }
    pub fn root(self) -> ArithmeticNodeV1<'a> {
        self.node(self.root_id()).expect("valid root")
    }
    pub fn node(self, id: LiteralNodeIdV1) -> Option<ArithmeticNodeV1<'a>> {
        self.core.node(id).map(node_view)
    }
    pub fn nodes(self) -> impl ExactSizeIterator<Item = ArithmeticNodeV1<'a>> + 'a {
        self.core.nodes().iter().map(node_view)
    }
}
impl fmt::Debug for ArithmeticAstV1<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.nodes()).finish()
    }
}
fn node_view(node: &CoreNode) -> ArithmeticNodeV1<'_> {
    ArithmeticNodeV1 {
        span: &node.span,
        kind: match &node.kind {
            CoreNodeKind::IndexedWrite { .. } | CoreNodeKind::Identifier { .. } | CoreNodeKind::Call { .. } | CoreNodeKind::Range { .. } => {
                unreachable!("arithmetic-v1 rejects identifiers during lexing")
            }
            CoreNodeKind::Number { literal_span } => ArithmeticNodeKindV1::Number { literal_span },
            CoreNodeKind::Signed {
                sign,
                sign_span,
                number,
            } => ArithmeticNodeKindV1::Signed {
                sign: *sign,
                sign_span,
                number: *number,
            },
            CoreNodeKind::Unary {
                sign,
                sign_span,
                operand,
            } => ArithmeticNodeKindV1::Unary {
                sign: *sign,
                sign_span,
                operand: *operand,
            },
            CoreNodeKind::Transpose {
                operator_span,
                operand,
            } => ArithmeticNodeKindV1::Transpose {
                operator_span,
                operand: *operand,
            },
            CoreNodeKind::Group { inner } => ArithmeticNodeKindV1::Group { inner: *inner },
            CoreNodeKind::Matrix { rows } => ArithmeticNodeKindV1::Matrix { rows },
            CoreNodeKind::Binary {
                operator,
                left,
                operator_span,
                right,
            } => ArithmeticNodeKindV1::Binary {
                operator: match operator {
                    BinaryOperator::Add => ArithmeticOperatorV1::Add,
                    BinaryOperator::Subtract => ArithmeticOperatorV1::Subtract,
                    BinaryOperator::Mtimes => ArithmeticOperatorV1::Multiply,
                    BinaryOperator::Times => ArithmeticOperatorV1::ElementwiseMultiply,
                    BinaryOperator::RDivide => ArithmeticOperatorV1::ElementwiseDivide,
                },
                left: *left,
                operator_span,
                right: *right,
            },
        },
    }
}

pub struct ParsedArithmeticExpressionV1 {
    core: CoreParsed,
}
impl ParsedArithmeticExpressionV1 {
    pub fn source(&self) -> &Arc<str> {
        self.core.source()
    }
    pub fn ast(&self) -> ArithmeticAstV1<'_> {
        ArithmeticAstV1 { core: &self.core }
    }
}
impl fmt::Debug for ParsedArithmeticExpressionV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedArithmeticExpressionV1")
            .field("ast", &self.ast())
            .finish()
    }
}
pub struct ArithmeticExpressionIrV1 {
    core: CoreIr,
}
impl ArithmeticExpressionIrV1 {
    pub fn source(&self) -> &Arc<str> {
        self.core.source()
    }
    pub fn ast(&self) -> ArithmeticAstV1<'_> {
        ArithmeticAstV1 {
            core: self.core.parsed(),
        }
    }
}
impl fmt::Debug for ArithmeticExpressionIrV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArithmeticExpressionIrV1")
            .field("ast", &self.ast())
            .finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub struct ArithmeticValueV1 {
    parsed: ParsedArithmeticExpressionV1,
    array: DenseDouble2d,
}
impl ArithmeticValueV1 {
    pub fn source(&self) -> &Arc<str> {
        self.parsed.source()
    }
    pub fn root_span(&self) -> &SourceSpan {
        &self.parsed.core.root().span
    }
    pub fn ast(&self) -> ArithmeticAstV1<'_> {
        self.parsed.ast()
    }
    pub const fn class(&self) -> &'static str {
        "double"
    }
    pub fn array(&self) -> &DenseDouble2d {
        &self.array
    }
    pub fn into_parts(self) -> (ParsedArithmeticExpressionV1, DenseDouble2d) {
        (self.parsed, self.array)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithmeticTokenKindV1 {
    Number,
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
pub enum ArithmeticExpressionErrorV1 {
    /// Original numeric/framing/resource/constructor/addition/host metadata.
    Shared(DecimalExpressionErrorV1),
    Syntax {
        source: Arc<str>,
        code: LiteralDiagnosticCodeV1,
        span: SourceSpan,
        expected: ExpectedSyntaxV1,
        found: Option<ArithmeticTokenKindV1>,
    },
    /// Project ArrayOperation at `*`, retaining the complete leaf error.
    Multiplication {
        source: Arc<str>,
        span: SourceSpan,
        error: MtimesErrorV1,
    },
}

impl From<DecimalExpressionErrorV1> for ArithmeticExpressionErrorV1 {
    fn from(error: DecimalExpressionErrorV1) -> Self {
        Self::Shared(error)
    }
}
impl From<LiteralExpressionErrorV1> for ArithmeticExpressionErrorV1 {
    fn from(error: LiteralExpressionErrorV1) -> Self {
        Self::Shared(error.into())
    }
}
impl From<LiteralDiagnosticV1> for ArithmeticExpressionErrorV1 {
    fn from(error: LiteralDiagnosticV1) -> Self {
        Self::Shared(error.into())
    }
}
impl From<CoreParseError> for ArithmeticExpressionErrorV1 {
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
                found: found.map(|kind| match kind {
                    CoreTokenKind::Identifier | CoreTokenKind::Assign | CoreTokenKind::Colon => {
                        unreachable!("arithmetic-v1 rejects session tokens during lexing")
                    }
                    CoreTokenKind::Number => ArithmeticTokenKindV1::Number,
                    CoreTokenKind::Plus => ArithmeticTokenKindV1::Plus,
                    CoreTokenKind::Minus => ArithmeticTokenKindV1::Minus,
                    CoreTokenKind::Star => ArithmeticTokenKindV1::Star,
                    CoreTokenKind::DotStar => ArithmeticTokenKindV1::DotStar,
                    CoreTokenKind::DotSlash => ArithmeticTokenKindV1::DotSlash,
                    CoreTokenKind::DotTranspose => ArithmeticTokenKindV1::DotTranspose,
                    CoreTokenKind::OpenParen => ArithmeticTokenKindV1::OpenParen,
                    CoreTokenKind::CloseParen => ArithmeticTokenKindV1::CloseParen,
                    CoreTokenKind::OpenBracket => ArithmeticTokenKindV1::OpenBracket,
                    CoreTokenKind::CloseBracket => ArithmeticTokenKindV1::CloseBracket,
                    CoreTokenKind::Comma => ArithmeticTokenKindV1::Comma,
                    CoreTokenKind::Semicolon => ArithmeticTokenKindV1::Semicolon,
                }),
            },
        }
    }
}
impl ArithmeticExpressionErrorV1 {
    pub fn original_source(&self) -> &Arc<str> {
        match self {
            Self::Shared(DecimalExpressionErrorV1::Diagnostic(error)) => &error.source,
            Self::Shared(DecimalExpressionErrorV1::Allocation { source, .. })
            | Self::Syntax { source, .. }
            | Self::Multiplication { source, .. } => source,
        }
    }
    pub fn span(&self) -> &SourceSpan {
        match self {
            Self::Shared(DecimalExpressionErrorV1::Diagnostic(error)) => &error.span,
            Self::Shared(DecimalExpressionErrorV1::Allocation { span, .. })
            | Self::Syntax { span, .. }
            | Self::Multiplication { span, .. } => span,
        }
    }
    pub fn stage(&self) -> LiteralStageV1 {
        match self {
            Self::Shared(DecimalExpressionErrorV1::Diagnostic(error)) => error.stage,
            Self::Shared(DecimalExpressionErrorV1::Allocation { stage, .. }) => *stage,
            Self::Syntax { .. } => LiteralStageV1::Parse,
            Self::Multiplication { .. } => LiteralStageV1::Evaluation,
        }
    }
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Self::Shared(DecimalExpressionErrorV1::Diagnostic(error)) => Some(error.code.as_str()),
            Self::Shared(DecimalExpressionErrorV1::Allocation { .. }) => None,
            Self::Syntax { code, .. } => Some(code.as_str()),
            Self::Multiplication { .. } => Some("ArrayOperation"),
        }
    }
}
impl fmt::Display for ArithmeticExpressionErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shared(error) => fmt::Display::fmt(error, f),
            Self::Syntax { code, span, .. } => {
                write!(f, "{} at bytes {}..{}", code.as_str(), span.start, span.end)
            }
            Self::Multiplication { span, error, .. } => write!(
                f,
                "ArrayOperation at bytes {}..{}: {error:?}",
                span.start, span.end
            ),
        }
    }
}
impl std::error::Error for ArithmeticExpressionErrorV1 {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Shared(error) => Some(error),
            _ => None,
        }
    }
}

fn scanner(
    source: &Arc<str>,
    start: usize,
    end: usize,
    line: usize,
) -> Option<Result<usize, ArithmeticExpressionErrorV1>> {
    scan_decimal_arithmetic(source, start, end, line).map(|result| result.map_err(Into::into))
}
fn decoder(source: &Arc<str>, span: &SourceSpan) -> Result<u64, ArithmeticExpressionErrorV1> {
    decimal_bits(source, span).map_err(Into::into)
}
fn multiply_error(
    source: &Arc<str>,
    span: &SourceSpan,
    error: MtimesErrorV1,
) -> ArithmeticExpressionErrorV1 {
    ArithmeticExpressionErrorV1::Multiplication {
        source: Arc::clone(source),
        span: span.clone(),
        error,
    }
}

/// Parse decimal arithmetic with explicit products; no leaf operation runs.
/// The caller owns source allocation. Framing and complete lexing precede parse.
pub fn parse_arithmetic_expression_v1(
    source: Arc<str>,
) -> Result<ParsedArithmeticExpressionV1, ArithmeticExpressionErrorV1> {
    Ok(ParsedArithmeticExpressionV1 {
        core: parse_core(source, scanner, true)?,
    })
}
pub fn admit_arithmetic_expression_v1(
    parsed: ParsedArithmeticExpressionV1,
) -> Result<ArithmeticExpressionIrV1, ArithmeticExpressionErrorV1> {
    Ok(ArithmeticExpressionIrV1 {
        core: admit_core(parsed.core, decoder)?,
    })
}
pub fn lower_arithmetic_expression_v1(
    source: Arc<str>,
) -> Result<ArithmeticExpressionIrV1, ArithmeticExpressionErrorV1> {
    admit_arithmetic_expression_v1(parse_arithmetic_expression_v1(source)?)
}
/// Consume admitted IR using the same term budget independently for each `*`.
/// Constructors and binary operators use explicit 16/256/2048 output limits. This
/// budget is not cumulative and gives no wall-time or aggregate-memory bound.
pub fn evaluate_arithmetic_expression_v1(
    ir: ArithmeticExpressionIrV1,
    per_operation_budget: MtimesWorkBudgetV1,
) -> Result<ArithmeticValueV1, ArithmeticExpressionErrorV1> {
    let (parsed, array) = evaluate_core(ir.core, Some(per_operation_budget), multiply_error)?;
    Ok(ArithmeticValueV1 {
        parsed: ParsedArithmeticExpressionV1 { core: parsed },
        array,
    })
}
pub fn evaluate_arithmetic_source_v1(
    source: Arc<str>,
    per_operation_budget: MtimesWorkBudgetV1,
) -> Result<ArithmeticValueV1, ArithmeticExpressionErrorV1> {
    evaluate_arithmetic_expression_v1(
        lower_arithmetic_expression_v1(source)?,
        per_operation_budget,
    )
}
