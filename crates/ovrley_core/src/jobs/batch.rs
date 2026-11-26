//! The native sequential batch queue: acceptance validates and pins inputs, the authoritative
//! queue state makes every transition, and one execution-service worker owns preparation,
//! cleanup and advancement. Sinks only observe compact snapshots.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::Arc;
use serde::Serialize;
use super::contracts::{
    BatchAcceptance, BatchActivity, BatchItemOutcome, BatchItemPhase, BatchItemProgress,
    BatchItemSnapshot, BatchOutput, BatchPhase, BatchProgressUpdate, BatchRenderEvent,
    BatchRenderRequest, BatchResultCounts, BatchSnapshot,
};
use crate::media::probe::InspectedVideoSource;
use super::execution::{execute_render, RenderExecutionService, RendererReservation};
use super::inspection::{InspectionRejection, VideoInspectionService};
use super::planning::{plan_batch_item, PlannedPiece, PlannedVideoRender, ValidatedBatchTemplate};
use crate::activity::schema::ParsedActivity;
use crate::activity::validate_render_activity;
use crate::encode::ffmpeg::assemble::OutputAssembly;
use crate::error::{CoreError, CoreResult, RenderPathError};
use crate::media::probe::check_source_freshness;
use crate::output::{create_parts_directory, RenderOutputTarget};
use crate::paths::AppPaths;
use crate::raster::RasterResourceResolver;
use ovrley_map::gpu::MapSource;
use crate::encode::progress::{RenderProgress, RenderStatus};
use crate::encode::progress::ProgressSink;
use std::sync::Mutex;
use std::time::Instant;
use super::inspection::{InspectionSourceSelection, InspectionValidation};
use super::planning::{
    referenced_source_ids, validate_batch_encoding, validate_batch_outputs,
    validate_batch_template, PlannedBatchOutput, };
use std::collections::HashSet;

#[derive(Debug, Serialize)]
#[serde(tag = "code", rename_all = "camelCase")]
pub enum BatchServiceError {
    /// A combined output file exists and was not cleared for overwriting.
    #[serde(rename = "already_exists")]
    AlreadyExists {
        message: String,
    },
    #[serde(rename = "output_error")]
    OutputError {
        #[serde(flatten)]
        error: RenderPathError,
    },
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

impl std::fmt::Display for BatchServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyExists { message }
            | Self::InvalidRequest { message }
            | Self::ReinspectionRequired { message, .. }
            | Self::RendererBusy { message }
            | Self::UnknownBatch { message }
            | Self::DispatchFailed { message } => formatter.write_str(message),
            Self::OutputError { error } => write!(formatter, "{error}"),
        }
    }
}

impl From<CoreError> for BatchServiceError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::OutputInvalid(error) => Self::OutputError { error },
            CoreError::OutputExists(message) => Self::AlreadyExists { message },
            CoreError::OutputIo { path, source } => Self::OutputError {
                error: RenderPathError::output_io(path, source),
            },
            error => Self::InvalidRequest {
                message: error.to_string(),
            },
        }
    }
}

/// External execution seam for deterministic service tests. Implementations
/// return only after native work and partial-output cleanup have completed.
pub trait BatchJobExecutor: Send + Sync {
    /// The source's embedded telemetry, or None when it has none.
    fn embedded_activity(
        &self,
        paths: &AppPaths,
        source: &InspectedVideoSource,
    ) -> CoreResult<Option<ParsedActivity>>;
    /// Renders a plan; without an activity the plan contains rasters only.
    fn execute(
        &self,
        paths: &AppPaths,
        plan: PlannedVideoRender,
        activity: Option<&ParsedActivity>,
        session: &RendererReservation,
        target: &RenderOutputTarget,
    ) -> CoreResult<String>;
    /// Joins the rendered parts of a multi-piece output into its target. Returns
    /// after the target is verified, or after removing a partial target.
    fn assemble(
        &self,
        paths: &AppPaths,
        assembly: &OutputAssembly,
        session: &RendererReservation,
    ) -> CoreResult<()>;
}

struct NativeBatchExecutor;

impl BatchJobExecutor for NativeBatchExecutor {
    fn embedded_activity(
        &self,
        paths: &AppPaths,
        source: &InspectedVideoSource,
    ) -> CoreResult<Option<ParsedActivity>> {
        crate::media::telemetry::extract_activity(&paths.repo_root, &source.metadata.path)
    }

    fn execute(
        &self,
        paths: &AppPaths,
        plan: PlannedVideoRender,
        activity: Option<&ParsedActivity>,
        session: &RendererReservation,
        target: &RenderOutputTarget,
    ) -> CoreResult<String> {
        execute_render(paths, plan, activity, session, target)
    }

    fn assemble(
        &self,
        paths: &AppPaths,
        assembly: &OutputAssembly,
        session: &RendererReservation,
    ) -> CoreResult<()> {
        crate::encode::pipeline::assemble_output(paths, assembly, session.controller())
    }
}

impl RenderExecutionService {
    pub fn submit_batch(
        &self,
        paths: &AppPaths,
        inspection: &VideoInspectionService,
        request: BatchRenderRequest,
        resources: Option<&dyn RasterResourceResolver>,
        map_source: Option<MapSource>,
    ) -> Result<BatchAcceptance, BatchServiceError> {
        self.submit_batch_with_executor(
            paths,
            inspection,
            request,
            resources,
            map_source,
            Arc::new(NativeBatchExecutor),
        )
    }

    pub fn submit_batch_with_executor(
        &self,
        paths: &AppPaths,
        inspection: &VideoInspectionService,
        request: BatchRenderRequest,
        resources: Option<&dyn RasterResourceResolver>,
        map_source: Option<MapSource>,
        executor: Arc<dyn BatchJobExecutor>,
    ) -> Result<BatchAcceptance, BatchServiceError> {
        let reservation = self
            .reserve_with_sink(self.batch.clone())
            .map_err(|error| BatchServiceError::RendererBusy {
                message: error.to_string(),
            })?;
        let accepted = accept_batch(inspection, request, resources, map_source)?;
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
        if let Some(render_id) = self.batch.active_render_id(batch_id)? {
            let _ = self.controller.cancel_session(render_id);
        }
        self.batch_snapshot(batch_id)
    }
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
        session.begin_item(job.output.frames, "Preparing batch source...")?;
        state.begin(index);
        let result = catch_unwind(AssertUnwindSafe(|| {
            execute_batch_item(
                paths,
                &batch.template,
                batch.external_activity.as_ref(),
                job,
                executor.as_ref(),
                session,
            )
        }))
        .unwrap_or_else(|_| Err(CoreError::Encode("Batch item worker panicked".into())));
        let cancelled = matches!(result, Err(CoreError::Cancelled));
        state.finish_item(
            index,
            result,
            &session.controller().progress(),
            &job.output.target,
        );
        if cancelled {
            return Err(CoreError::Cancelled);
        }
    }
    session.check_cancelled()?;
    Ok(None)
}

/// The activity a source's pieces render with, and its end second.
type PieceActivity = Option<(ParsedActivity, f64)>;

/// Renders one output while the queue retains its reservation and active row.
/// A single piece renders straight to the target; otherwise every planned segment
/// (pieces, or their heads, bodies and tails next to transitions) renders to a
/// part in a hidden directory next to it and the parts are joined. The parts are removed after
/// a verified output or cancellation and kept after a failure.
fn execute_batch_item(
    paths: &AppPaths,
    template: &ValidatedBatchTemplate,
    external_activity: Option<&(Arc<ParsedActivity>, f64, Vec<usize>)>,
    job: &AcceptedJob,
    executor: &dyn BatchJobExecutor,
    session: &RendererReservation,
) -> CoreResult<String> {
    let output = &job.output;
    // Embedded telemetry is read once per source of the output.
    let mut embedded: HashMap<&str, PieceActivity> = HashMap::new();
    if external_activity.is_none() {
        for piece in &output.pieces {
            if !embedded.contains_key(piece.source.source_id.as_str()) {
                check_source_freshness(&piece.source)?;
                session.check_cancelled()?;
                let activity = match executor
                    .embedded_activity(paths, &piece.source)?
                    .map(|activity| validate_render_activity(&activity).map(|end| (activity, end)))
                    .transpose()
                {
                    Ok(activity) => activity,
                    // Unusable embedded telemetry renders like a video without any.
                    Err(CoreError::Activity(error)) => {
                        log::warn!(
                            "Ignoring unusable telemetry of {}: {error}",
                            piece.source.metadata.path
                        );
                        None
                    }
                    Err(error) => return Err(error),
                };
                session.check_cancelled()?;
                embedded.insert(&piece.source.source_id, activity);
            }
        }
    }
    // Embedded telemetry carries no editor lap selection.
    let disabled_laps = external_activity.map_or(&[][..], |(_, _, laps)| laps.as_slice());
    let activity_of = |piece: &PlannedPiece| match external_activity {
        Some((activity, end, _)) => Some((activity.as_ref(), *end)),
        None => embedded[piece.source.source_id.as_str()]
            .as_ref()
            .map(|(activity, end)| (activity, *end)),
    };
    let render = |index: usize, target: &RenderOutputTarget, joined: bool| {
        let segment = &output.segments[index];
        let piece = &output.pieces[segment.piece];
        check_source_freshness(&piece.source)?;
        let activity = activity_of(piece);
        let plan = plan_batch_item(
            template,
            &piece.source,
            segment.window,
            job.piece_offsets[segment.piece],
            piece.skip_overlay,
            activity.map(|(_, end)| end),
            disabled_laps,
            segment.frames,
            output.container_fps,
        )?;
        let plan = if joined { plan.without_audio() } else { plan };
        session.check_cancelled()?;
        check_source_freshness(&piece.source)?;
        executor.execute(
            paths,
            plan,
            activity.map(|(activity, _)| activity),
            session,
            target,
        )
    };
    if output.pieces.len() == 1 {
        return render(0, &output.target, false);
    }
    let parts_directory = create_parts_directory(&output.target)?;
    let result = (|| {
        let mut parts = Vec::with_capacity(output.segments.len());
        let mut frames_before = 0;
        for (index, segment) in output.segments.iter().enumerate() {
            let part = parts_directory.join(format!(
                "part-{index:04}.{}",
                template.output_kind().extension()
            ));
            let target = RenderOutputTarget::validate(
                part.to_str().expect("validated Unicode output"),
                template.output_kind(),
                false,
            )?;
            session.controller().begin_piece(frames_before);
            render(index, &target, true)?;
            frames_before += segment.frames;
            parts.push(part);
        }
        session
            .controller()
            .set_item_message("Joining output pieces...");
        let assembly = template.plan_assembly(output, parts_directory.clone(), parts);
        executor.assemble(paths, &assembly, session)?;
        Ok(output.target.filename().to_owned())
    })();
    match result {
        Ok(_) | Err(CoreError::Cancelled) => {
            remove_parts_directory(&parts_directory);
            result
        }
        Err(error) => Err(CoreError::Encode(format!(
            "{}. The rendered parts were kept in {}",
            error.encode_message(),
            parts_directory.display()
        ))),
    }
}

fn remove_parts_directory(directory: &Path) {
    if let Err(error) = std::fs::remove_dir_all(directory) {
        log::warn!(
            "Could not remove rendered parts {}: {error}",
            directory.display()
        );
    }
}

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
    fn active_render_id(
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
                    && (state.active.is_some() || progress.status == RenderStatus::Cancelling)
            })
        })?;
        let previous_phase = running.phase;
        let previous_item_phase = running.active.map(|(index, _)| running.items[index].phase);
        if let Some(progress) = progress {
            if progress.status == RenderStatus::Cancelling {
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
                if progress.status == RenderStatus::Rendering {
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

    fn finish_item(
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

fn unknown_batch(batch_id: &str) -> BatchServiceError {
    BatchServiceError::UnknownBatch {
        message: format!("No retained batch with identity {batch_id}"),
    }
}

pub(crate) struct AcceptedJob {
    pub(super) id: String,
    pub(super) output: PlannedBatchOutput,
    /// Per piece, in piece order: the activity second where frame 0 of the
    /// piece's source sits (0 for embedded telemetry).
    pub(super) piece_offsets: Vec<f64>,
}

struct AcceptedBatch {
    pub(super) template: ValidatedBatchTemplate,
    /// External activity, its timeline end, and its disabled laps.
    pub(super) external_activity: Option<(Arc<ParsedActivity>, f64, Vec<usize>)>,
    pub(super) jobs: Vec<AcceptedJob>,
}

fn accept_batch(
    inspection: &VideoInspectionService,
    request: BatchRenderRequest,
    resources: Option<&dyn RasterResourceResolver>,
    map_source: Option<MapSource>,
) -> Result<AcceptedBatch, BatchServiceError> {
    if request.outputs.is_empty() || request.outputs.len() > u32::MAX as usize {
        return Err(
            CoreError::Config("Batch must contain a nonempty supported queue".into()).into(),
        );
    }
    let mut ids = HashSet::new();
    for output in &request.outputs {
        if output.id.trim().is_empty() || !ids.insert(&output.id) {
            return Err(CoreError::Config(
                "Batch output identities must be nonempty and distinct".into(),
            )
            .into());
        }
    }
    validate_batch_outputs(&request.outputs, &request.destination)?;
    let source_ids = referenced_source_ids(&request.outputs);
    let offsets = validate_batch_activity(&request.activity, &source_ids)?;
    let owned = match inspection.validate_sources(&InspectionSourceSelection {
        inspection_id: request.inspection_id.clone(),
        source_ids,
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
    let planned = encoding.plan_outputs(&request.outputs, &owned, &request.destination)?;
    let template =
        validate_batch_template(request.template, encoding, &owned, resources, map_source)?;
    let external_activity = match request.activity {
        BatchActivity::EmbeddedActivity {} => None,
        BatchActivity::ExternalActivity {
            activity,
            disabled_laps,
            ..
        } => {
            let end = validate_render_activity(&activity)?;
            let disabled_laps =
                crate::config::validate_disabled_laps(disabled_laps, "activity.disabledLaps")?;
            Some((activity, end, disabled_laps))
        }
    };
    let jobs = request
        .outputs
        .into_iter()
        .zip(planned)
        .map(|(output, planned)| AcceptedJob {
            // Embedded telemetry is video-local: frame 0 is activity second 0.
            piece_offsets: output
                .pieces
                .iter()
                .map(|piece| {
                    offsets
                        .as_ref()
                        .map_or(0.0, |offsets| offsets[&piece.source_id])
                })
                .collect(),
            id: output.id,
            output: planned,
        })
        .collect();
    Ok(AcceptedBatch {
        template,
        external_activity,
        jobs,
    })
}

/// Validate the committed offsets at submission: exactly one finite offset per
/// referenced source with an external activity; None in embedded mode.
fn validate_batch_activity(
    activity: &BatchActivity,
    source_ids: &[String],
) -> CoreResult<Option<HashMap<String, f64>>> {
    let BatchActivity::ExternalActivity { offsets, .. } = activity else {
        return Ok(None);
    };
    if offsets.len() != source_ids.len() || source_ids.iter().any(|id| !offsets.contains_key(id)) {
        return Err(CoreError::Config(
            "Batch offsets must identify exactly the queued sources".into(),
        ));
    }
    if offsets.values().any(|offset| !offset.is_finite()) {
        return Err(CoreError::Config(
            "Batch activity offsets must be finite".into(),
        ));
    }
    Ok(Some(offsets.clone()))
}
