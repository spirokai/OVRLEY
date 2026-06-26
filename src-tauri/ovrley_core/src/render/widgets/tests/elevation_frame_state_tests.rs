//! Elevation frame-state marker-y tests.
//!
//! Verifies that `build_elevation_frame_states` drives marker_y from
//! elapsed-time elevation data when `elevation_data_range` is available,
//! instead of relying solely on geometry polyline lookup.
//!
//! ## Type
//! Unit test (module-local). No I/O — pure frame-state computation.
//!
//! ## Regressions guarded
//! - Marker_y stuck at geometry y-coordinate during drone hover
//! - Marker_y not tracking elevation changes at same GPS position

use super::super::elevation::{build_elevation_completed_points, build_elevation_frame_states};
use super::super::types::{NormalizedElevationPlot, WidgetGeometry};
use crate::activity::schema::{DenseActivityReport, DenseSeriesReport, ParsedActivity};
use crate::normalize::ValidatedSceneConfig;
use std::collections::BTreeMap;

fn minimal_scene() -> ValidatedSceneConfig {
    ValidatedSceneConfig {
        fps: 1.0,
        start: 0.0,
        end: 3.0,
        width: 1920,
        height: 1080,
        scale: 1.0,
        font: None,
        font_size: None,
        opacity: None,
        decimal_rounding: None,
        time_format: None,
        custom_export_range_active: Some(false),
        shadow_color: String::new(),
        shadow_strength: 0.0,
        shadow_distance: 0.0,
        border_color: String::new(),
        border_thickness: 0.0,
        update_rate: 1,
        overlay_filename: None,
        ffmpeg: serde_json::Value::Null,
        composite_video_path: None,
        composite_bitrate: None,
        composite_sync_offset: None,
        composite_video_fps_num: None,
        composite_video_fps_den: None,
        composite_video_duration: None,
        composite_render_duration: None,
        composite_video_trim_start: None,
        composite_widget_update_rate: None,
    }
}

fn minimal_activity() -> ParsedActivity {
    ParsedActivity {
        file_name: None,
        file_format: None,
        metadata: serde_json::Value::Null,
        source_start_time: None,
        sample_elapsed_seconds: vec![0.0, 1.0, 2.0, 3.0],
        sample_distance_progress: vec![0.0, 0.25, 0.5, 1.0],
        frame_elapsed_seconds: Vec::new(),
        frame_timestamps: Vec::new(),
        frame_distance_progress: Vec::new(),
        trim_start_seconds: 0.0,
        trim_end_seconds: 3.0,
        sample_course_points: Vec::new(),
        sample_elevations: Vec::new(),
        course: Vec::new(),
        elevation: vec![Some(100.0), Some(150.0), Some(200.0), Some(250.0)],
        speed: Vec::new(),
        distance: Vec::new(),
        heartrate: Vec::new(),
        cadence: Vec::new(),
        power: Vec::new(),
        temperature: Vec::new(),
        pace: Vec::new(),
        g_force: Vec::new(),
        air_pressure: Vec::new(),
        ground_contact_time: Vec::new(),
        left_right_balance: Vec::new(),
        stride_length: Vec::new(),
        stroke_rate: Vec::new(),
        torque: Vec::new(),
        vertical_speed: Vec::new(),
        altitude: Vec::new(),
        iso: Vec::new(),
        aperture: Vec::new(),
        shutter_speed: Vec::new(),
        focal_length: Vec::new(),
        ev: Vec::new(),
        color_temperature: Vec::new(),
        gear_position: Vec::new(),
        vertical_ratio: Vec::new(),
        vertical_oscillation: Vec::new(),
        core_temperature: Vec::new(),
        gradient: Vec::new(),
        heading: Vec::new(),
        time: Vec::new(),
        extra: BTreeMap::new(),
    }
}

fn minimal_dense_activity() -> DenseActivityReport {
    DenseActivityReport {
        frame_count: 3,
        frame_elapsed_seconds: vec![0.0, 1.0, 2.0],
        frame_distance_progress: vec![Some(0.0), Some(0.5), Some(1.0)],
        full_activity_distance: None,
        series: DenseSeriesReport {
            speed: vec![None; 3],
            distance: vec![None; 3],
            elevation: vec![Some(100.0), Some(200.0), Some(300.0)],
            gradient: vec![None; 3],
            heartrate: vec![None; 3],
            cadence: vec![None; 3],
            power: vec![None; 3],
            temperature: vec![None; 3],
            pace: vec![None; 3],
            g_force: vec![None; 3],
            air_pressure: vec![None; 3],
            ground_contact_time: vec![None; 3],
            left_right_balance: vec![None; 3],
            stride_length: vec![None; 3],
            stroke_rate: vec![None; 3],
            torque: vec![None; 3],
            vertical_speed: vec![None; 3],
            altitude: vec![None; 3],
            iso: vec![None; 3],
            aperture: vec![None; 3],
            shutter_speed: vec![None; 3],
            focal_length: vec![None; 3],
            ev: vec![None; 3],
            color_temperature: vec![None; 3],
            gear_position: vec![None; 3],
            vertical_ratio: vec![None; 3],
            vertical_oscillation: vec![None; 3],
            core_temperature: vec![None; 3],
            heading: vec![None; 3],
            course_lat: vec![None; 3],
            course_lon: vec![None; 3],
            time: vec![None; 3],
        },
    }
}

fn vertical_segment_geometry() -> WidgetGeometry {
    // All points at the SAME y-coordinate — proving marker_y is driven by
    // elevation data, not geometry point lookup.
    WidgetGeometry {
        points: vec![(100.0, 400.0), (100.0, 400.0), (100.0, 400.0)],
        bbox: (0.0, 0.0, 200.0, 600.0),
        progress_values: vec![0.0, 0.5, 1.0],
        elapsed_fractions: vec![0.0, 0.5, 1.0],
        elevation_data_range: Some((100.0, 300.0)),
        source_point_count: 3,
        simplification: "test".into(),
    }
}

fn minimal_plot() -> NormalizedElevationPlot {
    NormalizedElevationPlot {
        x: 0.0,
        y: 0.0,
        width: 200,
        height: 600,
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

/// Marker_y must change across frames when elevation_data_range is set,
/// even though all geometry points share the same x-coordinate.
#[test]
fn marker_y_follows_elevation_during_hover() {
    let scene = minimal_scene();
    let activity = minimal_activity();
    let dense = minimal_dense_activity();
    let geometry = vertical_segment_geometry();
    let plot = minimal_plot();

    let states = build_elevation_frame_states(&scene, &activity, &dense, &geometry, &plot, true);

    assert_eq!(states.len(), 3, "should produce 3 frame states");

    // All marker_x values should be identical (vertical segment, same x)
    let marker_xs: Vec<f32> = states.iter().map(|s| s.marker_x).collect();
    assert!(
        marker_xs
            .windows(2)
            .all(|w| (w[0] - w[1]).abs() <= f32::EPSILON),
        "marker_x should be constant during hover, got {:?}",
        marker_xs
    );

    // marker_y values must differ because elevation changes across frames
    let marker_ys: Vec<f32> = states.iter().map(|s| s.marker_y).collect();
    assert!(
        marker_ys[0] != marker_ys[1] || marker_ys[1] != marker_ys[2],
        "marker_y should change during hover, got {:?}",
        marker_ys
    );

    // Elevation values must match the dense data
    assert_eq!(states[0].elevation_m, 100.0);
    assert_eq!(states[1].elevation_m, 200.0);
    assert_eq!(states[2].elevation_m, 300.0);
}

/// Completed profile fills a vertical segment chronologically — mid-hover
/// includes only the elapsed portion, not the entire segment.
///
/// Geometry: 5 points, all at progress=0.5, with elapsed_fractions
/// 0.0, 0.25, 0.5, 0.75, 1.0. At frame_elapsed_fraction=0.5, only
/// the first 3 points (elapsed <= 0.5) should be included.
#[test]
fn completed_points_fills_vertical_segment_chronologically() {
    let points = vec![
        (100.0, 500.0),
        (100.0, 450.0),
        (100.0, 400.0),
        (100.0, 350.0),
        (100.0, 300.0),
    ];
    let progress_values = vec![0.5, 0.5, 0.5, 0.5, 0.5];
    let elapsed_fractions = vec![0.0, 0.25, 0.5, 0.75, 1.0];

    // Mid-hover: elapsed_fraction = 0.5
    let completed =
        build_elevation_completed_points(&points, &progress_values, &elapsed_fractions, 0.5, 0.5);

    // Should include points with elapsed_fraction <= 0.5: indices 0, 1, 2
    assert!(
        completed.len() >= 3,
        "mid-hover should include at least 3 points, got {}",
        completed.len()
    );

    // First point must always be included
    assert_eq!(completed[0], points[0]);

    // The last point (elapsed=1.0) should NOT be included
    assert_ne!(
        *completed.last().unwrap(),
        points[4],
        "full-segment endpoint should not be in mid-hover completed"
    );
}

/// Completed profile at end of segment includes all points.
#[test]
fn completed_points_includes_all_at_full_elapsed() {
    let points = vec![
        (100.0, 500.0),
        (100.0, 450.0),
        (100.0, 400.0),
        (100.0, 350.0),
        (100.0, 300.0),
    ];
    let progress_values = vec![0.5, 0.5, 0.5, 0.5, 0.5];
    let elapsed_fractions = vec![0.0, 0.25, 0.5, 0.75, 1.0];

    let completed =
        build_elevation_completed_points(&points, &progress_values, &elapsed_fractions, 0.5, 1.0);

    // All 5 points should be included (plus marker if distant)
    assert!(
        completed.len() >= 5,
        "full elapsed should include all points, got {}",
        completed.len()
    );
}

/// Ordinary forward motion must preserve the underlying polyline prefix
/// instead of collapsing to a straight segment from the start point.
#[test]
fn completed_points_follow_geometry_prefix_outside_duplicate_run() {
    let points = vec![(0.0, 100.0), (30.0, 60.0), (70.0, 90.0), (100.0, 20.0)];
    let progress_values = vec![0.0, 0.3, 0.7, 1.0];
    let elapsed_fractions = vec![0.0, 0.3, 0.7, 1.0];

    let completed =
        build_elevation_completed_points(&points, &progress_values, &elapsed_fractions, 0.5, 0.5);

    assert_eq!(
        completed.len(),
        3,
        "should contain prefix plus interpolated endpoint"
    );
    assert_eq!(completed[0], points[0]);
    assert_eq!(completed[1], points[1]);
    assert!(
        (completed[2].0 - 50.0).abs() <= 1e-3 && (completed[2].1 - 75.0).abs() <= 1e-3,
        "expected interpolated geometry endpoint, got {:?}",
        completed[2]
    );
}

/// Duplicate-progress runs must keep the completed endpoint on the geometry,
/// not on the marker's independently projected y-coordinate.
#[test]
fn completed_points_use_geometry_endpoint_not_marker_y() {
    let points = vec![
        (100.0, 500.0),
        (100.0, 450.0),
        (100.0, 400.0),
        (100.0, 350.0),
        (100.0, 300.0),
    ];
    let progress_values = vec![0.5, 0.5, 0.5, 0.5, 0.5];
    let elapsed_fractions = vec![0.0, 0.25, 0.5, 0.75, 1.0];

    let completed =
        build_elevation_completed_points(&points, &progress_values, &elapsed_fractions, 0.5, 0.6);

    assert_eq!(
        *completed.last().unwrap(),
        (100.0, 380.0),
        "endpoint should interpolate within the geometry run"
    );
}
