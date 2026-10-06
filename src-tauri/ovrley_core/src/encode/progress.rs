//! Render progress estimation and lifecycle state.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::debug::RenderProgress;
use crate::error::{CoreError, CoreResult};

/// Number of initial `record` calls to skip before reporting estimates.
const WARMUP_FRAMES: u32 = 5;

/// Max recent `frame_seconds` samples in the FPS rolling window.
const WINDOW_SIZE: usize = 64;

/// Time-based rolling window for ETA estimation (seconds).
/// Samples older than this are discarded, so the estimator adapts to
/// throughput changes within ~45 s instead of carrying stale history.
const ETA_WINDOW_SECONDS: f64 = 45.0;

/// Clamp reported FPS to ±20 % of the warmup-excluded wall-clock throughput,
/// rejecting single outlier batches without masking real changes.
const WALL_TRUST_BAND: f64 = 0.20;

/// EMA smoothing factor for ETA.  Applied in log-space (geometric mean),
/// so the smoothing is relative: a 5 % swing moves the estimate 5 %
/// regardless of whether the ETA is 30 s or 6 000 s.
const DEFAULT_ETA_SMOOTHING: f64 = 0.7;

/// Relative deadband: the smoothed ETA must move by at least this fraction
/// of the last emitted value before the display updates.
const ETA_DEADBAND_FRACTION: f64 = 0.0002;

/// Minimum spacing between `set_frame_progress` emits (~10 Hz).
/// State still mutates on every call so `progress()` reads fresh data.
const PROGRESS_EMIT_MIN_INTERVAL: Duration = Duration::from_millis(100);

/// Rolling-window throughput estimator.
#[derive(Debug, Clone)]
pub struct ProgressEstimator {
    eta_smoothing: f64,
    warmup_counter: u32,
    elapsed_at_warmup_end: f64,
    current_at_warmup_end: u32,
    intervals: VecDeque<f64>,
    previous_current: u32,
    eta_samples: VecDeque<(f64, f64, u32)>,
    eta_ema_seconds: Option<f64>,
    last_emitted_eta: Option<f64>,
}

impl ProgressEstimator {
    pub fn new(eta_smoothing: f64) -> Self {
        assert!(
            (0.0..=1.0).contains(&eta_smoothing),
            "ETA smoothing must be in the inclusive range 0..=1"
        );
        Self {
            eta_smoothing,
            warmup_counter: 0,
            elapsed_at_warmup_end: 0.0,
            current_at_warmup_end: 0,
            intervals: VecDeque::with_capacity(WINDOW_SIZE),
            previous_current: 0,
            eta_samples: VecDeque::new(),
            eta_ema_seconds: None,
            last_emitted_eta: None,
        }
    }

    pub fn record(
        &mut self,
        current: u32,
        total: u32,
        frame_seconds: f64,
        elapsed_seconds: f64,
    ) -> (Option<u64>, Option<f64>) {
        let batch_count = current.saturating_sub(self.previous_current);
        self.previous_current = current;

        if self.warmup_counter < WARMUP_FRAMES {
            self.warmup_counter += 1;
            // Snapshot wall time for cold-start-excluded anchor.
            self.elapsed_at_warmup_end = elapsed_seconds;
            self.current_at_warmup_end = current;
            return (None, None);
        }

        let valid = frame_seconds.is_finite() && frame_seconds > 0.0 && batch_count > 0;
        if valid {
            if self.intervals.len() >= WINDOW_SIZE {
                self.intervals.pop_front();
            }
            self.intervals.push_back(frame_seconds);

            let batch_time = frame_seconds * f64::from(batch_count);
            self.eta_samples
                .push_back((elapsed_seconds, batch_time, batch_count));
            while let Some(&(t, _, _)) = self.eta_samples.front() {
                if elapsed_seconds - t > ETA_WINDOW_SECONDS {
                    self.eta_samples.pop_front();
                } else {
                    break;
                }
            }
        }

        let fps = self.compute_fps(current, elapsed_seconds);
        let eta = self.compute_eta(current, total);
        (eta, fps)
    }

    fn compute_fps(&self, current: u32, elapsed_seconds: f64) -> Option<f64> {
        let post_frames = current.saturating_sub(self.current_at_warmup_end);
        let post_elapsed = (elapsed_seconds - self.elapsed_at_warmup_end).max(0.0);
        let clean_wall_fps = (post_frames > 0 && post_elapsed > 0.0)
            .then_some(f64::from(post_frames) / post_elapsed);

        let window_fps = self.window_median_fps();

        match (window_fps, clean_wall_fps) {
            (Some(window), Some(clean)) if clean > 0.0 => {
                let lower = (clean * (1.0 - WALL_TRUST_BAND)).max(0.0);
                let upper = clean * (1.0 + WALL_TRUST_BAND);
                Some(window.clamp(lower, upper))
            }
            (Some(window), _) => Some(window),
            (None, Some(clean)) => Some(clean),
            (None, None) => None,
        }
    }

    fn window_median_fps(&self) -> Option<f64> {
        if self.intervals.is_empty() {
            return None;
        }
        let mut samples: Vec<f64> = self.intervals.iter().copied().collect();
        samples.sort_by(|a, b| {
            a.partial_cmp(b)
                .expect("recorded frame intervals must be finite")
        });
        let mid = samples.len() / 2;
        let median = if samples.len() % 2 == 0 {
            (samples[mid - 1] + samples[mid]) / 2.0
        } else {
            samples[mid]
        };
        (median > 0.0).then_some(1.0 / median)
    }

    fn compute_eta(&mut self, current: u32, total: u32) -> Option<u64> {
        let remaining = f64::from(total.saturating_sub(current));
        if remaining <= 0.0 {
            self.eta_ema_seconds = Some(0.0);
            return Some(0);
        }

        let (window_time, window_frames) = self
            .eta_samples
            .iter()
            .fold((0.0, 0u32), |(t, f), &(_, bt, bc)| (t + bt, f + bc));

        if window_frames == 0 {
            return None;
        }

        let raw_seconds = window_time * remaining / f64::from(window_frames);
        let smoothed = match self.eta_ema_seconds {
            Some(prev) if prev > 0.0 => {
                let a = self.eta_smoothing;
                (prev.ln() * a + raw_seconds.ln() * (1.0 - a)).exp()
            }
            _ => raw_seconds,
        };
        self.eta_ema_seconds = Some(smoothed);
        let emitted = match self.last_emitted_eta {
            Some(prev) if prev > 0.0 && (smoothed - prev).abs() < prev * ETA_DEADBAND_FRACTION => {
                prev
            }
            _ => smoothed,
        };
        self.last_emitted_eta = Some(emitted);
        Some(emitted.max(0.0).ceil() as u64)
    }
}

impl Default for ProgressEstimator {
    fn default() -> Self {
        Self::new(DEFAULT_ETA_SMOOTHING)
    }
}

/// Backend-agnostic sink for progress events. Tauri shell emits via this;
/// tests use [`NullSink`]. Must be `Send + Sync`.
pub trait ProgressSink: Send + Sync {
    fn emit_progress(&self, progress: &RenderProgress);

    fn emit_batch_progress(&self, _snapshot: &crate::render_jobs::contracts::BatchSnapshot) {}
}

/// No-op sink for `RenderController::default()` and tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSink;

impl ProgressSink for NullSink {
    fn emit_progress(&self, _progress: &RenderProgress) {}
}

/// Shared observations and cancellation for the execution service and pipeline.
/// Only the execution service may reserve or finalize a session.
#[derive(Clone)]
pub struct RenderController {
    session: Arc<Mutex<RenderSession>>,
    cancel_flag: Arc<AtomicBool>,
    progress_sink: Arc<dyn ProgressSink>,
}

struct RenderSession {
    progress: RenderProgress,
    next_render_id: u64,
    sink: Arc<dyn ProgressSink>,
    last_fps_emit_at: Option<Instant>,
}

impl Default for RenderController {
    /// Idle-state controller with a `NullSink`.
    fn default() -> Self {
        Self::with_sink(Arc::new(NullSink))
    }
}

impl RenderController {
    pub(crate) fn sink(&self) -> Arc<dyn ProgressSink> {
        self.progress_sink.clone()
    }

    /// Capture the destination with the progress mutation, before releasing
    /// the session lock. A later reservation cannot reroute an older snapshot.
    fn publish_progress(&self, session: MutexGuard<'_, RenderSession>) {
        let snapshot = session.progress.clone();
        let sink = session.sink.clone();
        drop(session);
        sink.emit_progress(&snapshot);
    }

    /// Wired to a concrete `ProgressSink`. [`default`] installs [`NullSink`].
    pub fn with_sink(progress_sink: Arc<dyn ProgressSink>) -> Self {
        Self {
            session: Arc::new(Mutex::new(RenderSession {
                progress: RenderProgress::default(),
                next_render_id: 0,
                sink: progress_sink.clone(),
                last_fps_emit_at: None,
            })),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            progress_sink,
        }
    }

    /// Snapshot of latest progress state (one-shot). Live updates via sink.
    #[must_use = "progress snapshot must be consumed for frontend reads"]
    pub fn progress(&self) -> RenderProgress {
        self.session
            .lock()
            .expect("render progress mutex poisoned")
            .progress
            .clone()
    }

    pub(crate) fn shares_state(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.session, &other.session)
    }

    /// Requests cancellation. Returns whether a render was active.
    #[must_use = "the return value indicates whether a render was in progress"]
    pub fn cancel(&self) -> bool {
        self.request_cancel(None)
    }

    /// Cancels only the named reservation, so a late batch request cannot
    /// interrupt a newer operation after the previous renderer was released.
    pub(crate) fn cancel_session(&self, render_id: u64) -> bool {
        self.request_cancel(Some(render_id))
    }

    fn request_cancel(&self, render_id: Option<u64>) -> bool {
        let mut session = self.session.lock().expect("render session mutex poisoned");
        let progress = &mut session.progress;
        if !progress.busy || render_id.is_some_and(|id| id != progress.render_id) {
            return false;
        }
        self.cancel_flag.store(true, Ordering::SeqCst);
        progress.status = "cancelling".to_string();
        progress.message = "Cancelling render...".to_string();
        self.publish_progress(session);
        true
    }

    /// Reserves a session atomically with its cancellation and progress state.
    pub(crate) fn reserve(&self, sink: Arc<dyn ProgressSink>) -> CoreResult<u64> {
        let mut session = self.session.lock().expect("render session mutex poisoned");
        let progress = &mut session.progress;
        if progress.busy {
            return Err(CoreError::Encode(
                "A render is already in progress".to_string(),
            ));
        }
        self.cancel_flag.store(false, Ordering::SeqCst);
        session.sink = sink;
        session.next_render_id += 1;
        let render_id = session.next_render_id;
        session.progress = RenderProgress {
            render_id,
            busy: true,
            current: 0,
            rendered: 0,
            total: 0,
            encoded: 0,
            status: "preparing".to_string(),
            message: "Preparing render...".to_string(),
            estimated_seconds_remaining: None,
            rendering_fps: None,
            filename: None,
        };
        self.publish_progress(session);
        Ok(render_id)
    }

    /// Resets item counters without releasing the session or clearing cancellation.
    pub(crate) fn begin_item(&self, total_frames: u32, message: &str) -> CoreResult<()> {
        let mut session = self.session.lock().expect("render session mutex poisoned");
        let progress = &mut session.progress;
        self.check_cancelled()?;
        assert!(progress.busy, "an item requires a renderer reservation");
        progress.current = 0;
        progress.rendered = 0;
        progress.total = total_frames;
        progress.encoded = 0;
        progress.status = "preparing".to_string();
        progress.message = message.to_string();
        progress.estimated_seconds_remaining = None;
        progress.rendering_fps = None;
        progress.filename = None;
        session.last_fps_emit_at = None;
        self.publish_progress(session);
        Ok(())
    }

    /// Cancellable preparation boundary, including immediately before FFmpeg startup.
    pub fn check_cancelled(&self) -> CoreResult<()> {
        if self.cancel_flag.load(Ordering::SeqCst) {
            return Err(CoreError::Cancelled);
        }
        Ok(())
    }

    /// Publishes encoding startup without overwriting a pending cancellation.
    pub fn start_encoding(&self) -> CoreResult<()> {
        let mut session = self.session.lock().expect("render session mutex poisoned");
        let progress = &mut session.progress;
        self.check_cancelled()?;
        progress.status = "rendering".to_string();
        progress.message = "Rendering frames...".to_string();
        self.publish_progress(session);
        self.check_cancelled()
    }

    /// Updates counts and emits through sink. `rendering_fps` is ~10 Hz.
    pub fn set_frame_progress(
        &self,
        current: u32,
        total: u32,
        rendered: u32,
        encoded: u32,
        estimate: Option<u64>,
        rendering_fps: Option<f64>,
    ) {
        let mut session = self.session.lock().expect("render session mutex poisoned");
        let now = Instant::now();
        let due = session
            .last_fps_emit_at
            .is_none_or(|previous| now.duration_since(previous) >= PROGRESS_EMIT_MIN_INTERVAL);
        if due {
            session.last_fps_emit_at = Some(now);
        }
        let progress = &mut session.progress;
        progress.current = current;
        progress.total = total;
        progress.rendered = rendered;
        progress.encoded = encoded;
        progress.estimated_seconds_remaining = estimate;
        if due {
            progress.rendering_fps = rendering_fps;
        }
        if progress.status == "rendering" {
            progress.message = if current >= total {
                "Encoding output file...".to_string()
            } else {
                "Rendering frames...".to_string()
            };
        }
        if due {
            self.publish_progress(session);
        }
    }

    pub(crate) fn finish_success(&self, filename: Option<String>) {
        let mut session = self.session.lock().expect("render session mutex poisoned");
        let progress = &mut session.progress;
        progress.current = progress.total;
        progress.encoded = progress.total;
        progress.status = "complete".to_string();
        progress.message = "Video rendered successfully".to_string();
        progress.estimated_seconds_remaining = Some(0);
        progress.rendering_fps = None;
        progress.filename = filename;
        progress.busy = false;
        self.publish_progress(session);
    }

    pub(crate) fn finish_error(&self, error: String, cancelled: bool) {
        let mut session = self.session.lock().expect("render session mutex poisoned");
        let progress = &mut session.progress;
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
        progress.busy = false;
        self.publish_progress(session);
    }

    /// Returns the shared cancellation flag for internal worker coordination.
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel_flag.clone()
    }
}
