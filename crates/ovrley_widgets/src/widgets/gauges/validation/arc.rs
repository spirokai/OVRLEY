//! Arc gauge metric widget validation.
//!
//! Arc gauges share metric formatting with text widgets, but render that value
//! inside a circular stroked track. The validator keeps every visual input
//! explicit and rejects out-of-range arc geometry before render preparation.

use crate::error::{WidgetError, WidgetResult};
use crate::model::types::{DisplayType, MetricKind, TrackFillStyle};
use crate::model::validation::{
    require_bool, require_f32, require_hex_color, require_positive_f32, require_string,
};
use crate::render::track_fill::{validate_track_fill_color, TrackFillColor};
use crate::widgets::gauges::bar_geometry::{resolve_arc_track_geometry, ResolvedBarGeometry};
use crate::widgets::value::config::ValueConfig;
use crate::widgets::value::validation::validate_arc_inner_value_widget;
use crate::widgets::value::validation::ValidatedValueWidget;

pub const MIN_ARC_ANGLE_DEGREES: f32 = 30.0;
pub const MAX_ARC_ANGLE_DEGREES: f32 = 360.0;
pub const CORNER_GAUGE_ANGLE_DEGREES: f32 = 90.0;
pub const MIN_GT_GAUGE_TICK_COUNT: u32 = 2;

/// The supported bottom-corner placements for a fixed 90° gauge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidatedCornerGaugeOrientation {
    BottomLeft,
    BottomRight,
}

/// GT gauge decoration drawn around the shared arc track.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedGtGaugeStyle {
    /// Evenly spaced value-less ticks, including both track ends.
    pub tick_count: u32,
    pub tick_length: f32,
    pub tick_color: String,
    /// Fill glow intensity in 0..=1; zero disables the glow.
    pub glow: f32,
}

/// Fully validated arc gauge configuration.
#[derive(Clone, Debug)]
pub struct ValidatedArcGaugeWidget {
    pub metric: MetricKind,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub display_type: DisplayType,
    pub arc_angle: f32,
    pub corner_orientation: Option<ValidatedCornerGaugeOrientation>,
    pub gt_style: Option<ValidatedGtGaugeStyle>,
    pub inner_widget_offset_x: f32,
    pub inner_widget_offset_y: f32,
    pub track_thickness: f32,
    pub track_corner_radius: f32,
    pub track_border_thickness: f32,
    pub track_border_color: String,
    pub track_empty_color: String,
    pub track_empty_opacity: f32,
    pub track_filled_color: TrackFillColor,
    pub track_filled_opacity: f32,
    pub track_fill_flat: bool,
    pub track_fill_style: TrackFillStyle,
    pub bar_geometry: Option<ResolvedBarGeometry>,
    pub show_min_max_labels: bool,
    pub min_max_label_font: String,
    pub min_max_label_font_size: f32,
    pub min_max_label_color: String,
    /// Shared text formatting and styling for the inner value/unit stack.
    /// `show_icon` is always false for this presentation.
    pub inner_value: ValidatedValueWidget,
}

/// Validates a raw value config as an arc gauge.
pub fn validate_arc_gauge(
    value: ValueConfig,
    index: usize,
) -> WidgetResult<ValidatedArcGaugeWidget> {
    let p = |field: &str| format!("values[{index}].{field}");

    if value.display_type != DisplayType::Arc {
        return Err(WidgetError::Config(format!(
            "{}: expected arc display_type, got '{}'",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    let arc_angle = validate_arc_angle(value.arc_angle, &p("arc_angle"))?;
    validate_arc_shaped_gauge(value, index, DisplayType::Arc, arc_angle, None, None)
}

/// Validates a raw value config as a GT gauge: a rotated arc with ticks and fill glow.
pub fn validate_gt_gauge(
    value: ValueConfig,
    index: usize,
) -> WidgetResult<ValidatedArcGaugeWidget> {
    let p = |field: &str| format!("values[{index}].{field}");

    if value.display_type != DisplayType::GtGauge {
        return Err(WidgetError::Config(format!(
            "{}: expected gt_gauge display_type, got '{}'",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    let arc_angle = validate_arc_angle(value.arc_angle, &p("arc_angle"))?;
    let tick_count = value
        .tick_count
        .ok_or_else(|| WidgetError::Config(format!("{}: required", p("tick_count"))))?;
    if tick_count < MIN_GT_GAUGE_TICK_COUNT {
        return Err(WidgetError::Config(format!(
            "{}: must be >= {MIN_GT_GAUGE_TICK_COUNT}, got {tick_count}",
            p("tick_count")
        )));
    }
    let tick_length = require_f32(value.tick_length, &p("tick_length"))?;
    if tick_length < 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be >= 0, got {tick_length}",
            p("tick_length")
        )));
    }
    let glow = require_f32(value.glow, &p("glow"))?;
    if !(0.0..=1.0).contains(&glow) {
        return Err(WidgetError::Config(format!(
            "{}: must be 0.0..1.0, got {glow}",
            p("glow")
        )));
    }
    let gt_style = ValidatedGtGaugeStyle {
        tick_count,
        tick_length,
        tick_color: require_hex_color(value.tick_color.as_deref(), &p("tick_color"))?,
        glow,
    };

    validate_arc_shaped_gauge(
        value,
        index,
        DisplayType::GtGauge,
        arc_angle,
        None,
        Some(gt_style),
    )
}

fn validate_arc_angle(arc_angle: Option<f32>, field: &str) -> WidgetResult<f32> {
    let arc_angle = require_f32(arc_angle, field)?;
    if !(MIN_ARC_ANGLE_DEGREES..=MAX_ARC_ANGLE_DEGREES).contains(&arc_angle) {
        return Err(WidgetError::Config(format!(
            "{field}: must be {}..={}, got {arc_angle}",
            MIN_ARC_ANGLE_DEGREES as u32, MAX_ARC_ANGLE_DEGREES as u32,
        )));
    }
    Ok(arc_angle)
}

/// Validates a raw value config as a fixed 90° bottom-corner gauge.
pub fn validate_corner_gauge(
    value: ValueConfig,
    index: usize,
) -> WidgetResult<ValidatedArcGaugeWidget> {
    let p = |field: &str| format!("values[{index}].{field}");

    if value.display_type != DisplayType::Corner {
        return Err(WidgetError::Config(format!(
            "{}: expected corner display_type, got '{}'",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    let corner_orientation = match require_string(
        value.corner_orientation.clone(),
        &p("corner_orientation"),
    )?
    .as_str()
    {
        "bottom-left" => ValidatedCornerGaugeOrientation::BottomLeft,
        "bottom-right" => ValidatedCornerGaugeOrientation::BottomRight,
        orientation => {
            return Err(WidgetError::Config(format!(
                "{}: must be 'bottom-left' or 'bottom-right', got '{orientation}'",
                p("corner_orientation")
            )));
        }
    };

    validate_arc_shaped_gauge(
        value,
        index,
        DisplayType::Corner,
        CORNER_GAUGE_ANGLE_DEGREES,
        Some(corner_orientation),
        None,
    )
}

fn validate_arc_shaped_gauge(
    value: ValueConfig,
    index: usize,
    display_type: DisplayType,
    arc_angle: f32,
    corner_orientation: Option<ValidatedCornerGaugeOrientation>,
    gt_style: Option<ValidatedGtGaugeStyle>,
) -> WidgetResult<ValidatedArcGaugeWidget> {
    let p = |field: &str| format!("values[{index}].{field}");

    super::validate_supported_metric(value.value, display_type)?;
    require_f32(Some(value.x), &p("x"))?;
    require_f32(Some(value.y), &p("y"))?;
    let width = require_positive_f32(value.width, &p("width"))?;
    let height = require_positive_f32(value.height, &p("height"))?;

    let track_thickness = require_f32(value.track_thickness, &p("track_thickness"))?;
    if track_thickness <= 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be > 0, got {track_thickness}",
            p("track_thickness")
        )));
    }

    let raw_corner_radius = require_f32(value.track_corner_radius, &p("track_corner_radius"))?;
    if raw_corner_radius < 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be >= 0, got {raw_corner_radius}",
            p("track_corner_radius")
        )));
    }

    let track_border_thickness =
        require_f32(value.track_border_thickness, &p("track_border_thickness"))?;
    if track_border_thickness < 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be >= 0, got {track_border_thickness}",
            p("track_border_thickness")
        )));
    }

    let track_empty_opacity = require_f32(value.track_empty_opacity, &p("track_empty_opacity"))?;
    let track_filled_opacity = require_f32(value.track_filled_opacity, &p("track_filled_opacity"))?;
    for (field, opacity) in [
        ("track_empty_opacity", track_empty_opacity),
        ("track_filled_opacity", track_filled_opacity),
    ] {
        if !(0.0..=1.0).contains(&opacity) {
            return Err(WidgetError::Config(format!(
                "{}: must be 0.0..1.0, got {opacity}",
                p(field)
            )));
        }
    }

    let min_max_label_font_size =
        require_f32(value.min_max_label_font_size, &p("min_max_label_font_size"))?;
    if min_max_label_font_size <= 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be > 0, got {min_max_label_font_size}",
            p("min_max_label_font_size")
        )));
    }

    let metric = value.value;
    let x = value.x;
    let y = value.y;
    let rotation = require_f32(Some(value.rotation.unwrap_or(0.0)), &p("rotation"))?;
    let inner_widget_offset_x =
        require_f32(value.inner_widget_offset_x, &p("inner_widget_offset_x"))?;
    let inner_widget_offset_y =
        require_f32(value.inner_widget_offset_y, &p("inner_widget_offset_y"))?;
    let track_border_color = require_hex_color(
        value.track_border_color.as_deref(),
        &p("track_border_color"),
    )?;
    let track_empty_color =
        require_hex_color(value.track_empty_color.as_deref(), &p("track_empty_color"))?;
    // GT gauges keep a solid fill; their glow is tinted with that single color.
    let track_filled_color = if gt_style.is_some() {
        TrackFillColor::Solid(require_hex_color(
            value.track_filled_color.as_deref(),
            &p("track_filled_color"),
        )?)
    } else {
        validate_track_fill_color(
            value.track_filled_color.as_deref(),
            value.track_filled_gradient,
            value.track_filled_gradient_stops.clone(),
            p,
        )?
    };
    let track_fill_flat = require_bool(value.track_fill_flat, &p("track_fill_flat"))?;
    let track_fill_style = value.track_fill_style.unwrap_or_default();
    let (track_corner_radius, bar_geometry) = resolve_arc_track_geometry(
        width.min(height),
        track_thickness,
        raw_corner_radius,
        track_border_thickness,
        arc_angle,
        corner_orientation.is_some(),
        track_fill_style,
        value.bar_count,
        value.bar_gap,
        &p("track_fill_style"),
    )?;
    let show_min_max_labels = require_bool(value.show_min_max_labels, &p("show_min_max_labels"))?;
    let min_max_label_font =
        super::validate_label_font(value.min_max_label_font.clone(), &p("min_max_label_font"))?;
    let min_max_label_color = require_hex_color(
        value.min_max_label_color.as_deref(),
        &p("min_max_label_color"),
    )?;

    let inner_value = validate_arc_inner_value_widget(value, index)?;

    Ok(ValidatedArcGaugeWidget {
        metric,
        x,
        y,
        width,
        height,
        rotation,
        display_type,
        arc_angle,
        corner_orientation,
        gt_style,
        inner_widget_offset_x,
        inner_widget_offset_y,
        track_thickness,
        track_corner_radius,
        track_border_thickness,
        track_border_color,
        track_empty_color,
        track_empty_opacity,
        track_filled_color,
        track_filled_opacity,
        track_fill_flat,
        track_fill_style,
        bar_geometry,
        show_min_max_labels,
        min_max_label_font,
        min_max_label_font_size,
        min_max_label_color,
        inner_value,
    })
}
