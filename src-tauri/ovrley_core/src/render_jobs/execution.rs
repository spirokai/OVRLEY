//! Native render execution ownership, independent of IPC and frontend events.
//!
//! Acceptance pins resource handles and validates the exact output destination.
//! The supervised operation worker owns configuration/activity preparation and
//! calls the existing synchronous pipelines. Those pipelines own frame workers,
//! writer/monitor joins, FFmpeg shutdown, and incomplete-output cleanup. Only
//! after joining the operation worker does the supervisor publish its outcome
//! and release the renderer. Dropping the service joins its supervisor.

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

/// Owns renderer reservation and background dispatch. Progress is observational;
/// it never joins workers or determines completion.
pub struct RenderExecutionService {
    controller: RenderController,
    supervisor: Mutex<Option<JoinHandle<()>>>,
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
        Self {
            controller,
            supervisor: Mutex::new(None),
        }
    }

    pub fn progress(&self) -> RenderProgress {
        self.controller.progress()
    }

    pub fn cancel(&self) -> bool {
        self.controller.cancel()
    }

    /// Reserves the renderer for an operation's entire lifetime. An eventual
    /// batch holds this same reservation across preparation and all its items.
    pub fn reserve(&self) -> CoreResult<RendererReservation> {
        let render_id = self.controller.reserve()?;
        Ok(RendererReservation {
            controller: self.controller.clone(),
            render_id,
            finalized: false,
        })
    }

    /// Accepts a single render without doing dense activity or asset preparation
    /// on the IPC caller. JSON decoding identifies resource handles; each raster
    /// is validated and resolved once here. All other config validation runs on
    /// the owned worker. Output/overwrite errors still precede activity parsing.
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

    /// Dispatches owned native work. The closure returns the real pipeline
    /// outcome, after cleanup, rather than waiting for a progress event. This
    /// boundary also permits controlled execution in lifecycle tests.
    pub fn dispatch<F>(&self, reservation: RendererReservation, operation: F) -> CoreResult<()>
    where
        F: FnOnce(&RendererReservation) -> CoreResult<String> + Send + 'static,
    {
        assert!(
            self.controller.shares_state(&reservation.controller),
            "the reservation must belong to this execution service"
        );
        let mut supervisor = self
            .supervisor
            .lock()
            .expect("render supervisor mutex poisoned");
        if let Some(previous) = supervisor.take() {
            previous.join().expect("render supervisor panicked");
        }
        *supervisor = Some(
            thread::Builder::new()
                .name("render-supervisor".into())
                .spawn(move || supervise_operation(reservation, operation))
                .map_err(|error| {
                    CoreError::Encode(format!("Could not start render supervisor: {error}"))
                })?,
        );
        Ok(())
    }
}

impl Drop for RenderExecutionService {
    fn drop(&mut self) {
        if let Some(supervisor) = self
            .supervisor
            .get_mut()
            .expect("render supervisor mutex poisoned")
            .take()
        {
            if supervisor.join().is_err() {
                log::error!("Render supervisor panicked during service shutdown");
            }
        }
    }
}

/// Unique session owner. Item starts never clear a requested cancellation or
/// release this reservation. Synchronous callers complete it after cleanup;
/// dispatched work leaves completion to its supervisor.
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

fn supervise_operation<F>(reservation: RendererReservation, operation: F)
where
    F: FnOnce(&RendererReservation) -> CoreResult<String> + Send,
{
    let outcome = thread::scope(|scope| {
        thread::Builder::new()
            .name("render-operation".into())
            .spawn_scoped(scope, || {
                reservation.check_cancelled()?;
                operation(&reservation)
            })
            .map_err(|error| CoreError::Encode(format!("Could not start render worker: {error}")))?
            .join()
            .map_err(|_| CoreError::Encode("Render operation worker panicked".into()))?
    });
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

/// Executes one validated render to its real outcome after pipeline cleanup.
/// Both single requests and future batch items use this seam under the same
/// reservation. Custom range, composite tolerance, and transparent decimation
/// stay owned by the existing planners; activity keeps its original time origin.
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
