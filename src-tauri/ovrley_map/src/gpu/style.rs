//! Reads installed styles with `maplibre-expr`, once per renderer.
//!
//! Registration only admits styles MapLibre loads, but this renderer draws less than MapLibre: a
//! layer it cannot read is logged and skipped, while a document with no layers at all fails.

use super::fill_patterns::pattern_index;
use super::labels::{is_label_source, parse_label_layer, LabelLayer};
use super::pipelines::ShaderDash;
use crate::{data_level, MapLayers, MapStyleBuildings, VECTOR_SOURCE};
use lyon::tessellation::LineJoin;
use maplibre_expr::{
    evaluate, is_expression, parse_filter, parse_property, typecheck, EvaluationContext, Expr,
    Type, Value,
};
use serde_json::{json, Value as Json};
use std::borrow::Cow;

/// The source layer every road, path and track comes from.
const TRANSPORTATION_LAYER: &str = "transportation";
/// The source layer of road, path and track names.
const TRANSPORTATION_NAME_LAYER: &str = "transportation_name";
/// The class OpenMapTiles gives footways, cycleways, steps and bridleways.
const PATH_CLASS: &str = "path";
/// The transportation classes that keep a style's dashes: railways, transit and ferry routes.
const DASHED_CLASSES: [&str; 3] = ["rail", "transit", "ferry"];

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
    /// Tile units to one output pixel on the tile level drawn at `zoom`, which is what line widths
    /// tessellate against.
    pub(crate) tile_units_per_pixel: f32,
    pub(crate) backgrounds: Vec<BackgroundLayer>,
    pub(crate) vector_layers: Vec<VectorLayer>,
    /// Symbol layers whose names draw, in style order.
    pub(crate) labels: Vec<LabelLayer>,
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
        // A 4096-unit tile spans 512 style pixels on its own level.
        tile_units_per_pixel: (8.0 * 2_f64.powf(f64::from(data_level(zoom)) - zoom)) as f32
            / pixel_ratio,
        backgrounds: Vec::new(),
        vector_layers: Vec::new(),
        labels: Vec::new(),
    };
    let mut next_index = 1;
    for layer in layers.iter().filter(|layer| keeps_layer(layer)) {
        if !is_drawable(layer, toggles, zoom) {
            continue;
        }
        let layer = without_hidden_features(layer, toggles);
        let variants = match with_solid_road_dashes(&layer, zoom) {
            Ok(variants) => variants,
            Err(error) => {
                let id = layer["id"].as_str().unwrap_or("<unnamed>");
                log::warn!("Skipping map layer {id}: {error}");
                continue;
            }
        };
        for (layer, width_share) in &variants {
            let id = layer["id"].as_str().unwrap_or("<unnamed>");
            let result = if layer["type"] == "background" {
                parse_background(layer, zoom, next_index).map(|background| {
                    parsed.backgrounds.push(background);
                })
            } else if layer["type"] == "symbol" {
                parse_label_layer(layer, id, zoom, pixel_ratio, next_index)
                    .map(|label| parsed.labels.push(label))
            } else {
                parse_vector_layer(layer, id, zoom, pixel_ratio, next_index).and_then(
                    |mut vector_layer| {
                        if let VectorPaint::Line { width, .. } = &mut vector_layer.paint {
                            *width *= width_share;
                        }
                        let outline =
                            if layer["source-layer"] == "building" && added_extrusions_draw {
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
                    },
                )
            };
            match result {
                Ok(()) => next_index += 1,
                Err(error) => log::warn!("Skipping map layer {id}: {error}"),
            }
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
/// and the names of paths leave every transportation name layer, including those that draw them with
/// other roads, so the filter changes rather than the layer list.
/// Every OpenFreeMap style writes its filters as expressions, so the narrowing is one too.
fn without_hidden_features(layer: &Json, toggles: MapLayers) -> Cow<'_, Json> {
    if toggles.paths
        || ![TRANSPORTATION_LAYER, TRANSPORTATION_NAME_LAYER]
            .iter()
            .any(|name| layer["source-layer"] == *name)
    {
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

/// A layer as it draws, each with the share of its width it keeps. A dashed transportation line
/// splits into its roads, drawn solid, its paths, drawn solid and thinner, and the classes whose
/// dashes mean something, which keep them. Dashes crawl as the camera moves and shimmer where a
/// pitched view packs them tight, and a road reads as well without them. The split runs on `class`,
/// so it holds for any style however it names or groups its layers.
fn with_solid_road_dashes(layer: &Json, zoom: f64) -> Result<Vec<(Cow<'_, Json>, f32)>, String> {
    if layer["type"] != "line"
        || layer["source-layer"] != TRANSPORTATION_LAYER
        || layer["paint"].get("line-dasharray").is_none()
    {
        return Ok(vec![(Cow::Borrowed(layer), 1.0)]);
    }
    // A solid path keeps the ink its dashes had, so it stays below the roads it runs beside. Only
    // paths narrow: a dashed road is usually a tunnel's casing, which would vanish under its road.
    let dash = line_dash(layer, zoom, 1.0)?;
    let period = dash.on + dash.off;
    let path_width_share = if period > 0.0 {
        ((dash.on + 2.0 * dash.cap_radius) / period).min(1.0)
    } else {
        1.0
    };
    let class_in = |classes: Json| json!(["match", ["get", "class"], classes, true, false]);
    let narrowed = |class_filter: Json| {
        let mut narrowed = layer.clone();
        narrowed["filter"] = match layer.get("filter") {
            Some(filter) => json!(["all", filter, class_filter]),
            None => class_filter,
        };
        narrowed
    };
    let solid = |class_filter: Json| {
        let mut solid = narrowed(class_filter);
        solid["paint"]
            .as_object_mut()
            .expect("a layer with line-dasharray has a paint object")
            .remove("line-dasharray");
        Cow::Owned(solid)
    };
    let mut not_roads = DASHED_CLASSES.to_vec();
    not_roads.push(PATH_CLASS);
    Ok(vec![
        (solid(json!(["!", class_in(json!(not_roads))])), 1.0),
        (solid(class_in(json!([PATH_CLASS]))), path_width_share),
        (Cow::Owned(narrowed(class_in(json!(DASHED_CLASSES)))), 1.0),
    ])
}

/// Whether this renderer draws a layer at `zoom` under these toggles.
fn is_drawable(layer: &Json, toggles: MapLayers, zoom: f64) -> bool {
    let layer_type = layer["type"].as_str().unwrap_or_default();
    let source_layer = layer["source-layer"].as_str();
    (matches!(
        layer_type,
        "background" | "fill" | "line" | "fill-extrusion"
    ) || (layer_type == "symbol" && is_label_source(source_layer)))
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

pub(crate) fn color_expr(name: &str, value: &Json) -> Result<Expr, String> {
    let expr = parse_property(name, value).map_err(|error| format!("{name}: {error}"))?;
    typecheck(&expr, Some(&Type::Color), false).map_err(|error| format!("{name}: {error}"))
}

pub(crate) fn number_property(layer: &Json, name: &str) -> Result<Option<Expr>, String> {
    layer["paint"]
        .get(name)
        .map(|value| parse_property(name, value).map_err(|error| format!("{name}: {error}")))
        .transpose()
}

/// A number property at the view zoom. `default` is MapLibre's for a missing property.
fn zoom_number(layer: &Json, name: &str, zoom: f64, default: f32) -> Result<f32, String> {
    let value = zoom_value(layer, "paint", name, zoom)?;
    number(&value, name, f64::from(default)).map(|value| value as f32)
}

/// A keyword from `layout` at the view zoom, or `None` when the layer doesn't set it.
fn layout_keyword(layer: &Json, name: &str, zoom: f64) -> Result<Option<String>, String> {
    let value = zoom_value(layer, "layout", name, zoom)?;
    Ok(keyword(&value, name)?.map(str::to_string))
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
    boolean(
        &zoom_value(layer, "paint", "fill-antialias", zoom)?,
        "fill-antialias",
        true,
    )
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
        cap_radius: if has_round_dashes(layer, zoom)? {
            width * 0.5
        } else {
            0.0
        },
    })
}

/// A paint property holding an array of numbers, at the view zoom.
fn zoom_numbers(layer: &Json, name: &str, zoom: f64) -> Result<Option<Vec<f64>>, String> {
    zoom_value(layer, "paint", name, zoom)?
        .map(|value| numbers(&value).ok_or_else(|| format!("{name} is not an array of numbers")))
        .transpose()
}

/// A property of a `section` (`layout` or `paint`) that depends on zoom alone, at the view zoom, or
/// `None` when the layer doesn't set it.
pub(crate) fn zoom_value(
    layer: &Json,
    section: &str,
    name: &str,
    zoom: f64,
) -> Result<Option<Value>, String> {
    let Some(raw) = layer[section].get(name) else {
        return Ok(None);
    };
    // A bare array isn't an expression, so it never reaches the expression parser.
    if raw.is_array() && !is_expression(raw) {
        return Ok(Some(Value::from_json(raw)));
    }
    let expr = parse_property(name, raw).map_err(|error| format!("{name}: {error}"))?;
    evaluate_at_zoom(&expr, name, zoom).map(Some)
}

/// Evaluates `expr` at the view zoom, for a property that doesn't read feature data.
pub(crate) fn evaluate_at_zoom(expr: &Expr, name: &str, zoom: f64) -> Result<Value, String> {
    evaluate(expr, &EvaluationContext::new().with_zoom(zoom))
        .map_err(|error| format!("{name}: {error}"))
}

pub(crate) fn keyword<'a>(value: &'a Option<Value>, name: &str) -> Result<Option<&'a str>, String> {
    value
        .as_ref()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| format!("{name} is not a keyword"))
        })
        .transpose()
}

pub(crate) fn number(value: &Option<Value>, name: &str, default: f64) -> Result<f64, String> {
    match value {
        None => Ok(default),
        Some(value) => value
            .as_number()
            .ok_or_else(|| format!("{name} is not a number")),
    }
}

pub(crate) fn boolean(value: &Option<Value>, name: &str, default: bool) -> Result<bool, String> {
    match value {
        None => Ok(default),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("{name} is not a boolean")),
    }
}

/// A pair of numbers, which is zero when the layer doesn't set the property.
pub(crate) fn pair(value: &Option<Value>, name: &str) -> Result<[f64; 2], String> {
    match value {
        None => Ok([0.0, 0.0]),
        Some(value) => numbers(value)
            .and_then(|numbers| <[f64; 2]>::try_from(numbers).ok())
            .ok_or_else(|| format!("{name} is not two numbers")),
    }
}

/// A list of numbers, whichever form the evaluator gave it.
fn numbers(value: &Value) -> Option<Vec<f64>> {
    match value {
        Value::NumberArray(numbers) => Some(numbers.clone()),
        Value::Array(values) => values.iter().map(Value::as_number).collect(),
        _ => None,
    }
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
    fn hidden_paths_take_their_names_too() {
        let names = json!({ "source-layer": "transportation_name" });
        let narrowed = without_hidden_features(&names, HIDDEN);
        assert!(passes(&narrowed, "minor"));
        assert!(!passes(&narrowed, "path"));
    }

    #[test]
    fn hidden_paths_leave_other_source_layers_alone() {
        let water = json!({ "source-layer": "water", "filter": ["==", ["get", "class"], "path"] });
        assert!(matches!(
            without_hidden_features(&water, HIDDEN),
            Cow::Borrowed(_)
        ));
    }
}
