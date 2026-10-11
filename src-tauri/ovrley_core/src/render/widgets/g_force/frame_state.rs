use crate::render::widgets::types::GForceFrameState;
use ovrley_widgets::widgets::value::format::format_number;

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
