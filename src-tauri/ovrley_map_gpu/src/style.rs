//! Reads OpenFreeMap styles with `maplibre-expr`, once per renderer.
//!
//! Styles are external data: a layer this renderer cannot read is logged and skipped, while a
//! document with no layers at all fails.

use maplibre_expr::{
    evaluate, parse_filter, parse_property, typecheck, EvaluationContext, Expr, Type, Value,
};
use ovrley_map::{MapLayers, MapStyle, VECTOR_SOURCE};
use serde_json::Value as Json;

/// Which 3D buildings a renderer draws.
#[derive(Clone, Debug)]
pub enum Buildings {
    /// No extrusions. Flat building layers still draw if the style has them.
    None,
    /// The style's own `fill-extrusion` layers.
    FromStyle,
    /// One `fill-extrusion` style layer, for styles without one. Replaces the style's own.
    Fixed(Json),
}

impl Buildings {
    /// Real 3D buildings for a preset: Liberty has its own, the others get the `building` source
    /// layer with a fixed extrusion, without touching the styles.
    pub fn for_style(style: MapStyle) -> Self {
        let color = match style {
            MapStyle::Liberty => return Self::FromStyle,
            MapStyle::Positron => "rgb(228,228,222)",
            MapStyle::Bright => "hsl(35,8%,85%)",
            MapStyle::Dark => "rgb(44,44,47)",
            MapStyle::Fiord => "hsl(222,23%,30%)",
        };
        Self::Fixed(serde_json::json!({
            "id": "fixed-building-3d",
            "type": "fill-extrusion",
            "source": VECTOR_SOURCE,
            "source-layer": "building",
            "minzoom": 14,
            "filter": ["!", ["has", "hide_3d"]],
            "paint": {
                "fill-extrusion-color": color,
                "fill-extrusion-height": ["get", "render_height"],
                "fill-extrusion-base": ["get", "render_min_height"],
            },
        }))
    }
}

/// The drawable part of one style, with zoom-dependent layer values evaluated at `zoom`.
pub(crate) struct ParsedStyle {
    pub(crate) zoom: f64,
    pub(crate) backgrounds: Vec<BackgroundLayer>,
    pub(crate) vector_layers: Vec<VectorLayer>,
}

pub(crate) struct BackgroundLayer {
    /// Draw order among all layers.
    pub(crate) index: u32,
    pub(crate) color: [f32; 4],
}

pub(crate) struct VectorLayer {
    pub(crate) id: String,
    /// Draw order among all layers.
    pub(crate) index: u32,
    pub(crate) source_layer: String,
    pub(crate) filter: Option<Expr>,
    pub(crate) paint: VectorPaint,
}

pub(crate) enum VectorPaint {
    Fill {
        color: Expr,
        opacity: Option<Expr>,
    },
    Line {
        color: Expr,
        opacity: Option<Expr>,
        /// Output pixels: the style width at the view zoom times the pixel ratio. Per layer, as the
        /// line shader takes it.
        width: f32,
    },
    Extrusion {
        color: Expr,
        height: Option<Expr>,
        base: Option<Expr>,
    },
}

/// Parses every layer drawable at `zoom`.
pub(crate) fn parse_style(
    style_json: &[u8],
    toggles: MapLayers,
    zoom: f64,
    pixel_ratio: f32,
    buildings: &Buildings,
) -> Result<ParsedStyle, String> {
    let style: Json = serde_json::from_slice(style_json)
        .map_err(|error| format!("Map style is not valid JSON: {error}"))?;
    let layers = style
        .get("layers")
        .and_then(Json::as_array)
        .ok_or_else(|| "Map style has no layers".to_string())?;

    let keeps_layer = |layer: &Json| match buildings {
        Buildings::FromStyle => true,
        Buildings::None | Buildings::Fixed(_) => layer["type"] != "fill-extrusion",
    };

    let mut parsed = ParsedStyle {
        zoom,
        backgrounds: Vec::new(),
        vector_layers: Vec::new(),
    };
    let mut next_index = 1;
    for layer in layers.iter().filter(|layer| keeps_layer(layer)) {
        if !is_drawable(layer, toggles, zoom) {
            continue;
        }
        let id = layer["id"].as_str().unwrap_or("<unnamed>");
        let result = if layer["type"] == "background" {
            parse_background(layer, zoom, next_index).map(|background| {
                parsed.backgrounds.push(background);
            })
        } else {
            parse_vector_layer(layer, id, zoom, pixel_ratio, next_index).map(|vector_layer| {
                parsed.vector_layers.push(vector_layer);
            })
        };
        match result {
            Ok(()) => next_index += 1,
            Err(error) => log::warn!("Skipping map layer {id}: {error}"),
        }
    }
    if let Buildings::Fixed(layer) = buildings {
        push_fixed_layer(&mut parsed, layer, toggles, pixel_ratio, next_index)?;
    }
    Ok(parsed)
}

/// The fixed building layer is our configuration, not external style data, so a bad one fails.
fn push_fixed_layer(
    parsed: &mut ParsedStyle,
    layer: &Json,
    toggles: MapLayers,
    pixel_ratio: f32,
    index: u32,
) -> Result<(), String> {
    let id = layer["id"].as_str().unwrap_or("<unnamed>");
    if layer["type"] != "fill-extrusion" || layer["source"] != VECTOR_SOURCE {
        return Err(format!(
            "Fixed building layer {id} must be a fill-extrusion layer of {VECTOR_SOURCE}"
        ));
    }
    if is_drawable(layer, toggles, parsed.zoom) {
        let vector_layer = parse_vector_layer(layer, id, parsed.zoom, pixel_ratio, index)
            .map_err(|error| format!("Fixed building layer {id}: {error}"))?;
        parsed.vector_layers.push(vector_layer);
    }
    Ok(())
}

/// Whether this renderer draws a layer at `zoom` under these toggles.
fn is_drawable(layer: &Json, toggles: MapLayers, zoom: f64) -> bool {
    let layer_type = layer["type"].as_str().unwrap_or_default();
    let source_layer = layer["source-layer"].as_str();
    let id = layer["id"].as_str().unwrap_or_default();
    let supported = match layer_type {
        "background" => true,
        // Patterns are out of scope, and a solid fill in their place would be wrong.
        "fill" => layer["paint"]["fill-pattern"].is_null(),
        "line" | "fill-extrusion" => true,
        _ => false,
    };
    supported
        && (layer_type == "background" || layer["source"] == VECTOR_SOURCE)
        && layer["layout"]["visibility"] != "none"
        && !toggles.hides_layer(layer_type, source_layer, id)
        && layer["minzoom"]
            .as_f64()
            .is_none_or(|minzoom| zoom >= minzoom)
        && layer["maxzoom"]
            .as_f64()
            .is_none_or(|maxzoom| zoom < maxzoom)
}

fn parse_background(layer: &Json, zoom: f64, index: u32) -> Result<BackgroundLayer, String> {
    let color = required_color(layer, "background-color")?;
    let context = EvaluationContext::new().with_zoom(zoom);
    let color = evaluate_color(&color, &context, 1.0)
        .ok_or("background-color does not evaluate to a colour")?;
    Ok(BackgroundLayer { index, color })
}

fn parse_vector_layer(
    layer: &Json,
    id: &str,
    zoom: f64,
    pixel_ratio: f32,
    index: u32,
) -> Result<VectorLayer, String> {
    let source_layer = layer["source-layer"]
        .as_str()
        .ok_or("no source-layer")?
        .to_string();
    let filter = layer
        .get("filter")
        .map(|filter| parse_filter(filter).map_err(|error| format!("filter: {error}")))
        .transpose()?;
    let paint = match layer["type"].as_str() {
        Some("fill") => VectorPaint::Fill {
            color: required_color(layer, "fill-color")?,
            opacity: number_property(layer, "fill-opacity")?,
        },
        Some("line") => VectorPaint::Line {
            color: required_color(layer, "line-color")?,
            opacity: number_property(layer, "line-opacity")?,
            width: line_width(layer, zoom)? * pixel_ratio,
        },
        Some("fill-extrusion") => VectorPaint::Extrusion {
            color: required_color(layer, "fill-extrusion-color")?,
            height: number_property(layer, "fill-extrusion-height")?,
            base: number_property(layer, "fill-extrusion-base")?,
        },
        other => return Err(format!("unsupported layer type {other:?}")),
    };
    Ok(VectorLayer {
        id: id.to_string(),
        index,
        source_layer,
        filter,
        paint,
    })
}

fn required_color(layer: &Json, name: &str) -> Result<Expr, String> {
    let value = layer["paint"]
        .get(name)
        .ok_or_else(|| format!("no {name}"))?;
    let expr = parse_property(name, value).map_err(|error| format!("{name}: {error}"))?;
    typecheck(&expr, Some(&Type::Color), false).map_err(|error| format!("{name}: {error}"))
}

fn number_property(layer: &Json, name: &str) -> Result<Option<Expr>, String> {
    layer["paint"]
        .get(name)
        .map(|value| parse_property(name, value).map_err(|error| format!("{name}: {error}")))
        .transpose()
}

/// `line-width` at the view zoom. A missing width is MapLibre's default of 1.
fn line_width(layer: &Json, zoom: f64) -> Result<f32, String> {
    let Some(expr) = number_property(layer, "line-width")? else {
        return Ok(1.0);
    };
    let context = EvaluationContext::new().with_zoom(zoom);
    evaluate(&expr, &context)
        .map_err(|error| format!("line-width: {error}"))?
        .as_number()
        .map(|width| width as f32)
        .ok_or_else(|| "line-width is not a number".to_string())
}

/// Evaluates a colour for a feature, as straight-alpha floats, with `opacity` applied.
pub(crate) fn evaluate_color(
    color: &Expr,
    context: &EvaluationContext,
    opacity: f64,
) -> Option<[f32; 4]> {
    match evaluate(color, context) {
        Ok(Value::Color(color)) => Some([
            color.r as f32,
            color.g as f32,
            color.b as f32,
            (color.a * opacity) as f32,
        ]),
        _ => None,
    }
}

/// Evaluates an optional number for a feature. Absence and failures read as the style default.
pub(crate) fn evaluate_number(
    expr: Option<&Expr>,
    context: &EvaluationContext,
    default: f64,
) -> f64 {
    expr.and_then(|expr| evaluate(expr, context).ok())
        .and_then(|value| value.as_number())
        .unwrap_or(default)
}

/// A filter passes only when it evaluates to `true`, as in MapLibre.
pub(crate) fn passes_filter(filter: Option<&Expr>, context: &EvaluationContext) -> bool {
    filter.is_none_or(|filter| matches!(evaluate(filter, context), Ok(Value::Bool(true))))
}
