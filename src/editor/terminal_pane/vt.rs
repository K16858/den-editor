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
}

#[derive(Default, PartialEq)]
enum State {
    #[default]
    Ground,
    Escape,
    CsiEntry,
    CsiParam,
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
                0x1b => self.state = State::Escape,
                b'\n' => buf.newline(),
                b'\r' => buf.carriage_return(),
                0x07 | 0x08 | 0x0b | 0x0c => {}
                _ if b >= 0x20 => {
                    let ch = b as char;
                    buf.current_row_mut().push(Cell {
                        ch,
                        fg: self.fg,
                        bg: self.bg,
                        bold: self.bold,
                    });
                }
                _ => {}
            },
            State::Escape => match b {
                b'[' => {
                    self.params.clear();
                    self.state = State::CsiEntry;
                }
                b'(' | b')' | b'#' | b'%' => {
                    self.state = State::Ground;
                }
                b'c' => {
                    self.reset_attrs();
                    self.state = State::Ground;
                }
                _ => self.state = State::Ground,
            },
            State::CsiEntry | State::CsiParam => {
                if b.is_ascii_digit() || b == b';' {
                    self.params.push(b);
                    self.state = State::CsiParam;
                } else {
                    let raw = std::str::from_utf8(&self.params)
                        .unwrap_or("")
                        .to_string();
                    match b {
                        b'm' => self.apply_sgr(&raw),
                        b'K' => {
                            let n: u8 = raw.parse().unwrap_or(0);
                            if n == 0 || n == 2 {
                                buf.current_row_mut().clear();
                            }
                        }
                        b'B' => {
                            let n: usize = raw.parse().unwrap_or(1).max(1);
                            for _ in 0..n {
                                buf.newline();
                            }
                        }
                        _ => {}
                    }
                    self.params.clear();
                    self.state = State::Ground;
                }
            }
        }
    }

    fn apply_sgr(&mut self, raw: &str) {
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
                        if is_fg { self.fg = Some(color); } else { self.bg = Some(color); }
                        i += 2;
                    } else if codes.get(i + 1) == Some(&2) && i + 4 < codes.len() {
                        let color = Color::Rgb { r: codes[i + 2], g: codes[i + 3], b: codes[i + 4] };
                        if is_fg { self.fg = Some(color); } else { self.bg = Some(color); }
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
