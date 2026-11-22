//! Port of app/src/tests/components/ui/BlurInput.test.jsx: BlurInput's draft lifecycle.

mod common;

use common::*;
use slint::platform::Key;
use slint::ComponentHandle;

fn harness() -> ovrley_ui::BlurInputHarness {
    let window = ovrley_ui::BlurInputHarness::new().unwrap();
    ovrley_ui::install_component_logic(&window);
    window.show().unwrap();
    window
}

fn preserves_an_in_progress_draft_when_the_committed_value_changes_externally() {
    let window = harness();
    let input = element(&window, "FPS");
    click(&window, element_center(&input));
    special_key(&window, Key::Backspace);
    special_key(&window, Key::Backspace);
    type_text(&window, "25");
    settle();
    assert_eq!(input.accessible_value().as_deref(), Some("25"));

    window.set_value("60".into());
    settle();

    assert_eq!(input.accessible_value().as_deref(), Some("25"));
}

fn adopts_external_committed_values_when_no_dirty_draft_is_active() {
    let window = harness();
    let input = element(&window, "FPS");

    window.set_value("60".into());
    settle();

    assert_eq!(input.accessible_value().as_deref(), Some("60"));
}

#[test]
fn cases() {
    run_cases(&[
        (
            "preserves_an_in_progress_draft_when_the_committed_value_changes_externally",
            preserves_an_in_progress_draft_when_the_committed_value_changes_externally,
        ),
        (
            "adopts_external_committed_values_when_no_dirty_draft_is_active",
            adopts_external_committed_values_when_no_dirty_draft_is_active,
        ),
    ]);
}
