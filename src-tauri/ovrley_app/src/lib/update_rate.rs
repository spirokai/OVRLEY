//! Port of `lib/update-rate.js`: the widget update-rate divisors of a render frame rate, and the
//! frame rate the preview steps at.

/// `PRESET_FPS_VALUES`.
pub const PRESET_FPS_VALUES: [u32; 3] = [24, 30, 60];

fn preset_update_rates(fps: u32) -> Option<&'static [u32]> {
    match fps {
        24 => Some(&[1, 2, 4, 8]),
        30 => Some(&[1, 2, 3, 5]),
        60 => Some(&[1, 2, 6, 10]),
        _ => None,
    }
}

/// `sanitizeIntegerFps`: a positive whole frame rate.
pub fn sanitize_integer_fps(fps: f64) -> u32 {
    if fps.is_finite() {
        (fps.trunc().max(1.0)).min(u32::MAX as f64) as u32
    } else {
        1
    }
}

/// `getFpsModeValue`: one of the preset rates, or `custom`.
pub fn fps_mode_value(fps: f64) -> String {
    PRESET_FPS_VALUES.iter().find(|preset| **preset as f64 == fps).map_or_else(|| "custom".to_string(), u32::to_string)
}

/// `getUpdateRateOptions`: the update-rate divisors of a frame rate, full rate included.
pub fn update_rate_options(fps: f64) -> Vec<u32> {
    let fps = sanitize_integer_fps(fps);
    if let Some(preset) = preset_update_rates(fps) {
        return preset.to_vec();
    }
    let divisors: Vec<u32> = (2..=fps / 3).filter(|divisor| fps.is_multiple_of(*divisor)).collect();
    if divisors.len() <= 3 {
        return std::iter::once(1).chain(divisors).collect();
    }
    let mut options = vec![1];
    for divisor in [divisors[0], divisors[(divisors.len() - 1) / 2], divisors[divisors.len() - 1]] {
        if !options.contains(&divisor) {
            options.push(divisor);
        }
    }
    options
}

/// `normalizeUpdateRateForFps`: an available divisor, else full rate.
pub fn normalize_update_rate_for_fps(fps: f64, update_rate: f64) -> u32 {
    let requested = sanitize_integer_fps(update_rate);
    if update_rate_options(fps).contains(&requested) {
        requested
    } else {
        1
    }
}

/// `getContainerFps`: the encoded frame rate after the update rate, which the preview steps at.
pub fn container_fps(fps: f64, update_rate: f64) -> f64 {
    let fps_whole = sanitize_integer_fps(fps);
    fps_whole as f64 / normalize_update_rate_for_fps(fps_whole as f64, update_rate) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_and_derived_divisors() {
        assert_eq!(update_rate_options(30.0), [1, 2, 3, 5]);
        assert_eq!(update_rate_options(25.0), [1, 5], "few divisors are all offered");
        assert_eq!(update_rate_options(120.0), [1, 2, 10, 40], "many divisors offer the first, the middle and the last");
        assert_eq!(update_rate_options(0.0), [1]);
    }

    #[test]
    fn the_container_rate_divides_by_an_available_update_rate() {
        assert_eq!(container_fps(30.0, 3.0), 10.0);
        assert_eq!(container_fps(30.0, 4.0), 30.0, "an unavailable divisor falls back to full rate");
        assert_eq!(container_fps(29.97, 1.0), 29.0);
        assert_eq!(fps_mode_value(60.0), "60");
        assert_eq!(fps_mode_value(25.0), "custom");
    }
}
