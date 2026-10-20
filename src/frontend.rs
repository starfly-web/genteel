//! Frontend Module - winit + pixels
//!
//! Provides cross-platform windowing, input handling, and rendering
//! for the Genesis emulator using pure Rust libraries.

use std::sync::LazyLock;
#[cfg(any(feature = "gui", feature = "test_headless"))]
use winit::keyboard::{Key, KeyCode};

/// Genesis display dimensions
pub const GENESIS_WIDTH: u32 = 320;
pub const GENESIS_HEIGHT: u32 = 240;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum InputMapping {
    #[default]
    Original,
    Ergonomic,
}

/// Key mapping for player 1 (Physical KeyCode)
#[cfg(any(feature = "gui", feature = "test_headless"))]
pub fn keycode_to_button(keycode: KeyCode, mapping: InputMapping) -> Option<(&'static str, bool)> {
    match mapping {
        InputMapping::Original => match keycode {
            // Player 1 - Arrow keys + ZXC/Enter
            KeyCode::ArrowUp => Some(("up", true)),
            KeyCode::ArrowDown => Some(("down", true)),
            KeyCode::ArrowLeft => Some(("left", true)),
            KeyCode::ArrowRight => Some(("right", true)),
            KeyCode::KeyZ => Some(("a", true)),
            KeyCode::KeyX => Some(("b", true)),
            KeyCode::KeyC => Some(("c", true)),
            KeyCode::Enter => Some(("start", true)),
            // 6-button extension
            KeyCode::KeyA => Some(("x", true)),
            KeyCode::KeyS => Some(("y", true)),
            KeyCode::KeyD => Some(("z", true)),
            KeyCode::KeyQ => Some(("mode", true)),
            _ => None,
        },
        InputMapping::Ergonomic => match keycode {
            // Player 1 - D-pad (WASD physical or Arrow keys)
            KeyCode::KeyW | KeyCode::ArrowUp => Some(("up", true)),
            KeyCode::KeyS | KeyCode::ArrowDown => Some(("down", true)),
            KeyCode::KeyA | KeyCode::ArrowLeft => Some(("left", true)),
            KeyCode::KeyD | KeyCode::ArrowRight => Some(("right", true)),

            // Face Buttons (Bottom Row: J, K, L -> A, B, C)
            KeyCode::KeyJ => Some(("a", true)),
            KeyCode::KeyK => Some(("b", true)),
            KeyCode::KeyL => Some(("c", true)),

            // Face Buttons (Top Row: U, I, O -> X, Y, Z)
            KeyCode::KeyU => Some(("x", true)),
            KeyCode::KeyI => Some(("y", true)),
            KeyCode::KeyO => Some(("z", true)),

            // System Buttons
            KeyCode::Enter => Some(("start", true)),
            KeyCode::Space => Some(("mode", true)),

            // Legacy/Alternative Mapping (ZX for A/B)
            KeyCode::KeyZ => Some(("a", true)),
            KeyCode::KeyX => Some(("b", true)),
            KeyCode::KeyC => Some(("c", true)),
            _ => None,
        },
    }
}

/// Key mapping for player 1 (Logical Key)
#[cfg(any(feature = "gui", feature = "test_headless"))]
pub fn key_to_button(key: &Key, mapping: InputMapping) -> Option<(&'static str, bool)> {
    match mapping {
        InputMapping::Original => match key {
            Key::Named(winit::keyboard::NamedKey::ArrowUp) => Some(("up", true)),
            Key::Named(winit::keyboard::NamedKey::ArrowDown) => Some(("down", true)),
            Key::Named(winit::keyboard::NamedKey::ArrowLeft) => Some(("left", true)),
            Key::Named(winit::keyboard::NamedKey::ArrowRight) => Some(("right", true)),
            Key::Character(s) if s == "z" || s == "Z" => Some(("a", true)),
            Key::Character(s) if s == "x" || s == "X" => Some(("b", true)),
            Key::Character(s) if s == "c" || s == "C" => Some(("c", true)),
            Key::Named(winit::keyboard::NamedKey::Enter) => Some(("start", true)),
            // 6-button extension
            Key::Character(s) if s == "a" || s == "A" => Some(("x", true)),
            Key::Character(s) if s == "s" || s == "S" => Some(("y", true)),
            Key::Character(s) if s == "d" || s == "D" => Some(("z", true)),
            Key::Character(s) if s == "q" || s == "Q" => Some(("mode", true)),
            _ => None,
        },
        InputMapping::Ergonomic => match key {
            // Player 1 - D-pad (WASD physical or Arrow keys)
            Key::Named(winit::keyboard::NamedKey::ArrowUp) => Some(("up", true)),
            Key::Named(winit::keyboard::NamedKey::ArrowDown) => Some(("down", true)),
            Key::Named(winit::keyboard::NamedKey::ArrowLeft) => Some(("left", true)),
            Key::Named(winit::keyboard::NamedKey::ArrowRight) => Some(("right", true)),

            Key::Character(s) if s == "w" || s == "W" => Some(("up", true)),
            Key::Character(s) if s == "s" || s == "S" => Some(("down", true)),
            Key::Character(s) if s == "a" || s == "A" => Some(("left", true)),
            Key::Character(s) if s == "d" || s == "D" => Some(("right", true)),
            Key::Character(s) if s == "j" || s == "J" => Some(("a", true)),
            Key::Character(s) if s == "k" || s == "K" => Some(("b", true)),
            Key::Character(s) if s == "l" || s == "L" => Some(("c", true)),
            Key::Character(s) if s == "u" || s == "U" => Some(("x", true)),
            Key::Character(s) if s == "i" || s == "I" => Some(("y", true)),
            Key::Character(s) if s == "o" || s == "O" => Some(("z", true)),
            Key::Named(winit::keyboard::NamedKey::Enter) => Some(("start", true)),
            Key::Named(winit::keyboard::NamedKey::Space) => Some(("mode", true)),
            _ => None,
        },
    }
}
/// Keys offered in the "Input Mapping" window: (stored winit name, display label).
#[cfg(any(feature = "gui", feature = "test_headless"))]
static NAMED_BINDABLE: &[(&str, &str)] = &[
    ("ArrowUp", "Up Arrow"),
    ("ArrowDown", "Down Arrow"),
    ("ArrowLeft", "Left Arrow"),
    ("ArrowRight", "Right Arrow"),
    ("Space", "Space"),
    ("Enter", "Enter"),
    ("Tab", "Tab"),
    ("F1", "F1"), ("F2", "F2"), ("F3", "F3"), ("F4", "F4"),
    ("F6", "F6"), ("F7", "F7"),
    ("F9", "F9"), ("F10", "F10"), ("F11", "F11"), ("F12", "F12"),
    ("ShiftLeft", "Left Shift"), ("ShiftRight", "Right Shift"),
    ("ControlLeft", "Left Ctrl"), ("ControlRight", "Right Ctrl"),
    ("AltLeft", "Left Alt"), ("AltRight", "Right Alt"),
    ("Comma", ", (Comma)"), ("Period", ". (Period)"), ("Slash", "/ (Slash)"),
    ("Semicolon", "; (Semicolon)"), ("Quote", "' (Quote)"),
    ("Minus", "- (Minus)"), ("Equal", "= (Equal)"), ("Backquote", "` (Backquote)"),
    ("BracketLeft", "[ (L Bracket)"), ("BracketRight", "] (R Bracket)"),
    ("Numpad0", "Numpad 0"), ("Numpad1", "Numpad 1"), ("Numpad2", "Numpad 2"),
    ("Numpad3", "Numpad 3"), ("Numpad4", "Numpad 4"), ("Numpad5", "Numpad 5"),
    ("Numpad6", "Numpad 6"), ("Numpad7", "Numpad 7"), ("Numpad8", "Numpad 8"),
    ("Numpad9", "Numpad 9"), ("NumpadEnter", "Numpad Enter"),
    ("Insert", "Insert"), ("Delete", "Delete"), ("Home", "Home"), ("End", "End"),
    ("PageUp", "Page Up"), ("PageDown", "Page Down"),
];

/// Full list of bindable keys: letters, digits, and NAMED_BINDABLE.
/// (F5/F8/Escape are excluded: save-state, load-state, quit.)
#[cfg(any(feature = "gui", feature = "test_headless"))]
pub fn bindable_keys() -> Vec<(String, String)> {
    let mut keys: Vec<(String, String)> = Vec::with_capacity(36 + NAMED_BINDABLE.len());
    for i in 0..26u8 {
        let ch = (b'A' + i) as char;
        keys.push((format!("Key{ch}"), ch.to_string()));
    }
    for i in 0..10 {
        keys.push((format!("Digit{i}"), i.to_string()));
    }
    for (stored, display) in NAMED_BINDABLE {
        keys.push((stored.to_string(), display.to_string()));
    }
    keys
}

/// Map a stored key name back to a winit KeyCode.
#[cfg(any(feature = "gui", feature = "test_headless"))]
pub fn key_name_to_keycode(name: &str) -> Option<KeyCode> {
    if let Some(rest) = name.strip_prefix("Key") {
        if rest.len() == 1 {
            let b = rest.as_bytes()[0];
            if b.is_ascii_uppercase() {
                const LETTERS: [KeyCode; 26] = [
                    KeyCode::KeyA, KeyCode::KeyB, KeyCode::KeyC, KeyCode::KeyD, KeyCode::KeyE,
                    KeyCode::KeyF, KeyCode::KeyG, KeyCode::KeyH, KeyCode::KeyI, KeyCode::KeyJ,
                    KeyCode::KeyK, KeyCode::KeyL, KeyCode::KeyM, KeyCode::KeyN, KeyCode::KeyO,
                    KeyCode::KeyP, KeyCode::KeyQ, KeyCode::KeyR, KeyCode::KeyS, KeyCode::KeyT,
                    KeyCode::KeyU, KeyCode::KeyV, KeyCode::KeyW, KeyCode::KeyX, KeyCode::KeyY,
                    KeyCode::KeyZ,
                ];
                return Some(LETTERS[(b - b'A') as usize]);
            }
        }
    }
    if let Some(rest) = name.strip_prefix("Digit") {
        if rest.len() == 1 {
            let b = rest.as_bytes()[0];
            if b.is_ascii_digit() {
                const DIGITS: [KeyCode; 10] = [
                    KeyCode::Digit0, KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3,
                    KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7,
                    KeyCode::Digit8, KeyCode::Digit9,
                ];
                return Some(DIGITS[(b - b'0') as usize]);
            }
        }
    }
    use winit::keyboard::KeyCode as K;
    let code = match name {
        "ArrowUp" => K::ArrowUp,
        "ArrowDown" => K::ArrowDown,
        "ArrowLeft" => K::ArrowLeft,
        "ArrowRight" => K::ArrowRight,
        "Space" => K::Space,
        "Enter" => K::Enter,
        "Tab" => K::Tab,
        "F1" => K::F1, "F2" => K::F2, "F3" => K::F3, "F4" => K::F4,
        "F6" => K::F6, "F7" => K::F7,
        "F9" => K::F9, "F10" => K::F10, "F11" => K::F11, "F12" => K::F12,
        "ShiftLeft" => K::ShiftLeft,
        "ShiftRight" => K::ShiftRight,
        "ControlLeft" => K::ControlLeft,
        "ControlRight" => K::ControlRight,
        "AltLeft" => K::AltLeft,
        "AltRight" => K::AltRight,
        "Comma" => K::Comma,
        "Period" => K::Period,
        "Slash" => K::Slash,
        "Semicolon" => K::Semicolon,
        "Quote" => K::Quote,
        "Minus" => K::Minus,
        "Equal" => K::Equal,
        "Backquote" => K::Backquote,
        "BracketLeft" => K::BracketLeft,
        "BracketRight" => K::BracketRight,
        "Numpad0" => K::Numpad0, "Numpad1" => K::Numpad1, "Numpad2" => K::Numpad2,
        "Numpad3" => K::Numpad3, "Numpad4" => K::Numpad4, "Numpad5" => K::Numpad5,
        "Numpad6" => K::Numpad6, "Numpad7" => K::Numpad7, "Numpad8" => K::Numpad8,
        "Numpad9" => K::Numpad9,
        "NumpadEnter" => K::NumpadEnter,
        "Insert" => K::Insert,
        "Delete" => K::Delete,
        "Home" => K::Home,
        "End" => K::End,
        "PageUp" => K::PageUp,
        "PageDown" => K::PageDown,
        _ => return None,
    };
    Some(code)
}



/// Map a pressed KeyCode back to its stored key name.
#[cfg(any(feature = "gui", feature = "test_headless"))]
pub fn keycode_to_key_name(keycode: KeyCode) -> Option<String> {
    use winit::keyboard::KeyCode as K;
    if (K::KeyA..=K::KeyZ).contains(&keycode) {
        let ch = (b'A' + (keycode as u32 - K::KeyA as u32) as u8) as char;
        return Some(format!("Key{ch}"));
    }
    if (K::Digit0..=K::Digit9).contains(&keycode) {
        let d = keycode as u32 - K::Digit0 as u32;
        return Some(format!("Digit{d}"));
    }
    NAMED_BINDABLE
        .iter()
        .find(|(name, _)| key_name_to_keycode(name) == Some(keycode))
        .map(|(name, _)| name.to_string())
}

/// Resolve a custom Input-Mapping-window binding for a pressed key.
#[cfg(any(feature = "gui", feature = "test_headless"))]
pub fn custom_button_for_key<'a>(
    keycode: KeyCode,
    custom: &'a std::collections::HashMap<String, String>,
) -> Option<&'a str> {
    for (button, key) in custom {
        if key_name_to_keycode(key) == Some(keycode) {
            return Some(button.as_str());
        }
    }
    None
}

/// Human-readable preset binding for a button (shown in the Input Mapping window).
#[cfg(any(feature = "gui", feature = "test_headless"))]
pub fn preset_key_display(button: &str, mapping: InputMapping) -> &'static str {
    match (mapping, button) {
        (InputMapping::Original, "up") => "Up Arrow",
        (InputMapping::Original, "down") => "Down Arrow",
        (InputMapping::Original, "left") => "Left Arrow",
        (InputMapping::Original, "right") => "Right Arrow",
        (InputMapping::Original, "a") => "Z",
        (InputMapping::Original, "b") => "X",
        (InputMapping::Original, "c") => "C",
        (InputMapping::Original, "x") => "A",
        (InputMapping::Original, "y") => "S",
        (InputMapping::Original, "z") => "D",
        (InputMapping::Original, "start") => "Enter",
        (InputMapping::Original, _) => "Q",
        (InputMapping::Ergonomic, "up") => "W / Up Arrow",
        (InputMapping::Ergonomic, "down") => "S / Down Arrow",
        (InputMapping::Ergonomic, "left") => "A / Left Arrow",
        (InputMapping::Ergonomic, "right") => "D / Right Arrow",
        (InputMapping::Ergonomic, "a") => "J",
        (InputMapping::Ergonomic, "b") => "K",
        (InputMapping::Ergonomic, "c") => "L",
        (InputMapping::Ergonomic, "x") => "U",
        (InputMapping::Ergonomic, "y") => "I",
        (InputMapping::Ergonomic, "z") => "O",
        (InputMapping::Ergonomic, "start") => "Enter",
        (InputMapping::Ergonomic, _) => "Space",
    }
}

/// Lookup table for RGB565 to RGBA8 conversion
static RGB565_TO_RGBA8_LUT: LazyLock<[u32; 65536]> = LazyLock::new(|| {
    let mut lut = [0u32; 65536];
    for pixel in 0..65536 {
        let p = pixel as u16;
        let r5 = ((p >> 11) & 0x1F) as u8;
        let g6 = ((p >> 5) & 0x3F) as u8;
        let b5 = (p & 0x1F) as u8;

        let r = (r5 << 3) | (r5 >> 2);
        let g = (g6 << 2) | (g6 >> 4);
        let b = (b5 << 3) | (b5 >> 2);
        let a = 255u8;

        lut[pixel] = u32::from_le_bytes([r, g, b, a]);
    }
    lut
});

/// Convert RGB565 framebuffer to RGBA8 for pixels crate
pub fn rgb565_to_rgba8(framebuffer_565: &[u16], output: &mut [u8]) {
    for (&pixel, chunk) in framebuffer_565.iter().zip(output.chunks_exact_mut(4)) {
        let rgba = RGB565_TO_RGBA8_LUT[pixel as usize];
        chunk.copy_from_slice(&rgba.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(feature = "gui", feature = "test_headless"))]
    #[test]
    fn test_keycode_mapping() {
        assert_eq!(
            keycode_to_button(KeyCode::KeyZ, InputMapping::Original),
            Some(("a", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::KeyX, InputMapping::Original),
            Some(("b", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::ArrowUp, InputMapping::Original),
            Some(("up", true))
        );

        // Ensure other basic mappings map properly
        assert_eq!(
            keycode_to_button(KeyCode::ArrowDown, InputMapping::Original),
            Some(("down", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::KeyC, InputMapping::Original),
            Some(("c", true))
        );
    }

    #[cfg(any(feature = "gui", feature = "test_headless"))]
    #[test]
    fn test_original_mapping_comprehensive() {
        let mapping = InputMapping::Original;

        // Directional keys
        assert_eq!(
            keycode_to_button(KeyCode::ArrowUp, mapping),
            Some(("up", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::ArrowDown, mapping),
            Some(("down", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::ArrowLeft, mapping),
            Some(("left", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::ArrowRight, mapping),
            Some(("right", true))
        );

        // Action keys (ABC)
        assert_eq!(keycode_to_button(KeyCode::KeyZ, mapping), Some(("a", true)));
        assert_eq!(keycode_to_button(KeyCode::KeyX, mapping), Some(("b", true)));
        assert_eq!(keycode_to_button(KeyCode::KeyC, mapping), Some(("c", true)));

        // Start
        assert_eq!(
            keycode_to_button(KeyCode::Enter, mapping),
            Some(("start", true))
        );

        // 6-button extension (XYZ Mode)
        assert_eq!(keycode_to_button(KeyCode::KeyA, mapping), Some(("x", true)));
        assert_eq!(keycode_to_button(KeyCode::KeyS, mapping), Some(("y", true)));
        assert_eq!(keycode_to_button(KeyCode::KeyD, mapping), Some(("z", true)));
        assert_eq!(
            keycode_to_button(KeyCode::KeyQ, mapping),
            Some(("mode", true))
        );
    }

    #[cfg(any(feature = "gui", feature = "test_headless"))]
    #[test]
    fn test_ergonomic_mapping_comprehensive() {
        let mapping = InputMapping::Ergonomic;

        // Directional keys (WASD)
        assert_eq!(
            keycode_to_button(KeyCode::KeyW, mapping),
            Some(("up", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::KeyS, mapping),
            Some(("down", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::KeyA, mapping),
            Some(("left", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::KeyD, mapping),
            Some(("right", true))
        );

        // Directional keys (Arrows)
        assert_eq!(
            keycode_to_button(KeyCode::ArrowUp, mapping),
            Some(("up", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::ArrowDown, mapping),
            Some(("down", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::ArrowLeft, mapping),
            Some(("left", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::ArrowRight, mapping),
            Some(("right", true))
        );

        // Face Buttons (JKL -> ABC)
        assert_eq!(keycode_to_button(KeyCode::KeyJ, mapping), Some(("a", true)));
        assert_eq!(keycode_to_button(KeyCode::KeyK, mapping), Some(("b", true)));
        assert_eq!(keycode_to_button(KeyCode::KeyL, mapping), Some(("c", true)));

        // Face Buttons (UIO -> XYZ)
        assert_eq!(keycode_to_button(KeyCode::KeyU, mapping), Some(("x", true)));
        assert_eq!(keycode_to_button(KeyCode::KeyI, mapping), Some(("y", true)));
        assert_eq!(keycode_to_button(KeyCode::KeyO, mapping), Some(("z", true)));

        // System Buttons
        assert_eq!(
            keycode_to_button(KeyCode::Enter, mapping),
            Some(("start", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::Space, mapping),
            Some(("mode", true))
        );

        // Legacy/Alternative Mapping (ZX -> AB)
        assert_eq!(keycode_to_button(KeyCode::KeyZ, mapping), Some(("a", true)));
        assert_eq!(keycode_to_button(KeyCode::KeyX, mapping), Some(("b", true)));
        assert_eq!(keycode_to_button(KeyCode::KeyC, mapping), Some(("c", true)));
    }

    #[cfg(any(feature = "gui", feature = "test_headless"))]
    #[test]
    fn test_unmapped_keys() {
        let mappings = [InputMapping::Original, InputMapping::Ergonomic];

        // Comprehensive list of commonly unmapped keys to ensure they don't accidentally trigger actions
        let unmapped_keys = [
            // Unmapped letters (B, E, F, G, H, M, N, P, R, T, V, X/Y/Z depending on mapping)
            KeyCode::KeyB,
            KeyCode::KeyE,
            KeyCode::KeyF,
            KeyCode::KeyG,
            KeyCode::KeyH,
            KeyCode::KeyM,
            KeyCode::KeyN,
            KeyCode::KeyP,
            KeyCode::KeyR,
            KeyCode::KeyT,
            KeyCode::KeyV,
            // Digits
            KeyCode::Digit0,
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
            // Function keys
            KeyCode::F1,
            KeyCode::F2,
            KeyCode::F3,
            KeyCode::F4,
            KeyCode::F5,
            KeyCode::F6,
            KeyCode::F7,
            KeyCode::F8,
            KeyCode::F9,
            KeyCode::F10,
            KeyCode::F11,
            KeyCode::F12,
            // Special keys
            KeyCode::Escape,
            KeyCode::ShiftLeft,
            KeyCode::ShiftRight,
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::AltLeft,
            KeyCode::AltRight,
            KeyCode::Tab,
            KeyCode::Backspace,
        ];

        for mapping in mappings {
            for key in unmapped_keys {
                // Some keys are unmapped in one mode but mapped in another
                // We test keys that should be unmapped in BOTH modes here.
                assert_eq!(
                    keycode_to_button(key, mapping),
                    None,
                    "Key {:?} should not be mapped in {:?} mode",
                    key,
                    mapping
                );
            }
        }
    }

    #[cfg(any(feature = "gui", feature = "test_headless"))]
    #[test]
    fn test_overlapping_keys() {
        // Verify keys that map to different things or same things across mappings

        // Enter is start in both
        assert_eq!(
            keycode_to_button(KeyCode::Enter, InputMapping::Original),
            Some(("start", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::Enter, InputMapping::Ergonomic),
            Some(("start", true))
        );

        // 'Z' is 'a' in Original, and also 'a' in Ergonomic (legacy)
        assert_eq!(
            keycode_to_button(KeyCode::KeyZ, InputMapping::Original),
            Some(("a", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::KeyZ, InputMapping::Ergonomic),
            Some(("a", true))
        );

        // 'A' is 'x' in Original, but 'left' in Ergonomic
        assert_eq!(
            keycode_to_button(KeyCode::KeyA, InputMapping::Original),
            Some(("x", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::KeyA, InputMapping::Ergonomic),
            Some(("left", true))
        );

        // 'S' is 'y' in Original, but 'down' in Ergonomic
        assert_eq!(
            keycode_to_button(KeyCode::KeyS, InputMapping::Original),
            Some(("y", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::KeyS, InputMapping::Ergonomic),
            Some(("down", true))
        );

        // 'D' is 'z' in Original, but 'right' in Ergonomic
        assert_eq!(
            keycode_to_button(KeyCode::KeyD, InputMapping::Original),
            Some(("z", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::KeyD, InputMapping::Ergonomic),
            Some(("right", true))
        );
    }

    #[test]
    fn test_rgb565_to_rgba8_black() {
        let input = [0x0000u16];
        let mut output = [0u8; 4];
        rgb565_to_rgba8(&input, &mut output);
        assert_eq!(output, [0, 0, 0, 255]);
    }

    #[test]
    fn test_rgb565_to_rgba8_white() {
        let input = [0xFFFFu16];
        let mut output = [0u8; 4];
        rgb565_to_rgba8(&input, &mut output);
        assert_eq!(output, [255, 255, 255, 255]);
    }

    #[test]
    fn test_rgb565_to_rgba8_colors() {
        // Red: 0xF800 (11111 000000 00000)
        let mut output = [0u8; 4];
        rgb565_to_rgba8(&[0xF800], &mut output);
        assert_eq!(output, [255, 0, 0, 255]);

        // Green: 0x07E0 (00000 111111 00000)
        rgb565_to_rgba8(&[0x07E0], &mut output);
        assert_eq!(output, [0, 255, 0, 255]);

        // Blue: 0x001F (00000 000000 11111)
        rgb565_to_rgba8(&[0x001F], &mut output);
        assert_eq!(output, [0, 0, 255, 255]);

        // A mid gray
        rgb565_to_rgba8(&[0x8410], &mut output); // 10000 100000 10000
                                                 // r: (16 << 3) | (16 >> 2) = 128 | 4 = 132
                                                 // g: (32 << 2) | (32 >> 4) = 128 | 2 = 130
                                                 // b: (16 << 3) | (16 >> 2) = 128 | 4 = 132
        assert_eq!(output, [132, 130, 132, 255]);
    }

    #[test]
    fn test_rgb565_to_rgba8_edge_cases() {
        // Test empty input/output
        let empty_input: [u16; 0] = [];
        let mut empty_output: [u8; 0] = [];
        rgb565_to_rgba8(&empty_input, &mut empty_output); // Should not panic

        // Mismatched lengths: Output too small (should process as much as it can)
        let input = [0xFFFF, 0xFFFF];
        let mut short_output = [0u8; 4]; // Only room for 1 pixel
        rgb565_to_rgba8(&input, &mut short_output);
        assert_eq!(short_output, [255, 255, 255, 255]); // First pixel processed correctly

        // Mismatched lengths: Output < 4 bytes (should not process at all due to chunks_exact_mut(4))
        let mut very_short_output = [1, 2, 3];
        rgb565_to_rgba8(&input, &mut very_short_output);
        assert_eq!(very_short_output, [1, 2, 3]); // Should be entirely untouched

        // Mismatched lengths: Output too large (should leave remainder untouched)
        let input_short = [0x0000]; // 1 pixel (black)
        let mut long_output = [255u8; 8]; // Room for 2 pixels, initialized to white
        rgb565_to_rgba8(&input_short, &mut long_output);
        // First pixel is black, second pixel remains white
        assert_eq!(long_output, [0, 0, 0, 255, 255, 255, 255, 255]);

        // Partial chunks at the end of output should be ignored safely
        let input_partial = [0xFFFF, 0xFFFF];
        let mut partial_output = [0u8; 7]; // Room for 1 pixel + 3 bytes
        rgb565_to_rgba8(&input_partial, &mut partial_output);
        // First pixel processed, remaining 3 bytes untouched
        assert_eq!(partial_output, [255, 255, 255, 255, 0, 0, 0]);

        // Test near-zero edge case (LSB set)
        // R:1, G:0, B:0 -> 0x0800 (00001 000000 00000)
        let mut output = [0u8; 4];
        rgb565_to_rgba8(&[0x0800], &mut output);
        // r: (1 << 3) | (1 >> 2) = 8 | 0 = 8
        assert_eq!(output, [8, 0, 0, 255]);

        // R:0, G:1, B:0 -> 0x0020 (00000 000001 00000)
        rgb565_to_rgba8(&[0x0020], &mut output);
        // g: (1 << 2) | (1 >> 4) = 4 | 0 = 4
        assert_eq!(output, [0, 4, 0, 255]);

        // R:0, G:0, B:1 -> 0x0001 (00000 000000 00001)
        rgb565_to_rgba8(&[0x0001], &mut output);
        // b: (1 << 3) | (1 >> 2) = 8 | 0 = 8
        assert_eq!(output, [0, 0, 8, 255]);

        // Test max-1 edge case (MSB cleared)
        // R:30, G:63, B:31 -> 0xF7FF (11110 111111 11111)
        rgb565_to_rgba8(&[0xF7FF], &mut output);
        // r: (30 << 3) | (30 >> 2) = 240 | 7 = 247
        assert_eq!(output, [247, 255, 255, 255]);

        // R:31, G:62, B:31 -> 0xFFDF (11111 111110 11111)
        rgb565_to_rgba8(&[0xFFDF], &mut output);
        // g: (62 << 2) | (62 >> 4) = 248 | 3 = 251
        assert_eq!(output, [255, 251, 255, 255]);

        // R:31, G:63, B:30 -> 0xFFFE (11111 111111 11110)
        rgb565_to_rgba8(&[0xFFFE], &mut output);
        // b: (30 << 3) | (30 >> 2) = 240 | 7 = 247
        assert_eq!(output, [255, 255, 247, 255]);

        // Extremely large buffer (e.g., 2MB output)
        let large_input = vec![0x0000u16; 512 * 1024];
        let mut large_output = vec![0u8; 512 * 1024 * 4];
        rgb565_to_rgba8(&large_input, &mut large_output);
        assert_eq!(large_output[3], 255);
        assert_eq!(large_output[large_output.len() - 1], 255);

        // Buffer not aligned to 4-byte boundary (e.g., 1003 bytes)
        let mut unaligned_output = vec![0u8; 1003];
        rgb565_to_rgba8(&vec![0xFFFFu16; 300], &mut unaligned_output);
        // 1003 / 4 = 250 pixels should be processed
        assert_eq!(unaligned_output[250 * 4 - 1], 255); // Last byte (A) of 250th pixel
                                                        // Bytes 1000, 1001, 1002 (0-indexed) should be untouched
        assert_eq!(unaligned_output[1000], 0);
        assert_eq!(unaligned_output[1001], 0);
        assert_eq!(unaligned_output[1002], 0);
    }

    #[test]
    fn test_rgb565_to_rgba8_exhaustive() {
        let all_pixels: Vec<u16> = (0..=65535).collect();
        let mut output = vec![0u8; 65536 * 4];
        rgb565_to_rgba8(&all_pixels, &mut output);

        for (i, &pixel) in all_pixels.iter().enumerate() {
            let chunk = &output[i * 4..(i + 1) * 4];
            let r5 = ((pixel >> 11) & 0x1F) as u8;
            let g6 = ((pixel >> 5) & 0x3F) as u8;
            let b5 = (pixel & 0x1F) as u8;

            let expected_r = (r5 << 3) | (r5 >> 2);
            let expected_g = (g6 << 2) | (g6 >> 4);
            let expected_b = (b5 << 3) | (b5 >> 2);

            assert_eq!(chunk[0], expected_r);
            assert_eq!(chunk[1], expected_g);
            assert_eq!(chunk[2], expected_b);
            assert_eq!(chunk[3], 255);
        }
    }

    #[cfg(any(feature = "gui", feature = "test_headless"))]
    #[test]
    fn test_keycode_to_button_exhaustive_table() {
        let test_cases = [
            // Original Mapping
            (InputMapping::Original, KeyCode::ArrowUp, Some(("up", true))),
            (
                InputMapping::Original,
                KeyCode::ArrowDown,
                Some(("down", true)),
            ),
            (
                InputMapping::Original,
                KeyCode::ArrowLeft,
                Some(("left", true)),
            ),
            (
                InputMapping::Original,
                KeyCode::ArrowRight,
                Some(("right", true)),
            ),
            (InputMapping::Original, KeyCode::KeyZ, Some(("a", true))),
            (InputMapping::Original, KeyCode::KeyX, Some(("b", true))),
            (InputMapping::Original, KeyCode::KeyC, Some(("c", true))),
            (
                InputMapping::Original,
                KeyCode::Enter,
                Some(("start", true)),
            ),
            (InputMapping::Original, KeyCode::KeyA, Some(("x", true))),
            (InputMapping::Original, KeyCode::KeyS, Some(("y", true))),
            (InputMapping::Original, KeyCode::KeyD, Some(("z", true))),
            (InputMapping::Original, KeyCode::KeyQ, Some(("mode", true))),
            (InputMapping::Original, KeyCode::KeyB, None),
            // Ergonomic Mapping
            (InputMapping::Ergonomic, KeyCode::KeyW, Some(("up", true))),
            (InputMapping::Ergonomic, KeyCode::KeyS, Some(("down", true))),
            (InputMapping::Ergonomic, KeyCode::KeyA, Some(("left", true))),
            (
                InputMapping::Ergonomic,
                KeyCode::KeyD,
                Some(("right", true)),
            ),
            (
                InputMapping::Ergonomic,
                KeyCode::ArrowUp,
                Some(("up", true)),
            ),
            (InputMapping::Ergonomic, KeyCode::KeyJ, Some(("a", true))),
            (InputMapping::Ergonomic, KeyCode::KeyK, Some(("b", true))),
            (InputMapping::Ergonomic, KeyCode::KeyL, Some(("c", true))),
            (InputMapping::Ergonomic, KeyCode::KeyU, Some(("x", true))),
            (InputMapping::Ergonomic, KeyCode::KeyI, Some(("y", true))),
            (InputMapping::Ergonomic, KeyCode::KeyO, Some(("z", true))),
            (
                InputMapping::Ergonomic,
                KeyCode::Enter,
                Some(("start", true)),
            ),
            (
                InputMapping::Ergonomic,
                KeyCode::Space,
                Some(("mode", true)),
            ),
            (InputMapping::Ergonomic, KeyCode::KeyZ, Some(("a", true))),
            (InputMapping::Ergonomic, KeyCode::KeyX, Some(("b", true))),
            (InputMapping::Ergonomic, KeyCode::KeyC, Some(("c", true))),
            (InputMapping::Ergonomic, KeyCode::KeyE, None),
        ];

        for (mapping, keycode, expected) in test_cases {
            let result = keycode_to_button(keycode, mapping);
            assert_eq!(
                result, expected,
                "Failed for mapping {:?} and keycode {:?}",
                mapping, keycode
            );
        }
    }

    #[cfg(any(feature = "gui", feature = "test_headless"))]
    #[test]
    fn test_keycode_to_button_mode_isolation() {
        // Keys that only resolve in Ergonomic mode
        assert_eq!(
            keycode_to_button(KeyCode::KeyW, InputMapping::Original),
            None
        );
        assert_eq!(
            keycode_to_button(KeyCode::KeyJ, InputMapping::Original),
            None
        );
        assert_eq!(
            keycode_to_button(KeyCode::Space, InputMapping::Original),
            None
        );

        // KeyA is mapped in both modes but to different buttons
        assert_eq!(
            keycode_to_button(KeyCode::KeyA, InputMapping::Original),
            Some(("x", true))
        );
        assert_eq!(
            keycode_to_button(KeyCode::KeyA, InputMapping::Ergonomic),
            Some(("left", true))
        );

        // Unmapped in both modes
        assert_eq!(
            keycode_to_button(KeyCode::KeyM, InputMapping::Original),
            None
        );
        assert_eq!(
            keycode_to_button(KeyCode::KeyM, InputMapping::Ergonomic),
            None
        );
    }

    #[cfg(any(feature = "gui", feature = "test_headless"))]
    #[test]
    fn test_key_to_button_comprehensive() {
        use winit::keyboard::NamedKey;

        // Original mapping
        let mapping = InputMapping::Original;
        assert_eq!(
            key_to_button(&Key::Named(NamedKey::ArrowUp), mapping),
            Some(("up", true))
        );
        assert_eq!(
            key_to_button(&Key::Named(NamedKey::ArrowDown), mapping),
            Some(("down", true))
        );
        assert_eq!(
            key_to_button(&Key::Named(NamedKey::ArrowLeft), mapping),
            Some(("left", true))
        );
        assert_eq!(
            key_to_button(&Key::Named(NamedKey::ArrowRight), mapping),
            Some(("right", true))
        );
        assert_eq!(
            key_to_button(&Key::Named(NamedKey::Enter), mapping),
            Some(("start", true))
        );
        // Action keys (case insensitive)
        assert_eq!(
            key_to_button(&Key::Character("z".into()), mapping),
            Some(("a", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("Z".into()), mapping),
            Some(("a", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("x".into()), mapping),
            Some(("b", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("X".into()), mapping),
            Some(("b", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("c".into()), mapping),
            Some(("c", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("C".into()), mapping),
            Some(("c", true))
        );
        // 6-button extension
        assert_eq!(
            key_to_button(&Key::Character("a".into()), mapping),
            Some(("x", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("A".into()), mapping),
            Some(("x", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("s".into()), mapping),
            Some(("y", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("d".into()), mapping),
            Some(("z", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("q".into()), mapping),
            Some(("mode", true))
        );
        // Unmapped in Original mode
        assert_eq!(key_to_button(&Key::Named(NamedKey::Space), mapping), None);

        // Ergonomic mapping
        let mapping = InputMapping::Ergonomic;
        assert_eq!(
            key_to_button(&Key::Character("w".into()), mapping),
            Some(("up", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("W".into()), mapping),
            Some(("up", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("s".into()), mapping),
            Some(("down", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("a".into()), mapping),
            Some(("left", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("d".into()), mapping),
            Some(("right", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("j".into()), mapping),
            Some(("a", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("J".into()), mapping),
            Some(("a", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("k".into()), mapping),
            Some(("b", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("l".into()), mapping),
            Some(("c", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("u".into()), mapping),
            Some(("x", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("i".into()), mapping),
            Some(("y", true))
        );
        assert_eq!(
            key_to_button(&Key::Character("o".into()), mapping),
            Some(("z", true))
        );
        assert_eq!(
            key_to_button(&Key::Named(NamedKey::Enter), mapping),
            Some(("start", true))
        );
        assert_eq!(
            key_to_button(&Key::Named(NamedKey::Space), mapping),
            Some(("mode", true))
        );
        // Ergonomic also supports arrows
        assert_eq!(
            key_to_button(&Key::Named(NamedKey::ArrowDown), mapping),
            Some(("down", true))
        );
    }
}
