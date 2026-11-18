//! Forwards core-owned single-render progress and batch events to Tauri.
//! The execution service selects one progress destination per reservation;
//! batch updates contain full queue transitions or compact frame observations.
//!
//! Owns: `TauriProgressSink` — the concrete `ProgressSink` implementation
//!   installed on the native execution service at app startup.
//! Does not own: render state or payloads, owned by the core controller and
//!   batch service.
//!
//! This is the single adapter boundary for progress events per the project
//! "adapters are permitted only at genuine external-system boundaries and MUST
//! translate once" rule: `ovrley_core` produces canonical progress payloads;
//! this module only forwards them through Tauri's `Emitter` with no
//! remapping, aliasing, or compatibility shims.

use ovrley_core::debug::RenderProgress;
use ovrley_core::encode::progress::ProgressSink;
use ovrley_core::render_jobs::contracts::BatchRenderEvent;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Core publishes every frame; the webview takes at most ~10 updates per second, as before core
/// published every frame. Status changes and queue snapshots always pass.
const IPC_PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(100);

/// The last single progress forwarded (its render, status and time) and the last batch tick.
#[derive(Debug, Default)]
struct Forwarded {
    single: Option<(u64, ovrley_core::debug::RenderStatus, Instant)>,
    batch_tick: Option<Instant>,
}

/// Event name used for streamed render-progress updates. The frontend
/// subscribes via `listen('render-progress', ...)` from `@tauri-apps/api/event`.
pub const RENDER_PROGRESS_EVENT: &str = "render-progress";

/// `ProgressSink` implementation that emits a Tauri event per progress
/// mutation, replacing the old frontend 500 ms polling loop with streaming.
///
/// `AppHandle` is `Clone + Send + Sync` and the `Emitter::emit` signature is
/// `&self`, so a single shared handle serves all `RenderController` clones
/// held by the execution service and its native workers.
#[derive(Debug, Clone)]
pub(crate) struct TauriProgressSink {
    app: AppHandle,
    forwarded: Arc<Mutex<Forwarded>>,
}

impl TauriProgressSink {
    pub(crate) fn new(app: AppHandle) -> Self {
        Self {
            app,
            forwarded: Arc::default(),
        }
    }
}

impl ProgressSink for TauriProgressSink {
    fn emit_batch_progress(&self, event: &BatchRenderEvent) {
        if let BatchRenderEvent::Progress(_) = event {
            let mut forwarded = self.forwarded.lock().expect("progress sink mutex poisoned");
            let now = Instant::now();
            if forwarded
                .batch_tick
                .is_some_and(|last| now.duration_since(last) < IPC_PROGRESS_MIN_INTERVAL)
            {
                return;
            }
            forwarded.batch_tick = Some(now);
        }
        if let Err(error) = self.app.emit("batch-render-progress", event) {
            log::warn!("failed to emit batch-render-progress event: {error}");
        }
    }

    fn emit_progress(&self, progress: &RenderProgress) {
        {
            let mut forwarded = self.forwarded.lock().expect("progress sink mutex poisoned");
            let now = Instant::now();
            let unchanged = forwarded.single.is_some_and(|(render_id, status, last)| {
                render_id == progress.render_id
                    && status == progress.status
                    && now.duration_since(last) < IPC_PROGRESS_MIN_INTERVAL
            });
            if unchanged {
                return;
            }
            forwarded.single = Some((progress.render_id, progress.status, now));
        }
        // `Emitter::emit` serializes the payload to the frontend. Failures are
        // non-fatal — e.g. no webview is listening yet during startup, or the
        // window has been torn down mid-render — so log-and-continue rather
        // than surfacing an error into the render hot path.
        if let Err(error) = self.app.emit(RENDER_PROGRESS_EVENT, progress.clone()) {
            log::warn!("failed to emit {RENDER_PROGRESS_EVENT} event: {error}");
        }
    }
}
