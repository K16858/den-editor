mod line;
mod position;
mod size;
use line::Line;
mod annotated_string;
use annotated_string::{AnnotatedString, AnnotationType};
pub mod highlight;
mod terminal;
use crossterm::event::{Event, KeyEvent, KeyEventKind, read};
use position::Position;
use size::Size;
mod document_status;
use document_status::DocumentStatus;
use std::{
    env,
    io::Error,
    panic::{set_hook, take_hook},
    path::{Path, PathBuf},
};
use terminal::Terminal;
mod command;
use ui_components::{CommandBar, FileTree, MessageBar, StatusBar, UIComponent, View};
mod ui_components;
use self::command::{
    Command::{self, Edit, Move, System},
    Edit::InsertNewline,
    MoveDirection,
    System::{Dismiss, FocusSidebar, Quit, Replace, Resize, Save, Search, ToggleSidebar},
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
    #[default]
    None,
}

impl PromptType {
    fn is_none(&self) -> bool {
        *self == Self::None
    }
}

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
            self.refresh_screen();
            if self.should_quit {
                break;
            }
            match read() {
                Ok(event) => self.evaluate_event(event),
                Err(err) => {
                    self.update_message(&format!("Input error: {err}"));
                }
            }
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
                if !(self.sidebar_visible && self.sidebar_focus) {
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
                    if self.sidebar.handle_move(m.direction) {
                        self.sidebar.mark_redraw(true);
                    }
                    true
                }
                Edit(InsertNewline) => {
                    self.sidebar.handle_enter();
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
            System(Dismiss) => self.view.clear_selection(),
            System(Search) => self.set_prompt(PromptType::Search),
            System(Replace) => self.set_prompt(PromptType::ReplaceSearch),
            System(Save) => self.handle_save(),
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
            self.sidebar.rebuild();
        } else {
            self.sidebar_focus = false;
        }
        self.resize(self.terminal_size);
        self.sidebar.mark_redraw(true);
        self.view.mark_redraw(true);
    }

    fn focus_sidebar(&mut self) {
        if !self.sidebar_visible {
            self.sidebar_visible = true;
            self.resize(self.terminal_size);
        }
        self.sidebar_focus = true;
        self.sidebar.rebuild();
        self.sidebar.mark_redraw(true);
        self.view.mark_redraw(true);
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
            System(Quit | Resize(_) | Search | Save | Replace | ToggleSidebar | FocusSidebar)
            | Move(_) => {}
        }
    }

    fn process_command_during_save(&mut self, command: Command) {
        match command {
            System(
                Quit | Resize(_) | Search | Save | Replace | ToggleSidebar | FocusSidebar,
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
            System(Quit | Resize(_) | Search | Save | Replace | ToggleSidebar | FocusSidebar)
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
            System(Quit | Resize(_) | Search | Save | Replace | ToggleSidebar | FocusSidebar)
            | Move(_) => {}
        }
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
        let main_height = size.height.saturating_sub(2);
        let sidebar_w = if self.sidebar_visible {
            FileTree::WIDTH
        } else {
            0
        };
        self.view.set_col_offset(sidebar_w);
        self.view.resize(Size {
            height: main_height,
            width: size.width.saturating_sub(sidebar_w),
        });
        self.sidebar.resize(Size {
            height: main_height,
            width: FileTree::WIDTH,
        });
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
        if self.terminal_size.height > 2 {
            if self.sidebar_visible {
                self.sidebar.render(0);
            }
            self.view.render(0);
        }

        let new_caret_pos = if self.in_prompt() {
            Position {
                row: bottom_bar_row,
                col: self.command_bar.caret_position_col(),
            }
        } else if self.sidebar_visible && self.sidebar_focus {
            self.sidebar.caret_position(0)
        } else {
            self.view.caret_position()
        };

        let _ = Terminal::move_caret_to(new_caret_pos);
        let _ = Terminal::show_caret();
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
        let _ = Terminal::terminate();
    }
}
