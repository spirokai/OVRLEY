//! Route-sample RDP simplification tests.
//!
//! Verifies `simplify_route_samples` against `RouteSample` data with
//! straight-line, collinear, peak-preservation, empty, and single-point
//! scenarios.
//!
//! ## Why nested test modules
//!
//! These tests require access to `render::widgets::route` internals
//! that are `pub(crate)` and not reachable from crate-level integration
//! tests. The parent module's `#[cfg(test)] mod tests;` wiring is retained
//! for this reason.
//!
//! ## Type
//! Unit test (module-local). No I/O, no fixtures — pure geometry.
//!
//! ## Regressions guarded
//! - Route RDP removing peaks it should preserve
//! - Empty or single-point inputs panicking
//! - Collinear simplification diverging from expected endpoints

use super::super::route::simplify_route_samples;
use super::super::types::RouteSample;

fn sample(x: f32, y: f32, progress01: f32) -> RouteSample {
    RouteSample {
        point: (x as f64, y as f64),
        progress01,
    }
}

#[test]
fn straight_line_keeps_endpoints() {
    let points = vec![sample(0.0, 0.0, 0.0), sample(100.0, 100.0, 1.0)];
    let simplified = simplify_route_samples(&points, 0.0);
    assert_eq!(simplified.len(), 2);
}

#[test]
fn collinear_removes_middle() {
    let points = vec![
        sample(0.0, 0.0, 0.0),
        sample(50.0, 50.0, 0.5),
        sample(100.0, 100.0, 1.0),
    ];
    let simplified = simplify_route_samples(&points, 0.0);
    assert_eq!(simplified.len(), 2);
    assert_eq!(simplified[0].point, (0.0, 0.0));
    assert_eq!(simplified[1].point, (100.0, 100.0));
}

#[test]
fn empty_returns_empty() {
    let points: Vec<RouteSample> = vec![];
    let simplified = simplify_route_samples(&points, 1.0);
    assert!(simplified.is_empty());
}

#[test]
fn single_point_returns_same() {
    let points = vec![sample(42.0, 42.0, 0.0)];
    let simplified = simplify_route_samples(&points, 1.0);
    assert_eq!(simplified.len(), 1);
}

#[test]
fn preserves_peak() {
    let points = vec![
        sample(0.0, 0.0, 0.0),
        sample(50.0, 100.0, 0.5),
        sample(100.0, 0.0, 1.0),
    ];
    let simplified = simplify_route_samples(&points, 0.0);
    assert_eq!(simplified.len(), 3);
    let simplified = simplify_route_samples(&points, 101.0);
    assert_eq!(simplified.len(), 2);
}
