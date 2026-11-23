use super::super::Size;
use crossterm::event::{
    KeyCode::{Char, Esc, F, Null},
    KeyEvent, KeyModifiers,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    RestartDebug,
    DisconnectDebug,
    NextThread,
    PrevThread,
    NextFrame,
    PrevFrame,
    NextVariable,
    PrevVariable,
    ExpandVariable,
    CollapseVariable,
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
                F(6) => Ok(Self::RestartDebug),
                _ => Err(format!("Unsupported SHIFT+{code:?} combination")),
            }
        } else if modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT {
            match code {
                Char('e' | 'E') => Ok(Self::FocusSidebar),
                Char('d' | 'D') => Ok(Self::FocusDebuggerSidebar),
                Char('n' | 'N') => Ok(Self::CreateFolder),
                Char('x' | 'X') => Ok(Self::DisconnectDebug),
                Null | Char('2' | '@') => Ok(Self::ToggleTerminal),
                _ => Err(format!("Unsupported CONTROL+SHIFT+{code:?} combination")),
            }
        } else if modifiers == KeyModifiers::CONTROL | KeyModifiers::ALT {
            match code {
                Char('e' | 'E') => Ok(Self::FocusSidebar),
                Char('d' | 'D') => Ok(Self::FocusDebuggerSidebar),
                Char('t' | 'T') => Ok(Self::NextThread),
                Char('g' | 'G') => Ok(Self::PrevThread),
                Char('j' | 'J') => Ok(Self::NextFrame),
                Char('k' | 'K') => Ok(Self::PrevFrame),
                Char('v' | 'V') => Ok(Self::NextVariable),
                Char('b' | 'B') => Ok(Self::PrevVariable),
                Char('l' | 'L') => Ok(Self::ExpandVariable),
                Char('h' | 'H') => Ok(Self::CollapseVariable),
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
                Null | Char('@' | '`') => Ok(Self::ToggleTerminal),
                _ => Err(format!("Unsupported CONTROL+{code:?} combination")),
            }
        } else {
            Err(format!(
                "Unsupported key code {code:?} or modifier {modifiers:?}"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::System;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn toggle_terminal_accepts_windows_and_unix_key_variants() {
        for (code, modifiers) in [
            (KeyCode::Char('@'), KeyModifiers::CONTROL),
            (KeyCode::Char('`'), KeyModifiers::CONTROL),
            (KeyCode::Null, KeyModifiers::CONTROL),
            (
                KeyCode::Char('2'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
            (
                KeyCode::Char('@'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
            (KeyCode::Null, KeyModifiers::CONTROL | KeyModifiers::SHIFT),
        ] {
            assert_eq!(
                System::try_from(KeyEvent::new(code, modifiers)),
                Ok(System::ToggleTerminal)
            );
        }

        assert_eq!(
            System::try_from(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::CONTROL)),
            Ok(System::FocusTerminal)
        );
    }
}
