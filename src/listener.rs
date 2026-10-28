use rdev::{EventType, Key};

/// Returns the scan code and whether the key went down (false = released).
pub fn classify(held: &mut Vec<Key>, event: &EventType) -> Option<(u16, bool)> {
    match event {
        EventType::KeyPress(key) => {
            if held.contains(key) {
                return None;
            }
            held.push(*key);
            key_to_scan_code(*key).map(|code| (code, true))
        }
        EventType::KeyRelease(key) => {
            let pos = held.iter().position(|k| k == key)?;
            held.remove(pos);
            key_to_scan_code(*key).map(|code| (code, false))
        }
        _ => None,
    }
}

pub fn key_to_scan_code(key: Key) -> Option<u16> {
    use Key::*;
    Some(match key {
        Escape => 1,
        Num1 => 2,
        Num2 => 3,
        Num3 => 4,
        Num4 => 5,
        Num5 => 6,
        Num6 => 7,
        Num7 => 8,
        Num8 => 9,
        Num9 => 10,
        Num0 => 11,
        Minus => 12,
        Equal => 13,
        Backspace => 14,
        Tab => 15,
        KeyQ => 16,
        KeyW => 17,
        KeyE => 18,
        KeyR => 19,
        KeyT => 20,
        KeyY => 21,
        KeyU => 22,
        KeyI => 23,
        KeyO => 24,
        KeyP => 25,
        LeftBracket => 26,
        RightBracket => 27,
        Return => 28,
        ControlLeft => 29,
        KeyA => 30,
        KeyS => 31,
        KeyD => 32,
        KeyF => 33,
        KeyG => 34,
        KeyH => 35,
        KeyJ => 36,
        KeyK => 37,
        KeyL => 38,
        SemiColon => 39,
        Quote => 40,
        BackQuote => 41,
        ShiftLeft => 42,
        BackSlash | IntlBackslash => 43,
        KeyZ => 44,
        KeyX => 45,
        KeyC => 46,
        KeyV => 47,
        KeyB => 48,
        KeyN => 49,
        KeyM => 50,
        Comma => 51,
        Dot => 52,
        Slash => 53,
        ShiftRight => 54,
        KpMultiply => 55,
        Alt => 56,
        Space => 57,
        CapsLock => 58,
        F1 => 59,
        F2 => 60,
        F3 => 61,
        F4 => 62,
        F5 => 63,
        F6 => 64,
        F7 => 65,
        F8 => 66,
        F9 => 67,
        F10 => 68,
        NumLock => 69,
        ScrollLock => 70,
        Kp7 => 71,
        Kp8 => 72,
        Kp9 => 73,
        KpMinus => 74,
        Kp4 => 75,
        Kp5 => 76,
        Kp6 => 77,
        KpPlus => 78,
        Kp1 => 79,
        Kp2 => 80,
        Kp3 => 81,
        Kp0 => 82,
        KpDelete => 83,
        F11 => 87,
        F12 => 88,
        KpReturn => 3612,
        ControlRight => 3613,
        KpDivide => 3637,
        PrintScreen => 3639,
        AltGr => 3640,
        Pause => 3653,
        Home => 3655,
        PageUp => 3657,
        End => 3663,
        PageDown => 3665,
        Insert => 3666,
        Delete => 3667,
        MetaLeft => 3675,
        MetaRight => 3676,
        UpArrow => 57416,
        LeftArrow => 57419,
        RightArrow => 57421,
        DownArrow => 57424,
        Function | Unknown(_) => return None,
    })
}

#[cfg(target_os = "macos")]
const HINT: &str =
    "grant Input Monitoring (and Accessibility) to your terminal in System Settings → Privacy & Security, then restart brokey";

#[cfg(target_os = "linux")]
const HINT: &str = "needs an X11 session; on Wayland only XWayland apps are heard";

#[cfg(target_os = "windows")]
const HINT: &str = "could not install keyboard hook";

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
const HINT: &str = "check OS permissions for global keyboard input";

pub fn listen(mut on_sound: impl FnMut(u16, bool) + 'static) -> Result<(), String> {
    let mut held: Vec<Key> = Vec::new();
    rdev::listen(move |event| {
        if let Some((code, down)) = classify(&mut held, &event.event_type) {
            on_sound(code, down);
        }
    })
    .map_err(|err| format!("{err:?}: {HINT}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdev::{EventType::*, Key};

    #[test]
    fn classify_press_and_space() {
        let mut held = vec![];
        assert_eq!(classify(&mut held, &KeyPress(Key::KeyA)), Some((30, true)));
        assert_eq!(classify(&mut held, &KeyPress(Key::Space)), Some((57, true)));
        assert_eq!(classify(&mut held, &KeyRelease(Key::Space)), Some((57, false)));
    }

    #[test]
    fn classify_ignores_auto_repeat() {
        let mut held = vec![];
        assert_eq!(classify(&mut held, &KeyPress(Key::KeyA)), Some((30, true)));
        assert_eq!(classify(&mut held, &KeyPress(Key::KeyA)), None);
        assert_eq!(classify(&mut held, &KeyRelease(Key::KeyA)), Some((30, false)));
        assert_eq!(classify(&mut held, &KeyRelease(Key::KeyA)), None);
        assert_eq!(classify(&mut held, &KeyPress(Key::KeyA)), Some((30, true)));
    }

    #[test]
    fn classify_ignores_non_key_events() {
        let mut held = vec![];
        assert_eq!(classify(&mut held, &MouseMove { x: 1.0, y: 1.0 }), None);
        assert_eq!(classify(&mut held, &ButtonPress(rdev::Button::Left)), None);
    }

    #[test]
    fn maps_representative_keys() {
        assert_eq!(key_to_scan_code(Key::KeyA), Some(30));
        assert_eq!(key_to_scan_code(Key::KeyZ), Some(44));
        assert_eq!(key_to_scan_code(Key::Num1), Some(2));
        assert_eq!(key_to_scan_code(Key::Num0), Some(11));
        assert_eq!(key_to_scan_code(Key::Minus), Some(12));
        assert_eq!(key_to_scan_code(Key::Equal), Some(13));
        assert_eq!(key_to_scan_code(Key::LeftBracket), Some(26));
        assert_eq!(key_to_scan_code(Key::SemiColon), Some(39));
        assert_eq!(key_to_scan_code(Key::Quote), Some(40));
        assert_eq!(key_to_scan_code(Key::BackQuote), Some(41));
        assert_eq!(key_to_scan_code(Key::BackSlash), Some(43));
        assert_eq!(key_to_scan_code(Key::IntlBackslash), Some(43));
        assert_eq!(key_to_scan_code(Key::Comma), Some(51));
        assert_eq!(key_to_scan_code(Key::Dot), Some(52));
        assert_eq!(key_to_scan_code(Key::Slash), Some(53));
        assert_eq!(key_to_scan_code(Key::Space), Some(57));
        assert_eq!(key_to_scan_code(Key::Return), Some(28));
        assert_eq!(key_to_scan_code(Key::Backspace), Some(14));
        assert_eq!(key_to_scan_code(Key::Tab), Some(15));
        assert_eq!(key_to_scan_code(Key::Escape), Some(1));
        assert_eq!(key_to_scan_code(Key::UpArrow), Some(57416));
        assert_eq!(key_to_scan_code(Key::LeftArrow), Some(57419));
        assert_eq!(key_to_scan_code(Key::RightArrow), Some(57421));
        assert_eq!(key_to_scan_code(Key::DownArrow), Some(57424));
        assert_eq!(key_to_scan_code(Key::Unknown(999)), None);
        assert_eq!(key_to_scan_code(Key::Function), None);
    }
}
