//! Port of `useEditorViewport` (D11): the fit and display scale, the scroll position, the wheel
//! (zoom anchored at the cursor, horizontal and vertical scroll) and the handle scale.
//!
//! Rust owns the scroll position: the stage's `Flickable` is not interactive, and the projection
//! writes its offsets. So a zoom can keep the scene point under the cursor fixed in the same
//! action, as `applyZoomAnchor` does after React's layout.

use crate::features::app_shell::{MAX_EDITOR_ZOOM, MIN_EDITOR_ZOOM};
use crate::lib::utils::clamp;

/// `VIEWPORT_PADDING`: the fit leaves this much room around the scene.
pub const VIEWPORT_PADDING: f64 = 72.0;
/// `MOVEABLE_FULL_SIZE_VIEWPORT_WIDTH` and `MOVEABLE_MIN_HANDLE_SCALE`.
const FULL_SIZE_VIEWPORT_WIDTH: f64 = 1600.0;
const MIN_HANDLE_SCALE: f64 = 0.5;
/// `WHEEL_LINE_PIXELS`.
pub const WHEEL_LINE_PIXELS: f64 = 16.0;

/// What the UI measured, in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewportMetrics {
    /// The editor area (`viewportRef`), which the fit uses.
    pub editor_width: f64,
    pub editor_height: f64,
    /// The scroll area below the toolbar row (`scrollViewportRef`).
    pub scroll_width: f64,
    pub scroll_height: f64,
    /// The stage's padding (`p-4`) on each side.
    pub padding: f64,
    /// Device pixels per logical pixel (`devicePixelRatio`).
    pub scale_factor: f64,
    /// The root font size in logical pixels, which the handle sizes follow.
    pub rem: f64,
}

/// The scroll offsets and the measurements they are clamped to.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EditorViewport {
    pub metrics: ViewportMetrics,
    pub scroll_x: f64,
    pub scroll_y: f64,
}

/// `getFitScale`.
pub fn fit_scale(metrics: &ViewportMetrics, content_width: f64, content_height: f64) -> f64 {
    let width = (metrics.editor_width - VIEWPORT_PADDING).max(1.0);
    let height = (metrics.editor_height - VIEWPORT_PADDING).max(1.0);
    (width / content_width)
        .min(height / content_height)
        .min(1.0)
}

/// `getHandleScale`: handles keep full size from a 1,600 px editor and shrink with it, to half.
pub fn handle_scale(metrics: &ViewportMetrics) -> f64 {
    clamp(
        metrics.editor_width / FULL_SIZE_VIEWPORT_WIDTH,
        MIN_HANDLE_SCALE,
        1.0,
    )
}

/// The stage: the scene at the display scale, padded, and at least as large as the scroll area,
/// with the scene centred while it is smaller (`grid min-h-full min-w-full place-items-center`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StageLayout {
    pub content_width: f64,
    pub content_height: f64,
    /// The scene's top-left corner in the stage.
    pub scene_left: f64,
    pub scene_top: f64,
    pub scene_width: f64,
    pub scene_height: f64,
}

pub fn stage_layout(
    metrics: &ViewportMetrics,
    scene_width: f64,
    scene_height: f64,
    display_scale: f64,
) -> StageLayout {
    let width = scene_width * display_scale;
    let height = scene_height * display_scale;
    let content_width = metrics.scroll_width.max(width + metrics.padding * 2.0);
    let content_height = metrics.scroll_height.max(height + metrics.padding * 2.0);
    StageLayout {
        content_width,
        content_height,
        scene_left: (content_width - width) / 2.0,
        scene_top: (content_height - height) / 2.0,
        scene_width: width,
        scene_height: height,
    }
}

impl EditorViewport {
    fn clamp_scroll(&mut self, stage: &StageLayout) {
        self.scroll_x = clamp(
            self.scroll_x,
            0.0,
            (stage.content_width - self.metrics.scroll_width).max(0.0),
        );
        self.scroll_y = clamp(
            self.scroll_y,
            0.0,
            (stage.content_height - self.metrics.scroll_height).max(0.0),
        );
    }

    /// New measurements keep the scroll inside the stage.
    pub fn set_metrics(&mut self, metrics: ViewportMetrics, scene: (f64, f64), zoom: f64) {
        self.metrics = metrics;
        let stage = stage_layout(&metrics, scene.0, scene.1, self.display_scale(scene, zoom));
        self.clamp_scroll(&stage);
    }

    /// The fit scale times the zoom.
    pub fn display_scale(&self, scene: (f64, f64), zoom: f64) -> f64 {
        fit_scale(&self.metrics, scene.0, scene.1) * zoom
    }

    /// Scrolls by a delta in logical pixels.
    pub fn scroll_by(&mut self, dx: f64, dy: f64, scene: (f64, f64), zoom: f64) {
        self.scroll_x += dx;
        self.scroll_y += dy;
        let stage = stage_layout(
            &self.metrics,
            scene.0,
            scene.1,
            self.display_scale(scene, zoom),
        );
        self.clamp_scroll(&stage);
    }

    /// Changes the zoom keeping the scene point under `cursor` (scroll-area coordinates) where it
    /// is (`getZoomAnchor`, `applyZoomAnchor`). Without a cursor the scroll is only clamped.
    pub fn rezoom(
        &mut self,
        scene: (f64, f64),
        from_zoom: f64,
        to_zoom: f64,
        cursor: Option<(f64, f64)>,
    ) {
        let before = stage_layout(
            &self.metrics,
            scene.0,
            scene.1,
            self.display_scale(scene, from_zoom),
        );
        let after = stage_layout(
            &self.metrics,
            scene.0,
            scene.1,
            self.display_scale(scene, to_zoom),
        );
        if let Some((x, y)) = cursor {
            if before.scene_width > 0.0 && before.scene_height > 0.0 {
                let ratio_x = (self.scroll_x + x - before.scene_left) / before.scene_width;
                let ratio_y = (self.scroll_y + y - before.scene_top) / before.scene_height;
                self.scroll_x = after.scene_left + after.scene_width * ratio_x - x;
                self.scroll_y = after.scene_top + after.scene_height * ratio_y - y;
            }
        }
        self.clamp_scroll(&after);
    }
}

/// `getNextZoomLevel`: one step per wheel event, in its direction (`deltaY < 0` zooms in).
pub fn next_wheel_zoom(zoom: f64, delta_y: f64) -> f64 {
    let step = if delta_y < 0.0 { 0.05 } else { -0.05 };
    clamp(
        ((zoom + step) * 100.0).round() / 100.0,
        MIN_EDITOR_ZOOM,
        MAX_EDITOR_ZOOM,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics() -> ViewportMetrics {
        ViewportMetrics {
            editor_width: 1072.0,
            editor_height: 672.0,
            scroll_width: 1072.0,
            scroll_height: 624.0,
            padding: 16.0,
            scale_factor: 2.0,
            rem: 16.0,
        }
    }

    #[test]
    fn the_fit_leaves_padding_and_never_enlarges() {
        assert_eq!(fit_scale(&metrics(), 2000.0, 1000.0), 0.5);
        assert_eq!(fit_scale(&metrics(), 100.0, 100.0), 1.0);
        assert_eq!(handle_scale(&metrics()), 0.67);
    }

    #[test]
    fn a_small_scene_is_centred_and_a_large_one_scrolls() {
        let stage = stage_layout(&metrics(), 1000.0, 500.0, 0.5);
        assert_eq!((stage.content_width, stage.scene_left), (1072.0, 286.0));
        let stage = stage_layout(&metrics(), 1000.0, 500.0, 2.0);
        assert_eq!((stage.content_width, stage.scene_left), (2032.0, 16.0));
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor() {
        let scene = (2000.0, 1000.0);
        let mut viewport = EditorViewport {
            metrics: metrics(),
            ..Default::default()
        };
        let cursor = (700.0, 300.0);
        let before = stage_layout(
            &viewport.metrics,
            scene.0,
            scene.1,
            viewport.display_scale(scene, 2.0),
        );
        viewport.scroll_x = 200.0;
        viewport.scroll_y = 100.0;
        let point = (
            (viewport.scroll_x + cursor.0 - before.scene_left) / before.scene_width,
            (viewport.scroll_y + cursor.1 - before.scene_top) / before.scene_height,
        );
        viewport.rezoom(scene, 2.0, 2.5, Some(cursor));
        let after = stage_layout(
            &viewport.metrics,
            scene.0,
            scene.1,
            viewport.display_scale(scene, 2.5),
        );
        let moved = (
            (viewport.scroll_x + cursor.0 - after.scene_left) / after.scene_width,
            (viewport.scroll_y + cursor.1 - after.scene_top) / after.scene_height,
        );
        assert!((point.0 - moved.0).abs() < 1e-9 && (point.1 - moved.1).abs() < 1e-9);
    }

    #[test]
    fn scrolling_clamps_to_the_stage() {
        let scene = (2000.0, 1000.0);
        let mut viewport = EditorViewport {
            metrics: metrics(),
            ..Default::default()
        };
        viewport.scroll_by(-50.0, 10_000.0, scene, 2.0);
        let stage = stage_layout(
            &viewport.metrics,
            scene.0,
            scene.1,
            viewport.display_scale(scene, 2.0),
        );
        assert_eq!(
            (viewport.scroll_x, viewport.scroll_y),
            (0.0, stage.content_height - 624.0)
        );
    }

    #[test]
    fn wheel_zoom_steps_in_the_wheel_direction() {
        assert_eq!(next_wheel_zoom(1.0, -3.0), 1.05);
        assert_eq!(next_wheel_zoom(1.0, 3.0), 0.95);
        assert_eq!(next_wheel_zoom(4.0, -1.0), 4.0);
    }
}
