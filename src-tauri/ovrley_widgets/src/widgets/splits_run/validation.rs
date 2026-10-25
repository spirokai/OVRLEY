//! Running splits widget validation.
use super::table::{SplitLength, SplitUnit};
use crate::error::{WidgetError, WidgetResult};
use crate::model::types::{DisplayType, MetricKind};
use crate::model::validation::{
    require_f32, require_hex_color, require_positive_f32, require_positive_u32, require_string,
    require_unit_opacity, rgba_from_hex,
};
use crate::render::track_fill::{validate_track_fill_color, TrackFillColor};
use crate::widgets::value::config::ValueConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitColumns {
    pub split: bool,
    pub bar: bool,
    pub pace: bool,
}

#[derive(Clone, Debug)]
pub struct ValidatedSplitsRun {
    pub x: f32,
    pub y: f32,
    pub font_name: String,
    pub font_size: f32,
    pub color: [u8; 4],
    pub opacity: f32,
    pub label: String,
    pub label_font_name: String,
    pub label_font_size: f32,
    pub label_color: [u8; 4],
    pub length: SplitLength,
    pub max_splits: usize,
    pub columns: SplitColumns,
    pub row_gap: f32,
    /// Full bar length, drawn for the slowest pace; the table width follows its columns.
    pub bar_width: f32,
    pub track_thickness: f32,
    pub track_corner_radius: f32,
    pub track_filled_color: TrackFillColor,
    pub track_filled_opacity: f32,
    /// Zero draws no border, as on linear gauges.
    pub track_border_thickness: f32,
    pub track_border_color: String,
    pub track_border_opacity: f32,
}

pub fn validate_splits_run(value: ValueConfig, index: usize) -> WidgetResult<ValidatedSplitsRun> {
    let p = |name: &str| format!("values[{index}].{name}");
    let invalid =
        |name: &str, message: String| Err(WidgetError::Config(format!("{}: {message}", p(name))));
    if value.value != MetricKind::SplitsRun {
        return invalid("value", "expected splits_run metric".into());
    }
    if value.display_type != DisplayType::SplitsRun {
        return invalid(
            "display_type",
            "splits_run widgets require display_type 'splits_run'".into(),
        );
    }

    let opacity = require_unit_opacity(value.opacity, &p("opacity"))?;
    let color = |field: Option<String>, name: &str| {
        rgba_from_hex(require_string(field, &p(name))?.as_str(), &p(name), opacity)
    };

    let split_distance = value
        .split_distance
        .ok_or_else(|| WidgetError::Config(format!("{}: required", p("split_distance"))))?;
    if !split_distance.is_finite() || split_distance <= 0.0 {
        return invalid(
            "split_distance",
            format!("must be > 0, got {split_distance}"),
        );
    }
    let split_unit = require_string(value.split_unit, &p("split_unit"))?;
    let unit = SplitUnit::parse(&split_unit).ok_or_else(|| {
        WidgetError::Config(format!(
            "{}: unsupported unit '{split_unit}'",
            p("split_unit")
        ))
    })?;

    let mut columns = SplitColumns {
        split: false,
        bar: false,
        pace: false,
    };
    let visible = value
        .visible_columns
        .ok_or_else(|| WidgetError::Config(format!("{}: required", p("visible_columns"))))?;
    if visible.is_empty() {
        return invalid("visible_columns", "must show at least one column".into());
    }
    for column in &visible {
        let shown = match column.as_str() {
            "split" => &mut columns.split,
            "bar" => &mut columns.bar,
            "pace" => &mut columns.pace,
            other => return invalid("visible_columns", format!("unsupported column '{other}'")),
        };
        if *shown {
            return invalid("visible_columns", format!("duplicate column '{column}'"));
        }
        *shown = true;
    }

    let row_gap = require_f32(value.row_gap, &p("row_gap"))?;
    if !(row_gap >= 0.0) {
        return invalid("row_gap", format!("must be >= 0, got {row_gap}"));
    }
    let track_thickness = require_positive_f32(value.track_thickness, &p("track_thickness"))?;
    let track_corner_radius = require_f32(value.track_corner_radius, &p("track_corner_radius"))?;
    if !(track_corner_radius >= 0.0) {
        return invalid(
            "track_corner_radius",
            format!("must be >= 0, got {track_corner_radius}"),
        );
    }
    let track_border_thickness =
        require_f32(value.track_border_thickness, &p("track_border_thickness"))?;
    if !(track_border_thickness >= 0.0) || track_border_thickness * 2.0 >= track_thickness {
        return invalid(
            "track_border_thickness",
            format!("must be >= 0 and leave a positive fill inside track_thickness, got {track_border_thickness}"),
        );
    }

    Ok(ValidatedSplitsRun {
        x: value.x,
        y: value.y,
        font_name: require_string(value.font, &p("font"))?,
        font_size: require_positive_f32(value.font_size, &p("font_size"))?,
        color: color(value.color, "color")?,
        opacity,
        label: require_string(value.label, &p("label"))?,
        label_font_name: require_string(value.label_font, &p("label_font"))?,
        label_font_size: require_positive_f32(value.label_font_size, &p("label_font_size"))?,
        label_color: color(value.label_color, "label_color")?,
        length: SplitLength {
            distance: split_distance,
            unit,
        },
        max_splits: require_positive_u32(value.max_splits, &p("max_splits"))? as usize,
        columns,
        row_gap,
        bar_width: require_positive_f32(value.bar_width, &p("bar_width"))?,
        track_thickness,
        track_corner_radius,
        track_filled_color: validate_track_fill_color(
            value.track_filled_color.as_deref(),
            value.track_filled_gradient,
            value.track_filled_gradient_stops,
            p,
        )?,
        track_filled_opacity: require_unit_opacity(
            value.track_filled_opacity,
            &p("track_filled_opacity"),
        )?,
        track_border_thickness,
        track_border_color: require_hex_color(
            value.track_border_color.as_deref(),
            &p("track_border_color"),
        )?,
        track_border_opacity: require_unit_opacity(
            value.track_border_opacity,
            &p("track_border_opacity"),
        )?,
    })
}
