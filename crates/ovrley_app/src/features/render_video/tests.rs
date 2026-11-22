//! The render dialog: ports of `useRenderWorkflow.test.jsx`, `useBatchRenderWorkflow.test.jsx`
//! and `useRenderVideoDialogState.test.jsx`, with the fake render service and dialogs, and the
//! plan's new cases (Phase 7 Tests).

use super::request::tests::{imported, with_videos};
use super::session::tests::snapshot;
use super::*;
use crate::lib::template::ThemeColors;
use crate::lib::video_program::fixtures::{video, TestVideo};
use crate::lib::video_program::{ImportedVideo, VideoTelemetry};
use crate::store::services::fakes::{fake_services, FakeRenderCall, Fakes};
use crate::store::services::BatchServiceError;
use crate::store::Dispatcher;
use ovrley_core::debug::{RenderProgress, RenderStatus};
use ovrley_core::encode::progress::ProgressSink;
use ovrley_core::encode::ffmpeg::detect::AvailableCodecs;
use ovrley_core::render_jobs::contracts::{BatchAcceptance, BatchActivity, BatchDestination};
use ovrley_widgets::model::document::OverlayConfig;
use serde_json::json;
use std::time::{Duration, Instant};

fn dispatcher() -> (Dispatcher, Fakes) {
    let (parts, fakes) = fake_services(json!({"last-render-output-dir": "/renders"}));
    let mut store = Store {
        theme: Some(ThemeColors {
            ice: "#010101".into(),
            aqua: "#020202".into(),
            teal: "#030303".into(),
            accent: "#040404".into(),
        }),
        ..Store::default()
    };
    store.editor.config = Rc::new(
        OverlayConfig::from_value(&json!({"scene": {"width": 1920, "height": 1080, "fps": 30}}))
            .unwrap(),
    );
    store
        .media
        .activate_activity_file(imported(&[0.0, 10.0, 20.0], 20.0));
    // Software encoders only, detected.
    store.video_import.available_codecs =
        crate::features::video_preview::filmstrips::CodecDetection::Done(Some(AvailableCodecs {
            prores_ks: true,
            qtrle: true,
            ..AvailableCodecs::default()
        }));
    (Dispatcher::new(store, parts), fakes)
}

/// Runs posted worker results until `done` holds.
fn wait_for(dispatcher: &Dispatcher, fakes: &Fakes, done: impl Fn(&Store) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        fakes.ui_post.drain(dispatcher);
        if dispatcher.with_store(&done) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the store never reached the expected state"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn read<R>(dispatcher: &Dispatcher, read: impl FnOnce(&Store) -> R) -> R {
    dispatcher.with_store(read)
}

fn draft(dispatcher: &Dispatcher) -> RenderSettingsDraft {
    read(dispatcher, |store| {
        store
            .render_dialog
            .draft
            .clone()
            .expect("the dialog has a draft")
    })
}

fn open(dispatcher: &Dispatcher) {
    dispatcher.dispatch(open_render_dialog);
}

fn start(dispatcher: &Dispatcher) {
    dispatcher.dispatch(|store, services| start_render(store, services, false, None));
}

fn import(
    dispatcher: &Dispatcher,
    videos: Vec<ImportedVideo>,
    offsets: &'static [(&'static str, f64)],
) {
    dispatcher.dispatch(move |store, _| with_videos(store, videos, offsets));
}

fn clip_video(id: &'static str, path: &'static str, import_order: u32) -> ImportedVideo {
    video(TestVideo {
        id,
        path,
        duration_seconds: 12.0,
        import_order,
        ..TestVideo::default()
    })
}

fn accepted_batch(batch_id: &str) -> BatchAcceptance {
    BatchAcceptance {
        batch_id: batch_id.into(),
        snapshot: snapshot(batch_id, 1, 1, true),
    }
}

#[test]
fn renders_the_trimmed_overlay_without_videos_and_commits_its_settings_outside_the_history() {
    let (dispatcher, fakes) = dispatcher();
    dispatcher.dispatch(|store, _| {
        store.media.activity_trim = Some(ovrley_core::activity::trim::ActivityTrim {
            start_second: 5.25,
            end_second: 15.75,
        })
    });
    let history_before = read(&dispatcher, |store| store.history.past.len());
    open(&dispatcher);
    assert_eq!(
        draft(&dispatcher).output_path,
        "/renders/overlay_1.mov",
        "core's suggestion in the remembered folder"
    );
    assert!(
        !read(&dispatcher, crate::features::undo_redo::history_enabled),
        "history waits while the dialog is open"
    );
    start(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| store.media.rendering_video);
    let config = fakes.render.singles.lock().unwrap()[0].clone();
    assert_eq!(
        (
            config.scene.start,
            config.scene.end,
            config.scene.ffmpeg["codec"].clone()
        ),
        (5.25, 15.75, json!("prores_ks"))
    );
    assert_eq!(
        fakes.render.calls()[0],
        FakeRenderCall::SubmitSingle {
            output_path: "/renders/overlay_1.mov".into(),
            overwrite: false
        }
    );
    read(&dispatcher, |store| {
        assert_eq!(
            (
                store.render_settings.export_mode,
                store.render_settings.codec.as_str()
            ),
            (ExportMode::Transparent, "prores_ks")
        );
        assert_eq!(
            store.history.past.len(),
            history_before,
            "accepted settings are committed outside the history"
        );
        assert_eq!(store.render_dialog.phase, DialogPhase::Progress);
    });
}

#[test]
fn defaults_to_composite_with_a_video_and_to_transparent_without_one() {
    let (dispatcher, _) = dispatcher();
    import(
        &dispatcher,
        vec![clip_video("clip", "/videos/clip.mp4", 0)],
        &[("clip", 0.0)],
    );
    open(&dispatcher);
    assert_eq!(
        draft(&dispatcher).settings.export_mode,
        ExportMode::Composite
    );
    assert_eq!(
        draft(&dispatcher).settings.codec,
        "libx264",
        "full video moves to the first available MP4 codec"
    );
    dispatcher.dispatch(|store, _| close_render_dialog(store));
    import(&dispatcher, Vec::new(), &[]);
    open(&dispatcher);
    assert_eq!(
        draft(&dispatcher).settings.export_mode,
        ExportMode::Transparent
    );
}

#[test]
fn the_output_mode_is_an_undo_step_and_the_dialog_stays_open_while_a_batch_runs() {
    let (dispatcher, _) = dispatcher();
    import(
        &dispatcher,
        vec![
            clip_video("clip-a", "/videos/a.mp4", 0),
            clip_video("clip-b", "/videos/b.mp4", 1),
        ],
        &[("clip-b", 20.0)],
    );
    open(&dispatcher);
    assert_eq!(
        draft(&dispatcher).settings.output_mode,
        OutputMode::Combined
    );
    let history_before = read(&dispatcher, |store| store.history.past.len());
    dispatcher.dispatch(|store, _| set_output_mode(store, OutputMode::Separate));
    read(&dispatcher, |store| {
        assert_eq!(
            store.render_settings.output_mode,
            OutputMode::Separate,
            "written to the store at once"
        );
        assert_eq!(
            store.history.past.len(),
            history_before + 1,
            "zundo's oddity: an undo step"
        );
    });
    dispatcher.dispatch(|store, _| {
        store
            .batch_render
            .accept_batch_snapshot(snapshot("batch-1", 1, 1, true))
    });
    dispatcher.dispatch(|store, _| close_render_dialog(store));
    assert_eq!(
        read(&dispatcher, |store| store.render_dialog.phase),
        DialogPhase::Confirm,
        "close waits for the batch"
    );
    dispatcher.dispatch(|store, _| {
        store
            .batch_render
            .apply_batch_snapshot(snapshot("batch-1", 2, 2, false))
    });
    dispatcher.dispatch(|store, _| close_render_dialog(store));
    read(&dispatcher, |store| {
        assert!(
            store.render_dialog.phase == DialogPhase::Closed
                && store.batch_render.batch_snapshot.is_none()
        )
    });
    open(&dispatcher);
    assert_eq!(
        (
            draft(&dispatcher).settings.output_mode,
            draft(&dispatcher).settings.export_mode
        ),
        (OutputMode::Separate, ExportMode::Composite)
    );
}

#[test]
fn an_explicit_transparent_setting_stays_with_a_video() {
    let (dispatcher, _) = dispatcher();
    import(
        &dispatcher,
        vec![clip_video("clip", "/videos/clip.mp4", 0)],
        &[("clip", 0.0)],
    );
    dispatcher.dispatch(|store, _| store.render_settings.export_mode = ExportMode::Transparent);
    open(&dispatcher);
    assert_eq!(
        draft(&dispatcher).settings.export_mode,
        ExportMode::Transparent
    );
}

#[test]
fn renders_videos_as_one_combined_file_and_asks_before_replacing_it() {
    let (dispatcher, fakes) = dispatcher();
    fakes
        .render
        .sources
        .lock()
        .unwrap()
        .insert("/videos/clip.mp4".into(), Ok("source-clip".into()));
    import(
        &dispatcher,
        vec![clip_video("clip", "/videos/clip.mp4", 0)],
        &[("clip", 0.0)],
    );
    open(&dispatcher);
    // Start waits for the inspection.
    wait_for(&dispatcher, &fakes, |store| ready_batch(store).is_some());
    fakes
        .render
        .batch_results
        .lock()
        .unwrap()
        .push_back(Err(BatchServiceError::AlreadyExists {
            message: "exists".into(),
        }));
    fakes
        .render
        .batch_results
        .lock()
        .unwrap()
        .push_back(Ok(accepted_batch("batch-1")));
    start(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| {
        store.render_dialog.pending_overwrite.is_some()
    });
    let path = draft(&dispatcher).output_path;
    assert_eq!(path, "/renders/video_1.mp4");
    let request = fakes.render.batches.lock().unwrap()[0].clone();
    assert_eq!(request.inspection_id, "inspection-1");
    assert!(
        matches!(&request.destination, BatchDestination::File { path: written, overwrite: false } if *written == path)
    );
    assert_eq!(request.outputs[0].pieces[0].source_id, "source-clip");
    assert!(
        matches!(&request.activity, BatchActivity::ExternalActivity { offsets, .. } if offsets["source-clip"] == 0.0)
    );
    dispatcher.dispatch(confirm_overwrite);
    wait_for(&dispatcher, &fakes, |store| {
        store.batch_render.batch_snapshot.is_some()
    });
    assert!(matches!(
        fakes.render.batches.lock().unwrap()[1].destination,
        BatchDestination::File {
            overwrite: true,
            ..
        }
    ));
    assert_eq!(
        read(&dispatcher, |store| store
            .render_dialog
            .pending_overwrite
            .clone()),
        None
    );
}

#[test]
fn video_telemetry_enables_an_mp4_render_with_embedded_activity() {
    let (dispatcher, fakes) = dispatcher();
    dispatcher.dispatch(|store, _| store.media.clear_activity_file(true));
    let mut clip = clip_video("clip", "/videos/clip.mp4", 0);
    clip.telemetry =
        VideoTelemetry::without_activity(crate::lib::video_program::TelemetryStatus::Absent)
            .unwrap();
    import(&dispatcher, vec![clip.clone()], &[("clip", 40.0)]);
    assert_eq!(
        read(&dispatcher, render_disabled_reason).as_deref(),
        Some("Load a GPX/FIT activity first")
    );
    clip.telemetry = VideoTelemetry::ready(imported(&[0.0, 12.0], 12.0));
    import(&dispatcher, vec![clip], &[("clip", 40.0)]);
    assert_eq!(read(&dispatcher, render_disabled_reason), None);
    fakes
        .render
        .sources
        .lock()
        .unwrap()
        .insert("/videos/clip.mp4".into(), Ok("source-clip".into()));
    fakes
        .render
        .batch_results
        .lock()
        .unwrap()
        .push_back(Ok(accepted_batch("batch-1")));
    open(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| ready_batch(store).is_some());
    start(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| {
        store.batch_render.batch_snapshot.is_some()
    });
    assert!(matches!(
        fakes.render.batches.lock().unwrap()[0].activity,
        BatchActivity::EmbeddedActivity {}
    ));
}

#[test]
fn an_existing_output_keeps_the_draft_and_retries_the_unchanged_path_only_after_confirmation() {
    let (dispatcher, fakes) = dispatcher();
    fakes
        .render
        .single_results
        .lock()
        .unwrap()
        .push_back(Err(CoreError::OutputExists("exists".into())));
    open(&dispatcher);
    let submitted = draft(&dispatcher).output_path;
    start(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| {
        store.render_dialog.pending_overwrite.is_some()
    });
    assert_eq!(
        read(&dispatcher, |store| store.render_dialog.phase),
        DialogPhase::Confirm
    );
    dispatcher.dispatch(confirm_overwrite);
    wait_for(&dispatcher, &fakes, |store| store.media.rendering_video);
    assert_eq!(
        fakes.render.calls()[1],
        FakeRenderCall::SubmitSingle {
            output_path: submitted,
            overwrite: true
        }
    );
}

#[test]
fn a_changed_path_cancels_the_overwrite_instead_of_writing() {
    let (dispatcher, fakes) = dispatcher();
    fakes
        .render
        .single_results
        .lock()
        .unwrap()
        .push_back(Err(CoreError::OutputExists("exists".into())));
    open(&dispatcher);
    start(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| {
        store.render_dialog.pending_overwrite.is_some()
    });
    dispatcher.dispatch(|store, services| {
        start_render(store, services, true, Some("/elsewhere.mov".into()))
    });
    assert_eq!(
        read(&dispatcher, |store| store
            .render_dialog
            .pending_overwrite
            .clone()),
        None
    );
    assert_eq!(fakes.render.calls().len(), 1, "nothing was submitted again");
}

#[test]
fn the_dialog_stays_open_while_acceptance_is_pending() {
    let (dispatcher, fakes) = dispatcher();
    open(&dispatcher);
    start(&dispatcher);
    assert_eq!(
        read(&dispatcher, |store| store.media.render_submission_target),
        Some(RenderTarget::Single)
    );
    dispatcher.dispatch(|store, _| close_render_dialog(store));
    assert_eq!(
        read(&dispatcher, |store| store.render_dialog.phase),
        DialogPhase::Confirm
    );
    wait_for(&dispatcher, &fakes, |store| store.media.rendering_video);
}

#[test]
fn output_errors_stay_with_the_draft_and_other_errors_close_and_alert() {
    let (dispatcher, fakes) = dispatcher();
    fakes
        .render
        .single_results
        .lock()
        .unwrap()
        .push_back(Err(CoreError::OutputInvalid(
            RenderPathError::OutputDirectoryMissing {
                path: "/gone".into(),
            },
        )));
    open(&dispatcher);
    start(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| {
        store.render_dialog.path_error.is_some()
    });
    let view = read(&dispatcher, |store| {
        view::render_dialog_view(store).unwrap()
    });
    assert_eq!(
        view.settings.path_error.as_deref(),
        Some("The output directory does not exist")
    );
    assert_eq!(view.phase, DialogPhase::Confirm);
    fakes
        .render
        .single_results
        .lock()
        .unwrap()
        .push_back(Err(CoreError::Config(
            "Invalid render configuration".into(),
        )));
    start(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| {
        store.media.error_message.is_some()
    });
    read(&dispatcher, |store| {
        assert_eq!(store.render_dialog.phase, DialogPhase::Closed);
        assert!(store
            .media
            .error_message
            .as_deref()
            .unwrap()
            .contains("Invalid render configuration"));
    });
}

#[test]
fn acceptance_enters_progress_remembers_the_path_and_completion_opens_the_file() {
    let (dispatcher, fakes) = dispatcher();
    open(&dispatcher);
    start(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| store.media.rendering_video);
    let path = read(&dispatcher, |store| {
        store.media.active_render_output_path.clone()
    })
    .unwrap();
    assert_eq!(
        fakes.preferences.saves.borrow().last().and_then(|saved| saved.get("last-render-output-dir").cloned()),
        Some(json!("/renders"))
    );
    let id = read(&dispatcher, |store| store.media.active_render_id).unwrap();
    fakes.render.mailbox.emit_progress(&RenderProgress {
        render_id: id,
        current: 1,
        total: 1,
        status: RenderStatus::Complete,
        ..RenderProgress::default()
    });
    wait_for(&dispatcher, &fakes, |store| !store.media.rendering_video);
    assert!(fakes
        .render
        .calls()
        .contains(&FakeRenderCall::OpenFile(path)));
    read(&dispatcher, |store| {
        assert_eq!(store.media.active_render_output_path, None);
        assert_eq!(
            store.render_dialog.phase,
            DialogPhase::Closed,
            "the progress phase ends with its render"
        );
        assert!(crate::features::undo_redo::history_enabled(store));
    });
}

#[test]
fn a_render_that_ended_before_its_acceptance_still_ends_the_session() {
    let (dispatcher, fakes) = dispatcher();
    // Core reports the outcome before the acceptance action runs on the UI thread.
    *fakes.render.progress.lock().unwrap() = RenderProgress {
        render_id: 1,
        status: RenderStatus::Error,
        message: "ffmpeg exited".into(),
        ..RenderProgress::default()
    };
    open(&dispatcher);
    start(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| {
        store.media.error_message.is_some()
    });
    read(&dispatcher, |store| {
        assert!(!store.media.rendering_video && store.render_dialog.phase == DialogPhase::Closed)
    });
}

#[test]
fn a_custom_fps_stays_custom_across_other_edits() {
    let (dispatcher, _) = dispatcher();
    open(&dispatcher);
    dispatcher.dispatch(|store, _| select_fps_mode(store, "custom"));
    dispatcher.dispatch(|store, _| set_custom_fps(store, 30.0));
    dispatcher.dispatch(|store, _| set_update_rate(store, 2));
    let view = read(&dispatcher, |store| {
        view::render_dialog_view(store).unwrap()
    });
    assert_eq!(
        (view.settings.fps_mode.as_str(), view.settings.update_rate),
        ("30", 2),
        "30 is a preset, so it is not anchored"
    );
    dispatcher.dispatch(|store, _| set_custom_fps(store, 50.0));
    dispatcher.dispatch(|store, _| set_render_full_activity(store, false));
    let view = read(&dispatcher, |store| {
        view::render_dialog_view(store).unwrap()
    });
    assert_eq!(
        (view.settings.fps_mode.as_str(), view.settings.fps.as_str()),
        ("custom", "50")
    );
    dispatcher.dispatch(|store, _| select_fps_mode(store, "60"));
    assert_eq!(draft(&dispatcher).settings.fps, 60.0);
}

#[test]
fn codec_detection_ending_while_open_moves_full_video_to_an_mp4_codec() {
    let (dispatcher, _) = dispatcher();
    dispatcher.dispatch(|store, _| {
        store.video_import.available_codecs =
            crate::features::video_preview::filmstrips::CodecDetection::Detecting
    });
    import(
        &dispatcher,
        vec![clip_video("clip", "/videos/clip.mp4", 0)],
        &[("clip", 0.0)],
    );
    open(&dispatcher);
    assert_eq!(
        draft(&dispatcher).settings.codec,
        "prores_ks",
        "nothing is available yet"
    );
    let view = read(&dispatcher, |store| {
        view::render_dialog_view(store).unwrap()
    });
    assert!(view
        .settings
        .formats
        .iter()
        .filter(|entry| entry.value == "h264")
        .all(|entry| entry.disabled && entry.trailing == "Unavailable"));
    assert!(view.settings.start_disabled);
    dispatcher.dispatch(|store, _| {
        store.video_import.available_codecs =
            crate::features::video_preview::filmstrips::CodecDetection::Done(Some(
                AvailableCodecs::default(),
            ))
    });
    assert_eq!(draft(&dispatcher).settings.codec, "libx264");
}

#[test]
fn a_codec_outside_the_catalog_opens_and_the_user_picks_another() {
    let (dispatcher, _) = dispatcher();
    dispatcher.dispatch(|store, _| {
        store.render_settings.codec = "x264_custom".into();
        store.render_settings.export_mode = ExportMode::Transparent;
    });
    open(&dispatcher);
    let view = read(&dispatcher, |store| {
        view::render_dialog_view(store).unwrap()
    });
    assert_eq!(
        (
            view.settings.format.as_str(),
            view.settings.accelerations.len()
        ),
        ("", 0),
        "no format is selected"
    );
    assert!(view.settings.start_disabled, "Start waits for a format");
    dispatcher.dispatch(|store, _| select_format(store, OutputFormat::Qtrle));
    let view = read(&dispatcher, |store| {
        view::render_dialog_view(store).unwrap()
    });
    assert_eq!(
        (view.settings.format.as_str(), view.settings.start_disabled),
        ("qtrle", false)
    );

    let (dispatcher, _) = self::dispatcher();
    import(
        &dispatcher,
        vec![clip_video("clip", "/videos/clip.mp4", 0)],
        &[("clip", 0.0)],
    );
    dispatcher.dispatch(|store, _| store.render_settings.codec = "x264_custom".into());
    open(&dispatcher);
    assert_eq!(
        draft(&dispatcher).settings.codec,
        "libx264",
        "full video replaces it"
    );
}

#[test]
fn format_and_quality_edits_follow_reacts_handlers() {
    let (dispatcher, _) = dispatcher();
    import(
        &dispatcher,
        vec![clip_video("clip", "/videos/clip.mp4", 0)],
        &[("clip", 0.0)],
    );
    open(&dispatcher);
    dispatcher.dispatch(|store, _| select_format(store, OutputFormat::Hevc));
    assert_eq!(
        (
            draft(&dispatcher).settings.codec.as_str(),
            draft(&dispatcher).settings.quality
        ),
        ("libx265", EncodingQuality::Quality(26))
    );
    dispatcher.dispatch(|store, _| set_quality_type(store, QualityType::Bitrate));
    assert_eq!(
        draft(&dispatcher).settings.quality,
        EncodingQuality::Bitrate(20.0),
        "1080p at 30 fps"
    );
    dispatcher.dispatch(|store, _| set_quality_slider(store, 45.0));
    assert_eq!(
        draft(&dispatcher).settings.quality,
        EncodingQuality::Bitrate(45.0)
    );
    dispatcher.dispatch(|store, _| set_quality_type(store, QualityType::Quality));
    dispatcher.dispatch(|store, _| set_quality_slider(store, 30.0));
    assert_eq!(
        draft(&dispatcher).settings.quality,
        EncodingQuality::Quality(15),
        "the slider runs inverted"
    );
    dispatcher.dispatch(|store, _| set_export_mode(store, ExportMode::Transparent));
    let draft = draft(&dispatcher);
    assert_eq!(
        (draft.settings.codec.as_str(), draft.output_path.as_str()),
        ("prores_ks", "/renders/video_1.mov"),
        "the codec and the extension follow the mode"
    );
}

#[test]
fn the_whole_activity_is_offered_only_for_a_transparent_video_export_with_an_activity() {
    let (dispatcher, _) = dispatcher();
    import(
        &dispatcher,
        vec![clip_video("clip", "/videos/clip.mp4", 0)],
        &[("clip", 0.0)],
    );
    open(&dispatcher);
    let shown = |dispatcher: &Dispatcher| {
        read(dispatcher, |store| {
            view::render_dialog_view(store)
                .unwrap()
                .settings
                .show_render_full_activity
        })
    };
    assert!(!shown(&dispatcher), "not for full video");
    dispatcher.dispatch(|store, _| set_export_mode(store, ExportMode::Transparent));
    assert!(shown(&dispatcher));
    dispatcher.dispatch(|store, _| set_render_full_activity(store, true));
    assert_eq!(
        read(&dispatcher, |store| render_transport(
            store,
            store.render_dialog.draft.as_ref()
        )),
        RenderTransport::Single
    );
    dispatcher.dispatch(|store, _| set_export_mode(store, ExportMode::Composite));
    assert!(
        !draft(&dispatcher).render_full_activity,
        "full video turns it off"
    );
}

#[test]
fn inspection_is_kept_across_openings_and_a_new_collection_disposes_it() {
    let (dispatcher, fakes) = dispatcher();
    fakes
        .render
        .sources
        .lock()
        .unwrap()
        .insert("/videos/a.mp4".into(), Ok("source-a".into()));
    import(
        &dispatcher,
        vec![clip_video("a", "/videos/a.mp4", 0)],
        &[("a", 0.0)],
    );
    open(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| ready_batch(store).is_some());
    dispatcher.dispatch(|store, _| close_render_dialog(store));
    open(&dispatcher);
    assert!(
        read(&dispatcher, |store| ready_batch(store).is_some()),
        "the inspection stayed"
    );
    let inspections = |fakes: &Fakes| {
        fakes
            .render
            .calls()
            .into_iter()
            .filter(|call| matches!(call, FakeRenderCall::CreateInspection(_)))
            .count()
    };
    assert_eq!(inspections(&fakes), 1);
    dispatcher.dispatch(|store, _| close_render_dialog(store));
    import(
        &dispatcher,
        vec![clip_video("b", "/videos/b.mp4", 0)],
        &[("b", 0.0)],
    );
    open(&dispatcher);
    assert_eq!(inspections(&fakes), 2);
    assert!(fakes
        .render
        .calls()
        .contains(&FakeRenderCall::DisposeInspection("inspection-1".into())));
    wait_for(&dispatcher, &fakes, |store| {
        store
            .render_dialog
            .review()
            .is_some_and(|review| review.rows[0].error.is_some())
    });
}

#[test]
fn stale_sources_mark_their_rows_and_block_start_until_inspected_again() {
    let (dispatcher, fakes) = dispatcher();
    fakes
        .render
        .sources
        .lock()
        .unwrap()
        .insert("/videos/a.mp4".into(), Ok("source-a".into()));
    import(
        &dispatcher,
        vec![clip_video("a", "/videos/a.mp4", 0)],
        &[("a", 0.0)],
    );
    open(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| ready_batch(store).is_some());
    fakes.render.batch_results.lock().unwrap().push_back(Err(
        BatchServiceError::ReinspectionRequired {
            message: "stale".into(),
            rejection: ovrley_core::render_jobs::inspection::InspectionRejection {
                inspection_id: "inspection-1".into(),
                issues: vec![
                    ovrley_core::render_jobs::inspection::SourceInspectionIssue {
                        source_id: "source-a".into(),
                        path: Some("/videos/a.mp4".into()),
                        reason:
                            ovrley_core::render_jobs::inspection::ReinspectionReason::SourceChanged,
                    },
                ],
            },
        },
    ));
    start(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| {
        !store.render_dialog.inspection.stale.is_empty()
    });
    read(&dispatcher, |store| {
        assert!(ready_batch(store).is_none());
        let view = view::render_dialog_view(store).unwrap();
        assert!(view.start_disabled && view.review_error.is_some());
        assert_eq!(
            store.render_dialog.review().unwrap().rows[0].status,
            review::RowStatus::Blocked
        );
    });
}

#[test]
fn missing_videos_open_the_notice_instead() {
    let (dispatcher, _) = dispatcher();
    let mut missing = clip_video("a", "/videos/a.mp4", 0);
    missing.missing = true;
    import(
        &dispatcher,
        vec![missing, clip_video("b", "/videos/b.mp4", 1)],
        &[("a", 0.0), ("b", 20.0)],
    );
    open(&dispatcher);
    let notices = read(&dispatcher, view::render_notices_view);
    assert_eq!(
        read(&dispatcher, |store| store.render_dialog.phase),
        DialogPhase::Closed
    );
    assert_eq!(
        notices.missing.unwrap().1,
        "1 video could not be found. Locate it or remove it from the project before exporting."
    );
    dispatcher.dispatch(|store, _| dismiss_missing_videos(store));
    assert!(read(&dispatcher, view::render_notices_view)
        .missing
        .is_none());
}

#[test]
fn a_resolution_mismatch_replaces_the_settings() {
    let (dispatcher, _) = dispatcher();
    import(
        &dispatcher,
        vec![video(TestVideo {
            id: "wide",
            path: "/videos/wide.mp4",
            width: 3840,
            height: 2160,
            ..TestVideo::default()
        })],
        &[("wide", 0.0)],
    );
    open(&dispatcher);
    let view = read(&dispatcher, |store| {
        view::render_dialog_view(store).unwrap()
    });
    assert_eq!(
        view.mismatch.as_deref(),
        Some("Overlay resolution 1920x1080 must match imported video (3840x2160).")
    );
    assert!(view.start_disabled);
}

#[test]
fn shortcuts_and_drops_wait_while_the_dialog_is_open() {
    let (dispatcher, _) = dispatcher();
    open(&dispatcher);
    read(&dispatcher, |store| {
        assert!(store.render_dialog.is_open());
        assert!(!crate::features::app_shell::accepts_file_drops(store));
    });
}

#[test]
fn the_progress_panel_rounds_labels_and_keeps_fractional_progress() {
    let (dispatcher, fakes) = dispatcher();
    open(&dispatcher);
    start(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| store.media.rendering_video);
    let id = read(&dispatcher, |store| store.media.active_render_id).unwrap();
    fakes.render.mailbox.emit_progress(&RenderProgress {
        render_id: id,
        busy: true,
        current: 1234,
        total: 3000,
        status: RenderStatus::Rendering,
        rendering_fps: Some(47.26),
        estimated_seconds_remaining: Some(125),
        ..RenderProgress::default()
    });
    wait_for(&dispatcher, &fakes, |store| {
        store.media.render_progress.current == 1234
    });
    let progress = read(&dispatcher, |store| {
        view::render_dialog_view(store).unwrap().progress
    });
    assert_eq!(
        (
            progress.percent_text.as_str(),
            progress.frames_text.as_str()
        ),
        ("41% Complete", "1,234 / 3,000 frames")
    );
    assert!((progress.percent - 41.133).abs() < 0.01);
    assert_eq!(
        (
            progress.fps.as_str(),
            progress.remaining.as_str(),
            progress.title.as_str()
        ),
        ("47.3", "2:05", "Exporting Overlay")
    );
    assert_eq!(
        progress.summary,
        ["1920x1080", "30 fps", "Update 1/1", "ProRes", "20 sec"]
    );
    dispatcher.dispatch(cancel_dialog_render);
    assert_eq!(fakes.render.calls().last(), Some(&FakeRenderCall::Cancel));
    assert!(read(&dispatcher, |store| view::render_dialog_view(store)
        .unwrap()
        .progress
        .cancelling));
}

#[test]
fn a_source_that_fails_inspection_blocks_one_combined_file_and_start() {
    let (dispatcher, fakes) = dispatcher();
    import(&dispatcher, vec![clip_video("a", "/videos/a.mp4", 0), clip_video("b", "/videos/b.mp4", 1)], &[("a", 0.0), ("b", 20.0)]);
    open(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| store.render_dialog.inspection.complete());
    let view = read(&dispatcher, |store| view::render_dialog_view(store).unwrap());
    assert_eq!(view.blockers, ["Some videos cannot be exported (a.mp4, b.mp4)."]);
    assert!(view.start_disabled);
}

#[test]
fn closing_the_window_during_a_render_cancels_it_and_closes_after_cleanup() {
    use crate::store::services::fakes::WindowCall;
    use std::sync::atomic::Ordering;
    let (dispatcher, fakes) = dispatcher();
    open(&dispatcher);
    start(&dispatcher);
    wait_for(&dispatcher, &fakes, |store| store.media.rendering_video);
    fakes.render.busy.store(true, Ordering::SeqCst);
    dispatcher.dispatch(close_window);
    assert!(fakes.render.calls().contains(&FakeRenderCall::Cancel), "the close cancels without asking");
    assert!(fakes.window.calls.borrow().is_empty(), "the window waits for the cleanup");
    fakes.render.busy.store(false, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(5);
    while fakes.window.calls.borrow().is_empty() {
        fakes.ui_post.drain(&dispatcher);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(*fakes.window.calls.borrow(), [WindowCall::Close]);
}

#[test]
fn the_png_frame_holds_the_renderer_and_opens_its_file() {
    let (dispatcher, fakes) = dispatcher();
    dispatcher.dispatch(|store, _| store.editor.set_selected_second(3.5));
    dispatcher.dispatch(render_preview_frame);
    assert_eq!(read(&dispatcher, |store| store.media.render_submission_target), Some(RenderTarget::Preview));
    assert!(read(&dispatcher, crate::features::projects::is_busy));
    wait_for(&dispatcher, &fakes, |store| store.media.render_submission_target.is_none());
    assert_eq!(fakes.render.calls(), [FakeRenderCall::PreviewFrame { second: 3.5 }, FakeRenderCall::OpenFile("/downloads/preview_frame.png".into())]);
}

#[test]
fn the_header_explains_why_start_is_disabled() {
    let (dispatcher, _) = dispatcher();
    assert_eq!(read(&dispatcher, header_actions_view).start_tooltip, "");
    dispatcher.dispatch(|store, _| store.media.render_submission_target = Some(RenderTarget::Preview));
    let view = read(&dispatcher, header_actions_view);
    assert!(view.start_disabled);
    assert_eq!(view.start_tooltip, "Rendering already in progress");
}
