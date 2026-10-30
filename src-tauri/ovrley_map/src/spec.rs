//! What a map draws: which style and which layers.

/// Style pixels along one tile edge at pixel ratio 1.
pub const TILE_SIZE: u32 = 512;
/// Deepest tile the widget renders. Data stops at z14 and overzooms from there.
pub const MAX_TILE_ZOOM: u8 = 18;
/// The vector source every OpenFreeMap style draws from.
pub const VECTOR_SOURCE: &str = "openmaptiles";

/// The OpenFreeMap style presets.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MapStyle {
    Positron,
    Bright,
    Liberty,
    Dark,
    Fiord,
}

impl MapStyle {
    pub const ALL: [Self; 5] = [
        Self::Positron,
        Self::Bright,
        Self::Liberty,
        Self::Dark,
        Self::Fiord,
    ];

    /// The name OpenFreeMap serves this style under.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Positron => "positron",
            Self::Bright => "bright",
            Self::Liberty => "liberty",
            Self::Dark => "dark",
            Self::Fiord => "fiord",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|style| style.name() == name)
    }
}

/// Which groups of map layers are drawn.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MapLayers {
    pub roads: bool,
    pub paths: bool,
    pub water: bool,
    pub buildings: bool,
    pub labels: bool,
}

impl MapLayers {
    /// Whether these toggles hide a style layer, from its `type`, `source-layer` and `id`.
    pub fn hides_layer(self, layer_type: &str, source_layer: Option<&str>, id: &str) -> bool {
        if !self.labels && layer_type == "symbol" {
            return true;
        }
        match source_layer {
            Some("transportation") if id.contains("path") || id.contains("pedestrian") => {
                !self.paths
            }
            Some("transportation" | "aeroway") => !self.roads,
            Some("water" | "waterway") => !self.water,
            Some("building") => !self.buildings,
            _ => false,
        }
    }
}

/// One translated style: a preset with its layer toggles applied.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MapStyleSpec {
    pub style: MapStyle,
    pub layers: MapLayers,
}
