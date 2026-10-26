use super::{
    buffer::ScrollbackBuffer,
    constants::DEFAULT_PANEL_ROWS,
    pty::PtySession,
    reader::{PtyEvent, ReaderThread},
    vt::VtParser,
};
use crate::editor::{Position, Size, terminal::Terminal};
use crossterm::style::{
    Attribute, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::QueueableCommand;
use std::io::{self, stdout};
use std::sync::mpsc::Receiver;

#[allow(dead_code)]
pub struct TerminalPane {
    pub size: Size,
    pub rows: usize,
    needs_redraw: bool,
    view_offset: usize,
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
            view_offset: 0,
            buffer: ScrollbackBuffer::new(),
            vt: VtParser::default(),
            session: None,
            reader_thread: None,
            rx: None,
            closed: false,
        }
    }

    pub fn start(&mut self, cwd: &std::path::Path, cols: u16) -> io::Result<()> {
        let content_rows = self.rows.saturating_sub(1);
        #[allow(clippy::cast_possible_truncation)]
        let pty_rows = content_rows as u16;
        self.buffer = ScrollbackBuffer::new();
        self.vt = VtParser::default();
        self.view_offset = 0;
        self.buffer.set_screen_size(cols as usize, content_rows);
        let (session, reader) = PtySession::spawn(cwd, cols, pty_rows)?;
        let (thread, rx) = ReaderThread::spawn(reader);
        self.session = Some(session);
        self.reader_thread = Some(thread);
        self.rx = Some(rx);
        self.closed = false;
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(ref mut s) = self.session {
            s.kill();
        }
        self.session = None;
        self.rx = None;
        if let Some(thread) = self.reader_thread.take()
            && thread.join().is_err()
        {
            self.vt
                .feed(b"\r\n[Reader thread join failed]\r\n", &mut self.buffer);
            self.needs_redraw = true;
        }
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
                    self.vt
                        .feed(b"\r\n[Process exited]\r\n", &mut self.buffer);
                    updated = true;
                }
            }
        }
        if updated {
            self.view_offset = 0;
            self.needs_redraw = true;
        }
        updated
    }

    pub fn scroll_page_up(&mut self) {
        let page = self.rows.saturating_sub(1).max(1);
        let origin = self.buffer.screen_origin();
        self.view_offset = self.view_offset.saturating_add(page).min(origin);
        self.needs_redraw = true;
    }

    pub fn scroll_page_down(&mut self) {
        let page = self.rows.saturating_sub(1).max(1);
        self.view_offset = self.view_offset.saturating_sub(page);
        self.needs_redraw = true;
    }

    pub fn viewing_history(&self) -> bool {
        self.view_offset > 0
    }

    fn view_start(&self) -> usize {
        self.buffer.screen_origin().saturating_sub(self.view_offset)
    }

    pub fn write(&mut self, data: &[u8]) -> io::Result<()> {
        if let Some(s) = &mut self.session {
            s.write_all(data)
        } else {
            Ok(())
        }
    }

    pub fn resize_pty(&mut self, cols: u16) -> io::Result<()> {
        let content_rows = self.rows.saturating_sub(1);
        #[allow(clippy::cast_possible_truncation)]
        let pty_rows = content_rows as u16;
        self.buffer.set_screen_size(cols as usize, content_rows);
        if let Some(s) = &self.session {
            s.resize(cols, pty_rows)
        } else {
            Ok(())
        }
    }

    pub fn cursor_position(&self, origin_y: usize, origin_col: usize) -> Position {
        let screen_row = self.buffer.cursor_row().saturating_sub(self.buffer.screen_origin());
        Position {
            row: origin_y + 1 + screen_row,
            col: origin_col + self.buffer.cursor_col(),
        }
    }

    pub fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    pub fn mark_redraw(&mut self, v: bool) {
        self.needs_redraw = v;
    }

    pub fn draw(&mut self, origin_y: usize, origin_col: usize) -> io::Result<()> {
        let h = self.rows;
        let w = self.size.width;

        if origin_col > 0 {
            Terminal::move_caret_to(Position { row: origin_y, col: origin_col - 1 })?;
            Terminal::print("├")?;
        } else {
            Terminal::move_caret_to(Position { row: origin_y, col: 0 })?;
        }
        let separator = "─".repeat(w);
        Terminal::print(&separator)?;

        let content_rows = h.saturating_sub(1);
        let start = self.view_start();

        for row in 0..content_rows {
            let screen_row = origin_y + 1 + row;
            Terminal::move_caret_to(Position { row: screen_row, col: origin_col })?;
            let mut out = stdout();
            out.queue(crossterm::terminal::Clear(crossterm::terminal::ClearType::UntilNewLine))?;

            if let Some(buf_row) = self.buffer.row(start + row) {
                for cell in &buf_row.cells {
                    if cell.ch == '\0' {
                        continue;
                    }
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
                    out.queue(Print(cell.ch))?;
                }
                out.queue(ResetColor)?;
                out.queue(SetAttribute(Attribute::Reset))?;
            }
        }
        self.needs_redraw = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalPane;

    #[test]
    fn page_up_scrolls_within_scrollback() {
        let mut pane = TerminalPane::new();
        pane.rows = 5;
        pane.buffer.set_screen_size(20, 4);
        for _ in 0..10 {
            pane.vt.feed(b"\n", &mut pane.buffer);
        }
        let origin = pane.buffer.screen_origin();
        assert!(origin > 4);
        pane.scroll_page_up();
        assert_eq!(pane.view_offset, 4);
        assert_eq!(pane.view_start(), origin - 4);
        assert!(pane.viewing_history());
        pane.scroll_page_up();
        assert_eq!(pane.view_offset, origin);
        pane.scroll_page_down();
        assert_eq!(pane.view_offset, origin - 4);
        pane.scroll_page_down();
        assert_eq!(pane.view_offset, 0);
        assert!(!pane.viewing_history());
    }
}
