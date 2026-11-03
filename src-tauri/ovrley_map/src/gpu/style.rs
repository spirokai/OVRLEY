//! Reads installed styles with `maplibre-expr`, once per renderer.
//!
//! Registration only admits styles MapLibre loads, but this renderer draws less than MapLibre: a
//! layer it cannot read is logged and skipped, while a document with no layers at all fails.

use super::fill_patterns::pattern_index;
use super::pipelines::ShaderDash;
use lyon::tessellation::LineJoin;
use maplibre_expr::{
    evaluate, is_expression, parse_filter, parse_property, typecheck, EvaluationContext, Expr, Type,
    Value,
};
use crate::{MapLayers, MapStyleBuildings, VECTOR_SOURCE};
use serde_json::{json, Value as Json};
use std::borrow::Cow;

/// The source layer every road, path and track comes from.
const TRANSPORTATION_LAYER: &str = "transportation";
/// The class OpenMapTiles gives footways, cycleways, steps and bridleways.
const PATH_CLASS: &str = "path";

/// OVRLEY's extrusion of the `building` source layer, for a style without its own.
fn added_buildings_layer(color: &Json) -> Json {
    json!({
        "id": "added-building-3d",
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
    })
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
        /// Style pixels on the map plane, at the view zoom. The presets only anchor it to the map,
        /// so `fill-translate-anchor` isn't read.
        translate: [f32; 2],
        /// A bundled image that replaces the colour, which then only carries the opacity.
        pattern: Option<u8>,
    },
    Line {
        color: Expr,
        opacity: Option<Expr>,
        /// Output pixels: the style width at the view zoom times the pixel ratio. Per layer, as the
        /// line shader takes it.
        width: f32,
        /// Output pixels, as `width`.
        blur: f32,
        /// Output pixels, as `width`. Draws two lines this far apart.
        gap_width: f32,
        /// Style pixels on the map plane. Only a translated fill's outline moves a line.
        translate: [f32; 2],
        /// A fill's outline, which skips the line geometry that its source layer may also hold.
        polygons_only: bool,
        join: LineJoin,
        dash: ShaderDash,
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
    buildings: &MapStyleBuildings,
) -> Result<ParsedStyle, String> {
    let style: Json = serde_json::from_slice(style_json)
        .map_err(|error| format!("Map style is not valid JSON: {error}"))?;
    let layers = style
        .get("layers")
        .and_then(Json::as_array)
        .ok_or_else(|| "Map style has no layers".to_string())?;

    let keeps_layer = |layer: &Json| match buildings {
        MapStyleBuildings::Own => true,
        MapStyleBuildings::None | MapStyleBuildings::Added(_) => layer["type"] != "fill-extrusion",
    };
    let added = match buildings {
        MapStyleBuildings::Added(color) => Some(added_buildings_layer(color)),
        MapStyleBuildings::None | MapStyleBuildings::Own => None,
    };

    // Added extrusions stand on the style's flat buildings and hide their outlines, which would
    // otherwise add about 30 % to the vertices of the styles that have them.
    let added_extrusions_draw = added
        .as_ref()
        .is_some_and(|layer| is_drawable(layer, toggles, zoom));

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
        let layer = &*without_hidden_features(layer, toggles);
        let id = layer["id"].as_str().unwrap_or("<unnamed>");
        let result = if layer["type"] == "background" {
            parse_background(layer, zoom, next_index).map(|background| {
                parsed.backgrounds.push(background);
            })
        } else {
            parse_vector_layer(layer, id, zoom, pixel_ratio, next_index).and_then(|vector_layer| {
                let outline = if layer["source-layer"] == "building" && added_extrusions_draw {
                    None
                } else {
                    parse_fill_outline(layer, id, zoom, next_index + 1)?
                };
                parsed.vector_layers.push(vector_layer);
                if let Some(outline) = outline {
                    parsed.vector_layers.push(outline);
                    next_index += 1;
                }
                Ok(())
            })
        };
        match result {
            Ok(()) => next_index += 1,
            Err(error) => log::warn!("Skipping map layer {id}: {error}"),
        }
    }
    if let Some(layer) = &added {
        push_added_layer(&mut parsed, layer, toggles, pixel_ratio, next_index)?;
    }
    Ok(parsed)
}

/// The added building layer is OVRLEY's, with a colour registration checked, so a bad one fails.
fn push_added_layer(
    parsed: &mut ParsedStyle,
    layer: &Json,
    toggles: MapLayers,
    pixel_ratio: f32,
    index: u32,
) -> Result<(), String> {
    if is_drawable(layer, toggles, parsed.zoom) {
        let vector_layer =
            parse_vector_layer(layer, "added-building-3d", parsed.zoom, pixel_ratio, index)
                .map_err(|error| format!("Added building layer: {error}"))?;
        parsed.vector_layers.push(vector_layer);
    }
    Ok(())
}

/// A layer narrowed to the features these toggles show. Hidden paths leave every transportation layer,
/// including those that draw them with other roads, so the filter changes rather than the layer list.
/// Every OpenFreeMap style writes its filters as expressions, so the narrowing is one too.
fn without_hidden_features(layer: &Json, toggles: MapLayers) -> Cow<'_, Json> {
    if toggles.paths || layer["source-layer"] != TRANSPORTATION_LAYER {
        return Cow::Borrowed(layer);
    }
    let no_paths = json!(["!=", ["get", "class"], PATH_CLASS]);
    let mut narrowed = layer.clone();
    narrowed["filter"] = match layer.get("filter") {
        Some(filter) => json!(["all", filter, no_paths]),
        None => no_paths,
    };
    Cow::Owned(narrowed)
}

/// Whether this renderer draws a layer at `zoom` under these toggles.
fn is_drawable(layer: &Json, toggles: MapLayers, zoom: f64) -> bool {
    let layer_type = layer["type"].as_str().unwrap_or_default();
    let source_layer = layer["source-layer"].as_str();
    let supported = match layer_type {
        "background" => true,
        "fill" | "line" | "fill-extrusion" => true,
        _ => false,
    };
    supported
        && (layer_type == "background" || layer["source"] == VECTOR_SOURCE)
        && layer["layout"]["visibility"] != "none"
        && !toggles.hides_layer(layer_type, source_layer)
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
    let paint = match layer["type"].as_str() {
        Some("fill") => {
            let pattern = fill_pattern(layer)?;
            VectorPaint::Fill {
                color: match pattern {
                    Some(_) => color_expr("fill-color", &Json::from("#fff"))?,
                    None => required_color(layer, "fill-color")?,
                },
                opacity: number_property(layer, "fill-opacity")?,
                translate: fill_translate(layer, zoom)?,
                pattern,
            }
        }
        Some("line") => {
            let width = zoom_number(layer, "line-width", zoom, 1.0)?;
            VectorPaint::Line {
                color: required_color(layer, "line-color")?,
                opacity: number_property(layer, "line-opacity")?,
                width: width * pixel_ratio,
                blur: zoom_number(layer, "line-blur", zoom, 0.0)? * pixel_ratio,
                gap_width: zoom_number(layer, "line-gap-width", zoom, 0.0)? * pixel_ratio,
                translate: [0.0, 0.0],
                polygons_only: false,
                join: line_join(layer, zoom)?,
                dash: line_dash(layer, zoom, width)?,
            }
        }
        Some("fill-extrusion") => VectorPaint::Extrusion {
            color: required_color(layer, "fill-extrusion-color")?,
            height: number_property(layer, "fill-extrusion-height")?,
            base: number_property(layer, "fill-extrusion-base")?,
        },
        other => return Err(format!("unsupported layer type {other:?}")),
    };
    vector_layer(layer, id, index, paint)
}

/// A fill's `fill-outline-color`, unless `fill-antialias` turns it off. It draws as its own layer
/// directly above the fill, one output pixel wide at any pixel ratio.
///
/// MapLibre also outlines a fill without that property, in its own colour, to smooth its edges. MSAA
/// does that here, and those outlines would cost `bright` 57 % more vertices and 0.18 ms a frame.
fn parse_fill_outline(
    layer: &Json,
    id: &str,
    zoom: f64,
    index: u32,
) -> Result<Option<VectorLayer>, String> {
    if layer["type"] != "fill"
        || layer["paint"].get("fill-pattern").is_some()
        || layer["paint"].get("fill-outline-color").is_none()
        || !fill_antialias(layer, zoom)?
    {
        return Ok(None);
    }
    let paint = VectorPaint::Line {
        color: required_color(layer, "fill-outline-color")?,
        opacity: number_property(layer, "fill-opacity")?,
        width: 1.0,
        blur: 0.0,
        gap_width: 0.0,
        translate: fill_translate(layer, zoom)?,
        polygons_only: true,
        join: LineJoin::Miter,
        dash: ShaderDash::default(),
    };
    vector_layer(layer, id, index, paint).map(Some)
}

fn vector_layer(
    layer: &Json,
    id: &str,
    index: u32,
    paint: VectorPaint,
) -> Result<VectorLayer, String> {
    let source_layer = layer["source-layer"]
        .as_str()
        .ok_or("no source-layer")?
        .to_string();
    let filter = layer
        .get("filter")
        .map(|filter| parse_filter(filter).map_err(|error| format!("filter: {error}")))
        .transpose()?;
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
    color_expr(name, value)
}

fn color_expr(name: &str, value: &Json) -> Result<Expr, String> {
    let expr = parse_property(name, value).map_err(|error| format!("{name}: {error}"))?;
    typecheck(&expr, Some(&Type::Color), false).map_err(|error| format!("{name}: {error}"))
}

fn number_property(layer: &Json, name: &str) -> Result<Option<Expr>, String> {
    layer["paint"]
        .get(name)
        .map(|value| parse_property(name, value).map_err(|error| format!("{name}: {error}")))
        .transpose()
}

/// A number property at the view zoom. `default` is MapLibre's for a missing property.
fn zoom_number(layer: &Json, name: &str, zoom: f64, default: f32) -> Result<f32, String> {
    let Some(expr) = number_property(layer, name)? else {
        return Ok(default);
    };
    let context = EvaluationContext::new().with_zoom(zoom);
    evaluate(&expr, &context)
        .map_err(|error| format!("{name}: {error}"))?
        .as_number()
        .map(|value| value as f32)
        .ok_or_else(|| format!("{name} is not a number"))
}

/// A keyword from `layout` at the view zoom, or `None` when the layer doesn't set it.
fn layout_keyword(layer: &Json, name: &str, zoom: f64) -> Result<Option<String>, String> {
    let Some(value) = layer["layout"].get(name) else {
        return Ok(None);
    };
    let expr = parse_property(name, value).map_err(|error| format!("{name}: {error}"))?;
    let context = EvaluationContext::new().with_zoom(zoom);
    let value = evaluate(&expr, &context).map_err(|error| format!("{name}: {error}"))?;
    value
        .as_str()
        .map(|keyword| Some(keyword.to_string()))
        .ok_or_else(|| format!("{name} is not a keyword"))
}

/// Whether `line-cap` rounds a dash's ends, which the line shader draws. A line's own ends stay butt
/// whatever this says: their geometry cost 8–15 % more vertices for a difference nobody could see.
fn has_round_dashes(layer: &Json, zoom: f64) -> Result<bool, String> {
    match layout_keyword(layer, "line-cap", zoom)?.as_deref() {
        None | Some("butt" | "square") => Ok(false),
        Some("round") => Ok(true),
        Some(other) => Err(format!("line-cap {other} is not a cap")),
    }
}

/// `line-join` at the view zoom. Missing is MapLibre's default of miter.
fn line_join(layer: &Json, zoom: f64) -> Result<LineJoin, String> {
    match layout_keyword(layer, "line-join", zoom)?.as_deref() {
        None | Some("miter") => Ok(LineJoin::Miter),
        Some("round") => Ok(LineJoin::Round),
        Some("bevel") => Ok(LineJoin::Bevel),
        Some(other) => Err(format!("line-join {other} is not a join")),
    }
}

/// `fill-antialias` at the view zoom. Missing is MapLibre's default of on.
fn fill_antialias(layer: &Json, zoom: f64) -> Result<bool, String> {
    let Some(value) = layer["paint"].get("fill-antialias") else {
        return Ok(true);
    };
    let expr = parse_property("fill-antialias", value)
        .map_err(|error| format!("fill-antialias: {error}"))?;
    let context = EvaluationContext::new().with_zoom(zoom);
    evaluate(&expr, &context)
        .map_err(|error| format!("fill-antialias: {error}"))?
        .as_bool()
        .ok_or_else(|| "fill-antialias is not a boolean".to_string())
}

/// The bundled image of `fill-pattern`. An image that isn't bundled skips the layer, because a solid
/// fill in its place would be wrong. That includes `dark`'s `wood-pattern`, which its sprite lacks.
fn fill_pattern(layer: &Json) -> Result<Option<u8>, String> {
    let Some(value) = layer["paint"].get("fill-pattern") else {
        return Ok(None);
    };
    let name = value.as_str().ok_or("fill-pattern is not an image name")?;
    pattern_index(name)
        .map(Some)
        .ok_or_else(|| format!("fill-pattern {name} is not bundled"))
}

/// `fill-translate` at the view zoom. Missing is MapLibre's default of no translation.
fn fill_translate(layer: &Json, zoom: f64) -> Result<[f32; 2], String> {
    let Some(numbers) = zoom_numbers(layer, "fill-translate", zoom)? else {
        return Ok([0.0, 0.0]);
    };
    match numbers[..] {
        [x, y] => Ok([x as f32, y as f32]),
        _ => Err("fill-translate is not two numbers".to_string()),
    }
}

/// `line-dasharray` at the view zoom, whose values count line widths of `width` style pixels. Every
/// preset's is one dash and one gap. No gap is a solid line, and a longer pattern isn't readable.
fn line_dash(layer: &Json, zoom: f64, width: f32) -> Result<ShaderDash, String> {
    let Some(numbers) = zoom_numbers(layer, "line-dasharray", zoom)? else {
        return Ok(ShaderDash::default());
    };
    let [on, off] = numbers[..] else {
        return Err(format!(
            "line-dasharray has {} values, not a dash and a gap",
            numbers.len()
        ));
    };
    if on < 0.0 || off < 0.0 {
        return Err("line-dasharray has a negative value".to_string());
    }
    if off == 0.0 {
        return Ok(ShaderDash::default());
    }
    Ok(ShaderDash {
        on: on as f32 * width,
        off: off as f32 * width,
        cap_radius: if has_round_dashes(layer, zoom)? { width * 0.5 } else { 0.0 },
    })
}

/// A paint property holding an array of numbers, at the view zoom.
fn zoom_numbers(layer: &Json, name: &str, zoom: f64) -> Result<Option<Vec<f64>>, String> {
    let Some(value) = layer["paint"].get(name) else {
        return Ok(None);
    };
    let not_numbers = || format!("{name} is not an array of numbers");
    // A bare array isn't an expression, so it never reaches the expression parser.
    if value.is_array() && !is_expression(value) {
        return value
            .as_array()
            .and_then(|values| values.iter().map(Json::as_f64).collect())
            .map(Some)
            .ok_or_else(not_numbers);
    }
    let expr = parse_property(name, value).map_err(|error| format!("{name}: {error}"))?;
    let context = EvaluationContext::new().with_zoom(zoom);
    let numbers = match evaluate(&expr, &context).map_err(|error| format!("{name}: {error}"))? {
        Value::NumberArray(numbers) => Some(numbers),
        Value::Array(values) => values.iter().map(Value::as_number).collect(),
        _ => None,
    };
    numbers.map(Some).ok_or_else(not_numbers)
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

#[cfg(test)]
mod tests {
    use super::*;
    use maplibre_expr::Feature;
    use std::collections::BTreeMap;

    const SHOWN: MapLayers = MapLayers {
        buildings: true,
        labels: false,
        paths: true,
    };
    const HIDDEN: MapLayers = MapLayers {
        paths: false,
        ..SHOWN
    };

    fn passes(layer: &Json, class: &str) -> bool {
        let filter = parse_filter(&layer["filter"]).unwrap();
        let mut context = EvaluationContext::new().with_zoom(14.0);
        context.feature = Feature {
            properties: BTreeMap::from([("class".to_string(), Value::String(class.into()))]),
            geometry_type: Some("LineString".into()),
            ..Feature::default()
        };
        passes_filter(Some(&filter), &context)
    }

    #[test]
    fn hidden_paths_leave_transportation_layers_that_keep_their_other_roads() {
        let layer = json!({
            "source-layer": "transportation",
            "filter": ["match", ["get", "class"], ["minor", "path"], true, false],
        });
        let narrowed = without_hidden_features(&layer, HIDDEN);
        assert!(passes(&narrowed, "minor"));
        assert!(!passes(&narrowed, "path"));
        assert!(passes(&without_hidden_features(&layer, SHOWN), "path"));

        let unfiltered = json!({ "source-layer": "transportation" });
        let unfiltered = without_hidden_features(&unfiltered, HIDDEN);
        assert!(passes(&unfiltered, "track"));
        assert!(!passes(&unfiltered, "path"));
    }

    #[test]
    fn hidden_paths_leave_other_source_layers_alone() {
        let water = json!({ "source-layer": "water", "filter": ["==", ["get", "class"], "path"] });
        assert!(matches!(without_hidden_features(&water, HIDDEN), Cow::Borrowed(_)));
    }
}
