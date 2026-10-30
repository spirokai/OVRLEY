//! The identity of a map texture: which style, which layers, which tile, how dense.

/// Style pixels along one tile edge at pixel ratio 1.
pub const TILE_SIZE: u32 = 512;
/// Deepest tile the widget renders. Data stops at z14 and overzooms from there.
pub const MAX_TILE_ZOOM: u8 = 18;
/// The vector source every OpenFreeMap style draws from.
pub const VECTOR_SOURCE: &str = "openmaptiles";
/// Pixel ratio bounds in thousandths, matching what ezu accepts.
pub const MIN_RATIO_MILLI: u16 = 1000;
pub const MAX_RATIO_MILLI: u16 = 4000;

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
    pub const ALL: Self = Self {
        roads: true,
        paths: true,
        water: true,
        buildings: true,
        labels: true,
    };

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

/// Fully describes one tile texture, so no style registration call is needed.
///
/// Built only through [`TileKey::new`], so a key in hand is always a valid tile.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TileKey {
    spec: MapStyleSpec,
    z: u8,
    x: u32,
    y: u32,
    ratio_milli: u16,
}

impl TileKey {
    /// Fails when the tile is off the grid or the pixel ratio is out of range.
    pub fn new(
        spec: MapStyleSpec,
        z: u8,
        x: u32,
        y: u32,
        ratio_milli: u16,
    ) -> Result<Self, String> {
        if z > MAX_TILE_ZOOM {
            return Err(format!("Map tile zoom {z} exceeds {MAX_TILE_ZOOM}"));
        }
        let limit = 1_u32 << z;
        if x >= limit || y >= limit {
            return Err(format!("Map tile {z}/{x}/{y} is off the tile grid"));
        }
        if !(MIN_RATIO_MILLI..=MAX_RATIO_MILLI).contains(&ratio_milli) {
            return Err(format!(
                "Map tile pixel ratio {ratio_milli} is outside {MIN_RATIO_MILLI}..={MAX_RATIO_MILLI}"
            ));
        }
        Ok(Self {
            spec,
            z,
            x,
            y,
            ratio_milli,
        })
    }

    pub const fn spec(self) -> MapStyleSpec {
        self.spec
    }

    pub const fn z(self) -> u8 {
        self.z
    }

    pub const fn x(self) -> u32 {
        self.x
    }

    pub const fn y(self) -> u32 {
        self.y
    }

    pub const fn ratio_milli(self) -> u16 {
        self.ratio_milli
    }
}

/// Texture edge in device pixels at a pixel ratio, rounded as ezu rounds its canvas.
pub const fn tile_pixels(ratio_milli: u16) -> u32 {
    (TILE_SIZE * ratio_milli as u32 + 500) / 1000
}
