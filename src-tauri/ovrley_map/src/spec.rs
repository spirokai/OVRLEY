//! What a map draws: which installed style and which layers.
//!
//! The shell registers the installed styles at startup and hands the catalog to everything that
//! validates a map widget, the WASM preview included. A widget naming a style that isn't installed
//! draws the first one instead, so a project keeps working on a machine without that style.

use serde::{Deserialize, Serialize};
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
        (!self.labels && layer_type == "symbol") || (!self.buildings && source_layer == Some("building"))
    }
}

/// One translated style: an installed style with its layer toggles applied.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MapStyleSpec {
    pub style: MapStyleId,
    pub layers: MapLayers,
}

/// An installed style's id: its file name without `.json`. ASCII letters, digits, `-` and `_`, so
/// it can sit in a URL path. Two ids that differ only in case name the same file on Windows, so they
/// collide.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MapStyleId(String);

impl MapStyleId {
    pub fn new(id: &str) -> Result<Self, String> {
        if id.is_empty()
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(format!(
                "Map style id {id:?} must be ASCII letters, digits, '-' or '_'"
            ));
        }
        Ok(Self(id.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the two would name the same file on a case-insensitive file system.
    pub fn collides(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
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

/// One installed style, as the editor lists it and the widget draws it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapStyleInfo {
    pub id: MapStyleId,
    /// The style's `name`, or its id when it has none.
    pub label: String,
    /// RGB of the style's opaque background, which the widget shows until its map draws.
    pub background: [u8; 3],
}

/// The installed styles, in the order the editor lists them. The first is what a widget draws when
/// its own style isn't installed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Vec<MapStyleInfo>", into = "Vec<MapStyleInfo>")]
pub struct MapStyleCatalog(Vec<MapStyleInfo>);

impl MapStyleCatalog {
    /// Fails when two ids collide.
    pub fn new(styles: Vec<MapStyleInfo>) -> Result<Self, String> {
        for (index, style) in styles.iter().enumerate() {
            if let Some(earlier) = styles[..index]
                .iter()
                .find(|earlier| earlier.id.collides(&style.id))
            {
                return Err(format!(
                    "Map styles {} and {} collide",
                    earlier.id, style.id
                ));
            }
        }
        Ok(Self(styles))
    }

    pub fn styles(&self) -> &[MapStyleInfo] {
        &self.0
    }

    pub fn get(&self, id: &str) -> Option<&MapStyleInfo> {
        self.0.iter().find(|style| style.id.as_str() == id)
    }

    /// What a widget asking for `id` draws: that style when installed, otherwise the first. `None`
    /// only when no style is installed at all.
    pub fn resolve(&self, id: &str) -> Option<&MapStyleInfo> {
        self.get(id).or_else(|| self.0.first())
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
        }
    }

    #[test]
    fn an_installed_style_resolves_to_itself_and_a_missing_one_to_the_first() {
        let catalog = MapStyleCatalog::new(vec![style("liberty"), style("dark")]).unwrap();
        assert_eq!(catalog.resolve("dark").unwrap().id.as_str(), "dark");
        assert_eq!(catalog.resolve("gone").unwrap().id.as_str(), "liberty");
        assert!(MapStyleCatalog::default().resolve("liberty").is_none());
    }

    #[test]
    fn ids_that_differ_only_in_case_collide() {
        assert!(MapStyleCatalog::new(vec![style("liberty"), style("Liberty")]).is_err());
    }

    #[test]
    fn an_id_must_fit_a_url_path() {
        for id in ["", "my style", "../liberty", "liberty.json"] {
            assert!(MapStyleId::new(id).is_err(), "{id:?}");
        }
        assert!(MapStyleId::new("my-style_2").is_ok());
    }
}
