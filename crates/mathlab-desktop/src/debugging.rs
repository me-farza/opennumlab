//! The debugger owns an independent command worker, so paused execution never blocks desktop controls.

use crate::{dimensions, BLUE, MUTED, RED};
use eframe::egui::{self, RichText};
use mathlab_debugger::{
    DebugFrame, DebugState, DebugVariable, Debugger, ResumeAction, SourceSnapshot,
};
use std::{
    path::PathBuf,
    sync::mpsc::{channel, Receiver, Sender},
    time::Duration,
};

enum Request {
    Resume {
        id: u64,
        revision: u64,
        action: ResumeAction,
    },
    Evaluate {
        id: u64,
        revision: u64,
        source: String,
    },
    Breakpoint {
        id: u64,
        file: PathBuf,
        line: usize,
        clear: bool,
    },
    Shutdown,
}

enum ResultData {
    Resumed,
    Evaluated {
        output: String,
        streamed: bool,
        truncated: bool,
        variables: Vec<DebugVariable>,
        values: Vec<(String, String)>,
    },
    Breakpoint {
        file: PathBuf,
        requested_line: usize,
        actual_lines: Vec<usize>,
        cleared: bool,
    },
}

struct ScopeData {
    frames: Vec<DebugFrame>,
    variables: Vec<DebugVariable>,
    values: Vec<(String, String)>,
    snapshot: Result<Option<SourceSnapshot>, String>,
}

enum Event {
    State(DebugState),
    Scope {
        revision: u64,
        result: Result<ScopeData, String>,
    },
    Completed {
        id: u64,
        revision: Option<u64>,
        result: Result<ResultData, String>,
    },
    Failed(String),
}

pub struct DebuggerPanel {
    requests: Sender<Request>,
    events: Receiver<Event>,
    pub state: Option<DebugState>,
    input_blocked: bool,
    console_streaming: bool,
    output_in_console: bool,
    output_truncated: bool,
    frames: Vec<DebugFrame>,
    variables: Vec<DebugVariable>,
    values: Vec<(String, String)>,
    snapshot: Option<SourceSnapshot>,
    snapshot_error: Option<String>,
    loading_scope: bool,
    pending: Option<u64>,
    pending_evaluation: bool,
    pending_revision: Option<u64>,
    next_id: u64,
    expression: String,
    output: String,
    error: Option<String>,
    breakpoint_file: String,
    breakpoint_line: usize,
    breakpoints: Vec<(PathBuf, usize)>,
    pub show: bool,
    pub screenshot_ready: bool,
    pub breakpoint_completed: Option<(u64, bool)>,
    pub breakpoint_requested: Option<(PathBuf, usize)>,
}

fn spawn_job(
    events: Sender<Event>,
    ctx: egui::Context,
    job: impl FnOnce() -> Event + Send + 'static,
) {
    let error_events = events.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("mathlab-debug-command".into())
        .spawn(move || {
            let _ = events.send(job());
            ctx.request_repaint();
        })
    {
        let _ = error_events.send(Event::Failed(format!(
            "Could not start debugger command worker: {error}"
        )));
    }
}

fn command(debugger: Debugger, request: Request) -> Event {
    let (id, revision, result) = match request {
        Request::Resume {
            id,
            revision,
            action,
        } => (
            id,
            Some(revision),
            debugger
                .resume(action, revision)
                .map(|()| ResultData::Resumed),
        ),
        Request::Evaluate {
            id,
            revision,
            source,
        } => {
            let result = debugger
                .evaluate_output(&source, revision)
                .and_then(|output| {
                    debugger.workspace(revision).map(|variables| {
                        let values = scalar_values(&debugger, &variables, revision);
                        ResultData::Evaluated {
                            output: output.output,
                            streamed: output.streamed,
                            truncated: output.truncated,
                            variables,
                            values,
                        }
                    })
                });
            (id, Some(revision), result)
        }
        Request::Breakpoint {
            id,
            file,
            line,
            clear,
        } => {
            let result = if clear {
                debugger.clear_breakpoint(&file, line).map(|()| Vec::new())
            } else {
                debugger.set_breakpoint(&file, line)
            };
            (
                id,
                None,
                result.map(|actual_lines| ResultData::Breakpoint {
                    file,
                    requested_line: line,
                    actual_lines,
                    cleared: clear,
                }),
            )
        }
        Request::Shutdown => unreachable!(),
    };
    Event::Completed {
        id,
        revision,
        result: result.map_err(|error| error.to_string()),
    }
}

fn user_frames(frames: Vec<DebugFrame>, internal_worker: &std::path::Path) -> Vec<DebugFrame> {
    frames
        .into_iter()
        .filter(|frame| {
            let path = frame
                .file
                .canonicalize()
                .unwrap_or_else(|_| frame.file.clone());
            path != internal_worker
        })
        .collect()
}

fn scalar_preview_source(variable: &DebugVariable) -> Option<String> {
    let mut name = variable.name.bytes();
    if !name
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        || !name.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || variable.dimensions.is_empty()
        || variable.dimensions.iter().any(|&dimension| dimension != 1)
        || !matches!(
            variable.class_name.as_str(),
            "double"
                | "single"
                | "logical"
                | "int8"
                | "uint8"
                | "int16"
                | "uint16"
                | "int32"
                | "uint32"
                | "int64"
                | "uint64"
        )
    {
        return None;
    }
    // Recheck the value inside the native paused scope. Builtins avoid user-defined display/class methods.
    Some(format!(
        "if (builtin('any', builtin('strcmp', builtin('class', {0}), {{'double','single','logical','int8','uint8','int16','uint16','int32','uint32','int64','uint64'}})) && builtin('numel', {0}) == 1); builtin('disp', {0}); endif;",
        variable.name
    ))
}

fn scalar_values(
    debugger: &Debugger,
    variables: &[DebugVariable],
    revision: u64,
) -> Vec<(String, String)> {
    let mut values = Vec::new();
    for (variable, source) in variables
        .iter()
        .filter_map(|variable| scalar_preview_source(variable).map(|source| (variable, source)))
        .take(24)
    {
        match debugger.evaluate(&source, revision) {
            Ok(output) => values.push((variable.name.clone(), output.trim().to_owned())),
            Err(_) => {
                // A resumed/replaced scope or lost connection invalidates the remaining previews.
                values.push((variable.name.clone(), "Unavailable".into()));
                break;
            }
        }
    }
    values
}

fn snapshot_excerpt(source: &str, line: usize) -> (usize, String, bool) {
    let start = line.saturating_sub(16);
    let mut truncated = false;
    let text = source
        .split('\n')
        .skip(start)
        .take(31)
        .map(|line| {
            // Bound each line so a long preceding line cannot consume the
            // whole preview and hide the statement where execution paused.
            if line.len() <= 512 {
                line.to_owned()
            } else {
                truncated = true;
                let mut end = 512 - '…'.len_utf8();
                while !line.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}…", &line[..end])
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    (start + 1, text, truncated)
}

impl DebuggerPanel {
    pub fn new(debugger: Debugger, ctx: egui::Context) -> Self {
        let internal_worker = debugger
            .directory()
            .parent()
            .unwrap_or(debugger.directory())
            .join("mathlab_worker.m");
        let internal_worker = internal_worker.canonicalize().unwrap_or(internal_worker);
        let (requests_tx, requests_rx) = channel();
        let (events_tx, events_rx) = channel();
        std::thread::Builder::new()
            .name("mathlab-debugger".into())
            .spawn(move || {
                let mut previous = None;
                let mut last_error = None;
                loop {
                    match requests_rx.recv_timeout(Duration::from_millis(150)) {
                        Ok(Request::Shutdown)
                        | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        Ok(request) => {
                            let debugger = debugger.clone();
                            spawn_job(events_tx.clone(), ctx.clone(), move || {
                                command(debugger, request)
                            });
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    match debugger.state() {
                        Ok(Some(state)) => {
                            last_error = None;
                            if previous.as_ref() != Some(&state) {
                                if events_tx.send(Event::State(state.clone())).is_err() {
                                    break;
                                }
                                ctx.request_repaint();
                                if state.paused {
                                    let debugger = debugger.clone();
                                    let revision = state.revision;
                                    let runtime_file = state.runtime_file.clone();
                                    let internal_worker = internal_worker.clone();
                                    spawn_job(events_tx.clone(), ctx.clone(), move || {
                                        let result = debugger
                                            .stack(revision)
                                            .and_then(|frames| {
                                                debugger.workspace(revision).map(|variables| {
                                                    let values = scalar_values(
                                                        &debugger, &variables, revision,
                                                    );
                                                    ScopeData {
                                                        frames: user_frames(
                                                            frames,
                                                            &internal_worker,
                                                        ),
                                                        variables,
                                                        values,
                                                        snapshot: debugger
                                                            .source_snapshot(&runtime_file)
                                                            .map_err(|error| error.to_string()),
                                                    }
                                                })
                                            })
                                            .map_err(|error| error.to_string());
                                        Event::Scope { revision, result }
                                    });
                                }
                                previous = Some(state);
                            }
                        }
                        Ok(None) => {}
                        Err(error) => {
                            let message = error.to_string();
                            if last_error.as_ref() != Some(&message) {
                                if events_tx.send(Event::Failed(message.clone())).is_err() {
                                    break;
                                }
                                ctx.request_repaint();
                                last_error = Some(message);
                            }
                        }
                    }
                }
            })
            .expect("could not create debugger worker thread");
        Self {
            requests: requests_tx,
            events: events_rx,
            state: None,
            input_blocked: false,
            console_streaming: false,
            output_in_console: false,
            output_truncated: false,
            frames: Vec::new(),
            variables: Vec::new(),
            values: Vec::new(),
            snapshot: None,
            snapshot_error: None,
            loading_scope: false,
            pending: None,
            pending_evaluation: false,
            pending_revision: None,
            next_id: 1,
            expression: String::new(),
            output: String::new(),
            error: None,
            breakpoint_file: String::new(),
            breakpoint_line: 1,
            breakpoints: Vec::new(),
            show: true,
            screenshot_ready: false,
            breakpoint_completed: None,
            breakpoint_requested: None,
        }
    }

    pub fn set_console_streaming(&mut self, streaming: bool) {
        self.console_streaming = streaming;
    }

    pub fn set_input_blocked(&mut self, blocked: bool) {
        self.input_blocked = blocked;
    }

    pub fn paused(&self) -> bool {
        self.state.as_ref().is_some_and(|state| state.paused)
    }

    /// Returns the most recent new pause for source navigation. Scope replies from prior revisions are ignored.
    pub fn poll(&mut self) -> Option<DebugState> {
        let mut pause = None;
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::State(state) => {
                    if self
                        .state
                        .as_ref()
                        .is_some_and(|current| state.revision < current.revision)
                    {
                        continue;
                    }
                    if self
                        .pending_revision
                        .is_some_and(|revision| revision != state.revision)
                    {
                        self.pending = None;
                        self.pending_evaluation = false;
                        self.pending_revision = None;
                    }
                    self.frames.clear();
                    self.variables.clear();
                    self.values.clear();
                    self.output.clear();
                    self.output_in_console = false;
                    self.output_truncated = false;
                    self.loading_scope = state.paused;
                    self.screenshot_ready = false;
                    self.snapshot = None;
                    self.snapshot_error = None;
                    if state.paused {
                        self.show = true;
                        pause = Some(state.clone());
                    }
                    self.state = Some(state);
                }
                Event::Scope { revision, result } => {
                    if !self.current_revision(revision) {
                        continue;
                    }
                    self.loading_scope = false;
                    self.screenshot_ready = true;
                    match result {
                        Ok(scope) => {
                            self.frames = scope.frames;
                            self.variables = scope.variables;
                            self.values = scope.values;
                            match scope.snapshot {
                                Ok(snapshot) => {
                                    self.snapshot = snapshot;
                                    self.snapshot_error = None;
                                }
                                Err(error) => {
                                    self.snapshot = None;
                                    self.snapshot_error = Some(error);
                                }
                            }
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                Event::Completed {
                    id,
                    revision,
                    result,
                } => {
                    if self.pending != Some(id) {
                        continue;
                    }
                    self.pending = None;
                    self.pending_evaluation = false;
                    self.pending_revision = None;
                    if revision.is_none() {
                        self.breakpoint_completed = Some((id, result.is_ok()));
                    }
                    if revision.is_some_and(|revision| !self.current_revision(revision)) {
                        continue;
                    }
                    match result {
                        Ok(ResultData::Resumed) => {
                            if let Some(state) = &mut self.state {
                                state.paused = false;
                            }
                            self.frames.clear();
                            self.variables.clear();
                            self.values.clear();
                            self.snapshot = None;
                            self.snapshot_error = None;
                            self.loading_scope = false;
                            self.screenshot_ready = false;
                        }
                        Ok(ResultData::Evaluated {
                            output,
                            streamed,
                            truncated,
                            variables,
                            values,
                        }) => {
                            self.output_in_console = streamed && self.console_streaming;
                            self.output_truncated = truncated && !self.output_in_console;
                            self.output = if self.output_in_console {
                                String::new()
                            } else {
                                output
                            };
                            self.variables = variables;
                            self.values = values;
                            self.error = None;
                        }
                        Ok(ResultData::Breakpoint {
                            file,
                            requested_line,
                            actual_lines,
                            cleared,
                        }) => {
                            if cleared {
                                self.breakpoints.retain(|(path, line)| {
                                    path != &file || *line != requested_line
                                });
                            } else {
                                if actual_lines.is_empty() || actual_lines.contains(&0) {
                                    self.error = Some("Octave could not install a breakpoint at an executable source line.".into());
                                    self.breakpoint_completed = Some((id, false));
                                    continue;
                                }
                                self.breakpoints.retain(|(path, line)| {
                                    path != &file || *line != requested_line
                                });
                                for line in actual_lines {
                                    if !self.breakpoints.contains(&(file.clone(), line)) {
                                        self.breakpoints.push((file.clone(), line));
                                    }
                                }
                            }
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                Event::Failed(error) => {
                    self.error = Some(error);
                    if self.pending_revision.is_none() {
                        if let Some(id) = self.pending {
                            self.breakpoint_completed = Some((id, false));
                        }
                    }
                    self.pending = None;
                    self.pending_evaluation = false;
                    self.pending_revision = None;
                    self.loading_scope = false;
                }
            }
        }
        pause
    }

    fn current_revision(&self, revision: u64) -> bool {
        self.state
            .as_ref()
            .is_some_and(|state| state.paused && state.revision == revision)
    }

    fn submit(&mut self, request: Request, id: u64) {
        self.pending = Some(id);
        self.pending_evaluation = matches!(&request, Request::Evaluate { .. });
        self.pending_revision = match &request {
            Request::Resume { revision, .. } | Request::Evaluate { revision, .. } => {
                Some(*revision)
            }
            _ => None,
        };
        self.error = None;
        if self.requests.send(request).is_err() {
            if self.pending_revision.is_none() {
                self.breakpoint_completed = Some((id, false));
            }
            self.pending = None;
            self.pending_evaluation = false;
            self.pending_revision = None;
            self.error = Some("The debugger worker stopped.".into());
        }
    }

    pub fn evaluation_pending(&self) -> bool {
        self.pending.is_some() && self.pending_evaluation
    }

    pub fn evaluate_source(&mut self, source: String) -> Option<u64> {
        if self.pending.is_some() || self.input_blocked || source.trim().is_empty() {
            return None;
        }
        let revision = self.state.as_ref().filter(|state| state.paused)?.revision;
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.submit(
            Request::Evaluate {
                id,
                revision,
                source,
            },
            id,
        );
        Some(id)
    }

    pub fn resume(&mut self, action: ResumeAction) -> Option<u64> {
        if self.pending.is_some() || self.input_blocked {
            return None;
        }
        let revision = self.state.as_ref().filter(|state| state.paused)?.revision;
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.submit(
            Request::Resume {
                id,
                revision,
                action,
            },
            id,
        );
        Some(id)
    }

    pub fn set_breakpoint(&mut self, file: PathBuf, line: usize) -> Option<u64> {
        if !file.is_file() {
            return None;
        }
        self.set_prepared_breakpoint(file, line)
    }

    pub fn breakpoint_lines(&self, file: &std::path::Path) -> Vec<usize> {
        let mut lines: Vec<_> = self
            .breakpoints
            .iter()
            .filter_map(|(path, line)| (path == file).then_some(*line))
            .collect();
        lines.sort_unstable();
        lines.dedup();
        lines
    }

    pub fn can_modify_breakpoints(&self) -> bool {
        self.pending.is_none() && !self.input_blocked
    }

    pub fn source_snapshot(&self) -> Option<&SourceSnapshot> {
        self.snapshot.as_ref()
    }

    pub fn snapshot_loading(&self) -> bool {
        self.loading_scope
    }

    pub fn clear_breakpoint(&mut self, file: PathBuf, line: usize) -> Option<u64> {
        if self.pending.is_some() || self.input_blocked || !file.is_absolute() || line == 0 {
            return None;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.submit(
            Request::Breakpoint {
                id,
                file,
                line,
                clear: true,
            },
            id,
        );
        Some(id)
    }

    pub fn set_prepared_breakpoint(&mut self, file: PathBuf, line: usize) -> Option<u64> {
        if self.pending.is_some() || self.input_blocked || !file.is_absolute() || line == 0 {
            return None;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.submit(
            Request::Breakpoint {
                id,
                file,
                line,
                clear: false,
            },
            id,
        );
        Some(id)
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        active_file: Option<&std::path::Path>,
        engine_busy: bool,
    ) {
        if !self.show {
            return;
        }
        let mut open = true;
        let mut resume = None;
        let mut evaluate = false;
        let mut breakpoint = None;
        egui::Window::new("Debugger").id(egui::Id::new("debugger_panel")).open(&mut open).default_pos([620.0, 225.0]).default_size([750.0, 630.0]).show(ctx, |ui| {
            if let Some(state) = &self.state {
                if state.paused {
                    ui.label(RichText::new(format!("Paused at {}:{}", state.file.display(), state.line)).strong().color(BLUE));
                    if !state.function_name.is_empty() { ui.label(RichText::new(&state.function_name).monospace()); }
                } else { ui.label(RichText::new(if engine_busy { "Running…" } else { "Ready for a breakpoint" }).strong().color(BLUE)); }
            } else { ui.label(RichText::new(if engine_busy { "Running…" } else { "Ready for a breakpoint" }).strong().color(BLUE)); }
            let enabled = self.paused() && self.pending.is_none() && !self.input_blocked;
            if self.input_blocked { ui.label(RichText::new("Waiting for a response in the Command Window.").color(MUTED)); }
            ui.horizontal(|ui| {
                for (label, action) in [("Continue", ResumeAction::Continue), ("Step", ResumeAction::Step), ("Step In", ResumeAction::StepIn), ("Step Out", ResumeAction::StepOut), ("Quit Debugging", ResumeAction::Quit)] {
                    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() { resume = Some(action); }
                }
                if self.pending.is_some() || self.loading_scope { ui.spinner(); }
            });
            if let Some(error) = &self.error { ui.colored_label(RED, error); }
            ui.separator();
            ui.collapsing("Breakpoints in source files and editor buffers", |ui| {
                ui.horizontal(|ui| {
                    ui.label("File");
                    ui.add(egui::TextEdit::singleline(&mut self.breakpoint_file).desired_width(420.0).hint_text("Project-relative or absolute .m path"));
                    if ui.button("Use Active Buffer").clicked() { if let Some(path) = active_file { self.breakpoint_file = path.display().to_string(); } }
                });
                ui.horizontal(|ui| {
                    ui.label("Line"); ui.add(egui::DragValue::new(&mut self.breakpoint_line).range(1..=usize::MAX));
                    if ui.add_enabled(self.pending.is_none() && !engine_busy && !self.input_blocked, egui::Button::new("Set Breakpoint")).clicked() { breakpoint = Some((PathBuf::from(self.breakpoint_file.trim()), self.breakpoint_line, false)); }
                });
                for (file, line) in &self.breakpoints {
                    ui.horizontal(|ui| {
                        ui.monospace(format!("{}:{line}", file.display()));
                        if ui.add_enabled(self.pending.is_none() && !self.input_blocked, egui::Button::new("Remove")).clicked() { breakpoint = Some((file.clone(), *line, true)); }
                    });
                }
                ui.label(RichText::new("Set breakpoints while the engine is idle. Script buffers support unsaved edits and local functions; save function and class files first.").small().color(MUTED));
            });
            if let Some(snapshot) = &self.snapshot {
                egui::CollapsingHeader::new("Executing source snapshot").show(ui, |ui| {
                    ui.label(snapshot.original_path.display().to_string());
                    let (start, excerpt, truncated) = snapshot_excerpt(&snapshot.source, self.state.as_ref().map_or(1, |state| state.line));
                    ui.label(RichText::new(format!("Read-only context from line {start} of the executing generation.")).small().color(MUTED));
                    egui::ScrollArea::both().max_height(240.0).show(ui, |ui| {
                        let mut source = excerpt.as_str();
                        ui.add(egui::TextEdit::multiline(&mut source).code_editor().interactive(false).desired_width(700.0));
                    });
                    if truncated { ui.label(RichText::new("Long lines are clipped in this preview.").small().color(MUTED)); }
                });
            }
            if let Some(error) = &self.snapshot_error {
                ui.label(RichText::new(format!("Executing source snapshot unavailable: {error}")).small().color(RED));
            }
            ui.separator();
            ui.label(RichText::new("Call Stack").strong());
            egui::ScrollArea::vertical().id_salt("debug_stack").max_height(120.0).show(ui, |ui| {
                for frame in &self.frames { ui.monospace(format!("{}  —  {}:{}", frame.name, frame.file.display(), frame.line)); }
                if self.frames.is_empty() { ui.label(RichText::new("The paused call stack appears here.").color(MUTED)); }
            });
            ui.separator();
            ui.label(RichText::new("Local Workspace").strong());
            egui::ScrollArea::both().id_salt("debug_locals").max_height(160.0).show(ui, |ui| {
                egui::Grid::new("debug_vars").striped(true).spacing([18.0, 5.0]).show(ui, |ui| {
                    for label in ["Name", "Size", "Class", "Value", "Bytes"] { ui.strong(label); } ui.end_row();
                    for variable in &self.variables {
                        ui.monospace(&variable.name); ui.monospace(dimensions(&variable.dimensions)); ui.label(&variable.class_name);
                        ui.monospace(self.values.iter().find(|(name, _)| name == &variable.name).map_or("—", |(_, value)| value.as_str()));
                        ui.label(variable.bytes.to_string()); ui.end_row();
                    }
                });
                ui.label(RichText::new("Up to 24 primitive scalar previews; evaluate an expression to inspect other values.").small().color(MUTED));
            });
            ui.separator();
            ui.label(RichText::new("Evaluate in the paused scope").strong());
            ui.horizontal(|ui| {
                ui.add_enabled(enabled, egui::TextEdit::singleline(&mut self.expression).desired_width(540.0).hint_text("Expression or command, e.g. x + 1"));
                if ui.add_enabled(enabled && !self.expression.trim().is_empty(), egui::Button::new("Evaluate")).clicked() { evaluate = true; }
            });
            if self.output_in_console { ui.label(RichText::new("Output is in the Command Window.").small().color(MUTED)); }
            else {
                egui::ScrollArea::both().id_salt("debug_output").max_height(100.0).show(ui, |ui| { ui.monospace(&self.output); });
                if self.output_truncated { ui.label(RichText::new("The output preview was truncated.").small().color(MUTED)); }
            }
            if self.paused() { ui.label(RichText::new("Quit Debugging stops the current program and preserves workspace variables.").small().color(MUTED)); }
        });
        self.show = open;
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        if let Some(action) = resume {
            if let Some(state) = &self.state {
                self.submit(
                    Request::Resume {
                        id,
                        revision: state.revision,
                        action,
                    },
                    id,
                );
            }
        } else if evaluate {
            self.evaluate_source(self.expression.clone());
        } else if let Some((file, line, clear)) = breakpoint {
            if !clear {
                self.breakpoint_requested = Some((file, line));
            } else if !file.is_absolute() {
                self.error = Some("The prepared breakpoint path must be absolute.".into());
            } else {
                self.submit(
                    Request::Breakpoint {
                        id,
                        file,
                        line,
                        clear,
                    },
                    id,
                );
            }
        }
    }
}

impl Drop for DebuggerPanel {
    fn drop(&mut self) {
        let _ = self.requests.send(Request::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (DebuggerPanel, Sender<Event>, Receiver<Request>) {
        let (requests, requests_rx) = channel();
        let (events_tx, events) = channel();
        (
            DebuggerPanel {
                requests,
                events,
                state: None,
                input_blocked: false,
                console_streaming: false,
                output_in_console: false,
                output_truncated: false,
                frames: Vec::new(),
                variables: Vec::new(),
                values: Vec::new(),
                snapshot: None,
                snapshot_error: None,
                loading_scope: false,
                pending: None,
                pending_evaluation: false,
                pending_revision: None,
                next_id: 1,
                expression: String::new(),
                output: String::new(),
                error: None,
                breakpoint_file: String::new(),
                breakpoint_line: 1,
                breakpoints: Vec::new(),
                show: false,
                screenshot_ready: false,
                breakpoint_completed: None,
                breakpoint_requested: None,
            },
            events_tx,
            requests_rx,
        )
    }

    fn state(revision: u64, paused: bool) -> DebugState {
        DebugState {
            revision,
            paused,
            file: PathBuf::from("/tmp/proof.m"),
            line: 2,
            function_name: "proof".into(),
            runtime_file: PathBuf::from("/tmp/proof.m"),
        }
    }

    #[test]
    fn editor_debug_shortcuts_use_the_current_pause_without_running_another_script() {
        for (key, modifiers, expected) in [
            (egui::Key::F5, egui::Modifiers::NONE, ResumeAction::Continue),
            (egui::Key::F10, egui::Modifiers::NONE, ResumeAction::Step),
            (egui::Key::F11, egui::Modifiers::NONE, ResumeAction::StepIn),
            (
                egui::Key::F11,
                egui::Modifiers::SHIFT,
                ResumeAction::StepOut,
            ),
            (egui::Key::F5, egui::Modifiers::SHIFT, ResumeAction::Quit),
        ] {
            let mut test = crate::lifecycle_tests::harness();
            let (mut panel, _events, requests) = fixture();
            panel.state = Some(state(42, true));
            test.app.debugger = Some(panel);
            test.app.busy = true;
            let _ = test.ctx.run(
                egui::RawInput {
                    modifiers,
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |ctx| test.app.shortcuts(ctx),
            );
            let Request::Resume {
                revision, action, ..
            } = requests.try_recv().unwrap()
            else {
                panic!("expected a debugger control request")
            };
            assert_eq!(revision, 42);
            assert_eq!(
                std::mem::discriminant(&action),
                std::mem::discriminant(&expected)
            );
            assert!(test.requests.try_recv().is_err());
        }
    }

    #[test]
    fn f12_uses_unicode_cursor_line_and_confirmed_clear_keeps_other_files() {
        let mut test = crate::lifecycle_tests::harness();
        test.app.documents[0].source = "α = 1;\nβ = 2;\nanswer = α + β;\n".into();
        test.app.documents[0].cursor = "α = 1;\nβ = 2;\n".chars().count();
        let path =
            crate::prepared_debug::original_path(&test.app.documents[0], &test.app.directory);
        let (panel, events, requests) = fixture();
        test.app.debugger = Some(panel);
        let _ = test.ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::F12,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| test.app.shortcuts(ctx),
        );
        let crate::worker::Request::PrepareDebug {
            request_id,
            document_id,
            path: original,
            ..
        } = test.requests.try_recv().unwrap()
        else {
            panic!("expected source preparation")
        };
        assert_eq!(original, path);
        assert_eq!(
            test.app.pending_prepared_breakpoint.as_ref().unwrap().line,
            3
        );
        test.events
            .send(crate::worker::Event::DebugSourcePrepared {
                request_id,
                document_id,
                result: Ok(mathlab_engine::PreparedDebugSource {
                    original_path: path.clone(),
                    script_path: path.clone(),
                    prepared: true,
                }),
            })
            .unwrap();
        test.app.poll_events(&test.ctx);
        let Request::Breakpoint {
            id, line, clear, ..
        } = requests.try_recv().unwrap()
        else {
            panic!("expected breakpoint installation")
        };
        assert_eq!(line, 3);
        assert!(!clear);
        events
            .send(Event::Completed {
                id,
                revision: None,
                result: Ok(ResultData::Breakpoint {
                    file: path.clone(),
                    requested_line: 3,
                    actual_lines: vec![3],
                    cleared: false,
                }),
            })
            .unwrap();
        test.app.debugger_updates(&test.ctx);
        let other = test.app.directory.join("other.m");
        test.app
            .debugger
            .as_mut()
            .unwrap()
            .breakpoints
            .push((other.clone(), 3));
        test.app.toggle_editor_breakpoint(3);
        let Request::Breakpoint {
            id,
            file,
            line,
            clear,
        } = requests.try_recv().unwrap()
        else {
            panic!("expected clear")
        };
        assert_eq!(file, path);
        assert_eq!(line, 3);
        assert!(clear);
        assert_eq!(
            test.app.debugger.as_ref().unwrap().breakpoint_lines(&path),
            [3]
        );
        events
            .send(Event::Completed {
                id,
                revision: None,
                result: Ok(ResultData::Breakpoint {
                    file: path.clone(),
                    requested_line: 3,
                    actual_lines: vec![],
                    cleared: true,
                }),
            })
            .unwrap();
        test.app.debugger_updates(&test.ctx);
        assert!(test
            .app
            .debugger
            .as_ref()
            .unwrap()
            .breakpoint_lines(&path)
            .is_empty());
        assert_eq!(
            test.app.debugger.as_ref().unwrap().breakpoint_lines(&other),
            [3]
        );
    }

    #[test]
    fn run_rebinds_every_breakpoint_to_the_edited_snapshot_before_execution() {
        let mut test = crate::lifecycle_tests::harness();
        let source = "x = 1;\ny = x + 1;\nz = y + 1;\nanswer = z + 1;\n";
        test.app.documents[0].source = source.into();
        let path =
            crate::prepared_debug::original_path(&test.app.documents[0], &test.app.directory);
        let (mut panel, events, requests) = fixture();
        panel.breakpoints = vec![(path.clone(), 4), (path.clone(), 2)];
        test.app.debugger = Some(panel);
        test.app.run_document();
        for expected_line in [2, 4] {
            let crate::worker::Request::PrepareDebug {
                request_id,
                document_id,
                path: original,
                source: snapshot,
            } = test.requests.try_recv().unwrap()
            else {
                panic!("execution must await source preparation")
            };
            assert_eq!(snapshot.as_deref(), Some(source));
            assert_eq!(original, path);
            test.events
                .send(crate::worker::Event::DebugSourcePrepared {
                    request_id,
                    document_id,
                    result: Ok(mathlab_engine::PreparedDebugSource {
                        original_path: path.clone(),
                        script_path: path.clone(),
                        prepared: true,
                    }),
                })
                .unwrap();
            test.app.poll_events(&test.ctx);
            let Request::Breakpoint { id, line, .. } = requests.try_recv().unwrap() else {
                panic!("expected binding")
            };
            assert_eq!(line, expected_line);
            assert!(test.requests.try_recv().is_err());
            events
                .send(Event::Completed {
                    id,
                    revision: None,
                    result: Ok(ResultData::Breakpoint {
                        file: path.clone(),
                        requested_line: line,
                        actual_lines: vec![line],
                        cleared: false,
                    }),
                })
                .unwrap();
            test.app.debugger_updates(&test.ctx);
        }
        let crate::worker::Request::RunBuffer {
            source: actual,
            path: original,
        } = test.requests.try_recv().unwrap()
        else {
            panic!("expected one final execution")
        };
        assert_eq!(actual, source);
        assert_eq!(original, path);
        assert!(test.requests.try_recv().is_err());
        assert!(test.app.busy);
        assert!(test.app.pending_prepared_breakpoint.is_none());
    }

    #[test]
    fn relocated_breakpoint_replaces_its_old_marker_only_after_confirmation() {
        let (mut panel, events, requests) = fixture();
        let path = PathBuf::from("/tmp/edited.m");
        let other = PathBuf::from("/tmp/other.m");
        panel.breakpoints = vec![(path.clone(), 2), (other.clone(), 2)];
        let id = panel.set_prepared_breakpoint(path.clone(), 2).unwrap();
        requests.recv().unwrap();
        assert_eq!(panel.breakpoint_lines(&path), [2]);
        events
            .send(Event::Completed {
                id,
                revision: None,
                result: Ok(ResultData::Breakpoint {
                    file: path.clone(),
                    requested_line: 2,
                    actual_lines: vec![3],
                    cleared: false,
                }),
            })
            .unwrap();
        panel.poll();
        assert_eq!(panel.breakpoint_lines(&path), [3]);
        assert_eq!(panel.breakpoint_lines(&other), [2]);
    }

    #[test]
    fn an_edit_during_rebinding_cancels_the_entire_queued_run() {
        let mut test = crate::lifecycle_tests::harness();
        test.app.documents[0].source = "x = 1;\ny = 2;\nz = 3;\n".into();
        let path =
            crate::prepared_debug::original_path(&test.app.documents[0], &test.app.directory);
        let (mut panel, events, requests) = fixture();
        panel.breakpoints = vec![(path.clone(), 1), (path.clone(), 3)];
        test.app.debugger = Some(panel);
        let mut old_error =
            crate::ConsoleEntry::new("historical source error", crate::ConsoleKind::Error);
        old_error.sources_active = true;
        test.app.console.push(old_error);
        test.app.run_document();
        assert!(!test.app.console.last().unwrap().sources_active);
        let crate::worker::Request::PrepareDebug {
            request_id,
            document_id,
            ..
        } = test.requests.try_recv().unwrap()
        else {
            panic!("expected preparation")
        };
        test.events
            .send(crate::worker::Event::DebugSourcePrepared {
                request_id,
                document_id,
                result: Ok(mathlab_engine::PreparedDebugSource {
                    original_path: path.clone(),
                    script_path: path.clone(),
                    prepared: true,
                }),
            })
            .unwrap();
        test.app.poll_events(&test.ctx);
        let Request::Breakpoint { id, line, .. } = requests.try_recv().unwrap() else {
            panic!("expected binding")
        };
        test.app.documents[0].source.push_str("changed = true;\n");
        events
            .send(Event::Completed {
                id,
                revision: None,
                result: Ok(ResultData::Breakpoint {
                    file: path,
                    requested_line: line,
                    actual_lines: vec![line],
                    cleared: false,
                }),
            })
            .unwrap();
        test.app.debugger_updates(&test.ctx);
        assert!(!test.app.busy);
        assert!(test.app.pending_prepared_breakpoint.is_none());
        assert!(test.requests.try_recv().is_err());
        assert!(requests.try_recv().is_err());
        assert!(test.app.documents[0].source.ends_with("changed = true;\n"));
    }

    #[test]
    fn snapshot_preview_is_utf8_bounded_and_centers_the_executing_line() {
        let source = (1..=100)
            .map(|line| format!("line_{line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (start, text, clipped) = snapshot_excerpt(&source, 50);
        assert_eq!(start, 35);
        assert_eq!(text.lines().count(), 31);
        assert!(text.lines().any(|line| line == "line_50"));
        assert!(!clipped);
        let (_, text, clipped) = snapshot_excerpt(&"αβγ".repeat(20_000), 1);
        assert!(clipped);
        assert!(text.len() <= 16 * 1024);
        assert!(text.chars().all(|ch| matches!(ch, 'α' | 'β' | 'γ' | '…')));
        assert!(text.ends_with('…'));
        let source = format!("{}\npaused_statement\nnext_statement", "αβγ".repeat(20_000));
        let (start, text, clipped) = snapshot_excerpt(&source, 2);
        assert_eq!(start, 1);
        assert!(clipped);
        assert_eq!(text.lines().count(), 3);
        assert_eq!(text.lines().nth(1), Some("paused_statement"));
        assert!(text.len() <= 16 * 1024);
    }

    #[test]
    fn scalar_previews_do_not_execute_objects_arrays_or_invalid_identifiers() {
        let mut variable = DebugVariable {
            name: "x".into(),
            class_name: "double".into(),
            dimensions: vec![1, 1],
            bytes: 8,
            is_global: false,
            complex: false,
            sparse: false,
            persistent: false,
        };
        let source = scalar_preview_source(&variable).unwrap();
        assert!(source.contains("builtin('class', x)"));
        assert!(source.contains("builtin('numel', x) == 1"));
        assert!(source.contains("builtin('disp', x)"));
        variable.name = "x); user_command(); (x".into();
        assert!(scalar_preview_source(&variable).is_none());
        variable.name = "x".into();
        variable.class_name = "user_object".into();
        assert!(scalar_preview_source(&variable).is_none());
        variable.class_name = "double".into();
        variable.dimensions = vec![1, 20];
        assert!(scalar_preview_source(&variable).is_none());
    }

    #[test]
    fn debugger_transport_failure_finishes_pending_breakpoint_instead_of_staying_busy() {
        let (mut panel, events, requests) = fixture();
        let id = panel
            .set_prepared_breakpoint(PathBuf::from("/tmp/unsaved.m"), 2)
            .unwrap();
        assert!(matches!(
            requests.recv().unwrap(),
            Request::Breakpoint { .. }
        ));
        events
            .send(Event::Failed("adapter disconnected".into()))
            .unwrap();
        panel.poll();
        assert_eq!(panel.breakpoint_completed, Some((id, false)));
        assert!(panel.pending.is_none());
        drop(requests);
        let next = panel
            .set_prepared_breakpoint(PathBuf::from("/tmp/unsaved.m"), 3)
            .unwrap();
        assert_eq!(panel.breakpoint_completed, Some((next, false)));
        assert!(panel.pending.is_none());
    }

    #[test]
    fn old_scope_and_evaluation_results_cannot_replace_a_new_pause() {
        let (mut panel, events, _requests) = fixture();
        events.send(Event::State(state(9, true))).unwrap();
        events
            .send(Event::Scope {
                revision: 8,
                result: Ok(ScopeData {
                    frames: Vec::new(),
                    variables: Vec::new(),
                    values: vec![("old".into(), "42".into())],
                    snapshot: Ok(None),
                }),
            })
            .unwrap();
        panel.poll();
        assert!(panel.loading_scope);
        assert!(panel.error.is_none());
        assert!(panel.values.is_empty());
        panel.pending = Some(3);
        events
            .send(Event::Completed {
                id: 3,
                revision: Some(8),
                result: Ok(ResultData::Evaluated {
                    output: "old value".into(),
                    streamed: false,
                    truncated: false,
                    variables: Vec::new(),
                    values: vec![("old".into(), "42".into())],
                }),
            })
            .unwrap();
        events
            .send(Event::Scope {
                revision: 9,
                result: Ok(ScopeData {
                    frames: Vec::new(),
                    variables: Vec::new(),
                    values: Vec::new(),
                    snapshot: Ok(None),
                }),
            })
            .unwrap();
        panel.poll();
        assert!(panel.output.is_empty());
        assert!(panel.values.is_empty());
        assert!(panel.pending.is_none());
        assert!(panel.screenshot_ready);
        events.send(Event::State(state(10, false))).unwrap();
        panel.poll();
        assert!(!panel.paused());
        assert!(!panel.screenshot_ready);
    }

    #[test]
    fn paused_stop_uses_the_exact_scope_revision_and_prevents_duplicate_commands() {
        let (mut panel, events, requests) = fixture();
        events.send(Event::State(state(17, true))).unwrap();
        panel.poll();
        assert_eq!(panel.resume(ResumeAction::Quit), Some(1));
        assert!(panel.resume(ResumeAction::Quit).is_none());
        assert!(matches!(
            requests.recv().unwrap(),
            Request::Resume {
                id: 1,
                revision: 17,
                action: ResumeAction::Quit
            }
        ));
    }

    #[test]
    fn nested_pause_unlocks_controls_while_outer_evaluation_is_still_waiting() {
        let (mut panel, events, requests) = fixture();
        events.send(Event::State(state(17, true))).unwrap();
        panel.poll();
        panel.submit(
            Request::Evaluate {
                id: 1,
                revision: 17,
                source: "nested()".into(),
            },
            1,
        );
        panel.next_id = 2;
        assert!(matches!(requests.recv().unwrap(), Request::Evaluate { .. }));
        events.send(Event::State(state(18, true))).unwrap();
        panel.poll();
        assert_eq!(panel.resume(ResumeAction::Continue), Some(2));
        assert!(matches!(
            requests.recv().unwrap(),
            Request::Resume { revision: 18, .. }
        ));
        events
            .send(Event::Completed {
                id: 1,
                revision: Some(17),
                result: Ok(ResultData::Evaluated {
                    output: "obsolete outer result".into(),
                    streamed: false,
                    truncated: false,
                    variables: Vec::new(),
                    values: Vec::new(),
                }),
            })
            .unwrap();
        panel.poll();
        assert_eq!(panel.pending, Some(2));
        assert!(panel.output.is_empty());
    }

    #[test]
    fn internal_worker_filter_keeps_user_files_with_the_same_name_and_prepared_sources() {
        let root = tempfile::tempdir().unwrap();
        let internal = root.path().join("mathlab_worker.m");
        let user = root.path().join("user/mathlab_worker.m");
        let prepared = root.path().join("prepared/generated.m");
        std::fs::create_dir_all(user.parent().unwrap()).unwrap();
        std::fs::create_dir_all(prepared.parent().unwrap()).unwrap();
        for path in [&internal, &user, &prepared] {
            std::fs::write(path, "x = 1;").unwrap();
        }
        let frames = [&internal, &user, &prepared]
            .into_iter()
            .map(|file| DebugFrame {
                file: file.clone(),
                name: "mathlab_worker".into(),
                line: 1,
                column: 0,
            })
            .collect();
        let visible = user_frames(frames, &internal.canonicalize().unwrap());
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].file, user);
        assert_eq!(visible[1].file, prepared);
    }

    #[test]
    fn debug_controller_processes_continue_before_nested_evaluate_response_arrives() {
        use std::{fs, thread, time::Instant};
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().to_owned();
        let publish = |directory: &std::path::Path, revision| {
            let bytes = serde_json::to_vec(&serde_json::json!({"revision": revision, "paused": true, "file": "/tmp/proof.m", "line": 2, "function_name": "proof"})).unwrap();
            fs::write(directory.join("state.pending"), bytes).unwrap();
            fs::rename(
                directory.join("state.pending"),
                directory.join("debug-state.json"),
            )
            .unwrap();
        };
        publish(&directory, 17);
        let server_directory = directory.clone();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(4);
            let mut evaluation = None;
            loop {
                if let Ok(bytes) = fs::read(server_directory.join("debug-command.json")) {
                    let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    fs::remove_file(server_directory.join("debug-command.json")).unwrap();
                    let id = request["id"].as_u64().unwrap();
                    let respond = |id, result| {
                        let temporary = server_directory.join(format!("response-{id}.pending"));
                        fs::write(
                            &temporary,
                            serde_json::to_vec(
                                &serde_json::json!({"id":id,"ok":true,"result":result}),
                            )
                            .unwrap(),
                        )
                        .unwrap();
                        fs::rename(
                            temporary,
                            server_directory.join(format!("debug-response-{id}.json")),
                        )
                        .unwrap();
                    };
                    match request["command"].as_str().unwrap() {
                        "evaluate_stream" => {
                            evaluation = Some(id);
                            publish(&server_directory, 18);
                        }
                        "continue" => {
                            assert_eq!(request["revision"], 18);
                            respond(id, serde_json::Value::Null);
                            respond(
                                evaluation.expect("Continue arrived before evaluate began"),
                                serde_json::json!("nested result"),
                            );
                            return;
                        }
                        "stack" | "workspace" => respond(id, serde_json::json!([])),
                        command => panic!("Unexpected command {command}"),
                    }
                }
                assert!(
                    Instant::now() < deadline,
                    "Debug controller blocked behind the pending evaluation"
                );
                thread::sleep(Duration::from_millis(2));
            }
        });
        let mut panel = DebuggerPanel::new(Debugger::new(&directory), egui::Context::default());
        let deadline = Instant::now() + Duration::from_secs(3);
        while panel
            .state
            .as_ref()
            .is_none_or(|state| state.revision != 17)
            || panel.loading_scope
        {
            panel.poll();
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        panel.submit(
            Request::Evaluate {
                id: 1,
                revision: 17,
                source: "nested()".into(),
            },
            1,
        );
        panel.next_id = 2;
        while panel
            .state
            .as_ref()
            .is_none_or(|state| state.revision != 18)
        {
            panel.poll();
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        assert!(panel.resume(ResumeAction::Continue).is_some());
        server.join().unwrap();
    }

    #[test]
    fn pending_input_blocks_debugger_resume_and_breakpoint_changes_until_finished() {
        let (mut panel, _events, requests) = fixture();
        panel.state = Some(state(1, true));
        panel.set_input_blocked(true);
        for action in [
            ResumeAction::Continue,
            ResumeAction::Step,
            ResumeAction::StepIn,
            ResumeAction::StepOut,
            ResumeAction::Quit,
        ] {
            assert!(panel.resume(action).is_none());
        }
        assert!(!panel.can_modify_breakpoints());
        assert!(panel
            .set_prepared_breakpoint(PathBuf::from("/tmp/input_fixture.m"), 2)
            .is_none());
        assert!(panel
            .clear_breakpoint(PathBuf::from("/tmp/input_fixture.m"), 2)
            .is_none());
        assert!(requests.try_recv().is_err());
        panel.set_input_blocked(false);
        assert!(panel.resume(ResumeAction::Continue).is_some());
        assert!(matches!(
            requests.recv().unwrap(),
            Request::Resume {
                action: ResumeAction::Continue,
                revision: 1,
                ..
            }
        ));
    }

    #[test]
    fn streamed_evaluation_uses_command_window_and_legacy_capture_stays_visible() {
        let (mut panel, events, _requests) = fixture();
        panel.state = Some(state(1, true));
        panel.set_console_streaming(true);
        for (id, streamed, expected) in [(1, true, ""), (2, false, "captured output")] {
            panel.pending = Some(id);
            panel.pending_revision = Some(1);
            events
                .send(Event::Completed {
                    id,
                    revision: Some(1),
                    result: Ok(ResultData::Evaluated {
                        output: "captured output".into(),
                        streamed,
                        truncated: true,
                        variables: Vec::new(),
                        values: Vec::new(),
                    }),
                })
                .unwrap();
            panel.poll();
            assert_eq!(panel.output, expected);
            assert_eq!(panel.output_in_console, streamed);
            assert_eq!(panel.output_truncated, !streamed);
        }
        // Standalone/mock users without a console subscription retain the capture.
        panel.set_console_streaming(false);
        panel.pending = Some(3);
        panel.pending_revision = Some(1);
        events
            .send(Event::Completed {
                id: 3,
                revision: Some(1),
                result: Ok(ResultData::Evaluated {
                    output: "standalone capture".into(),
                    streamed: true,
                    truncated: false,
                    variables: Vec::new(),
                    values: Vec::new(),
                }),
            })
            .unwrap();
        panel.poll();
        assert_eq!(panel.output, "standalone capture");
        assert!(!panel.output_in_console);
    }

    #[test]
    fn pending_user_evaluation_is_distinct_from_a_pending_resume_command() {
        let (mut panel, events, requests) = fixture();
        panel.state = Some(state(1, true));
        assert!(panel.evaluate_source("long_expression()".into()).is_some());
        assert!(panel.evaluation_pending());
        let Request::Evaluate { id, .. } = requests.recv().unwrap() else {
            panic!("expected evaluation");
        };
        events
            .send(Event::Completed {
                id,
                revision: Some(1),
                result: Err("Evaluation interrupted".into()),
            })
            .unwrap();
        panel.poll();
        assert!(!panel.evaluation_pending());
        assert!(panel.resume(ResumeAction::Quit).is_some());
        assert!(!panel.evaluation_pending());
    }
}
