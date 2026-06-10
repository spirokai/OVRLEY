//! Elevation geometry elapsed-fraction and data-range tests.
//!
//! Verifies that `build_elevation_geometry` carries elapsed-time information
//! through the pipeline and stores the correct elevation data range.
//!
//! ## Type
//! Unit test (module-local). No I/O — pure geometry pipeline.
//!
//! ## Regressions guarded
//! - Elapsed fractions not carried through smoothing/downsampling/simplification
//! - Elevation data range not stored or mismatched with source data
//! - Elapsed fractions not parallel to progress values in output

use super::super::elevation::prepare::build_elevation_geometry;
use super::super::types::NormalizedElevationPlot;

/// Builds a minimal `NormalizedElevationPlot` for testing.
///
/// All dimensions and style values are arbitrary — the test exercises
/// elapsed-fraction propagation, not rendering.
fn minimal_plot() -> NormalizedElevationPlot {
    NormalizedElevationPlot {
        x: 0.0,
        y: 0.0,
        width: 400,
        height: 200,
        rotation: 0.0,
        y_scale: 1.0,
        simplify_tolerance_px: 0.0,
        target_density: 2.0,
        remaining_line_width: 1.0,
        remaining_line_color: "#ffffff".into(),
        remaining_line_opacity: 1.0,
        remaining_line_shadow: None,
        completed_line_width: 2.0,
        completed_line_color: "#00ff00".into(),
        completed_line_opacity: 1.0,
        area_remaining_color: "#000000".into(),
        area_remaining_opacity: 0.3,
        area_completed_color: "#00ff00".into(),
        area_completed_opacity: 0.5,
        marker_variant: "circle".into(),
        marker_variant_diameter: 0.0,
        marker_size: 8.0,
        marker_color: "#ff0000".into(),
        marker_opacity: 1.0,
        show_elevation_metric: false,
        show_elevation_imperial: false,
        metric_label_offset_x: 0.0,
        metric_label_offset_y: 0.0,
        imperial_label_offset_x: 0.0,
        imperial_label_offset_y: 0.0,
        label_font: None,
        label_font_size: 14.0,
        label_color: "#ffffff".into(),
    }
}

#[test]
fn geometry_has_elapsed_fractions_parallel_to_progress() {
    let plot = minimal_plot();
    // 10 points: linear progress 0..1, linear elevation 100..200,
    // elapsed fractions 0.0, 0.1, ..., 0.9
    let raw_points: Vec<(f32, f64, f32)> = (0..10)
        .map(|i| {
            let progress = i as f32 / 9.0;
            let elevation = 100.0 + i as f64 * 10.0;
            let elapsed = i as f32 / 9.0;
            (progress, elevation, elapsed)
        })
        .collect();

    let geometry = build_elevation_geometry(&plot, &raw_points).unwrap();

    assert_eq!(
        geometry.elapsed_fractions.len(),
        geometry.points.len(),
        "elapsed_fractions must be parallel to points"
    );
    assert_eq!(
        geometry.elapsed_fractions.len(),
        geometry.progress_values.len(),
        "elapsed_fractions must be parallel to progress_values"
    );
    // Elapsed fractions should be monotonically non-decreasing
    for window in geometry.elapsed_fractions.windows(2) {
        assert!(
            window[0] <= window[1] + f32::EPSILON,
            "elapsed_fractions must be monotonically non-decreasing, got {:?}",
            geometry.elapsed_fractions
        );
    }
}

#[test]
fn geometry_elevation_data_range_matches_source() {
    let plot = minimal_plot();
    // Elevation ranges from 50.0 to 150.0
    let raw_points: Vec<(f32, f64, f32)> = (0..20)
        .map(|i| {
            let progress = i as f32 / 19.0;
            let elevation = 50.0 + i as f64 * (100.0 / 19.0);
            let elapsed = i as f32 / 19.0;
            (progress, elevation, elapsed)
        })
        .collect();

    let geometry = build_elevation_geometry(&plot, &raw_points).unwrap();

    let (min_elev, max_elev) = geometry.elevation_data_range.unwrap();
    assert!(
        (min_elev - 50.0).abs() < 1e-6,
        "min elevation should be 50.0, got {min_elev}"
    );
    assert!(
        (max_elev - 150.0).abs() < 1e-6,
        "max elevation should be 150.0, got {max_elev}"
    );
}

#[test]
fn geometry_elapsed_fractions_bounded_0_1() {
    let plot = minimal_plot();
    // Elapsed fractions that span a full 0..1 range
    let raw_points: Vec<(f32, f64, f32)> = (0..15)
        .map(|i| {
            let progress = i as f32 / 14.0;
            let elevation = 1000.0 + i as f64;
            let elapsed = i as f32 / 14.0;
            (progress, elevation, elapsed)
        })
        .collect();

    let geometry = build_elevation_geometry(&plot, &raw_points).unwrap();

    for &frac in &geometry.elapsed_fractions {
        assert!(
            frac >= -f32::EPSILON && frac <= 1.0 + f32::EPSILON,
            "elapsed_fraction {frac} out of [0, 1] range"
        );
    }
}
