//! Encoding diagnostics and timing summaries.

pub mod composite;
pub(crate) mod video;

pub(super) fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}
