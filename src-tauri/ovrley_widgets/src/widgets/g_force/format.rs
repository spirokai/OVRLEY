use crate::widgets::value::format::format_number;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GForceSample {
    pub horizontal: Option<f64>,
    pub vertical: Option<f64>,
    pub max_g: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GForceFrameState {
    pub marker_x: f32,
    pub marker_y: f32,
    pub magnitude: Option<f64>,
    pub label: String,
}

/// Whole-activity paired magnitudes, prepared once for nearest-rank percentile lookup.
pub fn sorted_magnitudes(horizontal: &[Option<f64>], vertical: &[Option<f64>]) -> Vec<f64> {
    let mut magnitudes: Vec<_> = horizontal
        .iter()
        .zip(vertical)
        .filter_map(|(h, v)| h.zip(*v).map(|(h, v)| h.hypot(v)))
        .collect();
    magnitudes.sort_unstable_by(f64::total_cmp);
    magnitudes
}

pub fn percentile_max_g(magnitudes: &[f64], clip_percentile: f32) -> f64 {
    if magnitudes.is_empty() {
        return 0.0;
    }
    let rank = ((f64::from(clip_percentile) / 100.0) * magnitudes.len() as f64).ceil() as usize;
    magnitudes[rank.saturating_sub(1)]
}

pub fn derive_max_g(
    horizontal: &[Option<f64>],
    vertical: &[Option<f64>],
    clip_percentile: f32,
) -> f64 {
    percentile_max_g(&sorted_magnitudes(horizontal, vertical), clip_percentile)
}

pub fn g_force_frame_state(
    horizontal: Option<f64>,
    vertical: Option<f64>,
    max_g: f64,
    center_x: f32,
    center_y: f32,
    radius: f32,
    label_decimals: usize,
) -> GForceFrameState {
    let (Some(horizontal), Some(vertical)) = (horizontal, vertical) else {
        return GForceFrameState {
            marker_x: center_x,
            marker_y: center_y,
            magnitude: None,
            label: "--".to_string(),
        };
    };

    let magnitude = horizontal.hypot(vertical);
    let (offset_x, offset_y) = if max_g > 0.0 && magnitude > 0.0 {
        let scale = radius as f64 / max_g;
        let clamp = if magnitude > max_g {
            max_g / magnitude
        } else {
            1.0
        };
        (horizontal * scale * clamp, vertical * scale * clamp)
    } else {
        (0.0, 0.0)
    };

    GForceFrameState {
        marker_x: center_x + offset_x as f32,
        marker_y: center_y + offset_y as f32,
        magnitude: Some(magnitude),
        label: format!("{} G", format_number(magnitude, label_decimals)),
    }
}
