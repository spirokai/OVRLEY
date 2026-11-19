//! macOS window chrome: the traffic lights 20 points from the left, centred on the header's
//! midline, and the full-screen transitions.
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

/// The traffic lights' distance from the window's left edge.
const TRAFFIC_LIGHT_X: f64 = 20.0;

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

/// Moves the traffic lights onto the midline of a header `header_height` points high. Must run on
/// the main thread.
pub fn apply_traffic_light_inset(window: &winit::window::Window, header_height: f64) {
    if let Some(ns_window) = ns_window(window) {
        inset(&ns_window, header_height);
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
/// re-applies the traffic-light inset, on the header `header_height` reports, as an exit starts
/// and ends.
pub fn observe_fullscreen(
    window: &winit::window::Window,
    header_height: impl Fn() -> f64 + 'static,
    on_transition: impl Fn(FullscreenTransition) + 'static,
) {
    let Some(ns_window) = ns_window(window) else { return };
    let on_transition = std::rc::Rc::new(on_transition);
    let header_height = std::rc::Rc::new(header_height);
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
        let header_height = header_height.clone();
        let target = ns_window.clone();
        let block = RcBlock::new(move |_: NonNull<NSNotification>| {
            if matches!(transition, FullscreenTransition::WillExit | FullscreenTransition::DidExit) {
                inset(&target, header_height());
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

/// After wry's `inset_traffic_lights`: the title bar container spans the header, and the buttons
/// sit `TRAFFIC_LIGHT_X` from the left, centred vertically in it.
fn inset(ns_window: &NSWindow, header_height: f64) {
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
    let mut title_bar_rect = NSView::frame(&title_bar_container_view);
    title_bar_rect.size.height = header_height;
    title_bar_rect.origin.y = ns_window.frame().size.height - header_height;
    title_bar_container_view.setFrame(title_bar_rect);
    // The buttons' own view fills the container; centring in it puts them on the header's
    // midline whether or not it is flipped.
    // SAFETY: as above.
    let Some(button_view) = (unsafe { close.superview() }) else {
        return;
    };
    let button_view_height = button_view.bounds().size.height;

    let space_between = NSView::frame(&miniaturize).origin.x - close_rect.origin.x;
    for (index, button) in [Some(close), Some(miniaturize), zoom]
        .into_iter()
        .flatten()
        .enumerate()
    {
        let mut rect = NSView::frame(&button);
        rect.origin.x = TRAFFIC_LIGHT_X + (index as f64 * space_between);
        rect.origin.y = (button_view_height - rect.size.height) / 2.0;
        button.setFrameOrigin(rect.origin);
    }
}
