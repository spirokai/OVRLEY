//! Port of the scene-timing part of `store/store-utils.js`: `syncSceneTimingFromConfig` and
//! `syncSceneTimingToConfig`. Render progress and `updateConfigPersistence` arrive with Phase 7.

use super::slices::editor::EditorSlice;
use ovrley_widgets::model::document::{number, OverlayConfig};
use serde_json::Value;
use std::rc::Rc;

fn scene_second(config: Option<&OverlayConfig>, key: &str) -> Option<f64> {
    config.and_then(|config| config.scene.get(key)).and_then(Value::as_f64)
}

/// `syncSceneTimingFromConfig`: a config replacement or a template hydration moves the timeline to
/// the scene's timing. Hydration (`reset_selected_second`) puts the playhead at the scene start; an
/// ordinary replacement moves it only when the start changed and the playhead was anchored to the
/// old start or the timeline was untouched.
pub fn sync_scene_timing_from_config(editor: &mut EditorSlice, previous: Option<&OverlayConfig>, reset_selected_second: bool) {
    let next = Rc::clone(&editor.config);
    let start = next.scene.get("start");
    let end = next.scene.get("end");
    let previous_start = previous.and_then(|config| config.scene.get("start"));
    let previous_end = previous.and_then(|config| config.scene.get("end"));
    let (old_start, old_end, old_selected) = (editor.start_second, editor.end_second, editor.selected_second);
    let untouched = old_start == 0.0 && old_end == editor.fallback_duration_seconds && old_selected == 0.0;
    let start_changed = start != previous_start;
    let end_changed = end != previous_end;
    if let Some(start) = scene_second(Some(&next), "start") {
        if reset_selected_second || start_changed {
            editor.start_second = start;
            let anchored = old_selected == old_start;
            if reset_selected_second || (start_changed && (untouched || anchored)) {
                editor.selected_second = start;
            }
        }
    }
    if let Some(end) = end.and_then(Value::as_f64) {
        if reset_selected_second || end_changed {
            editor.end_second = end;
        }
    }
}

/// `syncSceneTimingToConfig`: timeline edits write the changed bounds into `config.scene`.
pub fn sync_scene_timing_to_config(editor: &mut EditorSlice, start_second: Option<f64>, end_second: Option<f64>) {
    for (key, second) in [("start", start_second), ("end", end_second)] {
        let Some(second) = second else { continue };
        if scene_second(Some(&editor.config), key) != Some(second) {
            Rc::make_mut(&mut editor.config).scene.insert(key.into(), number(second));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn with_scene(scene: Value) -> OverlayConfig {
        OverlayConfig::from_value(&json!({ "scene": scene })).unwrap()
    }

    #[test]
    fn hydration_resets_the_playhead_to_the_scene_start() {
        let mut editor = EditorSlice::default();
        editor.selected_second = 40.0;
        editor.config = Rc::new(with_scene(json!({"start": 5, "end": 90})));
        sync_scene_timing_from_config(&mut editor, None, true);
        assert_eq!((editor.start_second, editor.end_second, editor.selected_second), (5.0, 90.0, 5.0));
    }

    #[test]
    fn a_replacement_moves_the_playhead_only_when_anchored_to_the_old_start() {
        let mut editor = EditorSlice::default();
        let previous = with_scene(json!({"start": 0, "end": 73}));
        editor.config = Rc::new(previous.clone());
        editor.start_second = 0.0;
        editor.selected_second = 12.0;
        editor.end_second = 60.0;
        editor.config = Rc::new(with_scene(json!({"start": 4, "end": 73})));
        sync_scene_timing_from_config(&mut editor, Some(&previous), false);
        assert_eq!(editor.start_second, 4.0);
        assert_eq!(editor.selected_second, 12.0, "the playhead was not at the old start");
        assert_eq!(editor.end_second, 60.0, "the end did not change in the config");

        let previous = (*editor.config).clone();
        editor.selected_second = 4.0;
        editor.config = Rc::new(with_scene(json!({"start": 6, "end": 73})));
        sync_scene_timing_from_config(&mut editor, Some(&previous), false);
        assert_eq!(editor.selected_second, 6.0);
    }

    #[test]
    fn an_untouched_timeline_follows_a_new_start() {
        let mut editor = EditorSlice::default();
        let previous = (*editor.config).clone();
        editor.selected_second = 0.0;
        editor.config = Rc::new(with_scene(json!({"start": 3})));
        sync_scene_timing_from_config(&mut editor, Some(&previous), false);
        assert_eq!((editor.start_second, editor.selected_second), (3.0, 3.0));
    }

    #[test]
    fn timeline_edits_write_back_into_the_scene() {
        let mut editor = EditorSlice::default();
        let before = Rc::clone(&editor.config);
        sync_scene_timing_to_config(&mut editor, Some(2.0), Some(30.5));
        assert_eq!(editor.config.scene["start"], json!(2));
        assert_eq!(editor.config.scene["end"], json!(30.5));
        assert!(!Rc::ptr_eq(&before, &editor.config));
        let after = Rc::clone(&editor.config);
        sync_scene_timing_to_config(&mut editor, Some(2.0), None);
        assert!(Rc::ptr_eq(&after, &editor.config), "an unchanged bound keeps the config");
    }
}
