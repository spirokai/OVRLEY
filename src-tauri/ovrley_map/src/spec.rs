//! What a map draws: which installed style and which layers.
//!
//! The shell registers the installed styles at startup. At least one is always installed, and a
//! widget naming a style that isn't installed draws the first one instead, so a project keeps
//! working on a machine without that style. Only drawing resolves a style: validating a widget
//! checks that its style id is well formed, nothing more.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::fmt;

/// Style pixels along one tile edge at pixel ratio 1.
pub const TILE_SIZE: u32 = 512;
/// Deepest tile the widget renders. Data stops at z14 and overzooms from there.
pub const MAX_TILE_ZOOM: u8 = 19;
/// The vector source every installed style draws from: the OpenMapTiles schema the tile service
/// serves.
pub const VECTOR_SOURCE: &str = "openmaptiles";
/// The `metadata` key naming the colour of the 3D buildings OVRLEY adds to a style without its own
/// `fill-extrusion` layer. Without it they take the colour of the style's flat building fill.
pub const BUILDING_COLOR_KEY: &str = "ovrley:building-color";

/// Which groups of map layers are drawn.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MapLayers {
    pub buildings: bool,
    pub labels: bool,
    /// Footways, cycleways, steps and bridleways: OpenMapTiles' `path` class of roads.
    pub paths: bool,
}

impl MapLayers {
    /// Whether these toggles hide a style layer, from its `type` and `source-layer`.
    pub fn hides_layer(self, layer_type: &str, source_layer: Option<&str>) -> bool {
        (!self.labels && layer_type == "symbol")
            || (!self.buildings && source_layer == Some("building"))
    }
}

/// One translated style: an installed style with its layer toggles applied.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MapStyleSpec {
    pub style: MapStyleId,
    pub layers: MapLayers,
}

/// An installed style's id: its file name without `.json`, in lowercase. Lowercase ASCII letters,
/// digits, `-` and `_`, so it can sit in a URL path and names one file on any file system.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MapStyleId(String);

impl MapStyleId {
    pub fn new(id: &str) -> Result<Self, String> {
        if id.is_empty()
            || !id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
            })
        {
            return Err(format!(
                "Map style id {id:?} must be lowercase ASCII letters, digits, '-' or '_'"
            ));
        }
        Ok(Self(id.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for MapStyleId {
    type Error = String;

    fn try_from(id: String) -> Result<Self, String> {
        Self::new(&id)
    }
}

impl From<MapStyleId> for String {
    fn from(id: MapStyleId) -> Self {
        id.0
    }
}

impl fmt::Display for MapStyleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The 3D buildings a style draws when a widget shows them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapStyleBuildings {
    /// The style has nothing to extrude.
    None,
    /// Its own `fill-extrusion` layers.
    Own,
    /// OVRLEY's extrusion of the `building` source layer, in this colour: a colour value as a
    /// style's `fill-extrusion-color` takes it.
    Added(Json),
}

/// One installed style, as the editor lists it and the widget draws it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapStyleInfo {
    pub id: MapStyleId,
    /// The style's `name`, or its id when it has none.
    pub label: String,
    /// RGB of the style's opaque background, which the widget shows until its map draws.
    pub background: [u8; 3],
    pub buildings: MapStyleBuildings,
}

/// The installed styles, in the order the editor lists them: never empty, and the first is what a
/// widget draws when its own style isn't installed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Vec<MapStyleInfo>", into = "Vec<MapStyleInfo>")]
pub struct MapStyleCatalog(Vec<MapStyleInfo>);

impl MapStyleCatalog {
    /// Fails without any style, or when two share an id.
    pub fn new(styles: Vec<MapStyleInfo>) -> Result<Self, String> {
        if styles.is_empty() {
            return Err("No map style is installed".to_string());
        }
        for (index, style) in styles.iter().enumerate() {
            if styles[..index].iter().any(|earlier| earlier.id == style.id) {
                return Err(format!("Map style {} is listed twice", style.id));
            }
        }
        Ok(Self(styles))
    }

    pub fn styles(&self) -> &[MapStyleInfo] {
        &self.0
    }

    pub fn get(&self, id: &MapStyleId) -> Option<&MapStyleInfo> {
        self.0.iter().find(|style| &style.id == id)
    }

    /// What a widget asking for `id` draws: that style when installed, otherwise the first.
    pub fn resolve(&self, id: &MapStyleId) -> &MapStyleInfo {
        self.get(id).unwrap_or(&self.0[0])
    }
}

impl TryFrom<Vec<MapStyleInfo>> for MapStyleCatalog {
    type Error = String;

    fn try_from(styles: Vec<MapStyleInfo>) -> Result<Self, String> {
        Self::new(styles)
    }
}

impl From<MapStyleCatalog> for Vec<MapStyleInfo> {
    fn from(catalog: MapStyleCatalog) -> Self {
        catalog.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(id: &str) -> MapStyleInfo {
        MapStyleInfo {
            id: MapStyleId::new(id).unwrap(),
            label: id.to_string(),
            background: [0, 0, 0],
            buildings: MapStyleBuildings::None,
        }
    }

    fn id(id: &str) -> MapStyleId {
        MapStyleId::new(id).unwrap()
    }

    #[test]
    fn an_installed_style_resolves_to_itself_and_a_missing_one_to_the_first() {
        let catalog = MapStyleCatalog::new(vec![style("liberty"), style("dark")]).unwrap();
        assert_eq!(catalog.resolve(&id("dark")).id, id("dark"));
        assert_eq!(catalog.resolve(&id("gone")).id, id("liberty"));
    }

    #[test]
    fn a_catalog_is_never_empty_and_lists_each_id_once() {
        assert!(MapStyleCatalog::new(Vec::new()).is_err());
        assert!(serde_json::from_str::<MapStyleCatalog>("[]").is_err());
        assert!(MapStyleCatalog::new(vec![style("liberty"), style("liberty")]).is_err());
    }

    #[test]
    fn an_id_must_fit_a_url_path_in_lowercase() {
        for id in ["", "my style", "../liberty", "liberty.json", "Liberty"] {
            assert!(MapStyleId::new(id).is_err(), "{id:?}");
        }
        assert!(MapStyleId::new("my-style_2").is_ok());
    }
}
