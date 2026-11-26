//! Widget validation: every collection of a render config, including static rasters and their
//! image sources, into its validated, renderer-ready form.

use super::{RenderConfig, ValidatedRenderConfig};
use serde_json::Value;
use crate::error::{CoreError, CoreResult};
use ovrley_widgets::model::catalog::LayoutMode;
use ovrley_widgets::model::scene::ValidatedScenePresentation;
use ovrley_widgets::widgets::backdrop::validate_backdrop;
use ovrley_widgets::widgets::diving_profile::{
    validate_diving_profile, DivingProfileConfig, ValidatedDivingProfile,
};
use ovrley_widgets::widgets::elevation::{
    validate_elevation_plot, ElevationPlotConfig, ValidatedElevationPlot,
};
use ovrley_widgets::widgets::label::validate_label;
use ovrley_widgets::widgets::map::{validate_map_widget, MapConfig, ValidatedMapWidget};
use ovrley_widgets::widgets::rolling_chart::{
    validate_rolling_chart, RollingChartConfig, ValidatedRollingChart,
};
use ovrley_widgets::widgets::route::{validate_route_plot, CoursePlotConfig, ValidatedRoutePlot};
use ovrley_widgets::widgets::value::{ValidatedValue, ValueConfig, ValueFamily};
use crate::raster::{RasterResourceResolver, SelectedRaster};
use ovrley_widgets::widgets::raster::RasterConfig;
use ovrley_widgets::widgets::raster::{validate_raster_geometry, RasterGeometry};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

pub(super) fn validate_render_contents(
    raw: RenderConfig<ValidatedScenePresentation>,
    resources: Option<&dyn crate::raster::RasterResourceResolver>,
) -> CoreResult<ValidatedRenderConfig<ValidatedScenePresentation>> {
    let scene = raw.scene;
    validate_unique_raster_ids(&raw.rasters)?;
    let mut rasters = Vec::with_capacity(raw.rasters.len());
    for (index, raster) in raw.rasters.iter().enumerate() {
        rasters.push(validate_raster(raster, index, resources)?);
    }
    let mut backdrops = Vec::with_capacity(raw.backdrops.len());
    for (index, backdrop) in raw.backdrops.iter().enumerate() {
        backdrops.push(validate_backdrop(backdrop, index)?);
    }
    let mut values = Vec::with_capacity(raw.values.len());
    for (index, value) in raw.values.into_iter().enumerate() {
        values.push(validate_render_value(value, index, &scene)?);
    }
    let mut labels = Vec::with_capacity(raw.labels.len());
    for (index, label) in raw.labels.iter().enumerate() {
        labels.push(validate_label(label, index)?);
    }
    Ok(ValidatedRenderConfig {
        course_plots: validate_route_plots(&raw.plots)?,
        maps: validate_map_widgets(&raw.plots)?,
        elevation_plots: validate_elevation_plots(&raw.plots, &scene)?,
        diving_profiles: validate_diving_profiles(&raw.plots, &scene)?,
        rolling_charts: validate_rolling_charts(&raw.plots)?,
        scene,
        backdrops,
        rasters,
        labels,
        values,
    })
}

fn validate_render_value(
    mut value: ValueConfig,
    index: usize,
    scene: &ValidatedScenePresentation,
) -> CoreResult<ValidatedValue> {
    if value.display_type.layout_mode() == LayoutMode::Boxed {
        value = promote_value_variant(&value, value.display_type.as_str())?;
    }
    // Templates saved before heading tapes had their own label font use the scene's.
    if ValueFamily::of(value.value, value.display_type) == ValueFamily::HeadingTape
        && value.label_font.is_none()
    {
        value.label_font = scene.font.clone();
    }
    Ok(ValidatedValue::validate(value, index)?)
}

pub fn validate_route_plots(plots: &serde_json::Value) -> CoreResult<Vec<ValidatedRoutePlot>> {
    let mut validated = Vec::new();
    for (index, value) in find_plot_values(plots, "course") {
        let plot: CoursePlotConfig =
            serde_json::from_value(value.clone()).map_err(course_config_error)?;
        validated.push(validate_route_plot(&plot, index)?);
    }
    Ok(validated)
}

fn course_config_error(error: serde_json::Error) -> CoreError {
    CoreError::Config(format!("course plot config: {error}"))
}

fn validate_map_widgets(plots: &serde_json::Value) -> CoreResult<Vec<ValidatedMapWidget>> {
    find_plot_values(plots, "map")
        .into_iter()
        .map(|(index, value)| {
            let map: MapConfig = serde_json::from_value(value.clone())
                .map_err(|error| CoreError::Config(format!("map config: {error}")))?;
            Ok(validate_map_widget(&map, index)?)
        })
        .collect()
}

pub fn validate_elevation_plots(
    plots: &serde_json::Value,
    scene: &ValidatedScenePresentation,
) -> CoreResult<Vec<ValidatedElevationPlot>> {
    let mut validated = Vec::new();
    for (index, value) in find_plot_values(plots, "elevation") {
        let plot: ElevationPlotConfig =
            serde_json::from_value(value.clone()).map_err(elevation_config_error)?;
        validated.push(validate_elevation_plot(&plot, index, scene)?);
    }
    Ok(validated)
}

fn elevation_config_error(error: serde_json::Error) -> CoreError {
    CoreError::Config(format!("elevation plot config: {error}"))
}

fn validate_rolling_charts(plots: &serde_json::Value) -> CoreResult<Vec<ValidatedRollingChart>> {
    find_plot_values(plots, "rolling_chart")
        .into_iter()
        .map(|(index, value)| {
            let chart: RollingChartConfig = serde_json::from_value(value.clone())
                .map_err(|error| CoreError::Config(format!("rolling chart config: {error}")))?;
            Ok(validate_rolling_chart(&chart, index)?)
        })
        .collect()
}

fn validate_diving_profiles(
    plots: &serde_json::Value,
    scene: &ValidatedScenePresentation,
) -> CoreResult<Vec<ValidatedDivingProfile>> {
    find_plot_values(plots, "diving_profile")
        .into_iter()
        .map(|(index, value)| {
            let plot: DivingProfileConfig = serde_json::from_value(value.clone())
                .map_err(|error| CoreError::Config(format!("diving profile config: {error}")))?;
            Ok(validate_diving_profile(&plot, index, scene)?)
        })
        .collect()
}

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

fn validate_raster_document(
    raw: &RasterConfig,
    index: usize,
) -> CoreResult<ValidatedRasterDocument> {
    let geometry = validate_raster_geometry(raw, index)?;
    let path = raw.path.as_ref().map(PathBuf::from);
    if path.as_ref().is_some_and(|path| !path.is_absolute()) {
        return Err(CoreError::Config(format!(
            "Raster {} requires an absolute image path [raster_error:relative_path:{}]",
            raw.id, raw.id
        )));
    }
    Ok(ValidatedRasterDocument { geometry, path })
}

fn validate_unique_raster_ids(rasters: &[RasterConfig]) -> CoreResult<()> {
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
    validate_raster_geometry(raw, index)?;
    // A template may come from another platform, so its path is checked for the absolute forms
    // of every platform; rendering checks it against this one.
    if raw.path.as_deref().is_some_and(|path| !is_portable_absolute_path(path)) {
        return Err(CoreError::Config(format!(
            "Raster {} requires an absolute image path [raster_error:relative_path:{}]",
            raw.id, raw.id
        )));
    }
    Ok(())
}

/// A POSIX path, a Windows drive path or a UNC path.
fn is_portable_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/');
    let unc = path
        .strip_prefix("\\\\")
        .and_then(|rest| rest.split_once('\\'))
        .is_some_and(|(server, _)| !server.is_empty());
    path.starts_with('/') || drive || unc
}

fn validate_raster(
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

/// Lifts a boxed value's `display_variants.<key>` fields to the top level, where its validator
/// reads them; fields already present at the top level win, and nulls fall back to defaults.
fn promote_value_variant(
    value: &ValueConfig,
    variant_key: &str,
) -> CoreResult<ValueConfig> {
    let mut raw = serde_json::to_value(value)
        .map_err(|e| CoreError::Config(format!("value config serialization: {e}")))?;
    strip_json_nulls(&mut raw);
    promote_variant_keys(&mut raw, variant_key);

    serde_json::from_value(raw).map_err(|e| CoreError::Config(format!("value config: {e}")))
}

/// Promotes all keys from a nested display variant to the top level if they
/// are not already present at the root. This ensures display-specific fields
/// (e.g. width, height, pixels_per_degree, tick_color, etc.) are visible to
/// validators that expect them at the top level.
fn promote_variant_keys(raw: &mut serde_json::Value, variant_key: &str) {
    let mut promotions: Vec<(String, serde_json::Value)> = Vec::new();

    if let Some(variants) = raw.get("display_variants") {
        if let Some(variant) = variants.get(variant_key) {
            if let Some(obj) = variant.as_object() {
                for (key, value) in obj {
                    if raw.get(key).is_none() {
                        promotions.push((key.clone(), value.clone()));
                    }
                }
            }
        }
    }

    if let Some(obj) = raw.as_object_mut() {
        for (k, v) in promotions {
            obj.insert(k, v);
        }
    }
}


/// Finds a plot config by value key in either array or object-shaped template data.
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

/// Finds every plot config with the requested value key, preserving array order.
fn find_plot_values<'a>(plots: &'a Value, value_key: &str) -> Vec<(usize, &'a Value)> {
    match plots {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.get("value").and_then(Value::as_str) == Some(value_key))
            .collect(),
        Value::Object(_) => find_plot_value(plots, value_key)
            .map(|plot| vec![(0, plot)])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Removes all `null` values from a JSON object tree so that `#[serde(default)]`
/// attributes on struct fields can take effect during deserialization.
fn strip_json_nulls(value: &mut Value) {
    if let Value::Object(map) = value {
        map.retain(|_, v| !v.is_null());
        for v in map.values_mut() {
            strip_json_nulls(v);
        }
    }
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
            serde_json::from_value::<super::super::RenderConfig>(malformed_render_config)
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
