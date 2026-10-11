/// Display content used by icon+value+unit widgets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetricDisplayContent {
    /// Main metric/time text and optional unit suffix.
    Standard {
        value_text: String,
        unit_text: Option<String>,
    },
    /// Coordinate lines with independently colored direction letters.
    Coordinates(MetricCoordinateDisplay),
}

/// Formatted metric content plus its shared icon configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricDisplayParts {
    pub content: MetricDisplayContent,
    pub show_icon: bool,
    pub icon_asset_file: Option<&'static str>,
}

impl MetricDisplayParts {
    /// Returns standard value/unit text. Coordinate content is invalid here.
    pub fn standard_text(&self) -> (&str, Option<&str>) {
        match &self.content {
            MetricDisplayContent::Standard {
                value_text,
                unit_text,
            } => (value_text, unit_text.as_deref()),
            MetricDisplayContent::Coordinates(_) => {
                panic!("coordinate display content has no standard value/unit text")
            }
        }
    }
}

/// One formatted latitude or longitude line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricCoordinateLine {
    pub direction: Option<String>,
    pub value_text: String,
}

/// Coordinate display content. Two lines are used for the `both` unit mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricCoordinateDisplay {
    pub lines: Vec<MetricCoordinateLine>,
}

pub fn format_number(value: f64, decimals: usize) -> String {
    if decimals == 0 {
        let rounded = value.round();
        return if rounded == 0.0 {
            "0".to_string()
        } else {
            rounded.to_string()
        };
    }

    let factor = 10_f64.powi(decimals as i32);
    let rounded = (value * factor).round() / factor;
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    format!("{rounded:.decimals$}")
}

/// Speed formatting shared by native frame samples and on-demand WASM samples.
pub fn format_speed_parts(
    widget: &crate::widgets::value::validation::ValidatedValueWidget,
    speed: Option<f64>,
) -> MetricDisplayParts {
    use crate::widgets::value::validation::ValidatedValueFormatting;
    let decimals = match widget.formatting {
        ValidatedValueFormatting::DecimalPlaces { decimals } => decimals,
        ValidatedValueFormatting::DecimalRounding { decimal_rounding } => {
            decimal_rounding.max(0) as usize
        }
        _ => unreachable!("speed has decimal precision"),
    };
    let factor = match widget.display_unit.as_str() {
        "kmh" => 3.6,
        "mph" => 2.23694,
        "kn" => 1.943844,
        "mps" => 1.0,
        _ => unreachable!("speed unit validated at ingress"),
    };
    let value = speed
        .map(|v| format_number(v * factor, decimals))
        .unwrap_or_else(|| "--".into());
    MetricDisplayParts {
        content: MetricDisplayContent::Standard {
            value_text: format!("{}{}{}", widget.prefix, value, widget.suffix),
            unit_text: widget.show_units.then(|| {
                crate::model::standard_metrics::standard_metric_unit_label(
                    crate::model::types::MetricKind::Speed,
                    Some(&widget.display_unit),
                )
                .to_owned()
            }),
        },
        show_icon: widget.show_icon,
        icon_asset_file: crate::render::icons::metric_icon_asset_file(
            crate::model::types::MetricKind::Speed,
        ),
    }
}
