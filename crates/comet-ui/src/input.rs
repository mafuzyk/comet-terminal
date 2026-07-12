use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// Converts a winit `KeyEvent` into an ANSI byte sequence to send to the PTY.
///
/// `ctrl` and `alt` should reflect the current state of the modifier keys
/// as tracked by the application (since winit 0.30.13 does not embed
/// modifier state in `KeyEvent`).
pub fn key_event_to_ansi(event: &KeyEvent, ctrl: bool, alt: bool) -> Option<Vec<u8>> {
    if !event.state.is_pressed() {
        return None;
    }

    // Ctrl+letter → control codes 0x01-0x1a
    if ctrl {
        if let PhysicalKey::Code(code) = event.physical_key {
            let c = match code {
                KeyCode::KeyA => Some(0x01),
                KeyCode::KeyB => Some(0x02),
                KeyCode::KeyC => Some(0x03),
                KeyCode::KeyD => Some(0x04),
                KeyCode::KeyE => Some(0x05),
                KeyCode::KeyF => Some(0x06),
                KeyCode::KeyG => Some(0x07),
                KeyCode::KeyH => Some(0x08),
                KeyCode::KeyI => Some(0x09),
                KeyCode::KeyJ => Some(0x0a),
                KeyCode::KeyK => Some(0x0b),
                KeyCode::KeyL => Some(0x0c),
                KeyCode::KeyM => Some(0x0d),
                KeyCode::KeyN => Some(0x0e),
                KeyCode::KeyO => Some(0x0f),
                KeyCode::KeyP => Some(0x10),
                KeyCode::KeyQ => Some(0x11),
                KeyCode::KeyR => Some(0x12),
                KeyCode::KeyS => Some(0x13),
                KeyCode::KeyT => Some(0x14),
                KeyCode::KeyU => Some(0x15),
                KeyCode::KeyV => Some(0x16),
                KeyCode::KeyW => Some(0x17),
                KeyCode::KeyX => Some(0x18),
                KeyCode::KeyY => Some(0x19),
                KeyCode::KeyZ => Some(0x1a),
                _ => None,
            };
            if let Some(byte) = c {
                return Some(vec![byte]);
            }
        }
    }

    // Use logical text when available (handles Shift, CapsLock, IME)
    if let Some(text) = &event.text
        && !text.is_empty()
    {
        let mut bytes = text.as_bytes().to_vec();
        if alt {
            bytes.insert(0, 0x1b); // Alt prefix
        }
        return Some(bytes);
    }

    // Special keys
    let mut result = match event.physical_key {
        PhysicalKey::Code(KeyCode::Enter) => Some(vec![b'\r']),
        PhysicalKey::Code(KeyCode::Backspace) => Some(vec![0x7f]),
        PhysicalKey::Code(KeyCode::Tab) => Some(vec![b'\t']),
        PhysicalKey::Code(KeyCode::Escape) => Some(vec![0x1b]),

        PhysicalKey::Code(KeyCode::ArrowUp) => Some(b"\x1b[A".to_vec()),
        PhysicalKey::Code(KeyCode::ArrowDown) => Some(b"\x1b[B".to_vec()),
        PhysicalKey::Code(KeyCode::ArrowRight) => Some(b"\x1b[C".to_vec()),
        PhysicalKey::Code(KeyCode::ArrowLeft) => Some(b"\x1b[D".to_vec()),

        PhysicalKey::Code(KeyCode::Home) => Some(b"\x1b[H".to_vec()),
        PhysicalKey::Code(KeyCode::End) => Some(b"\x1b[F".to_vec()),
        PhysicalKey::Code(KeyCode::PageUp) => Some(b"\x1b[5~".to_vec()),
        PhysicalKey::Code(KeyCode::PageDown) => Some(b"\x1b[6~".to_vec()),
        PhysicalKey::Code(KeyCode::Insert) => Some(b"\x1b[2~".to_vec()),
        PhysicalKey::Code(KeyCode::Delete) => Some(b"\x1b[3~".to_vec()),

        PhysicalKey::Code(KeyCode::F1) => Some(b"\x1bOP".to_vec()),
        PhysicalKey::Code(KeyCode::F2) => Some(b"\x1bOQ".to_vec()),
        PhysicalKey::Code(KeyCode::F3) => Some(b"\x1bOR".to_vec()),
        PhysicalKey::Code(KeyCode::F4) => Some(b"\x1bOS".to_vec()),
        PhysicalKey::Code(KeyCode::F5) => Some(b"\x1b[15~".to_vec()),
        PhysicalKey::Code(KeyCode::F6) => Some(b"\x1b[17~".to_vec()),
        PhysicalKey::Code(KeyCode::F7) => Some(b"\x1b[18~".to_vec()),
        PhysicalKey::Code(KeyCode::F8) => Some(b"\x1b[19~".to_vec()),
        PhysicalKey::Code(KeyCode::F9) => Some(b"\x1b[20~".to_vec()),
        PhysicalKey::Code(KeyCode::F10) => Some(b"\x1b[21~".to_vec()),
        PhysicalKey::Code(KeyCode::F11) => Some(b"\x1b[23~".to_vec()),
        PhysicalKey::Code(KeyCode::F12) => Some(b"\x1b[24~".to_vec()),

        _ => None,
    };
    // When Alt is held, prefix the sequence with ESC (0x1b).
    // This applies to all special keys: arrows, function keys, etc.
    if alt {
        if let Some(ref mut seq) = result {
            seq.insert(0, 0x1b);
        }
    }
    result
}

/// Updates the tracked modifier state from a key event.
pub fn update_modifiers(event: &KeyEvent, ctrl: &mut bool, alt: &mut bool) {
    if let PhysicalKey::Code(code) = event.physical_key {
        match code {
            KeyCode::ControlLeft | KeyCode::ControlRight => {
                *ctrl = event.state == ElementState::Pressed;
            }
            KeyCode::AltLeft | KeyCode::AltRight => {
                *alt = event.state == ElementState::Pressed;
            }
            _ => {}
        }
    }
}
