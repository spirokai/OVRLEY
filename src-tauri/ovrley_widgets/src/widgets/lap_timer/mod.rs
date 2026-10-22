//! Shared current-lap, best-lap, delta and lap-log rendering.
pub mod delta;
pub mod format;
pub mod frame;
pub mod interaction;
pub mod layout;
pub mod preview;
pub mod sampling;
pub mod validation;

const LINE_HEIGHT_RATIO: f32 = 0.92;
const LOG_COLUMN_GAP_RATIO: f32 = 1.8;
const LOG_ROW_GAP_RATIO: f32 = 0.38;
const LOG_HEADER_OPACITY: f32 = 0.7;
const TABLE_VERTICAL_METRICS_TEXT: &str = "0123456789+-:.";
const PLACEHOLDER: &str = "--:--.--";
