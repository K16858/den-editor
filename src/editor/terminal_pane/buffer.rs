#![allow(dead_code)]
use super::constants::{LINE_MAX_BYTES, SCROLLBACK_MAX_LINES};
use crossterm::style::Color;

#[derive(Clone)]
pub struct Cell {
    pub ch: char,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: None,
            bg: None,
            bold: false,
        }
    }
}

#[derive(Clone, Default)]
pub struct Row {
    pub cells: Vec<Cell>,
}

impl Row {
    pub fn write_at(&mut self, col: usize, cell: Cell) {
        if col >= LINE_MAX_BYTES {
            return;
        }
        while self.cells.len() <= col {
            self.cells.push(Cell::default());
        }
        self.cells[col] = cell;
    }

    pub fn clear_from(&mut self, col: usize) {
        self.cells.truncate(col);
    }

    pub fn clear_to(&mut self, col: usize) {
        let end = col.min(self.cells.len());
        for cell in &mut self.cells[..end] {
            *cell = Cell::default();
        }
    }

    pub fn clear(&mut self) {
        self.cells.clear();
    }
}

pub struct ScrollbackBuffer {
    rows: Vec<Row>,
    cursor_row: usize,
    cursor_col: usize,
    screen_rows: usize,
    screen_cols: usize,
    screen_origin: usize,
}

impl ScrollbackBuffer {
    pub fn new() -> Self {
        Self {
            rows: vec![Row::default()],
            cursor_row: 0,
            cursor_col: 0,
            screen_rows: 0,
            screen_cols: 0,
            screen_origin: 0,
        }
    }

    pub fn set_screen_size(&mut self, cols: usize, rows: usize) {
        self.screen_cols = cols;
        self.screen_rows = rows;
    }

    pub fn write_cell(&mut self, cell: Cell) {
        if self.screen_cols > 0 && self.cursor_col >= self.screen_cols {
            self.cursor_col = 0;
            self.newline();
        }
        self.ensure_cursor_row();
        self.rows[self.cursor_row].write_at(self.cursor_col, cell);
        self.cursor_col += 1;
    }

    pub fn newline(&mut self) {
        self.cursor_row += 1;
        self.ensure_cursor_row();
        if self.screen_rows > 0 && self.cursor_row >= self.screen_origin + self.screen_rows {
            self.screen_origin = self.cursor_row + 1 - self.screen_rows;
        }
        self.trim_scrollback();
    }

    pub fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    pub fn backspace(&mut self) {
        self.cursor_col = self.cursor_col.saturating_sub(1);
    }

    /// VT の CSI H / CSI f 用。row, col は 1-based。
    pub fn set_cursor_position(&mut self, vt_row: usize, vt_col: usize) {
        let row = vt_row.saturating_sub(1);
        let col = vt_col.saturating_sub(1);
        self.cursor_row = self.screen_origin + row;
        self.cursor_col = col;
        self.ensure_cursor_row();
    }

    pub fn set_cursor_col(&mut self, col: usize) {
        self.cursor_col = col;
    }

    pub fn move_cursor_up(&mut self, n: usize) {
        self.cursor_row = self.cursor_row.saturating_sub(n);
        if self.cursor_row < self.screen_origin {
            self.cursor_row = self.screen_origin;
        }
    }

    pub fn move_cursor_down(&mut self, n: usize) {
        self.cursor_row += n;
        if self.screen_rows > 0 {
            let bottom = self.screen_origin + self.screen_rows - 1;
            if self.cursor_row > bottom {
                self.cursor_row = bottom;
            }
        }
        self.ensure_cursor_row();
    }

    pub fn move_cursor_forward(&mut self, n: usize) {
        self.cursor_col += n;
        if self.screen_cols > 0 && self.cursor_col >= self.screen_cols {
            self.cursor_col = self.screen_cols - 1;
        }
    }

    pub fn move_cursor_back(&mut self, n: usize) {
        self.cursor_col = self.cursor_col.saturating_sub(n);
    }

    pub fn erase_in_line(&mut self, mode: u8) {
        self.ensure_cursor_row();
        match mode {
            0 => self.rows[self.cursor_row].clear_from(self.cursor_col),
            1 => self.rows[self.cursor_row].clear_to(self.cursor_col + 1),
            2 => self.rows[self.cursor_row].clear(),
            _ => {}
        }
    }

    pub fn erase_in_display(&mut self, mode: u8) {
        self.ensure_cursor_row();
        let screen_end = if self.screen_rows > 0 {
            (self.screen_origin + self.screen_rows).min(self.rows.len())
        } else {
            self.rows.len()
        };
        match mode {
            0 => {
                self.rows[self.cursor_row].clear_from(self.cursor_col);
                for r in (self.cursor_row + 1)..screen_end {
                    self.rows[r].clear();
                }
            }
            1 => {
                for r in self.screen_origin..self.cursor_row {
                    if r < self.rows.len() {
                        self.rows[r].clear();
                    }
                }
                self.rows[self.cursor_row].clear_to(self.cursor_col + 1);
            }
            2 => {
                for r in self.screen_origin..screen_end {
                    self.rows[r].clear();
                }
            }
            3 => {
                self.rows.clear();
                self.rows.push(Row::default());
                self.cursor_row = 0;
                self.cursor_col = 0;
                self.screen_origin = 0;
            }
            _ => {}
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn row(&self, idx: usize) -> Option<&Row> {
        self.rows.get(idx)
    }

    pub fn cursor_row(&self) -> usize {
        self.cursor_row
    }

    pub fn cursor_col(&self) -> usize {
        self.cursor_col
    }

    pub fn screen_origin(&self) -> usize {
        self.screen_origin
    }

    fn ensure_cursor_row(&mut self) {
        while self.rows.len() <= self.cursor_row {
            self.rows.push(Row::default());
        }
    }

    fn trim_scrollback(&mut self) {
        if self.rows.len() > SCROLLBACK_MAX_LINES {
            let excess = self.rows.len() - SCROLLBACK_MAX_LINES;
            self.rows.drain(..excess);
            self.cursor_row = self.cursor_row.saturating_sub(excess);
            self.screen_origin = self.screen_origin.saturating_sub(excess);
        }
    }
}
