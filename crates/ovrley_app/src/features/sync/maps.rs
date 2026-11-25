//! Sync Doctor's two maps: the top-down selection map on the right screen and the chase inset over
//! the video. Both draw through `ovrley_map` on the maps' frame thread.
//!
//! The selection map costs almost nothing while the video plays: its frame is drawn once per view,
//! the course is stroked into it once, and Slint shows that one image until the view changes. Only
//! its markers move with the playhead. While the user pans or zooms, the image follows the pointer
//! at once, and frames follow at most every 16 ms with one in flight.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use ovrley_core::activity::schema::ParsedActivity;
use ovrley_map::{LABEL_PLACEMENTS, MAX_TILE_ZOOM, MapCamera, MapLabelDraw, MapLayers, MapRouteDraw, MapRouteLine, MapRouteStyle, MapStyleInfo, MapStyleSpec, MapViewport, TILE_SIZE};
use ovrley_map::mercator::{lon_lat_to_world, world_to_lon_lat};
use ovrley_widgets::widgets::map::preview::{MapFramePixels, MapFrameRequest, MapFrameStream};
use ovrley_widgets::widgets::map::track::MapTrack;
use crate::features::sync::detection::LocationEvent;

/// Where the selection map starts before a course exists (Zurich).
pub const INITIAL_CENTER: (f64, f64) = (8.5417, 47.3769);
pub const INITIAL_ZOOM: f64 = 13.0;
/// The fit leaves this much room around the course, and stops at this zoom.
const FIT_PADDING_PX: f64 = 64.0;
const FIT_MAX_ZOOM: f64 = 17.0;

/// A course segment through a camera: `(x, y, second)` per point.
pub type ProjectedSegment = Vec<(f64, f64, f64)>;

/// A rectangle on screen: its origin and its size.
pub type ScreenRect = ((f64, f64), (f64, f64));
/// The renderer's deepest zoom.
pub const MAX_ZOOM: f64 = MAX_TILE_ZOOM as f64;
/// A pointer within this distance of the course snaps to it.
pub const SNAP_DISTANCE_PX: f64 = 20.0;
/// A new screen size is drawn once it has held for this long.
pub const RESIZE_SETTLE: Duration = Duration::from_millis(100);
/// The selection map asks for a frame at most this often.
pub const FRAME_INTERVAL: Duration = Duration::from_millis(16);
/// No side of a frame passes this many output pixels.
const MAX_CANVAS_PX: f64 = 4096.0;
/// The course line: `#EF6C15`, 6 px.
pub const COURSE_COLOR: [u8; 3] = [0xEF, 0x6C, 0x15];
const COURSE_WIDTH_PX: f64 = 6.0;
/// A press that moves less than this is a click.
const CLICK_TOLERANCE_PX: f64 = 3.0;

// The drag's coast.
const INERTIA_CUTOFF: Duration = Duration::from_millis(160);
const INERTIA_VELOCITY_WINDOW: Duration = Duration::from_millis(60);
const INERTIA_LINEARITY: f64 = 0.3;
const INERTIA_DECELERATION: f64 = 2500.0;
const INERTIA_MAX_SPEED: f64 = 1400.0;
const INERTIA_EASING: CubicBezier = CubicBezier::new(0.0, 0.0, 0.3, 1.0);

// The wheel and the trackpad.
const WHEEL_NOTCH: f64 = 4.000244140625;
const WHEEL_ZOOM_RATE: f64 = 1.0 / 450.0;
const TRACKPAD_ZOOM_RATE: f64 = 1.0 / 100.0;
const MAX_SCALE_PER_FRAME: f64 = 2.0;
const WHEEL_EASE: Duration = Duration::from_millis(200);
const WHEEL_TIME_ADJUSTMENT: Duration = Duration::from_millis(5);
/// The wheel's target zoom is kept this long after its ease ends, so a quick next tick continues it.
const WHEEL_FINISH: Duration = Duration::from_millis(200);

// The eased zooms.
const DEFAULT_EASING: CubicBezier = CubicBezier::new(0.25, 0.1, 0.25, 1.0);
const DOUBLE_CLICK_EASE: Duration = Duration::from_millis(300);
const BUTTON_EASE: Duration = Duration::from_millis(500);

/// A cubic Bézier timing curve through (0, 0) and (1, 1), as CSS timing functions are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

impl CubicBezier {
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self { x1, y1, x2, y2 }
    }

    fn sample(a1: f64, a2: f64, t: f64) -> f64 {
        // B(t) = 3(1-t)²t·a1 + 3(1-t)t²·a2 + t³
        let u = 1.0 - t;
        3.0 * u * u * t * a1 + 3.0 * u * t * t * a2 + t * t * t
    }

    fn slope(a1: f64, a2: f64, t: f64) -> f64 {
        let u = 1.0 - t;
        3.0 * u * u * a1 + 6.0 * u * t * (a2 - a1) + 3.0 * t * t * (1.0 - a2)
    }

    /// The curve's progress at time `x` in `0..=1`.
    pub fn at(&self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        // Newton's method from x, then bisection if it does not settle.
        let mut t = x;
        for _ in 0..8 {
            let error = Self::sample(self.x1, self.x2, t) - x;
            if error.abs() < 1e-7 {
                return Self::sample(self.y1, self.y2, t);
            }
            let slope = Self::slope(self.x1, self.x2, t);
            if slope.abs() < 1e-6 {
                break;
            }
            t -= error / slope;
        }
        let (mut low, mut high) = (0.0, 1.0);
        t = x;
        for _ in 0..60 {
            let value = Self::sample(self.x1, self.x2, t);
            if (value - x).abs() < 1e-7 {
                break;
            }
            if value < x {
                low = t;
            } else {
                high = t;
            }
            t = f64::midpoint(low, high);
        }
        Self::sample(self.y1, self.y2, t)
    }
}

/// A pitch-0, north-up camera: a centre in Mercator world coordinates and a zoom. Its projection
/// is exact and invertible, so the image on show and the markers can track the pointer between
/// frames. Points are logical pixels from the screen's top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TopDownCamera {
    pub world: (f64, f64),
    pub zoom: f64,
}

impl TopDownCamera {
    pub fn at_lon_lat(lon: f64, lat: f64, zoom: f64) -> Self {
        Self {
            world: lon_lat_to_world(lon, lat),
            zoom,
        }
    }

    /// The world's edge in logical pixels.
    pub fn scale(&self) -> f64 {
        f64::from(TILE_SIZE) * self.zoom.exp2()
    }

    pub fn project(&self, world: (f64, f64), size: (f64, f64)) -> (f64, f64) {
        let scale = self.scale();
        (
            (world.0 - self.world.0) * scale + size.0 / 2.0,
            (world.1 - self.world.1) * scale + size.1 / 2.0,
        )
    }

    pub fn unproject(&self, point: (f64, f64), size: (f64, f64)) -> (f64, f64) {
        let scale = self.scale();
        (
            self.world.0 + (point.0 - size.0 / 2.0) / scale,
            self.world.1 + (point.1 - size.1 / 2.0) / scale,
        )
    }

    /// Moved by a pointer delta: the map follows the pointer. The centre stays on the world.
    pub fn panned(&self, delta: (f64, f64)) -> Self {
        let scale = self.scale();
        Self {
            world: (
                (self.world.0 - delta.0 / scale).clamp(0.0, 1.0),
                (self.world.1 - delta.1 / scale).clamp(0.0, 1.0),
            ),
            zoom: self.zoom,
        }
    }

    /// At `zoom`, keeping the world point under `point` where it is.
    pub fn zoomed_about(&self, zoom: f64, point: (f64, f64), size: (f64, f64)) -> Self {
        let zoom = zoom.clamp(0.0, MAX_ZOOM);
        let anchor = self.unproject(point, size);
        let scale = f64::from(TILE_SIZE) * zoom.exp2();
        Self {
            world: (
                (anchor.0 - (point.0 - size.0 / 2.0) / scale).clamp(0.0, 1.0),
                (anchor.1 - (point.1 - size.1 / 2.0) / scale).clamp(0.0, 1.0),
            ),
            zoom,
        }
    }

    pub fn map_camera(&self) -> Result<MapCamera, String> {
        let (lon, lat) = world_to_lon_lat(self.world.0, self.world.1);
        MapCamera::new(lon, lat, self.zoom, 0.0, 0.0, 0.0)
    }
}

/// One timed course point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoursePoint {
    pub lon: f64,
    pub lat: f64,
    pub world: (f64, f64),
    pub second: f64,
}

/// The activity's course, split at missing coordinates; single
/// points are left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Course {
    pub segments: Vec<Vec<CoursePoint>>,
}

/// A point on the course: where, and at which activity second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoursePick {
    pub world: (f64, f64),
    pub second: f64,
}

impl Course {
    pub fn of(activity: &ParsedActivity) -> Self {
        let mut segments = Vec::new();
        let mut current: Vec<CoursePoint> = Vec::new();
        for (index, point) in activity.sample_course_points.iter().enumerate() {
            if let ((Some(lat), Some(lon)), Some(&second)) = (point, activity.sample_elapsed_seconds.get(index)) {
                current.push(CoursePoint {
                    lon: *lon,
                    lat: *lat,
                    world: lon_lat_to_world(*lon, *lat),
                    second,
                });
            } else {
                if current.len() > 1 {
                    segments.push(std::mem::take(&mut current));
                }
                current.clear();
            }
        }
        if current.len() > 1 {
            segments.push(current);
        }
        Self { segments }
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// The course's world bounds: `(min, max)`.
    pub fn bounds(&self) -> Option<((f64, f64), (f64, f64))> {
        let mut points = self.segments.iter().flatten();
        let first = points.next()?.world;
        Some(points.fold((first, first), |(min, max), point| {
            (
                (min.0.min(point.world.0), min.1.min(point.world.1)),
                (max.0.max(point.world.0), max.1.max(point.world.1)),
            )
        }))
    }

    /// The course position at `second`, interpolated by time; `None` outside the course
    ///.
    pub fn position_at(&self, second: f64) -> Option<(f64, f64)> {
        for segment in &self.segments {
            for pair in segment.windows(2) {
                let (start, end) = (pair[0], pair[1]);
                if second < start.second || second > end.second {
                    continue;
                }
                let duration = end.second - start.second;
                let progress = if duration == 0.0 {
                    0.0
                } else {
                    (second - start.second) / duration
                };
                return Some(lon_lat_to_world(
                    start.lon + progress * (end.lon - start.lon),
                    start.lat + progress * (end.lat - start.lat),
                ));
            }
        }
        None
    }

    /// The closest course point to `point` within the snap distance,
    /// on the course projected through `camera`.
    pub fn snap(
        projected: &[ProjectedSegment],
        camera: &TopDownCamera,
        size: (f64, f64),
        point: (f64, f64),
    ) -> Option<CoursePick> {
        let mut closest: Option<((f64, f64), f64, f64)> = None;
        for segment in projected {
            for pair in segment.windows(2) {
                let ((sx, sy, start_second), (ex, ey, end_second)) = (pair[0], pair[1]);
                let (dx, dy) = (ex - sx, ey - sy);
                let length_squared = dx * dx + dy * dy;
                let progress = if length_squared == 0.0 {
                    0.0
                } else {
                    (((point.0 - sx) * dx + (point.1 - sy) * dy) / length_squared).clamp(0.0, 1.0)
                };
                let on = (sx + progress * dx, sy + progress * dy);
                let distance = (point.0 - on.0).powi(2) + (point.1 - on.1).powi(2);
                if closest.is_none_or(|(_, best, _)| distance < best) {
                    closest = Some((
                        on,
                        distance,
                        start_second + progress * (end_second - start_second),
                    ));
                }
            }
        }
        let (on, distance, second) = closest?;
        (distance <= SNAP_DISTANCE_PX * SNAP_DISTANCE_PX).then(|| CoursePick {
            world: camera.unproject(on, size),
            second,
        })
    }

    /// The course through `camera`.
    pub fn project(&self, camera: &TopDownCamera, size: (f64, f64)) -> Vec<ProjectedSegment> {
        self.segments
            .iter()
            .map(|segment| {
                segment
                    .iter()
                    .map(|point| {
                        let (x, y) = camera.project(point.world, size);
                        (x, y, point.second)
                    })
                    .collect()
            })
            .collect()
    }
}

/// The camera that fits `bounds` into `size` with the padding, at most at the fit's zoom.
pub fn fit_camera(bounds: ((f64, f64), (f64, f64)), size: (f64, f64)) -> TopDownCamera {
    let ((min_x, min_y), (max_x, max_y)) = bounds;
    let available = (
        (size.0 - 2.0 * FIT_PADDING_PX).max(1.0),
        (size.1 - 2.0 * FIT_PADDING_PX).max(1.0),
    );
    let span = (
        (max_x - min_x) * f64::from(TILE_SIZE),
        (max_y - min_y) * f64::from(TILE_SIZE),
    );
    let fit = |available: f64, span: f64| {
        if span > 0.0 {
            (available / span).log2()
        } else {
            f64::INFINITY
        }
    };
    let zoom = fit(available.0, span.0)
        .min(fit(available.1, span.1))
        .clamp(0.0, FIT_MAX_ZOOM);
    TopDownCamera {
        world: (f64::midpoint(min_x, max_x), f64::midpoint(min_y, max_y)),
        zoom,
    }
}

/// A drag's coast: `(duration, offset)` from the recorded pan steps, or `None` when the pointer was
/// held still before the release.
pub fn drag_inertia(
    steps: &VecDeque<(Instant, (f64, f64))>,
    now: Instant,
) -> Option<(Duration, (f64, f64))> {
    let recent: Vec<&(Instant, (f64, f64))> = steps
        .iter()
        .filter(|(time, _)| now.duration_since(*time) <= INERTIA_CUTOFF)
        .collect();
    let window_start = now.checked_sub(INERTIA_VELOCITY_WINDOW);
    let mut first = recent.len().saturating_sub(2);
    while first > 0 && window_start.is_some_and(|start| recent[first - 1].0 >= start) {
        first -= 1;
    }
    let entries = &recent[first..];
    if entries.len() < 2 {
        return None;
    }
    let pan = entries[1..].iter().fold((0.0, 0.0), |sum, (_, delta)| {
        (sum.0 + delta.0, sum.1 + delta.1)
    });
    let magnitude = pan.0.hypot(pan.1);
    if magnitude == 0.0 {
        return None;
    }
    let seconds = now.duration_since(entries[0].0).as_secs_f64();
    let speed =
        (magnitude * INERTIA_LINEARITY / seconds).clamp(-INERTIA_MAX_SPEED, INERTIA_MAX_SPEED);
    let duration = speed.abs() / (INERTIA_DECELERATION * INERTIA_LINEARITY);
    let amount = speed * duration / 2.0;
    Some((
        Duration::from_secs_f64(duration),
        (pan.0 * amount / magnitude, pan.1 * amount / magnitude),
    ))
}

/// What sent a wheel event: a notched mouse wheel or a trackpad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelDevice {
    Wheel,
    Trackpad,
}

/// The guess from the numbers where the platform does not tell: a notch's exact delta is a wheel,
/// a tiny one a trackpad; otherwise a fast stream of small deltas is a trackpad.
fn guess_device(value: f64, since_last: Option<Duration>) -> WheelDevice {
    if value != 0.0 && value % WHEEL_NOTCH == 0.0 {
        WheelDevice::Wheel
    } else if (value != 0.0 && value.abs() < 4.0)
        || since_last.is_some_and(|gap| (gap.as_secs_f64() * 1000.0 * value).abs() < 200.0)
    {
        WheelDevice::Trackpad
    } else {
        WheelDevice::Wheel
    }
}

/// A run of wheel or trackpad zoom about a point: the deltas between two turns add up, a wheel
/// eases over 200 ms after its last tick and continues the previous ease's speed, a trackpad
/// applies at once, and nothing coasts.
#[derive(Clone, Debug, Default)]
pub struct ScrollZoom {
    device: Option<WheelDevice>,
    delta: f64,
    last_event: Option<Instant>,
    around: (f64, f64),
    target_zoom: Option<f64>,
    start_zoom: Option<f64>,
    easing: Option<CubicBezier>,
    previous_ease: Option<(Instant, Duration, CubicBezier)>,
    active: bool,
    finish_at: Option<Instant>,
}

impl ScrollZoom {
    /// One event: `value` positive zooms out, `device` when the platform
    /// told, `shift` slows it to a quarter.
    pub fn wheel(
        &mut self,
        value: f64,
        device: Option<WheelDevice>,
        shift: bool,
        around: (f64, f64),
        now: Instant,
    ) {
        let since_last = self.last_event.map(|last| now.duration_since(last));
        self.last_event = Some(now);
        let device = device.unwrap_or_else(|| guess_device(value, since_last));
        self.device = Some(device);
        let value = if shift { value / 4.0 } else { value };
        self.delta -= value;
        if self.delta != 0.0 {
            self.around = around;
            self.active = true;
            self.finish_at = None;
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// The next turn's deadline: every frame while easing, then the end of the finish.
    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        if self.active {
            Some(now + FRAME_INTERVAL)
        } else {
            self.finish_at
        }
    }

    /// Stops where it is (a press or a button).
    pub fn stop(&mut self) {
        self.active = false;
        self.delta = 0.0;
        self.target_zoom = None;
        self.start_zoom = None;
        self.finish_at = None;
    }

    fn smooth_out_easing(&mut self, now: Instant) -> CubicBezier {
        let easing = match self.previous_ease {
            Some((start, duration, previous)) => {
                let t = now.duration_since(start).as_secs_f64() / duration.as_secs_f64();
                let speed = previous.at(t + 0.01) - previous.at(t);
                let x = 0.27 / (speed * speed + 0.0001).sqrt() * 0.01;
                let y = (0.27 * 0.27 - x * x).max(0.0).sqrt();
                CubicBezier::new(x, y, 0.25, 1.0)
            }
            None => DEFAULT_EASING,
        };
        self.previous_ease = Some((now, WHEEL_EASE, easing));
        easing
    }

    /// One frame of the run: the zoom to show now, with its anchor, or `None` when idle.
    pub fn frame(&mut self, zoom: f64, maximum: f64, now: Instant) -> Option<(f64, (f64, f64))> {
        if self.finish_at.is_some_and(|at| at <= now) {
            self.finish_at = None;
            self.target_zoom = None;
        }
        if !self.active {
            return None;
        }
        let wheel = self.device == Some(WheelDevice::Wheel);
        if self.delta != 0.0 {
            let rate = if wheel && self.delta.abs() > WHEEL_NOTCH {
                WHEEL_ZOOM_RATE
            } else {
                TRACKPAD_ZOOM_RATE
            };
            let mut scale = MAX_SCALE_PER_FRAME / (1.0 + (-(self.delta * rate).abs()).exp());
            if self.delta < 0.0 && scale != 0.0 {
                scale = 1.0 / scale;
            }
            let from = self.target_zoom.unwrap_or(zoom).exp2();
            self.target_zoom = Some((from * scale).log2().clamp(0.0, maximum));
            if wheel {
                self.start_zoom = Some(zoom);
                self.easing = Some(self.smooth_out_easing(now));
            }
            self.delta = 0.0;
        }
        let target = self.target_zoom.unwrap_or(zoom);
        let (next, finished) = match (wheel, self.start_zoom, self.easing) {
            (true, Some(start), Some(easing)) => {
                let since = self
                    .last_event
                    .map_or(Duration::ZERO, |last| now.duration_since(last))
                    + WHEEL_TIME_ADJUSTMENT;
                let t = (since.as_secs_f64() / WHEEL_EASE.as_secs_f64()).min(1.0);
                (start + (target - start) * easing.at(t), t >= 1.0)
            }
            _ => (target, true),
        };
        if finished {
            self.active = false;
            self.finish_at = Some(now + WHEEL_FINISH);
        }
        Some((next, self.around))
    }
}

/// A running animation of the selection map's camera.
#[derive(Clone, Debug, PartialEq)]
pub enum Animation {
    /// A drag's coast: the offset eased over the duration.
    Coast {
        from: TopDownCamera,
        offset: (f64, f64),
        start: Instant,
        duration: Duration,
    },
    /// An eased zoom about a point (double-click, the buttons).
    Zoom {
        from: TopDownCamera,
        to_zoom: f64,
        around: (f64, f64),
        start: Instant,
        duration: Duration,
    },
}

impl Animation {
    /// The camera at `now`, and whether the animation has ended.
    fn camera_at(&self, size: (f64, f64), now: Instant) -> (TopDownCamera, bool) {
        let progress = |start: Instant, duration: Duration| {
            if duration.is_zero() {
                1.0
            } else {
                (now.duration_since(start).as_secs_f64() / duration.as_secs_f64()).min(1.0)
            }
        };
        match *self {
            Self::Coast {
                from,
                offset,
                start,
                duration,
            } => {
                let t = progress(start, duration);
                let k = INERTIA_EASING.at(t);
                (from.panned((offset.0 * k, offset.1 * k)), t >= 1.0)
            }
            Self::Zoom {
                from,
                to_zoom,
                around,
                start,
                duration,
            } => {
                let t = progress(start, duration);
                let zoom = from.zoom + (to_zoom - from.zoom) * DEFAULT_EASING.at(t);
                (from.zoomed_about(zoom, around, size), t >= 1.0)
            }
        }
    }
}

/// A press on the map: where it started, whether it moved past the click tolerance, and the pan
/// steps the coast is measured from.
#[derive(Clone, Debug, PartialEq)]
struct Press {
    origin: (f64, f64),
    last: (f64, f64),
    dragging: bool,
    steps: VecDeque<(Instant, (f64, f64))>,
}

/// The size and camera a frame was drawn for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FramedView {
    pub camera: TopDownCamera,
    pub size: (f64, f64),
}

/// The selection map's image: a frame with the course stroked in, and the view it was drawn for.
#[derive(Clone)]
pub struct SelectionImage {
    pub pixels: MapFramePixels,
    pub view: FramedView,
    /// The style it was drawn in.
    pub style: Option<ovrley_map::MapStyleId>,
    pub revision: u64,
}

/// The selection map.
pub struct SelectionMap {
    /// The camera on show, which follows every pointer move at once.
    pub camera: TopDownCamera,
    /// The settled screen size, in logical pixels, and the scale factor.
    pub size: Option<(f64, f64)>,
    pub scale_factor: f64,
    /// A new size waiting for the settle.
    pending_size: Option<((f64, f64), Instant)>,
    fitted: bool,
    pub stream: Option<MapFrameStream>,
    /// The view of the frame being drawn.
    in_flight: Option<FramedView>,
    pub image: Option<SelectionImage>,
    revision: u64,
    last_request: Option<Instant>,
    press: Option<Press>,
    pub animation: Option<Animation>,
    pub scroll: ScrollZoom,
    /// The course projected through the camera on show, cached per camera and size.
    projected: Option<(FramedView, Vec<ProjectedSegment>)>,
    /// Within snap distance of the pointer: the dot that follows the course.
    pub hover: Option<CoursePick>,
    /// The clicked course point that "Set location" sets.
    pub action: Option<CoursePick>,
    /// The location pin's popup with its delete button.
    pub popup_open: bool,
    /// Frame requests sent, for the performance tests and the review's debug counter.
    pub requests_sent: u64,
    /// The style the map draws; an image in another style is drawn again.
    pub style: Option<ovrley_map::MapStyleId>,
    /// The style of the frame in flight.
    in_flight_style: Option<ovrley_map::MapStyleId>,
}

impl Default for SelectionMap {
    fn default() -> Self {
        Self {
            camera: TopDownCamera::at_lon_lat(INITIAL_CENTER.0, INITIAL_CENTER.1, INITIAL_ZOOM),
            size: None,
            scale_factor: 1.0,
            pending_size: None,
            fitted: false,
            stream: None,
            in_flight: None,
            image: None,
            revision: 0,
            last_request: None,
            press: None,
            animation: None,
            scroll: ScrollZoom::default(),
            projected: None,
            hover: None,
            action: None,
            popup_open: false,
            requests_sent: 0,
            style: None,
            in_flight_style: None,
        }
    }
}

impl SelectionMap {
    /// The screen measured `size` (logical pixels): the first size applies at once, later ones once
    /// they have held for the settle. Centre and zoom are kept.
    pub fn set_size(&mut self, size: (f64, f64), scale_factor: f64, now: Instant) {
        if size.0 <= 0.0 || size.1 <= 0.0 {
            return;
        }
        self.scale_factor = scale_factor;
        if self.size.is_none() {
            self.size = Some(size);
            return;
        }
        if self.size == Some(size) {
            self.pending_size = None;
            return;
        }
        if self.pending_size.map(|(pending, _)| pending) != Some(size) {
            self.pending_size = Some((size, now + RESIZE_SETTLE));
        }
    }

    /// A new course: it fits again once there is a size, and the hover and action point go.
    pub fn reset_course(&mut self) {
        self.fitted = false;
        self.hover = None;
        self.action = None;
        self.projected = None;
    }

    fn fit(&mut self, course: &Course) {
        if self.fitted || course.is_empty() {
            return;
        }
        let (Some(size), Some(bounds)) = (self.size, course.bounds()) else {
            return;
        };
        self.camera = fit_camera(bounds, size);
        self.fitted = true;
    }

    /// The course through the camera on show.
    fn projected(&mut self, course: &Course) -> Option<&[ProjectedSegment]> {
        let view = FramedView {
            camera: self.camera,
            size: self.size?,
        };
        if self
            .projected
            .as_ref()
            .is_none_or(|(cached, _)| *cached != view)
        {
            self.projected = Some((view, course.project(&view.camera, view.size)));
        }
        self.projected.as_ref().map(|(_, points)| points.as_slice())
    }

    fn snap(&mut self, course: &Course, point: (f64, f64)) -> Option<CoursePick> {
        let (camera, size) = (self.camera, self.size?);
        let projected = self.projected(course)?;
        Course::snap(projected, &camera, size, point)
    }

    /// Stops a running animation where it is.
    fn stop_animations(&mut self) {
        self.animation = None;
        self.scroll.stop();
    }

    pub fn pointer_down(&mut self, point: (f64, f64), now: Instant) {
        self.stop_animations();
        self.press = Some(Press {
            origin: point,
            last: point,
            dragging: false,
            steps: VecDeque::from([(now, (0.0, 0.0))]),
        });
    }

    /// A move: with a press past the click tolerance it pans; without one it updates the hover.
    pub fn pointer_move(&mut self, course: &Course, point: (f64, f64), now: Instant) {
        let Some(press) = &mut self.press else {
            self.hover = self.snap(course, point);
            return;
        };
        if !press.dragging
            && (point.0 - press.origin.0).hypot(point.1 - press.origin.1) < CLICK_TOLERANCE_PX
        {
            return;
        }
        let delta = if press.dragging {
            (point.0 - press.last.0, point.1 - press.last.1)
        } else {
            (point.0 - press.origin.0, point.1 - press.origin.1)
        };
        press.dragging = true;
        press.last = point;
        while press
            .steps
            .front()
            .is_some_and(|(time, _)| now.duration_since(*time) > INERTIA_CUTOFF)
        {
            press.steps.pop_front();
        }
        press.steps.push_back((now, delta));
        self.camera = self.camera.panned(delta);
        self.hover = None;
    }

    /// The release: a drag coasts; a click picks the course point under it (or clears the pick)
    /// and closes the pin's popup.
    pub fn pointer_up(&mut self, course: &Course, point: (f64, f64), now: Instant) {
        let Some(press) = self.press.take() else {
            return;
        };
        if press.dragging {
            if let Some((duration, offset)) = drag_inertia(&press.steps, now) {
                self.animation = Some(Animation::Coast {
                    from: self.camera,
                    offset,
                    start: now,
                    duration,
                });
            }
            return;
        }
        self.popup_open = false;
        self.action = self.snap(course, point);
    }

    pub fn pointer_leave(&mut self) {
        if self.press.is_none() {
            self.hover = None;
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.press.as_ref().is_some_and(|press| press.dragging)
    }

    /// A wheel or trackpad event over the map, zooming about the pointer.
    pub fn wheel(
        &mut self,
        value: f64,
        device: Option<WheelDevice>,
        shift: bool,
        point: (f64, f64),
        now: Instant,
    ) {
        self.animation = None;
        self.scroll.wheel(value, device, shift, point, now);
    }

    /// Double-click: one level in about the point (Shift: out), eased over 300 ms.
    pub fn double_click(&mut self, point: (f64, f64), shift: bool, now: Instant) {
        self.stop_animations();
        let to_zoom = (self.camera.zoom + if shift { -1.0 } else { 1.0 }).clamp(0.0, MAX_ZOOM);
        self.animation = Some(Animation::Zoom {
            from: self.camera,
            to_zoom,
            around: point,
            start: now,
            duration: DOUBLE_CLICK_EASE,
        });
    }

    /// The +/- buttons: one level about the centre, eased over 500 ms.
    pub fn zoom_button(&mut self, zoom_in: bool, now: Instant) {
        let Some(size) = self.size else { return };
        self.stop_animations();
        let to_zoom = (self.camera.zoom + if zoom_in { 1.0 } else { -1.0 }).clamp(0.0, MAX_ZOOM);
        self.animation = Some(Animation::Zoom {
            from: self.camera,
            to_zoom,
            around: (size.0 / 2.0, size.1 / 2.0),
            start: now,
            duration: BUTTON_EASE,
        });
    }

    pub fn can_zoom_in(&self) -> bool {
        self.camera.zoom < MAX_ZOOM
    }

    pub fn can_zoom_out(&self) -> bool {
        self.camera.zoom > 0.0
    }

    /// Advances the settle and the animations to `now`.
    pub fn advance(&mut self, course: &Course, now: Instant) {
        if let Some((size, at)) = self.pending_size {
            if at <= now {
                self.size = Some(size);
                self.pending_size = None;
            }
        }
        self.fit(course);
        let Some(size) = self.size else { return };
        if let Some(animation) = &self.animation {
            let (camera, ended) = animation.camera_at(size, now);
            self.camera = camera;
            if ended {
                self.animation = None;
            }
        }
        if let Some((zoom, around)) = self.scroll.frame(self.camera.zoom, MAX_ZOOM, now) {
            self.camera = self.camera.zoomed_about(zoom, around, size);
        }
    }

    /// The frame viewport for `size`: the screen at the window's density, scaled down so neither
    /// side passes the canvas limit.
    fn viewport(&self, size: (f64, f64)) -> Result<MapViewport, String> {
        let ratio =
            self.scale_factor * (MAX_CANVAS_PX / (size.0.max(size.1) * self.scale_factor)).min(1.0);
        MapViewport::new(
            (size.0 * ratio).round().max(1.0) as u32,
            (size.1 * ratio).round().max(1.0) as u32,
            ratio as f32,
        )
    }

    fn request_for(
        &self,
        spec: &MapStyleSpec,
        view: FramedView,
    ) -> Result<MapFrameRequest, String> {
        let camera = view.camera.map_camera()?;
        Ok(MapFrameRequest {
            spec: spec.clone(),
            camera,
            viewport: self.viewport(view.size)?,
            route: None,
            labels: Some(MapLabelDraw::new([camera; LABEL_PLACEMENTS], 1.0)?),
            corner_radius_px: 0.0,
        })
    }

    /// Collects an arrived frame and strokes the course into it once.
    pub fn collect(&mut self, course: &Course) -> Result<bool, String> {
        let Some(stream) = &mut self.stream else {
            return Ok(false);
        };
        if !stream.collect() {
            return Ok(false);
        }
        let view = self.in_flight.take();
        let (Some(frame), Some(view)) = (stream.shown(), view) else {
            return Ok(true);
        };
        let ratio = f64::from(frame.image().width()) / view.size.0;
        let lines: Vec<Vec<(f32, f32)>> = course
            .project(&view.camera, view.size)
            .into_iter()
            .map(|segment| {
                segment
                    .into_iter()
                    .map(|(x, y, _)| ((x * ratio) as f32, (y * ratio) as f32))
                    .collect()
            })
            .collect();
        let pixels = ovrley_widgets::widgets::map::layout::stroke_lines_over_frame(
            frame,
            &lines,
            COURSE_COLOR,
            (COURSE_WIDTH_PX * ratio) as f32,
        )
        .map_err(|error| error.to_string())?;
        self.revision += 1;
        self.image = Some(SelectionImage {
            pixels,
            view,
            style: self.in_flight_style.take(),
            revision: self.revision,
        });
        Ok(true)
    }

    /// Whether the image on show is `view` in the map's style.
    fn is_current(&self, view: FramedView) -> bool {
        self.image.as_ref().is_some_and(|image| image.view == view && image.style == self.style)
    }

    /// The view to draw: the camera on show at the settled size.
    fn wanted(&self) -> Option<FramedView> {
        Some(FramedView {
            camera: self.camera,
            size: self.size?,
        })
    }

    /// Asks for the wanted view when it differs from the frame on show and the one in flight, none
    /// is being drawn, and the throttle allows.
    pub fn send_request(&mut self, spec: &MapStyleSpec, now: Instant) -> Result<(), String> {
        let Some(view) = self.wanted() else {
            return Ok(());
        };
        let Some(stream) = &self.stream else {
            return Ok(());
        };
        if stream.is_drawing() || self.is_current(view) {
            return Ok(());
        }
        if self
            .last_request
            .is_some_and(|last| now.duration_since(last) < FRAME_INTERVAL)
        {
            return Ok(());
        }
        let request = self.request_for(spec, view)?;
        let stream = self.stream.as_mut().expect("checked above");
        if stream.answers(&request) {
            return Ok(());
        }
        stream
            .request(&request)
            .map_err(|error| error.to_string())?;
        if stream.is_drawing() {
            self.in_flight = Some(view);
            self.in_flight_style = Some(spec.style.clone());
            self.last_request = Some(now);
            self.requests_sent += 1;
        }
        Ok(())
    }

    /// When the selection map next needs a turn: an animation's frame, the settle, or the
    /// throttle's end while a newer view waits.
    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        let animation = self.animation.as_ref().map(|_| now + FRAME_INTERVAL);
        let settle = self.pending_size.map(|(_, at)| at);
        let scroll = self.scroll.deadline(now);
        let waiting = self
            .wanted()
            .filter(|view| !self.is_current(*view))
            .filter(|_| {
                self.stream
                    .as_ref()
                    .is_some_and(|stream| !stream.is_drawing())
            });
        let throttle = waiting
            .and(self.last_request)
            .map(|last| last + FRAME_INTERVAL);
        [animation, settle, scroll, throttle]
            .into_iter()
            .flatten()
            .min()
    }

    /// The image's placement on screen: where the framed view's top left lands under the camera on
    /// show, and its displayed size.
    pub fn image_placement(&self) -> Option<ScreenRect> {
        let image = self.image.as_ref()?;
        let size = self.size?;
        let scale = (self.camera.zoom - image.view.camera.zoom).exp2();
        let centre = self.camera.project(image.view.camera.world, size);
        let shown = (image.view.size.0 * scale, image.view.size.1 * scale);
        Some(((centre.0 - shown.0 / 2.0, centre.1 - shown.1 / 2.0), shown))
    }

    /// A world point on screen under the camera on show.
    pub fn screen_point(&self, world: (f64, f64)) -> Option<(f64, f64)> {
        Some(self.camera.project(world, self.size?))
    }

    /// The location pin dropped at `point`: the course point within snap distance, if any.
    pub fn pin_drop(&mut self, course: &Course, point: (f64, f64)) -> Option<CoursePick> {
        self.snap(course, point)
    }
}

/// The inset's zoom and pitch ranges and its starting values.
pub const NAVIGATION_ZOOM: f64 = 15.5;
pub const NAVIGATION_MAX_ZOOM: f64 = 16.5;
pub const NAVIGATION_PITCH: f64 = 40.0;
pub const NAVIGATION_MAX_PITCH: f64 = 60.0;
/// The inset's route: orange on both sides of the split, 6 px.
const NAVIGATION_ROUTE_COLOR: [f32; 4] = [
    0xEF as f32 / 255.0,
    0x6C as f32 / 255.0,
    0x15 as f32 / 255.0,
    1.0,
];
const NAVIGATION_ROUTE_WIDTH_PX: f32 = 6.0;

/// The chase inset: the tile-map widget's chase camera, heading up, over the activity's track.
pub struct NavigationMap {
    pub zoom: f64,
    pub pitch: f64,
    /// The screen side in logical pixels, and the scale factor.
    pub side: Option<f64>,
    pub scale_factor: f64,
    pub stream: Option<MapFrameStream>,
    /// The route line, kept per whole zoom.
    line: Option<(f64, Arc<MapRouteLine>)>,
    pub scroll: ScrollZoom,
    /// An eased button zoom: `(from, to, start)`.
    zoom_ease: Option<(f64, f64, Instant)>,
    /// When the last request went out; requests are at least a frame interval apart, so a frame
    /// arriving does not ask for the next one at once and starve the UI thread.
    last_request: Option<Instant>,
    /// A newer request is waiting for the interval.
    waiting: bool,
    pub requests_sent: u64,
}

impl Default for NavigationMap {
    fn default() -> Self {
        Self {
            zoom: NAVIGATION_ZOOM,
            pitch: NAVIGATION_PITCH,
            side: None,
            scale_factor: 1.0,
            stream: None,
            line: None,
            scroll: ScrollZoom::default(),
            zoom_ease: None,
            last_request: None,
            waiting: false,
            requests_sent: 0,
        }
    }
}

impl NavigationMap {
    pub fn wheel(&mut self, value: f64, device: Option<WheelDevice>, shift: bool, now: Instant) {
        self.zoom_ease = None;
        self.scroll.wheel(value, device, shift, (0.0, 0.0), now);
    }

    pub fn zoom_button(&mut self, zoom_in: bool, now: Instant) {
        self.scroll.stop();
        let to = (self.zoom + if zoom_in { 1.0 } else { -1.0 }).clamp(0.0, NAVIGATION_MAX_ZOOM);
        self.zoom_ease = Some((self.zoom, to, now));
    }

    pub fn set_pitch(&mut self, pitch: f64) {
        self.pitch = pitch.clamp(0.0, NAVIGATION_MAX_PITCH);
    }

    pub fn advance(&mut self, now: Instant) {
        if let Some((from, to, start)) = self.zoom_ease {
            let t = (now.duration_since(start).as_secs_f64() / BUTTON_EASE.as_secs_f64()).min(1.0);
            self.zoom = from + (to - from) * DEFAULT_EASING.at(t);
            if t >= 1.0 {
                self.zoom_ease = None;
            }
        }
        if let Some((zoom, _)) = self.scroll.frame(self.zoom, NAVIGATION_MAX_ZOOM, now) {
            self.zoom = zoom;
        }
    }

    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        let ease = self.zoom_ease.map(|_| now + FRAME_INTERVAL);
        let throttle = self.last_request.filter(|_| self.waiting).map(|last| last + FRAME_INTERVAL);
        [ease, self.scroll.deadline(now), throttle]
            .into_iter()
            .flatten()
            .min()
    }

    /// The frame for `second`: the chase camera at the track's position, its labels on the
    /// camera's grid and the route to the renderer. `None` without a size.
    pub fn request(
        &mut self,
        spec: &MapStyleSpec,
        track: &MapTrack,
        second: f64,
    ) -> Result<Option<MapFrameRequest>, String> {
        use ovrley_widgets::widgets::map::WidgetCamera;
        use ovrley_widgets::widgets::map::MapOrientation;
        let Some(side) = self.side else {
            return Ok(None);
        };
        let camera = WidgetCamera::Follow {
            zoom: self.zoom,
            pitch_deg: self.pitch,
            orientation: MapOrientation::RiderUp,
        };
        let position = track.position_at(second);
        let map_camera = camera
            .at(track, &position)
            .map_err(|error| error.to_string())?;
        let labels = camera
            .label_draw(track, second)
            .map_err(|error| error.to_string())?;
        let line_zoom = self.zoom.ceil();
        let line = match &self.line {
            Some((zoom, line)) if *zoom == line_zoom => line.clone(),
            _ => {
                let line = Arc::new(
                    ovrley_widgets::widgets::map::track::route_line(track, line_zoom)
                        .map_err(|error| error.to_string())?,
                );
                self.line = Some((line_zoom, line.clone()));
                line
            }
        };
        let ratio = self.scale_factor as f32;
        let px = (side * self.scale_factor).round().max(1.0) as u32;
        Ok(Some(MapFrameRequest {
            spec: spec.clone(),
            camera: map_camera,
            viewport: MapViewport::new(px, px, ratio)?,
            route: Some(MapRouteDraw {
                line,
                style: MapRouteStyle {
                    completed_color: NAVIGATION_ROUTE_COLOR,
                    remaining_color: NAVIGATION_ROUTE_COLOR,
                    width_px: NAVIGATION_ROUTE_WIDTH_PX * ratio,
                },
                travelled_metres: position.distance_meters as f32,
            }),
            labels: Some(labels),
            corner_radius_px: 0.0,
        }))
    }

    /// Asks for `request`, the newest wins: one frame is drawn at a time, the last stays on show,
    /// and requests are at least a frame interval apart.
    pub fn send(&mut self, request: &MapFrameRequest, now: Instant) -> Result<(), String> {
        let Some(stream) = &mut self.stream else {
            return Ok(());
        };
        if stream.answers(request) {
            self.waiting = false;
            return Ok(());
        }
        if stream.is_drawing() || self.last_request.is_some_and(|last| now.duration_since(last) < FRAME_INTERVAL) {
            self.waiting = true;
            return Ok(());
        }
        stream.request(request).map_err(|error| error.to_string())?;
        self.waiting = false;
        self.last_request = Some(now);
        self.requests_sent += 1;
        Ok(())
    }
}

/// The style both maps draw: buildings, labels and paths on, international names.
pub fn sync_style_spec(style: &MapStyleInfo) -> MapStyleSpec {
    MapStyleSpec {
        style: style.id.clone(),
        layers: MapLayers {
            buildings: true,
            labels: true,
            paths: true,
            local_names: false,
        },
    }
}

/// The detected location's place on the course.
pub fn location_point(course: &Course, location: Option<LocationEvent>) -> Option<(f64, f64)> {
    course.position_at(location?.time)
}

pub const MAP_STYLE_PREFERENCE_KEY: &str = "sync-map-style";

/// The activity's course and track, built once per activity.
pub struct SyncCourse {
    activity: Arc<ParsedActivity>,
    pub course: Arc<Course>,
    /// The chase inset's track, as the map widget builds its own; `None` without GPS.
    pub track: Option<Arc<MapTrack>>,
}

/// Both maps, present while Sync Doctor is open.
pub struct SyncMaps {
    pub course: Option<SyncCourse>,
    /// The style id chosen in the selection map's select, or `None` for the first installed one.
    pub chosen_style: Option<String>,
    /// The style the streams draw.
    style: Option<MapStyleInfo>,
    pub selection: SelectionMap,
    pub navigation: NavigationMap,
}

impl std::fmt::Debug for SyncMaps {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("SyncMaps").field("camera", &self.selection.camera).field("zoom", &self.navigation.zoom).finish()
    }
}

/// The saved style preference: absent is the first style; a value that is not a style id fails
/// loudly, and the maps draw the first style meanwhile.
fn read_style_preference(store: &mut crate::store::Store, services: &crate::store::services::Services) -> Option<String> {
    match services.preferences.get(MAP_STYLE_PREFERENCE_KEY) {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(id)) => Some(id.clone()),
        Some(_) => {
            store.media.set_error_message(format!("Preference \"{MAP_STYLE_PREFERENCE_KEY}\" must be a map style id"));
            None
        }
    }
}

/// The map widget's default for `key`, from the compiled catalog.
fn map_default(key: &str) -> f64 {
    ovrley_widgets::model::catalog::catalog().require_widget("map").expect("the catalog has the map widget").defaults.get(key).and_then(serde_json::Value::as_f64).unwrap_or_else(|| panic!("the map widget has a numeric {key} default"))
}

fn build_course(store: &crate::store::Store) -> Option<SyncCourse> {
    let imported = store.media.parsed_activity.as_ref()?;
    let track = ovrley_widgets::activity::ActivitySampler::from_json(&imported.preview_bytes).ok().and_then(|sampler| sampler.map_track(map_default("route_smoothing_m"))).map(Arc::new);
    Some(SyncCourse { activity: imported.activity.clone(), course: Arc::new(Course::of(&imported.activity)), track })
}

/// The maps' part of the `video_sync` reconcile step: opens or drops them with the mode, rebuilds
/// the course for a new activity, collects arrived frames and sends each map's request.
pub fn reconcile_sync_maps(store: &mut crate::store::Store, services: &mut crate::store::services::Services, now: Instant) {
    if !crate::features::player::video_sync_mode(store) {
        store.video_sync.maps = None;
        return;
    }
    if store.video_sync.maps.is_none() {
        let chosen_style = read_style_preference(store, services);
        store.video_sync.maps = Some(SyncMaps { course: None, chosen_style, style: None, selection: SelectionMap::default(), navigation: NavigationMap::default() });
    }
    // A new activity: a new course, fitted again.
    let activity = store.media.parsed_activity.as_ref().map(|imported| imported.activity.clone());
    let maps = store.video_sync.maps.as_ref().expect("opened above");
    let stale = match (&maps.course, &activity) {
        (Some(course), Some(activity)) => !Arc::ptr_eq(&course.activity, activity),
        (None, None) => false,
        _ => true,
    };
    if stale {
        let course = build_course(store);
        let maps = store.video_sync.maps.as_mut().expect("opened above");
        maps.course = course;
        maps.selection.reset_course();
    }
    // The screens' sizes come from the editor's layout.
    let (scene_width, scene_height) = crate::features::canvas::scene_size(store);
    let scale = crate::features::canvas::display_scale(store);
    let scale_factor = store.canvas.viewport.metrics.scale_factor.max(1.0);
    let screen = (scene_width * scale, scene_height * scale);
    // The style both maps draw, and the streams on its renderer.
    let chosen = store.video_sync.maps.as_ref().and_then(|maps| maps.chosen_style.clone());
    let (_, style_id) = crate::features::maps::map_style_select(store, chosen.as_deref());
    let style = store.maps.styles.as_ref().filter(|_| !style_id.is_empty()).map(|styles| styles.resolve(&ovrley_map::MapStyleId::new(&style_id).expect("an installed style id")).clone());
    let factory = match style {
        Some(_) => match crate::features::maps::frame_factory(store, services) {
            Ok(factory) => Some(factory),
            Err(error) => {
                log::error!("Sync Doctor maps: {error}");
                None
            }
        },
        None => None,
    };
    let second = store.editor.selected_second;
    let maps = store.video_sync.maps.as_mut().expect("opened above");
    if let (Some(style), Some(factory)) = (&style, &factory) {
        if maps.style.as_ref() != Some(style) {
            let samples = map_default("msaa_samples") as u32;
            for stream in [&mut maps.selection.stream, &mut maps.navigation.stream] {
                match stream {
                    Some(stream) => stream.replace_provider(factory.provider(style, samples)),
                    None => *stream = Some(MapFrameStream::streamed(factory.provider(style, samples))),
                }
            }
            maps.style = Some(style.clone());
            maps.selection.style = Some(style.id.clone());
        }
    }
    let Some(spec) = maps.style.as_ref().map(sync_style_spec) else { return };
    let empty = Course::default();
    let course = maps.course.as_ref().map_or(&empty, |course| course.course.as_ref());
    let selection = &mut maps.selection;
    selection.set_size(screen, scale_factor, now);
    selection.advance(course, now);
    if let Err(error) = selection.collect(course).and_then(|_| selection.send_request(&spec, now)) {
        log::error!("Sync Doctor selection map: {error}");
    }
    let navigation = &mut maps.navigation;
    navigation.scale_factor = scale_factor;
    navigation.side = Some(if screen.0 < screen.1 { screen.1 } else { screen.0 } * 0.25);
    navigation.advance(now);
    if let Some(stream) = &mut navigation.stream {
        stream.collect();
    }
    // Outside the course the inset keeps its last view.
    if let Some(track) = maps.course.as_ref().filter(|course| course.course.position_at(second).is_some()).and_then(|course| course.track.clone()) {
        match maps.navigation.request(&spec, &track, second) {
            Ok(Some(request)) => {
                if let Err(error) = maps.navigation.send(&request, now) {
                    log::error!("Sync Doctor navigation map: {error}");
                }
            }
            Ok(None) => {}
            Err(error) => log::error!("Sync Doctor navigation map: {error}"),
        }
    }
}

/// When the sync maps next need a turn of the event loop.
pub fn next_deadline(store: &crate::store::Store, now: Instant) -> Option<Instant> {
    let maps = store.video_sync.maps.as_ref()?;
    [maps.selection.deadline(now), maps.navigation.deadline(now)].into_iter().flatten().min()
}

// --- Actions --------------------------------------------------------------------------------

/// The open maps and the course they share.
fn open_maps(store: &mut crate::store::Store) -> Option<(&mut SyncMaps, Arc<Course>)> {
    let maps = store.video_sync.maps.as_mut()?;
    let course = maps.course.as_ref().map_or_else(|| Arc::new(Course::default()), |course| course.course.clone());
    Some((maps, course))
}

/// A pointer event on the selection map: `down`, `move`, `up` or `leave`.
pub fn map_pointer(store: &mut crate::store::Store, phase: &str, point: (f64, f64), now: Instant) {
    let Some((maps, course)) = open_maps(store) else { return };
    let selection = &mut maps.selection;
    match phase {
        "down" => selection.pointer_down(point, now),
        "move" => selection.pointer_move(&course, point, now),
        "up" => selection.pointer_up(&course, point, now),
        _ => selection.pointer_leave(),
    }
}

/// A wheel event over the selection map; `value` positive zooms out.
pub fn map_wheel(store: &mut crate::store::Store, value: f64, device: Option<WheelDevice>, shift: bool, point: (f64, f64), now: Instant) {
    if let Some((maps, _)) = open_maps(store) {
        maps.selection.wheel(value, device, shift, point, now);
    }
}

pub fn map_double_click(store: &mut crate::store::Store, point: (f64, f64), shift: bool, now: Instant) {
    if let Some((maps, _)) = open_maps(store) {
        maps.selection.double_click(point, shift, now);
    }
}

pub fn map_zoom(store: &mut crate::store::Store, zoom_in: bool, now: Instant) {
    if let Some((maps, _)) = open_maps(store) {
        maps.selection.zoom_button(zoom_in, now);
    }
}

/// "Set location": the picked point's second becomes the detected location.
pub fn set_location_from_action(store: &mut crate::store::Store) -> Result<(), String> {
    let action = store.video_sync.maps.as_mut().and_then(|maps| maps.selection.action.take()).ok_or("A course point must be selected before setting the detected location")?;
    crate::features::sync::manual::set_detected_location(store, action.second)
}

/// The pin released at `point`: near the course it moves there, farther away it returns.
pub fn pin_dropped(store: &mut crate::store::Store, point: (f64, f64)) -> Result<(), String> {
    let Some((maps, course)) = open_maps(store) else { return Ok(()) };
    match maps.selection.pin_drop(&course, point) {
        Some(pick) => crate::features::sync::manual::set_detected_location(store, pick.second),
        None => Ok(()),
    }
}

pub fn toggle_popup(store: &mut crate::store::Store) {
    if let Some((maps, _)) = open_maps(store) {
        maps.selection.popup_open = !maps.selection.popup_open;
    }
}

/// The popup's `×`: the detected location goes.
pub fn delete_location(store: &mut crate::store::Store) {
    if let Some((maps, _)) = open_maps(store) {
        maps.selection.popup_open = false;
    }
    crate::features::sync::manual::clear_detected_location(store);
}

/// The style select: both maps draw `id`, saved as the preference.
pub fn set_map_style(store: &mut crate::store::Store, services: &mut crate::store::services::Services, id: String) {
    services.preferences.set(MAP_STYLE_PREFERENCE_KEY, serde_json::Value::String(id.clone()));
    if let Some((maps, _)) = open_maps(store) {
        maps.chosen_style = Some(id);
    }
}

pub fn nav_wheel(store: &mut crate::store::Store, value: f64, device: Option<WheelDevice>, shift: bool, now: Instant) {
    if let Some((maps, _)) = open_maps(store) {
        maps.navigation.wheel(value, device, shift, now);
    }
}

pub fn nav_zoom(store: &mut crate::store::Store, zoom_in: bool, now: Instant) {
    if let Some((maps, _)) = open_maps(store) {
        maps.navigation.zoom_button(zoom_in, now);
    }
}

pub fn nav_pitch(store: &mut crate::store::Store, pitch: f64) {
    if let Some((maps, _)) = open_maps(store) {
        maps.navigation.set_pitch(pitch);
    }
}

// --- The view ---------------------------------------------------------------------------------

/// What the screens show of the maps.
pub struct MapView<'a> {
    /// The selection image with its placement.
    pub image: Option<(&'a SelectionImage, ScreenRect)>,
    pub background: [u8; 3],
    pub playback: Option<(f64, f64)>,
    pub hover: Option<(f64, f64)>,
    pub action: Option<(f64, f64)>,
    pub pin: Option<(f64, f64)>,
    pub popup_open: bool,
    pub dragging: bool,
    pub can_zoom_in: bool,
    pub can_zoom_out: bool,
    /// The inset's frame with its serial, and whether its marker shows.
    pub navigation_frame: Option<(&'a MapFramePixels, u64)>,
    pub navigation_marker: bool,
    pub navigation_pitch: f64,
    pub navigation_zoom: f64,
}

pub fn map_view(store: &crate::store::Store) -> Option<MapView<'_>> {
    let maps = store.video_sync.maps.as_ref()?;
    let selection = &maps.selection;
    let empty = Course::default();
    let course = maps.course.as_ref().map_or(&empty, |course| course.course.as_ref());
    let second = store.editor.selected_second;
    let location = store.manual_video_sync_detection.as_ref().and_then(|detection| detection.location);
    let at = |world: Option<(f64, f64)>| world.and_then(|world| selection.screen_point(world));
    Some(MapView {
        image: selection.image.as_ref().zip(selection.image_placement()),
        background: maps.style.as_ref().map_or([0, 0, 0], |style| style.background),
        playback: at(course.position_at(second)),
        hover: at(selection.hover.map(|pick| pick.world)),
        action: at(selection.action.map(|pick| pick.world)),
        pin: at(location_point(course, location)),
        popup_open: selection.popup_open,
        dragging: selection.is_dragging(),
        can_zoom_in: selection.can_zoom_in(),
        can_zoom_out: selection.can_zoom_out(),
        navigation_frame: maps.navigation.stream.as_ref().and_then(|stream| stream.shown().map(|frame| (frame, stream.serial()))),
        navigation_marker: maps.course.as_ref().is_some_and(|course| course.track.is_some() && course.course.position_at(second).is_some()),
        navigation_pitch: maps.navigation.pitch,
        navigation_zoom: maps.navigation.zoom,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ovrley_widgets::WidgetResult;
    use ovrley_widgets::widgets::map::preview::{MapFrameProvider, MapFrameReply};
    use std::sync::mpsc::{channel, Sender};
    use std::sync::Mutex;

    /// Counts requests and holds each reply's sender, so a test answers them when it likes.
    /// A frame request with the channel its reply goes to.
    type PendingFrame = (MapFrameRequest, Sender<Result<MapFramePixels, String>>);

    #[derive(Default)]
    struct FakeProvider {
        requests: Mutex<Vec<PendingFrame>>,
    }

    impl MapFrameProvider for FakeProvider {
        fn frame(&self, request: &MapFrameRequest, _: Option<Vec<u8>>) -> WidgetResult<MapFrameReply> {
            let (sender, receiver) = channel();
            self.requests.lock().unwrap().push((request.clone(), sender));
            Ok(receiver)
        }
    }

    impl FakeProvider {
        fn count(&self) -> usize {
            self.requests.lock().unwrap().len()
        }

        /// Answers the newest request with a blank frame of its size.
        fn answer(&self) {
            let requests = self.requests.lock().unwrap();
            let (request, sender) = requests.last().unwrap();
            let (width, height) = (request.viewport.width_px(), request.viewport.height_px());
            sender.send(Ok(MapFramePixels::new(width, height, vec![0; (width * height * 4) as usize]).unwrap())).unwrap();
        }
    }

    fn spec() -> MapStyleSpec {
        MapStyleSpec { style: ovrley_map::MapStyleId::new("liberty").unwrap(), layers: MapLayers { buildings: true, labels: true, paths: true, local_names: false } }
    }

    fn course() -> Course {
        let point = |second: f64, lon: f64| CoursePoint { lon, lat: 47.0, world: lon_lat_to_world(lon, 47.0), second };
        Course { segments: vec![vec![point(0.0, 8.0), point(10.0, 8.01), point(20.0, 8.02)]] }
    }

    /// A selection map at 400×300 with a fake stream, the course fitted.
    fn map(now: Instant) -> (SelectionMap, Arc<FakeProvider>) {
        let provider = Arc::new(FakeProvider::default());
        let mut map = SelectionMap { stream: Some(MapFrameStream::streamed(provider.clone())), style: Some(spec().style), ..SelectionMap::default() };
        map.set_size((400.0, 300.0), 2.0, now);
        map.advance(&course(), now);
        (map, provider)
    }

    fn turn(map: &mut SelectionMap, now: Instant) {
        map.advance(&course(), now);
        map.collect(&course()).unwrap();
        map.send_request(&spec(), now).unwrap();
    }

    #[test]
    fn the_top_down_camera_inverts_and_zooms_about_a_point() {
        let camera = TopDownCamera::at_lon_lat(INITIAL_CENTER.0, INITIAL_CENTER.1, INITIAL_ZOOM);
        let size = (400.0, 300.0);
        let world = camera.unproject((123.0, 45.0), size);
        let back = camera.project(world, size);
        assert!((back.0 - 123.0).abs() < 1e-6 && (back.1 - 45.0).abs() < 1e-6);
        let zoomed = camera.zoomed_about(14.0, (123.0, 45.0), size);
        let still = zoomed.project(world, size);
        assert!((still.0 - 123.0).abs() < 1e-6 && (still.1 - 45.0).abs() < 1e-6, "the anchor stays under the pointer");
        let panned = camera.panned((10.0, -5.0));
        let moved = panned.project(camera.world, size);
        assert!((moved.0 - 210.0).abs() < 1e-6 && (moved.1 - 145.0).abs() < 1e-6);
    }

    #[test]
    fn starts_at_zurich_and_fits_a_course_with_padding_and_the_zoom_cap() {
        assert_eq!(SelectionMap::default().camera, TopDownCamera::at_lon_lat(8.5417, 47.3769, 13.0));
        let (map, _) = map(Instant::now());
        let bounds = course().bounds().unwrap();
        let left = map.camera.project(bounds.0, (400.0, 300.0));
        let right = map.camera.project(bounds.1, (400.0, 300.0));
        assert!((left.0 - 64.0).abs() < 1e-6 && (right.0 - 336.0).abs() < 1e-6, "the course spans the width less the padding");
        let point = fit_camera(((0.5, 0.5), (0.5, 0.5)), (400.0, 300.0));
        assert_eq!(point.zoom, 17.0, "a single point fits at the cap");
    }

    #[test]
    fn snaps_within_twenty_pixels_and_interpolates_the_second() {
        let (mut map, _) = map(Instant::now());
        let middle = map.camera.project(course().segments[0][1].world, (400.0, 300.0));
        let now = Instant::now();
        map.pointer_move(&course(), (middle.0, middle.1 + 10.0), now);
        let hover = map.hover.unwrap();
        assert!((hover.second - 10.0).abs() < 1e-6);
        map.pointer_move(&course(), (middle.0, middle.1 + 30.0), now);
        assert_eq!(map.hover, None);
        map.pointer_down((middle.0, middle.1 + 5.0), now);
        map.pointer_up(&course(), (middle.0, middle.1 + 5.0), now);
        assert!(map.action.is_some(), "a click picks the point");
    }

    #[test]
    fn the_image_moves_with_a_pan_and_scales_with_a_zoom_against_its_frame() {
        let now = Instant::now();
        let (mut map, provider) = map(now);
        turn(&mut map, now);
        provider.answer();
        turn(&mut map, now + FRAME_INTERVAL);
        assert_eq!(map.image_placement(), Some(((0.0, 0.0), (400.0, 300.0))));
        map.camera = map.camera.panned((30.0, 10.0));
        let (origin, _) = map.image_placement().unwrap();
        assert!((origin.0 - 30.0).abs() < 1e-6 && (origin.1 - 10.0).abs() < 1e-6);
        map.camera = map.camera.panned((-30.0, -10.0)).zoomed_about(map.camera.zoom + 1.0, (200.0, 150.0), (400.0, 300.0));
        let ((x, y), (w, h)) = map.image_placement().unwrap();
        assert!((w - 800.0).abs() < 1e-6 && (h - 600.0).abs() < 1e-6 && (x + 200.0).abs() < 1e-6 && (y + 150.0).abs() < 1e-6);
    }

    #[test]
    fn playback_asks_for_nothing_and_leaves_the_image_alone() {
        let now = Instant::now();
        let (mut map, provider) = map(now);
        turn(&mut map, now);
        provider.answer();
        turn(&mut map, now + FRAME_INTERVAL);
        let revision = map.image.as_ref().unwrap().revision;
        for frame in 2..120 {
            turn(&mut map, now + FRAME_INTERVAL * frame);
        }
        assert_eq!(provider.count(), 1);
        assert_eq!(map.image.as_ref().unwrap().revision, revision);
        assert_eq!(map.deadline(now + FRAME_INTERVAL * 120), None, "a still view needs no turn");
    }

    #[test]
    fn a_drag_keeps_one_frame_in_flight_throttles_coalesces_and_ends_with_its_camera() {
        let start = Instant::now();
        let (mut map, provider) = map(start);
        turn(&mut map, start);
        provider.answer();
        turn(&mut map, start + FRAME_INTERVAL);
        let at = |ms: u64| start + Duration::from_millis(100 + ms);
        map.pointer_down((200.0, 150.0), at(0));
        for step in 1..=20_u64 {
            map.pointer_move(&course(), (200.0 + step as f64 * 4.0, 150.0), at(step * 4));
            turn(&mut map, at(step * 4));
        }
        assert_eq!(provider.count(), 2, "one more in flight, unanswered");
        provider.answer();
        turn(&mut map, at(84));
        assert_eq!(provider.count(), 3, "the newest camera, the moves between coalesced");
        // Held still, then released: no coast.
        map.pointer_up(&course(), (280.0, 150.0), at(400));
        assert_eq!(map.animation, None);
        provider.answer();
        turn(&mut map, at(401));
        assert_eq!(provider.count(), 3, "the last frame is the drag's camera");
        assert_eq!(map.image.as_ref().unwrap().view.camera, map.camera);
        for ms in 402..460 {
            turn(&mut map, at(ms));
        }
        assert_eq!(provider.count(), 3, "quiet once still");
    }

    #[test]
    fn another_style_draws_the_same_view_again() {
        let now = Instant::now();
        let (mut map, provider) = map(now);
        turn(&mut map, now);
        provider.answer();
        turn(&mut map, now + FRAME_INTERVAL);
        assert_eq!(provider.count(), 1);
        let dark = MapStyleSpec { style: ovrley_map::MapStyleId::new("dark").unwrap(), ..spec() };
        map.style = Some(dark.style.clone());
        map.advance(&course(), now + FRAME_INTERVAL * 2);
        map.send_request(&dark, now + FRAME_INTERVAL * 2).unwrap();
        assert_eq!(provider.count(), 2, "the view in the new style");
        provider.answer();
        map.collect(&course()).unwrap();
        map.send_request(&dark, now + FRAME_INTERVAL * 3).unwrap();
        assert_eq!(provider.count(), 2, "then quiet again");
    }

    #[test]
    fn the_inset_asks_at_most_every_sixteen_milliseconds_and_comes_back_for_a_waiting_view() {
        let start = Instant::now();
        let provider = Arc::new(FakeProvider::default());
        let mut inset = NavigationMap { stream: Some(MapFrameStream::streamed(provider.clone())), ..NavigationMap::default() };
        let request = |zoom: f64| {
            let camera = MapCamera::new(8.0, 47.0, zoom, 40.0, 0.0, 0.45).unwrap();
            MapFrameRequest { spec: spec(), camera, viewport: MapViewport::new(100, 100, 1.0).unwrap(), route: None, labels: None, corner_radius_px: 0.0 }
        };
        inset.send(&request(15.5), start).unwrap();
        provider.answer();
        inset.stream.as_mut().unwrap().collect();
        // The frame arrived at once: the next zoom still waits for the interval.
        let soon = start + Duration::from_millis(4);
        inset.send(&request(15.6), soon).unwrap();
        assert_eq!(provider.count(), 1);
        assert_eq!(inset.deadline(soon), Some(start + FRAME_INTERVAL));
        inset.send(&request(15.6), start + FRAME_INTERVAL).unwrap();
        assert_eq!(provider.count(), 2);
        assert_eq!(inset.deadline(start + FRAME_INTERVAL), None);
    }

    #[test]
    fn requests_are_at_least_sixteen_milliseconds_apart() {
        let start = Instant::now();
        let (mut map, provider) = map(start);
        turn(&mut map, start);
        provider.answer();
        map.camera = map.camera.panned((5.0, 0.0));
        turn(&mut map, start + Duration::from_millis(5));
        assert_eq!(provider.count(), 1, "too soon after the first");
        assert_eq!(map.deadline(start + Duration::from_millis(5)), Some(start + FRAME_INTERVAL));
        turn(&mut map, start + FRAME_INTERVAL);
        assert_eq!(provider.count(), 2);
    }

    #[test]
    fn a_resize_sends_one_request_once_it_settles_and_keeps_the_view() {
        let start = Instant::now();
        let (mut map, provider) = map(start);
        turn(&mut map, start);
        provider.answer();
        turn(&mut map, start + FRAME_INTERVAL);
        let camera = map.camera;
        for ms in 0..50 {
            map.set_size((400.0 + ms as f64, 300.0), 2.0, start + Duration::from_millis(100 + ms));
            turn(&mut map, start + Duration::from_millis(100 + ms));
        }
        assert_eq!(provider.count(), 1, "nothing while it changes");
        turn(&mut map, start + Duration::from_millis(149) + RESIZE_SETTLE);
        assert_eq!(provider.count(), 2);
        assert_eq!(map.camera, camera, "centre and zoom are kept");
    }

    #[test]
    fn the_viewport_stops_at_the_canvas_limit() {
        let map = SelectionMap { scale_factor: 2.0, ..SelectionMap::default() };
        let viewport = map.viewport((3000.0, 1500.0)).unwrap();
        assert_eq!((viewport.width_px(), viewport.height_px()), (4096, 2048));
    }

    #[test]
    fn a_coast_follows_a_fling_and_none_follows_a_held_pointer() {
        let start = Instant::now();
        let mut steps = VecDeque::new();
        for index in 0..6_u64 {
            steps.push_back((start + Duration::from_millis(index * 10), (if index == 0 { 0.0 } else { 10.0 }, 0.0)));
        }
        let (duration, offset) = drag_inertia(&steps, start + Duration::from_millis(50)).unwrap();
        // 50 px in 60 ms of window (from the first entry inside it): speed 0.3 × 40 / 0.04 s = 300 px/s.
        assert!(offset.0 > 0.0 && offset.1 == 0.0);
        assert!((duration.as_secs_f64() - 300.0 / 750.0).abs() < 1e-6);
        assert!((offset.0 - 300.0 * 0.4 / 2.0).abs() < 1e-6);
        assert_eq!(drag_inertia(&steps, start + Duration::from_millis(400)), None, "held still");
        let mut fast = VecDeque::new();
        fast.push_back((start, (0.0, 0.0)));
        fast.push_back((start + Duration::from_millis(1), (500.0, 0.0)));
        let (duration, _) = drag_inertia(&fast, start + Duration::from_millis(1)).unwrap();
        assert!((duration.as_secs_f64() - 1400.0 / 750.0).abs() < 1e-6, "the speed is capped");
    }

    #[test]
    fn a_trackpad_zooms_at_once_and_a_wheel_eases_over_two_hundred_milliseconds() {
        let start = Instant::now();
        let mut trackpad = ScrollZoom::default();
        trackpad.wheel(-50.0, Some(WheelDevice::Trackpad), false, (0.0, 0.0), start);
        let (zoom, _) = trackpad.frame(10.0, MAX_ZOOM, start).unwrap();
        let expected = (2.0 / (1.0 + (-0.5_f64).exp())).log2() + 10.0;
        assert!((zoom - expected).abs() < 1e-9);
        assert!(!trackpad.is_active());

        let mut shifted = ScrollZoom::default();
        shifted.wheel(-200.0, Some(WheelDevice::Trackpad), true, (0.0, 0.0), start);
        let (zoom, _) = shifted.frame(10.0, MAX_ZOOM, start).unwrap();
        assert!((zoom - expected).abs() < 1e-9, "Shift slows to a quarter");

        let mut wheel = ScrollZoom::default();
        wheel.wheel(100.0, Some(WheelDevice::Wheel), false, (0.0, 0.0), start);
        let (first, _) = wheel.frame(10.0, MAX_ZOOM, start).unwrap();
        assert!(first < 10.0 && first > 9.9, "a wheel starts easing out");
        let target = 10.0 + (1.0 / (2.0 / (1.0 + (-100.0_f64 / 450.0).exp()))).log2();
        let (end, _) = wheel.frame(first, MAX_ZOOM, start + Duration::from_millis(200)).unwrap();
        assert!((end - target).abs() < 1e-9);
        assert!(!wheel.is_active());
    }

    #[test]
    fn eased_zooms_last_their_durations_and_a_press_stops_them() {
        let start = Instant::now();
        let (mut map, _) = map(start);
        let from = map.camera.zoom;
        map.zoom_button(true, start);
        map.advance(&course(), start + Duration::from_millis(250));
        assert!(map.camera.zoom > from && map.camera.zoom < from + 1.0);
        map.advance(&course(), start + Duration::from_millis(500));
        assert!((map.camera.zoom - (from + 1.0)).abs() < 1e-9);
        map.double_click((10.0, 10.0), true, start + Duration::from_millis(600));
        map.advance(&course(), start + Duration::from_millis(700));
        map.pointer_down((0.0, 0.0), start + Duration::from_millis(700));
        assert_eq!(map.animation, None, "a press stops it where it is");
        let mut top = SelectionMap { camera: TopDownCamera { world: (0.5, 0.5), zoom: MAX_ZOOM }, ..SelectionMap::default() };
        assert!(!top.can_zoom_in() && top.can_zoom_out());
        top.camera.zoom = 0.0;
        assert!(!top.can_zoom_out());
    }

    #[test]
    fn bezier_easing_matches_known_values() {
        let ease = CubicBezier::new(0.25, 0.1, 0.25, 1.0);
        assert!((ease.at(0.5) - 0.8024033877399112).abs() < 1e-5);
        assert_eq!(ease.at(0.0), 0.0);
        assert!((ease.at(1.0) - 1.0).abs() < 1e-9);
    }
}

