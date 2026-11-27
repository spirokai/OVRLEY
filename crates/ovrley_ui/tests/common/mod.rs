//! Helpers for the component tests: the testing backend, keyboard input and pointer input at
//! window coordinates.

#![allow(dead_code)]

use i_slint_backend_testing::ElementHandle;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, SharedString};

/// Initializes the testing backend. Slint's platform is process-wide and bound to the thread
/// that set it, while libtest runs each `#[test]` on its own thread, so each test file has one
/// `#[test]` that runs its cases in order through `run_cases`.
pub fn init() {
    i_slint_backend_testing::init_integration_test_with_mock_time();
}

/// Runs named cases in order, each against fresh windows, naming the case that fails.
pub fn run_cases(cases: &[(&str, fn())]) {
    init();
    for (name, case) in cases {
        if let Err(panic) = std::panic::catch_unwind(case) {
            eprintln!("case failed: {name}");
            std::panic::resume_unwind(panic);
        }
    }
}

/// The first element with this accessible label; panics when there is none.
pub fn element(window: &impl ComponentHandle, label: &str) -> ElementHandle {
    ElementHandle::find_by_accessible_label(window, label)
        .next()
        .unwrap_or_else(|| panic!("no element labelled {label:?}"))
}

pub fn element_center(element: &ElementHandle) -> LogicalPosition {
    let position = element.absolute_position();
    let size = element.size();
    LogicalPosition::new(
        position.x + size.width / 2.0,
        position.y + size.height / 2.0,
    )
}

pub fn key(window: &impl ComponentHandle, text: impl Into<SharedString>) {
    let text = text.into();
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

pub fn special_key(window: &impl ComponentHandle, key: Key) {
    self::key(window, SharedString::from(key));
}

pub fn type_text(window: &impl ComponentHandle, text: &str) {
    for c in text.chars() {
        key(window, SharedString::from(c.to_string()));
    }
}

pub fn press(window: &impl ComponentHandle, position: LogicalPosition) {
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
}

pub fn move_to(window: &impl ComponentHandle, position: LogicalPosition) {
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
}

pub fn release(window: &impl ComponentHandle, position: LogicalPosition) {
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
}

pub fn click(window: &impl ComponentHandle, position: LogicalPosition) {
    press(window, position);
    release(window, position);
}

/// Lets timers and change callbacks run.
pub fn settle() {
    i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(50));
}

/// Advances the mocked clock by `ms`.
pub fn wait(ms: u64) {
    i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(ms));
}
