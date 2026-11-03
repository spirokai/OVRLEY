//! Whether a style file can be installed, and what the catalog lists for it.
//!
//! A style installs when MapLibre would load it (see [`crate::style_spec`]) and it meets OVRLEY's
//! own contract:
//! - its tiles come from `sources.openmaptiles`, a vector source: the only tiles the tile service
//!   serves, whatever URL the file names;
//! - it has a visible, opaque `background` layer, whose colour the widget shows until the map
//!   draws. Export copies a frame's opaque interior rather than blending it, so it must be opaque;
//! - `metadata["ovrley:building-color"]`, when present, is a colour, and the style has no
//!   `fill-extrusion` layer of its own. Without either, its added 3D buildings take the colour of
//!   its flat building fill.
//!
//! The spec's optional `name` labels the style; without one its id does.
//!
//! The two style-document helpers are shared with the renderer, which reads what registration
//! checked.

#[cfg(feature = "validate")]
use crate::{style_spec::style_errors, MapStyleId, MapStyleInfo};
use crate::{BUILDING_COLOR_KEY, VECTOR_SOURCE};
#[cfg(feature = "validate")]
use maplibre_expr::{evaluate, parse_property, typecheck, Color, EvaluationContext, Type, Value};
use serde_json::Value as Json;

/// The colour `metadata` names for the 3D buildings OVRLEY adds, as written.
pub(crate) fn building_color(style: &Json) -> Option<&Json> {
    style.get("metadata")?.get(BUILDING_COLOR_KEY)
}

/// Whether the style draws 3D buildings of its own from the vector source.
pub(crate) fn has_own_extrusion(style: &Json) -> bool {
    style["layers"].as_array().is_some_and(|layers| {
        layers
            .iter()
            .any(|layer| layer["type"] == "fill-extrusion" && layer["source"] == VECTOR_SOURCE)
    })
}

#[cfg(feature = "validate")]
/// The zoom the background is read at: the deepest tile data, which every widget camera shows.
const BACKGROUND_ZOOM: f64 = 14.0;

/// Reads `bytes` as the style `id`, or says every way it fails.
#[cfg(feature = "validate")]
pub fn read_style_file(id: MapStyleId, bytes: &[u8]) -> Result<MapStyleInfo, Vec<String>> {
    let style: Json =
        serde_json::from_slice(bytes).map_err(|error| vec![format!("not valid JSON: {error}")])?;
    let errors = style_errors(&style);
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut errors = Vec::new();
    if style["sources"][VECTOR_SOURCE]["type"] != "vector" {
        errors.push(format!("sources.{VECTOR_SOURCE} must be a vector source"));
    }
    if let Some(color) = building_color(&style) {
        if color
            .as_str()
            .is_none_or(|color| Color::parse(color).is_none())
        {
            errors.push(format!(
                "metadata.{BUILDING_COLOR_KEY} must be a colour, found {color}"
            ));
        }
        if has_own_extrusion(&style) {
            errors.push(format!(
                "metadata.{BUILDING_COLOR_KEY} is only for a style without its own fill-extrusion layer"
            ));
        }
    }
    match background(&style) {
        Ok(background) if errors.is_empty() => Ok(MapStyleInfo {
            label: style["name"].as_str().unwrap_or(id.as_str()).to_string(),
            id,
            background,
        }),
        Ok(_) => Err(errors),
        Err(error) => {
            errors.push(error);
            Err(errors)
        }
    }
}

#[cfg(feature = "validate")]
/// The first visible background layer's colour at [`BACKGROUND_ZOOM`], which must be opaque.
fn background(style: &Json) -> Result<[u8; 3], String> {
    let layer = style["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|layer| layer["type"] == "background" && layer["layout"]["visibility"] != "none")
        .ok_or("needs a visible background layer for the widget to show until its map draws")?;
    let id = layer["id"].as_str().unwrap_or_default();
    let read = |name: &str, default: &str, expected: Type| -> Result<Value, String> {
        let value = layer["paint"]
            .get(name)
            .cloned()
            .unwrap_or_else(|| serde_json::from_str(default).expect("a valid default"));
        let expr = parse_property(name, &value)
            .and_then(|expr| typecheck(&expr, Some(&expected), false))
            .map_err(|error| format!("layers {id} {name}: {error}"))?;
        evaluate(&expr, &EvaluationContext::new().with_zoom(BACKGROUND_ZOOM))
            .map_err(|error| format!("layers {id} {name}: {error}"))
    };
    // The spec's defaults for both.
    let Value::Color(color) = read("background-color", "\"#000000\"", Type::Color)? else {
        return Err(format!("layers {id} background-color is not a colour"));
    };
    let opacity = read("background-opacity", "1", Type::Number)?
        .as_number()
        .ok_or_else(|| format!("layers {id} background-opacity is not a number"))?;
    if color.a * opacity < 1.0 {
        return Err(format!(
            "layers {id}: the background must be opaque at zoom {BACKGROUND_ZOOM}"
        ));
    }
    let channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Ok([channel(color.r), channel(color.g), channel(color.b)])
}

#[cfg(all(test, feature = "validate"))]
mod tests {
    use super::*;
    use serde_json::json;

    fn read(style: &Json) -> Result<MapStyleInfo, Vec<String>> {
        read_style_file(
            MapStyleId::new("custom").unwrap(),
            &serde_json::to_vec(style).unwrap(),
        )
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

    #[test]
    fn the_bundled_styles_keep_their_labels_and_backgrounds() {
        let bundled = [
            (
                "bright",
                include_str!("../../../assets/map-styles/bright.json"),
                "Bright",
                [0xf8, 0xf4, 0xf0],
            ),
            (
                "dark",
                include_str!("../../../assets/map-styles/dark.json"),
                "Dark",
                [12, 12, 12],
            ),
            (
                "fiord",
                include_str!("../../../assets/map-styles/fiord.json"),
                "Fiord",
                [0x45, 0x51, 0x6e],
            ),
            (
                "liberty",
                include_str!("../../../assets/map-styles/liberty.json"),
                "Liberty",
                [0xf8, 0xf4, 0xf0],
            ),
            (
                "positron",
                include_str!("../../../assets/map-styles/positron.json"),
                "Positron",
                [242, 243, 240],
            ),
        ];
        for (id, json, label, background) in bundled {
            let info = read_style_file(MapStyleId::new(id).unwrap(), json.as_bytes()).unwrap();
            assert_eq!(
                (info.label.as_str(), info.background),
                (label, background),
                "{id}"
            );
            let style: Json = serde_json::from_str(json).unwrap();
            // Liberty brings its own 3D buildings, the others name the colour of the added ones.
            assert_eq!(building_color(&style).is_some(), id != "liberty", "{id}");
        }
    }

    #[test]
    fn a_style_without_a_name_is_labelled_by_its_id_and_shows_its_first_visible_background() {
        let info = read(&style()).unwrap();
        assert_eq!(info.label, "custom");
        assert_eq!(info.background, [10, 20, 30]);
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
    fn building_color_must_be_a_colour_for_a_style_without_its_own_buildings() {
        let mut colour = style();
        colour["metadata"] = json!({ BUILDING_COLOR_KEY: "nope" });
        assert!(read(&colour).unwrap_err()[0].contains("must be a colour"));

        let mut both = style();
        both["metadata"] = json!({ BUILDING_COLOR_KEY: "red" });
        both["layers"].as_array_mut().unwrap().push(json!({
            "id": "3d", "type": "fill-extrusion", "source": "openmaptiles", "source-layer": "building",
        }));
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
