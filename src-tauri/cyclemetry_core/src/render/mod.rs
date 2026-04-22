use serde_json::{json, Value};

pub fn stub_demo_response(config: Value, parsed_activity: Value, second: u32) -> Value {
    let _ = (config, parsed_activity, second);
    json!({
        "error": "Phase 1 stub: backend_demo is not implemented yet.",
        "error_code": "UNIMPLEMENTED"
    })
}

pub fn stub_render_response(config: Value, parsed_activity: Value) -> Value {
    let _ = (config, parsed_activity);
    json!({
        "error": "Phase 1 stub: backend_render is not implemented yet.",
        "error_code": "UNIMPLEMENTED"
    })
}
