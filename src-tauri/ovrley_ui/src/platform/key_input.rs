//! Turns Slint key events into the matcher's input (`getEventKey`, plan D7).
//!
//! - The key is the lower-cased text, with Slint's private-use characters for named keys mapped to
//!   the manifest's names, and `' '` as `space`.
//! - Modifiers are the browser's: on macOS Slint reports ⌘ as `control` and the Control key as
//!   `meta`, so they are swapped back to `metaKey` and `ctrlKey`. Other platforms map directly.

use ovrley_app::lib::keyboard_shortcuts::KeyInput;
use slint::platform::Key;

/// Slint's modifier flags, as a `KeyEvent` carries them.
#[derive(Clone, Copy, Debug, Default)]
pub struct SlintModifiers {
    pub alt: bool,
    pub control: bool,
    pub shift: bool,
    pub meta: bool,
}

fn named_key(text: &str) -> Option<&'static str> {
    let named: [(Key, &str); 10] = [
        (Key::Escape, "escape"),
        (Key::LeftArrow, "arrowleft"),
        (Key::RightArrow, "arrowright"),
        (Key::UpArrow, "arrowup"),
        (Key::DownArrow, "arrowdown"),
        (Key::Backspace, "backspace"),
        (Key::Delete, "delete"),
        (Key::Home, "home"),
        (Key::End, "end"),
        (Key::Space, "space"),
    ];
    named.into_iter().find(|(key, _)| slint::SharedString::from(*key) == text).map(|(_, name)| name)
}

/// The matcher's input for a key press. `macos` selects the modifier swap.
pub fn key_input(text: &str, modifiers: SlintModifiers, repeat: bool, macos: bool) -> KeyInput {
    let key = named_key(text).map_or_else(|| text.to_lowercase(), str::to_string);
    let (ctrl, meta) = if macos { (modifiers.meta, modifiers.control) } else { (modifiers.control, modifiers.meta) };
    KeyInput {
        key,
        ctrl,
        meta,
        shift: modifiers.shift,
        alt: modifiers.alt,
        repeat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_keys_take_the_manifest_names() {
        let none = SlintModifiers::default();
        assert_eq!(key_input(&slint::SharedString::from(Key::Escape), none, false, false).key, "escape");
        assert_eq!(key_input(&slint::SharedString::from(Key::LeftArrow), none, false, false).key, "arrowleft");
        assert_eq!(key_input(" ", none, false, false).key, "space");
        assert_eq!(key_input("N", none, false, false).key, "n");
    }

    #[test]
    fn command_is_meta_on_macos_and_control_is_ctrl() {
        let command = SlintModifiers {
            control: true,
            ..Default::default()
        };
        let mac = key_input("n", command, false, true);
        assert!(mac.meta && !mac.ctrl);
        let windows = key_input("n", command, false, false);
        assert!(windows.ctrl && !windows.meta);
    }
}
