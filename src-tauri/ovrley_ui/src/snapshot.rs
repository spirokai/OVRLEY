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

/// One state of the tour: its name and the action that sets it up. A `None` action toggles native
/// full screen instead (macOS: the header's inset follows AppKit's notifications).
type Step = (&'static str, Option<StepAction>);
type StepAction = fn(&mut Store, &mut Services);

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
        ("settings-general", |store, _| {
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
    let mut tour: Vec<Step> = steps.into_iter().map(|(name, action)| (name, Some(action))).collect();
    tour.push(("fullscreen", None));
    tour.push(("fullscreen-left", None));
    tour
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
        Some(action) => {
            dispatcher.dispatch(action);
            800
        }
        None => {
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
