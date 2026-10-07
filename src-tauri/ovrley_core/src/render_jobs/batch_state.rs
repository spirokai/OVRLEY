//! Authoritative queue transitions and compact progress observations.

use super::batch::BatchServiceError;
use super::contracts::*;
use super::submission::AcceptedJob;
use crate::debug::RenderProgress;
use crate::encode::progress::ProgressSink;
use crate::error::{CoreError, CoreResult};
use crate::output::RenderOutputTarget;
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct RunningBatch {
    render_id: u64,
    batch_id: String,
    revision: u64,
    snapshot_revision: u64,
    phase: BatchPhase,
    renderer_busy: bool,
    items: Vec<BatchItemSnapshot>,
    planned_frames: u64,
    processed_frames: u64,
    rendered_frames: u64,
    encoded_frames: u64,
    active: Option<(usize, Instant)>,
    item_eta: Option<f64>,
    item_fps: Option<f64>,
    started: Instant,
    updated: Instant,
}

impl RunningBatch {
    /// Queue transitions settle outcomes; frame ticks update these totals by delta.
    fn refresh_frame_totals(&mut self) {
        self.processed_frames = 0;
        self.rendered_frames = 0;
        self.encoded_frames = 0;
        for item in &self.items {
            self.rendered_frames += item.rendered_frames;
            self.encoded_frames += item.encoded_frames;
            self.processed_frames += match item.outcome {
                Some(BatchItemOutcome::Succeeded { .. } | BatchItemOutcome::Failed { .. }) => {
                    item.planned_frames
                }
                Some(BatchItemOutcome::Unstarted) => 0,
                _ => item.current_frames,
            };
        }
    }

    fn progress_update(&self) -> BatchProgressUpdate {
        BatchProgressUpdate {
            batch_id: self.batch_id.clone(),
            revision: self.revision,
            snapshot_revision: self.snapshot_revision,
            processed_frames: self.processed_frames,
            rendered_frames: self.rendered_frames,
            encoded_frames: self.encoded_frames,
            current_item_progress: self.active.map(|(index, started)| {
                let item = &self.items[index];
                BatchItemProgress {
                    item_id: item.id.clone(),
                    planned_frames: item.planned_frames,
                    current_frames: item.current_frames,
                    rendered_frames: item.rendered_frames,
                    encoded_frames: item.encoded_frames,
                    elapsed_seconds: self.updated.duration_since(started).as_secs_f64(),
                    estimated_seconds_remaining: self.item_eta,
                    rendering_fps: self.item_fps,
                }
            }),
            elapsed_seconds: self.updated.duration_since(self.started).as_secs_f64(),
            estimated_seconds_remaining: self
                .item_fps
                .map(|fps| self.planned_frames.saturating_sub(self.processed_frames) as f64 / fps),
        }
    }

    fn snapshot(&self) -> BatchSnapshot {
        let mut counts = BatchResultCounts {
            succeeded: 0,
            failed: 0,
            cancelled: 0,
            unstarted: 0,
        };
        let mut outputs = Vec::new();
        for item in &self.items {
            match &item.outcome {
                Some(BatchItemOutcome::Succeeded { output_path }) => {
                    counts.succeeded += 1;
                    outputs.push(BatchOutput {
                        item_id: item.id.clone(),
                        output_path: output_path.clone(),
                    });
                }
                Some(BatchItemOutcome::Failed { .. }) => {
                    counts.failed += 1;
                }
                Some(BatchItemOutcome::Cancelled) => {
                    counts.cancelled += 1;
                }
                Some(BatchItemOutcome::Unstarted) => {
                    counts.unstarted += 1;
                }
                None => {}
            };
        }
        let progress = self.progress_update();
        BatchSnapshot {
            batch_id: self.batch_id.clone(),
            revision: self.revision,
            snapshot_revision: self.snapshot_revision,
            phase: self.phase,
            renderer_busy: self.renderer_busy,
            items: self.items.clone(),
            active_item_id: self.active.map(|(index, _)| self.items[index].id.clone()),
            planned_frames: self.planned_frames,
            processed_frames: progress.processed_frames,
            rendered_frames: progress.rendered_frames,
            encoded_frames: progress.encoded_frames,
            current_item_progress: progress.current_item_progress,
            elapsed_seconds: progress.elapsed_seconds,
            estimated_seconds_remaining: progress.estimated_seconds_remaining,
            outputs,
            result_counts: counts,
        }
    }
}

pub(crate) struct BatchState {
    inner: Mutex<Option<RunningBatch>>,
    sink: Arc<dyn ProgressSink>,
}

impl BatchState {
    pub(super) fn active_render_id(
        &self,
        batch_id: &str,
    ) -> Result<Option<u64>, BatchServiceError> {
        let inner = self.inner.lock().expect("batch state mutex poisoned");
        let batch = inner
            .as_ref()
            .filter(|batch| batch.batch_id == batch_id)
            .ok_or_else(|| unknown_batch(batch_id))?;
        Ok(batch.renderer_busy.then_some(batch.render_id))
    }

    pub(crate) fn new(sink: Arc<dyn ProgressSink>) -> Self {
        Self {
            inner: Mutex::new(None),
            sink,
        }
    }

    pub(super) fn accept(
        &self,
        render_id: u64,
        batch_id: String,
        jobs: &[AcceptedJob],
    ) -> BatchSnapshot {
        let now = Instant::now();
        *self.inner.lock().expect("batch state mutex poisoned") = Some(RunningBatch {
            render_id,
            batch_id,
            revision: 0,
            snapshot_revision: 0,
            phase: BatchPhase::Accepted,
            renderer_busy: true,
            items: jobs
                .iter()
                .map(|job| BatchItemSnapshot {
                    id: job.id.clone(),
                    phase: BatchItemPhase::Queued,
                    planned_frames: u64::from(job.output.frames),
                    current_frames: 0,
                    rendered_frames: 0,
                    encoded_frames: 0,
                    outcome: None,
                })
                .collect(),
            planned_frames: jobs.iter().map(|job| u64::from(job.output.frames)).sum(),
            processed_frames: 0,
            rendered_frames: 0,
            encoded_frames: 0,
            active: None,
            item_eta: None,
            item_fps: None,
            started: now,
            updated: now,
        });
        match self.update(None, true, |_| {}).expect("accepted batch") {
            BatchRenderEvent::Snapshot(snapshot) => snapshot,
            BatchRenderEvent::Progress(_) => unreachable!("acceptance publishes the full queue"),
        }
    }

    pub(super) fn snapshot(&self, batch_id: &str) -> Result<BatchSnapshot, BatchServiceError> {
        self.inner
            .lock()
            .expect("batch state mutex poisoned")
            .as_ref()
            .filter(|state| state.batch_id == batch_id)
            .map(RunningBatch::snapshot)
            .ok_or_else(|| unknown_batch(batch_id))
    }

    /// One mutation/publication boundary for queue transitions and pipeline observations.
    fn update(
        &self,
        progress: Option<&RenderProgress>,
        transition: bool,
        mutate: impl FnOnce(&mut RunningBatch),
    ) -> Option<BatchRenderEvent> {
        let mut inner = self.inner.lock().expect("batch state mutex poisoned");
        let running = inner.as_mut().filter(|state| {
            progress.is_none_or(|progress| {
                progress.busy
                    && state.render_id == progress.render_id
                    && state.renderer_busy
                    && (state.active.is_some() || progress.status == "cancelling")
            })
        })?;
        let previous_phase = running.phase;
        let previous_item_phase = running.active.map(|(index, _)| running.items[index].phase);
        if let Some(progress) = progress {
            if progress.status == "cancelling" {
                running.phase = BatchPhase::Cancelling;
            }
            if let Some((index, _)) = running.active {
                let item = &mut running.items[index];
                running.processed_frames =
                    running.processed_frames - item.current_frames + u64::from(progress.current);
                running.rendered_frames =
                    running.rendered_frames - item.rendered_frames + u64::from(progress.rendered);
                running.encoded_frames =
                    running.encoded_frames - item.encoded_frames + u64::from(progress.encoded);
                item.current_frames = u64::from(progress.current);
                item.rendered_frames = u64::from(progress.rendered);
                item.encoded_frames = u64::from(progress.encoded);
                if progress.status == "rendering" {
                    item.phase = BatchItemPhase::Rendering;
                    if running.phase != BatchPhase::Cancelling {
                        running.phase = BatchPhase::Rendering;
                    }
                }
                running.item_eta = progress
                    .estimated_seconds_remaining
                    .map(|value| value as f64);
                running.item_fps = progress.rendering_fps;
            }
        }
        mutate(running);
        running.revision += 1;
        running.updated = Instant::now();
        let item_phase = running.active.map(|(index, _)| running.items[index].phase);
        let event =
            if transition || running.phase != previous_phase || item_phase != previous_item_phase {
                running.refresh_frame_totals();
                running.snapshot_revision = running.revision;
                BatchRenderEvent::Snapshot(running.snapshot())
            } else {
                BatchRenderEvent::Progress(running.progress_update())
            };
        drop(inner);
        self.sink.emit_batch_progress(&event);
        Some(event)
    }

    pub(super) fn begin(&self, index: usize) {
        self.update(None, true, |state| {
            if state.phase != BatchPhase::Cancelling {
                state.phase = BatchPhase::Preparing;
            }
            state.items[index].phase = BatchItemPhase::Preparing;
            state.active = Some((index, Instant::now()));
            state.item_eta = None;
            state.item_fps = None;
        });
    }

    pub(super) fn finish_item(
        &self,
        index: usize,
        result: CoreResult<String>,
        progress: &RenderProgress,
        target: &RenderOutputTarget,
    ) {
        self.update(Some(progress), true, |state| {
            let item = &mut state.items[index];
            item.phase = BatchItemPhase::Finished;
            item.outcome = Some(match result {
                Ok(_) => BatchItemOutcome::Succeeded {
                    output_path: target
                        .path()
                        .to_str()
                        .expect("validated Unicode destination")
                        .to_owned(),
                },
                Err(CoreError::Cancelled) => BatchItemOutcome::Cancelled,
                Err(error) => BatchItemOutcome::Failed {
                    message: error.to_string(),
                },
            });
            state.active = None;
            state.item_eta = None;
            state.item_fps = None;
        });
    }

    /// The shared worker calls this after native cleanup and reservation release.
    pub(super) fn finish(&self, result: &CoreResult<Option<String>>) {
        self.update(None, true, |state| {
            let cancelled = matches!(result, Err(CoreError::Cancelled));
            for item in &mut state.items {
                if item.outcome.is_none() {
                    item.outcome = Some(if cancelled {
                        if item.phase == BatchItemPhase::Queued {
                            BatchItemOutcome::Unstarted
                        } else {
                            BatchItemOutcome::Cancelled
                        }
                    } else {
                        BatchItemOutcome::Failed {
                            message: "Batch worker ended before completing this item".into(),
                        }
                    });
                    item.phase = BatchItemPhase::Finished;
                }
            }
            state.active = None;
            state.item_eta = None;
            state.item_fps = None;
            state.renderer_busy = false;
            state.phase = if cancelled {
                BatchPhase::Cancelled
            } else if !state
                .items
                .iter()
                .any(|item| matches!(item.outcome, Some(BatchItemOutcome::Succeeded { .. })))
            {
                BatchPhase::Failed
            } else if state
                .items
                .iter()
                .any(|item| matches!(item.outcome, Some(BatchItemOutcome::Failed { .. })))
            {
                BatchPhase::CompletedWithErrors
            } else {
                BatchPhase::Completed
            };
        });
    }
}

impl ProgressSink for BatchState {
    fn emit_progress(&self, progress: &RenderProgress) {
        self.update(Some(progress), false, |_| {});
    }
}

pub(super) fn unknown_batch(batch_id: &str) -> BatchServiceError {
    BatchServiceError::UnknownBatch {
        message: format!("No retained batch with identity {batch_id}"),
    }
}
