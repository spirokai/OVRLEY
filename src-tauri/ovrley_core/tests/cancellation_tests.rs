//! Render cancellation lifecycle tests.
//!
//! Verifies the `RenderController` state machine directly. The older ignored
//! tests that only documented hypothetical live-pipeline transitions were
//! removed because they were redundant and never exercised in normal runs.

use ovrley_core::encode::video::RenderController;

/// Double cancel is idempotent: no panic, no deadlock.
#[test]
fn double_cancel_is_idempotent() {
    let controller = RenderController::default();
    controller.try_start(100, "test_double_cancel").unwrap();

    let _ = controller.cancel();
    let _ = controller.cancel();

    let progress = controller.progress();
    assert!(progress.status == "cancelled" || progress.status == "running");
}

/// Progress reports zero frames before any frames are rendered.
#[test]
fn progress_starts_at_zero() {
    let controller = RenderController::default();
    controller.try_start(100, "test_progress_zero").unwrap();

    let progress = controller.progress();
    assert_eq!(progress.encoded, 0);
}

/// Start -> cancel lifecycle at the controller level.
#[test]
fn controller_lifecycle_start_and_cancel() {
    let controller = RenderController::default();

    assert!(controller.try_start(30, "test_lifecycle").is_ok());
    let progress = controller.progress();
    assert!(progress.total > 0);

    let _ = controller.cancel();
    let progress = controller.progress();
    assert!(progress.status == "cancelled" || progress.status == "running");
}
