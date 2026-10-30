//! What one activity and window resolve to at a widget's constant zoom and size. Immutable, so an
//! export builds it once and shares it: the renderer recognises a route line by identity.
use super::{
    camera::WidgetCamera,
    route::route_line,
    track::{MapTrack, TrackPosition},
    validation::{MapCameraMode, ValidatedMapWidget},
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::ValidatedScenePresentation,
    render::preview::PreviewRasterScale,
    widgets::route::sampling::RouteWindow,
};
use ovrley_map::{
    view_tiles, MapCamera, MapRouteLine, MapViewport, TileId, ViewPadding, MAX_VIEW_TILES,
};
use std::sync::Arc;

/// The inputs a widget's content is built from, so unrelated edits keep it.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct ContentKey {
    camera: MapCameraMode,
    width: f32,
    height: f32,
    tolerance: f32,
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
            tolerance: config.simplify_tolerance_px,
            pixel_ratio: scene.scale * raster_scale.value(),
        }
    }
}

pub struct ActivityMap {
    pub(super) track: MapTrack,
    pub(super) line: Arc<MapRouteLine>,
    camera: WidgetCamera,
    pub(super) viewport: MapViewport,
    activity_end: f64,
}

impl ActivityMap {
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

    /// The tiles each of `seconds` sees, with consecutive equal views merged. A view at the tile
    /// limit may be missing tiles, so it fails instead of drawing a blank patch.
    pub fn views(&self, seconds: impl Iterator<Item = f64>) -> WidgetResult<Vec<Vec<TileId>>> {
        let mut views: Vec<Vec<TileId>> = Vec::new();
        for second in seconds.filter(|second| self.covers(*second)) {
            let camera = self.camera_at(&self.track.position_at(second))?;
            let tiles = view_tiles(&camera, self.viewport, ViewPadding::Tight);
            if tiles.len() >= MAX_VIEW_TILES {
                return Err(WidgetError::Config(format!(
                    "map: this camera sees at least {MAX_VIEW_TILES} tiles at {second:.1} s. Reduce its range or pitch."
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
            .and_then(MapTrack::new);
        let (Some(track), Some(activity)) = (track, activity) else {
            return Ok(Self::NoGps);
        };
        let camera = WidgetCamera::new(key.camera, &track, key.width, key.height)?;
        let ratio = key.pixel_ratio;
        let viewport = MapViewport::new(
            (key.width * ratio).round().max(1.0) as u32,
            (key.height * ratio).round().max(1.0) as u32,
            ratio,
        )
        .map_err(WidgetError::Render)?;
        let line = route_line(&track, camera.zoom(), key.tolerance)?;
        Ok(Self::Activity(Arc::new(ActivityMap {
            line: Arc::new(line),
            track,
            camera,
            viewport,
            activity_end: activity.route.end,
        })))
    }
}
