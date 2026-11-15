//! The raw winit events the app needs before Slint routes them, from the custom application
//! handler installed at backend selection (D10, D19; `concat-file-drop.md`). The handler exists
//! before the window and the dispatcher do, so it reaches them through the UI-thread slots that
//! `app.rs` fills.
//!
//! - **Exit:** on macOS, ⌘Q ends the loop inside the system's terminate, and `window.run()` may never
//!   return; the preferences are flushed here, while everything is still alive.
//! - **File drops:** "hovered" and "cancelled" drive the drop overlay; "dropped" events arrive one
//!   per file and are collected, and the batch is handed over when the loop is about to go idle:
//!   one drop, one dispatch, however many files. On Wayland winit sends nothing, so
//!   `wayland_drop` does the same from its own listener.
//! - **Press-away blur:** a left press more than 8px outside the focused text field's rectangle
//!   asks the field to give up its focus, before Slint routes the press.

use super::{dispatch_ui, APP_WINDOW, UI_DISPATCH};
use crate::FocusedField;
use ovrley_app::features::app_shell::{self, DroppedPath};
use slint::winit_030::{winit, CustomApplicationHandler, EventResult};
use slint::ComponentHandle;
use std::path::PathBuf;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;

/// How far outside the focused field a press still keeps the focus (the caret placement margin).
const PRESS_MARGIN: f32 = 8.0;

/// Whether a press at `point` lands more than [`PRESS_MARGIN`] outside `field` (x, y, width,
/// height), all in logical pixels.
pub fn press_is_away(field: (f32, f32, f32, f32), point: (f32, f32)) -> bool {
    let (x, y, width, height) = field;
    point.0 < x - PRESS_MARGIN || point.0 > x + width + PRESS_MARGIN || point.1 < y - PRESS_MARGIN || point.1 > y + height + PRESS_MARGIN
}

#[derive(Default)]
pub struct AppEvents {
    pending_drop: Vec<PathBuf>,
    /// The cursor in physical pixels, from the last move.
    cursor: Option<winit::dpi::PhysicalPosition<f64>>,
}

#[cfg(target_os = "linux")]
thread_local! {
    static WAYLAND_DROP: std::cell::RefCell<Option<super::wayland_drop::WaylandDropListener>> = const { std::cell::RefCell::new(None) };
}

/// Starts the Wayland drop listener when the window is on Wayland, where winit reports no drops.
/// Called on the UI thread once the winit window exists.
#[cfg(target_os = "linux")]
pub fn start_wayland_drop(winit_window: &winit::window::Window) {
    use super::wayland_drop::{self, DropEvent};
    use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
    let Ok(display) = winit_window.display_handle() else { return };
    let RawDisplayHandle::Wayland(wayland) = display.as_raw() else { return };
    let report = |event: DropEvent| {
        let posted = slint::invoke_from_event_loop(move || match event {
            DropEvent::Hovering(hovering) => set_hovering(hovering),
            DropEvent::Dropped(paths) => deliver_drop(paths),
        });
        if let Err(error) = posted {
            log::debug!("dropped a Wayland drop event after the event loop ended: {error}");
        }
    };
    // SAFETY: winit's display stays alive for the event loop's lifetime; the listener is stopped
    // when the loop exits.
    match unsafe { wayland_drop::start(wayland.display.as_ptr(), report) } {
        Ok(listener) => WAYLAND_DROP.with(|slot| *slot.borrow_mut() = Some(listener)),
        Err(error) => log::warn!("file drops are unavailable on this Wayland session: {error}"),
    }
}

/// Hands the collected paths over as one drop.
pub(crate) fn deliver_drop(paths: Vec<PathBuf>) {
    let dropped: Vec<DroppedPath> = paths
        .into_iter()
        .map(|path| DroppedPath {
            is_dir: path.is_dir(),
            path,
        })
        .collect();
    dispatch_ui(move |store, _| app_shell::drop_files(store, &dropped));
}

pub(crate) fn set_hovering(hovering: bool) {
    dispatch_ui(move |store, _| app_shell::set_file_drag_hovering(store, hovering));
}

fn blur_if_pressed_away(scale_factor: f32, cursor: winit::dpi::PhysicalPosition<f64>) {
    APP_WINDOW.with(|slot| {
        let Some(window) = slot.borrow().as_ref().and_then(|window| window.upgrade()) else { return };
        let field = window.global::<FocusedField>();
        if !field.get_active() {
            return;
        }
        let point = (cursor.x as f32 / scale_factor, cursor.y as f32 / scale_factor);
        if press_is_away((field.get_x(), field.get_y(), field.get_width(), field.get_height()), point) {
            field.set_blur_serial(field.get_blur_serial().wrapping_add(1));
        }
    });
}

impl CustomApplicationHandler for AppEvents {
    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        _winit_window: Option<&winit::window::Window>,
        slint_window: Option<&slint::Window>,
        event: &WindowEvent,
    ) -> EventResult {
        match event {
            WindowEvent::HoveredFile(_) => set_hovering(true),
            WindowEvent::HoveredFileCancelled => set_hovering(false),
            WindowEvent::DroppedFile(path) => self.pending_drop.push(path.clone()),
            WindowEvent::CursorMoved { position, .. } => self.cursor = Some(*position),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if let (Some(window), Some(cursor)) = (slint_window, self.cursor) {
                    blur_if_pressed_away(window.scale_factor(), cursor);
                }
            }
            _ => {}
        }
        EventResult::Propagate
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) -> EventResult {
        if !self.pending_drop.is_empty() {
            deliver_drop(std::mem::take(&mut self.pending_drop));
        }
        EventResult::Propagate
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) -> EventResult {
        #[cfg(target_os = "linux")]
        WAYLAND_DROP.with(|slot| slot.borrow_mut().take());
        UI_DISPATCH.with(|slot| {
            if let Some(dispatch) = slot.borrow().as_ref() {
                dispatch.flush_preferences();
            }
        });
        EventResult::Propagate
    }
}

/// Decodes a `text/uri-list` body into local paths. Comment lines and blank lines are skipped;
/// only `file:` URIs on this machine (`file:///path` or `file://localhost/path`) become paths;
/// percent escapes are decoded, and an entry with a malformed escape is skipped.
pub fn decode_uri_list(body: &str) -> Vec<PathBuf> {
    body.lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|uri| {
            let rest = uri.strip_prefix("file://")?;
            let path = if rest.starts_with('/') {
                rest
            } else {
                let (host, _) = rest.split_once('/')?;
                if !host.eq_ignore_ascii_case("localhost") {
                    log::debug!("skipped a dropped URI on another host: {uri}");
                    return None;
                }
                &rest[host.len()..]
            };
            match percent_decode(path) {
                Some(bytes) => Some(path_from_bytes(bytes)),
                None => {
                    log::warn!("skipped a dropped URI with a malformed escape: {uri}");
                    None
                }
            }
        })
        .collect()
}

fn percent_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            let value = u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
            out.push(value);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    Some(out)
}

#[cfg(unix)]
fn path_from_bytes(bytes: Vec<u8>) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(std::ffi::OsString::from_vec(bytes))
}

#[cfg(not(unix))]
fn path_from_bytes(bytes: Vec<u8>) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_within_8px_of_the_field_keeps_the_focus() {
        let field = (100.0, 50.0, 200.0, 30.0);
        assert!(!press_is_away(field, (150.0, 60.0)), "inside");
        assert!(!press_is_away(field, (93.0, 45.0)), "within the margin");
        assert!(press_is_away(field, (91.0, 60.0)), "left of the margin");
        assert!(press_is_away(field, (150.0, 89.0)), "below the margin");
    }

    #[test]
    fn decodes_local_file_uris() {
        let body = "# a comment\r\nfile:///home/rider/ride%201.fit\r\nfile://localhost/tmp/clip.mp4\r\n\r\n";
        assert_eq!(decode_uri_list(body), [PathBuf::from("/home/rider/ride 1.fit"), PathBuf::from("/tmp/clip.mp4")]);
    }

    #[test]
    fn skips_other_schemes_hosts_and_malformed_escapes() {
        let body = "https://example.com/a.fit\nfile://nas/share/b.fit\nfile:///c%2.fit\nfile:///d%zz.fit\nfile:///ok%C3%A9.fit";
        assert_eq!(decode_uri_list(body), [PathBuf::from("/oké.fit")]);
    }
}
