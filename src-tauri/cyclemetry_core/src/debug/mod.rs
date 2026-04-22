use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct RenderProgress {
    pub current: u32,
    pub total: u32,
    pub encoded: u32,
    pub status: String,
    pub message: String,
    pub estimated_seconds_remaining: Option<u64>,
}

impl Default for RenderProgress {
    fn default() -> Self {
        Self {
            current: 0,
            total: 0,
            encoded: 0,
            status: "idle".to_string(),
            message: String::new(),
            estimated_seconds_remaining: None,
        }
    }
}
