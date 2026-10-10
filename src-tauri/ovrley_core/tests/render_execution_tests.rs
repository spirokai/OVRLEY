//! Exercises real FFmpeg teardown and compares single/batch work and progress.
mod common;

use ovrley_core::debug::RenderProgress;
use ovrley_core::encode::progress::ProgressSink;
use ovrley_core::render_jobs::contracts::*;
use ovrley_core::render_jobs::execution::RenderExecutionService;
use ovrley_core::render_jobs::inspection::VideoInspectionService;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Default)]
struct Observations {
    single: Mutex<Vec<RenderProgress>>,
    batch: Mutex<Vec<BatchRenderEvent>>,
}

impl ProgressSink for Observations {
    fn emit_progress(&self, progress: &RenderProgress) {
        self.single.lock().unwrap().push(progress.clone());
    }
    fn emit_batch_progress(&self, event: &BatchRenderEvent) {
        self.batch.lock().unwrap().push(event.clone());
    }
}

#[test]
fn native_cancellation_and_coordinator_panic_clean_output_before_renderer_reuse() {
    use ovrley_core::encode::progress::RenderController;
    use ovrley_core::output::{RenderOutputKind, RenderOutputTarget};
    use ovrley_core::render_jobs::planning::plan_single_render;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Interrupt {
        panic: bool,
        fired: AtomicBool,
        cancel: Mutex<Option<Arc<AtomicBool>>>,
    }
    impl ProgressSink for Interrupt {
        fn emit_progress(&self, progress: &RenderProgress) {
            if progress.status == "rendering"
                && progress.current > 0
                && progress.current < progress.total
                && !self.fired.swap(true, Ordering::Relaxed)
            {
                if self.panic {
                    panic!("injected coordinator panic");
                }
                self.cancel
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .store(true, Ordering::Relaxed);
            }
        }
    }
    let paths = common::composite::test_paths_named("native-cleanup");
    let activity = ovrley_core::activity::parse_activity_json(
        &json!({
            "sample_elapsed_seconds": [0, 100], "trim_end_seconds": 100, "speed": [20, 21],
        })
        .to_string(),
    )
    .unwrap();
    let make_plan = |duration| {
        let mut config = serde_json::to_value(common::builders::batch_template()).unwrap();
        config["scene"].as_object_mut().unwrap().extend(
            serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(json!({
                "width": 1280, "height": 720, "start": 0, "end": duration, "fps": 30,
                "update_rate": 1, "custom_export_range_active": true, "ffmpeg": {"codec": "qtrle"},
            }))
            .unwrap(),
        );
        plan_single_render(serde_json::from_value(config).unwrap(), 100.0, None).unwrap()
    };
    for panic in [false, true] {
        let sink = Arc::new(Interrupt {
            panic,
            fired: AtomicBool::new(false),
            cancel: Mutex::new(None),
        });
        let controller = RenderController::with_sink(sink.clone());
        *sink.cancel.lock().unwrap() = Some(controller.cancel_flag());
        let service = RenderExecutionService::with_controller(controller);
        let output = paths.downloads_dir.join(format!("interrupted-{panic}.mov"));
        let target = RenderOutputTarget::validate(
            output.to_str().unwrap(),
            RenderOutputKind::Transparent,
            true,
        )
        .unwrap();
        let result = service.render(&paths, make_plan(100), &activity, &target);
        assert!(
            sink.fired.load(Ordering::Relaxed),
            "test must interrupt active frame production"
        );
        let error = result.unwrap_err();
        if panic {
            assert!(
                error.to_string().contains("Frame coordinator panicked"),
                "{error}"
            );
        } else {
            assert!(
                matches!(error, ovrley_core::error::CoreError::Cancelled),
                "{error}"
            );
        }
        assert!(
            !output.exists(),
            "partial output must be removed before render returns"
        );
        assert!(!service.progress().busy);
        service
            .render(&paths, make_plan(1), &activity, &target)
            .unwrap();
        assert!(output.metadata().unwrap().len() > 0);
        assert_eq!(service.progress().status, "complete");
    }
}

#[test]
fn single_and_batch_use_the_same_frame_work_and_native_fps() {
    let paths = common::composite::test_paths_named("execution-parity");
    let binary =
        ovrley_core::encode::ffmpeg::binary::resolve_ffmpeg_binary(&paths.repo_root).unwrap();
    let source = paths.temp_dir.join("source.mp4");
    let generated = std::process::Command::new(binary)
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=1280x720:rate=30:duration=20",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
        ])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let inspection = VideoInspectionService::default();
    let session = inspection.create_session();
    let source = inspection
        .inspect_source(&paths, &session.inspection_id, source.to_str().unwrap())
        .unwrap();
    let activity = ovrley_core::activity::parse_activity_json(
        &json!({
            "sample_elapsed_seconds": [0, 20], "trim_end_seconds": 20, "speed": [20, 21],
        })
        .to_string(),
    )
    .unwrap();

    for mode in [BatchExportMode::Transparent, BatchExportMode::Composite] {
        let (codec, extension) = match mode {
            BatchExportMode::Transparent => ("qtrle", "mov"),
            BatchExportMode::Composite => ("libx264", "mp4"),
        };
        let mut template = common::builders::batch_template();
        template.scene.width = Some(1280);
        template.scene.height = Some(720);
        let mut single = serde_json::to_value(&template).unwrap();
        let scene = single["scene"].as_object_mut().unwrap();
        scene.extend(
            serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(json!({
                "start": 0, "end": 20, "fps": 30, "update_rate": 2,
                "custom_export_range_active": false, "ffmpeg": {"codec": codec},
            }))
            .unwrap(),
        );
        if mode == BatchExportMode::Composite {
            scene.extend(
                serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(json!({
                    "composite_video_path": source.metadata.path,
                    "composite_video_duration": 20,
                    "composite_video_fps_num": 30, "composite_video_fps_den": 1,
                    "composite_sync_offset": 0, "composite_video_trim_start": 0,
                    "composite_widget_update_rate": 2, "qualityType": "bitrate", "qualityValue": 10,
                }))
                .unwrap(),
            );
        }
        let observed = Arc::new(Observations::default());
        let service = RenderExecutionService::with_sink(observed.clone());
        let started = Instant::now();
        service
            .submit_single(
                &paths,
                &single.to_string(),
                &serde_json::to_string(&activity).unwrap(),
                paths
                    .downloads_dir
                    .join(format!("single.{extension}"))
                    .to_str()
                    .unwrap(),
                true,
                None,
            )
            .unwrap();
        drop(service);
        let single_seconds = started.elapsed().as_secs_f64();
        let single = observed.single.lock().unwrap();
        let terminal = single.last().unwrap();
        assert_eq!(terminal.status, "complete", "{}", terminal.message);
        let single_counts = (terminal.total, terminal.rendered, terminal.encoded);
        let single_fps = single
            .iter()
            .filter_map(|p| p.rendering_fps)
            .collect::<Vec<_>>();
        drop(single);

        let mut request = common::builders::video_batch_request(
            &session.inspection_id,
            &source,
            BatchEncodingSettings {
                export_mode: mode,
                export_codec: codec.into(),
                fps: 30,
                update_rate: 2,
                quality_type: ovrley_core::encode::quality::QualityType::Bitrate,
                quality_value: 10.0,
                qsv_full_init_args: None,
            },
            template,
            activity.clone(),
            0.0,
            false,
        );
        request.output_directory = paths.downloads_dir.to_str().unwrap().into();
        let service = RenderExecutionService::with_sink(observed.clone());
        let started = Instant::now();
        service
            .submit_batch(&paths, &inspection, request, None)
            .unwrap();
        drop(service);
        let batch_seconds = started.elapsed().as_secs_f64();
        let events = observed.batch.lock().unwrap();
        let BatchRenderEvent::Snapshot(terminal) = events.last().unwrap() else {
            panic!("missing terminal batch")
        };
        assert_eq!(
            terminal.phase,
            BatchPhase::Completed,
            "{:?}",
            terminal.items
        );
        let item = &terminal.items[0];
        assert_eq!(
            (
                item.planned_frames,
                item.rendered_frames,
                item.encoded_frames
            ),
            (
                u64::from(single_counts.0),
                u64::from(single_counts.1),
                u64::from(single_counts.2)
            )
        );
        let batch_fps = events
            .iter()
            .filter_map(|event| match event {
                BatchRenderEvent::Snapshot(snapshot) => snapshot.current_item_progress.as_ref(),
                BatchRenderEvent::Progress(progress) => progress.current_item_progress.as_ref(),
            })
            .filter_map(|p| p.rendering_fps)
            .collect::<Vec<_>>();
        assert!(!single_fps.is_empty() && !batch_fps.is_empty());
        println!("{mode:?}: single {single_seconds:.3}s, FPS {single_fps:?}; batch {batch_seconds:.3}s, FPS {batch_fps:?}");
    }
}
