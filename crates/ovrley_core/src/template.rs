//! Template files: the format, version migrations, the editor's load and save of the durable
//! document, the render path's template ingress, and the bundled and user template folders.

use crate::config::validate_template_rasters;
use crate::error::{CoreError, CoreResult};
use ovrley_widgets::model::catalog::{catalog, LayoutMode};
use ovrley_widgets::model::document::{
    init_display_variant, normalize_color_fields, pick_defined, Collection, OverlayConfig,
    UnitSystem, WidgetData,
};
use ovrley_widgets::widgets::gauges::linear::LinearOrientation;
use ovrley_widgets::widgets::raster::RasterConfig;
use ovrley_widgets::widgets::time::{ElapsedTimeOrigin, TimeMode};
use ovrley_widgets::widgets::value::ContentAlignment;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::sync::OnceLock;
use crate::config::{parse_config_value, RenderConfig};
use ovrley_widgets::widgets::label::LabelTypography;
use crate::paths::AppPaths;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// The `format` field of a template file.
pub const TEMPLATE_FILE_FORMAT: &str = "ovrley-template";
/// The template file version this build writes; version 2 files are migrated on load.
pub const TEMPLATE_FILE_VERSION: u32 = 3;

/// `assets/standard-template.json`: template-wide defaults and scene keys.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateManifest {
    /// The default template-wide settings.
    pub globals: Map<String, Value>,
    /// The default scene; its keys are the durable scene keys.
    pub scene: Map<String, Value>,
    pub file_format: String,
    pub file_version: u32,
    pub scene_presentation_keys: Vec<String>,
    pub render_time_only_keys: Vec<String>,
}

/// The bundled template manifest. A broken manifest is a build defect and panics.
pub fn template_manifest() -> &'static TemplateManifest {
    static MANIFEST: OnceLock<TemplateManifest> = OnceLock::new();
    MANIFEST.get_or_init(|| {
        let manifest: TemplateManifest = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/standard-template.json"
        )))
        .expect("standard-template.json must be valid");
        assert_eq!(
            manifest.file_format, TEMPLATE_FILE_FORMAT,
            "the template manifest and the core agree on the format"
        );
        assert_eq!(
            manifest.file_version, TEMPLATE_FILE_VERSION,
            "the template manifest and the core agree on the version"
        );
        manifest
    })
}

/// The durable editor state: what a template file holds, and what the save status compares.
#[derive(Clone, Debug, PartialEq)]
pub struct DurableTemplateState {
    pub config: OverlayConfig,
    pub global_defaults: Map<String, Value>,
}

/// A loaded template: its durable state and its name.
#[derive(Clone, Debug, PartialEq)]
pub struct TemplateDocument {
    pub config: OverlayConfig,
    pub global_defaults: Map<String, Value>,
    pub name: Option<String>,
}

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::Config(message.into())
}

/// A JSON value as error messages show it: text unquoted, a missing value as `undefined`.
fn value_text(value: Option<&Value>) -> String {
    match value {
        None => "undefined".into(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

/// A value that sets a fallback: anything but null, `false`, zero or empty text.
fn set_value(value: Option<&Value>) -> Option<&Value> {
    value.filter(|value| match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(text) => !text.is_empty(),
        _ => true,
    })
}

fn keys(names: impl IntoIterator<Item = impl Into<String>>) -> Vec<String> {
    names.into_iter().map(Into::into).collect()
}

/// The template-wide defaults: only known keys, every missing one from the app defaults, colours
/// normalized.
pub fn normalize_global_defaults(
    global_defaults: Option<&Map<String, Value>>,
) -> Map<String, Value> {
    let defaults = &template_manifest().globals;
    let mut merged = defaults.clone();
    for (key, value) in pick_defined(global_defaults, &keys(defaults.keys().cloned())) {
        merged.insert(key, value);
    }
    normalize_color_fields(&merged)
}

/// Scene style fields that version 2 files kept on the scene, under the explicit settings,
/// normalized.
pub fn merge_scene_global_defaults(
    scene: &Map<String, Value>,
    global_defaults: Option<&Map<String, Value>>,
) -> Map<String, Value> {
    let mut merged = pick_defined(
        Some(scene),
        &keys(template_manifest().globals.keys().cloned()),
    );
    for (key, value) in global_defaults.into_iter().flatten() {
        merged.insert(key.clone(), value.clone());
    }
    normalize_global_defaults(Some(&merged))
}

/// The whole part of a saved update rate, written as a number or as numeric text.
fn whole_update_rate(value: &Value) -> Option<f64> {
    let number = match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }?;
    number.is_finite().then(|| number.trunc())
}

/// The durable scene: only the template-wide render defaults persist; scene timing belongs to the
/// activity and the export session.
fn normalize_scene(scene: &Map<String, Value>) -> Map<String, Value> {
    let manifest = template_manifest();
    let mut next = pick_defined(Some(scene), &keys(manifest.scene.keys().cloned()));
    match scene.get("updateRate").and_then(whole_update_rate) {
        Some(rate) if rate >= 1.0 => {
            next.insert("updateRate".into(), Value::from(rate as i64));
        }
        _ => {
            next.remove("updateRate");
        }
    }
    for key in &manifest.render_time_only_keys {
        next.remove(key);
    }
    normalize_color_fields(&next)
}

fn config_keys(kind: &str) -> &'static [String] {
    &catalog()
        .require_widget(kind)
        .expect("the catalog defines every collection type")
        .config_keys
}

fn normalize_variants(variants: Option<&Value>) -> CoreResult<Map<String, Value>> {
    let Some(variants) = variants else {
        return Ok(Map::new());
    };
    let variants = variants
        .as_object()
        .ok_or_else(|| invalid("Display variants must be an object"))?;
    let mut normalized = Map::new();
    for (display_type, variant) in variants {
        let definition = catalog()
            .display(display_type)
            .ok_or_else(|| invalid(format!("Unknown display variant: {display_type}")))?;
        let variant = variant
            .as_object()
            .ok_or_else(|| invalid(format!("Invalid display variant: {display_type}")))?;
        if definition.variant_keys.is_empty() {
            continue;
        }
        normalized.insert(
            display_type.clone(),
            Value::Object(normalize_color_fields(&pick_defined(
                Some(variant),
                &definition.variant_keys,
            ))),
        );
    }
    Ok(normalized)
}

fn normalize_backdrop(backdrop: &WidgetData) -> CoreResult<WidgetData> {
    let mut picked = normalize_color_fields(&pick_defined(Some(backdrop), config_keys("backdrop")));
    let display_type = picked
        .get("display_type")
        .and_then(Value::as_str)
        .filter(|display| catalog().display(display).is_some())
        .ok_or_else(|| {
            invalid(format!(
                "Invalid backdrop display_type: {}",
                value_text(backdrop.get("display_type"))
            ))
        })?
        .to_string();
    let allowed = &catalog().require_display(&display_type).variant_keys;
    let mut variants = normalize_variants(backdrop.get("display_variants"))?;
    let promoted = pick_defined(Some(backdrop), allowed);
    if !promoted.is_empty() {
        let mut variant = variants
            .get(&display_type)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (key, value) in promoted {
            variant.insert(key, value);
        }
        variants.insert(
            display_type.clone(),
            Value::Object(normalize_color_fields(&variant)),
        );
    }
    picked.insert("display_variants".into(), Value::Object(variants));
    let fallback = picked.clone();
    Ok(init_display_variant(
        &picked,
        &display_type,
        Some(&fallback),
        UnitSystem::Metric,
    ))
}

fn validate_display_geometry(value: &WidgetData, kind: &str) -> CoreResult<()> {
    let catalog = catalog();
    let display_type = match value.get("display_type") {
        Some(Value::String(display)) => display.clone(),
        Some(other) => return Err(invalid(format!("Invalid display_type: {other}"))),
        None => catalog
            .require_widget(kind)
            .map_err(invalid)?
            .defaults
            .get("display_type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    };
    let definition = catalog
        .display(&display_type)
        .ok_or_else(|| invalid(format!("Unknown display type: {display_type}")))?;
    let variant = value
        .get("display_variants")
        .and_then(|variants| variants.get(&display_type))
        .and_then(Value::as_object);
    for field in &definition.forbidden_frame_fields {
        if value.contains_key(field) || variant.is_some_and(|variant| variant.contains_key(field)) {
            return Err(invalid(format!(
                "{display_type} does not accept {}; use {}",
                definition.forbidden_frame_fields.join(" or "),
                definition.required_positive_fields.join(", ")
            )));
        }
    }
    for field in &definition.required_positive_fields {
        let positive = variant
            .and_then(|variant| variant.get(field))
            .and_then(Value::as_f64)
            .is_some_and(|number| number.is_finite() && number > 0.0);
        if !positive {
            return Err(invalid(format!(
                "{display_type} {field} must be a positive finite number"
            )));
        }
    }
    Ok(())
}

/// A linear gauge's orientation, horizontal unless it is vertical, and a label position that
/// orientation offers, its default otherwise.
fn normalize_linear_gauge_label_position(variant: &mut Map<String, Value>) {
    let orientation = variant
        .get("orientation")
        .and_then(Value::as_str)
        .and_then(LinearOrientation::parse)
        .unwrap_or(LinearOrientation::Horizontal);
    let position = variant
        .get("min_max_label_position")
        .and_then(Value::as_str)
        .and_then(|key| orientation.label_position(key))
        .unwrap_or(orientation.label_positions()[0]);
    variant.insert("orientation".into(), Value::from(orientation.as_str()));
    variant.insert("min_max_label_position".into(), Value::from(position.as_str()));
}

fn normalize_value(
    value: &WidgetData,
    global_defaults: &Map<String, Value>,
) -> CoreResult<WidgetData> {
    let catalog = catalog();
    let kind = value.get("value").and_then(Value::as_str);
    let definition = kind
        .and_then(|kind| catalog.widget(kind))
        .filter(|definition| definition.collection == "values")
        .ok_or_else(|| {
            invalid(format!(
                "Unknown value widget type: {}",
                value_text(value.get("value"))
            ))
        })?;
    let kind = definition.kind.as_str();
    validate_display_geometry(value, kind)?;
    let mut with_defaults = definition.defaults.clone();
    for (key, field) in value {
        with_defaults.insert(key.clone(), field.clone());
    }
    if kind == "time" {
        let key = |field: &str| with_defaults.get(field).and_then(Value::as_str);
        if key("time_mode").and_then(TimeMode::parse).is_none() {
            return Err(invalid(format!(
                "Invalid time_mode: {}",
                value_text(with_defaults.get("time_mode"))
            )));
        }
        if key("elapsed_origin")
            .and_then(ElapsedTimeOrigin::parse)
            .is_none()
        {
            return Err(invalid(format!(
                "Invalid elapsed_origin: {}",
                value_text(with_defaults.get("elapsed_origin"))
            )));
        }
        if !with_defaults
            .get("show_hundredths")
            .is_some_and(Value::is_boolean)
        {
            return Err(invalid(format!(
                "Invalid show_hundredths: {}",
                value_text(with_defaults.get("show_hundredths"))
            )));
        }
    }
    let supports_alignment = definition.content_alignment;
    if supports_alignment
        && with_defaults
            .get("content_alignment")
            .and_then(Value::as_str)
            .and_then(ContentAlignment::parse)
            .is_none()
    {
        return Err(invalid(format!(
            "Invalid content_alignment: {}",
            value_text(with_defaults.get("content_alignment"))
        )));
    }
    let display_type = with_defaults
        .get("display_type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if kind == "lap_timer" {
        let mode_value = value
            .get("lap_timer_mode")
            .and_then(Value::as_str)
            .unwrap_or("undefined");
        let display = catalog
            .display(&display_type)
            .ok_or_else(|| invalid(format!("Unknown display type: {display_type}")))?;
        let mode = display.require_mode(mode_value).map_err(invalid)?;
        if !value.contains_key("label_font_size") {
            if let Some(size) = mode.defaults.get("label_font_size") {
                with_defaults.insert("label_font_size".into(), size.clone());
            }
        }
        if !with_defaults
            .get("renumber_laps")
            .is_some_and(Value::is_boolean)
        {
            return Err(invalid(format!(
                "Invalid renumber_laps: {}",
                value_text(with_defaults.get("renumber_laps"))
            )));
        }
    }
    if kind == "lap_timer" || kind == "splits_run" {
        let display = catalog
            .display(&display_type)
            .ok_or_else(|| invalid(format!("Unknown display type: {display_type}")))?;
        for (field, global) in [("label_font", "font_text"), ("label_color", "color_text")] {
            if !value.contains_key(field) {
                let fallback = set_value(global_defaults.get(global))
                    .or_else(|| display.defaults.get(field))
                    .cloned()
                    .unwrap_or(Value::Null);
                with_defaults.insert(field.into(), fallback);
            }
        }
    }
    let mut picked = pick_defined(Some(&with_defaults), &definition.config_keys);
    if !supports_alignment {
        picked.remove("content_alignment");
    }
    if !picked.get("display_unit").is_some_and(Value::is_string) {
        picked.remove("display_unit");
    }
    if picked.contains_key("display_variants") {
        let variants = normalize_variants(picked.get("display_variants"))?;
        picked.insert("display_variants".into(), Value::Object(variants));
    }
    let display = catalog
        .display(&display_type)
        .ok_or_else(|| invalid(format!("Unknown display type: {display_type}")))?;
    if display.layout_mode == LayoutMode::Boxed {
        let fallback = picked.clone();
        let initialized =
            init_display_variant(&picked, &display_type, Some(&fallback), UnitSystem::Metric);
        if let Some(variants) = initialized.get("display_variants") {
            picked.insert("display_variants".into(), variants.clone());
        }
    }
    if let Some(linear) = picked
        .get_mut("display_variants")
        .and_then(|variants| variants.get_mut("linear"))
        .and_then(Value::as_object_mut)
    {
        normalize_linear_gauge_label_position(linear);
    }
    Ok(normalize_color_fields(&picked))
}

fn normalize_point_label(
    point_label: Option<&Value>,
    scene: &Map<String, Value>,
    global_defaults: &Map<String, Value>,
) -> Map<String, Value> {
    let point_label = point_label.and_then(Value::as_object);
    let field = |name: &str| point_label.and_then(|label| label.get(name));
    let fallback_font =
        set_value(global_defaults.get("font_values")).or_else(|| set_value(scene.get("font")));
    let fallback_color = set_value(field("color"))
        .or_else(|| set_value(global_defaults.get("color_values")))
        .cloned()
        .unwrap_or_else(|| json!("#ffffff"));
    let font_size = field("font_size")
        .filter(|size| !size.is_null())
        .or_else(|| scene.get("font_size").filter(|size| !size.is_null()))
        .cloned()
        .unwrap_or_else(|| json!(12.5));
    let mut normalized = Map::new();
    normalized.insert("font_size".into(), font_size);
    normalized.insert("color".into(), fallback_color);
    if let Some(font) = fallback_font {
        normalized.insert("font".into(), font.clone());
    }
    for (key, value) in pick_defined(point_label, &keys(["font", "font_size", "color"])) {
        normalized.insert(key, value);
    }
    normalize_color_fields(&normalized)
}

fn normalize_plot(
    plot: &WidgetData,
    scene: &Map<String, Value>,
    global_defaults: &Map<String, Value>,
) -> CoreResult<WidgetData> {
    let definition = plot
        .get("value")
        .and_then(Value::as_str)
        .and_then(|kind| catalog().widget(kind))
        .filter(|definition| definition.collection == "plots")
        .ok_or_else(|| {
            invalid(format!(
                "Unknown plot widget type: {}",
                value_text(plot.get("value"))
            ))
        })?;
    let mut with_defaults = definition.defaults.clone();
    for (key, value) in plot {
        with_defaults.insert(key.clone(), value.clone());
    }
    if definition.kind == "elevation" || definition.kind == "diving_profile" {
        with_defaults.insert(
            "point_label".into(),
            Value::Object(normalize_point_label(
                plot.get("point_label"),
                scene,
                global_defaults,
            )),
        );
    }
    Ok(normalize_color_fields(&pick_defined(
        Some(&with_defaults),
        &definition.config_keys,
    )))
}

/// The durable widget config a template file holds.
pub fn normalize_template_config(
    config: &OverlayConfig,
    global_defaults: &Map<String, Value>,
) -> CoreResult<OverlayConfig> {
    let label_keys = config_keys("label");
    let raster_keys = config_keys("raster");
    Ok(OverlayConfig {
        scene: normalize_scene(&config.scene),
        backdrops: config
            .backdrops
            .iter()
            .map(normalize_backdrop)
            .collect::<CoreResult<_>>()?,
        rasters: config
            .rasters
            .iter()
            .map(|raster| pick_defined(Some(raster), raster_keys))
            .collect(),
        labels: config
            .labels
            .iter()
            .map(|label| normalize_color_fields(&pick_defined(Some(label), label_keys)))
            .collect(),
        values: config
            .values
            .iter()
            .map(|value| normalize_value(value, global_defaults))
            .collect::<CoreResult<_>>()?,
        plots: config
            .plots
            .iter()
            .map(|plot| normalize_plot(plot, &config.scene, global_defaults))
            .collect::<CoreResult<_>>()?,
    })
}

/// The durable config and global defaults of an editor state.
pub fn create_durable_template_state(
    config: &OverlayConfig,
    global_defaults: Option<&Map<String, Value>>,
) -> CoreResult<DurableTemplateState> {
    let global_defaults = merge_scene_global_defaults(&config.scene, global_defaults);
    Ok(DurableTemplateState {
        config: normalize_template_config(config, &global_defaults)?,
        global_defaults,
    })
}

fn required_object<'a>(
    value: Option<&'a Value>,
    message: &str,
) -> CoreResult<&'a Map<String, Value>> {
    value
        .and_then(Value::as_object)
        .ok_or_else(|| invalid(message))
}

/// The raster rules of both template checks: exact fields, a valid id, finite geometry, a
/// positive size, opacity from 0 to 1, a portable absolute path or none, no runtime resource
/// state, and an id no other widget uses.
fn validate_rasters(config: &Map<String, Value>) -> CoreResult<()> {
    let rasters = config
        .get("rasters")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("Raster collection must be an array"))?;
    let rasters = rasters
        .iter()
        .enumerate()
        .map(|(index, raster)| {
            serde_json::from_value::<RasterConfig>(raster.clone()).map_err(|error| {
                invalid(format!(
                    "rasters[{index}]: {error} [raster_error:invalid_config:configuration]"
                ))
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    validate_template_rasters(&rasters)?;
    let raster_ids: HashSet<&str> = rasters.iter().map(|raster| raster.id.as_str()).collect();
    for collection in [
        Collection::Backdrops,
        Collection::Labels,
        Collection::Values,
        Collection::Plots,
    ] {
        for widget in config
            .get(collection.name())
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if widget
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| raster_ids.contains(id))
            {
                return Err(invalid("Raster widget IDs must be unique"));
            }
        }
    }
    Ok(())
}

/// The editor's template ingress: a parsed template file to its durable document.
pub fn load_template_document(file: &Value) -> CoreResult<TemplateDocument> {
    let object = file
        .as_object()
        .ok_or_else(|| invalid("Template file is empty or invalid."))?;
    if object.get("format").and_then(Value::as_str) != Some(TEMPLATE_FILE_FORMAT) {
        return Err(invalid("Unsupported template file format."));
    }
    required_object(object.get("config"), "Template config must be an object.")?;
    let settings = required_object(
        object.get("settings"),
        "Template settings must be an object.",
    )?;
    required_object(
        settings.get("globalDefaults"),
        "Template settings.globalDefaults must be an object.",
    )?;

    let mut migrated = migrate_template_version(file)?;
    migrate_saved_font_input(&mut migrated);
    let config = migrated["config"]
        .as_object()
        .expect("the migration checked the config");
    if config.contains_key("labels") {
        validate_saved_label_typography(&migrated["config"])?;
    }
    validate_rasters(config)?;
    let document = OverlayConfig::from_value(&migrated["config"]).map_err(invalid)?;
    let state = create_durable_template_state(
        &document,
        migrated["settings"]["globalDefaults"].as_object(),
    )?;
    // The editor sizes its canvas from these.
    for key in ["width", "height"] {
        let size = state.config.scene.get(key).and_then(Value::as_f64);
        if !size.is_some_and(|size| size.is_finite() && size > 0.0) {
            return Err(invalid(format!("Template scene {key} must be a positive number.")));
        }
    }
    Ok(TemplateDocument {
        config: state.config,
        global_defaults: state.global_defaults,
        name: migrated
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .map(str::to_string),
    })
}

/// The file a save writes, stamped with the format, the version, the
/// name and the time.
pub fn template_file_payload(
    config: &OverlayConfig,
    global_defaults: &Map<String, Value>,
    name: Option<&str>,
    saved_at: chrono::DateTime<chrono::Utc>,
) -> CoreResult<Value> {
    let state = create_durable_template_state(config, Some(global_defaults))?;
    Ok(json!({
        "format": TEMPLATE_FILE_FORMAT,
        "version": TEMPLATE_FILE_VERSION,
        "name": name.filter(|name| !name.is_empty()),
        "savedAt": saved_at.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        "config": state.config.to_value(),
        "settings": { "globalDefaults": state.global_defaults },
    }))
}

/// Parses either a raw render config or a wrapped OVRLEY template file.
#[must_use = "parsed config must be consumed for rendering"]
fn parse_template_json(input: &str) -> CoreResult<RenderConfig> {
    let value: Value = serde_json::from_str(input)
        .map_err(|error| CoreError::Config(format!("template JSON: {error}")))?;
    parse_template_value(&value)
}

/// Parses either a raw render config or a wrapped OVRLEY template value.
#[must_use = "parsed config must be consumed for rendering"]
pub fn parse_template_value(value: &Value) -> CoreResult<RenderConfig> {
    let Some(format) = value.get("format").and_then(Value::as_str) else {
        return parse_config_value(value);
    };

    if format != TEMPLATE_FILE_FORMAT {
        return Err(CoreError::Config(format!(
            "template format: expected {TEMPLATE_FILE_FORMAT}, got {format}"
        )));
    }

    let mut migrated = migrate_template_version(value)?;
    migrate_saved_font_input(&mut migrated);
    let mut config_value = migrated["config"].clone();
    materialize_template_scene_defaults(&mut config_value, &migrated);
    let mut config = parse_config_value(&config_value)?;
    apply_template_global_defaults(&mut config, &migrated);
    Ok(config)
}

/// The template file's version migration, shared by the render path and the editor's template
/// ingress: version 2 gains an empty raster collection, which it must not already have, and the
/// result is a current-version file whose `config.rasters` is an array.
pub fn migrate_template_version(template: &Value) -> CoreResult<Value> {
    let Some(version) = template.get("version").and_then(Value::as_u64) else {
        return Err(CoreError::Config("template version missing".into()));
    };
    if version != 2 && version != u64::from(TEMPLATE_FILE_VERSION) {
        return Err(CoreError::Config(format!(
            "unsupported template version: {version}. expected {TEMPLATE_FILE_VERSION}"
        )));
    }
    let mut migrated = template.clone();
    let config = migrated
        .get_mut("config")
        .ok_or_else(|| CoreError::Config("template config missing".into()))?
        .as_object_mut()
        .ok_or_else(|| CoreError::Config("template config must be an object".into()))?;
    if version == 2 {
        if config.contains_key("rasters") {
            return Err(CoreError::Config(
                "version 2 template cannot contain rasters".into(),
            ));
        }
        config.insert("rasters".into(), Value::Array(Vec::new()));
    }
    if !config.get("rasters").is_some_and(Value::is_array) {
        return Err(CoreError::Config(
            "template config.rasters must be an array".into(),
        ));
    }
    migrated["version"] = Value::from(TEMPLATE_FILE_VERSION);
    Ok(migrated)
}

/// Upgrades the typography of older saved documents in memory: label weight, italic and
/// spacing, and the retired `Inter ExtraBold` font.
/// Present typography values are preserved; validation belongs to the load boundary.
pub fn migrate_saved_font_input(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if let Some(labels) = object.get_mut("labels").and_then(Value::as_array_mut) {
                for label in labels {
                    if let Some(label) = label.as_object_mut() {
                        label.entry("font_weight").or_insert(Value::from(400));
                        label.entry("italic").or_insert(Value::Bool(false));
                        label.entry("letter_spacing").or_insert(Value::from(0));
                    }
                }
            }
            for (key, value) in object {
                match (key.as_str(), value.as_str()) {
                    (
                        "font" | "label_font" | "min_max_label_font" | "font_text" | "font_values",
                        Some("Inter ExtraBold.ttf" | "Inter ExtraBold"),
                    ) => *value = Value::from("Inter.ttf"),
                    ("font_family", Some("Inter ExtraBold")) => *value = Value::from("Inter"),
                    _ => migrate_saved_font_input(value),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                migrate_saved_font_input(item);
            }
        }
        _ => {}
    }
}

/// Validates project-owned label typography once at its JSON load boundary.
pub fn validate_saved_label_typography(config: &Value) -> CoreResult<()> {
    let labels = config
        .get("labels")
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Config("labels must be an array".into()))?;
    for (index, label) in labels.iter().enumerate() {
        serde_json::from_value::<LabelTypography>(label.clone())
            .map_err(|error| CoreError::Config(format!("labels[{index}].{error}")))?;
    }
    Ok(())
}

fn materialize_template_scene_defaults(config: &mut Value, template: &Value) {
    let Some(scene) = config.get_mut("scene").and_then(Value::as_object_mut) else {
        return;
    };

    if !scene.contains_key("start") {
        scene.insert("start".to_string(), Value::from(0.0));
    }
    if !scene.contains_key("end") {
        scene.insert("end".to_string(), Value::from(1.0));
    }

    let Some(globals) = template
        .get("settings")
        .and_then(|settings| settings.get("globalDefaults"))
        .and_then(Value::as_object)
    else {
        return;
    };

    copy_scene_default_if_missing(scene, globals, "scale", "scale");
    copy_scene_default_if_missing(scene, globals, "font", "font_values");
    copy_scene_default_if_missing(scene, globals, "font_size", "font_size");
    copy_scene_default_if_missing(scene, globals, "opacity", "opacity");
    copy_scene_default_if_missing(scene, globals, "shadow_color", "shadow_color");
    copy_scene_default_if_missing(scene, globals, "shadow_strength", "shadow_strength");
    copy_scene_default_if_missing(scene, globals, "shadow_distance", "shadow_distance");
    copy_scene_default_if_missing(scene, globals, "border_color", "border_color");
    copy_scene_default_if_missing(scene, globals, "border_thickness", "border_thickness");
    copy_scene_default_if_missing(scene, globals, "fixed_width_digits", "fixed_width_digits");
}

fn copy_scene_default_if_missing(
    scene: &mut serde_json::Map<String, Value>,
    globals: &serde_json::Map<String, Value>,
    scene_key: &str,
    globals_key: &str,
) {
    if scene.contains_key(scene_key) {
        return;
    }

    if let Some(value) = globals.get(globals_key) {
        scene.insert(scene_key.to_string(), value.clone());
    }
}

fn apply_template_global_defaults(config: &mut RenderConfig, template: &Value) {
    let Some(globals) = template
        .get("settings")
        .and_then(|settings| settings.get("globalDefaults"))
    else {
        return;
    };

    let font_values = globals
        .get("font_values")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if let Some(font_values) = font_values {
        for value in &mut config.values {
            if value.font.is_none() {
                value.font = Some(font_values.clone());
            }
            if value.font_family.is_none() {
                value.font_family = Some(font_values.clone());
            }
        }
    }
}

/// Whether a listed template is bundled or the user's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateKind {
    BuiltIn,
    User,
}

/// One listed template.
#[derive(Clone, Debug, PartialEq)]
pub struct TemplateDescriptor {
    pub id: String,
    pub name: String,
    pub kind: TemplateKind,
    /// The scene's size, 0 when the file declares none.
    pub width: u32,
    pub height: u32,
    /// User templates only.
    pub path: Option<String>,
}


/// Lists valid built-in and user templates.
///
/// Built-in templates are de-duplicated by filename, while user templates are
/// exposed with a `user:` id prefix to avoid collisions.
pub fn list_templates(paths: &AppPaths) -> Vec<TemplateDescriptor> {
    let mut templates = Vec::new();
    let mut seen = BTreeSet::new();

    for dir in &paths.bundled_templates_dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !matches!(
                    path.extension().and_then(|value| value.to_str()),
                    Some("json")
                ) {
                    continue;
                }
                let Some(filename) = path.file_name().and_then(|value| value.to_str()) else {
                    continue;
                };
                if !seen.insert(filename.to_string()) {
                    continue;
                }
                if let Some(descriptor) = template_descriptor(&path, filename, TemplateKind::BuiltIn, filename)
                {
                    templates.push(descriptor);
                }
            }
        }
    }

    if let Ok(entries) = fs::read_dir(&paths.user_templates_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("json")
            ) {
                continue;
            }
            let Some(filename) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            let id = format!("user:{filename}");
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(descriptor) = template_descriptor(&path, filename, TemplateKind::User, &id) {
                templates.push(descriptor);
            }
        }
    }

    templates
}

/// Reads a template's document by built-in filename, user-prefixed id, or unqualified id.
pub fn read_template(paths: &AppPaths, filename: &str) -> CoreResult<Value> {
    let (source, normalized) = parse_template_id(filename);
    let template_path = match source {
        TemplateSource::User => paths.user_template_path(&normalized),
        TemplateSource::BuiltIn => paths.bundled_template_path(&normalized),
        TemplateSource::Any => paths
            .bundled_template_path(&normalized)
            .or_else(|| paths.user_template_path(&normalized)),
    }
    .ok_or_else(|| CoreError::Config(format!("Template not found: {normalized}")))?;

    let text = fs::read_to_string(&template_path).map_err(|error| CoreError::Io {
        path: template_path.clone(),
        source: error,
    })?;
    Ok(serde_json::from_str(&text)?)
}


#[derive(Clone, Copy)]
enum TemplateSource {
    Any,
    BuiltIn,
    User,
}

// Parses a template id into its source namespace and sanitized filename.
fn parse_template_id(template_id: &str) -> (TemplateSource, String) {
    // Prefixes are part of the template list's ids; an unprefixed id searches built-ins first,
    // then user files.
    let (source, filename) = if let Some(filename) = template_id.strip_prefix("user:") {
        (TemplateSource::User, filename)
    } else if let Some(filename) = template_id.strip_prefix("built-in:") {
        (TemplateSource::BuiltIn, filename)
    } else {
        (TemplateSource::Any, template_id)
    };

    (source, ensure_json_filename(filename))
}

// Normalizes a template filename and ensures it has a `.json` extension.
fn ensure_json_filename(filename: &str) -> String {
    // Strip any directory components before resolving inside template roots.
    // This prevents traversal while still accepting user-provided bare names.
    let safe_filename = Path::new(filename)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("template.json");

    if safe_filename.ends_with(".json") {
        safe_filename.to_string()
    } else {
        format!("{safe_filename}.json")
    }
}

// Reads and parses a template JSON file, returning `None` for invalid files.
fn read_template_file(path: &Path) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

// Returns whether a parsed JSON document is an OVRLEY template.
fn is_app_template(value: &Value) -> bool {
    value
        .get("format")
        .and_then(Value::as_str)
        .map(|format| format == "ovrley-template")
        .unwrap_or(false)
}

// Extracts the declared scene resolution from a template JSON document.
fn read_template_resolution(value: &Value) -> Option<(u32, u32)> {
    let scene = value.get("config").and_then(|config| config.get("scene"))?;
    let width = u32::try_from(scene.get("width")?.as_u64()?).ok()?;
    let height = u32::try_from(scene.get("height")?.as_u64()?).ok()?;

    Some((width, height))
}

// Builds the list entry of one valid template file.
fn template_descriptor(
    path: &Path,
    filename: &str,
    kind: TemplateKind,
    id: &str,
) -> Option<TemplateDescriptor> {
    // Invalid or non-OVRLEY JSON files are silently skipped so users can keep
    // unrelated notes in the same Documents/OVRLEY directory.
    let value = read_template_file(path)?;
    if !is_app_template(&value) {
        return None;
    }
    let (width, height) = read_template_resolution(&value).unwrap_or((0, 0));

    Some(TemplateDescriptor {
        id: id.to_string(),
        name: filename.trim_end_matches(".json").replace('_', " ").to_uppercase(),
        kind,
        width,
        height,
        path: (kind == TemplateKind::User).then(|| path.to_string_lossy().into_owned()),
    })
}


/// Checks that template file text renders: the file format and version, the rasters, and the
/// render config with the template's defaults applied.
pub fn validate_template_contents(input: &str) -> CoreResult<()> {
    let mut config = parse_template_json(input)?;
    crate::config::validate_template_rasters(&config.rasters)?;
    config.rasters.clear();
    crate::config::validate_render_config(config, None)?;
    Ok(())
}

/// Writes a user template file after validating `contents`, creating its folder as needed.
pub fn write_template(path: &Path, contents: &str) -> CoreResult<()> {
    validate_template_contents(contents)?;
    let io = |source| CoreError::Io { path: path.to_path_buf(), source };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io)?;
    }
    fs::write(path, contents).map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    use ovrley_widgets::model::document::{
        create_widget_defaults, WidgetCreationOptions, WidgetSelection,
    };

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    fn document(value: Value) -> OverlayConfig {
        OverlayConfig::from_value(&value).unwrap()
    }

    fn normalize(value: Value) -> CoreResult<OverlayConfig> {
        normalize_template_config(&document(value), &Map::new())
    }

    fn file(mut config: Value, globals: Value) -> Value {
        if let Some(object) = config.as_object_mut() {
            object.entry("scene").or_insert(json!({"width": 1920, "height": 1080}));
        }
        json!({"format": TEMPLATE_FILE_FORMAT, "version": TEMPLATE_FILE_VERSION, "config": config, "settings": {"globalDefaults": globals}})
    }

    fn load_error(file: Value) -> String {
        load_template_document(&file).unwrap_err().to_string()
    }

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-11-15T10:00:00.123Z")
            .unwrap()
            .into()
    }

    #[test]
    fn global_defaults_fill_pick_and_normalize() {
        let defaults = &template_manifest().globals;
        let result = normalize_global_defaults(Some(&map(
            json!({"font_text": "Custom.ttf", "unknownKey": 1, "color_text": "#FFAA00"}),
        )));
        assert_eq!(result["font_text"], "Custom.ttf");
        assert_eq!(result["font_values"], defaults["font_values"]);
        assert_eq!(result["color_text"], "#ffaa00");
        assert!(!result.contains_key("unknownKey"));
        assert_eq!(normalize_global_defaults(None)["opacity"], 1);
        let merged = merge_scene_global_defaults(
            &map(json!({"border_color": "#111111", "opacity": 0.4})),
            Some(&map(json!({"opacity": 0.8}))),
        );
        assert_eq!(merged["border_color"], "#111111");
        assert_eq!(
            merged["opacity"], 0.8,
            "explicit settings win over the scene"
        );
    }

    #[test]
    fn backdrops_keep_shared_fields_and_move_geometry_into_the_variant() {
        let result = normalize(json!({"backdrops": [{"id": "bd-abc", "display_type": "rectangle", "x": 10, "y": 20, "fill_color": "#FFFFFF",
            "width": 240, "height": 160, "unknown_field": "remove"}]}))
        .unwrap();
        let backdrop = &result.backdrops[0];
        assert_eq!(backdrop["fill_color"], "#ffffff");
        assert_eq!(backdrop["display_variants"]["rectangle"]["width"], 240);
        assert!(!backdrop.contains_key("unknown_field") && !backdrop.contains_key("width"));
        assert!(normalize(json!({})).unwrap().backdrops.is_empty());
    }

    #[test]
    fn the_scene_keeps_only_durable_keys() {
        let result = normalize(json!({"scene": {"width": 1920, "height": 1080, "fps": 30, "updateRate": 5, "start": 12, "end": 144, "font": "F.ttf",
            "scale": 1.2, "composite_video_path": "/v.mp4", "qualityType": "quality", "ffmpeg": {}}}))
        .unwrap();
        assert_eq!(
            Value::Object(result.scene),
            json!({"width": 1920, "height": 1080, "fps": 30, "updateRate": 5})
        );
        let rate = normalize(json!({"scene": {"updateRate": 0.5}})).unwrap();
        assert!(!rate.scene.contains_key("updateRate"));
    }

    #[test]
    fn labels_and_values_keep_their_config_keys() {
        let result = normalize(
            json!({"labels": [{"id": "label-1", "text": "Hello", "x": 10, "extraField": 1}],
            "values": [{"id": "value-1", "value": "speed", "x": 100, "show_units": true}]}),
        )
        .unwrap();
        assert_eq!(result.labels[0]["text"], "Hello");
        assert!(!result.labels[0].contains_key("extraField"));
        assert_eq!(result.values[0]["content_alignment"], "left");
        assert_eq!(result.values[0]["show_units"], true);
    }

    #[test]
    fn time_widgets_validate_their_modes_and_alignment() {
        let result =
            normalize(json!({"values": [{"id": "time-1", "value": "time", "x": 50}]})).unwrap();
        assert_eq!(result.values[0]["time_mode"], "daytime");
        assert_eq!(result.values[0]["elapsed_origin"], "activity");
        assert_eq!(result.values[0]["show_hundredths"], false);
        for (widget, message) in [
            (
                json!({"value": "time", "time_mode": "clock"}),
                "Invalid time_mode: clock",
            ),
            (
                json!({"value": "time", "elapsed_origin": "video"}),
                "Invalid elapsed_origin: video",
            ),
            (
                json!({"value": "time", "show_hundredths": "false"}),
                "Invalid show_hundredths: false",
            ),
            (
                json!({"value": "speed", "content_alignment": "justify"}),
                "Invalid content_alignment: justify",
            ),
            (
                json!({"value": "unknown_metric"}),
                "Unknown value widget type: unknown_metric",
            ),
        ] {
            let error = normalize(json!({"values": [widget]}))
                .unwrap_err()
                .to_string();
            assert!(error.contains(message), "{error}");
        }
    }

    #[test]
    fn plots_take_global_fallbacks_and_new_fields() {
        let globals = map(json!({"font_values": "ElevFont.ttf", "color_values": "#333333"}));
        let result = normalize_template_config(
            &document(json!({"plots": [{"id": "p1", "value": "elevation", "point_label": {}}, {"id": "p2", "value": "elevation", "point_label": {}, "area_gradient": true}]})),
            &globals,
        )
        .unwrap();
        assert_eq!(result.plots[0]["point_label"]["font"], "ElevFont.ttf");
        assert_eq!(result.plots[0]["area_gradient"], false);
        assert_eq!(result.plots[1]["area_gradient"], true);
    }

    #[test]
    fn boxed_values_complete_their_variants() {
        let result = normalize(json!({"values": [{"id": "l", "value": "speed", "display_type": "linear", "display_variants": {"linear": {"width": 320, "height": 80, "track_corner_radius": 10}}},
            {"id": "h", "value": "heading", "display_type": "heading_tape", "display_variants": {"heading_tape": {"width": 400, "height": 80, "label_font_size": 16}}}]}))
        .unwrap();
        let linear = &result.values[0]["display_variants"]["linear"];
        assert_eq!(linear["track_corner_radius"], 10);
        assert_eq!(linear["orientation"], "horizontal");
        assert_eq!(linear["min_max_label_position"], "bottom");
        assert_eq!(
            result.values[1]["display_variants"]["heading_tape"]["label_font_size"],
            16
        );
    }

    #[test]
    fn lean_angle_geometry_is_validated() {
        let error = normalize(
            json!({"values": [{"value": "lean_angle", "display_type": "lean_angle", "width": 10}]}),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("lean_angle does not accept"), "{error}");
        let error = normalize(json!({"values": [{"value": "lean_angle", "display_type": "lean_angle", "display_variants": {"lean_angle": {"diameter": 0}}}]})).unwrap_err().to_string();
        assert!(
            error.contains("lean_angle diameter must be a positive finite number"),
            "{error}"
        );
    }

    #[test]
    fn lap_timers_take_label_typography_from_globals_and_their_mode() {
        let config = document(
            json!({"values": [{"id": "lap", "value": "lap_timer", "display_type": "lap_timer", "lap_timer_mode": "lap_log"}]}),
        );
        let state = create_durable_template_state(
            &config,
            Some(&map(
                json!({"font_text": "Text.ttf", "color_text": "#123456"}),
            )),
        )
        .unwrap();
        assert_eq!(state.config.values[0]["label_font"], "Text.ttf");
        assert_eq!(state.config.values[0]["label_color"], "#123456");
        assert_eq!(state.config.values[0]["label_font_size"], 15);
    }

    #[test]
    fn durable_state_merges_scene_styles_and_drops_render_keys() {
        let config = document(
            json!({"scene": {"width": 1920, "start": 3, "opacity": 0.4, "border_color": "#123456", "composite_video_path": "C:\\clip.mp4"}}),
        );
        let state =
            create_durable_template_state(&config, Some(&map(json!({"opacity": 0.8})))).unwrap();
        assert_eq!(state.global_defaults["opacity"], 0.8);
        assert_eq!(state.global_defaults["border_color"], "#123456");
        assert!(
            !state.config.scene.contains_key("opacity")
                && !state.config.scene.contains_key("composite_video_path")
        );
    }

    #[test]
    fn the_file_checks_report_each_problem() {
        assert!(load_error(json!([])).contains("Template file is empty or invalid."));
        assert!(
            load_error(json!({"format": "other"})).contains("Unsupported template file format.")
        );
        for (config, settings, message) in [
            (
                json!("invalid"),
                json!({"globalDefaults": {}}),
                "Template config must be an object.",
            ),
            (
                json!([]),
                json!({"globalDefaults": {}}),
                "Template config must be an object.",
            ),
            (
                json!({}),
                json!("invalid"),
                "Template settings must be an object.",
            ),
            (json!({}), json!([]), "Template settings must be an object."),
            (
                json!({}),
                json!({"globalDefaults": "invalid"}),
                "Template settings.globalDefaults must be an object.",
            ),
        ] {
            let error = load_error(
                json!({"format": TEMPLATE_FILE_FORMAT, "version": 3, "config": config, "settings": settings}),
            );
            assert!(error.contains(message), "{error}");
        }
        let error = load_error(
            json!({"format": TEMPLATE_FILE_FORMAT, "version": 1, "config": {}, "settings": {"globalDefaults": {}}}),
        );
        assert!(
            error.contains("unsupported template version: 1. expected 3"),
            "{error}"
        );
    }

    #[test]
    fn version_2_gains_an_empty_raster_collection_and_version_3_must_have_one() {
        let loaded = load_template_document(&json!({"format": TEMPLATE_FILE_FORMAT, "version": 2, "config": {"scene": {"width": 1920, "height": 1080}}, "settings": {"globalDefaults": {}}})).unwrap();
        assert!(loaded.config.rasters.is_empty());
        let error = load_error(
            json!({"format": TEMPLATE_FILE_FORMAT, "version": 2, "config": {"rasters": []}, "settings": {"globalDefaults": {}}}),
        );
        assert!(
            error.contains("version 2 template cannot contain rasters"),
            "{error}"
        );
        assert!(
            load_error(file(json!({"scene": {}}), json!({}))).contains("rasters must be an array")
        );
    }

    #[test]
    fn label_typography_is_migrated_when_absent_and_strict_when_present() {
        for (field, value) in [
            ("letter_spacing", json!("0")),
            ("italic", json!("true")),
            ("font_weight", json!(1001)),
            ("font_weight", json!(null)),
        ] {
            let error = load_error(file(
                json!({"rasters": [], "labels": [{"text": "T", field: value}]}),
                json!({}),
            ));
            assert!(error.contains(field), "{field}: {error}");
        }
        let loaded = load_template_document(&file(
            json!({"rasters": [], "labels": [{"id": "title", "text": "T"}, {"id": "sub", "font": "Inter ExtraBold.ttf", "font_weight": 537, "italic": true, "letter_spacing": -1.25}]}),
            json!({"font_text": "Inter ExtraBold.ttf"}),
        ))
        .unwrap();
        assert_eq!(loaded.config.labels[0]["font_weight"], 400);
        assert_eq!(loaded.config.labels[0]["italic"], false);
        assert_eq!(loaded.config.labels[1]["font"], "Inter.ttf");
        assert_eq!(loaded.config.labels[1]["font_weight"], 537);
        assert_eq!(loaded.global_defaults["font_text"], "Inter.ttf");
        let saved =
            template_file_payload(&loaded.config, &loaded.global_defaults, None, now()).unwrap();
        let reloaded = load_template_document(&saved).unwrap();
        assert_eq!(
            reloaded, loaded,
            "a saved file loads back to the same document"
        );
    }

    fn raster(path: Value) -> Value {
        json!({"id": "raster-1", "x": 10, "y": 20, "width": 640, "height": 480, "rotation": 15, "opacity": 0.8, "path": path})
    }

    #[test]
    fn rasters_round_trip_with_portable_absolute_paths() {
        for path in [
            json!("C:\\images\\finish.png"),
            json!("/images/finish.png"),
            json!("\\\\server\\share\\a.png"),
            json!(null),
        ] {
            let loaded = load_template_document(&file(
                json!({"rasters": [raster(path.clone())]}),
                json!({}),
            ))
            .unwrap();
            assert_eq!(
                Value::Object(loaded.config.rasters[0].clone()),
                raster(path)
            );
        }
    }

    #[test]
    fn raster_rules_reject_each_violation() {
        let mut cases = vec![
            (raster(json!("images/finish.png")), "absolute image path"),
            (json!({"id": "raster-1", "x": 0}), "rasters[0]"),
            (
                json!({"id": "bad id", "x": 0, "y": 0, "width": 1, "height": 1, "rotation": 0, "opacity": 1, "path": null}),
                "letters, digits",
            ),
            (
                json!({"id": "r", "x": 0, "y": 0, "width": 0, "height": 1, "rotation": 0, "opacity": 1, "path": null}),
                "positive frame size",
            ),
            (
                json!({"id": "r", "x": 0, "y": 0, "width": 1, "height": 1, "rotation": 0, "opacity": 2, "path": null}),
                "opacity must be between 0 and 1",
            ),
            (
                json!({"id": "r", "x": 0, "y": 0, "width": 1, "height": 1, "rotation": 0, "opacity": 1, "path": null, "extra": 1}),
                "unknown field",
            ),
        ];
        let mut resource = raster(json!(null));
        resource["resourceId"] = json!("snapshot-1");
        cases.push((resource, "runtime resource state"));
        for (raster, message) in cases {
            let error = load_error(file(json!({"rasters": [raster]}), json!({})));
            assert!(error.contains(message), "{message}: {error}");
        }
        let error = load_error(file(
            json!({"rasters": [raster(json!(null)), raster(json!(null))]}),
            json!({}),
        ));
        assert!(error.contains("duplicated"), "{error}");
        let error = load_error(file(
            json!({"rasters": [raster(json!(null))], "labels": [{"id": "raster-1", "text": "x"}]}),
            json!({}),
        ));
        assert!(
            error.contains("Raster widget IDs must be unique"),
            "{error}"
        );
    }

    #[test]
    fn templates_without_widget_ids_gain_them_and_saving_keeps_them() {
        let loaded = load_template_document(&file(json!({"rasters": [], "labels": [{"text": "L"}], "values": [{"value": "speed"}, {"value": "heading"}]}), json!({}))).unwrap();
        assert_eq!(
            loaded.config.widget_ids(),
            ["widget-1", "widget-2", "widget-3"]
        );
        let payload =
            template_file_payload(&loaded.config, &loaded.global_defaults, Some("Ride"), now())
                .unwrap();
        assert_eq!(payload["config"]["values"][1]["id"], "widget-3");
        assert_eq!(payload["format"], TEMPLATE_FILE_FORMAT);
        assert_eq!(payload["version"], 3);
        assert_eq!(payload["name"], "Ride");
        assert_eq!(payload["savedAt"], "2026-11-15T10:00:00.123Z");
        assert_eq!(payload["config"]["rasters"], json!([]));
    }

    #[test]
    fn new_metric_widgets_keep_display_unit_canonical() {
        let speed = create_widget_defaults(
            &WidgetSelection {
                kind: "speed".into(),
                ..Default::default()
            },
            &template_manifest().globals,
            &WidgetCreationOptions::default(),
        )
        .unwrap();
        let config = document(
            json!({"values": [speed, {"id": "g", "value": "gear_position", "display_unit": true}]}),
        );
        let state = create_durable_template_state(&config, None).unwrap();
        assert_eq!(state.config.values[0]["display_unit"], "kmh");
        assert!(
            !state.config.values[1].contains_key("display_unit"),
            "a non-string unit is not persisted"
        );
    }

    #[test]
    fn a_scene_without_a_size_fails() {
        let error = load_error(file(json!({"scene": {"width": 1920}, "rasters": []}), json!({})));
        assert!(error.contains("Template scene height must be a positive number."), "{error}");
    }

    #[test]
    fn every_bundled_template_loads() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates");
        let mut count = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let value: Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            load_template_document(&value)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            count += 1;
        }
        assert!(count > 0);
    }
}
