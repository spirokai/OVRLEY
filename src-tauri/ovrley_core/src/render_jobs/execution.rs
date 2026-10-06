//! Native render ownership. One worker prepares and executes the operation;
//! pipeline guards clean up before its reservation publishes the outcome.
//! The service joins the worker before reuse and on drop.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use serde::Serialize;

use super::batch::BatchState;
use super::batch_plan::{plan_single_render, PlannedVideoRender, VideoRenderModePlan};
use crate::activity::schema::ParsedActivity;
use crate::activity::{parse_activity_json, validate_render_activity};
use crate::debug::RenderProgress;
use crate::encode::pipeline::composite::render_inspected_composite_video;
use crate::encode::pipeline::composite_plan::verify_composite_source_resolution;
use crate::encode::pipeline::transparent::render_planned_video;
use crate::encode::progress::{ProgressSink, RenderController};
use crate::error::{CoreError, CoreResult};
use crate::normalize::{parse_config_json, validate_render_config_with_resources};
use crate::output::{RenderOutputKind, RenderOutputTarget};
use crate::paths::AppPaths;
use crate::raster::RasterResourceResolver;

/// Existing single-render acceptance shape, serialized directly by the shell.
#[derive(Debug, Serialize)]
pub struct RenderAccepted {
    pub started: bool,
    pub render_id: u64,
    #[serde(rename = "outputPath")]
    pub output_path: PathBuf,
}

/// Owns the background worker; progress events never supervise execution.
pub struct RenderExecutionService {
    pub(crate) controller: RenderController,
    pub(crate) batch: Arc<BatchState>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Default for RenderExecutionService {
    fn default() -> Self {
        Self::with_controller(RenderController::default())
    }
}

impl RenderExecutionService {
    pub fn with_sink(sink: Arc<dyn ProgressSink>) -> Self {
        Self::with_controller(RenderController::with_sink(sink))
    }

    /// Allows standalone pipeline callers to observe the same controller.
    pub fn with_controller(controller: RenderController) -> Self {
        let batch = Arc::new(BatchState::new(controller.sink()));
        Self {
            controller,
            batch,
            worker: Mutex::new(None),
        }
    }

    pub fn progress(&self) -> RenderProgress {
        self.controller.progress()
    }

    pub fn cancel(&self) -> bool {
        self.controller.cancel()
    }

    /// Held across preparation, cleanup, and all items of an eventual batch.
    pub fn reserve(&self) -> CoreResult<RendererReservation> {
        self.reserve_with_sink(self.controller.sink())
    }

    /// Each reservation has one progress destination: the single-render sink
    /// or internal batch state. Pipeline code uses the same controller API.
    pub(crate) fn reserve_with_sink(
        &self,
        sink: Arc<dyn ProgressSink>,
    ) -> CoreResult<RendererReservation> {
        let render_id = self.controller.reserve(sink)?;
        let reservation = RendererReservation {
            controller: self.controller.clone(),
            render_id,
            finalized: false,
        };
        // A previous worker may still be publishing its terminal batch result
        // after releasing the controller. Join it before accepting new inputs.
        if let Some(previous) = self
            .worker
            .lock()
            .expect("render worker mutex poisoned")
            .take()
        {
            previous.join().expect("render worker panicked");
        }
        Ok(reservation)
    }

    /// Validates config/activity and pins resources at submission ingress.
    /// Dense activity, source probing and native rendering stay on the worker.
    #[allow(clippy::too_many_arguments)]
    pub fn submit_single(
        &self,
        paths: &AppPaths,
        config_json: &str,
        parsed_activity_json: &str,
        output_path: &str,
        overwrite: bool,
        resources: Option<&dyn RasterResourceResolver>,
    ) -> CoreResult<RenderAccepted> {
        let config =
            validate_render_config_with_resources(parse_config_json(config_json)?, resources)?;
        let kind = if config.scene.composite_video_path.is_some() {
            RenderOutputKind::Composite
        } else {
            RenderOutputKind::Transparent
        };
        let target = RenderOutputTarget::validate(output_path, kind, overwrite)?;
        let activity = parse_activity_json(parsed_activity_json)?;
        let end = validate_render_activity(&activity)?;
        let plan = plan_single_render(config, end)?;
        let reservation = self.reserve()?;
        let accepted = RenderAccepted {
            started: true,
            render_id: reservation.render_id,
            output_path: target.path().to_path_buf(),
        };
        let paths = paths.clone();
        self.dispatch(
            reservation,
            move |session| {
                session.begin_item(plan.planned_frames(), "Preparing video assets...")?;
                execute_render(&paths, plan, &activity, session, &target)
            },
            |_| {},
        )?;
        Ok(accepted)
    }

    /// Dispatches an operation returning its actual outcome after cleanup.
    pub fn dispatch<T, F, C>(
        &self,
        reservation: RendererReservation,
        operation: F,
        completed: C,
    ) -> CoreResult<()>
    where
        T: Clone + Into<Option<String>> + Send + 'static,
        F: FnOnce(&RendererReservation) -> CoreResult<T> + Send + 'static,
        C: FnOnce(&CoreResult<T>) + Send + 'static,
    {
        assert!(
            self.controller.shares_state(&reservation.controller),
            "the reservation must belong to this execution service"
        );
        let mut worker = self.worker.lock().expect("render worker mutex poisoned");
        assert!(worker.is_none(), "reservation joins the previous worker");
        *worker = Some(
            thread::Builder::new()
                .name("render-operation".into())
                .spawn(move || {
                    // Accepted inputs leave scope before the reservation is released,
                    // including cancellation before preparation and worker unwinding.
                    let session = &reservation;
                    let outcome = catch_unwind(AssertUnwindSafe(move || {
                        session.check_cancelled()?;
                        operation(session)
                    }))
                    .unwrap_or_else(|_| {
                        Err(CoreError::Encode("Render operation worker panicked".into()))
                    });
                    let outcome = reservation.complete(outcome);
                    completed(&outcome);
                })
                .map_err(|error| {
                    CoreError::Encode(format!("Could not start render worker: {error}"))
                })?,
        );
        Ok(())
    }
}

impl Drop for RenderExecutionService {
    fn drop(&mut self) {
        if let Some(worker) = self
            .worker
            .get_mut()
            .expect("render worker mutex poisoned")
            .take()
        {
            if worker.join().is_err() {
                log::error!("Render worker panicked during service shutdown");
            }
        }
    }
}

/// Unique session owner; item resets preserve cancellation and reservation.
pub struct RendererReservation {
    controller: RenderController,
    render_id: u64,
    finalized: bool,
}

impl RendererReservation {
    pub(crate) fn render_id(&self) -> u64 {
        self.render_id
    }
    pub fn controller(&self) -> &RenderController {
        &self.controller
    }

    pub fn check_cancelled(&self) -> CoreResult<()> {
        self.controller.check_cancelled()
    }

    pub fn begin_item(&self, total_frames: u32, message: &str) -> CoreResult<()> {
        self.controller.begin_item(total_frames, message)
    }

    /// Returns the synchronous operation's actual result and finalizes once.
    /// Cancellation arriving after a successful pipeline has preserved its
    /// output cannot turn that completed output into an interrupted one.
    pub fn complete<T: Clone + Into<Option<String>>>(
        mut self,
        outcome: CoreResult<T>,
    ) -> CoreResult<T> {
        self.finalized = true;
        match &outcome {
            Ok(filename) => self.controller.finish_success(filename.clone().into()),
            Err(error) => self
                .controller
                .finish_error(error.to_string(), matches!(error, CoreError::Cancelled)),
        }
        outcome
    }
}

impl Drop for RendererReservation {
    fn drop(&mut self) {
        if !self.finalized {
            self.controller
                .finish_error("Render operation ended without an outcome".into(), false);
        }
    }
}

/// Executes a finalized single or batch plan. The caller owns item start and
/// the reservation; pipelines return only after native cleanup.
pub fn execute_render(
    paths: &AppPaths,
    plan: PlannedVideoRender,
    activity: &ParsedActivity,
    session: &RendererReservation,
    target: &RenderOutputTarget,
) -> CoreResult<String> {
    session.check_cancelled()?;
    let dense = plan.prepare_activity(activity)?;
    session.check_cancelled()?;
    match plan.mode {
        VideoRenderModePlan::Composite {
            render,
            source_metadata,
        } => {
            let (has_audio, rotation) = match source_metadata {
                Some(metadata) => metadata,
                None => {
                    let (rotation, audio) = verify_composite_source_resolution(
                        paths,
                        &render.video_path,
                        plan.config.scene.presentation.width,
                        plan.config.scene.presentation.height,
                    )?;
                    (audio, rotation)
                }
            };
            session.check_cancelled()?;
            render_inspected_composite_video(
                paths,
                &plan.config,
                activity,
                &dense,
                session.controller(),
                render,
                has_audio,
                rotation,
                target,
            )
        }
        VideoRenderModePlan::Transparent(render) => render_planned_video(
            paths,
            &plan.config,
            activity,
            &dense,
            session.controller(),
            target,
            &render,
        ),
    }
}
