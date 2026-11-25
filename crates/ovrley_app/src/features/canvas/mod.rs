//! The canvas: the stage's viewport, the widget drafts, selection and transform, the editor
//! keyboard scope, the background, the grid and the wheel.

pub mod drafts;
pub mod geometry;
pub mod interaction;
pub mod keyboard;
pub mod viewport;

use serde_json::Value;
use viewport::{next_wheel_zoom, EditorViewport, StageLayout, ViewportMetrics};
use crate::shortcuts::{match_keyboard_shortcut, KeyInput, Scope};
use crate::store::Store;
use std::fmt::Write;

/// A widget's data with its collection, as the editor's clipboard holds it.
pub type PreviewWidgetData = (ovrley_widgets::model::document::Collection, ovrley_widgets::model::document::WidgetData);

/// The stage's background colour per mode. The image draws over its colour; the video draws over
/// black.
pub fn canvas_background_color(mode: crate::features::shell::EditorBackgroundMode) -> &'static str {
    use crate::features::shell::EditorBackgroundMode;
    match mode {
        EditorBackgroundMode::Checker | EditorBackgroundMode::Black | EditorBackgroundMode::Image | EditorBackgroundMode::Video => "#000000",
        EditorBackgroundMode::White => "#f4ead2",
    }
}

pub const EDITOR_GRID_DIVISIONS: f64 = 72.0;

/// Grid spacing in scene pixels.
pub fn editor_grid_size(scene: (f64, f64)) -> f64 {
    (scene.0.min(scene.1) / EDITOR_GRID_DIVISIONS).round().max(1.0)
}

/// Grid lines as path commands in display pixels, on pixel centres and inside the scene.
pub fn grid_commands(scene: (f64, f64), display_scale: f64) -> String {
    let width = scene.0 * display_scale;
    let height = scene.1 * display_scale;
    let grid = editor_grid_size(scene);
    let mut commands = String::new();
    let line = |value: f64, extent: f64| (value * display_scale).round().min((extent.round() - 0.5).max(0.5) - 0.5) + 0.5;
    let mut x = 0.0;
    while x <= scene.0 {
        let display_x = line(x, width);
        let _ = write!(commands, "M {display_x} 0 L {display_x} {height} ");
        x += grid;
    }
    let mut y = 0.0;
    while y <= scene.1 {
        let display_y = line(y, height);
        let _ = write!(commands, "M 0 {display_y} L {width} {display_y} ");
        y += grid;
    }
    commands.trim_end().to_string()
}

/// The canvas state outside the document: the viewport, the interaction and the handles.
#[derive(Debug, Default)]
pub struct CanvasState {
    pub viewport: EditorViewport,
    /// What the pointer is doing: nothing, a marquee or a transform.
    pub pointer: interaction::PointerState,
    /// The widget under the pointer.
    pub hovered_widget_id: Option<String>,
    /// The selection handle under the pointer.
    pub hovered_handle: Option<interaction::Handle>,
    /// Copied widgets with their collections, local to the editor.
    pub clipboard: Vec<PreviewWidgetData>,
}

impl CanvasState {
    /// Whether the preview may reuse a widget's cached pixels at a fractional position: it is
    /// being dragged, or a profile is being resized.
    pub fn is_transient(&self, widget_id: &str, kind: &str) -> bool {
        match &self.pointer {
            interaction::PointerState::Gesture(gesture) => gesture.is_transient(widget_id, kind),
            _ => false,
        }
    }
}

/// The scene's width and height. A document always has them: the default scene
/// and every normalized template carry both.
pub fn scene_size(store: &Store) -> (f64, f64) {
    let scene = &store.editor.config.scene;
    let dimension = |key: &str| scene.get(key).and_then(Value::as_f64).unwrap_or_else(|| panic!("the scene has a numeric {key}"));
    (dimension("width"), dimension("height"))
}

/// What the viewport fits and scrolls: the scene, or in Sync Doctor the two screens side by side
/// with their gap.
pub fn viewport_content_size(store: &Store) -> (f64, f64) {
    let (width, height) = scene_size(store);
    if crate::features::player::video_sync_mode(store) {
        (width * 2.0 + crate::features::sync::PREVIEW_SCREEN_GAP, height)
    } else {
        (width, height)
    }
}

/// The display scale: the fit times the zoom.
pub fn display_scale(store: &Store) -> f64 {
    store.canvas.viewport.display_scale(viewport_content_size(store), store.editor_shell.editor_zoom_level)
}

/// The stage the scroll area shows: the scene's, or in Sync Doctor the pair's.
pub fn stage(store: &Store) -> StageLayout {
    let (width, height) = viewport_content_size(store);
    viewport::stage_layout(&store.canvas.viewport.metrics, width, height, display_scale(store))
}

/// Widgets rasterize at the device density the zoomed scene is shown at, never above native
/// export resolution. Before the editor is measured, the scale is 1.
pub fn raster_scale(store: &Store) -> f64 {
    let metrics = &store.canvas.viewport.metrics;
    if metrics.editor_width <= 0.0 || metrics.scale_factor <= 0.0 {
        return 1.0;
    }
    (display_scale(store) * metrics.scale_factor).min(1.0)
}

/// The UI measured the editor.
pub fn set_viewport_metrics(store: &mut Store, metrics: ViewportMetrics) {
    let scene = viewport_content_size(store);
    let zoom = store.editor_shell.editor_zoom_level;
    store.canvas.viewport.set_metrics(metrics, scene, zoom);
}

/// Sets the zoom (the toolbar, the shortcuts and the wheel all come here), keeping the scene point
/// under `cursor` fixed when there is one.
pub fn set_zoom(store: &mut Store, zoom: f64, cursor: Option<(f64, f64)>) {
    let from = store.editor_shell.editor_zoom_level;
    store.editor_shell.set_editor_zoom_level(zoom);
    let to = store.editor_shell.editor_zoom_level;
    let scene = viewport_content_size(store);
    store.canvas.viewport.rezoom(scene, from, to, cursor);
}

/// One wheel event over the stage: deltas in pixels (lines already converted), the modifiers, and
/// the cursor in scroll-area coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelInput {
    pub delta_x: f64,
    pub delta_y: f64,
    pub cursor: (f64, f64),
}

/// The wheel through the keyboard matcher's `wheel` key: `mod`+wheel zooms at the cursor,
/// Shift+wheel scrolls horizontally, and a plain wheel scrolls vertically.
pub fn handle_wheel(store: &mut Store, input: &KeyInput, wheel: WheelInput) {
    let scene = viewport_content_size(store);
    let zoom = store.editor_shell.editor_zoom_level;
    match match_keyboard_shortcut(input, Scope::Editor).map(|found| found.command.id.as_str()) {
        Some("editor.wheelZoom") => {
            if wheel.delta_y == 0.0 {
                return;
            }
            set_zoom(store, next_wheel_zoom(zoom, wheel.delta_y), Some(wheel.cursor));
        }
        Some("editor.wheelScroll") => {
            let delta = if wheel.delta_x == 0.0 { wheel.delta_y } else { wheel.delta_x };
            store.canvas.viewport.scroll_by(delta, 0.0, scene, zoom);
        }
        _ => store.canvas.viewport.scroll_by(wheel.delta_x, wheel.delta_y, scene, zoom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured() -> Store {
        let mut store = Store::default();
        set_viewport_metrics(
            &mut store,
            ViewportMetrics {
                editor_width: 1992.0,
                editor_height: 1152.0,
                scroll_width: 1992.0,
                scroll_height: 1104.0,
                padding: 16.0,
                scale_factor: 2.0,
                rem: 16.0,
            },
        );
        store
    }

    #[test]
    fn the_default_scene_fits_and_rasterizes_at_device_density() {
        let store = measured();
        assert_eq!(scene_size(&store), (3840.0, 2160.0));
        assert_eq!(display_scale(&store), 0.5);
        assert_eq!(raster_scale(&store), 1.0);
        assert_eq!(editor_grid_size((3840.0, 2160.0)), 30.0);
    }

    #[test]
    fn grid_lines_sit_on_pixel_centres_inside_the_scene() {
        let commands = grid_commands((90.0, 72.0), 1.0);
        assert!(commands.starts_with("M 0.5 0 L 0.5 72 "));
        assert!(commands.contains("M 89.5 0 L 89.5 72"), "the last line stays inside: {commands}");
        assert!(commands.contains("M 0 71.5 L 90 71.5"));
    }

    #[test]
    fn mod_wheel_zooms_shift_wheel_scrolls_sideways_and_a_plain_wheel_scrolls() {
        let mut store = measured();
        let mut zoom = KeyInput::new("wheel");
        zoom.ctrl = !cfg!(target_os = "macos");
        zoom.meta = cfg!(target_os = "macos");
        handle_wheel(&mut store, &zoom, WheelInput { delta_x: 0.0, delta_y: -10.0, cursor: (900.0, 500.0) });
        assert_eq!(store.editor_shell.editor_zoom_level, 1.05);
        let mut sideways = KeyInput::new("wheel");
        sideways.shift = true;
        let before = store.canvas.viewport;
        handle_wheel(&mut store, &sideways, WheelInput { delta_x: 0.0, delta_y: 30.0, cursor: (0.0, 0.0) });
        assert_eq!(store.canvas.viewport.scroll_x, before.scroll_x + 30.0);
        assert_eq!(store.canvas.viewport.scroll_y, before.scroll_y);
        handle_wheel(&mut store, &KeyInput::new("wheel"), WheelInput { delta_x: 0.0, delta_y: 20.0, cursor: (0.0, 0.0) });
        assert_eq!(store.canvas.viewport.scroll_y, before.scroll_y + 20.0);
    }
}
