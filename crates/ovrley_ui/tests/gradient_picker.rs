//! Port of app/src/tests/components/ui/GradientPicker.test.jsx. The React test fixes the track at
//! 200px; here positions are fractions of the track's measured width.

mod common;

use common::*;
use ovrley_ui::GradientStop;
use slint::platform::Key;
use slint::{ComponentHandle, LogicalPosition, Model, ModelRc, VecModel};

fn stop(offset: f32, color: &str) -> GradientStop {
    GradientStop {
        offset,
        color: color.into(),
    }
}

fn two_stops() -> Vec<GradientStop> {
    vec![stop(0.0, "#000000"), stop(1.0, "#ffffff")]
}

fn harness(stops: Vec<GradientStop>) -> ovrley_ui::GradientPickerHarness {
    let window = ovrley_ui::GradientPickerHarness::new().unwrap();
    ovrley_ui::install_component_logic(&window);
    window.set_stops(ModelRc::new(VecModel::from(stops)));
    window.show().unwrap();
    settle();
    window
}

/// A point on the track at `fraction` of its width.
fn on_track(window: &ovrley_ui::GradientPickerHarness, fraction: f32) -> LogicalPosition {
    let track = element(window, "Gradient stops");
    let position = track.absolute_position();
    let size = track.size();
    LogicalPosition::new(
        position.x + size.width * fraction,
        position.y + size.height / 2.0,
    )
}

/// Focuses a stop node without opening its picker: pressing focuses it, and releasing away from
/// it is not a click. (The React test fires `keyDown` on the node directly.)
fn focus_node(
    window: &ovrley_ui::GradientPickerHarness,
    node: &i_slint_backend_testing::ElementHandle,
) {
    press(window, element_center(node));
    release(window, LogicalPosition::new(1.0, 1.0));
}

fn last_change(window: &ovrley_ui::GradientPickerHarness) -> Vec<GradientStop> {
    window.get_last_change().iter().collect()
}

fn clicking_the_track_adds_an_interpolated_stop() {
    let window = harness(two_stops());

    press(&window, on_track(&window, 0.25));

    let mut expected = two_stops();
    expected.push(stop(0.25, "#404040"));
    assert_eq!(window.get_change_count(), 1);
    assert_eq!(last_change(&window), expected);
}

fn does_not_add_more_than_five_stops() {
    let full = [0.0, 0.25, 0.5, 0.75, 1.0]
        .map(|offset| stop(offset, "#000000"))
        .to_vec();
    let window = harness(full);

    press(&window, on_track(&window, 0.125));

    assert_eq!(window.get_change_count(), 0);
}

fn dragging_a_stop_moves_it_horizontally_without_adding_a_stop() {
    let window = harness(two_stops());
    let node = element(&window, "Color stop at 0%");

    press(&window, element_center(&node));
    move_to(&window, on_track(&window, 0.6));
    release(&window, on_track(&window, 0.6));

    assert_eq!(window.get_change_count(), 1);
    assert_eq!(
        last_change(&window),
        vec![stop(0.6, "#000000"), stop(1.0, "#ffffff")]
    );
}

fn delete_removes_a_stop_only_while_more_than_two_remain() {
    let mut three = two_stops();
    three.push(stop(0.5, "#808080"));
    let window = harness(three);
    let node = element(&window, "Color stop at 50%");

    focus_node(&window, &node);
    special_key(&window, Key::Delete);

    assert_eq!(window.get_change_count(), 1);
    assert_eq!(last_change(&window), two_stops());
}

fn keeps_at_least_two_stops() {
    let window = harness(two_stops());
    let node = element(&window, "Color stop at 0%");

    focus_node(&window, &node);
    special_key(&window, Key::Delete);

    assert_eq!(window.get_change_count(), 0);
}

fn clicking_a_stop_opens_the_color_picker() {
    let window = harness(two_stops());
    let node = element(&window, "Color stop at 0%");

    click(&window, element_center(&node));
    settle();

    assert_eq!(
        element(&window, "Remove stop").accessible_enabled(),
        Some(false)
    );
}

#[test]
fn cases() {
    run_cases(&[
        (
            "clicking_the_track_adds_an_interpolated_stop",
            clicking_the_track_adds_an_interpolated_stop,
        ),
        (
            "does_not_add_more_than_five_stops",
            does_not_add_more_than_five_stops,
        ),
        (
            "dragging_a_stop_moves_it_horizontally_without_adding_a_stop",
            dragging_a_stop_moves_it_horizontally_without_adding_a_stop,
        ),
        (
            "delete_removes_a_stop_only_while_more_than_two_remain",
            delete_removes_a_stop_only_while_more_than_two_remain,
        ),
        ("keeps_at_least_two_stops", keeps_at_least_two_stops),
        (
            "clicking_a_stop_opens_the_color_picker",
            clicking_a_stop_opens_the_color_picker,
        ),
    ]);
}
