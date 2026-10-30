//! The route a map frame draws as a ground layer: above the roads and below the buildings.
//!
//! The widget owns what the route is and the renderer owns drawing it, so this is shared vocabulary.

use std::sync::Arc;

/// One vertex of a route line.
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
            return Err(format!(
                "Map route needs two points, got {}",
                points.len()
            ));
        }
        Ok(Self { points })
    }

    pub fn points(&self) -> &[MapRoutePoint] {
        &self.points
    }
}

/// How the two sides of the split draw. Colours are straight-alpha floats and widths are output pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapRouteStyle {
    pub completed_color: [f32; 4],
    pub completed_width_px: f32,
    pub remaining_color: [f32; 4],
    pub remaining_width_px: f32,
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
