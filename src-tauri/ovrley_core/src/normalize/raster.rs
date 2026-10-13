//! Strict render-submission contract for static raster widgets.

use ovrley_widgets::widgets::raster::config::RasterConfig;
use crate::error::{CoreError, CoreResult};
use crate::raster::{ RasterResourceResolver, SelectedRaster};
use ovrley_widgets::widgets::raster::validation::{RasterGeometry, validate_raster_geometry};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ValidatedRaster {
    pub id: String,
    pub geometry: RasterGeometry,
    pub source: RasterSource,
}

/// Render preparation follows this explicit choice; a session image is never
/// replaced by reopening the original path.
#[derive(Clone, Debug)]
pub enum RasterSource {
    File(PathBuf),
    Image(Arc<SelectedRaster>),
}

struct ValidatedRasterDocument {
    geometry: RasterGeometry,
    path: Option<PathBuf>,
}

fn invalid_raster(id: &str, detail: String) -> CoreError {
    CoreError::Config(format!(
        "{detail} [raster_error:invalid_config:{}]",
        if id.is_empty() { "configuration" } else { id }
    ))
}

fn validate_raster_document(raw: &RasterConfig, index: usize) -> CoreResult<ValidatedRasterDocument> {
    let geometry = validate_raster_geometry(raw, index)?;
    let path = raw.path.as_ref().map(PathBuf::from);
    if path.as_ref().is_some_and(|path| !path.is_absolute()) {
        return Err(CoreError::Config(format!("Raster {} requires an absolute image path [raster_error:relative_path:{}]", raw.id, raw.id)));
    }
    Ok(ValidatedRasterDocument { geometry, path })
}

pub(crate) fn validate_unique_raster_ids(rasters: &[RasterConfig]) -> CoreResult<()> {
    let mut seen = HashSet::new();
    for raster in rasters {
        if !seen.insert(&raster.id) {
            return Err(invalid_raster(
                &raster.id,
                format!("Raster {} id is duplicated", raster.id),
            ));
        }
    }
    Ok(())
}

/// Validates persisted raster widgets without requiring an image for placeholders.
/// Returns the populated widget IDs for document-bound asset resolution.
pub fn validate_template_rasters(rasters: &[RasterConfig]) -> CoreResult<HashSet<String>> {
    validate_unique_raster_ids(rasters)?;
    let mut populated = HashSet::new();
    for (index, raster) in rasters.iter().enumerate() {
        validate_template_raster(raster, index)?;
        if raster.path.is_some() {
            populated.insert(raster.id.clone());
        }
    }
    Ok(populated)
}

fn validate_template_raster(raw: &RasterConfig, index: usize) -> CoreResult<()> {
    if raw.resource_id.is_some() || raw.resource_error_code.is_some() {
        return Err(invalid_raster(
            &raw.id,
            "Template raster cannot contain runtime resource state".into(),
        ));
    }
    validate_raster_document(raw, index)?;
    Ok(())
}

pub(crate) fn validate_raster(
    raw: &RasterConfig,
    index: usize,
    resources: Option<&dyn RasterResourceResolver>,
) -> CoreResult<ValidatedRaster> {
    let document = validate_raster_document(raw, index)?;
    let path = document.path.ok_or_else(|| {
        CoreError::Config(format!("Raster {} has no selected image. Select one before rendering. [raster_error:no_image:{}]", raw.id, raw.id))
    })?;
    if raw.resource_id.is_some() && raw.resource_error_code.is_some() {
        return Err(invalid_raster(
            &raw.id,
            "Raster has conflicting resource state".into(),
        ));
    }
    if let Some(error) = raw.resource_error_code {
        return Err(error.for_widget(&raw.id).into());
    }
    let missing_resource = || {
        CoreError::Config(format!(
            "Raster {} has no loaded image resource [raster_error:missing_resource:{}]",
            raw.id, raw.id
        ))
    };
    if raw.resource_id.as_deref() == Some("") {
        return Err(invalid_raster(
            &raw.id,
            "Raster resourceId must not be empty".into(),
        ));
    }
    let source = match (resources, raw.resource_id.as_deref()) {
        (Some(resources), Some(id)) => {
            RasterSource::Image(resources.resolve(id).ok_or_else(missing_resource)?)
        }
        (None, None) => RasterSource::File(path),
        _ => return Err(missing_resource()),
    };
    Ok(ValidatedRaster {
        id: raw.id.clone(),
        geometry: document.geometry,
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn raster() -> serde_json::Value {
        json!({
            "id": "raster-1", "x": 2, "y": 3, "width": 4, "height": 5,
            "rotation": 0, "opacity": 1, "path": std::env::current_dir().unwrap().join("gone.png")
        })
    }

    #[test]
    fn rejects_missing_fields_and_malformed_present_values() {
        let mut missing = raster();
        missing.as_object_mut().unwrap().remove("path");
        assert!(serde_json::from_value::<RasterConfig>(missing).is_err());
        let render_config = json!({
            "scene": { "fps": 30, "start": 0, "end": 1 },
            "rasters": [raster()]
        });
        let mut malformed_render_config = render_config;
        malformed_render_config["rasters"][0]["width"] = json!("4");
        assert!(
            serde_json::from_value::<super::super::raw::RenderConfig>(malformed_render_config)
                .unwrap_err()
                .to_string()
                .contains("raster_error:invalid_config:configuration")
        );

        let mut malformed = raster();
        malformed["width"] = json!("4");
        assert!(serde_json::from_value::<RasterConfig>(malformed).is_err());

        let mut out_of_range = raster();
        out_of_range["opacity"] = json!(1.1);
        let parsed: RasterConfig = serde_json::from_value(out_of_range).unwrap();
        assert!(validate_raster(&parsed, 0, None).is_err());
    }

    #[test]
    fn rejects_unresolved_raster_before_rendering() {
        let mut unresolved = raster();
        unresolved["path"] = serde_json::Value::Null;
        let parsed: RasterConfig = serde_json::from_value(unresolved).unwrap();
        assert!(validate_raster(&parsed, 0, None)
            .unwrap_err()
            .to_string()
            .contains("raster-1 has no selected image"));
    }

    #[test]
    fn runtime_resource_fields_are_typed_and_cannot_be_persisted() {
        for (field, value) in [
            ("resourceId", json!(null)),
            ("resourceErrorCode", json!(null)),
            ("resourceErrorCode", json!("unknown_error")),
        ] {
            let mut value_with_resource = raster();
            value_with_resource[field] = value;
            assert!(serde_json::from_value::<RasterConfig>(value_with_resource).is_err());
        }
        let mut value = raster();
        value["resourceId"] = json!("handle");
        let raw: RasterConfig = serde_json::from_value(value.clone()).unwrap();
        assert!(validate_template_raster(&raw, 0).is_err());
        value["resourceErrorCode"] = json!("missing_asset");
        let raw: RasterConfig = serde_json::from_value(value).unwrap();
        assert!(validate_raster(&raw, 0, None)
            .unwrap_err()
            .to_string()
            .contains("conflicting"));
    }

    #[test]
    fn session_submission_requires_a_resource_and_keeps_its_snapshot() {
        struct Resources(Arc<SelectedRaster>);
        impl RasterResourceResolver for Resources {
            fn resolve(&self, id: &str) -> Option<Arc<SelectedRaster>> {
                (id == "selected").then(|| Arc::clone(&self.0))
            }
        }
        let mut png = Vec::new();
        image::DynamicImage::new_rgba8(1, 1)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let resources = Resources(Arc::new(
            crate::raster::load_embedded_raster("png", png).unwrap(),
        ));
        let mut raw: RasterConfig = serde_json::from_value(raster()).unwrap();
        assert!(validate_raster(&raw, 0, Some(&resources))
            .unwrap_err()
            .to_string()
            .contains("missing_resource"));
        raw.resource_id = Some("selected".into());
        let validated = validate_raster(&raw, 0, Some(&resources)).unwrap();
        let RasterSource::Image(image) = validated.source else {
            panic!("expected session image")
        };
        assert!(Arc::ptr_eq(&image, &resources.0));
        raw.resource_id = Some("stale".into());
        assert!(validate_raster(&raw, 0, Some(&resources)).is_err());
        // A session handle can never silently become a standalone file source.
        assert!(validate_raster(&raw, 0, None).is_err());
    }

    #[test]
    fn opacity_range_is_checked_before_float_narrowing() {
        let mut value = raster();
        value["opacity"] = json!(1.00000001);
        let raw: RasterConfig = serde_json::from_value(value).unwrap();
        assert!(validate_template_raster(&raw, 0).is_err());
    }

    #[test]
    fn persisted_rasters_require_unique_ids_even_for_placeholders() {
        let mut raw: RasterConfig = serde_json::from_value(raster()).unwrap();
        raw.path = None;
        assert!(validate_template_rasters(&[raw.clone(), raw]).is_err());
    }
}
