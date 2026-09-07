//! Streaming image-motion extraction, multirate fingerprints and activity matching.
//! Managed application jobs own worker shutdown and IPC snapshots.

pub mod jobs;

mod decode;
pub mod fingerprint;
pub mod matcher;
pub mod motion_estimation;
pub mod optical_flow;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub use matcher::MatchResult;
pub use motion_estimation::{MotionInterval, MotionOutcome};

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStage {
    Analyzing,
    Matching,
}

/// Synchronous analysis entry point. Settings fail before
/// decoder launch; terminal cancellation is distinct from an error or no-match.
pub fn synchronize_video(
    repo_root: &Path,
    video_path: &Path,
    activity: &crate::activity::schema::ParsedActivity,
    analysis_settings: AnalysisSettings,
    cancellation: &Cancellation,
    mut progress: impl FnMut(AnalysisStage, Option<f64>),
) -> AnalysisResult<MatchResult> {
    analysis_settings.validate()?;
    if cancellation.is_cancelled() {
        return Err(AnalysisError::Cancelled);
    }
    progress(AnalysisStage::Analyzing, None);
    let video = prepare_video_fingerprint(
        repo_root,
        video_path,
        analysis_settings,
        cancellation,
        |seconds| progress(AnalysisStage::Analyzing, Some(seconds)),
    )?;
    if cancellation.is_cancelled() {
        return Err(AnalysisError::Cancelled);
    }
    progress(AnalysisStage::Matching, None);
    let activity = fingerprint::prepare_activity(activity);
    matcher::match_fingerprints(&activity, &video, cancellation)
}

/// Shared extraction for direct analysis and managed jobs. Settings are already validated.
fn prepare_video_fingerprint(
    repo_root: &Path,
    video_path: &Path,
    settings: AnalysisSettings,
    cancellation: &Cancellation,
    mut progress: impl FnMut(f64),
) -> AnalysisResult<fingerprint::Fingerprint> {
    let mut video = fingerprint::VideoFingerprintBuilder::default();
    #[cfg(debug_assertions)]
    let mut motion_intervals = Vec::new();
    analyze_validated_video(repo_root, video_path, settings, cancellation, |interval| {
        progress(interval.end_seconds);
        #[cfg(debug_assertions)]
        motion_intervals.push(interval.clone());
        video.push(interval);
        Ok(())
    })?;
    #[cfg(debug_assertions)]
    {
        // Dump motion intervals to a fixed dev directory under the repo root,
        // named after the video file, so it is easy to find and compare.
        let dump_name = video_path
            .with_extension("motion.jsonl")
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_else(|| "video.motion.jsonl".into());
        let dump_path = repo_root
            .join("target")
            .join("debug")
            .join("motion-analysis")
            .join(dump_name);
        if let Err(error) = motion_estimation::dump_intervals(&motion_intervals, &dump_path) {
            eprintln!("Failed to write dev motion dump to {dump_path:?}: {error}");
        } else {
            eprintln!("Wrote dev motion dump to {dump_path:?}");
        }
    }
    Ok(video.finish())
}

#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("Invalid analysis settings: {0}")]
    Settings(String),
    #[error("Video decoder: {0}")]
    Decode(String),
    #[error("Analysis cancelled")]
    Cancelled,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    OpenCv(#[from] opencv::Error),
    #[error(transparent)]
    Core(#[from] crate::CoreError),
}

pub type AnalysisResult<T> = Result<T, AnalysisError>;

/// Required user settings are checked once, before any process is started.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisSettings {
    pub frames_per_second: f64,
    pub long_edge_pixels: u32,
    pub start_seconds: f64,
    /// Optional absence means analyze through EOF.
    pub end_seconds: Option<f64>,
}

impl AnalysisSettings {
    fn validate(&self) -> AnalysisResult<()> {
        if !self.frames_per_second.is_finite()
            || !(1.0..=40.0).contains(&self.frames_per_second)
            || !(64..=1280).contains(&self.long_edge_pixels)
            || !self.start_seconds.is_finite()
            || self.start_seconds < 0.0
            || self
                .end_seconds
                .is_some_and(|end| !end.is_finite() || end <= self.start_seconds)
        {
            return Err(AnalysisError::Settings(
                "rate must be 1–40 Hz, long edge 64–1280 pixels, and window finite and increasing"
                    .into(),
            ));
        }
        Ok(())
    }
}

/// May be shared with the owning job; cancellation is observed while waiting for frames.
#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Extract intervals synchronously on the caller's job worker. The sink receives compact
/// numeric results, never images or OpenCV objects. Sink failure also kills/reaps the decoder.
/// The sink must return promptly so the job can observe cancellation.
pub fn analyze_video(
    repo_root: &Path,
    video_path: &Path,
    settings: AnalysisSettings,
    cancellation: &Cancellation,
    emit: impl FnMut(MotionInterval) -> AnalysisResult<()>,
) -> AnalysisResult<()> {
    settings.validate()?;
    analyze_validated_video(repo_root, video_path, settings, cancellation, emit)
}

fn analyze_validated_video(
    repo_root: &Path,
    video_path: &Path,
    settings: AnalysisSettings,
    cancellation: &Cancellation,
    mut emit: impl FnMut(MotionInterval) -> AnalysisResult<()>,
) -> AnalysisResult<()> {
    if cancellation.is_cancelled() {
        return Err(AnalysisError::Cancelled);
    }
    let mut decoder = decode::Decoder::start(repo_root, video_path, &settings)?;
    let mut previous: Option<decode::GrayFrame> = None;
    while let Some(frame) = decoder.next(cancellation)? {
        if let Some(before) = previous.take() {
            let elapsed = frame.seconds - before.seconds;
            let outcome = if elapsed <= 0.0 || elapsed > 3.0 / settings.frames_per_second {
                MotionOutcome::Unavailable(motion_estimation::QualityReason::TimestampGap)
            } else if before.width != frame.width || before.height != frame.height {
                MotionOutcome::Unavailable(motion_estimation::QualityReason::GeometryChange)
            } else if before.pixels == frame.pixels {
                MotionOutcome::Unavailable(motion_estimation::QualityReason::DuplicateFrame)
            } else {
                match optical_flow::opencv_pyrlk::track(&before, &frame)? {
                    optical_flow::TrackingOutcome::Tracked(pairs) => {
                        motion_estimation::homography::estimate(pairs, elapsed)?
                    }
                    optical_flow::TrackingOutcome::Insufficient(quality) => {
                        MotionOutcome::Untrackable(quality)
                    }
                }
            };
            if cancellation.is_cancelled() {
                return Err(AnalysisError::Cancelled);
            }
            emit(MotionInterval {
                start_seconds: before.seconds,
                end_seconds: frame.seconds,
                outcome,
            })?;
        }
        previous = Some(frame);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_settings_fail_before_decoder_launch() {
        for rate in [0.0, f64::NAN, f64::INFINITY, 41.0] {
            let settings = AnalysisSettings {
                frames_per_second: rate,
                long_edge_pixels: 640,
                start_seconds: 0.0,
                end_seconds: None,
            };
            assert!(matches!(
                analyze_video(
                    Path::new("."),
                    Path::new("missing.mp4"),
                    settings,
                    &Cancellation::default(),
                    |_| Ok(())
                ),
                Err(AnalysisError::Settings(_))
            ));
        }
        assert!(serde_json::from_str::<AnalysisSettings>(
            r#"{"frames_per_second":10,"long_edge_pixels":640}"#
        )
        .is_err());
    }
}
