//! Translates an OpenFreeMap MapLibre style into an ezu recipe, once per spec.

use crate::{MapLayers, TILE_SIZE, VECTOR_SOURCE};
use ezu::translate::maplibre::{convert, ConvertOptions};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Raster relief below z7, which the widget's zoom range never reaches.
const RELIEF_SOURCE: &str = "ne2_shaded";
const VECTOR_MAX_ZOOM: u8 = 14;
const ARROW_ICON: &str = "arrow";
/// MapLibre's default `symbol-spacing`.
const DEFAULT_SYMBOL_SPACING: u32 = 250;

/// Font `url`s in the recipe. Nothing reads them: the rasterizer binds bundled bytes by `url`.
pub(crate) const FONT_REGULAR_URL: &str = "file:NotoSans-Regular.ttf";
pub(crate) const FONT_BOLD_URL: &str = "file:NotoSans-Bold.ttf";
pub(crate) const FONT_ITALIC_URL: &str = "file:NotoSans-Italic.ttf";

/// An ezu recipe and what translating it reported.
pub struct TranslatedStyle {
    pub recipe: String,
    /// The style paints a background layer, so its tiles have no transparent pixels.
    pub opaque: bool,
    /// Approximations made along the way. The style is external data, so these never fail it.
    pub warnings: Vec<String>,
}

/// Applies the layer toggles and label rewrite, then converts to an ezu recipe.
pub fn translate(style_json: &[u8], layers: MapLayers) -> Result<TranslatedStyle, String> {
    let mut style: Value = serde_json::from_slice(style_json)
        .map_err(|error| format!("Map style is not valid JSON: {error}"))?;

    let sources = style
        .get_mut("sources")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "Map style has no sources".to_string())?;
    sources.remove(RELIEF_SOURCE);
    sources
        .get_mut(VECTOR_SOURCE)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("Map style has no {VECTOR_SOURCE} source"))?
        .insert("maxzoom".to_string(), json!(VECTOR_MAX_ZOOM));

    let style_layers = style
        .get_mut("layers")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "Map style has no layers".to_string())?;
    // ezu tiles a fill pattern across the whole canvas even on tiles without that
    // area, which costs about 25 ms per layer per tile for a decorative hatch.
    style_layers.retain(|layer| {
        layer["source"] != RELIEF_SOURCE && layer["paint"]["fill-pattern"].is_null()
    });
    for layer in style_layers.iter_mut() {
        rewrite_text_field(layer);
        if is_hidden(layer, layers) || is_opposite_arrow_layer(layer) {
            layer["layout"]["visibility"] = json!("none");
        }
    }
    let opaque = style_layers
        .iter()
        .any(|layer| layer["type"] == "background");

    // Without this, a fontstack missing from the font map would silently route
    // to a glyph endpoint nothing fetches. Now it surfaces as a warning.
    style
        .as_object_mut()
        .expect("style with sources is an object")
        .remove("glyphs");

    let (mut recipe, mut report) = convert(&style, &convert_options())
        .map_err(|error| format!("Failed to translate map style: {error}"))?;
    let arrow_warnings = stamp_arrows_along_lines(&style, &mut recipe);
    report.warnings.extend(arrow_warnings);
    Ok(TranslatedStyle {
        recipe: recipe.to_string(),
        opaque,
        warnings: report.warnings,
    })
}

fn convert_options() -> ConvertOptions {
    ConvertOptions {
        tile_size: TILE_SIZE,
        fonts: HashMap::from([
            (
                "Noto Sans Regular".to_string(),
                FONT_REGULAR_URL.to_string(),
            ),
            ("Noto Sans Bold".to_string(), FONT_BOLD_URL.to_string()),
            ("Noto Sans Italic".to_string(), FONT_ITALIC_URL.to_string()),
        ]),
        ..ConvertOptions::default()
    }
}

fn is_hidden(layer: &Value, layers: MapLayers) -> bool {
    layers.hides_layer(
        layer["type"].as_str().unwrap_or_default(),
        layer["source-layer"].as_str(),
        layer["id"].as_str().unwrap_or_default(),
    )
}

/// Labels draw from the bundled Latin font, so stacked non-Latin names are dropped.
fn rewrite_text_field(layer: &mut Value) {
    let reads_latin_names = layer
        .get("layout")
        .and_then(|layout| layout.get("text-field"))
        .is_some_and(reads_name_variant);
    if reads_latin_names {
        layer["layout"]["text-field"] = json!([
            "coalesce",
            ["get", "name:latin"],
            ["get", "name_en"],
            ["get", "name"]
        ]);
    }
}

/// One-way arrows: the only line-placed icon layers without text in these styles.
fn is_arrow_layer(layer: &Value) -> bool {
    let layout = &layer["layout"];
    layer["type"] == "symbol"
        && layout["symbol-placement"] == "line"
        && layout["icon-image"].is_string()
        && layout["text-field"].is_null()
}

/// `line-stamp` cannot rotate its image, and a wrong-way arrow is worse than none.
fn is_opposite_arrow_layer(layer: &Value) -> bool {
    is_arrow_layer(layer)
        && layer["id"]
            .as_str()
            .is_some_and(|id| id.ends_with("_opposite"))
}

/// ezu stamps a line-placed icon only at point geometry, so arrows on roads
/// never draw. Its `line-stamp` node walks the line instead, always along the
/// tangent, which only the sprite's `arrow` icon is drawn for: `oneway` relies
/// on `icon-rotate`. The node names follow ezu's converter, under the exact
/// version pin, and a mismatch is reported rather than leaving arrows silently off.
fn stamp_arrows_along_lines(style: &Value, recipe: &mut Value) -> Vec<String> {
    let mut warnings = Vec::new();
    let arrow_layers = style["layers"]
        .as_array()
        .expect("translated style has layers")
        .iter()
        .filter(|layer| is_arrow_layer(layer) && layer["layout"]["visibility"] != "none");
    for layer in arrow_layers {
        let id = layer["id"].as_str().unwrap_or_default();
        let stamp_id = format!("{id}__stamp");
        let icon_id = format!("{id}__icon");
        if recipe["nodes"].get(&stamp_id).is_none() || recipe["nodes"].get(&icon_id).is_none() {
            warnings.push(format!(
                "layer `{id}`: no icon stamp to run along its lines"
            ));
            continue;
        }
        let stamp = &recipe["nodes"][&stamp_id];
        let mut line_stamp = json!({
            "op": "line-stamp",
            "features": stamp["features"],
            "image": stamp["image"],
            "spacing-px": layer["layout"].get("symbol-spacing").unwrap_or(&json!(DEFAULT_SYMBOL_SPACING)),
        });
        if let Some(opacity) = stamp.get("opacity") {
            line_stamp["opacity"] = opacity.clone();
        }
        recipe["nodes"][&stamp_id] = line_stamp;
        recipe["nodes"][&icon_id]["name"] = json!(ARROW_ICON);
    }
    warnings
}

fn reads_name_variant(expression: &Value) -> bool {
    match expression {
        Value::String(text) => text == "name:latin" || text == "name:nonlatin",
        Value::Array(items) => items.iter().any(reads_name_variant),
        _ => false,
    }
}
