//! Queue behavior through the public service, with controlled native execution.
//! These scenarios cover ownership/races without launching FFmpeg for each item.

mod common;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use ovrley_core::activity::schema::ParsedActivity;
use ovrley_core::debug::RenderProgress;
use ovrley_core::encode::ffmpeg::assemble::{AssemblySegment, OutputAssembly};
use ovrley_core::encode::progress::ProgressSink;
use ovrley_core::encode::quality::QualityType;
use ovrley_core::error::{CoreError, CoreResult};
use ovrley_core::media::SourceVideoMetadata;
use ovrley_core::output::RenderOutputTarget;
use ovrley_core::paths::AppPaths;
use ovrley_core::render_jobs::batch::{BatchJobExecutor, BatchServiceError};
use ovrley_core::render_jobs::contracts::*;
use ovrley_core::render_jobs::execution::{RenderExecutionService, RendererReservation};
use ovrley_core::render_jobs::inspection::{SourceMetadataProbe, VideoInspectionService};
use ovrley_core::render_jobs::planning::{
    plan_batch_configuration, BatchPlanningResponse, PlannedVideoRender, VideoRenderModePlan,
};
use serde_json::json;

const WAIT: Duration = Duration::from_secs(10);
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

struct Fixture {
    directory: PathBuf,
    paths: AppPaths,
    inspection: VideoInspectionService,
}

struct Probe;
impl SourceMetadataProbe for Probe {
    fn probe(&self, _: &AppPaths, path: &str) -> CoreResult<SourceVideoMetadata> {
        let name = PathBuf::from(path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let duration = name.chars().next().unwrap().to_digit(10).unwrap() as f64 * 2.0;
        let width = if name.contains("wide") { 128 } else { 64 };
        Ok(serde_json::from_value(json!({
            "path":path, "duration":duration, "fps":30, "fpsNum":30, "fpsDen":1,
            "resolution":{"width":width,"height":32}, "hasAudio":false,
            "creationTime":"2026-05-20T12:00:00Z", "timeSource":"gps"
        }))
        .unwrap())
    }
}

impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "ovrley-batch-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let paths =
            AppPaths::from_runtime_roots(directory.clone(), directory.clone(), directory.clone());
        Self {
            directory,
            paths,
            inspection: VideoInspectionService::with_probe(Arc::new(Probe)),
        }
    }

    fn request(&self, names: &[&str], external: bool) -> BatchRenderRequest {
        let session = self.inspection.create_session();
        let sources = names
            .iter()
            .map(|name| {
                let path = self.directory.join(name);
                fs::write(&path, b"inspected source").unwrap();
                self.inspection
                    .inspect_source(&self.paths, &session.inspection_id, path.to_str().unwrap())
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let encoding = BatchEncodingSettings {
            export_mode: BatchExportMode::Transparent,
            export_codec: "qtrle".into(),
            fps: 30,
            update_rate: 2,
            quality_type: QualityType::Bitrate,
            quality_value: 20.0,
            qsv_full_init_args: None,
        };
        let outputs = sources
            .iter()
            .enumerate()
            .map(|(index, source)| BatchRenderOutput {
                id: format!("item-{index}"),
                pieces: vec![common::builders::whole_source_piece(source, index == 0)],
                transitions: Vec::new(),
            })
            .collect::<Vec<_>>();
        let destination = BatchDestination::SourceNamed {
            directory: self.directory.to_str().unwrap().into(),
        };
        let selection = BatchOutputSelection {
            inspection_id: session.inspection_id.clone(),
            outputs: outputs.clone(),
            destination: destination.clone(),
        };
        let BatchPlanningResponse::Planned { .. } =
            plan_batch_configuration(&self.inspection, &selection, &encoding).unwrap()
        else {
            panic!("fresh sources")
        };
        let mut template = common::builders::batch_template();
        template.rasters.push(
            serde_json::from_value(json!({
                "id":"logo", "x":2, "y":3, "width":4, "height":5,
                "rotation":0, "opacity":1, "path":self.directory.join("logo.png")
            }))
            .unwrap(),
        );
        BatchRenderRequest {
            inspection_id: session.inspection_id,
            template,
            encoding,
            activity: if external {
                BatchActivity::ExternalActivity {
                    activity: activity(),
                    offsets: sources
                        .iter()
                        .map(|source| (source.source_id.clone(), 0.0))
                        .collect(),
                    disabled_laps: Vec::new(),
                }
            } else {
                BatchActivity::EmbeddedActivity {}
            },
            destination,
            outputs,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).unwrap();
    }
}

fn activity() -> ParsedActivity {
    serde_json::from_value(json!({"sample_elapsed_seconds":[0,120], "trim_end_seconds":120, "speed":[1,2], "sync_time":"2026-05-20T12:00:00Z"})).unwrap()
}

struct Sink {
    single_progress: Mutex<Vec<RenderProgress>>,
    snapshots: Mutex<Vec<BatchSnapshot>>,
    events: Mutex<Vec<BatchRenderEvent>>,
    terminal: mpsc::Sender<BatchSnapshot>,
}

impl ProgressSink for Sink {
    fn emit_progress(&self, progress: &RenderProgress) {
        self.single_progress.lock().unwrap().push(progress.clone());
    }
    fn emit_batch_progress(&self, event: &BatchRenderEvent) {
        self.events.lock().unwrap().push(event.clone());
        if let BatchRenderEvent::Snapshot(snapshot) = event {
            self.snapshots.lock().unwrap().push(snapshot.clone());
            if !snapshot.renderer_busy {
                self.terminal.send(snapshot.clone()).unwrap();
            }
        }
    }
}

fn service() -> (
    RenderExecutionService,
    Arc<Sink>,
    mpsc::Receiver<BatchSnapshot>,
) {
    let (tx, rx) = mpsc::channel();
    let sink = Arc::new(Sink {
        single_progress: Mutex::new(Vec::new()),
        snapshots: Mutex::new(Vec::new()),
        events: Mutex::new(Vec::new()),
        terminal: tx,
    });
    (RenderExecutionService::with_sink(sink.clone()), sink, rx)
}

struct ControlledExecutor {
    entered: mpsc::Sender<String>,
    resume: Mutex<mpsc::Receiver<()>>,
    embedded: Mutex<Vec<String>>,
    offsets: Mutex<Vec<f64>>,
    /// Per executed job: whether it received an activity and its covered frame count.
    activities: Mutex<Vec<(bool, u64)>>,
    assemblies: Mutex<Vec<OutputAssembly>>,
    cleanup: Option<(mpsc::Sender<()>, Mutex<mpsc::Receiver<()>>)>,
}

impl BatchJobExecutor for ControlledExecutor {
    fn embedded_activity(
        &self,
        _: &AppPaths,
        source: &InspectedVideoSource,
    ) -> CoreResult<Option<ParsedActivity>> {
        self.embedded
            .lock()
            .unwrap()
            .push(source.metadata.path.clone());
        if source.metadata.path.contains("broken") {
            return Err(CoreError::Activity("Unreadable embedded telemetry".into()));
        }
        if source.metadata.path.contains("missing") {
            return Ok(None);
        }
        let mut activity = activity();
        // Different owning sources must retain their own telemetry payload.
        activity.file_name = Some(source.metadata.path.clone());
        Ok(Some(activity))
    }

    fn execute(
        &self,
        _: &AppPaths,
        plan: PlannedVideoRender,
        activity: Option<&ParsedActivity>,
        session: &RendererReservation,
        target: &RenderOutputTarget,
    ) -> CoreResult<String> {
        let filename = target.filename().to_owned();
        let frames = plan.planned_frames();
        let covered = match plan.mode() {
            VideoRenderModePlan::Composite { render, .. } => render.coverage.frame_count,
            VideoRenderModePlan::Transparent(render) => render.coverage.frame_count,
        };
        self.activities
            .lock()
            .unwrap()
            .push((activity.is_some(), covered));
        self.offsets
            .lock()
            .unwrap()
            .push(plan.config().scene.export_start_seconds);
        if filename.starts_with('1') || activity.is_none() {
            assert!(plan.config().values.is_empty());
            assert!(plan.config().labels.is_empty());
            assert!(plan.config().backdrops.is_empty());
            assert!(plan.config().course_plots.is_empty());
            assert!(plan.config().elevation_plots.is_empty());
        } else {
            assert!(!plan.config().values.is_empty());
            assert_eq!(plan.config().labels.len(), 1);
            assert_eq!(plan.config().backdrops.len(), 1);
        }
        assert_eq!(plan.config().rasters.len(), 1);
        assert_eq!(plan.config().rasters[0].id, "logo");
        assert_eq!(plan.config().rasters[0].geometry.width, 4.0);
        if let Some(path) = activity.and_then(|activity| activity.file_name.as_ref()) {
            assert!(path.contains(filename.split('_').next().unwrap()));
        }
        session.controller().start_encoding()?;
        let encoded = if filename.contains("off") { 0 } else { 7 };
        session.controller().set_frame_progress(
            frames / 2,
            frames,
            frames / 2,
            encoded,
            None,
            Some(120.0),
        );
        fs::write(target.path(), b"partial output").unwrap();
        self.entered.send(filename.clone()).unwrap();
        self.resume.lock().unwrap().recv_timeout(WAIT).unwrap();
        if session.check_cancelled().is_err() {
            if let Some((entered, release)) = &self.cleanup {
                entered.send(()).unwrap();
                release.lock().unwrap().recv_timeout(WAIT).unwrap();
            }
            fs::remove_file(target.path()).unwrap();
            return Err(CoreError::Cancelled);
        }
        if filename.contains("error") {
            fs::remove_file(target.path()).unwrap();
            return Err(CoreError::Encode("Controlled encoding failure".into()));
        }
        session
            .controller()
            .set_frame_progress(frames, frames, frames, frames, Some(0), None);
        fs::write(target.path(), b"completed output").unwrap();
        Ok(filename)
    }

    fn assemble(
        &self,
        _: &AppPaths,
        assembly: &OutputAssembly,
        session: &RendererReservation,
    ) -> CoreResult<()> {
        for segment in &assembly.segments {
            match segment {
                AssemblySegment::Part(part) => assert!(part.is_file()),
                AssemblySegment::Transition {
                    outgoing, incoming, ..
                } => assert!(outgoing.is_file() && incoming.is_file()),
            }
        }
        self.assemblies.lock().unwrap().push(assembly.clone());
        session.check_cancelled()?;
        if assembly.target.to_str().unwrap().contains("joinfail") {
            return Err(CoreError::Encode("Controlled join failure".into()));
        }
        fs::write(&assembly.target, b"joined output").unwrap();
        Ok(())
    }
}

fn executor(
    cleanup: Option<(mpsc::Sender<()>, Mutex<mpsc::Receiver<()>>)>,
) -> (
    Arc<ControlledExecutor>,
    mpsc::Receiver<String>,
    mpsc::Sender<()>,
) {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    (
        Arc::new(ControlledExecutor {
            entered: entered_tx,
            resume: Mutex::new(resume_rx),
            embedded: Mutex::new(Vec::new()),
            offsets: Mutex::new(Vec::new()),
            activities: Mutex::new(Vec::new()),
            assemblies: Mutex::new(Vec::new()),
            cleanup,
        }),
        entered_rx,
        resume_tx,
    )
}

#[test]
fn sequential_mixed_results_settle_frame_weights_and_preserve_completed_outputs() {
    let fixture = Fixture::new();
    let request = fixture.request(
        &["1-off.mp4", "2-error.mp4", "3-changed.mp4", "4-later.mp4"],
        true,
    );
    let changed = fixture.directory.join("3-changed.mp4");
    let first_output = fixture.directory.join("1-off_overlay.mov");
    let inspection_id = request.inspection_id.clone();
    let first_source_id = request.outputs[0].pieces[0].source_id.clone();
    let (service, sink, terminal) = service();
    let (executor, entered, resume) = executor(None);
    // Shared template validation belongs to submission, before any item runs.
    let mut malformed = request.clone();
    malformed.template.scene.width = Some(0);
    assert!(matches!(
        service.submit_batch_with_executor(
            &fixture.paths,
            &fixture.inspection,
            malformed,
            None,
            executor.clone()
        ),
        Err(BatchServiceError::InvalidRequest { .. })
    ));
    assert!(!service.progress().busy);
    assert!(entered.try_recv().is_err());
    assert!(!PathBuf::from(&first_output).exists());
    let accepted = service
        .submit_batch_with_executor(
            &fixture.paths,
            &fixture.inspection,
            request.clone(),
            None,
            executor.clone(),
        )
        .unwrap();
    assert_eq!(accepted.snapshot.planned_frames, 300);
    assert_eq!(accepted.snapshot.estimated_seconds_remaining, None);
    assert_eq!(entered.recv_timeout(WAIT).unwrap(), "1-off_overlay.mov");
    assert!(
        entered.try_recv().is_err(),
        "only the active job may execute"
    );
    let progress = service.batch_snapshot(&accepted.batch_id).unwrap();
    assert_eq!(progress.phase, BatchPhase::Rendering);
    assert_eq!(
        (
            progress.processed_frames,
            progress.rendered_frames,
            progress.encoded_frames
        ),
        (15, 15, 0)
    );
    assert_eq!(
        progress.estimated_seconds_remaining,
        Some(285.0 / 120.0),
        "batch ETA is available before composite encoding reports frames"
    );
    assert_eq!(
        progress
            .current_item_progress
            .as_ref()
            .unwrap()
            .rendering_fps,
        service.progress().rendering_fps
    );
    let events = sink.events.lock().unwrap();
    assert!(events.iter().any(|event| matches!(event, BatchRenderEvent::Snapshot(snapshot)
        if snapshot.phase == BatchPhase::Rendering && snapshot.current_item_progress.as_ref().unwrap().rendering_fps.is_none())));
    let tick = events
        .iter()
        .find(|event| matches!(event, BatchRenderEvent::Progress(_)))
        .unwrap();
    let wire = serde_json::to_value(tick).unwrap();
    assert_eq!(wire["data"]["currentItemProgress"]["renderingFps"], 120.0);
    assert!(wire["data"].get("items").is_none());
    assert!(wire["data"].get("outputs").is_none());
    drop(events);
    assert!(matches!(
        service.submit_batch_with_executor(
            &fixture.paths,
            &fixture.inspection,
            request.clone(),
            None,
            executor.clone()
        ),
        Err(BatchServiceError::RendererBusy { .. })
    ));
    assert!(service.reserve().is_err());
    fixture.inspection.dispose_session(&inspection_id);
    fs::write(changed, b"source changed after acceptance").unwrap();
    resume.send(()).unwrap();
    assert_eq!(entered.recv_timeout(WAIT).unwrap(), "2-error_overlay.mov");
    assert_eq!(fs::read(&first_output).unwrap(), b"completed output");
    resume.send(()).unwrap();
    assert_eq!(entered.recv_timeout(WAIT).unwrap(), "4-later_overlay.mov");
    let progress = service.batch_snapshot(&accepted.batch_id).unwrap();
    assert_eq!(progress.result_counts.failed, 2);
    assert_eq!(progress.processed_frames, 240); // 30 + 60 + 90 settled, plus 60 current.
    resume.send(()).unwrap();
    let done = terminal.recv_timeout(WAIT).unwrap();
    assert_eq!(done.phase, BatchPhase::CompletedWithErrors);
    assert_eq!(
        (done.result_counts.succeeded, done.result_counts.failed),
        (2, 2)
    );
    assert_eq!(
        (
            done.processed_frames,
            done.rendered_frames,
            done.encoded_frames
        ),
        (300, 180, 157)
    );
    assert_eq!(done.outputs.len(), 2);
    assert!(done
        .outputs
        .iter()
        .all(|output| fs::read(&output.output_path).unwrap() == b"completed output"));
    assert!(
        matches!(&done.items[2].outcome,Some(BatchItemOutcome::Failed { message }) if message.contains("changed"))
    );
    assert!(
        executor.embedded.lock().unwrap().is_empty(),
        "shared activity needs no extraction"
    );
    assert_eq!(
        service.batch_snapshot(&accepted.batch_id).unwrap().revision,
        done.revision
    );
    let snapshots = sink.snapshots.lock().unwrap();
    assert_eq!(snapshots.iter().filter(|s| !s.renderer_busy).count(), 1);
    assert!(snapshots
        .windows(2)
        .all(|pair| pair[1].revision > pair[0].revision));
    drop(snapshots);
    let stale = service
        .submit_batch(&fixture.paths, &fixture.inspection, request, None)
        .unwrap_err();
    let stale = serde_json::to_value(stale).unwrap();
    assert_eq!(stale["code"], "reinspectionRequired");
    assert_eq!(stale["issues"][0]["sourceId"], first_source_id);
    assert!(
        sink.single_progress.lock().unwrap().is_empty(),
        "batch pipeline progress stays internal, including acceptance rejection and completion"
    );
    let next = service.reserve().unwrap();
    service.cancel_batch(&accepted.batch_id).unwrap();
    next.check_cancelled().unwrap(); // A terminal batch cannot cancel the new owner.
    next.complete(Ok("next.mov".to_owned())).unwrap();
    let single_progress = sink.single_progress.lock().unwrap();
    assert_eq!(single_progress.first().unwrap().status, "preparing");
    assert_eq!(single_progress.last().unwrap().status, "complete");
}

#[test]
fn embedded_failures_continue_and_all_item_failure_has_a_terminal_snapshot() {
    let fixture = Fixture::new();
    let (service, _, terminal) = service();
    let (executor, entered, resume) = executor(None);
    let request = fixture.request(&["1-broken.mp4", "2-error.mp4"], false);
    let inspection_id = request.inspection_id.clone();
    let accepted = service
        .submit_batch_with_executor(
            &fixture.paths,
            &fixture.inspection,
            request,
            None,
            executor.clone(),
        )
        .unwrap();
    fixture.inspection.dispose_session(&inspection_id);
    assert_eq!(entered.recv_timeout(WAIT).unwrap(), "2-error_overlay.mov");
    assert_eq!(
        service
            .batch_snapshot(&accepted.batch_id)
            .unwrap()
            .result_counts
            .failed,
        1
    );
    resume.send(()).unwrap();
    let done = terminal.recv_timeout(WAIT).unwrap();
    assert_eq!(done.phase, BatchPhase::Failed);
    assert_eq!(done.result_counts.failed, 2);
    assert_eq!(done.processed_frames, 90);
    assert_eq!(done.encoded_frames, 7);
    assert!(done.outputs.is_empty());
    assert_eq!(executor.embedded.lock().unwrap().len(), 2);
    assert!(fixture.directory.read_dir().unwrap().all(|entry| entry
        .unwrap()
        .path()
        .extension()
        .unwrap()
        != "mov"));
}

#[test]
fn cancellation_waits_for_cleanup_retains_success_and_never_launches_later_jobs() {
    let fixture = Fixture::new();
    let request = fixture.request(&["1-done.mp4", "2-active.mp4", "3-unstarted.mp4"], true);
    let outputs = [
        "1-done_overlay.mov",
        "2-active_overlay.mov",
        "3-unstarted_overlay.mov",
    ]
    .iter()
    .map(|filename| fixture.directory.join(filename))
    .collect::<Vec<_>>();
    let (service, sink, terminal) = service();
    let (cleanup_tx, cleanup_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (executor, entered, resume) = executor(Some((cleanup_tx, Mutex::new(release_rx))));
    let accepted = service
        .submit_batch_with_executor(&fixture.paths, &fixture.inspection, request, None, executor)
        .unwrap();
    entered.recv_timeout(WAIT).unwrap();
    resume.send(()).unwrap();
    assert_eq!(entered.recv_timeout(WAIT).unwrap(), "2-active_overlay.mov");
    let cancelling = service.cancel_batch(&accepted.batch_id).unwrap();
    assert_eq!(cancelling.phase, BatchPhase::Cancelling);
    assert!(cancelling.renderer_busy);
    assert!(
        sink.single_progress.lock().unwrap().is_empty(),
        "batch cancellation emits only a batch snapshot"
    );
    resume.send(()).unwrap();
    cleanup_rx.recv_timeout(WAIT).unwrap();
    assert!(service.reserve().is_err());
    assert!(
        terminal.try_recv().is_err(),
        "cleanup still owns the renderer"
    );
    assert!(PathBuf::from(&outputs[1]).exists());
    release_tx.send(()).unwrap();
    let done = terminal.recv_timeout(WAIT).unwrap();
    assert_eq!(done.phase, BatchPhase::Cancelled);
    assert_eq!(
        (
            done.result_counts.succeeded,
            done.result_counts.cancelled,
            done.result_counts.unstarted
        ),
        (1, 1, 1)
    );
    assert_eq!(done.processed_frames, 60); // Successful 30, interrupted item's actual 30.
    assert_eq!(done.encoded_frames, 37);
    assert_eq!(done.outputs.len(), 1);
    assert_eq!(fs::read(&outputs[0]).unwrap(), b"completed output");
    assert!(!PathBuf::from(&outputs[1]).exists());
    assert!(!PathBuf::from(&outputs[2]).exists());
    assert!(entered.try_recv().is_err());
    assert!(sink.single_progress.lock().unwrap().is_empty());
    let next = service.reserve().unwrap();
    next.begin_item(1, "Next render").unwrap();
    next.check_cancelled().unwrap();
    next.complete(Ok("next.mov".to_owned())).unwrap();
}

#[test]
fn batch_jobs_render_at_their_committed_offsets() {
    let fixture = Fixture::new();
    let mut request = fixture.request(&["1-first.mp4", "2-second.mp4"], true);
    let BatchActivity::ExternalActivity { offsets, .. } = &mut request.activity else {
        unreachable!();
    };
    offsets.insert(request.outputs[0].pieces[0].source_id.clone(), -1.0);
    offsets.insert(request.outputs[1].pieces[0].source_id.clone(), 24.0);
    let (service, _, terminal) = service();
    let (executor, entered, resume) = executor(None);
    service
        .submit_batch_with_executor(
            &fixture.paths,
            &fixture.inspection,
            request,
            None,
            executor.clone(),
        )
        .unwrap();
    for expected in ["1-first_overlay.mov", "2-second_overlay.mov"] {
        assert_eq!(entered.recv_timeout(WAIT).unwrap(), expected);
        resume.send(()).unwrap();
    }
    assert_eq!(
        terminal.recv_timeout(WAIT).unwrap().phase,
        BatchPhase::Completed
    );
    assert_eq!(*executor.offsets.lock().unwrap(), vec![-1.0, 24.0]);
}

#[test]
fn missing_foreign_or_non_finite_offsets_reject_before_reserving_the_renderer() {
    let fixture = Fixture::new();
    let request = fixture.request(&["1-first.mp4"], true);
    let (service, _, _) = service();
    let (executor, entered, _) = executor(None);
    for case in 0..3 {
        let mut malformed = request.clone();
        let BatchActivity::ExternalActivity { offsets, .. } = &mut malformed.activity else {
            unreachable!();
        };
        match case {
            0 => offsets.clear(),
            1 => {
                offsets.insert(
                    request.outputs[0].pieces[0].source_id.clone(),
                    f64::INFINITY,
                );
            }
            2 => {
                offsets.clear();
                offsets.insert("foreign-source".into(), 0.0);
            }
            _ => unreachable!(),
        }
        assert!(
            matches!(
                service.submit_batch_with_executor(
                    &fixture.paths,
                    &fixture.inspection,
                    malformed,
                    None,
                    executor.clone(),
                ),
                Err(BatchServiceError::InvalidRequest { .. })
            ),
            "case {case}"
        );
        assert!(!service.progress().busy);
        assert!(entered.try_recv().is_err());
    }
}

#[test]
fn batch_wire_accepts_frontend_template_and_rejects_redundant_fields() {
    let fixture = Fixture::new();
    let mut frontend_request = fixture.request(&["1-frontend.mp4"], true);
    // Frontend workflow coverage emits this same materialized wire template.
    frontend_request.template =
        serde_json::from_str(include_str!("fixtures/config/batch-template.json")).unwrap();
    let inspection_id = frontend_request.inspection_id.clone();
    let plan = common::builders::batch_video_plan(
        &fixture.paths,
        &fixture.inspection,
        &inspection_id,
        frontend_request,
    )
    .unwrap();
    assert_eq!(plan.config().scene.presentation.width, 64);
    let request = fixture.request(&["1-first.mp4"], false);
    let wire = serde_json::to_value(request).unwrap();
    serde_json::from_value::<BatchRenderRequest>(wire.clone()).unwrap();
    for field in ["source", "outputPath", "timing"] {
        let mut malformed = wire.clone();
        malformed["outputs"][0][field] = json!("legacy echo");
        assert!(serde_json::from_value::<BatchRenderRequest>(malformed).is_err());
        let mut malformed = wire.clone();
        malformed["outputs"][0]["pieces"][0][field] = json!("legacy echo");
        assert!(serde_json::from_value::<BatchRenderRequest>(malformed).is_err());
    }
    let mut legacy = wire.clone();
    legacy["jobs"] = legacy["outputs"].take();
    legacy.as_object_mut().unwrap().remove("outputs");
    assert!(serde_json::from_value::<BatchRenderRequest>(legacy).is_err());
    let mut malformed = wire;
    malformed["activity"]["reference"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<BatchRenderRequest>(malformed).is_err());
}

#[test]
fn sources_without_embedded_telemetry_render_rasters_only() {
    let fixture = Fixture::new();
    let request = fixture.request(&["2-missing.mp4", "4-telemetry.mp4"], false);
    let (service, _, terminal) = service();
    let (executor, entered, resume) = executor(None);
    service
        .submit_batch_with_executor(
            &fixture.paths,
            &fixture.inspection,
            request,
            None,
            executor.clone(),
        )
        .unwrap();
    for expected in ["2-missing_overlay.mov", "4-telemetry_overlay.mov"] {
        assert_eq!(entered.recv_timeout(WAIT).unwrap(), expected);
        resume.send(()).unwrap();
    }
    assert_eq!(
        terminal.recv_timeout(WAIT).unwrap().phase,
        BatchPhase::Completed
    );
    let activities = executor.activities.lock().unwrap().clone();
    assert_eq!(activities[0], (false, 0));
    assert!(activities[1].0 && activities[1].1 > 0);
}

#[test]
fn a_trimmed_piece_renders_its_window_at_the_activity_second_of_its_first_frame() {
    let fixture = Fixture::new();
    let mut request = fixture.request(&["4-ride.mp4"], true);
    request.outputs[0].pieces[0].start_frame = 30;
    request.outputs[0].pieces[0].end_frame = 150;
    let BatchActivity::ExternalActivity { offsets, .. } = &mut request.activity else {
        unreachable!();
    };
    *offsets.values_mut().next().unwrap() = 10.0;
    let plan = common::builders::batch_video_plan(
        &fixture.paths,
        &fixture.inspection,
        &request.inspection_id.clone(),
        request,
    )
    .unwrap();
    assert_eq!(plan.config().scene.export_start_seconds, 11.0);
    let VideoRenderModePlan::Transparent(render) = plan.mode() else {
        panic!("transparent fixture")
    };
    // 4 s at 30 fps decimated by 2.
    assert_eq!(plan.planned_frames(), 60);
    assert_eq!(render.coverage.blank_leading_frame_count, 0);
}

fn piece(
    request: &BatchRenderRequest,
    output: usize,
    start_frame: u64,
    end_frame: u64,
) -> BatchRenderPiece {
    BatchRenderPiece {
        source_id: request.outputs[output].pieces[0].source_id.clone(),
        start_frame,
        end_frame,
        skip_overlay: false,
    }
}

fn parts_directories(directory: &std::path::Path) -> Vec<PathBuf> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with(".ovrley-parts-")
        })
        .collect()
}

/// One combined output joining a window of each of two sources.
fn combined_request(fixture: &Fixture, names: &[&str], file: &str) -> BatchRenderRequest {
    let mut request = fixture.request(names, true);
    let pieces = vec![piece(&request, 0, 0, 60), piece(&request, 1, 30, 90)];
    request.outputs = vec![BatchRenderOutput {
        id: "joined".into(),
        pieces,
        transitions: common::builders::hard_cuts(2),
    }];
    request.destination = BatchDestination::File {
        path: fixture.directory.join(file).to_str().unwrap().into(),
        overwrite: false,
    };
    request
}

#[test]
fn a_separate_output_renders_its_pieces_to_parts_and_joins_them() {
    let fixture = Fixture::new();
    let mut request = fixture.request(&["4-ride.mp4"], true);
    let BatchActivity::ExternalActivity { offsets, .. } = &mut request.activity else {
        unreachable!();
    };
    *offsets.values_mut().next().unwrap() = 10.0;
    // A deleted part between seconds 2 and 4.
    request.outputs[0].pieces = vec![piece(&request, 0, 0, 60), piece(&request, 0, 120, 240)];
    request.outputs[0].transitions = common::builders::hard_cuts(2);
    let (service, sink, terminal) = service();
    let (executor, entered, resume) = executor(None);
    let acceptance = service
        .submit_batch_with_executor(
            &fixture.paths,
            &fixture.inspection,
            request,
            None,
            executor.clone(),
        )
        .unwrap();
    // 2 s and 4 s at 30 fps, decimated by 2.
    assert_eq!(acceptance.snapshot.items[0].planned_frames, 90);
    for expected in ["part-0000.mov", "part-0001.mov"] {
        assert_eq!(entered.recv_timeout(WAIT).unwrap(), expected);
        resume.send(()).unwrap();
    }
    let snapshot = terminal.recv_timeout(WAIT).unwrap();
    assert_eq!(snapshot.phase, BatchPhase::Completed);
    let target = fixture.directory.join("4-ride_overlay.mov");
    // The output directory is canonical (long names on Windows); the fixture path may not be.
    assert_eq!(
        fs::canonicalize(&snapshot.outputs[0].output_path).unwrap(),
        fs::canonicalize(&target).unwrap()
    );
    assert_eq!(fs::read(&target).unwrap(), b"joined output");
    assert_eq!(*executor.offsets.lock().unwrap(), vec![10.0, 14.0]);
    let assemblies = executor.assemblies.lock().unwrap().clone();
    assert_eq!(assemblies.len(), 1);
    let assembly = &assemblies[0];
    assert_eq!(
        fs::canonicalize(&assembly.target).unwrap(),
        fs::canonicalize(&target).unwrap()
    );
    assert_eq!(assembly.frames, 90);
    assert_eq!(
        assembly.segments,
        vec![
            AssemblySegment::Part(assembly.parts_directory.join("part-0000.mov")),
            AssemblySegment::Part(assembly.parts_directory.join("part-0001.mov"))
        ]
    );
    assert!(assembly
        .parts_directory
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with(".ovrley-parts-4-ride_overlay-"));
    // Verified output: the parts are gone.
    assert!(parts_directories(&fixture.directory).is_empty());
    // The second piece reports item progress after the first piece's 30 frames.
    let progress = sink
        .events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| match event {
            BatchRenderEvent::Snapshot(snapshot) => snapshot.current_item_progress.clone(),
            BatchRenderEvent::Progress(update) => update.current_item_progress.clone(),
        })
        .map(|progress| (progress.current_frames, progress.planned_frames))
        .collect::<Vec<_>>();
    assert!(progress.contains(&(15, 90)), "{progress:?}");
    assert!(progress.contains(&(60, 90)), "{progress:?}");
}

#[test]
fn a_combined_output_joins_pieces_of_several_sources_into_one_file() {
    let fixture = Fixture::new();
    let request = combined_request(&fixture, &["2-first.mp4", "4-second.mp4"], "joined.mov");
    let (service, _, terminal) = service();
    let (executor, entered, resume) = executor(None);
    service
        .submit_batch_with_executor(
            &fixture.paths,
            &fixture.inspection,
            request,
            None,
            executor.clone(),
        )
        .unwrap();
    for _ in 0..2 {
        entered.recv_timeout(WAIT).unwrap();
        resume.send(()).unwrap();
    }
    let snapshot = terminal.recv_timeout(WAIT).unwrap();
    assert_eq!(snapshot.phase, BatchPhase::Completed);
    let target = fixture.directory.join("joined.mov");
    assert_eq!(snapshot.outputs[0].output_path, target.to_str().unwrap());
    assert_eq!(fs::read(&target).unwrap(), b"joined output");
    // Each piece starts at the activity second of its own first frame.
    assert_eq!(*executor.offsets.lock().unwrap(), vec![0.0, 1.0]);
    assert!(parts_directories(&fixture.directory).is_empty());
}

#[test]
fn outputs_must_fit_their_destination_and_join_compatible_sources() {
    let fixture = Fixture::new();
    let (service, _, _) = service();
    let (executor, entered, _) = executor(None);
    let submit = |request: BatchRenderRequest| {
        service.submit_batch_with_executor(
            &fixture.paths,
            &fixture.inspection,
            request,
            None,
            executor.clone(),
        )
    };
    let mut two_outputs = fixture.request(&["2-first.mp4", "4-second.mp4"], true);
    two_outputs.destination = BatchDestination::File {
        path: fixture
            .directory
            .join("joined.mov")
            .to_str()
            .unwrap()
            .into(),
        overwrite: false,
    };
    assert!(
        matches!(submit(two_outputs), Err(BatchServiceError::InvalidRequest { message }) if message.contains("exactly one output"))
    );
    let mut mixed = combined_request(&fixture, &["2-first.mp4", "4-second.mp4"], "joined.mov");
    mixed.destination = BatchDestination::SourceNamed {
        directory: fixture.directory.to_str().unwrap().into(),
    };
    assert!(
        matches!(submit(mixed), Err(BatchServiceError::InvalidRequest { message }) if message.contains("pieces of one source"))
    );
    let mut empty = fixture.request(&["2-first.mp4"], true);
    empty.outputs[0].pieces.clear();
    assert!(
        matches!(submit(empty), Err(BatchServiceError::InvalidRequest { message }) if message.contains("at least one piece"))
    );
    let incompatible = combined_request(&fixture, &["2-first.mp4", "4-wide.mp4"], "joined.mov");
    assert!(
        matches!(submit(incompatible), Err(BatchServiceError::InvalidRequest { message }) if message.contains("different display resolution"))
    );
    fs::write(fixture.directory.join("existing.mov"), b"previous output").unwrap();
    // The frontend asks before replacing an existing combined output.
    assert!(matches!(
        submit(combined_request(
            &fixture,
            &["2-first.mp4", "4-second.mp4"],
            "existing.mov"
        )),
        Err(BatchServiceError::AlreadyExists { .. })
    ));
    assert_eq!(
        fs::read(fixture.directory.join("existing.mov")).unwrap(),
        b"previous output"
    );
    let mut aliasing = combined_request(&fixture, &["2-first.mp4", "4-second.mov"], "joined.mov");
    aliasing.destination = BatchDestination::File {
        path: fixture
            .directory
            .join("4-second.mov")
            .to_str()
            .unwrap()
            .into(),
        overwrite: true,
    };
    assert!(matches!(
        submit(aliasing),
        Err(BatchServiceError::OutputError { .. })
    ));
    assert!(!service.progress().busy);
    assert!(entered.try_recv().is_err());
}

#[test]
fn a_failed_join_keeps_the_parts_and_a_cancelled_one_removes_them() {
    let fixture = Fixture::new();
    let request = combined_request(&fixture, &["2-first.mp4", "4-second.mp4"], "joinfail.mov");
    let (service, _, terminal) = service();
    let (executor, entered, resume) = executor(None);
    service
        .submit_batch_with_executor(
            &fixture.paths,
            &fixture.inspection,
            request,
            None,
            executor.clone(),
        )
        .unwrap();
    for _ in 0..2 {
        entered.recv_timeout(WAIT).unwrap();
        resume.send(()).unwrap();
    }
    let snapshot = terminal.recv_timeout(WAIT).unwrap();
    assert_eq!(snapshot.phase, BatchPhase::Failed);
    let retained = parts_directories(&fixture.directory);
    assert_eq!(retained.len(), 1);
    assert!(retained[0].join("part-0000.mov").is_file());
    assert!(retained[0].join("part-0001.mov").is_file());
    let Some(BatchItemOutcome::Failed { message }) = &snapshot.items[0].outcome else {
        panic!("failed join")
    };
    assert!(message.contains("Controlled join failure"));
    assert!(message.contains(retained[0].to_str().unwrap()));
    assert!(!fixture.directory.join("joinfail.mov").exists());
    fs::remove_dir_all(&retained[0]).unwrap();

    let request = combined_request(&fixture, &["2-first.mp4", "4-second.mp4"], "joined.mov");
    let acceptance = service
        .submit_batch_with_executor(
            &fixture.paths,
            &fixture.inspection,
            request,
            None,
            executor.clone(),
        )
        .unwrap();
    entered.recv_timeout(WAIT).unwrap();
    resume.send(()).unwrap();
    entered.recv_timeout(WAIT).unwrap();
    service.cancel_batch(&acceptance.batch_id).unwrap();
    resume.send(()).unwrap();
    let snapshot = terminal.recv_timeout(WAIT).unwrap();
    assert_eq!(snapshot.phase, BatchPhase::Cancelled);
    assert!(matches!(
        snapshot.items[0].outcome,
        Some(BatchItemOutcome::Cancelled)
    ));
    assert!(parts_directories(&fixture.directory).is_empty());
    assert!(!fixture.directory.join("joined.mov").exists());
}

fn composite(mut request: BatchRenderRequest) -> BatchRenderRequest {
    request.encoding.export_mode = BatchExportMode::Composite;
    request.encoding.export_codec = "libx264".into();
    request.encoding.update_rate = 1;
    request
}

fn fade(seconds: f64) -> BatchTransition {
    BatchTransition {
        transition_type: "fade".into(),
        duration_seconds: seconds,
    }
}

#[test]
fn transitions_render_heads_and_tails_and_blend_them_between_bodies() {
    let fixture = Fixture::new();
    let mut request = composite(fixture.request(&["4-ride.mp4"], true));
    let BatchActivity::ExternalActivity { offsets, .. } = &mut request.activity else {
        unreachable!();
    };
    *offsets.values_mut().next().unwrap() = 10.0;
    // A deleted part between seconds 3 and 4, bridged by a one-second fade.
    request.outputs[0].pieces = vec![piece(&request, 0, 0, 90), piece(&request, 0, 120, 240)];
    request.outputs[0].transitions = vec![fade(1.0)];
    let (service, _, terminal) = service();
    let (executor, entered, resume) = executor(None);
    let acceptance = service
        .submit_batch_with_executor(
            &fixture.paths,
            &fixture.inspection,
            request,
            None,
            executor.clone(),
        )
        .unwrap();
    // Every piece frame renders once; the joined output loses the 30 blended frames.
    assert_eq!(acceptance.snapshot.items[0].planned_frames, 210);
    for expected in [
        "part-0000.mp4",
        "part-0001.mp4",
        "part-0002.mp4",
        "part-0003.mp4",
    ] {
        assert_eq!(entered.recv_timeout(WAIT).unwrap(), expected);
        resume.send(()).unwrap();
    }
    assert_eq!(
        terminal.recv_timeout(WAIT).unwrap().phase,
        BatchPhase::Completed
    );
    // Body 0-2 s, tail 2-3 s, head 4-5 s, body 5-8 s: each at its own activity second.
    assert_eq!(
        *executor.offsets.lock().unwrap(),
        vec![10.0, 12.0, 14.0, 15.0]
    );
    let assembly = executor.assemblies.lock().unwrap()[0].clone();
    let part = |index: usize| {
        assembly
            .parts_directory
            .join(format!("part-{index:04}.mp4"))
    };
    assert_eq!(assembly.frames, 180);
    assert_eq!(
        assembly.segments,
        vec![
            AssemblySegment::Part(part(0)),
            AssemblySegment::Transition {
                outgoing: part(1),
                incoming: part(2),
                name: "fade".into(),
                duration: 1.0,
                output: assembly.parts_directory.join("transition-0000.mp4"),
            },
            AssemblySegment::Part(part(3)),
        ]
    );
}

#[test]
fn transitions_must_fit_their_pieces_and_composite_output() {
    let fixture = Fixture::new();
    let (service, _, _) = service();
    let (executor, entered, _) = executor(None);
    let transparent_request = fixture.request(&["4-ride.mp4"], true);
    let request = composite(transparent_request.clone());
    let with = |transitions: Vec<BatchTransition>, end: u64| {
        let mut request = request.clone();
        request.outputs[0].pieces = vec![piece(&request, 0, 0, end), piece(&request, 0, 120, 240)];
        request.outputs[0].transitions = transitions;
        request
    };
    let rejected = |request: BatchRenderRequest, reason: &str| match service
        .submit_batch_with_executor(
            &fixture.paths,
            &fixture.inspection,
            request,
            None,
            executor.clone(),
        ) {
        Err(BatchServiceError::InvalidRequest { message }) => {
            assert!(message.contains(reason), "{message}")
        }
        other => panic!("expected rejection for {reason}: {other:?}"),
    };
    rejected(with(Vec::new(), 90), "one transition per join");
    rejected(
        with(
            vec![BatchTransition {
                transition_type: "spin".into(),
                duration_seconds: 1.0,
            }],
            90,
        ),
        "unknown transition spin",
    );
    rejected(
        with(
            vec![BatchTransition {
                transition_type: "none".into(),
                duration_seconds: 1.0,
            }],
            90,
        ),
        "hard cut has a duration of 0",
    );
    rejected(with(vec![fade(0.01)], 90), "shorter than one frame");
    rejected(with(vec![fade(0.0)], 90), "must be positive");
    // 30 frames before the fade leave no frame of the first piece untouched.
    rejected(with(vec![fade(1.0)], 30), "leave none of its 30 frames");
    let mut transparent = with(vec![fade(1.0)], 90);
    transparent.encoding = transparent_request.encoding;
    rejected(transparent, "hard cuts only");
    assert!(!service.progress().busy);
    assert!(entered.try_recv().is_err());
}
