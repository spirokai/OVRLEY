//! Arc gauge metric widget validation.
//!
//! Arc gauges share metric formatting with text widgets, but render that value
//! inside a circular stroked track. The validator keeps every visual input
//! explicit and rejects out-of-range arc geometry before render preparation.

use super::ValidatedValueWidget;
use super::{
    arc_track_radius, corner_track_radius, resolve_bar_style_geometry, track_corner_radius_max,
    ResolvedBarGeometry,
};
use crate::error::{CoreError, CoreResult};
use ovrley_widgets::model::types::{DisplayType, MetricKind, TrackFillStyle};
use ovrley_widgets::model::validation::{require_bool, require_f32, require_string};
use ovrley_widgets::widgets::value::config::ValueConfig;
use ovrley_widgets::widgets::value::validation::validate_arc_inner_value_widget;

pub const MIN_ARC_ANGLE_DEGREES: f32 = 30.0;
pub const MAX_ARC_ANGLE_DEGREES: f32 = 360.0;
pub const CORNER_GAUGE_ANGLE_DEGREES: f32 = 90.0;

/// The supported bottom-corner placements for a fixed 90° gauge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidatedCornerGaugeOrientation {
    BottomLeft,
    BottomRight,
}

/// Fully validated arc gauge configuration.
#[derive(Clone, Debug)]
pub struct ValidatedArcGaugeWidget {
    pub metric: MetricKind,
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub rotation: f32,
    pub display_type: DisplayType,
    pub arc_angle: f32,
    pub corner_orientation: Option<ValidatedCornerGaugeOrientation>,
    pub inner_widget_offset_x: f32,
    pub inner_widget_offset_y: f32,
    pub track_thickness: f32,
    pub track_corner_radius: f32,
    pub track_border_thickness: f32,
    pub track_border_color: String,
    pub track_empty_color: String,
    pub track_empty_opacity: f32,
    pub track_filled_color: String,
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
pub fn validate_arc_gauge(value: ValueConfig, index: usize) -> CoreResult<ValidatedArcGaugeWidget> {
    let p = |field: &str| format!("values[{index}].{field}");

    if value.display_type != DisplayType::Arc {
        return Err(CoreError::Config(format!(
            "{}: expected arc display_type, got '{}'",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    let arc_angle = require_f32(value.arc_angle, &p("arc_angle"))?;
    if !(MIN_ARC_ANGLE_DEGREES..=MAX_ARC_ANGLE_DEGREES).contains(&arc_angle) {
        return Err(CoreError::Config(format!(
            "{}: must be {}..={}, got {arc_angle}",
            p("arc_angle"),
            MIN_ARC_ANGLE_DEGREES as u32,
            MAX_ARC_ANGLE_DEGREES as u32,
        )));
    }

    validate_arc_shaped_gauge(value, index, DisplayType::Arc, arc_angle, None)
}

/// Validates a raw value config as a fixed 90° bottom-corner gauge.
pub fn validate_corner_gauge(
    value: ValueConfig,
    index: usize,
) -> CoreResult<ValidatedArcGaugeWidget> {
    let p = |field: &str| format!("values[{index}].{field}");

    if value.display_type != DisplayType::Corner {
        return Err(CoreError::Config(format!(
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
            return Err(CoreError::Config(format!(
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
    )
}

fn validate_arc_shaped_gauge(
    value: ValueConfig,
    index: usize,
    display_type: DisplayType,
    arc_angle: f32,
    corner_orientation: Option<ValidatedCornerGaugeOrientation>,
) -> CoreResult<ValidatedArcGaugeWidget> {
    let p = |field: &str| format!("values[{index}].{field}");

    let width = value
        .width
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("width"))))?;
    let height = value
        .height
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("height"))))?;
    if width == 0 {
        return Err(CoreError::Config(format!("{}: must be > 0", p("width"))));
    }
    if height == 0 {
        return Err(CoreError::Config(format!("{}: must be > 0", p("height"))));
    }

    let track_thickness = require_f32(value.track_thickness, &p("track_thickness"))?;
    if track_thickness <= 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be > 0, got {track_thickness}",
            p("track_thickness")
        )));
    }

    let raw_corner_radius = require_f32(value.track_corner_radius, &p("track_corner_radius"))?;
    if raw_corner_radius < 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be >= 0, got {raw_corner_radius}",
            p("track_corner_radius")
        )));
    }
    // Arc tracks use a filled outline with endpoint fillets. Keep the shared
    // radius contract bounded by the track half-width, matching linear gauges.
    let initial_corner_radius = raw_corner_radius.min(track_thickness * 0.5);

    let track_border_thickness =
        require_f32(value.track_border_thickness, &p("track_border_thickness"))?;
    if track_border_thickness < 0.0 {
        return Err(CoreError::Config(format!(
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
            return Err(CoreError::Config(format!(
                "{}: must be 0.0..1.0, got {opacity}",
                p(field)
            )));
        }
    }

    let min_max_label_font_size =
        require_f32(value.min_max_label_font_size, &p("min_max_label_font_size"))?;
    if min_max_label_font_size <= 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be > 0, got {min_max_label_font_size}",
            p("min_max_label_font_size")
        )));
    }

    let metric = value.value;
    let x = value.x;
    let y = value.y;
    let rotation = value.rotation.unwrap_or(0.0);
    let inner_widget_offset_x =
        require_f32(value.inner_widget_offset_x, &p("inner_widget_offset_x"))?;
    let inner_widget_offset_y =
        require_f32(value.inner_widget_offset_y, &p("inner_widget_offset_y"))?;
    let track_border_color =
        require_string(value.track_border_color.clone(), &p("track_border_color"))?;
    let track_empty_color =
        require_string(value.track_empty_color.clone(), &p("track_empty_color"))?;
    let track_filled_color =
        require_string(value.track_filled_color.clone(), &p("track_filled_color"))?;
    let track_fill_flat = require_bool(value.track_fill_flat, &p("track_fill_flat"))?;
    let track_fill_style = value.track_fill_style.unwrap_or_default();
    let frame_size = width.min(height) as f32;
    let initial_radius = match corner_orientation {
        Some(_) => corner_track_radius(
            frame_size,
            track_thickness,
            initial_corner_radius,
            track_border_thickness,
        ),
        None => arc_track_radius(frame_size, track_thickness, track_border_thickness),
    };
    let initial_bar_span = arc_angle.to_radians().abs() * initial_radius;
    let initial_bar_geometry = resolve_bar_style_geometry(
        track_fill_style,
        initial_bar_span,
        value.bar_count,
        value.bar_gap,
        &p("track_fill_style"),
    )?;
    let track_corner_radius = raw_corner_radius.min(track_corner_radius_max(
        track_thickness,
        track_thickness,
        initial_bar_geometry.as_ref(),
    ));
    let bar_geometry =
        if corner_orientation.is_some() && track_corner_radius < initial_corner_radius {
            let radius = corner_track_radius(
                frame_size,
                track_thickness,
                track_corner_radius,
                track_border_thickness,
            );
            resolve_bar_style_geometry(
                track_fill_style,
                arc_angle.to_radians().abs() * radius,
                value.bar_count,
                value.bar_gap,
                &p("track_fill_style"),
            )?
        } else {
            initial_bar_geometry
        };
    let show_min_max_labels = require_bool(value.show_min_max_labels, &p("show_min_max_labels"))?;
    let min_max_label_font =
        require_string(value.min_max_label_font.clone(), &p("min_max_label_font"))?;
    let min_max_label_color =
        require_string(value.min_max_label_color.clone(), &p("min_max_label_color"))?;

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
