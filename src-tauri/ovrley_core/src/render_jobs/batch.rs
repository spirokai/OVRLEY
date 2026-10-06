//! Native sequential queue. Acceptance owns validated inputs and resources;
//! one execution-service worker owns preparation, cleanup and advancement.

use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;

use super::batch_plan::{
    plan_batch_item, validate_batch_encoding, validate_batch_template, PlannedVideoRender,
    ValidatedBatchTemplate,
};
use super::contracts::*;
use super::execution::{execute_render, RenderExecutionService, RendererReservation};
use super::inspection::{
    InspectionRejection, InspectionSourceSelection, InspectionValidation, VideoInspectionService,
};
use crate::activity::schema::ParsedActivity;
use crate::activity::validate_render_activity;
use crate::debug::RenderProgress;
use crate::encode::fps::Fps;
use crate::encode::progress::ProgressSink;
use crate::error::{CoreError, CoreResult};
use crate::media::prepared_video::check_source_freshness;
use crate::output::{plan_batch_output_targets, RenderOutputKind, RenderOutputTarget};
use crate::paths::AppPaths;
use crate::raster::RasterResourceResolver;

#[derive(Debug, Serialize)]
#[serde(tag = "code", rename_all = "camelCase")]
pub enum BatchServiceError {
    InvalidRequest {
        message: String,
    },
    ReinspectionRequired {
        message: String,
        #[serde(flatten)]
        rejection: InspectionRejection,
    },
    RendererBusy {
        message: String,
    },
    UnknownBatch {
        message: String,
    },
    DispatchFailed {
        message: String,
    },
}

impl From<CoreError> for BatchServiceError {
    fn from(error: CoreError) -> Self {
        Self::InvalidRequest {
            message: error.to_string(),
        }
    }
}

/// External execution seam for deterministic service tests. Implementations
/// return only after native work and partial-output cleanup have completed.
pub trait BatchJobExecutor: Send + Sync {
    fn embedded_activity(
        &self,
        paths: &AppPaths,
        source: &InspectedVideoSource,
    ) -> CoreResult<ParsedActivity>;
    fn execute(
        &self,
        paths: &AppPaths,
        plan: PlannedVideoRender,
        activity: &ParsedActivity,
        session: &RendererReservation,
        target: &RenderOutputTarget,
    ) -> CoreResult<String>;
}

struct NativeBatchExecutor;

impl BatchJobExecutor for NativeBatchExecutor {
    fn embedded_activity(
        &self,
        paths: &AppPaths,
        source: &InspectedVideoSource,
    ) -> CoreResult<ParsedActivity> {
        crate::media::mp4_telemetry::extract_activity(&paths.repo_root, &source.metadata.path)?
            .map(|response| response.parsed_activity)
            .ok_or_else(|| {
                CoreError::Activity(format!("No embedded activity in {}", source.metadata.path))
            })
    }

    fn execute(
        &self,
        paths: &AppPaths,
        plan: PlannedVideoRender,
        activity: &ParsedActivity,
        session: &RendererReservation,
        target: &RenderOutputTarget,
    ) -> CoreResult<String> {
        execute_render(paths, plan, activity, session, target)
    }
}

struct AcceptedJob {
    id: String,
    source: Arc<InspectedVideoSource>,
    offset: f64,
    skip_overlay: bool,
    target: RenderOutputTarget,
    frames: u32,
    container_fps: Fps,
}

struct AcceptedBatch {
    template: ValidatedBatchTemplate,
    external_activity: Option<(ParsedActivity, f64)>,
    jobs: Vec<AcceptedJob>,
}

impl RenderExecutionService {
    pub fn submit_batch(
        &self,
        paths: &AppPaths,
        inspection: &VideoInspectionService,
        request: BatchRenderRequest,
        resources: Option<&dyn RasterResourceResolver>,
    ) -> Result<BatchAcceptance, BatchServiceError> {
        self.submit_batch_with_executor(
            paths,
            inspection,
            request,
            resources,
            Arc::new(NativeBatchExecutor),
        )
    }

    pub fn submit_batch_with_executor(
        &self,
        paths: &AppPaths,
        inspection: &VideoInspectionService,
        request: BatchRenderRequest,
        resources: Option<&dyn RasterResourceResolver>,
        executor: Arc<dyn BatchJobExecutor>,
    ) -> Result<BatchAcceptance, BatchServiceError> {
        let reservation = self
            .reserve_with_sink(self.batch.clone())
            .map_err(|error| BatchServiceError::RendererBusy {
                message: error.to_string(),
            })?;
        let accepted = accept_batch(inspection, request, resources)?;
        let batch_id = format!("batch-{}", reservation.render_id());
        let snapshot = self
            .batch
            .accept(reservation.render_id(), batch_id.clone(), &accepted.jobs);
        let state = self.batch.clone();
        let paths = paths.clone();
        let finished = self.batch.clone();
        if let Err(error) = self.dispatch(
            reservation,
            move |session| run_batch(&paths, accepted, executor, &state, session),
            move |outcome| finished.finish(outcome),
        ) {
            let message = error.to_string();
            let outcome = Err(error);
            self.batch.finish(&outcome);
            return Err(BatchServiceError::DispatchFailed { message });
        }
        Ok(BatchAcceptance { batch_id, snapshot })
    }

    pub fn batch_snapshot(&self, batch_id: &str) -> Result<BatchSnapshot, BatchServiceError> {
        self.batch.snapshot(batch_id)
    }

    pub fn cancel_batch(&self, batch_id: &str) -> Result<BatchSnapshot, BatchServiceError> {
        // The controller atomically checks the reservation identity before
        // cancelling, including when a newer operation races this request.
        let state = self.batch.inner.lock().expect("batch state mutex poisoned");
        let active = state
            .as_ref()
            .filter(|state| state.batch_id == batch_id)
            .ok_or_else(|| unknown_batch(batch_id))?;
        let busy = active.renderer_busy;
        let render_id = active.render_id;
        drop(state);
        if busy {
            let _ = self.controller.cancel_session(render_id);
        }
        self.batch_snapshot(batch_id)
    }
}

fn accept_batch(
    inspection: &VideoInspectionService,
    request: BatchRenderRequest,
    resources: Option<&dyn RasterResourceResolver>,
) -> Result<AcceptedBatch, BatchServiceError> {
    if request.jobs.is_empty() || request.jobs.len() > u32::MAX as usize {
        return Err(
            CoreError::Config("Batch must contain a nonempty supported queue".into()).into(),
        );
    }
    let mut ids = HashSet::new();
    for job in &request.jobs {
        if job.id.trim().is_empty() || !ids.insert(&job.id) {
            return Err(CoreError::Config(
                "Batch job identities must be nonempty and distinct".into(),
            )
            .into());
        }
    }
    let offsets = validate_batch_activity(&request.activity, &request.jobs)?;
    let calibration_source_id = match &request.activity {
        BatchActivity::EmbeddedActivity {} => None,
        BatchActivity::ExternalActivity { reference, .. } => reference
            .as_ref()
            .map(|reference| reference.source_id.clone()),
    };
    let owned = match inspection.validate_sources(&InspectionSourceSelection {
        inspection_id: request.inspection_id.clone(),
        source_ids: request
            .jobs
            .iter()
            .map(|job| job.source_id.clone())
            .collect(),
        calibration_source_id,
    }) {
        InspectionValidation::Valid(sources) => sources,
        InspectionValidation::Rejected(rejection) => {
            return Err(BatchServiceError::ReinspectionRequired {
                message: "Batch sources require fresh inspection before submission".into(),
                rejection,
            })
        }
    };
    let encoding = validate_batch_encoding(&request.encoding)?;
    let targets = plan_batch_output_targets(
        Path::new(&request.output_directory),
        match request.encoding.export_mode {
            BatchExportMode::Composite => RenderOutputKind::Composite,
            BatchExportMode::Transparent => RenderOutputKind::Transparent,
        },
        &owned
            .sources()
            .iter()
            .map(|source| PathBuf::from(&source.metadata.path))
            .collect::<Vec<_>>(),
        owned
            .calibration_source()
            .map(|source| Path::new(&source.metadata.path)),
    )?;
    let template = validate_batch_template(request.template, encoding, resources)?;
    let external_activity = match request.activity {
        BatchActivity::EmbeddedActivity {} => None,
        BatchActivity::ExternalActivity { activity, .. } => {
            let activity = crate::activity::normalize_parsed_activity(activity)?;
            let end = validate_render_activity(&activity)?;
            Some((activity, end))
        }
    };
    let jobs = request
        .jobs
        .into_iter()
        .zip(owned.sources())
        .zip(targets)
        .zip(offsets)
        .map(|(((job, source), target), offset)| {
            let (frames, container_fps) = template.output_work(source)?;
            Ok(AcceptedJob {
                id: job.id,
                source: source.clone(),
                offset,
                skip_overlay: job.skip_overlay,
                target,
                frames,
                container_fps,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(AcceptedBatch {
        template,
        external_activity,
        jobs,
    })
}

/// Validate calibration consistency at submission and derive each signed offset once.
fn validate_batch_activity(
    activity: &BatchActivity,
    jobs: &[BatchRenderJob],
) -> CoreResult<Vec<f64>> {
    let BatchActivity::ExternalActivity {
        reference,
        automatic_offsets,
        ..
    } = activity
    else {
        return Ok(vec![0.0; jobs.len()]);
    };
    let correction = match reference {
        None => 0.0,
        Some(reference) => {
            if reference.creation_time.trim().is_empty()
                || !reference.committed_offset_seconds.is_finite()
                || !reference.automatic_offset_seconds.is_finite()
                || automatic_offsets
                    .get(&reference.source_id)
                    .is_some_and(|offset| *offset != reference.automatic_offset_seconds)
            {
                return Err(CoreError::Config(
                    "Batch calibration reference must have a consistent finite baseline".into(),
                ));
            }
            reference.committed_offset_seconds - reference.automatic_offset_seconds
        }
    };
    if automatic_offsets.len() != jobs.len() {
        return Err(CoreError::Config(
            "Batch automatic offsets must identify exactly the queued sources".into(),
        ));
    }
    jobs.iter()
        .map(|job| {
            let automatic = automatic_offsets.get(&job.source_id).ok_or_else(|| {
                CoreError::Config(format!("Missing automatic offset for {}", job.source_id))
            })?;
            let offset = automatic + correction;
            if !automatic.is_finite() || !offset.is_finite() {
                return Err(CoreError::Config(
                    "Batch activity offsets must be finite".into(),
                ));
            }
            Ok(offset)
        })
        .collect()
}

fn run_batch(
    paths: &AppPaths,
    batch: AcceptedBatch,
    executor: Arc<dyn BatchJobExecutor>,
    state: &BatchState,
    session: &RendererReservation,
) -> CoreResult<Option<String>> {
    for (index, job) in batch.jobs.iter().enumerate() {
        session.check_cancelled()?;
        session.begin_item(job.frames, "Preparing batch source...")?;
        state.begin(index);
        let result = catch_unwind(AssertUnwindSafe(|| {
            check_source_freshness(&job.source)?;
            session.check_cancelled()?;
            let embedded;
            let (activity, end) = match &batch.external_activity {
                Some((activity, end)) => (activity, *end),
                None => {
                    embedded = executor.embedded_activity(paths, &job.source)?;
                    session.check_cancelled()?;
                    let end = validate_render_activity(&embedded)?;
                    (&embedded, end)
                }
            };
            let plan = plan_batch_item(
                &batch.template,
                &job.source,
                job.offset,
                job.skip_overlay,
                end,
                job.frames,
                job.container_fps,
            )?;
            session.check_cancelled()?;
            check_source_freshness(&job.source)?;
            executor.execute(paths, plan, activity, session, &job.target)
        }))
        .unwrap_or_else(|_| Err(CoreError::Encode("Batch item worker panicked".into())));
        let result = match result {
            Err(_) if session.check_cancelled().is_err() => Err(CoreError::Cancelled),
            outcome => outcome,
        };
        let cancelled = matches!(result, Err(CoreError::Cancelled));
        state.finish_item(index, result, &session.controller().progress(), &job.target);
        if cancelled {
            return Err(CoreError::Cancelled);
        }
    }
    session.check_cancelled()?;
    Ok(None)
}

struct RunningBatch {
    render_id: u64,
    batch_id: String,
    revision: u64,
    phase: BatchPhase,
    renderer_busy: bool,
    items: Vec<BatchItemSnapshot>,
    active: Option<(usize, Instant)>,
    item_eta: Option<f64>,
    started: Instant,
    updated: Instant,
}

impl RunningBatch {
    fn snapshot(&self) -> BatchSnapshot {
        let mut counts = BatchResultCounts {
            succeeded: 0,
            failed: 0,
            cancelled: 0,
            unstarted: 0,
        };
        let mut outputs = Vec::new();
        let (mut planned, mut processed, mut rendered, mut encoded) = (0, 0, 0, 0);
        for item in &self.items {
            planned += item.planned_frames;
            rendered += item.rendered_frames;
            encoded += item.encoded_frames;
            processed += match &item.outcome {
                Some(BatchItemOutcome::Succeeded { output_path }) => {
                    counts.succeeded += 1;
                    outputs.push(BatchOutput {
                        item_id: item.id.clone(),
                        output_path: output_path.clone(),
                    });
                    item.planned_frames
                }
                Some(BatchItemOutcome::Failed { .. }) => {
                    counts.failed += 1;
                    item.planned_frames
                }
                Some(BatchItemOutcome::Cancelled) => {
                    counts.cancelled += 1;
                    item.current_frames
                }
                Some(BatchItemOutcome::Unstarted) => {
                    counts.unstarted += 1;
                    0
                }
                None => item.current_frames,
            };
        }
        let elapsed = self.updated.duration_since(self.started).as_secs_f64();
        let current_item_progress = self.active.map(|(index, started)| {
            let item = &self.items[index];
            BatchItemProgress {
                item_id: item.id.clone(),
                planned_frames: item.planned_frames,
                current_frames: item.current_frames,
                rendered_frames: item.rendered_frames,
                encoded_frames: item.encoded_frames,
                elapsed_seconds: self.updated.duration_since(started).as_secs_f64(),
                estimated_seconds_remaining: self.item_eta,
            }
        });
        BatchSnapshot {
            batch_id: self.batch_id.clone(),
            revision: self.revision,
            phase: self.phase,
            renderer_busy: self.renderer_busy,
            items: self.items.clone(),
            active_item_id: self.active.map(|(index, _)| self.items[index].id.clone()),
            planned_frames: planned,
            processed_frames: processed,
            rendered_frames: rendered,
            encoded_frames: encoded,
            current_item_progress,
            elapsed_seconds: elapsed,
            // Composite encoding reports zero until finalization; rendered work
            // provides throughput while keeping failed items out of that rate.
            estimated_seconds_remaining: (self.renderer_busy && rendered > 0 && elapsed > 0.0)
                .then(|| planned.saturating_sub(processed) as f64 * elapsed / rendered as f64),
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
    pub(crate) fn new(sink: Arc<dyn ProgressSink>) -> Self {
        Self {
            inner: Mutex::new(None),
            sink,
        }
    }

    fn accept(&self, render_id: u64, batch_id: String, jobs: &[AcceptedJob]) -> BatchSnapshot {
        let now = Instant::now();
        *self.inner.lock().expect("batch state mutex poisoned") = Some(RunningBatch {
            render_id,
            batch_id,
            revision: 0,
            phase: BatchPhase::Accepted,
            renderer_busy: true,
            items: jobs
                .iter()
                .map(|job| BatchItemSnapshot {
                    id: job.id.clone(),
                    phase: BatchItemPhase::Queued,
                    planned_frames: u64::from(job.frames),
                    current_frames: 0,
                    rendered_frames: 0,
                    encoded_frames: 0,
                    outcome: None,
                })
                .collect(),
            active: None,
            item_eta: None,
            started: now,
            updated: now,
        });
        self.update(None, |_| {}).expect("accepted batch")
    }

    fn snapshot(&self, batch_id: &str) -> Result<BatchSnapshot, BatchServiceError> {
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
        mutate: impl FnOnce(&mut RunningBatch),
    ) -> Option<BatchSnapshot> {
        let mut inner = self.inner.lock().expect("batch state mutex poisoned");
        let running = inner.as_mut().filter(|state| {
            progress.is_none_or(|progress| {
                progress.busy
                    && state.render_id == progress.render_id
                    && state.renderer_busy
                    && (state.active.is_some() || progress.status == "cancelling")
            })
        })?;
        if let Some(progress) = progress {
            if progress.status == "cancelling" {
                running.phase = BatchPhase::Cancelling;
            }
            if let Some((index, _)) = running.active {
                let item = &mut running.items[index];
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
            }
        }
        mutate(running);
        running.revision += 1;
        running.updated = Instant::now();
        let snapshot = running.snapshot();
        drop(inner);
        self.sink.emit_batch_progress(&snapshot);
        Some(snapshot)
    }

    fn begin(&self, index: usize) {
        self.update(None, |state| {
            if state.phase != BatchPhase::Cancelling {
                state.phase = BatchPhase::Preparing;
            }
            state.items[index].phase = BatchItemPhase::Preparing;
            state.active = Some((index, Instant::now()));
            state.item_eta = None;
        });
    }

    fn finish_item(
        &self,
        index: usize,
        result: CoreResult<String>,
        progress: &RenderProgress,
        target: &RenderOutputTarget,
    ) {
        self.update(Some(progress), |state| {
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
        });
    }

    /// The shared worker calls this after native cleanup and reservation release.
    fn finish(&self, result: &CoreResult<Option<String>>) {
        self.update(None, |state| {
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
        self.update(Some(progress), |_| {});
    }
}

fn unknown_batch(batch_id: &str) -> BatchServiceError {
    BatchServiceError::UnknownBatch {
        message: format!("No retained batch with identity {batch_id}"),
    }
}
