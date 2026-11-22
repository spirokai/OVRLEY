//! Debug builds only: `OVRLEY_SNAPSHOT=<dir>` renders the window through Slint's own
//! `take_snapshot()` into `<dir>/shell-<n>-<state>.png` for a fixed tour of shell states, then
//! quits. It needs no screen-recording permission, as the Phase 1 showcase's snapshots did.
//! `OVRLEY_SNAPSHOT_SIZE=<width>x<height>` sets the window size.

use crate::AppWindow;
use ovrley_app::store::services::Services;
use ovrley_app::store::{Dispatcher, Store};
use slint::ComponentHandle;
use std::path::PathBuf;
use std::time::Duration;

/// One state of the tour: its name and what sets it up.
type Step = (&'static str, StepKind);
type StepAction = fn(&mut Store, &mut Services);

#[derive(Clone, Copy)]
enum StepKind {
    Action(StepAction),
    /// Toggles native full screen (macOS: the header's inset follows AppKit's notifications).
    Fullscreen,
    /// Clicks the control panel's Widgets tab, which is local state of the markup.
    WidgetsTab,
}

fn tour() -> Vec<Step> {
    use ovrley_app::features::app_shell::SettingsSection;
    use ovrley_app::features::app_update::UpdatePhase;
    use ovrley_app::i18n::{change_language_preference, Locale};
    use ovrley_app::store::slices::layout::LeftDrawerTool;
    let steps: Vec<(&'static str, StepAction)> = vec![
        // The startup projects dialog opens once its listing arrives; the tour shows it, then
        // dismisses it as the user would (D14).
        ("startup-projects", |store, _| store.projects.startup.open = true),
        ("shell", |store, _| {
            ovrley_app::features::projects::dismiss_startup_dialog(store);
            store.layout.set_left_drawer_pinned(false);
            store.layout.dismiss_left_drawer_overlay();
        }),
        ("drawer-overlay", |store, _| store.layout.select_left_drawer_tool(LeftDrawerTool::Activity)),
        ("drawer-pinned", |store, _| store.layout.set_left_drawer_pinned(true)),
        ("projects-drawer", |store, _| store.layout.select_left_drawer_tool(LeftDrawerTool::Projects)),
        ("projects-drawer-back", |store, _| store.layout.select_left_drawer_tool(LeftDrawerTool::Activity)),
        ("activity-imported", |store, services| {
            let ride = ovrley_services::runtime::source_repo_root().join("crates/ovrley_core/tests/fixtures/activity/test-ride.fit");
            ovrley_app::features::app_shell::import_activity(store, services, ride.to_string_lossy().into_owned(), None);
        }),
        ("activity-trimmed", |store, _| {
            let duration = store.media.activity_summary.as_ref().map_or(0.0, |summary| summary.duration_seconds);
            store.media.set_activity_trim(Some(ovrley_core::activity::trim::ActivityTrim { start_second: duration * 0.1, end_second: duration * 0.8 }));
            store.editor.set_selected_second(duration * 0.4);
            store.player.hovered_lane = Some(ovrley_app::store::slices::video_import::ClipLane::Activity);
        }),
        ("widget-selected", |store, _| {
            store.player.hovered_lane = None;
            store.layout.set_left_drawer_pinned(false);
            store.layout.dismiss_left_drawer_overlay();
            let ids = store.editor.config.widget_ids();
            let last = ids.last().cloned();
            store.editor.set_widget_selection(&ids[ids.len().saturating_sub(2)..], last.as_deref());
        }),
        ("template-confirm", |store, services| {
            store.layout.set_left_drawer_pinned(false);
            store.layout.dismiss_left_drawer_overlay();
            store.template.set_last_saved_template_state(None);
            ovrley_app::features::template_manager::create_new_template_request(store, services, None);
        }),
        ("update-available", |store, services| {
            ovrley_app::features::template_manager::answer_unsaved_confirm(store, services, ovrley_app::features::app_shell::UnsavedAnswer::Cancel);
            store.app_update.phase = UpdatePhase::Available(std::sync::Arc::new(SnapshotUpdate));
        }),
        ("update-available-korean", |store, _| store.i18n.language = Locale::Ko),
        ("update-downloading", |store, _| {
            store.i18n.language = Locale::En;
            store.app_update.phase = UpdatePhase::Downloading { update: std::sync::Arc::new(SnapshotUpdate), progress: None };
        }),
        ("update-progress", |store, _| {
            if let UpdatePhase::Downloading { progress, .. } = &mut store.app_update.phase {
                *progress = Some(ovrley_services::updater::DownloadProgress { downloaded: 84, content_length: Some(200) });
            }
        }),
        ("update-failed", |store, _| store.app_update.phase = UpdatePhase::Failed),
        ("settings-general", |store, services| {
            ovrley_app::features::app_update::dismiss(store);
            ovrley_app::features::template_manager::answer_unsaved_confirm(store, services, ovrley_app::features::app_shell::UnsavedAnswer::Cancel);
            store.layout.set_left_drawer_pinned(false);
            store.layout.dismiss_left_drawer_overlay();
            store.editor_shell.set_settings_section(SettingsSection::Settings);
            store.editor_shell.open_settings();
        }),
        ("settings-shortcuts", |store, _| store.editor_shell.set_settings_section(SettingsSection::Shortcuts)),
        ("error-over-dialog", |store, _| store.media.set_error_message("Project save failed [project_error:save_failed]".into())),
        ("german", |store, services| {
            store.editor_shell.close_settings();
            store.media.set_error_message("ffmpeg exited with status 1:\nInvalid argument".into());
            store.layout.select_left_drawer_tool(LeftDrawerTool::Widgets);
            change_language_preference(store, services, Locale::De);
        }),
        ("english-again", |store, services| {
            store.media.clear_error();
            store.layout.dismiss_left_drawer_overlay();
            change_language_preference(store, services, Locale::En);
        }),
        // The render dialog (Phase 7): the settings without videos, a single render's progress
        // (a session set as acceptance sets it), the settings with two videos, the mismatch screen.
        ("render-settings", |store, services| ovrley_app::features::render_video::open_render_dialog(store, services)),
        ("render-progress", |store, _| {
            store.render_dialog.phase = ovrley_app::features::render_video::DialogPhase::Progress;
            store.media.start_render_session(999, "/tmp/overlay.mov".into(), "Starting render...".into());
            store.media.render_progress = ovrley_core::debug::RenderProgress {
                render_id: 999,
                busy: true,
                current: 1234,
                total: 3000,
                rendered: 1234,
                status: ovrley_core::debug::RenderStatus::Rendering,
                rendering_fps: Some(47.3),
                estimated_seconds_remaining: Some(125),
                ..Default::default()
            };
        }),
        ("render-videos", |store, services| {
            store.media.clear_render_session();
            store.render_dialog.phase = ovrley_app::features::render_video::DialogPhase::Closed;
            ovrley_app::features::render_video::close_render_dialog(store);
            snapshot_videos(store, &[(3840, 2160, 0.0), (3840, 2160, 90.0)]);
            ovrley_app::features::render_video::open_render_dialog(store, services);
        }),
        ("render-mismatch", |store, services| {
            ovrley_app::features::render_video::close_render_dialog(store);
            snapshot_videos(store, &[(1920, 1080, 0.0)]);
            ovrley_app::features::render_video::open_render_dialog(store, services);
        }),
        ("render-closed", |store, _| {
            ovrley_app::features::render_video::close_render_dialog(store);
            snapshot_videos(store, &[]);
        }),
        // Sync Doctor (Phase 8): the Autodetect tab over the pair, then the Manual Sync tab with
        // landmarks, a course location and a search.
        ("sync-autodetect", |store, _| {
            use ovrley_app::store::slices::manual_video_sync as manual;
            snapshot_videos(store, &[(1920, 1080, 30.0)]);
            manual::select_video_sync_video(store, Some("clip-0".into())).expect("a snapshot clip");
            store.layout.select_left_drawer_tool(LeftDrawerTool::VideoSync);
        }),
        ("sync-manual", |store, services| {
            use ovrley_app::features::video_sync::{calculate, matching::MatchScope};
            use ovrley_app::store::slices::manual_video_sync as manual;
            use ovrley_core::media::manual_video_sync::LandmarkKind;
            store.layout.open_manual_video_sync();
            for (kind, second) in [(LandmarkKind::Stop, 5.0), (LandmarkKind::LeftTurn, 12.0), (LandmarkKind::Location, 20.0)] {
                manual::add_landmark(store, kind, second).expect("a landmark inside the clip");
            }
            manual::set_detected_location(store, 100.0).expect("a course location");
            store.editor.set_selected_second(40.0);
            store.editor.timeline_viewport = ovrley_app::store::slices::editor::TimelineViewport { view_start: 0.0, view_end: 120.0 };
            calculate(store, services, MatchScope::All, None).expect("an eligible search");
        }),
        ("sync-closed", |store, _| {
            ovrley_app::store::slices::manual_video_sync::reset_video_sync_state(store);
            store.layout.select_left_drawer_tool(LeftDrawerTool::Widgets);
            store.layout.dismiss_left_drawer_overlay();
            snapshot_videos(store, &[]);
        }),
    ];
    let mut tour: Vec<Step> = steps.into_iter().map(|(name, action)| (name, StepKind::Action(action))).collect();
    tour.push(("widgets-tab", StepKind::WidgetsTab));
    tour.push((
        "widgets-tab-open",
        StepKind::Action(|store, _| {
            // A fixed document, so the capture does not depend on the template the shared
            // preferences restore.
            use ovrley_widgets::model::document::{create_widget_defaults, OverlayConfig, WidgetSelection};
            let mut values = Vec::new();
            for (id, kind) in [("speed", "speed"), ("distance", "distance")] {
                let selection = WidgetSelection { kind: kind.into(), ..WidgetSelection::default() };
                let mut data = create_widget_defaults(&selection, &store.template.global_defaults, &Default::default()).expect("catalog defaults");
                data.insert("id".into(), id.into());
                values.push(serde_json::Value::Object(data));
            }
            let config = OverlayConfig::from_value(&serde_json::json!({"scene": store.editor.config.scene, "values": values})).expect("a valid document");
            store.editor.set_config(config);
            store.editor.set_selected_widget_id(Some("speed"));
        }),
    ));
    // One editor per capture (phase 5): the widget alone in the document, selected.
    tour.push(("editor-arc", StepKind::Action(|store, _| show_editor(store, "speed", "arc", serde_json::json!({})))));
    tour.push(("editor-linear-bars", StepKind::Action(|store, _| show_editor(store, "speed", "linear", serde_json::json!({"track_fill_style": "bars", "bar_count": 12, "bar_gap": 4})))));
    tour.push(("editor-g-force", StepKind::Action(|store, _| show_editor(store, "g_force", "g_force", serde_json::json!({})))));
    tour.push(("editor-backdrop", StepKind::Action(|store, _| show_editor(store, "backdrop", "rectangle", serde_json::json!({"edge_strips": ["left"], "round_top_left": true})))));
    tour.push(("editor-rolling-chart", StepKind::Action(|store, _| show_editor(store, "rolling_chart", "", serde_json::json!({})))));
    tour.push(("editor-label", StepKind::Action(|store, _| show_editor(store, "label", "", serde_json::json!({"text": "Hello"})))));
    tour.push(("editor-time", StepKind::Action(|store, _| show_editor(store, "time", "", serde_json::json!({})))));
    tour.push(("editor-elevation", StepKind::Action(|store, _| show_editor(store, "elevation", "", serde_json::json!({})))));
    tour.push(("editor-heart-zones", StepKind::Action(|store, _| show_editor(store, "heartrate", "heart_zones", serde_json::json!({"zone_calculation": "manual"})))));
    // Maps (phase 5 step 9): a chase camera over the imported ride, then a top-down circle in its
    // view mode (the dashed outline, the hint and Reset).
    tour.push(("editor-map", StepKind::Action(|store, _| show_editor(store, "map", "", serde_json::json!({})))));
    tour.push((
        "editor-map-view",
        StepKind::Action(|store, _| {
            show_editor(store, "map", "", serde_json::json!({"camera_mode": "fixed", "shape": "circle", "width": 350, "height": 350}));
            store.editor.set_map_view_widget_id(Some("editor"));
        }),
    ));
    tour.push((
        "agent-connected",
        StepKind::Action(|store, _| {
            store.agent_session.allow_agent_edits = true;
            store.agent_session.session_opened();
            store.editor_shell.set_settings_section(ovrley_app::features::app_shell::SettingsSection::Settings);
            store.editor_shell.open_settings();
        }),
    ));
    tour.push((
        "agent-connected-closed",
        StepKind::Action(|store, _| store.editor_shell.close_settings()),
    ));
    tour.push(("fullscreen", StepKind::Fullscreen));
    tour.push(("fullscreen-left", StepKind::Fullscreen));
    tour
}

/// The update the dialog's captures offer. It is never downloaded.
#[derive(Debug)]
struct SnapshotUpdate;

impl ovrley_app::store::services::AppUpdate for SnapshotUpdate {
    fn version(&self) -> &str {
        "9.8.7"
    }

    fn download(&self, _on_progress: &dyn Fn(ovrley_services::updater::DownloadProgress)) -> Result<Vec<u8>, String> {
        Err("the snapshot tour does not update".into())
    }

    fn install(&self, _package: Vec<u8>) -> Result<(), String> {
        Err("the snapshot tour does not update".into())
    }
}

/// Videos of the given display size and offset, on the timeline without files: the render dialog
/// shows them; their inspection fails, as for a moved file.
fn snapshot_videos(store: &mut Store, videos: &[(u32, u32, f64)]) {
    use ovrley_core::media::video_edits::VideoEdits;
    let mut imported = Vec::new();
    let mut edits = std::collections::BTreeMap::new();
    for (index, (width, height, offset)) in videos.iter().enumerate() {
        let metadata = ovrley_core::media::SourceVideoMetadata {
            path: format!("/snapshot/clip-{index}.mp4"),
            duration: Some(60.0),
            fps_num: Some(30),
            fps_den: Some(1),
            resolution: Some(ovrley_core::media::Resolution { width: (*width).into(), height: (*height).into() }),
            ..Default::default()
        };
        let video = ovrley_app::lib::video_program::create_imported_video(&metadata, format!("clip-{index}"), index as u32).expect("snapshot videos are valid");
        edits.insert(video.id.clone(), VideoEdits { offset_seconds: *offset, ..VideoEdits::default() });
        imported.push(video);
    }
    store.video_import.video_edits = std::rc::Rc::new(edits);
    store.video_import.imported_videos = std::rc::Rc::new(imported);
}

/// Replaces the document with one `kind` widget in `display`, its variant patched with `variant`,
/// and selects it, so the capture shows its editor.
fn show_editor(store: &mut Store, kind: &str, display: &str, variant: serde_json::Value) {
    use ovrley_widgets::model::catalog::catalog;
    use ovrley_widgets::model::document::{create_widget_defaults, merge_widget_data, OverlayConfig, WidgetSelection};
    // An empty display is a type without displays; `variant` then patches the widget itself.
    let (display_type, mode) = if display.is_empty() {
        (None, None)
    } else {
        (Some(display.to_string()), catalog().require_display(display).modes.first().map(|mode| mode.value.clone()))
    };
    let selection = WidgetSelection { kind: kind.into(), display_type, mode, ..WidgetSelection::default() };
    let data = create_widget_defaults(&selection, &store.template.global_defaults, &Default::default()).expect("catalog defaults");
    let patch = if display.is_empty() { variant } else { serde_json::json!({"display_variants": {display: variant}}) };
    let mut data = merge_widget_data(Some(&data), patch.as_object().expect("a patch"));
    data.insert("id".into(), "editor".into());
    let collection = catalog().require_widget(kind).expect("a catalog widget").collection.clone();
    let config = OverlayConfig::from_value(&serde_json::json!({"scene": store.editor.config.scene, collection: [data]})).expect("a valid document");
    store.editor.set_config(config);
    store.editor.set_selected_widget_id(Some("editor"));
}

/// Starts the tour when `OVRLEY_SNAPSHOT` is set.
pub fn start_if_requested(window: &AppWindow, dispatcher: &Dispatcher) {
    let Some(dir) = std::env::var_os("OVRLEY_SNAPSHOT").map(PathBuf::from) else {
        return;
    };
    if let Some((width, height)) = std::env::var("OVRLEY_SNAPSHOT_SIZE")
        .ok()
        .and_then(|size| size.split_once('x').map(|(w, h)| (w.parse().expect("width"), h.parse().expect("height"))))
    {
        window.window().set_size(slint::LogicalSize::new(width, height));
    }
    std::fs::create_dir_all(&dir).expect("create the snapshot directory");
    let steps = tour();
    take(window.as_weak(), dispatcher.clone(), dir, steps, 0);
}

fn take(window: slint::Weak<AppWindow>, dispatcher: Dispatcher, dir: PathBuf, steps: Vec<Step>, index: usize) {
    let Some((name, action)) = steps.get(index).copied() else {
        slint::quit_event_loop().expect("quit");
        return;
    };
    let settle = match action {
        StepKind::Action(action) => {
            dispatcher.dispatch(action);
            // An editor capture waits for the accordion item to grow to its new content: its
            // height animation advances slowly while the window only renders snapshots.
            if name.starts_with("editor-") || name.starts_with("sync-") {
                3000
            } else {
                800
            }
        }
        StepKind::WidgetsTab => {
            let app = window.upgrade().expect("the window is open");
            let size = app.window().size().to_logical(app.window().scale_factor());
            // The window's rem (`FluidWindow`), the control panel's 26.5rem and its tab row below the header.
            let rem = (size.width * 0.00584 + 5.5).clamp(9.0, 14.6);
            let position = slint::LogicalPosition::new(size.width - 26.5 * rem / 4.0, 4.75 * rem);
            let button = slint::platform::PointerEventButton::Left;
            app.window().dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
            app.window().dispatch_event(slint::platform::WindowEvent::PointerPressed { position, button });
            app.window().dispatch_event(slint::platform::WindowEvent::PointerReleased { position, button });
            800
        }
        StepKind::Fullscreen => {
            let app = window.upgrade().expect("the window is open");
            let fullscreen = !app.window().is_fullscreen();
            log::debug!("snapshot: set_fullscreen({fullscreen})");
            app.window().set_fullscreen(fullscreen);
            2000
        }
    };
    // Let animations and the layout settle before the capture.
    slint::Timer::single_shot(Duration::from_millis(settle), move || {
        let app = window.upgrade().expect("the window is open");
        let pixels = app.window().take_snapshot().expect("snapshot");
        let path = dir.join(format!("shell-{index}-{name}.png"));
        image::save_buffer(&path, pixels.as_bytes(), pixels.width(), pixels.height(), image::ColorType::Rgba8).expect("save png");
        println!("{}", path.display());
        take(window, dispatcher, dir, steps, index + 1);
    });
}
