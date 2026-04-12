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
        Ok(())
    }

    pub fn stop(&mut self) {
        self.session = None;
        if let Some(t) = self.reader_thread.take() {
            t.join();
        }
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

    pub fn resize_pty(&self, cols: u16, rows: u16) -> io::Result<()> {
        if let Some(s) = &self.session {
            s.resize(cols, rows)
        } else {
            Ok(())
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
        let total = self.buffer.len();
        let start = total.saturating_sub(h);

        for row in 0..h {
            let screen_row = origin_y + row;
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
