use super::{Position, Size};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
mod attribute;
use super::AnnotatedString;
use attribute::Attribute;
use crossterm::style::{
    Attribute::{Reset, Reverse},
    Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{
    BeginSynchronizedUpdate, Clear, ClearType, DisableLineWrap, EnableLineWrap, EndSynchronizedUpdate,
    EnterAlternateScreen, LeaveAlternateScreen, SetTitle, disable_raw_mode, enable_raw_mode, size,
};
use crossterm::{Command, queue};
use std::io::{Error, Write, stdout};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub struct Terminal {}

impl Terminal {
    pub fn terminate() -> Result<(), Error> {
        Self::queue_command(DisableBracketedPaste)?;
        Self::execute()?;
        Self::leave_alternate_screen()?;
        Self::enable_line_wrap()?;
        Self::show_caret()?;
        Self::execute()?;
        disable_raw_mode()?;
        Ok(())
    }

    pub fn initialize() -> Result<(), Error> {
        enable_raw_mode()?;
        Self::disable_line_wrap()?;
        Self::enter_alternate_screen()?;
        Self::clear_screen()?;
        Self::queue_command(EnableBracketedPaste)?;
        // Steady cursor (disable terminal-native blink) on terminals that support DECSET 12.
        Self::queue_command(Print("\x1b[?12l"))?;
        Self::execute()?;
        Ok(())
    }

    pub fn clear_screen() -> Result<(), Error> {
        Self::queue_command(Clear(ClearType::All))?;
        Ok(())
    }

    pub fn move_caret_to(position: Position) -> Result<(), Error> {
        Self::queue_command(MoveTo(
            u16::try_from(position.col).unwrap_or(u16::MAX),
            u16::try_from(position.row).unwrap_or(u16::MAX),
        ))?;
        Ok(())
    }

    pub fn hide_caret() -> Result<(), Error> {
        Self::queue_command(Hide)?;
        Ok(())
    }

    pub fn show_caret() -> Result<(), Error> {
        Self::queue_command(Show)?;
        Ok(())
    }

    pub fn enter_alternate_screen() -> Result<(), Error> {
        Self::queue_command(EnterAlternateScreen)?;
        Ok(())
    }

    pub fn leave_alternate_screen() -> Result<(), Error> {
        Self::queue_command(LeaveAlternateScreen)?;
        Ok(())
    }

    pub fn disable_line_wrap() -> Result<(), Error> {
        Self::queue_command(DisableLineWrap)?;
        Ok(())
    }

    pub fn enable_line_wrap() -> Result<(), Error> {
        Self::queue_command(EnableLineWrap)?;
        Ok(())
    }

    pub fn set_title(title: &str) -> Result<(), Error> {
        Self::queue_command(SetTitle(title))?;
        Ok(())
    }

    pub fn print_annotated_row_with_gutter(
        row: usize,
        col_start: usize,
        row_display_width: usize,
        marker: &str,
        line_number_prefix: &str,
        marker_is_breakpoint: bool,
        marker_is_stopped: bool,
        annotated_string: &AnnotatedString,
        highlight_prefix: bool,
    ) -> Result<(), Error> {
        Self::move_caret_to(Position { row, col: col_start })?;

        let row_bg = marker_is_stopped.then_some(Color::DarkBlue);
        // Apply background before clear so the cleared tail keeps the row highlight where supported;
        // re-apply after clear because some terminals reset attributes on clear.
        if let Some(bg) = row_bg {
            Self::queue_command(SetBackgroundColor(bg))?;
        }
        Self::queue_command(Clear(ClearType::UntilNewLine))?;
        if let Some(bg) = row_bg {
            Self::queue_command(SetBackgroundColor(bg))?;
        }

        if marker_is_stopped {
            Self::queue_command(SetForegroundColor(Color::Yellow))?;
            Self::print(marker)?;
            if let Some(bg) = row_bg {
                Self::queue_command(SetBackgroundColor(bg))?;
            }
        } else if marker_is_breakpoint {
            Self::queue_command(SetForegroundColor(Color::Red))?;
            Self::print(marker)?;
            if let Some(bg) = row_bg {
                Self::queue_command(SetBackgroundColor(bg))?;
            } else {
                Self::reset_color()?;
            }
        } else if !highlight_prefix {
            Self::queue_command(SetForegroundColor(Color::DarkGrey))?;
            Self::print(marker)?;
            if let Some(bg) = row_bg {
                Self::queue_command(SetBackgroundColor(bg))?;
            } else {
                Self::reset_color()?;
            }
        } else {
            Self::print(marker)?;
        }

        if !highlight_prefix {
            Self::queue_command(SetForegroundColor(Color::DarkGrey))?;
        }
        Self::print(line_number_prefix)?;
        Self::reset_color()?;

        annotated_string
            .into_iter()
            .try_for_each(|part| -> Result<(), Error> {
                if let Some(annotation_type) = part.annotation_type {
                    let attribute: Attribute = annotation_type.into();
                    Self::set_attribute(&attribute)?;
                    if let Some(bg) = row_bg {
                        Self::queue_command(SetBackgroundColor(bg))?;
                    }
                }

                Self::print(part.string)?;
                if let Some(bg) = row_bg {
                    Self::queue_command(ResetColor)?;
                    Self::queue_command(SetBackgroundColor(bg))?;
                } else {
                    Self::reset_color()?;
                }
                Ok(())
            })?;

        if marker_is_stopped {
            if let Some(bg) = row_bg {
                let gutter_w = marker.width() + line_number_prefix.width();
                let content_w: usize = annotated_string
                    .into_iter()
                    .map(|p| p.string.width())
                    .sum();
                let used = gutter_w.saturating_add(content_w);
                let pad = row_display_width.saturating_sub(used);
                if pad > 0 {
                    Self::queue_command(SetBackgroundColor(bg))?;
                    Self::print(&" ".repeat(pad))?;
                }
            }
        }

        Self::reset_color()?;
        Ok(())
    }

    fn set_attribute(attribute: &Attribute) -> Result<(), Error> {
        if let Some(foreground_color) = attribute.foreground {
            Self::queue_command(SetForegroundColor(foreground_color))?;
        }
        if let Some(background_color) = attribute.background {
            Self::queue_command(SetBackgroundColor(background_color))?;
        }
        Ok(())
    }

    fn reset_color() -> Result<(), Error> {
        Self::queue_command(ResetColor)?;
        Ok(())
    }

    pub fn print_inverted_row(row: usize, line_text: &str) -> Result<(), Error> {
        Self::print_row(row, 0, &format!("{Reverse}{line_text}{Reset}"))
    }

    pub fn print(string: &str) -> Result<(), Error> {
        Self::queue_command(Print(string))?;
        Ok(())
    }

    pub fn print_row(row: usize, col_start: usize, line_text: &str) -> Result<(), Error> {
        Self::move_caret_to(Position { row, col: col_start })?;
        Self::queue_command(Clear(ClearType::UntilNewLine))?;
        Self::print(line_text)?;
        Ok(())
    }

    pub fn size() -> Result<Size, Error> {
        match size() {
            Ok((w, h)) => Ok(Size {
                width: w as usize,
                height: h as usize,
            }),
            Err(err) => Err(err),
        }
    }

    pub fn execute() -> Result<(), Error> {
        stdout().flush()?;
        Ok(())
    }

    pub fn begin_synchronized_update() -> Result<(), Error> {
        Self::queue_command(BeginSynchronizedUpdate)?;
        Ok(())
    }

    pub fn end_synchronized_update() -> Result<(), Error> {
        Self::queue_command(EndSynchronizedUpdate)?;
        Ok(())
    }

    fn queue_command<T: Command>(command: T) -> Result<(), Error> {
        queue!(stdout(), command)?;
        Ok(())
    }
}

/// Cuts `text` to `cols` display columns, keeping the start. Adds `…` when cut.
pub fn clip_display(text: &str, cols: usize) -> String {
    clip_display_side(text, cols, false)
}

/// Cuts `text` to `cols` display columns, keeping the end. Adds `…` when cut.
pub fn clip_display_end(text: &str, cols: usize) -> String {
    clip_display_side(text, cols, true)
}

fn clip_display_side(text: &str, cols: usize, keep_end: bool) -> String {
    if cols == 0 {
        return String::new();
    }
    if text.width() <= cols {
        return text.to_string();
    }
    let budget = cols.saturating_sub(1);
    let mut chars = Vec::new();
    let mut used = 0usize;
    if keep_end {
        for ch in text.chars().rev() {
            if !push_col(&mut chars, &mut used, budget, ch) {
                break;
            }
        }
        chars.reverse();
        let mut out = String::from("…");
        out.extend(chars);
        out
    } else {
        for ch in text.chars() {
            if !push_col(&mut chars, &mut used, budget, ch) {
                break;
            }
        }
        let mut out: String = chars.into_iter().collect();
        out.push('…');
        out
    }
}

fn push_col(chars: &mut Vec<char>, used: &mut usize, budget: usize, ch: char) -> bool {
    let width = ch.width().unwrap_or(0);
    if width > 0 && *used + width > budget {
        return false;
    }
    chars.push(ch);
    *used += width;
    true
}

#[cfg(test)]
mod tests {
    use super::{clip_display, clip_display_end};
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn clip_keeps_wide_characters_inside_the_column_budget() {
        let clipped = clip_display("あいう", 3);
        assert!(clipped.width() <= 3);
        assert_eq!(clipped, "あ…");
    }

    #[test]
    fn clip_end_keeps_the_tail() {
        assert_eq!(clip_display_end("abcdef", 4), "…def");
    }

    #[test]
    fn short_text_is_unchanged() {
        assert_eq!(clip_display("ab", 4), "ab");
        assert_eq!(clip_display_end("ab", 4), "ab");
    }
}
