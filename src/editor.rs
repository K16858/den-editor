mod line;
mod position;
mod size;
use line::Line;
mod annotated_string;
use annotated_string::{AnnotatedString, AnnotationType};
pub mod highlight;
mod terminal;
use crossterm::event::{Event, KeyEvent, KeyEventKind, KeyModifiers, poll, read};
use position::Position;
use size::Size;
mod document_status;
use document_status::DocumentStatus;
use serde_json::json;
use std::{
    collections::HashMap,
    env,
    io::Error,
    panic::{set_hook, take_hook},
    path::{Component, Path, PathBuf},
    process::{Command as ProcessCommand, Stdio},
};
use terminal::Terminal;
mod command;
mod debugger;
mod terminal_pane;
use debugger::{AdapterConfig, DapSession, DebugState, discover_adapter_configs};
use terminal_pane::TerminalPane;
use ui_components::{CommandBar, DebugPanel, FileTree, MessageBar, StatusBar, UIComponent, View};
mod ui_components;
use self::command::{
    Command::{self, Edit, Move, System},
    Edit::InsertNewline,
    MoveDirection,
    System::{
        Continue, CreateFile, CreateFolder, Dismiss, FocusSidebar, FocusTerminal, FocusView, Quit,
        Replace, Resize, Save, Search, StartDebug, StepInto, StepOut, StepOver, StopDebug,
        ToggleBreakpoint, ToggleSidebar, ToggleTerminal,
    },
};

pub const NAME: &str = env!("CARGO_PKG_NAME");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const QUIT_TIMES: u8 = 2;

#[derive(Eq, PartialEq, Default)]
enum PromptType {
    Search,
    Save,
    ReplaceSearch,
    Replace,
    CreateFile,
    CreateFolder,
    #[default]
    None,
}

impl PromptType {
    fn is_none(&self) -> bool {
        *self == Self::None
    }
}

#[allow(clippy::struct_excessive_bools)]
pub struct Editor {
    should_quit: bool,
    view: View,
    status_bar: StatusBar,
    terminal_size: Size,
    message_bar: MessageBar,
    command_bar: CommandBar,
    prompt_type: PromptType,
    replace_query: String,
    title: String,
    quit_times: u8,
    sidebar: FileTree,
    sidebar_visible: bool,
    sidebar_focus: bool,
    terminal_pane: TerminalPane,
    terminal_visible: bool,
    terminal_focus: bool,
    debug_panel: DebugPanel,
    debug_adapters: Vec<AdapterConfig>,
    active_debug_adapter: Option<AdapterConfig>,
    debug_session: Option<DapSession>,
    debug_state: DebugState,
    breakpoints: HashMap<PathBuf, Vec<i64>>,
    pending_configuration_done: bool,
}

impl Editor {
    pub fn new() -> Result<Self, Error> {
        let current_hook = take_hook();
        set_hook(Box::new(move |panic_info| {
            let _ = Terminal::terminate();
            current_hook(panic_info);
        }));
        Terminal::initialize()?;

        let cwd = env::current_dir()?;
        let mut workspace_root = cwd.clone();
        let mut load_path: Option<PathBuf> = None;
        let mut sidebar_visible = false;
        let mut sidebar_focus = false;
        let args: Vec<String> = env::args().collect();

        if let Some(arg) = args.get(1) {
            let p = PathBuf::from(arg);
            if p.exists() {
                if p.is_dir() {
                    workspace_root = p.canonicalize().unwrap_or(p);
                    sidebar_visible = true;
                    sidebar_focus = false;
                } else if p.is_file() {
                    let parent = p.parent().unwrap_or_else(|| Path::new("."));
                    workspace_root = parent
                        .canonicalize()
                        .unwrap_or_else(|_| parent.to_path_buf());
                    load_path = Some(p.canonicalize().unwrap_or(p));
                }
            }
        }

        let sidebar = FileTree::new(workspace_root);

        let mut editor = Self {
            should_quit: false,
            view: View::default(),
            status_bar: StatusBar::default(),
            terminal_size: Size::default(),
            message_bar: MessageBar::default(),
            command_bar: CommandBar::default(),
            prompt_type: PromptType::None,
            replace_query: String::new(),
            title: String::new(),
            quit_times: 0,
            sidebar,
            sidebar_visible,
            sidebar_focus,
            terminal_pane: TerminalPane::new(),
            terminal_visible: false,
            terminal_focus: false,
            debug_panel: DebugPanel::new(),
            debug_adapters: discover_adapter_configs(),
            active_debug_adapter: None,
            debug_session: None,
            debug_state: DebugState::default(),
            breakpoints: HashMap::new(),
            pending_configuration_done: false,
        };

        let size = Terminal::size().unwrap_or_default();
        editor.resize(size);

        if let Some(path) = load_path {
            let s = path.to_string_lossy();
            if editor.view.load(&s).is_err() {
                editor.update_message(&format!("ERROR: Could not open file: {s}"));
            }
        } else if let Some(arg) = args.get(1) {
            let p = PathBuf::from(arg);
            if !p.exists() {
                editor.update_message(&format!("ERROR: Path does not exist: {arg}"));
            }
        }

        editor.refresh_status();
        Ok(editor)
    }

    pub fn run(&mut self) {
        loop {
            let _ = self.terminal_pane.poll();
            self.poll_debug_events();
            if self.needs_refresh() {
                self.refresh_screen();
            }
            if self.should_quit {
                break;
            }
            match poll(std::time::Duration::from_millis(50)) {
                Ok(true) => match read() {
                    Ok(event) => self.evaluate_event(event),
                    Err(err) => {
                        self.update_message(&format!("Input error: {err}"));
                    }
                },
                Ok(false) => {}
                Err(err) => {
                    self.update_message(&format!("Poll error: {err}"));
                }
            }
            let _ = self.terminal_pane.poll();
            self.poll_debug_events();
            let status = self.view.get_status();
            self.status_bar.update_status(status);
        }
    }

    // =========================================
    // Event
    // =========================================
    fn evaluate_event(&mut self, event: Event) {
        if let Event::Paste(ref data) = event {
            if self.prompt_type.is_none() {
                if self.terminal_visible && self.terminal_focus {
                    let _ = self.terminal_pane.write(data.as_bytes());
                } else if !(self.sidebar_visible && self.sidebar_focus) {
                    self.view.paste_text(data);
                }
            } else {
                for ch in data.chars() {
                    self.command_bar
                        .handle_edit_command(command::Edit::Insert(ch));
                }
            }
            return;
        }

        if self.terminal_visible && self.terminal_focus && self.prompt_type.is_none() {
            if let Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) = &event
                && kind == &KeyEventKind::Press
            {
                use crossterm::event::KeyCode;
                if let (KeyCode::Null | KeyCode::Char('@' | '2'), KeyModifiers::CONTROL) =
                    (code, *modifiers)
                {
                    if let Ok(cmd) = Command::try_from(event) {
                        self.process_command(cmd);
                    }
                    return;
                }
                if *modifiers == KeyModifiers::CONTROL {
                    if let Ok(cmd) = Command::try_from(event.clone())
                        && let System(Quit | FocusView | FocusTerminal | ToggleTerminal) = cmd
                    {
                        self.process_command(cmd);
                        return;
                    }
                    if let KeyCode::Char('c') = code {
                        let _ = self.terminal_pane.write(&[0x03]);
                        return;
                    }
                    if let KeyCode::Char('d') = code {
                        let _ = self.terminal_pane.write(&[0x04]);
                        return;
                    }
                }
                let bytes = key_event_to_bytes(*code, *modifiers);
                if !bytes.is_empty() {
                    let _ = self.terminal_pane.write(&bytes);
                }
                return;
            }
            if let Event::Resize(_, _) = &event
                && let Ok(cmd) = Command::try_from(event)
            {
                self.process_command(cmd);
            }
            return;
        }

        let should_process = match &event {
            Event::Key(KeyEvent { kind, .. }) => kind == &KeyEventKind::Press,
            Event::Resize(_, _) => true,
            _ => false,
        };

        if should_process && let Ok(command) = Command::try_from(event) {
            self.process_command(command);
        }
    }

    fn process_command(&mut self, command: Command) {
        if let System(Resize(size)) = command {
            self.resize(size);
            return;
        }

        match self.prompt_type {
            PromptType::Search => self.process_command_during_search(command),
            PromptType::Save => self.process_command_during_save(command),
            PromptType::ReplaceSearch => self.process_command_during_replace_search(command),
            PromptType::Replace => self.process_command_during_replace(command),
            PromptType::CreateFile => self.process_command_during_create(command, false),
            PromptType::CreateFolder => self.process_command_during_create(command, true),
            PromptType::None => self.process_command_no_prompt(command),
        }
    }

    // =========================================
    // CommandDispatch
    // =========================================
    fn process_command_no_prompt(&mut self, command: Command) {
        if matches!(command, System(Quit)) {
            self.handle_quit();
            return;
        }
        self.reset_quit_times();

        if self.sidebar_visible && self.sidebar_focus {
            let tree_consumed = match &command {
                Move(m) if !m.is_selection => {
                    match self.sidebar.handle_move(m.direction) {
                        Ok(moved) => {
                            if moved {
                                self.sidebar.mark_redraw(true);
                            }
                        }
                        Err(e) => self.update_message(&format!("File tree error: {e}")),
                    }
                    true
                }
                Edit(InsertNewline) => {
                    if let Err(e) = self.sidebar.handle_enter() {
                        self.update_message(&format!("File tree error: {e}"));
                    }
                    self.sidebar.mark_redraw(true);
                    self.open_from_sidebar_selection();
                    true
                }
                Move(_) | Edit(_) => true,
                System(Dismiss) => {
                    self.sidebar_focus = false;
                    self.sidebar.mark_redraw(true);
                    self.view.mark_redraw(true);
                    true
                }
                System(ToggleSidebar) => {
                    self.toggle_sidebar();
                    true
                }
                System(FocusSidebar) => {
                    self.focus_sidebar();
                    true
                }
                System(FocusView) => {
                    self.focus_view();
                    true
                }
                System(_) => false,
            };
            if tree_consumed {
                return;
            }
        }

        match command {
            System(Quit | Resize(_)) => {}
            System(ToggleSidebar) => self.toggle_sidebar(),
            System(FocusSidebar) => self.focus_sidebar(),
            System(FocusView) => self.focus_view(),
            System(ToggleTerminal) => self.toggle_terminal(),
            System(FocusTerminal) => self.focus_terminal(),
            System(StartDebug) => {
                if self.debug_session.is_some() {
                    self.continue_debug();
                } else {
                    self.start_debug();
                }
            }
            System(StopDebug) => self.stop_debug(),
            System(ToggleBreakpoint) => self.toggle_breakpoint(),
            System(StepOver) => self.step_over(),
            System(StepInto) => self.step_into(),
            System(StepOut) => self.step_out(),
            System(Continue) => self.continue_debug(),
            System(Dismiss) => self.view.clear_selection(),
            System(Search) => self.set_prompt(PromptType::Search),
            System(Replace) => self.set_prompt(PromptType::ReplaceSearch),
            System(Save) => self.handle_save(),
            System(CreateFile) => self.set_prompt(PromptType::CreateFile),
            System(CreateFolder) => self.set_prompt(PromptType::CreateFolder),
            Edit(edit_command) => {
                if let Some(err) = self.view.handle_edit_command(edit_command) {
                    self.update_message(err);
                }
            }
            Move(move_command) => self.view.handle_move_command(move_command),
        }
    }

    fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
        if self.sidebar_visible {
            self.sidebar_focus = true;
            if let Err(e) = self.sidebar.rebuild() {
                self.update_message(&format!("File tree error: {e}"));
            }
        } else {
            self.sidebar_focus = false;
        }
        self.resize(self.terminal_size);
        self.sidebar.mark_redraw(true);
        self.view.mark_redraw(true);
        self.debug_panel.mark_redraw(true);
    }

    fn focus_sidebar(&mut self) {
        if !self.sidebar_visible {
            self.sidebar_visible = true;
            self.resize(self.terminal_size);
        }
        self.sidebar_focus = true;
        if let Err(e) = self.sidebar.rebuild() {
            self.update_message(&format!("File tree error: {e}"));
        }
        self.sidebar.mark_redraw(true);
        self.view.mark_redraw(true);
        self.debug_panel.mark_redraw(true);
    }

    fn focus_view(&mut self) {
        self.sidebar_focus = false;
        self.terminal_focus = false;
        self.sidebar.mark_redraw(true);
        self.view.mark_redraw(true);
        self.debug_panel.mark_redraw(true);
    }

    fn toggle_terminal(&mut self) {
        self.terminal_visible = !self.terminal_visible;
        if self.terminal_visible {
            self.terminal_focus = true;
            self.sidebar_focus = false;
            self.start_terminal_if_needed();
        } else {
            self.terminal_focus = false;
        }
        let size = self.terminal_size;
        self.resize(size);
        self.view.mark_redraw(true);
        self.terminal_pane.mark_redraw(true);
        self.debug_panel.mark_redraw(true);
    }

    fn focus_terminal(&mut self) {
        if !self.terminal_visible {
            self.terminal_visible = true;
            self.start_terminal_if_needed();
            let size = self.terminal_size;
            self.resize(size);
        }
        self.terminal_focus = true;
        self.sidebar_focus = false;
        self.terminal_pane.mark_redraw(true);
        self.view.mark_redraw(true);
        self.debug_panel.mark_redraw(true);
    }

    fn start_terminal_if_needed(&mut self) {
        if self.terminal_pane.is_running() {
            return;
        }
        let cwd = self.sidebar.workspace_root().to_path_buf();
        let sidebar_w = if self.sidebar_visible {
            FileTree::WIDTH
        } else {
            0
        };
        #[allow(clippy::cast_possible_truncation)]
        let cols = self.terminal_size.width.saturating_sub(sidebar_w) as u16;
        if let Err(e) = self.terminal_pane.start(&cwd, cols) {
            self.update_message(&format!("Terminal error: {e}"));
        }
    }

    fn open_from_sidebar_selection(&mut self) {
        if let Some(path) = self.sidebar.take_pending_open() {
            let s = path.to_string_lossy();
            match self.view.load(&s) {
                Ok(()) => {
                    self.sidebar_focus = false;
                    self.refresh_status();
                    self.update_message("");
                }
                Err(e) => {
                    self.update_message(&format!("ERROR: Could not open file: {e}"));
                }
            }
            self.view.mark_redraw(true);
            self.sidebar.mark_redraw(true);
        }
    }

    fn active_file_extension(&self) -> Option<String> {
        self.view.file_path().and_then(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(str::to_lowercase)
        })
    }

    fn select_debug_adapter(&self) -> Option<&AdapterConfig> {
        let ext = self.active_file_extension()?;
        self.debug_adapters.iter().find(|adapter| {
            adapter
                .file_extensions
                .iter()
                .any(|candidate| candidate == &ext)
        })
    }

    fn start_debug(&mut self) {
        if self.debug_session.is_some() {
            self.update_message("Debug session is already running.");
            return;
        }
        let Some(adapter) = self.select_debug_adapter().cloned() else {
            self.update_message("No debug adapter for current file.");
            return;
        };
        if let Err(msg) = Self::ensure_adapter_ready(&adapter) {
            self.update_message(&msg);
            return;
        }
        match DapSession::start(&adapter) {
            Ok(mut session) => {
                if let Err(e) = session.send_request(
                    "initialize",
                    json!({
                        "clientID": "den",
                        "clientName": "den",
                        "adapterID": adapter.dap_adapter_type,
                        "linesStartAt1": true,
                        "columnsStartAt1": true,
                        "pathFormat": "path"
                    }),
                ) {
                    self.update_message(&format!("Debug init error: {e}"));
                    return;
                }
                let launch_args = match self.build_launch_arguments(&adapter) {
                    Ok(args) => args,
                    Err(msg) => {
                        self.update_message(&msg);
                        return;
                    }
                };
                if let Err(e) = session.send_request("launch", launch_args) {
                    self.update_message(&format!("Debug launch error: {e}"));
                    return;
                }
                self.debug_session = Some(session);
                self.active_debug_adapter = Some(adapter.clone());
                self.debug_state.active = true;
                self.pending_configuration_done = true;
                self.debug_panel.update(&self.debug_state);
                self.update_message(&format!("Debug started: {}", adapter.display_name));
            }
            Err(e) => {
                self.update_message(&format!("Debug start error: {e}"));
            }
        }
    }

    fn stop_debug(&mut self) {
        if let Some(session) = &mut self.debug_session {
            session.stop();
        }
        self.debug_session = None;
        self.active_debug_adapter = None;
        self.debug_state.active = false;
        self.pending_configuration_done = false;
        self.debug_state.current_thread_id = None;
        self.debug_state.threads.clear();
        self.debug_state.stack_frames.clear();
        self.debug_state.variables.clear();
        self.debug_panel.update(&self.debug_state);
        self.update_message("Debug stopped.");
    }

    fn poll_debug_events(&mut self) {
        let mut should_stop = false;
        let mut had_activity = false;
        loop {
            let event = self.debug_session.as_ref().and_then(DapSession::try_recv);
            let Some(event) = event else { break };
            had_activity = true;
            match event {
                debugger::DapEvent::Message(envelope) => match envelope.message {
                    debugger::DapMessage::Event { event, body, .. } => {
                        if event == "initialized" {
                            self.sync_all_breakpoints();
                            if self.pending_configuration_done {
                                self.with_debug_session(|session| {
                                    session
                                        .send_request("configurationDone", json!({}))
                                        .map_err(|e| format!("Debug configuration error: {e}"))?;
                                    Ok(())
                                });
                                self.pending_configuration_done = false;
                            }
                        } else if event == "stopped" {
                            self.debug_state.current_thread_id =
                                body.get("threadId").and_then(serde_json::Value::as_i64);
                            self.request_threads();
                            self.request_stack_trace();
                            self.update_message("Debug paused.");
                        }
                    }
                    debugger::DapMessage::Response {
                        success,
                        command,
                        body,
                        ..
                    } => {
                        if success {
                            self.handle_debug_response(&command, &body);
                        }
                    }
                    debugger::DapMessage::Request { .. } => {}
                },
                debugger::DapEvent::Closed => should_stop = true,
                debugger::DapEvent::Error(e) => self.update_message(&format!("DAP error: {e}")),
            }
        }
        if should_stop {
            self.stop_debug();
        } else if had_activity {
            self.status_bar.mark_redraw(true);
            self.debug_panel.update(&self.debug_state);
        }
    }

    fn request_threads(&mut self) {
        self.with_debug_session(|session| {
            session
                .send_request("threads", json!({}))
                .map_err(|e| format!("threads request error: {e}"))?;
            Ok(())
        });
    }

    fn request_stack_trace(&mut self) {
        let thread_id = self.debug_state.current_thread_id.unwrap_or(0);
        self.with_debug_session(|session| {
            session
                .send_request(
                    "stackTrace",
                    json!({
                        "threadId": thread_id,
                        "startFrame": 0,
                        "levels": 20
                    }),
                )
                .map_err(|e| format!("stackTrace request error: {e}"))?;
            Ok(())
        });
    }

    fn request_scopes(&mut self, frame_id: i64) {
        self.with_debug_session(|session| {
            session
                .send_request("scopes", json!({ "frameId": frame_id }))
                .map_err(|e| format!("scopes request error: {e}"))?;
            Ok(())
        });
    }

    fn request_variables(&mut self, reference: i64) {
        self.with_debug_session(|session| {
            session
                .send_request("variables", json!({ "variablesReference": reference }))
                .map_err(|e| format!("variables request error: {e}"))?;
            Ok(())
        });
    }

    fn handle_debug_response(&mut self, command: &str, body: &serde_json::Value) {
        match command {
            "threads" => {
                let threads = body
                    .get("threads")
                    .and_then(serde_json::Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .map(|v| debugger::ThreadSummary {
                                id: v.get("id").and_then(serde_json::Value::as_i64).unwrap_or(0),
                                name: v
                                    .get("name")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                self.debug_state.threads = threads;
            }
            "stackTrace" => {
                let frames = body
                    .get("stackFrames")
                    .and_then(serde_json::Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .map(|v| debugger::StackFrameSummary {
                                id: v.get("id").and_then(serde_json::Value::as_i64).unwrap_or(0),
                                name: v
                                    .get("name")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                                line: v
                                    .get("line")
                                    .and_then(serde_json::Value::as_i64)
                                    .unwrap_or(0),
                                column: v
                                    .get("column")
                                    .and_then(serde_json::Value::as_i64)
                                    .unwrap_or(0),
                                source_path: v
                                    .get("source")
                                    .and_then(|s| s.get("path"))
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let frame_id = frames.first().map_or(0, |f| f.id);
                self.debug_state.stack_frames = frames;
                if frame_id != 0 {
                    self.request_scopes(frame_id);
                }
            }
            "scopes" => {
                let reference = body
                    .get("scopes")
                    .and_then(serde_json::Value::as_array)
                    .and_then(|arr| arr.first())
                    .and_then(|scope| scope.get("variablesReference"))
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0);
                if reference != 0 {
                    self.request_variables(reference);
                }
            }
            "variables" => {
                let vars = body
                    .get("variables")
                    .and_then(serde_json::Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .map(|v| debugger::VariableSummary {
                                name: v
                                    .get("name")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                                value: v
                                    .get("value")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                                type_name: v
                                    .get("type")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                                variables_reference: v
                                    .get("variablesReference")
                                    .and_then(serde_json::Value::as_i64)
                                    .unwrap_or(0),
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                self.debug_state.variables = vars;
            }
            _ => {}
        }
    }

    fn build_launch_arguments(&self, adapter: &AdapterConfig) -> Result<serde_json::Value, String> {
        let file_path = self
            .view
            .file_path()
            .ok_or_else(|| "Open a file before starting debug.".to_string())?;
        let workspace = self.sidebar.workspace_root().to_path_buf();

        if adapter.dap_adapter_type.eq_ignore_ascii_case("debugpy") {
            return Ok(json!({
                "name": "Debug current file",
                "type": "python",
                "request": "launch",
                "program": file_path,
                "cwd": workspace
            }));
        }

        if adapter.dap_adapter_type.eq_ignore_ascii_case("codelldb") {
            let stem = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| "Could not resolve binary name from file.".to_string())?;
            let exe = if cfg!(windows) {
                workspace
                    .join("target")
                    .join("debug")
                    .join(format!("{stem}.exe"))
            } else {
                workspace.join("target").join("debug").join(stem)
            };
            return Ok(json!({
                "name": "Debug current binary",
                "type": "lldb",
                "request": "launch",
                "program": exe,
                "cwd": workspace
            }));
        }

        if adapter.dap_adapter_type.eq_ignore_ascii_case("dlv-dap") {
            return Ok(json!({
                "name": "Debug workspace package",
                "type": "go",
                "request": "launch",
                "mode": "debug",
                "program": workspace,
                "cwd": workspace
            }));
        }

        Err(format!(
            "Unsupported adapter type for launch: {}",
            adapter.dap_adapter_type
        ))
    }

    fn ensure_adapter_ready(adapter: &AdapterConfig) -> Result<(), String> {
        if adapter.dap_adapter_type.eq_ignore_ascii_case("debugpy") {
            let python_ok = ProcessCommand::new("python")
                .args(["-c", "import debugpy.adapter"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            if python_ok {
                return Ok(());
            }

            let py_launcher_ok = ProcessCommand::new("py")
                .args(["-3", "-c", "import debugpy.adapter"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            if py_launcher_ok {
                return Ok(());
            }

            Err("Python/debugpy not found. Install: python -m pip install debugpy (or py -3 -m pip install debugpy)".to_string())
        } else {
            let mut cmd = ProcessCommand::new(&adapter.command);
            if adapter.dap_adapter_type.eq_ignore_ascii_case("dlv-dap") {
                cmd.arg("version");
            } else {
                cmd.arg("--version");
            }
            match cmd
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
            {
                Ok(_) => Ok(()),
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        if adapter.dap_adapter_type.eq_ignore_ascii_case("codelldb") {
                            Err("Rust debug adapter 'codelldb' is missing. Install VS Code CodeLLDB extension or add codelldb to PATH.".to_string())
                        } else if adapter.dap_adapter_type.eq_ignore_ascii_case("dlv-dap") {
                            Err("Go debug adapter 'dlv' is missing. Install: go install github.com/go-delve/delve/cmd/dlv@latest and add GOPATH/bin to PATH.".to_string())
                        } else {
                            Err(format!(
                                "Debug adapter command not found: {}. Install it and add to PATH.",
                                adapter.command
                            ))
                        }
                    } else {
                        Err(format!(
                            "Failed to execute debug adapter '{}': {e}",
                            adapter.command
                        ))
                    }
                }
            }
        }
    }

    fn with_debug_session<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut DapSession) -> Result<(), String>,
    {
        let Some(session) = &mut self.debug_session else {
            self.update_message("Debug session is not running.");
            return;
        };
        if let Err(e) = f(session) {
            self.update_message(&e);
        }
    }

    fn active_file_path(&self) -> Option<PathBuf> {
        self.view.file_path()
    }

    fn toggle_breakpoint(&mut self) {
        let Some(path) = self.active_file_path() else {
            self.update_message("Open a file before toggling breakpoints.");
            return;
        };
        let line =
            i64::try_from(self.view.current_line_index().saturating_add(1)).unwrap_or(i64::MAX);
        let entry = self.breakpoints.entry(path.clone()).or_default();
        if let Some(pos) = entry.iter().position(|l| *l == line) {
            entry.remove(pos);
        } else {
            entry.push(line);
            entry.sort_unstable();
        }
        let lines = entry.clone();

        self.with_debug_session(|session| {
            session
                .send_request(
                    "setBreakpoints",
                    json!({
                        "source": { "path": path },
                        "breakpoints": lines.iter().map(|line| json!({ "line": line })).collect::<Vec<_>>()
                    }),
                )
                .map_err(|e| format!("setBreakpoints error: {e}"))?;
            Ok(())
        });
    }

    fn sync_all_breakpoints(&mut self) {
        let all = self.breakpoints.clone();
        self.with_debug_session(|session| {
            for (path, lines) in &all {
                session
                    .send_request(
                        "setBreakpoints",
                        json!({
                            "source": { "path": path },
                            "breakpoints": lines.iter().map(|line| json!({ "line": line })).collect::<Vec<_>>()
                        }),
                    )
                    .map_err(|e| format!("setBreakpoints sync error: {e}"))?;
            }
            Ok(())
        });
    }

    fn continue_debug(&mut self) {
        let thread_id = self.debug_state.current_thread_id.unwrap_or(0);
        self.with_debug_session(|session| {
            session
                .send_request("continue", json!({ "threadId": thread_id }))
                .map_err(|e| format!("Continue error: {e}"))?;
            Ok(())
        });
    }

    fn step_over(&mut self) {
        let thread_id = self.debug_state.current_thread_id.unwrap_or(0);
        self.with_debug_session(|session| {
            session
                .send_request("next", json!({ "threadId": thread_id }))
                .map_err(|e| format!("StepOver error: {e}"))?;
            Ok(())
        });
    }

    fn step_into(&mut self) {
        let thread_id = self.debug_state.current_thread_id.unwrap_or(0);
        self.with_debug_session(|session| {
            session
                .send_request("stepIn", json!({ "threadId": thread_id }))
                .map_err(|e| format!("StepInto error: {e}"))?;
            Ok(())
        });
    }

    fn step_out(&mut self) {
        let thread_id = self.debug_state.current_thread_id.unwrap_or(0);
        self.with_debug_session(|session| {
            session
                .send_request("stepOut", json!({ "threadId": thread_id }))
                .map_err(|e| format!("StepOut error: {e}"))?;
            Ok(())
        });
    }

    fn process_command_during_search(&mut self, command: Command) {
        match command {
            System(Dismiss) => {
                self.set_prompt(PromptType::None);
                self.view.dismiss_search();
            }
            Edit(InsertNewline) => {
                self.set_prompt(PromptType::None);
                self.view.exit_search();
            }
            Edit(edit_command) => {
                self.command_bar.handle_edit_command(edit_command);
                let query = self.command_bar.value();
                self.view.search(&query);
            }
            Move(move_cmd)
                if matches!(
                    move_cmd.direction,
                    MoveDirection::Right | MoveDirection::Down
                ) =>
            {
                self.view.search_next();
            }
            Move(move_cmd)
                if matches!(move_cmd.direction, MoveDirection::Up | MoveDirection::Left) =>
            {
                self.view.search_prev();
            }
            System(
                Quit | Resize(_) | Search | Save | Replace | ToggleSidebar | FocusSidebar
                | FocusView | CreateFile | CreateFolder | ToggleTerminal | FocusTerminal
                | StartDebug | StopDebug | ToggleBreakpoint | StepOver | StepInto | StepOut
                | Continue,
            )
            | Move(_) => {}
        }
    }

    fn process_command_during_save(&mut self, command: Command) {
        match command {
            System(
                Quit | Resize(_) | Search | Save | Replace | ToggleSidebar | FocusSidebar
                | FocusView | CreateFile | CreateFolder | ToggleTerminal | FocusTerminal
                | StartDebug | StopDebug | ToggleBreakpoint | StepOver | StepInto | StepOut
                | Continue,
            )
            | Move(_) => {} // Not applicable during save, Resize already handled at this stage
            System(Dismiss) => {
                self.set_prompt(PromptType::None);
                self.update_message("Save aborted.");
            }
            Edit(InsertNewline) => {
                let file_name = self.command_bar.value();
                self.save(Some(&file_name));
                self.set_prompt(PromptType::None);
            }
            Edit(edit_command) => self.command_bar.handle_edit_command(edit_command),
        }
    }

    fn process_command_during_replace_search(&mut self, command: Command) {
        match command {
            System(Dismiss) => {
                self.set_prompt(PromptType::None);
                self.view.dismiss_search();
            }
            Edit(InsertNewline) => {
                self.replace_query = self.command_bar.value();
                self.set_prompt(PromptType::Replace);
            }
            Edit(edit_command) => {
                self.command_bar.handle_edit_command(edit_command);
                let query = self.command_bar.value();
                self.view.search(&query);
            }
            Move(move_cmd)
                if matches!(
                    move_cmd.direction,
                    MoveDirection::Right | MoveDirection::Down
                ) =>
            {
                self.view.search_next();
            }
            Move(move_cmd)
                if matches!(move_cmd.direction, MoveDirection::Up | MoveDirection::Left) =>
            {
                self.view.search_prev();
            }
            System(
                Quit | Resize(_) | Search | Save | Replace | ToggleSidebar | FocusSidebar
                | FocusView | CreateFile | CreateFolder | ToggleTerminal | FocusTerminal
                | StartDebug | StopDebug | ToggleBreakpoint | StepOver | StepInto | StepOut
                | Continue,
            )
            | Move(_) => {}
        }
    }

    fn process_command_during_replace(&mut self, command: Command) {
        match command {
            System(Dismiss) => {
                self.set_prompt(PromptType::None);
                self.view.dismiss_search();
            }
            Edit(InsertNewline) => {
                let replacement = self.command_bar.value();
                let query = self.replace_query.clone();
                self.view.exit_search();
                let count = self.view.replace_all(&query, &replacement);
                self.set_prompt(PromptType::None);
                if count == 0 {
                    self.update_message("No matches found.");
                } else {
                    self.update_message(&format!("Replaced {count} occurrence(s)."));
                }
            }
            Edit(edit_command) => self.command_bar.handle_edit_command(edit_command),
            System(
                Quit | Resize(_) | Search | Save | Replace | ToggleSidebar | FocusSidebar
                | FocusView | CreateFile | CreateFolder | ToggleTerminal | FocusTerminal
                | StartDebug | StopDebug | ToggleBreakpoint | StepOver | StepInto | StepOut
                | Continue,
            )
            | Move(_) => {}
        }
    }

    fn process_command_during_create(&mut self, command: Command, is_dir: bool) {
        match command {
            System(Dismiss) => {
                self.set_prompt(PromptType::None);
                self.update_message("Cancelled.");
            }
            Edit(InsertNewline) => {
                let name = self.command_bar.value();
                self.set_prompt(PromptType::None);
                if name.is_empty() {
                    self.update_message("Name is empty.");
                    return;
                }
                let base = self.create_base_dir();
                let target = match self.resolve_workspace_target(&base, &name) {
                    Ok(path) => path,
                    Err(msg) => {
                        self.update_message(&msg);
                        return;
                    }
                };
                if is_dir {
                    match std::fs::create_dir_all(&target) {
                        Ok(()) => {
                            self.update_message(&format!("Created: {name}"));
                            if let Err(e) = self.sidebar.rebuild() {
                                self.update_message(&format!("File tree error: {e}"));
                            }
                            self.sidebar.mark_redraw(true);
                        }
                        Err(e) => self.update_message(&format!("Error: {e}")),
                    }
                } else {
                    if let Some(parent) = target.parent()
                        && let Err(e) = std::fs::create_dir_all(parent)
                    {
                        self.update_message(&format!("Error: {e}"));
                        return;
                    }
                    match std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&target)
                    {
                        Ok(_) => {
                            self.update_message(&format!("Created: {name}"));
                            if let Err(e) = self.sidebar.rebuild() {
                                self.update_message(&format!("File tree error: {e}"));
                            }
                            self.sidebar.mark_redraw(true);
                            let path_str = target.to_string_lossy().to_string();
                            if let Err(e) = self.view.load(&path_str) {
                                self.update_message(&format!("Open error: {e}"));
                            } else {
                                self.status_bar.mark_redraw(true);
                            }
                        }
                        Err(e) => self.update_message(&format!("Error: {e}")),
                    }
                }
            }
            Edit(edit_command) => self.command_bar.handle_edit_command(edit_command),
            System(
                Quit | Resize(_) | Search | Save | Replace | ToggleSidebar | FocusSidebar
                | FocusView | CreateFile | CreateFolder | ToggleTerminal | FocusTerminal
                | StartDebug | StopDebug | ToggleBreakpoint | StepOver | StepInto | StepOut
                | Continue,
            )
            | Move(_) => {}
        }
    }

    fn create_base_dir(&self) -> std::path::PathBuf {
        if let Some(p) = self.sidebar.selected_path() {
            if p.is_dir() {
                return p;
            }
            if let Some(parent) = p.parent() {
                return parent.to_path_buf();
            }
        }
        self.sidebar.workspace_root().to_path_buf()
    }

    fn resolve_workspace_target(&self, base: &Path, input: &str) -> Result<PathBuf, String> {
        let candidate = Path::new(input);
        if candidate.is_absolute() {
            return Err("Invalid path: absolute paths are not allowed".to_string());
        }

        let mut relative = PathBuf::new();
        for component in candidate.components() {
            match component {
                Component::Normal(part) => relative.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err("Invalid path: outside workspace".to_string());
                }
            }
        }

        if relative.as_os_str().is_empty() {
            return Err("Invalid path: name is empty".to_string());
        }

        let root = self
            .sidebar
            .workspace_root()
            .canonicalize()
            .map_err(|e| format!("Workspace path error: {e}"))?;
        let base = base
            .canonicalize()
            .map_err(|e| format!("Base path error: {e}"))?;
        if !base.starts_with(&root) {
            return Err("Invalid path: outside workspace".to_string());
        }

        let target = base.join(relative);
        if !target.starts_with(&root) {
            return Err("Invalid path: outside workspace".to_string());
        }
        Ok(target)
    }

    // =========================================
    // PromptHandling
    // =========================================
    fn set_prompt(&mut self, prompt_type: PromptType) {
        match prompt_type {
            PromptType::None => self.message_bar.mark_redraw(true),
            PromptType::Save => self.command_bar.set_prompt("Save as: "),
            PromptType::Search => {
                self.view.enter_search();
                self.command_bar
                    .set_prompt("Search (Esc to cancel, Arrows to navigate): ");
            }
            PromptType::ReplaceSearch => {
                self.view.enter_search();
                self.command_bar.set_prompt("Replace (search): ");
            }
            PromptType::Replace => {
                self.command_bar.set_prompt("Replace with: ");
            }
            PromptType::CreateFile => {
                self.command_bar.set_prompt("New file name: ");
            }
            PromptType::CreateFolder => {
                self.command_bar.set_prompt("New folder name: ");
            }
        }
        self.command_bar.clear_value();
        self.prompt_type = prompt_type;
    }

    fn in_prompt(&self) -> bool {
        !self.prompt_type.is_none()
    }

    // =========================================
    // SystemCommands
    // =========================================
    fn handle_save(&mut self) {
        if self.view.is_file_loaded() {
            self.save(None);
        } else {
            self.set_prompt(PromptType::Save);
        }
    }

    fn save(&mut self, file_name: Option<&str>) {
        let result = if let Some(name) = file_name {
            self.view.save_as(name)
        } else {
            self.view.save()
        };
        if result.is_ok() {
            self.update_message("File saved successfully.");
        } else {
            self.update_message("Error writing file!");
        }
    }

    fn handle_quit(&mut self) {
        if !self.view.get_status().is_modified || self.quit_times + 1 == QUIT_TIMES {
            self.should_quit = true;
        } else if self.view.get_status().is_modified {
            self.update_message(&format!(
                "WARNING! File has unsaved changes. Press Ctrl-Q {} more times to quit.",
                QUIT_TIMES - self.quit_times - 1
            ));

            self.quit_times += 1;
        }
    }

    fn reset_quit_times(&mut self) {
        if self.quit_times > 0 {
            self.quit_times = 0;
            self.message_bar.update_message("");
        }
    }

    // =========================================
    // Rendering
    // =========================================
    fn needs_refresh(&self) -> bool {
        self.view.needs_redraw()
            || self.sidebar.needs_redraw()
            || self.status_bar.needs_redraw()
            || self.message_bar.needs_redraw()
            || self.command_bar.needs_redraw()
            || self.debug_panel.needs_redraw()
            || (self.terminal_visible && self.terminal_pane.needs_redraw())
    }

    pub fn refresh_status(&mut self) {
        let status = self.view.get_status();
        let title = format!("{} - {NAME}", status.file_name);
        self.status_bar.update_status(status);

        if title != self.title && matches!(Terminal::set_title(&title), Ok(())) {
            self.title = title;
        }
    }

    fn resize(&mut self, size: Size) {
        self.terminal_size = size;
        let term_rows = if self.terminal_visible {
            self.terminal_pane.rows
        } else {
            0
        };
        let debug_rows = if self.debug_state.active {
            self.debug_panel.rows
        } else {
            0
        };
        let main_height = size
            .height
            .saturating_sub(2)
            .saturating_sub(term_rows)
            .saturating_sub(debug_rows);
        let sidebar_w = if self.sidebar_visible {
            FileTree::WIDTH
        } else {
            0
        };
        let right_width = size.width.saturating_sub(sidebar_w);

        self.view.set_col_offset(sidebar_w);
        self.view.resize(Size {
            height: main_height,
            width: right_width,
        });
        self.sidebar.resize(Size {
            height: main_height + term_rows + debug_rows,
            width: FileTree::WIDTH,
        });
        self.debug_panel.resize(Size {
            height: debug_rows,
            width: right_width,
        });
        self.debug_panel.set_col_offset(sidebar_w);
        self.terminal_pane.size = Size {
            height: term_rows,
            width: right_width,
        };
        if self.terminal_visible && self.terminal_pane.is_running() {
            #[allow(clippy::cast_possible_truncation)]
            let _ = self.terminal_pane.resize_pty(right_width as u16);
        }
        let bar_size = Size {
            height: 1,
            width: size.width,
        };
        self.message_bar.resize(bar_size);
        self.status_bar.resize(bar_size);
        self.command_bar.resize(bar_size);
    }

    fn refresh_screen(&mut self) {
        if self.terminal_size.height == 0 || self.terminal_size.width == 0 {
            return;
        }
        let _ = Terminal::begin_synchronized_update();
        let bottom_bar_row = self.terminal_size.height.saturating_sub(1);
        let _ = Terminal::hide_caret();
        if self.in_prompt() {
            self.command_bar.render(bottom_bar_row);
        } else {
            self.message_bar.render(bottom_bar_row);
        }
        if self.terminal_size.height > 1 {
            self.status_bar
                .render(self.terminal_size.height.saturating_sub(2));
        }
        let term_rows = if self.terminal_visible {
            self.terminal_pane.rows
        } else {
            0
        };
        let debug_rows = if self.debug_state.active {
            self.debug_panel.rows
        } else {
            0
        };
        let main_height = self
            .terminal_size
            .height
            .saturating_sub(2)
            .saturating_sub(term_rows)
            .saturating_sub(debug_rows);
        if main_height > 0 {
            if self.sidebar_visible {
                self.sidebar.render(0);
            }
            self.view.render(0);
        }
        let sidebar_w = if self.sidebar_visible {
            FileTree::WIDTH
        } else {
            0
        };
        if self.debug_state.active && debug_rows > 0 {
            self.debug_panel.render(main_height);
        }
        if self.terminal_visible {
            let term_origin = main_height + debug_rows;
            if self.terminal_pane.needs_redraw() {
                let _ = self.terminal_pane.draw(term_origin, sidebar_w);
            }
        }

        let new_caret_pos = if self.in_prompt() {
            Position {
                row: bottom_bar_row,
                col: self.command_bar.caret_position_col(),
            }
        } else if self.sidebar_visible && self.sidebar_focus {
            self.sidebar.caret_position(0)
        } else if self.terminal_focus && self.terminal_visible {
            self.terminal_pane
                .cursor_position(main_height + debug_rows, sidebar_w)
        } else {
            self.view.caret_position()
        };

        let _ = Terminal::move_caret_to(new_caret_pos);
        let _ = Terminal::show_caret();
        let _ = Terminal::end_synchronized_update();
        let _ = Terminal::execute();
    }

    // =========================================
    // Util
    // =========================================
    fn update_message(&mut self, new_message: &str) {
        self.message_bar.update_message(new_message);
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.stop_debug();
        self.terminal_pane.stop();
        let _ = Terminal::terminate();
    }
}

fn key_event_to_bytes(code: crossterm::event::KeyCode, modifiers: KeyModifiers) -> Vec<u8> {
    use crossterm::event::KeyCode;
    if modifiers == KeyModifiers::NONE || modifiers == KeyModifiers::SHIFT {
        match code {
            KeyCode::Char(c) => {
                let mut buf = [0u8; 4];
                c.encode_utf8(&mut buf).as_bytes().to_vec()
            }
            KeyCode::Enter => b"\r".to_vec(),
            KeyCode::Backspace => b"\x7f".to_vec(),
            KeyCode::Tab => b"\t".to_vec(),
            KeyCode::Up => b"\x1b[A".to_vec(),
            KeyCode::Down => b"\x1b[B".to_vec(),
            KeyCode::Right => b"\x1b[C".to_vec(),
            KeyCode::Left => b"\x1b[D".to_vec(),
            KeyCode::Home => b"\x1b[H".to_vec(),
            KeyCode::End => b"\x1b[F".to_vec(),
            KeyCode::Delete => b"\x1b[3~".to_vec(),
            KeyCode::Esc => b"\x1b".to_vec(),
            _ => vec![],
        }
    } else {
        vec![]
    }
}
