//! A real map behind the route: camera, GPU frame, marker and attribution, shared by both hosts.
//! The renderer draws the map and the route together, so the widget only owns what they show.
use crate::activity::ActivitySampler;
use crate::model::scene::{
    validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation,
};
use crate::model::validation::{
    require_f32, require_hex_color, require_non_negative_f32, require_positive_f32,
    require_unit_opacity,
};
use crate::preview::ResizeGesture;
use crate::preview::WidgetHost;
use crate::render::tile::PreviewRasterScale;
use crate::widgets::map::track::{route_line, screen_line, MapTrack, TrackPosition};
use crate::widgets::route::geometry::ProgressLine;
use crate::widgets::route::{
    validate_route_path, RoutePathConfig, RouteWindow, ValidatedRoutePath,
};
use crate::{WidgetError, WidgetResult};
use ovrley_map::mercator::world_to_lon_lat;
use ovrley_map::{
    frame_tiles, MapCamera, MapLabelDraw, MapLayers, MapRouteLine, MapStyleId, MapStyleInfo,
    MapStyleSpec, MapViewport, TileId, LABEL_PLACEMENTS, MAX_PITCH_DEG, MAX_TILE_ZOOM,
    MAX_VIEW_TILES, TILE_SIZE,
};
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;
use std::sync::Arc;

pub mod layout;
pub mod preview;
pub mod track;

/// Effective map config; every output-affecting field is required at ingress.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct MapConfig {
    pub value: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
    /// The attribution's font. The attribution itself is always drawn.
    pub font: String,
    /// Need not be installed: drawing resolves it. See [`ovrley_map::MapStyleCatalog::resolve`].
    pub map_style: MapStyleId,
    pub show_buildings: bool,
    pub show_paths: bool,
    pub show_labels: bool,
    /// Labels with each place's own name instead of its international one.
    pub local_names: bool,
    pub camera_mode: String,
    pub orientation: String,
    pub pitch: f32,
    /// The chase camera's zoom level.
    pub follow_zoom: f32,
    /// The top-down camera's zoom level.
    pub fixed_zoom: f32,
    /// How far the top-down route sits from the centre, as a share of its longest side.
    pub fixed_pan_x: f32,
    pub fixed_pan_y: f32,
    pub msaa_samples: u32,
    pub shape: String,
    pub corner_radius: f32,
    pub border_color: String,
    pub border_thickness: f32,
    pub border_opacity: f32,
    /// How strongly GPS noise is smoothed out of the route, in metres.
    pub route_smoothing_m: f32,
    pub show_full_activity: bool,
    /// Both sides of the split draw at this width.
    pub line_width: f32,
    #[serde(flatten)]
    pub path: RoutePathConfig,
}

/// Further out, a follow camera's pitched view takes in too many tiles.
const MIN_FOLLOW_ZOOM: f32 = 14.0;
pub(super) const FOLLOW_ZOOM_RANGE: RangeInclusive<f32> = MIN_FOLLOW_ZOOM..=MAX_TILE_ZOOM as f32;
/// Further out, the world is narrower than a large widget.
const MIN_FIXED_ZOOM: f32 = 2.0;
pub(super) const FIXED_ZOOM_RANGE: RangeInclusive<f32> = MIN_FIXED_ZOOM..=MAX_TILE_ZOOM as f32;
/// Metres. The low end looks almost raw, and past the high end a road's own bends start to go.
const ROUTE_SMOOTHING_RANGE: RangeInclusive<f32> = 1.0..=20.0;
/// The sample counts every GPU supports, so a project draws the same on any machine.
const MSAA_SAMPLES: [u32; 2] = [1, 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapOrientation {
    NorthUp,
    RiderUp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MapCameraMode {
    /// North up and pitch 0. The tiles never move: only the marker and the route's split do.
    Fixed { zoom: f32, pan: [f32; 2] },
    Follow {
        orientation: MapOrientation,
        pitch_deg: f32,
        zoom: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MapShape {
    Rectangle { corner_radius: f32 },
    Circle,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapBorder {
    pub color: String,
    pub thickness: f32,
    pub opacity: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedMapWidget {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
    pub font: String,
    /// The style it asks for, which need not be installed.
    pub style: MapStyleId,
    pub layers: MapLayers,
    pub camera: MapCameraMode,
    pub msaa_samples: u32,
    pub shape: MapShape,
    pub border: MapBorder,
    pub route_smoothing_m: f32,
    pub show_full_activity: bool,
    pub line_width: f32,
    pub path: ValidatedRoutePath,
}

impl ValidatedMapWidget {
    /// What its map draws in `style`, the installed style its own resolved to.
    pub fn spec(&self, style: &MapStyleInfo) -> MapStyleSpec {
        MapStyleSpec {
            style: style.id.clone(),
            layers: self.layers,
        }
    }
}

pub fn validate_map_widget(map: &MapConfig, index: usize) -> WidgetResult<ValidatedMapWidget> {
    let p = |f: &str| format!("plots[{index}].{f}");
    let invalid =
        |field: &str, message: &str| WidgetError::Config(format!("{}: {message}", p(field)));
    let in_range = |value: f32, field: &str, range: RangeInclusive<f32>| -> WidgetResult<f32> {
        let value = require_f32(Some(value), &p(field))?;
        if range.contains(&value) {
            Ok(value)
        } else {
            Err(invalid(
                field,
                &format!("must be between {} and {}", range.start(), range.end()),
            ))
        }
    };
    if map.value != "map" {
        return Err(invalid("value", "must be map"));
    }
    if map.font.trim().is_empty() {
        return Err(invalid("font", "must identify a font"));
    }
    let camera = match map.camera_mode.as_str() {
        "fixed" => MapCameraMode::Fixed {
            zoom: in_range(map.fixed_zoom, "fixed_zoom", FIXED_ZOOM_RANGE)?,
            pan: [
                require_f32(Some(map.fixed_pan_x), &p("fixed_pan_x"))?,
                require_f32(Some(map.fixed_pan_y), &p("fixed_pan_y"))?,
            ],
        },
        "follow" => MapCameraMode::Follow {
            orientation: match map.orientation.as_str() {
                "north_up" => MapOrientation::NorthUp,
                "rider_up" => MapOrientation::RiderUp,
                _ => return Err(invalid("orientation", "unsupported orientation")),
            },
            pitch_deg: in_range(map.pitch, "pitch", 0.0..=MAX_PITCH_DEG as f32)?,
            zoom: in_range(map.follow_zoom, "follow_zoom", FOLLOW_ZOOM_RANGE)?,
        },
        _ => return Err(invalid("camera_mode", "unsupported camera mode")),
    };
    // Seen from straight above, a building is only its roof, so the toggle needs a pitched camera.
    let buildings = map.show_buildings
        && matches!(camera, MapCameraMode::Follow { pitch_deg, .. } if pitch_deg > 0.0);
    if !MSAA_SAMPLES.contains(&map.msaa_samples) {
        return Err(invalid("msaa_samples", "must be 1 or 4"));
    }
    let width = require_positive_f32(Some(map.width), &p("width"))?;
    let height = require_positive_f32(Some(map.height), &p("height"))?;
    let shape = match map.shape.as_str() {
        "rectangle" => MapShape::Rectangle {
            corner_radius: require_non_negative_f32(Some(map.corner_radius), &p("corner_radius"))?,
        },
        "circle" if width == height => MapShape::Circle,
        "circle" => return Err(invalid("shape", "a circular map must be square")),
        _ => return Err(invalid("shape", "unsupported shape")),
    };

    Ok(ValidatedMapWidget {
        x: require_f32(Some(map.x), &p("x"))?,
        y: require_f32(Some(map.y), &p("y"))?,
        width,
        height,
        opacity: require_unit_opacity(Some(map.opacity), &p("opacity"))?,
        font: map.font.clone(),
        style: map.map_style.clone(),
        layers: MapLayers {
            buildings,
            labels: map.show_labels,
            paths: map.show_paths,
            local_names: map.local_names,
        },
        camera,
        msaa_samples: map.msaa_samples,
        shape,
        border: MapBorder {
            color: require_hex_color(Some(&map.border_color), &p("border_color"))?,
            thickness: require_non_negative_f32(
                Some(map.border_thickness),
                &p("border_thickness"),
            )?,
            opacity: require_unit_opacity(Some(map.border_opacity), &p("border_opacity"))?,
        },
        route_smoothing_m: in_range(
            map.route_smoothing_m,
            "route_smoothing_m",
            ROUTE_SMOOTHING_RANGE,
        )?,
        show_full_activity: map.show_full_activity,
        line_width: require_positive_f32(Some(map.line_width), &p("line_width"))?,
        path: validate_route_path(&map.path, &p)?,
    })
}

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
/// Empty share of the frame on each side of a fitted route.
const FIT_MARGIN: f64 = 0.08;
/// Seconds of activity time between label placements.
const LABEL_STEP_SECONDS: f64 = 0.3;

/// The world-space bounding box of a track, which a top-down camera centres on and fits, and whose
/// longest side its pan is measured in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteExtent {
    pub centre: (f64, f64),
    pub size: (f64, f64),
    pub longest: f64,
}

impl RouteExtent {
    pub fn of(track: &MapTrack) -> Self {
        let (mut min, mut max) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
        for point in track.world() {
            min = (min.0.min(point.0), min.1.min(point.1));
            max = (max.0.max(point.0), max.1.max(point.1));
        }
        let size = (max.0 - min.0, max.1 - min.1);
        Self {
            centre: ((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0),
            size,
            longest: size.0.max(size.1),
        }
    }

    /// The zoom at which the whole route fits a `frame` of widget units, inside its margin. Infinite
    /// for a route that is one point, which fits at any zoom.
    pub fn fit_zoom(&self, frame: [f64; 2]) -> f64 {
        let tile = f64::from(TILE_SIZE);
        let fit = |frame: f64, size: f64| (frame * (1.0 - FIT_MARGIN * 2.0) / (size * tile)).log2();
        fit(frame[0], self.size.0).min(fit(frame[1], self.size.1))
    }

    /// `pan` with the camera's centre kept on the map. A route that is one point has no pan.
    pub fn clamp_pan(&self, pan: [f64; 2]) -> [f64; 2] {
        if self.longest <= 0.0 {
            return [0.0, 0.0];
        }
        let centre = [self.centre.0, self.centre.1];
        [0, 1].map(|axis| {
            pan[axis].clamp(
                (centre[axis] - 1.0) / self.longest,
                centre[axis] / self.longest,
            )
        })
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
/// share of its longest side, so a project's framing carries over to another activity, where it may
/// reach past the map's edge.
fn fixed_camera(track: &MapTrack, zoom: f64, pan: [f64; 2]) -> WidgetResult<MapCamera> {
    let route = RouteExtent::of(track);
    let pan = route.clamp_pan(pan);
    let (lon, lat) = world_to_lon_lat(
        route.centre.0 - pan[0] * route.longest,
        route.centre.1 - pan[1] * route.longest,
    );
    MapCamera::new(lon, lat, zoom, 0.0, 0.0, 0.0).map_err(WidgetError::Render)
}

/// The editor's smallest widget edge.
const MIN_EDGE: f32 = 8.0;
/// Wheel pixels per zoom level, so a 100-pixel mouse notch is a fifth of a level.
const WHEEL_PIXELS_PER_LEVEL: f32 = 500.0;

fn within(value: f32, range: RangeInclusive<f32>) -> f32 {
    value.clamp(*range.start(), *range.end())
}

/// The zoom field of the widget's camera mode.
#[derive(serde::Serialize)]
#[serde(untagged)]
pub enum ZoomUpdate {
    Fixed { fixed_zoom: f32 },
    Follow { follow_zoom: f32 },
}

#[derive(serde::Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// A circle has no corner radius of its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f32>,
    pub line_width: f32,
    pub marker_size: f32,
    pub marker_variant_diameter: f32,
    #[serde(flatten)]
    pub zoom: ZoomUpdate,
}

/// The factor of the edge that moved further, so a corner follows the pointer's stronger axis.
fn dominant_factor(requested: [f32; 2], current: [f32; 2]) -> f32 {
    let [x, y] = [requested[0] / current[0], requested[1] / current[1]];
    if (x - 1.0).abs() >= (y - 1.0).abs() {
        x
    } else {
        y
    }
}

pub fn resize_map(
    widget: MapConfig,
    scene: ScenePresentationConfig,
    size: [f32; 2],
    gesture: ResizeGesture,
) -> WidgetResult<ResizeUpdate> {
    let ResizeGesture { fixed, point, .. } = gesture;
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(WidgetError::Config("invalid map resize size".into()));
    }
    let config = validate_map_widget(&widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    let requested = [size[0] / scene.scale, size[1] / scene.scale];
    // A circle has no sides, so it always scales.
    let corner =
        matches!(config.shape, MapShape::Circle) || fixed.iter().all(|share| *share != 0.5);
    let (width, height, factor) = if corner {
        let factor = dominant_factor(requested, [config.width, config.height])
            .max(MIN_EDGE / config.width.min(config.height));
        (config.width * factor, config.height * factor, factor)
    } else {
        (requested[0].max(MIN_EDGE), requested[1].max(MIN_EDGE), 1.0)
    };
    // One zoom level doubles the map, so this scales it by `factor` until the camera's limit.
    let zoom = |zoom: f32, range: RangeInclusive<f32>| within(zoom + factor.log2(), range);
    Ok(ResizeUpdate {
        x: point[0] - width * scene.scale * fixed[0],
        y: point[1] - height * scene.scale * fixed[1],
        width,
        height,
        corner_radius: match config.shape {
            MapShape::Rectangle { corner_radius } => Some(corner_radius * factor),
            MapShape::Circle => None,
        },
        line_width: config.line_width * factor,
        marker_size: config.path.marker_size * factor,
        marker_variant_diameter: config.path.marker_variant_diameter * factor,
        zoom: match config.camera {
            MapCameraMode::Fixed { zoom: fixed, .. } => ZoomUpdate::Fixed {
                fixed_zoom: zoom(fixed, FIXED_ZOOM_RANGE),
            },
            MapCameraMode::Follow { zoom: follow, .. } => ZoomUpdate::Follow {
                follow_zoom: zoom(follow, FOLLOW_ZOOM_RANGE),
            },
        },
    })
}

/// A view-mode gesture, in scene pixels.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapViewGesture {
    /// How far a top-down map has been dragged.
    Pan([f32; 2]),
    /// A wheel's `deltaY` at a scene point. A top-down map keeps that point in place.
    Zoom { wheel_delta: f32, point: [f32; 2] },
    /// The top-down view that centres the whole route in the frame, in either camera mode, so a map
    /// can switch to top down already fitted.
    Fit,
}

/// The view fields of the widget's camera mode.
#[derive(serde::Serialize)]
#[serde(untagged)]
pub enum MapViewUpdate {
    Fixed {
        fixed_zoom: f32,
        fixed_pan_x: f32,
        fixed_pan_y: f32,
    },
    Follow {
        follow_zoom: f32,
    },
}

/// `route` is what a top-down camera pans across: without GPS there is none, and only zoom moves.
pub fn adjust_map_view(
    widget: MapConfig,
    scene: ScenePresentationConfig,
    gesture: MapViewGesture,
    route: Option<RouteExtent>,
) -> WidgetResult<MapViewUpdate> {
    let finite = match gesture {
        MapViewGesture::Pan(delta) => delta.iter().all(|value| value.is_finite()),
        MapViewGesture::Zoom { wheel_delta, point } => {
            wheel_delta.is_finite() && point.iter().all(|value| value.is_finite())
        }
        MapViewGesture::Fit => true,
    };
    if !finite {
        return Err(WidgetError::Config("invalid map view gesture".into()));
    }
    let config = validate_map_widget(&widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    let levels = |wheel_delta: f32| -wheel_delta / WHEEL_PIXELS_PER_LEVEL;
    let (zoom, pan) = match (config.camera, gesture) {
        (_, MapViewGesture::Fit) => {
            // Without GPS there is nothing to fit, and the zoom stays.
            let zoom = route.map_or(widget.fixed_zoom, |route| {
                route.fit_zoom([f64::from(config.width), f64::from(config.height)]) as f32
            });
            return Ok(MapViewUpdate::Fixed {
                fixed_zoom: within(zoom, FIXED_ZOOM_RANGE),
                fixed_pan_x: 0.0,
                fixed_pan_y: 0.0,
            });
        }
        (MapCameraMode::Follow { zoom, .. }, MapViewGesture::Zoom { wheel_delta, .. }) => {
            return Ok(MapViewUpdate::Follow {
                follow_zoom: within(zoom + levels(wheel_delta), FOLLOW_ZOOM_RANGE),
            });
        }
        (MapCameraMode::Follow { .. }, MapViewGesture::Pan(_)) => {
            return Err(WidgetError::Config(
                "a chase camera follows the rider and has no pan".into(),
            ));
        }
        (MapCameraMode::Fixed { zoom, pan }, _) => (zoom, pan),
    };
    // Shares of the route per widget unit at `zoom`. A route that is one point has no pan to move.
    let per_unit = |zoom: f32| {
        route
            .map(|route| route.span(f64::from(zoom)))
            .filter(|span| *span > 0.0)
            .map_or(0.0, |span| (1.0 / span) as f32)
    };
    let (next, shift) = match gesture {
        MapViewGesture::Pan(delta) => (
            zoom,
            delta.map(|value| value / scene.scale * per_unit(zoom)),
        ),
        MapViewGesture::Fit => unreachable!("fit returned above"),
        MapViewGesture::Zoom { wheel_delta, point } => {
            let next = within(zoom + levels(wheel_delta), FIXED_ZOOM_RANGE);
            // The cursor's offset from the frame's centre, in widget units, shows the same place.
            let anchor = [
                (point[0] - config.x) / scene.scale - config.width / 2.0,
                (point[1] - config.y) / scene.scale - config.height / 2.0,
            ];
            let change = per_unit(next) - per_unit(zoom);
            (next, anchor.map(|value| value * change))
        }
    };
    let pan = [pan[0] + shift[0], pan[1] + shift[1]];
    let [fixed_pan_x, fixed_pan_y] = match route {
        Some(route) => route
            .clamp_pan(pan.map(f64::from))
            .map(|value| value as f32),
        None => pan,
    };
    Ok(MapViewUpdate::Fixed {
        fixed_zoom: next,
        fixed_pan_x,
        fixed_pan_y,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::route::TrackSample;
    use serde_json::json;

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

    fn map(camera_mode: &str) -> MapConfig {
        serde_json::from_value(json!({
        "value": "map", "x": 100, "y": 50, "opacity": 1, "font": "Inter.ttf", "width": 200, "height": 100,
        "map_style": "liberty", "show_buildings": false, "show_paths": true, "show_labels": true, "local_names": false, "camera_mode": camera_mode,
        "orientation": "north_up", "pitch": 0, "follow_zoom": 16, "fixed_zoom": 15, "fixed_pan_x": 0,
        "fixed_pan_y": 0, "msaa_samples": 4, "shape": "rectangle", "corner_radius": 0,
        "border_color": "#ffffff", "border_thickness": 0, "border_opacity": 1, "route_smoothing_m": 8,
        "show_full_activity": true, "line_width": 6, "completed_line_opacity": 1, "remaining_line_opacity": 1,
        "completed_line_color": "#ff4d1f", "remaining_line_color": "#1f7bff", "marker_variant": "single",
        "marker_variant_diameter": 44, "marker_size": 10, "marker_opacity": 1, "marker_color": "#ffffff"
    }))
    .unwrap()
    }

    fn scene() -> ScenePresentationConfig {
        serde_json::from_value(json!({
        "width": 1920, "height": 1080, "scale": 2, "opacity": 1, "shadow_strength": 0, "shadow_distance": 0,
        "shadow_color": "#000000", "border_thickness": 0, "border_color": "#000000"
    }))
    .unwrap()
    }

    fn fixed(update: MapViewUpdate) -> [f32; 3] {
        match update {
            MapViewUpdate::Fixed {
                fixed_zoom,
                fixed_pan_x,
                fixed_pan_y,
            } => [fixed_zoom, fixed_pan_x, fixed_pan_y],
            MapViewUpdate::Follow { .. } => panic!("expected a top-down update"),
        }
    }

    const LONGEST: f64 = 420.0 / (ovrley_map::TILE_SIZE as f64 * 32768.0);

    /// A route whose longest side spans 420 widget units at zoom 15, 840 scene pixels at scale 2.
    fn route() -> Option<RouteExtent> {
        Some(RouteExtent {
            centre: (0.5, 0.5),
            size: (LONGEST, LONGEST / 2.0),
            longest: LONGEST,
        })
    }

    #[test]
    fn the_view_follows_the_drag_and_zooms_around_the_cursor() {
        let [zoom, pan_x, pan_y] = fixed(
            adjust_map_view(
                map("fixed"),
                scene(),
                MapViewGesture::Pan([84.0, -42.0]),
                route(),
            )
            .unwrap(),
        );
        assert_eq!(zoom, 15.0);
        assert!((pan_x - 0.1).abs() < 1e-6 && (pan_y + 0.05).abs() < 1e-6);

        // The frame's centre stays put; a point right of it moves the route away from the cursor.
        let centre = [100.0 + 200.0, 50.0 + 100.0];
        let [zoom, pan_x, pan_y] = fixed(
            adjust_map_view(
                map("fixed"),
                scene(),
                MapViewGesture::Zoom {
                    wheel_delta: -500.0,
                    point: centre,
                },
                route(),
            )
            .unwrap(),
        );
        assert_eq!([zoom, pan_x, pan_y], [16.0, 0.0, 0.0]);
        let right = [centre[0] + 84.0, centre[1]];
        let [_, pan_x, _] = fixed(
            adjust_map_view(
                map("fixed"),
                scene(),
                MapViewGesture::Zoom {
                    wheel_delta: -500.0,
                    point: right,
                },
                route(),
            )
            .unwrap(),
        );
        assert!((pan_x + 0.05).abs() < 1e-6);

        assert!(adjust_map_view(
            map("follow"),
            scene(),
            MapViewGesture::Pan([1.0, 0.0]),
            route()
        )
        .is_err());
    }

    #[test]
    fn fit_centres_the_route_and_fills_the_frame_inside_its_margin() {
        // A 200 by 100 frame: the route's 420 by 210 units at zoom 15 fit its height, 84 units inside
        // the margin, at zoom 15 + log2(84 / 210).
        let mut panned = map("follow");
        panned.fixed_pan_x = 3.0;
        let [zoom, pan_x, pan_y] =
            fixed(adjust_map_view(panned, scene(), MapViewGesture::Fit, route()).unwrap());
        assert!((zoom - (15.0 + (84.0f32 / 210.0).log2())).abs() < 1e-4);
        assert_eq!([pan_x, pan_y], [0.0, 0.0]);
        // Without GPS only the pan resets.
        let [zoom, ..] =
            fixed(adjust_map_view(map("fixed"), scene(), MapViewGesture::Fit, None).unwrap());
        assert_eq!(zoom, 15.0);
    }

    #[test]
    fn the_zoom_reaches_out_to_the_minimum_and_a_map_without_gps_only_zooms() {
        let [zoom, pan_x, pan_y] = fixed(
            adjust_map_view(
                map("fixed"),
                scene(),
                MapViewGesture::Zoom {
                    wheel_delta: 500.0 * 30.0,
                    point: [400.0, 150.0],
                },
                None,
            )
            .unwrap(),
        );
        assert_eq!([zoom, pan_x, pan_y], [*FIXED_ZOOM_RANGE.start(), 0.0, 0.0]);
        let [_, pan_x, pan_y] = fixed(
            adjust_map_view(
                map("fixed"),
                scene(),
                MapViewGesture::Pan([84.0, 42.0]),
                None,
            )
            .unwrap(),
        );
        assert_eq!([pan_x, pan_y], [0.0, 0.0]);
    }
}
