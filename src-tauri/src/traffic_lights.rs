//! Keeps the macOS traffic lights at the window's configured `trafficLightPosition`.
//!
//! Tauri only re-applies the inset when its webview parent view redraws. When the
//! window leaves fullscreen, AppKit puts the buttons back in the window corner without
//! triggering that redraw, so the inset is re-applied on every resize (leaving
//! fullscreen always resizes the window).

use objc2_app_kit::{NSView, NSWindow, NSWindowButton};
use tauri::{Manager, WebviewWindow, WindowEvent};

/// Re-applies the configured traffic light inset after every resize of `window`.
/// Does nothing when the window config has no `trafficLightPosition`.
pub fn keep_inset(window: &WebviewWindow) {
    let Some(position) = window
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == window.label())
        .and_then(|config| config.traffic_light_position.as_ref())
    else {
        return;
    };
    let (x, y) = (position.x, position.y);

    let handle = window.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Resized(_)) {
            // Window events are delivered on the main thread.
            match handle.ns_window() {
                Ok(ns_window) => apply_inset(ns_window.cast(), x, y),
                Err(error) => log::warn!("failed to get NSWindow for traffic light inset: {error}"),
            }
        }
    });
}

/// Mirrors wry's `inset_traffic_lights`, which Tauri uses for the initial placement.
fn apply_inset(ns_window: *mut NSWindow, x: f64, y: f64) {
    // SAFETY: callers pass the live NSWindow of the app window, on the main thread.
    let Some(ns_window) = (unsafe { ns_window.as_ref() }) else {
        return;
    };
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
    let buttons = [Some(close), Some(miniaturize), zoom];
    for (index, button) in buttons.into_iter().flatten().enumerate() {
        let mut rect = NSView::frame(&button);
        rect.origin.x = x + (index as f64 * space_between);
        button.setFrameOrigin(rect.origin);
    }
}
