//! The route line: the track's smoothed line, reduced once at the widget's constant view zoom.
//! Reduction only drops vertices, so the smoothing comes from [`MapTrack`]. A chase camera's renderer
//! draws it under the buildings. A top-down widget strokes it over the frame in Skia, projected once.
use super::track::MapTrack;
use crate::{
    error::{WidgetError, WidgetResult},
    rdp::simplify_rdp_indices_f64,
    widgets::route::geometry::ProgressLine,
};
use ovrley_map::{MapCamera, MapRouteLine, MapRoutePoint, MapViewport, ViewState, TILE_SIZE};

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

/// `line` in `viewport`'s output pixels, where the renderer places the map under `camera`, with
/// travelled distance as progress. The marker projects through the same matrix, so the route meets the
/// roads and the marker wherever the camera points. Straight down, every ground point is in front of
/// the camera, so the whole line projects.
pub fn screen_line(
    line: &MapRouteLine,
    camera: &MapCamera,
    viewport: MapViewport,
) -> WidgetResult<ProgressLine> {
    let projection = ViewState::new(camera, viewport).ground_projection();
    let (width, height) = (
        f64::from(viewport.width_px()),
        f64::from(viewport.height_px()),
    );
    let points = line
        .points()
        .iter()
        .map(|point| {
            let (x, y) = projection.clip(point.world_x, point.world_y).window_share();
            ((x * width) as f32, (y * height) as f32)
        })
        .collect();
    ProgressLine::new(
        points,
        line.points()
            .iter()
            .map(|point| point.along_metres)
            .collect(),
    )
}
