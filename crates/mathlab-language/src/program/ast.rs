// SPDX-License-Identifier: GPL-3.0-or-later
//! Located syntax only. These nodes do not execute a program.

use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramSpanV1 {
    pub start: usize,
    pub end: usize,
    pub start_line: usize,
    pub end_line: usize,
}

impl ProgramSpanV1 {
    pub(crate) fn through(self, last: Self) -> Self {
        Self {
            end: last.end,
            end_line: last.end_line,
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeIdV1(pub(crate) u32);
impl NodeIdV1 {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperatorV1 {
    Plus,
    Minus,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperatorV1 {
    Add,
    Subtract,
    Multiply,
    Divide,
    LeftDivide,
    ElementMultiply,
    ElementDivide,
    ElementLeftDivide,
    Power,
    ElementPower,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
    And,
    Or,
    ShortCircuitAnd,
    ShortCircuitOr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentTargetsV1 {
    Single(NodeIdV1),
    Multiple(NodeIdV1),
}

/// Every list edge, row, block and branch also consumes the shared node budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramNodeKindV1 {
    Number {
        bits: u64,
    },
    Name,
    End,
    FullAxis,
    Group {
        value: NodeIdV1,
    },
    Unary {
        operator: UnaryOperatorV1,
        input: NodeIdV1,
    },
    Binary {
        operator: BinaryOperatorV1,
        left: NodeIdV1,
        right: NodeIdV1,
    },
    Range {
        start: NodeIdV1,
        step: Option<NodeIdV1>,
        stop: NodeIdV1,
    },
    Apply {
        callee: NodeIdV1,
        arguments: Option<NodeIdV1>,
    },
    Transpose {
        input: NodeIdV1,
        conjugate: bool,
    },
    Matrix {
        rows: Option<NodeIdV1>,
    },
    Row {
        items: Option<NodeIdV1>,
    },
    List {
        item: NodeIdV1,
        next: Option<NodeIdV1>,
    },
    Block {
        statements: Option<NodeIdV1>,
    },
    ExpressionStatement {
        expression: NodeIdV1,
        suppressed: bool,
    },
    Assignment {
        targets: AssignmentTargetsV1,
        value: NodeIdV1,
        suppressed: bool,
    },
    If {
        branches: Option<NodeIdV1>,
        otherwise: Option<NodeIdV1>,
    },
    IfBranch {
        condition: NodeIdV1,
        body: NodeIdV1,
    },
    For {
        variable: NodeIdV1,
        iterable: NodeIdV1,
        body: NodeIdV1,
    },
    While {
        condition: NodeIdV1,
        body: NodeIdV1,
    },
    Break,
    Continue,
    Return,
    Function {
        name: NodeIdV1,
        inputs: Option<NodeIdV1>,
        outputs: Option<NodeIdV1>,
        body: NodeIdV1,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramNodeV1 {
    pub span: ProgramSpanV1,
    pub kind: ProgramNodeKindV1,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ProgramParseStatsV1 {
    pub source_bytes: usize,
    pub tokens: usize,
    pub nodes: usize,
    pub statements: usize,
    pub numeric_literals: usize,
    pub functions: usize,
    pub max_nesting: usize,
    pub polls: usize,
    pub reserved_bytes: usize,
}

/// Read-only arena and original UTF-8 source. No expression text is reparsed.
#[derive(Debug)]
pub struct ProgramModuleV1 {
    pub(crate) source_id: u64,
    pub(crate) source: Arc<str>,
    pub(crate) nodes: Vec<ProgramNodeV1>,
    pub(crate) functions: Vec<NodeIdV1>,
    pub(crate) script: NodeIdV1,
    pub(crate) stats: ProgramParseStatsV1,
}

impl ProgramModuleV1 {
    pub fn source_id(&self) -> u64 {
        self.source_id
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn nodes(&self) -> &[ProgramNodeV1] {
        &self.nodes
    }
    pub fn node(&self, id: NodeIdV1) -> Option<&ProgramNodeV1> {
        self.nodes.get(id.index())
    }
    pub fn script(&self) -> NodeIdV1 {
        self.script
    }
    pub fn functions(&self) -> &[NodeIdV1] {
        &self.functions
    }
    pub fn stats(&self) -> ProgramParseStatsV1 {
        self.stats
    }
    pub fn source_text(&self, span: ProgramSpanV1) -> Option<&str> {
        self.source.get(span.start..span.end)
    }
}
