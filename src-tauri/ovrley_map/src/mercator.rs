use std::f64::consts::PI;

/// Latitude where the square Web Mercator world ends.
const MAX_LATITUDE: f64 = 85.051_128_779_806_59;

/// Longitude and latitude in degrees to world coordinates in `0.0..=1.0`, north at 0.
pub fn lon_lat_to_world(lon: f64, lat: f64) -> (f64, f64) {
    let lat = lat.clamp(-MAX_LATITUDE, MAX_LATITUDE).to_radians();
    (
        (lon + 180.0) / 360.0,
        (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / PI) / 2.0,
    )
}

/// The tile containing a world coordinate at a zoom.
pub fn world_to_tile(world_x: f64, world_y: f64, z: u8) -> (u32, u32) {
    let tiles = 1_u32 << z;
    let axis = |world: f64| ((world * f64::from(tiles)) as u32).min(tiles - 1);
    (axis(world_x), axis(world_y))
}
