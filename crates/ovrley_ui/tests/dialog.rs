//! Dialog keyboard behaviour: Escape closes it, and Tab stays inside it while it is open.

mod common;

use common::*;
use slint::platform::Key;
use slint::ComponentHandle;

fn harness() -> ovrley_ui::DialogHarness {
    let window = ovrley_ui::DialogHarness::new().unwrap();
    ovrley_ui::install_component_logic(&window);
    window.show().unwrap();
    window.set_open(true);
    settle();
    window
}

/// Tabs through the dialog several times, activating each focused element with Enter; only the
/// dialog's buttons may be reached.
fn tab_stays_inside_the_dialog() {
    let window = harness();
    for _ in 0..6 {
        special_key(&window, Key::Tab);
        special_key(&window, Key::Return);
        assert_ne!(
            window.get_last_clicked().as_str(),
            "behind",
            "Tab reached the button behind the dialog"
        );
    }
    assert!(
        ["first", "second"].contains(&window.get_last_clicked().as_str()),
        "Tab never reached the dialog's buttons"
    );
}

fn escape_closes_the_dialog() {
    let window = harness();
    special_key(&window, Key::Escape);
    settle();
    assert!(!window.get_open());
}

#[test]
fn cases() {
    run_cases(&[
        ("tab_stays_inside_the_dialog", tab_stays_inside_the_dialog),
        ("escape_closes_the_dialog", escape_closes_the_dialog),
    ]);
}
