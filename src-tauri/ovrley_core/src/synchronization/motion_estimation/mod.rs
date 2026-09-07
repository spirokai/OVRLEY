//! Image-space proxies, not calibrated physical yaw or translation.

pub(crate) mod homography;
use super::optical_flow::TrackingQuality;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MotionInterval {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub outcome: MotionOutcome,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MotionOutcome {
    Estimated(MotionEstimate),
    Untrackable(TrackingQuality),
    Unavailable(QualityReason),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum QualityReason {
    TimestampGap,
    GeometryChange,
    DuplicateFrame,
    NoGlobalModel,
    PoorFit,
    IllConditionedTransform,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MotionEstimate {
    pub horizontal_per_second: f64,
    pub vertical_per_second: f64,
    pub roll_per_second: f64,
    pub expansion_per_second: f64,
    pub residual_per_second: f64,
    pub inliers: usize,
    pub inlier_fraction: f64,
    pub inlier_coverage: f64,
    pub rms_residual: f64,
    pub tracking: TrackingQuality,
}

/// Dev-only helper: write decoded motion intervals to disk so matching can be
/// debugged without re-decoding the source video.
#[cfg(debug_assertions)]
pub fn dump_intervals(intervals: &[MotionInterval], path: &std::path::Path) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(path)?;
    for interval in intervals {
        let line = serde_json::to_string(interval)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        writeln!(file, "{line}")?;
    }
    Ok(())
}
