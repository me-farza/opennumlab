// SPDX-License-Identifier: GPL-3.0-or-later
//! Owned leaf composition for the bounded N01 literal-expression profile v1.
//!
//! This adapter has no workspace, session, backend selection or fallback. Its
//! source-level MATLAB comparisons remain unmeasured, including signed zero
//! and grouped addition. The original parser AST is retained with the value.

#![forbid(unsafe_code)]

use crate::SourceSpan;
use crate::expression::{
    LiteralDiagnosticCodeV1, LiteralDiagnosticDetailV1, LiteralDiagnosticV1,
    LiteralExpressionErrorV1, LiteralExpressionIrV1, LiteralNodeIdV1, LiteralSignV1,
    LiteralStageV1, LoweredLiteralV1, MAX_ARRAY_BYTES_V1, MAX_ARRAY_DIMENSION_V1,
    MAX_ARRAY_ELEMENTS_V1, ParsedLiteralExpressionV1, lower_literal_expression_v1,
};
use crate::expression_core::{BinaryOperator, CoreIr, CoreParsed, NodeArena, NodeKindView};
use mathlab_array::{ArrayErrorV1, DenseDouble2d, LimitsV1, MtimesErrorV1, MtimesWorkBudgetV1};
use std::sync::Arc;

const OUTPUT_LIMITS: LimitsV1 = LimitsV1 {
    max_dimension: MAX_ARRAY_DIMENSION_V1 as u64,
    max_elements: MAX_ARRAY_ELEMENTS_V1 as u64,
    max_bytes: MAX_ARRAY_BYTES_V1 as u64,
};

/// One owned real-double result and its immutable original AST/source snapshot.
#[derive(Debug)]
pub struct LiteralValueV1 {
    parsed: ParsedLiteralExpressionV1,
    array: DenseDouble2d,
}

impl LiteralValueV1 {
    pub fn source(&self) -> &Arc<str> {
        self.parsed.source()
    }

    pub fn root_span(&self) -> &SourceSpan {
        &self.parsed.root().span
    }

    pub fn parsed(&self) -> &ParsedLiteralExpressionV1 {
        &self.parsed
    }

    pub const fn class(&self) -> &'static str {
        "double"
    }

    pub fn array(&self) -> &DenseDouble2d {
        &self.array
    }

    pub fn into_parts(self) -> (ParsedLiteralExpressionV1, DenseDouble2d) {
        (self.parsed, self.array)
    }
}

#[derive(Debug, Clone, Copy)]
enum Task {
    Visit(LiteralNodeIdV1),
    Binary(LiteralNodeIdV1),
    Unary(LiteralNodeIdV1),
    Transpose(LiteralNodeIdV1),
}

fn leaf_error(
    source: &Arc<str>,
    span: &SourceSpan,
    error: ArrayErrorV1,
) -> LiteralExpressionErrorV1 {
    LiteralDiagnosticV1 {
        source: Arc::clone(source),
        code: LiteralDiagnosticCodeV1::ArrayOperation,
        stage: LiteralStageV1::Evaluation,
        span: span.clone(),
        detail: LiteralDiagnosticDetailV1::ArrayOperation { error },
    }
    .into()
}

/// Consume admitted IR and evaluate Group children and Add subtrees left first.
///
/// Every constructor and addition receives the explicit v1 output limits. Signed
/// scalars use their admitted bits directly; matrix entries never become arrays.
/// Group transfers its child's value. No arrays are cloned, and the leaf alone
/// resolves broadcasting, output limits, arithmetic and allocation failures.
pub fn evaluate_literal_expression_v1(
    ir: LiteralExpressionIrV1,
) -> Result<LiteralValueV1, LiteralExpressionErrorV1> {
    let (parsed, literals) = ir.into_parts();
    let array = evaluate_arena(
        &parsed,
        literals,
        None,
        no_mtimes::<LiteralExpressionErrorV1>,
    )?;
    Ok(LiteralValueV1 { parsed, array })
}

pub(crate) fn no_mtimes<E>(_: &Arc<str>, _: &SourceSpan, _: MtimesErrorV1) -> E {
    unreachable!("restricted profiles contain no multiplication nodes")
}

pub(crate) fn evaluate_core<E>(
    ir: CoreIr,
    work_budget: Option<MtimesWorkBudgetV1>,
    multiplication_error: fn(&Arc<str>, &SourceSpan, MtimesErrorV1) -> E,
) -> Result<(CoreParsed, DenseDouble2d), E>
where
    E: From<LiteralExpressionErrorV1>,
{
    let CoreIr { parsed, literals } = ir;
    let array = evaluate_arena(&parsed, literals, work_budget, multiplication_error)?;
    Ok((parsed, array))
}

pub(crate) fn evaluate_arena<E>(
    parsed: &impl NodeArena,
    literals: Vec<Option<LoweredLiteralV1>>,
    work_budget: Option<MtimesWorkBudgetV1>,
    multiplication_error: fn(&Arc<str>, &SourceSpan, MtimesErrorV1) -> E,
) -> Result<DenseDouble2d, E>
where
    E: From<LiteralExpressionErrorV1>,
{
    evaluate_arena_controlled(
        parsed,
        literals,
        work_budget,
        multiplication_error,
        &mut |_, _| unreachable!("legacy profiles cannot resolve names"),
        &mut |_, _| Ok(()),
    )
}

pub(crate) fn evaluate_arena_controlled<E>(
    parsed: &impl NodeArena,
    mut literals: Vec<Option<LoweredLiteralV1>>,
    work_budget: Option<MtimesWorkBudgetV1>,
    multiplication_error: fn(&Arc<str>, &SourceSpan, MtimesErrorV1) -> E,
    resolve: &mut impl FnMut(&Arc<str>, &SourceSpan) -> Result<DenseDouble2d, E>,
    poll: &mut impl FnMut(&Arc<str>, &SourceSpan) -> Result<(), E>,
) -> Result<DenseDouble2d, E>
where
    E: From<LiteralExpressionErrorV1>,
{
    let mut tasks: Vec<Task> = Vec::new();
    let mut values: Vec<DenseDouble2d> = Vec::new();
    let count = parsed.len();
    // Every live task names a distinct AST node: Visit becomes an operator, or is
    // replaced by its distinct children. Every live value represents a disjoint
    // evaluated subtree. The AST count therefore bounds both stack capacities.
    tasks
        .try_reserve_exact(count)
        .map_err(|cause| LiteralExpressionErrorV1::Allocation {
            source: Arc::clone(parsed.source()),
            stage: LiteralStageV1::Evaluation,
            span: parsed.view(parsed.root_id()).span.clone(),
            requested_items: count,
            cause,
        })?;
    values
        .try_reserve_exact(count)
        .map_err(|cause| LiteralExpressionErrorV1::Allocation {
            source: Arc::clone(parsed.source()),
            stage: LiteralStageV1::Evaluation,
            span: parsed.view(parsed.root_id()).span.clone(),
            requested_items: count,
            cause,
        })?;
    tasks.push(Task::Visit(parsed.root_id()));
    while let Some(task) = tasks.pop() {
        let id = match task {
            Task::Visit(id) | Task::Binary(id) | Task::Unary(id) | Task::Transpose(id) => id,
        };
        let task_span = parsed.view(id).span;
        poll(parsed.source(), task_span)?;
        match task {
            Task::Visit(id) => {
                let node = parsed.view(id);
                match node.kind {
                    NodeKindView::Identifier { identifier_span } => {
                        values.push(resolve(parsed.source(), identifier_span)?);
                    }
                    NodeKindView::Number { .. } | NodeKindView::Signed { .. } => {
                        let Some(LoweredLiteralV1::Scalar { bits }) = literals[id.index()].take()
                        else {
                            unreachable!("admitted scalar has exact constructor bits");
                        };
                        let array = DenseDouble2d::try_from_col_major(
                            [1, 1],
                            &[f64::from_bits(bits)],
                            OUTPUT_LIMITS,
                        )
                        .map_err(|error| leaf_error(parsed.source(), node.span, error))?;
                        values.push(array);
                    }
                    NodeKindView::Matrix { .. } => {
                        let Some(LoweredLiteralV1::Matrix {
                            shape,
                            column_major_bits,
                        }) = literals[id.index()].take()
                        else {
                            unreachable!("admitted matrix has column-major constructor bits");
                        };
                        // Geometry admission bounds this scratch array. Rebuild
                        // f64 values by bits, without parsing or extra allocation.
                        let mut scratch = [0.0_f64; MAX_ARRAY_ELEMENTS_V1];
                        for (slot, bits) in scratch.iter_mut().zip(&column_major_bits) {
                            *slot = f64::from_bits(*bits);
                        }
                        let array = DenseDouble2d::try_from_col_major(
                            shape,
                            &scratch[..column_major_bits.len()],
                            OUTPUT_LIMITS,
                        )
                        .map_err(|error| leaf_error(parsed.source(), node.span, error))?;
                        values.push(array);
                    }
                    NodeKindView::Unary { operand, .. } => {
                        tasks.push(Task::Unary(id));
                        tasks.push(Task::Visit(operand));
                    }
                    NodeKindView::Transpose { operand, .. } => {
                        tasks.push(Task::Transpose(id));
                        tasks.push(Task::Visit(operand));
                    }
                    NodeKindView::Group { inner } => tasks.push(Task::Visit(inner)),
                    NodeKindView::Binary { left, right, .. } => {
                        // LIFO visits the complete left subtree first. An error
                        // returns before any pending right subtree is evaluated.
                        tasks.push(Task::Binary(id));
                        tasks.push(Task::Visit(right));
                        tasks.push(Task::Visit(left));
                    }
                }
            }
            Task::Unary(id) => {
                let NodeKindView::Unary { sign, .. } = parsed.view(id).kind else {
                    unreachable!("only Unary nodes schedule a unary operation");
                };
                let operand = values.pop().expect("unary child produced one owned array");
                // Plus transfers the owned buffer. Minus flips signs in that
                // same buffer, with unchanged shape and no allocation or budget.
                values.push(match sign {
                    LiteralSignV1::Plus => operand,
                    LiteralSignV1::Minus => operand.into_negated(),
                });
            }
            Task::Transpose(id) => {
                let NodeKindView::Transpose { operator_span, .. } = parsed.view(id).kind else {
                    unreachable!("only Transpose nodes schedule a transpose");
                };
                let operand = values
                    .pop()
                    .expect("transpose child produced one owned array");
                let array = operand
                    .try_transpose(OUTPUT_LIMITS)
                    .map_err(|error| E::from(leaf_error(parsed.source(), operator_span, error)))?;
                values.push(array);
            }
            Task::Binary(id) => {
                let node = parsed.view(id);
                let NodeKindView::Binary {
                    operator,
                    operator_span,
                    ..
                } = node.kind
                else {
                    unreachable!("only Binary nodes schedule a binary operation");
                };
                let right = values.pop().expect("right child produced one owned array");
                let left = values.pop().expect("left child produced one owned array");
                let array = match operator {
                    BinaryOperator::Add => {
                        left.try_add(&right, OUTPUT_LIMITS).map_err(|error| {
                            E::from(leaf_error(parsed.source(), operator_span, error))
                        })?
                    }
                    BinaryOperator::Subtract => {
                        left.try_sub(&right, OUTPUT_LIMITS).map_err(|error| {
                            E::from(leaf_error(parsed.source(), operator_span, error))
                        })?
                    }
                    BinaryOperator::Times => {
                        left.try_times(&right, OUTPUT_LIMITS).map_err(|error| {
                            E::from(leaf_error(parsed.source(), operator_span, error))
                        })?
                    }
                    BinaryOperator::RDivide => {
                        left.try_rdivide(&right, OUTPUT_LIMITS).map_err(|error| {
                            E::from(leaf_error(parsed.source(), operator_span, error))
                        })?
                    }
                    BinaryOperator::Mtimes => left
                        .try_mtimes(
                            &right,
                            OUTPUT_LIMITS,
                            work_budget.expect(
                                "multiplication profile requires an explicit per-operation budget",
                            ),
                        )
                        .map_err(|error| {
                            multiplication_error(parsed.source(), operator_span, error)
                        })?,
                };
                values.push(array);
            }
        }
        poll(parsed.source(), task_span)?;
    }
    let array = values
        .pop()
        .expect("root expression produced one owned array");
    debug_assert!(values.is_empty());
    Ok(array)
}

/// Frame, lex, parse and admit the entire source before invoking the leaf.
pub fn evaluate_literal_source_v1(
    source: Arc<str>,
) -> Result<LiteralValueV1, LiteralExpressionErrorV1> {
    evaluate_literal_expression_v1(lower_literal_expression_v1(source)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mathlab_array::{ErrorCodeV1, ErrorContextV1};

    #[test]
    fn constructor_error_attachment_retains_the_complete_original_leaf_error() {
        // Real allocation rejection is not forced through the public leaf API.
        // Check the attachment seam with the original, structured leaf metadata.
        let source: Arc<str> = Arc::from("% λ\n[1,2]");
        let at = SourceSpan {
            start: 5,
            end: 10,
            start_line: 2,
            end_line: 2,
        };
        let original = ArrayErrorV1 {
            context: ErrorContextV1::Construct,
            code: ErrorCodeV1::AllocationFailure {
                requested_elements: 2,
                requested_bytes: 16,
            },
        };
        let LiteralExpressionErrorV1::Diagnostic(attached) = leaf_error(&source, &at, original)
        else {
            panic!("leaf error must remain a project ArrayOperation diagnostic");
        };
        assert!(Arc::ptr_eq(&source, &attached.source));
        assert_eq!(attached.code, LiteralDiagnosticCodeV1::ArrayOperation);
        assert_eq!(attached.stage, LiteralStageV1::Evaluation);
        assert_eq!(attached.span, at);
        assert_eq!(
            attached.detail,
            LiteralDiagnosticDetailV1::ArrayOperation { error: original }
        );
    }
}
