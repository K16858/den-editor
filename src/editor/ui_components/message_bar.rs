use std::io::Error;

use super::super::terminal::clip_display;
use super::super::{Size, Terminal};
use super::UIComponent;

#[derive(Default)]
pub struct MessageBar {
    current_message: String,
    needs_redraw: bool,
    size: Size,
}

impl MessageBar {
    pub fn update_message(&mut self, new_message: &str) {
        self.current_message = new_message.to_string();
        self.mark_redraw(true);
    }
}

impl UIComponent for MessageBar {
    fn mark_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    fn set_size(&mut self, size: Size) {
        self.size = size;
    }

    fn draw(&mut self, origin: usize) -> Result<(), Error> {
        let to_print = clip_display(&self.current_message, self.size.width);
        Terminal::print_row(origin, 0, &to_print)
    }
}
