//! A map handle retains the track and the last frame across config edits and live resize drafts.
use super::{
    camera::WidgetCamera,
    config::MapConfig,
    frame_provider::{MapFrame, MapFrameProvider, MapFrameRequest},
    layout::MapLayout,
    route::route_line,
    track::MapTrack,
    validation::{validate_map_widget, ValidatedMapWidget},
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::{
        validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
    },
    render::{
        preview::{render_tile, PreviewRasterScale},
        text::FontResolver,
    },
    widgets::{
        route::{layout::outward, sampling::RouteWindow},
        value::preview::{ContentBounds, TileBounds, WidgetGeometry, SHAPE_COLOR_TYPE},
    },
};
use ovrley_map::{MapCamera, MapRouteDraw, MapRouteLine, MapViewport, ViewState};
use skia_safe::{Canvas, Image, Point, Rect, Surface};
use std::sync::Arc;

/// What the frame at one second depends on: a camera when GPS exists, and the marker's progress.
/// With a camera that's travelled metres; without one, the placeholder's share of its route.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapFrameState {
    pub camera: Option<MapCamera>,
    pub progress: f32,
}

/// What one activity and window resolve to at the widget's constant zoom and size.
struct ActivityMap {
    track: MapTrack,
    line: Arc<MapRouteLine>,
    camera: WidgetCamera,
    viewport: MapViewport,
}

enum MapContent {
    NoGps,
    Activity(Box<ActivityMap>),
}

/// The inputs `MapContent` was built from, so unrelated edits keep it.
#[derive(Clone, Copy, PartialEq)]
struct ContentKey {
    camera: super::validation::MapCameraMode,
    width: f32,
    height: f32,
    tolerance: f32,
    pixel_ratio: f32,
}

pub struct PreviewMap {
    config: ValidatedMapWidget,
    scene: ValidatedScenePresentation,
    window: Option<RouteWindow>,
    preview_duration: f64,
    raster_scale: PreviewRasterScale,
    frames: Arc<dyn MapFrameProvider>,
    layout: MapLayout,
    content: Option<MapContent>,
    /// The inputs of `content`, committed sizes only: a live resize rescales and asks for nothing.
    content_key: ContentKey,
    transient: bool,
    /// The last frame by request, so an unchanged request asks the provider once.
    last_frame: Option<(MapFrameRequest, Image)>,
    last_render: Option<(MapFrameState, bool)>,
    geometry_stale: bool,
    surface: Option<Surface>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
}

fn content_key(
    config: &ValidatedMapWidget,
    scene: &ValidatedScenePresentation,
    raster_scale: PreviewRasterScale,
) -> ContentKey {
    ContentKey {
        camera: config.camera,
        width: config.width,
        height: config.height,
        tolerance: config.simplify_tolerance_px,
        pixel_ratio: scene.scale * raster_scale.value(),
    }
}

impl PreviewMap {
    pub fn new(
        widget: MapConfig,
        scene: ScenePresentationConfig,
        frames: Arc<dyn MapFrameProvider>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let config = validate_map_widget(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        Self::from_validated(
            config,
            validate_scene_presentation(scene)?,
            &host,
            frames,
            fonts,
        )
    }

    pub fn from_validated(
        config: ValidatedMapWidget,
        scene: ValidatedScenePresentation,
        host: &WidgetHost,
        frames: Arc<dyn MapFrameProvider>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let raster_scale = host.raster_scale;
        Ok(Self {
            layout: MapLayout::new(&config, &scene, raster_scale, fonts)?,
            content_key: content_key(&config, &scene, raster_scale),
            window: host.plot_window(config.show_full_activity),
            preview_duration: host.placeholder_duration()?,
            raster_scale,
            config,
            scene,
            frames,
            content: None,
            transient: false,
            last_frame: None,
            last_render: None,
            geometry_stale: true,
            surface: None,
            pixels: Vec::new(),
            geometry: Vec::new(),
            bounds: None,
            revision: 0,
        })
    }

    pub fn update(
        &mut self,
        widget: MapConfig,
        scene: ScenePresentationConfig,
        transient: bool,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let config = validate_map_widget(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        let window = host.plot_window(config.show_full_activity);
        let preview_duration = host.placeholder_duration()?;
        let raster_scale = host.raster_scale;
        let scene = validate_scene_presentation(scene)?;
        if window != self.window {
            self.invalidate_activity();
        }
        // A live resize keeps the committed zoom and viewport; its commit recomputes them.
        if !transient {
            let key = content_key(&config, &scene, raster_scale);
            if key != self.content_key {
                self.content = None;
                self.content_key = key;
            }
        }
        self.layout = MapLayout::new(&config, &scene, raster_scale, fonts)?;
        self.config = config;
        self.scene = scene;
        self.window = window;
        self.preview_duration = preview_duration;
        self.raster_scale = raster_scale;
        self.transient = transient;
        self.last_render = None;
        self.geometry_stale = true;
        Ok(())
    }

    pub fn invalidate_activity(&mut self) {
        self.content = None;
        self.last_frame = None;
        self.last_render = None;
    }

    fn prepare(&mut self, activity: Option<&ActivitySampler>) -> WidgetResult<()> {
        if self.content.is_some() {
            return Ok(());
        }
        let track = activity
            .map(|activity| activity.route.track_samples(self.window))
            .and_then(MapTrack::new);
        self.content = Some(match track {
            None => MapContent::NoGps,
            Some(track) => {
                let camera = WidgetCamera::new(
                    self.config.camera,
                    &track,
                    self.content_key.width,
                    self.content_key.height,
                )?;
                let ratio = self.content_key.pixel_ratio;
                let viewport = MapViewport::new(
                    (self.content_key.width * ratio).round().max(1.0) as u32,
                    (self.content_key.height * ratio).round().max(1.0) as u32,
                    ratio,
                )
                .map_err(WidgetError::Render)?;
                let line = route_line(&track, camera.zoom(), self.content_key.tolerance)?;
                MapContent::Activity(Box::new(ActivityMap {
                    line: Arc::new(line),
                    track,
                    camera,
                    viewport,
                }))
            }
        });
        Ok(())
    }

    /// A pure function of `second`. Outside the activity the placeholder plays, as on the route.
    pub fn frame_state(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<MapFrameState> {
        self.prepare(activity)?;
        let in_activity =
            activity.is_some_and(|activity| second >= 0.0 && second <= activity.route.end);
        match self.content.as_ref().expect("map content prepared") {
            MapContent::Activity(map) if in_activity => {
                let position = map.track.position_at(second);
                Ok(MapFrameState {
                    camera: Some(map.camera.at(&map.track, &position)?),
                    progress: position.distance_meters as f32,
                })
            }
            _ => Ok(MapFrameState {
                camera: None,
                progress: (second / self.preview_duration.max(1e-9)).clamp(0.0, 1.0) as f32,
            }),
        }
    }

    /// The camera and image to draw. During a live resize that's the last frame and its own camera,
    /// so the marker stays where the frame put the road, and nothing is requested.
    fn resolve_frame(
        &mut self,
        state: &MapFrameState,
    ) -> WidgetResult<Option<(MapCamera, Option<Image>)>> {
        let Some(camera) = state.camera else {
            return Ok(None);
        };
        let Some(MapContent::Activity(map)) = &self.content else {
            return Ok(None);
        };
        if self.transient {
            return Ok(Some(self.last_frame.as_ref().map_or(
                (camera, None),
                |(request, image)| (request.camera, Some(image.clone())),
            )));
        }
        let request = MapFrameRequest {
            spec: self.config.spec,
            camera,
            viewport: map.viewport,
            route: MapRouteDraw {
                line: map.line.clone(),
                style: self.layout.route_style,
                travelled_metres: state.progress,
            },
        };
        if let Some((cached, image)) = &self.last_frame {
            if *cached == request {
                return Ok(Some((camera, Some(image.clone()))));
            }
        }
        match self.frames.frame(&request)? {
            MapFrame::Ready(image) => {
                self.last_frame = Some((request, image.clone()));
                Ok(Some((camera, Some(image))))
            }
            MapFrame::Pending => Ok(Some((camera, None))),
        }
    }

    /// Scene pixel under the marker. Follow cameras keep it on their centre; a fixed one moves it.
    fn marker_point(&self, camera: &MapCamera, second: f64) -> Option<Point> {
        let Some(MapContent::Activity(map)) = &self.content else {
            return None;
        };
        let position = map.track.position_at(second);
        let (world_x, world_y) = ovrley_map::mercator::lon_lat_to_world(position.lon, position.lat);
        let clip = ViewState::new(camera, map.viewport)
            .ground_projection()
            .clip(world_x, world_y);
        let frame = self.layout.frame;
        (clip.z >= 0.0).then(|| {
            Point::new(
                frame.left + ((clip.x / clip.w * 0.5 + 0.5) as f32) * frame.width(),
                frame.top + ((0.5 - clip.y / clip.w * 0.5) as f32) * frame.height(),
            )
        })
    }

    fn paint(
        layout: &MapLayout,
        canvas: &Canvas,
        progress: f32,
        marker: Option<Point>,
        resolved: Option<&(MapCamera, Option<Image>)>,
    ) {
        canvas.save();
        canvas.clip_rect(layout.frame, None, true);
        match resolved {
            Some((_, Some(image))) => layout.draw_image(canvas, image),
            Some((_, None)) => layout.draw_placeholder(canvas),
            None => {
                layout.draw_placeholder(canvas);
                let split = layout.draw_placeholder_route(canvas, progress);
                layout.draw_marker(canvas, split);
            }
        }
        if let Some(marker) = marker {
            layout.draw_marker(canvas, marker);
        }
        layout.draw_attribution(canvas);
        canvas.restore();
    }

    pub fn layout_bounds(&self) -> TileBounds {
        outward(self.layout.frame).into()
    }

    /// Draws the frame at `second` onto a host canvas in scene coordinates.
    pub fn draw(
        &mut self,
        canvas: &Canvas,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<TileBounds> {
        let state = self.frame_state(activity, second)?;
        let resolved = self.resolve_frame(&state)?;
        let marker = resolved
            .as_ref()
            .and_then(|(camera, _)| self.marker_point(camera, second));
        Self::paint(
            &self.layout,
            canvas,
            state.progress,
            marker,
            resolved.as_ref(),
        );
        Ok(self.layout_bounds())
    }

    pub fn render(&mut self, activity: Option<&ActivitySampler>, second: f64) -> WidgetResult<()> {
        let state = self.frame_state(activity, second)?;
        let resolved = self.resolve_frame(&state)?;
        let has_frame = resolved
            .as_ref()
            .is_some_and(|(_, image)| image.is_some());
        // A pending frame may have arrived, so this compares more than the sampled state.
        if self.last_render == Some((state, has_frame)) {
            return Ok(());
        }
        let marker = resolved
            .as_ref()
            .and_then(|(camera, _)| self.marker_point(camera, second));
        let layout = &self.layout;
        let bounds: Rect = outward(layout.frame);
        let (width, height, bounds) = render_tile(
            &mut self.surface,
            &mut self.pixels,
            SHAPE_COLOR_TYPE,
            self.raster_scale,
            bounds,
            |canvas| {
                Self::paint(layout, canvas, state.progress, marker, resolved.as_ref());
                Ok(())
            },
        )?;
        if self.geometry_stale {
            let tile = TileBounds::from(bounds);
            let content = ContentBounds::from(layout.frame);
            self.geometry = serde_json::to_vec(&WidgetGeometry::new(
                tile,
                (width, height),
                self.raster_scale.value(),
                content.width,
                content,
            ))
            .map_err(|error| WidgetError::Render(error.to_string()))?;
            self.bounds = Some(tile);
            self.geometry_stale = false;
        }
        self.revision = self.revision.wrapping_add(1);
        self.last_render = Some((state, has_frame));
        Ok(())
    }
}
