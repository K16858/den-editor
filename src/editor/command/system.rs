use super::super::Size;
use crossterm::event::{
    KeyCode::{Char, Esc, F, Null},
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
    FocusDebuggerSidebar,
    FocusView,
    CreateFile,
    CreateFolder,
    ToggleTerminal,
    FocusTerminal,
    StartDebug,
    StopDebug,
    ToggleBreakpoint,
    StepOver,
    StepInto,
    StepOut,
    Continue,
    Pause,
}

impl TryFrom<KeyEvent> for System {
    type Error = String;
    fn try_from(event: KeyEvent) -> Result<Self, Self::Error> {
        let KeyEvent {
            code, modifiers, ..
        } = event;

        if modifiers == KeyModifiers::NONE {
            match code {
                F(5) => Ok(Self::StartDebug),
                F(9) => Ok(Self::ToggleBreakpoint),
                F(10) => Ok(Self::StepOver),
                F(11) => Ok(Self::StepInto),
                F(6) => Ok(Self::Pause),
                Esc => Ok(Self::Dismiss),
                _ => Err(format!("Unsupported key code {code:?} with no modifiers")),
            }
        } else if modifiers == KeyModifiers::SHIFT {
            match code {
                F(5) => Ok(Self::StopDebug),
                F(11) => Ok(Self::StepOut),
                _ => Err(format!("Unsupported SHIFT+{code:?} combination")),
            }
        } else if modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT {
            match code {
                Char('e' | 'E') => Ok(Self::FocusSidebar),
                Char('d' | 'D') => Ok(Self::FocusDebuggerSidebar),
                Char('n' | 'N') => Ok(Self::CreateFolder),
                _ => Err(format!("Unsupported CONTROL+SHIFT+{code:?} combination")),
            }
        } else if modifiers == KeyModifiers::CONTROL | KeyModifiers::ALT {
            match code {
                Char('e' | 'E') => Ok(Self::FocusSidebar),
                Char('d' | 'D') => Ok(Self::FocusDebuggerSidebar),
                _ => Err(format!("Unsupported CONTROL+ALT+{code:?} combination")),
            }
        } else if modifiers == KeyModifiers::CONTROL {
            match code {
                Char('q') => Ok(Self::Quit),
                Char('s') => Ok(Self::Save),
                Char('f') => Ok(Self::Search),
                Char('h') => Ok(Self::Replace),
                Char('b') => Ok(Self::ToggleSidebar),
                Char('1') => Ok(Self::FocusView),
                Char('2') => Ok(Self::FocusTerminal),
                Char('n') => Ok(Self::CreateFile),
                Char('r') => Ok(Self::Continue),
                Null | Char('@') => Ok(Self::ToggleTerminal),
                _ => Err(format!("Unsupported CONTROL+{code:?} combination")),
            }
        } else {
            Err(format!(
                "Unsupported key code {code:?} or modifier {modifiers:?}"
            ))
        }
    }
}
