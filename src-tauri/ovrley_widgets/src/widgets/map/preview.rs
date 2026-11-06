//! A map handle retains the track and the last frame across config edits. Every edit draws at its own
//! size and camera straight away, so a resize shows more or less of the map and never stretches it.
use super::{
    arrow::MarkerPose,
    config::MapConfig,
    content::{ContentKey, MapContent},
    frame_provider::{MapFrame, MapFramePixels, MapFrameProvider, MapFrameRequest},
    layout::{MapLayout, ScreenRoute},
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
use ovrley_map::{
    MapCamera, MapLabelDraw, MapRouteDraw, MapRouteLine, MapStyleCatalog, MapStyleInfo,
};
use serde_json::json;
use skia_safe::{Canvas, Rect, Surface};
use std::sync::{mpsc::Receiver, Arc};

/// What is behind the marker and attribution.
enum MapBacking {
    Image(MapFramePixels),
    /// The host's map canvas shows through, so the widget leaves the frame clear.
    Layer,
    Placeholder,
}

/// What the frame at one second depends on: a camera when GPS exists, how its labels are placed and
/// faded when the map shows them, and the marker's progress. With a camera that's travelled metres;
/// without one, the placeholder's share of its route.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapFrameState {
    pub camera: Option<MapCamera>,
    pub labels: Option<MapLabelDraw>,
    pub progress: f32,
}

/// The installed style `config`'s resolves to. `styles` are the host's installed styles, which the
/// editor preview sets once it has them: until then it creates no map.
fn resolve_style(
    styles: Option<&MapStyleCatalog>,
    config: &ValidatedMapWidget,
) -> WidgetResult<MapStyleInfo> {
    let styles = styles.ok_or_else(|| {
        WidgetError::Config(
            "a map needs the installed map styles, which this host has not set".into(),
        )
    })?;
    Ok(styles.resolve(&config.style).clone())
}

pub struct PreviewMap {
    config: ValidatedMapWidget,
    /// The installed style the config's resolved to.
    style: MapStyleInfo,
    scene: ValidatedScenePresentation,
    window: Option<RouteWindow>,
    preview_duration: f64,
    raster_scale: PreviewRasterScale,
    frames: Arc<dyn MapFrameProvider>,
    layout: MapLayout,
    content: Option<MapContent>,
    /// The inputs of `content`.
    content_key: ContentKey,
    /// The last frame by request, so an unchanged request asks the provider once.
    last_frame: Option<(MapFrameRequest, MapFramePixels)>,
    /// The frame the provider is drawing.
    drawing: Option<(MapFrameRequest, Receiver<Result<MapFramePixels, String>>)>,
    /// The top-down route's stroke under this layout, kept while the map's line stays the same.
    /// [`Self::frame_request`] brings it up to date, so it is current for any frame with a request.
    screen_route: Option<ScreenRoute>,
    last_render: Option<(MapFrameState, bool)>,
    /// Whether the host's map canvas is showing under this widget.
    layer_ready: bool,
    /// JSON of what the map canvas should draw. Empty when it should draw nothing.
    pub view: Vec<u8>,
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
        styles: Option<&MapStyleCatalog>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let config = validate_map_widget(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene)?;
        Self::with_content(
            resolve_style(styles, &config)?,
            config,
            validate_scene_presentation(scene)?,
            &host,
            frames,
            None,
            fonts,
        )
    }

    /// `style` is the installed style `config`'s resolved to, and `content` what the host resolved
    /// for this widget, from [`MapContent::for_host`].
    pub fn from_validated(
        config: ValidatedMapWidget,
        style: MapStyleInfo,
        scene: ValidatedScenePresentation,
        host: &WidgetHost,
        frames: Arc<dyn MapFrameProvider>,
        content: MapContent,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        Self::with_content(style, config, scene, host, frames, Some(content), fonts)
    }

    fn with_content(
        style: MapStyleInfo,
        config: ValidatedMapWidget,
        scene: ValidatedScenePresentation,
        host: &WidgetHost,
        frames: Arc<dyn MapFrameProvider>,
        content: Option<MapContent>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let raster_scale = host.raster_scale;
        Ok(Self {
            layout: MapLayout::new(&config, &style, &scene, raster_scale, fonts)?,
            content_key: ContentKey::new(&config, &scene, raster_scale),
            style,
            window: host.plot_window(config.show_full_activity),
            preview_duration: host.placeholder_duration()?,
            raster_scale,
            config,
            scene,
            frames,
            content,
            last_frame: None,
            drawing: None,
            screen_route: None,
            last_render: None,
            layer_ready: false,
            view: Vec::new(),
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
        styles: Option<&MapStyleCatalog>,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let config = validate_map_widget(&widget, 0)?;
        let style = resolve_style(styles, &config)?;
        let host = WidgetHost::from_scene(&scene)?;
        let window = host.plot_window(config.show_full_activity);
        let preview_duration = host.placeholder_duration()?;
        let raster_scale = host.raster_scale;
        let scene = validate_scene_presentation(scene)?;
        if window != self.window {
            self.invalidate_activity();
        }
        let key = ContentKey::new(&config, &scene, raster_scale);
        if key != self.content_key {
            self.content = self
                .content
                .take()
                .map(|content| content.rekeyed(key))
                .transpose()?;
            self.content_key = key;
        }
        self.layout = MapLayout::new(&config, &style, &scene, raster_scale, fonts)?;
        // Its stroke is the old layout's.
        self.screen_route = None;
        self.config = config;
        self.style = style;
        self.scene = scene;
        self.window = window;
        self.preview_duration = preview_duration;
        self.raster_scale = raster_scale;
        self.last_render = None;
        self.geometry_stale = true;
        Ok(())
    }

    pub fn invalidate_activity(&mut self) {
        self.content = None;
        self.last_frame = None;
        self.drawing = None;
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
                    labels: map.label_draw(second)?,
                    progress: position.distance_meters as f32,
                })
            }
            _ => Ok(MapFrameState {
                camera: None,
                labels: None,
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

    /// Publishes what the host's map canvas should draw for `request`. A top-down request has no
    /// route, since the tile strokes it, so the canvas draws only the map. Its view then holds still
    /// through playback, and the canvas draws again only for an edit or a tile.
    fn publish_view(&mut self, request: Option<&MapFrameRequest>) -> WidgetResult<()> {
        let Some(request) = request else {
            self.view.clear();
            return Ok(());
        };
        let route = match &request.route {
            None => serde_json::Value::Null,
            Some(route) => {
                if !self
                    .view_line
                    .as_ref()
                    .is_some_and(|line| Arc::ptr_eq(line, &route.line))
                {
                    self.route_points = route
                        .line
                        .points()
                        .iter()
                        .flat_map(|point| {
                            [point.world_x, point.world_y, f64::from(point.along_metres)]
                        })
                        .collect();
                    self.route_id = self.route_id.wrapping_add(1);
                    self.view_line = Some(route.line.clone());
                }
                let style = route.style;
                let [c0, c1, c2, c3] = style.completed_color;
                let [r0, r1, r2, r3] = style.remaining_color;
                json!({
                    "id": self.route_id,
                    "style": [c0, c1, c2, c3, r0, r1, r2, r3, style.width_px],
                    "travelled_metres": route.travelled_metres,
                })
            }
        };
        let (camera, viewport) = (request.camera, request.viewport);
        self.view = serde_json::to_vec(&json!({
            "renderer": {
                "style": self.style.id,
                "style_buildings": self.style.buildings,
                "buildings": self.config.layers.buildings,
                "paths": self.config.layers.paths,
                "pixel_ratio": viewport.pixel_ratio(),
                "msaa_samples": self.config.msaa_samples,
            },
            "width": viewport.width_px(),
            "height": viewport.height_px(),
            "style_zoom": camera.zoom(),
            "opacity": self.layout.opacity,
            "corner_radius_px": request.corner_radius_px,
            "camera": camera.values(),
            "route": route,
        }))
        .map_err(|error| WidgetError::Render(error.to_string()))?;
        Ok(())
    }

    /// What the renderer must draw at this state; none without a camera over the activity. This is
    /// where the route's drawer is chosen: a top-down route is the widget's, whose stroke this keeps
    /// in `screen_route`, so its request draws none. Any other goes to the renderer.
    fn frame_request(&mut self, state: &MapFrameState) -> Option<MapFrameRequest> {
        let camera = state.camera?;
        let Some(MapContent::Activity(map)) = &self.content else {
            return None;
        };
        let route = match &map.screen_route {
            Some(line) => {
                if !self
                    .screen_route
                    .as_ref()
                    .is_some_and(|route| Arc::ptr_eq(&route.line, line))
                {
                    self.screen_route = Some(self.layout.screen_route(line.clone(), map.viewport));
                }
                None
            }
            None => {
                self.screen_route = None;
                Some(MapRouteDraw {
                    line: map.line.clone(),
                    style: self.layout.route_style,
                    travelled_metres: state.progress,
                })
            }
        };
        Some(MapFrameRequest {
            spec: self.config.spec(&self.style),
            camera,
            viewport: map.viewport,
            route,
            labels: state.labels,
            corner_radius_px: self.layout.corner_radius_px,
        })
    }

    /// Asks for a frame neither held nor being drawn, handing it the last frame's buffer.
    fn start_request(&mut self, request: &MapFrameRequest) -> WidgetResult<()> {
        let held = |frame: Option<&MapFrameRequest>| frame == Some(request);
        if held(self.last_frame.as_ref().map(|(frame, _)| frame))
            || held(self.drawing.as_ref().map(|(frame, _)| frame))
        {
            return Ok(());
        }
        // Performance: the previous frame's buffer is reused, so a frame allocates nothing.
        let spare = self
            .last_frame
            .take()
            .and_then(|(_, frame)| frame.into_pixels());
        self.drawing = match self.frames.frame(request, spare)? {
            MapFrame::Drawing(answer) => Some((request.clone(), answer)),
            MapFrame::Pending => None,
        };
        Ok(())
    }

    /// Starts the frame `draw` will show at `second`, so a host with other work to do first does not
    /// wait for it. `draw` collects it.
    pub fn start_frame(
        &mut self,
        activity: Option<&ActivitySampler>,
        second: f64,
    ) -> WidgetResult<()> {
        let state = self.frame_state(activity, second)?;
        match self.frame_request(&state) {
            Some(request) if !self.layer_ready => self.start_request(&request),
            _ => Ok(()),
        }
    }

    /// The camera and backing to draw `request` with.
    fn resolve_frame(
        &mut self,
        request: Option<MapFrameRequest>,
    ) -> WidgetResult<Option<(MapCamera, MapBacking)>> {
        let Some(request) = request else {
            return Ok(None);
        };
        let camera = request.camera;
        if self.layer_ready {
            return Ok(Some((camera, MapBacking::Layer)));
        }
        self.start_request(&request)?;
        if let Some((_, answer)) = self.drawing.take() {
            let frame = answer
                .recv()
                .map_err(|_| WidgetError::Render("The map renderer stopped mid-frame".into()))?
                .map_err(WidgetError::Render)?;
            self.last_frame = Some((request, frame));
        }
        Ok(Some((
            camera,
            match &self.last_frame {
                Some((_, frame)) => MapBacking::Image(frame.clone()),
                None => MapBacking::Placeholder,
            },
        )))
    }

    /// How the frame sees the marker. Follow cameras keep it on their centre; a fixed one moves it.
    fn marker_pose(&self, camera: &MapCamera, second: f64) -> Option<MarkerPose> {
        let Some(MapContent::Activity(map)) = &self.content else {
            return None;
        };
        let position = map.track.position_at(second);
        MarkerPose::on_camera(
            camera,
            map.viewport,
            self.layout.frame,
            position.world,
            map.track.bearing_at(&position),
        )
    }

    /// `screen_route` is the top-down route, drawn over whatever backs a resolved frame and under the
    /// marker. It is current only for a resolved frame, so a frame without one never draws it.
    fn paint(
        layout: &MapLayout,
        canvas: &Canvas,
        progress: f32,
        marker: Option<MarkerPose>,
        resolved: Option<&(MapCamera, MapBacking)>,
        screen_route: Option<&ScreenRoute>,
    ) {
        canvas.save();
        layout.clip_shape(canvas);
        match resolved {
            Some((_, backing)) => {
                match backing {
                    MapBacking::Image(frame) => layout.draw_image(canvas, frame.image()),
                    MapBacking::Layer => {}
                    MapBacking::Placeholder => layout.draw_placeholder(canvas),
                }
                if let Some(route) = screen_route {
                    layout.draw_route(canvas, route, progress);
                }
            }
            None => {
                layout.draw_placeholder(canvas);
                let (split, heading) = layout.draw_placeholder_route(canvas, progress);
                layout.draw_marker(canvas, &MarkerPose::top_down(split, heading));
            }
        }
        if let Some(pose) = &marker {
            layout.draw_marker(canvas, pose);
        }
        layout.draw_attribution(canvas);
        layout.draw_border(canvas);
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
        let request = self.frame_request(&state);
        let resolved = self.resolve_frame(request)?;
        let marker = resolved
            .as_ref()
            .and_then(|(camera, _)| self.marker_pose(camera, second));
        Self::paint(
            &self.layout,
            canvas,
            state.progress,
            marker,
            resolved.as_ref(),
            self.screen_route.as_ref(),
        );
        Ok(self.layout_bounds())
    }

    pub fn render(&mut self, activity: Option<&ActivitySampler>, second: f64) -> WidgetResult<()> {
        let state = self.frame_state(activity, second)?;
        let request = self.frame_request(&state);
        self.publish_view(request.as_ref())?;
        let resolved = self.resolve_frame(request)?;
        let has_frame = resolved
            .as_ref()
            .is_some_and(|(_, backing)| !matches!(backing, MapBacking::Placeholder));
        // A pending frame may have arrived, so this compares more than the sampled state.
        if self.last_render == Some((state, has_frame)) {
            return Ok(());
        }
        let marker = resolved
            .as_ref()
            .and_then(|(camera, _)| self.marker_pose(camera, second));
        let layout = &self.layout;
        let screen_route = self.screen_route.as_ref();
        let bounds: Rect = outward(layout.frame);
        let (width, height, bounds) = render_tile(
            &mut self.surface,
            &mut self.pixels,
            SHAPE_COLOR_TYPE,
            self.raster_scale,
            bounds,
            |canvas| {
                Self::paint(
                    layout,
                    canvas,
                    state.progress,
                    marker,
                    resolved.as_ref(),
                    screen_route,
                );
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
