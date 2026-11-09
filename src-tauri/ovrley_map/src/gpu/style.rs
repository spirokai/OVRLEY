//! Reads installed styles with `maplibre-expr`, once per renderer.
//!
//! Registration only admits styles MapLibre loads, but this renderer draws less than MapLibre: a
//! layer it cannot read is logged and skipped, while a document with no layers at all fails.
//!
//! Everything that depends on zoom alone is evaluated here and nowhere else. A label property that
//! reads feature data stays an expression for layout to evaluate per feature, but only where MapLibre
//! allows it to. A symbol layer asking for something this renderer cannot draw is an error here, and
//! the layer is logged and skipped. Line labels lie flat on the ground, turning with the map.
//!
//! Icons never draw. An `icon-image` that depends on zoom alone is dropped with the icon, and the
//! layer draws its text. One that reads feature data (road shields) skips the layer, since its text
//! belongs on the icon.

use super::labels::{font_chain, TextAnchor, TextJustify, ONE_EM};
use super::pipelines::{pattern_index, ShaderDash};
use super::tile::TILE_UNITS;
use crate::{data_level, MapLayers, MapStyleBuildings, TILE_SIZE, VECTOR_SOURCE};
use lyon::tessellation::LineJoin;
use maplibre_expr::{
    evaluate, is_expression, parse_filter, parse_property, typecheck, EvaluationContext, Expr,
    FormatArg, Type, Value,
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
        let layer = with_label_names(&layer, toggles);
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

/// A place's own name, as OpenMapTiles has it, or the one it is known by internationally: English,
/// then the international name, then a Latin transliteration, before falling back to its own.
fn label_name(local: bool) -> Json {
    if local {
        json!(["get", "name"])
    } else {
        json!([
            "coalesce",
            ["get", "name_en"],
            ["get", "name_int"],
            ["get", "name:latin"],
            ["get", "name"]
        ])
    }
}

/// Whether an expression reads one of a feature's names, rather than a road's `ref`.
fn reads_name(expr: &Json) -> bool {
    match expr {
        Json::Array(items) => {
            matches!(items.as_slice(), [Json::String(op), Json::String(field)]
                if op == "get" && (field == "name" || field.starts_with("name:") || field.starts_with("name_")))
                || items.iter().any(reads_name)
        }
        _ => false,
    }
}

/// A label layer with the style's own choice of name replaced by one side of the switch. Styles mix
/// scripts and languages in their own ways; the switch gives each map one of two consistent ends.
fn with_label_names(layer: &Json, toggles: MapLayers) -> Cow<'_, Json> {
    if layer["type"] != "symbol" || !reads_name(&layer["layout"]["text-field"]) {
        return Cow::Borrowed(layer);
    }
    let mut named = layer.clone();
    named["layout"]["text-field"] = label_name(toggles.local_names);
    Cow::Owned(named)
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
    let (source_layer, filter) = source_and_filter(layer)?;
    Ok(VectorLayer {
        id: id.to_string(),
        index,
        source_layer,
        filter,
        paint,
    })
}

/// The layer's `source-layer`, which it needs, and its `filter`.
fn source_and_filter(layer: &Json) -> Result<(String, Option<Expr>), String> {
    let source_layer = layer["source-layer"]
        .as_str()
        .ok_or("no source-layer")?
        .to_string();
    let filter = layer
        .get("filter")
        .map(|filter| parse_filter(filter).map_err(|error| format!("filter: {error}")))
        .transpose()?;
    Ok((source_layer, filter))
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
fn zoom_value(layer: &Json, section: &str, name: &str, zoom: f64) -> Result<Option<Value>, String> {
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
fn evaluate_at_zoom(expr: &Expr, name: &str, zoom: f64) -> Result<Value, String> {
    evaluate(expr, &EvaluationContext::new().with_zoom(zoom))
        .map_err(|error| format!("{name}: {error}"))
}

fn keyword<'a>(value: &'a Option<Value>, name: &str) -> Result<Option<&'a str>, String> {
    value
        .as_ref()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| format!("{name} is not a keyword"))
        })
        .transpose()
}

fn number(value: &Option<Value>, name: &str, default: f64) -> Result<f64, String> {
    match value {
        None => Ok(default),
        Some(value) => value
            .as_number()
            .ok_or_else(|| format!("{name} is not a number")),
    }
}

fn boolean(value: &Option<Value>, name: &str, default: bool) -> Result<bool, String> {
    match value {
        None => Ok(default),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("{name} is not a boolean")),
    }
}

/// A pair of numbers, which is zero when the layer doesn't set the property.
fn pair(value: &Option<Value>, name: &str) -> Result<[f64; 2], String> {
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

/// The source layers whose names draw. `transportation_name` and `waterway` are the line labels.
const LABEL_SOURCE_LAYERS: [&str; 5] = [
    "place",
    "water_name",
    "park",
    "transportation_name",
    "waterway",
];

/// The operators that read the feature being styled.
const FEATURE_OPERATORS: [&str; 10] = [
    "get",
    "has",
    "properties",
    "id",
    "geometry-type",
    "feature-state",
    "line-progress",
    "accumulated",
    "within",
    "distance",
];

/// Layout properties this renderer cannot draw. A layer that sets one to anything but its default
/// is skipped.
const UNSUPPORTED_LAYOUT: [&str; 7] = [
    "text-variable-anchor",
    "text-variable-anchor-offset",
    "text-writing-mode",
    "text-radial-offset",
    "text-rotate",
    "symbol-avoid-edges",
    "symbol-z-order",
];

/// Whether `value` is what MapLibre defaults an unsupported property to. Those without a default
/// are never set.
fn is_unsupported_default(name: &str, value: &Json) -> bool {
    match name {
        "text-rotate" => value == &Json::from(0),
        "symbol-avoid-edges" => value == &Json::from(false),
        "symbol-z-order" => value == &Json::from("auto"),
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SymbolPlacement {
    Point,
    Line,
    LineCenter,
}

/// What a label turns with: the map, or the screen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Alignment {
    Map,
    Viewport,
}

/// A property's value: fixed at the style zoom and validated there, or an expression that reads
/// feature data and is evaluated per feature.
pub(crate) enum Varying<T> {
    Constant(T),
    PerFeature(Expr),
}

impl Varying<f64> {
    /// The value for a feature. A per-feature expression that fails reads as `default`.
    pub(crate) fn number(&self, context: &EvaluationContext, default: f64) -> f64 {
        match self {
            Self::Constant(value) => *value,
            Self::PerFeature(expr) => evaluate_number(Some(expr), context, default),
        }
    }
}

impl Varying<[f32; 4]> {
    /// The value for a feature, or `None` when a per-feature expression doesn't give a colour.
    pub(crate) fn color(&self, context: &EvaluationContext) -> Option<[f32; 4]> {
        match self {
            Self::Constant(value) => Some(*value),
            Self::PerFeature(expr) => evaluate_color(expr, context, 1.0),
        }
    }
}

/// A symbol layer's paint.
pub(crate) struct LabelPaint {
    pub(crate) color: Varying<[f32; 4]>,
    pub(crate) halo_color: Option<Varying<[f32; 4]>>,
    /// Style pixels.
    pub(crate) halo_width: Option<Varying<f64>>,
    /// Style pixels.
    pub(crate) halo_blur: Option<Varying<f64>>,
    pub(crate) opacity: Option<Varying<f64>>,
}

/// One drawable symbol layer, with its zoom-dependent values evaluated at the style zoom. Lengths in
/// pixels are in output pixels, ems stay ems.
pub(crate) struct LabelLayer {
    /// Draw order among all layers.
    pub(crate) index: u32,
    pub(crate) id: String,
    pub(crate) source_layer: String,
    pub(crate) filter: Option<Expr>,
    pub(crate) placement: SymbolPlacement,
    /// The `text-font` name whose chain draws the layer.
    pub(crate) font: String,
    pub(crate) text_field: Expr,
    pub(crate) sort_key: Option<Varying<f64>>,
    pub(crate) size_px: f64,
    pub(crate) max_width_ems: f64,
    pub(crate) line_height_ems: f64,
    pub(crate) letter_spacing_ems: f64,
    pub(crate) anchor: TextAnchor,
    pub(crate) justify: TextJustify,
    pub(crate) offset_ems: [f64; 2],
    pub(crate) uppercase: bool,
    pub(crate) padding_px: f64,
    pub(crate) translate_px: [f64; 2],
    /// `translate_px` in tile units at the style zoom, which is how far a map-anchored translation moves
    /// a label on the ground.
    pub(crate) translate_units: [f64; 2],
    pub(crate) translate_anchor: Alignment,
    pub(crate) allow_overlap: bool,
    pub(crate) ignore_placement: bool,
    /// What halo width and blur multiply by to reach output pixels.
    pub(crate) pixel_ratio: f64,
    pub(crate) paint: LabelPaint,
    /// Read by line labels.
    pub(crate) line: LineLabel,
}

/// The properties only a label along a line reads.
pub(crate) struct LineLabel {
    /// Output pixels.
    pub(crate) spacing_px: f64,
    pub(crate) max_angle_deg: f64,
    pub(crate) keep_upright: bool,
    /// Tile units in one output pixel of a tile cut for the style zoom, which tile data is laid out in.
    pub(crate) units_per_px: f64,
    /// Tile units in one glyph unit at the style zoom, which is the label's size on the ground when the
    /// ground is as far from the camera as the map centre.
    pub(crate) units_per_glyph: f64,
}

/// Whether a symbol layer's source layer is one whose names draw.
fn is_label_source(source_layer: Option<&str>) -> bool {
    source_layer.is_some_and(|name| LABEL_SOURCE_LAYERS.contains(&name))
}

/// Whether evaluating `expr` reads the feature it is evaluated for.
fn reads_feature(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(_) | Expr::Var(_) => false,
        Expr::Call { op, args } => FEATURE_OPERATORS.contains(&op.as_str()) || any_reads(args),
        Expr::Let { bindings, body } => {
            any_reads(bindings.iter().map(|(_, value)| value)) || reads_feature(body)
        }
        Expr::Match {
            input,
            arms,
            default,
        } => {
            reads_feature(input)
                || any_reads(arms.iter().map(|(_, output)| output))
                || reads_feature(default)
        }
        Expr::Step {
            input,
            output0,
            stops,
        } => {
            reads_feature(input)
                || reads_feature(output0)
                || any_reads(stops.iter().map(|(_, output)| output))
        }
        Expr::Interpolate { input, stops, .. } => {
            reads_feature(input) || any_reads(stops.iter().map(|(_, output)| output))
        }
        Expr::Format(sections) => sections.iter().any(|section: &FormatArg| {
            let options = [
                &section.scale,
                &section.font,
                &section.text_color,
                &section.vertical_align,
            ];
            reads_feature(&section.content) || options.into_iter().flatten().any(reads_feature)
        }),
        Expr::Within(_) | Expr::Distance(_) => true,
        Expr::Collator {
            case_sensitive,
            diacritic_sensitive,
            locale,
        } => [case_sensitive, diacritic_sensitive, locale]
            .into_iter()
            .flatten()
            .any(|expr| reads_feature(expr)),
        Expr::NumberFormat {
            value,
            locale,
            currency,
            min_fraction_digits,
            max_fraction_digits,
            unit,
        } => {
            reads_feature(value)
                || [
                    locale,
                    currency,
                    min_fraction_digits,
                    max_fraction_digits,
                    unit,
                ]
                .into_iter()
                .flatten()
                .any(|expr| reads_feature(expr))
        }
        Expr::Assert(_, inner) | Expr::Coerce(_, inner) => reads_feature(inner),
    }
}

fn any_reads<'a>(exprs: impl IntoIterator<Item = &'a Expr>) -> bool {
    exprs.into_iter().any(reads_feature)
}

/// A label property that depends on zoom alone, at the view zoom, or `None` when the layer doesn't
/// set it. Reading feature data is an error: MapLibre allows it for these, and this renderer doesn't.
fn label_zoom_value(
    layer: &Json,
    section: &str,
    name: &str,
    zoom: f64,
) -> Result<Option<Value>, String> {
    if let Some(raw) = layer[section].get(name) {
        if !raw.is_array() || is_expression(raw) {
            let expr = parse_property(name, raw).map_err(|error| format!("{name}: {error}"))?;
            if reads_feature(&expr) {
                return Err(format!("{name} reads feature data, which is not supported"));
            }
        }
    }
    zoom_value(layer, section, name, zoom)
}

/// Reads one symbol layer. `Err` says why this renderer cannot draw it.
fn parse_label_layer(
    layer: &Json,
    id: &str,
    zoom: f64,
    pixel_ratio: f32,
    index: u32,
) -> Result<LabelLayer, String> {
    let ratio = f64::from(pixel_ratio);
    let at = |name: &str| label_zoom_value(layer, "layout", name, zoom);

    for name in UNSUPPORTED_LAYOUT {
        if let Some(value) = layer["layout"].get(name) {
            if !is_unsupported_default(name, value) {
                return Err(format!("{name} is not supported"));
            }
        }
    }
    reject_feature_dependent_icon(layer)?;

    let placement = match keyword(&at("symbol-placement")?, "symbol-placement")? {
        None | Some("point") => SymbolPlacement::Point,
        Some("line") => SymbolPlacement::Line,
        Some("line-center") => SymbolPlacement::LineCenter,
        Some(other) => return Err(format!("symbol-placement {other} is not a placement")),
    };
    let alignment = |name: &str, value: &Option<Value>, auto: Alignment| {
        Ok(match keyword(value, name)? {
            None | Some("auto") => auto,
            Some("map") => Alignment::Map,
            Some("viewport") => Alignment::Viewport,
            Some(other) => return Err(format!("{name} {other} is not an alignment")),
        })
    };
    let rotation_alignment = alignment(
        "text-rotation-alignment",
        &at("text-rotation-alignment")?,
        if placement == SymbolPlacement::Point {
            Alignment::Viewport
        } else {
            Alignment::Map
        },
    )?;
    let pitch_alignment = alignment(
        "text-pitch-alignment",
        &at("text-pitch-alignment")?,
        rotation_alignment,
    )?;
    if placement == SymbolPlacement::Point {
        if rotation_alignment != Alignment::Viewport || pitch_alignment != Alignment::Viewport {
            return Err("a point label aligned to the map is not supported".to_string());
        }
    } else if rotation_alignment != Alignment::Map || pitch_alignment != Alignment::Map {
        return Err("a line label aligned to the viewport is not supported".to_string());
    }

    // Without a text-font, MapLibre draws Open Sans, which has no chain.
    let fonts = match at("text-font")? {
        Some(Value::Array(fonts)) => fonts,
        Some(_) => return Err("text-font is not a list of names".to_string()),
        None => return Err("no font chain for the default text-font".to_string()),
    };
    let font = fonts
        .iter()
        .filter_map(Value::as_str)
        .find(|name| font_chain(name).is_some())
        .ok_or_else(|| format!("no font chain for {fonts:?}"))?
        .to_string();

    let uppercase = match keyword(&at("text-transform")?, "text-transform")? {
        None | Some("none") => false,
        Some("uppercase") => true,
        Some(other) => return Err(format!("text-transform {other} is not supported")),
    };

    let text_field = {
        let value = layer["layout"].get("text-field").ok_or("no text-field")?;
        let expr =
            parse_property("text-field", value).map_err(|error| format!("text-field: {error}"))?;
        typecheck(&expr, Some(&Type::Formatted), false)
            .map_err(|error| format!("text-field: {error}"))?
    };

    let translate_anchor = match keyword(
        &label_zoom_value(layer, "paint", "text-translate-anchor", zoom)?,
        "text-translate-anchor",
    )? {
        None | Some("map") => Alignment::Map,
        Some("viewport") => Alignment::Viewport,
        Some(other) => return Err(format!("text-translate-anchor {other} is not an anchor")),
    };
    let translate_px = pair(
        &label_zoom_value(layer, "paint", "text-translate", zoom)?,
        "text-translate",
    )?
    .map(|value| value * ratio);
    if placement != SymbolPlacement::Point
        && translate_anchor == Alignment::Viewport
        && translate_px != [0.0, 0.0]
    {
        return Err("a line label translated along the viewport is not supported".to_string());
    }

    let line_height_ems = number(&at("text-line-height")?, "text-line-height", 1.2)?;
    if line_height_ems <= 0.0 {
        return Err("text-line-height must be positive".to_string());
    }
    let size_px = number(&at("text-size")?, "text-size", 16.0)? * ratio;
    let units_per_px = TILE_UNITS
        / (f64::from(TILE_SIZE) * ratio * 2_f64.powf(zoom - f64::from(data_level(zoom))));

    let (source_layer, filter) = source_and_filter(layer)?;
    Ok(LabelLayer {
        index,
        id: id.to_string(),
        source_layer,
        filter,
        placement,
        font,
        text_field,
        sort_key: varying_number(layer, "layout", "symbol-sort-key", zoom)?,
        size_px,
        max_width_ems: number(&at("text-max-width")?, "text-max-width", 10.0)?,
        line_height_ems,
        letter_spacing_ems: number(&at("text-letter-spacing")?, "text-letter-spacing", 0.0)?,
        anchor: match keyword(&at("text-anchor")?, "text-anchor")? {
            None | Some("center") => TextAnchor::Center,
            Some("left") => TextAnchor::Left,
            Some("right") => TextAnchor::Right,
            Some("top") => TextAnchor::Top,
            Some("bottom") => TextAnchor::Bottom,
            Some("top-left") => TextAnchor::TopLeft,
            Some("top-right") => TextAnchor::TopRight,
            Some("bottom-left") => TextAnchor::BottomLeft,
            Some("bottom-right") => TextAnchor::BottomRight,
            Some(other) => return Err(format!("text-anchor {other} is not an anchor")),
        },
        justify: match keyword(&at("text-justify")?, "text-justify")? {
            None | Some("center") => TextJustify::Center,
            Some("auto") => TextJustify::Auto,
            Some("left") => TextJustify::Left,
            Some("right") => TextJustify::Right,
            Some(other) => return Err(format!("text-justify {other} is not a justification")),
        },
        offset_ems: pair(&at("text-offset")?, "text-offset")?,
        uppercase,
        padding_px: number(&at("text-padding")?, "text-padding", 2.0)? * ratio,
        translate_px,
        translate_units: translate_px.map(|value| value * units_per_px),
        translate_anchor,
        allow_overlap: boolean(&at("text-allow-overlap")?, "text-allow-overlap", false)?,
        ignore_placement: boolean(
            &at("text-ignore-placement")?,
            "text-ignore-placement",
            false,
        )?,
        pixel_ratio: ratio,
        paint: LabelPaint {
            color: varying_color(
                "text-color",
                layer["paint"]
                    .get("text-color")
                    .unwrap_or(&Json::from("#000000")),
                zoom,
            )?,
            halo_color: layer["paint"]
                .get("text-halo-color")
                .map(|value| varying_color("text-halo-color", value, zoom))
                .transpose()?,
            halo_width: varying_number(layer, "paint", "text-halo-width", zoom)?,
            halo_blur: varying_number(layer, "paint", "text-halo-blur", zoom)?,
            opacity: varying_number(layer, "paint", "text-opacity", zoom)?,
        },
        line: LineLabel {
            spacing_px: number(&at("symbol-spacing")?, "symbol-spacing", 250.0)? * ratio,
            max_angle_deg: number(&at("text-max-angle")?, "text-max-angle", 45.0)?,
            keep_upright: boolean(&at("text-keep-upright")?, "text-keep-upright", true)?,
            units_per_px,
            units_per_glyph: size_px / ONE_EM * units_per_px,
        },
    })
}

/// A number property that may read feature data. One that doesn't is evaluated and checked at the
/// view zoom; `None` when the layer doesn't set it.
fn varying_number(
    layer: &Json,
    section: &str,
    name: &str,
    zoom: f64,
) -> Result<Option<Varying<f64>>, String> {
    let Some(raw) = layer[section].get(name) else {
        return Ok(None);
    };
    let expr = parse_property(name, raw).map_err(|error| format!("{name}: {error}"))?;
    if reads_feature(&expr) {
        return Ok(Some(Varying::PerFeature(expr)));
    }
    number(&Some(evaluate_at_zoom(&expr, name, zoom)?), name, 0.0)
        .map(|value| Some(Varying::Constant(value)))
}

/// A colour property that may read feature data. One that doesn't is evaluated and checked at the
/// view zoom.
fn varying_color(name: &str, raw: &Json, zoom: f64) -> Result<Varying<[f32; 4]>, String> {
    let expr = color_expr(name, raw)?;
    if reads_feature(&expr) {
        return Ok(Varying::PerFeature(expr));
    }
    evaluate_color(&expr, &EvaluationContext::new().with_zoom(zoom), 1.0)
        .map(Varying::Constant)
        .ok_or_else(|| format!("{name} is not a colour"))
}

/// An icon that depends on feature data (a road shield) skips the layer. One that depends on zoom
/// alone is a place's dot, which is not drawn.
fn reject_feature_dependent_icon(layer: &Json) -> Result<(), String> {
    let Some(raw) = layer["layout"].get("icon-image") else {
        return Ok(());
    };
    let expr = parse_property("icon-image", raw).map_err(|error| format!("icon-image: {error}"))?;
    if reads_feature(&expr) {
        return Err("icon-image reads feature data".to_string());
    }
    Ok(())
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
        local_names: false,
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

    fn evaluate_name(layer: &Json, properties: &[(&str, &str)]) -> Option<String> {
        let text_field = parse_property("text-field", &layer["layout"]["text-field"]).unwrap();
        let mut context = EvaluationContext::new().with_zoom(14.0);
        context.feature = Feature {
            properties: properties
                .iter()
                .map(|(key, value)| (key.to_string(), Value::String(value.to_string())))
                .collect(),
            ..Feature::default()
        };
        match maplibre_expr::evaluate(&text_field, &context) {
            Ok(Value::String(text)) => Some(text),
            _ => None,
        }
    }

    #[test]
    fn the_name_switch_picks_a_places_own_name_or_its_international_one() {
        let moscow = [
            ("name", "Москва"),
            ("name:latin", "Moskva"),
            ("name_en", "Moscow"),
        ];
        let village = [("name", "Ústí"), ("name:latin", "Ústí")];
        let local = MapLayers {
            local_names: true,
            ..SHOWN
        };
        let name = |toggles, place: &[(&str, &str)]| {
            evaluate_name(&with_label_names(&place_layer(), toggles), place)
        };
        assert_eq!(name(local, &moscow).as_deref(), Some("Москва"));
        assert_eq!(name(SHOWN, &moscow).as_deref(), Some("Moscow"));
        assert_eq!(name(SHOWN, &village).as_deref(), Some("Ústí"));

        // A road's number is not a name.
        let shield =
            json!({ "type": "symbol", "layout": { "text-field": ["to-string", ["get", "ref"]] } });
        assert!(matches!(with_label_names(&shield, local), Cow::Borrowed(_)));
    }

    fn place_layer() -> Json {
        json!({
            "type": "symbol",
            "layout": {
                "text-field": ["case", ["has", "name:nonlatin"],
                    ["concat", ["get", "name:latin"], "\n", ["get", "name:nonlatin"]],
                    ["coalesce", ["get", "name_en"], ["get", "name"]]],
            },
        })
    }

    fn parse(layer: &Json) -> Result<LabelLayer, String> {
        parse_label_layer(layer, "label", 14.0, 2.0, 7)
    }

    fn place() -> Json {
        json!({
            "id": "label",
            "type": "symbol",
            "source-layer": "place",
            "layout": {
                "text-field": ["coalesce", ["get", "name_en"], ["get", "name"]],
                "text-font": ["Noto Sans Bold"],
                "text-size": ["interpolate", ["linear"], ["zoom"], 10, 10, 18, 18],
                "text-anchor": ["step", ["zoom"], "left", 8, "center"],
                "text-offset": [0.5, 0.2],
                "icon-image": ["step", ["zoom"], "circle-11", 9, ""],
            },
            "paint": { "text-color": "#123456", "text-halo-width": 1.5 },
        })
    }

    #[test]
    fn zoom_values_are_evaluated_once_in_output_pixels() {
        let layer = parse(&place()).unwrap();
        assert_eq!(layer.index, 7);
        assert_eq!(layer.font, "Noto Sans Bold");
        assert_eq!(layer.size_px, 14.0 * 2.0);
        assert_eq!(layer.anchor, TextAnchor::Center);
        assert_eq!(layer.offset_ems, [0.5, 0.2]);
        assert_eq!(layer.padding_px, 4.0);
        assert!(reads_feature(&layer.text_field));
        assert!(!layer.allow_overlap);
    }

    #[test]
    fn an_unsupported_property_a_missing_font_or_a_data_driven_icon_skips_the_layer() {
        let with = |name: &str, value: Json| {
            let mut layer = place();
            layer["layout"][name] = value;
            parse(&layer).err().unwrap()
        };
        assert!(with("text-variable-anchor", json!(["top"])).contains("text-variable-anchor"));
        assert!(with("text-font", json!(["Open Sans Regular"])).contains("no font chain"));
        assert!(with("icon-image", json!(["get", "class"])).contains("icon-image"));
        assert!(with("text-size", json!(["get", "rank"])).contains("feature data"));
        assert!(with("text-rotation-alignment", json!("map")).contains("aligned to the map"));

        let mut layer = place();
        layer["layout"]["text-rotate"] = json!(0);
        assert!(parse(&layer).is_ok());
    }
}
