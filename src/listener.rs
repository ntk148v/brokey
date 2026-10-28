use rdev::{EventType, Key};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundKind {
    Press,
    Space,
}

pub fn classify(held: &mut Vec<Key>, event: &EventType) -> Option<SoundKind> {
    match event {
        EventType::KeyPress(key) => {
            if held.contains(key) {
                return None;
            }
            held.push(*key);
            if *key == Key::Space {
                Some(SoundKind::Space)
            } else {
                Some(SoundKind::Press)
            }
        }
        EventType::KeyRelease(key) => {
            held.retain(|k| k != key);
            None
        }
        _ => None,
    }
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

pub fn listen(mut on_sound: impl FnMut(SoundKind) + 'static) -> Result<(), String> {
    let mut held: Vec<Key> = Vec::new();
    rdev::listen(move |event| {
        if let Some(k) = classify(&mut held, &event.event_type) {
            on_sound(k);
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
        assert_eq!(
            classify(&mut held, &KeyPress(Key::KeyA)),
            Some(SoundKind::Press)
        );
        assert_eq!(
            classify(&mut held, &KeyPress(Key::Space)),
            Some(SoundKind::Space)
        );
    }

    #[test]
    fn classify_ignores_auto_repeat() {
        let mut held = vec![];
        assert_eq!(
            classify(&mut held, &KeyPress(Key::KeyA)),
            Some(SoundKind::Press)
        );
        assert_eq!(classify(&mut held, &KeyPress(Key::KeyA)), None);
        assert_eq!(classify(&mut held, &KeyRelease(Key::KeyA)), None);
        assert_eq!(
            classify(&mut held, &KeyPress(Key::KeyA)),
            Some(SoundKind::Press)
        );
    }

    #[test]
    fn classify_ignores_non_key_events() {
        let mut held = vec![];
        assert_eq!(classify(&mut held, &MouseMove { x: 1.0, y: 1.0 }), None);
        assert_eq!(classify(&mut held, &ButtonPress(rdev::Button::Left)), None);
    }
}
