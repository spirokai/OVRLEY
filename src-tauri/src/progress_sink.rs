//! Adapter that streams `RenderProgress` snapshots from `ovrley_core` to the
//! frontend as Tauri `render-progress` events.
//!
//! Owns: `TauriProgressSink` — the concrete `ProgressSink` implementation
//!   installed on `RenderController` at app startup.
//! Does not own: render state, event payload shape (both belong to
//!   `ovrley_core::encode::progress`).
//!
//! This is the single adapter boundary for progress events per the project
//! "adapters are permitted only at genuine external-system boundaries and MUST
//! translate once" rule: `ovrley_core` produces canonical `RenderProgress`
//! values; this module only forwards them through Tauri's `Emitter` with no
//! remapping, aliasing, or compatibility shims.

use ovrley_core::debug::RenderProgress;
use ovrley_core::encode::progress::ProgressSink;
use tauri::{AppHandle, Emitter};

/// Event name used for streamed render-progress updates. The frontend
/// subscribes via `listen('render-progress', ...)` from `@tauri-apps/api/event`.
pub const RENDER_PROGRESS_EVENT: &str = "render-progress";

/// `ProgressSink` implementation that emits a Tauri event per progress
/// mutation, replacing the old frontend 500 ms polling loop with streaming.
///
/// `AppHandle` is `Clone + Send + Sync` and the `Emitter::emit` signature is
/// `&self`, so a single shared handle serves all `RenderController` clones
/// (the one in Tauri managed state, and the one moved into the background
/// render thread).
#[derive(Debug, Clone)]
pub(crate) struct TauriProgressSink {
    app: AppHandle,
}

impl TauriProgressSink {
    pub(crate) fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl ProgressSink for TauriProgressSink {
    fn emit_progress(&self, progress: &RenderProgress) {
        // `Emitter::emit` serializes the payload to the frontend. Failures are
        // non-fatal — e.g. no webview is listening yet during startup, or the
        // window has been torn down mid-render — so log-and-continue rather
        // than surfacing an error into the render hot path.
        if let Err(error) = self.app.emit(RENDER_PROGRESS_EVENT, progress.clone()) {
            log::warn!("failed to emit {RENDER_PROGRESS_EVENT} event: {error}");
        }
    }
}
