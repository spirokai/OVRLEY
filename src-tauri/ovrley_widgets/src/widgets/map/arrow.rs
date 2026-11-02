//! The caret on a real map: a small extruded solid with a flat top, standing on the road at the rider,
//! seen through the map's own camera and lit by the buildings' light, with a soft shadow on the ground
//! beneath it.
//!
//! It draws in Skia over the finished frame, so preview and export share it and buildings never hide it.
use crate::render::text::parse_color;
use ovrley_map::{MapCamera, MapViewport, ViewState, LIGHT_XY};
use skia_safe::{paint::Style, BlurStyle, Canvas, Color, MaskFilter, Paint, PathBuilder, Point, Rect};
use std::sync::LazyLock;

/// Scene pixels each axis is measured over: long enough to survive rounding, short enough to stay
/// straight under perspective.
const AXIS_STEP_PX: f64 = 8.0;

/// The solid at radius 1: `x` right, `y` forward, `z` up. Walls rise from the ground to [`TOP`]. The
/// underside always faces away from a camera above the ground, so it isn't drawn.
const TOP: f32 = 0.4;
/// The outline's corners, clockwise seen from above: tip, right, notch, left. The rounded tip reaches
/// the origin and the body trails behind. The sharp corner sits past the origin by what
/// [`CORNER_ROUNDING`] takes off it.
const CORNERS: [[f32; 2]; 4] = [[0.0, 0.14], [0.95, -2.21], [0.0, -1.71], [-0.95, -2.21]];
/// From this pitch on, the tip sits on the rider. A pitched view looks along the road, where a solid
/// centred on the rider would seem to run ahead of them. Straight down, its middle sits on the rider so
/// it turns in place.
const TIP_ANCHOR_PITCH_DEG: f32 = 60.0;
/// How far along its edges each corner's rounding starts.
const CORNER_ROUNDING: f32 = 0.3;
/// Segments each rounded corner is drawn with.
const CORNER_SEGMENTS: usize = 6;

/// One flat side of the extrusion.
struct Wall {
    quad: [[f32; 3]; 4],
    /// Outward and unit length.
    normal: [f32; 3],
}

struct ArrowSolid {
    outline: Vec<[f32; 2]>,
    walls: Vec<Wall>,
    /// Forward extent of the rounded outline: its tip, and halfway from its tail to its tip.
    tip: f32,
    middle: f32,
}

impl ArrowSolid {
    /// The forward position on the solid that sits on the rider at this camera pitch.
    fn anchor_at(&self, pitch_deg: f32) -> f32 {
        let toward_tip = (pitch_deg / TIP_ANCHOR_PITCH_DEG).min(1.0);
        self.middle + (self.tip - self.middle) * toward_tip
    }
}

/// The same shape for every arrow, so it's built once.
static SOLID: LazyLock<ArrowSolid> = LazyLock::new(|| {
    let outline = rounded_outline();
    let walls = (0..outline.len())
        .map(|index| {
            let (a, b) = (outline[index], outline[(index + 1) % outline.len()]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let length = dx.hypot(dy);
            Wall {
                quad: [
                    [a[0], a[1], 0.0],
                    [b[0], b[1], 0.0],
                    [b[0], b[1], TOP],
                    [a[0], a[1], TOP],
                ],
                // A clockwise outline has the outside on its left.
                normal: [-dy / length, dx / length, 0.0],
            }
        })
        .collect();
    let (tail, tip) = outline
        .iter()
        .fold((f32::MAX, f32::MIN), |(tail, tip), point| {
            (tail.min(point[1]), tip.max(point[1]))
        });
    ArrowSolid {
        outline,
        walls,
        tip,
        middle: (tail + tip) / 2.0,
    }
});

/// [`CORNERS`] with each corner replaced by a quadratic curve between its two edges.
fn rounded_outline() -> Vec<[f32; 2]> {
    let count = CORNERS.len();
    let toward = |from: [f32; 2], to: [f32; 2]| {
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let length = dx.hypot(dy);
        [
            from[0] + dx / length * CORNER_ROUNDING,
            from[1] + dy / length * CORNER_ROUNDING,
        ]
    };
    let mut outline = Vec::with_capacity(count * (CORNER_SEGMENTS + 1));
    for index in 0..count {
        let corner = CORNERS[index];
        let start = toward(corner, CORNERS[(index + count - 1) % count]);
        let end = toward(corner, CORNERS[(index + 1) % count]);
        for step in 0..=CORNER_SEGMENTS {
            let t = step as f32 / CORNER_SEGMENTS as f32;
            let (from_start, from_corner, from_end) = ((1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t), t * t);
            outline.push([
                from_start * start[0] + from_corner * corner[0] + from_end * end[0],
                from_start * start[1] + from_corner * corner[1] + from_end * end[1],
            ]);
        }
    }
    outline
}

/// The building shader's light, so the arrow's walls turn as a building's do. MapLibre's light is longer
/// than 1, which saturates anything facing up; [`LIGHT_LENGTH`] keeps the top from washing out.
const LIGHT_Z: f32 = 0.996;
const LIGHT_LENGTH: f32 = 1.15;
const LIGHT_INTENSITY: f32 = 0.5;
const AMBIENT: f32 = 0.03;

const SHADOW_OPACITY: f32 = 0.45;
/// Blur sigma as a share of the radius.
const SHADOW_BLUR: f32 = 0.16;
/// How far the light pushes the shadow from under the arrow, as a share of the radius.
const SHADOW_SHIFT: f32 = 0.2;
/// Faces grow by this stroke in scene pixels, so antialiased edges leave no seam between them.
const SEAM_STROKE: f32 = 0.6;

fn add(a: Point, b: Point) -> Point {
    Point::new(a.x + b.x, a.y + b.y)
}

fn scaled(vector: Point, by: f32) -> Point {
    Point::new(vector.x * by, vector.y * by)
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `extrusion.vertex.wgsl`'s lighting of one surface, with its light at unit length.
fn shaded(color: Color, normal: [f32; 3], light_xy: [f32; 2]) -> Color {
    let channels = [color.r(), color.g(), color.b()].map(|channel| f32::from(channel) / 255.0);
    let light = (dot(normal, [light_xy[0], light_xy[1], LIGHT_Z]) / LIGHT_LENGTH).clamp(0.0, 1.0);
    let luminance = 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
    let highlight = (1.0 - luminance + LIGHT_INTENSITY).max(1.0);
    let directional = (1.0 - LIGHT_INTENSITY) + (highlight - (1.0 - LIGHT_INTENSITY)) * light;
    let [r, g, b] =
        channels.map(|channel| (((channel + AMBIENT) * directional).clamp(0.0, 1.0) * 255.0) as u8);
    Color::from_rgb(r, g, b)
}

/// How one frame sees the arrow. It is tiny against the view, so one linear map is enough: scene pixels
/// per scene pixel of the solid along its axes.
#[derive(Clone, Copy, Debug)]
pub struct MarkerPose {
    pub center: Point,
    right: Point,
    forward: Point,
    up: Point,
    /// The ground part of the light, along `right` and `forward`.
    light: [f32; 2],
    /// The solid's forward position that sits on `center`, from [`ArrowSolid::anchor_at`].
    anchor: f32,
}

impl MarkerPose {
    /// The arrow on the road at a world position, facing `bearing_deg` clockwise from north. `None`
    /// when any of it is behind the camera. `frame` is the widget's frame in scene pixels.
    pub fn on_camera(
        camera: &MapCamera,
        viewport: MapViewport,
        frame: Rect,
        (world_x, world_y): (f64, f64),
        bearing_deg: f64,
    ) -> Option<Self> {
        let view = ViewState::new(camera, viewport);
        let projection = view.ground_projection();
        let project = |x: f64, y: f64, height: f64| {
            let clip = projection.clip_above(x, y, height);
            (clip.z >= 0.0).then(|| {
                (
                    f64::from(frame.left) + (clip.x / clip.w * 0.5 + 0.5) * f64::from(frame.width()),
                    f64::from(frame.top) + (0.5 - clip.y / clip.w * 0.5) * f64::from(frame.height()),
                )
            })
        };
        // World pixels are output pixels, which the frame's scene pixels are rasterised to.
        let world_px_per_scene_px = f64::from(viewport.width_px()) / f64::from(frame.width());
        let step = AXIS_STEP_PX * world_px_per_scene_px / view.world_size();
        let center = project(world_x, world_y, 0.0)?;
        let axis = |at: (f64, f64)| {
            Point::new(
                ((at.0 - center.0) / AXIS_STEP_PX) as f32,
                ((at.1 - center.1) / AXIS_STEP_PX) as f32,
            )
        };
        // World y runs south.
        let (sin, cos) = bearing_deg.to_radians().sin_cos();
        let right = axis(project(world_x + cos * step, world_y + sin * step, 0.0)?);
        let forward = axis(project(world_x + sin * step, world_y - cos * step, 0.0)?);
        let up = axis(project(world_x, world_y, AXIS_STEP_PX * world_px_per_scene_px)?);
        let [light_x, light_y] = view.light_xy().map(f64::from);
        Some(Self {
            center: Point::new(center.0 as f32, center.1 as f32),
            right,
            forward,
            up,
            light: [
                (light_x * cos + light_y * sin) as f32,
                (light_x * sin - light_y * cos) as f32,
            ],
            anchor: SOLID.anchor_at(camera.pitch_deg() as f32),
        })
    }

    /// Straight down with north up, for a flat stand-in route with no camera.
    pub fn top_down(center: Point, heading_deg: f32) -> Self {
        let (sin, cos) = heading_deg.to_radians().sin_cos();
        let (light_x, light_y) = (LIGHT_XY.0 as f32, LIGHT_XY.1 as f32);
        Self {
            center,
            right: Point::new(cos, sin),
            forward: Point::new(sin, -cos),
            up: Point::new(0.0, 0.0),
            light: [light_x * cos + light_y * sin, light_x * sin - light_y * cos],
            anchor: SOLID.middle,
        }
    }

    /// Scene position of a point on the solid at `radius`.
    fn place(&self, [x, y, z]: [f32; 3], radius: f32) -> Point {
        add(
            add(self.center, scaled(self.right, x * radius)),
            add(
                scaled(self.forward, (y - self.anchor) * radius),
                scaled(self.up, z * radius),
            ),
        )
    }

    /// The solid's axis toward the camera: the one direction this linear map flattens away.
    fn toward_camera(&self) -> [f32; 3] {
        let axis = cross(
            [self.right.x, self.forward.x, self.up.x],
            [self.right.y, self.forward.y, self.up.y],
        );
        if axis[2] < 0.0 {
            axis.map(|value| -value)
        } else {
            axis
        }
    }

    /// The walls the camera sees, far to near, so nearer ones paint over. The top paints over them all.
    fn visible_walls(&self) -> Vec<&'static Wall> {
        let toward = self.toward_camera();
        let mut walls: Vec<_> = SOLID
            .walls
            .iter()
            .filter(|wall| dot(wall.normal, toward) > 0.0)
            .collect();
        let depth = |wall: &Wall| dot(wall.quad[0], toward) + dot(wall.quad[1], toward);
        walls.sort_by(|a, b| depth(a).total_cmp(&depth(b)));
        walls
    }
}

/// Prepared colours and scene-pixel size of one arrow style.
#[derive(Clone, Debug)]
pub struct MapArrow {
    radius: f32,
    color: Color,
    opacity: f32,
    shadow: Paint,
}

impl MapArrow {
    /// `radius` is in scene pixels; `opacity` is the final opacity.
    pub fn new(radius: f32, color: &str, opacity: f32) -> Self {
        let mut shadow = Paint::default();
        shadow.set_anti_alias(true);
        shadow.set_color(Color::BLACK);
        shadow.set_alpha_f(SHADOW_OPACITY * opacity);
        shadow.set_mask_filter(MaskFilter::blur(
            BlurStyle::Normal,
            radius * SHADOW_BLUR,
            None,
        ));
        Self {
            radius,
            color: parse_color(color, 1.0),
            opacity,
            shadow,
        }
    }

    pub fn draw(&self, canvas: &Canvas, pose: &MarkerPose) {
        self.draw_shadow(canvas, pose);
        // Faces overlap by their seam stroke, so opacity applies to the finished solid.
        if self.opacity < 1.0 {
            canvas.save_layer_alpha_f(None, self.opacity);
        }
        let face = |points: &mut dyn Iterator<Item = [f32; 3]>, normal: [f32; 3]| {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_style(Style::StrokeAndFill);
            paint.set_stroke_width(SEAM_STROKE);
            paint.set_color(shaded(self.color, normal, pose.light));
            let scene = points.map(|point| pose.place(point, self.radius));
            canvas.draw_path(&closed_path(scene), &paint);
        };
        for wall in pose.visible_walls() {
            face(&mut wall.quad.iter().copied(), wall.normal);
        }
        face(
            &mut SOLID.outline.iter().map(|[x, y]| [*x, *y, TOP]),
            [0.0, 0.0, 1.0],
        );
        if self.opacity < 1.0 {
            canvas.restore();
        }
    }

    /// The footprint on the ground, pushed away from the light.
    fn draw_shadow(&self, canvas: &Canvas, pose: &MarkerPose) {
        let shift = [
            -pose.light[0] * SHADOW_SHIFT,
            -pose.light[1] * SHADOW_SHIFT,
        ];
        let ground = SOLID
            .outline
            .iter()
            .map(|[x, y]| pose.place([x + shift[0], y + shift[1], 0.0], self.radius));
        canvas.draw_path(&closed_path(ground), &self.shadow);
    }
}

fn closed_path(mut points: impl Iterator<Item = Point>) -> skia_safe::Path {
    let mut path = PathBuilder::new();
    if let Some(first) = points.next() {
        path.move_to(first);
    }
    for point in points {
        path.line_to(point);
    }
    path.close();
    path.detach()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: (f32, f32) = (400.0, 300.0);

    fn pose(pitch: f64, camera_bearing: f64, marker_bearing: f64) -> MarkerPose {
        let camera = MapCamera::new(14.42, 50.08, 16.0, pitch, camera_bearing, 0.0).unwrap();
        let viewport = MapViewport::new(800, 600, 1.0).unwrap();
        let frame = Rect::from_wh(FRAME.0, FRAME.1);
        let world = ovrley_map::mercator::lon_lat_to_world(14.42, 50.08);
        MarkerPose::on_camera(&camera, viewport, frame, world, marker_bearing).unwrap()
    }

    fn close(a: Point, b: (f32, f32)) {
        assert!(
            (a.x - b.0).abs() < 1e-3 && (a.y - b.1).abs() < 1e-3,
            "{a:?} is not {b:?}"
        );
    }

    #[test]
    fn looking_straight_down_at_north_keeps_scene_pixels() {
        let pose = pose(0.0, 0.0, 0.0);
        close(pose.center, (200.0, 150.0));
        close(pose.right, (1.0, 0.0));
        close(pose.forward, (0.0, -1.0));
        close(pose.up, (0.0, 0.0));
    }

    #[test]
    fn straight_down_matches_the_placeholder_pose() {
        let (camera, flat) = (
            pose(0.0, 0.0, 90.0),
            MarkerPose::top_down(Point::new(200.0, 150.0), 90.0),
        );
        close(camera.right, (flat.right.x, flat.right.y));
        close(camera.forward, (flat.forward.x, flat.forward.y));
        assert!((camera.light[0] - flat.light[0]).abs() < 1e-3);
        assert!((camera.light[1] - flat.light[1]).abs() < 1e-3);
    }

    #[test]
    fn pitch_foreshortens_the_road_and_stands_the_arrow_up() {
        let pose = pose(60.0, 0.0, 0.0);
        assert!(pose.up.y < -0.5, "up should rise on screen: {:?}", pose.up);
        assert!(pose.forward.y > -0.75, "forward should foreshorten: {:?}", pose.forward);
        assert!(pose.forward.y < 0.0);
    }

    #[test]
    fn a_follow_camera_sees_the_rear_walls_and_a_top_view_does_not() {
        let behind = pose(60.0, 0.0, 0.0);
        let walls = behind.visible_walls();
        assert!(!walls.is_empty());
        assert!(walls.iter().all(|wall| wall.normal[1] < 0.0));
        assert!(pose(0.0, 0.0, 0.0).visible_walls().is_empty());
    }

    #[test]
    fn the_top_is_flat_and_every_corner_is_rounded() {
        for wall in &SOLID.walls {
            assert_eq!([wall.quad[2][2], wall.quad[3][2]], [TOP, TOP]);
        }
        for corner in CORNERS {
            assert!(!SOLID.outline.contains(&corner), "{corner:?} stayed sharp");
        }
    }

    #[test]
    fn the_rounded_tip_reaches_the_rider_and_the_body_trails_behind() {
        let tip = SOLID.outline.iter().map(|point| point[1]).fold(f32::MIN, f32::max);
        assert!(tip.abs() < 0.01, "{tip}");
        let middle = SOLID.outline.iter().position(|point| point[1] == tip).unwrap();
        assert!(SOLID.outline[middle][0].abs() < 0.01);
    }

    #[test]
    fn the_rider_holds_the_middle_from_above_and_the_tip_from_a_pitched_view() {
        let on_rider = |pose: &MarkerPose, forward: f32| {
            close(
                pose.place([0.0, forward, 0.0], 10.0),
                (pose.center.x, pose.center.y),
            )
        };
        on_rider(&pose(0.0, 0.0, 0.0), SOLID.middle);
        on_rider(&MarkerPose::top_down(Point::new(200.0, 150.0), 30.0), SOLID.middle);
        on_rider(&pose(30.0, 0.0, 0.0), (SOLID.middle + SOLID.tip) / 2.0);
        on_rider(&pose(60.0, 0.0, 0.0), SOLID.tip);
        on_rider(&pose(70.0, 0.0, 0.0), SOLID.tip);
        assert!(SOLID.middle < -0.9, "the middle trails the tip: {}", SOLID.middle);
    }

    #[test]
    fn the_camera_turning_with_the_road_keeps_the_arrow_on_screen_unchanged() {
        let (north, east) = (pose(50.0, 0.0, 0.0), pose(50.0, 90.0, 90.0));
        close(north.right, (east.right.x, east.right.y));
        close(north.forward, (east.forward.x, east.forward.y));
        close(north.up, (east.up.x, east.up.y));
        assert!((north.light[0] - east.light[0]).abs() < 1e-3);
        assert!((north.light[1] - east.light[1]).abs() < 1e-3);
    }

    #[test]
    fn the_lit_side_is_brighter_than_the_shaded_side() {
        let color = Color::from_rgb(220, 60, 40);
        let top = shaded(color, [0.0, 0.0, 1.0], [0.3, 0.5]);
        let rear = shaded(color, [0.0, -1.0, 0.0], [0.3, 0.5]);
        assert!(top.r() > rear.r());
    }
}
