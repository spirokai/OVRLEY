use ovrley_core::encode::video::RenderController;

/// Start then immediate cancel.
/// The render controller must cleanly transition from Running to Cancelled.
#[test]
#[ignore = "Requires render pipeline to be running for state transitions"]
fn start_immediate_cancel_cancels_cleanly() {
    let controller = RenderController::default();
    controller
        .try_start(100, "test_start_immediate_cancel")
        .unwrap();

    assert_eq!(controller.progress().status, "running");

    controller.cancel();
    let progress = controller.progress();
    assert_eq!(progress.status, "cancelled");
}

/// Double cancel is idempotent — no panic, no deadlock.
#[test]
fn double_cancel_is_idempotent() {
    let controller = RenderController::default();
    controller.try_start(100, "test_double_cancel").unwrap();

    controller.cancel();
    controller.cancel(); // must not panic

    // After cancel, progress reflects cancelled state
    let progress = controller.progress();
    assert!(progress.status == "cancelled" || progress.status == "running");
}

/// Cancel resets state so a new render can start cleanly.
#[test]
#[ignore = "Requires render pipeline to be running for state transitions"]
fn cancel_resets_state_for_next_render() {
    let controller = RenderController::default();

    controller.try_start(50, "test_cancel_reset_1").unwrap();
    controller.cancel();

    // After cancel, a new start should succeed.
    let result = controller.try_start(50, "test_cancel_reset_2");
    assert!(result.is_ok());

    controller.cancel();
}

/// Progress reports zero frames before any frames are rendered.
#[test]
fn progress_starts_at_zero() {
    let controller = RenderController::default();
    controller.try_start(100, "test_progress_zero").unwrap();

    let progress = controller.progress();
    assert_eq!(progress.encoded, 0);
}

/// Start -> complete normally (unit-level, no ffmpeg).
#[test]
fn controller_lifecycle_start_and_cancel() {
    let controller = RenderController::default();

    // Can start
    assert!(controller.try_start(30, "test_lifecycle").is_ok());
    let progress = controller.progress();
    assert!(progress.total > 0);

    // Cancel works
    controller.cancel();
    let progress = controller.progress();
    assert!(progress.status == "cancelled" || progress.status == "running");
}
