//! The camera as a pure function of track position: the zoom and framing are constant per widget.
use super::{
    track::{MapTrack, TrackPosition},
    validation::{MapCameraMode, MapOrientation},
};
use crate::error::{WidgetError, WidgetResult};
use ovrley_map::{mercator::world_to_lon_lat, MapCamera, MAX_TILE_ZOOM, TILE_SIZE};

/// A follow camera sits low in the frame, showing more road ahead, as `NavigationMapController` does.
const FOLLOW_TOP_PADDING: f64 = 0.45;
/// Empty share of a fixed camera's frame on each side of the route.
const FIXED_MARGIN: f64 = 0.08;
/// The frame a fixed camera fits the route into at zoom offset 0, in scene units: the default widget's.
/// It doesn't follow the widget's size, so a side resize crops the map. A corner resize rescales it
/// through the zoom offset instead.
const FIXED_FIT_FRAME: f64 = 500.0;
/// Scene units the route's longest side spans at zoom offset 0.
const FIXED_ROUTE_SPAN: f64 = FIXED_FIT_FRAME * (1.0 - FIXED_MARGIN * 2.0);

/// Widget units one unit of pan spans at `zoom_offset`: the route's longest side on screen.
pub(super) fn fixed_route_span(zoom_offset: f64) -> f64 {
    FIXED_ROUTE_SPAN * zoom_offset.exp2()
}

/// One widget's camera with its constant parts resolved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WidgetCamera {
    Fixed(MapCamera),
    Follow {
        zoom: f64,
        pitch_deg: f64,
        orientation: MapOrientation,
    },
}

impl WidgetCamera {
    /// Never depends on the widget's size, so a size edit shows more or less of the same map.
    pub fn new(mode: MapCameraMode, track: &MapTrack) -> WidgetResult<Self> {
        Ok(match mode {
            MapCameraMode::Fixed { zoom_offset, pan } => Self::Fixed(fixed_camera(
                track,
                f64::from(zoom_offset),
                pan.map(f64::from),
            )?),
            MapCameraMode::Follow {
                orientation,
                pitch_deg,
                zoom,
            } => Self::Follow {
                zoom: f64::from(zoom),
                pitch_deg: f64::from(pitch_deg),
                orientation,
            },
        })
    }

    pub fn zoom(&self) -> f64 {
        match self {
            Self::Fixed(camera) => camera.zoom(),
            Self::Follow { zoom, .. } => *zoom,
        }
    }

    pub fn at(&self, track: &MapTrack, position: &TrackPosition) -> WidgetResult<MapCamera> {
        match *self {
            Self::Fixed(camera) => Ok(camera),
            Self::Follow {
                zoom,
                pitch_deg,
                orientation,
            } => {
                let bearing = match orientation {
                    MapOrientation::NorthUp => 0.0,
                    MapOrientation::RiderUp => track.bearing_at(position),
                };
                let (lon, lat) = world_to_lon_lat(position.world.0, position.world.1);
                MapCamera::new(
                    lon,
                    lat,
                    zoom,
                    pitch_deg,
                    bearing,
                    FOLLOW_TOP_PADDING,
                )
                .map_err(WidgetError::Render)
            }
        }
    }
}

fn clamp_zoom(zoom: f64) -> f64 {
    zoom.clamp(0.0, f64::from(MAX_TILE_ZOOM))
}

/// North up, pitch 0. `zoom_offset` counts levels past the zoom that fits the whole route, and a
/// positive `pan` moves the route right and down in the frame, as a share of its longest side. Both
/// are relative to the route, so a project's framing carries over to another activity.
fn fixed_camera(track: &MapTrack, zoom_offset: f64, pan: [f64; 2]) -> WidgetResult<MapCamera> {
    let (mut min, mut max) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
    for point in track.world() {
        min = (min.0.min(point.0), min.1.min(point.1));
        max = (max.0.max(point.0), max.1.max(point.1));
    }
    let tile = f64::from(TILE_SIZE);
    let longest = (max.0 - min.0).max(max.1 - min.1);
    // A span of zero fits at any zoom, and the clamp takes it to the deepest one.
    let zoom = clamp_zoom((FIXED_ROUTE_SPAN / (longest * tile)).log2() + zoom_offset);
    let (lon, lat) = world_to_lon_lat(
        (min.0 + max.0) / 2.0 - pan[0] * longest,
        (min.1 + max.1) / 2.0 - pan[1] * longest,
    );
    MapCamera::new(lon, lat, zoom, 0.0, 0.0, 0.0).map_err(WidgetError::Render)
}
