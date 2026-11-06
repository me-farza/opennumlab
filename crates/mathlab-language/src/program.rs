// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded whole-module syntax frontend; no interpreter or path resolution.
#![forbid(unsafe_code)]

mod ast;
mod lexer;
mod numeric;
mod parser;
mod class_graph;
pub use class_graph::*;
pub use parser::{parse_class_graph_v1, parse_class_graph_controlled_v1};
mod value;
mod scientific_value;
mod scientific_numeric;
mod scientific_value_v2;
mod scientific_numeric_v2;
pub use scientific_value::*;
pub use scientific_numeric::*;
pub use scientific_value_v2::*;
pub use scientific_numeric_v2::*;
pub use ast::*;
pub use numeric::{
    ProgramNumericOperationV1, evaluate_program_numeric_controlled_v1,
    program_numeric_output_layout_v1,
};
use std::{fmt, sync::Arc};
pub use value::*;

pub const MAX_PROGRAM_SOURCE_BYTES_V1: usize = 65_536;
pub const MAX_PROGRAM_NODES_V1: usize = 4096;
pub const MAX_PROGRAM_STATEMENTS_V1: usize = 1024;
pub const MAX_PROGRAM_NUMBERS_V1: usize = 1024;
pub const MAX_PROGRAM_FUNCTIONS_V1: usize = 32;
pub const MAX_PROGRAM_NESTING_V1: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramParseErrorCodeV1 {
    Syntax,
    UnsupportedFeature,
    InvalidAssignment,
    InvalidControlFlow,
    DuplicateName,
    ResourceLimit,
    AllocationFailure,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramParseErrorV1 {
    pub source_id: u64,
    pub span: ProgramSpanV1,
    pub code: ProgramParseErrorCodeV1,
}
impl fmt::Display for ProgramParseErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Program syntax {:?} at line {}, bytes {}..{}",
            self.code, self.span.start_line, self.span.start, self.span.end
        )
    }
}
impl std::error::Error for ProgramParseErrorV1 {}

pub fn parse_program_v1(
    source_id: u64,
    source: Arc<str>,
) -> Result<ProgramModuleV1, ProgramParseErrorV1> {
    parse_program_controlled_v1(source_id, source, || false)
}

pub fn parse_program_controlled_v1(
    source_id: u64,
    source: Arc<str>,
    mut poll: impl FnMut() -> bool,
) -> Result<ProgramModuleV1, ProgramParseErrorV1> {
    let mut budget = Budget {
        source_id,
        stats: ProgramParseStatsV1::default(),
        poll: &mut poll,
    };
    let origin = ProgramSpanV1 {
        start: 0,
        end: 0,
        start_line: 1,
        end_line: 1,
    };
    budget.check(origin)?;
    if source.len() > MAX_PROGRAM_SOURCE_BYTES_V1 {
        return Err(budget.error(origin, ProgramParseErrorCodeV1::ResourceLimit));
    }
    budget.stats.source_bytes = source.len();
    let tokens = lexer::lex(&source, &mut budget)?;
    let mut module = parser::parse(source, tokens, &mut budget)?;
    validate_script_names(&module, module.script(), &mut budget)?;
    module.stats = budget.stats;
    Ok(module)
}

// Script lexical writes and local definitions share this module namespace.
// Walk only the script tree: function bodies have isolated variable scopes.
fn validate_script_names(
    module: &ProgramModuleV1,
    id: NodeIdV1,
    budget: &mut Budget<'_>,
) -> ParseResult<()> {
    use ProgramNodeKindV1 as N;
    budget.check(module.nodes()[id.index()].span)?;
    match module.nodes()[id.index()].kind {
        N::Block { mut statements } => {
            while let Some(list) = statements {
                budget.check(module.nodes()[list.index()].span)?;
                let N::List { item, next } = module.nodes()[list.index()].kind else {
                    unreachable!()
                };
                statements = next;
                validate_script_names(module, item, budget)?;
            }
        }
        N::Assignment { targets, .. } => match targets {
            AssignmentTargetsV1::Single(target) => {
                let name = match module.nodes()[target.index()].kind {
                    N::Apply { callee, .. } => callee,
                    _ => target,
                };
                validate_script_write(module, name, budget)?;
            }
            AssignmentTargetsV1::Multiple(list) => {
                let mut cursor = Some(list);
                while let Some(list) = cursor {
                    budget.check(module.nodes()[list.index()].span)?;
                    let N::List { item, next } = module.nodes()[list.index()].kind else {
                        unreachable!()
                    };
                    cursor = next;
                    validate_script_write(module, item, budget)?;
                }
            }
        },
        N::For { variable, body, .. } => {
            validate_script_write(module, variable, budget)?;
            validate_script_names(module, body, budget)?;
        }
        N::While { body, .. } => validate_script_names(module, body, budget)?,
        N::If {
            mut branches,
            otherwise,
        } => {
            while let Some(list) = branches {
                budget.check(module.nodes()[list.index()].span)?;
                let N::List { item, next } = module.nodes()[list.index()].kind else {
                    unreachable!()
                };
                branches = next;
                let N::IfBranch { body, .. } = module.nodes()[item.index()].kind else {
                    unreachable!()
                };
                validate_script_names(module, body, budget)?;
            }
            if let Some(body) = otherwise {
                validate_script_names(module, body, budget)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn validate_script_write(
    module: &ProgramModuleV1,
    name: NodeIdV1,
    budget: &mut Budget<'_>,
) -> ParseResult<()> {
    let span = module.nodes()[name.index()].span;
    for &function in module.functions() {
        budget.check(span)?;
        let ProgramNodeKindV1::Function { name: other, .. } = module.nodes()[function.index()].kind
        else {
            unreachable!()
        };
        let other = module.nodes()[other.index()].span;
        if span.end - span.start != other.end - other.start {
            continue;
        }
        let mut offset = 0;
        let mut equal = true;
        while offset < span.end - span.start {
            budget.check(span)?;
            let n = (span.end - span.start - offset).min(128);
            if module.source().as_bytes()[span.start + offset..span.start + offset + n]
                != module.source().as_bytes()[other.start + offset..other.start + offset + n]
            {
                equal = false;
                break;
            }
            offset += n;
        }
        if equal {
            return Err(budget.error(span, ProgramParseErrorCodeV1::DuplicateName));
        }
    }
    Ok(())
}

pub(crate) type ParseResult<T> = Result<T, ProgramParseErrorV1>;
pub(crate) struct Budget<'a> {
    source_id: u64,
    stats: ProgramParseStatsV1,
    poll: &'a mut dyn FnMut() -> bool,
}
impl Budget<'_> {
    fn error(&self, span: ProgramSpanV1, code: ProgramParseErrorCodeV1) -> ProgramParseErrorV1 {
        ProgramParseErrorV1 {
            source_id: self.source_id,
            span,
            code,
        }
    }
    fn check(&mut self, span: ProgramSpanV1) -> ParseResult<()> {
        self.stats.polls = self
            .stats
            .polls
            .checked_add(1)
            .ok_or_else(|| self.error(span, ProgramParseErrorCodeV1::ResourceLimit))?;
        if (self.poll)() {
            Err(self.error(span, ProgramParseErrorCodeV1::Cancelled))
        } else {
            Ok(())
        }
    }
    fn count(&self, value: usize, limit: usize, span: ProgramSpanV1) -> ParseResult<usize> {
        value
            .checked_add(1)
            .filter(|v| *v <= limit)
            .ok_or_else(|| self.error(span, ProgramParseErrorCodeV1::ResourceLimit))
    }
    fn reserve<T>(
        &mut self,
        vector: &mut Vec<T>,
        capacity: usize,
        span: ProgramSpanV1,
    ) -> ParseResult<()> {
        self.check(span)?;
        vector
            .try_reserve_exact(capacity)
            .map_err(|_| self.error(span, ProgramParseErrorCodeV1::AllocationFailure))?;
        if vector.capacity() > capacity {
            return Err(self.error(span, ProgramParseErrorCodeV1::ResourceLimit));
        }
        let bytes = vector
            .capacity()
            .checked_mul(std::mem::size_of::<T>())
            .ok_or_else(|| self.error(span, ProgramParseErrorCodeV1::ResourceLimit))?;
        self.stats.reserved_bytes = self
            .stats
            .reserved_bytes
            .checked_add(bytes)
            .ok_or_else(|| self.error(span, ProgramParseErrorCodeV1::ResourceLimit))?;
        self.check(span)
    }
}
#[cfg(test)]
mod namespace_tests {
    use super::*;
    const F16: &str = "pf_forward=sum(1);\nfunction y=sum(x)\ny=x+100;\nend\npf_late=sum(2);\nsum=[7,8];\npf_index=sum(2);\n";
    #[test]
    fn whole_script_conflict_is_located_before_module_publication() {
        let e = parse_program_v1(73, Arc::from(F16)).unwrap_err();
        assert_eq!(e.code, ProgramParseErrorCodeV1::DuplicateName);
        assert_eq!(e.source_id, 73);
        assert_eq!(&F16[e.span.start..e.span.end], "sum");
        assert_eq!(e.span.start, F16.find("sum=[").unwrap());
        assert_eq!(e.span.start_line, 6);
        for script in [
            "if 0\nsum=1;\nend\n",
            "sum(1)=3;\n",
            "[x,sum]=pair();\n",
            "for sum=[]\nend\n",
        ] {
            let source = format!("{script}function y=sum(x)\ny=x;\nend\n");
            assert_eq!(
                parse_program_v1(1, Arc::from(source)).unwrap_err().code,
                ProgramParseErrorCodeV1::DuplicateName
            );
        }
        parse_program_v1(
            1,
            Arc::from(
                "sum(1);\nfunction y=sum(x)\ny=x;\nend\nfunction y=other\nsum=2;y=sum;\nend\n",
            ),
        )
        .unwrap();
        parse_program_v1(1, Arc::from("2;\nfunction y=ans\ny=1;\nend\n")).unwrap();
    }
    #[test]
    fn namespace_validation_polls_are_reported_and_cancellable() {
        let source: Arc<str> = Arc::from("x=1;\nfunction y=f(a)\ny=a;\nend\n");
        let module = parse_program_v1(7, source.clone()).unwrap();
        for cancel in 1..=module.stats().polls {
            let mut count = 0;
            let e = parse_program_controlled_v1(7, source.clone(), || {
                count += 1;
                count == cancel
            })
            .unwrap_err();
            assert_eq!(e.code, ProgramParseErrorCodeV1::Cancelled);
            assert_eq!(count, cancel);
        }
        let mut count = 0;
        let repeated = parse_program_controlled_v1(7, source, || {
            count += 1;
            false
        })
        .unwrap();
        assert_eq!(count, repeated.stats().polls);
        assert_eq!(module.stats().nodes, repeated.stats().nodes);
        assert_eq!(
            module.stats().reserved_bytes,
            repeated.stats().reserved_bytes
        );
    }
}

mod scientific_least_squares;
pub use scientific_least_squares::*;

#[cfg(onl_qr_phase_probe)]
pub use mathlab_array::scientific::{install_scientific_qr_phase_probe_v1,ScientificQrPhaseProbeGuardV1};
