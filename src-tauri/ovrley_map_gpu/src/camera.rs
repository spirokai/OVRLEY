//! MapLibre-style camera and view state, ported from maplibre-rs `render/camera.rs` and
//! `render/view_state.rs`. It matches how maplibre-gl-js builds its perspective.

use crate::pattern::{TileId, ViewRegion};
use cgmath::{frustum, Deg, InnerSpace, Matrix4, Rad, SquareMatrix, Vector3, Vector4};
use ovrley_map::mercator::lon_lat_to_world;
use ovrley_map::{MAX_TILE_ZOOM, TILE_SIZE};
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
const MAX_N_TILES: usize = 512;

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
#[derive(Clone, Copy, Debug)]
pub struct MapCamera {
    lon: f64,
    lat: f64,
    zoom: f64,
    pitch_deg: f64,
    bearing_deg: f64,
}

impl MapCamera {
    /// `lon` and `lat` are the map centre in degrees. Bearing is clockwise from north.
    pub fn new(
        lon: f64,
        lat: f64,
        zoom: f64,
        pitch_deg: f64,
        bearing_deg: f64,
    ) -> Result<Self, String> {
        if ![lon, lat, zoom, pitch_deg, bearing_deg]
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
        Ok(Self {
            lon,
            lat,
            zoom,
            pitch_deg,
            bearing_deg,
        })
    }

    pub const fn zoom(self) -> f64 {
        self.zoom
    }
}

/// How far past the visible tiles a view region reaches.
#[derive(Clone, Copy, Debug)]
pub enum ViewPadding {
    /// One extra ring of tiles, for loading ahead of the camera.
    Loose,
    /// Exactly the visible tiles, for drawing.
    Tight,
}

/// One camera on one viewport: what every frame's matrices and tile lists derive from.
pub(crate) struct ViewState {
    /// World position of the map centre, in pixels at `zoom`.
    position: (f64, f64),
    zoom: f64,
    lat: f64,
    pitch: Rad<f64>,
    roll: Rad<f64>,
    width: f64,
    height: f64,
}

impl ViewState {
    pub(crate) fn new(camera: &MapCamera, width: u32, height: u32) -> Self {
        let (world_x, world_y) = lon_lat_to_world(camera.lon, camera.lat);
        let world_size = world_size(camera.zoom);
        Self {
            position: (world_x * world_size, world_y * world_size),
            zoom: camera.zoom,
            lat: camera.lat,
            pitch: Deg(camera.pitch_deg).into(),
            // Bearing is roll, in the opposite sense.
            roll: Deg(-camera.bearing_deg).into(),
            width: f64::from(width),
            height: f64::from(height),
        }
    }

    pub(crate) const fn zoom(&self) -> f64 {
        self.zoom
    }

    /// World units per metre at the camera's latitude, as MapLibre scales extrusion heights.
    pub(crate) fn metres_to_world(&self) -> f64 {
        let circumference = 2.0 * PI * EARTH_RADIUS * self.lat.to_radians().cos();
        world_size(self.zoom) / circumference
    }

    /// The light turned with the view, because MapLibre anchors it to the viewport. Roll is
    /// `-bearing`, so this rotates by the bearing.
    pub(crate) fn light_xy(&self) -> [f32; 2] {
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

    fn furthest_distance(&self, camera_height: f64) -> f64 {
        let (x, y) = (self.x_tan(), self.y_tan());
        let rays = [
            Vector3::new(x, y, 1.0),
            Vector3::new(-x, y, 1.0),
            Vector3::new(x, -y, 1.0),
            Vector3::new(-x, -y, 1.0),
        ];
        let plane_normal = (self.rotation() * Vector4::new(0.0, 0.0, 1.0, 1.0)).truncate();
        let m = Vector3::new(0.0, 0.0, camera_height);
        rays.iter()
            .map(|ray| m.dot(plane_normal) / ray.dot(plane_normal))
            .fold(f64::NAN, f64::max)
    }

    /// `FLIP_Y * OPENGL_TO_WGPU_MATRIX * perspective * camera`.
    pub(crate) fn view_projection(&self) -> Matrix4<f64> {
        let camera_to_center_distance = self.camera_to_center_distance();
        // Add a bit extra to avoid precision problems at exactly `furthest_distance`.
        let far_z = self.furthest_distance(camera_to_center_distance) * 1.01;
        let near_z = self.height / 50.0;
        let (xmax, ymax) = (near_z * self.x_tan(), near_z * self.y_tan());
        let perspective = frustum(-xmax, xmax, -ymax, ymax, near_z, far_z);
        FLIP_Y * OPENGL_TO_WGPU_MATRIX * perspective * self.camera_matrix()
    }

    /// The tiles at `level` under the frustum's footprint on the ground. `None` when the ground
    /// is not in view.
    pub(crate) fn create_view_region(&self, level: u8, padding: ViewPadding) -> Option<ViewRegion> {
        let inverse = self.view_projection().invert()?;
        let (mut min, mut max) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
        for (x, y) in [
            (0.0, 0.0),
            (self.width, 0.0),
            (self.width, self.height),
            (0.0, self.height),
        ] {
            let (ground_x, ground_y) = self.window_to_world_at_ground(x, y, &inverse);
            min = (min.0.min(ground_x), min.1.min(ground_y));
            max = (max.0.max(ground_x), max.1.max(ground_y));
        }
        if !(min.0.is_finite() && min.1.is_finite() && max.0.is_finite() && max.1.is_finite()) {
            return None;
        }

        let tile_scale = 2_f64.powf(f64::from(level) - self.zoom) / f64::from(TILE_SIZE);
        let to_tile = |value: f64| (value * tile_scale) as i32;
        Some(ViewRegion::new(
            TileId::new(level, to_tile(min.0), to_tile(min.1)),
            TileId::new(level, to_tile(max.0), to_tile(max.1)),
            match padding {
                ViewPadding::Loose => VIEW_REGION_PADDING,
                ViewPadding::Tight => 0,
            },
            MAX_N_TILES,
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

fn world_size(zoom: f64) -> f64 {
    f64::from(TILE_SIZE) * 2_f64.powf(zoom)
}
