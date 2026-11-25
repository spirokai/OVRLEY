//! The editor scope's keyboard commands, and the history scope. Keys reach these only when no
//! focused control used them.

use ovrley_widgets::model::document::number;
use serde_json::{Map, Value};
use crate::features::canvas::{set_zoom, PreviewWidgetData};
use crate::features::canvas::interaction::cancel_gesture;
use crate::features::history::{history_enabled, redo, undo};
use crate::features::shell::KeyboardWorkspace;
use crate::shortcuts::{match_keyboard_shortcut, KeyInput, Scope};
use crate::store::Store;

/// Overlays block the editor's, the player's and the history's keys: the
/// settings dialog, the unpinned drawer and the app's dialogs. Popups take the keys themselves.
pub(crate) fn overlay_open(store: &Store) -> bool {
    store.editor_shell.settings_open || store.layout.is_unpinned_overlay() || store.unsaved_confirm.is_open() || store.video_import.cut_removal_request.is_some() || store.app_update.is_open()
}

/// A repeated key or one with Alt is ignored, as are keys while overlays are
/// open or history is disabled.
pub fn handle_history_key(store: &mut Store, input: &KeyInput) -> bool {
    if input.repeat || input.alt || overlay_open(store) || !history_enabled(store) {
        return false;
    }
    match match_keyboard_shortcut(input, Scope::History).map(|found| found.command.id.as_str()) {
        Some("history.undo") if store.history.can_undo() => undo(store),
        Some("history.redo") if store.history.can_redo() => redo(store),
        _ => false,
    }
}

/// The editor scope. Returns whether the key was used.
pub fn handle_editor_key(store: &mut Store, input: &KeyInput) -> bool {
    let Some(found) = match_keyboard_shortcut(input, Scope::Editor) else { return false };
    let selected = store.editor.selected_widget_ids.clone();
    match found.command.id.as_str() {
        "editor.clearSelection" => {
            if overlay_open(store) {
                return false;
            }
            store.editor.set_widget_selection(&[], None);
            true
        }
        "editor.delete" => {
            if selected.is_empty() {
                return false;
            }
            store.editor.edit_config(|config| config.delete_widgets(&selected));
            store.editor.set_widget_selection(&[], None);
            true
        }
        "editor.copy" => {
            let widgets = selected_preview_data(store);
            if widgets.is_empty() {
                return false;
            }
            store.canvas.clipboard = widgets;
            true
        }
        "editor.paste" => {
            if store.canvas.clipboard.is_empty() {
                return false;
            }
            let clipboard = store.canvas.clipboard.clone();
            let inserted = store.editor.edit_config(|config| config.duplicate_widgets(&clipboard, (24.0, 24.0)));
            let primary = inserted.last().cloned();
            store.editor.set_widget_selection(&inserted, primary.as_deref());
            true
        }
        "editor.nudge" => {
            if store.editor_shell.active_keyboard_workspace != KeyboardWorkspace::Editor || selected.is_empty() {
                return false;
            }
            let step = found.binding.step.unwrap_or(1.0);
            let delta = match found.binding.key.as_str() {
                "arrowleft" => (-step, 0.0),
                "arrowright" => (step, 0.0),
                "arrowup" => (0.0, -step),
                _ => (0.0, step),
            };
            let updates: Vec<(String, Map<String, Value>)> = selected_preview_data(store)
                .into_iter()
                .filter_map(|(_, data)| {
                    let id = data.get("id")?.as_str()?.to_string();
                    let coordinate = |key: &str| data.get(key).and_then(Value::as_f64).unwrap_or_default();
                    let mut patch = Map::new();
                    patch.insert("x".into(), number(coordinate("x") + delta.0));
                    patch.insert("y".into(), number(coordinate("y") + delta.1));
                    Some((id, patch))
                })
                .collect();
            store.editor.edit_config(|config| config.update_widgets(&updates)).is_ok()
        }
        "editor.toggleSnap" => {
            let snap = !store.editor_shell.editor_snap_to_grid;
            store.editor_shell.set_editor_snap_to_grid(snap);
            true
        }
        "editor.toggleGrid" => {
            let grid = !store.editor_shell.editor_grid_visible;
            store.editor_shell.set_editor_grid_visible(grid);
            true
        }
        "editor.zoomIn" | "editor.zoomOut" | "editor.resetZoom" => {
            let mut shell = store.editor_shell;
            match found.command.id.as_str() {
                "editor.zoomIn" => shell.increase_zoom(),
                "editor.zoomOut" => shell.decrease_zoom(),
                _ => shell.reset_editor_zoom(),
            }
            set_zoom(store, shell.editor_zoom_level, None);
            true
        }
        _ => false,
    }
}

/// The selected widgets' preview data with their collections, drafts and global defaults
/// resolved.
fn selected_preview_data(store: &Store) -> Vec<PreviewWidgetData> {
    let Some(theme) = store.theme.clone() else { return Vec::new() };
    let widgets = crate::features::widgets::preview::preview_widgets(store, &theme);
    let collections: Vec<_> = store.editor.config.widgets().map(|(collection, data)| (collection, ovrley_widgets::model::document::widget_id(data).to_string())).collect();
    store
        .editor
        .selected_widget_ids
        .iter()
        .filter_map(|id| {
            let widget = widgets.iter().find(|widget| &widget.id == id)?;
            let collection = collections.iter().find(|(_, candidate)| candidate == id)?.0;
            let mut data = widget.data.clone();
            data.insert("id".into(), Value::String(id.clone()));
            Some((collection, data))
        })
        .collect()
}

/// The capture-phase Escape: a running gesture is cancelled before anything else sees the key.
pub fn handle_capture_key(store: &mut Store, input: &KeyInput) -> bool {
    if input.key == "escape" && cancel_gesture(store) {
        return true;
    }
    // A map's view mode leaves on Escape wherever focus is.
    matches!(match_keyboard_shortcut(input, Scope::Editor), Some(found) if found.command.id == "editor.clearSelection") && crate::features::maps::escape(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::template::ThemeColors;
    use ovrley_widgets::model::document::OverlayConfig;
    use serde_json::json;

    fn store() -> Store {
        let mut store = Store { theme: Some(ThemeColors { ice: "#86aaaa".into(), aqua: "#40e0d0".into(), teal: "#005b5b".into(), accent: "#ad5013".into() }), ..Store::default() };
        store.editor.set_config(OverlayConfig::from_value(&json!({"scene": {"width": 1920, "height": 1080}, "labels": [{"id": "a", "text": "A", "x": 10, "y": 20}, {"id": "b", "text": "B", "x": 0, "y": 0}]})).unwrap());
        store.editor.set_selected_widget_id(Some("a"));
        store
    }

    fn key(name: &str, modify: impl FnOnce(&mut KeyInput)) -> KeyInput {
        let mut input = KeyInput::new(name);
        modify(&mut input);
        input
    }

    fn command(input: &mut KeyInput) {
        if cfg!(target_os = "macos") {
            input.meta = true;
        } else {
            input.ctrl = true;
        }
    }

    #[test]
    fn arrows_nudge_and_shift_arrows_nudge_further() {
        let mut store = store();
        assert!(handle_editor_key(&mut store, &KeyInput::new("arrowright")));
        assert!(handle_editor_key(&mut store, &key("arrowdown", |input| input.shift = true)));
        let widget = store.editor.config.widget("a").unwrap();
        assert_eq!((widget["x"].clone(), widget["y"].clone()), (json!(11), json!(30)));
    }

    #[test]
    fn delete_removes_the_selection_and_escape_clears_it() {
        let mut store = store();
        assert!(handle_editor_key(&mut store, &KeyInput::new("delete")));
        assert_eq!(store.editor.config.widget_ids(), ["b"]);
        assert!(store.editor.selected_widget_ids.is_empty());
        assert!(!handle_editor_key(&mut store, &KeyInput::new("delete")), "nothing is selected");
        store.editor.set_selected_widget_id(Some("b"));
        assert!(handle_editor_key(&mut store, &KeyInput::new("escape")));
        assert!(store.editor.selected_widget_ids.is_empty());
    }

    #[test]
    fn copy_then_paste_duplicates_offset_and_selects_the_copies() {
        let mut store = store();
        assert!(handle_editor_key(&mut store, &key("c", command)));
        assert!(handle_editor_key(&mut store, &key("v", command)));
        let pasted = store.editor.selected_widget_ids.clone();
        assert_eq!(pasted.len(), 1);
        let widget = store.editor.config.widget(&pasted[0]).unwrap();
        assert_eq!((widget["x"].clone(), widget["y"].clone(), widget["text"].clone()), (json!(34), json!(44), json!("A")));
    }

    #[test]
    fn toggles_and_zoom_keys() {
        let mut store = store();
        assert!(handle_editor_key(&mut store, &KeyInput::new("g")));
        assert!(handle_editor_key(&mut store, &KeyInput::new("n")));
        assert!(store.editor_shell.editor_grid_visible && store.editor_shell.editor_snap_to_grid);
        assert!(handle_editor_key(&mut store, &key("=", command)));
        assert_eq!(store.editor_shell.editor_zoom_level, 1.05);
        assert!(handle_editor_key(&mut store, &key("0", command)));
        assert_eq!(store.editor_shell.editor_zoom_level, 1.0);
    }

    #[test]
    fn history_keys_undo_and_redo_but_not_with_alt_or_overlays() {
        let mut store = store();
        store.history.past.push(crate::features::history::UndoableState::of(&Store::default()));
        assert!(!handle_history_key(&mut store, &key("z", |input| {
            command(input);
            input.alt = true;
        })));
        store.editor_shell.open_settings();
        assert!(!handle_history_key(&mut store, &key("z", command)));
        store.editor_shell.close_settings();
        assert!(handle_history_key(&mut store, &key("z", command)));
        assert!(store.history.can_redo());
        assert!(handle_history_key(&mut store, &key("z", |input| {
            command(input);
            input.shift = true;
        })));
    }
}
