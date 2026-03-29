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
};
use terminal::Terminal;
mod command;
use ui_components::{CommandBar, MessageBar, StatusBar, UIComponent, View};
mod ui_components;
use self::command::{
    Command::{self, Edit, Move, System},
    Edit::InsertNewline,
    MoveDirection,
    System::{Dismiss, Quit, Replace, Resize, Save, Search},
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

#[derive(Default)]
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
}

impl Editor {
    pub fn new() -> Result<Self, Error> {
        let current_hook = take_hook();
        set_hook(Box::new(move |panic_info| {
            let _ = Terminal::terminate();
            current_hook(panic_info);
        }));
        Terminal::initialize()?;

        let mut editor = Self::default();
        let size = Terminal::size().unwrap_or_default();
        editor.resize(size);
        // editor.update_message("HELP: Ctrl-S = save | Ctrl-Q = quit | Ctrl-C = copy | Ctrl-X = cut | Ctrl-V = paste | Ctrl-F = find | Ctrl-Z = undo | Ctrl-Shift-Z = redo | Ctrl-A = select all");

        let args: Vec<String> = env::args().collect();
        if let Some(file_name) = args.get(1)
            && editor.view.load(file_name).is_err()
        {
            editor.update_message(&format!("ERROR: Could not open file: {file_name}"));
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
        if let Event::Paste(data) = event {
            if self.prompt_type.is_none() {
                self.view.paste_text(&data);
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

        match command {
            System(Quit | Resize(_)) => {}
            System(Dismiss) => self.view.clear_selection(),
            System(Search) => self.set_prompt(PromptType::Search),
            System(Replace) => self.set_prompt(PromptType::ReplaceSearch),
            System(Save) => self.handle_save(),
            Edit(edit_command) => self.view.handle_edit_command(edit_command),

            Move(move_command) => self.view.handle_move_command(move_command),
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
            System(Quit | Resize(_) | Search | Save | Replace) | Move(_) => {}
        }
    }

    fn process_command_during_save(&mut self, command: Command) {
        match command {
            System(Quit | Resize(_) | Search | Save | Replace) | Move(_) => {} // Not applicable during save, Resize already handled at this stage
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
            System(Quit | Resize(_) | Search | Save | Replace) | Move(_) => {}
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
                    self.update_message(&format!("{count} 箇所を置換しました。"));
                }
            }
            Edit(edit_command) => self.command_bar.handle_edit_command(edit_command),
            System(Quit | Resize(_) | Search | Save | Replace) | Move(_) => {}
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
        self.view.resize(Size {
            height: size.height.saturating_sub(2),
            width: size.width,
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
            self.view.render(0);
        }

        let new_caret_pos = if self.in_prompt() {
            Position {
                row: bottom_bar_row,
                col: self.command_bar.caret_position_col(),
            }
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
