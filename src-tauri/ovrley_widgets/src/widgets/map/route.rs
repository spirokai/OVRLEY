//! The route line the renderer draws: the track's smoothed line, reduced once at the widget's
//! constant view zoom. Reduction only drops vertices, so the smoothing comes from [`MapTrack`].
use super::track::MapTrack;
use crate::{
    error::{WidgetError, WidgetResult},
    rdp::simplify_rdp_indices_f64,
};
use ovrley_map::{MapRouteLine, MapRoutePoint, TILE_SIZE};

/// How far a reduced line may leave the smoothed one, in scene units at the view zoom. Under a pixel,
/// so a curve never shows its facets.
const REDUCTION_TOLERANCE_PX: f64 = 0.3;

pub fn route_line(track: &MapTrack, zoom: f64) -> WidgetResult<MapRouteLine> {
    let scene_units = f64::from(TILE_SIZE) * 2_f64.powf(zoom);
    let scaled: Vec<(f64, f64)> = track
        .world()
        .iter()
        .map(|point| (point.0 * scene_units, point.1 * scene_units))
        .collect();
    let kept = simplify_rdp_indices_f64(&scaled, REDUCTION_TOLERANCE_PX);
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
