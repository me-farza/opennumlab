// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded live graphics registry consumer for the CP04 construction spike.
//!
//! Native numeric handles never leave the worker. Workspace effect revisions
//! remain separate from the object tokens used here. All requests use the
//! ordinary serialized owner, including cancellation and recovery settlement.

use crate::{
    DecimalU64, Engine, Error, ExecutionResult, Result, SessionContext, WorkerCapabilities,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json, value::RawValue};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const CAP_GRAPHICS_REGISTRY: &str = "graphics_registry.v1";
pub const MAX_NODES: usize = 128;
pub const MAX_QUERY_SAMPLES: usize = 512;
pub const MAX_SOURCE_SAMPLES: u64 = 65_536;
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
pub const MAX_CALLBACK_QUEUE: usize = 32;
pub const MAX_NAME_BYTES: usize = 256;
const MAX_ID_BYTES: usize = 128;
const MAX_WORKER_ID_BYTES: usize = 256;
const MAX_TYPE_BYTES: usize = 64;

/// Qualify the complete private worker closure, including protocol publication.
/// Native graphics enable upgrades this private dispatcher to an internal handle
/// bound to Octave's actual builtin object. A simple @builtin still name-resolves.
/// The fixed base-workspace seams are versioned; unknown source fails explicitly.
pub(crate) fn qualify_worker_source(source: &str) -> Result<String> {
    let names = "names = builtin('evalin', 'base', \"(@builtin) ('who')\");";
    let metadata = "selected = builtin('evalin', 'base', builtin('sprintf', \"(@builtin) ('whos', '%s')\", name));";
    if source.matches(names).count() != 2 || source.matches(metadata).count() != 1 {
        return Err(Error::Protocol(
            "The graphics primitive worker seam changed; integration must be reviewed.".into(),
        ));
    }
    let source = source
        .replace(names, "names = mathlab_graphics_base_names ();")
        .replace(
            metadata,
            "selected = mathlab_graphics_base_metadata (name);",
        );
    let mut qualified = String::with_capacity(source.len() + 8192);
    for line in source.lines() {
        qualified.push_str(line);
        qualified.push('\n');
        let declaration = line.trim_start();
        if declaration.starts_with("function ")
            && !declaration.contains("mathlab_graphics_primitives (")
        {
            let indent = &line[..line.len() - declaration.len()];
            qualified.push_str(indent);
            qualified.push_str("  builtin = mathlab_graphics_primitives ();\n");
        }
    }
    Ok(qualified)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphicsContext {
    pub worker_id: String,
    pub generation: DecimalU64,
    pub invocation: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphicsToken {
    pub context: GraphicsContext,
    pub object_id: String,
    pub lifetime: DecimalU64,
    pub revision: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphicsProperties {
    Figure {
        name: String,
        toolkit: String,
    },
    Axes {
        xlim: [f64; 2],
        ylim: [f64; 2],
        xlim_mode: String,
        ylim_mode: String,
    },
    Line {
        line_width: f64,
        source_shape: [u32; 2],
        total_count: DecimalU64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphicsNode {
    pub token: GraphicsToken,
    #[serde(deserialize_with = "required_option")]
    pub parent_id: Option<String>,
    pub children: Vec<String>,
    pub properties: GraphicsProperties,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsupportedGraphics {
    #[serde(deserialize_with = "required_option")]
    pub parent_id: Option<String>,
    pub native_type: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphicsSnapshot {
    #[serde(deserialize_with = "strict_session_context")]
    pub context: SessionContext,
    pub scene_revision: DecimalU64,
    pub nodes: Vec<GraphicsNode>,
    pub unsupported: Vec<UnsupportedGraphics>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphicsSamples {
    pub class_name: String,
    pub source_shape: [u32; 2],
    pub total_count: DecimalU64,
    pub start: DecimalU64,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub original_indices: Vec<DecimalU64>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphicsQuery {
    #[serde(deserialize_with = "strict_session_context")]
    pub context: SessionContext,
    pub node: GraphicsNode,
    #[serde(deserialize_with = "required_option")]
    pub samples: Option<GraphicsSamples>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphicsMutation {
    LineWidth { value: f64 },
    XLim { value: [f64; 2] },
    YLim { value: [f64; 2] },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphicsCallbackTicket {
    pub sequence: DecimalU64,
    pub expected: GraphicsToken,
    pub stimulus: String,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Protocol(format!("Invalid graphics result: {}", message.into()))
}

fn bounded(field: &str, actual: usize, limit: usize) -> Result<()> {
    if actual > limit {
        return Err(Error::LimitExceeded(format!(
            "Graphics {field} exceeds its {limit} limit ({actual})."
        )));
    }
    Ok(())
}

fn identity(field: &str, text: &str, limit: usize) -> Result<()> {
    bounded(field, text.len(), limit)?;
    if text.is_empty() || text.chars().any(char::is_control) {
        return Err(invalid(format!(
            "{field} must be nonempty and control-free"
        )));
    }
    Ok(())
}

fn required_option<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

fn strict_session_context<'de, D>(deserializer: D) -> std::result::Result<SessionContext, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Context {
        worker_id: String,
        generation: DecimalU64,
        invocation: DecimalU64,
        revision: DecimalU64,
    }
    let context = Context::deserialize(deserializer)?;
    Ok(SessionContext {
        worker_id: context.worker_id,
        generation: context.generation.get(),
        invocation: context.invocation.get(),
        revision: context.revision.get(),
    })
}

impl GraphicsContext {
    fn validate(&self) -> Result<()> {
        identity("worker identity", &self.worker_id, MAX_WORKER_ID_BYTES)
    }

    fn matches(&self, context: &SessionContext) -> bool {
        self.worker_id == context.worker_id
            && self.generation.get() == context.generation
            && self.invocation.get() == context.invocation
    }
}

impl GraphicsToken {
    fn validate(&self) -> Result<()> {
        self.context.validate()?;
        identity("object identity", &self.object_id, MAX_ID_BYTES)
    }

    fn same_object(&self, other: &Self) -> bool {
        self.context == other.context
            && self.object_id == other.object_id
            && self.lifetime == other.lifetime
    }
}

fn limits_pair(value: &[f64; 2]) -> Result<()> {
    if !value.iter().all(|number| number.is_finite()) || value[0] >= value[1] {
        return Err(invalid(
            "axes limits require two finite doubles with min < max",
        ));
    }
    Ok(())
}

fn line_width(value: f64) -> Result<()> {
    if !value.is_finite() || value <= 0.0 {
        return Err(invalid(
            "line width must be a finite positive double scalar",
        ));
    }
    Ok(())
}

fn source_shape(shape: &[u32; 2], total: DecimalU64) -> Result<()> {
    let count = u64::from(shape[0]) * u64::from(shape[1]);
    if count != total.get()
        || count > MAX_SOURCE_SAMPLES
        || shape
            .iter()
            .any(|&extent| u64::from(extent) > MAX_SOURCE_SAMPLES)
        || (count != 0 && shape[0] != 1 && shape[1] != 1)
    {
        return Err(invalid(
            "line shape/count must describe a bounded 2-D vector",
        ));
    }
    Ok(())
}

impl GraphicsNode {
    fn validate(&self, context: &SessionContext) -> Result<()> {
        self.token.validate()?;
        if !self.token.context.matches(context) {
            return Err(invalid("node belongs to a different worker or invocation"));
        }
        if let Some(parent) = &self.parent_id {
            identity("parent identity", parent, MAX_ID_BYTES)?;
            if parent == &self.token.object_id {
                return Err(invalid("node cannot be its own parent"));
            }
        }
        bounded("children", self.children.len(), MAX_NODES)?;
        let mut children = BTreeSet::new();
        for child in &self.children {
            identity("child identity", child, MAX_ID_BYTES)?;
            if child == &self.token.object_id || !children.insert(child) {
                return Err(invalid(
                    "child identities must be distinct and exclude self",
                ));
            }
        }
        match &self.properties {
            GraphicsProperties::Figure { name, toolkit } => {
                bounded("figure name bytes", name.len(), MAX_NAME_BYTES)?;
                identity("toolkit", toolkit, MAX_TYPE_BYTES)?;
                if self.parent_id.is_some() {
                    return Err(invalid("a figure must be a registry root"));
                }
            }
            GraphicsProperties::Axes {
                xlim,
                ylim,
                xlim_mode,
                ylim_mode,
            } => {
                limits_pair(xlim)?;
                limits_pair(ylim)?;
                for mode in [xlim_mode, ylim_mode] {
                    if mode != "auto" && mode != "manual" {
                        return Err(invalid("axes limit mode must be auto or manual"));
                    }
                }
                if self.parent_id.is_none() {
                    return Err(invalid("axes require a registered figure parent"));
                }
            }
            GraphicsProperties::Line {
                line_width: width,
                source_shape: shape,
                total_count,
            } => {
                line_width(*width)?;
                source_shape(shape, *total_count)?;
                if self.parent_id.is_none() || !self.children.is_empty() {
                    return Err(invalid("a line requires an axes parent and no children"));
                }
            }
        }
        Ok(())
    }
}

fn session_context(context: &SessionContext) -> Result<()> {
    identity("worker identity", &context.worker_id, MAX_WORKER_ID_BYTES)
}

impl GraphicsSnapshot {
    fn validate(&self) -> Result<()> {
        session_context(&self.context)?;
        bounded("live nodes", self.nodes.len(), MAX_NODES)?;
        bounded("unsupported nodes", self.unsupported.len(), MAX_NODES)?;
        let mut nodes = BTreeMap::new();
        for node in &self.nodes {
            node.validate(&self.context)?;
            if nodes.insert(node.token.object_id.as_str(), node).is_some() {
                return Err(invalid("snapshot contains duplicate object identities"));
            }
        }
        for node in &self.nodes {
            if let Some(parent_id) = &node.parent_id {
                let parent = nodes
                    .get(parent_id.as_str())
                    .ok_or_else(|| invalid("node parent is absent from the snapshot"))?;
                let valid_kind = matches!(
                    (&parent.properties, &node.properties),
                    (
                        GraphicsProperties::Figure { .. },
                        GraphicsProperties::Axes { .. }
                    ) | (
                        GraphicsProperties::Axes { .. },
                        GraphicsProperties::Line { .. }
                    )
                );
                if !valid_kind || !parent.children.contains(&node.token.object_id) {
                    return Err(invalid("parent/child kinds or reciprocal edges disagree"));
                }
            }
            for child_id in &node.children {
                let child = nodes
                    .get(child_id.as_str())
                    .ok_or_else(|| invalid("node child is absent from the snapshot"))?;
                if child.parent_id.as_deref() != Some(node.token.object_id.as_str()) {
                    return Err(invalid("child points to a different parent"));
                }
            }
        }
        for unsupported in &self.unsupported {
            identity("unsupported type", &unsupported.native_type, MAX_TYPE_BYTES)?;
            identity("unsupported reason", &unsupported.reason, MAX_NAME_BYTES)?;
            if let Some(parent) = &unsupported.parent_id {
                identity("unsupported parent identity", parent, MAX_ID_BYTES)?;
                if !nodes.contains_key(parent.as_str()) {
                    return Err(invalid("unsupported object parent is absent"));
                }
            }
        }
        Ok(())
    }
}

fn query_window(start: u32, count: u32) -> Result<()> {
    if start == 0 || u64::from(start) > MAX_SOURCE_SAMPLES {
        return Err(invalid("sample starts are one-based and at most 65536"));
    }
    if count == 0 || count as usize > MAX_QUERY_SAMPLES {
        return Err(invalid("sample count must be between 1 and 512"));
    }
    Ok(())
}

impl GraphicsQuery {
    fn validate(
        &self,
        expected: &GraphicsToken,
        start: u32,
        count: u32,
        mutation: bool,
    ) -> Result<()> {
        query_window(start, count)?;
        expected.validate()?;
        session_context(&self.context)?;
        self.node.validate(&self.context)?;
        if !self.node.token.same_object(expected)
            || (!mutation && self.node.token.revision != expected.revision)
            || (mutation && self.node.token.revision <= expected.revision)
        {
            return Err(invalid(
                "query token does not match the requested object/revision",
            ));
        }
        match (&self.node.properties, &self.samples) {
            (
                GraphicsProperties::Line {
                    source_shape: shape,
                    total_count,
                    ..
                },
                Some(samples),
            ) => {
                if samples.class_name != "double"
                    || samples.source_shape != *shape
                    || samples.total_count != *total_count
                    || samples.start.get() != u64::from(start)
                {
                    return Err(invalid(
                        "sample metadata does not match line/query metadata",
                    ));
                }
                source_shape(&samples.source_shape, samples.total_count)?;
                let total = total_count.get();
                let offset = u64::from(start) - 1;
                if offset > total || (total != 0 && offset == total) {
                    return Err(invalid("sample page starts outside the line data"));
                }
                let length = (total - offset).min(u64::from(count)) as usize;
                bounded("query samples", samples.x.len(), MAX_QUERY_SAMPLES)?;
                if samples.x.len() != length
                    || samples.y.len() != length
                    || samples.original_indices.len() != length
                    || !samples
                        .x
                        .iter()
                        .chain(&samples.y)
                        .all(|number| number.is_finite())
                    || samples.truncated != (offset + (length as u64) < total)
                {
                    return Err(invalid(
                        "sample lengths, finite values or continuation disagree",
                    ));
                }
                for (index, original) in samples.original_indices.iter().enumerate() {
                    if original.get() != u64::from(start) + index as u64 {
                        return Err(invalid("samples lost their original one-based indices"));
                    }
                }
            }
            (GraphicsProperties::Line { .. }, None) => {
                return Err(invalid("a line query must include its bounded sample page"));
            }
            (_, Some(_)) => return Err(invalid("only line queries may contain samples")),
            (_, None) => {}
        }
        Ok(())
    }
}

impl GraphicsMutation {
    fn validate(&self) -> Result<()> {
        match self {
            Self::LineWidth { value } => line_width(*value),
            Self::XLim { value } | Self::YLim { value } => limits_pair(value),
        }
    }

    fn validate_readback(&self, query: &GraphicsQuery) -> Result<()> {
        let accepted = match (self, &query.node.properties) {
            (Self::LineWidth { value }, GraphicsProperties::Line { line_width, .. }) => {
                value == line_width
            }
            (
                Self::XLim { value },
                GraphicsProperties::Axes {
                    xlim, xlim_mode, ..
                },
            ) => value == xlim && xlim_mode == "manual",
            (
                Self::YLim { value },
                GraphicsProperties::Axes {
                    ylim, ylim_mode, ..
                },
            ) => value == ylim && ylim_mode == "manual",
            _ => false,
        };
        if !accepted {
            return Err(invalid(
                "mutation readback does not confirm the requested property",
            ));
        }
        Ok(())
    }
}

impl GraphicsCallbackTicket {
    fn validate(&self) -> Result<()> {
        self.expected.validate()?;
        if self.sequence.get() == 0 || self.stimulus != "programmatic" {
            return Err(invalid(
                "callback ticket requires a nonzero sequence and programmatic stimulus",
            ));
        }
        Ok(())
    }
}

fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
    bounded("result bytes", bytes.len(), MAX_RESPONSE_BYTES)?;
    serde_json::from_slice(bytes).map_err(|error| invalid(error.to_string()))
}

fn decode_snapshot(bytes: &[u8]) -> Result<GraphicsSnapshot> {
    let snapshot: GraphicsSnapshot = decode(bytes)?;
    snapshot.validate()?;
    Ok(snapshot)
}

fn decode_query(
    bytes: &[u8],
    expected: &GraphicsToken,
    start: u32,
    count: u32,
    mutation: bool,
) -> Result<GraphicsQuery> {
    let query: GraphicsQuery = decode(bytes)?;
    query.validate(expected, start, count, mutation)?;
    Ok(query)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphicsReady {
    version: String,
    working_directory: PathBuf,
    protocol: WorkerCapabilities,
    #[serde(deserialize_with = "strict_session_context")]
    context: SessionContext,
}

impl Engine {
    /// Enable the optional native lifetime bridge from an exact existing file.
    /// Restart requires explicit re-enabling; recovery retains the pinned bridge
    /// through the worker's configured re-entry path and starts a new registry.
    pub fn enable_graphics_service(&mut self, bridge_path: &Path) -> Result<()> {
        if !self.capabilities().supports(CAP_GRAPHICS_REGISTRY) && self.user_execution_started {
            return Err(Error::Protocol("Enable the qualified graphics profile before user execution; restart to select it.".into()));
        }
        let bridge_path = std::fs::canonicalize(bridge_path)?;
        if !bridge_path.is_file() {
            return Err(Error::Protocol(
                "The graphics bridge path must name a file.".into(),
            ));
        }
        let (raw, _) = self.request_raw(
            json!({"command": "enable_graphics", "bridge_path": bridge_path}),
            Some(crate::COMMAND_TIMEOUT),
            MAX_RESPONSE_BYTES + 4096,
        )?;
        let ready: GraphicsReady = decode(raw.get().as_bytes())?;
        ready.protocol.validate()?;
        ready.protocol.require(CAP_GRAPHICS_REGISTRY)?;
        self.validate_context(&ready.context)?;
        if ready.version.is_empty() {
            return Err(invalid("graphics readiness omitted the runtime version"));
        }
        self.capabilities = Some(ready.protocol);
        self.version = ready.version;
        self.config.working_directory = ready.working_directory;
        Ok(())
    }

    fn graphics_request_raw(
        &mut self,
        payload: Value,
    ) -> Result<(Box<RawValue>, crate::console::ConsoleCapture)> {
        self.require_capability(CAP_GRAPHICS_REGISTRY)?;
        self.request_raw(
            payload,
            Some(crate::COMMAND_TIMEOUT),
            MAX_RESPONSE_BYTES + 4096,
        )
    }

    /// Enumerate the supported live registry without creating figures or axes.
    pub fn graphics_snapshot(&mut self) -> Result<GraphicsSnapshot> {
        let (raw, _) = self.graphics_request_raw(json!({"command": "graphics_snapshot"}))?;
        let snapshot = decode_snapshot(raw.get().as_bytes())?;
        self.validate_context(&snapshot.context)?;
        Ok(snapshot)
    }

    /// Read one exact object revision and a bounded, original-indexed line page.
    pub fn graphics_query(
        &mut self,
        expected: &GraphicsToken,
        start: u32,
        count: u32,
    ) -> Result<GraphicsQuery> {
        expected.validate()?;
        query_window(start, count)?;
        let (raw, _) = self.graphics_request_raw(json!({
            "command": "graphics_query", "expected": expected, "start": start, "count": count,
        }))?;
        let query = decode_query(raw.get().as_bytes(), expected, start, count, false)?;
        self.validate_context(&query.context)?;
        Ok(query)
    }

    /// Commit a validated property write once. Failure never triggers a retry.
    /// Listener effects may already have happened when the backend reports error.
    pub fn graphics_mutate(
        &mut self,
        expected: &GraphicsToken,
        mutation: &GraphicsMutation,
    ) -> Result<GraphicsQuery> {
        expected.validate()?;
        mutation.validate()?;
        self.user_execution_started = true;
        let (raw, _) = self.graphics_request_raw(json!({
            "command": "graphics_mutate", "expected": expected, "mutation": mutation,
        }))?;
        let query = decode_query(
            raw.get().as_bytes(),
            expected,
            1,
            MAX_QUERY_SAMPLES as u32,
            true,
        )?;
        self.validate_context(&query.context)?;
        mutation.validate_readback(&query)?;
        Ok(query)
    }

    /// Queue a programmatic callback. This is not a toolkit mouse event.
    pub fn graphics_enqueue_callback(
        &mut self,
        expected: &GraphicsToken,
    ) -> Result<GraphicsCallbackTicket> {
        expected.validate()?;
        let (raw, _) = self.graphics_request_raw(json!({
            "command": "graphics_enqueue_callback", "expected": expected,
        }))?;
        let ticket: GraphicsCallbackTicket = decode(raw.get().as_bytes())?;
        ticket.validate()?;
        if &ticket.expected != expected {
            return Err(invalid("callback ticket changed its expected object token"));
        }
        self.validate_graphics_context(&ticket.expected.context)?;
        Ok(ticket)
    }

    pub fn graphics_cancel_callback(
        &mut self,
        ticket: &GraphicsCallbackTicket,
    ) -> Result<GraphicsCallbackTicket> {
        ticket.validate()?;
        let (raw, _) = self.graphics_request_raw(json!({
            "command": "graphics_cancel_callback", "ticket": ticket,
        }))?;
        let canceled: GraphicsCallbackTicket = decode(raw.get().as_bytes())?;
        canceled.validate()?;
        if &canceled != ticket {
            return Err(invalid("cancellation returned a different callback ticket"));
        }
        self.validate_graphics_context(&canceled.expected.context)?;
        Ok(canceled)
    }

    /// Dispatch an ordered programmatic ticket through the ordinary owner.
    /// Errors retain original-source mapping and completion waits for recovery.
    pub fn graphics_dispatch_callback(
        &mut self,
        ticket: &GraphicsCallbackTicket,
    ) -> Result<ExecutionResult> {
        ticket.validate()?;
        self.user_execution_started = true;
        let (raw, capture) = self.graphics_request_raw(json!({
            "command": "graphics_dispatch_callback", "ticket": ticket,
        }))?;
        bounded("result bytes", raw.get().len(), MAX_RESPONSE_BYTES)?;
        let value: Value =
            serde_json::from_str(raw.get()).map_err(|error| invalid(error.to_string()))?;
        self.execution_result(value, capture)
    }

    fn validate_graphics_context(&self, context: &GraphicsContext) -> Result<()> {
        self.validate_context(&SessionContext {
            worker_id: context.worker_id.clone(),
            generation: context.generation.get(),
            invocation: context.invocation.get(),
            revision: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphics_primitive_qualification_is_closed_over_the_pinned_worker_seams() {
        let source = format!(
            "{}\n{}\n{}",
            crate::WORKER,
            crate::VALUE_PAGE_WORKER,
            crate::GRAPHICS_WORKER
        );
        let qualified = qualify_worker_source(&source).unwrap();
        assert!(qualified.contains("function mathlab_worker"));
        assert!(qualified.contains("names = mathlab_graphics_base_names ();"));
        assert!(qualified.contains("selected = mathlab_graphics_base_metadata (name);"));
        assert!(qualified.contains("  builtin = mathlab_graphics_primitives ();"));
        assert!(!qualified.contains("function [dispatcher, bridge] = mathlab_graphics_primitives (mode = 0, candidate = [], native_bridge = [])\n  builtin ="));
        assert!(
            qualify_worker_source(
                &source.replace("names = builtin('evalin'", "names = unsupported('evalin'")
            )
            .is_err()
        );
    }

    fn context() -> SessionContext {
        SessionContext {
            worker_id: "worker-a".into(),
            generation: 2,
            invocation: 3,
            revision: 7,
        }
    }

    fn token(id: &str) -> GraphicsToken {
        GraphicsToken {
            context: GraphicsContext {
                worker_id: "worker-a".into(),
                generation: 2.into(),
                invocation: 3.into(),
            },
            object_id: id.into(),
            lifetime: 1.into(),
            revision: 9.into(),
        }
    }

    fn snapshot() -> GraphicsSnapshot {
        GraphicsSnapshot {
            context: context(),
            scene_revision: 11.into(),
            nodes: vec![
                GraphicsNode {
                    token: token("g1"),
                    parent_id: None,
                    children: vec!["g2".into()],
                    properties: GraphicsProperties::Figure {
                        name: "Original sine".into(),
                        toolkit: "gnuplot".into(),
                    },
                },
                GraphicsNode {
                    token: token("g2"),
                    parent_id: Some("g1".into()),
                    children: vec!["g3".into()],
                    properties: GraphicsProperties::Axes {
                        xlim: [0., 10.],
                        ylim: [-1., 1.],
                        xlim_mode: "auto".into(),
                        ylim_mode: "auto".into(),
                    },
                },
                GraphicsNode {
                    token: token("g3"),
                    parent_id: Some("g2".into()),
                    children: vec![],
                    properties: GraphicsProperties::Line {
                        line_width: 1.5,
                        source_shape: [1, 501],
                        total_count: 501.into(),
                    },
                },
            ],
            unsupported: vec![],
        }
    }

    fn query() -> GraphicsQuery {
        let mut node = snapshot().nodes.remove(2);
        node.properties = GraphicsProperties::Line {
            line_width: 1.5,
            source_shape: [1, 3],
            total_count: 3.into(),
        };
        GraphicsQuery {
            context: context(),
            node,
            samples: Some(GraphicsSamples {
                class_name: "double".into(),
                source_shape: [1, 3],
                total_count: 3.into(),
                start: 2.into(),
                x: vec![0.02, 0.04],
                y: vec![0.02_f64.sin(), 0.04_f64.sin()],
                original_indices: vec![2.into(), 3.into()],
                truncated: false,
            }),
        }
    }

    fn bytes<T: Serialize>(value: &T) -> Vec<u8> {
        serde_json::to_vec(value).unwrap()
    }

    #[test]
    fn graphics_snapshot_accepts_reciprocal_tree_and_exact_large_decimal_identity() {
        let mut original = snapshot();
        original.nodes[2].token.lifetime = u64::MAX.into();
        original.nodes[2].token.revision = 9_007_199_254_741_009.into();
        assert_eq!(decode_snapshot(&bytes(&original)).unwrap(), original);
        let encoded = String::from_utf8(bytes(&original)).unwrap();
        assert!(encoded.contains("\"lifetime\":\"18446744073709551615\""));
        assert!(encoded.contains("\"revision\":\"9007199254741009\""));
    }

    #[test]
    fn graphics_decoder_rejects_unknown_duplicate_and_noncanonical_identity_fields() {
        let original = serde_json::to_value(snapshot()).unwrap();
        for value in [
            json!(1),
            json!("01"),
            json!("+1"),
            json!("18446744073709551616"),
        ] {
            let mut changed = original.clone();
            changed["nodes"][0]["token"]["revision"] = value;
            assert!(decode_snapshot(&bytes(&changed)).is_err());
        }
        for field in ["context", "nodes"] {
            let mut changed = original.clone();
            if field == "context" {
                changed["context"]["extra"] = json!(true);
            } else {
                changed["nodes"][0]["properties"]["extra"] = json!(true);
            }
            assert!(decode_snapshot(&bytes(&changed)).is_err());
        }
        let encoded = String::from_utf8(bytes(&snapshot())).unwrap();
        let duplicate = encoded.replacen(
            "\"scene_revision\":\"11\"",
            "\"scene_revision\":\"11\",\"scene_revision\":\"12\"",
            1,
        );
        assert_ne!(duplicate, encoded);
        assert!(decode_snapshot(duplicate.as_bytes()).is_err());
    }

    #[test]
    fn graphics_snapshot_rejects_cross_session_duplicate_and_broken_tree_nodes() {
        for case in 0..7 {
            let mut changed = snapshot();
            match case {
                0 => changed.nodes[2].token.context.worker_id = "worker-b".into(),
                1 => changed.nodes[2].token.context.invocation = 4.into(),
                2 => changed.nodes[2].token.object_id = "g2".into(),
                3 => changed.nodes[1].children.clear(),
                4 => changed.nodes[2].parent_id = Some("missing".into()),
                5 => changed.nodes[2].parent_id = Some("g1".into()),
                6 => changed.nodes[0].children.push("g2".into()),
                _ => unreachable!(),
            }
            assert!(
                decode_snapshot(&bytes(&changed)).is_err(),
                "accepted case {case}"
            );
        }
    }

    #[test]
    fn graphics_decoder_enforces_byte_node_name_and_source_budgets() {
        assert!(matches!(
            decode_snapshot(&vec![b' '; MAX_RESPONSE_BYTES + 1]),
            Err(Error::LimitExceeded(_))
        ));
        let mut changed = snapshot();
        changed.nodes = (0..MAX_NODES + 1)
            .map(|index| GraphicsNode {
                token: token(&format!("root{index}")),
                parent_id: None,
                children: vec![],
                properties: GraphicsProperties::Figure {
                    name: String::new(),
                    toolkit: "gnuplot".into(),
                },
            })
            .collect();
        assert!(decode_snapshot(&bytes(&changed)).is_err());
        let mut changed = snapshot();
        changed.nodes[0].properties = GraphicsProperties::Figure {
            name: "é".repeat(129),
            toolkit: "gnuplot".into(),
        };
        assert!(decode_snapshot(&bytes(&changed)).is_err());
        for (shape, count) in [([1, 65_537], 65_537), ([2, 3], 6), ([1, 501], 500)] {
            let mut changed = snapshot();
            changed.nodes[2].properties = GraphicsProperties::Line {
                line_width: 1.5,
                source_shape: shape,
                total_count: count.into(),
            };
            assert!(decode_snapshot(&bytes(&changed)).is_err());
        }
    }

    #[test]
    fn graphics_query_keeps_original_indices_and_rejects_inconsistent_pages() {
        let original = query();
        assert_eq!(
            decode_query(&bytes(&original), &token("g3"), 2, 2, false).unwrap(),
            original
        );
        for case in 0..7 {
            let mut changed = query();
            let samples = changed.samples.as_mut().unwrap();
            match case {
                0 => samples.original_indices[0] = 1.into(),
                1 => samples.y.pop().map(|_| ()).unwrap(),
                2 => samples.truncated = true,
                3 => samples.source_shape = [3, 1],
                4 => samples.start = 1.into(),
                5 => samples.class_name = "single".into(),
                6 => changed.node.token.lifetime = 2.into(),
                _ => unreachable!(),
            }
            assert!(
                decode_query(&bytes(&changed), &token("g3"), 2, 2, false).is_err(),
                "accepted case {case}"
            );
        }
        assert!(decode_query(&bytes(&original), &token("g3"), 0, 2, false).is_err());
        assert!(decode_query(&bytes(&original), &token("g3"), 2, 513, false).is_err());
    }

    #[test]
    fn graphics_mutation_requires_finite_values_and_advancing_actual_readback() {
        for value in [0., -1., f64::NAN, f64::INFINITY] {
            assert!(GraphicsMutation::LineWidth { value }.validate().is_err());
        }
        for value in [[2., 2.], [4., 2.], [f64::NEG_INFINITY, 4.], [2., f64::NAN]] {
            assert!(GraphicsMutation::XLim { value }.validate().is_err());
        }
        let mutation = GraphicsMutation::LineWidth { value: 2.5 };
        mutation.validate().unwrap();
        let mut response = query();
        response.samples.as_mut().unwrap().start = 1.into();
        response.samples.as_mut().unwrap().x.insert(0, 0.);
        response.samples.as_mut().unwrap().y.insert(0, 0.);
        response
            .samples
            .as_mut()
            .unwrap()
            .original_indices
            .insert(0, 1.into());
        assert!(response.validate(&token("g3"), 1, 512, true).is_err());
        response.node.token.revision = 10.into();
        response.validate(&token("g3"), 1, 512, true).unwrap();
        assert!(mutation.validate_readback(&response).is_err());
        if let GraphicsProperties::Line { line_width, .. } = &mut response.node.properties {
            *line_width = 2.5;
        }
        mutation.validate_readback(&response).unwrap();
    }

    #[test]
    fn graphics_callback_tickets_are_explicitly_programmatic_and_strict() {
        let ticket = GraphicsCallbackTicket {
            sequence: 1.into(),
            expected: token("g3"),
            stimulus: "programmatic".into(),
        };
        decode::<GraphicsCallbackTicket>(&bytes(&ticket))
            .unwrap()
            .validate()
            .unwrap();
        for stimulus in ["native_mouse", "", "programmatic\n"] {
            let changed = GraphicsCallbackTicket {
                stimulus: stimulus.into(),
                ..ticket.clone()
            };
            assert!(changed.validate().is_err());
        }
        let mut changed = ticket;
        changed.sequence = 0.into();
        assert!(changed.validate().is_err());
    }

    #[test]
    fn graphics_enable_readiness_requires_the_optional_capability() {
        let mut response = json!({
            "version": "11.3.0", "working_directory": "/tmp",
            "context": context(),
            "protocol": {
                "name": "mathlab-worker", "version": 1,
                "capabilities": ["binding_snapshot.v1", "checked_scalar_write.v1", "display_page.v1"],
                "limits": {"variable_cells": 2000, "response_bytes": 33554432},
            },
        });
        let ready: GraphicsReady = decode(&bytes(&response)).unwrap();
        ready.protocol.validate().unwrap();
        assert!(ready.protocol.require(CAP_GRAPHICS_REGISTRY).is_err());
        response["protocol"]["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(json!(CAP_GRAPHICS_REGISTRY));
        let ready: GraphicsReady = decode(&bytes(&response)).unwrap();
        ready.protocol.validate().unwrap();
        ready.protocol.require(CAP_GRAPHICS_REGISTRY).unwrap();
        response["context"]["invocation"] = json!(3);
        assert!(decode::<GraphicsReady>(&bytes(&response)).is_err());
    }

    /// Unlike the eight codec tests above, this test must use the real worker
    /// and native bridge. Raw malformed JSON values bypass the Rust validators
    /// so the backend's pre-set validation is exercised independently.
    #[test]
    fn actual_worker_rejects_raw_values_and_idempotent_enable_keeps_lifetimes() {
        let required = std::env::var("MATHLAB_REQUIRE_GRAPHICS_TESTS").as_deref() == Ok("1");
        let Some(executable) = crate::discover_octave() else {
            assert!(
                !required,
                "required graphics backend validation needs installed Octave"
            );
            eprintln!("SKIP graphics backend validation: no installed Octave executable.");
            return;
        };
        let Some(bridge) = std::env::var_os("MATHLAB_GRAPHICS_BRIDGE") else {
            assert!(
                !required,
                "required graphics backend validation needs MATHLAB_GRAPHICS_BRIDGE"
            );
            eprintln!("SKIP graphics backend validation: MATHLAB_GRAPHICS_BRIDGE is unset.");
            return;
        };
        let bridge = PathBuf::from(bridge);
        assert!(
            bridge.is_file(),
            "graphics bridge is missing: {}",
            bridge.display()
        );
        let directory = tempfile::Builder::new()
            .prefix("numjinn-graphics-raw-test-")
            .tempdir()
            .unwrap();
        let mut engine = Engine::start(crate::EngineConfig {
            executable,
            working_directory: directory.path().to_owned(),
        })
        .unwrap();
        assert!(!engine.capabilities().supports(CAP_GRAPHICS_REGISTRY));
        engine.enable_graphics_service(&bridge).unwrap();
        let fixtures =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/graphics");
        let quoted = fixtures.to_string_lossy().replace('\'', "''");
        let result = engine
            .execute(&format!("addpath('{quoted}','-end');"))
            .unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        let result = engine
            .run_file(&fixtures.join("numjinn_graphics_fixture.m"))
            .unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);

        let before = engine.graphics_snapshot().unwrap();
        assert_eq!(before.nodes.len(), 3);
        engine.enable_graphics_service(&bridge).unwrap();
        let enabled_again = engine.graphics_snapshot().unwrap();
        assert_eq!(enabled_again.nodes.len(), before.nodes.len());
        for node in &before.nodes {
            let after = enabled_again
                .nodes
                .iter()
                .find(|candidate| candidate.token.object_id == node.token.object_id)
                .expect("idempotent enable must preserve every public object identity");
            assert!(after.token.same_object(&node.token));
        }
        let wrong = directory.path().join("other_graphics_bridge.oct");
        std::fs::write(
            &wrong,
            b"not a native module; must be rejected before loading",
        )
        .unwrap();
        assert_raw_rejection(engine.enable_graphics_service(&wrong));
        assert!(
            engine
                .enable_graphics_service(&directory.path().join("missing.oct"))
                .is_err()
        );

        let result = engine.execute(
            "raw_before_xlim=builtin('get',graphics_axes,'xlim'); raw_before_ylim=builtin('get',graphics_axes,'ylim'); raw_before_xmode=builtin('get',graphics_axes,'xlimmode'); raw_before_ymode=builtin('get',graphics_axes,'ylimmode');"
        ).unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);

        for value in [
            json!(0),
            json!(-1),
            Value::Null,
            json!({"real": 2.5, "imaginary": 1}),
            json!([1, 2]),
            json!("2.5"),
        ] {
            let expected = live_token(&mut engine, "line");
            let result = engine.request(
                json!({
                    "command": "graphics_mutate", "expected": expected,
                    "mutation": {"kind": "line_width", "value": value},
                }),
                Some(crate::COMMAND_TIMEOUT),
            );
            assert_raw_rejection(result);
            assert_backend_properties_unchanged(&mut engine);
        }
        for kind in ["x_lim", "y_lim"] {
            for value in [
                json!([2]),
                json!([2, 4, 6]),
                json!([4, 2]),
                json!([2, 2]),
                Value::Null,
                json!("[2,4]"),
            ] {
                let expected = live_token(&mut engine, "axes");
                let result = engine.request(
                    json!({
                        "command": "graphics_mutate", "expected": expected,
                        "mutation": {"kind": kind, "value": value},
                    }),
                    Some(crate::COMMAND_TIMEOUT),
                );
                assert_raw_rejection(result);
                assert_backend_properties_unchanged(&mut engine);
            }
        }
        let expected = live_token(&mut engine, "line");
        assert_raw_rejection(engine.request(
            json!({
                "command": "graphics_mutate", "expected": expected,
                "mutation": {"kind": "unsupported_property", "value": 2.5},
            }),
            Some(crate::COMMAND_TIMEOUT),
        ));
        assert_backend_properties_unchanged(&mut engine);
    }

    fn assert_raw_rejection<T>(result: Result<T>) {
        match result {
            Err(Error::Worker(error)) => assert!(
                error.identifier == "MathLab:GraphicsValue"
                    || error.identifier == "MathLab:GraphicsUnsupported",
                "expected backend validation rejection, received {error:?}"
            ),
            Err(error) => panic!("expected backend validation rejection, received {error:?}"),
            Ok(_) => panic!("malformed graphics request unexpectedly succeeded"),
        }
    }

    fn live_token(engine: &mut Engine, kind: &str) -> GraphicsToken {
        engine
            .graphics_snapshot()
            .unwrap()
            .nodes
            .into_iter()
            .find(|node| {
                matches!(
                    (&node.properties, kind),
                    (GraphicsProperties::Line { .. }, "line")
                        | (GraphicsProperties::Axes { .. }, "axes")
                )
            })
            .expect("original fixture must retain the supported object")
            .token
    }

    fn assert_backend_properties_unchanged(engine: &mut Engine) {
        let result = engine.execute(
            "assert(builtin('get',graphics_line,'linewidth')==1.5); assert(!builtin('any',builtin('get',graphics_axes,'xlim')!=raw_before_xlim)); assert(!builtin('any',builtin('get',graphics_axes,'ylim')!=raw_before_ylim)); assert(builtin('strcmp',builtin('get',graphics_axes,'xlimmode'),raw_before_xmode)); assert(builtin('strcmp',builtin('get',graphics_axes,'ylimmode'),raw_before_ymode));"
        ).unwrap();
        assert!(
            result.error.is_none(),
            "independent builtin get changed: {:?}",
            result.error
        );
    }
}
