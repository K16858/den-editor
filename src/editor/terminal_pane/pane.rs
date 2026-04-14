use super::{
    buffer::ScrollbackBuffer,
    constants::DEFAULT_PANEL_ROWS,
    pty::PtySession,
    reader::{PtyEvent, ReaderThread},
    vt::VtParser,
};
use crate::editor::{Position, Size, terminal::Terminal};
use crossterm::style::{Attribute, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor};
use crossterm::QueueableCommand;
use std::io::{self, stdout};
use std::sync::mpsc::Receiver;

#[allow(dead_code)]
pub struct TerminalPane {
    pub size: Size,
    pub rows: usize,
    needs_redraw: bool,
    buffer: ScrollbackBuffer,
    vt: VtParser,
    session: Option<PtySession>,
    reader_thread: Option<ReaderThread>,
    rx: Option<Receiver<PtyEvent>>,
    closed: bool,
}

#[allow(dead_code)]
impl TerminalPane {
    pub fn new() -> Self {
        Self {
            size: Size::default(),
            rows: DEFAULT_PANEL_ROWS,
            needs_redraw: false,
            buffer: ScrollbackBuffer::new(),
            vt: VtParser::default(),
            session: None,
            reader_thread: None,
            rx: None,
            closed: false,
        }
    }

    pub fn start(&mut self, cwd: &std::path::Path, cols: u16, rows: u16) -> io::Result<()> {
        let (session, reader) = PtySession::spawn(cwd, cols, rows)?;
        let (thread, rx) = ReaderThread::spawn(reader);
        self.session = Some(session);
        self.reader_thread = Some(thread);
        self.rx = Some(rx);
        self.closed = false;
        let content_rows = self.rows.saturating_sub(1);
        self.buffer.set_screen_size(cols as usize, content_rows);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(ref mut s) = self.session {
            s.kill();
        }
        self.session = None;
        self.reader_thread = None;
        self.rx = None;
        self.closed = false;
    }

    pub fn is_running(&self) -> bool {
        self.session.is_some()
    }

    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.rx else { return false };
        let mut updated = false;
        while let Ok(event) = rx.try_recv() {
            match event {
                PtyEvent::Data(bytes) => {
                    self.vt.feed(&bytes, &mut self.buffer);
                    updated = true;
                }
                PtyEvent::Closed => {
                    self.closed = true;
                    updated = true;
                }
            }
        }
        if updated {
            self.needs_redraw = true;
        }
        updated
    }

    pub fn write(&mut self, data: &[u8]) -> io::Result<()> {
        if let Some(s) = &mut self.session {
            s.write_all(data)
        } else {
            Ok(())
        }
    }

    pub fn resize_pty(&mut self, cols: u16, rows: u16) -> io::Result<()> {
        let content_rows = self.rows.saturating_sub(1);
        self.buffer.set_screen_size(cols as usize, content_rows);
        if let Some(s) = &self.session {
            s.resize(cols, rows)
        } else {
            Ok(())
        }
    }

    pub fn cursor_position(&self, origin_y: usize) -> Position {
        let screen_row = self.buffer.cursor_row().saturating_sub(self.buffer.screen_origin());
        Position {
            row: origin_y + 1 + screen_row,
            col: self.buffer.cursor_col(),
        }
    }

    pub fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    pub fn mark_redraw(&mut self, v: bool) {
        self.needs_redraw = v;
    }

    pub fn draw(&mut self, origin_y: usize) -> io::Result<()> {
        let h = self.rows;
        let w = self.size.width;

        Terminal::move_caret_to(Position { row: origin_y, col: 0 })?;
        let separator = "─".repeat(w);
        Terminal::print(&separator)?;

        let content_rows = h.saturating_sub(1);
        let start = self.buffer.screen_origin();

        for row in 0..content_rows {
            let screen_row = origin_y + 1 + row;
            Terminal::move_caret_to(Position { row: screen_row, col: 0 })?;
            let mut out = stdout();
            out.queue(crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine))?;

            if let Some(buf_row) = self.buffer.row(start + row) {
                for cell in &buf_row.cells {
                    if let Some(fg) = cell.fg {
                        out.queue(SetForegroundColor(fg))?;
                    } else {
                        out.queue(ResetColor)?;
                    }
                    if let Some(bg) = cell.bg {
                        out.queue(SetBackgroundColor(bg))?;
                    }
                    if cell.bold {
                        out.queue(SetAttribute(Attribute::Bold))?;
                    } else {
                        out.queue(SetAttribute(Attribute::NormalIntensity))?;
                    }
                    Terminal::print(&cell.ch.to_string())?;
                }
                out.queue(ResetColor)?;
                out.queue(SetAttribute(Attribute::Reset))?;
            }

            let used = self
                .buffer
                .row(start + row)
                .map_or(0, |r| r.cells.len());
            if used < w {
                Terminal::print(&" ".repeat(w - used))?;
            }
        }
        self.needs_redraw = false;
        Ok(())
    }
}
