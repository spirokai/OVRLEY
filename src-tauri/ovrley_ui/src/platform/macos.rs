//! Keeps the macOS traffic lights at the Tauri config's `trafficLightPosition` (x 20, y 28), as the
//! Tauri app's `traffic_lights.rs` does. AppKit puts the buttons back in the window corner on
//! resizes, leaving fullscreen included, so the inset is re-applied after every resize.

use objc2_app_kit::{NSView, NSWindow, NSWindowButton};
use slint::winit_030::winit;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// `trafficLightPosition` in `tauri.macos.conf.json`.
const TRAFFIC_LIGHT_X: f64 = 20.0;
const TRAFFIC_LIGHT_Y: f64 = 28.0;

/// Moves the traffic lights to their configured position. Must run on the main thread.
pub fn apply_traffic_light_inset(window: &winit::window::Window) {
    let handle = match window.window_handle() {
        Ok(handle) => handle,
        Err(error) => {
            log::warn!("no window handle for the traffic light inset: {error}");
            return;
        }
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    // SAFETY: winit hands out the live NSView of its window, on the main thread.
    let ns_view: &NSView = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    let Some(ns_window) = ns_view.window() else {
        return;
    };
    inset(&ns_window, TRAFFIC_LIGHT_X, TRAFFIC_LIGHT_Y);
}

/// Mirrors wry's `inset_traffic_lights`, as the Tauri app does.
fn inset(ns_window: &NSWindow, x: f64, y: f64) {
    let (Some(close), Some(miniaturize)) = (
        ns_window.standardWindowButton(NSWindowButton::CloseButton),
        ns_window.standardWindowButton(NSWindowButton::MiniaturizeButton),
    ) else {
        return;
    };
    let zoom = ns_window.standardWindowButton(NSWindowButton::ZoomButton);
    // SAFETY: the window buttons are live views owned by the window's title bar.
    let Some(title_bar_container_view) =
        (unsafe { close.superview().and_then(|view| view.superview()) })
    else {
        return;
    };

    let close_rect = NSView::frame(&close);
    let title_bar_frame_height = close_rect.size.height + y;
    let mut title_bar_rect = NSView::frame(&title_bar_container_view);
    title_bar_rect.size.height = title_bar_frame_height;
    title_bar_rect.origin.y = ns_window.frame().size.height - title_bar_frame_height;
    title_bar_container_view.setFrame(title_bar_rect);

    let space_between = NSView::frame(&miniaturize).origin.x - close_rect.origin.x;
    for (index, button) in [Some(close), Some(miniaturize), zoom]
        .into_iter()
        .flatten()
        .enumerate()
    {
        let mut rect = NSView::frame(&button);
        rect.origin.x = x + (index as f64 * space_between);
        button.setFrameOrigin(rect.origin);
    }
}
