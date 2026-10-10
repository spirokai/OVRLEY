//! Service-level regressions for session lifetime and stale media acceptance.
//! Probes are injected; no preview import, activity extraction or encoder runs.

mod common;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use ovrley_core::error::CoreResult;
use ovrley_core::media::prepared_video::check_source_freshness;
use ovrley_core::media::SourceVideoMetadata;
use ovrley_core::paths::AppPaths;
use ovrley_core::render_jobs::contracts::{BatchEncodingSettings, BatchExportMode};
use ovrley_core::render_jobs::inspection::{
    InspectionSourceSelection, InspectionValidation, ReinspectionReason, SourceMetadataProbe,
    VideoInspectionService,
};
use ovrley_core::render_jobs::planning::{
    plan_batch_configuration, BatchPlanningResponse, VideoRenderModePlan,
};
use serde_json::json;

fn encoding(mode: BatchExportMode) -> BatchEncodingSettings {
    BatchEncodingSettings {
        export_mode: mode,
        export_codec: if mode == BatchExportMode::Composite {
            "libx264"
        } else {
            "qtrle"
        }
        .into(),
        fps: 30,
        update_rate: 2,
        quality_type: ovrley_core::encode::quality::QualityType::Bitrate,
        quality_value: 20.0,
        qsv_full_init_args: None,
    }
}

#[test]
fn configuration_plans_native_names_and_exact_source_or_layout_work() {
    let directory = SourcesDirectory::new();
    let service = service();
    let session = service.create_session();
    let path = directory.source("queue/ride.朝 morning.mp4");
    let source = service
        .inspect_source(&directory.paths(), &session.inspection_id, &path)
        .unwrap();
    let mut selection = InspectionSourceSelection {
        inspection_id: session.inspection_id.clone(),
        source_ids: vec![source.source_id],
        calibration_source_id: None,
    };
    for (mode, filename, frames, rate) in [
        (
            BatchExportMode::Composite,
            "ride.朝 morning_video.mp4",
            600,
            (30000, 1001),
        ),
        (
            BatchExportMode::Transparent,
            "ride.朝 morning_overlay.mov",
            300,
            (15, 1),
        ),
    ] {
        let existing = directory.0.join(filename);
        fs::write(&existing, b"previous completed output").unwrap();
        let response =
            plan_batch_configuration(&service, &selection, &encoding(mode), &directory.0).unwrap();
        let BatchPlanningResponse::Planned { plans } = response else {
            panic!("fresh plan")
        };
        assert_eq!(plans[0].output_path.file_name().unwrap(), filename);
        assert_eq!(plans[0].output_duration_seconds, 20.0);
        assert_eq!(plans[0].planned_frames, frames);
        assert_eq!(
            (plans[0].container_fps_num, plans[0].container_fps_den),
            rate
        );
        assert_eq!(fs::read(existing).unwrap(), b"previous completed output");
    }
    let duplicate = directory.source("queue/ride.朝 morning.mov");
    selection.source_ids.push(
        service
            .inspect_source(&directory.paths(), &session.inspection_id, &duplicate)
            .unwrap()
            .source_id,
    );
    let error = plan_batch_configuration(
        &service,
        &selection,
        &encoding(BatchExportMode::Composite),
        &directory.0,
    )
    .unwrap_err();
    assert!(error.to_string().contains("conflicting destinations"));
    service.dispose_session(&session.inspection_id);
    assert!(matches!(
        plan_batch_configuration(
            &service,
            &selection,
            &encoding(BatchExportMode::Composite),
            &directory.0
        )
        .unwrap(),
        BatchPlanningResponse::Rejected(_)
    ));
}

struct SmallMetadataProbe;

impl SourceMetadataProbe for SmallMetadataProbe {
    fn probe(&self, _: &AppPaths, path: &str) -> CoreResult<SourceVideoMetadata> {
        let mut metadata = metadata(path);
        metadata.resolution = Some(ovrley_core::media::Resolution {
            width: 64,
            height: 32,
        });
        Ok(metadata)
    }
}

#[test]
fn video_plans_sample_the_video_clock_and_clear_padding_with_static_art() {
    use ovrley_core::debug::RenderProfiler;
    use ovrley_core::render::{prepare_preview_assets, FrameSize, VideoFrameRenderer};
    let directory = SourcesDirectory::new();
    let service = VideoInspectionService::with_probe(Arc::new(SmallMetadataProbe));
    let session = service.create_session();
    let path = directory.source("portrait.mp4");
    let source = service
        .inspect_source(&directory.paths(), &session.inspection_id, &path)
        .unwrap();
    let activity = ovrley_core::activity::parse_activity_json(
        &json!({
            "sample_elapsed_seconds":[0,120], "trim_end_seconds":120, "speed":[0,120],
            "sync_time":"2026-05-20T12:00:00Z"
        })
        .to_string(),
    )
    .unwrap();
    let original_activity = serde_json::to_value(&activity).unwrap();
    let template = common::builders::batch_template();
    let original_template = serde_json::to_value(&template).unwrap();
    let paths = common::composite::test_paths_named("batch-video-window");
    for mode in [BatchExportMode::Transparent, BatchExportMode::Composite] {
        let prepare = |offset, skip_overlay| {
            common::builders::batch_video_plan(
                &paths,
                &service,
                &session.inspection_id,
                common::builders::video_batch_request(
                    &session.inspection_id,
                    &source,
                    encoding(mode),
                    template.clone(),
                    activity.clone(),
                    offset,
                    skip_overlay,
                ),
            )
        };
        for offset in [-5.0, 110.0, -5.01, 100.1] {
            let plan = prepare(offset, false).unwrap();
            let dense = plan.prepare_activity(&activity).unwrap();
            assert_eq!(
                (
                    plan.config().scene.presentation.width,
                    plan.config().scene.presentation.height
                ),
                (32, 64)
            );
            let (coverage, stride, layout_rate, frames) = match plan.mode() {
                VideoRenderModePlan::Transparent(render) => {
                    assert_eq!(render.layout_frame_count, 600);
                    assert_eq!(render.container_fps, "15/1");
                    (&render.coverage, 2usize, 30.0, 300)
                }
                VideoRenderModePlan::Composite { render, .. } => {
                    (&render.coverage, 1usize, 15000.0 / 1001.0, 600)
                }
            };
            assert_eq!(plan.planned_frames(), frames);
            let leading = coverage.blank_leading_frame_count as usize;
            let first_time = offset + leading as f64 / layout_rate;
            assert!((dense.series.speed[0].unwrap() - first_time).abs() < 1e-9);
            assert!(
                (dense.frame_elapsed_seconds[0] + plan.config().scene.start - first_time).abs()
                    < 1e-9
            );
            let suppressed = prepare(offset, true).unwrap();
            assert_eq!(suppressed.planned_frames(), frames);
            assert!(suppressed.config().values.is_empty());
            assert!(suppressed.config().labels.is_empty());
            assert!(suppressed.config().backdrops.is_empty());
            let dense = suppressed.prepare_activity(&activity).unwrap();
            let (assets, _, _, _) =
                prepare_preview_assets(&paths, suppressed.config(), &activity, &dense).unwrap();
            let frame_size = FrameSize {
                width: 32,
                height: 64,
            };
            let renderer = VideoFrameRenderer::new(
                &paths,
                &dense,
                &assets,
                frame_size,
                coverage.blank_leading_frame_count,
            )
            .unwrap();
            let mut pixels = vec![255; frame_size.rgba_len().unwrap()];
            let mut profiler = RenderProfiler::default();
            let covered_frame = leading.div_ceil(stride) * stride;
            renderer
                .render_rgba(covered_frame, &mut pixels, &mut profiler)
                .unwrap();
            assert!(
                pixels.chunks_exact(4).all(|pixel| pixel[3] == 0),
                "suppressed overlay without rasters is transparent in coverage"
            );
            let blank_frame = if offset < 0.0 {
                0
            } else {
                (leading + dense.frame_count).div_ceil(stride) * stride
            };
            renderer
                .render_rgba(blank_frame, &mut pixels, &mut profiler)
                .unwrap();
            assert!(
                pixels.iter().all(|byte| *byte == 0),
                "reused buffers cannot leak art into padding"
            );
        }
        assert!(prepare(120.0, true).is_err());
    }
    assert_eq!(serde_json::to_value(&activity).unwrap(), original_activity);
    assert_eq!(serde_json::to_value(&template).unwrap(), original_template);
}

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

struct SourcesDirectory(PathBuf);

impl SourcesDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "ovrley-inspection-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn source(&self, relative: &str) -> String {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"source").unwrap();
        path.to_str().unwrap().to_owned()
    }

    fn paths(&self) -> AppPaths {
        AppPaths::from_runtime_roots(self.0.clone(), self.0.clone(), self.0.clone())
    }
}

impl Drop for SourcesDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn metadata(path: &str) -> SourceVideoMetadata {
    serde_json::from_value(json!({
        "path": path,
        "duration": 20.0,
        "fps": 29.97,
        "fpsNum": 30000,
        "fpsDen": 1001,
        "resolution": {"width": 1920, "height": 1080},
        "rotationDegrees": if path.ends_with("portrait.mp4") { -90 } else { 0 },
        "creationTime": "2026-05-20T12:00:00Z",
        "syncTime": "2026-05-20T12:00:00Z",
        "timeSource": "gps",
        "hasAudio": false,
    }))
    .unwrap()
}

struct MetadataProbe;

impl SourceMetadataProbe for MetadataProbe {
    fn probe(&self, _: &AppPaths, path: &str) -> CoreResult<SourceVideoMetadata> {
        Ok(metadata(path))
    }
}

fn service() -> VideoInspectionService {
    VideoInspectionService::with_probe(Arc::new(MetadataProbe))
}

#[test]
fn accepted_sources_survive_disposal_but_closed_and_foreign_sessions_reject() {
    let directory = SourcesDirectory::new();
    let queue_path = directory.source("queue/portrait.mp4");
    let reference_path = directory.source("outside/reference.mp4");
    let service = service();
    let session = service.create_session();
    let queued = service
        .inspect_source(&directory.paths(), &session.inspection_id, &queue_path)
        .unwrap();
    let reference = service
        .inspect_source(&directory.paths(), &session.inspection_id, &reference_path)
        .unwrap();
    // Preserve per-source rotation and both geometries for codec-specific plans.
    assert_eq!(queued.metadata.rotation_degrees, Some(270));
    assert_eq!(queued.metadata.resolution.as_ref().unwrap().width, 1920);
    assert_eq!(queued.display_resolution.width, 1080);
    assert_eq!(queued.display_resolution.height, 1920);
    assert_eq!(reference.metadata.rotation_degrees, Some(0));
    assert_eq!(reference.display_resolution.width, 1920);
    assert_eq!(queued.metadata.fps, Some(30000.0 / 1001.0));
    let mut selection = InspectionSourceSelection {
        inspection_id: session.inspection_id.clone(),
        source_ids: vec![queued.source_id.clone()],
        calibration_source_id: Some(reference.source_id.clone()),
    };
    let accepted = match service.validate_sources(&selection) {
        InspectionValidation::Valid(accepted) => accepted,
        InspectionValidation::Rejected(_) => panic!("fresh inspection must validate"),
    };
    service.dispose_session(&session.inspection_id);
    assert_eq!(accepted.sources()[0].as_ref(), &queued);
    assert_eq!(accepted.calibration_source(), Some(&reference));
    check_source_freshness(&accepted.sources()[0]).unwrap();
    let response = serde_json::to_value(
        plan_batch_configuration(
            &service,
            &selection,
            &encoding(BatchExportMode::Composite),
            &directory.0,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(response["status"], "rejected");
    assert_eq!(response["issues"].as_array().unwrap().len(), 2);
    assert_eq!(response["issues"][0]["reason"], "closedSession");
    assert_eq!(response["issues"][1]["sourceId"], reference.source_id);
    selection.inspection_id = service.create_session().inspection_id;
    match service.validate_sources(&selection) {
        InspectionValidation::Rejected(rejection) => {
            assert!(rejection
                .issues
                .iter()
                .all(|issue| issue.reason == ReinspectionReason::UnknownDescriptor));
        }
        _ => panic!("descriptors cannot cross sessions"),
    }
}

#[test]
fn submission_requires_reinspection_of_changed_queue_and_calibration_files() {
    let directory = SourcesDirectory::new();
    let queue_path = directory.source("queue/clip.mp4");
    let reference_path = directory.source("outside/reference.mp4");
    let service = service();
    let session = service.create_session();
    let mut selection = InspectionSourceSelection {
        inspection_id: session.inspection_id.clone(),
        source_ids: vec![
            service
                .inspect_source(&directory.paths(), &session.inspection_id, &queue_path)
                .unwrap()
                .source_id,
        ],
        calibration_source_id: Some(
            service
                .inspect_source(&directory.paths(), &session.inspection_id, &reference_path)
                .unwrap()
                .source_id,
        ),
    };
    // Same-size edits must be caught by mtime; reference edits must also block.
    fs::File::options()
        .write(true)
        .open(&queue_path)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
        .unwrap();
    fs::write(&reference_path, b"changed reference contents").unwrap();
    match service.validate_sources(&selection) {
        InspectionValidation::Rejected(rejection) => {
            assert_eq!(rejection.issues.len(), 2);
            assert!(rejection
                .issues
                .iter()
                .all(|issue| issue.reason == ReinspectionReason::SourceChanged));
            assert_eq!(rejection.issues[0].source_id, selection.source_ids[0]);
            assert_eq!(
                rejection.issues[1].source_id,
                selection.calibration_source_id.as_ref().unwrap().as_str()
            );
        }
        _ => panic!("stale queue or calibration must prevent acceptance"),
    }
    selection.source_ids[0] = service
        .inspect_source(&directory.paths(), &session.inspection_id, &queue_path)
        .unwrap()
        .source_id;
    selection.calibration_source_id = Some(
        service
            .inspect_source(&directory.paths(), &session.inspection_id, &reference_path)
            .unwrap()
            .source_id,
    );
    assert!(matches!(
        service.validate_sources(&selection),
        InspectionValidation::Valid(_)
    ));
    selection.source_ids[0] = "foreign-source".into();
    match service.validate_sources(&selection) {
        InspectionValidation::Rejected(rejection) => {
            assert_eq!(
                rejection.issues[0].reason,
                ReinspectionReason::UnknownDescriptor
            );
        }
        _ => panic!("unknown identities cannot select session-owned metadata"),
    }
}

struct PausedProbe {
    started: mpsc::Sender<()>,
    resume: Mutex<mpsc::Receiver<()>>,
}

impl SourceMetadataProbe for PausedProbe {
    fn probe(&self, _: &AppPaths, path: &str) -> CoreResult<SourceVideoMetadata> {
        self.started.send(()).unwrap();
        self.resume
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        Ok(metadata(path))
    }
}

#[test]
fn inspection_allows_five_active_probes_across_sessions_and_releases_capacity() {
    let directory = SourcesDirectory::new();
    let (started_tx, started_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let service = Arc::new(VideoInspectionService::with_probe(Arc::new(PausedProbe {
        started: started_tx,
        resume: Mutex::new(resume_rx),
    })));
    let sessions = [service.create_session(), service.create_session()];
    let workers = (0..7)
        .map(|index| {
            let service = service.clone();
            let inspection_id = sessions[index % 2].inspection_id.clone();
            let path = directory.source(&format!("clip-{index}.mp4"));
            let paths = directory.paths();
            std::thread::spawn(move || service.inspect_source(&paths, &inspection_id, &path))
        })
        .collect::<Vec<_>>();
    for _ in 0..5 {
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    assert!(started_rx.recv_timeout(Duration::from_millis(100)).is_err());
    resume_tx.send(()).unwrap();
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    for _ in 0..6 {
        resume_tx.send(()).unwrap();
    }
    for worker in workers {
        worker.join().unwrap().unwrap();
    }
}

#[test]
fn probing_cannot_publish_changed_media_or_results_after_disposal() {
    let directory = SourcesDirectory::new();
    let path = directory.source("clip.mp4");
    let (started_tx, started_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let service = Arc::new(VideoInspectionService::with_probe(Arc::new(PausedProbe {
        started: started_tx,
        resume: Mutex::new(resume_rx),
    })));
    let session = service.create_session();
    for close in [false, true] {
        let worker_service = service.clone();
        let inspection_id = session.inspection_id.clone();
        let paths = directory.paths();
        let worker_path = path.clone();
        let worker = std::thread::spawn(move || {
            worker_service.inspect_source(&paths, &inspection_id, &worker_path)
        });
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        if close {
            service.dispose_session(&session.inspection_id);
        } else {
            fs::write(&path, b"source changed during probe").unwrap();
        }
        resume_tx.send(()).unwrap();
        let error = worker.join().unwrap().unwrap_err().to_string();
        assert!(
            error.contains(if close {
                "session is closed"
            } else {
                "changed during inspection"
            }),
            "{error}"
        );
    }
}
