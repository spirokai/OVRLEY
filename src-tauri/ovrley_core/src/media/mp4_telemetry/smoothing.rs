//! Noise-reduction smoothing for continuous telemetry series.
//!
//! GPS altitude, speed, heading, and IMU g-force are continuous physical
//! quantities that benefit from noise reduction. A centred moving average
//! applied forward and then backward (zero-phase) removes jitter without the
//! lag of a simple one-pole filter — important for map/course widgets where
//! lagging the rendered position behind the GPS trace is immediately visible.
//!
//! Camera settings (ISO, aperture, shutter, focal length, EV, colour
//! temperature) are excluded because they change discretely; smoothing those
//! would invent intermediate camera states that never occurred.
//!
//! Owns: [`smooth_series`].
//! Does not own: sample extraction or JSON serialization.

use crate::activity::finalize::smoothing::{smoothing_window_for_seconds, zero_phase_smooth};
use crate::media::native_sample::NativeSample;

const GPS_SPEED_SMOOTHING_SECONDS: f64 = 0.5;
const GPS_ALTITUDE_SMOOTHING_SECONDS: f64 = 1.0;
const GPS_HEADING_SMOOTHING_SECONDS: f64 = 0.5;
const G_FORCE_SMOOTHING_SECONDS: f64 = 1.0;

/// Smooths only continuous telemetry fields before column assembly.
///
/// GPS altitude/speed/heading and g-force benefit from noise reduction. Camera
/// settings are intentionally excluded because ISO, aperture, shutter, focal
/// length, exposure value, and color temperature can change discretely;
/// smoothing those would invent intermediate camera states.
pub(crate) fn smooth_series(samples: &mut [NativeSample]) {
    let timestamps: Vec<_> = samples.iter().map(|sample| sample.timestamp_ms).collect();
    let altitude: Vec<_> = samples.iter().map(|sample| sample.altitude).collect();
    let speed: Vec<_> = samples.iter().map(|sample| sample.speed).collect();
    let heading: Vec<_> = samples.iter().map(|sample| sample.heading).collect();
    let g_force: Vec<_> = samples.iter().map(|sample| sample.g_force).collect();

    let smoothed_altitude = zero_phase_smooth(
        &altitude,
        smoothing_window_for_seconds(&timestamps, GPS_ALTITUDE_SMOOTHING_SECONDS),
    );
    let smoothed_speed = zero_phase_smooth(
        &speed,
        smoothing_window_for_seconds(&timestamps, GPS_SPEED_SMOOTHING_SECONDS),
    );
    let smoothed_heading = zero_phase_smooth(
        &heading,
        smoothing_window_for_seconds(&timestamps, GPS_HEADING_SMOOTHING_SECONDS),
    );
    let smoothed_g_force = zero_phase_smooth(
        &g_force,
        smoothing_window_for_seconds(&timestamps, G_FORCE_SMOOTHING_SECONDS),
    );

    for index in 0..samples.len() {
        samples[index].altitude = smoothed_altitude[index];
        samples[index].speed = smoothed_speed[index];
        samples[index].heading = smoothed_heading[index];
        samples[index].g_force = smoothed_g_force[index];
    }
}
