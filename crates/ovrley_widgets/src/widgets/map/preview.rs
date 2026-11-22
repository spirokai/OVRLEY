//! A map handle retains the track and the last frame across config edits. Every edit draws at its own
//! size and camera straight away, so a resize shows more or less of the map and never stretches it.
//!
//! Export's frames reach the widget through a [`MapFrameProvider`] its host implements:
//! `ovrley_widgets` never depends on the renderer. The editor preview streams frames from a
//! [`MapFrameFactory`]: it never waits for one, and keeps showing the last frame until the next
//! arrives.
use super::{
    config::MapConfig,
    content::{ContentKey, MapContent, RouteExtent},
    layout::{MapLayout, MarkerPose, ScreenRoute},
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
    frame_tiles, MapCamera, MapLabelDraw, MapRouteDraw, MapRouteLine, MapStyleCatalog,
    MapStyleInfo, MapStyleSpec, MapViewport, TileId,
};
use serde_json::json;
use skia_safe::{images, AlphaType, Canvas, ColorType, Data, Image, ImageInfo, Rect, Surface};
use std::sync::{
    mpsc::{Receiver, TryRecvError},
    Arc,
};

/// Everything a frame's pixels depend on.
#[derive(Clone, Debug, PartialEq)]
pub struct MapFrameRequest {
    pub spec: MapStyleSpec,
    pub camera: MapCamera,
    pub viewport: MapViewport,
    /// What the renderer draws under the buildings. None for a top-down camera: its widget strokes the
    /// route over the frame, so the request, and the frame, stay the same all activity long.
    pub route: Option<MapRouteDraw>,
    /// How the renderer places and fades labels: None when the map shows none. A top-down camera sends
    /// one placement, so its request stays the same all activity long.
    pub labels: Option<MapLabelDraw>,
    /// The renderer crops the frame to this, in output pixels. A circle's is half its width.
    pub corner_radius_px: f32,
}

impl MapFrameRequest {
    /// The tiles a renderer must hold to draw this frame: those its camera sees, and with labels those
    /// each placement camera sees, since a label is placed from the tiles around its own camera.
    pub fn tiles(&self) -> Vec<TileId> {
        frame_tiles(&self.camera, self.labels.as_ref(), self.viewport).tiles
    }
}

/// A finished frame: its premultiplied RGBA pixels, and an image reading them without a copy.
///
/// Performance: do not go back to `Data::new_copy` or a new buffer per frame. Without them, the map
/// thread went from about 6.5 to 4.5 ms per frame on the 4K benchmark.
#[derive(Clone)]
pub struct MapFramePixels {
    /// Declared first, so it drops before the pixels it reads.
    image: Image,
    pixels: Arc<Vec<u8>>,
}

impl MapFramePixels {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> WidgetResult<Self> {
        let info = ImageInfo::new(
            (width as i32, height as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        let pixels = Arc::new(pixels);
        // SAFETY: the bytes are never written while shared, and every clone of this frame holds them
        // for as long as its image. `image()` lends the image out only to draw it.
        let data = unsafe { Data::new_bytes(&pixels) };
        let image = images::raster_from_data(&info, data, width as usize * 4)
            .ok_or_else(|| WidgetError::Render("unable to wrap map frame pixels".into()))?;
        Ok(Self { image, pixels })
    }

    pub fn image(&self) -> &Image {
        &self.image
    }

    /// The premultiplied RGBA bytes, for a host that shows the frame itself.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// The buffer back for another frame, unless a clone is still reading it. Keeping a clone across
    /// frames still works, but silently allocates a new buffer every frame.
    pub fn into_pixels(self) -> Option<Vec<u8>> {
        let Self { image, pixels } = self;
        drop(image);
        Arc::into_inner(pixels)
    }
}

/// A frame being drawn elsewhere: the finished frame, or why it failed, arrives here.
pub type MapFrameReply = Receiver<Result<MapFramePixels, String>>;

pub trait MapFrameProvider: Send + Sync {
    /// Starts the frame without waiting for it, drawing into `spare` when given one. Never blocks on
    /// the network. Fails only when the renderer has failed.
    /// Performance: must return before the frame is drawn, or export workers wait on the map again.
    fn frame(
        &self,
        request: &MapFrameRequest,
        spare: Option<Vec<u8>>,
    ) -> WidgetResult<MapFrameReply>;

    /// Rises each time the provider's renderer is rebuilt, so a frame it drew before is asked for
    /// again. Export's renderers live as long as their job and never rebuild.
    fn generation(&self) -> u64 {
        0
    }
}

/// Makes the frame provider of each map in a native editor preview.
pub trait MapFrameFactory: Send + Sync {
    /// The provider of one map drawing `style` with `sample_count` MSAA samples. Dropping it releases
    /// the renderer it holds.
    fn provider(&self, style: &MapStyleInfo, sample_count: u32) -> Arc<dyn MapFrameProvider>;
}

/// What the editor preview's maps draw with.
#[derive(Default)]
pub struct PreviewMaps {
    /// The installed styles, which the host sets once it has them: until then no map is created.
    pub styles: Option<MapStyleCatalog>,
    /// The host's frames.
    pub frames: Option<Arc<dyn MapFrameFactory>>,
}

/// The renderer identity a streamed map's provider was made for: its style and MSAA samples.
type StreamKey = (ovrley_map::MapStyleId, u32);

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

/// One map's frames from a [`MapFrameProvider`]: the frame on show, the one being drawn and the
/// one that failed (Phase 8 D7). The editor's maps and Sync Doctor's maps share its rules:
///
/// - a request is asked for once, and never again while its frame is held, being drawn or failed;
/// - a streamed stream draws one frame at a time and keeps showing the last frame until the next
///   arrives, never waiting; export's waits for each frame and hands the last one's buffer back;
/// - a renderer rebuilt by the provider (a new generation) asks again.
pub struct MapFrameStream {
    provider: Arc<dyn MapFrameProvider>,
    streamed: bool,
    /// The frame on show.
    shown: Option<MapFramePixels>,
    /// The request and provider generation `shown` answers, so an unchanged request asks the
    /// provider once. A new provider has drawn nothing yet, so it clears this and keeps `shown`.
    held: Option<(MapFrameRequest, u64)>,
    /// The frame the provider is drawing, with the provider generation it was asked of.
    drawing: Option<(MapFrameRequest, u64, MapFrameReply)>,
    /// A streamed request whose frame failed, so it is not asked for again. The provider has
    /// logged why.
    failed: Option<(MapFrameRequest, u64)>,
    /// Counts the streamed frames that arrived or failed, so one arriving for an unchanged state
    /// draws again.
    serial: u64,
}

impl MapFrameStream {
    /// A native preview's stream: it never waits for a frame.
    pub fn streamed(provider: Arc<dyn MapFrameProvider>) -> Self {
        Self::new(provider, true)
    }

    /// Export's stream: each frame is waited for, drawn into the last one's buffer.
    pub fn blocking(provider: Arc<dyn MapFrameProvider>) -> Self {
        Self::new(provider, false)
    }

    fn new(provider: Arc<dyn MapFrameProvider>, streamed: bool) -> Self {
        Self { provider, streamed, shown: None, held: None, drawing: None, failed: None, serial: 0 }
    }

    /// Another renderer (another style or sample count): the frame on show stays until the new one
    /// draws, so the change never flashes the placeholder.
    pub fn replace_provider(&mut self, provider: Arc<dyn MapFrameProvider>) {
        self.provider = provider;
        self.held = None;
        self.drawing = None;
        self.failed = None;
    }

    /// Forgets every frame, as for another activity.
    pub fn clear(&mut self) {
        self.shown = None;
        self.held = None;
        self.drawing = None;
        self.failed = None;
    }

    pub fn shown(&self) -> Option<&MapFramePixels> {
        self.shown.as_ref()
    }

    /// The request the frame on show answers.
    pub fn shown_request(&self) -> Option<&MapFrameRequest> {
        self.held.as_ref().map(|(request, _)| request)
    }

    /// Whether a frame is being drawn.
    pub fn is_drawing(&self) -> bool {
        self.drawing.is_some()
    }

    /// Counts the frames that arrived or failed.
    pub fn serial(&self) -> u64 {
        self.serial
    }

    /// Whether `request` is held, being drawn or failed, so asking again would do nothing.
    pub fn answers(&self, request: &MapFrameRequest) -> bool {
        let generation = self.provider.generation();
        let answered = |slot: &Option<(MapFrameRequest, u64)>| {
            slot.as_ref()
                .is_some_and(|(frame, drawn)| frame == request && *drawn == generation)
        };
        answered(&self.held)
            || answered(&self.failed)
            || self
                .drawing
                .as_ref()
                .is_some_and(|(frame, _, _)| frame == request)
    }

    /// Asks for a frame neither held nor being drawn. A streamed stream asks for one frame at a
    /// time; export hands the last frame's buffer over.
    pub fn request(&mut self, request: &MapFrameRequest) -> WidgetResult<()> {
        if self.answers(request) {
            return Ok(());
        }
        let generation = self.provider.generation();
        if self.streamed {
            if self.drawing.is_none() {
                self.drawing = Some((request.clone(), generation, self.provider.frame(request, None)?));
            }
            return Ok(());
        }
        // Performance: the previous frame's buffer is reused, so a frame allocates nothing.
        self.held = None;
        let spare = self.shown.take().and_then(MapFramePixels::into_pixels);
        self.drawing = Some((request.clone(), generation, self.provider.frame(request, spare)?));
        Ok(())
    }

    /// Takes a streamed frame that has arrived, or failed, since the last look. Returns whether
    /// one did.
    pub fn collect(&mut self) -> bool {
        let Some((_, _, reply)) = &self.drawing else {
            return false;
        };
        let result = match reply.try_recv() {
            Err(TryRecvError::Empty) => return false,
            Ok(result) => result,
            Err(TryRecvError::Disconnected) => Err("The map frame thread stopped".into()),
        };
        let (request, generation, _) = self.drawing.take().expect("a frame was being drawn");
        self.serial = self.serial.wrapping_add(1);
        match result {
            Ok(frame) => {
                self.shown = Some(frame);
                self.held = Some((request, generation));
                self.failed = None;
            }
            Err(_) => {
                self.shown = None;
                self.held = None;
                self.failed = Some((request, generation));
            }
        }
        true
    }

    /// The frame for `request`: streamed, the newest one in (asking for this one); otherwise
    /// waited for.
    fn resolve(&mut self, request: &MapFrameRequest) -> WidgetResult<()> {
        if self.streamed {
            self.collect();
            return self.request(request);
        }
        self.request(request)?;
        if let Some((request, generation, answer)) = self.drawing.take() {
            let frame = answer
                .recv()
                .map_err(|_| WidgetError::Render("The map renderer stopped mid-frame".into()))?
                .map_err(WidgetError::Render)?;
            self.shown = Some(frame);
            self.held = Some((request, generation));
        }
        Ok(())
    }
}

pub struct PreviewMap {
    config: ValidatedMapWidget,
    /// The installed style the config's resolved to.
    style: MapStyleInfo,
    scene: ValidatedScenePresentation,
    window: Option<RouteWindow>,
    preview_duration: f64,
    raster_scale: PreviewRasterScale,
    /// Export's frames, or a native preview's. The WASM preview has none: its map is the host's
    /// canvas.
    frames: Option<MapFrameStream>,
    /// What a native preview's provider was made for. Some when frames stream: then the widget
    /// never waits for a frame.
    streamed: Option<StreamKey>,
    layout: MapLayout,
    content: Option<MapContent>,
    /// The inputs of `content`.
    content_key: ContentKey,
    /// The top-down route's stroke under this layout, kept while the map's line stays the same.
    /// [`Self::frame_request`] brings it up to date, so it is current for any frame with a request.
    screen_route: Option<ScreenRoute>,
    last_render: Option<(MapFrameState, bool, u64)>,
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
    /// The editor preview's map: drawn by the host's map canvas (WASM), or from frames streamed
    /// by the host's factory (native).
    pub fn new(
        widget: MapConfig,
        scene: ScenePresentationConfig,
        raster_scale: PreviewRasterScale,
        maps: &PreviewMaps,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<Self> {
        let config = validate_map_widget(&widget, 0)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
        let style = resolve_style(maps.styles.as_ref(), &config)?;
        let streamed = maps
            .frames
            .as_ref()
            .map(|factory| (factory.provider(&style, config.msaa_samples), (style.id.clone(), config.msaa_samples)));
        let (frames, streamed) = streamed.map_or((None, None), |(frames, key)| (Some(MapFrameStream::streamed(frames)), Some(key)));
        let mut map = Self::with_content(
            style,
            config,
            validate_scene_presentation(scene)?,
            &host,
            frames,
            None,
            fonts,
        )?;
        map.streamed = streamed;
        Ok(map)
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
        Self::with_content(
            style,
            config,
            scene,
            host,
            Some(MapFrameStream::blocking(frames)),
            Some(content),
            fonts,
        )
    }

    fn with_content(
        style: MapStyleInfo,
        config: ValidatedMapWidget,
        scene: ValidatedScenePresentation,
        host: &WidgetHost,
        frames: Option<MapFrameStream>,
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
            streamed: None,
            content,
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
        raster_scale: PreviewRasterScale,
        maps: &PreviewMaps,
        fonts: &dyn FontResolver,
    ) -> WidgetResult<()> {
        let config = validate_map_widget(&widget, 0)?;
        let style = resolve_style(maps.styles.as_ref(), &config)?;
        let host = WidgetHost::from_scene(&scene, raster_scale)?;
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
        // Another style or sample count needs another renderer. The frame on show stays until the
        // new one draws, so the edit never flashes the placeholder.
        if let (Some(factory), Some(key)) = (&maps.frames, &self.streamed) {
            let wanted = (style.id.clone(), config.msaa_samples);
            if *key != wanted {
                self.frames.as_mut().expect("a streamed map has frames").replace_provider(factory.provider(&style, config.msaa_samples));
                self.streamed = Some(wanted);
            }
        }
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

    /// What a top-down view gesture pans across: the activity's route, once the map has one.
    pub fn route_extent(
        &mut self,
        activity: Option<&ActivitySampler>,
    ) -> WidgetResult<Option<RouteExtent>> {
        self.prepare(activity)?;
        Ok(match self.content.as_ref() {
            Some(MapContent::Activity(map)) => Some(RouteExtent::of(&map.track)),
            _ => None,
        })
    }

    pub fn invalidate_activity(&mut self) {
        self.content = None;
        if let Some(frames) = &mut self.frames {
            frames.clear();
        }
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
        // The placement cameras, then the blend; empty when the frame draws no labels.
        let labels: Vec<f64> = request.labels.map_or_else(Vec::new, |draw| {
            let mut values: Vec<f64> = draw
                .cameras()
                .into_iter()
                .flat_map(MapCamera::values)
                .collect();
            values.push(f64::from(draw.blend()));
            values
        });
        self.view = serde_json::to_vec(&json!({
            "renderer": {
                "style": self.style.id,
                "style_buildings": self.style.buildings,
                "buildings": self.config.layers.buildings,
                "labels": self.config.layers.labels,
                "paths": self.config.layers.paths,
                "local_names": self.config.layers.local_names,
                "pixel_ratio": viewport.pixel_ratio(),
                "msaa_samples": self.config.msaa_samples,
            },
            "width": viewport.width_px(),
            "height": viewport.height_px(),
            "style_zoom": camera.zoom(),
            "opacity": self.layout.opacity,
            "corner_radius_px": request.corner_radius_px,
            "camera": camera.values(),
            "labels": labels,
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

    /// Asks the provider for a frame neither held nor being drawn (see [`MapFrameStream`]).
    fn start_request(&mut self, request: &MapFrameRequest) -> WidgetResult<()> {
        match &mut self.frames {
            Some(frames) => frames.request(request),
            None => Ok(()),
        }
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
        if let Some(frames) = &mut self.frames {
            frames.resolve(&request)?;
        }
        Ok(Some((
            camera,
            match self.frames.as_ref().and_then(MapFrameStream::shown) {
                Some(frame) => MapBacking::Image(frame.clone()),
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

    fn frame_serial(&self) -> u64 {
        self.frames.as_ref().map_or(0, MapFrameStream::serial)
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
        if self.last_render == Some((state, has_frame, self.frame_serial())) {
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
        self.last_render = Some((state, has_frame, self.frame_serial()));
        Ok(())
    }
}
