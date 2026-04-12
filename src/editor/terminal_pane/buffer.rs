#![allow(dead_code)]
use super::constants::{LINE_MAX_BYTES, SCROLLBACK_MAX_LINES};
use crossterm::style::Color;

#[derive(Clone, Default)]
pub struct Cell {
    pub ch: char,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
}

#[derive(Clone, Default)]
pub struct Row {
    pub cells: Vec<Cell>,
}

impl Row {
    pub fn push(&mut self, cell: Cell) {
        if self.cells.len() < LINE_MAX_BYTES {
            self.cells.push(cell);
        }
    }

    pub fn clear(&mut self) {
        self.cells.clear();
    }
}

pub struct ScrollbackBuffer {
    rows: Vec<Row>,
    cursor_row: usize,
}

impl ScrollbackBuffer {
    pub fn new() -> Self {
        Self {
            rows: vec![Row::default()],
            cursor_row: 0,
        }
    }

    pub fn current_row_mut(&mut self) -> &mut Row {
        &mut self.rows[self.cursor_row]
    }

    pub fn newline(&mut self) {
        self.cursor_row += 1;
        if self.cursor_row >= self.rows.len() {
            self.rows.push(Row::default());
        }
        if self.rows.len() > SCROLLBACK_MAX_LINES {
            self.rows.remove(0);
            self.cursor_row = self.cursor_row.saturating_sub(1);
        }
    }

    pub fn carriage_return(&mut self) {
        self.rows[self.cursor_row].clear();
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
}
