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
use ovrley_core::encode::progress::ProgressSink;
use ovrley_core::encode::quality::QualityType;
use ovrley_core::error::{CoreError, CoreResult};
use ovrley_core::media::SourceVideoMetadata;
use ovrley_core::output::RenderOutputTarget;
use ovrley_core::paths::AppPaths;
use ovrley_core::render_jobs::batch::{BatchJobExecutor, BatchServiceError};
use ovrley_core::render_jobs::batch_plan::{
    plan_batch_configuration, BatchPlanningResponse, PlannedVideoRender,
};
use ovrley_core::render_jobs::contracts::*;
use ovrley_core::render_jobs::execution::{RenderExecutionService, RendererReservation};
use ovrley_core::render_jobs::inspection::{
    InspectionSourceSelection, SourceMetadataProbe, VideoInspectionService,
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
        Ok(serde_json::from_value(json!({
            "path":path, "duration":duration, "fps":30, "fpsNum":30, "fpsDen":1,
            "resolution":{"width":64,"height":32}, "hasAudio":false,
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
        let selection = InspectionSourceSelection {
            inspection_id: session.inspection_id.clone(),
            source_ids: sources
                .iter()
                .map(|source| source.source_id.clone())
                .collect(),
            calibration_source_id: None,
        };
        let BatchPlanningResponse::Planned { .. } =
            plan_batch_configuration(&self.inspection, &selection, &encoding, &self.directory)
                .unwrap()
        else {
            panic!("fresh sources")
        };
        BatchRenderRequest {
            inspection_id: session.inspection_id,
            template: common::builders::batch_template(),
            encoding,
            activity: if external {
                BatchActivity::ExternalActivity {
                    activity: activity(),
                    timezone_mode: VideoSyncTimezoneMode::Utc,
                    reference: None,
                    automatic_offsets: sources
                        .iter()
                        .map(|source| (source.source_id.clone(), 0.0))
                        .collect(),
                }
            } else {
                BatchActivity::EmbeddedActivity {}
            },
            output_directory: self.directory.to_str().unwrap().into(),
            jobs: sources
                .into_iter()
                .enumerate()
                .map(|(index, source)| BatchRenderJob {
                    id: format!("item-{index}"),
                    source_id: source.source_id,
                    skip_overlay: index == 0,
                })
                .collect(),
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
    terminal: mpsc::Sender<BatchSnapshot>,
}

impl ProgressSink for Sink {
    fn emit_progress(&self, progress: &RenderProgress) {
        self.single_progress.lock().unwrap().push(progress.clone());
    }
    fn emit_batch_progress(&self, snapshot: &BatchSnapshot) {
        self.snapshots.lock().unwrap().push(snapshot.clone());
        if !snapshot.renderer_busy {
            self.terminal.send(snapshot.clone()).unwrap();
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
        terminal: tx,
    });
    (RenderExecutionService::with_sink(sink.clone()), sink, rx)
}

struct ControlledExecutor {
    entered: mpsc::Sender<String>,
    resume: Mutex<mpsc::Receiver<()>>,
    embedded: Mutex<Vec<String>>,
    offsets: Mutex<Vec<f64>>,
    cleanup: Option<(mpsc::Sender<()>, Mutex<mpsc::Receiver<()>>)>,
}

impl BatchJobExecutor for ControlledExecutor {
    fn embedded_activity(
        &self,
        _: &AppPaths,
        source: &InspectedVideoSource,
    ) -> CoreResult<ParsedActivity> {
        self.embedded
            .lock()
            .unwrap()
            .push(source.metadata.path.clone());
        if source.metadata.path.contains("missing") {
            return Err(CoreError::Activity("No embedded activity".into()));
        }
        let mut activity = activity();
        // Different owning sources must retain their own telemetry payload.
        activity.file_name = Some(source.metadata.path.clone());
        Ok(activity)
    }

    fn execute(
        &self,
        _: &AppPaths,
        plan: PlannedVideoRender,
        activity: &ParsedActivity,
        session: &RendererReservation,
        target: &RenderOutputTarget,
    ) -> CoreResult<String> {
        let filename = target.filename().to_owned();
        let frames = plan.planned_frames();
        self.offsets
            .lock()
            .unwrap()
            .push(plan.config().scene.export_start_seconds);
        if filename.starts_with('1') {
            assert!(plan.config().values.is_empty());
            assert_eq!(plan.config().labels.len(), 1);
            assert_eq!(plan.config().backdrops.len(), 1);
        } else {
            assert!(!plan.config().values.is_empty());
        }
        if let Some(path) = &activity.file_name {
            assert!(path.contains(filename.split('_').next().unwrap()));
        }
        session.controller().start_encoding()?;
        session
            .controller()
            .set_frame_progress(frames / 2, frames, frames / 2, 7, None, None);
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
    let first_source_id = request.jobs[0].source_id.clone();
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
        (15, 15, 7)
    );
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
    let request = fixture.request(&["1-missing.mp4", "2-error.mp4"], false);
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
fn batch_calibration_applies_one_correction_to_each_automatic_baseline() {
    let fixture = Fixture::new();
    let mut request = fixture.request(&["1-first.mp4", "2-second.mp4"], true);
    let BatchActivity::ExternalActivity {
        reference,
        automatic_offsets,
        ..
    } = &mut request.activity
    else {
        unreachable!();
    };
    *reference = Some(BatchCalibrationReference {
        source_id: request.jobs[0].source_id.clone(),
        creation_time: "2026-05-20T12:00:00Z".into(),
        time_source: Some("gps".into()),
        automatic_offset_seconds: -5.0,
        committed_offset_seconds: -1.0,
    });
    automatic_offsets.insert(request.jobs[0].source_id.clone(), -5.0);
    automatic_offsets.insert(request.jobs[1].source_id.clone(), 20.0);
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
fn invalid_calibration_and_missing_baselines_reject_before_reserving_the_renderer() {
    let fixture = Fixture::new();
    let request = fixture.request(&["1-first.mp4"], true);
    let (service, _, _) = service();
    let (executor, entered, _) = executor(None);
    for case in 0..4 {
        let mut malformed = request.clone();
        let BatchActivity::ExternalActivity {
            reference,
            automatic_offsets,
            ..
        } = &mut malformed.activity
        else {
            unreachable!();
        };
        match case {
            0 => automatic_offsets.clear(),
            1 => {
                automatic_offsets.insert(request.jobs[0].source_id.clone(), f64::INFINITY);
            }
            2 => {
                automatic_offsets.clear();
                automatic_offsets.insert("foreign-source".into(), 0.0);
            }
            3 => {
                *reference = Some(BatchCalibrationReference {
                    source_id: request.jobs[0].source_id.clone(),
                    creation_time: "2026-05-20T12:00:00Z".into(),
                    time_source: None,
                    automatic_offset_seconds: 1.0,
                    committed_offset_seconds: 2.0,
                });
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
fn batch_wire_rejects_redundant_job_fields_and_embedded_calibration() {
    let fixture = Fixture::new();
    let request = fixture.request(&["1-first.mp4"], false);
    let wire = serde_json::to_value(request).unwrap();
    serde_json::from_value::<BatchRenderRequest>(wire.clone()).unwrap();
    for field in ["source", "outputPath", "timing"] {
        let mut malformed = wire.clone();
        malformed["jobs"][0][field] = json!("legacy echo");
        assert!(serde_json::from_value::<BatchRenderRequest>(malformed).is_err());
    }
    let mut malformed = wire;
    malformed["activity"]["reference"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<BatchRenderRequest>(malformed).is_err());
}
