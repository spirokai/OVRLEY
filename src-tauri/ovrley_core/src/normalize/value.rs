//! Value widget validation.
//!
//! `validate_value_widget` verifies that every output-affecting field is
//! already explicit. Missing fields are rejected — the backend owns zero
//! render-affecting defaults. The frontend must materialise all defaults
//! before sending the config to the backend.
//!
//! Shadow, border, and other scene-level properties are NOT part of the
//! value contract — they belong to the scene validation contract.

use super::helpers::{require_bool, require_f32, require_str, require_string, rgba_from_hex};
use super::raw::ValueConfig;
use crate::error::{CoreError, CoreResult};
use crate::standard_metrics::is_standard_metric;
use crate::types::DisplayType;
use crate::MetricKind;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ValidatedValueWidget — zero backend-side defaults
// ---------------------------------------------------------------------------

/// Horizontal point of an intrinsic metric row attached to the configured x coordinate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentAlignment {
    Left,
    Center,
    Right,
}

pub(super) fn validate_content_alignment(
    content_alignment: Option<String>,
    path: &str,
) -> CoreResult<ContentAlignment> {
    match content_alignment.as_deref() {
        Some("left") => Ok(ContentAlignment::Left),
        Some("center") => Ok(ContentAlignment::Center),
        Some("right") => Ok(ContentAlignment::Right),
        Some(value) => Err(CoreError::Config(format!(
            "{path}: expected 'left', 'center', or 'right', got '{value}'"
        ))),
        None => Err(CoreError::Config(format!("{path}: required"))),
    }
}

/// Explicit formatting contract for the standard-metric text/value slice.
#[derive(Clone, Debug)]
pub enum ValidatedValueFormatting {
    DecimalPlaces {
        decimals: usize,
    },
    DecimalRounding {
        decimal_rounding: i32,
    },
    Balance {
        decimals: usize,
        balance_format: String,
    },
    BalanceRounded {
        decimal_rounding: i32,
        balance_format: String,
    },
}

impl ValidatedValueFormatting {
    /// Returns the balance format string if this is a balance variant.
    pub fn balance_format(&self) -> Option<&str> {
        match self {
            Self::Balance { balance_format, .. } | Self::BalanceRounded { balance_format, .. } => {
                Some(balance_format)
            }
            _ => None,
        }
    }
}

/// Every output-affecting field for the standard-metric text/value slice is explicit.
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
    pub formatting: ValidatedValueFormatting,
    pub hours_offset: Option<i64>,
    pub format: Option<String>,
}

pub(super) fn normalize_starting_altitude_m(
    metric: MetricKind,
    starting_altitude: Option<f64>,
    display_unit: Option<&str>,
    altitude_path: &str,
    unit_path: &str,
) -> CoreResult<Option<f64>> {
    if metric != MetricKind::Altitude {
        if starting_altitude.is_some() {
            return Err(CoreError::Config(format!(
                "{altitude_path}: is only valid for altitude widgets"
            )));
        }
        return Ok(None);
    }

    let display_unit =
        display_unit.ok_or_else(|| CoreError::Config(format!("{unit_path}: required")))?;

    let meter_scale = match display_unit {
        "m" => 1.0,
        "ft" => 3.280_84,
        unit => {
            return Err(CoreError::Config(format!(
                "{unit_path}: expected 'm' or 'ft', got '{unit}'"
            )))
        }
    };

    Ok(starting_altitude.map(|altitude| altitude / meter_scale))
}

// ---------------------------------------------------------------------------
// Validation — every output-affecting field must be explicit
// ---------------------------------------------------------------------------

pub fn validate_value_widget(value: ValueConfig, index: usize) -> CoreResult<ValidatedValueWidget> {
    let p = |f: &str| format!("values[{index}].{f}");

    if value.display_type != DisplayType::Text {
        return Err(CoreError::Config(format!(
            "{}: display_type '{}' is outside the standard metric text/value validation slice",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    validate_value_widget_fields(value, index, true, true)
}

/// Validates the text fields used inside a boxed arc gauge.
///
/// Arc gauges deliberately do not render icons, so icon configuration is not
/// required for this path. All fields which affect the inner value or unit are
/// still validated explicitly before drawing.
pub(super) fn validate_arc_inner_value_widget(
    value: ValueConfig,
    index: usize,
) -> CoreResult<ValidatedValueWidget> {
    validate_value_widget_fields(value, index, false, false)
}

fn validate_value_widget_fields(
    value: ValueConfig,
    index: usize,
    require_icon_fields: bool,
    require_alignment: bool,
) -> CoreResult<ValidatedValueWidget> {
    let p = |f: &str| format!("values[{index}].{f}");

    if !is_standard_metric(value.value) {
        return Err(CoreError::Config(format!(
            "{}: metric {:?} is outside the standard metric text/value validation slice",
            p("value"),
            value.value
        )));
    }
    if !require_icon_fields && value.value == MetricKind::GpsCoordinates {
        return Err(CoreError::Config(format!(
            "{}: gps_coordinates only supports text display",
            p("value")
        )));
    }

    let font_name = require_string(value.font, &p("font"))?;
    let content_alignment = if require_alignment {
        validate_content_alignment(value.content_alignment, &p("content_alignment"))?
    } else {
        ContentAlignment::Left
    };

    // -- opacity ----------------------------------------------------------
    let opacity = require_f32(value.opacity, &p("opacity"))?;
    if !(0.0..=1.0).contains(&opacity) {
        return Err(CoreError::Config(format!(
            "{}: must be 0.0..1.0, got {opacity}",
            p("opacity")
        )));
    }

    let font_size = require_f32(value.font_size, &p("font_size"))?;
    if font_size <= 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be > 0, got {font_size}",
            p("font_size")
        )));
    }

    // -- colour -----------------------------------------------------------
    let colour_hex = require_str(value.color.as_deref(), &p("color"))?;
    let color = rgba_from_hex(colour_hex, &p("color"), opacity)?;

    // -- icon --------------------------------------------------------------
    // Arc gauges have no icon. Keep the validated struct shape shared with
    // normal metric formatting, while avoiding needless required fields for a
    // visual element that this presentation never draws.
    let (show_icon, icon_color, icon_size, icon_offset_x, icon_offset_y) = if require_icon_fields {
        let show_icon = require_bool(value.show_icon, &p("show_icon"))?;
        let icon_color = rgba_from_hex(
            require_str(value.icon_color.as_deref(), &p("icon_color"))?,
            &p("icon_color"),
            opacity,
        )?;
        let icon_size = require_f32(value.icon_size, &p("icon_size"))?;
        if icon_size < 0.0 {
            return Err(CoreError::Config(format!(
                "{}: must be >= 0, got {icon_size}",
                p("icon_size")
            )));
        }
        let icon_offset_x = require_f32(value.icon_offset_x, &p("icon_offset_x"))?;
        let icon_offset_y = require_f32(value.icon_offset_y, &p("icon_offset_y"))?;
        (
            show_icon,
            icon_color,
            icon_size,
            icon_offset_x,
            icon_offset_y,
        )
    } else {
        (false, [0, 0, 0, 0], 0.0, 0.0, 0.0)
    };

    // -- units -- all explicit --------------------------------------------
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
            return Err(CoreError::Config(format!(
                "{}: is only valid for total_ascent widgets",
                p("show_full_ascent")
            )));
        }
        None
    };
    let coordinate_format = if value.value == MetricKind::GpsCoordinates {
        let coordinate_format = require_string(value.coordinate_format, &p("coordinate_format"))?;
        if !matches!(coordinate_format.as_str(), "dms" | "ddm") {
            return Err(CoreError::Config(format!(
                "{}: expected 'dms' or 'ddm', got '{coordinate_format}'",
                p("coordinate_format")
            )));
        }
        Some(coordinate_format)
    } else {
        if value.coordinate_format.is_some() {
            return Err(CoreError::Config(format!(
                "{}: is only valid for gps_coordinates widgets",
                p("coordinate_format")
            )));
        }
        None
    };

    let unit_color = rgba_from_hex(
        require_str(value.unit_color.as_deref(), &p("unit_color"))?,
        &p("unit_color"),
        opacity,
    )?;
    let display_unit = require_string(value.display_unit, &p("display_unit"))?;
    if value.value == MetricKind::GpsCoordinates
        && !matches!(display_unit.as_str(), "latitude" | "longitude" | "both")
    {
        return Err(CoreError::Config(format!(
            "{}: expected 'latitude', 'longitude', or 'both', got '{display_unit}'",
            p("display_unit")
        )));
    }
    if value.value == MetricKind::TotalAscent && !matches!(display_unit.as_str(), "m" | "ft") {
        return Err(CoreError::Config(format!(
            "{}: expected 'm' or 'ft', got '{display_unit}'",
            p("display_unit")
        )));
    }
    if value.value == MetricKind::DistanceToHome
        && !matches!(display_unit.as_str(), "m" | "km" | "mi" | "ft")
    {
        return Err(CoreError::Config(format!(
            "{}: expected 'm', 'km', 'mi', or 'ft', got '{display_unit}'",
            p("display_unit")
        )));
    }
    if value.value == MetricKind::Calories && display_unit != "kcal" {
        return Err(CoreError::Config(format!(
            "{}: expected 'kcal', got '{display_unit}'",
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

    // -- affixes are output-affecting and must be explicit ----------------
    let prefix = require_string(value.prefix, &p("prefix"))?;
    let suffix = require_string(value.suffix, &p("suffix"))?;

    // -- formatting must be explicit, not inferred later ------------------
    let formatting = if value.value == MetricKind::LeftRightBalance {
        match (value.decimals, value.decimal_rounding) {
            (Some(decimals), None) => ValidatedValueFormatting::Balance {
                decimals,
                balance_format: require_string(value.balance_format, &p("balance_format"))?,
            },
            (None, Some(decimal_rounding)) => ValidatedValueFormatting::BalanceRounded {
                decimal_rounding,
                balance_format: require_string(value.balance_format, &p("balance_format"))?,
            },
            (Some(_), Some(_)) => {
                return Err(CoreError::Config(format!(
                    "{} and {}: provide exactly one precision field for balance widgets",
                    p("decimals"),
                    p("decimal_rounding")
                )));
            }
            (None, None) => {
                return Err(CoreError::Config(format!(
                    "{} or {}: one precision field must be explicit",
                    p("decimals"),
                    p("decimal_rounding")
                )));
            }
        }
    } else {
        match (value.decimals, value.decimal_rounding) {
            (Some(decimals), None) => ValidatedValueFormatting::DecimalPlaces { decimals },
            (None, Some(decimal_rounding)) => {
                ValidatedValueFormatting::DecimalRounding { decimal_rounding }
            }
            (Some(_), Some(_)) => {
                return Err(CoreError::Config(format!(
                    "{} and {}: provide exactly one precision field",
                    p("decimals"),
                    p("decimal_rounding")
                )));
            }
            (None, None) => {
                return Err(CoreError::Config(format!(
                    "{} or {}: one precision field must be explicit",
                    p("decimals"),
                    p("decimal_rounding")
                )));
            }
        }
    };

    if value.value == MetricKind::Distance {
        let decimals = match &formatting {
            ValidatedValueFormatting::DecimalPlaces { decimals } => *decimals,
            ValidatedValueFormatting::DecimalRounding { decimal_rounding } => {
                (*decimal_rounding).max(0) as usize
            }
            ValidatedValueFormatting::Balance { .. }
            | ValidatedValueFormatting::BalanceRounded { .. } => 0,
        };
        if decimals > 2 {
            return Err(CoreError::Config(format!(
                "{}: distance supports only 0, 1, or 2 decimals, got {decimals}",
                p("decimals")
            )));
        }
    }

    Ok(ValidatedValueWidget {
        metric: value.value,
        x: value.x,
        y: value.y,
        display_type: value.display_type,
        content_alignment,
        font_name,
        font_size,
        color,
        opacity,
        show_icon,
        icon_color,
        icon_size,
        icon_offset_x,
        icon_offset_y,
        show_units,
        show_full_distance,
        show_full_ascent,
        coordinate_format,
        unit_color,
        display_unit,
        starting_altitude_m,
        prefix,
        suffix,
        formatting,
        hours_offset: value.hours_offset.map(i64::from),
        format: value.format,
    })
}
