//! Native render ownership. One worker prepares and executes the operation;
//! pipeline guards clean up before its reservation publishes the outcome.
//! The service joins the worker before reuse and on drop.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use serde::Serialize;

use crate::activity::schema::ParsedActivity;
use crate::activity::{
    build_dense_activity_report_for_timeline, build_dense_activity_report_validated,
    parse_activity_json,
};
use crate::debug::RenderProgress;
use crate::encode::pipeline::composite::render_composite_video;
use crate::encode::pipeline::composite_plan::derive_composite_render_plan;
use crate::encode::pipeline::transparent::{render_video, rendered_frame_count};
use crate::encode::progress::{ProgressSink, RenderController};
use crate::error::{CoreError, CoreResult};
use crate::normalize::{
    parse_config_json, raw::RenderConfig, resolve_render_rasters,
    validate_render_config_with_rasters, ValidatedRaster, ValidatedRenderConfig,
};
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
#[derive(Default)]
pub struct RenderExecutionService {
    controller: RenderController,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl RenderExecutionService {
    pub fn with_sink(sink: Arc<dyn ProgressSink>) -> Self {
        Self::with_controller(RenderController::with_sink(sink))
    }

    /// Allows standalone pipeline callers to observe the same controller.
    pub fn with_controller(controller: RenderController) -> Self {
        Self {
            controller,
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
        let render_id = self.controller.reserve()?;
        Ok(RendererReservation {
            controller: self.controller.clone(),
            render_id,
            finalized: false,
        })
    }

    /// Pins raster handles and validates output before dispatch. Remaining
    /// configuration validation and activity preparation belong to the worker.
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
        let config = parse_config_json(config_json)?;
        let rasters = resolve_render_rasters(&config.rasters, resources)?;
        let kind = if config.scene.composite_video_path.is_some() {
            RenderOutputKind::Composite
        } else {
            RenderOutputKind::Transparent
        };
        let target = RenderOutputTarget::validate(output_path, kind, overwrite)?;
        let request = AcceptedSingleRender {
            config,
            rasters,
            activity_json: parsed_activity_json.to_owned(),
            target,
        };
        let reservation = self.reserve()?;
        let accepted = RenderAccepted {
            started: true,
            render_id: reservation.render_id,
            output_path: request.target.path().to_path_buf(),
        };
        let paths = paths.clone();
        self.dispatch(reservation, move |session| {
            execute_single(&paths, request, session)
        })?;
        Ok(accepted)
    }

    /// Dispatches an operation returning its actual outcome after cleanup.
    pub fn dispatch<F>(&self, reservation: RendererReservation, operation: F) -> CoreResult<()>
    where
        F: FnOnce(&RendererReservation) -> CoreResult<String> + Send + 'static,
    {
        assert!(
            self.controller.shares_state(&reservation.controller),
            "the reservation must belong to this execution service"
        );
        let mut worker = self.worker.lock().expect("render worker mutex poisoned");
        if let Some(previous) = worker.take() {
            previous.join().expect("render worker panicked");
        }
        *worker = Some(
            thread::Builder::new()
                .name("render-operation".into())
                .spawn(move || run_operation(reservation, operation))
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
    pub fn complete(mut self, outcome: CoreResult<String>) -> CoreResult<String> {
        self.finalized = true;
        match &outcome {
            Ok(filename) => self.controller.finish_success(filename.clone()),
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

fn run_operation<F>(reservation: RendererReservation, operation: F)
where
    F: FnOnce(&RendererReservation) -> CoreResult<String>,
{
    // Move accepted resources into the unwind boundary so they are released
    // even when cancellation prevents the operation from being called.
    let session = &reservation;
    let outcome = catch_unwind(AssertUnwindSafe(move || {
        session.check_cancelled()?;
        operation(session)
    }));
    let outcome = outcome
        .unwrap_or_else(|_| Err(CoreError::Encode("Render operation worker panicked".into())));
    let _ = reservation.complete(outcome);
}

/// Owned acceptance inputs; private fields prevent replacing pinned rasters or
/// pairing them with another raw configuration after ingress.
struct AcceptedSingleRender {
    config: RenderConfig,
    rasters: Vec<ValidatedRaster>,
    activity_json: String,
    target: RenderOutputTarget,
}

/// Completes single-request ingress on the worker before shared execution.
fn execute_single(
    paths: &AppPaths,
    request: AcceptedSingleRender,
    session: &RendererReservation,
) -> CoreResult<String> {
    session.check_cancelled()?;
    let config = validate_render_config_with_rasters(request.config, request.rasters)?;
    session.check_cancelled()?;
    let activity = parse_activity_json(&request.activity_json)?;
    session.check_cancelled()?;
    execute_render(paths, config, &activity, session, &request.target)
}

/// Shared synchronous execution; existing planners retain timing ownership.
pub fn execute_render(
    paths: &AppPaths,
    mut config: ValidatedRenderConfig,
    activity: &ParsedActivity,
    session: &RendererReservation,
    target: &RenderOutputTarget,
) -> CoreResult<String> {
    session.check_cancelled()?;
    if config.scene.composite_video_path.is_some() {
        let activity_end = activity.trim_end_seconds.max(
            activity
                .sample_elapsed_seconds
                .last()
                .copied()
                .unwrap_or_default(),
        );
        let plan = derive_composite_render_plan(&mut config.scene, Some(activity_end))?;
        session.check_cancelled()?;
        session.begin_item(plan.output_frame_count, "Preparing composite assets...")?;
        let dense = build_dense_activity_report_for_timeline(
            activity,
            &config,
            plan.overlay_pipe_fps
                .timeline_for_duration(plan.activity_overlap_duration)?,
        )?;
        session.check_cancelled()?;
        render_composite_video(
            paths,
            &config,
            activity,
            &dense,
            session.controller(),
            plan,
            true,
            target,
        )
    } else {
        let dense = build_dense_activity_report_validated(activity, &config)?;
        session.check_cancelled()?;
        let total = u32::try_from(rendered_frame_count(
            dense.frame_count,
            config.widget_update_rate(),
        )?)
        .map_err(|_| CoreError::Encode("Transparent progress frame count exceeds u32".into()))?;
        session.begin_item(total, "Preparing render assets...")?;
        render_video(
            paths,
            &config,
            activity,
            &dense,
            session.controller(),
            target,
        )
    }
}
