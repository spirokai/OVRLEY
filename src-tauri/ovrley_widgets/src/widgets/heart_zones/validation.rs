//! Effective heart-zones configuration is validated once at ingress; zones are resolved here.
use super::zones::{coggan_zones, lthr_from_max_heartrate, HeartZoneRange, ZONE_COUNT};
use crate::{
    error::{WidgetError, WidgetResult},
    model::{
        types::{DisplayType, MetricKind},
        validation::{
            require_bool, require_f32, require_hex_color, require_non_negative_f32,
            require_positive_f32, require_string, require_unit_opacity,
        },
    },
    widgets::value::config::ValueConfig,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeartZonesIndicatorStyle {
    /// A vertical line across the active zone.
    Line,
    /// Two triangles above and below the track, pointing at it.
    Arrow,
}

#[derive(Clone, Debug)]
pub struct ValidatedHeartZones {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
    pub zones: [HeartZoneRange; ZONE_COUNT],
    pub zone_colors: [String; ZONE_COUNT],
    pub zone_gap: f32,
    pub track_corner_radius: f32,
    pub indicator_style: HeartZonesIndicatorStyle,
    pub indicator_size: f32,
    pub indicator_color: String,
    pub show_label: bool,
    pub font: String,
    pub label_font_weight: f32,
    pub label_color: String,
}

pub fn validate_heart_zones(value: ValueConfig, index: usize) -> WidgetResult<ValidatedHeartZones> {
    let p = |field: &str| format!("values[{index}].{field}");
    if value.value != MetricKind::Heartrate || value.display_type != DisplayType::HeartZones {
        return Err(WidgetError::Config(format!(
            "values[{index}]: expected heartrate/heart_zones"
        )));
    }
    let width = require_positive_f32(value.width, &p("width"))?;
    let zone_gap =
        require_non_negative_f32(require_f32(value.zone_gap, &p("zone_gap"))?, &p("zone_gap"))?;
    if zone_gap * (ZONE_COUNT - 1) as f32 >= width {
        return Err(WidgetError::Config(format!(
            "{}: {} gaps must be narrower than width {width}",
            p("zone_gap"),
            ZONE_COUNT - 1
        )));
    }
    let zones = match require_string(value.zone_calculation, &p("zone_calculation"))?.as_str() {
        "lthr" => coggan_zones(require_positive_f32(value.lthr, &p("lthr"))?),
        "max_heartrate" => coggan_zones(lthr_from_max_heartrate(require_positive_f32(
            value.max_heartrate,
            &p("max_heartrate"),
        )?)),
        "manual" => validate_manual_zones(value.manual_zones, &p("manual_zones"))?,
        other => {
            return Err(WidgetError::Config(format!(
                "{}: invalid value '{other}'",
                p("zone_calculation")
            )))
        }
    };
    let zone_colors = value
        .zone_colors
        .ok_or_else(|| WidgetError::Config(format!("{}: required", p("zone_colors"))))?;
    let zone_colors: [String; ZONE_COUNT] = zone_colors
        .iter()
        .enumerate()
        .map(|(i, color)| require_hex_color(Some(color), &p(&format!("zone_colors[{i}]"))))
        .collect::<WidgetResult<Vec<_>>>()?
        .try_into()
        .map_err(|_| {
            WidgetError::Config(format!(
                "{}: expected {ZONE_COUNT} colors",
                p("zone_colors")
            ))
        })?;
    let indicator_style =
        match require_string(value.indicator_style, &p("indicator_style"))?.as_str() {
            "line" => HeartZonesIndicatorStyle::Line,
            "arrow" => HeartZonesIndicatorStyle::Arrow,
            other => {
                return Err(WidgetError::Config(format!(
                    "{}: invalid value '{other}'",
                    p("indicator_style")
                )))
            }
        };
    let font = require_string(value.font, &p("font"))?;
    if font.trim().is_empty() {
        return Err(WidgetError::Config(format!("{}: required", p("font"))));
    }
    let color = |raw: Option<String>, field: &str| require_hex_color(raw.as_deref(), &p(field));
    Ok(ValidatedHeartZones {
        x: require_f32(Some(value.x), &p("x"))?,
        y: require_f32(Some(value.y), &p("y"))?,
        width,
        height: require_positive_f32(value.height, &p("height"))?,
        opacity: require_unit_opacity(value.opacity, &p("opacity"))?,
        zones,
        zone_colors,
        zone_gap,
        track_corner_radius: require_non_negative_f32(
            require_f32(value.track_corner_radius, &p("track_corner_radius"))?,
            &p("track_corner_radius"),
        )?,
        indicator_style,
        indicator_size: require_positive_f32(value.indicator_size, &p("indicator_size"))?,
        indicator_color: color(value.indicator_color, "indicator_color")?,
        show_label: require_bool(value.show_label, &p("show_label"))?,
        font,
        label_font_weight: require_positive_f32(value.label_font_weight, &p("label_font_weight"))?,
        label_color: color(value.label_color, "label_color")?,
    })
}

/// Manual zones must be five ascending, non-overlapping spans.
fn validate_manual_zones(
    zones: Option<Vec<HeartZoneRange>>,
    field: &str,
) -> WidgetResult<[HeartZoneRange; ZONE_COUNT]> {
    let zones: [HeartZoneRange; ZONE_COUNT] = zones
        .ok_or_else(|| WidgetError::Config(format!("{field}: required")))?
        .try_into()
        .map_err(|_| WidgetError::Config(format!("{field}: expected {ZONE_COUNT} zones")))?;
    for (index, zone) in zones.iter().enumerate() {
        if !zone.from.is_finite() || !zone.to.is_finite() || zone.from < 0.0 || zone.from >= zone.to
        {
            return Err(WidgetError::Config(format!(
                "{field}[{index}]: requires 0 <= from < to, got {}..{}",
                zone.from, zone.to
            )));
        }
        if index > 0 && zone.from < zones[index - 1].to {
            return Err(WidgetError::Config(format!(
                "{field}[{index}]: must start at or above the previous zone's end {}",
                zones[index - 1].to
            )));
        }
    }
    Ok(zones)
}
