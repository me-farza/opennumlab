// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit immutable class source graph. Legacy AST/value layouts are unchanged.
use super::*;
pub const MAX_CLASS_SOURCES_V1: usize = 9;
pub const MAX_CLASS_SOURCE_BYTES_V1: usize = 262_144;
pub const MAX_CLASSES_V1: usize = 8;
pub const MAX_CLASS_PROPERTIES_V1: usize = 64;
#[derive(Debug, Clone)]
pub struct ClassSourceV1 {
    pub source_id: u64,
    pub text: Arc<str>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassParseErrorCodeV1 {
    Syntax,
    Unsupported,
    DuplicateName,
    UnknownBase,
    InheritanceCycle,
    Resource,
    Allocation,
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassParseErrorV1 {
    pub source_id: u64,
    pub span: ProgramSpanV1,
    pub code: ClassParseErrorCodeV1,
}
#[derive(Debug, Clone, Copy)]
pub enum ClassNodeKindV1 {
    Common(ProgramNodeKindV1),
    Member { receiver: NodeIdV1, name: NodeIdV1 },
}
#[derive(Debug, Clone, Copy)]
pub struct ClassNodeV1 {
    pub source_slot: u8,
    pub span: ProgramSpanV1,
    pub kind: ClassNodeKindV1,
}
#[derive(Debug, Clone, Copy)]
pub struct ClassPropertyV1 {
    pub name: NodeIdV1,
    pub initial: Option<NodeIdV1>,
    pub declaring_class: usize,
}
#[derive(Debug, Clone, Copy)]
pub struct ClassMethodV1 {
    pub name: NodeIdV1,
    pub function: NodeIdV1,
    pub declaring_class: usize,
    pub constructor: bool,
}
#[derive(Debug)]
pub struct ClassDefinitionV1 {
    pub name: NodeIdV1,
    pub base_name: Option<NodeIdV1>,
    pub base: Option<usize>,
    pub is_handle: bool,
    pub properties: Vec<ClassPropertyV1>,
    pub methods: Vec<ClassMethodV1>,
}
#[derive(Debug)]
pub struct ClassProgramGraphV1 {
    pub(super) sources: Vec<ClassSourceV1>,
    pub(super) nodes: Vec<ClassNodeV1>,
    pub(super) functions: Vec<NodeIdV1>,
    pub(super) script: NodeIdV1,
    pub(super) classes: Vec<ClassDefinitionV1>,
    pub(super) stats: ProgramParseStatsV1,
}
impl ClassProgramGraphV1 {
    pub fn sources(&self) -> &[ClassSourceV1] {
        &self.sources
    }
    pub fn nodes(&self) -> &[ClassNodeV1] {
        &self.nodes
    }
    pub fn node(&self, id: NodeIdV1) -> Option<&ClassNodeV1> {
        self.nodes.get(id.index())
    }
    pub fn common_kind(&self, id: NodeIdV1) -> Option<ProgramNodeKindV1> {
        match self.node(id)?.kind {
            ClassNodeKindV1::Common(n) => Some(n),
            _ => None,
        }
    }
    pub fn member(&self, id: NodeIdV1) -> Option<(NodeIdV1, NodeIdV1)> {
        match self.node(id)?.kind {
            ClassNodeKindV1::Member { receiver, name } => Some((receiver, name)),
            _ => None,
        }
    }
    pub fn text(&self, id: NodeIdV1) -> Option<&str> {
        let n = self.node(id)?;
        self.sources
            .get(n.source_slot as usize)?
            .text
            .get(n.span.start..n.span.end)
    }
    pub fn source_id_for(&self, id: NodeIdV1) -> Option<u64> {
        self.sources
            .get(self.node(id)?.source_slot as usize)
            .map(|s| s.source_id)
    }
    pub fn script(&self) -> NodeIdV1 {
        self.script
    }
    pub fn functions(&self) -> &[NodeIdV1] {
        &self.functions
    }
    pub fn classes(&self) -> &[ClassDefinitionV1] {
        &self.classes
    }
    pub fn stats(&self) -> ProgramParseStatsV1 {
        self.stats
    }
    pub fn inherited_property(&self, class: usize, slot: usize) -> Option<&ClassPropertyV1> {
        self.classes.get(class)?.properties.get(slot)
    }
}
impl ClassProgramGraphV1 {
    pub fn class_index(&self, name: &str) -> Option<usize> {
        self.classes
            .iter()
            .position(|c| self.text(c.name) == Some(name))
    }
    pub fn property_index(&self, class: usize, name: &str) -> Option<usize> {
        self.classes
            .get(class)?
            .properties
            .iter()
            .position(|p| self.text(p.name) == Some(name))
    }
    /// Most-derived own method wins; constructors are never inherited as methods.
    pub fn method(&self, mut class: usize, name: &str) -> Option<&ClassMethodV1> {
        for _ in 0..MAX_CLASSES_V1 {
            let definition = self.classes.get(class)?;
            if let Some(method) = definition
                .methods
                .iter()
                .find(|m| !m.constructor && self.text(m.name) == Some(name))
            {
                return Some(method);
            }
            class = definition.base?;
        }
        None
    }
    pub fn constructor(&self, class: usize) -> Option<&ClassMethodV1> {
        self.classes
            .get(class)?
            .methods
            .iter()
            .find(|m| m.constructor)
    }
}
impl std::fmt::Display for ClassParseErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Class syntax {:?} in source {} at line {}, bytes {}..{}",
            self.code, self.source_id, self.span.start_line, self.span.start, self.span.end
        )
    }
}
impl std::error::Error for ClassParseErrorV1 {}
