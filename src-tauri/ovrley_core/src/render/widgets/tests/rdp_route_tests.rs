use super::super::route::simplify_route_samples;
use super::super::types::RouteSample;

fn sample(x: f32, y: f32, progress01: f32) -> RouteSample {
    RouteSample {
        point: (x, y),
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
