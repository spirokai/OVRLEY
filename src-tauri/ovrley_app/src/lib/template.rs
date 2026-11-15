//! Port of the editor-effective part of `lib/template/template-state.js`
//! (`createEditorEffectiveConfig`, `getEffectiveWidgetData`): global defaults resolved into widget
//! data for rendering and editing. It stays in the app because chart legends are named in the app
//! language (D3). Durable normalization is `ovrley_core::normalize::template`.

use crate::i18n::{catalog as translations, Locale};
use crate::lib::fonts::{font_family_name, font_selection};
use ovrley_core::normalize::template::{normalize_global_defaults, template_manifest};
use ovrley_widgets::model::catalog::catalog;
use ovrley_widgets::model::document::{pick_defined, resolve_active_widget_data, Collection, OverlayConfig, WidgetData};
use serde_json::{Map, Value};
#[cfg(test)]
use serde_json::json;

/// `getThemeColor('ice')` and `getThemeColor('aqua')` (`index.css`).
const ICE: &str = "#86aaaa";
const AQUA: &str = "#40e0d0";

/// JavaScript truthiness of an optional value.
fn truthy(value: Option<&Value>) -> Option<&Value> {
    value.filter(|value| match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(text) => !text.is_empty(),
        _ => true,
    })
}

fn or_color(value: Option<&Value>, fallback: &str) -> Value {
    truthy(value).cloned().unwrap_or_else(|| Value::String(fallback.into()))
}

/// The effective scene: app defaults, the scene, then the global defaults.
pub fn effective_scene(scene: &Map<String, Value>, globals: &Map<String, Value>) -> Map<String, Value> {
    let defaults = &template_manifest().globals;
    let mut effective = defaults.clone();
    for (key, value) in scene {
        effective.insert(key.clone(), value.clone());
    }
    for (key, value) in pick_defined(Some(globals), &defaults.keys().cloned().collect::<Vec<_>>()) {
        effective.insert(key, value);
    }
    effective
}

fn set_font(data: &mut Map<String, Value>, resolved: &Map<String, Value>, global: Option<&Value>) {
    let font = truthy(resolved.get("font")).or(global).cloned();
    if truthy(data.get("font")).is_none() {
        if let Some(font) = truthy(font.as_ref()) {
            data.insert("font".into(), font.clone());
        }
    }
    if truthy(data.get("font_family")).is_none() {
        let source = truthy(font.as_ref()).or_else(|| data.get("font_family")).and_then(Value::as_str).unwrap_or("");
        data.insert("font_family".into(), Value::String(font_family_name(source)));
    }
}

fn effective_label(data: &WidgetData, globals: &Map<String, Value>) -> WidgetData {
    let mut next = data.clone();
    set_font(&mut next, data, globals.get("font_text"));
    if truthy(next.get("color")).is_none() {
        next.insert("color".into(), or_color(globals.get("color_text"), ICE));
    }
    if !next.contains_key("opacity") {
        next.insert("opacity".into(), globals.get("opacity").cloned().unwrap_or(Value::Null));
    }
    next
}

fn effective_value(data: &WidgetData, globals: &Map<String, Value>) -> WidgetData {
    let resolved = resolve_active_widget_data(data);
    let mut next = resolved.clone();
    set_font(&mut next, &resolved, globals.get("font_values"));
    if truthy(next.get("min_max_label_font")).is_none() {
        if let Some(font) = truthy(resolved.get("min_max_label_font")).or_else(|| truthy(globals.get("font_values"))) {
            next.insert("min_max_label_font".into(), font.clone());
        }
    }
    if truthy(next.get("color")).is_none() {
        next.insert("color".into(), or_color(globals.get("color_values"), ICE));
    }
    let kind = resolved.get("value").and_then(Value::as_str).unwrap_or_default();
    if kind == "lap_timer" || kind == "splits_run" {
        if truthy(next.get("label_font")).is_none() {
            next.insert("label_font".into(), globals.get("font_text").cloned().unwrap_or(Value::Null));
        }
        if truthy(next.get("label_color")).is_none() {
            next.insert("label_color".into(), or_color(globals.get("color_text"), ICE));
        }
    }
    if resolved.get("display_type").and_then(Value::as_str) == Some("heading_tape") && next.get("label_font").is_none_or(Value::is_null) {
        next.insert("label_font".into(), globals.get("font_values").cloned().unwrap_or(Value::Null));
    }
    if !next.contains_key("icon_color") {
        next.insert("icon_color".into(), or_color(globals.get("color_icons"), AQUA));
    }
    if !next.contains_key("unit_color") && kind != "time" {
        next.insert("unit_color".into(), or_color(globals.get("color_units"), "#ffffff"));
    }
    if !next.contains_key("opacity") {
        next.insert("opacity".into(), globals.get("opacity").cloned().unwrap_or(Value::Null));
    }
    next
}

/// Chart legends name each metric in the app language at render time, upper-cased.
fn chart_legend_series(series: &Value, language: Locale) -> Value {
    let Some(rows) = series.as_array() else { return series.clone() };
    Value::Array(
        rows.iter()
            .map(|row| {
                let mut row = row.as_object().cloned().unwrap_or_default();
                let label = row
                    .get("metric")
                    .and_then(Value::as_str)
                    .and_then(|metric| catalog().widget(metric))
                    .map(|definition| translations().t(language, &definition.short_name_key).to_uppercase())
                    .unwrap_or_default();
                row.insert("label".into(), Value::String(label));
                Value::Object(row)
            })
            .collect(),
    )
}

fn effective_plot(data: &WidgetData, globals: &Map<String, Value>, language: Locale) -> WidgetData {
    let mut next = data.clone();
    let kind = data.get("value").and_then(Value::as_str).unwrap_or_default();
    if kind == "rolling_chart" {
        let series = chart_legend_series(next.get("series").unwrap_or(&Value::Null), language);
        next.insert("series".into(), series);
    }
    if truthy(next.get("color")).is_none() {
        next.insert("color".into(), or_color(globals.get("color_values"), ICE));
    }
    if !next.contains_key("opacity") {
        next.insert("opacity".into(), globals.get("opacity").cloned().unwrap_or(Value::Null));
    }
    if (kind == "elevation" || kind == "diving_profile") && truthy(globals.get("font_values")).is_some() {
        let mut label = next.get("point_label").and_then(Value::as_object).cloned().unwrap_or_default();
        if truthy(label.get("font")).is_none() {
            let font = globals["font_values"].as_str().unwrap_or_default();
            for (key, value) in font_selection(font) {
                label.insert(key, value);
            }
        }
        next.insert("point_label".into(), Value::Object(label));
    }
    next
}

/// `getEffectiveWidgetData`: one widget's render-effective data. `globals` are normalized.
pub fn effective_widget_data(collection: Collection, data: &WidgetData, globals: &Map<String, Value>, language: Locale) -> WidgetData {
    match collection {
        Collection::Backdrops => resolve_active_widget_data(data),
        Collection::Labels => effective_label(data, globals),
        Collection::Values => effective_value(data, globals),
        Collection::Plots => effective_plot(data, globals, language),
        Collection::Rasters => data.clone(),
    }
}

/// `createEditorEffectiveConfig`.
pub fn editor_effective_config(config: &OverlayConfig, global_defaults: &Map<String, Value>, language: Locale) -> OverlayConfig {
    let globals = normalize_global_defaults(Some(global_defaults));
    let mut effective = OverlayConfig {
        scene: effective_scene(&config.scene, &globals),
        ..OverlayConfig::default()
    };
    for collection in Collection::ALL {
        *effective.collection_mut(collection) = config.collection(collection).iter().map(|data| effective_widget_data(collection, data, &globals, language)).collect();
    }
    effective
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    fn globals() -> Map<String, Value> {
        normalize_global_defaults(Some(&map(json!({"font_text": "Text.ttf", "font_values": "Value.ttf", "color_text": "#111111",
            "color_values": "#222222", "color_icons": "#333333", "color_units": "#444444", "opacity": 0.5}))))
    }

    #[test]
    fn global_defaults_resolve_into_labels_values_and_plots() {
        let config = OverlayConfig::from_value(&json!({
            "scene": {"width": 1920},
            "labels": [{"id": "l", "text": "Ride"}],
            "values": [{"id": "v", "value": "speed"}, {"id": "t", "value": "time"}],
            "plots": [{"id": "p", "value": "elevation", "point_label": {}}]
        }))
        .unwrap();
        let effective = editor_effective_config(&config, &globals(), Locale::En);
        assert_eq!(effective.scene["width"], 1920);
        assert_eq!(effective.scene["opacity"], 0.5);
        let label = &effective.labels[0];
        assert_eq!((label["font"].clone(), label["font_family"].clone(), label["color"].clone()), (json!("Text.ttf"), json!("Text"), json!("#111111")));
        let value = &effective.values[0];
        assert_eq!(value["font"], "Value.ttf");
        assert_eq!(value["icon_color"], "#333333");
        assert_eq!(value["unit_color"], "#444444");
        assert!(!effective.values[1].contains_key("unit_color"), "time widgets have no unit colour");
        assert_eq!(effective.plots[0]["point_label"]["font"], "Value.ttf");
        assert_eq!(effective.plots[0]["color"], "#222222");
    }

    #[test]
    fn values_resolve_their_active_display() {
        let data = map(json!({"id": "h", "value": "heading", "display_type": "heading_tape", "display_variants": {"heading_tape": {"width": 500}}}));
        let effective = effective_widget_data(Collection::Values, &data, &globals(), Locale::En);
        assert_eq!(effective["width"], 500);
        assert_eq!(effective["label_font"], "Value.ttf", "an absent heading label font takes the value font");
    }

    #[test]
    fn chart_legends_are_named_in_the_app_language() {
        let data = map(json!({"id": "c", "value": "rolling_chart", "series": [{"metric": "speed"}]}));
        let effective = effective_widget_data(Collection::Plots, &data, &globals(), Locale::En);
        let expected = translations().t(Locale::En, &catalog().require_widget("speed").unwrap().short_name_key).to_uppercase();
        assert_eq!(effective["series"][0]["label"], Value::String(expected));
    }

    #[test]
    fn explicit_widget_values_win() {
        let data = map(json!({"id": "l", "text": "x", "font": "Own.ttf", "color": "#abcdef", "opacity": 1}));
        let effective = effective_widget_data(Collection::Labels, &data, &globals(), Locale::En);
        assert_eq!((effective["font"].clone(), effective["color"].clone(), effective["opacity"].clone()), (json!("Own.ttf"), json!("#abcdef"), json!(1)));
    }
}
