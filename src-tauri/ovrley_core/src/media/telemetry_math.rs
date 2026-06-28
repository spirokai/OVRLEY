//! Shared math primitives for telemetry processing.
//!
//! Telemetry-parser (`mp4_telemetry`) and the DJI AC004 protobuf decoder
//! (`dji_ac004`) both compute g-force from raw accelerometer components and
//! guard floating-point values against NaN/inf propagation. Extracting these
//! to a shared module avoids duplicating the 1g-subtraction convention and
//! the finite-value check between the two extraction paths.
//!
//! Owns: [`finite_f64`], [`g_force_from_components`].
//! Does not own: camera metadata extraction, protobuf decoding, or smoothing.

/// Returns `Some(value)` when the float is finite, `None` otherwise.
///
/// Every numeric value extracted from telemetry passes through this guard to
/// prevent NaN and infinity from reaching JSON serialization.
#[inline]
pub fn finite_f64(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

/// Great-circle distance between two GPS coordinates in meters.
///
/// Uses the Haversine formula with Earth radius 6,371,000 m.
pub fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6_371_000.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    R * c
}

/// Computes initial GPS bearing in degrees using the great-circle formula.
///
/// Heading derivation needs wrap-safe course direction from two coordinates;
/// invalid inputs return `None` so sparse GPS samples can be skipped without
/// failing the entire activity import.
pub fn bearing_degrees(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> Option<f64> {
    if !lat1.is_finite() || !lon1.is_finite() || !lat2.is_finite() || !lon2.is_finite() {
        return None;
    }

    let from_lat = lat1.to_radians();
    let to_lat = lat2.to_radians();
    let delta_lon = (lon2 - lon1).to_radians();
    let y = delta_lon.sin() * to_lat.cos();
    let x = from_lat.cos() * to_lat.sin() - from_lat.sin() * to_lat.cos() * delta_lon.cos();
    let bearing = y.atan2(x).to_degrees();
    Some((bearing + 360.0) % 360.0)
}

/// Rounds finite telemetry values to a fixed decimal precision.
///
/// The migrated backend mirrors frontend JSON stability by rounding at the same
/// assembly points, while returning `None` for NaN/inf so invalid math never
/// reaches serialization.
pub fn round_f64(value: f64, digits: i32) -> Option<f64> {
    if !value.is_finite() {
        return None;
    }
    let scale = 10_f64.powi(digits);
    Some((value * scale).round() / scale)
}

/// Dynamic g-force magnitude from three-axis accelerometer components.
///
/// Returns `magnitude - 1g` so a resting sensor reports 0.0 and positive values
/// mean extra dynamic load. The caller is responsible for applying any unit
/// conversion (`m/s² → g`) or scale factor before passing components in.
/// Returns `None` when the result is non-finite.
pub fn g_force_from_components(x: f64, y: f64, z: f64) -> Option<f64> {
    let g = (x.powi(2) + y.powi(2) + z.powi(2)).sqrt() - 1.0;
    finite_f64(g)
}
