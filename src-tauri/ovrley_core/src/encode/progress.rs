//! Live render progress estimation helpers and render lifecycle state.
//!
//! Owns: ProgressEstimator (EMA-based FPS/ETA), RenderController (shared
//!   render state for frontend polling and cancellation).
//! Does not own: ffmpeg process lifecycle, frame rendering, queue management.
//!
//! Allowed dependencies: std, crate::debug, crate::error.
//! Forbidden dependencies: crate::commands, crate::render.
//!
//! ## Thread Safety
//! RenderController is Send + Sync (internally uses Arc<Mutex> and
//! Arc<AtomicBool>).  ProgressEstimator is not Sync — it should be used
//! by a single writer thread.
//!
//! ## State Transitions
//! ```text
//! Idle -> Running -> Completed
//!                 -> Failed
//!                 -> Cancelled
//! ```

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::debug::RenderProgress;
use crate::error::{CoreError, CoreResult};

/// Number of initial frames to skip before reporting estimates.
///
/// First frames are often outliers due to GPU warmup, shader compilation,
/// Skia asset caching, and FFmpeg pipeline priming.  Skipping them prevents
/// a single fast cold-start frame from poisoning the EMA for hundreds of frames.
const WARMUP_FRAMES: u32 = 5;

/// Exponential moving average estimator for remaining render time and FPS.
#[derive(Debug, Clone)]
pub struct ProgressEstimator {
    // test seam
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
    pub fn new(smoothing_factor: f64) -> Self {
        // test seam
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
    pub fn record(
        // test seam
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

/// Shared render state used by frontend commands and worker threads.
///
/// Clones share the same underlying progress state via `Arc<Mutex>`.
/// Only one render may be active at a time (enforced by `try_start`).
///
/// # Cancellation Contract
///
/// When `cancel()` is called, the pipeline MUST:
///
/// 1. Stop enqueueing new frames for rendering
/// 2. Drop the frame sender (closing ffmpeg stdin)
/// 3. Wait for ffmpeg to exit (with timeout, then kill on hang)
/// 4. Join all worker threads (render, writer, monitor)
/// 5. Update progress state to Cancelled via `finish_error` with `cancelled: true`
/// 6. Clean up partial output files
/// 7. Reset `running` to `false` (allowing subsequent renders)
///
/// # State Transitions
///
/// ```text
/// Idle ──try_start()──▶ Running ──finish_success()──▶ Completed
///                       │
///                       ├──finish_error(cancelled=true)──▶ Cancelled
///                       └──finish_error(cancelled=false)──▶ Failed
/// ```
///
/// After any terminal state, the caller must call `try_start()` again
/// to begin a new render.
#[derive(Clone)]
pub struct RenderController {
    pub(crate) progress: Arc<Mutex<RenderProgress>>,
    pub(crate) cancel_flag: Arc<AtomicBool>,
    pub(crate) running: Arc<AtomicBool>,
    pub(crate) next_render_id: Arc<AtomicU32>,
}

impl Default for RenderController {
    /// Creates a controller in the idle state with no active render.
    fn default() -> Self {
        Self {
            progress: Arc::new(Mutex::new(RenderProgress::default())),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(false)),
            next_render_id: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl RenderController {
    /// Returns a snapshot of the latest progress state.
    pub fn progress(&self) -> RenderProgress {
        self.progress
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default()
    }

    /// Requests cancellation and returns whether a render was active.
    pub fn cancel(&self) -> bool {
        self.cancel_flag.store(true, Ordering::SeqCst);
        if let Ok(mut progress) = self.progress.lock() {
            progress.status = "cancelled".to_string();
            progress.message = "Cancelling render...".to_string();
        }
        self.running.load(Ordering::SeqCst)
    }

    /// Starts a render if none is currently running.
    ///
    /// On success this resets cancellation state, creates a new render id, and
    /// initializes progress totals. Concurrent starts fail fast.
    pub fn try_start(&self, total_frames: u32, message: &str) -> CoreResult<u64> {
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(CoreError::Encode(
                "A render is already in progress".to_string(),
            ));
        }
        self.cancel_flag.store(false, Ordering::SeqCst);
        let render_id = self.next_render_id.fetch_add(1, Ordering::SeqCst) as u64 + 1;
        if let Ok(mut progress) = self.progress.lock() {
            *progress = RenderProgress {
                render_id,
                current: 0,
                total: total_frames,
                encoded: 0,
                status: "rendering".to_string(),
                message: message.to_string(),
                estimated_seconds_remaining: None,
                rendering_fps: None,
                filename: None,
            };
        }
        Ok(render_id)
    }

    /// Updates producer/encoder frame counts and remaining-time estimate.
    pub fn set_frame_progress(
        &self,
        current: u32,
        total: u32,
        encoded: u32,
        estimate: Option<u64>,
        rendering_fps: Option<f64>,
    ) {
        if let Ok(mut progress) = self.progress.lock() {
            progress.current = current;
            progress.total = total;
            progress.encoded = encoded;
            progress.estimated_seconds_remaining = estimate;
            progress.rendering_fps = rendering_fps;
            progress.message = if current >= total {
                "Encoding output file...".to_string()
            } else {
                "Rendering frames...".to_string()
            };
        }
    }

    /// Marks the active render as complete and stores the output filename.
    pub fn finish_success(&self, filename: String) {
        if let Ok(mut progress) = self.progress.lock() {
            progress.current = progress.total;
            progress.encoded = progress.total;
            progress.status = "complete".to_string();
            progress.message = "Video rendered successfully".to_string();
            progress.estimated_seconds_remaining = Some(0);
            progress.rendering_fps = None;
            progress.filename = Some(filename);
        }
        self.running.store(false, Ordering::SeqCst);
        self.cancel_flag.store(false, Ordering::SeqCst);
    }

    /// Marks the active render as failed or cancelled.
    pub fn finish_error(&self, error: String, cancelled: bool) {
        if let Ok(mut progress) = self.progress.lock() {
            progress.status = if cancelled {
                "cancelled".to_string()
            } else {
                "error".to_string()
            };
            progress.message = if cancelled {
                "Rendering cancelled".to_string()
            } else {
                error
            };
            progress.estimated_seconds_remaining = None;
            progress.rendering_fps = None;
            progress.filename = None;
        }
        self.running.store(false, Ordering::SeqCst);
        self.cancel_flag.store(false, Ordering::SeqCst);
    }

    /// Returns the shared cancellation flag for internal worker coordination.
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        // test seam
        self.cancel_flag.clone()
    }
}
