//! G-force percentile, dense-axis, and frame-state tests.

use ovrley_core::activity::interpolate::densify_activity;
use ovrley_core::activity::schema::{DenseActivityReport, ParsedActivity};
use ovrley_core::activity::trim::trim_activity;
use ovrley_core::normalize::{RenderDataRequirements, ValidatedSceneConfig};
use ovrley_widgets::widgets::g_force::{
    format::{derive_max_g, g_force_frame_state, GForceSample},
    layout::{prepare_g_force_cache as prepare_g_force_cache_layout, GForceCache},
    validation::{GForceAxis, ValidatedGForceWidget},
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
        width: 220.0,
        height: 220.0,
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
        presentation: ovrley_widgets::model::scene::ValidatedScenePresentation {
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
            numerals: ovrley_widgets::model::scene::Numerals::Default,
            overlay_filename: None,
        },
        fps: 1.0,
        start: 0.0,
        end: 5.0,
        custom_export_range_active: Some(false),
        update_rate: std::num::NonZeroU32::MIN,
        export_start_seconds: 0.0,
        disabled_laps: Vec::new(),
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
        derive_max_g(
            &fixture.activity.g_force_x,
            &fixture.activity.g_force_y,
            widget.clip_percentile
        ),
        fixture.expected_max_g
    );

    let dense = dense_activity(&fixture.activity);
    let cache = prepare_g_force_cache(
        &widget,
        &scene(),
        &fixture.activity,
        &dense,
    );
    assert_eq!(cache.samples[0].max_g, fixture.expected_max_g);
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
    );

    let state = |index: usize| {
        g_force_frame_state(
            cache.samples[index].horizontal,
            cache.samples[index].vertical,
            cache.samples[0].max_g,
            cache.layout.layout.center_x,
            cache.layout.layout.center_y,
            cache.layout.layout.radius,
            widget.label_decimals,
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
    );
    let inverted_state = g_force_frame_state(
        inverted.samples[1].horizontal,
        inverted.samples[1].vertical,
        inverted.samples[0].max_g,
        inverted.layout.layout.center_x,
        inverted.layout.layout.center_y,
        inverted.layout.layout.radius,
        inverted_widget.label_decimals,
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
    );
    assert_eq!(remapped.samples[1].horizontal, Some(4.0));
    assert_eq!(remapped.samples[1].vertical, Some(2.0));
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
    );
    assert!(cache
        .samples
        .iter()
        .all(|sample| sample.horizontal.is_none()));
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

/// The widget crate's g-force layout and its sample at every dense frame.
struct GForceFrames {
    layout: GForceCache,
    samples: Vec<GForceSample>,
}

fn prepare_g_force_cache(
    widget: &ValidatedGForceWidget,
    scene: &ValidatedSceneConfig,
    activity: &ParsedActivity,
    dense: &DenseActivityReport,
) -> GForceFrames {
    let max_g = derive_max_g(
        activity.g_force_axis(widget.axis_horizontal),
        activity.g_force_axis(widget.axis_vertical),
        widget.clip_percentile,
    );
    let horizontal = dense.g_force_axis(widget.axis_horizontal);
    let vertical = dense.g_force_axis(widget.axis_vertical);
    GForceFrames {
        layout: prepare_g_force_cache_layout(widget, &scene.presentation).unwrap(),
        samples: (0..dense.frame_count)
            .map(|index| {
                GForceSample::new(
                    widget,
                    horizontal.get(index).copied().flatten(),
                    vertical.get(index).copied().flatten(),
                    max_g,
                )
            })
            .collect(),
    }
}
