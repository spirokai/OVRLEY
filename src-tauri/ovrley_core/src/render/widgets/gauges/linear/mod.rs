//! Linear gauge preparation and per-frame rendering orchestration.
//!
//! Preparation derives the metric range, precomputes normalized frame fills,
//! resolves label overflow, and caches the empty track. Drawing then composes
//! that static image with either a continuous reveal or whole filled bars.
//!
//! Module ownership:
//! - `geometry` — oriented fill rectangles, bar rectangles, and cap frames.
//! - `labels` — min/max measurement, padding, and baseline placement.
//! - `layer` — static track construction and dynamic fill painting.

mod geometry;
mod labels;
mod layer;

pub use geometry::{bar_fill_rect, bordered_bar_fill_rect};

use self::labels::label_padding;
use self::layer::{draw_continuous_fill, draw_segmented_fill, draw_static_layer};
use super::metric::{fill_percentage, metric_range, metric_values};
use crate::activity::schema::DenseActivityReport;
use crate::debug::RenderProfiler;
use crate::error::CoreResult;
use crate::normalize::{ValidatedLinearGaugeWidget, ValidatedSceneConfig};
use crate::render::format::resolve_metric_display_value;
use crate::render::surface::create_surface;
use crate::render::widgets::common::{normalize_shadow_style_validated, static_layer_padding};
use crate::render::widgets::types::{
    LinearGaugeCache, LinearGaugeFrameState, WidgetFrameReport, WidgetGeometryReport,
    WidgetRenderReport,
};
use crate::types::TrackFillStyle;
use skia_safe::Canvas;
use std::path::PathBuf;

/// Prepares the cached static layer and per-frame fill states.
pub fn prepare_linear_gauge_cache(
    gauge: &ValidatedLinearGaugeWidget,
    altitude_offset_m: f64,
    dense_activity: &DenseActivityReport,
    scene: &ValidatedSceneConfig,
    scale: f32,
    font_dirs: &[PathBuf],
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<LinearGaugeCache> {
    prepare_profiler.measure("gauge.linear.prepare", || {
        let scaled_width = ((gauge.width as f32) * scale).round().max(1.0) as u32;
        let scaled_height = ((gauge.height as f32) * scale).round().max(1.0) as u32;
        let (min_value, max_value) = metric_range(&dense_activity.series, gauge.metric);
        let shadow = normalize_shadow_style_validated(
            &scene.shadow_color,
            scene.shadow_strength,
            scene.shadow_distance,
            scale,
        );
        let track_padding =
            static_layer_padding(gauge.track_border_thickness * scale, shadow.as_ref());
        let (label_left, label_top, label_right, label_bottom) = label_padding(
            gauge,
            scaled_width,
            scaled_height,
            scale,
            font_dirs,
            min_value,
            max_value,
            altitude_offset_m,
        )?;
        let left_padding = track_padding.max(label_left);
        let top_padding = track_padding.max(label_top);
        let right_padding = track_padding.max(label_right);
        let bottom_padding = track_padding.max(label_bottom);
        let layer_width = scaled_width
            .saturating_add(left_padding)
            .saturating_add(right_padding)
            .max(1);
        let layer_height = scaled_height
            .saturating_add(top_padding)
            .saturating_add(bottom_padding)
            .max(1);
        let frame_states = metric_values(&dense_activity.series, gauge.metric)
            .iter()
            .map(|value| {
                let value = resolve_metric_display_value(gauge.metric, *value, dense_activity)
                    .unwrap_or(min_value);
                LinearGaugeFrameState {
                    fill01: fill_percentage(value, min_value, max_value),
                }
            })
            .collect::<Vec<_>>();

        let mut surface = create_surface(layer_width, layer_height)?;
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::TRANSPARENT);
        canvas.translate((left_padding as f32, top_padding as f32));
        draw_static_layer(
            canvas,
            gauge,
            scene,
            scaled_width,
            scaled_height,
            scale,
            font_dirs,
            min_value,
            max_value,
            altitude_offset_m,
        )?;

        Ok(LinearGaugeCache {
            static_image: surface.image_snapshot(),
            static_image_x: gauge.x - left_padding as f32,
            static_image_y: gauge.y - top_padding as f32,
            x: gauge.x,
            y: gauge.y,
            width: scaled_width,
            height: scaled_height,
            rotation: gauge.rotation,
            orientation: gauge.orientation,
            track_corner_radius: gauge.track_corner_radius * scale,
            track_border_thickness: gauge.track_border_thickness * scale,
            track_filled_color: gauge.track_filled_color.clone(),
            track_filled_opacity: gauge.track_filled_opacity,
            track_fill_flat: gauge.track_fill_flat,
            track_fill_style: gauge.track_fill_style,
            bar_geometry: crate::normalize::scale_bar_geometry(gauge.bar_geometry, scale),
            frame_states,
        })
    })
}

/// Draws the cached static image and current dynamic fill.
pub fn draw_linear_gauge_widget(
    canvas: &Canvas,
    cache: &LinearGaugeCache,
    frame_index: usize,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    frame_profiler.measure("gauge.linear.draw", || {
        canvas.draw_image(
            &cache.static_image,
            (cache.static_image_x, cache.static_image_y),
            None,
        );

        let state = cache
            .frame_states
            .get(frame_index)
            .or_else(|| cache.frame_states.last())?;
        match cache.track_fill_style {
            TrackFillStyle::Bars => draw_segmented_fill(canvas, cache, state.fill01),
            TrackFillStyle::Fill => draw_continuous_fill(canvas, cache, state.fill01),
        }

        Some(WidgetRenderReport {
            geometry: WidgetGeometryReport {
                point_count: 0,
                source_point_count: 0,
                simplification: "linear_gauge".to_string(),
                bbox: [cache.x, cache.y, cache.width as f32, cache.height as f32],
                widget_width: cache.width,
                widget_height: cache.height,
                rotation_deg: cache.rotation,
            },
            frame: WidgetFrameReport {
                progress01: state.fill01,
                marker_x: cache.width as f32 * state.fill01,
                marker_y: cache.height as f32 * (1.0 - state.fill01),
                marker_abs_x: cache.x + cache.width as f32 * state.fill01,
                marker_abs_y: cache.y + cache.height as f32 * (1.0 - state.fill01),
            },
        })
    })
}
