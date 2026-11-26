//! The render session: progress, cancel and completion, and the batch's state.
//!
//! Core's progress sink is the [`RenderMailbox`]: it keeps the newest single progress, the newest
//! batch snapshot and the newest batch tick, and posts one drain action when it goes from empty to
//! full. Core publishes every frame; the UI thread takes the newest of each kind once per turn.
//! The sink exists before any submission and lives with the app, so nothing is buffered per
//! submission: a progress applies only to the active render id, a snapshot only to the accepted
//! batch at a newer revision, and a tick only on its snapshot's revision.

use std::sync::{Arc, Mutex};
use ovrley_core::encode::progress::{RenderProgress, RenderStatus};
use ovrley_core::encode::progress::ProgressSink;
use ovrley_core::jobs::contracts::{BatchProgressUpdate, BatchRenderEvent, BatchSnapshot};
use crate::store::Store;
use crate::store::services::{Services, UiPost};

/// What owns the renderer while a submission is pending.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderTarget {
    Single,
    Batch,
    /// The debug PNG frame.
    Preview,
}

#[derive(Default)]
struct Pending {
    single: Option<RenderProgress>,
    snapshot: Option<BatchSnapshot>,
    tick: Option<BatchProgressUpdate>,
    posted: bool,
}

/// Core's progress sink.
pub struct RenderMailbox {
    pending: Mutex<Pending>,
    ui_post: Arc<dyn UiPost>,
    /// Itself, for the drain actions it posts.
    this: std::sync::Weak<RenderMailbox>,
}

impl RenderMailbox {
    pub fn new(ui_post: Arc<dyn UiPost>) -> Arc<Self> {
        Arc::new_cyclic(|this| Self {
            pending: Mutex::default(),
            ui_post,
            this: this.clone(),
        })
    }

    fn deliver(&self, put: impl FnOnce(&mut Pending)) {
        let mut pending = self.pending.lock().expect("render mailbox poisoned");
        put(&mut pending);
        if pending.posted {
            return;
        }
        pending.posted = true;
        drop(pending);
        let Some(mailbox) = self.this.upgrade() else {
            return;
        };
        self.ui_post.post(Box::new(
            move |store: &mut Store, services: &mut Services| mailbox.drain(store, services),
        ));
    }

    fn drain(&self, store: &mut Store, services: &mut Services) {
        let Pending {
            single,
            snapshot,
            tick,
            ..
        } = std::mem::take(&mut *self.pending.lock().expect("render mailbox poisoned"));
        if let Some(snapshot) = snapshot {
            apply_batch_snapshot(store, snapshot);
        }
        if let Some(tick) = tick {
            store.batch_render.apply_batch_tick(tick);
        }
        if let Some(progress) = single {
            apply_render_progress(store, services, progress);
        }
    }
}

impl ProgressSink for RenderMailbox {
    fn emit_progress(&self, progress: &RenderProgress) {
        let progress = progress.clone();
        self.deliver(|pending| pending.single = Some(progress));
    }

    fn emit_batch_progress(&self, event: &BatchRenderEvent) {
        let event = event.clone();
        self.deliver(|pending| match event {
            BatchRenderEvent::Snapshot(snapshot) => {
                // A queue transition supersedes ticks of the transitions before it.
                if pending
                    .tick
                    .as_ref()
                    .is_some_and(|tick| tick.snapshot_revision < snapshot.snapshot_revision)
                {
                    pending.tick = None;
                }
                pending.snapshot = Some(snapshot);
            }
            BatchRenderEvent::Progress(tick) => pending.tick = Some(tick),
        });
    }
}

/// A single render, a running batch, or a pending submission.
pub fn is_renderer_busy(store: &Store) -> bool {
    store.media.rendering_video
        || store
            .batch_render
            .batch_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.renderer_busy)
        || store.media.render_submission_target.is_some()
}

/// The single render's progress, for the active render only. A terminal status ends the session:
/// complete opens the file in the system player, an error goes to the error alert, a cancelled
/// render does neither.
pub fn apply_render_progress(store: &mut Store, services: &mut Services, progress: RenderProgress) {
    if store.media.active_render_id != Some(progress.render_id) {
        return;
    }
    let status = progress.status;
    let message = progress.message.clone();
    store.media.render_progress = progress;
    if !status.is_terminal() {
        return;
    }
    let output_path = store.media.active_render_output_path.take();
    store.media.clear_render_session();
    match status {
        RenderStatus::Error => store.media.set_error_message(message),
        RenderStatus::Complete => {
            if let Some(path) = output_path {
                if let Err(error) = services.render.open_file(&path) {
                    store.media.set_error_message(error);
                }
            }
        }
        _ => {}
    }
}

/// A batch snapshot of the accepted batch, at a newer revision. The cancellation ends with the
/// renderer's cleanup; the results stay until the dialog closes.
pub fn apply_batch_snapshot(store: &mut Store, snapshot: BatchSnapshot) {
    store.batch_render.apply_batch_snapshot(snapshot);
    if store
        .batch_render
        .batch_snapshot
        .as_ref()
        .is_some_and(|snapshot| !snapshot.renderer_busy)
        && store.media.render_cancellation_target == Some(RenderTarget::Batch)
    {
        store.media.render_cancellation_target = None;
    }
}

/// `cancel`: a single render through `cancel`, a batch through `cancel_batch`, whose snapshot
/// applies. One cancellation at a time; "Cancelling…" until core's cleanup ends.
pub fn cancel_render(store: &mut Store, services: &mut Services, target: RenderTarget) {
    if store.media.render_cancellation_target.is_some() {
        return;
    }
    match target {
        RenderTarget::Single
            if store.media.rendering_video && store.media.active_render_id.is_some() =>
        {
            store.media.render_cancellation_target = Some(RenderTarget::Single);
            services.render.cancel();
        }
        RenderTarget::Batch => {
            let Some(batch_id) = store
                .batch_render
                .batch_snapshot
                .as_ref()
                .filter(|snapshot| snapshot.renderer_busy)
                .map(|snapshot| snapshot.batch_id.clone())
            else {
                return;
            };
            store.media.render_cancellation_target = Some(RenderTarget::Batch);
            match services.render.cancel_batch(&batch_id) {
                Ok(snapshot) => apply_batch_snapshot(store, snapshot),
                Err(error) => {
                    store.media.render_cancellation_target = None;
                    store.media.set_error_message(error.to_string());
                }
            }
        }
        _ => {}
    }
}

#[derive(Clone, Debug, Default)]
pub struct BatchRenderState {
    pub batch_output_folder: Option<String>,
    /// The accepted export's latest snapshot, kept until the dialog closes.
    pub batch_snapshot: Option<BatchSnapshot>,
}

impl BatchRenderState {
    /// A non-empty path, or none.
    pub fn set_batch_output_folder(&mut self, folder: Option<String>) -> Result<(), String> {
        if folder.as_deref().is_some_and(|folder| folder.trim().is_empty()) {
            return Err("Batch folder must be a nonempty path or null".into());
        }
        self.batch_output_folder = folder;
        Ok(())
    }

    /// The submission's first snapshot.
    pub fn accept_batch_snapshot(&mut self, snapshot: BatchSnapshot) {
        self.batch_snapshot = Some(snapshot);
    }

    /// Only of the accepted batch, at a newer revision.
    pub fn apply_batch_snapshot(&mut self, snapshot: BatchSnapshot) {
        if self.batch_snapshot.as_ref().is_some_and(|current| current.batch_id == snapshot.batch_id && snapshot.revision > current.revision) {
            self.batch_snapshot = Some(snapshot);
        }
    }

    /// A frame tick belongs to one queue transition: it applies only on that snapshot's revision,
    /// and only at a newer revision.
    pub fn apply_batch_tick(&mut self, tick: BatchProgressUpdate) {
        let Some(snapshot) = self.batch_snapshot.as_mut() else { return };
        if snapshot.batch_id != tick.batch_id || snapshot.snapshot_revision != tick.snapshot_revision || tick.revision <= snapshot.revision {
            return;
        }
        snapshot.revision = tick.revision;
        snapshot.processed_frames = tick.processed_frames;
        snapshot.rendered_frames = tick.rendered_frames;
        snapshot.encoded_frames = tick.encoded_frames;
        snapshot.current_item_progress = tick.current_item_progress;
        snapshot.elapsed_seconds = tick.elapsed_seconds;
        snapshot.estimated_seconds_remaining = tick.estimated_seconds_remaining;
    }

    pub fn clear_batch_results(&mut self) {
        self.batch_snapshot = None;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::store::services::fakes::{fake_services, FakeRenderCall};
    use crate::store::Dispatcher;
    use ovrley_core::jobs::contracts::{BatchPhase, BatchResultCounts};
    use serde_json::json;

    fn dispatcher() -> (Dispatcher, crate::store::services::fakes::Fakes) {
        let (parts, fakes) = fake_services(json!({}));
        (Dispatcher::new(Store::default(), parts), fakes)
    }

    fn progress(render_id: u64, status: RenderStatus, current: u32) -> RenderProgress {
        RenderProgress {
            render_id,
            busy: !status.is_terminal(),
            current,
            total: 100,
            status,
            ..RenderProgress::default()
        }
    }

    pub(crate) fn snapshot(
        batch_id: &str,
        revision: u64,
        snapshot_revision: u64,
        busy: bool,
    ) -> BatchSnapshot {
        BatchSnapshot {
            batch_id: batch_id.into(),
            revision,
            snapshot_revision,
            phase: if busy {
                BatchPhase::Rendering
            } else {
                BatchPhase::Completed
            },
            renderer_busy: busy,
            items: Vec::new(),
            active_item_id: None,
            planned_frames: 100,
            processed_frames: 0,
            rendered_frames: 0,
            encoded_frames: 0,
            current_item_progress: None,
            elapsed_seconds: 0.0,
            estimated_seconds_remaining: None,
            outputs: Vec::new(),
            result_counts: BatchResultCounts {
                succeeded: 0,
                failed: 0,
                cancelled: 0,
                unstarted: 0,
            },
        }
    }

    fn tick(
        batch_id: &str,
        revision: u64,
        snapshot_revision: u64,
        processed: u64,
    ) -> BatchProgressUpdate {
        BatchProgressUpdate {
            batch_id: batch_id.into(),
            revision,
            snapshot_revision,
            processed_frames: processed,
            rendered_frames: processed,
            encoded_frames: processed,
            current_item_progress: None,
            elapsed_seconds: 1.0,
            estimated_seconds_remaining: None,
        }
    }

    #[test]
    fn many_events_collapse_into_one_drain_and_the_newest_progress_wins() {
        let (dispatcher, fakes) = dispatcher();
        dispatcher.dispatch(|store, _| {
            store
                .media
                .start_render_session(7, "/out.mov".into(), "Starting render...".into());
        });
        let mailbox = fakes.render.mailbox.clone();
        for current in 1..=50 {
            mailbox.emit_progress(&progress(7, RenderStatus::Rendering, current));
        }
        assert_eq!(
            fakes.ui_post.posted.lock().unwrap().len(),
            1,
            "one drain for many events"
        );
        fakes.ui_post.drain(&dispatcher);
        assert_eq!(
            dispatcher.with_store(|store| store.media.render_progress.current),
            50
        );
        mailbox.emit_progress(&progress(7, RenderStatus::Rendering, 51));
        assert_eq!(
            fakes.ui_post.posted.lock().unwrap().len(),
            1,
            "a drained mailbox posts again"
        );
    }

    #[test]
    fn progress_of_another_render_is_ignored() {
        let (dispatcher, fakes) = dispatcher();
        dispatcher.dispatch(|store, _| {
            store
                .media
                .start_render_session(2, "/out.mov".into(), String::new());
        });
        fakes
            .render
            .mailbox
            .emit_progress(&progress(1, RenderStatus::Rendering, 30));
        fakes.ui_post.drain(&dispatcher);
        assert_eq!(
            dispatcher.with_store(|store| store.media.render_progress.current),
            0
        );
    }

    #[test]
    fn completion_opens_the_file_an_error_alerts_and_cancelled_does_neither() {
        let (dispatcher, fakes) = dispatcher();
        let finish = |id: u64, status: RenderStatus, message: &str| {
            dispatcher.dispatch(move |store, _| {
                store
                    .media
                    .start_render_session(id, format!("/out-{id}.mov"), String::new());
            });
            fakes.render.mailbox.emit_progress(&RenderProgress {
                message: message.into(),
                ..progress(id, RenderStatus::Rendering, 99)
            });
            fakes.render.mailbox.emit_progress(&RenderProgress {
                message: message.into(),
                ..progress(id, status, 100)
            });
            fakes.ui_post.drain(&dispatcher);
        };
        finish(1, RenderStatus::Complete, "Video rendered successfully");
        assert!(!dispatcher.with_store(|store| store.media.rendering_video));
        assert_eq!(
            fakes.render.calls(),
            [FakeRenderCall::OpenFile("/out-1.mov".into())]
        );
        finish(2, RenderStatus::Error, "ffmpeg failed");
        assert_eq!(
            dispatcher.with_store(|store| store.media.error_message.clone()),
            Some("ffmpeg failed".into())
        );
        dispatcher.dispatch(|store, _| store.media.clear_error());
        finish(3, RenderStatus::Cancelled, "Rendering cancelled");
        assert_eq!(
            dispatcher.with_store(|store| store.media.error_message.clone()),
            None
        );
        assert_eq!(
            fakes.render.calls().len(),
            1,
            "only the completed render opened"
        );
    }

    #[test]
    fn a_tick_waits_for_its_snapshot_and_old_revisions_are_dropped() {
        let (dispatcher, fakes) = dispatcher();
        dispatcher.dispatch(|store, _| {
            store
                .batch_render
                .accept_batch_snapshot(snapshot("batch-1", 1, 1, true));
        });
        let mailbox = fakes.render.mailbox.clone();
        // A tick of a transition the store has not seen waits.
        mailbox.emit_batch_progress(&BatchRenderEvent::Progress(tick("batch-1", 3, 2, 40)));
        fakes.ui_post.drain(&dispatcher);
        assert_eq!(
            dispatcher.with_store(|store| store
                .batch_render
                .batch_snapshot
                .as_ref()
                .unwrap()
                .processed_frames),
            0
        );
        // Its snapshot and a tick on it.
        mailbox.emit_batch_progress(&BatchRenderEvent::Snapshot(snapshot("batch-1", 4, 4, true)));
        mailbox.emit_batch_progress(&BatchRenderEvent::Progress(tick("batch-1", 5, 4, 60)));
        fakes.ui_post.drain(&dispatcher);
        assert_eq!(
            dispatcher.with_store(|store| store
                .batch_render
                .batch_snapshot
                .as_ref()
                .map(|s| (s.revision, s.processed_frames))),
            Some((5, 60))
        );
        // An older snapshot and another batch are ignored.
        mailbox.emit_batch_progress(&BatchRenderEvent::Snapshot(snapshot(
            "batch-1", 2, 2, false,
        )));
        fakes.ui_post.drain(&dispatcher);
        mailbox.emit_batch_progress(&BatchRenderEvent::Snapshot(snapshot(
            "batch-9", 9, 9, false,
        )));
        fakes.ui_post.drain(&dispatcher);
        assert_eq!(
            dispatcher.with_store(|store| store
                .batch_render
                .batch_snapshot
                .as_ref()
                .map(|s| s.revision)),
            Some(5)
        );
    }

    #[test]
    fn a_terminal_progress_is_never_lost_behind_later_ticks() {
        let (dispatcher, fakes) = dispatcher();
        dispatcher.dispatch(|store, _| {
            store
                .batch_render
                .accept_batch_snapshot(snapshot("batch-1", 1, 1, true));
        });
        fakes
            .render
            .mailbox
            .emit_batch_progress(&BatchRenderEvent::Snapshot(snapshot(
                "batch-1", 6, 6, false,
            )));
        fakes
            .render
            .mailbox
            .emit_batch_progress(&BatchRenderEvent::Progress(tick("batch-1", 5, 5, 99)));
        fakes.ui_post.drain(&dispatcher);
        let busy = dispatcher.with_store(|store| {
            store
                .batch_render
                .batch_snapshot
                .as_ref()
                .map(|s| (s.revision, s.renderer_busy))
        });
        assert_eq!(busy, Some((6, false)));
    }

    #[test]
    fn cancelling_a_batch_applies_its_snapshot_once() {
        let (dispatcher, fakes) = dispatcher();
        dispatcher.dispatch(|store, _| {
            store
                .batch_render
                .accept_batch_snapshot(snapshot("batch-1", 1, 1, true));
        });
        *fakes.render.cancel_snapshot.lock().unwrap() = Some(BatchSnapshot {
            phase: BatchPhase::Cancelling,
            ..snapshot("batch-1", 2, 2, true)
        });
        dispatcher.dispatch(|store, services| cancel_render(store, services, RenderTarget::Batch));
        dispatcher.dispatch(|store, services| cancel_render(store, services, RenderTarget::Batch));
        assert_eq!(
            fakes.render.calls(),
            [FakeRenderCall::CancelBatch("batch-1".into())],
            "one cancellation at a time"
        );
        assert_eq!(
            dispatcher.with_store(|store| store.media.render_cancellation_target),
            Some(RenderTarget::Batch)
        );
        fakes
            .render
            .mailbox
            .emit_batch_progress(&BatchRenderEvent::Snapshot(BatchSnapshot {
                phase: BatchPhase::Cancelled,
                ..snapshot("batch-1", 3, 3, false)
            }));
        fakes.ui_post.drain(&dispatcher);
        assert_eq!(
            dispatcher.with_store(|store| store.media.render_cancellation_target),
            None,
            "cleanup ended the cancellation"
        );
        assert!(!dispatcher.with_store(is_renderer_busy));
    }

    #[test]
    fn a_busy_renderer_blocks_project_commands() {
        let mut store = Store::default();
        assert!(!crate::features::projects::is_busy(&store));
        store.media.render_submission_target = Some(RenderTarget::Preview);
        assert!(crate::features::projects::is_busy(&store), "a pending submission");
        store.media.render_submission_target = None;
        store.media.start_render_session(1, "/out.mov".into(), String::new());
        assert!(crate::features::projects::is_busy(&store), "a single render");
        store.media.clear_render_session();
        store.batch_render.accept_batch_snapshot(snapshot("batch-1", 1, 1, true));
        assert!(crate::features::projects::is_busy(&store), "a running batch");
    }

    #[test]
    fn the_folder_is_a_nonempty_path_or_none() {
        let mut slice = BatchRenderState::default();
        assert!(slice.set_batch_output_folder(Some("  ".into())).is_err());
        slice.set_batch_output_folder(Some("/renders".into())).unwrap();
        slice.set_batch_output_folder(None).unwrap();
        assert_eq!(slice.batch_output_folder, None);
    }
}
