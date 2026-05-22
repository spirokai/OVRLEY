//! Live render progress estimation helpers.
//!
//! The estimator blends EMA-smoothed per-frame timing with wall-clock
//! throughput so early FFmpeg buffering cannot make ETA/FPS look faster than
//! actually achieved.  A brief warmup phase skips cold-start frame outliers
//! (shader compilation, asset caching, encoder pipeline priming).

/// Number of initial frames to skip before reporting estimates.
///
/// First frames are often outliers due to GPU warmup, shader compilation,
/// Skia asset caching, and FFmpeg pipeline priming.  Skipping them prevents
/// a single fast cold-start frame from poisoning the EMA for hundreds of frames.
const WARMUP_FRAMES: u32 = 5;

/// Exponential moving average estimator for remaining render time and FPS.
#[derive(Debug, Clone)]
pub struct ProgressEstimator { // test seam
    ema_seconds_per_frame: Option<f64>,
    smoothing_factor: f64,
    warmup_counter: u32,
}

impl ProgressEstimator {
    /// Default smoothing factor — higher = more stable but slower to react.
    ///
    /// 0.97 was the original value, but it took ~150 frames to converge from a
    /// cold-start outlier.  0.90 cuts that to ~25 frames while still providing
    /// very stable FPS/ETA updates (individual ±5 ms jitter produces only
    /// ±1 % FPS wobble).
    const DEFAULT_SMOOTHING_FACTOR: f64 = 0.85;

    /// Creates an estimator with the given EMA smoothing factor.
    pub fn new(smoothing_factor: f64) -> Self { // test seam
        Self {
            ema_seconds_per_frame: None,
            smoothing_factor: smoothing_factor.clamp(0.0, 1.0),
            warmup_counter: 0,
        }
    }

    /// Records one frame duration and returns `(eta_seconds, rendering_fps)`.
    ///
    /// Returns `(None, None)` during the warmup phase so the UI shows `--:--`.
    /// After warmup, blends frame timing with wall-clock throughput for a
    /// stable, conservative estimate that converges in ~20–30 frames.
    pub fn record( // test seam
        &mut self,
        current: u32,
        total: u32,
        frame_seconds: f64,
        elapsed_seconds: f64,
    ) -> (Option<u64>, Option<f64>) {
        if !frame_seconds.is_finite() || frame_seconds <= 0.0 {
            return self.current_estimate(current, total, elapsed_seconds);
        }

        // Warmup: skip cold-start frames entirely so a single fast outlier
        // cannot poison the EMA.  The UI shows --:-- until warmup completes.
        if self.warmup_counter < WARMUP_FRAMES {
            self.warmup_counter += 1;
            return (None, None);
        }

        self.ema_seconds_per_frame = Some(match self.ema_seconds_per_frame {
            Some(previous) => {
                previous * self.smoothing_factor + frame_seconds * (1.0 - self.smoothing_factor)
            }
            None => frame_seconds,
        });

        self.current_estimate(current, total, elapsed_seconds)
    }

    fn current_estimate(
        &self,
        current: u32,
        total: u32,
        elapsed_seconds: f64,
    ) -> (Option<u64>, Option<f64>) {
        let remaining = total.saturating_sub(current);
        let ema_fps = self
            .ema_seconds_per_frame
            .filter(|&avg| avg > 0.0)
            .map(|avg| 1.0 / avg);
        let wall_fps = (elapsed_seconds.is_finite() && elapsed_seconds > 0.0 && current > 0)
            .then_some(f64::from(current) / elapsed_seconds);
        let fps = match (ema_fps, wall_fps) {
            (Some(ema), Some(wall)) => Some(ema.min(wall)),
            (Some(ema), None) => Some(ema),
            (None, Some(wall)) => Some(wall),
            (None, None) => None,
        };
        let estimate = fps
            .filter(|&fps| fps > 0.0)
            .map(|fps| (f64::from(remaining) / fps).max(0.0).ceil() as u64);
        (estimate, fps)
    }
}

impl Default for ProgressEstimator {
    fn default() -> Self {
        Self::new(Self::DEFAULT_SMOOTHING_FACTOR)
    }
}

