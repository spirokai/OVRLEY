//! Native sequential queue. Acceptance owns validated inputs and resources;
//! one execution-service worker owns preparation, cleanup and advancement.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use super::batch_state::BatchState;
use super::contracts::*;
use super::execution::{execute_render, RenderExecutionService, RendererReservation};
use super::inspection::{InspectionRejection, VideoInspectionService};
use super::planning::{plan_batch_item, PlannedPiece, PlannedVideoRender, ValidatedBatchTemplate};
use super::submission::{accept_batch, AcceptedBatch, AcceptedJob};
use crate::activity::schema::ParsedActivity;
use crate::activity::validate_render_activity;
use crate::encode::ffmpeg::assemble::OutputAssembly;
use crate::error::{CoreError, CoreResult, RenderPathError};
use crate::media::prepared_video::check_source_freshness;
use crate::output::{create_parts_directory, RenderOutputTarget};
use crate::paths::AppPaths;
use crate::raster::RasterResourceResolver;

#[derive(Debug, Serialize)]
#[serde(tag = "code", rename_all = "camelCase")]
pub enum BatchServiceError {
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

impl From<CoreError> for BatchServiceError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::OutputInvalid(error) => Self::OutputError { error },
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
        Ok(
            crate::media::mp4_telemetry::extract_activity(&paths.repo_root, &source.metadata.path)?
                .map(|response| response.parsed_activity),
        )
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
        crate::encode::pipeline::assemble::assemble_output(paths, assembly, session.controller())
    }
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
/// A single piece renders straight to the target; several pieces render to parts
/// in a hidden directory next to it and are joined. The parts are removed after
/// a verified output or cancellation and kept after a failure.
fn execute_batch_item(
    paths: &AppPaths,
    template: &ValidatedBatchTemplate,
    external_activity: Option<&(ParsedActivity, f64)>,
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
                let activity = executor
                    .embedded_activity(paths, &piece.source)?
                    .map(|activity| validate_render_activity(&activity).map(|end| (activity, end)))
                    .transpose()?;
                session.check_cancelled()?;
                embedded.insert(&piece.source.source_id, activity);
            }
        }
    }
    let activity_of = |piece: &PlannedPiece| match external_activity {
        Some((activity, end)) => Some((activity, *end)),
        None => embedded[piece.source.source_id.as_str()]
            .as_ref()
            .map(|(activity, end)| (activity, *end)),
    };
    let render = |index: usize, target: &RenderOutputTarget, joined: bool| {
        let piece = &output.pieces[index];
        check_source_freshness(&piece.source)?;
        let activity = activity_of(piece);
        let plan = plan_batch_item(
            template,
            &piece.source,
            piece.window,
            job.piece_offsets[index],
            piece.skip_overlay,
            activity.map(|(_, end)| end),
            piece.frames,
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
        let mut parts = Vec::with_capacity(output.pieces.len());
        let mut frames_before = 0;
        for (index, piece) in output.pieces.iter().enumerate() {
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
            frames_before += piece.frames;
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
            "{error}. The rendered parts were kept in {}",
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
