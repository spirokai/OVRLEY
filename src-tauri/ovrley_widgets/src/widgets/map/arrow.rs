//! The caret on a real map: a small solid hovering over the road, seen through the map's own camera
//! and lit by the buildings' light, with a soft shadow on the ground beneath it.
//!
//! It draws in Skia over the finished frame, so preview and export share it and buildings never hide it.
use crate::render::text::parse_color;
use ovrley_map::{MapCamera, MapViewport, ViewState, LIGHT_XY};
use skia_safe::{paint::Style, BlurStyle, Canvas, Color, MaskFilter, Paint, PathBuilder, Point, Rect};

/// Scene pixels each axis is measured over: long enough to survive rounding, short enough to stay
/// straight under perspective.
const AXIS_STEP_PX: f64 = 8.0;

/// The solid at radius 1: `x` right, `y` forward, `z` up. Its flat underside hovers at [`HOVER`] and a
/// ridge climbs from the tip to the notch.
const HOVER: f32 = 0.2;
const TIP: [f32; 3] = [0.0, 1.35, HOVER];
const RIGHT: [f32; 3] = [0.95, -1.0, HOVER];
const LEFT: [f32; 3] = [-0.95, -1.0, HOVER];
const NOTCH: [f32; 3] = [0.0, -0.5, HOVER];
const RIDGE: [f32; 3] = [0.0, -0.5, HOVER + 0.3];
/// Each facet winds counter-clockwise seen from outside. The underside always faces away from a camera
/// above the ground, so it has none.
const FACETS: [[[f32; 3]; 3]; 4] = [
    [TIP, RIDGE, RIGHT],
    [TIP, LEFT, RIDGE],
    [RIGHT, RIDGE, NOTCH],
    [LEFT, NOTCH, RIDGE],
];
const FOOTPRINT: [[f32; 3]; 4] = [TIP, RIGHT, NOTCH, LEFT];

/// The building shader's light, so the arrow's facets turn as its walls do. MapLibre's light is longer
/// than 1, which saturates anything facing up; [`LIGHT_LENGTH`] keeps the ridge visible from above.
const LIGHT_Z: f32 = 0.996;
const LIGHT_LENGTH: f32 = 1.15;
const LIGHT_INTENSITY: f32 = 0.5;
const AMBIENT: f32 = 0.03;

const SHADOW_OPACITY: f32 = 0.45;
/// Blur sigma as a share of the radius.
const SHADOW_BLUR: f32 = 0.16;
/// How far the light pushes the shadow from under the arrow, as a share of the radius.
const SHADOW_SHIFT: f32 = 0.2;
/// Facets grow by this stroke in scene pixels, so antialiased edges leave no seam between them.
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

fn minus(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// The facet's outward unit normal.
fn facet_normal([a, b, c]: &[[f32; 3]; 3]) -> [f32; 3] {
    let normal = cross(minus(*b, *a), minus(*c, *a));
    let length = dot(normal, normal).sqrt();
    normal.map(|axis| axis / length)
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
        }
    }

    /// Scene position of a point on the solid at `radius`.
    fn place(&self, [x, y, z]: [f32; 3], radius: f32) -> Point {
        add(
            add(self.center, scaled(self.right, x * radius)),
            add(
                scaled(self.forward, y * radius),
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

    /// The facets the camera sees, far to near, so nearer ones paint over.
    fn visible_facets(&self) -> Vec<&'static [[f32; 3]; 3]> {
        let toward = self.toward_camera();
        let mut facets: Vec<_> = FACETS
            .iter()
            .filter(|facet| dot(facet_normal(facet), toward) > 0.0)
            .collect();
        let depth = |[a, b, c]: &[[f32; 3]; 3]| dot([a[0] + b[0] + c[0], a[1] + b[1] + c[1], a[2] + b[2] + c[2]], toward);
        facets.sort_by(|a, b| depth(a).total_cmp(&depth(b)));
        facets
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
        // Facets overlap by their seam stroke, so opacity applies to the finished solid.
        if self.opacity < 1.0 {
            canvas.save_layer_alpha_f(None, self.opacity);
        }
        for facet in pose.visible_facets() {
            let [a, b, c] = facet.map(|vertex| pose.place(vertex, self.radius));
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_style(Style::StrokeAndFill);
            paint.set_stroke_width(SEAM_STROKE);
            paint.set_color(shaded(self.color, facet_normal(facet), pose.light));
            let mut path = PathBuilder::new();
            path.move_to(a);
            path.line_to(b);
            path.line_to(c);
            path.close();
            canvas.draw_path(&path.detach(), &paint);
        }
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
        let ground = |[x, y, _]: [f32; 3]| pose.place([x + shift[0], y + shift[1], 0.0], self.radius);
        let mut path = PathBuilder::new();
        path.move_to(ground(FOOTPRINT[0]));
        for vertex in &FOOTPRINT[1..] {
            path.line_to(ground(*vertex));
        }
        path.close();
        canvas.draw_path(&path.detach(), &self.shadow);
    }
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
        let walls = |pose: &MarkerPose| {
            pose.visible_facets()
                .iter()
                .filter(|facet| facet.contains(&NOTCH))
                .count()
        };
        assert_eq!(walls(&pose(60.0, 0.0, 0.0)), 2);
        assert_eq!(walls(&pose(0.0, 0.0, 0.0)), 0);
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
