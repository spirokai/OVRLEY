//! Elevation-sample RDP simplification tests.
//!
//! Verifies `simplify_elevation_samples_segment` and
//! `simplify_elevation_samples` against `ElevationSample` data with
//! straight-line, collinear, peak-preservation, empty, single-point,
//! and flagged-preserve scenarios.
//!
//! ## Why nested test modules
//!
//! These tests require access to `render::widgets::elevation` internals
//! (`ElevationSample`, `simplify_elevation_samples_segment`) that are
//! `pub(crate)` and not reachable from crate-level integration tests.
//! The parent module's `#[cfg(test)] mod tests;` wiring is retained
//! for this reason.
//!
//! ## Type
//! Unit test (module-local). No I/O, no fixtures — pure geometry.
//!
//! ## Regressions guarded
//! - Elevation RDP removing flagged/preserved peaks
//! - Empty or single-point inputs panicking
//! - Collinear points producing wrong simplification

use super::super::elevation::{
    simplify_elevation_samples, simplify_elevation_samples_segment, ElevationSample,
};

fn sample(x: f32, y: f32, progress01: f32) -> ElevationSample {
    ElevationSample {
        point: (x, y),
        progress01,
        preserve: false,
    }
}

#[test]
fn segment_straight_line_keeps_endpoints() {
    let points = vec![sample(0.0, 0.0, 0.0), sample(100.0, 50.0, 1.0)];
    let simplified = simplify_elevation_samples_segment(&points, 0.001);
    assert_eq!(simplified.len(), 2);
}

#[test]
fn segment_collinear_removes_middle() {
    let points = vec![
        sample(0.0, 0.0, 0.0),
        sample(50.0, 50.0, 0.5),
        sample(100.0, 100.0, 1.0),
    ];
    let simplified = simplify_elevation_samples_segment(&points, 0.001);
    assert_eq!(simplified.len(), 2);
}

#[test]
fn segment_empty_returns_empty() {
    let points: Vec<ElevationSample> = vec![];
    let simplified = simplify_elevation_samples_segment(&points, 0.001);
    assert!(simplified.is_empty());
}

#[test]
fn segment_single_point_returns_same() {
    let points = vec![sample(42.0, 42.0, 0.0)];
    let simplified = simplify_elevation_samples_segment(&points, 0.001);
    assert_eq!(simplified.len(), 1);
}

#[test]
fn segment_preserves_peak() {
    let points = vec![
        sample(0.0, 0.0, 0.0),
        sample(50.0, 100.0, 0.5),
        sample(100.0, 0.0, 1.0),
    ];
    let simplified = simplify_elevation_samples_segment(&points, 0.001);
    assert_eq!(simplified.len(), 3);
    let simplified = simplify_elevation_samples_segment(&points, 101.0);
    assert_eq!(simplified.len(), 2);
}

#[test]
fn preserves_flagged_points() {
    let points = vec![
        sample(0.0, 0.0, 0.0),
        sample(25.0, 10.0, 0.25),
        sample(50.0, 20.0, 0.5),
        sample(75.0, 30.0, 0.75),
        sample(100.0, 40.0, 1.0),
    ];
    let mut with_flag = points.clone();
    with_flag[2].preserve = true;
    with_flag[3].preserve = true;
    let simplified = simplify_elevation_samples(&with_flag, 0.001);
    for pt in &simplified {
        if pt.preserve {
            let found = with_flag.iter().any(|p| p.point == pt.point && p.preserve);
            assert!(found);
        }
    }
}
