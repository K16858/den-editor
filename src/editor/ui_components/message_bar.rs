use std::{
    io::Error,
    time::{Duration, Instant},
};

use super::super::{Size, Terminal};
use super::UIComponent;

const DEFAULT_DURATION: Duration = Duration::new(5, 0);

struct Message {
    text: String,
    time: Instant,
}
impl Default for Message {
    fn default() -> Self {
        Self {
            text: String::new(),
            time: Instant::now(),
        }
    }
}

impl Message {
    fn is_expired(&self) -> bool {
        Instant::now().duration_since(self.time) > DEFAULT_DURATION
    }
}

#[derive(Default)]
pub struct MessageBar {
    current_message: Message,
    needs_redraw: bool,
    cleared_after_expiry: bool,
    size: Size,
}

impl MessageBar {
    pub fn update_message(&mut self, new_message: &str) {
        self.current_message = Message {
            text: new_message.to_string(),
            time: Instant::now(),
        };
        self.cleared_after_expiry = false;
        self.mark_redraw(true);
    }
}

impl UIComponent for MessageBar {
    fn mark_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        (!self.cleared_after_expiry && self.current_message.is_expired()) || self.needs_redraw
    }

    fn set_size(&mut self, size: Size) {
        self.size = size;
    }

    fn draw(&mut self, origin: usize) -> Result<(), Error> {
        if self.current_message.is_expired() {
            self.cleared_after_expiry = true;
        }
        let message = if self.current_message.is_expired() {
            ""
        } else {
            &self.current_message.text
        };

        let to_print = if message.len() <= self.size.width {
            format!("{message:width$.width$}", width = self.size.width)
        } else {
            String::new()
        };
        Terminal::print_row(origin, 0, &to_print)
    }
}
