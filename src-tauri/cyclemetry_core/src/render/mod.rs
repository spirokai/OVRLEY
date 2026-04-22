use crate::activity::schema::DenseActivityReport;
use crate::config::RenderConfig;
use serde_json::{json, Value};

pub fn stub_demo_response(config: &RenderConfig, dense_activity: &DenseActivityReport, second: u32) -> Value {
    json!({
        "error": "Phase 2 partial: activity processing is implemented, but preview rendering is not implemented yet.",
        "error_code": "UNIMPLEMENTED",
        "validated": true,
        "requested_second": second,
        "frame_count": dense_activity.frame_count,
        "fps": config.scene.fps
    })
}

pub fn stub_render_response(config: &RenderConfig, dense_activity: &DenseActivityReport) -> Value {
    json!({
        "error": "Phase 2 partial: activity processing is implemented, but video rendering is not implemented yet.",
        "error_code": "UNIMPLEMENTED",
        "validated": true,
        "frame_count": dense_activity.frame_count,
        "fps": config.scene.fps
    })
}
