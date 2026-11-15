//! The press-away blur (D19): a press away from the focused text field blurs it and commits. The
//! testing backend has no winit events, so `press` runs the platform handler's check first, as the
//! application handler does before Slint routes a press.

mod common;

use common::*;
use ovrley_ui::platform::window_events::press_is_away;
use ovrley_ui::FocusedField;
use slint::platform::Key;
use slint::{ComponentHandle, LogicalPosition};

fn harness() -> ovrley_ui::BlurInputHarness {
    let window = ovrley_ui::BlurInputHarness::new().unwrap();
    ovrley_ui::install_component_logic(&window);
    window.show().unwrap();
    window
}

/// The handler's check, then the press itself.
fn press_at(window: &ovrley_ui::BlurInputHarness, position: LogicalPosition) {
    let field = window.global::<FocusedField>();
    if field.get_active() && press_is_away((field.get_x(), field.get_y(), field.get_width(), field.get_height()), (position.x, position.y)) {
        field.set_blur_serial(field.get_blur_serial() + 1);
    }
    click(window, position);
    settle();
}

fn edit_fps(window: &ovrley_ui::BlurInputHarness) {
    click(window, element_center(&element(window, "FPS")));
    special_key(window, Key::Backspace);
    special_key(window, Key::Backspace);
    type_text(window, "25");
    settle();
    assert!(window.global::<FocusedField>().get_active());
}

fn a_press_outside_blurs_and_commits() {
    let window = harness();
    edit_fps(&window);
    press_at(&window, LogicalPosition::new(900.0, 600.0));
    assert!(!window.global::<FocusedField>().get_active());
    assert_eq!(window.get_commit_count(), 1);
}

fn a_press_inside_or_within_8px_keeps_the_focus() {
    let window = harness();
    edit_fps(&window);
    let input = element(&window, "FPS");
    press_at(&window, element_center(&input));
    let position = input.absolute_position();
    let size = input.size();
    press_at(&window, LogicalPosition::new(position.x + size.width + 5.0, position.y + size.height / 2.0));
    assert!(window.global::<FocusedField>().get_active());
    assert_eq!(window.get_commit_count(), 0);
}

fn a_press_on_a_second_field_moves_the_focus_and_commits_the_first() {
    let window = harness();
    edit_fps(&window);
    let first_y = window.global::<FocusedField>().get_y();
    let second = element(&window, "Bitrate");
    press_at(&window, element_center(&second));
    assert_eq!(window.get_commit_count(), 1);
    let field = window.global::<FocusedField>();
    assert!(field.get_active());
    assert!(field.get_y() > first_y + 10.0, "the second field, below the first, reports itself");
}

fn a_focused_button_keeps_its_focus() {
    let window = harness();
    click(&window, element_center(&element(&window, "Probe")));
    settle();
    assert!(window.get_button_focused());
    press_at(&window, LogicalPosition::new(900.0, 600.0));
    assert!(window.get_button_focused());
}

#[test]
fn cases() {
    run_cases(&[
        ("a_press_outside_blurs_and_commits", a_press_outside_blurs_and_commits),
        ("a_press_inside_or_within_8px_keeps_the_focus", a_press_inside_or_within_8px_keeps_the_focus),
        ("a_press_on_a_second_field_moves_the_focus_and_commits_the_first", a_press_on_a_second_field_moves_the_focus_and_commits_the_first),
        ("a_focused_button_keeps_its_focus", a_focused_button_keeps_its_focus),
    ]);
}
