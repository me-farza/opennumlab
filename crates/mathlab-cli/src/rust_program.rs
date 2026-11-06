// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit one-file Rust route; no Engine construction, discovery or fallback.
mod signal;
pub(super) mod scientific;
pub(super) mod scientific_v2;
pub(super) mod class;
use mathlab_engine::rust_program::*;
use mathlab_language::program::*;
use serde::{
    Serialize, Serializer,
    ser::{SerializeSeq, SerializeStruct},
};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
const REPLY_LIMIT: usize = 8 * 1024 * 1024;
const KIND: &str = "mathlab-rust-program";
#[derive(Clone, Copy)]
struct Early {
    phase: &'static str,
    code: &'static str,
    status: &'static str,
    span: Option<ProgramSpanV1>,
    reason: Option<ProgramCancelReasonV1>,
}
impl Early {
    fn error(phase: &'static str, code: &'static str) -> Self {
        Self {
            phase,
            code,
            status: "error",
            span: None,
            reason: None,
        }
    }
    fn resource(phase: &'static str, code: &'static str) -> Self {
        Self {
            status: "resource",
            ..Self::error(phase, code)
        }
    }
    fn cancelled(phase: &'static str, reason: ProgramCancelReasonV1) -> Self {
        Self {
            status: "cancelled",
            reason: Some(reason),
            ..Self::error(phase, "Cancelled")
        }
    }
}
#[derive(Serialize)]
struct Source {
    id: u64,
    path_display: String,
    sha256: HexDigest,
    bytes: usize,
}
struct Failure {
    early: Early,
    source: Option<Source>,
}
#[derive(Serialize, Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
    start_line: usize,
    end_line: usize,
}
impl From<ProgramSpanV1> for Span {
    fn from(s: ProgramSpanV1) -> Self {
        Self {
            start: s.start,
            end: s.end,
            start_line: s.start_line,
            end_line: s.end_line,
        }
    }
}
#[derive(Serialize, Clone, Copy)]
struct Location {
    source_id: u64,
    span: Span,
}
impl From<ProgramLocationV1> for Location {
    fn from(l: ProgramLocationV1) -> Self {
        Self {
            source_id: l.source_id,
            span: l.span.into(),
        }
    }
}
#[derive(Serialize, Clone, Copy)]
struct Context {
    session_id: u64,
    epoch: u64,
    revision: u64,
}
impl From<ProgramContextV1> for Context {
    fn from(c: ProgramContextV1) -> Self {
        Self {
            session_id: c.session_id,
            epoch: c.epoch,
            revision: c.revision,
        }
    }
}
#[derive(Serialize, Clone, Copy)]
struct Key {
    session_id: u64,
    epoch: u64,
    request_id: u64,
}
impl From<ProgramRequestKeyV1> for Key {
    fn from(k: ProgramRequestKeyV1) -> Self {
        Self {
            session_id: k.session_id,
            epoch: k.epoch,
            request_id: k.request_id,
        }
    }
}
#[derive(Serialize, Default)]
struct Stats {
    transitions: u64,
    primitive_steps: u64,
    peak_live_payload_bytes: usize,
    peak_metadata_bytes: usize,
}
impl From<ProgramExecutionStatsV1> for Stats {
    fn from(s: ProgramExecutionStatsV1) -> Self {
        Self {
            transitions: s.transitions,
            primitive_steps: s.primitive_steps,
            peak_live_payload_bytes: s.peak_live_payload_bytes,
            peak_metadata_bytes: s.peak_metadata_bytes,
        }
    }
}
struct HexDigest([u8; 32]);
fn hex(bytes: &[u8], out: &mut [u8]) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    for (i, b) in bytes.iter().enumerate() {
        out[2 * i] = DIGITS[(b >> 4) as usize];
        out[2 * i + 1] = DIGITS[(b & 15) as usize];
    }
}
impl Serialize for HexDigest {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut b = [0; 64];
        hex(&self.0, &mut b);
        s.serialize_str(std::str::from_utf8(&b).unwrap())
    }
}
struct Word(f64);
impl Serialize for Word {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut b = [0; 16];
        hex(&self.0.to_bits().to_be_bytes(), &mut b);
        s.serialize_str(std::str::from_utf8(&b).unwrap())
    }
}
struct Value<'a>(&'a ProgramValueV1);
impl Serialize for Value<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_struct("Value", 4)?;
        out.serialize_field(
            "class",
            if self.0.class() == ProgramValueClassV1::Double {
                "double"
            } else {
                "logical"
            },
        )?;
        out.serialize_field("shape", &self.0.shape())?;
        out.serialize_field("storage_order", "column-major")?;
        if let Some(words) = self.0.doubles() {
            struct Words<'a>(&'a [f64]);
            impl Serialize for Words<'_> {
                fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                    s.collect_seq(self.0.iter().map(|v| Word(*v)))
                }
            }
            out.serialize_field("binary64_words", &Words(words))?;
        } else {
            struct Bools<'a>(&'a [u8]);
            impl Serialize for Bools<'_> {
                fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                    s.collect_seq(self.0.iter().map(|v| *v != 0))
                }
            }
            out.serialize_field("logical_values", &Bools(self.0.logicals().unwrap()))?;
        }
        out.end()
    }
}
#[derive(Serialize)]
struct Entry<'a> {
    name: &'a str,
    value: Value<'a>,
}
struct Workspace<'a>(Option<&'a ProgramSessionV1>);
impl Serialize for Workspace<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_seq(Some(self.0.map(|v| v.bindings().len()).unwrap_or(0)))?;
        if let Some(session) = self.0 {
            for entry in session.bindings() {
                out.serialize_element(&Entry {
                    name: entry.name,
                    value: Value(entry.value),
                })?;
            }
        }
        out.end()
    }
}
struct Events<'a>(&'a [ProgramEventV1]);
impl Serialize for Events<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_seq(Some(self.0.len()))?;
        #[derive(Serialize)]
        struct Event<'a> {
            sequence: u64,
            location: Location,
            function: Option<usize>,
            name: Option<&'a str>,
            value: Value<'a>,
        }
        for e in self.0 {
            out.serialize_element(&Event {
                sequence: e.sequence,
                location: e.location.into(),
                function: e.function.map(NodeIdV1::index),
                name: e.name.as_deref(),
                value: Value(&e.value),
            })?;
        }
        out.end()
    }
}
fn code(c: ProgramErrorCodeV1) -> &'static str {
    if c.is_allocation() {
        return "Allocation";
    }
    match c {
        ProgramErrorCodeV1::UnknownBinding => "UnknownBinding",
        ProgramErrorCodeV1::Arity => "Arity",
        ProgramErrorCodeV1::UnassignedOutput => "UnassignedOutput",
        ProgramErrorCodeV1::Type => "Type",
        ProgramErrorCodeV1::Shape => "Shape",
        ProgramErrorCodeV1::Index => "Index",
        ProgramErrorCodeV1::UnsupportedFeature => "UnsupportedFeature",
        ProgramErrorCodeV1::Allocation => "Allocation",
        ProgramErrorCodeV1::Resource => "Resource",
        ProgramErrorCodeV1::CounterExhausted => "CounterExhausted",
        ProgramErrorCodeV1::Cancelled => "Cancelled",
        ProgramErrorCodeV1::Value(_) => "Numeric",
    }
}
fn parse_code(c: ProgramParseErrorCodeV1) -> &'static str {
    match c {
        ProgramParseErrorCodeV1::Syntax => "Syntax",
        ProgramParseErrorCodeV1::UnsupportedFeature => "UnsupportedFeature",
        ProgramParseErrorCodeV1::InvalidAssignment => "InvalidAssignment",
        ProgramParseErrorCodeV1::InvalidControlFlow => "InvalidControlFlow",
        ProgramParseErrorCodeV1::DuplicateName => "DuplicateName",
        ProgramParseErrorCodeV1::ResourceLimit => "ResourceLimit",
        ProgramParseErrorCodeV1::AllocationFailure => "Allocation",
        ProgramParseErrorCodeV1::Cancelled => "Cancelled",
    }
}
fn reason(r: ProgramCancelReasonV1) -> &'static str {
    match r {
        ProgramCancelReasonV1::UserStop => "UserStop",
        ProgramCancelReasonV1::Deadline => "Deadline",
    }
}
struct Frames<'a>(Option<&'a ProgramErrorV1>);
impl Serialize for Frames<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_seq(Some(self.0.map(|e| e.call_depth).unwrap_or(0)))?;
        if let Some(e) = self.0 {
            for f in e.call_stack[..e.call_depth].iter().flatten() {
                out.serialize_element(&Location::from(*f))?;
            }
        }
        out.end()
    }
}
struct Diagnostic<'a> {
    early: Option<Early>,
    runtime: Option<&'a ProgramErrorV1>,
    cancelled: bool,
}
impl Serialize for Diagnostic<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_struct("Error", 5)?;
        let stage = self.early.map(|e| e.phase).unwrap_or("execution");
        let name = if self.cancelled {
            "Cancelled"
        } else if let Some(e) = self.early {
            e.code
        } else {
            code(self.runtime.unwrap().code)
        };
        let location = self
            .early
            .and_then(|e| e.span)
            .map(|span| Location {
                source_id: 1,
                span: span.into(),
            })
            .or_else(|| self.runtime.map(|e| e.location.into()));
        let cause = self.runtime.and_then(|e| match e.code {
            ProgramErrorCodeV1::Value(ProgramValueErrorV1::Array(a)) => Some(a.code.as_str()),
            ProgramErrorCodeV1::Value(ProgramValueErrorV1::Multiply(_)) => Some("Multiply"),
            ProgramErrorCodeV1::Value(ProgramValueErrorV1::Shape) => Some("Shape"),
            ProgramErrorCodeV1::Value(ProgramValueErrorV1::Type) => Some("Type"),
            ProgramErrorCodeV1::Value(ProgramValueErrorV1::UnsupportedFeature) => {
                Some("UnsupportedFeature")
            }
            ProgramErrorCodeV1::Value(ProgramValueErrorV1::Allocation) => Some("Allocation"),
            ProgramErrorCodeV1::Value(ProgramValueErrorV1::Cancelled) => Some("Cancelled"),
            _ => None,
        });
        out.serialize_field("stage", stage)?;
        out.serialize_field("code", name)?;
        out.serialize_field("location", &location)?;
        out.serialize_field("call_stack", &Frames(self.runtime))?;
        out.serialize_field("cause_code", &cause)?;
        out.end()
    }
}
struct Report<'a> {
    source: Option<&'a Source>,
    outcome: Option<&'a ProgramOutcomeV1>,
    early: Option<Early>,
    session: Option<&'a ProgramSessionV1>,
    workspace: bool,
    complete: bool,
}
impl Report<'_> {
    fn status(&self) -> &'static str {
        if let Some(e) = self.early {
            return e.status;
        }
        match self.outcome.unwrap().status {
            ProgramStatusV1::Success => "success",
            ProgramStatusV1::Error => "error",
            ProgramStatusV1::Resource => "resource",
            ProgramStatusV1::Cancelled(_) => "cancelled",
        }
    }
    fn reason(&self) -> Option<ProgramCancelReasonV1> {
        self.early.and_then(|e| e.reason).or_else(|| {
            self.outcome.and_then(|o| match o.status {
                ProgramStatusV1::Cancelled(r) => Some(r),
                _ => None,
            })
        })
    }
    fn exit(&self) -> i32 {
        match self.reason() {
            Some(ProgramCancelReasonV1::UserStop) => 130,
            Some(ProgramCancelReasonV1::Deadline) => 124,
            None if self.status() == "success" => 0,
            _ => 1,
        }
    }
}
impl Serialize for Report<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_struct("Outcome", 19)?;
        out.serialize_field("schema_version", &1)?;
        out.serialize_field("kind", KIND)?;
        out.serialize_field("backend", "rust")?;
        out.serialize_field("profile", PROGRAM_PROFILE_V1)?;
        out.serialize_field("event", "outcome")?;
        out.serialize_field("status", self.status())?;
        out.serialize_field("phase", self.early.map(|e| e.phase).unwrap_or("execution"))?;
        out.serialize_field("source", &self.source)?;
        out.serialize_field("key", &self.outcome.map(|o| Key::from(o.key)))?;
        out.serialize_field("before", &self.outcome.map(|o| Context::from(o.start)))?;
        out.serialize_field(
            "after",
            &self.outcome.map(|o| Context::from(o.final_context)),
        )?;
        out.serialize_field(
            "effect_sequence",
            &self
                .outcome
                .map(|o| o.committed_effect_sequence)
                .unwrap_or(0),
        )?;
        out.serialize_field(
            "completed_leaf_statement_count",
            &self
                .outcome
                .map(|o| o.completed_leaf_statement_count)
                .unwrap_or(0),
        )?;
        out.serialize_field(
            "stats",
            &self
                .outcome
                .map(|o| Stats::from(o.stats))
                .unwrap_or_default(),
        )?;
        out.serialize_field("reason", &self.reason().map(reason))?;
        out.serialize_field("events_complete", &self.complete)?;
        out.serialize_field(
            "events",
            &Events(if self.complete {
                self.outcome.map(|o| o.events.as_slice()).unwrap_or(&[])
            } else {
                &[]
            }),
        )?;
        out.serialize_field(
            "workspace",
            &if self.workspace && self.complete {
                Some(Workspace(self.session))
            } else {
                None
            },
        )?;
        let runtime = self.outcome.and_then(|o| o.error.as_ref());
        let err = if self.early.is_some() || runtime.is_some() || self.reason().is_some() {
            Some(Diagnostic {
                early: self.early,
                runtime,
                cancelled: self.status() == "cancelled",
            })
        } else {
            None
        };
        out.serialize_field("error", &err)?;
        out.end()
    }
}
struct Reply {
    bytes: Vec<u8>,
    limit: usize,
}
impl Reply {
    fn new(limit: usize) -> io::Result<Self> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(limit)
            .map_err(|_| io::Error::from(io::ErrorKind::OutOfMemory))?;
        if bytes.capacity() > limit {
            return Err(io::Error::from(io::ErrorKind::OutOfMemory));
        }
        Ok(Self { bytes, limit })
    }
    fn json(&mut self, v: &impl Serialize) -> io::Result<()> {
        self.bytes.clear();
        serde_json::to_writer(&mut *self, v)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        self.write_all(b"\n")
    }
}
impl Write for Reply {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.limit)
        {
            return Err(io::Error::from(io::ErrorKind::OutOfMemory));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn emit(reply: &Reply) -> io::Result<()> {
    let mut out = io::stdout().lock();
    out.write_all(&reply.bytes)?;
    out.flush()
}
fn label(path: &Path) -> Result<String, Early> {
    #[cfg(unix)]
    use std::os::unix::ffi::OsStrExt;
    #[cfg(unix)]
    let mut bytes = path.as_os_str().as_bytes();
    #[cfg(not(unix))]
    let mut bytes = path
        .to_str()
        .ok_or(Early::error("source", "PathEncoding"))?
        .as_bytes();
    if bytes.len() > 4096 {
        return Err(Early::resource("source", "PathLimit"));
    }
    let mut out = String::new();
    out.try_reserve_exact(4096)
        .map_err(|_| Early::resource("admission", "Allocation"))?;
    if out.capacity() > 4096 {
        return Err(Early::resource("admission", "Allocation"));
    }
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                if out.len() + text.len() > 4096 {
                    return Err(Early::resource("source", "PathLimit"));
                }
                out.push_str(text);
                break;
            }
            Err(e) => {
                let valid = e.valid_up_to();
                let text = std::str::from_utf8(&bytes[..valid]).unwrap();
                if out.len() + text.len() + 3 > 4096 {
                    return Err(Early::resource("source", "PathLimit"));
                }
                out.push_str(text);
                out.push('\u{fffd}');
                bytes = &bytes[valid + e.error_len().unwrap_or(bytes.len() - valid)..];
            }
        }
    }
    Ok(out)
}
fn open(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC);
    }
    options.open(path)
}
fn invalid_utf8_span(bytes: &[u8], error: std::str::Utf8Error) -> ProgramSpanV1 {
    let start = error.valid_up_to();
    let mut line = 1;
    let mut i = 0;
    while i < start {
        if bytes[i] == b'\r' {
            line += 1;
            i += 1;
            if bytes.get(i) == Some(&b'\n') {
                i += 1
            }
        } else {
            if bytes[i] == b'\n' {
                line += 1
            }
            i += 1;
        }
    }
    ProgramSpanV1 {
        start,
        end: (start + error.error_len().unwrap_or(bytes.len() - start)).min(bytes.len()),
        start_line: line,
        end_line: line,
    }
}
fn load(path: &Path) -> Result<(Source, Arc<str>), Failure> {
    let path_display = label(path).map_err(|early| Failure {
        early,
        source: None,
    })?;
    let mut file = open(path).map_err(|_| Failure {
        early: Early::error("source", "SourceIo"),
        source: None,
    })?;
    let metadata = file.metadata().map_err(|_| Failure {
        early: Early::error("source", "SourceIo"),
        source: None,
    })?;
    if !metadata.is_file() {
        return Err(Failure {
            early: Early::error("source", "NotRegularFile"),
            source: None,
        });
    }
    if metadata.len() > 65536 {
        return Err(Failure {
            early: Early::resource("source", "SourceLimit"),
            source: None,
        });
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(65537).map_err(|_| Failure {
        early: Early::resource("admission", "Allocation"),
        source: None,
    })?;
    if bytes.capacity() > 65537 {
        return Err(Failure {
            early: Early::resource("admission", "Allocation"),
            source: None,
        });
    }
    let mut chunk = [0u8; 8192];
    loop {
        if signal::requested() {
            return Err(Failure {
                early: Early::cancelled("source", ProgramCancelReasonV1::UserStop),
                source: None,
            });
        }
        let room = (65537 - bytes.len()).min(chunk.len());
        if room == 0 {
            break;
        }
        let read = match file.read(&mut chunk[..room]) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                return Err(Failure {
                    early: Early::error("source", "SourceIo"),
                    source: None,
                });
            }
        };
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    if bytes.len() > 65536 {
        return Err(Failure {
            early: Early::resource("source", "SourceLimit"),
            source: None,
        });
    }
    let source = Source {
        id: 1,
        path_display,
        sha256: HexDigest(Sha256::digest(&bytes).into()),
        bytes: bytes.len(),
    };
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(error) => {
            return Err(Failure {
                early: Early {
                    span: Some(invalid_utf8_span(&bytes, error)),
                    ..Early::error("source", "InvalidUtf8")
                },
                source: Some(source),
            });
        }
    };
    Ok((source, Arc::from(text)))
}
#[derive(Serialize)]
struct Limits {
    transition_fuel: u64,
    primitive_fuel: u64,
}
#[derive(Serialize)]
struct Ready<'a> {
    schema_version: u8,
    kind: &'static str,
    backend: &'static str,
    profile: &'static str,
    event: &'static str,
    source: &'a Source,
    key: Key,
    context: Context,
    limits: Limits,
}
fn text_value(out: &mut Reply, name: Option<&str>, value: &ProgramValueV1) -> io::Result<()> {
    let [rows, cols] = value.shape();
    writeln!(
        out,
        "{} = ({} {rows}x{cols})",
        name.unwrap_or("ans"),
        if value.class() == ProgramValueClassV1::Double {
            "double"
        } else {
            "logical"
        }
    )?;
    if rows == 0 || cols == 0 {
        return writeln!(out, "[]");
    }
    for row in 0..rows as usize {
        for col in 0..cols as usize {
            let index = row + col * rows as usize;
            if let Some(words) = value.doubles() {
                write!(out, "{:.16e}", words[index])?;
            } else {
                write!(
                    out,
                    "{}",
                    if value.logicals().unwrap()[index] != 0 {
                        "true"
                    } else {
                        "false"
                    }
                )?;
            }
            if col + 1 < cols as usize {
                out.write_all(b" ")?;
            }
        }
        out.write_all(b"\n")?;
    }
    Ok(())
}
fn text(out: &mut Reply, r: &Report<'_>) -> io::Result<()> {
    out.bytes.clear();
    if let Some(result) = r.outcome {
        if r.complete {
            for e in &result.events {
                text_value(out, e.name.as_deref(), &e.value)?;
            }
        }
    }
    if r.workspace && r.complete {
        writeln!(out, "Workspace:")?;
        if let Some(session) = r.session {
            for entry in session.bindings() {
                text_value(out, Some(entry.name), entry.value)?;
            }
        }
    }
    if r.status() == "success" {
        writeln!(
            out,
            "Completed Rust program: {} effects, {} completed statements.",
            r.outcome.map(|o| o.committed_effect_sequence).unwrap_or(0),
            r.outcome
                .map(|o| o.completed_leaf_statement_count)
                .unwrap_or(0)
        )?;
    } else {
        let name = r
            .early
            .map(|e| e.code)
            .or_else(|| r.outcome.and_then(|o| o.error.map(|e| code(e.code))))
            .unwrap_or("Cancelled");
        writeln!(
            out,
            "Rust program {}: {name}{}",
            r.status(),
            if r.complete {
                ""
            } else {
                " (output incomplete)"
            }
        )?;
        if let Some(reason) = r.reason() {
            writeln!(out, "Reason: {}", self::reason(reason))?;
        }
        let location = r
            .early
            .and_then(|e| e.span)
            .map(|span| ProgramLocationV1 { source_id: 1, span })
            .or_else(|| r.outcome.and_then(|o| o.error.map(|e| e.location)));
        if let Some(l) = location {
            writeln!(
                out,
                "source {} line {}, bytes {}..{}",
                l.source_id, l.span.start_line, l.span.start, l.span.end
            )?;
        }
        if let Some(error) = r.outcome.and_then(|o| o.error.as_ref()) {
            for frame in error.call_stack[..error.call_depth].iter().flatten() {
                writeln!(
                    out,
                    "  called from source {} line {}, bytes {}..{}",
                    frame.source_id, frame.span.start_line, frame.span.start, frame.span.end
                )?;
            }
        }
    }
    Ok(())
}
fn encode(reply: &mut Reply, json: bool, report: Report<'_>) -> io::Result<i32> {
    let exit = report.exit();
    let encoded = if json {
        reply.json(&report)
    } else {
        text(reply, &report)
    };
    if encoded.is_err() {
        let fallback = Report {
            early: Some(Early::resource("encoding", "ReplyLimit")),
            complete: false,
            ..report
        };
        if json {
            reply.json(&fallback)?
        } else {
            text(reply, &fallback)?
        }
        return Ok(1);
    }
    Ok(exit)
}
fn finish(reply: &mut Reply, json: bool, report: Report<'_>) -> i32 {
    match encode(reply, json, report) {
        Ok(exit) => {
            if emit(reply).is_ok() {
                exit
            } else {
                1
            }
        }
        Err(_) => 1,
    }
}

fn early(
    reply: &mut Reply,
    json: bool,
    workspace: bool,
    source: Option<&Source>,
    failure: Early,
) -> i32 {
    finish(
        reply,
        json,
        Report {
            source,
            outcome: None,
            early: Some(failure),
            session: None,
            workspace,
            complete: true,
        },
    )
}
pub(super) fn run(
    path: &Path,
    json: bool,
    workspace: bool,
    timeout: Option<Duration>,
    transition_fuel: Option<u64>,
    primitive_fuel: Option<u64>,
) -> i32 {
    let mut reply = match Reply::new(REPLY_LIMIT) {
        Ok(reply) => reply,
        Err(_) => {
            let message = if json {
                "{\"schema_version\":1,\"kind\":\"mathlab-rust-program\",\"backend\":\"rust\",\"profile\":\"rust-program-execution-v1\",\"event\":\"outcome\",\"status\":\"resource\",\"phase\":\"admission\",\"source\":null,\"key\":null,\"before\":null,\"after\":null,\"effect_sequence\":0,\"completed_leaf_statement_count\":0,\"stats\":{\"transitions\":0,\"primitive_steps\":0,\"peak_live_payload_bytes\":0,\"peak_metadata_bytes\":0},\"reason\":null,\"events_complete\":true,\"events\":[],\"workspace\":null,\"error\":{\"stage\":\"admission\",\"code\":\"Allocation\",\"location\":null,\"call_stack\":[],\"cause_code\":null}}\n"
            } else {
                "Rust program resource: Allocation\n"
            };
            let _ = io::stdout().lock().write_all(message.as_bytes());
            return 1;
        }
    };
    let _signal = match signal::SignalGuard::install() {
        Ok(guard) => guard,
        Err(_) => {
            return early(
                &mut reply,
                json,
                workspace,
                None,
                Early::error(
                    "admission",
                    if cfg!(unix) {
                        "SignalSetup"
                    } else {
                        "UnsupportedPlatform"
                    },
                ),
            );
        }
    };
    let PreparedSource { source, module, deadline } = match prepare_file(path, timeout) {
        Ok(value) => value,
        Err(failure) => return early(&mut reply,json,workspace,failure.source.as_ref(),failure.early),
    };
    let mut session = match ProgramSessionV1::new() {
        Ok(session) => session,
        Err(error) => {
            return early(
                &mut reply,
                json,
                workspace,
                Some(&source),
                Early::resource("admission", code(error)),
            );
        }
    };
    let context = session.context();
    let options = ProgramExecutionOptionsV1 {
        deadline,
        transition_fuel: transition_fuel.unwrap_or(1_000_000),
        primitive_fuel: primitive_fuel.unwrap_or(100_000_000),
    };
    let request = match session.prepare(module, options) {
        Ok(request) => request,
        Err(error) => {
            return early(
                &mut reply,
                json,
                workspace,
                Some(&source),
                Early::resource("admission", code(error)),
            );
        }
    };
    let handle = request.cancel_handle();
    let ready = if json {
        reply.json(&Ready {
            schema_version: 1,
            kind: KIND,
            backend: "rust",
            profile: PROGRAM_PROFILE_V1,
            event: "ready",
            source: &source,
            key: handle.key().into(),
            context: context.into(),
            limits: Limits {
                transition_fuel: options.transition_fuel,
                primitive_fuel: options.primitive_fuel,
            },
        })
    } else {
        reply.bytes.clear();
        writeln!(
            &mut reply,
            "Running {:?} through {PROGRAM_PROFILE_V1}; Ctrl+C requests Stop.",
            source.path_display
        )
    };
    if ready.is_err() {
        drop(request);
        return early(
            &mut reply,
            json,
            workspace,
            Some(&source),
            Early::resource("encoding", "ReplyLimit"),
        );
    }
    if emit(&reply).is_err() {
        return 1;
    }
    reply.limit -= reply.bytes.len();
    reply.bytes.clear();
    let outcome = request.execute_controlled(|_, handle| {
        if signal::requested() {
            handle.request_stop();
        }
    });
    finish(
        &mut reply,
        json,
        Report {
            source: Some(&source),
            outcome: Some(&outcome),
            early: None,
            session: Some(&session),
            workspace,
            complete: true,
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capped_writer_preflights_and_preserves_encoding_failure_context() {
        let mut reply = Reply::new(1024).unwrap();
        let value = ProgramValueV1::try_double([16, 16], &[1.; 256]).unwrap();
        let loc = ProgramLocationV1 {
            source_id: 1,
            span: ProgramSpanV1 {
                start: 0,
                end: 1,
                start_line: 1,
                end_line: 1,
            },
        };
        let context = ProgramContextV1 {
            session_id: 1,
            epoch: 1,
            revision: 1,
        };
        let outcome = ProgramOutcomeV1 {
            profile: PROGRAM_PROFILE_V1,
            key: ProgramRequestKeyV1 {
                session_id: 1,
                epoch: 1,
                request_id: 1,
            },
            start: context,
            final_context: context,
            committed_effect_sequence: 3,
            completed_leaf_statement_count: 2,
            stats: Default::default(),
            events: vec![ProgramEventV1 {
                sequence: 3,
                location: loc,
                function: None,
                name: Some("x".into()),
                value,
            }],
            status: ProgramStatusV1::Success,
            error: None,
        };
        let report = Report {
            source: None,
            outcome: Some(&outcome),
            early: None,
            session: None,
            workspace: false,
            complete: true,
        };
        assert_eq!(encode(&mut reply, true, report).unwrap(), 1);
        let record: serde_json::Value = serde_json::from_slice(&reply.bytes).unwrap();
        assert_eq!(record["status"], "resource");
        assert_eq!(record["error"]["code"], "ReplyLimit");
        assert_eq!(record["events_complete"], false);
        assert_eq!(record["effect_sequence"], 3);
        assert_eq!(record["completed_leaf_statement_count"], 2);
        assert_eq!(record["after"]["revision"], 1);
        assert!(record["events"].as_array().unwrap().is_empty());
        assert!(reply.bytes.len() <= 1024);
        let mut reply = Reply::new(4).unwrap();
        assert_eq!(reply.write(b"1234").unwrap(), 4);
        assert!(reply.write(b"5").is_err());
        assert_eq!(reply.bytes, b"1234");
        assert!(Reply::new(usize::MAX).is_err());
    }
    #[test]
    fn exact_value_words_and_bounded_scientific_notation() {
        let value = ProgramValueV1::try_double(
            [1, 5],
            &[f64::MAX, f64::from_bits(1), -0.0, f64::INFINITY, f64::NAN],
        )
        .unwrap();
        let mut reply = Reply::new(4096).unwrap();
        reply.json(&Value(&value)).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&reply.bytes).unwrap();
        assert_eq!(parsed["binary64_words"][0], "7fefffffffffffff");
        assert_eq!(parsed["binary64_words"][1], "0000000000000001");
        assert_eq!(parsed["binary64_words"][2], "8000000000000000");
        assert_eq!(parsed["binary64_words"][3], "7ff0000000000000");
        reply.bytes.clear();
        text_value(&mut reply, Some("x"), &value).unwrap();
        let text = std::str::from_utf8(&reply.bytes).unwrap();
        for token in text.lines().nth(1).unwrap().split_whitespace() {
            assert!(token.len() <= 26);
        }
        let logical = ProgramValueV1::try_logical([1, 2], &[0, 1]).unwrap();
        reply.json(&Value(&logical)).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&reply.bytes).unwrap();
        assert_eq!(parsed["class"], "logical");
        assert_eq!(parsed["logical_values"], serde_json::json!([false, true]));
        assert!(parsed.get("binary64_words").is_none());
    }
    #[test]
    fn display_path_bounds_and_engine_views_are_borrowed() {
        assert!(label(Path::new(&"a".repeat(4096))).is_ok());
        assert!(label(Path::new(&"a".repeat(4097))).is_err());
        let module = Arc::new(parse_program_v1(1, Arc::from("x=1;mask=1<2;")).unwrap());
        let mut session = ProgramSessionV1::new().unwrap();
        let result = session
            .prepare(module, Default::default())
            .unwrap()
            .execute();
        assert_eq!(result.status, ProgramStatusV1::Success);
        let view = session.bindings().collect::<Vec<_>>();
        assert_eq!(view.len(), 2);
        assert_eq!(view[0].name, "x");
        assert_eq!(view[1].value.class(), ProgramValueClassV1::Logical);
        assert_eq!(view[0].value.scalar().unwrap(), 1.);
    }
}

// One immutable input/UTF8/hash/located-AST preparation pipeline for both routes.
struct PreparedSource {source:Source,module:Arc<ProgramModuleV1>,deadline:Option<Instant>}
fn prepare_file(path:&Path,timeout:Option<Duration>)->Result<PreparedSource,Failure>{
    let(source,text)=load(path)?;
    let deadline=match timeout.map(|d|Instant::now().checked_add(d)){Some(None)=>return Err(Failure{source:Some(source),early:Early::error("admission","InvalidDeadline")}),Some(Some(d))=>Some(d),None=>None};
    let mut cancel=None;
    let module=parse_program_controlled_v1(1,text,||{if cancel.is_none(){if signal::requested(){cancel=Some(ProgramCancelReasonV1::UserStop)}else if deadline.is_some_and(|d|Instant::now()>=d){cancel=Some(ProgramCancelReasonV1::Deadline)}}cancel.is_some()});
    let module=match module{Ok(module)=>Arc::new(module),Err(error)=>{
        let early=if let Some(reason)=cancel{Early{span:Some(error.span),..Early::cancelled("parse",reason)}}else{Early{span:Some(error.span),..if matches!(error.code,ProgramParseErrorCodeV1::ResourceLimit|ProgramParseErrorCodeV1::AllocationFailure){Early::resource("parse",parse_code(error.code))}else{Early::error("parse",parse_code(error.code))}}};
        return Err(Failure{source:Some(source),early});
    }};
    Ok(PreparedSource{source,module,deadline})
}
