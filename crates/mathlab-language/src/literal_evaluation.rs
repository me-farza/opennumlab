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
use crate::expression_core::{CallArgument, BinaryOperator, CoreIr, CoreParsed, NodeArena, NodeKindView};
use mathlab_array::{ArrayErrorV1, DenseDouble2d, ErrorCodeV1, ErrorContextV1, LimitsV1, MtimesErrorV1, MtimesWorkBudgetV1};
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
    Visit(LiteralNodeIdV1, Option<u64>),
    Binary(LiteralNodeIdV1),
    Unary(LiteralNodeIdV1),
    Transpose(LiteralNodeIdV1),
    Call(LiteralNodeIdV1),
    Range(LiteralNodeIdV1),
    Construction(LiteralNodeIdV1, usize, usize, Option<u64>),
    IndexedWrite(LiteralNodeIdV1, bool),
}

pub(crate) fn leaf_error(
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

// Complete one whole row/list. Nested brackets arrive as independent values.
fn join_arrays(source: &Arc<str>, span: &SourceSpan, arrays: &[DenseDouble2d],
    horizontal: bool) -> Result<DenseDouble2d, LiteralExpressionErrorV1> {
    let error = |code| leaf_error(source, span,
        ArrayErrorV1 { context: ErrorContextV1::Construct, code });
    let axis = usize::from(horizontal);
    let other = 1 - axis;
    let nonempty = arrays.iter().any(|a| !a.as_slice().is_empty());
    let mut shape = [0_u64; 2];
    if nonempty {
        let mut extent = None;
        for array in arrays.iter().filter(|a| !a.as_slice().is_empty()) {
            let next = array.shape();
            if let Some(expected) = extent {
                if expected != next[other] {
                    return Err(error(ErrorCodeV1::IncompatibleShape {
                        axis: other, lhs_extent: expected, rhs_extent: next[other],
                    }));
                }
            } else { extent = Some(next[other]); }
        }
        shape[other] = extent.expect("one nonempty operand");
    } else {
        for array in arrays {
            let next = array.shape();
            shape[0] = shape[0].max(next[0]);
            shape[1] = shape[1].max(next[1]);
        }
        if shape[0] > 0 && shape[1] > 0 { shape = [0,0]; }
    }
    // Crossed all-empty axes canonicalize before any sum or output cap.
    let crossed = !nonempty && arrays.iter().any(|a| a.shape()[0] > 0)
        && arrays.iter().any(|a| a.shape()[1] > 0);
    if !crossed {
        shape[axis] = 0;
        for array in arrays.iter().filter(|a| !nonempty || !a.as_slice().is_empty()) {
            shape[axis] = shape[axis].checked_add(array.shape()[axis]).ok_or_else(||
                error(ErrorCodeV1::DimensionLimitExceeded {
                    axis, actual: u64::MAX, limit: OUTPUT_LIMITS.max_dimension,
                }))?;
        }
    }
    let elements = shape[0].checked_mul(shape[1]).ok_or_else(||
        error(ErrorCodeV1::ShapeProductOverflow { rows: shape[0], columns: shape[1] }))?;
    let bytes = elements.checked_mul(8).ok_or_else(||
        error(ErrorCodeV1::ByteSizeOverflow { elements, element_width_bytes: 8 }))?;
    for (axis, actual) in shape.into_iter().enumerate() {
        if actual > OUTPUT_LIMITS.max_dimension {
            return Err(error(ErrorCodeV1::DimensionLimitExceeded {
                axis, actual, limit: OUTPUT_LIMITS.max_dimension,
            }));
        }
    }
    if elements > OUTPUT_LIMITS.max_elements {
        return Err(error(ErrorCodeV1::ElementLimitExceeded { actual: elements, limit: OUTPUT_LIMITS.max_elements }));
    }
    if bytes > OUTPUT_LIMITS.max_bytes {
        return Err(error(ErrorCodeV1::ByteLimitExceeded { actual: bytes, limit: OUTPUT_LIMITS.max_bytes }));
    }
    let mut words = [0.0; MAX_ARRAY_ELEMENTS_V1];
    let mut offset = 0;
    if horizontal {
        for array in arrays.iter().filter(|a| !a.as_slice().is_empty()) {
            let data = array.as_slice();
            words[offset..offset + data.len()].copy_from_slice(data);
            offset += data.len();
        }
    } else {
        for column in 0..shape[1] as usize {
            for array in arrays.iter().filter(|a| !a.as_slice().is_empty()) {
                let rows = array.shape()[0] as usize;
                words[offset..offset + rows].copy_from_slice(&array.as_slice()[column*rows..(column+1)*rows]);
                offset += rows;
            }
        }
    }
    debug_assert_eq!(offset, elements as usize);
    DenseDouble2d::try_from_col_major(shape, &words[..offset], OUTPUT_LIMITS)
        .map_err(|cause| leaf_error(source, span, cause))
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
        &mut |_, _, _| unreachable!("legacy profiles cannot resolve names"),
        &mut |_, _| Ok(()),
        &mut |_, _, _, _, _, _| unreachable!("legacy profiles cannot call constructors"),
        &mut |_, _, _, _| unreachable!("legacy profiles cannot evaluate ranges"),
        &mut |_, _, _, _, _, _, _| unreachable!("legacy profiles cannot perform indexed writes"),
    )
}

pub(crate) fn evaluate_arena_controlled<E>(
    parsed: &impl NodeArena,
    mut literals: Vec<Option<LoweredLiteralV1>>,
    work_budget: Option<MtimesWorkBudgetV1>,
    multiplication_error: fn(&Arc<str>, &SourceSpan, MtimesErrorV1) -> E,
    resolve: &mut impl FnMut(&Arc<str>, &SourceSpan, Option<u64>) -> Result<DenseDouble2d, E>,
    poll: &mut impl FnMut(&Arc<str>, &SourceSpan) -> Result<(), E>,
    // None performs call preflight before children; Some receives every evaluated
    // expression child in source order; full-axis markers produce no value.
    // Preflight sets original-position extents only for a bound read. Ordinary
    // calls leave them absent so Visit preserves the incoming optional extent.
    // The callback belongs only to the session profile.
    call: &mut impl FnMut(
        &Arc<str>, &SourceSpan, &SourceSpan, &[CallArgument], Option<&[DenseDouble2d]>, &mut Option<[u64; 2]>,
    ) -> Result<Option<DenseDouble2d>, E>,
    range: &mut impl FnMut(&Arc<str>, &SourceSpan, &[LiteralNodeIdV1], &[DenseDouble2d])
        -> Result<DenseDouble2d, E>,
    write: &mut impl FnMut(&Arc<str>, &SourceSpan, &SourceSpan, &[CallArgument], &SourceSpan,
        Option<(&DenseDouble2d, &[DenseDouble2d])>, &mut Option<[u64; 2]>)
        -> Result<Option<DenseDouble2d>, E>,
) -> Result<DenseDouble2d, E>
where
    E: From<LiteralExpressionErrorV1>,
{
    let mut tasks: Vec<Task> = Vec::new();
    let mut values: Vec<DenseDouble2d> = Vec::new();
    let count = parsed.len();
    // Every live task names a distinct AST node: Visit becomes an operator, or is
    // replaced by its distinct children. Every live value represents a disjoint
    // evaluated subtree (a completed construction row covers its disjoint child
    // subtrees). A construction keeps exactly one task and queues only its current
    // row, replacing that continuation in place. An indexed write similarly keeps
    // one phase task, visiting RHS before disjoint selector children; its RHS value
    // remains one disjoint subtree. The AST count bounds both stacks.
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
    tasks.push(Task::Visit(parsed.root_id(), None));
    while let Some(task) = tasks.pop() {
        let id = match task {
            Task::IndexedWrite(id, _) | Task::Construction(id, _, _, _) | Task::Visit(id, _) | Task::Binary(id) | Task::Unary(id) | Task::Transpose(id) | Task::Call(id) | Task::Range(id) => id,
        };
        let task_span = parsed.view(id).span;
        poll(parsed.source(), task_span)?;
        match task {
            Task::Visit(id, extent) => {
                let node = parsed.view(id);
                match node.kind {
                    NodeKindView::IndexedWrite { rhs, .. } => {
                        tasks.push(Task::IndexedWrite(id, false));
                        tasks.push(Task::Visit(rhs, None));
                    }
                    NodeKindView::Construction { rows } => {
                        tasks.push(Task::Construction(id, 0, values.len(), extent));
                        for &child in rows[0].iter().rev() {
                            tasks.push(Task::Visit(child, extent));
                        }
                    }
                    NodeKindView::Range { start,step,end } => {
                        tasks.push(Task::Range(id));
                        tasks.push(Task::Visit(end, extent));
                        if let Some(step) = step { tasks.push(Task::Visit(step, extent)); }
                        tasks.push(Task::Visit(start, extent));
                    }
                    NodeKindView::Call { callee_span, arguments } => {
                        let mut bound_extents = None;
                        call(parsed.source(), callee_span, node.span, arguments, None, &mut bound_extents)?;
                        tasks.push(Task::Call(id));
                        for (index, argument) in arguments.iter().enumerate().rev() {
                            let child_extent = bound_extents.map_or(extent, |axes| Some(axes[index]));
                            if let Some(id) = argument.expression() { tasks.push(Task::Visit(id, child_extent)); }
                        }
                    }
                    NodeKindView::Identifier { identifier_span } => {
                        values.push(resolve(parsed.source(), identifier_span, extent)?);
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
                        tasks.push(Task::Visit(operand, extent));
                    }
                    NodeKindView::Transpose { operand, .. } => {
                        tasks.push(Task::Transpose(id));
                        tasks.push(Task::Visit(operand, extent));
                    }
                    NodeKindView::Group { inner } => tasks.push(Task::Visit(inner, extent)),
                    NodeKindView::Binary { left, right, .. } => {
                        // LIFO visits the complete left subtree first. An error
                        // returns before any pending right subtree is evaluated.
                        tasks.push(Task::Binary(id));
                        tasks.push(Task::Visit(right, extent));
                        tasks.push(Task::Visit(left, extent));
                    }
                }
            }
            Task::IndexedWrite(id, selectors_done) => {
                let node = parsed.view(id);
                let NodeKindView::IndexedWrite { target_span, selection_span, arguments, rhs } = node.kind else {
                    unreachable!("only indexed writes schedule write phases");
                };
                let rhs_span = parsed.view(rhs).span;
                if !selectors_done {
                    let mut extents = None;
                    write(parsed.source(), target_span, selection_span, arguments, rhs_span, None, &mut extents)?;
                    let extents = extents.expect("write preflight freezes target extents");
                    tasks.push(Task::IndexedWrite(id, true));
                    for (index, argument) in arguments.iter().enumerate().rev() {
                        if let Some(child) = argument.expression() {
                            tasks.push(Task::Visit(child, Some(extents[index])));
                        }
                    }
                } else {
                    let start = values.len() - arguments.iter().filter_map(CallArgument::expression).count() - 1;
                    let array = write(parsed.source(), target_span, selection_span, arguments, rhs_span,
                        Some((&values[start], &values[start+1..])), &mut None)?
                        .expect("write completion produces the complete owned target");
                    values.truncate(start);
                    values.push(array);
                }
            }
            Task::Construction(id, row, base, extent) => {
                let node = parsed.view(id);
                let NodeKindView::Construction { rows } = node.kind else {
                    unreachable!("only constructions schedule row completion");
                };
                let start = values.len() - rows[row].len();
                let value = join_arrays(parsed.source(), node.span, &values[start..], true)?;
                values.truncate(start);
                values.push(value);
                if row + 1 < rows.len() {
                    // One continuation, with only the next row's children queued.
                    tasks.push(Task::Construction(id, row + 1, base, extent));
                    for &child in rows[row + 1].iter().rev() {
                        tasks.push(Task::Visit(child, extent));
                    }
                } else {
                    let value = join_arrays(parsed.source(), node.span, &values[base..], false)?;
                    values.truncate(base);
                    values.push(value);
                }
            }
            Task::Range(id) => {
                let node = parsed.view(id);
                let NodeKindView::Range { start,step,end } = node.kind else {
                    unreachable!("only Range nodes schedule range completion");
                };
                let arguments = [start,step.unwrap_or(end),end];
                let count = if step.is_some() {3} else {2};
                let offset = values.len()-count;
                let array = range(parsed.source(),node.span,&arguments[..count],&values[offset..])?;
                values.truncate(offset);
                values.push(array);
            }
            Task::Call(id) => {
                let node = parsed.view(id);
                let NodeKindView::Call { callee_span, arguments } = node.kind else {
                    unreachable!("only Call nodes schedule call completion");
                };
                let start = values.len() - arguments.iter().filter_map(CallArgument::expression).count();
                let array = call(parsed.source(), callee_span, node.span, arguments,
                    Some(&values[start..]), &mut None)?.expect("call completion produces one owned array");
                values.truncate(start);
                values.push(array);
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
