//! Source keyboard routing; input state is explicit and testable without a window.
use bevy::prelude::{ButtonInput, KeyCode};
#[derive(Debug, PartialEq)]
pub enum PauseAction {
    Pause,
    Resume,
}
pub fn pause_action(state: &str, keys: &ButtonInput<KeyCode>) -> Option<PauseAction> {
    if state == "pause"
        && [KeyCode::Space, KeyCode::Enter, KeyCode::KeyP]
            .iter()
            .any(|k| keys.just_pressed(*k))
    {
        Some(PauseAction::Resume)
    } else if state == "play"
        && [KeyCode::Escape, KeyCode::KeyP]
            .iter()
            .any(|k| keys.just_pressed(*k))
    {
        Some(PauseAction::Pause)
    } else {
        None
    }
}
pub fn fire_down(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::Space) || keys.pressed(KeyCode::Enter)
}
pub fn riddle_action(label: &str) -> &str {
    if label == "Backspace" { "clear" } else { label }
}
pub const RIDDLE_KEYS: &[(KeyCode, &str)] = &[
    (KeyCode::Digit0, "0"),
    (KeyCode::Digit1, "1"),
    (KeyCode::Digit2, "2"),
    (KeyCode::Digit3, "3"),
    (KeyCode::Digit4, "4"),
    (KeyCode::Digit5, "5"),
    (KeyCode::Digit6, "6"),
    (KeyCode::Digit7, "7"),
    (KeyCode::Digit8, "8"),
    (KeyCode::Digit9, "9"),
    (KeyCode::Numpad0, "0"),
    (KeyCode::Numpad1, "1"),
    (KeyCode::Numpad2, "2"),
    (KeyCode::Numpad3, "3"),
    (KeyCode::Numpad4, "4"),
    (KeyCode::Numpad5, "5"),
    (KeyCode::Numpad6, "6"),
    (KeyCode::Numpad7, "7"),
    (KeyCode::Numpad8, "8"),
    (KeyCode::Numpad9, "9"),
    (KeyCode::Backspace, "clear"),
];
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_pause_keys_do_not_make_escape_resume() {
        for key in [KeyCode::Space, KeyCode::Enter, KeyCode::KeyP] {
            let mut keys = ButtonInput::default();
            keys.press(key);
            assert_eq!(pause_action("pause", &keys), Some(PauseAction::Resume));
        }
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::Escape);
        assert_eq!(pause_action("pause", &keys), None);
        assert_eq!(pause_action("play", &keys), Some(PauseAction::Pause));
        assert_eq!(pause_action("riddle", &keys), None);
    }
    #[test]
    fn number_pad_and_clear_match_source() {
        assert_eq!(RIDDLE_KEYS.len(), 21);
        assert_eq!(
            RIDDLE_KEYS
                .iter()
                .find(|(k, _)| *k == KeyCode::Backspace)
                .map(|(_, s)| riddle_action(s)),
            Some("clear")
        );
        assert_eq!(riddle_action("Backspace"), "clear");
        for n in 0..10 {
            assert_eq!(
                RIDDLE_KEYS
                    .iter()
                    .filter(|(_, v)| *v == n.to_string())
                    .count(),
                2
            );
        }
    }
    #[test]
    fn enter_is_fire_but_numpad_enter_is_not() {
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::Enter);
        assert!(fire_down(&keys));
        keys.release(KeyCode::Enter);
        keys.press(KeyCode::NumpadEnter);
        assert!(!fire_down(&keys));
    }
}
