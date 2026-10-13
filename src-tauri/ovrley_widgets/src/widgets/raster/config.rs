use serde::{Deserialize, Serialize};

/// Complete template render configuration.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RasterConfig {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    pub opacity: f64,
    #[serde(deserialize_with = "deserialize_raster_path")]
    pub path: Option<String>,
    #[serde(
        default,
        rename = "resourceId",
        deserialize_with = "deserialize_present_resource",
        skip_serializing_if = "Option::is_none"
    )]
    pub resource_id: Option<String>,
    #[serde(
        default,
        rename = "resourceErrorCode",
        deserialize_with = "deserialize_present_resource",
        skip_serializing_if = "Option::is_none"
    )]
    pub resource_error_code: Option<super::error::RasterError>,
}

fn deserialize_present_resource<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

fn deserialize_raster_path<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}
