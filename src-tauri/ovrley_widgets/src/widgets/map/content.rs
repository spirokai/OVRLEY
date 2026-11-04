//! What one activity and window resolve to at a widget's constant zoom and size. Immutable, so an
//! export builds it once and shares it: the renderer recognises a route line by identity.
use super::{
    camera::WidgetCamera,
    route::{route_line, screen_line},
    track::{MapTrack, TrackPosition},
    validation::{MapCameraMode, ValidatedMapWidget},
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::ValidatedScenePresentation,
    render::preview::PreviewRasterScale,
    widgets::route::{geometry::ProgressLine, sampling::RouteWindow},
};
use ovrley_map::{view_tiles, MapCamera, MapRouteLine, MapViewport, TileId, MAX_VIEW_TILES};
use std::sync::Arc;

/// The inputs a widget's content is built from, so unrelated edits keep it.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct ContentKey {
    camera: MapCameraMode,
    width: f32,
    height: f32,
    route_smoothing_m: f32,
    pixel_ratio: f32,
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
        })
    }

    /// Whether the activity is playing at `second`. Outside it the placeholder plays.
    pub(super) fn covers(&self, second: f64) -> bool {
        second >= 0.0 && second <= self.activity_end
    }

    pub(super) fn camera_at(&self, position: &TrackPosition) -> WidgetResult<MapCamera> {
        self.camera.at(&self.track, position)
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

    /// The tiles each of `seconds` sees, with consecutive equal views merged. A view at the tile
    /// limit may be missing tiles, so it fails instead of drawing a blank patch.
    pub fn views(&self, seconds: impl Iterator<Item = f64>) -> WidgetResult<Vec<Vec<TileId>>> {
        let mut views: Vec<Vec<TileId>> = Vec::new();
        for second in seconds.filter(|second| self.covers(*second)) {
            let camera = self.camera_at(&self.track.position_at(second))?;
            let tiles = view_tiles(&camera, self.viewport);
            if tiles.len() >= MAX_VIEW_TILES {
                return Err(WidgetError::Config(format!(
                    "map: this camera sees at least {MAX_VIEW_TILES} tiles at {second:.1} s. Raise its zoom or lower its pitch."
                )));
            }
            if views.last() != Some(&tiles) {
                views.push(tiles);
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
