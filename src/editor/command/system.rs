use super::super::Size;
use crossterm::event::{
    KeyCode::{Char, Esc},
    KeyEvent, KeyModifiers,
};

#[derive(Clone, Copy)]
pub enum System {
    Save,
    Resize(Size),
    Quit,
    Dismiss,
    Search,
    Replace,
    ToggleSidebar,
    FocusSidebar,
    FocusView,
}

impl TryFrom<KeyEvent> for System {
    type Error = String;
    fn try_from(event: KeyEvent) -> Result<Self, Self::Error> {
        let KeyEvent {
            code, modifiers, ..
        } = event;

        if modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT {
            match code {
                Char('e' | 'E') => Ok(Self::FocusSidebar),
                _ => Err(format!("Unsupported CONTROL+SHIFT+{code:?} combination")),
            }
        } else if modifiers == KeyModifiers::CONTROL {
            match code {
                Char('q') => Ok(Self::Quit),
                Char('s') => Ok(Self::Save),
                Char('f') => Ok(Self::Search),
                Char('h') => Ok(Self::Replace),
                Char('b') => Ok(Self::ToggleSidebar),
                Char('1') => Ok(Self::FocusView),
                _ => Err(format!("Unsupported CONTROL+{code:?} combination")),
            }
        } else if modifiers == KeyModifiers::NONE && matches!(code, Esc) {
            Ok(Self::Dismiss)
        } else {
            Err(format!(
                "Unsupported key code {code:?} or modifier {modifiers:?}"
            ))
        }
    }
}
