//! The raw winit events the app needs before Slint routes them, from the custom application
//! handler installed at backend selection. The handler exists before the window and the dispatcher
//! do, so it reaches them through the UI-thread slots that `app.rs` fills.
//!
//! - **Exit:** on macOS, ⌘Q ends the loop inside the system's terminate, and `window.run()` may never
//!   return; the preferences are flushed here, while everything is still alive.
//! - **File drops:** "hovered" and "cancelled" drive the drop overlay; "dropped" events arrive one
//!   per file and are collected, and the batch is handed over when the loop is about to go idle:
//!   one drop, one dispatch, however many files. On Wayland winit sends nothing, so the `wayland`
//!   listener does the same from its own thread.
//! - **Press-away blur:** a left press more than 8px outside the focused text field's rectangle
//!   asks the field to give up its focus, before Slint routes the press.
//! - **Keyboard workspace:** a press in the player selects the `player` workspace, a press in the
//!   editor or the control panel `editor`, before the press reaches any element.

use super::{dispatch_ui, APP_WINDOW, UI_DISPATCH};
use crate::{AppShell, FocusedField, Player};
use ovrley_app::features::shell::{self, DroppedPath};
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
    static WAYLAND_DROP: std::cell::RefCell<Option<super::wayland::WaylandDropListener>> = const { std::cell::RefCell::new(None) };
}

/// Starts the Wayland drop listener when the window is on Wayland, where winit reports no drops.
/// Called on the UI thread once the winit window exists.
#[cfg(target_os = "linux")]
pub fn start_wayland_drop(winit_window: &winit::window::Window) {
    use super::wayland::{self, DropEvent};
    use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
    let Ok(display) = winit_window.display_handle() else { return };
    let RawDisplayHandle::Wayland(handle) = display.as_raw() else { return };
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
    match unsafe { wayland::start(handle.display.as_ptr(), report) } {
        Ok(listener) => WAYLAND_DROP.with(|slot| *slot.borrow_mut() = Some(listener)),
        Err(error) => log::warn!("file drops are unavailable on this Wayland session: {error}"),
    }
}

/// Hands the collected paths over as one drop.
fn deliver_drop(paths: Vec<PathBuf>) {
    let dropped: Vec<DroppedPath> = paths
        .into_iter()
        .map(|path| DroppedPath {
            is_dir: path.is_dir(),
            path,
        })
        .collect();
    dispatch_ui(move |store, services| shell::drop_files(store, services, &dropped));
}

fn set_hovering(hovering: bool) {
    dispatch_ui(move |store, _| shell::set_file_drag_hovering(store, hovering));
}

/// Window chrome, not app state: written to the global directly, like minimise and maximise.
pub(crate) fn set_window_maximized(maximized: bool) {
    APP_WINDOW.with(|slot| {
        let Some(window) = slot.borrow().as_ref().and_then(slint::Weak::upgrade) else { return };
        let shell = window.global::<AppShell>();
        if shell.get_window_maximized() != maximized {
            shell.set_window_maximized(maximized);
        }
    });
}

fn blur_if_pressed_away(scale_factor: f32, cursor: winit::dpi::PhysicalPosition<f64>) {
    APP_WINDOW.with(|slot| {
        let Some(window) = slot.borrow().as_ref().and_then(slint::Weak::upgrade) else { return };
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

fn area(rect: crate::PlacementRect) -> shell::Area {
    shell::Area { x: rect.x.into(), y: rect.y.into(), width: rect.width.into(), height: rect.height.into() }
}

/// The keyboard workspace of a press at `point`, selected before Slint routes the press.
fn select_workspace(scale_factor: f32, cursor: winit::dpi::PhysicalPosition<f64>) {
    let point = (cursor.x / f64::from(scale_factor), cursor.y / f64::from(scale_factor));
    let workspace = APP_WINDOW.with(|slot| {
        let window = slot.borrow().as_ref().and_then(slint::Weak::upgrade)?;
        let shell = window.global::<AppShell>();
        let player_area = window.global::<Player>().get_visible().then(|| area(shell.get_player_area()));
        shell::workspace_at(point, area(shell.get_editor_area()), area(shell.get_control_panel_area()), player_area)
    });
    if let Some(workspace) = workspace {
        dispatch_ui(move |store, _| {
            if store.editor_shell.active_keyboard_workspace != workspace {
                store.editor_shell.set_active_keyboard_workspace(workspace);
            }
        });
    }
}

impl CustomApplicationHandler for AppEvents {
    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        winit_window: Option<&winit::window::Window>,
        slint_window: Option<&slint::Window>,
        event: &WindowEvent,
    ) -> EventResult {
        match event {
            WindowEvent::HoveredFile(_) => set_hovering(true),
            WindowEvent::HoveredFileCancelled => set_hovering(false),
            WindowEvent::DroppedFile(path) => self.pending_drop.push(path.clone()),
            WindowEvent::CursorMoved { position, .. } => self.cursor = Some(*position),
            // Maximising, restoring and fullscreen each resize the window: the frame's corners follow.
            WindowEvent::Resized(_) => {
                if let Some(winit_window) = winit_window {
                    set_window_maximized(winit_window.is_maximized() || winit_window.fullscreen().is_some());
                }
            }
            // Losing the window's focus cancels a running gesture.
            WindowEvent::Focused(false) => dispatch_ui(|store, _| {
                ovrley_app::features::canvas::interaction::cancel_gesture(store);
                ovrley_app::features::player::gestures::surface_cancel(store);
            }),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if let (Some(window), Some(cursor)) = (slint_window, self.cursor) {
                    blur_if_pressed_away(window.scale_factor(), cursor);
                    select_workspace(window.scale_factor(), cursor);
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
}
