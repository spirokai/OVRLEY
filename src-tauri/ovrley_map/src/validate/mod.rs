//! Whether a style file can be installed, and what the catalog lists for it.
//!
//! A style installs when MapLibre would load it (see [`spec`]) and it meets OVRLEY's
//! own contract:
//! - its tiles come from `sources.openmaptiles`, a vector source: the only tiles the tile service
//!   serves, whatever URL the file names;
//! - it has a visible, opaque `background` layer, whose colour the widget shows until the map
//!   draws. Export copies a frame's opaque interior rather than blending it, so it must be opaque;
//! - `metadata["ovrley:building-color"]`, when present, is a colour, and the style has no
//!   `fill-extrusion` layer of its own. Without either, its added 3D buildings take the colour of
//!   its flat building fill;
//! - the layers OVRLEY reads itself, its backgrounds and flat building fills, say whether they show
//!   with a plain `visibility`, not an expression.
//!
//! The spec's optional `name` labels the style; without one its id does.

mod expression;
mod spec;

use crate::{MapStyleBuildings, MapStyleId, MapStyleInfo, BUILDING_COLOR_KEY, VECTOR_SOURCE};
use maplibre_expr::{evaluate, parse_property, typecheck, Color, EvaluationContext, Type, Value};
use serde_json::Value as Json;
use spec::{reference, style_errors};

/// The zoom the background is read at: the deepest tile data, which every widget camera shows.
const BACKGROUND_ZOOM: f64 = 14.0;

/// Reads `bytes` as the style `id`. On success also returns the document, as read.
pub fn read_style_file(id: MapStyleId, bytes: &[u8]) -> Result<(MapStyleInfo, Json), Vec<String>> {
    let style: Json =
        serde_json::from_slice(bytes).map_err(|error| vec![format!("not valid JSON: {error}")])?;
    let errors = style_errors(&style);
    if !errors.is_empty() {
        return Err(errors);
    }
    let layers = style["layers"]
        .as_array()
        .expect("a valid style lists its layers");
    let mut errors = Vec::new();
    if style["sources"][VECTOR_SOURCE]["type"] != "vector" {
        errors.push(format!("sources.{VECTOR_SOURCE} must be a vector source"));
    }
    let buildings = buildings(&style, layers).map_err(|error| errors.push(error));
    let background = background(layers).map_err(|error| errors.push(error));
    match (buildings, background) {
        (Ok(buildings), Ok(background)) if errors.is_empty() => Ok((
            MapStyleInfo {
                label: style["name"].as_str().unwrap_or(id.as_str()).to_string(),
                id,
                background,
                buildings,
            },
            style,
        )),
        _ => Err(errors),
    }
}

/// Whether the layer at `index` shows. OVRLEY reads this layer itself, so an expression, which
/// only the preview's MapLibre can evaluate, fails.
fn visible(layers: &[Json], index: usize) -> Result<bool, String> {
    match layers[index]["layout"].get("visibility") {
        None => Ok(true),
        Some(visibility) if visibility == "visible" => Ok(true),
        Some(visibility) if visibility == "none" => Ok(false),
        Some(_) => Err(format!(
            "layers[{index}].layout.visibility: OVRLEY reads this layer, so it must be \"visible\" or \"none\""
        )),
    }
}

/// The first visible layer `matches` picks.
fn first_visible(
    layers: &[Json],
    matches: impl Fn(&Json) -> bool,
) -> Result<Option<usize>, String> {
    for index in (0..layers.len()).filter(|index| matches(&layers[*index])) {
        if visible(layers, index)? {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

/// The style's 3D buildings: its own `fill-extrusion` layers, or OVRLEY's in the colour
/// `metadata` names, or else in its first visible flat building fill's colour. A fill colour
/// reads as an extrusion colour, as both properties take the same values.
fn buildings(style: &Json, layers: &[Json]) -> Result<MapStyleBuildings, String> {
    let own = layers
        .iter()
        .any(|layer| layer["type"] == "fill-extrusion" && layer["source"] == VECTOR_SOURCE);
    match style["metadata"].get(BUILDING_COLOR_KEY) {
        Some(_) if own => Err(format!(
            "metadata.{BUILDING_COLOR_KEY} is only for a style without its own fill-extrusion layer"
        )),
        Some(color)
            if color
                .as_str()
                .is_some_and(|color| Color::parse(color).is_some()) =>
        {
            Ok(MapStyleBuildings::Added(color.clone()))
        }
        Some(color) => Err(format!(
            "metadata.{BUILDING_COLOR_KEY} must be a colour, found {color}"
        )),
        None if own => Ok(MapStyleBuildings::Own),
        None => {
            let flat = first_visible(layers, |layer| {
                layer["type"] == "fill"
                    && layer["source"] == VECTOR_SOURCE
                    && layer["source-layer"] == "building"
            })?;
            Ok(
                match flat.and_then(|index| layers[index]["paint"].get("fill-color")) {
                    Some(color) => MapStyleBuildings::Added(color.clone()),
                    None => MapStyleBuildings::None,
                },
            )
        }
    }
}

/// The first visible background layer's colour at [`BACKGROUND_ZOOM`], which must be opaque.
fn background(layers: &[Json]) -> Result<[u8; 3], String> {
    let index = first_visible(layers, |layer| layer["type"] == "background")?
        .ok_or("needs a visible background layer for the widget to show until its map draws")?;
    let read = |name: &str, expected: Type| -> Result<Value, String> {
        let value = layers[index]["paint"]
            .get(name)
            .unwrap_or(&reference()["paint_background"][name]["default"]);
        let expr = parse_property(name, value)
            .and_then(|expr| typecheck(&expr, Some(&expected), false))
            .map_err(|error| format!("layers[{index}].paint.{name}: {error}"))?;
        evaluate(&expr, &EvaluationContext::new().with_zoom(BACKGROUND_ZOOM))
            .map_err(|error| format!("layers[{index}].paint.{name}: {error}"))
    };
    let Value::Color(color) = read("background-color", Type::Color)? else {
        return Err(format!(
            "layers[{index}].paint.background-color is not a colour"
        ));
    };
    let opacity = read("background-opacity", Type::Number)?
        .as_number()
        .ok_or_else(|| format!("layers[{index}].paint.background-opacity is not a number"))?;
    if color.a * opacity < 1.0 {
        return Err(format!(
            "layers[{index}]: the background must be opaque at zoom {BACKGROUND_ZOOM}"
        ));
    }
    let channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Ok([channel(color.r), channel(color.g), channel(color.b)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read(style: &Json) -> Result<MapStyleInfo, Vec<String>> {
        read_style_file(
            MapStyleId::new("custom").unwrap(),
            &serde_json::to_vec(style).unwrap(),
        )
        .map(|(info, _)| info)
    }

    fn style() -> Json {
        json!({
            "version": 8,
            "sources": { "openmaptiles": { "type": "vector", "url": "https://example.com/tiles.json" } },
            "layers": [
                { "id": "hidden", "type": "background", "layout": { "visibility": "none" }, "paint": { "background-color": "red" } },
                { "id": "bg", "type": "background", "paint": { "background-color": "rgb(10, 20, 30)" } },
            ],
        })
    }

    fn with_layer(layer: Json) -> Json {
        let mut style = style();
        style["layers"].as_array_mut().unwrap().push(layer);
        style
    }

    #[test]
    fn the_bundled_styles_install_with_their_labels_backgrounds_and_buildings() {
        let bundled = [
            (
                "bright",
                include_str!("../../../../assets/map-styles/bright.json"),
                "Bright",
                [0xf8, 0xf4, 0xf0],
            ),
            (
                "dark",
                include_str!("../../../../assets/map-styles/dark.json"),
                "Dark",
                [12, 12, 12],
            ),
            (
                "fiord",
                include_str!("../../../../assets/map-styles/fiord.json"),
                "Fiord",
                [0x45, 0x51, 0x6e],
            ),
            (
                "liberty",
                include_str!("../../../../assets/map-styles/liberty.json"),
                "Liberty",
                [0xf8, 0xf4, 0xf0],
            ),
            (
                "positron",
                include_str!("../../../../assets/map-styles/positron.json"),
                "Positron",
                [242, 243, 240],
            ),
        ];
        for (id, json, label, background) in bundled {
            let (info, _) = read_style_file(MapStyleId::new(id).unwrap(), json.as_bytes())
                .unwrap_or_else(|errors| panic!("{id}: {errors:#?}"));
            assert_eq!(
                (info.label.as_str(), info.background),
                (label, background),
                "{id}"
            );
            // Liberty brings its own 3D buildings, the others name the colour of the added ones.
            let style: Json = serde_json::from_str(json).unwrap();
            let expected = match style["metadata"].get(BUILDING_COLOR_KEY) {
                Some(color) => MapStyleBuildings::Added(color.clone()),
                None => MapStyleBuildings::Own,
            };
            assert_eq!(info.buildings, expected, "{id}");
            assert_eq!(id == "liberty", expected == MapStyleBuildings::Own, "{id}");
        }
    }

    #[test]
    fn a_style_without_a_name_is_labelled_by_its_id_and_shows_its_first_visible_background() {
        let info = read(&style()).unwrap();
        assert_eq!(info.label, "custom");
        assert_eq!(info.background, [10, 20, 30]);
        assert_eq!(info.buildings, MapStyleBuildings::None);
    }

    #[test]
    fn the_background_must_be_opaque() {
        for paint in [
            json!({ "background-color": "rgba(10, 20, 30, 0.5)" }),
            json!({ "background-opacity": 0.5 }),
        ] {
            let mut translucent = style();
            translucent["layers"][1]["paint"] = paint;
            assert!(read(&translucent).unwrap_err()[0].contains("must be opaque"));
        }
    }

    #[test]
    fn a_style_must_draw_openmaptiles_over_a_background() {
        let mut raster = style();
        raster["sources"]["openmaptiles"] = json!({ "type": "raster", "tiles": ["x"] });
        assert!(read(&raster).unwrap_err()[0].contains("must be a vector source"));

        let mut bare = style();
        bare["layers"] = json!([]);
        assert!(read(&bare).unwrap_err()[0].contains("needs a visible background layer"));
    }

    #[test]
    fn a_layer_ovrley_reads_says_plainly_whether_it_shows() {
        let mut computed = style();
        computed["layers"][0]["layout"]["visibility"] =
            json!(["case", ["global-state", "dark"], "visible", "none"]);
        assert!(read(&computed).unwrap_err()[0].contains("layers[0].layout.visibility"));
    }

    #[test]
    fn added_buildings_take_the_named_colour_or_else_the_flat_buildings_colour() {
        let fill = json!({ "id": "b", "type": "fill", "source": "openmaptiles", "source-layer": "building",
            "paint": { "fill-color": { "stops": [[13, "red"], [16, "blue"]] } } });
        assert_eq!(
            read(&with_layer(fill.clone())).unwrap().buildings,
            MapStyleBuildings::Added(fill["paint"]["fill-color"].clone())
        );

        let mut named = with_layer(fill.clone());
        named["metadata"] = json!({ BUILDING_COLOR_KEY: "green" });
        assert_eq!(
            read(&named).unwrap().buildings,
            MapStyleBuildings::Added(json!("green"))
        );

        let mut hidden = fill;
        hidden["layout"] = json!({ "visibility": "none" });
        assert_eq!(
            read(&with_layer(hidden)).unwrap().buildings,
            MapStyleBuildings::None
        );

        let own = with_layer(json!({
            "id": "3d", "type": "fill-extrusion", "source": "openmaptiles", "source-layer": "building",
        }));
        assert_eq!(read(&own).unwrap().buildings, MapStyleBuildings::Own);
    }

    #[test]
    fn a_fill_colour_reads_as_an_extrusion_colour() {
        let spec = |layer: &str, property: &str| {
            let spec = &reference()[format!("paint_{layer}")][property];
            (
                spec["type"].clone(),
                spec["expression"].clone(),
                spec["property-type"].clone(),
            )
        };
        assert_eq!(
            spec("fill", "fill-color"),
            spec("fill-extrusion", "fill-extrusion-color")
        );
    }

    #[test]
    fn building_color_must_be_a_colour_for_a_style_without_its_own_buildings() {
        let mut colour = style();
        colour["metadata"] = json!({ BUILDING_COLOR_KEY: "nope" });
        assert!(read(&colour).unwrap_err()[0].contains("must be a colour"));

        let mut both = with_layer(json!({
            "id": "3d", "type": "fill-extrusion", "source": "openmaptiles", "source-layer": "building",
        }));
        both["metadata"] = json!({ BUILDING_COLOR_KEY: "red" });
        assert!(read(&both).unwrap_err()[0]
            .contains("only for a style without its own fill-extrusion layer"));
    }

    #[test]
    fn maplibre_errors_and_broken_json_are_reported_whole() {
        assert!(
            read_style_file(MapStyleId::new("x").unwrap(), b"{").unwrap_err()[0]
                .contains("not valid JSON")
        );
        let mut invalid = style();
        invalid["layers"][1]["paint"]["background-color"] = json!(5);
        assert!(read(&invalid).unwrap_err()[0].contains("color expected, number found"));
    }
}

/// MapLibre parity of the validator.
#[cfg(test)]
mod spec_tests {
    use super::spec::*;
    use serde_json::{json, Value as Json};

    fn style(layers: Json) -> Json {
        json!({
            "version": 8,
            "sources": {
                "openmaptiles": { "type": "vector", "url": "https://example.com/tiles.json" },
                "shade": { "type": "raster", "tiles": ["https://example.com/{z}/{x}/{y}.png"], "tileSize": 256 },
            },
            "glyphs": "https://example.com/{fontstack}/{range}.pbf",
            "layers": layers,
        })
    }

    fn fill(paint: Json) -> Json {
        style(
            json!([{ "id": "water", "type": "fill", "source": "openmaptiles", "source-layer": "water", "paint": paint }]),
        )
    }

    fn assert_valid(style: &Json) {
        let errors = style_errors(style);
        assert!(errors.is_empty(), "{errors:#?}");
    }

    #[track_caller]
    fn assert_error(style: &Json, expected: &str) {
        let errors = style_errors(style);
        assert!(
            errors.iter().any(|error| error.contains(expected)),
            "expected an error containing {expected:?}, got {errors:#?}"
        );
    }

    #[test]
    fn the_root_needs_version_8_sources_and_layers() {
        assert_error(
            &json!({ "version": 7, "sources": {}, "layers": [] }),
            "version: expected one of [8], 7 found",
        );
        assert_error(
            &json!({ "version": "8", "sources": {}, "layers": [] }),
            "version: expected one of [8]",
        );
        assert_error(
            &json!({ "version": 8, "layers": [] }),
            "missing required property \"sources\"",
        );
        assert_error(
            &json!({ "version": 8, "sources": {} }),
            "missing required property \"layers\"",
        );
        assert_error(&json!([]), "object expected, array found");
        assert_valid(
            &json!({ "version": 8, "sources": {}, "layers": [], "anything": 1, "metadata": { "x": [1] } }),
        );
    }

    #[test]
    fn root_properties_follow_their_specs() {
        let mut glyphs = style(json!([]));
        glyphs["glyphs"] = json!("https://example.com/{range}.pbf");
        assert_error(
            &glyphs,
            "\"glyphs\" url must include a \"{fontstack}\" token",
        );

        let mut constants = style(json!([]));
        constants["constants"] = json!({ "@a": 1 });
        assert_error(&constants, "constants have been deprecated as of v8");

        let mut center = style(json!([]));
        center["center"] = json!([1]);
        assert_error(&center, "center: array length 2 expected, length 1 found");

        let mut light = style(json!([]));
        light["light"] = json!({ "intensity": 2, "color-transition": { "duration": 1 } });
        assert_error(&light, "2 is greater than the maximum value 1");

        let mut sprite = style(json!([]));
        sprite["sprite"] = json!([{ "id": "a", "url": "x" }, { "id": "a", "url": "y" }]);
        assert_error(
            &sprite,
            "all the sprites' ids must be unique, but a is duplicated",
        );

        let mut faces = style(json!([]));
        faces["font-faces"] =
            json!({ "Sans": [{ "url": "x", "unicode-range": ["U+0-7F", "U+4??", "U+110000"] }] });
        let errors = style_errors(&faces);
        assert_eq!(errors.len(), 1, "{errors:#?}");
        assert!(errors[0].contains("out of bounds"));
    }

    #[test]
    fn layers_follow_their_sources() {
        assert_error(
            &style(json!([{ "type": "fill", "source": "openmaptiles", "source-layer": "x" }])),
            "missing required property \"id\"",
        );
        assert_error(
            &style(json!([{ "id": "a", "type": "blob" }])),
            "layers[0].type: expected one of [",
        );
        assert_error(
            &style(json!([{ "id": "a", "type": "fill" }])),
            "missing required property \"source\"",
        );
        assert_error(
            &style(json!([{ "id": "a", "type": "fill", "source": "nope" }])),
            "source \"nope\" not found",
        );
        assert_error(
            &style(json!([{ "id": "a", "type": "fill", "source": "openmaptiles" }])),
            "must specify a \"source-layer\"",
        );
        assert_error(
            &style(json!([{ "id": "a", "type": "raster", "source": "openmaptiles" }])),
            "requires a raster source",
        );
        assert_error(
            &style(json!([{ "id": "a", "type": "fill", "source": "shade" }])),
            "requires a vector source",
        );
        assert_error(
            &style(
                json!([{ "id": "a", "type": "background" }, { "id": "a", "type": "background" }]),
            ),
            "duplicate layer id \"a\"",
        );
        assert_error(
            &style(json!([{ "id": "a", "type": "background", "minzoom": 30 }])),
            "30 is greater than the maximum value 24",
        );
        assert_valid(&style(json!([
            { "id": "a", "type": "background", "interactive": true },
            { "id": "b", "type": "raster", "source": "shade" },
        ])));
    }

    #[test]
    fn properties_follow_their_layer_type() {
        assert_error(
            &fill(json!({ "line-color": "red" })),
            "unknown property \"line-color\"",
        );
        assert_error(
            &fill(json!({ "fill-color": 42 })),
            "fill-color: color expected, number found",
        );
        assert_error(
            &fill(json!({ "fill-color": "not a colour" })),
            "color expected, \"not a colour\" found",
        );
        assert_error(
            &fill(json!({ "fill-opacity": 2 })),
            "2 is greater than the maximum value 1",
        );
        assert_error(
            &fill(json!({ "fill-color": "{colour}" })),
            "does not support interpolation syntax",
        );
        assert_valid(&fill(
            json!({ "fill-color": "hsl(35, 8%, 85%)", "fill-color-transition": { "duration": 0 } }),
        ));
        assert_error(
            &fill(json!({ "fill-antialias-transition": { "duration": 0 } })),
            "unknown property \"fill-antialias-transition\"",
        );
        let line = |layout: Json| {
            style(
                json!([{ "id": "a", "type": "line", "source": "openmaptiles", "source-layer": "x", "layout": layout }]),
            )
        };
        assert_error(
            &line(json!({ "line-cap": "pointy" })),
            "expected one of [butt, round, square], \"pointy\" found",
        );
        assert_valid(&line(json!({ "line-cap": "round", "visibility": "none" })));
    }

    #[test]
    fn expressions_are_parsed_against_their_property() {
        assert_valid(&fill(json!({
            "fill-color": ["interpolate", ["linear"], ["zoom"], 5, "red", 10, ["get", "colour"]],
            "fill-opacity": ["step", ["zoom"], 0, 10, 1],
        })));
        assert_error(
            &fill(json!({ "fill-color": ["+", 1, 2] })),
            "Expected color but found number",
        );
        assert_error(
            &fill(json!({ "fill-antialias": ["get", "aa"] })),
            "data expressions not supported",
        );
        assert_error(
            &fill(json!({ "fill-opacity": ["*", ["zoom"], 0.1] })),
            "\"zoom\" expression may only be used as input to a top-level \"step\" or \"interpolate\" expression.",
        );
        assert_error(
            &fill(
                json!({ "fill-opacity": ["interpolate", ["linear"], ["zoom"], 1, ["step", ["zoom"], 0, 2, 1], 3, 1] }),
            ),
            "Only one zoom-based",
        );
        let symbol = |layout: Json| {
            style(
                json!([{ "id": "a", "type": "symbol", "source": "openmaptiles", "source-layer": "x", "layout": layout }]),
            )
        };
        assert_error(
            &symbol(json!({ "text-font": ["get", "font"] })),
            "Invalid data expression for \"text-font\"",
        );
        assert_valid(&symbol(
            json!({ "text-font": ["step", ["zoom"], ["literal", ["A"]], 10, ["literal", ["B"]]] }),
        ));
        assert_error(
            &symbol(json!({ "text-size": ["feature-state", "size"] })),
            "\"feature-state\" data expressions are not supported with layout properties.",
        );
        assert_error(
            &symbol(json!({ "visibility": ["step", ["zoom"], "none", 10, "visible"] })),
            "zoom expressions not supported",
        );
    }

    #[test]
    fn legacy_functions_follow_their_stops() {
        assert_valid(&fill(
            json!({ "fill-opacity": { "base": 1.2, "stops": [[5, 0], [10, 1]] } }),
        ));
        assert_valid(&fill(
            json!({ "fill-color": { "property": "class", "type": "categorical", "stops": [["a", "red"], ["b", "blue"]] } }),
        ));
        assert_error(
            &fill(json!({ "fill-opacity": { "stops": [[10, 0], [5, 1]] } })),
            "stop domain values must appear in ascending order",
        );
        assert_error(
            &fill(json!({ "fill-opacity": { "stops": [] } })),
            "array must have at least one stop",
        );
        assert_error(
            &fill(json!({ "fill-opacity": { "stops": [[5, 2]] } })),
            "2 is greater than the maximum value 1",
        );
        assert_error(
            &fill(json!({ "fill-opacity": { "stops": [[5, ["get", "x"]]] } })),
            "expressions are not allowed in function stops.",
        );
        assert_error(
            &fill(json!({ "fill-antialias": { "property": "x", "stops": [[1, true]] } })),
            "property functions not supported",
        );
        assert_error(
            &fill(json!({ "fill-color": { "type": "categorical", "stops": [["a", "red"]] } })),
            "\"property\" property is required",
        );
        assert_error(
            &fill(
                json!({ "fill-color": { "property": "x", "type": "categorical", "stops": [[1.5, "red"]] } }),
            ),
            "integer expected, found 1.5",
        );
        assert_error(
            &fill(
                json!({ "fill-color": { "property": "x", "type": "categorical", "stops": [["a", "red"], ["a", "blue"]] } }),
            ),
            "stop domain values must be unique",
        );
        assert_error(
            &fill(json!({ "fill-opacity": { "stops": [[5, 1]], "bogus": 1 } })),
            "unknown property \"bogus\"",
        );
        assert_valid(&fill(
            json!({ "fill-opacity": { "property": "h", "stops": [[{ "zoom": 0, "value": 0 }, 0], [{ "zoom": 10, "value": 1 }, 1]] } }),
        ));
    }

    #[test]
    fn filters_are_legacy_or_expressions() {
        let filtered = |filter: Json| {
            style(
                json!([{ "id": "a", "type": "fill", "source": "openmaptiles", "source-layer": "x", "filter": filter }]),
            )
        };
        assert_valid(&filtered(json!([
            "all",
            ["==", "class", "lake"],
            ["!in", "brunnel", "tunnel"],
            ["has", "name"]
        ])));
        assert_valid(&filtered(json!([
            "all",
            ["==", ["get", "class"], "lake"],
            ["has", "name"]
        ])));
        assert_valid(&filtered(json!(true)));
        // A two-element comparison reads as an expression; only under `none` is it a legacy filter.
        assert_error(
            &filtered(json!(["==", "class"])),
            "Expected 2 or 3 arguments",
        );
        assert_error(
            &filtered(json!(["none", ["==", "class"]])),
            "filter array for operator \"==\" must have 3 elements",
        );
        assert_error(
            &filtered(json!(["<", "$type", "Point"])),
            "\"$type\" cannot be use with operator \"<\"",
        );
        assert_error(
            &filtered(json!(["==", "$type", "Circle"])),
            "Polygon], \"Circle\" found",
        );
        assert_error(
            &filtered(json!(["!has"])),
            "filter array for \"!has\" operator must have 2 elements",
        );
        assert_error(
            &filtered(json!(["==", ["get", "a"]])),
            "Expected 2 or 3 arguments",
        );
        assert_error(
            &filtered(json!(["==", ["feature-state", "a"], 1])),
            "not supported with filters",
        );
        // A `match` label is data: the zoom elsewhere in the filter leaves it alone.
        assert_valid(&filtered(json!([
            "all",
            ["<", ["zoom"], 10],
            ["match", ["get", "class"], ["zoom"], true, false]
        ])));

        assert!(is_legacy_filter(&json!(["none", ["has", "a"]])));
        assert!(!is_legacy_filter(&json!(["all", ["has", "a"]])));
        assert!(is_legacy_filter(&json!(["in", "class", "a", "b"])));
        assert!(!is_legacy_filter(&json!([
            "in",
            "class",
            ["literal", ["a"]]
        ])));
    }

    #[test]
    fn sources_follow_their_type() {
        let with_source =
            |source: Json| json!({ "version": 8, "sources": { "s": source }, "layers": [] });
        assert_error(
            &with_source(json!({ "url": "x" })),
            "sources.s: \"type\" is required",
        );
        assert_error(
            &with_source(json!({ "type": "canvas" })),
            "Please use runtime APIs",
        );
        assert_error(
            &with_source(json!({ "type": "tiles" })),
            "sources.s.type: expected one of [vector, raster",
        );
        assert_error(
            &with_source(json!({ "type": "raster", "tiles": "x" })),
            "array expected, string found",
        );
        assert_error(
            &with_source(json!({ "type": "geojson" })),
            "missing required property \"data\"",
        );
        assert_error(
            &with_source(json!({ "type": "raster-dem", "redFactor": 1 })),
            "only valid when \"encoding\" is set to \"custom\"",
        );
        assert_valid(&with_source(
            json!({ "type": "vector", "tiles": ["x"], "promoteId": { "a": "id" }, "custom": 1 }),
        ));
        assert_valid(&with_source(
            json!({ "type": "raster-dem", "encoding": "custom", "redFactor": 1 }),
        ));
    }
}
