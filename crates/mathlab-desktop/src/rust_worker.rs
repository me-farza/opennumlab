//! Native class-v1 adapter for the existing desktop actor. No session crosses threads.
use mathlab_engine::Variable;
use mathlab_engine::rust_program::{ProgramCancelHandleV1, ProgramCancelReasonV1, ProgramStatusV1};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

pub const MAX_RESPONSE_BYTES: usize = 1 << 20;
pub const MAX_PAGE_CELLS: usize = 512;
pub const MAX_EVENT_PREVIEWS: usize = 256;
pub const MAX_SOURCE_BYTES: usize = 65536;
pub const MAX_GRAPH_BYTES: usize = 256 << 10;
pub const MAX_SOURCES: usize = 9;
pub const MAX_PATH_BYTES: usize = 4096;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    #[default]
    Octave,
    RustClassV1,
}
impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Self::Octave => "GNU Octave",
            Self::RustClassV1 => "Rust class-v1",
        }
    }
    pub fn is_rust(self) -> bool {
        self == Self::RustClassV1
    }
}

/// A UI generation changes on reset/backend switch; a job never repeats in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobKey {
    pub generation: u64,
    pub job: u64,
}
/// Request-specific, never reused. Setting after completion cannot change the engine outcome.
#[derive(Debug, Clone, Default)]
pub struct AdmissionStop(Arc<AtomicBool>);
impl AdmissionStop {
    pub fn request(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn requested(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
#[derive(Debug)]
pub struct RunRequest {
    lease: Option<Lease>,
    key: JobKey,
    document_id: usize,
    entry_path: PathBuf,
    entry_text: String,
    /// Explicit ordered modules; no ambient directory/class lookup.
    modules: Vec<PathBuf>,
    pub stop: AdmissionStop,
    #[cfg(test)]
    parse_probe: Option<Arc<ParseProbe>>,
}
#[derive(Debug, Clone)]
pub struct SourceSnapshot {
    pub source_id: u64,
    pub document_id: Option<usize>,
    pub path: PathBuf,
    pub text: Arc<str>,
    pub sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageToken {
    pub key: JobKey,
    pub revision: u64,
    pub binding: String,
}
#[derive(Debug)]
pub struct PageRequest {
    lease: Option<Lease>,
    request_id: u64,
    token: PageToken,
    /// Zero-based logical row/column plus exact higher-axis coordinates.
    row: usize,
    column: usize,
    higher: Vec<u64>,
    rows: usize,
    columns: usize,
}
#[derive(Debug, Clone)]
pub struct Cell {
    pub text: String,
    /// CSC implicit zeros are distinct from stored entries. Dense cells are always stored.
    pub stored: bool,
}
#[derive(Debug)]
pub struct Page {
    pub token: PageToken,
    pub dimensions: Vec<usize>,
    pub row: usize,
    pub column: usize,
    pub higher: Vec<u64>,
    pub rows: usize,
    pub columns: usize,
    pub class_name: String,
    pub storage: &'static str,
    pub complex: bool,
    pub cells: Vec<Vec<Cell>>,
    /// Object metadata only, never an object token/root or numeric surrogate.
    pub object: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Success,
    Error,
    Resource,
    Cancelled,
}
#[derive(Debug)]
pub struct LocatedError {
    pub source_id: u64,
    pub line: usize,
    pub column: usize,
    pub message: String,
}
#[derive(Debug)]
pub struct Finished {
    pub(crate) lease: Option<Lease>,
    pub execution_started: bool,
    pub key: JobKey,
    pub revision: u64,
    pub status: Status,
    pub reason: Option<ProgramCancelReasonV1>,
    pub error: Option<LocatedError>,
    pub completed_statements: u64,
    pub effect_sequence: u64,
    pub output: String,
    pub event_count: usize,
    pub events_complete: bool,
    pub workspace: Vec<Variable>,
    pub workspace_complete: bool,
    pub sources: Vec<SourceSnapshot>,
    /// None only if admission failed before the complete immutable closure was captured.
    pub graph_sha256: Option<String>,
    pub duration_ms: u64,
    /// UI/codec omission never changes status, reason or committed prefix.
    pub preview_omitted: bool,
}
#[derive(Debug)]
pub enum NativeEvent {
    Ready {
        generation: u64,
        lease: Lease,
    },
    Prepared {
        key: JobKey,
        cancel: ProgramCancelHandleV1,
    },
    Finished(Finished),
    Page {
        request_id: u64,
        token: PageToken,
        result: Result<Page, String>,
        lease: Option<Lease>,
    },
    StartupFailed {
        generation: u64,
        message: String,
        lease: Option<Lease>,
    },
}

/// Native outstanding ownership is bounded even though the shared legacy channel is unbounded.
/// A lease lasts through queued completion, not just actor computation. Acquire before cloning inputs.
#[derive(Debug, Clone, Default)]
pub struct NativeGate {
    run: Arc<AtomicBool>,
    read: Arc<AtomicBool>,
    bytes: Arc<AtomicUsize>,
    history: Arc<AtomicUsize>,
}
#[derive(Debug)]
pub struct Lease {
    slot: Arc<AtomicBool>,
    _bytes: DisplayLease,
}
#[derive(Debug)]
pub struct DisplayLease {
    used: Arc<AtomicUsize>,
    history: Option<Arc<AtomicUsize>>,
    bytes: usize,
}
impl Drop for DisplayLease {
    fn drop(&mut self) {
        self.used.fetch_sub(self.bytes, Ordering::AcqRel);
        if let Some(history) = &self.history {
            history.fetch_sub(self.bytes, Ordering::AcqRel);
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.slot.store(false, Ordering::Release);
    }
}
impl NativeGate {
    pub fn start(&self) -> Result<Lease, String> {
        self.claim(&self.run)
    }
    pub fn reserved_bytes(&self) -> usize {
        self.bytes.load(Ordering::Acquire)
    }
    pub fn reserve_display(&self, bytes: usize) -> Result<DisplayLease, String> {
        Self::add(&self.history, bytes, 4 << 20)?;
        match self.reserve_pool(bytes) {
            Ok(mut lease) => {
                lease.history = Some(Arc::clone(&self.history));
                Ok(lease)
            }
            Err(e) => {
                self.history.fetch_sub(bytes, Ordering::AcqRel);
                Err(e)
            }
        }
    }
    fn add(pool: &AtomicUsize, bytes: usize, limit: usize) -> Result<(), String> {
        let mut old = pool.load(Ordering::Acquire);
        loop {
            let new = old
                .checked_add(bytes)
                .filter(|&n| n <= limit)
                .ok_or("Native display history is full; preview omitted.")?;
            match pool.compare_exchange_weak(old, new, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return Ok(()),
                Err(now) => old = now,
            }
        }
    }
    fn reserve_pool(&self, bytes: usize) -> Result<DisplayLease, String> {
        Self::add(&self.bytes, bytes, 8 << 20)?;
        Ok(DisplayLease {
            used: Arc::clone(&self.bytes),
            history: None,
            bytes,
        })
    }
    fn claim(&self, slot: &Arc<AtomicBool>) -> Result<Lease, String> {
        slot.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "A native request is already outstanding.".to_owned())?;
        match self.reserve_pool(2 * MAX_RESPONSE_BYTES) {
            Ok(bytes) => Ok(Lease {
                slot: Arc::clone(slot),
                _bytes: bytes,
            }),
            Err(e) => {
                slot.store(false, Ordering::Release);
                Err(e)
            }
        }
    }
}
impl RunRequest {
    pub fn new(
        gate: &NativeGate,
        key: JobKey,
        document_id: usize,
        path: &std::path::Path,
        text: &str,
        modules: &[PathBuf],
    ) -> Result<Self, String> {
        if text.len() > MAX_SOURCE_BYTES
            || modules.len() >= MAX_SOURCES
            || path.as_os_str().len() > MAX_PATH_BYTES
            || modules.iter().any(|p| p.as_os_str().len() > MAX_PATH_BYTES)
        {
            return Err("Native source/path bounds exceeded.".into());
        }
        let lease = gate.claim(&gate.run)?;
        Ok(Self {
            lease: Some(lease),
            key,
            document_id,
            entry_path: path.to_owned(),
            entry_text: text.to_owned(),
            modules: modules.to_vec(),
            stop: AdmissionStop::default(),
            #[cfg(test)]
            parse_probe: None,
        })
    }
}
impl PageRequest {
    pub fn new(
        gate: &NativeGate,
        request_id: u64,
        token: &PageToken,
        row: usize,
        column: usize,
        higher: &[u64],
        rows: usize,
        columns: usize,
    ) -> Result<Self, String> {
        if rows > MAX_PAGE_CELLS
            || columns > MAX_PAGE_CELLS
            || rows.checked_mul(columns).is_none_or(|n| n > MAX_PAGE_CELLS)
            || higher.len() > 6
            || token.binding.len() > MAX_SOURCE_BYTES
        {
            return Err("Native page bounds exceeded.".into());
        }
        let lease = gate.claim(&gate.read)?;
        Ok(Self {
            lease: Some(lease),
            request_id,
            token: token.clone(),
            row,
            column,
            higher: higher.to_vec(),
            rows,
            columns,
        })
    }
}

use mathlab_engine::rust_program::class::{
    ClassEventValueV1, ClassProgramOptionsV1, ClassProgramSessionV1, ClassValueViewV1,
};
use mathlab_language::program::{
    ClassParseErrorCodeV1, ClassSourceV1, Complex64V1, ProgramValueClassV1, ScientificCscValuesV1,
    ScientificProgramValueV2, ScientificStorageViewV2, parse_class_graph_controlled_v1,
};
use sha2::{Digest, Sha256};
use std::{fs::OpenOptions, io::Read, path::Path, time::Instant};

/// Worker-local mutable state. Deliberately not Send; only owned DTOs leave this actor.
pub struct NativeSession {
    generation: u64,
    last_job: u64,
    session: ClassProgramSessionV1,
}
impl NativeSession {
    pub fn new(generation: u64) -> Result<Self, String> {
        if generation == 0 {
            return Err("Native generation must be nonzero.".into());
        }
        Ok(Self {
            generation,
            last_job: 0,
            session: ClassProgramSessionV1::new().map_err(|e| format!("Native session: {e:?}"))?,
        })
    }
    pub fn run(
        &mut self,
        mut request: RunRequest,
        mut prepared: impl FnMut(NativeEvent),
    ) -> Finished {
        let start = Instant::now();
        let mut result = Finished {
            lease: request.lease.take(),
            execution_started: false,
            key: request.key,
            revision: self.session.context().revision,
            status: Status::Error,
            reason: None,
            error: None,
            completed_statements: 0,
            effect_sequence: 0,
            output: String::new(),
            event_count: 0,
            events_complete: true,
            workspace: Vec::new(),
            workspace_complete: true,
            sources: Vec::new(),
            graph_sha256: None,
            duration_ms: 0,
            preview_omitted: false,
        };
        if request.key.generation != self.generation || request.key.job <= self.last_job {
            result.error = Some(located(1, 1, "Stale native job."));
            return result;
        }
        self.last_job = request.key.job;
        let closure = load_sources(&request, &mut result.sources);
        if let Err(failure) = closure {
            let (message, stopped) = match failure {
                AdmissionError::Stopped => ("Stopped during source admission.".to_owned(), true),
                AdmissionError::Message(s) => (s, false),
            };
            result.reason = stopped.then_some(ProgramCancelReasonV1::UserStop);
            result.status = if result.reason.is_some() {
                Status::Cancelled
            } else {
                Status::Error
            };
            result.error = Some(located((result.sources.len() + 1) as u64, 1, &message));
            self.finish_workspace(&mut result);
            return result;
        }
        result.graph_sha256 = Some(graph_hash(&result.sources));
        let sources = result
            .sources
            .iter()
            .map(|s| ClassSourceV1 {
                source_id: s.source_id,
                text: Arc::clone(&s.text),
            })
            .collect();
        let graph = match parse_class_graph_controlled_v1(sources, || {
            #[cfg(test)]
            if let Some(probe) = &request.parse_probe {
                probe.checkpoint();
            }
            request.stop.requested()
        }) {
            Ok(g) => Arc::new(g),
            Err(e) => {
                result.reason = (e.code == ClassParseErrorCodeV1::Cancelled)
                    .then_some(ProgramCancelReasonV1::UserStop);
                result.status = if result.reason.is_some() {
                    Status::Cancelled
                } else {
                    Status::Error
                };
                result.error = Some(located(
                    e.source_id,
                    e.span.start_line as usize,
                    &format!("Parse: {:?}", e.code),
                ));
                self.finish_workspace(&mut result);
                return result;
            }
        };
        let execution = match self
            .session
            .prepare(graph, ClassProgramOptionsV1::default())
        {
            Err(e) => Err(e),
            Ok(run) => {
                let cancel = run.cancel_handle();
                if request.stop.requested() {
                    cancel.request_stop();
                }
                prepared(NativeEvent::Prepared {
                    key: request.key,
                    cancel,
                });
                result.execution_started = true;
                Ok(run.execute_controlled(|_, cancel| {
                    if request.stop.requested() {
                        cancel.request_stop();
                    }
                }))
            }
        };
        let outcome = match execution {
            Ok(o) => o,
            Err(e) => {
                result.error = Some(located(1, 1, &format!("Prepare: {e:?}")));
                self.finish_workspace(&mut result);
                return result;
            }
        };
        result.revision = outcome.final_context.revision;
        result.status = match outcome.status {
            ProgramStatusV1::Success => Status::Success,
            ProgramStatusV1::Resource => Status::Resource,
            ProgramStatusV1::Cancelled(reason) => {
                result.reason = Some(reason);
                Status::Cancelled
            }
            _ => Status::Error,
        };
        result.completed_statements = outcome.completed_leaf_statement_count;
        result.effect_sequence = outcome.committed_effect_sequence;
        result.error = outcome.error.map(|e| {
            located(
                e.location.source_id,
                e.location.span.start_line as usize,
                &format!("Execution: {:?}", e.code),
            )
        });
        result.event_count = outcome.events.len();
        // 256 * 32 numeric cells, bounded names and formatting; never format whole values.
        for event in outcome.events.iter().take(MAX_EVENT_PREVIEWS) {
            let name = clip(event.name.as_deref().unwrap_or("ans"), 120);
            let mut preview = match &event.value {
                ClassEventValueV1::Scientific(v) => numeric_preview(v, 32),
                ClassEventValueV1::Object(o) => format!(
                    "{} object {:?} slot={} generation={} valid={}",
                    clip(o.class_name(), 120),
                    o.kind(),
                    o.slot(),
                    o.generation(),
                    o.is_valid()
                ),
            };
            if preview.len() > 4096 {
                preview.truncate(floor_boundary(&preview, 4096));
            }
            if result.output.len() + name.len() + preview.len() + 8 > 512 * 1024 {
                result.preview_omitted = true;
                break;
            }
            result.output.push_str(&name);
            result.output.push_str(" = ");
            result.output.push_str(&preview);
            result.output.push('\n');
        }
        result.events_complete =
            outcome.events.len() <= MAX_EVENT_PREVIEWS && !result.preview_omitted;
        self.finish_workspace(&mut result);
        result.duration_ms = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
        result.enforce_bound();
        result
    }
    fn finish_workspace(&self, result: &mut Finished) {
        result.revision = self.session.context().revision;
        for binding in self.session.bindings() {
            let variable = match self.session.view(binding.value) {
                Ok(ClassValueViewV1::Scientific(v)) => Variable {
                    name: binding.name.to_owned(),
                    class_name: class_name(v).into(),
                    dimensions: v.axes().iter().map(|&x| x as usize).collect(),
                    bytes: v.payload_bytes() as u64,
                    preview: numeric_preview(v, 1),
                    dimensions_known: true,
                    bytes_known: true,
                },
                Ok(ClassValueViewV1::Object {
                    class_name,
                    metadata,
                }) => Variable {
                    name: binding.name.to_owned(),
                    class_name: clip(class_name, 256),
                    dimensions: Vec::new(),
                    bytes: 0,
                    preview: format!(
                        "{:?} object slot={} generation={} valid={}; metadata only",
                        metadata.kind(),
                        metadata.slot(),
                        metadata.generation(),
                        metadata.is_valid()
                    ),
                    dimensions_known: false,
                    bytes_known: false,
                },
                Err(_) => {
                    result.workspace_complete = false;
                    continue;
                }
            };
            let need = variable_bytes(&variable);
            if result
                .owned_bytes()
                .saturating_add(need + std::mem::size_of::<Variable>())
                > MAX_RESPONSE_BYTES - 16384
            {
                result.workspace_complete = false;
                result.preview_omitted = true;
                break;
            }
            result.workspace.push(variable);
        }
        result.enforce_bound();
    }
    pub fn page(&self, request: &PageRequest) -> Result<Page, String> {
        if request.token.key.generation != self.generation
            || request.token.key.job != self.last_job
            || request.token.revision != self.session.context().revision
        {
            return Err("Stale native workspace page.".into());
        }
        if request.rows > MAX_PAGE_CELLS
            || request.columns > MAX_PAGE_CELLS
            || request
                .rows
                .checked_mul(request.columns)
                .is_none_or(|n| n > MAX_PAGE_CELLS)
        {
            return Err("Native page exceeds512 cells.".into());
        }
        let value = self
            .session
            .binding(&request.token.binding)
            .ok_or("Binding no longer exists.")?;
        let mut page = Page {
            token: request.token.clone(),
            dimensions: Vec::new(),
            row: request.row,
            column: request.column,
            higher: request.higher.clone(),
            rows: 0,
            columns: 0,
            class_name: String::new(),
            storage: "object",
            complex: false,
            cells: Vec::new(),
            object: None,
        };
        match self
            .session
            .view(value)
            .map_err(|e| format!("Object: {e:?}"))?
        {
            ClassValueViewV1::Object {
                class_name,
                metadata,
            } => {
                page.class_name = clip(class_name, 256);
                page.object = Some(format!(
                    "{:?} object slot={} generation={} valid={}; read-only metadata",
                    metadata.kind(),
                    metadata.slot(),
                    metadata.generation(),
                    metadata.is_valid()
                ));
            }
            ClassValueViewV1::Scientific(v) => {
                page.dimensions = v.axes().iter().map(|&x| x as usize).collect();
                page.class_name = class_name(v).into();
                page.complex = v.is_complex();
                page.storage = if v.is_sparse() { "csc" } else { "dense" };
                let dims = &page.dimensions;
                if request.higher.len() != dims.len().saturating_sub(2)
                    || request
                        .higher
                        .iter()
                        .zip(dims.iter().skip(2))
                        .any(|(&x, &n)| x >= n as u64)
                {
                    return Err("Exact higher-axis slice required.".into());
                }
                let nr = dims[0];
                let nc = dims[1];
                if request.row > nr || request.column > nc {
                    return Err("Page starts outside array.".into());
                }
                page.rows = request.rows.min(nr - request.row);
                page.columns = request.columns.min(nc - request.column);
                if page.rows == 0 || page.columns == 0 {
                    page.rows = 0;
                    page.columns = 0;
                }
                let mut base = 0usize;
                let mut stride = nr.checked_mul(nc).ok_or("Index overflow.")?;
                for (&index, &axis) in request.higher.iter().zip(dims.iter().skip(2)) {
                    base = base
                        .checked_add(
                            (index as usize)
                                .checked_mul(stride)
                                .ok_or("Index overflow.")?,
                        )
                        .ok_or("Index overflow.")?;
                    stride = stride.checked_mul(axis).ok_or("Index overflow.")?;
                }
                for row in request.row..request.row + page.rows {
                    let mut cells = Vec::with_capacity(page.columns);
                    for col in request.column..request.column + page.columns {
                        let (z, stored) = match v.storage_view() {
                            ScientificStorageViewV2::Dense(d) => {
                                let index = base
                                    .checked_add(
                                        col.checked_mul(nr)
                                            .and_then(|x| x.checked_add(row))
                                            .ok_or("Index overflow.")?,
                                    )
                                    .ok_or("Index overflow.")?;
                                (d.get(index).ok_or("Index outside value.")?, true)
                            }
                            ScientificStorageViewV2::Csc(c) => {
                                let a = c.column_pointers()[col] as usize;
                                let b = c.column_pointers()[col + 1] as usize;
                                if let Ok(i) = c.row_indices()[a..b].binary_search(&(row as u32)) {
                                    let i = a + i;
                                    let z = match c.values() {
                                        ScientificCscValuesV1::Real(x) => {
                                            Complex64V1 { re: x[i], im: 0. }
                                        }
                                        ScientificCscValuesV1::Complex(x) => x[i],
                                    };
                                    (z, true)
                                } else {
                                    (Complex64V1 { re: 0., im: 0. }, false)
                                }
                            }
                        };
                        cells.push(Cell {
                            text: word_text(z, page.complex),
                            stored,
                        });
                    }
                    page.cells.push(cells);
                }
            }
        }
        if page.owned_bytes() > MAX_RESPONSE_BYTES {
            return Err("Native page response limit.".into());
        }
        Ok(page)
    }
    pub fn page_event(&self, mut request: PageRequest) -> NativeEvent {
        let result = self.page(&request);
        NativeEvent::Page {
            request_id: request.request_id,
            token: request.token,
            result,
            lease: request.lease.take(),
        }
    }
}
fn located(source_id: u64, line: usize, message: &str) -> LocatedError {
    LocatedError {
        source_id,
        line,
        column: 1,
        message: clip(message, 1024),
    }
}
fn floor_boundary(s: &str, n: usize) -> usize {
    let mut n = n.min(s.len());
    while !s.is_char_boundary(n) {
        n -= 1
    }
    n
}
fn clip(s: &str, n: usize) -> String {
    let end = floor_boundary(s, n);
    let mut out = s[..end].to_owned();
    if end < s.len() {
        out.push('…')
    }
    out
}
fn class_name(v: &ScientificProgramValueV2) -> &'static str {
    if v.class() == ProgramValueClassV1::Double {
        "double"
    } else {
        "logical"
    }
}
fn word_text(z: Complex64V1, complex: bool) -> String {
    if complex {
        format!("{:.16e} {:+.16e}i", z.re, z.im)
    } else {
        format!("{:.16e}", z.re)
    }
}
fn numeric_preview(v: &ScientificProgramValueV2, count: usize) -> String {
    let mut out = format!(
        "{}{} {} {:?}",
        class_name(v),
        if v.is_complex() { " complex" } else { "" },
        if v.is_sparse() { "CSC" } else { "dense" },
        &*v.axes()
    );
    for i in 0..v.stored_len().min(count) {
        let z = match v.storage_view() {
            ScientificStorageViewV2::Dense(d) => d.get(i).unwrap(),
            ScientificStorageViewV2::Csc(c) => match c.values() {
                ScientificCscValuesV1::Real(x) => Complex64V1 { re: x[i], im: 0. },
                ScientificCscValuesV1::Complex(x) => x[i],
            },
        };
        out.push(' ');
        out.push_str(&word_text(z, v.is_complex()));
    }
    if v.stored_len() > count {
        out.push_str(" … (preview)")
    }
    out
}
fn variable_bytes(v: &Variable) -> usize {
    std::mem::size_of::<Variable>()
        + v.name.capacity()
        + v.class_name.capacity()
        + v.preview.capacity()
        + v.dimensions.capacity() * std::mem::size_of::<usize>()
}
impl Finished {
    pub fn owned_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.output.capacity()
            + self.workspace.capacity() * std::mem::size_of::<Variable>()
            + self
                .workspace
                .iter()
                .map(|v| variable_bytes(v) - std::mem::size_of::<Variable>())
                .sum::<usize>()
            + self.sources.capacity() * std::mem::size_of::<SourceSnapshot>()
            + self
                .sources
                .iter()
                .map(|s| s.text.len() + 64 + s.sha256.capacity() + s.path.capacity())
                .sum::<usize>()
            + self.graph_sha256.as_ref().map_or(0, String::capacity)
            + self.error.as_ref().map_or(0, |e| e.message.capacity())
    }
    fn enforce_bound(&mut self) {
        if self.owned_bytes() > MAX_RESPONSE_BYTES {
            self.output = String::new();
            self.workspace = Vec::new();
            self.events_complete = false;
            self.workspace_complete = false;
            self.preview_omitted = true;
        }
    }
}
impl Page {
    pub fn owned_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.token.binding.capacity()
            + self.class_name.capacity()
            + self.dimensions.capacity() * std::mem::size_of::<usize>()
            + self.higher.capacity() * 8
            + self.cells.capacity() * std::mem::size_of::<Vec<Cell>>()
            + self
                .cells
                .iter()
                .map(|r| {
                    r.capacity() * std::mem::size_of::<Cell>()
                        + r.iter().map(|c| c.text.capacity()).sum::<usize>()
                })
                .sum::<usize>()
            + self.object.as_ref().map_or(0, String::capacity)
    }
}
enum AdmissionError {
    Stopped,
    Message(String),
}
impl From<String> for AdmissionError {
    fn from(s: String) -> Self {
        Self::Message(s)
    }
}
impl From<&str> for AdmissionError {
    fn from(s: &str) -> Self {
        Self::Message(s.into())
    }
}
fn load_sources(
    request: &RunRequest,
    into: &mut Vec<SourceSnapshot>,
) -> Result<(), AdmissionError> {
    if request.stop.requested() {
        return Err(AdmissionError::Stopped);
    }
    if request.modules.len() >= MAX_SOURCES || request.entry_text.len() > MAX_SOURCE_BYTES {
        return Err("Native source bounds exceeded.".into());
    }
    let mut paths = Vec::new();
    let mut total = request.entry_text.len();
    let entry = source_identity(&request.entry_path)?;
    paths.push(entry.clone());
    into.push(snapshot(
        1,
        Some(request.document_id),
        entry,
        Arc::from(request.entry_text.as_str()),
    ));
    for (index, path) in request.modules.iter().enumerate() {
        if request.stop.requested() {
            return Err(AdmissionError::Stopped);
        }
        let canonical = source_identity(path)?;
        if paths.contains(&canonical) {
            return Err("Duplicate explicit native source.".into());
        }
        paths.push(canonical.clone());
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC);
        }
        let mut file = options
            .open(path)
            .map_err(|e| format!("Module open: {e}"))?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Explicit module is not a regular file.".into());
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(MAX_SOURCE_BYTES + 1)
            .map_err(|_| "Module allocation failed.")?;
        let mut block = [0u8; 4096];
        loop {
            if request.stop.requested() {
                return Err(AdmissionError::Stopped);
            }
            let remaining = MAX_SOURCE_BYTES + 1 - bytes.len();
            if remaining == 0 {
                break;
            }
            let take = remaining.min(block.len());
            let n = file.read(&mut block[..take]).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&block[..n]);
        }
        if bytes.len() > MAX_SOURCE_BYTES {
            return Err("Native module exceeds64KiB.".into());
        }
        total = total
            .checked_add(bytes.len())
            .ok_or("Source byte overflow.")?;
        if total > MAX_GRAPH_BYTES {
            return Err("Native closure exceeds256KiB.".into());
        }
        let text = String::from_utf8(bytes).map_err(|_| "Native module is not UTF-8.")?;
        into.push(snapshot(
            (index + 2) as u64,
            None,
            canonical,
            Arc::from(text),
        ));
    }
    if request.stop.requested() {
        return Err(AdmissionError::Stopped);
    }
    Ok(())
}
fn source_identity(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().len() > MAX_PATH_BYTES || path.to_str().is_none() {
        return Err("Source path must be bounded UTF-8.".into());
    }
    // Unsaved entries need not exist. Existing modules are opened once and checked on that handle.
    let path = if path.exists() {
        path.canonicalize().map_err(|e| e.to_string())?
    } else if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    if path.as_os_str().len() > MAX_PATH_BYTES || path.to_str().is_none() {
        return Err("Canonical source path exceeds bounds.".into());
    }
    Ok(path)
}
fn snapshot(
    source_id: u64,
    document_id: Option<usize>,
    path: PathBuf,
    text: Arc<str>,
) -> SourceSnapshot {
    let sha256 = format!("{:x}", Sha256::digest(text.as_bytes()));
    SourceSnapshot {
        source_id,
        document_id,
        path,
        text,
        sha256,
    }
}
fn graph_hash(sources: &[SourceSnapshot]) -> String {
    let mut hash = Sha256::new();
    for s in sources {
        let path = s.path.to_str().unwrap().as_bytes();
        hash.update((path.len() as u64).to_le_bytes());
        hash.update(path);
        hash.update(s.source_id.to_le_bytes());
        hash.update((s.text.len() as u64).to_le_bytes());
        hash.update(Sha256::digest(s.text.as_bytes()));
    }
    format!("{:x}", hash.finalize())
}

impl RunRequest {
    pub fn rejected(mut self, message: &str) -> Finished {
        Finished {
            lease: self.lease.take(),
            execution_started: false,
            key: self.key,
            revision: 0,
            status: Status::Error,
            reason: None,
            error: Some(located(1, 1, message)),
            completed_statements: 0,
            effect_sequence: 0,
            output: String::new(),
            event_count: 0,
            events_complete: true,
            workspace: Vec::new(),
            workspace_complete: false,
            sources: Vec::new(),
            graph_sha256: None,
            duration_ms: 0,
            preview_omitted: false,
        }
    }
}
impl PageRequest {
    pub fn rejected(mut self, message: &str) -> NativeEvent {
        NativeEvent::Page {
            request_id: self.request_id,
            token: self.token,
            result: Err(clip(message, 1024)),
            lease: self.lease.take(),
        }
    }
}

// Request-local, bounded test-only handshake at the REAL parser callback. Not an
// event claiming that parsing happened, nor a second parser/evaluator. Production
// requests contain no probe, synchronization primitive, wait or phase override.
#[cfg(test)]
#[derive(Debug)]
struct ParseProbe {
    callbacks: AtomicUsize,
    entered: std::sync::mpsc::SyncSender<()>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}
#[cfg(test)]
impl ParseProbe {
    fn checkpoint(&self) {
        if self.callbacks.fetch_add(1, Ordering::AcqRel) == 0 {
            let _ = self.entered.try_send(());
            let _ = self
                .release
                .lock()
                .expect("test parse handshake mutex")
                .recv_timeout(std::time::Duration::from_secs(2));
        }
    }
}
#[cfg(test)]
mod parse_phase_tests {
    use super::*;
    use crate::worker::{self, Event, Request};
    use std::sync::mpsc::{Receiver, sync_channel};
    use std::time::Duration;
    fn event(rx: &Receiver<Event>) -> NativeEvent {
        match rx
            .recv_timeout(Duration::from_secs(2))
            .expect("bounded native actor response")
        {
            Event::Native(e) => e,
            _ => panic!("unexpected legacy event"),
        }
    }
    #[test]
    fn actor_stop_inside_actual_parser_callback_then_recovers_same_session() {
        let (tx, rx) = worker::spawn(eframe::egui::Context::default());
        let gate = NativeGate::default();
        tx.send(Request::NativeStart {
            generation: 1,
            lease: gate.start().unwrap(),
        })
        .unwrap();
        match event(&rx) {
            NativeEvent::Ready {
                generation: 1,
                lease,
            } => drop(lease),
            e => panic!("{e:?}"),
        }
        let (entered_tx, entered_rx) = sync_channel(1);
        let (release_tx, release_rx) = sync_channel(1);
        let probe = Arc::new(ParseProbe {
            callbacks: AtomicUsize::new(0),
            entered: entered_tx,
            release: std::sync::Mutex::new(release_rx),
        });
        let mut request = RunRequest::new(
            &gate,
            JobKey {
                generation: 1,
                job: 1,
            },
            1,
            Path::new("/private/tmp/desktop-in-parser-stop.m"),
            "must_not_publish=42;",
            &[],
        )
        .unwrap();
        request.parse_probe = Some(Arc::clone(&probe));
        let stop = request.stop.clone();
        assert!(!stop.requested());
        tx.send(Request::NativeRun(request)).unwrap();
        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("real parser callback entered");
        assert!(probe.callbacks.load(Ordering::Acquire) >= 1);
        assert!(!stop.requested());
        stop.request();
        release_tx.send(()).unwrap();
        // Prepared would prove cancellation arrived too late; do not skip it here.
        let result = match event(&rx) {
            NativeEvent::Finished(f) => f,
            e => panic!("parse cancellation must precede prepare: {e:?}"),
        };
        assert_eq!(result.status, Status::Cancelled);
        assert_eq!(result.reason, Some(ProgramCancelReasonV1::UserStop));
        assert!(!result.execution_started);
        assert_eq!(result.effect_sequence, 0);
        assert_eq!(result.completed_statements, 0);
        assert!(result.workspace.is_empty());
        assert!(result.graph_sha256.is_some());
        assert!(result.error.as_ref().unwrap().message.contains("Cancelled"));
        assert_eq!(result.sources[0].text.as_ref(), "must_not_publish=42;");
        drop(result);
        assert_eq!(gate.reserved_bytes(), 0);
        let request = RunRequest::new(
            &gate,
            JobKey {
                generation: 1,
                job: 2,
            },
            1,
            Path::new("/private/tmp/desktop-in-parser-stop.m"),
            "recovered=31;",
            &[],
        )
        .unwrap();
        tx.send(Request::NativeRun(request)).unwrap();
        match event(&rx) {
            NativeEvent::Prepared { key, .. } => assert_eq!(
                key,
                JobKey {
                    generation: 1,
                    job: 2
                }
            ),
            e => panic!("{e:?}"),
        }
        let result = match event(&rx) {
            NativeEvent::Finished(f) => f,
            e => panic!("{e:?}"),
        };
        assert_eq!(result.status, Status::Success, "{:?}", result.error);
        assert!(result.execution_started);
        assert!(result.workspace.iter().any(|v| v.name == "recovered"));
        assert!(
            !result
                .workspace
                .iter()
                .any(|v| v.name == "must_not_publish")
        );
        println!(
            "desktop_parse_stop_handshake callbacks={} cancelled_before_prepare=true effect_sequence=0 recovered_same_session=true",
            probe.callbacks.load(Ordering::Acquire)
        );
        drop(result);
        assert_eq!(gate.reserved_bytes(), 0);
    }
}
