//! The camera as a pure function of track position: the zoom and framing are constant per widget.
use super::{
    track::{MapTrack, TrackPosition},
    validation::{MapCameraMode, MapOrientation},
};
use crate::error::{WidgetError, WidgetResult};
use ovrley_map::{
    mercator::world_to_lon_lat, MapCamera, MapLabelDraw, LABEL_PLACEMENTS, MAX_TILE_ZOOM, TILE_SIZE,
};

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

/// Seconds of activity time between label placements.
const LABEL_STEP_SECONDS: f64 = 0.3;

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

    /// How a frame at `second` places and fades its labels. Placement runs on a grid of
    /// `LABEL_STEP_SECONDS`, at the camera of each grid time. A frame between grid times `t_k` and `t_k+1`
    /// fades from the placement at `t_k-1` to the one at `t_k` as it runs from `t_k` to `t_k+1`, and names
    /// the grid times before those too, back to `LABEL_PLACEMENTS` in all, so the renderer can place each
    /// on the one before it. A fixed camera never moves, so its one placement serves every frame.
    pub fn label_draw(&self, track: &MapTrack, second: f64) -> WidgetResult<MapLabelDraw> {
        let (cameras, blend) = match *self {
            Self::Fixed(camera) => ([camera; LABEL_PLACEMENTS], 1.0),
            Self::Follow { .. } => {
                let steps = second / LABEL_STEP_SECONDS;
                let step = steps.floor();
                let at = |back: usize| {
                    let grid = (step - back as f64) * LABEL_STEP_SECONDS;
                    self.at(track, &track.position_at(grid))
                };
                // Filled with the newest, then the older ones are written oldest first.
                let mut cameras = [at(0)?; LABEL_PLACEMENTS];
                for (back, camera) in (1..LABEL_PLACEMENTS).rev().zip(&mut cameras) {
                    *camera = at(back)?;
                }
                (cameras, (steps - step) as f32)
            }
        };
        MapLabelDraw::new(cameras, blend).map_err(WidgetError::Render)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::route::sampling::TrackSample;

    /// A minute riding north at 5 m/s.
    fn track() -> MapTrack {
        let samples = (0..=60)
            .map(|second| TrackSample {
                second: f64::from(second),
                lat: f64::from(second) * 4.5e-5,
                lon: 14.0,
            })
            .collect();
        MapTrack::new(samples, 8.0).unwrap()
    }

    #[test]
    fn a_fixed_camera_has_one_placement_for_the_whole_activity() {
        let track = track();
        let camera = WidgetCamera::new(
            MapCameraMode::Fixed {
                zoom_offset: 0.0,
                pan: [0.0, 0.0],
            },
            &track,
        )
        .unwrap();
        let early = camera.label_draw(&track, 3.0).unwrap();
        assert_eq!(early, camera.label_draw(&track, 41.7).unwrap());
        assert_eq!(early.blend(), 1.0);
        assert_eq!(early.cameras(), [early.cameras()[0]; LABEL_PLACEMENTS]);
    }

    #[test]
    fn a_chase_camera_places_on_the_grid_and_fades_from_the_last_placement() {
        let track = track();
        let follow = WidgetCamera::Follow {
            zoom: 16.0,
            pitch_deg: 60.0,
            orientation: MapOrientation::RiderUp,
        };
        let at = |second| follow.label_draw(&track, second).unwrap();
        let camera_at = |step: f64| {
            follow
                .at(&track, &track.position_at(step * LABEL_STEP_SECONDS))
                .unwrap()
        };

        // Both frames are in the step from the grid time 3 to 4, which fades from the placement at
        // grid time 2 to the one at 3, and names the grid times before them back to 0.
        let (early, late) = (at(0.95), at(1.1));
        assert_eq!(
            early.cameras(),
            [camera_at(0.0), camera_at(1.0), camera_at(2.0), camera_at(3.0)]
        );
        assert_eq!(early.cameras(), late.cameras());
        assert!((early.blend() - 1.0 / 6.0).abs() < 1e-5);
        assert!((late.blend() - 2.0 / 3.0).abs() < 1e-5);
        // The next step starts where this one ends: the placement it faded to is the one it fades from.
        let next = at(1.25).cameras();
        assert_eq!(next[LABEL_PLACEMENTS - 2], early.cameras()[LABEL_PLACEMENTS - 1]);
    }
}
