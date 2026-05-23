//! Dynamic metric value widgets.
//!
//! Metric values can render as plain text, icon/value/unit groups, or the
//! special gradient display with a slope triangle. Icons are loaded from the
//! frontend SVG assets and parsed into a small subset of Skia primitives so the
//! Rust renderer visually matches the editor.
//!
//! Module ownership:
//! - `layout` — text positioning, icon/unit row layout, static icon cache logic.
//! - `gradient` — slope triangle rendering and triangle height math.
//! - `icons` — icon kind mapping, SVG cache, paint creation, primitives drawing.
//! - `svg` — lightweight SVG parser, path tokenizer, and Skia path conversion.

mod gradient;
mod icons;
mod layout;
mod svg;

use crate::activity::schema::DenseActivityReport;
use crate::config::{RenderConfig, ValueConfig};
use crate::render::format::format_metric_parts;
use crate::render::text::ResolvedTextStyle;
use crate::MetricKind;
use skia_safe::Canvas;
use std::path::PathBuf;

pub use gradient::gradient_triangle_height;
pub use layout::{
    metric_icon_top_from_value_layout, metric_vertical_metrics_text, NUMERIC_VERTICAL_METRICS_TEXT,
};

pub(crate) use layout::{
    draw_metric_parts, draw_static_metric_icon_for_value, has_static_metric_icon,
};

/// Bundled parameters for drawing a metric value widget.
pub(crate) struct MetricWidgetRequest<'a> {
    pub canvas: &'a Canvas,
    pub config: &'a RenderConfig,
    pub value: &'a ValueConfig,
    pub base_style: &'a ResolvedTextStyle,
    pub dense_activity: &'a DenseActivityReport,
    pub frame_index: usize,
    pub scale: f32,
    pub font_dirs: &'a [PathBuf],
    pub static_icon_rendered: bool,
}

/// Draws a configured metric widget and reports whether it handled the value.
///
/// Returns false for unsupported values so callers can fall back to generic
/// formatted text drawing.
pub(crate) fn draw_metric_value_widget_with_config(request: MetricWidgetRequest<'_>) -> bool {
    if request.value.value == MetricKind::Gradient {
        return gradient::draw_gradient_value_widget(
            request.canvas,
            request.config,
            request.value,
            request.base_style,
            request.dense_activity,
            request.frame_index,
            request.scale,
            request.font_dirs,
        );
    }

    let Some(parts) = format_metric_parts(
        request.config,
        request.value,
        request.dense_activity,
        request.frame_index,
    ) else {
        return false;
    };
    draw_metric_parts(
        request.canvas,
        request.value,
        request.base_style,
        &parts,
        request.scale,
        request.font_dirs,
        request.static_icon_rendered,
    );
    true
}
