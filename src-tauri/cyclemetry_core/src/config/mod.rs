use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SceneConfig {
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    pub fps: f64,
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub font_size: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub decimal_rounding: Option<i32>,
    #[serde(default)]
    pub overlay_filename: Option<String>,
    #[serde(default)]
    pub ffmpeg: Value,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub scale: Option<f32>,
    #[serde(default)]
    pub time_format: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelConfig {
    #[serde(default)]
    pub text: String,
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
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ValueConfig {
    pub value: String,
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
    pub speed_unit: Option<String>,
    #[serde(default)]
    pub temperature_unit: Option<String>,
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
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenderConfig {
    pub scene: SceneConfig,
    #[serde(default)]
    pub labels: Vec<LabelConfig>,
    #[serde(default)]
    pub values: Vec<ValueConfig>,
    #[serde(default)]
    pub plots: Value,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderDataRequirements {
    pub speed: bool,
    pub elevation: bool,
    pub gradient: bool,
    pub heartrate: bool,
    pub cadence: bool,
    pub power: bool,
    pub temperature: bool,
    pub time: bool,
    pub distance_progress: bool,
    pub course: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct MarkerPointConfig {
    #[serde(default)]
    pub weight: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct LineStyleConfig {
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct FillStyleConfig {
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct PointLabelConfig {
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub font_size: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub x_offset: Option<f32>,
    #[serde(default)]
    pub y_offset: Option<f32>,
    #[serde(default)]
    pub units: Vec<String>,
    #[serde(default)]
    pub decimal_rounding: Option<i32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CoursePlotConfig {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub rotation: f32,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub margin: Option<f32>,
    #[serde(default)]
    pub completed_line_width: Option<f32>,
    #[serde(default)]
    pub completed_line_color: Option<String>,
    #[serde(default)]
    pub completed_line_opacity: Option<f32>,
    #[serde(default)]
    pub remaining_line_width: Option<f32>,
    #[serde(default)]
    pub remaining_line_color: Option<String>,
    #[serde(default)]
    pub remaining_line_opacity: Option<f32>,
    #[serde(default)]
    pub marker_size: Option<f32>,
    #[serde(default)]
    pub marker_color: Option<String>,
    #[serde(default)]
    pub marker_opacity: Option<f32>,
    #[serde(default)]
    pub line: Option<LineStyleConfig>,
    #[serde(default)]
    pub points: Vec<MarkerPointConfig>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ElevationPlotConfig {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub rotation: f32,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub margin: Option<f32>,
    #[serde(default)]
    pub completed_line_width: Option<f32>,
    #[serde(default)]
    pub completed_line_color: Option<String>,
    #[serde(default)]
    pub completed_line_opacity: Option<f32>,
    #[serde(default)]
    pub remaining_line_width: Option<f32>,
    #[serde(default)]
    pub remaining_line_color: Option<String>,
    #[serde(default)]
    pub remaining_line_opacity: Option<f32>,
    #[serde(default)]
    pub marker_size: Option<f32>,
    #[serde(default)]
    pub marker_color: Option<String>,
    #[serde(default)]
    pub marker_opacity: Option<f32>,
    #[serde(default)]
    pub area_completed_color: Option<String>,
    #[serde(default)]
    pub area_completed_opacity: Option<f32>,
    #[serde(default)]
    pub area_remaining_color: Option<String>,
    #[serde(default)]
    pub area_remaining_opacity: Option<f32>,
    #[serde(default)]
    pub show_elevation_metric: Option<bool>,
    #[serde(default)]
    pub show_elevation_imperial: Option<bool>,
    #[serde(default)]
    pub y_scale: Option<f32>,
    #[serde(default)]
    pub simplify_tolerance_px: Option<f32>,
    #[serde(default)]
    pub target_density: Option<f32>,
    #[serde(default)]
    pub metric_label_offset_x: Option<f32>,
    #[serde(default)]
    pub metric_label_offset_y: Option<f32>,
    #[serde(default)]
    pub imperial_label_offset_x: Option<f32>,
    #[serde(default)]
    pub imperial_label_offset_y: Option<f32>,
    #[serde(default)]
    pub line: Option<LineStyleConfig>,
    #[serde(default)]
    pub fill: Option<FillStyleConfig>,
    #[serde(default)]
    pub points: Vec<MarkerPointConfig>,
    #[serde(default)]
    pub point_label: Option<PointLabelConfig>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

pub fn parse_config_json(input: &str) -> Result<RenderConfig, String> {
    let config: RenderConfig =
        serde_json::from_str(input).map_err(|error| format!("Invalid config JSON: {error}"))?;
    if config.scene.fps <= 0.0 {
        return Err(format!("Invalid scene.fps: {}", config.scene.fps));
    }
    if config.scene.end <= config.scene.start {
        return Err(format!(
            "Invalid scene range. scene.end ({}) must be greater than scene.start ({})",
            config.scene.end, config.scene.start
        ));
    }
    Ok(config)
}

impl RenderConfig {
    pub fn render_data_requirements(&self) -> Result<RenderDataRequirements, String> {
        let mut requirements = RenderDataRequirements::default();

        for value in &self.values {
            match value.value.as_str() {
                "speed" => requirements.speed = true,
                "elevation" => requirements.elevation = true,
                "gradient" => requirements.gradient = true,
                "heartrate" => requirements.heartrate = true,
                "cadence" => requirements.cadence = true,
                "power" => requirements.power = true,
                "temperature" => requirements.temperature = true,
                "time" => requirements.time = true,
                _ => {}
            }
        }

        if self.course_plot()?.is_some() {
            requirements.distance_progress = true;
        }

        if self.elevation_plot()?.is_some() {
            requirements.elevation = true;
            requirements.distance_progress = true;
        }

        Ok(requirements)
    }

    pub fn course_plot(&self) -> Result<Option<CoursePlotConfig>, String> {
        self.parse_plot("course")
    }

    pub fn elevation_plot(&self) -> Result<Option<ElevationPlotConfig>, String> {
        self.parse_plot("elevation")
    }

    fn parse_plot<T>(&self, value_key: &str) -> Result<Option<T>, String>
    where
        T: for<'de> Deserialize<'de>,
    {
        let Some(raw_plot) = find_plot_value(&self.plots, value_key) else {
            return Ok(None);
        };
        serde_json::from_value(raw_plot.clone())
            .map(Some)
            .map_err(|error| format!("Invalid {value_key} plot config: {error}"))
    }
}

fn find_plot_value<'a>(plots: &'a Value, value_key: &str) -> Option<&'a Value> {
    match plots {
        Value::Array(items) => items.iter().find(|item| {
            item.get("value")
                .and_then(Value::as_str)
                .map(|value| value == value_key)
                .unwrap_or(false)
        }),
        Value::Object(map) => map.get(value_key).or_else(|| {
            map.values().find(|item| {
                item.get("value")
                    .and_then(Value::as_str)
                    .map(|value| value == value_key)
                    .unwrap_or(false)
            })
        }),
        _ => None,
    }
}
