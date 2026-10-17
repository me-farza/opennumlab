// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-bound N01 literal-expression grammar profile v1.
//!
//! This is a bounded parser and exact-integer semantic lowering component.
//! It does not evaluate addition or admit a backend, workspace or session.
//! The frozen project predictions are not MATLAB source-expression oracles.

#![forbid(unsafe_code)]

use crate::SourceSpan;
use crate::expression_core::{admit_literals, diagnostic, parse_core, scan_exact_integer};
use mathlab_array::ArrayErrorV1;
use std::fmt;
use std::sync::Arc;

pub const MAX_SOURCE_BYTES_V1: usize = 4096;
pub const MAX_AST_NODES_V1: usize = 1024;
pub const MAX_OPEN_DELIMITERS_V1: usize = 32;
pub const MAX_NUMBER_NODES_V1: usize = 256;
pub const MAX_LITERAL_DIGITS_V1: usize = 16;
pub const MAX_EXACT_INTEGER_V1: u64 = 9_007_199_254_740_992;
pub const MAX_ARRAY_DIMENSION_V1: usize = 16;
pub const MAX_ARRAY_ELEMENTS_V1: usize = 256;
pub const MAX_ARRAY_BYTES_V1: usize = 2048;

/// An arena reference. References belong to the parsed expression that made them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiteralNodeIdV1(usize);

impl LiteralNodeIdV1 {
    pub(crate) const fn from_index(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralSignV1 {
    Plus,
    Minus,
}

#[derive(Debug, PartialEq, Eq)]
pub struct LiteralNodeV1 {
    pub span: SourceSpan,
    pub kind: LiteralNodeKindV1,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LiteralNodeKindV1 {
    Number {
        digits_span: SourceSpan,
    },
    Signed {
        sign: LiteralSignV1,
        sign_span: SourceSpan,
        number: LiteralNodeIdV1,
    },
    Group {
        inner: LiteralNodeIdV1,
    },
    Matrix {
        /// Source row order; each reference is a Number or Signed node.
        rows: Vec<Vec<LiteralNodeIdV1>>,
    },
    Add {
        left: LiteralNodeIdV1,
        operator_span: SourceSpan,
        right: LiteralNodeIdV1,
    },
}

/// An immutable original source and its immutable arena AST.
///
/// Arena storage is postorder, while resource charging follows introducing
/// tokens in source order. No recursive allocation or recursive drop is needed.
#[derive(Debug)]
pub struct ParsedLiteralExpressionV1 {
    source: Arc<str>,
    nodes: Vec<LiteralNodeV1>,
    root: LiteralNodeIdV1,
}

impl ParsedLiteralExpressionV1 {
    pub fn source(&self) -> &Arc<str> {
        &self.source
    }

    pub const fn root_id(&self) -> LiteralNodeIdV1 {
        self.root
    }

    pub fn root(&self) -> &LiteralNodeV1 {
        &self.nodes[self.root.0]
    }

    pub fn nodes(&self) -> &[LiteralNodeV1] {
        &self.nodes
    }

    pub fn node(&self, id: LiteralNodeIdV1) -> Option<&LiteralNodeV1> {
        self.nodes.get(id.0)
    }

    pub(crate) fn from_parts(
        source: Arc<str>,
        nodes: Vec<LiteralNodeV1>,
        root: LiteralNodeIdV1,
    ) -> Self {
        Self {
            source,
            nodes,
            root,
        }
    }

    /// Transfer ownership to a later adapter without cloning the AST.
    pub fn into_parts(self) -> (Arc<str>, Vec<LiteralNodeV1>, LiteralNodeIdV1) {
        (self.source, self.nodes, self.root)
    }
}

/// Exact constructor inputs; no array or arithmetic result is created here.
#[derive(Debug, PartialEq, Eq)]
pub enum LoweredLiteralV1 {
    Scalar {
        bits: u64,
    },
    Matrix {
        shape: [u64; 2],
        /// Source (row,column) is stored at row + rows * column.
        column_major_bits: Vec<u64>,
    },
}

/// Admitted literal constructors alongside the unchanged, source-spanned AST.
///
/// Number and Signed nodes have scalar inputs; Matrix nodes have matrix inputs.
/// Group and Add remain operators for the later owned leaf adapter. In
/// particular this pass performs no addition, reassociation or constant folding.
#[derive(Debug)]
pub struct LiteralExpressionIrV1 {
    parsed: ParsedLiteralExpressionV1,
    literals: Vec<Option<LoweredLiteralV1>>,
}

impl LiteralExpressionIrV1 {
    pub fn parsed(&self) -> &ParsedLiteralExpressionV1 {
        &self.parsed
    }

    pub fn source(&self) -> &Arc<str> {
        self.parsed.source()
    }

    pub fn literal(&self, id: LiteralNodeIdV1) -> Option<&LoweredLiteralV1> {
        self.literals.get(id.0).and_then(Option::as_ref)
    }

    /// A later evaluator can move constructors and nodes out without cloning.
    pub fn into_parts(self) -> (ParsedLiteralExpressionV1, Vec<Option<LoweredLiteralV1>>) {
        (self.parsed, self.literals)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralDiagnosticCodeV1 {
    ExpectedExpression,
    UnsupportedSourceForm,
    UnsupportedIdentifier,
    UnsupportedToken,
    UnexpectedToken,
    UnexpectedEof,
    ResourceLimit,
    LiteralNotAdmitted,
    MatrixNotAdmitted,
    ArrayOperation,
}

impl LiteralDiagnosticCodeV1 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExpectedExpression => "ExpectedExpression",
            Self::UnsupportedSourceForm => "UnsupportedSourceForm",
            Self::UnsupportedIdentifier => "UnsupportedIdentifier",
            Self::UnsupportedToken => "UnsupportedToken",
            Self::UnexpectedToken => "UnexpectedToken",
            Self::UnexpectedEof => "UnexpectedEof",
            Self::ResourceLimit => "ResourceLimit",
            Self::LiteralNotAdmitted => "LiteralNotAdmitted",
            Self::MatrixNotAdmitted => "MatrixNotAdmitted",
            Self::ArrayOperation => "ArrayOperation",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralStageV1 {
    Framing,
    Lex,
    Parse,
    LiteralAdmission,
    MatrixAdmission,
    Evaluation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralResourceV1 {
    SourceBytes,
    AstNodes,
    OpenDelimiters,
    NumberNodes,
}

impl LiteralResourceV1 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceBytes => "source_bytes",
            Self::AstNodes => "ast_nodes",
            Self::OpenDelimiters => "open_delimiters",
            Self::NumberNodes => "number_nodes",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedSourceFormV1 {
    ByteOrderMark,
    BlockCommentMarker,
    LoneCarriageReturn,
    SecondCodeLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralTokenKindV1 {
    Digits,
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
pub enum ExpectedSyntaxV1 {
    Primary,
    Number,
    CloseParen,
    MatrixSeparatorOrClose,
    EndOfExpression,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LiteralAdmissionFailureV1 {
    TooManyDigits { actual: usize, limit: usize },
    IntegerOverflow,
    Magnitude { actual: u64, limit: u64 },
}

#[derive(Debug, PartialEq, Eq)]
pub enum MatrixAdmissionFailureV1 {
    RaggedRows {
        row_lengths: Vec<usize>,
    },
    Dimension {
        axis: usize,
        actual: usize,
        limit: usize,
    },
    Elements {
        actual: usize,
        limit: usize,
    },
    Bytes {
        actual: usize,
        limit: usize,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum LiteralDiagnosticDetailV1 {
    SourceForm(UnsupportedSourceFormV1),
    Identifier,
    Character(char),
    Syntax {
        expected: ExpectedSyntaxV1,
        found: Option<LiteralTokenKindV1>,
    },
    Resource {
        name: LiteralResourceV1,
        limit: Option<usize>,
        attempted_count: usize,
    },
    Literal(LiteralAdmissionFailureV1),
    Matrix(MatrixAdmissionFailureV1),
    /// Retains the leaf's original context and complete structured code.
    ArrayOperation {
        error: ArrayErrorV1,
    },
}

/// Project diagnostic, bound to original UTF-8 bytes, without a native MATLAB ID.
#[derive(Debug, PartialEq, Eq)]
pub struct LiteralDiagnosticV1 {
    pub source: Arc<str>,
    pub code: LiteralDiagnosticCodeV1,
    pub stage: LiteralStageV1,
    pub span: SourceSpan,
    pub detail: LiteralDiagnosticDetailV1,
}

impl fmt::Display for LiteralDiagnosticV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at bytes {}..{} (line {})",
            self.code.as_str(),
            self.span.start,
            self.span.end,
            self.span.start_line
        )
    }
}

impl std::error::Error for LiteralDiagnosticV1 {}

/// Host allocation failure is separate from frozen project profile diagnostics.
/// The caller owns allocation of the source Arc passed to this component.
#[derive(Debug)]
pub enum LiteralExpressionErrorV1 {
    Diagnostic(LiteralDiagnosticV1),
    Allocation {
        source: Arc<str>,
        stage: LiteralStageV1,
        span: SourceSpan,
        requested_items: usize,
        cause: std::collections::TryReserveError,
    },
}

impl From<LiteralDiagnosticV1> for LiteralExpressionErrorV1 {
    fn from(value: LiteralDiagnosticV1) -> Self {
        Self::Diagnostic(value)
    }
}

impl fmt::Display for LiteralExpressionErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Diagnostic(error) => fmt::Display::fmt(error, f),
            Self::Allocation {
                requested_items,
                cause,
                ..
            } => write!(
                f,
                "literal-expression host allocation of {requested_items} items failed: {cause}"
            ),
        }
    }
}

impl std::error::Error for LiteralExpressionErrorV1 {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Diagnostic(error) => error,
            Self::Allocation { cause, .. } => cause,
        })
    }
}

/// Parse the frozen exact-integer literal-expression profile.
///
/// Taking an Arc avoids an implicit infallible copy of arbitrary source bytes.
/// Byte/framing checks, complete lexing and parser resource ordering are shared
/// with the opt-in profiles. One fallible arena projection retains the v1 AST.
pub fn parse_literal_expression_v1(
    source: Arc<str>,
) -> Result<ParsedLiteralExpressionV1, LiteralExpressionErrorV1> {
    parse_core(source, scan_exact_integer, false)?.into_legacy(LiteralStageV1::Parse)
}

fn number_bits(source: &Arc<str>, digits: &SourceSpan) -> Result<u64, LiteralExpressionErrorV1> {
    let text = &source[digits.start..digits.end];
    let failure = |reason| {
        diagnostic(
            source,
            LiteralDiagnosticCodeV1::LiteralNotAdmitted,
            LiteralStageV1::LiteralAdmission,
            digits.clone(),
            LiteralDiagnosticDetailV1::Literal(reason),
        )
    };
    if text.len() > MAX_LITERAL_DIGITS_V1 {
        return Err(failure(LiteralAdmissionFailureV1::TooManyDigits {
            actual: text.len(),
            limit: MAX_LITERAL_DIGITS_V1,
        })
        .into());
    }
    let mut magnitude = 0_u64;
    for byte in text.bytes() {
        magnitude = magnitude
            .checked_mul(10)
            .and_then(|value| value.checked_add(u64::from(byte - b'0')))
            .ok_or_else(|| failure(LiteralAdmissionFailureV1::IntegerOverflow))?;
    }
    if magnitude > MAX_EXACT_INTEGER_V1 {
        return Err(failure(LiteralAdmissionFailureV1::Magnitude {
            actual: magnitude,
            limit: MAX_EXACT_INTEGER_V1,
        })
        .into());
    }
    Ok((magnitude as f64).to_bits())
}

/// Admit the frozen v1 exact-integer literals and original matrix policy.
pub fn admit_literal_expression_v1(
    parsed: ParsedLiteralExpressionV1,
) -> Result<LiteralExpressionIrV1, LiteralExpressionErrorV1> {
    let literals = admit_literals(&parsed, number_bits)?;
    Ok(LiteralExpressionIrV1 { parsed, literals })
}

pub fn lower_literal_expression_v1(
    source: Arc<str>,
) -> Result<LiteralExpressionIrV1, LiteralExpressionErrorV1> {
    admit_literal_expression_v1(parse_literal_expression_v1(source)?)
}
