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

    pub fn print_annotated_row_with_prefix(
        row: usize,
        col_start: usize,
        prefix: &str,
        annotated_string: &AnnotatedString,
        highlight_prefix: bool,
    ) -> Result<(), Error> {
        Self::move_caret_to(Position { row, col: col_start })?;
        Self::queue_command(Clear(ClearType::UntilNewLine))?;
        if !highlight_prefix {
            Self::queue_command(SetForegroundColor(Color::DarkGrey))?;
        }
        Self::print(prefix)?;
        Self::reset_color()?;
        annotated_string
            .into_iter()
            .try_for_each(|part| -> Result<(), Error> {
                if let Some(annotation_type) = part.annotation_type {
                    let attribute: Attribute = annotation_type.into();
                    Self::set_attribute(&attribute)?;
                }

                Self::print(part.string)?;
                Self::reset_color()?;
                Ok(())
            })?;
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
        let width = Self::size()?.width;
        Self::print_row(
            row,
            0,
            &format!("{Reverse}{line_text:width$.width$}{Reset}"),
        )
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
