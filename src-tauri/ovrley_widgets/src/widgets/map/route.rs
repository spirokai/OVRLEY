//! The route line the renderer draws, simplified once at the widget's constant view zoom.
use super::track::MapTrack;
use crate::{
    error::{WidgetError, WidgetResult},
    rdp::simplify_rdp_indices_f64,
};
use ovrley_map::{MapRouteLine, MapRoutePoint, TILE_SIZE};

/// `simplify_tolerance_px` applies in scene units at `zoom`, as the route widget's tolerance does.
pub fn route_line(track: &MapTrack, zoom: f64, tolerance_px: f32) -> WidgetResult<MapRouteLine> {
    let scene_units = f64::from(TILE_SIZE) * 2_f64.powf(zoom);
    let scaled: Vec<(f64, f64)> = track
        .world()
        .iter()
        .map(|point| (point.0 * scene_units, point.1 * scene_units))
        .collect();
    let kept = simplify_rdp_indices_f64(&scaled, f64::from(tolerance_px.max(0.05)));
    MapRouteLine::new(
        kept.into_iter()
            .map(|index| MapRoutePoint {
                world_x: track.world()[index].0,
                world_y: track.world()[index].1,
                along_metres: track.distance()[index] as f32,
            })
            .collect(),
    )
    .map_err(WidgetError::Render)
}
