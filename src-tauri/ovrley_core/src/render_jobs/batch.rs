//! Native sequential queue. Acceptance owns validated inputs and resources;
//! one execution-service worker owns preparation, cleanup and advancement.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use serde::Serialize;

use super::batch_state::BatchState;
use super::contracts::*;
use super::execution::{execute_render, RenderExecutionService, RendererReservation};
use super::inspection::{InspectionRejection, VideoInspectionService};
use super::planning::{plan_batch_item, PlannedVideoRender};
use super::submission::{accept_batch, AcceptedBatch, AcceptedJob};
use crate::activity::schema::ParsedActivity;
use crate::activity::validate_render_activity;
use crate::error::{CoreError, CoreResult, RenderPathError};
use crate::media::prepared_video::check_source_freshness;
use crate::output::RenderOutputTarget;
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

/// Prepares one source while the queue retains its reservation and active row.
fn execute_batch_item(
    paths: &AppPaths,
    template: &super::planning::ValidatedBatchTemplate,
    external_activity: Option<&(ParsedActivity, f64)>,
    job: &AcceptedJob,
    executor: &dyn BatchJobExecutor,
    session: &RendererReservation,
) -> CoreResult<String> {
    check_source_freshness(&job.source)?;
    session.check_cancelled()?;
    let embedded;
    let (activity, end) = match external_activity {
        Some((activity, end)) => (activity, *end),
        None => {
            embedded = executor.embedded_activity(paths, &job.source)?;
            session.check_cancelled()?;
            let end = validate_render_activity(&embedded)?;
            (&embedded, end)
        }
    };
    let plan = plan_batch_item(
        template,
        &job.source,
        job.offset,
        job.skip_overlay,
        end,
        job.output.frames,
        job.output.container_fps,
    )?;
    session.check_cancelled()?;
    check_source_freshness(&job.source)?;
    executor.execute(paths, plan, activity, session, &job.output.target)
}
