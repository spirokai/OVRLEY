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

/// One state of the tour: its name and the action that sets it up.
type Step = (&'static str, fn(&mut Store, &mut Services));

fn tour() -> Vec<Step> {
    vec![("startup", |_, _| {})]
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
    dispatcher.dispatch(action);
    // Let animations and the layout settle before the capture.
    slint::Timer::single_shot(Duration::from_millis(800), move || {
        let app = window.upgrade().expect("the window is open");
        let pixels = app.window().take_snapshot().expect("snapshot");
        let path = dir.join(format!("shell-{index}-{name}.png"));
        image::save_buffer(&path, pixels.as_bytes(), pixels.width(), pixels.height(), image::ColorType::Rgba8).expect("save png");
        println!("{}", path.display());
        take(window, dispatcher, dir, steps, index + 1);
    });
}
