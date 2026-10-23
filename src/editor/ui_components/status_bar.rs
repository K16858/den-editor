use super::super::terminal::{clip_display, clip_display_end};
use super::super::{DocumentStatus, Size, Terminal};
use super::UIComponent;
use std::io::Error;
use unicode_width::UnicodeWidthStr;

fn status_line(beginning: &str, position: &str, width: usize) -> String {
    let position_width = position.width();
    if position_width >= width {
        return clip_display_end(position, width);
    }
    let left = clip_display(beginning, width - position_width);
    let gap = width.saturating_sub(left.width() + position_width);
    format!("{left}{}{position}", " ".repeat(gap))
}

#[derive(Default)]
pub struct StatusBar {
    current_status: DocumentStatus,
    needs_redraw: bool,
    size: Size,
}

impl StatusBar {
    pub fn update_status(&mut self, new_status: DocumentStatus) {
        if new_status != self.current_status {
            self.current_status = new_status;
            self.mark_redraw(true);
        }
    }
}

impl UIComponent for StatusBar {
    fn mark_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    fn set_size(&mut self, size: Size) {
        self.size = size;
    }
    fn draw(&mut self, origin_row: usize) -> Result<(), Error> {
        let line_count = self.current_status.line_count_to_string();
        let modified_indicator = self.current_status.modified_indicator_to_string();
        let language_indicator = self
            .current_status
            .language_name
            .as_ref()
            .map(|lang| format!(" ({lang})"))
            .unwrap_or_default();
        let debug_indicator = self
            .current_status
            .debug_state_label
            .as_ref()
            .map(|s| format!(" [{s}]"))
            .unwrap_or_default();

        let beginning = format!(
            "{}{language_indicator}{debug_indicator} - {line_count} {modified_indicator}",
            self.current_status.file_name
        );
        let to_print = status_line(
            &beginning,
            &self.current_status.position_indicator_to_string(),
            self.size.width,
        );
        Terminal::print_inverted_row(origin_row, &to_print)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::status_line;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn long_name_keeps_the_position() {
        let line = status_line(&"あ".repeat(40), "12/99", 20);
        assert_eq!(line.width(), 20);
        assert!(line.ends_with("12/99"));
        assert!(!line.is_empty());
    }
}
