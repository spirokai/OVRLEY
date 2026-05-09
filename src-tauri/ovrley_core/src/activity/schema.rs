use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub type NumericSeries = Vec<Option<f64>>;
pub type TimeSeries = Vec<Option<String>>;
pub type CourseSeries = Vec<(Option<f64>, Option<f64>)>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ParsedActivity {
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub file_format: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub source_start_time: Option<String>,
    #[serde(default)]
    pub sample_elapsed_seconds: Vec<f64>,
    #[serde(default)]
    pub sample_distance_progress: Vec<f64>,
    #[serde(default)]
    pub frame_elapsed_seconds: Vec<f64>,
    #[serde(default)]
    pub frame_timestamps: Vec<Option<String>>,
    #[serde(default)]
    pub frame_distance_progress: Vec<Option<f64>>,
    #[serde(default)]
    pub trim_start_seconds: f64,
    #[serde(default)]
    pub trim_end_seconds: f64,
    #[serde(default)]
    pub sample_course_points: CourseSeries,
    #[serde(default)]
    pub sample_elevations: NumericSeries,
    #[serde(default)]
    pub course: CourseSeries,
    #[serde(default)]
    pub elevation: NumericSeries,
    #[serde(default)]
    pub speed: NumericSeries,
    #[serde(default)]
    pub heartrate: NumericSeries,
    #[serde(default)]
    pub cadence: NumericSeries,
    #[serde(default)]
    pub power: NumericSeries,
    #[serde(default)]
    pub temperature: NumericSeries,
    #[serde(default)]
    pub gradient: NumericSeries,
    #[serde(default)]
    pub time: TimeSeries,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DebugPayload {
    pub parsed_activity: ParsedActivity,
}

#[derive(Clone, Debug, Serialize)]
pub struct DenseActivityReport {
    pub frame_count: usize,
    pub frame_elapsed_seconds: Vec<f64>,
    pub frame_distance_progress: Vec<Option<f64>>,
    pub series: DenseSeriesReport,
}

#[derive(Clone, Debug, Serialize)]
pub struct DenseSeriesReport {
    pub speed: Vec<Option<f64>>,
    pub elevation: Vec<Option<f64>>,
    pub gradient: Vec<Option<f64>>,
    pub heartrate: Vec<Option<f64>>,
    pub cadence: Vec<Option<f64>>,
    pub power: Vec<Option<f64>>,
    pub temperature: Vec<Option<f64>>,
    pub course_lat: Vec<Option<f64>>,
    pub course_lon: Vec<Option<f64>>,
    pub time: Vec<Option<String>>,
}

#[derive(Clone, Debug)]
pub struct TrimmedActivity {
    pub source_start_time: Option<String>,
    pub sample_elapsed_seconds: Vec<f64>,
    pub sample_distance_progress: Vec<Option<f64>>,
    pub course: CourseSeries,
    pub elevation: NumericSeries,
    pub speed: NumericSeries,
    pub heartrate: NumericSeries,
    pub cadence: NumericSeries,
    pub power: NumericSeries,
    pub temperature: NumericSeries,
    pub gradient: NumericSeries,
    pub time: TimeSeries,
}
