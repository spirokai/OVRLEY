use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricKind {
    #[serde(rename = "speed")]
    Speed,
    #[serde(rename = "heartrate")]
    Heartrate,
    #[serde(rename = "elevation")]
    Elevation,
    #[serde(rename = "time")]
    Time,
    #[serde(rename = "gradient")]
    Gradient,
    #[serde(rename = "cadence")]
    Cadence,
    #[serde(rename = "power")]
    Power,
    #[serde(rename = "temperature")]
    Temperature,
}
