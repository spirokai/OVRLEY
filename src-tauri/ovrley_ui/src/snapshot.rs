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
    use ovrley_app::i18n::{change_language_preference, Locale};
    use ovrley_app::store::slices::layout::LeftDrawerTool;
    let steps: Vec<(&'static str, StepAction)> = vec![
        ("shell", |store, _| {
            store.layout.set_left_drawer_pinned(false);
            store.layout.dismiss_left_drawer_overlay();
        }),
        ("drawer-overlay", |store, _| store.layout.select_left_drawer_tool(LeftDrawerTool::Activity)),
        ("drawer-pinned", |store, _| store.layout.set_left_drawer_pinned(true)),
        ("activity-imported", |store, services| {
            let ride = ovrley_services::runtime_paths::source_repo_root().join("src-tauri/ovrley_core/tests/fixtures/activity/test-ride.fit");
            ovrley_app::features::app_shell::import_activity(store, services, ride.to_string_lossy().into_owned(), None);
        }),
        ("activity-trimmed", |store, _| {
            let duration = store.media.activity_summary.as_ref().map_or(0.0, |summary| summary.duration_seconds);
            store.media.set_activity_trim(Some(ovrley_app::lib::activity::ActivityTrim { start_second: duration * 0.1, end_second: duration * 0.8 }));
            store.editor.set_selected_second(duration * 0.4);
            store.player.hovered_lane = Some(ovrley_app::store::slices::video_import::ClipLane::Activity);
        }),
        // With `OVRLEY_DEV_VIDEOS` (two or more clips): the first join, with a fade, and its panel.
        ("join-panel", |store, _| {
            store.player.hovered_lane = None;
            let first = store.player.timeline.timeline().displayed.program.joins.first().map(|join| join.key.clone());
            if let Some(key) = first {
                ovrley_app::features::transitions::select_join_transition(store, &key, "fade").expect("a join takes a fade");
                ovrley_app::features::transitions::open_join_panel(store, &key);
            }
        }),
        ("widget-selected", |store, _| {
            store.player.join_panel = None;
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
        ("settings-general", |store, services| {
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
            if name.starts_with("editor-") {
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
