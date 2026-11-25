//! Map vocabulary shared by the widgets, the preview and export, plus the renderer and the style
//! validator.
//!
//! Without features this crate has no wgpu, which `ovrley_widgets` does not need. The `gpu`
//! feature adds the renderer that export and the preview draw with. The `validate`
//! feature adds what the shell registers installed styles with.
//!
//! What a map draws: which installed style and which layers. The shell registers the installed styles
//! at startup. At least one is always installed, and a widget naming a style that isn't installed
//! draws the first one instead, so a project keeps working on a machine without that style. Only
//! drawing resolves a style: validating a widget checks that its style id is well formed, nothing more.

mod camera;
#[cfg(feature = "gpu")]
pub mod gpu;
#[cfg(feature = "validate")]
mod spec;
#[cfg(feature = "validate")]
mod validate;

pub use camera::{
    data_level, frame_tiles, view_tiles, ClipPoint, FrameTiles, GroundProjection, MapCamera,
    MapLabelDraw, MapViewport, TileId, ViewState, LABEL_PLACEMENTS, LIGHT_XY, MAX_PITCH_DEG,
    MAX_VIEW_TILES,
};
#[cfg(feature = "validate")]
pub use validate::read_style_file;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::fmt;
use std::sync::Arc;

/// Web Mercator projection helpers.
pub mod mercator {
    use std::f64::consts::PI;

    /// Latitude where the square Web Mercator world ends.
    const MAX_LATITUDE: f64 = 85.051_128_779_806_59;

    /// Longitude and latitude in degrees to world coordinates in `0.0..=1.0`, north at 0.
    pub fn lon_lat_to_world(lon: f64, lat: f64) -> (f64, f64) {
        let lat = lat.clamp(-MAX_LATITUDE, MAX_LATITUDE).to_radians();
        (
            (lon + 180.0) / 360.0,
            (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / PI) / 2.0,
        )
    }

    /// World coordinates in `0.0..=1.0` back to longitude and latitude in degrees.
    pub fn world_to_lon_lat(world_x: f64, world_y: f64) -> (f64, f64) {
        let lat = (PI * (1.0 - 2.0 * world_y)).sinh().atan();
        (world_x * 360.0 - 180.0, lat.to_degrees())
    }
}

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
    /// Each place's own name, as written locally, instead of the one it is known by internationally.
    pub local_names: bool,
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

    /// What a widget asking for `id` draws: that style when installed, otherwise the first.
    pub fn resolve(&self, id: &MapStyleId) -> &MapStyleInfo {
        self.0
            .iter()
            .find(|style| &style.id == id)
            .unwrap_or(&self.0[0])
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

/// One vertex of a route line: the route a map frame draws as a ground layer, above the roads and
/// below the buildings. The widget owns what the route is and the renderer owns drawing it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapRoutePoint {
    /// Mercator world coordinates in `0.0..=1.0`.
    pub world_x: f64,
    pub world_y: f64,
    /// Travelled distance at this vertex. Never decreases along the line.
    pub along_metres: f32,
}

/// A route's vertices. Built only through [`MapRouteLine::new`], so a line in hand has a segment.
#[derive(Debug, PartialEq)]
pub struct MapRouteLine {
    points: Vec<MapRoutePoint>,
}

impl MapRouteLine {
    pub fn new(points: Vec<MapRoutePoint>) -> Result<Self, String> {
        if points.len() < 2 {
            return Err(format!("Map route needs two points, got {}", points.len()));
        }
        Ok(Self { points })
    }

    pub fn points(&self) -> &[MapRoutePoint] {
        &self.points
    }
}

/// How the two sides of the split draw. Colours are straight-alpha floats; both sides share one width
/// in output pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapRouteStyle {
    pub completed_color: [f32; 4],
    pub remaining_color: [f32; 4],
    pub width_px: f32,
}

/// One frame's route: which line, how it looks, and where the split sits.
#[derive(Clone, Debug)]
pub struct MapRouteDraw {
    pub line: Arc<MapRouteLine>,
    pub style: MapRouteStyle,
    pub travelled_metres: f32,
}

/// Lines compare by identity: a widget keeps one until its geometry changes, and comparing every
/// vertex per frame would cost more than drawing them.
impl PartialEq for MapRouteDraw {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.line, &other.line)
            && self.style == other.style
            && self.travelled_metres == other.travelled_metres
    }
}

/// The direct sub-expressions of `expr`, in the order MapLibre's `eachChild` visits them.
#[cfg(any(feature = "gpu", feature = "validate"))]
fn sub_expressions(expr: &maplibre_expr::Expr) -> Vec<&maplibre_expr::Expr> {
    use maplibre_expr::Expr;
    match expr {
        Expr::Literal(_) | Expr::Var(_) | Expr::Within(_) | Expr::Distance(_) => Vec::new(),
        Expr::Call { args, .. } => args.iter().collect(),
        Expr::Let { bindings, body } => bindings
            .iter()
            .map(|(_, value)| value)
            .chain([&**body])
            .collect(),
        Expr::Match {
            input,
            arms,
            default,
        } => [&**input]
            .into_iter()
            .chain(arms.iter().map(|(_, output)| output))
            .chain([&**default])
            .collect(),
        Expr::Step {
            input,
            output0,
            stops,
        } => [&**input, &**output0]
            .into_iter()
            .chain(stops.iter().map(|(_, output)| output))
            .collect(),
        Expr::Interpolate { input, stops, .. } => [&**input]
            .into_iter()
            .chain(stops.iter().map(|(_, output)| output))
            .collect(),
        Expr::Format(sections) => sections
            .iter()
            .flat_map(|section| {
                [Some(&section.content)]
                    .into_iter()
                    .chain([
                        section.scale.as_ref(),
                        section.font.as_ref(),
                        section.text_color.as_ref(),
                        section.vertical_align.as_ref(),
                    ])
                    .flatten()
            })
            .collect(),
        Expr::Collator {
            case_sensitive,
            diacritic_sensitive,
            locale,
        } => [case_sensitive, diacritic_sensitive, locale]
            .into_iter()
            .flatten()
            .map(|expr| &**expr)
            .collect(),
        Expr::NumberFormat {
            value,
            locale,
            currency,
            min_fraction_digits,
            max_fraction_digits,
            unit,
        } => [&**value]
            .into_iter()
            .chain(
                [
                    locale,
                    currency,
                    min_fraction_digits,
                    max_fraction_digits,
                    unit,
                ]
                .into_iter()
                .flatten()
                .map(|expr| &**expr),
            )
            .collect(),
        Expr::Assert(_, inner) | Expr::Coerce(_, inner) => vec![&**inner],
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
