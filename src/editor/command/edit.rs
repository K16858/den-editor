use crossterm::event::{
    KeyCode::{Backspace, Char, Delete, Enter, Tab},
    KeyEvent, KeyModifiers,
};
use std::convert::TryFrom;

#[derive(Clone, Copy)]
pub enum Edit {
    Insert(char),
    InsertNewline,
    Backspace,
    Delete,
    Copy,
    Cut,
    Paste,
    SelectAll,
    Undo,
    Redo,
}

impl TryFrom<KeyEvent> for Edit {
    type Error = String;

    fn try_from(event: KeyEvent) -> Result<Self, Self::Error> {
        match (event.code, event.modifiers) {
            (Char('c'), m) if m == KeyModifiers::CONTROL => Ok(Self::Copy),
            (Char('x'), m) if m == KeyModifiers::CONTROL => Ok(Self::Cut),
            (Char('v'), m) if m == KeyModifiers::CONTROL => Ok(Self::Paste),
            (Char('a'), m) if m == KeyModifiers::CONTROL => Ok(Self::SelectAll),
            (Char('z' | 'Z'), m) if m == KeyModifiers::CONTROL | KeyModifiers::SHIFT => {
                Ok(Self::Redo)
            }
            (Char('z' | 'Z'), m) if m == KeyModifiers::CONTROL => Ok(Self::Undo),
            (Char(character), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                Ok(Self::Insert(character))
            }
            (Tab, KeyModifiers::NONE) => Ok(Self::Insert('\t')),
            (Enter, KeyModifiers::NONE) => Ok(Self::InsertNewline),
            (Backspace, KeyModifiers::NONE) => Ok(Self::Backspace),
            (Delete, KeyModifiers::NONE) => Ok(Self::Delete),
            _ => Err(format!(
                "Unsupported key code {:?} with modifiers {:?}",
                event.code, event.modifiers
            )),
        }
    }
}
