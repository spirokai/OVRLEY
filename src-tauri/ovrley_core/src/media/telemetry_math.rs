//! Shared math primitives for telemetry processing.
//!
//! Telemetry-parser (`mp4_telemetry`) and the DJI AC004 protobuf decoder
//! (`dji_ac004`) both compute g-force from raw accelerometer components and
//! guard floating-point values against NaN/inf propagation. Extracting these
//! to a shared module avoids duplicating the 1g-subtraction convention and
//! the finite-value check between the two extraction paths.
//!
//! Owns: [`finite_f64`], [`g_force_from_components`],
//! [`lean_angle_from_lateral_g`].
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

/// Calculates signed theoretical lean angle from lateral acceleration in g.
///
/// This assumes earth-frame lateral acceleration (physically correct for
/// the g-force from a coordinated turn). Raw device-frame IMU `LatAcc`
/// values such as those exported by RaceBox are not earth-frame lateral
/// acceleration and will produce incorrect results when passed here.
#[inline]
pub fn lean_angle_from_lateral_g(lateral_g: f64) -> Option<f64> {
    finite_f64(-lateral_g.atan().to_degrees())
}

/// Calculates theoretical lean angle from ground speed and heading rate.
///
/// Ground speed in metres per second and heading/course rate in radians per
/// second are used to compute steady-state lateral acceleration, which is
/// then converted to a signed lean angle in degrees.
#[inline]
pub fn lean_angle_from_speed_and_heading_rate(
    speed_mps: f64,
    heading_rate_rad_s: f64,
) -> Option<f64> {
    const G: f64 = 9.806_65;
    finite_f64((speed_mps * heading_rate_rad_s / G).atan().to_degrees())
}

/// Derives lean angle from ground speed and heading rate at each sample interval.
///
/// Uses the steady-state formula `atan(v · ω / g)` where `v` is speed in m/s and
/// `ω` is the heading change rate in rad/s. The heading rate is computed over a
/// configurable smoothing window to reduce GPS noise instead of using raw
/// sample-to-sample differences. Returns a series of the same length as the
/// inputs; samples without a valid window are `None`.
pub fn derive_lean_from_speed_heading(
    speed_mps: &[Option<f64>],
    heading_deg: &[Option<f64>],
    elapsed_seconds: &[f64],
) -> Vec<Option<f64>> {
    derive_lean_from_speed_heading_with_window(speed_mps, heading_deg, elapsed_seconds, 1.0)
}

/// Same as [`derive_lean_from_speed_heading`] with an explicit rate-smoothing
/// window in seconds.
fn derive_lean_from_speed_heading_with_window(
    speed_mps: &[Option<f64>],
    heading_deg: &[Option<f64>],
    elapsed_seconds: &[f64],
    window_seconds: f64,
) -> Vec<Option<f64>> {
    let sample_count = elapsed_seconds.len();
    let mut lean = vec![None; sample_count];
    for i in 0..sample_count {
        let current_time = elapsed_seconds[i];
        let hdg_curr = match heading_deg[i] {
            Some(h) => h,
            None => continue,
        };
        let speed = match speed_mps[i] {
            Some(s) => s,
            None => continue,
        };

        let mut j = i;
        while j > 0 {
            j -= 1;
            let dt = current_time - elapsed_seconds[j];
            if dt >= window_seconds {
                if let Some(hdg_prev) = heading_deg[j] {
                    let mut dh = hdg_curr - hdg_prev;
                    if dh > 180.0 {
                        dh -= 360.0;
                    }
                    if dh < -180.0 {
                        dh += 360.0;
                    }
                    let omega_rad_s = dh.to_radians() / dt;
                    lean[i] = lean_angle_from_speed_and_heading_rate(speed, omega_rad_s);
                    break;
                }
            }
        }
    }
    lean
}

/// Back-fills the missing lateral g-force axis from a lean-angle series.
///
/// When exactly one of `g_force_x` or `g_force_y` is entirely absent and
/// `lean_angle` is present, the missing axis is filled with lateral
/// acceleration computed from lean angle via `tan(angle_rad)`.
/// When both axes are absent, only `g_force_x` is backfilled.
pub fn backfill_lateral_g_from_lean_angle(
    g_force_x: &[Option<f64>],
    g_force_y: &[Option<f64>],
    lean_angle: &[Option<f64>],
) -> (Vec<Option<f64>>, Vec<Option<f64>>) {
    let x_missing = g_force_x.iter().all(Option::is_none);
    let y_missing = g_force_y.iter().all(Option::is_none);

    let backfill = |lateral: &[Option<f64>]| -> Vec<Option<f64>> {
        lateral
            .iter()
            .zip(lean_angle)
            .map(|(value, lean)| {
                value.or_else(|| lean.and_then(|angle| finite_f64(angle.to_radians().tan())))
            })
            .collect()
    };

    let new_x = if x_missing {
        backfill(g_force_x)
    } else {
        g_force_x.to_vec()
    };

    let new_y = if y_missing && !x_missing {
        backfill(g_force_y)
    } else {
        g_force_y.to_vec()
    };

    (new_x, new_y)
}
