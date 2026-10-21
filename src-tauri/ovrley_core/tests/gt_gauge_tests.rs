mod common;

use ovrley_core::activity::schema::DenseActivityReport;
use ovrley_core::fonts::NativeFonts;
use ovrley_core::normalize::{validate_render_config, RenderConfig};
use ovrley_widgets::model::types::{DisplayType, MetricKind, TrackFillStyle};
use ovrley_widgets::widgets::gauges::arc::{arc_point, gt_gauge_geometry, gt_start_end_angles};
use ovrley_widgets::widgets::gauges::frame::GaugeLayout;
use ovrley_widgets::widgets::gauges::validation::ValidatedGtGaugeStyle;
use ovrley_widgets::widgets::value::config::ValueConfig;
use ovrley_widgets::widgets::value::validation::ValidatedValue;
use skia_safe::{surfaces, Color};
use std::collections::BTreeMap;

#[test]
fn value_config_deserializes_gt_gauge_fields() {
    let value: ValueConfig = serde_json::from_value(full_gt_gauge_config()).unwrap();

    assert_eq!(value.display_type, DisplayType::GtGauge);
    assert_eq!(value.tick_count, Some(11));
    assert_eq!(value.tick_length, Some(14.0));
    assert_eq!(value.glow, Some(0.6));
}

#[test]
fn gt_track_opens_its_gap_on_the_right() {
    assert_eq!(gt_start_end_angles(270.0), (45.0, 315.0));

    let geometry = gt_gauge_geometry(200.0, 200.0, 270.0, 8.0, 0.0);
    assert_eq!(geometry.sweep_angle, 270.0);
    let start = arc_point(geometry.center_x, geometry.center_y, geometry.radius, 45.0);
    let end = arc_point(geometry.center_x, geometry.center_y, geometry.radius, 315.0);
    // Track starts bottom-right and ends top-right, mirrored about the horizontal axis.
    assert!(start.x > geometry.center_x && start.y > geometry.center_y);
    assert!((start.x - end.x).abs() < 0.001);
    assert!((start.y - geometry.center_y + (end.y - geometry.center_y)).abs() < 0.001);
}

#[test]
fn gt_style_is_validated_at_ingress() {
    let config = validate_single(full_gt_gauge_config()).unwrap();
    let ValidatedValue::ArcGauge(gauge) = &config.values[0] else {
        panic!("gt gauge should validate as an arc gauge");
    };
    assert_eq!(gauge.display_type, DisplayType::GtGauge);
    assert_eq!(
        gauge.gt_style,
        Some(ValidatedGtGaugeStyle {
            tick_count: 11,
            tick_length: 14.0,
            tick_color: "#8c8c8c".into(),
            glow: 0.6,
        })
    );

    for (field, value, message) in [
        ("tick_count", serde_json::Value::Null, "required"),
        ("tick_count", serde_json::json!(1), ">= 2"),
        ("tick_length", serde_json::json!(-1), ">= 0"),
        ("tick_color", serde_json::json!("grey"), "tick_color"),
        ("glow", serde_json::json!(1.5), "0.0..1.0"),
        ("glow", serde_json::Value::Null, "required"),
        ("arc_angle", serde_json::json!(400), "arc_angle"),
    ] {
        let mut invalid = full_gt_gauge_config();
        invalid[field] = value;
        let Err(error) = validate_single(invalid) else {
            panic!("invalid {field} should fail validation");
        };
        let error = error.to_string();
        assert!(
            error.contains(field) && error.contains(message),
            "{field}: {error}"
        );
    }
}

#[test]
fn gt_gauge_separates_unit_further_than_arc() {
    let gt = rendered_layout(full_gt_gauge_config());
    let mut arc_config = full_gt_gauge_config();
    arc_config["display_type"] = serde_json::json!("arc");
    let arc = rendered_layout(arc_config);
    let (GaugeLayout::Arc(gt), GaugeLayout::Arc(arc)) = (&gt.0, &arc.0) else {
        panic!("arc-shaped layouts");
    };
    assert_eq!(gt.geometry.start_angle, 45.0);
    assert_eq!(arc.geometry.start_angle, 135.0);
    assert!(gt.inner_widget_gap > arc.inner_widget_gap * 2.0);
    assert_eq!(arc.glow, 0.0);
}

#[test]
fn gt_gauge_always_renders_a_continuous_fill() {
    let mut value = full_gt_gauge_config();
    value["track_fill_style"] = serde_json::json!("bars");
    value["bar_count"] = serde_json::json!(12);
    value["bar_gap"] = serde_json::json!(3);
    let (GaugeLayout::Arc(layout), _, _) = rendered_layout(value) else {
        panic!("arc-shaped layout");
    };
    assert_eq!(layout.track_fill_style, TrackFillStyle::Fill);
    assert_eq!(layout.bar_geometry, None);
}

#[test]
fn gt_gauge_draws_static_ticks_and_a_fill_glow() {
    // Radius 96, ticks from 84 inward by 14px; index 5 of 11 lies at 180°.
    let tick_point = (120 - 77, 120);
    // Just outside the fully filled track's outer edge (radius 100).
    let glow_point = (120 - 103, 120);

    let mut with = render_full(full_gt_gauge_config());
    let tick = with.get_color(tick_point);
    assert!(tick.a() > 0 && tick.r() == tick.g() && tick.g() == tick.b());
    let halo = with.get_color(glow_point);
    assert!(halo.a() > 0 && halo.r() > halo.g());

    let mut plain = full_gt_gauge_config();
    plain["tick_length"] = serde_json::json!(0);
    plain["glow"] = serde_json::json!(0);
    let mut without = render_full(plain);
    assert_eq!(without.get_color(tick_point).a(), 0);
    assert_eq!(without.get_color(glow_point).a(), 0);
}

struct Rendered(skia_safe::Surface);

impl Rendered {
    fn get_color(&mut self, point: (i32, i32)) -> Color {
        self.0.peek_pixels().unwrap().get_color(point)
    }
}

fn render_full(value: serde_json::Value) -> Rendered {
    let (layout, state, font_dirs) = rendered_layout(value);
    let fonts = NativeFonts(&font_dirs);
    let frame = layout.prepare_frame(state, &fonts).unwrap();
    assert_eq!(frame.state.fill01, 1.0);
    let mut surface = surfaces::raster_n32_premul((260, 260)).unwrap();
    surface.canvas().clear(Color::TRANSPARENT);
    frame.draw(surface.canvas(), &layout);
    Rendered(surface)
}

fn rendered_layout(
    value: serde_json::Value,
) -> (
    GaugeLayout,
    ovrley_widgets::widgets::gauges::metric::GaugeFrameState,
    Vec<std::path::PathBuf>,
) {
    let config = validate_single(value).unwrap();
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let dense = dense_speed_activity(vec![Some(0.0), Some(100.0)]);
    let (layout, mut frame_states) = common::gauge::gauge_layout(&config, &dense, &NativeFonts(&font_dirs));
    (layout, frame_states.remove(1), font_dirs)
}

fn validate_single(
    value: serde_json::Value,
) -> Result<ovrley_core::normalize::ValidatedRenderConfig, ovrley_core::CoreError> {
    let mut scene = common::builders::scene_json();
    scene["scale"] = serde_json::json!(1);
    scene["shadow_strength"] = serde_json::json!(0);
    validate_render_config(RenderConfig {
        scene: serde_json::from_value(scene).unwrap(),
        backdrops: vec![],
        rasters: vec![],
        labels: vec![],
        values: vec![serde_json::from_value(value).unwrap()],
        plots: serde_json::Value::Object(serde_json::Map::new()),
        extra: BTreeMap::new(),
    })
}

fn full_gt_gauge_config() -> serde_json::Value {
    serde_json::json!({
        "value": "speed",
        "x": 20,
        "y": 20,
        "display_type": "gt_gauge",
        "width": 200,
        "height": 200,
        "rotation": 0,
        "arc_angle": 270,
        "tick_count": 11,
        "tick_length": 14,
        "tick_color": "#8c8c8c",
        "glow": 0.6,
        "inner_widget_offset_x": 0,
        "inner_widget_offset_y": 0,
        "track_thickness": 8,
        "track_corner_radius": 4,
        "track_border_thickness": 0,
        "track_border_color": "#ffffff",
        "track_empty_color": "#ffffff",
        "track_empty_opacity": 0.2,
        "track_filled_color": "#ff0a3c",
        "track_filled_opacity": 1,
        "track_fill_flat": false,
        "show_min_max_labels": false,
        "min_max_label_font": "Arial.ttf",
        "min_max_label_font_size": 12,
        "min_max_label_color": "#ffffff",
        "font": "Arial.ttf",
        "font_size": 40,
        "color": "#ffffff",
        "opacity": 1,
        "show_units": true,
        "unit_color": "#aaaaaa",
        "display_unit": "kmh",
        "prefix": "",
        "suffix": "",
        "decimals": 0
    })
}

fn dense_speed_activity(speed: Vec<Option<f64>>) -> DenseActivityReport {
    let frame_count = speed.len();
    let mut series = common::builders::empty_dense_series();
    series.speed = speed;
    DenseActivityReport {
        frame_count,
        frame_elapsed_seconds: (0..frame_count).map(|index| index as f64).collect(),
        frame_distance_progress: vec![Some(0.0); frame_count],
        full_activity_distance: None,
        full_activity_total_ascent: None,
        full_activity_metric_ranges: ovrley_core::activity::sampling::calculate_metric_range(
            &series.speed,
        )
        .map(|range| (MetricKind::Speed, range))
        .into_iter()
        .collect(),
        series,
    }
}
