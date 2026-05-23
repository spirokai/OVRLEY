//! RDP (Ramer-Douglas-Peucker) behavior tests.
//!
//! These tests validate the production RDP implementation in
//! `ovrley_core::rdp` — the single canonical source shared by route and
//! elevation widget simplification. Tests include: straight-line
//! preservation, collinear point removal, peak preservation, empty/single-
//! point edge cases, and tolerance-at-threshold behavior.
//!
//! The tests call `perpendicular_distance` and `simplify_rdp_indices`
//! directly from `ovrley_core::rdp`. No local algorithm copy exists.
//!
//! ## Type
//! Integration test (production-api). No I/O, no fixtures — pure math.
//!
//! ## Regressions guarded
//! - Perpendicular distance returning wrong values for offset points
//! - RDP simplification removing peaks it should preserve
//! - Empty/single-point inputs panicking
//! - Tolerance boundary (exactly at max distance) removing or keeping
//!   the wrong points

use ovrley_core::rdp::{perpendicular_distance, simplify_rdp_indices};

/// Straight line (2 points) — both endpoints kept.
#[test]
fn straight_line_keeps_both_endpoints() {
    let points = vec![(0.0, 0.0), (100.0, 100.0)];
    let indices = simplify_rdp_indices(&points, 0.0);
    assert_eq!(indices.len(), 2);
    assert_eq!(indices[0], 0);
    assert_eq!(indices[1], 1);
}

/// Collinear points — middle points removed.
#[test]
fn collinear_points_remove_middle() {
    let points = vec![(0.0, 0.0), (50.0, 50.0), (100.0, 100.0)];
    let indices = simplify_rdp_indices(&points, 0.0);
    assert_eq!(indices.len(), 2);
    assert_eq!(indices[0], 0);
    assert_eq!(indices[1], 2);
}

/// Single point — returns the point.
#[test]
fn single_point_returns_itself() {
    let points = vec![(42.0, 42.0)];
    let indices = simplify_rdp_indices(&points, 1.0);
    assert_eq!(indices.len(), 1);
    assert_eq!(indices[0], 0);
}

/// Empty input — returns empty.
#[test]
fn empty_input_returns_empty() {
    let points: Vec<(f32, f32)> = vec![];
    let indices = simplify_rdp_indices(&points, 1.0);
    assert!(indices.is_empty());
}

/// Perpendicular distance for a point exactly on the line segment.
#[test]
fn perpendicular_distance_on_line_is_zero() {
    let dist = perpendicular_distance((50.0, 50.0), (0.0, 0.0), (100.0, 100.0));
    assert!(dist.abs() < f32::EPSILON);
}

/// Perpendicular distance for a point offset from the line.
#[test]
fn perpendicular_distance_offset_point() {
    // Point (50, 60) is 10 units vertically offset from line y=x.
    let dist = perpendicular_distance((50.0, 60.0), (0.0, 0.0), (100.0, 100.0));
    let expected = (10.0_f32 / 2.0_f32.sqrt()).abs();
    assert!((dist - expected).abs() < 0.001);
}

/// Peak (triangle shape) — preserves the peak point at reasonable tolerance.
#[test]
fn rdp_preserves_peaks() {
    let points = vec![(0.0, 0.0), (50.0, 100.0), (100.0, 0.0)];
    // With tolerance 0, the peak must be kept.
    let indices = simplify_rdp_indices(&points, 0.0);
    assert_eq!(indices.len(), 3);
    assert_eq!(indices, vec![0, 1, 2]);
    // With tolerance large enough, the peak is removed.
    let indices = simplify_rdp_indices(&points, 101.0);
    assert_eq!(indices.len(), 2);
    assert_eq!(indices, vec![0, 2]);
}

/// Tolerance exactly at the maximum distance removes the point.
#[test]
fn rdp_tolerance_at_max_distance_removes_point() {
    let points = vec![(0.0, 0.0), (50.0, 50.0), (100.0, 0.0)];
    let dist_to_mid = perpendicular_distance((50.0, 50.0), (0.0, 0.0), (100.0, 0.0));
    let indices = simplify_rdp_indices(&points, dist_to_mid);
    assert_eq!(indices.len(), 2);
    assert_eq!(indices, vec![0, 2]);
    let indices = simplify_rdp_indices(&points, dist_to_mid - 0.001);
    assert_eq!(indices.len(), 3);
    assert_eq!(indices, vec![0, 1, 2]);
}
