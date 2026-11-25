//! Value widgets: configuration, family dispatch, standard-metric validation, preview and resize.
use crate::activity::ActivitySampler;
use crate::model::catalog::{catalog, DisplayType, MetricKind, TrackFillStyle};
use crate::model::scene::{
    validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::require_non_negative_f32;
use crate::model::validation::require_positive_f32;
use crate::model::validation::require_unit_opacity;
use crate::model::validation::{require_bool, require_f32, require_string};
use crate::model::validation::{require_precision, require_rgba, Precision};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::text::{validated_value_style, FontResolver, ResolvedTextStyle};
use crate::render::tile::{Presented, Tile, TileBounds, TEXT_COLOR_TYPE};
use crate::render::tile::{PreviewRasterScale, StaticRaster};
use crate::render::track_fill::GradientStop;
use crate::widgets::altitude_tape::{validate_altitude_tape, ValidatedAltitudeTape};
use crate::widgets::g_force::{validate_g_force, ValidatedGForceWidget};
use crate::widgets::gauges::ValidatedGauge;
use crate::widgets::gradient::{validate_gradient_widget, ValidatedGradientWidget};
use crate::widgets::heading::{validate_heading, ValidatedHeading};
use crate::widgets::heading_compass::{validate_heading_compass, ValidatedHeadingCompass};
use crate::widgets::heart_zones::{validate_heart_zones, HeartZoneRange, ValidatedHeartZones};
use crate::widgets::lap_timer::{validate_lap_timer, ValidatedLapTimer};
use crate::widgets::lean_angle::{validate_lean_angle, ValidatedLeanAngleWidget};
use crate::widgets::splits_run::{validate_splits_run, ValidatedSplitsRun};
use crate::widgets::time::{
    format_validated_time_parts, validate_time_value, ElapsedTimeValues, ValidatedTimeValue,
};
use crate::widgets::value::format::{format_metric_parts, MetricDisplayParts};
use crate::widgets::value::layout::{
    draw_prepared_metric_frame, draw_static_metric_parts_for_value, prepare_metric_frame,
    static_metric_parts_for_value, MetricFrame, StaticMetricParts,
};
use crate::{WidgetError, WidgetResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use skia_safe::{Canvas, Point, Rect};
use std::collections::BTreeMap;

pub mod format;
pub mod layout;

/// Dynamic telemetry value configuration.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ValueConfig {
    pub value: MetricKind,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub font_size: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub suffix: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub hours_offset: Option<i32>,
    #[serde(default)]
    pub time_format: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub time_mode: Option<String>,
    #[serde(default)]
    pub elapsed_origin: Option<String>,
    #[serde(default)]
    pub show_hundredths: Option<bool>,
    #[serde(default)]
    pub decimal_rounding: Option<i32>,
    #[serde(default)]
    pub decimals: Option<usize>,
    #[serde(default)]
    pub show_icon: Option<bool>,
    #[serde(default)]
    pub icon_color: Option<String>,
    #[serde(default)]
    pub icon_size: Option<f32>,
    #[serde(default)]
    pub icon_offset_x: Option<f32>,
    #[serde(default)]
    pub icon_offset_y: Option<f32>,
    #[serde(default)]
    pub show_units: Option<bool>,
    #[serde(default)]
    pub show_full_distance: Option<bool>,
    #[serde(default)]
    pub show_full_ascent: Option<bool>,
    #[serde(default)]
    pub unit_color: Option<String>,
    #[serde(default)]
    pub display_unit: Option<String>,
    #[serde(default)]
    pub starting_altitude: Option<f64>,
    #[serde(default)]
    pub coordinate_format: Option<String>,
    #[serde(default)]
    pub balance_format: Option<String>,
    #[serde(default)]
    pub value_offset: Option<f32>,
    #[serde(default)]
    pub triangle_positive_color: Option<String>,
    #[serde(default)]
    pub triangle_negative_color: Option<String>,
    #[serde(default)]
    pub show_sign: Option<bool>,
    #[serde(default)]
    pub show_triangle: Option<bool>,
    #[serde(default)]
    pub triangle_width: Option<f32>,
    #[serde(default)]
    pub shadow_color: Option<String>,
    #[serde(default)]
    pub shadow_strength: Option<f32>,
    #[serde(default)]
    pub shadow_distance: Option<f32>,
    #[serde(default)]
    pub border_color: Option<String>,
    #[serde(default)]
    pub border_thickness: Option<f32>,
    #[serde(default)]
    pub border_strength: Option<f32>,
    #[serde(default)]
    pub border_distance: Option<f32>,
    #[serde(default)]
    pub display_type: DisplayType,
    #[serde(default)]
    pub content_alignment: Option<String>,
    #[serde(default)]
    pub lap_timer_mode: Option<String>,
    #[serde(default)]
    pub show_label: Option<bool>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub split_distance: Option<f64>,
    #[serde(default)]
    pub split_unit: Option<String>,
    #[serde(default)]
    pub max_splits: Option<u32>,
    #[serde(default)]
    pub visible_columns: Option<Vec<String>>,
    #[serde(default)]
    pub row_gap: Option<f32>,
    #[serde(default)]
    pub bar_width: Option<f32>,
    #[serde(default)]
    pub track_border_opacity: Option<f32>,
    #[serde(default)]
    pub positive_delta_color: Option<String>,
    #[serde(default)]
    pub negative_delta_color: Option<String>,
    #[serde(default)]
    pub renumber_laps: Option<bool>,
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub height: Option<f32>,
    #[serde(default)]
    pub rotation: Option<f32>,
    #[serde(default)]
    pub orientation: Option<String>,
    #[serde(default)]
    pub arc_angle: Option<f32>,
    #[serde(default)]
    pub corner_orientation: Option<String>,
    #[serde(default)]
    pub tick_count: Option<u32>,
    #[serde(default)]
    pub tick_length: Option<f32>,
    #[serde(default)]
    pub glow: Option<f32>,
    #[serde(default)]
    pub inner_widget_offset_x: Option<f32>,
    #[serde(default)]
    pub inner_widget_offset_y: Option<f32>,
    #[serde(default)]
    pub value_offset_x: Option<f32>,
    #[serde(default)]
    pub value_offset_y: Option<f32>,
    #[serde(default)]
    pub track_thickness: Option<f32>,
    #[serde(default)]
    pub track_corner_radius: Option<f32>,
    #[serde(default)]
    pub track_border_thickness: Option<f32>,
    #[serde(default)]
    pub track_border_color: Option<String>,
    #[serde(default)]
    pub track_empty_color: Option<String>,
    #[serde(default)]
    pub track_empty_opacity: Option<f32>,
    #[serde(default)]
    pub track_filled_color: Option<String>,
    #[serde(default)]
    pub track_filled_opacity: Option<f32>,
    #[serde(default)]
    pub track_filled_gradient: Option<bool>,
    #[serde(default)]
    pub track_filled_gradient_stops: Option<Vec<GradientStop>>,
    #[serde(default)]
    pub track_fill_flat: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_track_fill_style"
    )]
    pub track_fill_style: Option<TrackFillStyle>,
    #[serde(default)]
    pub bar_count: Option<u32>,
    #[serde(default)]
    pub bar_gap: Option<f32>,
    #[serde(default)]
    pub show_min_max_labels: Option<bool>,
    #[serde(default)]
    pub min_max_label_font: Option<String>,
    #[serde(default)]
    pub min_max_label_font_size: Option<f32>,
    #[serde(default)]
    pub min_max_label_position: Option<String>,
    #[serde(default)]
    pub min_max_label_color: Option<String>,
    #[serde(default)]
    pub diameter: Option<f32>,
    #[serde(default)]
    pub fill_color: Option<String>,
    #[serde(default)]
    pub fill_opacity: Option<f32>,
    #[serde(default)]
    pub border_opacity: Option<f32>,
    #[serde(default)]
    pub marker_size: Option<f32>,
    #[serde(default)]
    pub marker_color: Option<String>,
    #[serde(default)]
    pub marker_opacity: Option<f32>,
    #[serde(default)]
    pub axis_horizontal: Option<String>,
    #[serde(default)]
    pub axis_vertical: Option<String>,
    #[serde(default)]
    pub invert_horizontal: Option<bool>,
    #[serde(default)]
    pub invert_vertical: Option<bool>,
    #[serde(default)]
    pub clip_percentile: Option<f32>,
    #[serde(default)]
    pub label_font: Option<String>,
    #[serde(default)]
    pub label_font_size: Option<f32>,
    #[serde(default)]
    pub label_color: Option<String>,
    #[serde(default)]
    pub label_decimals: Option<usize>,
    #[serde(default)]
    pub label_unit: Option<String>,
    #[serde(default)]
    pub label_unit_color: Option<String>,
    #[serde(default)]
    pub label_offset_x: Option<f32>,
    #[serde(default)]
    pub label_offset_y: Option<f32>,
    #[serde(default)]
    pub pixels_per_degree: Option<f32>,
    #[serde(default)]
    pub major_tick_interval: Option<u32>,
    #[serde(default)]
    pub minor_ticks_per_major: Option<u32>,
    #[serde(default)]
    pub show_major_ticks: Option<bool>,
    #[serde(default)]
    pub show_minor_ticks: Option<bool>,
    #[serde(default)]
    pub major_tick_length_pct: Option<f32>,
    #[serde(default)]
    pub minor_tick_length_pct: Option<f32>,
    #[serde(default)]
    pub major_tick_thickness: Option<f32>,
    #[serde(default)]
    pub minor_tick_thickness: Option<f32>,
    #[serde(default)]
    pub tick_color: Option<String>,
    #[serde(default)]
    pub cardinal_tick_color: Option<String>,
    #[serde(default)]
    pub tick_alignment: Option<String>,
    #[serde(default)]
    pub show_minor_labels: Option<bool>,
    #[serde(default)]
    pub show_major_labels: Option<bool>,
    #[serde(default)]
    pub cardinal_label_color: Option<String>,
    #[serde(default)]
    pub label_offset: Option<f32>,
    #[serde(default)]
    pub show_indicator: Option<bool>,
    #[serde(default)]
    pub indicator_style: Option<String>,
    #[serde(default)]
    pub indicator_placement: Option<String>,
    #[serde(default)]
    pub indicator_color: Option<String>,
    #[serde(default)]
    pub indicator_size: Option<f32>,
    #[serde(default)]
    pub indicator_corner_radius: Option<f32>,
    #[serde(default)]
    pub rotation_mode: Option<String>,
    #[serde(default)]
    pub track_fill: Option<String>,
    #[serde(default)]
    pub show_cardinal_labels: Option<bool>,
    #[serde(default)]
    pub cardinal_font_size: Option<f32>,
    #[serde(default)]
    pub show_value: Option<bool>,
    #[serde(default)]
    pub zone_calculation: Option<String>,
    #[serde(default)]
    pub lthr: Option<f32>,
    #[serde(default)]
    pub max_heartrate: Option<f32>,
    #[serde(default)]
    pub manual_zones: Option<Vec<HeartZoneRange>>,
    #[serde(default)]
    pub zone_colors: Option<Vec<String>>,
    #[serde(default)]
    pub zone_gap: Option<f32>,
    #[serde(default)]
    pub label_font_weight: Option<f32>,
    #[serde(default)]
    pub major_tick_spacing: Option<f32>,
    #[serde(default)]
    pub major_tick_color: Option<String>,
    #[serde(default)]
    pub minor_tick_color: Option<String>,
    #[serde(default)]
    pub show_tick_labels: Option<bool>,
    #[serde(default)]
    pub show_value_decimal: Option<bool>,
    #[serde(default)]
    pub value_background_color: Option<String>,
    #[serde(default)]
    pub value_border_color: Option<String>,
    #[serde(default)]
    pub value_border_thickness: Option<f32>,
    #[serde(default)]
    pub value_shadow_color: Option<String>,
    #[serde(default)]
    pub value_shadow_sigma: Option<f32>,
    #[serde(default)]
    pub value_shadow_offset: Option<f32>,
    #[serde(default)]
    pub tape_orientation: Option<String>,
    #[serde(default)]
    pub show_vertical_speed: Option<bool>,
    #[serde(default)]
    pub vertical_speed_unit: Option<String>,
    #[serde(default)]
    pub show_vertical_speed_decimal: Option<bool>,
    #[serde(default)]
    pub vertical_speed_font_size: Option<f32>,
    #[serde(default)]
    pub vertical_speed_bar_thickness: Option<f32>,
    #[serde(default)]
    pub vertical_speed_positive_color: Option<String>,
    #[serde(default)]
    pub vertical_speed_negative_color: Option<String>,
    #[serde(default)]
    pub vertical_speed_trend_seconds: Option<f32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// Omitted track_fill_style retains the documented continuous-fill default;
// malformed present values, including null, must fail at ingress.
fn deserialize_track_fill_style<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<TrackFillStyle>, D::Error> {
    TrackFillStyle::deserialize(deserializer).map(Some)
}

/// Horizontal point of an intrinsic metric row attached to the configured x coordinate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentAlignment {
    Left,
    Center,
    Right,
}

impl ContentAlignment {
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "right" => Some(Self::Right),
            _ => None,
        }
    }
}

pub fn validate_content_alignment(
    content_alignment: Option<String>,
    path: &str,
) -> WidgetResult<ContentAlignment> {
    let key = require_string(content_alignment, path)?;
    ContentAlignment::parse(&key).ok_or_else(|| {
        WidgetError::Config(format!(
            "{path}: expected 'left', 'center', or 'right', got '{key}'"
        ))
    })
}

/// How a left/right balance reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BalanceFormat {
    /// `52/48`
    Plain,
    /// `52%/48%`
    PercentLabel,
    /// `L52/R48`
    LPrefix,
    /// `52L/48R`
    LSuffix,
}

impl BalanceFormat {
    fn parse(value: Option<&str>, path: &str) -> WidgetResult<Self> {
        match value {
            Some("plain") => Ok(Self::Plain),
            Some("percent_label") => Ok(Self::PercentLabel),
            Some("l_prefix") => Ok(Self::LPrefix),
            Some("l_suffix") => Ok(Self::LSuffix),
            _ => Err(WidgetError::Config(format!(
                "{path}: expected a supported balance format"
            ))),
        }
    }
}

/// Where a metric's text is drawn: on its own, with an icon and an alignment, or inside another
/// presentation (an arc gauge, the gradient triangle), which draws no icon and places the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextPlacement {
    Standalone,
    Embedded,
}

/// The text fields every value text validates: font, colour, icon and affixes.
pub(crate) struct TextFields {
    pub content_alignment: ContentAlignment,
    pub font_name: String,
    pub font_size: f32,
    pub opacity: f32,
    pub color: [u8; 4],
    pub show_icon: bool,
    pub icon_color: [u8; 4],
    pub icon_size: f32,
    pub icon_offset_x: f32,
    pub icon_offset_y: f32,
    pub prefix: String,
    pub suffix: String,
}

impl TextFields {
    /// Takes the text fields out of `value`; embedded text has no icon and aligns left.
    pub(crate) fn validate(
        value: &mut ValueConfig,
        placement: TextPlacement,
        p: &dyn Fn(&str) -> String,
    ) -> WidgetResult<Self> {
        require_f32(Some(value.x), &p("x"))?;
        require_f32(Some(value.y), &p("y"))?;
        let font_name = require_string(value.font.take(), &p("font"))?;
        let opacity = require_unit_opacity(value.opacity, &p("opacity"))?;
        let font_size = require_positive_f32(value.font_size, &p("font_size"))?;
        let color = require_rgba(value.color.as_deref(), &p("color"), opacity)?;
        let standalone = placement == TextPlacement::Standalone;
        let content_alignment = if standalone {
            validate_content_alignment(value.content_alignment.take(), &p("content_alignment"))?
        } else {
            ContentAlignment::Left
        };
        let (show_icon, icon_color, icon_size, icon_offset_x, icon_offset_y) = if standalone {
            (
                require_bool(value.show_icon, &p("show_icon"))?,
                require_rgba(value.icon_color.as_deref(), &p("icon_color"), opacity)?,
                require_non_negative_f32(value.icon_size, &p("icon_size"))?,
                require_f32(value.icon_offset_x, &p("icon_offset_x"))?,
                require_f32(value.icon_offset_y, &p("icon_offset_y"))?,
            )
        } else {
            (false, [0, 0, 0, 0], 0.0, 0.0, 0.0)
        };
        Ok(Self {
            content_alignment,
            font_name,
            font_size,
            opacity,
            color,
            show_icon,
            icon_color,
            icon_size,
            icon_offset_x,
            icon_offset_y,
            prefix: require_string(value.prefix.take(), &p("prefix"))?,
            suffix: require_string(value.suffix.take(), &p("suffix"))?,
        })
    }
}

/// A validated standard-metric text widget.
#[derive(Clone, Debug)]
pub struct ValidatedValueWidget {
    pub metric: MetricKind,
    pub x: f32,
    pub y: f32,
    pub display_type: DisplayType,
    pub content_alignment: ContentAlignment,
    pub font_name: String,
    pub font_size: f32,
    pub color: [u8; 4],
    pub opacity: f32,
    pub show_icon: bool,
    pub icon_color: [u8; 4],
    pub icon_size: f32,
    pub icon_offset_x: f32,
    pub icon_offset_y: f32,
    pub show_units: bool,
    pub show_full_distance: Option<bool>,
    pub show_full_ascent: Option<bool>,
    pub coordinate_format: Option<String>,
    pub unit_color: [u8; 4],
    pub display_unit: String,
    /// Optional presentation target for the first altitude sample, normalized to meters.
    pub starting_altitude_m: Option<f64>,
    pub prefix: String,
    pub suffix: String,
    pub precision: Precision,
    /// Present for left/right balance only.
    pub balance_format: Option<BalanceFormat>,
    pub hours_offset: Option<i64>,
    pub format: Option<String>,
}

pub fn normalize_starting_altitude_m(
    metric: MetricKind,
    starting_altitude: Option<f64>,
    display_unit: Option<&str>,
    altitude_path: &str,
    unit_path: &str,
) -> WidgetResult<Option<f64>> {
    if metric != MetricKind::Altitude {
        if starting_altitude.is_some() {
            return Err(WidgetError::Config(format!(
                "{altitude_path}: is only valid for altitude widgets"
            )));
        }
        return Ok(None);
    }

    let display_unit =
        display_unit.ok_or_else(|| WidgetError::Config(format!("{unit_path}: required")))?;

    let catalog = catalog();
    let meter_scale = catalog.altitude_unit_scale(display_unit).ok_or_else(|| {
        let units: Vec<String> = catalog
            .altitude_unit_scales
            .iter()
            .map(|(unit, _)| format!("'{unit}'"))
            .collect();
        WidgetError::Config(format!(
            "{unit_path}: expected {}, got '{display_unit}'",
            units.join(" or ")
        ))
    })?;

    if starting_altitude.is_some_and(|altitude| !altitude.is_finite()) {
        return Err(WidgetError::Config(format!(
            "{altitude_path}: must be finite"
        )));
    }
    Ok(starting_altitude.map(|altitude| altitude / meter_scale))
}

pub fn validate_value_widget(
    value: ValueConfig,
    index: usize,
) -> WidgetResult<ValidatedValueWidget> {
    let p = |f: &str| format!("values[{index}].{f}");

    require_f32(Some(value.x), &p("x"))?;
    require_f32(Some(value.y), &p("y"))?;

    if value.display_type != DisplayType::Text {
        return Err(WidgetError::Config(format!(
            "{}: display_type '{}' is not supported by metric text",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    if !catalog().supports_display(value.value, DisplayType::Text) {
        return Err(WidgetError::Config(format!(
            "{}: metric does not support text display",
            p("value")
        )));
    }

    validate_value_widget_fields(value, index, TextPlacement::Standalone)
}

/// Validates the value and unit text inside an arc gauge, which draws no icon.
pub fn validate_arc_inner_value_widget(
    value: ValueConfig,
    index: usize,
) -> WidgetResult<ValidatedValueWidget> {
    validate_value_widget_fields(value, index, TextPlacement::Embedded)
}

fn validate_value_widget_fields(
    value: ValueConfig,
    index: usize,
    placement: TextPlacement,
) -> WidgetResult<ValidatedValueWidget> {
    let p = |f: &str| format!("values[{index}].{f}");

    if !catalog().standard_metric(value.value).is_some() {
        return Err(WidgetError::Config(format!(
            "{}: metric {:?} is not a standard metric",
            p("value"),
            value.value
        )));
    }
    if placement == TextPlacement::Embedded && value.value == MetricKind::GpsCoordinates {
        return Err(WidgetError::Config(format!(
            "{}: gps_coordinates only supports text display",
            p("value")
        )));
    }

    let mut value = value;
    let text = TextFields::validate(&mut value, placement, &p)?;
    let opacity = text.opacity;

    let show_units = require_bool(value.show_units, &p("show_units"))?;
    let show_full_distance = if value.value == MetricKind::Distance {
        Some(require_bool(
            value.show_full_distance,
            &p("show_full_distance"),
        )?)
    } else {
        value.show_full_distance
    };
    let show_full_ascent = if value.value == MetricKind::TotalAscent {
        Some(require_bool(
            value.show_full_ascent,
            &p("show_full_ascent"),
        )?)
    } else {
        if value.show_full_ascent.is_some() {
            return Err(WidgetError::Config(format!(
                "{}: is only valid for total_ascent widgets",
                p("show_full_ascent")
            )));
        }
        None
    };
    let coordinate_format = if value.value == MetricKind::GpsCoordinates {
        let coordinate_format = require_string(value.coordinate_format, &p("coordinate_format"))?;
        if !matches!(coordinate_format.as_str(), "dms" | "ddm") {
            return Err(WidgetError::Config(format!(
                "{}: expected 'dms' or 'ddm', got '{coordinate_format}'",
                p("coordinate_format")
            )));
        }
        Some(coordinate_format)
    } else {
        if value.coordinate_format.is_some() {
            return Err(WidgetError::Config(format!(
                "{}: is only valid for gps_coordinates widgets",
                p("coordinate_format")
            )));
        }
        None
    };

    let unit_color = require_rgba(value.unit_color.as_deref(), &p("unit_color"), opacity)?;
    let display_unit = require_string(value.display_unit, &p("display_unit"))?;
    let supported_units = &catalog()
        .standard_metric(value.value)
        .expect("standard metric validated above")
        .supported_display_units;
    if !supported_units
        .iter()
        .any(|unit| unit.value == display_unit)
    {
        let names: Vec<_> = supported_units
            .iter()
            .map(|unit| format!("'{}'", unit.value))
            .collect();
        let expected = match names.as_slice() {
            [only] => only.clone(),
            [first, last] => format!("{first} or {last}"),
            [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
            [] => unreachable!("standard metric declares display units"),
        };
        return Err(WidgetError::Config(format!(
            "{}: expected {expected}, got '{display_unit}'",
            p("display_unit")
        )));
    }
    let starting_altitude_m = normalize_starting_altitude_m(
        value.value,
        value.starting_altitude,
        Some(display_unit.as_str()),
        &p("starting_altitude"),
        &p("display_unit"),
    )?;

    let precision = require_precision(value.decimals, value.decimal_rounding, p)?;
    let balance_format = (value.value == MetricKind::LeftRightBalance)
        .then(|| BalanceFormat::parse(value.balance_format.as_deref(), &p("balance_format")))
        .transpose()?;
    if value.value == MetricKind::Distance && precision.decimals() > 2 {
        return Err(WidgetError::Config(format!(
            "{}: distance supports only 0, 1, or 2 decimals, got {}",
            p("decimals"),
            precision.decimals()
        )));
    }

    Ok(ValidatedValueWidget {
        metric: value.value,
        x: value.x,
        y: value.y,
        display_type: value.display_type,
        content_alignment: text.content_alignment,
        font_name: text.font_name,
        font_size: text.font_size,
        color: text.color,
        opacity,
        show_icon: text.show_icon,
        icon_color: text.icon_color,
        icon_size: text.icon_size,
        icon_offset_x: text.icon_offset_x,
        icon_offset_y: text.icon_offset_y,
        show_units,
        show_full_distance,
        show_full_ascent,
        coordinate_format,
        unit_color,
        display_unit,
        starting_altitude_m,
        prefix: text.prefix,
        suffix: text.suffix,
        precision,
        balance_format,
        hours_offset: value.hours_offset.map(i64::from),
        format: value.format,
    })
}

#[derive(Clone, Debug)]
pub enum ValidatedValue {
    StandardText(ValidatedValueWidget),
    TimeText(ValidatedTimeValue),
    Gradient(ValidatedGradientWidget),
    HeadingTape(ValidatedHeading),
    HeadingCompass(ValidatedHeadingCompass),
    HeartZones(ValidatedHeartZones),
    AltitudeTape(ValidatedAltitudeTape),
    LeanAngle(ValidatedLeanAngleWidget),
    Gauge(ValidatedGauge),
    GForce(ValidatedGForceWidget),
    LapTimer(ValidatedLapTimer),
    SplitsRun(ValidatedSplitsRun),
}

/// The renderer that presents a value widget, chosen by its metric and display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueFamily {
    /// Icon, value and unit text: standard metrics and time.
    Text,
    Gradient,
    LapTimer,
    SplitsRun,
    LeanAngle,
    GForce,
    HeadingTape,
    HeadingCompass,
    HeartZones,
    AltitudeTape,
    Gauge,
}

impl ValueFamily {
    pub fn of(metric: MetricKind, display: DisplayType) -> Self {
        use DisplayType as D;
        use MetricKind as M;
        match (metric, display) {
            (M::Gradient, _) => Self::Gradient,
            (M::LapTimer, _) => Self::LapTimer,
            (M::SplitsRun, _) => Self::SplitsRun,
            (M::LeanAngle, D::LeanAngle) => Self::LeanAngle,
            (M::GForce, D::GForce) => Self::GForce,
            (M::Heading, D::HeadingTape) => Self::HeadingTape,
            (M::Heading, D::HeadingCompass) => Self::HeadingCompass,
            (M::Heartrate, D::HeartZones) => Self::HeartZones,
            (M::Altitude | M::Depth, D::AltitudeTape) => Self::AltitudeTape,
            (_, D::Linear | D::Arc | D::Corner | D::GtGauge) => Self::Gauge,
            _ => Self::Text,
        }
    }
}

impl ValidatedValue {
    /// Validates a value widget with its family's rules; `index` names it in errors.
    pub fn validate(widget: ValueConfig, index: usize) -> WidgetResult<Self> {
        Ok(match ValueFamily::of(widget.value, widget.display_type) {
            ValueFamily::Text if widget.value == MetricKind::Time => {
                Self::TimeText(validate_time_value(widget, index)?)
            }
            ValueFamily::Text => Self::StandardText(validate_value_widget(widget, index)?),
            ValueFamily::Gradient => Self::Gradient(validate_gradient_widget(widget, index)?),
            ValueFamily::LapTimer => Self::LapTimer(validate_lap_timer(widget, index)?),
            ValueFamily::SplitsRun => Self::SplitsRun(validate_splits_run(widget, index)?),
            ValueFamily::LeanAngle => Self::LeanAngle(validate_lean_angle(widget, index)?),
            ValueFamily::GForce => Self::GForce(validate_g_force(widget, index)?),
            ValueFamily::HeadingTape => Self::HeadingTape(validate_heading(widget, index)?),
            ValueFamily::HeadingCompass => {
                Self::HeadingCompass(validate_heading_compass(widget, index)?)
            }
            ValueFamily::HeartZones => Self::HeartZones(validate_heart_zones(widget, index)?),
            ValueFamily::AltitudeTape => Self::AltitudeTape(validate_altitude_tape(widget, index)?),
            ValueFamily::Gauge => Self::Gauge(ValidatedGauge::validate(widget, index)?),
        })
    }

    pub fn metric_kind(&self) -> MetricKind {
        match self {
            Self::StandardText(value) => value.metric,
            Self::TimeText(_) => MetricKind::Time,
            Self::Gradient(_) => MetricKind::Gradient,
            Self::HeadingTape(_) | Self::HeadingCompass(_) => MetricKind::Heading,
            Self::HeartZones(_) => MetricKind::Heartrate,
            Self::AltitudeTape(tape) => tape.source.metric(),
            Self::LeanAngle(_) => MetricKind::LeanAngle,
            Self::Gauge(gauge) => gauge.metric(),
            Self::GForce(_) => MetricKind::GForce,
            Self::LapTimer(_) => MetricKind::LapTimer,
            Self::SplitsRun(_) => MetricKind::SplitsRun,
        }
    }

    pub fn display_type(&self) -> DisplayType {
        match self {
            Self::StandardText(value) => value.display_type,
            Self::TimeText(value) => value.base.display_type,
            Self::Gradient(_) => DisplayType::Text,
            Self::HeadingTape(_) => DisplayType::HeadingTape,
            Self::HeadingCompass(_) => DisplayType::HeadingCompass,
            Self::HeartZones(_) => DisplayType::HeartZones,
            Self::AltitudeTape(_) => DisplayType::AltitudeTape,
            Self::LeanAngle(_) => DisplayType::LeanAngle,
            Self::Gauge(gauge) => gauge.display_type(),
            Self::GForce(_) => DisplayType::GForce,
            Self::LapTimer(_) => DisplayType::LapTimer,
            Self::SplitsRun(_) => DisplayType::SplitsRun,
        }
    }

    pub fn text_widget(&self) -> Option<&ValidatedValueWidget> {
        match self {
            Self::StandardText(value) => Some(value),
            Self::TimeText(value) => Some(&value.base),
            _ => None,
        }
    }
}

pub enum ValuePresentation {
    Metric(ValidatedValueWidget),
    Time {
        widget: ValidatedTimeValue,
        export_start_seconds: f64,
    },
}

impl ValuePresentation {
    pub fn new(widget: ValueConfig, host: &WidgetHost) -> WidgetResult<Self> {
        if widget.value == MetricKind::Time {
            Self::time(validate_time_value(widget, 0)?, host)
        } else {
            Ok(Self::Metric(validate_value_widget(widget, 0)?))
        }
    }

    /// Elapsed export time counts from the host's clock origin.
    pub fn time(widget: ValidatedTimeValue, host: &WidgetHost) -> WidgetResult<Self> {
        Ok(Self::Time {
            widget,
            export_start_seconds: host.export_start_seconds()?,
        })
    }

    pub fn base(&self) -> &ValidatedValueWidget {
        match self {
            Self::Metric(widget) => widget,
            Self::Time { widget, .. } => &widget.base,
        }
    }

    pub fn base_mut(&mut self) -> &mut ValidatedValueWidget {
        match self {
            Self::Metric(widget) => widget,
            Self::Time { widget, .. } => &mut widget.base,
        }
    }

    pub fn parts(&self, activity: Option<&ActivitySampler>, second: f64) -> MetricDisplayParts {
        match self {
            Self::Metric(widget) => format_metric_parts(
                widget,
                &activity
                    .map(|a| a.sample(widget, second))
                    .unwrap_or_default(),
            ),
            Self::Time {
                widget,
                export_start_seconds,
            } => {
                let raw = activity.and_then(|a| a.time.at(second));
                format_validated_time_parts(
                    widget,
                    raw.as_deref(),
                    ElapsedTimeValues {
                        activity_seconds: second,
                        export_seconds: second - export_start_seconds,
                    },
                    activity.and_then(|a| a.timezone),
                )
            }
        }
    }
}

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub icon_size: f32,
    pub icon_offset_x: f32,
    pub icon_offset_y: f32,
}

pub fn resize_value(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    factor: f32,
    gesture: ResizeGesture,
    activity: Option<&ActivitySampler>,
    fonts: &dyn FontResolver,
) -> WidgetResult<ResizeUpdate> {
    let ResizeGesture {
        fixed,
        point,
        second,
        ..
    } = gesture;
    let host = WidgetHost::from_scene(&scene, PreviewRasterScale::NATIVE)?;
    let mut presentation = ValuePresentation::new(widget, &host)?;
    let config = presentation.base_mut();
    let scene = validate_scene_presentation(scene)?;
    // The editor's intrinsic size limits, without rounding on release.
    config.font_size = (config.font_size * factor).clamp(8.0, 400.0);
    config.icon_size = (config.icon_size * factor).clamp(0.0, 400.0);
    config.icon_offset_x *= factor;
    config.icon_offset_y *= factor;
    let style = validated_value_style(config, &scene, scene.scale);
    let parts = presentation.parts(activity, second);
    let config = presentation.base();
    let bounds = prepare_metric_frame(&style, &parts, scene.scale, fonts, config)?.content_bounds;
    Ok(ResizeUpdate {
        x: config.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: config.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        font_size: config.font_size,
        icon_size: config.icon_size,
        icon_offset_x: config.icon_offset_x,
        icon_offset_y: config.icon_offset_y,
    })
}

/// Layout and drawing state, borrowed apart from the tile surface while rendering.
struct ValueLayout {
    config: ValuePresentation,
    style: ResolvedTextStyle,
    scale: f32,
    static_parts: StaticMetricParts,
    static_image: Option<StaticRaster>,
}

impl ValueLayout {
    /// Layout step: the metric frame and the tile rectangle covering it and the static parts.
    fn frame(
        &self,
        parts: &MetricDisplayParts,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<(MetricFrame, Rect)> {
        let frame =
            prepare_metric_frame(&self.style, parts, self.scale, fonts, self.config.base())?;
        let mut bounds = frame.bounds;
        if let Some(cache) = &self.static_image {
            bounds.join(cache.bounds());
        }
        Ok((frame, bounds))
    }

    fn draw(
        &self,
        canvas: &Canvas,
        parts: &MetricDisplayParts,
        fonts: &dyn FontResolver,
        frame: &MetricFrame,
    ) -> WidgetResult<()> {
        if let Some(cache) = &self.static_image {
            cache.draw(canvas, None);
        }
        draw_prepared_metric_frame(
            canvas,
            &self.style,
            parts,
            fonts,
            self.static_parts,
            self.config.base(),
            frame,
        )
    }
}

pub struct PreviewValue {
    layout: ValueLayout,
    preview_raster_scale: PreviewRasterScale,
    pub tile: Tile,
    last_parts: Option<MetricDisplayParts>,
}

impl PreviewValue {
    pub fn new(
        widget: ValueConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        let config = ValuePresentation::new(widget, &host)?;
        Self::from_validated(config, &validate_scene_presentation(scene)?, &host, fonts)
    }

    pub fn from_validated(
        config: ValuePresentation,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let preview_raster_scale = host.raster_scale;
        let style = validated_value_style(config.base(), scene, scene.scale);
        let static_parts = static_metric_parts_for_value(config.base());
        let static_image = if static_parts.icon || static_parts.unit {
            let parts = config.parts(None, 0.0);
            let frame = prepare_metric_frame(&style, &parts, scene.scale, fonts, config.base())?;
            Some(StaticRaster::new(
                preview_raster_scale,
                Point::default(),
                frame.bounds,
                TEXT_COLOR_TYPE,
                |canvas| {
                    draw_static_metric_parts_for_value(
                        canvas,
                        config.base(),
                        &style,
                        scene.scale,
                        fonts,
                    )?;
                    Ok(())
                },
            )?)
        } else {
            None
        };
        Ok(Self {
            layout: ValueLayout {
                config,
                style,
                scale: scene.scale,
                static_parts,
                static_image,
            },
            preview_raster_scale,
            tile: Tile::default(),
            last_parts: None,
        })
    }

    /// The parts the frame at `second` draws; equal parts draw identical pixels.
    pub fn frame_key(&self, activity: Option<&ActivitySampler>, second: f64) -> MetricDisplayParts {
        self.layout.config.parts(activity, second)
    }

    /// Scene rectangle a render at `second` draws into, from text layout alone.
    pub fn layout_bounds(
        &self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let parts = self.layout.config.parts(activity, second);
        Ok(self.layout.frame(&parts, fonts)?.1.into())
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<TileBounds> {
        let parts = self.layout.config.parts(activity, second);
        let (frame, bounds) = self.layout.frame(&parts, fonts)?;
        self.layout.draw(canvas, &parts, fonts, &frame)?;
        Ok(bounds.into())
    }

    pub fn render(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let parts = self.layout.config.parts(activity, second);
        if self.last_parts.as_ref() == Some(&parts) {
            return Ok(());
        }
        let (frame, bounds) = self.layout.frame(&parts, fonts)?;
        let layout = &self.layout;
        self.tile.render(
            self.preview_raster_scale,
            TEXT_COLOR_TYPE,
            bounds,
            Presented::new(frame.content_bounds, frame.content_width),
            |canvas| layout.draw(canvas, &parts, fonts, &frame),
        )?;
        self.last_parts = Some(parts);
        Ok(())
    }
}
