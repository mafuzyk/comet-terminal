use winit::keyboard::{KeyCode, PhysicalKey, ModifiersState};

#[derive(Debug, Clone, PartialEq)]
pub enum TabAction {
    NewTab,
    CloseTab,
    NextTab,
    PrevTab,
    None,
}

pub fn process_key(
    physical_key: PhysicalKey,
    mods: ModifiersState,
    text: Option<&str>,
) -> (TabAction, Option<Vec<u8>>) {
    let ctrl = mods.control_key();
    let alt = mods.alt_key();
    let shift = mods.shift_key();

    // Tab management shortcuts
    if ctrl && !alt {
        let PhysicalKey::Code(code) = physical_key else {
            return (TabAction::None, text.filter(|t| !t.is_empty()).map(|t| t.as_bytes().to_vec()));
        };
        match code {
            KeyCode::KeyT if !shift => return (TabAction::NewTab, None),
            KeyCode::KeyW if !shift => return (TabAction::CloseTab, None),
            KeyCode::Tab => {
                return if shift {
                    (TabAction::PrevTab, None)
                } else {
                    (TabAction::NextTab, None)
                };
            }
            _ => {}
        }
    }

    // Regular text input
    if let Some(text) = text {
        if !text.is_empty() {
            return (TabAction::None, Some(text.as_bytes().to_vec()));
        }
    }

    // Non-text key presses with modifiers
    let PhysicalKey::Code(code) = physical_key else {
        return (TabAction::None, None);
    };

    let bytes = key_to_ansi(code, ctrl, alt, shift);
    (TabAction::None, bytes)
}

fn key_to_ansi(code: KeyCode, ctrl: bool, alt: bool, shift: bool) -> Option<Vec<u8>> {
    // Function keys and special keys
    match code {
        KeyCode::Escape => return Some(vec![0x1b]),
        KeyCode::Enter | KeyCode::NumpadEnter => return Some(vec![b'\r']),
        KeyCode::Tab => return Some(vec![b'\t']),
        KeyCode::Backspace => return Some(vec![0x7f]),
        KeyCode::Delete => return Some(vec![0x1b, b'[', b'3', b'~']),
        KeyCode::Home => return Some(vec![0x1b, b'[', b'H']),
        KeyCode::End => return Some(vec![0x1b, b'[', b'F']),
        KeyCode::PageUp => return Some(vec![0x1b, b'[', b'5', b'~']),
        KeyCode::PageDown => return Some(vec![0x1b, b'[', b'6', b'~']),
        KeyCode::ArrowUp => return Some(vec![0x1b, b'[', b'A']),
        KeyCode::ArrowDown => return Some(vec![0x1b, b'[', b'B']),
        KeyCode::ArrowRight => return Some(vec![0x1b, b'[', b'C']),
        KeyCode::ArrowLeft => return Some(vec![0x1b, b'[', b'D']),
        KeyCode::F1 => return Some(vec![0x1b, b'O', b'P']),
        KeyCode::F2 => return Some(vec![0x1b, b'O', b'Q']),
        KeyCode::F3 => return Some(vec![0x1b, b'O', b'R']),
        KeyCode::F4 => return Some(vec![0x1b, b'O', b'S']),
        _ => {}
    }

    // Regular keys with Ctrl -> control codes
    let c = key_to_char(code, shift)?;
    if ctrl && c >= b'a' && c <= b'z' {
        return Some(vec![c - b'a' + 1]);
    }
    if alt {
        return Some(vec![0x1b, c]);
    }
    Some(vec![c])
}

fn key_to_char(code: KeyCode, shift: bool) -> Option<u8> {
    let c = match code {
        KeyCode::KeyA => if shift { b'A' } else { b'a' },
        KeyCode::KeyB => if shift { b'B' } else { b'b' },
        KeyCode::KeyC => if shift { b'C' } else { b'c' },
        KeyCode::KeyD => if shift { b'D' } else { b'd' },
        KeyCode::KeyE => if shift { b'E' } else { b'e' },
        KeyCode::KeyF => if shift { b'F' } else { b'f' },
        KeyCode::KeyG => if shift { b'G' } else { b'g' },
        KeyCode::KeyH => if shift { b'H' } else { b'h' },
        KeyCode::KeyI => if shift { b'I' } else { b'i' },
        KeyCode::KeyJ => if shift { b'J' } else { b'j' },
        KeyCode::KeyK => if shift { b'K' } else { b'k' },
        KeyCode::KeyL => if shift { b'L' } else { b'l' },
        KeyCode::KeyM => if shift { b'M' } else { b'm' },
        KeyCode::KeyN => if shift { b'N' } else { b'n' },
        KeyCode::KeyO => if shift { b'O' } else { b'o' },
        KeyCode::KeyP => if shift { b'P' } else { b'p' },
        KeyCode::KeyQ => if shift { b'Q' } else { b'q' },
        KeyCode::KeyR => if shift { b'R' } else { b'r' },
        KeyCode::KeyS => if shift { b'S' } else { b's' },
        KeyCode::KeyT => if shift { b'T' } else { b't' },
        KeyCode::KeyU => if shift { b'U' } else { b'u' },
        KeyCode::KeyV => if shift { b'V' } else { b'v' },
        KeyCode::KeyW => if shift { b'W' } else { b'w' },
        KeyCode::KeyX => if shift { b'X' } else { b'x' },
        KeyCode::KeyY => if shift { b'Y' } else { b'y' },
        KeyCode::KeyZ => if shift { b'Z' } else { b'z' },
        KeyCode::Digit0 => if shift { b')' } else { b'0' },
        KeyCode::Digit1 => if shift { b'!' } else { b'1' },
        KeyCode::Digit2 => if shift { b'@' } else { b'2' },
        KeyCode::Digit3 => if shift { b'#' } else { b'3' },
        KeyCode::Digit4 => if shift { b'$' } else { b'4' },
        KeyCode::Digit5 => if shift { b'%' } else { b'5' },
        KeyCode::Digit6 => if shift { b'^' } else { b'6' },
        KeyCode::Digit7 => if shift { b'&' } else { b'7' },
        KeyCode::Digit8 => if shift { b'*' } else { b'8' },
        KeyCode::Digit9 => if shift { b'(' } else { b'9' },
        KeyCode::Space => b' ',
        KeyCode::Minus => if shift { b'_' } else { b'-' },
        KeyCode::Equal => if shift { b'+' } else { b'=' },
        KeyCode::BracketLeft => if shift { b'{' } else { b'[' },
        KeyCode::BracketRight => if shift { b'}' } else { b']' },
        KeyCode::Semicolon => if shift { b':' } else { b';' },
        KeyCode::Quote => if shift { b'"' } else { b'\'' },
        KeyCode::Comma => if shift { b'<' } else { b',' },
        KeyCode::Period => if shift { b'>' } else { b'.' },
        KeyCode::Slash => if shift { b'?' } else { b'/' },
        KeyCode::Backslash => if shift { b'|' } else { b'\\' },
        KeyCode::Backquote => if shift { b'~' } else { b'`' },
        KeyCode::IntlBackslash => if shift { b'|' } else { b'\\' },
        _ => return None,
    };
    Some(c)
}
