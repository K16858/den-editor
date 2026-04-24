#![allow(dead_code)]
use super::buffer::{Cell, ScrollbackBuffer};
use crossterm::style::Color;

#[derive(Default)]
pub struct VtParser {
    state: State,
    params: Vec<u8>,
    fg: Option<Color>,
    bg: Option<Color>,
    bold: bool,
    utf8: Utf8Accum,
}

#[derive(Default)]
struct Utf8Accum {
    buf: [u8; 4],
    len: usize,
    needed: usize,
}

impl Utf8Accum {
    fn reset(&mut self) {
        self.len = 0;
        self.needed = 0;
    }

    fn feed(&mut self, b: u8) -> Option<char> {
        if self.needed > 0 {
            if b & 0xC0 == 0x80 {
                self.buf[self.len] = b;
                self.len += 1;
                if self.len == self.needed {
                    let result = std::str::from_utf8(&self.buf[..self.len])
                        .ok()
                        .and_then(|s| s.chars().next());
                    self.reset();
                    return result;
                }
                return None;
            }
            self.reset();
        }

        if b < 0x80 {
            Some(b as char)
        } else if b & 0xE0 == 0xC0 {
            self.buf[0] = b;
            self.len = 1;
            self.needed = 2;
            None
        } else if b & 0xF0 == 0xE0 {
            self.buf[0] = b;
            self.len = 1;
            self.needed = 3;
            None
        } else if b & 0xF8 == 0xF0 {
            self.buf[0] = b;
            self.len = 1;
            self.needed = 4;
            None
        } else {
            None
        }
    }
}

#[derive(Default, PartialEq)]
enum State {
    #[default]
    Ground,
    Escape,
    EscapeIntermediate,
    CsiEntry,
    CsiParam,
    OscString,
}

impl VtParser {
    pub fn feed(&mut self, bytes: &[u8], buf: &mut ScrollbackBuffer) {
        for &b in bytes {
            self.advance(b, buf);
        }
    }

    fn advance(&mut self, b: u8, buf: &mut ScrollbackBuffer) {
        match self.state {
            State::Ground => match b {
                0x1b => {
                    self.utf8.reset();
                    self.state = State::Escape;
                }
                b'\n' => buf.newline(),
                b'\r' => buf.carriage_return(),
                0x08 => buf.backspace(),
                0x07 | 0x0b | 0x0c => {}
                _ if b >= 0x20 => {
                    if let Some(ch) = self.utf8.feed(b) {
                        buf.write_cell(Cell {
                            ch,
                            fg: self.fg,
                            bg: self.bg,
                            bold: self.bold,
                        });
                    }
                }
                _ => {}
            },
            State::Escape => match b {
                b'[' => {
                    self.params.clear();
                    self.state = State::CsiEntry;
                }
                b']' | b'P' | b'^' | b'_' => {
                    self.state = State::OscString;
                }
                b'(' | b')' | b'*' | b'+' | b'#' | b'%' => {
                    self.state = State::EscapeIntermediate;
                }
                b'c' => {
                    self.reset_attrs();
                    self.state = State::Ground;
                }
                _ => self.state = State::Ground,
            },
            State::EscapeIntermediate => {
                self.state = State::Ground;
            },
            State::CsiEntry | State::CsiParam => {
                if (0x20..=0x3F).contains(&b) {
                    self.params.push(b);
                    self.state = State::CsiParam;
                } else if (0x40..=0x7E).contains(&b) {
                    self.dispatch_csi(b, buf);
                    self.params.clear();
                    self.state = State::Ground;
                } else if b == 0x1b {
                    self.params.clear();
                    self.state = State::Escape;
                } else {
                    self.params.clear();
                    self.state = State::Ground;
                }
            }
            State::OscString => match b {
                0x07 => self.state = State::Ground,
                0x1b => self.state = State::Escape,
                _ => {}
            },
        }
    }

    fn dispatch_csi(&mut self, final_byte: u8, buf: &mut ScrollbackBuffer) {
        let raw = std::str::from_utf8(&self.params)
            .unwrap_or("")
            .to_string();

        match final_byte {
            b'm' => self.apply_sgr(&raw),
            b'A' => {
                let n: usize = raw.parse().unwrap_or(1).max(1);
                buf.move_cursor_up(n);
            }
            b'B' => {
                let n: usize = raw.parse().unwrap_or(1).max(1);
                buf.move_cursor_down(n);
            }
            b'C' => {
                let n: usize = raw.parse().unwrap_or(1).max(1);
                buf.move_cursor_forward(n);
            }
            b'D' => {
                let n: usize = raw.parse().unwrap_or(1).max(1);
                buf.move_cursor_back(n);
            }
            b'H' | b'f' => {
                let parts: Vec<usize> = raw
                    .split(';')
                    .map(|s| s.parse().unwrap_or(1).max(1))
                    .collect();
                let vt_row = parts.first().copied().unwrap_or(1);
                let vt_col = parts.get(1).copied().unwrap_or(1);
                buf.set_cursor_position(vt_row, vt_col);
            }
            b'G' => {
                let vt_col: usize = raw.parse().unwrap_or(1).max(1);
                buf.set_cursor_col(vt_col.saturating_sub(1));
            }
            b'd' => {
                let vt_row: usize = raw.parse().unwrap_or(1).max(1);
                buf.set_cursor_position(vt_row, buf.cursor_col() + 1);
            }
            b'J' => {
                let n: u8 = raw.parse().unwrap_or(0);
                buf.erase_in_display(n);
            }
            b'K' => {
                let n: u8 = raw.parse().unwrap_or(0);
                buf.erase_in_line(n);
            }
            _ => {}
        }
    }

    fn apply_sgr(&mut self, raw: &str) {
        if raw.is_empty() {
            self.reset_attrs();
            return;
        }

        let codes: Vec<u8> = raw
            .split(';')
            .filter_map(|s| s.parse().ok())
            .collect();

        let mut i = 0;
        while i < codes.len() {
            match codes[i] {
                0 => self.reset_attrs(),
                1 => self.bold = true,
                22 => self.bold = false,
                30..=37 => self.fg = Some(ansi_color(codes[i] - 30, false)),
                39 => self.fg = None,
                40..=47 => self.bg = Some(ansi_color(codes[i] - 40, false)),
                49 => self.bg = None,
                90..=97 => self.fg = Some(ansi_color(codes[i] - 90, true)),
                100..=107 => self.bg = Some(ansi_color(codes[i] - 100, true)),
                38 | 48 => {
                    let is_fg = codes[i] == 38;
                    if codes.get(i + 1) == Some(&5) && i + 2 < codes.len() {
                        let color = Color::AnsiValue(codes[i + 2]);
                        if is_fg {
                            self.fg = Some(color);
                        } else {
                            self.bg = Some(color);
                        }
                        i += 2;
                    } else if codes.get(i + 1) == Some(&2) && i + 4 < codes.len() {
                        let color = Color::Rgb {
                            r: codes[i + 2],
                            g: codes[i + 3],
                            b: codes[i + 4],
                        };
                        if is_fg {
                            self.fg = Some(color);
                        } else {
                            self.bg = Some(color);
                        }
                        i += 4;
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn reset_attrs(&mut self) {
        self.fg = None;
        self.bg = None;
        self.bold = false;
    }
}

fn ansi_color(idx: u8, bright: bool) -> Color {
    match (idx, bright) {
        (0, false) => Color::Black,
        (1, false) => Color::DarkRed,
        (2, false) => Color::DarkGreen,
        (3, false) => Color::DarkYellow,
        (4, false) => Color::DarkBlue,
        (5, false) => Color::DarkMagenta,
        (6, false) => Color::DarkCyan,
        (7, false) => Color::Grey,
        (0, true) => Color::DarkGrey,
        (1, true) => Color::Red,
        (2, true) => Color::Green,
        (3, true) => Color::Yellow,
        (4, true) => Color::Blue,
        (5, true) => Color::Magenta,
        (6, true) => Color::Cyan,
        (7, true) => Color::White,
        _ => Color::Reset,
    }
}
