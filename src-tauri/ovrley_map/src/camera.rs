//! MapLibre-style camera and view state, ported from maplibre-rs `render/camera.rs` and
//! `render/view_state.rs`. It matches how maplibre-gl-js builds its perspective.
//!
//! This is the one owner of the view matrix: the GPU renderer draws roads with it and the widget
//! projects the route with it, so the two cannot disagree.

use crate::mercator::lon_lat_to_world;
use crate::view_region::{TileId, ViewRegion};
use crate::{MAX_TILE_ZOOM, TILE_SIZE};
use cgmath::{frustum, Deg, InnerSpace, Matrix4, Rad, SquareMatrix, Vector3, Vector4};
use std::f64::consts::PI;

/// MapLibre's vertical field of view: `2 * atan(0.75 / 1.5)` in the transform it descends from.
const FOVY: f64 = 0.6435011087932844;
/// Steeper than this, the top of the frustum no longer meets the ground in the view region.
const MAX_PITCH_DEG: f64 = 85.0;
/// MapLibre's mean earth radius, in metres.
const EARTH_RADIUS: f64 = 6371008.8;
/// MapLibre's default light `position: [1.15, 210, 30]` on the ground plane, at bearing 0.
const LIGHT_XY: (f64, f64) = (0.2875, -0.498);

const VIEW_REGION_PADDING: i32 = 1;
/// The most tiles one view draws. A view that reaches this may be missing some.
pub const MAX_VIEW_TILES: usize = 512;

#[rustfmt::skip]
const OPENGL_TO_WGPU_MATRIX: Matrix4<f64> = Matrix4::new(
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 0.5, 0.0,
    0.0, 0.0, 0.5, 1.0,
);

#[rustfmt::skip]
const FLIP_Y: Matrix4<f64> = Matrix4::new(
    1.0, 0.0, 0.0, 0.0,
    0.0, -1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 0.0, 1.0,
);

/// Where the map is viewed from. Built only through [`MapCamera::new`], so a camera in hand is valid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapCamera {
    lon: f64,
    lat: f64,
    zoom: f64,
    pitch_deg: f64,
    bearing_deg: f64,
    top_padding_ratio: f64,
}

impl MapCamera {
    /// `lon` and `lat` are the map centre in degrees. Bearing is clockwise from north.
    /// `top_padding_ratio` is the share of the height inset from the top, as MapLibre's `padding.top`:
    /// the centre projects to `(0.5 + ratio / 2) * height`.
    pub fn new(
        lon: f64,
        lat: f64,
        zoom: f64,
        pitch_deg: f64,
        bearing_deg: f64,
        top_padding_ratio: f64,
    ) -> Result<Self, String> {
        if ![lon, lat, zoom, pitch_deg, bearing_deg, top_padding_ratio]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err("Map camera values must be finite".to_string());
        }
        if !(0.0..=f64::from(MAX_TILE_ZOOM)).contains(&zoom) {
            return Err(format!(
                "Map camera zoom {zoom} is outside 0..={MAX_TILE_ZOOM}"
            ));
        }
        if !(0.0..=MAX_PITCH_DEG).contains(&pitch_deg) {
            return Err(format!(
                "Map camera pitch {pitch_deg} is outside 0..={MAX_PITCH_DEG}"
            ));
        }
        if !(0.0..1.0).contains(&top_padding_ratio) {
            return Err(format!(
                "Map camera top padding {top_padding_ratio} is outside 0..1"
            ));
        }
        Ok(Self {
            lon,
            lat,
            zoom,
            pitch_deg,
            bearing_deg,
            top_padding_ratio,
        })
    }

    pub const fn zoom(self) -> f64 {
        self.zoom
    }
}

/// The output a camera draws to. One world unit is one device pixel at pixel ratio 1, so a higher
/// ratio shows the same map sharper. Built only through [`MapViewport::new`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapViewport {
    width_px: u32,
    height_px: u32,
    pixel_ratio: f32,
}

impl MapViewport {
    pub fn new(width_px: u32, height_px: u32, pixel_ratio: f32) -> Result<Self, String> {
        if width_px == 0 || height_px == 0 {
            return Err(format!(
                "Map size {width_px}x{height_px} must not be empty"
            ));
        }
        if !pixel_ratio.is_finite() || pixel_ratio <= 0.0 {
            return Err(format!("Map pixel ratio {pixel_ratio} must be positive"));
        }
        Ok(Self {
            width_px,
            height_px,
            pixel_ratio,
        })
    }

    pub const fn width_px(self) -> u32 {
        self.width_px
    }

    pub const fn height_px(self) -> u32 {
        self.height_px
    }

    pub const fn pixel_ratio(self) -> f32 {
        self.pixel_ratio
    }
}

/// How far past the visible tiles a view region reaches.
#[derive(Clone, Copy, Debug)]
pub enum ViewPadding {
    /// One extra ring of tiles, for loading ahead of the camera.
    Loose,
    /// Exactly the tiles under the frustum's footprint, for drawing.
    Tight,
}

/// A ground point in clip space, before the perspective divide.
#[derive(Clone, Copy, Debug)]
pub struct ClipPoint {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

/// The view matrix reduced to ground points, for projecting many per frame. `z >= 0` is in front
/// of the near plane; points behind it must be clipped before dividing by `w`.
#[derive(Clone, Copy, Debug)]
pub struct GroundProjection {
    x: [f64; 3],
    y: [f64; 3],
    z: [f64; 3],
    w: [f64; 3],
}

impl GroundProjection {
    /// `world_x` and `world_y` are Mercator world coordinates in `0.0..=1.0`.
    pub fn clip(&self, world_x: f64, world_y: f64) -> ClipPoint {
        let row = |row: &[f64; 3]| row[0] * world_x + row[1] * world_y + row[2];
        ClipPoint {
            x: row(&self.x),
            y: row(&self.y),
            z: row(&self.z),
            w: row(&self.w),
        }
    }
}

/// One camera on one viewport: what every frame's matrices and tile lists derive from.
pub struct ViewState {
    /// World position of the map centre, in pixels at `zoom`.
    position: (f64, f64),
    zoom: f64,
    lat: f64,
    pitch: Rad<f64>,
    roll: Rad<f64>,
    width: f64,
    height: f64,
    pixel_ratio: f64,
    top_padding_ratio: f64,
}

impl ViewState {
    pub fn new(camera: &MapCamera, viewport: MapViewport) -> Self {
        let (world_x, world_y) = lon_lat_to_world(camera.lon, camera.lat);
        let pixel_ratio = f64::from(viewport.pixel_ratio);
        let world_size = world_size(camera.zoom, pixel_ratio);
        Self {
            position: (world_x * world_size, world_y * world_size),
            zoom: camera.zoom,
            lat: camera.lat,
            pitch: Deg(camera.pitch_deg).into(),
            // Bearing is roll, in the opposite sense.
            roll: Deg(-camera.bearing_deg).into(),
            width: f64::from(viewport.width_px),
            height: f64::from(viewport.height_px),
            pixel_ratio,
            top_padding_ratio: camera.top_padding_ratio,
        }
    }

    pub const fn zoom(&self) -> f64 {
        self.zoom
    }

    /// Edge of the square Mercator world in output pixels.
    pub fn world_size(&self) -> f64 {
        world_size(self.zoom, self.pixel_ratio)
    }

    /// World units per metre at the camera's latitude, as MapLibre scales extrusion heights.
    pub fn metres_to_world(&self) -> f64 {
        let circumference = 2.0 * PI * EARTH_RADIUS * self.lat.to_radians().cos();
        self.world_size() / circumference
    }

    /// The light turned with the view, because MapLibre anchors it to the viewport. Roll is
    /// `-bearing`, so this rotates by the bearing.
    pub fn light_xy(&self) -> [f32; 2] {
        let (sin, cos) = (-self.roll.0).sin_cos();
        [
            (cos * LIGHT_XY.0 - sin * LIGHT_XY.1) as f32,
            (sin * LIGHT_XY.0 + cos * LIGHT_XY.1) as f32,
        ]
    }

    fn camera_to_center_distance(&self) -> f64 {
        (self.height / 2.0) / (FOVY / 2.0).tan()
    }

    fn y_tan(&self) -> f64 {
        (FOVY / 2.0).tan()
    }

    fn x_tan(&self) -> f64 {
        self.y_tan() * self.width / self.height
    }

    fn rotation(&self) -> Matrix4<f64> {
        Matrix4::from_angle_x(self.pitch) * Matrix4::from_angle_z(self.roll)
    }

    fn camera_matrix(&self) -> Matrix4<f64> {
        Matrix4::from_translation(Vector3::new(0.0, 0.0, -self.camera_to_center_distance()))
            * self.rotation()
            * Matrix4::from_translation(Vector3::new(-self.position.0, -self.position.1, 0.0))
    }

    /// Top padding widens the frustum upward, so the top rays reach further than the bottom ones.
    fn furthest_distance(&self, camera_height: f64) -> f64 {
        let (x, y) = (self.x_tan(), self.y_tan());
        let (top, bottom) = (y * (1.0 + self.top_padding_ratio), y * (self.top_padding_ratio - 1.0));
        let rays = [
            Vector3::new(x, top, 1.0),
            Vector3::new(-x, top, 1.0),
            Vector3::new(x, bottom, 1.0),
            Vector3::new(-x, bottom, 1.0),
        ];
        let plane_normal = (self.rotation() * Vector4::new(0.0, 0.0, 1.0, 1.0)).truncate();
        let m = Vector3::new(0.0, 0.0, camera_height);
        rays.iter()
            .map(|ray| m.dot(plane_normal) / ray.dot(plane_normal))
            .fold(f64::NAN, f64::max)
    }

    /// `FLIP_Y * OPENGL_TO_WGPU_MATRIX * perspective * camera`.
    pub fn view_projection(&self) -> Matrix4<f64> {
        let camera_to_center_distance = self.camera_to_center_distance();
        // Add a bit extra to avoid precision problems at exactly `furthest_distance`.
        let far_z = self.furthest_distance(camera_to_center_distance) * 1.01;
        let near_z = self.height / 50.0;
        let (xmax, ymax) = (near_z * self.x_tan(), near_z * self.y_tan());
        // Upstream shifts this frustum the other way, which moves the centre up. FLIP_Y runs after
        // the frustum, so MapLibre's padding needs the opposite sign here.
        let offset = self.top_padding_ratio;
        let perspective = frustum(
            -xmax,
            xmax,
            ymax * (-1.0 - offset),
            ymax * (1.0 - offset),
            near_z,
            far_z,
        );
        FLIP_Y * OPENGL_TO_WGPU_MATRIX * perspective * self.camera_matrix()
    }

    /// The view matrix for points on the ground, taking Mercator world coordinates.
    pub fn ground_projection(&self) -> GroundProjection {
        let matrix = self.view_projection();
        let world_size = self.world_size();
        let row = |index: usize| {
            [
                matrix.x[index] * world_size,
                matrix.y[index] * world_size,
                matrix.w[index],
            ]
        };
        GroundProjection {
            x: row(0),
            y: row(1),
            z: row(2),
            w: row(3),
        }
    }

    /// The window's corners on the ground, in order, in world pixels. `None` when the ground is
    /// not in view.
    fn ground_corners(&self) -> Option<[(f64, f64); 4]> {
        let inverse = self.view_projection().invert()?;
        let corners = [
            (0.0, 0.0),
            (self.width, 0.0),
            (self.width, self.height),
            (0.0, self.height),
        ]
        .map(|(x, y)| self.window_to_world_at_ground(x, y, &inverse));
        corners
            .iter()
            .all(|corner| corner.0.is_finite() && corner.1.is_finite())
            .then_some(corners)
    }

    /// The frustum's footprint on the ground as `(min, max)`, in world pixels. `None` when the
    /// ground is not in view.
    fn ground_footprint(&self) -> Option<((f64, f64), (f64, f64))> {
        let corners = self.ground_corners()?;
        Some(corners.iter().fold(
            ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN)),
            |(min, max), corner| {
                (
                    (min.0.min(corner.0), min.1.min(corner.1)),
                    (max.0.max(corner.0), max.1.max(corner.1)),
                )
            },
        ))
    }

    /// The frustum's footprint on the ground as `(min, max)`, in Mercator world coordinates.
    pub fn ground_bounds(&self) -> Option<((f64, f64), (f64, f64))> {
        let (min, max) = self.ground_footprint()?;
        let size = self.world_size();
        Some(((min.0 / size, min.1 / size), (max.0 / size, max.1 / size)))
    }

    /// The tiles at `level` under the frustum's footprint on the ground. `None` when the ground
    /// is not in view.
    pub fn create_view_region(&self, level: u8, padding: ViewPadding) -> Option<ViewRegion> {
        let (min, max) = self.ground_footprint()?;
        let tile_scale = 2_f64.powi(i32::from(level)) / self.world_size();
        let to_tile = |value: f64| (value * tile_scale) as i32;
        let (padding, footprint) = match padding {
            ViewPadding::Loose => (VIEW_REGION_PADDING, None),
            ViewPadding::Tight => (
                0,
                Some(
                    self.ground_corners()?
                        .map(|corner| (corner.0 * tile_scale, corner.1 * tile_scale)),
                ),
            ),
        };
        Some(ViewRegion::new(
            TileId::new(level, to_tile(min.0), to_tile(min.1)),
            TileId::new(level, to_tile(max.0), to_tile(max.1)),
            padding,
            footprint,
            MAX_VIEW_TILES,
        ))
    }

    /// The world coordinates under a window position, on the `z = 0` plane.
    fn window_to_world_at_ground(&self, x: f64, y: f64, inverse: &Matrix4<f64>) -> (f64, f64) {
        let near = self.window_to_world(x, y, 0.0, inverse);
        let far = self.window_to_world(x, y, 1.0, inverse);
        let u = -near.z / (far.z - near.z);
        let ground = near + (far - near) * u;
        (ground.x, ground.y)
    }

    fn window_to_world(&self, x: f64, y: f64, z: f64, inverse: &Matrix4<f64>) -> Vector3<f64> {
        let ndc = Vector4::new(
            2.0 * x / self.width - 1.0,
            1.0 - 2.0 * y / self.height,
            z,
            1.0,
        );
        let unprojected = inverse * ndc;
        unprojected.truncate() / unprojected.w
    }
}

fn world_size(zoom: f64, pixel_ratio: f64) -> f64 {
    f64::from(TILE_SIZE) * 2_f64.powf(zoom) * pixel_ratio
}
