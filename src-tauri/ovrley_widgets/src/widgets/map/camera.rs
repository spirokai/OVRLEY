//! The camera as a pure function of track position: the zoom and framing are constant per widget.
use super::{
    track::{MapTrack, TrackPosition},
    validation::{MapCameraMode, MapOrientation},
};
use crate::error::{WidgetError, WidgetResult};
use ovrley_map::{mercator::world_to_lon_lat, MapCamera, MAX_TILE_ZOOM, TILE_SIZE};

/// Equatorial circumference, in metres.
const EARTH_CIRCUMFERENCE_METERS: f64 = 40_075_016.69;
/// A follow camera sits low in the frame, showing more road ahead, as `NavigationMapController` does.
const FOLLOW_TOP_PADDING: f64 = 0.45;
/// Empty share of a fixed camera's frame on each side of the route.
const FIXED_MARGIN: f64 = 0.08;

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
    /// `width` and `height` are in scene units, so a scaled scene is a sharper version of one map.
    pub fn new(
        mode: MapCameraMode,
        track: &MapTrack,
        width: f32,
        height: f32,
    ) -> WidgetResult<Self> {
        Ok(match mode {
            MapCameraMode::Fixed => Self::Fixed(fixed_camera(
                track,
                f64::from(width),
                f64::from(height),
            )?),
            MapCameraMode::Follow {
                orientation,
                pitch_deg,
                range_meters,
            } => Self::Follow {
                zoom: follow_zoom(
                    track.mean_latitude(),
                    f64::from(range_meters),
                    f64::from(width),
                ),
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
                    MapOrientation::RiderUp => track.bearing_at(position.distance_meters),
                };
                MapCamera::new(
                    position.lon,
                    position.lat,
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

/// The zoom whose `width` scene units span `range_meters` of ground at this latitude.
fn follow_zoom(mean_latitude: f64, range_meters: f64, width: f64) -> f64 {
    let meters_per_unit = range_meters / width;
    clamp_zoom(
        (mean_latitude.to_radians().cos() * EARTH_CIRCUMFERENCE_METERS
            / (f64::from(TILE_SIZE) * meters_per_unit))
            .log2(),
    )
}

/// The whole route, north up, pitch 0.
fn fixed_camera(track: &MapTrack, width: f64, height: f64) -> WidgetResult<MapCamera> {
    let (mut min, mut max) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
    for point in track.world() {
        min = (min.0.min(point.0), min.1.min(point.1));
        max = (max.0.max(point.0), max.1.max(point.1));
    }
    let usable = 1.0 - FIXED_MARGIN * 2.0;
    let tile = f64::from(TILE_SIZE);
    let fit = |extent: f64, span: f64| (extent * usable / (span * tile)).log2();
    // A span of zero fits at any zoom, and the clamp takes it to the deepest one.
    let zoom = clamp_zoom(fit(width, max.0 - min.0).min(fit(height, max.1 - min.1)));
    let (lon, lat) = world_to_lon_lat((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0);
    MapCamera::new(lon, lat, zoom, 0.0, 0.0, 0.0).map_err(WidgetError::Render)
}
