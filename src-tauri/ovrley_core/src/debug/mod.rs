//! Progress and performance diagnostics.
//!
//! Rendering can be long-running and CPU/GPU sensitive, so the backend exposes
//! a stable progress payload to the UI and collects named timing buckets for
//! preview, frame rendering, encoding, and debug artifact generation.

use serde::Serialize;
use std::collections::BTreeMap;
use std::time::Instant;

/// Serializable render progress snapshot.
///
/// Instances are stored behind a mutex in `RenderController` and cloned for UI
/// polling. The shape is intentionally frontend-friendly and avoids exposing
/// synchronization primitives.
#[derive(Clone, Debug, Serialize)]
pub struct RenderProgress {
    /// Monotonically increasing identifier for the current render session.
    pub render_id: u64,
    /// Number of frames rendered by the Skia producer.
    pub current: u32,
    /// Total number of frames expected in the encoded output.
    pub total: u32,
    /// Number of frames ffmpeg has reported as encoded.
    pub encoded: u32,
    /// State string such as `idle`, `rendering`, `complete`, `error`, or `cancelled`.
    pub status: String,
    /// Human-readable status message shown by the UI.
    pub message: String,
    /// Estimated remaining render seconds, when enough samples exist.
    pub estimated_seconds_remaining: Option<u64>,
    /// Final output filename after a successful render.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

impl Default for RenderProgress {
    /// Creates the idle progress state used before any render starts.
    fn default() -> Self {
        Self {
            render_id: 0,
            current: 0,
            total: 0,
            encoded: 0,
            status: "idle".to_string(),
            message: String::new(),
            estimated_seconds_remaining: None,
            filename: None,
        }
    }
}

/// Aggregated timing statistics for a named operation.
#[derive(Clone, Debug, Default, Serialize)]
pub struct TimingBucket {
    /// Optional alternate label used by legacy timing dashboards.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt_name: Option<String>,
    /// Number of recorded samples.
    pub count: u32,
    /// Total time across all samples in milliseconds.
    pub total_ms: f64,
    /// Average sample time in milliseconds.
    pub avg_ms: f64,
    /// Maximum observed sample time in milliseconds.
    pub max_ms: f64,
}

impl TimingBucket {
    /// Adds one duration sample and updates total, average, and max values.
    pub fn add_sample(&mut self, duration_ms: f64) {
        self.count += 1;
        self.total_ms += duration_ms;
        self.avg_ms = if self.count == 0 {
            0.0
        } else {
            self.total_ms / f64::from(self.count)
        };
        if duration_ms > self.max_ms {
            self.max_ms = duration_ms;
        }
    }
}

/// Lightweight named profiler used during render preparation and frames.
#[derive(Clone, Debug, Default)]
pub struct RenderProfiler {
    buckets: BTreeMap<String, TimingBucket>,
}

impl RenderProfiler {
    /// Records a duration in milliseconds under `name`.
    pub fn record_ms(&mut self, name: impl Into<String>, duration_ms: f64) {
        self.buckets
            .entry(name.into())
            .or_default()
            .add_sample(duration_ms);
    }

    /// Measures a synchronous callback and records its elapsed duration.
    pub fn measure<T>(&mut self, name: impl Into<String>, callback: impl FnOnce() -> T) -> T {
        let name = name.into();
        let started = Instant::now();
        let result = callback();
        self.record_ms(name, started.elapsed().as_secs_f64() * 1000.0);
        result
    }

    /// Returns a clone of the current timing buckets for serialization.
    pub fn summary(&self) -> BTreeMap<String, TimingBucket> {
        self.buckets.clone()
    }
}
