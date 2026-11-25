//! Regressions in the shared components: tooltips, gradient stops and other review findings.

mod common;

use common::*;
use ovrley_ui::{GradientStop, TooltipLayer};
use slint::platform::WindowEvent;
use slint::{ComponentHandle, LogicalPosition, Model, ModelRc, VecModel};

fn harness() -> ovrley_ui::RegressionHarness {
    let window = ovrley_ui::RegressionHarness::new().unwrap();
    ovrley_ui::install_component_logic(&window);
    let stops = vec![GradientStop { offset: 0.0, color: "#000000".into() }, GradientStop { offset: 1.0, color: "#ffffff".into() }];
    window.set_stops(ModelRc::new(VecModel::from(stops)));
    window.show().unwrap();
    settle();
    window
}

fn wait(ms: u64) {
    i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(ms));
}

fn tooltip_shown(window: &ovrley_ui::RegressionHarness) -> bool {
    window.global::<TooltipLayer>().get_shown()
}

/// The tooltip stays while the pointer moves within its trigger, and does not block other controls.
fn tooltip_stays_while_hovering_and_takes_no_input() {
    let window = harness();
    let trigger = element(&window, "Hover me");
    let center = element_center(&trigger);
    move_to(&window, center);
    wait(450);
    assert!(tooltip_shown(&window), "tooltip did not show after the delay");
    for dx in [-6.0, -3.0, 0.0, 3.0, 6.0] {
        move_to(&window, LogicalPosition::new(center.x + dx, center.y));
        wait(20);
        assert!(tooltip_shown(&window), "tooltip hid while the pointer moved inside the trigger");
    }
    let other = element(&window, "Other");
    click(&window, element_center(&other));
    assert_eq!(window.get_other_clicks(), 1, "another control did not take a click while the tooltip showed");
    wait(20);
    assert!(!tooltip_shown(&window), "tooltip stayed after the pointer left");
}

/// After a stop is added (with the track's hint tooltip showing), the window still takes input.
fn window_responds_after_adding_a_stop() {
    let window = harness();
    let track = element(&window, "Gradient stops");
    let position = track.absolute_position();
    let size = track.size();
    let point = LogicalPosition::new(position.x + size.width * 0.25, position.y + size.height / 2.0);
    move_to(&window, point);
    wait(450);
    press(&window, point);
    release(&window, point);
    assert_eq!(window.get_stops().row_count(), 3);
    let other = element(&window, "Other");
    click(&window, element_center(&other));
    assert_eq!(window.get_other_clicks(), 1, "the window stopped taking input after a stop was added");
}

/// A stop's picker stays open while its colour is edited, even though each edit hands over new stops.
fn stop_picker_stays_open_while_editing() {
    let window = harness();
    let node = element(&window, "Color stop at 0%");
    click(&window, element_center(&node));
    settle();
    // Positions inside popups are reported relative to the popup, so the preset is pressed
    // through its accessible action.
    element(&window, "Use color #40e0d0").invoke_accessible_default_action();
    settle();
    assert_eq!(window.get_stops().row_data(0).unwrap().color.as_str(), "#40e0d0");
    assert!(exists(&window, "Remove stop"), "the stop's picker closed after a colour change");
}

/// A number field rejects characters `<input type="number">` does not accept.
fn number_input_rejects_letters() {
    let window = harness();
    let input = element(&window, "Num");
    click(&window, element_center(&input));
    type_text(&window, "a#");
    settle();
    assert_eq!(input.accessible_value().as_deref(), Some("30"));
    type_text(&window, "5");
    settle();
    assert_eq!(input.accessible_value().as_deref(), Some("305"));
}

/// An upstream change reaches a BlurInput the user is not editing, also right after the user
/// committed an edit by clicking the control that makes the change.
fn blur_input_adopts_upstream_values() {
    let window = harness();
    let input = element(&window, "Num");
    click(&window, element_center(&element(&window, "Set 60")));
    settle();
    assert_eq!(input.accessible_value().as_deref(), Some("60"), "untouched field");

    click(&window, element_center(&input));
    type_text(&window, "1");
    settle();
    click(&window, element_center(&element(&window, "Set 60")));
    settle();
    assert_eq!(window.get_number().as_str(), "60");
    assert_eq!(input.accessible_value().as_deref(), Some("60"), "after committing an edit by clicking the button");
}

/// The hex picker's trigger: the hex field commits a valid typed or pasted value on Enter
/// (normalized) and reverts anything else; the swatch button opens the picker.
fn hex_field_commits_valid_values_and_swatch_opens_picker() {
    let window = harness();
    let field = element(&window, "Hex color value");
    click(&window, element_center(&field));
    for _ in 0..7 {
        special_key(&window, slint::platform::Key::Backspace);
    }
    type_text(&window, "abc");
    special_key(&window, slint::platform::Key::Return);
    settle();
    assert_eq!(window.get_hex().as_str(), "#aabbcc");
    assert_eq!(field.accessible_value().as_deref(), Some("#AABBCC"));

    click(&window, element_center(&field));
    type_text(&window, "zz");
    special_key(&window, slint::platform::Key::Return);
    settle();
    assert_eq!(window.get_hex_changes(), 1, "invalid text committed");
    assert_eq!(field.accessible_value().as_deref(), Some("#AABBCC"), "invalid text did not revert");

    // A paste replaces the focused field's text.
    click(&window, element_center(&field));
    field.set_accessible_value("#FF8800");
    special_key(&window, slint::platform::Key::Return);
    settle();
    assert_eq!(window.get_hex().as_str(), "#ff8800", "pasted value not committed");

    element(&window, "Current color: #ff8800").invoke_accessible_default_action();
    settle();
    assert!(exists(&window, "Use color #ffffff"), "the swatch button did not open the picker");
}

/// Scroll areas scroll with the wheel, but not by dragging the content with the mouse.
fn scroll_area_scrolls_by_wheel_not_drag() {
    let window = harness();
    let area = element(&window, "Scroll area");
    let center = element_center(&area);
    press(&window, center);
    move_to(&window, LogicalPosition::new(center.x - 100.0, center.y - 100.0));
    release(&window, LogicalPosition::new(center.x - 100.0, center.y - 100.0));
    assert_eq!(window.get_scroll_y(), 0.0, "dragging with the mouse scrolled the area");
    move_to(&window, center);
    window.window().dispatch_event(WindowEvent::PointerScrolled { position: center, delta_x: 0.0, delta_y: -60.0 });
    settle();
    assert!(window.get_scroll_y() < 0.0, "the wheel did not scroll the area");
}

fn exists(window: &impl ComponentHandle, label: &str) -> bool {
    i_slint_backend_testing::ElementHandle::find_by_accessible_label(window, label).next().is_some()
}

#[test]
fn cases() {
    run_cases(&[
        ("tooltip_stays_while_hovering_and_takes_no_input", tooltip_stays_while_hovering_and_takes_no_input),
        ("window_responds_after_adding_a_stop", window_responds_after_adding_a_stop),
        ("stop_picker_stays_open_while_editing", stop_picker_stays_open_while_editing),
        ("number_input_rejects_letters", number_input_rejects_letters),
        ("scroll_area_scrolls_by_wheel_not_drag", scroll_area_scrolls_by_wheel_not_drag),
        ("blur_input_adopts_upstream_values", blur_input_adopts_upstream_values),
        ("hex_field_commits_valid_values_and_swatch_opens_picker", hex_field_commits_valid_values_and_swatch_opens_picker),
    ]);
}
