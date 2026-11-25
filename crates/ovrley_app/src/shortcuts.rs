//! The keyboard shortcut manifest and its matcher.
//!
//! `assets/keyboard-shortcuts.json` is embedded and parsed once, strictly: an unknown scope,
//! modifier or field fails.

use std::sync::OnceLock;
use serde::Deserialize;

/// The component that owns a shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    App,
    Drawer,
    Editor,
    History,
    Player,
    SecondsInput,
    TimelineClip,
    VideoCard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modifier {
    /// Exactly one of Ctrl or ⌘.
    Mod,
    Ctrl,
    Shift,
    Alt,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    /// The lower-cased logical key (`n`, `escape`, `arrowleft`, `space`, `wheel`).
    pub key: String,
    pub modifiers: Vec<Modifier>,
    /// Shown in the shortcuts reference; absent means shown.
    #[serde(default)]
    pub display: Option<bool>,
    /// Listed in the reference but never matched (the plain wheel).
    #[serde(default)]
    pub display_only: bool,
    /// Shown instead of the key and modifiers.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub display_aliases: Vec<String>,
    /// Overrides the command's description in the reference.
    #[serde(default)]
    pub description_key: Option<String>,
    /// Editor nudge distance in pixels.
    #[serde(default)]
    pub step: Option<f64>,
    /// Timeline clip nudge, or seconds-input step, in seconds.
    #[serde(default)]
    pub seconds: Option<f64>,
    /// Seconds-input step in frames of the video.
    #[serde(default)]
    pub frames: Option<i32>,
}

impl Binding {
    pub fn is_displayed(&self) -> bool {
        self.display != Some(false)
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Command {
    pub id: String,
    pub display: bool,
    pub scope: Scope,
    pub description_key: String,
    pub bindings: Vec<Binding>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Category {
    pub name_key: String,
    pub commands: Vec<Command>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub categories: Vec<Category>,
}

/// The manifest, parsed on first use.
pub fn manifest() -> &'static Manifest {
    static MANIFEST: OnceLock<Manifest> = OnceLock::new();
    MANIFEST.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../assets/keyboard-shortcuts.json"
        ))
        .unwrap_or_else(|error| panic!("keyboard-shortcuts.json is malformed: {error}"))
    })
}

/// A key press: the logical key, lower-cased, and the platform's command modifier (`meta` is ⌘
/// on macOS, `ctrl` is Control elsewhere). `ovrley_ui`'s key adapter builds it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyInput {
    pub key: String,
    pub ctrl: bool,
    pub meta: bool,
    pub shift: bool,
    pub alt: bool,
    pub repeat: bool,
}

impl KeyInput {
    /// A key with no modifiers.
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_string(),
            ..Self::default()
        }
    }
}

fn matches_modifiers(input: &KeyInput, modifiers: &[Modifier]) -> bool {
    let uses_mod = modifiers.contains(&Modifier::Mod);
    let uses_ctrl = modifiers.contains(&Modifier::Ctrl);
    if uses_mod && input.ctrl == input.meta {
        return false;
    }
    if !uses_mod && !uses_ctrl && (input.ctrl || input.meta) {
        return false;
    }
    if uses_ctrl && (!input.ctrl || input.meta) {
        return false;
    }
    if !uses_mod && uses_ctrl != input.ctrl {
        return false;
    }
    modifiers.contains(&Modifier::Shift) == input.shift
        && modifiers.contains(&Modifier::Alt) == input.alt
}

/// A matched command and the binding that matched.
#[derive(Clone, Copy, Debug)]
pub struct ShortcutMatch {
    pub command: &'static Command,
    pub binding: &'static Binding,
}

/// The command declared for `input` in `scope`. Modifier matching is exact; `displayOnly`
/// bindings never match.
pub fn match_keyboard_shortcut(input: &KeyInput, scope: Scope) -> Option<ShortcutMatch> {
    manifest()
        .categories
        .iter()
        .flat_map(|category| &category.commands)
        .filter(|command| command.scope == scope)
        .find_map(|command| {
            command
                .bindings
                .iter()
                .find(|binding| {
                    !binding.display_only
                        && binding.key == input.key
                        && matches_modifiers(input, &binding.modifiers)
                })
                .map(|binding| ShortcutMatch { command, binding })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: &str, configure: impl FnOnce(&mut KeyInput)) -> KeyInput {
        let mut input = KeyInput::new(key);
        configure(&mut input);
        input
    }

    fn command(input: &KeyInput, scope: Scope) -> Option<&'static str> {
        match_keyboard_shortcut(input, scope).map(|found| found.command.id.as_str())
    }

    #[test]
    fn matches_ctrl_and_command_as_mod_bindings() {
        assert_eq!(
            command(&key("n", |k| k.ctrl = true), Scope::App),
            Some("project.new")
        );
        assert_eq!(
            command(&key("n", |k| k.meta = true), Scope::App),
            Some("project.new")
        );
    }

    #[test]
    fn matches_project_workflow_and_reassigned_template_bindings() {
        let ctrl = |k: &mut KeyInput| k.ctrl = true;
        let ctrl_shift = |k: &mut KeyInput| {
            k.ctrl = true;
            k.shift = true;
        };
        assert_eq!(command(&key("o", ctrl), Scope::App), Some("project.open"));
        assert_eq!(command(&key("s", ctrl), Scope::App), Some("project.save"));
        assert_eq!(
            command(&key("s", ctrl_shift), Scope::App),
            Some("project.saveAs")
        );
        assert_eq!(
            command(&key("t", ctrl_shift), Scope::App),
            Some("template.new")
        );
        assert_eq!(
            command(&key("l", ctrl_shift), Scope::App),
            Some("template.import")
        );
        assert_eq!(
            command(&key("p", ctrl_shift), Scope::App),
            Some("template.save")
        );
    }

    #[test]
    fn keeps_modifier_matching_exact_for_altgr_like_input() {
        assert_eq!(
            command(&key("k", |k| k.ctrl = true), Scope::App),
            Some("activity.import")
        );
        let alt_gr = key("k", |k| {
            k.ctrl = true;
            k.alt = true;
        });
        assert_eq!(command(&alt_gr, Scope::App), None);
    }

    #[test]
    fn preserves_ctrl_y_redo() {
        assert_eq!(
            command(&key("y", |k| k.ctrl = true), Scope::History),
            Some("history.redo")
        );
    }

    #[test]
    fn matches_both_key_values_for_ctrl_plus() {
        let plus = key("+", |k| {
            k.ctrl = true;
            k.shift = true;
        });
        assert_eq!(command(&plus, Scope::Editor), Some("editor.zoomIn"));
        assert_eq!(
            command(&key("=", |k| k.ctrl = true), Scope::Editor),
            Some("editor.zoomIn")
        );
    }

    #[test]
    fn display_only_bindings_never_match() {
        assert_eq!(command(&KeyInput::new("wheel"), Scope::Editor), None);
        assert_eq!(
            command(&key("wheel", |k| k.shift = true), Scope::Editor),
            Some("editor.wheelScroll")
        );
    }
}
