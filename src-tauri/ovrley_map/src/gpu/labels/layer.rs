//! Label layers: the symbol layers of a style this renderer draws, read once at the view zoom.
//!
//! Everything that depends on zoom alone is evaluated here and nowhere else. A property
//! that reads feature data stays an expression for layout to evaluate per feature, but only where
//! MapLibre allows it to. A layer asking for something this renderer cannot draw is an error here, and
//! the caller logs it and skips the layer.
//!
//! Line labels lie flat on the ground, turning with the map. A line layer that asks for anything else is
//! an error here.
//!
//! Icons never draw. An `icon-image` that depends on zoom alone is dropped with the icon, and the
//! layer draws its text. One that reads feature data (road shields) skips the layer, since its text
//! belongs on the icon.

use super::layout::TILE_UNITS;
use super::text::{font_chain, TextAnchor, TextJustify, ONE_EM};
use crate::gpu::style::{
    boolean, color_expr, evaluate_at_zoom, evaluate_color, evaluate_number, keyword, number, pair,
    zoom_value as ground_zoom_value,
};
use crate::{data_level, TILE_SIZE};
use maplibre_expr::{
    is_expression, parse_property, typecheck, EvaluationContext, Expr, FormatArg, Type, Value,
};
use serde_json::Value as Json;

/// The source layers whose names draw. `transportation_name` and `waterway` are the line labels.
pub(crate) const LABEL_SOURCE_LAYERS: [&str; 5] = [
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
pub(crate) fn is_label_source(source_layer: Option<&str>) -> bool {
    source_layer.is_some_and(|name| LABEL_SOURCE_LAYERS.contains(&name))
}

/// Whether evaluating `expr` reads the feature it is evaluated for.
pub(crate) fn reads_feature(expr: &Expr) -> bool {
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
fn zoom_value(layer: &Json, section: &str, name: &str, zoom: f64) -> Result<Option<Value>, String> {
    if let Some(raw) = layer[section].get(name) {
        if !raw.is_array() || is_expression(raw) {
            let expr = parse_property(name, raw).map_err(|error| format!("{name}: {error}"))?;
            if reads_feature(&expr) {
                return Err(format!("{name} reads feature data, which is not supported"));
            }
        }
    }
    ground_zoom_value(layer, section, name, zoom)
}

/// Reads one symbol layer. `Err` says why this renderer cannot draw it.
pub(crate) fn parse_label_layer(
    layer: &Json,
    id: &str,
    zoom: f64,
    pixel_ratio: f32,
    index: u32,
) -> Result<LabelLayer, String> {
    let ratio = f64::from(pixel_ratio);
    let at = |name: &str| zoom_value(layer, "layout", name, zoom);

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
        &zoom_value(layer, "paint", "text-translate-anchor", zoom)?,
        "text-translate-anchor",
    )? {
        None | Some("map") => Alignment::Map,
        Some("viewport") => Alignment::Viewport,
        Some(other) => return Err(format!("text-translate-anchor {other} is not an anchor")),
    };
    let translate_px = pair(
        &zoom_value(layer, "paint", "text-translate", zoom)?,
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

    Ok(LabelLayer {
        index,
        id: id.to_string(),
        source_layer: layer["source-layer"]
            .as_str()
            .ok_or("no source-layer")?
            .to_string(),
        filter: layer
            .get("filter")
            .map(|filter| {
                maplibre_expr::parse_filter(filter).map_err(|error| format!("filter: {error}"))
            })
            .transpose()?,
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
    use serde_json::json;

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
