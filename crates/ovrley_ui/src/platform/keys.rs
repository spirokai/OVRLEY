//! Slint's key events and modifiers as the shortcut matcher's input.
//!
//! - The key is the lower-cased text, with Slint's private-use characters for named keys mapped to
//!   the manifest's names, and `' '` as `space`.
//! - On macOS Slint reports ⌘ as `control` and the Control key as `meta`. The matcher's `meta` is
//!   ⌘ and its `ctrl` the Control key on every platform, so macOS swaps them back.

use ovrley_app::features::canvas::interaction::PointerModifiers;
use ovrley_app::shortcuts::KeyInput;
use slint::platform::Key;

/// Slint's modifier flags, as its key and pointer events carry them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub alt: bool,
    pub control: bool,
    pub shift: bool,
    pub meta: bool,
}

/// Which physical key Slint reports as `control`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layout {
    /// ⌘ is `control`.
    Mac,
    /// The Control key is `control`.
    Pc,
}

const LAYOUT: Layout = if cfg!(target_os = "macos") { Layout::Mac } else { Layout::Pc };

impl Modifiers {
    /// The matcher's `(ctrl, meta)`.
    fn ctrl_meta(self, layout: Layout) -> (bool, bool) {
        match layout {
            Layout::Mac => (self.meta, self.control),
            Layout::Pc => (self.control, self.meta),
        }
    }

    /// A key press, not repeated.
    pub fn key_input(self, text: &str) -> KeyInput {
        self.key_input_on(text, LAYOUT)
    }

    fn key_input_on(self, text: &str, layout: Layout) -> KeyInput {
        let (ctrl, meta) = self.ctrl_meta(layout);
        KeyInput { key: key_name(text), ctrl, meta, shift: self.shift, alt: self.alt, repeat: false }
    }

    /// The modifiers of a pointer event.
    pub fn pointer(self) -> PointerModifiers {
        let (ctrl, meta) = self.ctrl_meta(LAYOUT);
        PointerModifiers { shift: self.shift, ctrl, meta, alt: self.alt }
    }
}

/// The manifest's name of a key Slint reports as `text`.
pub fn key_name(text: &str) -> String {
    const NAMED: [(Key, &str); 11] = [
        (Key::Return, "enter"),
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
    // A field's `accepted` reports Enter as a line feed.
    if text == "\n" {
        return "enter".to_string();
    }
    NAMED
        .into_iter()
        .find(|(key, _)| slint::SharedString::from(*key) == text)
        .map_or_else(|| text.to_lowercase(), |(_, name)| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_keys_take_the_manifest_names() {
        assert_eq!(key_name(&slint::SharedString::from(Key::Escape)), "escape");
        assert_eq!(key_name(&slint::SharedString::from(Key::LeftArrow)), "arrowleft");
        assert_eq!(key_name(" "), "space");
        assert_eq!(key_name("N"), "n");
    }

    #[test]
    fn command_is_meta_on_macos_and_control_is_ctrl() {
        let command = Modifiers { control: true, ..Modifiers::default() };
        let mac = command.key_input_on("n", Layout::Mac);
        assert!(mac.meta && !mac.ctrl);
        let pc = command.key_input_on("n", Layout::Pc);
        assert!(pc.ctrl && !pc.meta);
    }
}
