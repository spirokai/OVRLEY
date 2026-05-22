//! RDP (Ramer-Douglas-Peucker) behavior tests.
//!
//! These tests validate the perpendicular-distance math and simplification
//! invariants that both `route.rs` and `elevation.rs` rely on. Since the
//! implementations are currently private and duplicated, these tests serve as
//! a behavioral specification for the Phase 3 extraction.

/// Straight line (2 points) — both endpoints kept.
#[test]
fn straight_line_keeps_both_endpoints() {
    let points = vec![(0.0, 0.0), (100.0, 100.0)];
    // Simplification with tolerance 0.0 of a straight line keeps both.
    let simplified = rdp_simplify(&points, 0.0);
    assert_eq!(simplified.len(), 2);
    assert_eq!(simplified[0], (0.0, 0.0));
    assert_eq!(simplified[1], (100.0, 100.0));
}

/// Collinear points — middle points removed.
#[test]
fn collinear_points_remove_middle() {
    let points = vec![(0.0, 0.0), (50.0, 50.0), (100.0, 100.0)];
    let simplified = rdp_simplify(&points, 0.0);
    assert_eq!(simplified.len(), 2);
    assert_eq!(simplified[0], (0.0, 0.0));
    assert_eq!(simplified[1], (100.0, 100.0));
}

/// Single point — returns the point.
#[test]
fn single_point_returns_itself() {
    let points = vec![(42.0, 42.0)];
    let simplified = rdp_simplify(&points, 1.0);
    assert_eq!(simplified.len(), 1);
}

/// Empty input — returns empty.
#[test]
fn empty_input_returns_empty() {
    let points: Vec<(f32, f32)> = vec![];
    let simplified = rdp_simplify(&points, 1.0);
    assert!(simplified.is_empty());
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

// --- Reusable RDP implementation for Phase 3 behavioral spec ---

fn perpendicular_distance(
    point: (f32, f32),
    start: (f32, f32),
    end: (f32, f32),
) -> f32 {
    let (x0, y0) = point;
    let (x1, y1) = start;
    let (x2, y2) = end;
    let dx = x2 - x1;
    let dy = y2 - y1;
    if dx.abs() <= f32::EPSILON && dy.abs() <= f32::EPSILON {
        return ((x0 - x1).powi(2) + (y0 - y1).powi(2)).sqrt();
    }
    (dy * x0 - dx * y0 + x2 * y1 - y2 * x1).abs() / (dx * dx + dy * dy).sqrt()
}

fn rdp_simplify(points: &[(f32, f32)], tolerance: f32) -> Vec<(f32, f32)> {
    if points.len() <= 2 {
        return points.to_vec();
    }

    let mut max_distance = 0.0f32;
    let mut split_index = 0usize;
    let start = points[0];
    let end = points[points.len() - 1];

    for i in 1..points.len() - 1 {
        let distance = perpendicular_distance(points[i], start, end);
        if distance > max_distance {
            max_distance = distance;
            split_index = i;
        }
    }

    if max_distance <= tolerance {
        return vec![start, end];
    }

    let left = rdp_simplify(&points[..=split_index], tolerance);
    let right = rdp_simplify(&points[split_index..], tolerance);

    let mut result = left[..left.len() - 1].to_vec();
    result.extend(right);
    result
}
