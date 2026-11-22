use super::format::{format_lean_angle_value, lean_angle_fill_sweep};
use super::layout::lean_angle_layout;

#[test]
fn lean_angle_layout_uses_diameter_and_includes_the_stable_label_frame() {
    let geometry = lean_angle_layout(300.0, 100.0, 60.0);

    assert_eq!(geometry.start_angle, 210.0);
    assert_eq!(geometry.sweep_angle, 120.0);
    assert_eq!(geometry.outer_radius, 150.0);
    assert_eq!(geometry.inner_radius, 50.0);
    assert!((geometry.width - 259.80762).abs() < 0.0001);
    assert!((geometry.height - 177.6).abs() < 0.0001);
    assert!((geometry.center_x - 129.90381).abs() < 0.0001);
    assert_eq!(geometry.center_y, 150.0);
}

#[test]
fn lean_angle_inner_radius_respects_track_thickness() {
    let geometry = lean_angle_layout(180.0, 24.0, 60.0);

    assert_eq!(geometry.outer_radius, 90.0);
    assert_eq!(geometry.inner_radius, 66.0);
    assert_eq!(geometry.outer_radius - geometry.inner_radius, 24.0);
}

#[test]
fn lean_angle_maps_signed_samples_and_formats_missing_values() {
    assert_eq!(lean_angle_fill_sweep(Some(30.0)), 30.0);
    assert_eq!(lean_angle_fill_sweep(Some(-70.0)), -60.0);
    assert_eq!(lean_angle_fill_sweep(Some(0.0)), 0.0);
    assert_eq!(lean_angle_fill_sweep(None), 0.0);
    assert_eq!(format_lean_angle_value(Some(-30.4)), "30");
    assert_eq!(format_lean_angle_value(None), "--");
}
