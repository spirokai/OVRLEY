//! The shell's drawer layout and window controls (ports of `ToolbarDrawerLayout.test.jsx` and
//! `TitleBar-boundary.test.jsx`).

mod common;

use common::*;
use slint::ComponentHandle;

fn harness(visible: bool, pinned: bool) -> ovrley_ui::ShellHarness {
    let window = ovrley_ui::ShellHarness::new().unwrap();
    ovrley_ui::install_component_logic(&window);
    window.set_drawer_visible(visible);
    window.set_pinned(pinned);
    window.show().unwrap();
    // Let the drawer's slide and the allocation settle.
    i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(400));
    window
}

/// The unpinned drawer's backdrop covers the control panel too: a press there dismisses the drawer
/// and does not reach the control panel.
fn a_press_on_the_control_panel_dismisses_an_unpinned_drawer() {
    let window = harness(true, false);
    click(&window, element_center(&element(&window, "Control panel button")));
    settle();
    assert!(!window.get_drawer_visible());
    assert_eq!(window.get_control_panel_clicks(), 0);
}

fn a_pinned_drawer_has_no_backdrop() {
    let window = harness(true, true);
    click(&window, element_center(&element(&window, "Control panel button")));
    settle();
    assert!(window.get_drawer_visible());
    assert_eq!(window.get_control_panel_clicks(), 1);
}

/// While the pinned allocation animates, a blocker covers the workspace.
fn the_blocker_covers_the_workspace_while_the_allocation_animates() {
    let window = harness(true, false);
    click(&window, element_center(&element(&window, "Pin drawer")));
    let workspace = element_center(&element(&window, "Workspace"));
    click(&window, workspace);
    assert_eq!(window.get_workspace_presses(), 0, "the blocker takes the press");
    i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(300));
    click(&window, workspace);
    assert_eq!(window.get_workspace_presses(), 1, "the blocker is gone once the allocation settled");
}

/// Minimise, maximise and close everywhere but macOS, where the native traffic lights are used.
fn window_controls_are_drawn_except_on_macos() {
    let window = harness(false, false);
    for label in ["Minimize", "Maximize", "Close"] {
        let found = i_slint_backend_testing::ElementHandle::find_by_accessible_label(&window, label).next();
        assert_eq!(found.is_some(), !cfg!(target_os = "macos"), "{label}");
    }
}

#[test]
fn cases() {
    run_cases(&[
        ("a_press_on_the_control_panel_dismisses_an_unpinned_drawer", a_press_on_the_control_panel_dismisses_an_unpinned_drawer),
        ("a_pinned_drawer_has_no_backdrop", a_pinned_drawer_has_no_backdrop),
        ("the_blocker_covers_the_workspace_while_the_allocation_animates", the_blocker_covers_the_workspace_while_the_allocation_animates),
        ("window_controls_are_drawn_except_on_macos", window_controls_are_drawn_except_on_macos),
    ]);
}
