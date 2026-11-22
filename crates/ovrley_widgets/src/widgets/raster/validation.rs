use super::{config::RasterConfig, error::valid_raster_id};
use crate::error::{WidgetError, WidgetResult};

#[derive(Clone, Debug)]
pub struct RasterGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub opacity: f32,
}

fn invalid_raster(id: &str, detail: String) -> WidgetError {
    WidgetError::Config(format!(
        "{detail} [raster_error:invalid_config:{}]",
        if id.is_empty() { "configuration" } else { id }
    ))
}

fn raster_number(id: &str, index: usize, name: &str, value: f64) -> WidgetResult<f32> {
    if !value.is_finite() || value.abs() > f32::MAX as f64 {
        return Err(invalid_raster(
            id,
            format!("rasters[{index}].{name} must be a finite number"),
        ));
    }
    Ok(value as f32)
}

pub fn validate_raster_geometry(raw: &RasterConfig, index: usize) -> WidgetResult<RasterGeometry> {
    if !valid_raster_id(&raw.id) {
        return Err(invalid_raster(
            &raw.id,
            format!(
                "rasters[{index}].id must contain only letters, digits, hyphens or underscores"
            ),
        ));
    }
    let width = raster_number(&raw.id, index, "width", raw.width)?;
    let height = raster_number(&raw.id, index, "height", raw.height)?;
    let opacity = raster_number(&raw.id, index, "opacity", raw.opacity)?;
    if width <= 0.0 || height <= 0.0 {
        return Err(invalid_raster(
            &raw.id,
            format!("Raster {} must have a positive frame size", raw.id),
        ));
    }
    if !(0.0..=1.0).contains(&raw.opacity) {
        return Err(invalid_raster(
            &raw.id,
            format!("Raster {} opacity must be between 0 and 1", raw.id),
        ));
    }
    Ok(RasterGeometry {
        x: raster_number(&raw.id, index, "x", raw.x)?,
        y: raster_number(&raw.id, index, "y", raw.y)?,
        width,
        height,
        rotation: raster_number(&raw.id, index, "rotation", raw.rotation)?,
        opacity,
    })
}
