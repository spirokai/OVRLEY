//! Live render progress estimation helpers.
//!
//! The estimator intentionally skips initial cold-start samples so shader
//! compilation, cache preparation, and first-write overhead do not skew ETA/FPS.

/// Exponential moving average estimator for remaining render time and FPS.
#[derive(Debug, Clone)]
pub(crate) struct ProgressEstimator {
    ema_seconds_per_frame: Option<f64>,
    smoothing_factor: f64,
    warmup_counter: u32,
    warmup_frames: u32,
}

impl ProgressEstimator {
    const DEFAULT_SMOOTHING_FACTOR: f64 = 0.97;
    const DEFAULT_WARMUP_FRAMES: u32 = 10;

    /// Creates an estimator with the default warmup sample count.
    pub(crate) fn new(smoothing_factor: f64) -> Self {
        Self::with_warmup(smoothing_factor, Self::DEFAULT_WARMUP_FRAMES)
    }

    /// Creates an estimator with an explicit warmup sample count.
    pub(crate) fn with_warmup(smoothing_factor: f64, warmup_frames: u32) -> Self {
        Self {
            ema_seconds_per_frame: None,
            smoothing_factor: smoothing_factor.clamp(0.0, 1.0),
            warmup_counter: 0,
            warmup_frames,
        }
    }

    /// Records one frame duration and returns `(eta_seconds, rendering_fps)`.
    pub(crate) fn record(
        &mut self,
        current: u32,
        total: u32,
        frame_seconds: f64,
    ) -> (Option<u64>, Option<f64>) {
        self.warmup_counter = self.warmup_counter.saturating_add(1);
        if self.warmup_counter <= self.warmup_frames {
            return (None, None);
        }
        if !frame_seconds.is_finite() || frame_seconds <= 0.0 {
            return self.current_estimate(current, total);
        }

        self.ema_seconds_per_frame = Some(match self.ema_seconds_per_frame {
            Some(previous) => {
                previous * self.smoothing_factor + frame_seconds * (1.0 - self.smoothing_factor)
            }
            None => frame_seconds,
        });

        self.current_estimate(current, total)
    }

    fn current_estimate(&self, current: u32, total: u32) -> (Option<u64>, Option<f64>) {
        let remaining = total.saturating_sub(current);
        let estimate = self
            .ema_seconds_per_frame
            .map(|avg| (avg * f64::from(remaining)).max(0.0).round() as u64);
        let fps = self
            .ema_seconds_per_frame
            .filter(|&avg| avg > 0.0)
            .map(|avg| 1.0 / avg);
        (estimate, fps)
    }
}

impl Default for ProgressEstimator {
    fn default() -> Self {
        Self::new(Self::DEFAULT_SMOOTHING_FACTOR)
    }
}

#[cfg(test)]
mod tests {
    use super::ProgressEstimator;

    #[test]
    fn skips_warmup_samples_before_reporting_eta_or_fps() {
        let mut estimator = ProgressEstimator::with_warmup(0.85, 2);

        assert_eq!(estimator.record(1, 10, 1.0), (None, None));
        assert_eq!(estimator.record(2, 10, 1.0), (None, None));

        let (eta, fps) = estimator.record(3, 10, 0.5);
        assert_eq!(eta, Some(4));
        assert_eq!(fps, Some(2.0));
    }

    #[test]
    fn smooths_from_first_post_warmup_sample() {
        let mut estimator = ProgressEstimator::with_warmup(0.5, 1);

        assert_eq!(estimator.record(1, 10, 10.0), (None, None));
        assert_eq!(estimator.record(2, 10, 1.0), (Some(8), Some(1.0)));

        let (eta, fps) = estimator.record(3, 10, 3.0);
        assert_eq!(eta, Some(14));
        assert_eq!(fps, Some(0.5));
    }

    #[test]
    fn can_report_output_equivalent_fps_from_scaled_frame_seconds() {
        let mut estimator = ProgressEstimator::with_warmup(0.85, 0);

        let (_eta, fps) = estimator.record(6, 60, 0.1 / 6.0);

        assert!((fps.unwrap() - 60.0).abs() < 1e-9);
    }
}
