//! What one activity and window resolve to at a widget's constant zoom and size. Immutable, so an
//! export builds it once and shares it: the renderer recognises a route line by identity.
//!
//! The camera is a pure function of track position: the zoom and framing are constant per widget.
use super::{
    track::{route_line, screen_line, MapTrack, TrackPosition},
    validation::{MapCameraMode, MapOrientation, ValidatedMapWidget},
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::ValidatedScenePresentation,
    render::preview::PreviewRasterScale,
    widgets::route::{geometry::ProgressLine, sampling::RouteWindow},
};
use ovrley_map::{
    frame_tiles, mercator::world_to_lon_lat, MapCamera, MapLabelDraw, MapRouteLine, MapViewport,
    TileId, LABEL_PLACEMENTS, MAX_VIEW_TILES, TILE_SIZE,
};
use std::sync::Arc;

/// The inputs a widget's content is built from, so unrelated edits keep it.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct ContentKey {
    camera: MapCameraMode,
    width: f32,
    height: f32,
    route_smoothing_m: f32,
    pixel_ratio: f32,
    labels: bool,
}

impl ContentKey {
    pub(super) fn new(
        config: &ValidatedMapWidget,
        scene: &ValidatedScenePresentation,
        raster_scale: PreviewRasterScale,
    ) -> Self {
        Self {
            camera: config.camera,
            width: config.width,
            height: config.height,
            route_smoothing_m: config.route_smoothing_m,
            pixel_ratio: scene.scale * raster_scale.value(),
            labels: config.layers.labels,
        }
    }
}

/// What a route line is built for. Reduction applies at the next whole zoom, so a run of zoom edits
/// uploads a new line only when it crosses a level.
#[derive(Clone, Copy, PartialEq)]
struct LineKey {
    zoom: f64,
    route_smoothing_m: f32,
}

pub struct ActivityMap {
    pub(super) track: Arc<MapTrack>,
    pub(super) line: Arc<MapRouteLine>,
    /// The top-down route in the viewport's output pixels, projected once through the fixed camera,
    /// which the widget strokes over a frame that never changes. None for a chase camera, whose
    /// renderer draws the route under the buildings through every frame's camera.
    pub(super) screen_route: Option<Arc<ProgressLine>>,
    line_key: LineKey,
    camera: WidgetCamera,
    pub(super) viewport: MapViewport,
    activity_end: f64,
    /// Whether the map shows labels, so whether its frames carry a label draw.
    labels: bool,
}

impl ActivityMap {
    /// Resolves `key` over a track. Only a line that `key` changes is simplified again.
    fn new(
        track: Arc<MapTrack>,
        key: ContentKey,
        activity_end: f64,
        previous: Option<&Self>,
    ) -> WidgetResult<Self> {
        let camera = WidgetCamera::new(key.camera, &track)?;
        let ratio = key.pixel_ratio;
        let viewport = MapViewport::new(
            (key.width * ratio).round().max(1.0) as u32,
            (key.height * ratio).round().max(1.0) as u32,
            ratio,
        )
        .map_err(WidgetError::Render)?;
        let line_key = LineKey {
            zoom: camera.zoom().ceil(),
            route_smoothing_m: key.route_smoothing_m,
        };
        let line = match previous.filter(|previous| previous.line_key == line_key) {
            Some(previous) => previous.line.clone(),
            None => Arc::new(route_line(&track, line_key.zoom)?),
        };
        let screen_route = match camera {
            WidgetCamera::Fixed(camera) => Some(Arc::new(screen_line(&line, &camera, viewport)?)),
            WidgetCamera::Follow { .. } => None,
        };
        Ok(Self {
            track,
            line,
            screen_route,
            line_key,
            camera,
            viewport,
            activity_end,
            labels: key.labels,
        })
    }

    /// Whether the activity is playing at `second`. Outside it the placeholder plays.
    pub(super) fn covers(&self, second: f64) -> bool {
        second >= 0.0 && second <= self.activity_end
    }

    pub(super) fn camera_at(&self, position: &TrackPosition) -> WidgetResult<MapCamera> {
        self.camera.at(&self.track, position)
    }

    /// How a frame at `second` places and fades its labels. None when the map shows none.
    pub(super) fn label_draw(&self, second: f64) -> WidgetResult<Option<MapLabelDraw>> {
        self.labels
            .then(|| self.camera.label_draw(&self.track, second))
            .transpose()
    }

    pub fn viewport(&self) -> MapViewport {
        self.viewport
    }

    /// The constant zoom every frame draws at.
    pub fn zoom(&self) -> f64 {
        self.camera.zoom()
    }

    /// The whole activity, one second apart. Views change far slower than this needs to catch them.
    pub fn activity_seconds(&self) -> impl Iterator<Item = f64> {
        (0..=self.activity_end.floor() as u64).map(|second| second as f64)
    }

    /// The tiles each of `seconds` sees, with consecutive equal views merged. A view also holds what the
    /// cameras of its labels' placements see. A view at the tile limit may be missing tiles, so it
    /// fails instead of drawing a blank patch.
    pub fn views(&self, seconds: impl Iterator<Item = f64>) -> WidgetResult<Vec<Vec<TileId>>> {
        let mut views: Vec<Vec<TileId>> = Vec::new();
        for second in seconds.filter(|second| self.covers(*second)) {
            let camera = self.camera_at(&self.track.position_at(second))?;
            let frame = frame_tiles(&camera, self.label_draw(second)?.as_ref(), self.viewport);
            if frame.busiest >= MAX_VIEW_TILES {
                return Err(WidgetError::Config(format!(
                    "map: this camera sees at least {MAX_VIEW_TILES} tiles at {second:.1} s. Raise its zoom or lower its pitch."
                )));
            }
            if views.last() != Some(&frame.tiles) {
                views.push(frame.tiles);
            }
        }
        Ok(views)
    }
}

/// A widget's content: a placeholder without GPS, or a real map.
#[derive(Clone)]
pub enum MapContent {
    NoGps,
    Activity(Arc<ActivityMap>),
}

impl MapContent {
    /// What a host other than the editor preview hands its widgets.
    pub fn for_host(
        config: &ValidatedMapWidget,
        scene: &ValidatedScenePresentation,
        host: &WidgetHost,
        activity: Option<&ActivitySampler>,
    ) -> WidgetResult<Self> {
        Self::build(
            ContentKey::new(config, scene, host.raster_scale),
            host.plot_window(config.show_full_activity),
            activity,
        )
    }

    pub(super) fn build(
        key: ContentKey,
        window: Option<RouteWindow>,
        activity: Option<&ActivitySampler>,
    ) -> WidgetResult<Self> {
        let track = activity
            .map(|activity| activity.route.track_samples(window))
            .and_then(|samples| MapTrack::new(samples, f64::from(key.route_smoothing_m)));
        let (Some(track), Some(activity)) = (track, activity) else {
            return Ok(Self::NoGps);
        };
        Ok(Self::Activity(Arc::new(ActivityMap::new(
            Arc::new(track),
            key,
            activity.route.end,
            None,
        )?)))
    }

    /// The same activity under a new key, which keeps the track and the line when it can.
    pub(super) fn rekeyed(&self, key: ContentKey) -> WidgetResult<Self> {
        Ok(match self {
            Self::NoGps => Self::NoGps,
            Self::Activity(map) => {
                let track = if map.line_key.route_smoothing_m == key.route_smoothing_m {
                    map.track.clone()
                } else {
                    Arc::new(map.track.smoothed(f64::from(key.route_smoothing_m)))
                };
                Self::Activity(Arc::new(ActivityMap::new(
                    track,
                    key,
                    map.activity_end,
                    Some(map),
                )?))
            }
        })
    }
}

/// A follow camera sits low in the frame, showing more road ahead, as `NavigationMapController` does.
const FOLLOW_TOP_PADDING: f64 = 0.45;
/// Seconds of activity time between label placements.
const LABEL_STEP_SECONDS: f64 = 0.3;

/// The world-space bounding box of a track: its centre and its longest side, which a top-down camera
/// centres on and measures its pan in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteExtent {
    pub centre: (f64, f64),
    pub longest: f64,
}

impl RouteExtent {
    pub fn of(track: &MapTrack) -> Self {
        let (mut min, mut max) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
        for point in track.world() {
            min = (min.0.min(point.0), min.1.min(point.1));
            max = (max.0.max(point.0), max.1.max(point.1));
        }
        Self {
            centre: ((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0),
            longest: (max.0 - min.0).max(max.1 - min.1),
        }
    }

    /// Widget units one unit of pan spans at `zoom`: the route's longest side on screen.
    pub fn span(&self, zoom: f64) -> f64 {
        self.longest * f64::from(TILE_SIZE) * zoom.exp2()
    }
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
            MapCameraMode::Fixed { zoom, pan } => {
                Self::Fixed(fixed_camera(track, f64::from(zoom), pan.map(f64::from))?)
            }
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
                MapCamera::new(lon, lat, zoom, pitch_deg, bearing, FOLLOW_TOP_PADDING)
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

/// North up, pitch 0, at `zoom`. A positive `pan` moves the route right and down in the frame, as a
/// share of its longest side, so a project's framing carries over to another activity.
fn fixed_camera(track: &MapTrack, zoom: f64, pan: [f64; 2]) -> WidgetResult<MapCamera> {
    let route = RouteExtent::of(track);
    let (lon, lat) = world_to_lon_lat(
        route.centre.0 - pan[0] * route.longest,
        route.centre.1 - pan[1] * route.longest,
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
                zoom: 15.0,
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
            [
                camera_at(0.0),
                camera_at(1.0),
                camera_at(2.0),
                camera_at(3.0)
            ]
        );
        assert_eq!(early.cameras(), late.cameras());
        assert!((early.blend() - 1.0 / 6.0).abs() < 1e-5);
        assert!((late.blend() - 2.0 / 3.0).abs() < 1e-5);
        // The next step starts where this one ends: the placement it faded to is the one it fades from.
        let next = at(1.25).cameras();
        assert_eq!(
            next[LABEL_PLACEMENTS - 2],
            early.cameras()[LABEL_PLACEMENTS - 1]
        );
    }
}
