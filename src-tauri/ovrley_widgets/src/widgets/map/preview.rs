//! A map handle retains the track and the last frame across config edits and live resize drafts.
use super::{
    config::MapConfig,
    content::{ContentKey, MapContent},
    frame_provider::{MapFrame, MapFrameProvider, MapFrameRequest},
    layout::MapLayout,
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
use ovrley_map::{MapCamera, MapRouteDraw, MapRouteLine, ViewState};
use serde_json::json;
use skia_safe::{Canvas, Image, Point, Rect, Surface};
use std::sync::Arc;

/// What is behind the marker and attribution.
enum MapBacking {
    Image(Image),
    /// The host's map canvas shows through, so the widget leaves the frame clear.
    Layer,
    Placeholder,
}

/// What the frame at one second depends on: a camera when GPS exists, and the marker's progress.
/// With a camera that's travelled metres; without one, the placeholder's share of its route.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapFrameState {
    pub camera: Option<MapCamera>,
    pub progress: f32,
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
    /// Whether the host's map canvas is showing under this widget.
    layer_ready: bool,
    /// JSON of what the map canvas should draw. Empty when it should draw nothing.
    pub view: Vec<u8>,
    /// The camera of `view`. A live resize keeps it, so the marker stays where the canvas put the road.
    view_camera: Option<MapCamera>,
    view_line: Option<Arc<MapRouteLine>>,
    /// `world_x, world_y, along_metres` per vertex of `view_line`. `route_id` changes with it.
    pub route_points: Vec<f64>,
    route_id: u32,
    geometry_stale: bool,
    surface: Option<Surface>,
    pub pixels: Vec<u8>,
    pub geometry: Vec<u8>,
    /// Scene bounds of the last rendered tile; absent until the first render.
    pub bounds: Option<TileBounds>,
    pub revision: u32,
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
        Self::with_content(
            config,
            validate_scene_presentation(scene)?,
            &host,
            frames,
            None,
            fonts,
        )
    }

    /// `content` is what the host resolved for this widget, from [`MapContent::for_host`].
    pub fn from_validated(
        config: ValidatedMapWidget,
        scene: ValidatedScenePresentation,
        host: &WidgetHost,
        frames: Arc<dyn MapFrameProvider>,
        content: MapContent,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        Self::with_content(config, scene, host, frames, Some(content), fonts)
    }

    fn with_content(
        config: ValidatedMapWidget,
        scene: ValidatedScenePresentation,
        host: &WidgetHost,
        frames: Arc<dyn MapFrameProvider>,
        content: Option<MapContent>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let raster_scale = host.raster_scale;
        Ok(Self {
            layout: MapLayout::new(&config, &scene, raster_scale, fonts)?,
            content_key: ContentKey::new(&config, &scene, raster_scale),
            window: host.plot_window(config.show_full_activity),
            preview_duration: host.placeholder_duration()?,
            raster_scale,
            config,
            scene,
            frames,
            content,
            transient: false,
            last_frame: None,
            last_render: None,
            layer_ready: false,
            view: Vec::new(),
            view_camera: None,
            view_line: None,
            route_points: Vec::new(),
            route_id: 0,
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
            let key = ContentKey::new(&config, &scene, raster_scale);
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
        self.content = Some(MapContent::build(self.content_key, self.window, activity)?);
        Ok(())
    }

    /// A pure function of `second`. Outside the activity the placeholder plays, as on the route.
    pub fn frame_state(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<MapFrameState> {
        self.prepare(activity)?;
        match self.content.as_ref().expect("map content prepared") {
            MapContent::Activity(map) if activity.is_some() && map.covers(second) => {
                let position = map.track.position_at(second);
                Ok(MapFrameState {
                    camera: Some(map.camera_at(&position)?),
                    progress: position.distance_meters as f32,
                })
            }
            _ => Ok(MapFrameState {
                camera: None,
                progress: (second / self.preview_duration.max(1e-9)).clamp(0.0, 1.0) as f32,
            }),
        }
    }

    /// Tells the widget whether the host's map canvas is showing. Until it is, the placeholder shows.
    pub fn set_layer_ready(&mut self, ready: bool) {
        if self.layer_ready != ready {
            self.layer_ready = ready;
            self.last_render = None;
        }
    }

    /// Publishes what the host's map canvas should draw at this state. During a live resize the
    /// canvas keeps its last frame, rescaled, and nothing is asked of it.
    fn publish_view(&mut self, state: &MapFrameState) -> WidgetResult<()> {
        if self.transient {
            return Ok(());
        }
        let (Some(camera), Some(MapContent::Activity(map))) = (state.camera, &self.content) else {
            self.view.clear();
            self.view_camera = None;
            return Ok(());
        };
        if !self
            .view_line
            .as_ref()
            .is_some_and(|line| Arc::ptr_eq(line, &map.line))
        {
            self.route_points = map
                .line
                .points()
                .iter()
                .flat_map(|point| [point.world_x, point.world_y, f64::from(point.along_metres)])
                .collect();
            self.route_id = self.route_id.wrapping_add(1);
            self.view_line = Some(map.line.clone());
        }
        let layers = self.config.spec.layers;
        let viewport = map.viewport;
        let style = self.layout.route_style;
        let [c0, c1, c2, c3] = style.completed_color;
        let [r0, r1, r2, r3] = style.remaining_color;
        self.view = serde_json::to_vec(&json!({
            "renderer": {
                "style": self.config.spec.style.name(),
                "roads": layers.roads,
                "paths": layers.paths,
                "water": layers.water,
                "buildings": layers.buildings,
                "width": viewport.width_px(),
                "height": viewport.height_px(),
                "pixel_ratio": viewport.pixel_ratio(),
                "zoom": camera.zoom(),
            },
            "camera": camera.values(),
            "route_id": self.route_id,
            "route_style": [
                c0, c1, c2, c3, style.completed_width_px,
                r0, r1, r2, r3, style.remaining_width_px,
            ],
            "travelled_metres": state.progress,
        }))
        .map_err(|error| WidgetError::Render(error.to_string()))?;
        self.view_camera = Some(camera);
        Ok(())
    }

    /// The camera and backing to draw. A map layer's camera is the one it last drew.
    fn resolve_frame(
        &mut self,
        state: &MapFrameState,
    ) -> WidgetResult<Option<(MapCamera, MapBacking)>> {
        let Some(camera) = state.camera else {
            return Ok(None);
        };
        let Some(MapContent::Activity(map)) = &self.content else {
            return Ok(None);
        };
        if self.layer_ready {
            return Ok(Some((
                self.view_camera.unwrap_or(camera),
                MapBacking::Layer,
            )));
        }
        if self.transient {
            return Ok(Some((camera, MapBacking::Placeholder)));
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
                return Ok(Some((camera, MapBacking::Image(image.clone()))));
            }
        }
        match self.frames.frame(&request)? {
            MapFrame::Ready(image) => {
                self.last_frame = Some((request, image.clone()));
                Ok(Some((camera, MapBacking::Image(image))))
            }
            MapFrame::Pending => Ok(Some((camera, MapBacking::Placeholder))),
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
        resolved: Option<&(MapCamera, MapBacking)>,
    ) {
        canvas.save();
        canvas.clip_rect(layout.frame, None, true);
        match resolved {
            Some((_, MapBacking::Image(image))) => layout.draw_image(canvas, image),
            Some((_, MapBacking::Layer)) => {}
            Some((_, MapBacking::Placeholder)) => layout.draw_placeholder(canvas),
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
        self.publish_view(&state)?;
        let resolved = self.resolve_frame(&state)?;
        let has_frame = resolved
            .as_ref()
            .is_some_and(|(_, backing)| !matches!(backing, MapBacking::Placeholder));
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
