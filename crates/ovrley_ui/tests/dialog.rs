//! Dialog behaviour: Escape closes it, Tab stays inside it while it is open, and its tooltips show
//! above it.

mod common;

use common::*;
use ovrley_ui::TooltipLayer;
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

/// A tooltip inside the dialog shows in the dialog's own layer, above the scrim, and hides when the
/// dialog closes under it.
fn tooltips_show_above_the_dialog() {
    let window = harness();
    move_to(&window, element_center(&element(&window, "First")));
    wait(450);
    let layer = window.global::<TooltipLayer>();
    assert!(layer.get_shown(), "the tooltip did not show");
    assert_eq!((layer.get_layer(), layer.get_popup_depth()), (1, 1), "the tooltip is not in the dialog's layer");
    window.set_open(false);
    settle();
    assert!(!layer.get_shown(), "the tooltip outlived its dialog");
    assert_eq!(layer.get_popup_depth(), 0);
}

#[test]
fn cases() {
    run_cases(&[
        ("tab_stays_inside_the_dialog", tab_stays_inside_the_dialog),
        ("escape_closes_the_dialog", escape_closes_the_dialog),
        ("tooltips_show_above_the_dialog", tooltips_show_above_the_dialog),
    ]);
}
