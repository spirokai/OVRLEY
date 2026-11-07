//! Progress estimator unit tests.
//!
//! Verifies `ProgressEstimator` throughput / ETA behavior: warmup period
//! (no estimates for the first few `record` calls), the warmup-excluded
//! long-term wall-clock anchor (no asymptotic FPS climb), the rolling-window
//! median's robustness to single outlier batches, ETA calculation, and the
//! scaled-frame-second input for output-equivalent FPS.
//!
//! ## Type
//! Unit test. Pure math — no I/O, no fixtures, no threading.
//!
//! ## Regressions guarded
//! - Warmup returning flawed early estimates (NaN, Infinity)
//! - Cold-start wall-clock cost permanently dragging reported FPS downward
//! - Single outlier batch (reorder-window flush) yanking reported FPS wildly
//! - Scaled frame seconds not producing correct output-equivalent FPS

use ovrley_core::encode::progress::ProgressEstimator;

#[test]
fn returns_none_during_warmup() {
    let mut estimator = ProgressEstimator::new(0.90);

    for i in 1..=5 {
        let (eta, fps) = estimator.record(i, 100, 0.033, 0.033 * f64::from(i));
        assert_eq!(eta, None, "frame {i} should still be in warmup");
        assert_eq!(fps, None, "frame {i} should still be in warmup");
    }

    // Frame 6 exits warmup
    let (eta, fps) = estimator.record(6, 100, 0.033, 0.2);
    assert!(eta.is_some());
    assert!(fps.is_some());
}

#[test]
fn reports_immediately_when_progress_and_elapsed_time_exist() {
    let mut estimator = ProgressEstimator::new(0.90);

    // First 5 frames are warmup; manually skip them.
    for i in 1..=5 {
        estimator.record(i, 100, 0.5, 0.5 * f64::from(i));
    }

    let (eta, fps) = estimator.record(6, 10, 0.5, 3.0);
    assert_eq!(eta, Some(2));
    assert_eq!(fps, Some(2.0));
}

/// The headline regression: with the old `min(ema, wall)` estimator, the
/// wall-clock term included warmup time and reported FPS climbed asymptotically
/// toward true throughput for the entire job.  The new estimator subtracts
/// the warmup wall cost from the long-term anchor, so the first post-warmup
/// report already matches true steady-state throughput and stays there.
#[test]
fn long_term_anchor_excludes_warmup_so_fps_does_not_asymptotically_climb() {
    let mut estimator = ProgressEstimator::new(0.70);

    // Cold-start warmup: 5 frames at 0.5 s each (2.5 s of warmup wall time),
    // then steady-state rendering at 0.1 s/frame (10 fps).
    for i in 1..=5 {
        estimator.record(i, 100, 0.5, 0.5 * f64::from(i));
    }

    // First post-warmup record.  With the old estimator this would report
    // `min(ema, 6/2.6) ≈ min(10, 2.31) ≈ 2.3 fps` — a permanent underestimate.
    // The new estimator reports ~10 fps immediately because the 2.5 s warmup
    // cost is excluded from `clean_wall_fps`.
    let (_eta, fps) = estimator.record(6, 100, 0.1, 2.6);
    let fps = fps.expect("post-warmup FPS should be reported");
    assert!(
        (fps - 10.0).abs() < 1.0,
        "first post-warmup FPS should match true throughput (~10), got {fps}"
    );

    // After many steady-state frames the reported FPS should STILL be near 10 —
    // not climb further toward 10 as the old estimator would.
    for i in 7..=60 {
        let elapsed = 2.6 + 0.1 * f64::from(i - 6);
        let (_eta, fps) = estimator.record(i, 100, 0.1, elapsed);
        let fps = fps.expect("FPS should remain reported in steady state");
        assert!(
            (fps - 10.0).abs() < 2.0,
            "steady-state FPS should stay near 10, got {fps} at frame {i}"
        );
    }
}

/// A single outlier `frame_seconds` sample (e.g. from a reorder-window flush
/// that releases many frames in one short batch) must not throw the reported
/// FPS wildly off the steady state.  The rolling-window median is robust to a
/// single outlier; the wall-trust-band clamp additionally guards against it.
#[test]
fn rolling_window_median_ignores_single_outlier_batch() {
    let mut estimator = ProgressEstimator::new(0.70);

    // Warmup at 0.1 s/frame.
    for i in 1..=5 {
        estimator.record(i, 100, 0.1, 0.1 * f64::from(i));
    }

    // 15 steady-state records at 0.1 s/frame (10 fps).
    for i in 6..=20 {
        estimator.record(i, 100, 0.1, 0.1 * f64::from(i));
    }

    // One outlier fast batch — a reorder-window flush claiming 0.001 s/frame
    // (instantaneous 1000 fps).  Without rolling-window protection this would
    // yank the displayed FPS sharply upward.
    let (_eta, fps) = estimator.record(21, 100, 0.001, 2.001);
    let fps = fps.expect("FPS should still be reported after an outlier");
    assert!(
        (fps - 10.0).abs() < 3.0,
        "single fast outlier must not yank reported FPS, got {fps}"
    );
}

/// When the steady-state throughput equals the warmup throughput, the
/// long-term anchor and the rolling-window median agree and ETA reflects
/// the remaining frame count / throughput.
#[test]
fn converges_to_true_throughput_when_warmup_matches_steady_state() {
    let mut estimator = ProgressEstimator::new(0.90);

    // Warmup with frame_seconds=1.0 (1 fps).
    for i in 1..=5 {
        estimator.record(i, 100, 1.0, 1.0 * f64::from(i));
    }

    // Steady state also 1 fps.
    let (eta, fps) = estimator.record(10, 10, 1.0, 10.0);
    assert_eq!(eta, Some(0));
    assert_eq!(fps, Some(1.0));
}

#[test]
fn can_report_output_equivalent_fps_from_scaled_frame_seconds() {
    let mut estimator = ProgressEstimator::new(0.90);

    // Skip warmup.
    for i in 1..=5 {
        estimator.record(i, 60, 0.1 / 6.0, 0.1);
    }

    let (_eta, fps) = estimator.record(6, 60, 0.1 / 6.0, 0.1);

    assert!((fps.unwrap() - 60.0).abs() < 1e-9);
}
