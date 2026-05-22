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

#[test]
fn clamps_optimistic_ema_to_wall_clock_throughput() {
    let mut estimator = ProgressEstimator::new(0.90);

    // Skip warmup.
    for i in 1..=5 {
        estimator.record(i, 50, 0.1, 0.1 * f64::from(i));
    }

    // Frame 6 took 0.1s (10 fps via EMA) but wall clock says 6 frames
    // in 2.0s = 3 fps overall.  Reported FPS should be 3 (the slower).
    let (eta, fps) = estimator.record(6, 50, 0.1, 2.0);
    assert_eq!(fps, Some(3.0));
    assert_eq!(eta, Some(15)); // ceil((50 - 6) / 3.0)
}

#[test]
fn uses_ema_when_it_is_slower_than_wall_clock_throughput() {
    let mut estimator = ProgressEstimator::new(0.90);

    // Skip warmup.
    for i in 1..=5 {
        estimator.record(i, 100, 1.0, 1.0 * f64::from(i));
    }

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
