//! Native MATLAB-familiar scientific desktop. Numerical execution belongs to the engine worker.

mod console;
mod debugging;
mod editor_gutter;
mod input;
mod prepared_debug;
pub mod range_paste;
#[cfg(test)]
mod range_transaction_tests;
pub mod range_transactions;
mod search;
mod session;
mod source_navigation;
mod syntax;
mod variable_editor;
mod worker;

use eframe::egui::{self, Align, Color32, FontId, Key, Layout, RichText, Sense, Stroke, Vec2};
use mathlab_engine::{CancellationHandle, Figure, Package, Variable};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use variable_editor::{Action as VariableAction, VariableEditor};
use worker::{Event, Request};

const BLUE: Color32 = Color32::from_rgb(26, 72, 110);
const INK: Color32 = Color32::from_rgb(44, 54, 66);
const MUTED: Color32 = Color32::from_rgb(111, 122, 134);
const BORDER: Color32 = Color32::from_rgb(215, 222, 229);
const PALE: Color32 = Color32::from_rgb(246, 248, 250);
const GREEN: Color32 = Color32::from_rgb(42, 143, 82);
const RED: Color32 = Color32::from_rgb(181, 58, 54);
const DEMO: &str = include_str!("../assets/getting_started.m");

#[derive(Clone, Debug)]
pub struct DesktopOptions {
    pub directory: PathBuf,
    pub executable: Option<PathBuf>,
    pub initial_file: Option<PathBuf>,
    pub run_on_start: bool,
    /// Capture the rendered desktop after startup, execution or an interactive pause, then exit.
    pub screenshot: Option<PathBuf>,
    /// Session JSON location. Defaults to startup directory/.mathlab/session.json.
    pub state_file: Option<PathBuf>,
    /// Open this variable after the startup script has populated the workspace.
    pub initial_variable: Option<String>,
    /// Optional compiled Octave native debugger adapter (.oct).
    pub debugger_bridge: Option<PathBuf>,
    /// Prepare and set a breakpoint in the startup editor buffer before --run.
    pub initial_breakpoint: Option<usize>,
    /// Navigate to this one-based user source frame after startup execution fails.
    pub initial_error_target: Option<usize>,
}

impl Default for DesktopOptions {
    fn default() -> Self {
        Self {
            directory: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            executable: None,
            initial_file: None,
            run_on_start: false,
            screenshot: None,
            state_file: None,
            initial_variable: None,
            debugger_bridge: None,
            initial_breakpoint: None,
            initial_error_target: None,
        }
    }
}

pub fn launch(options: DesktopOptions) -> eframe::Result<()> {
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 920.0])
            .with_min_inner_size([1040.0, 680.0])
            .with_app_id("org.openmathlab.desktop"),
        ..Default::default()
    };
    eframe::run_native(
        "OpenNumLab",
        native,
        Box::new(move |cc| Ok(Box::new(MathLabApp::new(cc, options)))),
    )
}

struct Document {
    id: usize,
    path: Option<PathBuf>,
    name: String,
    source: String,
    saved_source: String,
    cursor: usize,
    selection: Option<(usize, usize)>,
    executed_paths: Vec<PathBuf>,
}

impl Document {
    fn dirty(&self) -> bool {
        self.source != self.saved_source
    }
}

struct ConsoleEntry {
    text: String,
    kind: ConsoleKind,
    sources: Vec<source_navigation::SourceTarget>,
    sources_active: bool,
}

impl ConsoleEntry {
    fn new(text: impl Into<String>, kind: ConsoleKind) -> Self {
        Self {
            text: text.into(),
            kind,
            sources: Vec::new(),
            sources_active: false,
        }
    }
}

enum ConsoleKind {
    Command,
    Output,
    Error,
    Notice,
}

struct FigureView {
    figure: Figure,
    texture: Option<egui::TextureHandle>,
    loading_error: Option<String>,
}

pub struct MathLabApp {
    requests: Sender<Request>,
    events: Receiver<Event>,
    cancellation: Option<CancellationHandle>,
    executable: Option<PathBuf>,
    configured_executable: String,
    directory: PathBuf,
    directory_input: String,
    documents: Vec<Document>,
    next_document_id: usize,
    active_document: usize,
    console: console::ConsoleBuffer,
    console_subscription: Option<mathlab_engine::ConsoleSubscription>,
    console_stream: console::ConsoleStream,
    streaming_console: bool,
    input: input::InputController,
    pending_worker_event: Option<(Event, Option<(u64, u64)>)>,
    command: String,
    history: Vec<String>,
    history_index: Option<usize>,
    command_draft: String,
    workspace: Vec<Variable>,
    inspected: Option<VariableEditor>,
    selected_variable: Option<String>,
    inspector_request: u64,
    range_transactions: range_transactions::RangeTransactions,
    force_restart_confirmation: bool,
    figures: Vec<FigureView>,
    selected_figure: usize,
    figure_zoom: f32,
    show_figures: bool,
    show_settings: bool,
    show_help: bool,
    show_packages: bool,
    packages: Vec<Package>,
    show_folder: bool,
    show_workspace: bool,
    show_history: bool,
    selected_file: Option<PathBuf>,
    ribbon_tab: String,
    busy: bool,
    restarting: bool,
    connected: bool,
    status: String,
    duration: Option<f64>,
    pending_auto_run: bool,
    focus_command: bool,
    console_follow: bool,
    unsaved_close: Option<usize>,
    screenshot: Option<PathBuf>,
    screenshot_ready: bool,
    screenshot_ready_at: Option<std::time::Instant>,
    screenshot_frames: usize,
    screenshot_requested: bool,
    pending_exit: bool,
    exit_confirmed: bool,
    state_file: Option<PathBuf>,
    last_saved_state: Vec<u8>,
    state_saved_at: std::time::Instant,
    initial_variable: Option<String>,
    show_find: bool,
    find_focus: bool,
    find_text: String,
    replacement_text: String,
    match_case: bool,
    whole_word: bool,
    show_go_to_line: bool,
    go_to_line: usize,
    pending_editor_selection: Option<(usize, usize, usize)>,
    configured_debugger_bridge: String,
    debugger: Option<debugging::DebuggerPanel>,
    initial_breakpoint: Option<usize>,
    pending_breakpoint_id: Option<u64>,
    internal_worker: Option<PathBuf>,
    executed_document: Option<usize>,
    initial_error_target: Option<usize>,
    pending_prepared_breakpoint: Option<prepared_debug::PreparedBreakpoint>,
    next_debug_preparation: u64,
}

impl MathLabApp {
    pub fn new(cc: &eframe::CreationContext<'_>, mut options: DesktopOptions) -> Self {
        if options.debugger_bridge.is_none() {
            options.debugger_bridge = std::env::var_os("MATHLAB_DEBUG_BRIDGE").map(PathBuf::from);
        }
        style(&cc.egui_ctx);
        let (requests, events) = worker::spawn(cc.egui_ctx.clone());
        let mut directory = options
            .directory
            .canonicalize()
            .unwrap_or(options.directory);
        let initial_file = options.initial_file.map(|path| {
            if path.is_relative() {
                directory.join(path)
            } else {
                path
            }
        });
        let mut state_file = options.state_file.clone().or_else(|| {
            options
                .screenshot
                .is_none()
                .then(|| directory.join(".mathlab/session.json"))
        });
        let restored = state_file
            .as_ref()
            .map(|path| session::load(path))
            .unwrap_or(Ok(None));
        let mut executable = options.executable.clone();
        if let Ok(Some(session)) = &restored {
            if let Ok(saved_directory) = session.directory.canonicalize() {
                if saved_directory.is_dir() {
                    directory = saved_directory;
                }
            }
            if executable.is_none() {
                executable = session.executable.clone();
            }
        }
        let _ = requests.send(Request::Start {
            executable: executable.clone(),
            directory: directory.clone(),
        });
        let mut app = Self {
            requests,
            events,
            cancellation: None,
            configured_executable: executable
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            executable,
            directory_input: directory.display().to_string(),
            directory,
            documents: vec![Document {
                id: 1,
                path: None,
                name: "getting_started.m".into(),
                source: DEMO.into(),
                saved_source: DEMO.into(),
                cursor: 0,
                selection: None,
                executed_paths: Vec::new(),
            }],
            active_document: 0,
            next_document_id: 2,
            console: console::ConsoleBuffer::new([ConsoleEntry::new(
                "OpenNumLab\nA native Rust desktop powered by GNU Octave.\nOpen a .m file or run the getting started script with F5.",
                ConsoleKind::Notice,
            )]),
            console_subscription: None,
            console_stream: console::ConsoleStream::default(),
            streaming_console: false,
            input: input::InputController::default(),
            pending_worker_event: None,
            command: String::new(),
            history: Vec::new(),
            history_index: None,
            command_draft: String::new(),
            workspace: Vec::new(),
            inspected: None,
            selected_variable: None,
            inspector_request: 0,
            range_transactions: Default::default(),
            force_restart_confirmation: false,
            figures: Vec::new(),
            selected_figure: 0,
            figure_zoom: 1.0,
            show_figures: false,
            show_settings: false,
            show_help: false,
            show_packages: false,
            packages: Vec::new(),
            show_folder: true,
            show_workspace: true,
            show_history: true,
            selected_file: None,
            ribbon_tab: "Home".into(),
            busy: true,
            restarting: false,
            connected: false,
            status: "Starting GNU Octave…".into(),
            duration: None,
            pending_auto_run: options.run_on_start,
            focus_command: false,
            console_follow: true,
            unsaved_close: None,
            screenshot: options.screenshot,
            screenshot_ready: false,
            screenshot_ready_at: None,
            screenshot_frames: 0,
            screenshot_requested: false,
            pending_exit: false,
            exit_confirmed: false,
            state_file: state_file.clone(),
            last_saved_state: Vec::new(),
            state_saved_at: std::time::Instant::now(),
            initial_variable: options.initial_variable,
            show_find: false,
            find_focus: false,
            find_text: String::new(),
            replacement_text: String::new(),
            match_case: false,
            whole_word: false,
            show_go_to_line: false,
            go_to_line: 1,
            pending_editor_selection: None,
            configured_debugger_bridge: options
                .debugger_bridge
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            debugger: None,
            initial_breakpoint: options.initial_breakpoint,
            pending_breakpoint_id: None,
            internal_worker: None,
            executed_document: None,
            initial_error_target: options.initial_error_target,
            pending_prepared_breakpoint: None,
            next_debug_preparation: 1,
        };
        match restored {
            Ok(Some(session)) => {
                app.history = session
                    .history
                    .into_iter()
                    .rev()
                    .take(1000)
                    .collect::<Vec<_>>();
                app.history.reverse();
                app.show_folder = session.show_folder;
                app.show_workspace = session.show_workspace;
                app.show_history = session.show_history;
                app.ribbon_tab = session.ribbon_tab;
                app.figure_zoom = session.figure_zoom.clamp(0.25, 3.0);
                if options.debugger_bridge.is_none() {
                    app.configured_debugger_bridge = session
                        .debugger_bridge
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default();
                }
                if initial_file.is_none() && !session.documents.is_empty() {
                    app.documents = session
                        .documents
                        .into_iter()
                        .enumerate()
                        .map(|(index, saved)| {
                            let mut source = saved.source;
                            let mut saved_source = saved.saved_source;
                            if let Some(path) = &saved.path {
                                if let Ok(on_disk) = fs::read_to_string(path) {
                                    if source == saved_source {
                                        source.clone_from(&on_disk);
                                    }
                                    saved_source = on_disk;
                                }
                            }
                            let cursor = saved.cursor.min(source.chars().count());
                            Document {
                                id: index + 1,
                                path: saved.path,
                                name: saved.name,
                                source,
                                saved_source,
                                cursor,
                                selection: None,
                                executed_paths: Vec::new(),
                            }
                        })
                        .collect();
                    app.active_document = session.active_document.min(app.documents.len() - 1);
                    app.next_document_id = app.documents.len() + 1;
                }
                app.notice("Restored project tabs, unsaved buffers, command history, and desktop preferences.");
            }
            Err(error) => {
                state_file = None;
                app.state_file = state_file;
                app.notice(format!("Could not restore session: {error}. Automatic session saving is disabled to preserve the recovery file."));
            }
            Ok(None) => {}
        }
        if let Some(path) = initial_file {
            app.open_document(path);
            if app.documents.len() > 1 {
                app.documents.remove(0);
                app.active_document = 0;
            }
        }
        app
    }

    fn persist_session(&mut self) {
        let Some(path) = &self.state_file else {
            return;
        };
        let session = session::Session {
            version: 1,
            directory: self.directory.clone(),
            executable: self.executable.clone(),
            documents: self
                .documents
                .iter()
                .map(|document| session::SavedDocument {
                    path: document.path.clone(),
                    name: document.name.clone(),
                    source: document.source.clone(),
                    saved_source: document.saved_source.clone(),
                    cursor: document.cursor,
                })
                .collect(),
            active_document: self.active_document,
            history: self
                .history
                .iter()
                .rev()
                .take(1000)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect(),
            show_folder: self.show_folder,
            show_workspace: self.show_workspace,
            show_history: self.show_history,
            ribbon_tab: self.ribbon_tab.clone(),
            figure_zoom: self.figure_zoom,
            debugger_bridge: (!self.configured_debugger_bridge.trim().is_empty())
                .then(|| PathBuf::from(self.configured_debugger_bridge.trim())),
        };
        match serde_json::to_vec(&session) {
            Ok(data) if data != self.last_saved_state => match session::save(path, &data) {
                Ok(()) => self.last_saved_state = data,
                Err(error) => {
                    self.state_file = None;
                    self.notice(format!("Session recovery could not be saved: {error}"));
                }
            },
            Err(error) => self.notice(format!("Session recovery could not be encoded: {error}")),
            _ => {}
        }
        self.state_saved_at = std::time::Instant::now();
    }

    fn notice(&mut self, text: impl Into<String>) {
        self.console
            .push(ConsoleEntry::new(text, ConsoleKind::Notice));
        self.console_follow = true;
    }

    fn error(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.status = text.clone();
        if !self
            .console
            .last()
            .is_some_and(|entry| matches!(entry.kind, ConsoleKind::Error) && entry.text == text)
        {
            self.console
                .push(ConsoleEntry::new(text, ConsoleKind::Error));
        }
        self.console_follow = true;
    }

    fn clear_source_links(&mut self) {
        for entry in &mut self.console {
            entry.sources_active = false;
        }
    }

    fn execution_error(&mut self, error: mathlab_engine::EngineError) {
        let sources = source_navigation::targets(
            error.stack,
            &self.documents,
            &self.directory,
            self.executed_document,
            self.internal_worker.as_deref(),
        );
        let text = if error.identifier.is_empty() {
            error.message
        } else {
            format!("{}\n{}", error.identifier, error.message)
        };
        self.status = text.lines().last().unwrap_or("Execution failed").into();
        self.console.push(ConsoleEntry {
            text,
            kind: ConsoleKind::Error,
            sources,
            sources_active: true,
        });
        self.console_follow = true;
    }

    fn navigate_source(&mut self, target: source_navigation::SourceTarget) {
        if let Some(index) =
            source_navigation::existing_document(&self.documents, &target, &self.directory)
        {
            self.active_document = index;
        } else if target.file.is_file() {
            self.open_document(target.file.clone());
            if self.documents[self.active_document]
                .path
                .as_ref()
                .is_none_or(|path| {
                    source_navigation::identity(path, &self.directory) != target.file
                })
            {
                return;
            }
        } else {
            self.error(format!(
                "The source buffer for {} is no longer open, and the file does not exist.",
                target.file.display()
            ));
            return;
        }
        let document = &self.documents[self.active_document];
        let (start, end) = source_navigation::line_span(&document.source, target.line);
        let dirty = document.dirty();
        self.select_source_span(start, end);
        self.focus_command = false;
        self.status = if dirty {
            format!(
                "{}:{} — unsaved editor buffer; line refers to the last execution",
                target.file.display(),
                target.line
            )
        } else {
            format!("{}:{}", target.file.display(), target.line)
        };
    }

    fn navigate_initial_error(&mut self) {
        let Some(index) = self.initial_error_target.take() else {
            return;
        };
        let target = self
            .console
            .iter()
            .rev()
            .find(|entry| entry.sources_active)
            .and_then(|entry| entry.sources.get(index.saturating_sub(1)))
            .cloned();
        if let Some(target) = target {
            self.navigate_source(target);
        } else {
            self.notice(format!(
                "Error source frame {index} is not available for navigation."
            ));
        }
    }

    fn poll_console(&mut self, budget: &mut console::DrainBudget, ctx: &egui::Context) {
        while !budget.exhausted() {
            let Some(subscription) = &self.console_subscription else {
                break;
            };
            match subscription.try_recv() {
                Ok(event) => {
                    budget.record(&event);
                    let disconnected = self.console_stream.accept(event, &mut self.console);
                    if let Some(message) =
                        disconnected.filter(|_| self.connected && !self.restarting)
                    {
                        self.disconnected(message);
                    }
                    self.console_follow = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.console_stream.finish(&mut self.console);
                    self.console_subscription = None;
                    if self.connected && !self.restarting {
                        self.disconnected("The console transport stopped unexpectedly. Reconnect to start a fresh session.".into());
                    }
                    break;
                }
            }
        }
        if budget.exhausted() {
            ctx.request_repaint();
        }
    }

    fn poll_events(&mut self, ctx: &egui::Context) {
        let mut console_budget = console::DrainBudget::default();
        // A stream flood cannot make a frame unbounded, and a perpetual background
        // writer cannot postpone a completed foreground request indefinitely.
        for _ in 0..64 {
            self.poll_console(&mut console_budget, ctx);
            if self.pending_worker_event.is_none() {
                let event = match self.events.try_recv() {
                    Ok(event) => event,
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        // A stopped Rust worker cannot answer Restart/Start on its old channel.
                        (self.requests, self.events) = worker::spawn(ctx.clone());
                        self.restarting = false;
                        self.disconnected("The engine worker stopped unexpectedly. Reconnect to start a fresh session.".into());
                        break;
                    }
                };
                let barrier = if self.streaming_console {
                    match &event {
                        Event::Executed(result) if result.console_end_sequence > 0 => {
                            Some((result.console_generation, result.console_end_sequence))
                        }
                        _ => self
                            .console_subscription
                            .as_ref()
                            .map(|stream| stream.latest_sequence()),
                    }
                } else {
                    None
                };
                self.pending_worker_event = Some((event, barrier));
            }
            if self
                .pending_worker_event
                .as_ref()
                .and_then(|(_, barrier)| *barrier)
                .is_some_and(|barrier| !self.console_stream.reached(barrier))
            {
                ctx.request_repaint();
                break;
            }
            let (event, _) = self.pending_worker_event.take().unwrap();
            match event {
                Event::ConsoleConnected {
                    subscription,
                    generation,
                } => {
                    let initial = subscription.initial_sequence();
                    debug_assert_eq!(initial.0, generation);
                    self.console_stream.connect(initial, &mut self.console);
                    self.console_subscription = Some(subscription);
                    self.streaming_console = true;
                }
                Event::Ready {
                    executable,
                    cancel,
                    internal_worker,
                    input,
                } => {
                    self.input.install(input, self.console_stream.progress().0);
                    self.clear_source_links();
                    self.internal_worker = Some(internal_worker);
                    self.workspace.clear();
                    self.figures.clear();
                    self.packages.clear();
                    self.inspected = None;
                    self.range_transactions.restarted();
                    self.selected_variable = None;
                    self.debugger = None;
                    self.pending_prepared_breakpoint = None;
                    self.pending_breakpoint_id = None;
                    self.configured_executable = executable.display().to_string();
                    self.executable = Some(executable);
                    self.cancellation = Some(cancel);
                    self.connected = true;
                    self.busy = false;
                    self.restarting = false;
                    self.status = "Ready".into();
                    self.notice("GNU Octave connected. Commands and scripts share one workspace.");
                    if !self.configured_debugger_bridge.trim().is_empty() {
                        self.enable_debugger();
                    } else if self.initial_breakpoint.is_some() {
                        self.error("A startup breakpoint requires --debugger BRIDGE.oct or MATHLAB_DEBUG_BRIDGE. Select the adapter in Engine Settings.");
                        self.show_settings = true;
                        self.screenshot_ready = true;
                        self.screenshot_ready_at = Some(std::time::Instant::now());
                    } else if self.pending_auto_run {
                        self.pending_auto_run = false;
                        self.run_document();
                    } else {
                        self.screenshot_ready = true;
                        self.screenshot_ready_at = Some(std::time::Instant::now());
                    }
                }
                Event::Executed(result) => {
                    if self.restarting {
                        continue;
                    }
                    self.busy = false;
                    self.input.clear();
                    let interrupted = result.error.as_ref().is_some_and(|error| {
                        error.identifier == "MathLab:Interrupted"
                            || error.identifier == "MathLab:DebugQuit"
                    });
                    self.duration = Some(result.duration_ms);
                    if !self.streaming_console && !result.output.is_empty() {
                        self.console.output(&result.output);
                    }
                    self.workspace = result.workspace;
                    self.refresh_inspector();
                    self.directory = result.working_directory;
                    self.directory_input = self.directory.display().to_string();
                    for warning in result.figure_warnings {
                        self.notice(warning);
                    }
                    let previous_figure_count = self.figures.len();
                    if !interrupted {
                        self.figures = result
                            .figures
                            .into_iter()
                            .map(|figure| FigureView {
                                figure,
                                texture: None,
                                loading_error: None,
                            })
                            .collect();
                    }
                    self.selected_figure = self
                        .selected_figure
                        .min(self.figures.len().saturating_sub(1));
                    if self.figures.len() > previous_figure_count {
                        self.show_figures = true;
                    } else if self.figures.is_empty() {
                        self.show_figures = false;
                    }
                    if let Some(error) = result.error {
                        if interrupted {
                            self.status = "Interrupted — workspace preserved".into();
                            self.notice(error.message);
                        } else {
                            self.execution_error(error);
                        }
                    } else {
                        self.status = "Ready".into();
                    }
                    self.console_follow = true;
                    self.focus_command = true;
                    if self
                        .console
                        .last()
                        .is_some_and(|entry| entry.sources_active)
                    {
                        self.navigate_initial_error();
                    }
                    self.screenshot_ready = true;
                    self.screenshot_ready_at = Some(std::time::Instant::now());
                    if let Some(name) = self.initial_variable.take() {
                        if let Some(variable) = self
                            .workspace
                            .iter()
                            .find(|variable| variable.name == name)
                            .cloned()
                        {
                            self.selected_variable = Some(name);
                            self.open_variable(variable);
                            self.screenshot_ready = false;
                        } else {
                            self.notice(format!(
                                "Variable {name} was not created by the startup script."
                            ));
                        }
                    }
                }
                Event::Directory(directory) => {
                    if self.restarting {
                        continue;
                    }
                    self.range_transactions.invalidate_context();
                    self.directory_input = directory.display().to_string();
                    self.directory = directory;
                    self.busy = false;
                    self.status = "Ready".into();
                }
                Event::Restarted {
                    debugger,
                    internal_worker,
                    input,
                } => {
                    self.input.install(input, self.console_stream.progress().0);
                    self.clear_source_links();
                    self.pending_prepared_breakpoint = None;
                    self.pending_breakpoint_id = None;
                    self.internal_worker = Some(internal_worker);
                    self.busy = false;
                    self.restarting = false;
                    self.workspace.clear();
                    self.figures.clear();
                    self.packages.clear();
                    self.inspected = None;
                    self.selected_variable = None;
                    self.range_transactions.restarted();
                    self.status = "Ready".into();
                    self.notice("Engine restarted. Workspace variables and figures were cleared.");
                    self.debugger =
                        debugger.map(|handle| debugging::DebuggerPanel::new(handle, ctx.clone()));
                }
                Event::DebuggerReady(handle) => {
                    if self.restarting {
                        continue;
                    }
                    self.busy = false;
                    self.refresh_inspector();
                    self.debugger = Some(debugging::DebuggerPanel::new(handle, ctx.clone()));
                    self.status = "Debugger enabled".into();
                    self.notice("Native debugger enabled. Set a breakpoint in a source file or script buffer before running.");
                    if let Some(line) = self.initial_breakpoint.take() {
                        let path = prepared_debug::original_path(
                            &self.documents[self.active_document],
                            &self.directory,
                        );
                        self.prepare_breakpoint(path, line, self.pending_auto_run);
                    } else if self.pending_auto_run {
                        self.pending_auto_run = false;
                        self.run_document();
                    } else {
                        self.screenshot_ready = true;
                        self.screenshot_ready_at = Some(std::time::Instant::now());
                    }
                }
                Event::DebuggerUnavailable { error, debugger } => {
                    if self.restarting {
                        continue;
                    }
                    self.busy = false;
                    self.debugger =
                        debugger.map(|handle| debugging::DebuggerPanel::new(handle, ctx.clone()));
                    self.refresh_inspector();
                    self.pending_breakpoint_id = None;
                    self.pending_prepared_breakpoint = None;
                    self.error(format!("Debugger could not be enabled: {error}\nBuild native/mathlab_debug_bridge.oct with scripts/build-debugger.sh, then select that file in Engine Settings."));
                    self.show_settings = true;
                    self.screenshot_ready = true;
                    self.screenshot_ready_at = Some(std::time::Instant::now());
                }
                Event::DebugSourcePrepared {
                    request_id,
                    document_id,
                    result,
                } => {
                    if self.restarting
                        || self
                            .pending_prepared_breakpoint
                            .as_ref()
                            .is_none_or(|pending| {
                                pending.token != request_id || pending.document_id != document_id
                            })
                    {
                        continue;
                    }
                    self.busy = false;
                    if !self
                        .pending_prepared_breakpoint
                        .as_ref()
                        .unwrap()
                        .current(&self.documents, &self.directory)
                    {
                        self.preparation_failed(
                            "The editor buffer changed while preparing. Set the breakpoint again.",
                        );
                        continue;
                    }
                    match result {
                        Ok(source) => {
                            let line = self.pending_prepared_breakpoint.as_ref().unwrap().line;
                            self.pending_breakpoint_id = self.debugger.as_mut().and_then(|panel| {
                                if source.prepared {
                                    panel.set_prepared_breakpoint(source.original_path, line)
                                } else {
                                    panel.set_breakpoint(source.original_path, line)
                                }
                            });
                            if self.pending_breakpoint_id.is_some() {
                                self.busy = true;
                                self.status = "Installing breakpoint…".into();
                            } else {
                                self.preparation_failed("The debugger is unavailable or another debugger command is pending.");
                            }
                        }
                        Err(mathlab_engine::Error::Worker(error)) => {
                            self.pending_prepared_breakpoint = None;
                            self.pending_auto_run = false;
                            self.execution_error(error);
                            self.navigate_initial_error();
                            self.screenshot_ready = true;
                            self.screenshot_ready_at = Some(std::time::Instant::now());
                        }
                        Err(error) => self.preparation_failed(error.to_string()),
                    }
                }
                Event::Packages { packages, output } => {
                    if self.restarting {
                        continue;
                    }
                    self.packages = packages;
                    self.busy = false;
                    self.status = "Ready".into();
                    self.refresh_inspector();
                    if !self.streaming_console && !output.trim().is_empty() {
                        self.notice(output);
                    }
                }
                Event::Failed(error) => {
                    self.input.clear();
                    if self.restarting {
                        self.notice(error);
                    } else {
                        self.busy = false;
                        self.error(error);
                        self.refresh_inspector();
                        self.screenshot_ready = true;
                        self.screenshot_ready_at = Some(std::time::Instant::now());
                    }
                }
                Event::SourceFailed(error) => {
                    self.input.clear();
                    if self.restarting {
                        self.notice(error.message);
                    } else {
                        self.busy = false;
                        self.execution_error(error);
                        self.navigate_initial_error();
                        self.refresh_inspector();
                        self.screenshot_ready = true;
                        self.screenshot_ready_at = Some(std::time::Instant::now());
                    }
                }
                Event::Disconnected(error) => {
                    if self.restarting {
                        self.notice(error);
                        continue;
                    }
                    self.disconnected(error);
                }
                Event::Unavailable(error) => {
                    self.range_transactions.disconnected();
                    self.input.disconnect();
                    self.busy = false;
                    self.restarting = false;
                    self.connected = false;
                    self.cancellation = None;
                    self.debugger = None;
                    self.pending_breakpoint_id = None;
                    self.pending_prepared_breakpoint = None;
                    self.invalidate_inspector();
                    if let Some(editor) = &mut self.inspected {
                        editor.pending = false;
                        editor.stale = true;
                        editor.error = Some("The engine is unavailable.".into());
                    }
                    self.error(error);
                    self.show_settings = true;
                    self.screenshot_ready = true;
                    self.screenshot_ready_at = Some(std::time::Instant::now());
                }
                Event::RangeTransactionFinished { envelope, outcome } => {
                    self.finish_range_transaction(*envelope, *outcome);
                }
                Event::VariableRead { request_id, result } => {
                    if self.connected && !self.busy && !self.restarting {
                        if let Some(editor) = &mut self.inspected {
                            // Only accepted active reads can retire history or
                            // observe a page. A stale preview cannot replace either.
                            if editor.pending && editor.request_id == request_id {
                                let preview = matches!(
                                    &result,
                                    Ok(variable_editor::VariablePage::Preview(_))
                                );
                                editor.accept(request_id, result);
                                if !editor.stale {
                                    if preview {
                                        self.range_transactions.invalidate_context();
                                    }
                                    if let Some(page) = &editor.typed_page {
                                        self.range_transactions
                                            .observe_page(editor.request_id, page);
                                    }
                                }
                            }
                        }
                    }
                    self.screenshot_ready = true;
                    self.screenshot_ready_at = Some(std::time::Instant::now());
                }
                Event::VariableChanged { request_id, result } => {
                    if !self.connected || self.busy || self.restarting {
                        continue;
                    }
                    if self
                        .inspected
                        .as_ref()
                        .is_some_and(|editor| editor.request_id == request_id)
                    {
                        match result {
                            Ok(receipt) => {
                                self.workspace = receipt.workspace;
                                self.refresh_inspector_with_token(Some(receipt.token));
                                self.status = "Variable updated".into();
                            }
                            Err(error) => {
                                if let Some(editor) = &mut self.inspected {
                                    editor.accept(request_id, Err(error));
                                }
                            }
                        }
                    } else if let Ok(receipt) = result {
                        self.workspace = receipt.workspace;
                    }
                }
            }
            ctx.request_repaint();
        }
    }

    fn poll_input(&mut self) {
        if self.connected && self.busy && !self.restarting {
            if self.input.poll(self.console_stream.progress()) {
                self.focus_command = true;
                if self.screenshot.is_some() && !self.screenshot_requested {
                    self.screenshot_ready = true;
                    self.screenshot_ready_at = Some(std::time::Instant::now());
                }
            }
        } else {
            self.input.clear();
        }
        if let Some(panel) = &mut self.debugger {
            panel.set_input_blocked(self.input.active());
            panel.set_console_streaming(
                self.streaming_console && self.console_subscription.is_some(),
            );
        }
    }

    fn disconnected(&mut self, error: String) {
        self.range_transactions.disconnected();
        self.input.disconnect();
        self.busy = false;
        self.connected = false;
        self.debugger = None;
        self.cancellation = None;
        self.pending_breakpoint_id = None;
        self.pending_prepared_breakpoint = None;
        self.pending_auto_run = false;
        self.invalidate_inspector();
        if let Some(editor) = &mut self.inspected {
            editor.pending = false;
            editor.stale = true;
            editor.error = Some(
                "The engine disconnected. Reconnect before reading or editing variables.".into(),
            );
        }
        self.error(error);
        self.show_settings = true;
        self.screenshot_ready = true;
        self.screenshot_ready_at = Some(std::time::Instant::now());
    }

    fn execute(&mut self, code: String, label: String) {
        if self.input.active() || self.busy || !self.connected || code.trim().is_empty() {
            return;
        }
        self.executed_document = None;
        self.dispatch(Request::Execute(code), label);
    }

    fn dispatch(&mut self, request: Request, label: String) {
        if !self.connected || self.busy || self.input.active() || self.range_transactions.pending()
        {
            return;
        }
        self.range_transactions.invalidate_context();
        if matches!(
            &request,
            Request::Execute(_) | Request::RunFile(_) | Request::RunBuffer { .. }
        ) {
            self.clear_source_links();
        }
        self.console
            .push(ConsoleEntry::new(label, ConsoleKind::Command));
        self.busy = true;
        self.invalidate_inspector();
        self.duration = None;
        self.status = "Running…".into();
        self.console_follow = true;
        if self.requests.send(request).is_err() {
            self.busy = false;
            self.connected = false;
            self.error("The engine worker stopped unexpectedly.");
        }
    }

    fn submit_command(&mut self) {
        if self.input.active() {
            self.input.submit(false);
            return;
        }
        let command = self.command.trim().to_owned();
        if command.is_empty() || self.busy || !self.connected {
            return;
        }
        if self.history.last() != Some(&command) {
            self.history.push(command.clone());
        }
        self.command.clear();
        self.history_index = None;
        self.command_draft.clear();
        self.execute(command.clone(), command);
    }

    fn run_or_continue(&mut self) {
        if self.input.active() {
            return;
        }
        if let Some(panel) = self.debugger.as_mut().filter(|panel| panel.paused()) {
            panel.resume(mathlab_debugger::ResumeAction::Continue);
        } else {
            self.run_document();
        }
    }

    fn run_document(&mut self) {
        if !self.connected || self.busy || self.input.active() {
            return;
        }
        self.clear_source_links();
        let path =
            prepared_debug::original_path(&self.documents[self.active_document], &self.directory);
        if let Some(panel) = &self.debugger {
            let mut lines = panel.breakpoint_lines(&path).into_iter();
            if let Some(line) = lines.next() {
                if !panel.can_modify_breakpoints() {
                    self.notice("Wait for the pending debugger command before running.");
                    return;
                }
                self.prepare_breakpoint(path, line, true);
                if let Some(pending) = &mut self.pending_prepared_breakpoint {
                    pending.remaining_lines = lines.collect();
                }
                return;
            }
        }
        self.run_prepared_document();
    }

    fn run_prepared_document(&mut self) {
        if !self.connected || self.busy || self.input.active() {
            return;
        }
        let document = &mut self.documents[self.active_document];
        let original_path = document
            .path
            .clone()
            .unwrap_or_else(|| self.directory.join(&document.name));
        let original_path = source_navigation::identity(&original_path, &self.directory);
        if !document.executed_paths.contains(&original_path) {
            document.executed_paths.push(original_path);
        }
        self.executed_document = Some(document.id);
        let doc = &self.documents[self.active_document];
        if let Some(path) = &doc.path {
            if !doc.dirty() {
                self.dispatch(Request::RunFile(path.clone()), format!("run {}", doc.name));
                return;
            }
        }
        let code = doc.source.clone();
        let label = format!(
            "run {}{}",
            doc.name,
            if doc.dirty() { " (editor buffer)" } else { "" }
        );
        let path = doc
            .path
            .clone()
            .unwrap_or_else(|| self.directory.join(&doc.name));
        self.dispatch(Request::RunBuffer { source: code, path }, label);
    }

    fn run_section(&mut self) {
        let doc = &self.documents[self.active_document];
        let code = syntax::section_at(&doc.source, doc.cursor).to_owned();
        let label = format!("run section — {}", doc.name);
        self.execute(code, label);
    }

    fn run_selection(&mut self) {
        let doc = &self.documents[self.active_document];
        if let Some((start, end)) = doc.selection {
            let code: String = doc.source.chars().skip(start).take(end - start).collect();
            self.execute(code, "evaluate selection".into());
        }
    }

    fn stop(&mut self) {
        if !self.busy || !self.connected || self.restarting {
            return;
        }
        if self.input.active()
            || self
                .debugger
                .as_ref()
                .is_some_and(|panel| panel.evaluation_pending())
        {
            if !cfg!(unix) {
                self.force_restart_confirmation = true;
                return;
            }
            if let Some(cancel) = &self.cancellation {
                match cancel.interrupt() {
                    Ok(()) => {
                        self.input.stopping();
                        self.status = "Interrupting…".into();
                    }
                    Err(error) => self.error(error.to_string()),
                }
            }
            return;
        }
        if let Some(pending) = &mut self.pending_prepared_breakpoint {
            pending.run_after = false;
            self.pending_auto_run = false;
            self.status = "Preparing breakpoint; startup run canceled".into();
            return;
        }
        if let Some(panel) = &mut self.debugger {
            if panel.paused() {
                if panel.resume(mathlab_debugger::ResumeAction::Quit).is_some() {
                    self.status = "Stopping debugging…".into();
                } else {
                    self.notice("A debugger command is pending. Wait for it to finish, or use Force Terminate and Restart.");
                }
                return;
            }
        }
        if !cfg!(unix) {
            self.force_restart_confirmation = true;
            return;
        }
        if let Some(cancel) = &self.cancellation {
            match cancel.interrupt() {
                Ok(()) => {
                    self.status = "Interrupting…".into();
                    self.notice("Interrupt requested. The engine will return with workspace variables preserved.");
                }
                Err(error) => self.error(error.to_string()),
            }
        }
    }

    fn force_restart(&mut self) {
        self.clear_source_links();
        if let Some(cancel) = &self.cancellation {
            if let Err(error) = cancel.terminate() {
                self.error(error.to_string());
                return;
            }
        }
        self.range_transactions.invalidate_context();
        self.input.disconnect();
        self.invalidate_inspector();
        self.force_restart_confirmation = false;
        self.debugger = None;
        self.pending_breakpoint_id = None;
        self.pending_prepared_breakpoint = None;
        self.pending_auto_run = false;
        self.busy = true;
        self.restarting = true;
        self.status = "Terminating and restarting…".into();
        self.notice("Force restart requested. This session's workspace will be cleared.");
        let _ = self.requests.send(Request::Restart);
    }

    fn open_variable(&mut self, variable: Variable) {
        self.range_transactions.clear_page();
        self.inspected = Some(VariableEditor::new(variable));
        self.read_variable();
    }

    /// Controller seam only: no current widget, shortcut or clipboard event
    /// calls these transaction methods. Interaction design remains unenabled.
    pub fn submit_range_paste(
        &mut self,
        paste: range_paste::ParsedRangePaste,
    ) -> Result<(), String> {
        let request_id = self.range_request_id()?;
        let envelope = self.range_transactions.begin_paste(request_id, paste)?;
        self.queue_range_transaction(envelope)
    }

    pub fn undo_range_edit(&mut self) -> Result<(), String> {
        self.submit_range_history(mathlab_engine::value_edit::EditOperation::Undo)
    }

    pub fn redo_range_edit(&mut self) -> Result<(), String> {
        self.submit_range_history(mathlab_engine::value_edit::EditOperation::Redo)
    }

    fn submit_range_history(
        &mut self,
        operation: mathlab_engine::value_edit::EditOperation,
    ) -> Result<(), String> {
        let request_id = self.range_request_id()?;
        let envelope = self
            .range_transactions
            .begin_history(request_id, operation)?;
        self.queue_range_transaction(envelope)
    }

    /// Explicit reset requires an accepted current page, even in quarantine.
    /// It never retries the operation which caused uncertainty.
    pub fn reset_range_edit_history(&mut self) -> Result<(), String> {
        let request_id = self.range_request_id()?;
        let envelope = self.range_transactions.begin_reset(request_id)?;
        self.queue_range_transaction(envelope)
    }

    pub fn range_edit_state(&self) -> &range_transactions::RangeTransactions {
        &self.range_transactions
    }

    fn range_request_id(&self) -> Result<u64, String> {
        if !self.connected || self.busy || self.restarting || self.input.active() {
            return Err("Wait for a connected idle engine before this transaction.".into());
        }
        let editor = self
            .inspected
            .as_ref()
            .ok_or_else(|| "Open and refresh a typed variable page first.".to_owned())?;
        let page = editor
            .typed_page
            .as_ref()
            .ok_or_else(|| "This view has no accepted typed page.".to_owned())?;
        if editor.pending
            || editor.stale
            || self.range_transactions.accepted_page()
                != Some(&range_transactions::PageIdentity::accepted(
                    editor.request_id,
                    page,
                ))
            || u64::try_from(editor.row_start).ok() != Some(page.selection.row_start.get())
            || u64::try_from(editor.column_start).ok() != Some(page.selection.column_start.get())
            || !editor.higher_indices.iter().copied().eq(page
                .selection
                .higher_indices
                .iter()
                .map(|index| index.get()))
        {
            return Err(
                "The accepted variable page changed. Refresh before this transaction.".into(),
            );
        }
        self.inspector_request
            .checked_add(1)
            .ok_or_else(|| "Inspector request counter exhausted; restart the application.".into())
    }

    fn queue_range_transaction(
        &mut self,
        envelope: range_transactions::TransactionEnvelope,
    ) -> Result<(), String> {
        self.inspector_request = envelope.request_id;
        if let Some(editor) = &mut self.inspected {
            editor.begin(envelope.request_id);
        }
        self.busy = true;
        self.status = "Updating variable…".into();
        if self
            .requests
            .send(Request::RangeTransaction(Box::new(envelope.clone())))
            .is_err()
        {
            self.range_transactions.finish(
                &envelope,
                range_transactions::TransactionOutcome::Failed(
                    range_transactions::TransactionFailure {
                        message: "The engine worker stopped before accepting the transaction."
                            .into(),
                        uncertain: false,
                        disconnected: true,
                    },
                ),
            );
            self.disconnected("The engine worker stopped before accepting the transaction.".into());
            return Err("The engine worker stopped before accepting the transaction.".into());
        }
        Ok(())
    }

    fn finish_range_transaction(
        &mut self,
        envelope: range_transactions::TransactionEnvelope,
        outcome: range_transactions::TransactionOutcome,
    ) {
        use range_transactions::TransactionOutcome;
        if self.restarting {
            self.range_transactions.invalidate_context();
            return;
        }
        let Some(outcome) = self.range_transactions.finish(&envelope, outcome) else {
            return;
        };
        self.busy = false;
        match outcome {
            TransactionOutcome::Applied {
                receipt,
                refresh: Ok(refresh),
            } => {
                let metadata = refresh
                    .snapshot
                    .variables
                    .iter()
                    .find(|binding| binding.token == receipt.token)
                    .map(|binding| binding.variable.clone());
                self.workspace = refresh
                    .snapshot
                    .variables
                    .into_iter()
                    .map(|binding| binding.variable)
                    .collect();
                if let Some(editor) = &mut self.inspected {
                    let same_view = editor.request_id == envelope.request_id
                        && editor.variable.name == receipt.token.name
                        && u64::try_from(editor.row_start).ok()
                            == Some(envelope.page.window.row_start.get())
                        && u64::try_from(editor.column_start).ok()
                            == Some(envelope.page.window.column_start.get())
                        && editor.higher_indices.iter().copied().eq(envelope
                            .page
                            .window
                            .higher_indices
                            .iter()
                            .map(|index| index.get()))
                        && editor.typed_page.as_ref().is_some_and(|before| {
                            range_transactions::PageIdentity::accepted(
                                envelope.page.view_id,
                                before,
                            ) == envelope.page
                        });
                    if same_view {
                        editor.invalidate(metadata.clone());
                        editor.begin_read(envelope.request_id, Some(receipt.token));
                        editor.accept(
                            envelope.request_id,
                            Ok(variable_editor::VariablePage::Typed {
                                page: Box::new(refresh.page),
                                metadata,
                            }),
                        );
                        if !editor.stale {
                            if let Some(page) = &editor.typed_page {
                                self.range_transactions
                                    .observe_page(editor.request_id, page);
                            }
                        }
                    } else if editor.request_id == envelope.request_id {
                        // The operation settled, but the user changed the view's
                        // selection/path. Keep the receipt/history, never its old page.
                        editor.pending = false;
                        editor.stale = true;
                        editor.token = None;
                        editor.error = Some(
                            "The edit committed. Refresh the newly selected view before editing."
                                .into(),
                        );
                    }
                }
                self.status = "Variable transaction completed".into();
            }
            TransactionOutcome::Applied {
                refresh: Err(failure),
                ..
            } => {
                let message = format!("The edit committed, but its refresh failed: {}. Refresh explicitly; do not repeat the edit.", failure.message);
                if let Some(editor) = &mut self.inspected {
                    editor.accept(envelope.request_id, Err(message.clone()));
                }
                self.status = "Edit committed; refresh required".into();
                if failure.disconnected {
                    self.disconnected(message);
                } else {
                    self.error(message);
                }
            }
            TransactionOutcome::Reset(snapshot) => {
                self.workspace = snapshot
                    .variables
                    .into_iter()
                    .map(|binding| binding.variable)
                    .collect();
                if let Some(editor) = &mut self.inspected {
                    let metadata = self
                        .workspace
                        .iter()
                        .find(|variable| variable.name == editor.variable.name)
                        .cloned();
                    editor.invalidate(metadata);
                }
                self.status = "Edit history reset; refresh values before editing".into();
            }
            TransactionOutcome::Failed(failure) => {
                if let Some(editor) = &mut self.inspected {
                    editor.accept(envelope.request_id, Err(failure.message.clone()));
                }
                if failure.disconnected {
                    self.disconnected(failure.message);
                } else {
                    self.error(failure.message);
                }
            }
        }
    }

    fn read_variable(&mut self) {
        let token = self
            .inspected
            .as_ref()
            .and_then(|editor| editor.token.clone());
        self.read_variable_with_token(token);
    }

    fn read_variable_with_token(&mut self, token: Option<mathlab_engine::BindingToken>) {
        if self.busy || !self.connected || self.range_transactions.pending() {
            return;
        }
        self.range_transactions.clear_page();
        if let Some(editor) = &mut self.inspected {
            self.inspector_request = self.inspector_request.wrapping_add(1);
            editor.stale = true;
            editor.begin_read(self.inspector_request, token.clone());
            let request = Request::ReadVariable {
                request_id: self.inspector_request,
                name: editor.variable.name.clone(),
                token,
                row_start: editor.row_start,
                column_start: editor.column_start,
                higher_indices: editor.higher_indices.clone(),
            };
            if self.requests.send(request).is_err() {
                editor.pending = false;
                editor.error = Some("The engine worker stopped unexpectedly.".into());
            }
        }
    }

    fn refresh_inspector(&mut self) {
        self.refresh_inspector_with_token(None);
    }

    fn refresh_inspector_with_token(&mut self, token: Option<mathlab_engine::BindingToken>) {
        if let Some(editor) = &mut self.inspected {
            let variable = self
                .workspace
                .iter()
                .find(|variable| variable.name == editor.variable.name)
                .cloned();
            let exists = variable.is_some();
            editor.invalidate(variable);
            if exists {
                self.read_variable_with_token(token);
            }
        }
        if self
            .selected_variable
            .as_ref()
            .is_some_and(|name| !self.workspace.iter().any(|variable| &variable.name == name))
        {
            self.selected_variable = None;
        }
    }

    fn invalidate_inspector(&mut self) {
        self.range_transactions.clear_page();
        self.inspector_request = self.inspector_request.wrapping_add(1);
        if let Some(editor) = &mut self.inspected {
            editor.request_id = self.inspector_request;
            editor.invalidate(Some(editor.variable.clone()));
        }
    }

    fn variable_editor(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.inspected else {
            return;
        };
        editor.writes_blocked = self.range_transactions.writes_blocked();
        let (open, action) = editor.show(ctx, self.connected && !self.busy);
        if !open {
            self.range_transactions.clear_page();
            self.inspected = None;
            return;
        }
        match action {
            Some(VariableAction::Read) => self.read_variable(),
            Some(VariableAction::Refresh) => {
                self.invalidate_inspector();
                self.read_variable();
            }
            Some(VariableAction::Write {
                token,
                row,
                column,
                literal,
            }) => {
                if self.range_transactions.writes_blocked() {
                    return;
                }
                self.range_transactions.invalidate_context();
                self.inspector_request = self.inspector_request.wrapping_add(1);
                if let Some(editor) = &mut self.inspected {
                    editor.begin(self.inspector_request);
                    if self
                        .requests
                        .send(Request::WriteVariable {
                            request_id: self.inspector_request,
                            token,
                            row,
                            column,
                            literal,
                        })
                        .is_err()
                    {
                        editor.pending = false;
                        editor.error = Some("The engine worker stopped unexpectedly.".into());
                    }
                }
            }
            Some(VariableAction::Display(name)) => self.execute(name.clone(), name),
            None => {}
        }
    }

    fn restart(&mut self) {
        if self.busy {
            return;
        }
        self.range_transactions.invalidate_context();
        self.input.disconnect();
        self.clear_source_links();
        self.invalidate_inspector();
        self.busy = true;
        self.debugger = None;
        self.restarting = true;
        self.status = "Restarting…".into();
        let _ = self.requests.send(Request::Restart);
    }

    fn enable_debugger(&mut self) {
        if !self.connected || self.busy {
            return;
        }
        let path = PathBuf::from(self.configured_debugger_bridge.trim());
        self.dispatch(
            Request::EnableDebugger(path),
            "enable native debugger".into(),
        );
        self.status = "Enabling debugger…".into();
    }

    fn prepare_breakpoint(&mut self, file: PathBuf, line: usize, run_after: bool) {
        if !self.connected || self.busy || self.debugger.is_none() {
            return;
        }
        let file = source_navigation::identity(&file, &self.directory);
        let document_id = if prepared_debug::original_path(
            &self.documents[self.active_document],
            &self.directory,
        ) == file
        {
            Some(self.active_document)
        } else {
            self.documents.iter().position(|document| {
                prepared_debug::original_path(document, &self.directory) == file
            })
        };
        let (document_id, source_snapshot, source) = if let Some(index) = document_id {
            let document = &mut self.documents[index];
            if !document.executed_paths.contains(&file) {
                document.executed_paths.push(file.clone());
            }
            let source =
                (document.path.is_none() || document.dirty()).then(|| document.source.clone());
            (Some(document.id), Some(document.source.clone()), source)
        } else {
            (None, None, None)
        };
        let token = self.next_debug_preparation;
        self.next_debug_preparation = self.next_debug_preparation.wrapping_add(1);
        self.executed_document = document_id;
        self.pending_prepared_breakpoint = Some(prepared_debug::PreparedBreakpoint {
            token,
            document_id,
            original_path: file.clone(),
            line,
            source_snapshot,
            run_after,
            remaining_lines: Vec::new(),
        });
        self.busy = true;
        self.status = "Preparing debugger source…".into();
        if self
            .requests
            .send(Request::PrepareDebug {
                request_id: token,
                document_id,
                path: file,
                source,
            })
            .is_err()
        {
            self.preparation_failed("The engine worker stopped unexpectedly.");
        }
    }

    fn preparation_failed(&mut self, message: impl Into<String>) {
        self.pending_prepared_breakpoint = None;
        self.pending_breakpoint_id = None;
        self.pending_auto_run = false;
        self.busy = false;
        self.error(message);
        self.screenshot_ready = true;
        self.screenshot_ready_at = Some(std::time::Instant::now());
    }

    fn toggle_editor_breakpoint(&mut self, line: usize) {
        if self.busy || !self.connected {
            return;
        }
        let path =
            prepared_debug::original_path(&self.documents[self.active_document], &self.directory);
        let Some(panel) = &mut self.debugger else {
            self.show_settings = true;
            self.notice("Enable the native debugger in Engine Settings to set breakpoints.");
            return;
        };
        if !panel.can_modify_breakpoints() {
            return;
        }
        if panel.breakpoint_lines(&path).contains(&line) {
            panel.clear_breakpoint(path, line);
        } else {
            self.prepare_breakpoint(path, line, false);
        }
    }

    fn debugger_updates(&mut self, ctx: &egui::Context) {
        let Some(panel) = &mut self.debugger else {
            return;
        };
        let pause = panel.poll();
        let screenshot_ready = panel.screenshot_ready;
        let breakpoint_completed = panel.breakpoint_completed.take();
        if let Some((id, success)) = breakpoint_completed {
            if self.pending_breakpoint_id == Some(id) {
                self.pending_breakpoint_id = None;
                self.busy = false;
                let pending = self.pending_prepared_breakpoint.take();
                if !success {
                    self.status = "Could not set the breakpoint".into();
                    self.pending_auto_run = false;
                    self.screenshot_ready = true;
                    self.screenshot_ready_at = Some(std::time::Instant::now());
                } else if let Some(pending) = pending {
                    if !pending.current(&self.documents, &self.directory) {
                        self.preparation_failed("The editor buffer changed before the breakpoint was installed. Prepare it again before running.");
                    } else if pending.run_after {
                        if let Some(id) = pending.document_id {
                            if let Some(index) =
                                self.documents.iter().position(|document| document.id == id)
                            {
                                self.active_document = index;
                            }
                        }
                        self.pending_auto_run = false;
                        if let Some((&line, rest)) = pending.remaining_lines.split_first() {
                            self.prepare_breakpoint(pending.original_path, line, true);
                            if let Some(next) = &mut self.pending_prepared_breakpoint {
                                next.remaining_lines = rest.to_vec();
                            }
                        } else {
                            self.run_prepared_document();
                        }
                    } else {
                        self.status = "Breakpoint installed".into();
                    }
                }
            }
        }
        if let Some(state) = pause {
            let paused_status = format!("Paused at {}:{}", state.file.display(), state.line);
            let target = source_navigation::targets(
                vec![mathlab_engine::StackFrame {
                    file: state.file.display().to_string(),
                    name: state.function_name,
                    line: state.line,
                }],
                &self.documents,
                &self.directory,
                self.executed_document,
                self.internal_worker.as_deref(),
            )
            .pop();
            if let Some(target) = target {
                self.navigate_source(target);
            }
            self.status = paused_status;
        }
        if screenshot_ready && !self.screenshot_ready {
            self.screenshot_ready = true;
            self.screenshot_ready_at = Some(std::time::Instant::now());
        }
        let active_file =
            prepared_debug::original_path(&self.documents[self.active_document], &self.directory);
        let mut breakpoint_requested = None;
        if let Some(panel) = &mut self.debugger {
            panel.set_input_blocked(self.input.active());
            panel.set_console_streaming(
                self.streaming_console && self.console_subscription.is_some(),
            );
            panel.show(ctx, Some(&active_file), self.busy);
            breakpoint_requested = panel.breakpoint_requested.take();
        }
        if let Some((file, line)) = breakpoint_requested {
            self.prepare_breakpoint(file, line, false);
        }
    }

    fn new_document(&mut self) {
        let name = format!("untitled{}.m", self.next_document_id);
        self.documents.push(Document {
            id: self.next_document_id,
            path: None,
            name,
            source: String::new(),
            saved_source: String::new(),
            cursor: 0,
            selection: None,
            executed_paths: Vec::new(),
        });
        self.next_document_id += 1;
        self.active_document = self.documents.len() - 1;
    }

    fn choose_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_directory(&self.directory)
            .add_filter("MATLAB / Octave scripts", &["m"])
            .pick_file()
        {
            self.open_document(path);
        }
    }

    fn open_document(&mut self, path: PathBuf) {
        let path = if path.is_relative() {
            self.directory.join(path)
        } else {
            path
        };
        let path = path.canonicalize().unwrap_or(path);
        if let Some(index) = self
            .documents
            .iter()
            .position(|d| d.path.as_ref() == Some(&path))
        {
            self.active_document = index;
            return;
        }
        match fs::read_to_string(&path) {
            Ok(source) => {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                self.documents.push(Document {
                    id: self.next_document_id,
                    path: Some(path),
                    name,
                    source: source.clone(),
                    saved_source: source,
                    cursor: 0,
                    selection: None,
                    executed_paths: Vec::new(),
                });
                self.next_document_id += 1;
                self.active_document = self.documents.len() - 1;
            }
            Err(error) => self.error(format!("Could not open {}: {error}", path.display())),
        }
    }

    fn save_document(&mut self, save_as: bool) -> bool {
        let doc = &self.documents[self.active_document];
        let path = if save_as || doc.path.is_none() {
            rfd::FileDialog::new()
                .set_directory(&self.directory)
                .set_file_name(&doc.name)
                .add_filter("MATLAB / Octave scripts", &["m"])
                .save_file()
        } else {
            doc.path.clone()
        };
        let Some(path) = path else {
            return false;
        };
        match fs::write(&path, &doc.source) {
            Ok(()) => {
                let doc = &mut self.documents[self.active_document];
                doc.name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                doc.path = Some(path);
                doc.saved_source = doc.source.clone();
                self.status = format!("Saved {}", doc.name);
                true
            }
            Err(error) => {
                self.error(format!("Could not save {}: {error}", path.display()));
                false
            }
        }
    }

    fn close_document(&mut self, index: usize) {
        if self.documents[index].dirty() {
            self.unsaved_close = Some(index);
            return;
        }
        self.remove_document(index);
    }

    fn remove_document(&mut self, index: usize) {
        self.documents.remove(index);
        if self.documents.is_empty() {
            self.new_document();
        }
        self.active_document = self.active_document.min(self.documents.len() - 1);
        self.unsaved_close = None;
    }

    fn set_directory(&mut self, directory: PathBuf) {
        if self.busy {
            return;
        }
        match directory.canonicalize() {
            Ok(directory) if directory.is_dir() => {
                self.busy = true;
                let _ = self.requests.send(Request::Directory(directory));
            }
            _ => self.error("The selected path is not a directory."),
        }
    }

    fn load_mat(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_directory(&self.directory)
            .add_filter("MAT files", &["mat"])
            .pick_file()
        {
            let code = format!(
                "load({});",
                syntax::quote_matlab(&path.display().to_string())
            );
            self.execute(code, format!("load {}", path.display()));
        }
    }

    fn save_mat(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_directory(&self.directory)
            .set_file_name("workspace.mat")
            .add_filter("MAT files", &["mat"])
            .save_file()
        {
            let code = format!(
                "save('-mat7-binary', {});",
                syntax::quote_matlab(&path.display().to_string())
            );
            self.execute(code, format!("save workspace {}", path.display()));
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::F))
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::H))
        {
            self.show_find = true;
            self.find_focus = true;
            let doc = &self.documents[self.active_document];
            if let Some((start, end)) = doc.selection {
                self.find_text = doc.source.chars().skip(start).take(end - start).collect();
            }
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, Key::F3)) {
            self.find_in_source(true);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F3)) {
            self.find_in_source(false);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::G)) {
            self.show_go_to_line = true;
            self.go_to_line = self.documents[self.active_document]
                .source
                .chars()
                .take(self.documents[self.active_document].cursor)
                .filter(|&c| c == '\n')
                .count()
                + 1;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::O)) {
            self.choose_file();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::S)) {
            self.save_document(false);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::N)) {
            self.new_document();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, Key::F5)) {
            if self.input.active() {
                self.stop();
            } else if let Some(panel) = &mut self.debugger {
                panel.resume(mathlab_debugger::ResumeAction::Quit);
            }
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F5)) {
            self.run_or_continue();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F10)) {
            if let Some(panel) = &mut self.debugger {
                panel.resume(mathlab_debugger::ResumeAction::Step);
            }
        }
        let step_action = if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, Key::F11)) {
            Some(mathlab_debugger::ResumeAction::StepOut)
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F11)) {
            Some(mathlab_debugger::ResumeAction::StepIn)
        } else {
            None
        };
        if let (Some(action), Some(panel)) = (step_action, &mut self.debugger) {
            panel.resume(action);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F12)) {
            let doc = &self.documents[self.active_document];
            let line = doc
                .source
                .chars()
                .take(doc.cursor)
                .filter(|&c| c == '\n')
                .count()
                + 1;
            self.toggle_editor_breakpoint(line);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::Enter)) {
            self.run_section();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F9)) {
            self.run_selection();
        }
        if self.busy && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::C)) {
            self.stop();
        }
    }

    fn select_source_span(&mut self, start: usize, end: usize) {
        let doc = &mut self.documents[self.active_document];
        let start = doc.source[..start].chars().count();
        let end = doc.source[..end].chars().count();
        doc.cursor = end;
        doc.selection = (start < end).then_some((start, end));
        self.pending_editor_selection = Some((doc.id, start, end));
    }

    fn find_in_source(&mut self, previous: bool) {
        if self.find_text.is_empty() {
            self.show_find = true;
            self.find_focus = true;
            return;
        }
        let doc = &self.documents[self.active_document];
        let spans = search::matches(
            &doc.source,
            &self.find_text,
            self.match_case,
            self.whole_word,
        );
        let cursor = doc
            .source
            .char_indices()
            .nth(doc.cursor)
            .map_or(doc.source.len(), |(byte, _)| byte);
        let start_of_selection = doc
            .selection
            .map(|(start, _)| {
                doc.source
                    .char_indices()
                    .nth(start)
                    .map_or(doc.source.len(), |(byte, _)| byte)
            })
            .unwrap_or(cursor);
        let next = if previous {
            spans
                .iter()
                .rev()
                .find(|span| span.start < start_of_selection)
                .or_else(|| spans.last())
        } else {
            spans
                .iter()
                .find(|span| span.start >= cursor)
                .or_else(|| spans.first())
        };
        if let Some(span) = next {
            self.select_source_span(span.start, span.end);
        } else {
            self.status = format!("No matches for {}", self.find_text);
        }
    }

    fn search_dialogs(&mut self, ctx: &egui::Context) {
        if self.show_find {
            let mut open = true;
            let mut find = None;
            let mut replace = false;
            let mut replace_all = false;
            let doc = &self.documents[self.active_document];
            let mut spans = Vec::new();
            egui::Window::new("Find and Replace")
                .id(egui::Id::new("source_find"))
                .open(&mut open)
                .default_width(460.0)
                .show(ctx, |ui| {
                    egui::Grid::new("find_fields")
                        .spacing([10.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Find");
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut self.find_text)
                                    .desired_width(340.0),
                            );
                            if self.find_focus {
                                response.request_focus();
                                self.find_focus = false;
                            }
                            if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                find = Some(false);
                            }
                            ui.end_row();
                            ui.label("Replace with");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.replacement_text)
                                    .desired_width(340.0),
                            );
                            ui.end_row();
                        });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.match_case, "Match Case");
                        ui.checkbox(&mut self.whole_word, "Whole Word");
                    });
                    spans = search::matches(
                        &doc.source,
                        &self.find_text,
                        self.match_case,
                        self.whole_word,
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Find Next").clicked() {
                            find = Some(false);
                        }
                        if ui.button("Previous").clicked() {
                            find = Some(true);
                        }
                        if ui
                            .add_enabled(!spans.is_empty(), egui::Button::new("Replace"))
                            .clicked()
                        {
                            replace = true;
                        }
                        if ui
                            .add_enabled(!spans.is_empty(), egui::Button::new("Replace All"))
                            .clicked()
                        {
                            replace_all = true;
                        }
                    });
                    ui.label(
                        RichText::new(format!(
                            "{} matches in {} · F3 next · Shift+F3 previous",
                            spans.len(),
                            doc.name
                        ))
                        .small()
                        .color(MUTED),
                    );
                });
            self.show_find = open;
            if let Some(previous) = find {
                self.find_in_source(previous);
            }
            if replace_all {
                let doc = &mut self.documents[self.active_document];
                search::replace_all(&mut doc.source, &spans, &self.replacement_text);
                doc.cursor = doc.cursor.min(doc.source.chars().count());
                doc.selection = None;
                self.status = format!("Replaced {} matches", spans.len());
            } else if replace {
                let doc = &self.documents[self.active_document];
                let selected = doc.selection.map(|(start, end)| {
                    let start = doc
                        .source
                        .char_indices()
                        .nth(start)
                        .map_or(doc.source.len(), |(byte, _)| byte);
                    let end = doc
                        .source
                        .char_indices()
                        .nth(end)
                        .map_or(doc.source.len(), |(byte, _)| byte);
                    start..end
                });
                if let Some(span) = selected.filter(|selected| spans.contains(selected)) {
                    self.documents[self.active_document]
                        .source
                        .replace_range(span.clone(), &self.replacement_text);
                    self.select_source_span(span.start, span.start + self.replacement_text.len());
                } else {
                    self.find_in_source(false);
                }
            }
        }
        if self.show_go_to_line {
            let mut open = true;
            let mut go = false;
            let total = self.documents[self.active_document]
                .source
                .chars()
                .filter(|&c| c == '\n')
                .count()
                + 1;
            egui::Window::new("Go to Line")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!("Line (1–{total})"));
                        ui.add(egui::DragValue::new(&mut self.go_to_line).range(1..=total));
                        if ui.button("Go").clicked() {
                            go = true;
                        }
                    });
                });
            self.show_go_to_line = open;
            if go {
                let position = search::line_start(
                    &self.documents[self.active_document].source,
                    self.go_to_line,
                );
                self.select_source_span(position, position);
                self.show_go_to_line = false;
            }
        }
    }

    fn top(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("ribbon")
            .frame(egui::Frame::NONE.fill(PALE))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(BLUE)
                    .inner_margin(egui::Margin::symmetric(14, 7))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("OpenNumLab")
                                    .size(16.0)
                                    .strong()
                                    .color(Color32::WHITE),
                            );
                            ui.add_space(12.0);
                            ui.label(
                                RichText::new("Scientific computing, open to everyone")
                                    .size(12.0)
                                    .color(Color32::from_rgb(199, 220, 235)),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new("OPEN SOURCE  /  ALPHA 0.1")
                                        .size(10.0)
                                        .color(Color32::from_rgb(199, 220, 235)),
                                );
                            });
                        });
                    });
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("New Script                 Ctrl+N").clicked() {
                            self.new_document();
                            ui.close();
                        }
                        if ui.button("Open…                        Ctrl+O").clicked() {
                            self.choose_file();
                            ui.close();
                        }
                        if ui
                            .button("Save                            Ctrl+S")
                            .clicked()
                        {
                            self.save_document(false);
                            ui.close();
                        }
                        if ui.button("Save As…").clicked() {
                            self.save_document(true);
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(
                                self.connected && !self.busy,
                                egui::Button::new("Load MAT File…"),
                            )
                            .clicked()
                        {
                            self.load_mat();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.connected && !self.busy,
                                egui::Button::new("Save Workspace…"),
                            )
                            .clicked()
                        {
                            self.save_mat();
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Exit").clicked() {
                            if self.documents.iter().any(Document::dirty) {
                                self.pending_exit = true;
                            } else {
                                self.exit_confirmed = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                    });
                    ui.menu_button("Run", |ui| {
                        let enabled = self.connected && !self.busy;
                        let paused = self.debugger.as_ref().is_some_and(|panel| panel.paused());
                        if ui
                            .add_enabled(
                                (enabled || paused) && !self.input.active(),
                                egui::Button::new(if paused {
                                    "Continue                            F5"
                                } else {
                                    "Run Script                         F5"
                                }),
                            )
                            .clicked()
                        {
                            self.run_or_continue();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Button::new("Run Section              Ctrl+Enter"),
                            )
                            .clicked()
                        {
                            self.run_section();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Button::new("Evaluate Selection               F9"),
                            )
                            .clicked()
                        {
                            self.run_selection();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.busy && self.connected,
                                egui::Button::new("Stop Execution                    Ctrl+C"),
                            )
                            .clicked()
                        {
                            self.stop();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.connected,
                                egui::Button::new("Force Terminate and Restart…"),
                            )
                            .clicked()
                        {
                            self.force_restart_confirmation = true;
                            ui.close();
                        }
                    });
                    ui.menu_button("View", |ui| {
                        ui.checkbox(&mut self.show_folder, "Current Folder");
                        ui.checkbox(&mut self.show_workspace, "Workspace");
                        ui.checkbox(&mut self.show_history, "Command History");
                        ui.checkbox(&mut self.show_figures, "Figures");
                        if let Some(panel) = &mut self.debugger {
                            ui.checkbox(&mut panel.show, "Debugger");
                        }
                        if ui.button("Reset Layout").clicked() {
                            self.show_folder = true;
                            self.show_workspace = true;
                            self.show_history = true;
                            ui.close();
                        }
                    });
                    ui.menu_button("Edit", |ui| {
                        if ui.button("Find / Replace                 Ctrl+F").clicked() {
                            self.show_find = true;
                            self.find_focus = true;
                            ui.close();
                        }
                        if ui
                            .button("Go to Line                       Ctrl+G")
                            .clicked()
                        {
                            self.show_go_to_line = true;
                            ui.close();
                        }
                    });
                    ui.menu_button("Help", |ui| {
                        if ui.button("Getting Started").clicked() {
                            self.show_help = true;
                            ui.close();
                        }
                        if ui.button("Engine Settings").clicked() {
                            self.show_settings = true;
                            ui.close();
                        }
                        ui.hyperlink_to(
                            "GNU Octave documentation",
                            "https://docs.octave.org/latest/",
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Help").clicked() {
                            self.show_help = true;
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    for tab in ["Home", "Plots", "Apps", "Editor"] {
                        if ui
                            .selectable_label(self.ribbon_tab == tab, RichText::new(tab).strong())
                            .clicked()
                        {
                            self.ribbon_tab = tab.into();
                        }
                        ui.add_space(8.0);
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    let ready = self.connected && !self.busy;
                    match self.ribbon_tab.as_str() {
                        "Plots" => {
                            if ribbon_button(
                                ui,
                                "Figures",
                                "Show rendered figures",
                                Icon::Plot,
                                true,
                            ) {
                                self.show_figures = true;
                            }
                            ui.separator();
                            for (name, command) in [
                                ("Plot", "plot"),
                                ("Scatter", "scatter"),
                                ("Surface", "surf"),
                                ("Histogram", "hist"),
                            ] {
                                if ribbon_button(
                                    ui,
                                    name,
                                    "Open function help in the Command Window",
                                    Icon::Plot,
                                    ready,
                                ) {
                                    self.execute(
                                        format!("help {command}"),
                                        format!("help {command}"),
                                    );
                                }
                            }
                            ribbon_note(
                                ui,
                                "PLOT TOOLS",
                                "Use plot, scatter, surf, and other Octave graphics functions.",
                            );
                        }
                        "Apps" => {
                            if ribbon_button(
                                ui,
                                "Packages",
                                "List installed Octave packages",
                                Icon::Data,
                                ready,
                            ) {
                                self.show_packages = true;
                                self.dispatch(Request::Packages, "pkg list".into());
                            }
                            if ribbon_button(
                                ui,
                                "Help",
                                "Open package documentation",
                                Icon::Help,
                                true,
                            ) {
                                self.show_help = true;
                            }
                            ribbon_note(
                                ui,
                                "TOOLBOXES",
                                "Installed Octave packages are available in this same session.",
                            );
                        }
                        _ => {
                            if ribbon_button(
                                ui,
                                "New Script",
                                "New script (Ctrl+N)",
                                Icon::New,
                                true,
                            ) {
                                self.new_document();
                            }
                            if ribbon_button(
                                ui,
                                "Open",
                                "Open .m script (Ctrl+O)",
                                Icon::Open,
                                true,
                            ) {
                                self.choose_file();
                            }
                            if ribbon_button(ui, "Save", "Save script (Ctrl+S)", Icon::Save, true) {
                                self.save_document(false);
                            }
                            ui.separator();
                            let paused = self.debugger.as_ref().is_some_and(|panel| panel.paused());
                            if ribbon_button(
                                ui,
                                if paused { "Continue" } else { "Run" },
                                "Run or continue (F5)",
                                Icon::Play,
                                (ready || paused) && !self.input.active(),
                            ) {
                                self.run_or_continue();
                            }
                            if ribbon_button(
                                ui,
                                "Run Section",
                                "Run current %% section (Ctrl+Enter)",
                                Icon::Section,
                                ready,
                            ) {
                                self.run_section();
                            }
                            if ribbon_button(
                                ui,
                                "Stop",
                                if cfg!(unix) {
                                    "Interrupt execution while preserving the workspace (Ctrl+C)"
                                } else {
                                    "Stop this session with a confirmed restart (Ctrl+C)"
                                },
                                Icon::Stop,
                                self.busy && self.connected,
                            ) {
                                self.stop();
                            }
                            ui.separator();
                            if ribbon_button(ui, "Load Data", "Load a MAT file", Icon::Open, ready)
                            {
                                self.load_mat();
                            }
                            if ribbon_button(
                                ui,
                                "Save Data",
                                "Save workspace to MAT v7",
                                Icon::Data,
                                ready,
                            ) {
                                self.save_mat();
                            }
                            if ribbon_button(
                                ui,
                                "Clear",
                                "Clear workspace variables",
                                Icon::Clear,
                                ready,
                            ) {
                                self.execute("clear -variables;".into(), "clear variables".into());
                            }
                            ui.separator();
                            if ribbon_button(
                                ui,
                                "Figures",
                                "View and export figures",
                                Icon::Plot,
                                true,
                            ) {
                                self.show_figures = true;
                            }
                            if ribbon_button(
                                ui,
                                "Engine",
                                "Configure Octave executable",
                                Icon::Engine,
                                true,
                            ) {
                                self.show_settings = true;
                            }
                            ribbon_note(
                                ui,
                                "WORKSPACE",
                                "One session. Your scripts, commands, variables, and figures.",
                            );
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(RichText::new("Current Folder").small().color(MUTED));
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("Up"))
                        .on_hover_text("Parent folder")
                        .clicked()
                    {
                        if let Some(parent) = self.directory.parent() {
                            self.set_directory(parent.to_path_buf());
                        }
                    }
                    let path = ui.add_enabled(
                        !self.busy,
                        egui::TextEdit::singleline(&mut self.directory_input)
                            .font(egui::TextStyle::Small)
                            .desired_width(ui.available_width() - 48.0),
                    );
                    if path.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        self.set_directory(PathBuf::from(&self.directory_input));
                    }
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("…"))
                        .on_hover_text("Choose folder")
                        .clicked()
                    {
                        if let Some(folder) = rfd::FileDialog::new()
                            .set_directory(&self.directory)
                            .pick_folder()
                        {
                            self.set_directory(folder);
                        }
                    }
                });
                ui.add_space(5.0);
            });
    }

    fn current_folder(&mut self, ctx: &egui::Context) {
        if !self.show_folder {
            return;
        }
        egui::SidePanel::left("current_folder")
            .default_width(220.0)
            .width_range(160.0..=400.0)
            .frame(panel_frame())
            .show(ctx, |ui| {
                panel_header(ui, "Current Folder", None);
                let mut entries = match fs::read_dir(&self.directory) {
                    Ok(entries) => entries
                        .filter_map(Result::ok)
                        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                        .collect::<Vec<_>>(),
                    Err(error) => {
                        ui.colored_label(RED, error.to_string());
                        Vec::new()
                    }
                };
                entries.sort_by_key(|e| {
                    (
                        !e.path().is_dir(),
                        e.file_name().to_string_lossy().to_lowercase(),
                    )
                });
                let mut open = None;
                let mut folder = None;
                egui::ScrollArea::vertical()
                    .id_salt("folder_scroll")
                    .max_height((ui.available_height() - 40.0).max(30.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for entry in entries {
                            let path = entry.path();
                            let directory = path.is_dir();
                            let is_script = path.extension().is_some_and(|e| e == "m");
                            let name = entry.file_name().to_string_lossy().into_owned();
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(if directory {
                                        "▸"
                                    } else if is_script {
                                        "m"
                                    } else {
                                        "·"
                                    })
                                    .monospace()
                                    .strong()
                                    .color(if directory {
                                        Color32::from_rgb(182, 131, 49)
                                    } else if is_script {
                                        BLUE
                                    } else {
                                        MUTED
                                    }),
                                );
                                let response = ui
                                    .selectable_label(
                                        self.selected_file.as_ref() == Some(&path),
                                        name,
                                    )
                                    .on_hover_text(path.display().to_string());
                                if response.clicked() {
                                    self.selected_file = Some(path.clone());
                                }
                                if response.double_clicked() {
                                    if directory {
                                        folder = Some(path.clone());
                                    } else if is_script {
                                        open = Some(path.clone());
                                    }
                                }
                                response.context_menu(|ui| {
                                    if directory {
                                        if ui.button("Open Folder").clicked() {
                                            folder = Some(path.clone());
                                            ui.close();
                                        }
                                    } else if is_script && ui.button("Open in Editor").clicked() {
                                        open = Some(path.clone());
                                        ui.close();
                                    }
                                });
                            });
                        }
                    });
                if let Some(path) = open {
                    self.open_document(path);
                }
                if let Some(path) = folder {
                    self.set_directory(path);
                }
                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    ui.separator();
                    ui.label(
                        RichText::new("Double-click a folder or .m file to open it.")
                            .size(11.0)
                            .color(MUTED),
                    );
                });
            });
    }

    fn workspace_panel(&mut self, ctx: &egui::Context) {
        if !self.show_workspace && !self.show_history {
            return;
        }
        let mut open_variable = None;
        egui::SidePanel::right("workspace").default_width(290.0).width_range(230.0..=460.0).frame(panel_frame()).show(ctx, |ui| {
            if self.show_workspace {
                panel_header(ui, "Workspace", Some(self.workspace.len().to_string()));
                let height = if self.show_history { (ui.available_height()*0.56).max(180.0) } else { ui.available_height()-28.0 };
                egui::ScrollArea::both().id_salt("workspace_scroll").max_height(height).auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("variables").striped(true).spacing([12.0, 8.0]).min_col_width(56.0).show(ui, |ui| {
                        for heading in ["Name", "Size", "Class"] { ui.label(RichText::new(heading).size(11.0).strong().color(MUTED)); }
                        ui.end_row();
                        for variable in &self.workspace {
                            let selected = self.selected_variable.as_ref() == Some(&variable.name);
                            let response = ui.selectable_label(selected, RichText::new(&variable.name).monospace().color(BLUE)).on_hover_text(variable_bytes(variable));
                            if response.clicked() { self.selected_variable = Some(variable.name.clone()); }
                            if response.double_clicked() { open_variable = Some(variable.clone()); }
                            let response = ui.selectable_label(selected, RichText::new(variable_size(variable)).monospace().size(12.0));
                            if response.clicked() { self.selected_variable = Some(variable.name.clone()); }
                            if response.double_clicked() { open_variable = Some(variable.clone()); }
                            let response = ui.selectable_label(selected, RichText::new(&variable.class_name).size(12.0));
                            if response.clicked() { self.selected_variable = Some(variable.name.clone()); }
                            if response.double_clicked() { open_variable = Some(variable.clone()); }
                            ui.end_row();
                        }
                    });
                    if self.workspace.is_empty() {
                        ui.add_space(25.0);
                        ui.label(RichText::new("Your variables appear here after you run a command or script.").color(MUTED).size(12.0));
                    }
                });
                ui.add_space(4.0);
                ui.label(RichText::new("Double-click a variable to open Variable Editor.").size(11.0).color(MUTED));
                if let Some(name) = self.selected_variable.clone() {
                    if ui.small_button("Open Selected Variable").clicked() {
                        open_variable = self.workspace.iter().find(|variable| variable.name == name).cloned();
                    }
                }
            }
            if self.show_history {
                ui.add_space(10.0);
                panel_header(ui, "Command History", None);
                let mut replay = None;
                egui::ScrollArea::vertical().id_salt("history_scroll").auto_shrink([false, false]).show(ui, |ui| {
                    if self.history.is_empty() { ui.label(RichText::new("Commands you enter are recorded here.").color(MUTED).size(12.0)); }
                    for (index, command) in self.history.iter().enumerate().rev() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{:02}", index+1)).size(10.0).color(MUTED));
                            if ui.selectable_label(false, RichText::new(command).monospace().size(12.0)).on_hover_text("Double-click to evaluate again").double_clicked() { replay = Some(command.clone()); }
                        });
                    }
                });
                if let Some(command) = replay { self.execute(command.clone(), command); }
            }
        });
        if let Some(variable) = open_variable {
            self.open_variable(variable);
        }
    }

    fn command_window(&mut self, ctx: &egui::Context) {
        let mut navigate = None;
        egui::TopBottomPanel::bottom("command_window")
            .default_height(245.0)
            .height_range(150.0..=550.0)
            .resizable(true)
            .frame(panel_frame())
            .show(ctx, |ui| {
                // Reserve the panel's full height while leaving enough space for the input row.
                // Otherwise content can grow the resizable panel by a few pixels every frame.
                ui.set_min_height(ui.available_height());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Command Window").strong().color(INK));
                    if let Some(status) = self.input.status() { ui.label(RichText::new(status).small().color(BLUE)); }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .small_button("Clear")
                            .on_hover_text("Clear displayed output (workspace is unchanged)")
                            .clicked()
                        {
                            self.console.clear();
                        }
                        if self.busy {
                            ui.spinner();
                        }
                    });
                });
                ui.separator();
                let available = (ui.available_height() - if self.input.active() { 72.0 } else { 48.0 }).max(25.0);
                let follow = self.console_follow;
                egui::ScrollArea::vertical()
                    .id_salt("console_scroll")
                    .max_height(available)
                    .auto_shrink([false, false])
                    .stick_to_bottom(follow)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        if self.console.trimmed {
                            ui.label(RichText::new("Older console history was discarded (16 MiB / 4096 entry limit).")
                                .monospace().size(11.0).color(MUTED));
                        }
                        for entry in &self.console {
                            let (prefix, color) = match entry.kind {
                                ConsoleKind::Command => (">> ", BLUE),
                                ConsoleKind::Output => ("", INK),
                                ConsoleKind::Error => ("", RED),
                                ConsoleKind::Notice => ("", MUTED),
                            };
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!("{prefix}{}", if matches!(entry.kind, ConsoleKind::Output) {
                                        entry.text.as_str()
                                    } else {
                                        entry.text.trim_end()
                                    }))
                                        .monospace()
                                        .size(12.0)
                                        .color(color),
                                )
                                .wrap(),
                            );
                            if !matches!(entry.kind, ConsoleKind::Output) {
                                ui.add_space(3.0);
                            }
                            for target in &entry.sources {
                                if entry.sources_active {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            RichText::new(format!("  at {}", target.function_name))
                                                .monospace()
                                                .size(12.0)
                                                .color(RED),
                                        );
                                        let filename = target
                                            .file
                                            .file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy();
                                        if ui
                                            .link(
                                                RichText::new(format!(
                                                    "{filename}:{}",
                                                    target.line
                                                ))
                                                .monospace()
                                                .size(12.0),
                                            )
                                            .on_hover_text(format!(
                                                "Open {} at line {}",
                                                target.file.display(),
                                                target.line
                                            ))
                                            .clicked()
                                        {
                                            navigate = Some(target.clone());
                                        }
                                    });
                                } else {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(format!(
                                                "  at {} ({}:{})",
                                                target.function_name,
                                                target.file.display(),
                                                target.line
                                            ))
                                            .monospace()
                                            .size(12.0)
                                            .color(MUTED),
                                        )
                                        .wrap(),
                                    );
                                }
                            }
                        }
                    });
                ui.separator();
                if self.input.active() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Reply").monospace().strong().color(BLUE));
                        let enabled = self.connected && self.busy && !self.restarting && self.input.can_submit();
                        let hint = self.input.hint();
                        // egui single-line paste normally rewrites CR/LF as spaces.
                        // Reject it instead of silently changing an input reply.
                        let input_id = egui::Id::new("interactive_input_reply");
                        if enabled && ui.memory(|memory| memory.has_focus(input_id)) {
                            let rejected = ui.input_mut(|input| {
                                let mut rejected = false;
                                input.events.retain(|event| {
                                    let invalid = matches!(event, egui::Event::Paste(text)
                                        if text.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n')));
                                    rejected |= invalid;
                                    !invalid
                                });
                                rejected
                            });
                            if rejected { self.input.reject_multiline_paste(); }
                        }
                        let response = ui.add_enabled(enabled,
                            egui::TextEdit::singleline(self.input.reply_mut().unwrap())
                                .id(egui::Id::new("interactive_input_reply"))
                                .font(egui::TextStyle::Monospace)
                                .desired_width((ui.available_width() - 135.0).max(80.0))
                                .hint_text(hint)
                                .frame(false));
                        if self.focus_command && enabled {
                            response.request_focus();
                            self.focus_command = false;
                        }
                        let eof_key = response.has_focus()
                            && ui.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, Key::D));
                        if response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                            self.submit_command();
                        }
                        if ui.add_enabled(enabled, egui::Button::new("Send")).clicked() {
                            self.submit_command();
                        }
                        if ui.add_enabled(enabled, egui::Button::new("Send EOF"))
                            .on_hover_text("End input without sending a response (Ctrl+D)").clicked() || eof_key {
                            self.input.submit(true);
                        }
                    });
                    if let Some(error) = self.input.error() { ui.colored_label(RED, error); }
                } else {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(">>").monospace().strong().color(BLUE));
                    let response = ui.add_enabled(
                        !self.busy && self.connected,
                        egui::TextEdit::singleline(&mut self.command)
                            .id(egui::Id::new("command_input"))
                            .font(egui::TextStyle::Monospace)
                            .desired_width(ui.available_width() - 55.0)
                            .hint_text(if self.connected {
                                "Enter a command…"
                            } else {
                                "Connect GNU Octave in Engine Settings to begin"
                            })
                            .frame(false),
                    );
                    if self.focus_command {
                        response.request_focus();
                        self.focus_command = false;
                    }
                    if response.has_focus() {
                        if ui.input(|i| i.key_pressed(Key::ArrowUp)) {
                            self.navigate_history(true);
                        }
                        if ui.input(|i| i.key_pressed(Key::ArrowDown)) {
                            self.navigate_history(false);
                        }
                    }
                    if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        self.submit_command();
                        self.focus_command = true;
                    }
                    if ui
                        .add_enabled(!self.busy && self.connected, egui::Button::new("Enter"))
                        .clicked()
                    {
                        self.submit_command();
                    }
                });
                }
            });
        if let Some(target) = navigate {
            self.navigate_source(target);
        }
    }

    fn navigate_history(&mut self, backward: bool) {
        if self.history.is_empty() {
            return;
        }
        if backward {
            if self.history_index.is_none() {
                self.command_draft = self.command.clone();
            }
            let index = self
                .history_index
                .map(|i| i.saturating_sub(1))
                .unwrap_or(self.history.len() - 1);
            self.history_index = Some(index);
            self.command = self.history[index].clone();
        } else if let Some(index) = self.history_index {
            if index + 1 < self.history.len() {
                self.history_index = Some(index + 1);
                self.command = self.history[index + 1].clone();
            } else {
                self.history_index = None;
                self.command = self.command_draft.clone();
            }
        }
    }

    fn editor(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(panel_frame())
            .show(ctx, |ui| {
                panel_header(ui, "Editor", Some("MATLAB / Octave".into()));
                let mut close = None;
                egui::ScrollArea::horizontal()
                    .id_salt("editor_tabs")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (index, document) in self.documents.iter().enumerate() {
                                let name = format!(
                                    "{}{}",
                                    document.name,
                                    if document.dirty() { " •" } else { "" }
                                );
                                if ui
                                    .selectable_label(index == self.active_document, name)
                                    .clicked()
                                {
                                    self.active_document = index;
                                }
                                if ui.small_button("×").on_hover_text("Close file").clicked() {
                                    close = Some(index);
                                }
                                ui.add_space(4.0);
                            }
                            if ui.small_button("+").on_hover_text("New script").clicked() {
                                self.new_document();
                            }
                        });
                    });
                if let Some(index) = close {
                    self.close_document(index);
                }
                ui.separator();
                let source_path = prepared_debug::original_path(
                    &self.documents[self.active_document],
                    &self.directory,
                );
                let breakpoints = self
                    .debugger
                    .as_ref()
                    .map(|panel| panel.breakpoint_lines(&source_path))
                    .unwrap_or_default();
                let paused_line = self
                    .debugger
                    .as_ref()
                    .and_then(|panel| panel.state.as_ref())
                    .filter(|state| {
                        state.paused
                            && source_navigation::identity(&state.file, &self.directory)
                                == source_path
                    })
                    .map(|state| state.line);
                let snapshot = self.debugger.as_ref().and_then(|panel| panel.source_snapshot());
                let paused_source_matches = snapshot.is_some_and(|snapshot| source_navigation::identity(&snapshot.original_path, &self.directory) == source_path && snapshot.source == self.documents[self.active_document].source);
                if paused_line.is_some() && !paused_source_matches {
                    let message = if snapshot.is_some() {
                        "The editor differs from the paused source. View Executing source snapshot in the Debugger."
                    } else if self.debugger.as_ref().is_some_and(|panel| panel.snapshot_loading()) {
                        "Loading the executing source snapshot…"
                    } else {
                        "No executing source snapshot is available for this file. The interpreter reports the pause location."
                    };
                    ui.label(RichText::new(message).small().color(MUTED));
                }
                let paused_line = paused_line.filter(|_| paused_source_matches);
                let gutter_enabled = self.connected
                    && !self.busy
                    && self
                        .debugger
                        .as_ref()
                        .is_some_and(|panel| panel.can_modify_breakpoints());
                let mut toggle_line = None;
                let document = &mut self.documents[self.active_document];
                let line_count = document.source.chars().filter(|&c| c == '\n').count() + 1;
                let height = (ui.available_height() - 40.0).max(50.0);
                egui::ScrollArea::both()
                    .id_salt(("source_scroll", document.id))
                    .max_height(height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            let row_height =
                                ui.fonts_mut(|fonts| fonts.row_height(&FontId::monospace(13.0)));
                            let (gutter, _) = ui.allocate_exact_size(
                                Vec2::new(
                                    editor_gutter::width(line_count),
                                    row_height * line_count as f32,
                                ),
                                Sense::hover(),
                            );
                            ui.separator();
                            let mut layouter =
                                |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, width: f32| {
                                    let mut job = syntax::highlight(buffer.as_str(), 13.0);
                                    job.wrap.max_width = width;
                                    ui.fonts_mut(|fonts| fonts.layout_job(job))
                                };
                            let mut output = egui::TextEdit::multiline(&mut document.source)
                                .id(egui::Id::new(("source", document.id)))
                                .font(FontId::monospace(13.0))
                                .code_editor()
                                .desired_rows(24)
                                .desired_width((ui.available_width() - 6.0).max(700.0))
                                .lock_focus(true)
                                .frame(false)
                                .layouter(&mut layouter)
                                .show(ui);
                            toggle_line = editor_gutter::show(
                                ui,
                                &output,
                                gutter,
                                &breakpoints,
                                paused_line,
                                gutter_enabled,
                            );
                            let mut applied_selection = false;
                            if let Some((index, start, end)) = self.pending_editor_selection.take()
                            {
                                if index == document.id {
                                    let cursor = egui::text::CCursor::new(end);
                                    output.state.cursor.set_char_range(Some(
                                        egui::text::CCursorRange::two(
                                            cursor,
                                            egui::text::CCursor::new(start),
                                        ),
                                    ));
                                    output.state.store(ctx, output.response.id);
                                    output.response.request_focus();
                                    let position = output
                                        .galley
                                        .pos_from_cursor(cursor)
                                        .translate(output.galley_pos.to_vec2());
                                    ui.scroll_to_rect(position, Some(Align::Center));
                                    document.cursor = end;
                                    document.selection = (start < end).then_some((start, end));
                                    applied_selection = true;
                                }
                            }
                            if let Some(cursor) = output.cursor_range.filter(|_| !applied_selection)
                            {
                                document.cursor = cursor.primary.index;
                                let start = cursor.primary.index.min(cursor.secondary.index);
                                let end = cursor.primary.index.max(cursor.secondary.index);
                                document.selection = if end > start {
                                    Some((start, end))
                                } else {
                                    None
                                };
                            }
                        });
                    });
                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    ui.separator();
                    let prefix: String = document.source.chars().take(document.cursor).collect();
                    let line = prefix.chars().filter(|&c| c == '\n').count() + 1;
                    let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("Ln {line}, Col {column}"))
                                .size(11.0)
                                .color(MUTED),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(
                                    "F5 Run/Continue  ·  F12 Breakpoint  ·  F10/F11 Step",
                                )
                                .size(11.0)
                                .color(MUTED),
                            );
                        });
                    });
                });
                if let Some(line) = toggle_line {
                    self.toggle_editor_breakpoint(line);
                }
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(26.0)
            .frame(
                egui::Frame::NONE
                    .fill(PALE)
                    .inner_margin(egui::Margin::symmetric(10, 3)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (indicator, _) = ui.allocate_exact_size(Vec2::splat(9.0), Sense::hover());
                    ui.painter().circle_filled(
                        indicator.center(),
                        3.5,
                        if self.busy {
                            Color32::from_rgb(201, 145, 44)
                        } else if self.connected {
                            GREEN
                        } else {
                            RED
                        },
                    );
                    ui.label(RichText::new(self.input.status().unwrap_or(&self.status)).size(11.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(duration) = self.duration {
                            ui.label(
                                RichText::new(format!("{:.3} s", duration / 1000.0))
                                    .size(11.0)
                                    .color(MUTED),
                            );
                        }
                        ui.label(
                            RichText::new(format!("{} variables", self.workspace.len()))
                                .size(11.0)
                                .color(MUTED),
                        );
                        ui.label(
                            RichText::new(if self.connected {
                                "GNU Octave"
                            } else {
                                "Engine disconnected"
                            })
                            .size(11.0)
                            .color(MUTED),
                        );
                    });
                });
            });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        self.search_dialogs(ctx);
        self.variable_editor(ctx);
        if self.force_restart_confirmation {
            egui::Window::new("Force terminate this engine session?")
                .collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("This stops this OpenNumLab session's Octave process and starts a clean session.");
                    ui.label(RichText::new("Workspace variables and figures will be lost unless already saved.").color(RED));
                    ui.horizontal(|ui| {
                        if ui.button("Terminate and Restart").clicked() { self.force_restart(); }
                        if ui.button("Cancel").clicked() { self.force_restart_confirmation = false; }
                    });
                });
        }
        if self.show_packages {
            let mut open = true;
            egui::Window::new("Scientific Packages").open(&mut open).default_size([680.0, 420.0]).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Installed toolboxes");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add_enabled(self.connected && !self.busy, egui::Button::new("Refresh")).clicked() { self.dispatch(Request::Packages, "pkg list".into()); }
                        if self.busy { ui.spinner(); }
                    });
                });
                ui.label("Load a package to make its functions available to your scripts and commands.");
                ui.separator();
                let mut load = None;
                egui::ScrollArea::vertical().id_salt("packages_scroll").max_height(330.0).show(ui, |ui| {
                    if self.packages.is_empty() && !self.busy { ui.label("No packages are installed in this Octave runtime."); }
                    for package in &self.packages {
                        egui::Frame::NONE.fill(PALE).inner_margin(8).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&package.name).strong().color(BLUE));
                                ui.label(RichText::new(&package.version).small().color(MUTED));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if package.loaded { ui.label(RichText::new("Loaded").color(GREEN)); }
                                    else if ui.add_enabled(self.connected && !self.busy, egui::Button::new("Load")).clicked() { load = Some(package.name.clone()); }
                                });
                            });
                            if !package.description.is_empty() { ui.label(RichText::new(&package.description).size(12.0).color(MUTED)); }
                        });
                        ui.add_space(4.0);
                    }
                });
                if let Some(name) = load { self.dispatch(Request::LoadPackage(name.clone()), format!("pkg load {name}")); }
                ui.separator();
                ui.hyperlink_to("Browse the Octave package catalog", "https://packages.octave.org/");
                ui.label(RichText::new("Packages retain their upstream licenses and are installed into the selected Octave runtime.").small().color(MUTED));
            });
            self.show_packages = open;
        }
        if self.show_settings {
            let mut open = true;
            egui::Window::new("Engine Settings").open(&mut open).resizable(false).default_width(550.0).show(ctx, |ui| {
                ui.heading("GNU Octave runtime");
                ui.label("OpenNumLab uses an installed GNU Octave executable to run your MATLAB-style code.");
                ui.add_space(8.0);
                ui.label(RichText::new("Executable path").strong());
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.configured_executable).desired_width(410.0).hint_text("Auto-detect from PATH"));
                    if ui.button("Choose…").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() { self.configured_executable = path.display().to_string(); }
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.busy, egui::Button::new("Connect / Retry")).clicked() {
                        self.debugger = None;
                        self.pending_breakpoint_id = None;
                        self.pending_prepared_breakpoint = None;
                        self.input.disconnect();
                        self.connected = false;
                        self.busy = true;
                        self.status = "Starting GNU Octave…".into();
                        let executable = if self.configured_executable.trim().is_empty() { None } else { Some(PathBuf::from(self.configured_executable.trim())) };
                        let _ = self.requests.send(Request::Start { executable, directory: self.directory.clone() });
                    }
                    if ui.add_enabled(self.connected && !self.busy, egui::Button::new("Restart Engine")).clicked() { self.restart(); }
                    ui.label(if self.connected { "Connected" } else if self.busy { "Connecting…" } else { "Not connected" });
                });
                ui.separator();
                ui.label(RichText::new("Install Octave").strong());
                ui.monospace("macOS:    brew install octave\nUbuntu:   sudo apt install octave");
                ui.hyperlink_to("Windows installers and other platforms", "https://octave.org/download");
                ui.separator();
                ui.label(RichText::new("Optional native debugger adapter").strong());
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.configured_debugger_bridge).desired_width(400.0).hint_text("Path to mathlab_debug_bridge.oct"));
                    if ui.button("Choose Adapter…").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("Octave native adapter", &["oct"]).pick_file() { self.configured_debugger_bridge = path.display().to_string(); }
                    }
                });
                if ui.add_enabled(self.connected && !self.busy && !self.configured_debugger_bridge.trim().is_empty(), egui::Button::new("Enable Debugger")).clicked() { self.enable_debugger(); }
                ui.label(if self.debugger.is_some() { "Debugger enabled" } else { "Debugger disabled" });
                ui.label(RichText::new("Build with scripts/build-debugger.sh using the development headers for this Octave runtime. The debugger is optional and defaults to disabled.").small().color(MUTED));
                ui.add_space(8.0);
                ui.label(RichText::new(if cfg!(unix) { "Stop / Ctrl+C requests an interrupt and preserves the workspace. Use Run → Force Terminate and Restart if a computation cannot be interrupted; that clears this session's variables." } else { "This platform currently stops execution by restarting the engine, with confirmation. This clears the session's variables." }).size(12.0).color(MUTED));
            });
            self.show_settings = open;
        }
        if self.show_help {
            let mut open = true;
            egui::Window::new("Getting Started").open(&mut open).default_width(590.0).show(ctx, |ui| {
                ui.heading("Welcome to OpenNumLab");
                ui.label("The familiar scientific workflow: open a script, run it, inspect variables, and view figures.");
                ui.add_space(10.0);
                egui::Grid::new("help_shortcuts").spacing([20.0, 10.0]).show(ui, |ui| {
                    for (key, action) in [("F5", "Run the active script or continue debugging"), ("F12 / gutter", "Toggle a breakpoint while idle"), ("F10 / F11", "Step / step in"), ("Shift+F11", "Step out"), ("Shift+F5", "Quit debugging"), ("Ctrl+Enter", "Run the %% section containing the cursor"), ("F9", "Evaluate selected code"), ("Ctrl/Cmd+O", "Open a .m script"), ("Ctrl/Cmd+S", "Save the active script"), ("Ctrl/Cmd+F", "Find and replace in the active script"), ("Ctrl/Cmd+G", "Go to a line"), ("Ctrl+C", "Stop execution"), ("Ctrl+D", "Send EOF when replying to input"), ("Up / Down", "Recall commands in the Command Window")] {
                        ui.monospace(key); ui.label(action); ui.end_row();
                    }
                });
                ui.separator();
                ui.label("Scripts and commands share one Octave workspace. Load Data and Save Data read and write MAT files. Figures render in a viewer with zoom and PNG export.");
                ui.label("Double-click a workspace variable to open its paged Variable Editor. Numeric and logical 2D arrays support cell edits; other values show a read-only preview. Whole-script Run supports forward local functions; section and selection execution evaluate only the selected source.");
                ui.add_space(8.0);
                ui.label(RichText::new("Compatibility is provided by GNU Octave. MATLAB toolboxes, Simulink models, .mlx documents, and .mlapp applications are not implemented by this initial release.").color(MUTED));
                ui.add_space(8.0);
                ui.hyperlink_to("Octave language and function reference", "https://docs.octave.org/latest/");
                ui.hyperlink_to("Octave package catalog", "https://packages.octave.org/");
            });
            self.show_help = open;
        }
        if let Some(index) = self.unsaved_close {
            egui::Window::new("Unsaved changes")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Save changes to {} before closing?",
                        self.documents[index].name
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            self.active_document = index;
                            if self.save_document(false) {
                                self.remove_document(index);
                            }
                        }
                        if ui.button("Discard").clicked() {
                            self.remove_document(index);
                        }
                        if ui.button("Cancel").clicked() {
                            self.unsaved_close = None;
                        }
                    });
                });
        }
        if self.pending_exit {
            egui::Window::new("Save your work before exiting")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("One or more scripts have unsaved changes.");
                    ui.horizontal(|ui| {
                        if ui.button("Save All and Exit").clicked() {
                            let dirty = self
                                .documents
                                .iter()
                                .enumerate()
                                .filter(|(_, doc)| doc.dirty())
                                .map(|(index, _)| index)
                                .collect::<Vec<_>>();
                            let mut saved = true;
                            for index in dirty {
                                self.active_document = index;
                                if !self.save_document(false) {
                                    saved = false;
                                    break;
                                }
                            }
                            if saved {
                                self.exit_confirmed = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        if ui.button("Discard and Exit").clicked() {
                            for document in &mut self.documents {
                                document.source.clone_from(&document.saved_source);
                            }
                            self.exit_confirmed = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending_exit = false;
                        }
                    });
                });
        }
    }

    fn figure_window(&mut self, ctx: &egui::Context) {
        if !self.show_figures {
            return;
        }
        let mut open = true;
        egui::Window::new("Figures").open(&mut open).default_size([820.0, 620.0]).show(ctx, |ui| {
            if self.figures.is_empty() {
                ui.add_space(35.0);
                ui.label(RichText::new("No figures yet").size(20.0).strong().color(BLUE));
                ui.label("Run a plotting command or the getting started script. Figures will appear here.");
                return;
            }
            ui.horizontal(|ui| {
                for (index, figure) in self.figures.iter().enumerate() {
                    if ui.selectable_label(self.selected_figure == index, &figure.figure.title).clicked() { self.selected_figure = index; }
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Zoom");
                ui.add(egui::Slider::new(&mut self.figure_zoom, 0.25..=3.0).show_value(false));
                ui.label(format!("{:.0}%", self.figure_zoom*100.0));
                if ui.button("Fit").clicked() { self.figure_zoom = 1.0; }
                if ui.button("Export PNG…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().set_directory(&self.directory).set_file_name("figure.png").add_filter("PNG image", &["png"]).save_file() {
                        if let Err(error) = fs::copy(&self.figures[self.selected_figure].figure.path, &path) { self.error(format!("Could not export figure: {error}")); }
                    }
                }
            });
            let figure = &mut self.figures[self.selected_figure];
            if figure.texture.is_none() && figure.loading_error.is_none() {
                match load_texture(ctx, &figure.figure.path) {
                    Ok(texture) => figure.texture = Some(texture),
                    Err(error) => figure.loading_error = Some(error),
                }
            }
            if let Some(texture) = &figure.texture {
                let source = texture.size_vec2();
                let fit = ((ui.available_width()-20.0)/source.x).min((ui.available_height()-25.0)/source.y).clamp(0.1,1.0);
                let size = source*fit*self.figure_zoom;
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add(egui::Image::new(texture).fit_to_exact_size(size));
                });
            } else if let Some(error) = &figure.loading_error { ui.colored_label(RED, error); }
        });
        self.show_figures = open;
    }
}

impl eframe::App for MathLabApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested())
            && !self.exit_confirmed
            && self.documents.iter().any(Document::dirty)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending_exit = true;
        }
        if let Some(path) = self.screenshot.clone() {
            let images = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            });
            for image in images {
                let pixels = image
                    .pixels
                    .iter()
                    .flat_map(|color| color.to_array())
                    .collect::<Vec<_>>();
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                match image::save_buffer(
                    &path,
                    &pixels,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                ) {
                    Ok(()) => {
                        eprintln!("Desktop screenshot saved to {}", path.display());
                        self.exit_confirmed = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Err(error) => {
                        self.error(format!("Could not save screenshot: {error}"));
                        self.screenshot = None;
                    }
                }
            }
        }
        self.poll_events(ctx);
        self.poll_input();
        self.shortcuts(ctx);
        self.top(ctx);
        self.status_bar(ctx);
        self.current_folder(ctx);
        self.workspace_panel(ctx);
        self.command_window(ctx);
        self.editor(ctx);
        self.dialogs(ctx);
        self.figure_window(ctx);
        self.debugger_updates(ctx);
        if self.screenshot.is_some() && self.screenshot_ready && !self.screenshot_requested {
            self.screenshot_frames += 1;
            if self.screenshot_frames >= 4
                && self
                    .screenshot_ready_at
                    .is_some_and(|instant| instant.elapsed().as_millis() >= 400)
            {
                self.screenshot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        if self.console_subscription.is_some() {
            // Session-lifetime output also arrives while the request worker is idle.
            ctx.request_repaint_after(std::time::Duration::from_millis(25));
        } else if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.state_file.is_some() {
            if self.state_saved_at.elapsed().as_secs() >= 3 {
                self.persist_session();
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(3));
        }
    }
}

impl Drop for MathLabApp {
    fn drop(&mut self) {
        self.persist_session();
        if let Some(cancellation) = &self.cancellation {
            let _ = cancellation.terminate();
        }
    }
}

fn style(ctx: &egui::Context) {
    let mut style = egui::Style {
        visuals: egui::Visuals::light(),
        ..Default::default()
    };
    style.visuals.panel_fill = Color32::WHITE;
    style.visuals.window_fill = Color32::WHITE;
    style.visuals.faint_bg_color = PALE;
    style.visuals.selection.bg_fill = Color32::from_rgb(222, 235, 246);
    style.visuals.selection.stroke = Stroke::new(1.0, BLUE);
    style.visuals.widgets.inactive.bg_fill = PALE;
    style.visuals.widgets.inactive.weak_bg_fill = PALE;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(231, 239, 246);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(231, 239, 246);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(216, 231, 243);
    style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(216, 231, 243);
    style.visuals.window_stroke = Stroke::new(1.0, BORDER);
    style.spacing.item_spacing = Vec2::new(7.0, 5.0);
    style.spacing.button_padding = Vec2::new(8.0, 5.0);
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(12.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, FontId::monospace(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(11.0));
    ctx.set_style(style);
}

fn panel_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(Color32::WHITE)
        .inner_margin(egui::Margin::symmetric(10, 8))
}

fn panel_header(ui: &mut egui::Ui, title: &str, detail: Option<String>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong().color(INK));
        if let Some(detail) = detail {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(detail).small().color(MUTED));
            });
        }
    });
    ui.separator();
    ui.add_space(3.0);
}

fn dimensions(dimensions: &[usize]) -> String {
    dimensions
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join("×")
}

fn variable_size(variable: &Variable) -> String {
    if variable.dimensions_known {
        dimensions(&variable.dimensions)
    } else {
        "—".into()
    }
}

fn variable_bytes(variable: &Variable) -> String {
    if variable.bytes_known {
        format!("{} bytes", variable.bytes)
    } else {
        "bytes unavailable".into()
    }
}

fn load_texture(ctx: &egui::Context, path: &Path) -> Result<egui::TextureHandle, String> {
    let image = image::open(path).map_err(|e| e.to_string())?.into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Ok(ctx.load_texture(
        path.display().to_string(),
        image,
        egui::TextureOptions::LINEAR,
    ))
}

enum Icon {
    New,
    Open,
    Save,
    Play,
    Section,
    Stop,
    Data,
    Clear,
    Plot,
    Engine,
    Help,
}

fn ribbon_button(ui: &mut egui::Ui, label: &str, hint: &str, icon: Icon, enabled: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(68.0, 60.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    if enabled && response.hovered() {
        ui.painter()
            .rect_filled(rect, 4.0, Color32::from_rgb(230, 238, 246));
    }
    let color = if enabled {
        BLUE
    } else {
        Color32::from_rgb(175, 184, 193)
    };
    let center = rect.center_top() + Vec2::new(0.0, 21.0);
    let painter = ui.painter();
    let stroke = Stroke::new(1.7, color);
    match icon {
        Icon::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + Vec2::new(-8.0, -10.0),
                    center + Vec2::new(-8.0, 10.0),
                    center + Vec2::new(10.0, 0.0),
                ],
                if enabled { GREEN } else { color },
                Stroke::NONE,
            ));
        }
        Icon::Stop => {
            painter.rect_filled(
                egui::Rect::from_center_size(center, Vec2::splat(17.0)),
                2.0,
                if enabled { RED } else { color },
            );
        }
        Icon::Plot => {
            painter.line_segment(
                [
                    center + Vec2::new(-11.0, -10.0),
                    center + Vec2::new(-11.0, 10.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-11.0, 10.0),
                    center + Vec2::new(12.0, 10.0),
                ],
                stroke,
            );
            painter.add(egui::Shape::line(
                vec![
                    center + Vec2::new(-8.0, 5.0),
                    center + Vec2::new(-2.0, -5.0),
                    center + Vec2::new(4.0, 2.0),
                    center + Vec2::new(11.0, -9.0),
                ],
                Stroke::new(2.0, if enabled { GREEN } else { color }),
            ));
        }
        Icon::Open => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + Vec2::new(-12.0, -7.0),
                    center + Vec2::new(-3.0, -7.0),
                    center + Vec2::new(0.0, -3.0),
                    center + Vec2::new(12.0, -3.0),
                    center + Vec2::new(9.0, 10.0),
                    center + Vec2::new(-12.0, 10.0),
                ],
                Color32::TRANSPARENT,
                stroke,
            ));
        }
        Icon::Data => {
            painter.rect_stroke(
                egui::Rect::from_center_size(center, Vec2::new(23.0, 20.0)),
                2.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            for x in [-4.0, 4.0] {
                painter.line_segment(
                    [center + Vec2::new(x, -9.0), center + Vec2::new(x, 9.0)],
                    stroke,
                );
            }
            painter.line_segment(
                [
                    center + Vec2::new(-10.0, 0.0),
                    center + Vec2::new(10.0, 0.0),
                ],
                stroke,
            );
        }
        Icon::Help => {
            painter.circle_stroke(center, 11.0, stroke);
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                "?",
                FontId::proportional(18.0),
                color,
            );
        }
        Icon::Engine => {
            painter.circle_stroke(center, 9.0, stroke);
            painter.circle_stroke(center, 3.0, stroke);
            for a in 0..8 {
                let direction = Vec2::angled(a as f32 * std::f32::consts::TAU / 8.0);
                painter.line_segment(
                    [center + direction * 9.0, center + direction * 13.0],
                    stroke,
                );
            }
        }
        Icon::Clear => {
            painter.line_segment(
                [center + Vec2::new(-9.0, 8.0), center + Vec2::new(9.0, -9.0)],
                Stroke::new(5.0, color),
            );
            painter.line_segment(
                [
                    center + Vec2::new(-8.0, 11.0),
                    center + Vec2::new(11.0, 11.0),
                ],
                stroke,
            );
        }
        Icon::New | Icon::Save | Icon::Section => {
            painter.rect_stroke(
                egui::Rect::from_center_size(center, Vec2::new(18.0, 24.0)),
                2.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            if matches!(icon, Icon::Save) {
                painter.rect_stroke(
                    egui::Rect::from_center_size(
                        center + Vec2::new(0.0, 3.0),
                        Vec2::new(10.0, 11.0),
                    ),
                    0.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            } else {
                for y in [-5.0, 0.0, 5.0] {
                    painter.line_segment(
                        [center + Vec2::new(-5.0, y), center + Vec2::new(5.0, y)],
                        stroke,
                    );
                }
            }
            if matches!(icon, Icon::Section) {
                painter.circle_filled(
                    center + Vec2::new(8.0, 8.0),
                    5.0,
                    if enabled { GREEN } else { color },
                );
            }
        }
    }
    painter.text(
        rect.center_bottom() - Vec2::new(0.0, 8.0),
        egui::Align2::CENTER_BOTTOM,
        label,
        FontId::proportional(11.0),
        if enabled { INK } else { color },
    );
    let clicked = enabled && response.clicked();
    response.on_hover_text(hint);
    clicked
}

fn ribbon_note(ui: &mut egui::Ui, heading: &str, text: &str) {
    ui.add_space(12.0);
    ui.vertical(|ui| {
        ui.add_space(7.0);
        ui.label(RichText::new(heading).size(10.0).strong().color(MUTED));
        ui.add(egui::Label::new(RichText::new(text).size(11.0).color(MUTED)).wrap());
    });
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use std::sync::mpsc::channel;

    pub(super) struct Harness {
        pub(super) app: MathLabApp,
        pub(super) events: Sender<Event>,
        pub(super) requests: Receiver<Request>,
        pub(super) ctx: egui::Context,
        _directory: tempfile::TempDir,
    }

    pub(super) fn harness() -> Harness {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = MathLabApp::new(
            &cc,
            DesktopOptions {
                directory: directory.path().into(),
                executable: Some(directory.path().join("missing-test-runtime")),
                screenshot: Some(directory.path().join("unused.png")),
                ..DesktopOptions::default()
            },
        );
        let (events, events_rx) = channel();
        let (requests_tx, requests) = channel();
        app.events = events_rx;
        app.requests = requests_tx;
        app.connected = true;
        app.busy = false;
        Harness {
            app,
            events,
            requests,
            ctx,
            _directory: directory,
        }
    }

    fn inspected_token(revision: u64) -> mathlab_engine::BindingToken {
        mathlab_engine::BindingToken {
            context: mathlab_engine::SessionContext {
                worker_id: "lifecycle-worker".into(),
                generation: 1,
                invocation: 1,
                revision,
            },
            name: "A".into(),
        }
    }

    fn inspected_variable() -> Variable {
        Variable {
            name: "A".into(),
            class_name: "double".into(),
            dimensions: vec![2, 2],
            dimensions_known: true,
            bytes: 32,
            bytes_known: true,
            preview: "1 2 3 4".into(),
        }
    }

    fn inspected_page(revision: u64, value: &str) -> variable_editor::VariablePage {
        variable_editor::VariablePage::Checked(mathlab_engine::BoundVariableSlice {
            token: inspected_token(revision),
            slice: mathlab_engine::VariableSlice {
                name: "A".into(),
                class_name: "double".into(),
                dimensions: vec![2, 2],
                row_start: 1,
                column_start: 1,
                rows: 1,
                columns: 1,
                cells: vec![vec![value.into()]],
                editable: true,
            },
        })
    }

    fn seed_inspector(test: &mut Harness, revision: u64) {
        let variable = inspected_variable();
        let mut editor = VariableEditor::new(variable.clone());
        editor.begin(1);
        editor.accept(1, Ok(inspected_page(revision, "1")));
        test.app.inspected = Some(editor);
        test.app.workspace = vec![variable];
        test.app.inspector_request = 1;
    }

    #[test]
    fn variable_pagination_keeps_the_page_binding_and_explicit_refresh_drops_it() {
        let mut test = harness();
        seed_inspector(&mut test, 7);
        test.app.read_variable();
        let request_id = match test.requests.recv().unwrap() {
            Request::ReadVariable {
                request_id, token, ..
            } => {
                assert_eq!(token, Some(inspected_token(7)));
                request_id
            }
            _ => panic!("expected checked page request"),
        };
        assert!(test.app.inspected.as_ref().unwrap().token.is_none());
        test.events
            .send(Event::VariableRead {
                request_id,
                result: Ok(inspected_page(7, "1")),
            })
            .unwrap();
        test.app.poll_events(&test.ctx);
        assert_eq!(
            test.app.inspected.as_ref().unwrap().token,
            Some(inspected_token(7))
        );
        test.app.invalidate_inspector();
        test.app.read_variable();
        assert!(matches!(
            test.requests.recv().unwrap(),
            Request::ReadVariable { token: None, .. }
        ));
        assert!(test.app.inspected.as_ref().unwrap().token.is_none());
    }

    #[test]
    fn nd_page_request_carries_the_selected_plane_with_its_accepted_token() {
        let mut test = harness();
        seed_inspector(&mut test, 7);
        let editor = test.app.inspected.as_mut().unwrap();
        editor.variable.dimensions = vec![3, 4, 5];
        editor.higher_indices = vec![4];
        test.app.read_variable();
        match test.requests.recv().unwrap() {
            Request::ReadVariable {
                name,
                token,
                higher_indices,
                row_start,
                column_start,
                ..
            } => {
                assert_eq!(name, "A");
                assert_eq!(token, Some(inspected_token(7)));
                assert_eq!(higher_indices, [4]);
                assert_eq!((row_start, column_start), (1, 1));
            }
            _ => panic!("expected a plane-bound checked read"),
        }
        let editor = test.app.inspected.as_ref().unwrap();
        assert!(editor.pending);
        assert!(editor.stale);
        assert!(editor.token.is_none());
    }

    #[test]
    fn accepted_write_receipt_rereads_its_exact_token_before_reenabling_edits() {
        let mut test = harness();
        seed_inspector(&mut test, 7);
        test.app.inspector_request = 2;
        test.app.inspected.as_mut().unwrap().begin(2);
        test.events
            .send(Event::VariableChanged {
                request_id: 2,
                result: Ok(mathlab_engine::MutationReceipt {
                    token: inspected_token(8),
                    workspace: vec![inspected_variable()],
                }),
            })
            .unwrap();
        test.app.poll_events(&test.ctx);
        let request_id = match test.requests.recv().unwrap() {
            Request::ReadVariable {
                request_id, token, ..
            } => {
                assert_eq!(token, Some(inspected_token(8)));
                request_id
            }
            _ => panic!("expected receipt-bound page request"),
        };
        let editor = test.app.inspected.as_ref().unwrap();
        assert!(editor.pending);
        assert!(editor.stale);
        assert!(editor.token.is_none());
        test.events
            .send(Event::VariableRead {
                request_id,
                result: Ok(inspected_page(8, "99")),
            })
            .unwrap();
        test.app.poll_events(&test.ctx);
        assert_eq!(
            test.app.inspected.as_ref().unwrap().token,
            Some(inspected_token(8))
        );
        assert_eq!(
            test.app
                .inspected
                .as_ref()
                .unwrap()
                .slice
                .as_ref()
                .unwrap()
                .cells[0][0],
            "99"
        );
    }

    #[test]
    fn stale_variable_write_reports_failure_without_reread_or_retry() {
        let mut test = harness();
        seed_inspector(&mut test, 7);
        test.app.inspected.as_mut().unwrap().begin(2);
        test.events
            .send(Event::VariableChanged {
                request_id: 2,
                result: Err("The workspace binding changed. Refresh this variable.".into()),
            })
            .unwrap();
        test.app.poll_events(&test.ctx);
        let editor = test.app.inspected.as_ref().unwrap();
        assert!(editor.stale);
        assert!(!editor.pending);
        assert!(editor.token.is_none());
        assert!(editor.error.as_ref().unwrap().contains("binding changed"));
        assert_eq!(editor.slice.as_ref().unwrap().cells[0][0], "1");
        assert!(
            test.requests.try_recv().is_err(),
            "stale edits require an explicit refresh"
        );
    }

    #[test]
    fn dispatching_source_revokes_edit_token_and_ignores_a_late_page() {
        let mut test = harness();
        seed_inspector(&mut test, 7);
        test.app.read_variable();
        let request_id = match test.requests.recv().unwrap() {
            Request::ReadVariable { request_id, .. } => request_id,
            _ => panic!("expected page request"),
        };
        test.app.execute("A=42;".into(), "replace A".into());
        assert!(matches!(test.requests.recv().unwrap(), Request::Execute(_)));
        test.events
            .send(Event::VariableRead {
                request_id,
                result: Ok(inspected_page(7, "1")),
            })
            .unwrap();
        test.app.poll_events(&test.ctx);
        let editor = test.app.inspected.as_ref().unwrap();
        assert!(editor.stale);
        assert!(editor.token.is_none());
        assert!(test.app.busy);
    }

    #[test]
    fn late_debugger_enable_replies_cannot_reattach_a_terminated_session() {
        let mut test = harness();
        test.app.initial_breakpoint = Some(5);
        test.app.pending_auto_run = true;
        test.app.force_restart();
        assert!(matches!(test.requests.recv().unwrap(), Request::Restart));
        let restart_status = test.app.status.clone();
        test.events
            .send(Event::DebuggerReady(mathlab_debugger::Debugger::new(
                test.app.directory.join("obsolete-debugger"),
            )))
            .unwrap();
        test.events
            .send(Event::DebuggerUnavailable {
                error: "old enable request was canceled".into(),
                debugger: None,
            })
            .unwrap();
        test.app.poll_events(&test.ctx);
        assert!(test.app.restarting);
        assert!(test.app.busy);
        assert!(test.app.debugger.is_none());
        assert_eq!(test.app.initial_breakpoint, Some(5));
        assert_eq!(test.app.status, restart_status);
        assert!(test.requests.try_recv().is_err());
        assert!(!test.app.show_settings);
        test.events
            .send(Event::Restarted {
                debugger: None,
                internal_worker: test.app.directory.join("new-worker.m"),
                input: None,
            })
            .unwrap();
        test.app.poll_events(&test.ctx);
        assert!(!test.app.restarting);
        assert!(!test.app.busy);
        assert_eq!(test.app.status, "Ready");
    }

    #[test]
    fn stopped_response_worker_releases_busy_state_and_recreates_a_retryable_worker() {
        let mut test = harness();
        let source = test.app.documents[0].source.clone();
        test.app.busy = true;
        test.app.status = "Running…".into();
        test.app.pending_auto_run = true;
        test.app.pending_prepared_breakpoint = Some(prepared_debug::PreparedBreakpoint {
            token: 1,
            document_id: Some(test.app.documents[0].id),
            original_path: prepared_debug::original_path(
                &test.app.documents[0],
                &test.app.directory,
            ),
            line: 2,
            source_snapshot: Some(source.clone()),
            run_after: true,
            remaining_lines: Vec::new(),
        });
        drop(test.events);
        test.app.poll_events(&test.ctx);
        assert!(!test.app.busy);
        assert!(!test.app.connected);
        assert!(!test.app.restarting);
        assert!(test.app.show_settings);
        assert!(test.app.debugger.is_none());
        assert!(test.app.pending_prepared_breakpoint.is_none());
        assert!(!test.app.pending_auto_run);
        assert_eq!(test.app.documents[0].source, source);
        test.app.requests.send(Request::Packages).unwrap();
        assert!(matches!(
            test.app.events.recv_timeout(std::time::Duration::from_secs(2)).unwrap(),
            Event::Failed(message) if message.contains("not connected")
        ));
    }

    #[test]
    fn execution_completion_waits_for_stream_fence_and_does_not_duplicate_the_aggregate() {
        use mathlab_engine::{ConsoleEvent, ConsoleEventKind};
        let mut test = harness();
        test.app.console.clear();
        test.app.busy = true;
        test.app.streaming_console = true;
        let result = serde_json::from_value(serde_json::json!({
            "output": "should not be duplicated",
            "error": {"identifier": "Test:Failure", "message": "source error", "stack": []},
            "workspace": [], "figures": [], "working_directory": test.app.directory,
            "duration_ms": 1.0, "figure_warnings": [],
            "console_generation": 1, "console_end_sequence": 5,
        }))
        .unwrap();
        test.app.pending_worker_event = Some((Event::Executed(result), Some((1, 5))));
        test.app.console_stream.accept(
            ConsoleEvent {
                generation: 1,
                request_id: Some(1),
                sequence: 2,
                kind: ConsoleEventKind::Output {
                    bytes: b"visible before completion".to_vec(),
                },
            },
            &mut test.app.console,
        );
        test.app.poll_events(&test.ctx);
        assert!(test.app.busy);
        assert!(test.app.pending_worker_event.is_some());
        assert_eq!(
            test.app
                .console
                .iter()
                .map(|entry| entry.text.as_str())
                .collect::<String>(),
            "visible before completion"
        );
        // A loss marker also passes an End fence that was evicted by a slow GUI.
        test.app.console_stream.accept(
            ConsoleEvent {
                generation: 1,
                request_id: None,
                sequence: 6,
                kind: ConsoleEventKind::OutputLimit { dropped_bytes: 10 },
            },
            &mut test.app.console,
        );
        test.app.poll_events(&test.ctx);
        assert!(!test.app.busy);
        assert!(test.app.pending_worker_event.is_none());
        let text = test
            .app
            .console
            .iter()
            .map(|entry| entry.text.as_str())
            .collect::<String>();
        assert!(text.starts_with("visible before completion"));
        assert!(text.ends_with("Test:Failure\nsource error"));
        assert!(!text.contains("should not be duplicated"));
    }

    #[test]
    fn interrupted_completion_waits_for_reentry_and_ping_output_without_reconnecting() {
        use mathlab_engine::{ConsoleEvent, ConsoleEventKind};
        let mut test = harness();
        test.app.console.clear();
        test.app.busy = true;
        test.app.streaming_console = true;
        let result = serde_json::from_value(serde_json::json!({
            "output": "partial output",
            "error": {"identifier": "MathLab:Interrupted", "message": "Execution interrupted. Workspace preserved.", "stack": []},
            "workspace": [], "figures": [], "working_directory": test.app.directory,
            "duration_ms": 1.0, "figure_warnings": [],
            "console_generation": 1, "console_end_sequence": 8,
        })).unwrap();
        test.app.pending_worker_event = Some((Event::Executed(result), Some((1, 8))));
        for (sequence, kind) in [
            (1, ConsoleEventKind::Begin),
            (
                2,
                ConsoleEventKind::Output {
                    bytes: b"partial output".to_vec(),
                },
            ),
            (3, ConsoleEventKind::End),
        ] {
            test.app.console_stream.accept(
                ConsoleEvent {
                    generation: 1,
                    request_id: Some(1),
                    sequence,
                    kind,
                },
                &mut test.app.console,
            );
        }
        test.app.poll_events(&test.ctx);
        assert!(
            test.app.busy,
            "the original End is not the settled completion barrier"
        );
        assert!(test.app.connected);
        for (sequence, kind) in [
            (4, ConsoleEventKind::Begin),
            (5, ConsoleEventKind::End),
            (6, ConsoleEventKind::Begin),
            (
                7,
                ConsoleEventKind::Output {
                    bytes: b"\nrecovery diagnostic\n".to_vec(),
                },
            ),
            (8, ConsoleEventKind::End),
        ] {
            test.app.console_stream.accept(
                ConsoleEvent {
                    generation: 1,
                    request_id: Some(2),
                    sequence,
                    kind,
                },
                &mut test.app.console,
            );
        }
        test.app.poll_events(&test.ctx);
        assert!(!test.app.busy);
        assert!(
            test.app.connected,
            "worker reentry is not a process disconnection"
        );
        assert_eq!(test.app.status, "Interrupted — workspace preserved");
        let text = test
            .app
            .console
            .iter()
            .map(|entry| entry.text.as_str())
            .collect::<String>();
        assert_eq!(text.matches("partial output").count(), 1);
        assert!(
            text.find("recovery diagnostic").unwrap()
                < text.find("Execution interrupted.").unwrap()
        );
    }
}
