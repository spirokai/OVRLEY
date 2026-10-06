//! G-force percentile, dense-axis, and frame-state tests.

use super::super::g_force::{derive_max_g, g_force_frame_state, prepare_g_force_cache};
use crate::activity::interpolate::densify_activity;
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::activity::trim::trim_activity;
use crate::debug::RenderProfiler;
use crate::normalize::{
    GForceAxis, RenderDataRequirements, ValidatedFfmpegConfig, ValidatedGForceWidget,
    ValidatedSceneConfig,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    activity: ParsedActivity,
    config: FixtureConfig,
    expected_max_g: f64,
}

#[derive(Deserialize)]
struct FixtureConfig {
    axis_horizontal: String,
    axis_vertical: String,
    clip_percentile: f32,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/g-force-frame-state.json"
    )))
    .unwrap()
}

fn axis(value: &str) -> GForceAxis {
    match value {
        "x" => GForceAxis::X,
        "y" => GForceAxis::Y,
        "z" => GForceAxis::Z,
        _ => panic!("fixture contains an invalid axis"),
    }
}

fn widget(config: &FixtureConfig) -> ValidatedGForceWidget {
    ValidatedGForceWidget {
        x: 0.0,
        y: 0.0,
        width: 220,
        height: 220,
        opacity: 1.0,
        diameter: 200.0,
        fill_color: "#212121".to_string(),
        fill_opacity: 0.5,
        border_thickness: 2.0,
        border_color: "#ffffff".to_string(),
        border_opacity: 1.0,
        marker_size: 12.0,
        marker_color: "#ffffff".to_string(),
        marker_opacity: 1.0,
        axis_horizontal: axis(&config.axis_horizontal),
        axis_vertical: axis(&config.axis_vertical),
        invert_horizontal: false,
        invert_vertical: false,
        clip_percentile: config.clip_percentile,
        label_font: "Arial.ttf".to_string(),
        label_font_size: 14.0,
        label_color: "#ffffff".to_string(),
        label_decimals: 1,
        label_unit: "G".to_string(),
        label_unit_color: "#ffffff".to_string(),
        label_offset_x: 0.0,
        label_offset_y: 0.0,
    }
}

fn scene() -> ValidatedSceneConfig {
    ValidatedSceneConfig {
        presentation: crate::normalize::ValidatedScenePresentation {
            width: 1920,
            height: 1080,
            scale: 1.0,
            font: None,
            font_size: None,
            opacity: 1.0,
            decimal_rounding: None,
            time_format: None,
            shadow_color: String::new(),
            shadow_strength: 0.0,
            shadow_distance: 0.0,
            border_color: String::new(),
            border_thickness: 0.0,
            overlay_filename: None,
        },
        fps: 1.0,
        start: 0.0,
        end: 5.0,
        custom_export_range_active: Some(false),
        update_rate: std::num::NonZeroU32::MIN,
        ffmpeg: ValidatedFfmpegConfig::default(),
        composite_video_path: None,
        quality: None,
        composite_sync_offset: None,
        composite_video_fps_num: None,
        composite_video_fps_den: None,
        composite_video_duration: None,
        composite_render_duration: None,
        composite_video_trim_start: None,
        composite_widget_update_rate: None,
    }
}

fn dense_activity(activity: &ParsedActivity) -> DenseActivityReport {
    let requirements = RenderDataRequirements {
        g_force_x: true,
        g_force_y: true,
        g_force_z: true,
        ..RenderDataRequirements::default()
    };
    let trimmed = trim_activity(activity, 0.0, 5.0, &requirements).unwrap();
    densify_activity(&trimmed, vec![0.0, 1.0, 2.0, 3.0, 4.0], &requirements)
}

#[test]
fn fixture_max_g_matches_nearest_rank_percentile_exactly() {
    let fixture = fixture();
    let widget = widget(&fixture.config);
    assert_eq!(
        derive_max_g(&fixture.activity, &widget),
        fixture.expected_max_g
    );

    let dense = dense_activity(&fixture.activity);
    let cache = prepare_g_force_cache(
        &widget,
        &scene(),
        &fixture.activity,
        &dense,
        &mut RenderProfiler::default(),
    )
    .unwrap();
    assert_eq!(cache.max_g, fixture.expected_max_g);
}

#[test]
fn known_clamped_missing_and_zero_frames_match_contract() {
    let fixture = fixture();
    let widget = widget(&fixture.config);
    let dense = dense_activity(&fixture.activity);
    let cache = prepare_g_force_cache(
        &widget,
        &scene(),
        &fixture.activity,
        &dense,
        &mut RenderProfiler::default(),
    )
    .unwrap();

    let state = |index| {
        g_force_frame_state(
            cache.horizontal_values[index],
            cache.vertical_values[index],
            cache.max_g,
            cache.center_x,
            cache.center_y,
            cache.radius,
            cache.label_decimals,
        )
    };

    let known = state(1);
    assert_eq!((known.marker_x, known.marker_y), (170.0, 190.0));
    assert_eq!(known.label, "5.0 G");

    let clamped = state(4);
    assert_eq!((clamped.marker_x, clamped.marker_y), (190.0, 170.0));
    assert_eq!(clamped.label, "10.0 G");

    let missing = state(3);
    assert_eq!((missing.marker_x, missing.marker_y), (110.0, 110.0));
    assert_eq!(missing.label, "--");

    let zero = state(0);
    assert_eq!((zero.marker_x, zero.marker_y), (110.0, 110.0));
    assert_eq!(zero.label, "0.0 G");
}

#[test]
fn invert_and_axis_remap_select_the_canonical_dense_series() {
    let fixture = fixture();
    let dense = dense_activity(&fixture.activity);

    let mut inverted_widget = widget(&fixture.config);
    inverted_widget.invert_horizontal = true;
    let inverted = prepare_g_force_cache(
        &inverted_widget,
        &scene(),
        &fixture.activity,
        &dense,
        &mut RenderProfiler::default(),
    )
    .unwrap();
    let inverted_state = g_force_frame_state(
        inverted.horizontal_values[1],
        inverted.vertical_values[1],
        inverted.max_g,
        inverted.center_x,
        inverted.center_y,
        inverted.radius,
        inverted.label_decimals,
    );
    assert_eq!(
        (inverted_state.marker_x, inverted_state.marker_y),
        (50.0, 190.0)
    );

    let mut remapped_widget = widget(&fixture.config);
    remapped_widget.axis_horizontal = GForceAxis::Y;
    remapped_widget.axis_vertical = GForceAxis::Z;
    let remapped = prepare_g_force_cache(
        &remapped_widget,
        &scene(),
        &fixture.activity,
        &dense,
        &mut RenderProfiler::default(),
    )
    .unwrap();
    assert_eq!(remapped.horizontal_values[1], Some(4.0));
    assert_eq!(remapped.vertical_values[1], Some(2.0));
}

#[test]
fn absent_selected_series_is_missing() {
    let state = g_force_frame_state(None, Some(2.0), 5.0, 110.0, 110.0, 100.0, 1);
    assert_eq!((state.marker_x, state.marker_y), (110.0, 110.0));
    assert_eq!(state.label, "--");

    let fixture = fixture();
    let mut activity = fixture.activity;
    activity.g_force_x.clear();
    let dense = dense_activity(&activity);
    let cache = prepare_g_force_cache(
        &widget(&fixture.config),
        &scene(),
        &activity,
        &dense,
        &mut RenderProfiler::default(),
    )
    .unwrap();
    assert!(cache.horizontal_values.is_empty());
}

#[test]
fn trim_boundary_preserves_an_explicit_null_axis_sample() {
    let fixture = fixture();
    let requirements = RenderDataRequirements {
        g_force_x: true,
        g_force_y: true,
        ..RenderDataRequirements::default()
    };
    let trimmed = trim_activity(&fixture.activity, 3.0, 5.0, &requirements).unwrap();
    let dense = densify_activity(&trimmed, vec![0.0, 1.0], &requirements);

    assert_eq!(dense.series.g_force_x, [None, Some(8.0)]);
    assert_eq!(dense.series.g_force_y, [Some(1.0), Some(6.0)]);
}
