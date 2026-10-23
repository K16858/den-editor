use std::{cmp::min, io::Error};

use super::super::terminal::clip_display;
use super::super::{Line, Size, Terminal, command::Edit};
use super::UIComponent;
use unicode_width::UnicodeWidthStr;

#[derive(Default)]
pub struct CommandBar {
    prompt: String,
    value: Line,
    cursor: usize,
    needs_redraw: bool,
    size: Size,
}

impl CommandBar {
    pub fn handle_edit_command(&mut self, command: Edit) {
        match command {
            Edit::Insert(character) => {
                self.value.insert_char(character, self.cursor);
                self.cursor += 1;
            }
            Edit::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.value.delete(self.cursor);
                }
            }
            Edit::Delete => {
                self.value.delete(self.cursor);
            }
            Edit::InsertNewline
            | Edit::Copy
            | Edit::Cut
            | Edit::Paste
            | Edit::SelectAll
            | Edit::Undo
            | Edit::Redo
            | Edit::IndentLine
            | Edit::DedentLine => {}
        }
        self.mark_redraw(true);
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
        self.mark_redraw(true);
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.value.grapheme_count() {
            self.cursor += 1;
            self.mark_redraw(true);
        }
    }

    pub fn caret_position_col(&self) -> usize {
        let (start, _) = self.value_window();
        let cursor_col =
            self.prompt.width() + self.value.width_until(self.cursor).saturating_sub(start);
        min(cursor_col, self.size.width)
    }

    fn value_window(&self) -> (usize, usize) {
        let area = self.size.width.saturating_sub(self.prompt.width().min(self.size.width));
        let value_width = self.value.width();
        let start = if value_width <= area {
            0
        } else {
            self.value
                .width_until(self.cursor)
                .saturating_add(1)
                .saturating_sub(area)
                .min(value_width.saturating_sub(area))
        };
        (start, start.saturating_add(area))
    }

    pub fn value(&self) -> String {
        self.value.to_string()
    }

    pub fn set_prompt(&mut self, prompt: &str) {
        self.prompt = prompt.to_string();
        self.mark_redraw(true);
    }

    pub fn clear_value(&mut self) {
        self.value = Line::default();
        self.cursor = 0;
        self.mark_redraw(true);
    }
}

impl UIComponent for CommandBar {
    fn mark_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }
    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }
    fn set_size(&mut self, size: Size) {
        self.size = size;
    }
    fn draw(&mut self, origin: usize) -> Result<(), Error> {
        let (start, end) = self.value_window();
        let message = format!(
            "{}{}",
            self.prompt,
            self.value.get_visible_graphemes(start..end)
        );
        let to_print = clip_display(&message, self.size.width);
        Terminal::print_row(origin, 0, &to_print)
    }
}

#[cfg(test)]
mod tests {
    use super::super::UIComponent;
    use super::super::super::Size;
    use super::super::super::command::Edit;
    use super::CommandBar;

    #[test]
    fn cursor_edits_the_middle_of_the_value() {
        let mut bar = CommandBar::default();
        bar.set_size(Size { width: 40, height: 1 });
        bar.set_prompt("P: ");
        bar.handle_edit_command(Edit::Insert('a'));
        bar.handle_edit_command(Edit::Insert('c'));
        bar.move_left();
        bar.handle_edit_command(Edit::Insert('b'));
        assert_eq!(bar.value(), "abc");
        assert_eq!(bar.caret_position_col(), 5);
        bar.move_left();
        bar.handle_edit_command(Edit::Backspace);
        assert_eq!(bar.value(), "bc");
        bar.handle_edit_command(Edit::Delete);
        assert_eq!(bar.value(), "c");
    }
}
