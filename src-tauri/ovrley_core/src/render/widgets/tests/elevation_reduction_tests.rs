//! Elevation downsampler vertical-segment preservation tests.
//!
//! Verifies that `downsample_elevation_points` preserves meaningful
//! vertical segments (same progress, changing elevation) while collapsing
//! barometric noise at stops.
//!
//! ## Type
//! Unit test (module-local). No I/O — pure downsampling logic.
//!
//! ## Regressions guarded
//! - Drone hover-climb collapsed to single point
//! - Noise-level barometric variation preserved as vertical scribble
//! - Mixed forward-progress + vertical runs mishandled

use super::super::elevation::downsample_elevation_points;

/// 10 consecutive same-progress samples spanning 5m — real vertical flight.
/// Multiple points must survive downsampling.
#[test]
fn downsample_preserves_meaningful_vertical_run() {
    let progress = 0.0f32;
    let elevation_base = 100.0f64;
    let points: Vec<(f32, f64, f32)> = (0..10)
        .map(|i| {
            (
                progress,
                elevation_base + i as f64 * 0.5,
                i as f32 / 9.0,
            )
        })
        .collect();

    let target_count = 5;
    let result = downsample_elevation_points(&points, target_count);

    assert!(
        result.len() >= 3,
        "meaningful vertical run should preserve multiple points, got {}",
        result.len()
    );

    for pt in &result {
        assert!(
            (pt.progress01 - progress).abs() <= f32::EPSILON,
            "all points in vertical run should have same progress"
        );
    }
}

/// 10 consecutive same-progress samples spanning 0.1m — barometric noise.
/// Output should collapse to a single point (noise threshold < 0.5m).
#[test]
fn downsample_collapses_noise_vertical_run() {
    let progress = 0.5f32;
    let elevation_base = 100.0f64;
    let points: Vec<(f32, f64, f32)> = (0..10)
        .map(|i| {
            (
                progress,
                elevation_base + i as f64 * 0.01, // 0.09m span — below 0.5m threshold
                i as f32 / 9.0,
            )
        })
        .collect();

    let target_count = 5;
    let result = downsample_elevation_points(&points, target_count);

    // All points share same progress and span < 0.5m — only 1 or 2 should survive.
    assert!(
        result.len() <= 2,
        "noise-level vertical run should collapse, got {} points",
        result.len()
    );
}

/// Alternating forward progress and vertical runs — correct points in each.
///
/// Structure: 5 forward-progress points (0.0, 0.25, 0.5, 0.75, 1.0)
/// interleaved with a 3-point meaningful vertical run at progress 0.5 (5m span).
/// The vertical run should preserve multiple points while forward points
/// progress normally.
#[test]
fn downsample_mixed_runs() {
    // Forward progress points with normal elevation change
    let mut points: Vec<(f32, f64, f32)> = vec![
        (0.0, 100.0, 0.0),
        (0.25, 110.0, 0.1),
        (0.5, 120.0, 0.2),
        (0.75, 130.0, 0.3),
        (1.0, 140.0, 0.4),
    ];

    // Insert a 3-point vertical run at progress 0.5 spanning 5m
    // (indices 2-4 in the final array, after the 0.25 forward point)
    let vertical_run = vec![
        (0.5, 120.0, 0.21),
        (0.5, 122.0, 0.22),
        (0.5, 125.0, 0.23), // 5m span from 120 to 125
        (0.5, 120.0, 0.24),
    ];
    // Insert after index 2 (the original 0.5 point)
    points.splice(3..3, vertical_run);

    let target_count = 10;
    let result = downsample_elevation_points(&points, target_count);

    // Forward-progress points (0.0, 0.25, 0.75, 1.0) must survive
    let progresses: Vec<f32> = result.iter().map(|p| p.progress01).collect();
    assert!(
        progresses.contains(&0.0),
        "forward point at 0.0 should survive"
    );
    assert!(
        progresses.contains(&0.25),
        "forward point at 0.25 should survive"
    );
    assert!(
        progresses.contains(&0.75),
        "forward point at 0.75 should survive"
    );
    assert!(
        progresses.contains(&1.0),
        "forward point at 1.0 should survive"
    );

    // Vertical run at 0.5 should have multiple points (5m span > 0.5m)
    let at_0_5: Vec<_> = result.iter().filter(|p| (p.progress01 - 0.5).abs() <= f32::EPSILON).collect();
    assert!(
        at_0_5.len() >= 2,
        "vertical run at 0.5 should preserve >=2 points, got {}",
        at_0_5.len()
    );
}
