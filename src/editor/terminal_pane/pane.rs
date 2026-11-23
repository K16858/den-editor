use super::{
    buffer::ScrollbackBuffer,
    constants::DEFAULT_PANEL_ROWS,
    pty::PtySession,
    reader::{PtyEvent, ReaderThread},
    vt::VtParser,
};
use crate::editor::{Position, Size, terminal::Terminal};
use crossterm::style::{Attribute, Color};
use std::io;
use std::path::Path;
use std::sync::mpsc::Receiver;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[allow(dead_code)]
pub struct TerminalPane {
    pub size: Size,
    pub rows: usize,
    title: String,
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
        Self::new_with_title("Terminal")
    }

    pub fn new_with_title(title: &str) -> Self {
        Self {
            size: Size::default(),
            rows: DEFAULT_PANEL_ROWS,
            title: title.to_string(),
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

    pub fn start(&mut self, cwd: &Path, cols: u16) -> io::Result<()> {
        let content_rows = self.rows.saturating_sub(1).max(1);
        #[allow(clippy::cast_possible_truncation)]
        let pty_rows = content_rows as u16;
        let pty_cols = cols.max(1);
        self.stop();
        self.buffer = ScrollbackBuffer::new();
        self.vt = VtParser::default();
        self.view_offset = 0;
        self.buffer.set_screen_size(pty_cols as usize, content_rows);
        let (session, reader) = PtySession::spawn(cwd, pty_cols, pty_rows)?;
        let (thread, rx) = ReaderThread::spawn(reader);
        self.session = Some(session);
        self.reader_thread = Some(thread);
        self.rx = Some(rx);
        self.closed = false;
        self.needs_redraw = true;
        Ok(())
    }

    pub fn start_command(
        &mut self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        env_overrides: &[(String, Option<String>)],
        cols: u16,
    ) -> io::Result<Option<u32>> {
        let content_rows = self.rows.saturating_sub(1).max(1);
        #[allow(clippy::cast_possible_truncation)]
        let pty_rows = content_rows as u16;
        let pty_cols = cols.max(1);
        self.stop();
        self.buffer = ScrollbackBuffer::new();
        self.vt = VtParser::default();
        self.view_offset = 0;
        self.buffer.set_screen_size(pty_cols as usize, content_rows);
        let (session, reader) =
            PtySession::spawn_command(program, args, cwd, env_overrides, pty_cols, pty_rows)?;
        let pid = session.process_id();
        let (thread, rx) = ReaderThread::spawn(reader);
        self.session = Some(session);
        self.reader_thread = Some(thread);
        self.rx = Some(rx);
        self.closed = false;
        self.needs_redraw = true;
        Ok(pid)
    }

    pub fn clear_for_output(&mut self, cols: usize) {
        let content_rows = self.rows.saturating_sub(1).max(1);
        self.stop();
        self.buffer = ScrollbackBuffer::new();
        self.vt = VtParser::default();
        self.view_offset = 0;
        self.buffer.set_screen_size(cols.max(1), content_rows);
        self.needs_redraw = true;
    }

    pub fn feed_output(&mut self, text: &str, is_stderr: bool) {
        let mut normalized = String::with_capacity(text.len());
        let mut prev_cr = false;
        for ch in text.chars() {
            if ch == '\n' && !prev_cr {
                normalized.push('\r');
            }
            normalized.push(ch);
            prev_cr = ch == '\r';
        }
        if is_stderr {
            self.vt.feed(b"\x1b[31m", &mut self.buffer);
            self.vt.feed(normalized.as_bytes(), &mut self.buffer);
            self.vt.feed(b"\x1b[0m", &mut self.buffer);
        } else {
            self.vt.feed(normalized.as_bytes(), &mut self.buffer);
        }
        self.view_offset = 0;
        self.needs_redraw = true;
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
        let mut events = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }
        if events.is_empty() {
            return false;
        }
        let mut updated = false;
        for event in events {
            match event {
                PtyEvent::Data(bytes) => {
                    self.vt.feed(&bytes, &mut self.buffer);
                    let replies = self.vt.take_responses();
                    if let Some(s) = &mut self.session {
                        for reply in replies {
                            let _ = s.write_all(&reply);
                        }
                    }
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
        let content_rows = self.rows.saturating_sub(1).max(1);
        #[allow(clippy::cast_possible_truncation)]
        let pty_rows = content_rows as u16;
        let pty_cols = cols.max(1);
        self.buffer.set_screen_size(pty_cols as usize, content_rows);
        if let Some(s) = &self.session {
            s.resize(pty_cols, pty_rows)
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
        let label = format!("── {} ", self.title);
        let label_w = label.width();
        let separator = if w > label_w {
            format!("{label}{}", "─".repeat(w - label_w))
        } else {
            "─".repeat(w)
        };
        Terminal::print(&separator)?;

        let content_rows = h.saturating_sub(1);
        let start = self.view_start();

        for row in 0..content_rows {
            let screen_row = origin_y + 1 + row;
            Terminal::move_caret_to(Position {
                row: screen_row,
                col: origin_col,
            })?;

            let mut current_fg: Option<Color> = None;
            let mut current_background: Option<Color> = None;
            let mut cur_bold = false;
            let mut printed_cols = 0usize;

            if let Some(buf_row) = self.buffer.row(start + row) {
                for cell in &buf_row.cells {
                    if cell.ch == '\0' {
                        continue;
                    }
                    let ch_w = UnicodeWidthChar::width_cjk(cell.ch).unwrap_or(1).max(1);
                    if printed_cols + ch_w > w {
                        break;
                    }
                    if (cell.fg, cell.bg, cell.bold)
                        != (current_fg, current_background, cur_bold)
                    {
                        let need_reset = (current_fg.is_some() && cell.fg.is_none())
                            || (current_background.is_some() && cell.bg.is_none());
                        if need_reset {
                            Terminal::reset_style()?;
                            current_fg = None;
                            current_background = None;
                            cur_bold = false;
                        }
                        if cell.fg != current_fg
                            && let Some(fg) = cell.fg
                        {
                            Terminal::set_foreground(fg)?;
                        }
                        if cell.bg != current_background
                            && let Some(bg) = cell.bg
                        {
                            Terminal::set_background(bg)?;
                        }
                        if cell.bold != cur_bold {
                            Terminal::set_attribute(if cell.bold {
                                Attribute::Bold
                            } else {
                                Attribute::NormalIntensity
                            })?;
                        }
                        current_fg = cell.fg;
                        current_background = cell.bg;
                        cur_bold = cell.bold;
                    }
                    Terminal::print_char(cell.ch)?;
                    printed_cols += ch_w;
                }
            }
            if (current_fg, current_background, cur_bold) != (None, None, false) {
                Terminal::reset_style()?;
            }
            if printed_cols < w {
                Terminal::clear_until_newline()?;
            }
        }
        self.needs_redraw = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalPane;

    impl TerminalPane {
        pub(crate) fn dump_text(&self) -> String {
            let mut out = String::new();
            for idx in 0..self.buffer.len() {
                if let Some(row) = self.buffer.row(idx) {
                    for cell in &row.cells {
                        if cell.ch != '\0' {
                            out.push(cell.ch);
                        }
                    }
                    out.push('\n');
                }
            }
            out
        }
    }

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

    #[test]
    fn feed_output_normalizes_newlines_and_marks_stderr_red() {
        use crossterm::style::Color;

        let mut pane = TerminalPane::new();
        pane.rows = 5;
        pane.buffer.set_screen_size(40, 4);
        pane.feed_output("line1\nline2", false);
        assert_eq!(pane.buffer.row(0).unwrap().cells[0].ch, 'l');
        assert_eq!(pane.buffer.row(1).unwrap().cells[0].ch, 'l');
        assert_eq!(pane.buffer.row(1).unwrap().cells[4].ch, '2');

        pane.feed_output("\nerr", true);
        let row2 = pane.buffer.row(2).unwrap();
        assert_eq!(row2.cells[0].ch, 'e');
        assert_eq!(row2.cells[0].fg, Some(Color::DarkRed));
        assert_eq!(row2.cells[2].ch, 'r');
        assert_eq!(row2.cells[2].fg, Some(Color::DarkRed));
    }

    #[cfg(windows)]
    #[test]
    fn windows_conpty_starts_receives_output_and_stops_cleanly() {
        use std::time::{Duration, Instant};

        let mut pane = TerminalPane::new();
        pane.start(&std::env::temp_dir(), 80).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut got_output = false;
        while Instant::now() < deadline {
            pane.poll();
            let text = pane.dump_text();
            if text.contains('>') || text.contains("Microsoft Windows") {
                got_output = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let snapshot = pane.dump_text();
        pane.stop();
        assert!(got_output, "expected Windows shell output, got: {snapshot:?}");
    }
}
