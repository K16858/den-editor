use super::UIComponent;
use super::super::{Size, terminal::Terminal};
use crate::editor::debugger::DebugState;
use std::io::Error;

pub struct DebugPanel {
    pub rows: usize,
    size: Size,
    col_offset: usize,
    needs_redraw: bool,
    lines: Vec<String>,
}

impl DebugPanel {
    pub const DEFAULT_ROWS: usize = 6;

    pub fn new() -> Self {
        Self {
            rows: Self::DEFAULT_ROWS,
            size: Size::default(),
            col_offset: 0,
            needs_redraw: false,
            lines: Vec::new(),
        }
    }

    pub fn set_col_offset(&mut self, col_offset: usize) {
        self.col_offset = col_offset;
    }

    pub fn update(&mut self, state: &DebugState) {
        let mut lines = Vec::new();
        if state.active {
            lines.push("Debug Session".to_string());
            lines.push(format!(
                "Status: active  thread={}",
                state
                    .current_thread_id
                    .map_or_else(|| "-".to_string(), |id| id.to_string())
            ));
            lines.push("Threads".to_string());
            if state.threads.is_empty() {
                lines.push("  -".to_string());
            } else {
                for (idx, thread) in state.threads.iter().take(2).enumerate() {
                    let marker = if idx == state.selected_thread_idx { ">" } else { " " };
                    lines.push(format!(" {marker} {} ({})", thread.name, thread.id));
                }
            }
            lines.push("Frames".to_string());
            if state.stack_frames.is_empty() {
                lines.push("  -".to_string());
            } else {
                let frame_rows = self.rows.saturating_sub(9).max(1);
                for (idx, frame) in state.stack_frames.iter().take(frame_rows).enumerate() {
                    let marker = if idx == state.selected_frame_idx { ">" } else { " " };
                    lines.push(format!(
                        " {marker}#{idx} {} ({}:{}:{})",
                        frame.name, frame.source_path, frame.line, frame.column,
                    ));
                }
            }
            lines.push("Variables".to_string());
            if !state.variable_path.is_empty() {
                lines.push(format!("  path: {}", state.variable_path.join(".")));
            }
            if state.variables.is_empty() {
                lines.push("  -".to_string());
            } else {
                for (idx, var) in state
                    .variables
                    .iter()
                    .take(self.rows.saturating_sub(9))
                    .enumerate()
                {
                    let marker = if idx == state.selected_variable_idx {
                        ">"
                    } else {
                        " "
                    };
                    let expandable = if var.variables_reference > 0 { " +" } else { "" };
                    lines.push(format!(" {marker} {} = {}{expandable}", var.name, var.value));
                }
            }
        } else {
            lines.push("Debug: inactive".to_string());
        }
        if lines != self.lines {
            self.lines = lines;
            self.mark_redraw(true);
        }
    }
}

impl UIComponent for DebugPanel {
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
        for row in 0..self.size.height {
            let text = self.lines.get(row).map_or("", String::as_str);
            let line = if text.len() <= self.size.width {
                format!("{text:width$.width$}", width = self.size.width)
            } else {
                text.chars().take(self.size.width).collect()
            };
            Terminal::print_row(origin_row + row, self.col_offset, &line)?;
        }
        Ok(())
    }
}
