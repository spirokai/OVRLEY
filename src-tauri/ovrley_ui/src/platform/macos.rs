//! macOS window chrome: the traffic lights at the Tauri config's `trafficLightPosition` (x 20, y
//! 28), as the Tauri app's `traffic_lights.rs` places them, and the full-screen transitions.
//!
//! AppKit puts the buttons back in the window corner on resizes, and lays the title bar out again
//! when a full-screen transition ends, after the last resize. winit only learns that the window
//! left full screen at that point too. So the app follows AppKit's own full-screen notifications:
//! the header's inset changes when a transition starts, and the inset of the buttons is applied
//! again when one ends.

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{
    NSView, NSWindow, NSWindowButton, NSWindowDidEnterFullScreenNotification, NSWindowDidExitFullScreenNotification,
    NSWindowWillEnterFullScreenNotification, NSWindowWillExitFullScreenNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter};
use std::cell::RefCell;
use std::ptr::NonNull;
use slint::winit_030::winit;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// `trafficLightPosition` in `tauri.macos.conf.json`.
const TRAFFIC_LIGHT_X: f64 = 20.0;
const TRAFFIC_LIGHT_Y: f64 = 28.0;

fn ns_window(window: &winit::window::Window) -> Option<Retained<NSWindow>> {
    let handle = match window.window_handle() {
        Ok(handle) => handle,
        Err(error) => {
            log::warn!("no window handle for the macOS window chrome: {error}");
            return None;
        }
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };
    // SAFETY: winit hands out the live NSView of its window, on the main thread.
    let ns_view: &NSView = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    ns_view.window()
}

/// Moves the traffic lights to their configured position. Must run on the main thread.
pub fn apply_traffic_light_inset(window: &winit::window::Window) {
    if let Some(ns_window) = ns_window(window) {
        inset(&ns_window, TRAFFIC_LIGHT_X, TRAFFIC_LIGHT_Y);
    }
}

/// A full-screen transition of the window, as AppKit announces it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FullscreenTransition {
    WillEnter,
    DidEnter,
    WillExit,
    DidExit,
}

thread_local! {
    /// The notification observers, kept for the app's lifetime.
    static OBSERVERS: RefCell<Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>> = const { RefCell::new(Vec::new()) };
}

/// Calls `on_transition` for each full-screen transition of `window`, on the main thread, and
/// re-applies the traffic-light inset as each one starts and ends.
pub fn observe_fullscreen(window: &winit::window::Window, on_transition: impl Fn(FullscreenTransition) + 'static) {
    let Some(ns_window) = ns_window(window) else { return };
    let on_transition = std::rc::Rc::new(on_transition);
    let center = NSNotificationCenter::defaultCenter();
    // SAFETY: AppKit's notification names are immutable statics.
    let names = unsafe {
        [
            (NSWindowWillEnterFullScreenNotification, FullscreenTransition::WillEnter),
            (NSWindowDidEnterFullScreenNotification, FullscreenTransition::DidEnter),
            (NSWindowWillExitFullScreenNotification, FullscreenTransition::WillExit),
            (NSWindowDidExitFullScreenNotification, FullscreenTransition::DidExit),
        ]
    };
    for (name, transition) in names {
        let on_transition = on_transition.clone();
        let target = ns_window.clone();
        let block = RcBlock::new(move |_: NonNull<NSNotification>| {
            if matches!(transition, FullscreenTransition::WillExit | FullscreenTransition::DidExit) {
                inset(&target, TRAFFIC_LIGHT_X, TRAFFIC_LIGHT_Y);
            }
            on_transition(transition);
        });
        let object: &AnyObject = &ns_window;
        // SAFETY: with no queue the block runs on the posting thread; AppKit posts window
        // notifications on the main thread, where the app's state lives.
        let observer = unsafe { center.addObserverForName_object_queue_usingBlock(Some(name), Some(object), None, &block) };
        OBSERVERS.with(|observers| observers.borrow_mut().push(observer));
    }
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
