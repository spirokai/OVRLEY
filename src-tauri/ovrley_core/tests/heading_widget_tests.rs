//! Heading widget unit tests.
//!
//! Verifies tick/label geometry, cache preparation, and per-frame drawing
//! for the heading compass tape widget.
//!
//! ## Type
//! Unit test. Pure struct construction and Skia surface rendering — no I/O, no fixtures.
//!
//! ## Regressions guarded
//! - Tick position calculation at various headings and scales
//! - Cardinal label priority override
//! - Indicator vertex geometry for chevron and highlight bar
//! - Cache preparation producing correct dimensions and indicator config
//! - Per-frame drawing producing a valid render report

use ovrley_core::debug::RenderProfiler;
use ovrley_core::normalize::raw::RenderConfig;
use ovrley_core::normalize::{
    validate_render_config, validate_scene_config, ValidatedHeading, ValidatedSceneConfig,
};
use ovrley_core::render::surface::create_surface;
use ovrley_core::render::widgets::heading::draw::draw_heading_widget;
use ovrley_core::render::widgets::heading::geometry::{
    chevron_vertices, visible_labels, visible_ticks, TapeTick,
};
use ovrley_core::render::widgets::heading::prepare::prepare_heading_cache;
use ovrley_core::render::widgets::types::HeadingWidgetCache;
use ovrley_core::types::DisplayType;
use std::collections::BTreeMap;

// ── Geometry helpers ──────────────────────────────────────────────────────

// ── Fixtures ──────────────────────────────────────────────────────────────

fn default_heading() -> ValidatedHeading {
    ValidatedHeading {
        x: 100.0,
        y: 200.0,
        width: 400,
        height: 80,
        pixels_per_degree: 5.0,
        major_tick_interval: 15,
        minor_ticks_per_major: 3,
        show_major_ticks: true,
        show_minor_ticks: true,
        major_tick_length_pct: 40.0,
        minor_tick_length_pct: 20.0,
        major_tick_thickness: 2.0,
        minor_tick_thickness: 2.0,
        tick_color: "#FFFFFF".to_string(),
        cardinal_tick_color: "#FF0000".to_string(),
        tick_alignment: "below".to_string(),
        show_minor_labels: true,
        show_major_labels: true,
        label_color: "#CCCCCC".to_string(),
        cardinal_label_color: "#FF0000".to_string(),
        label_font: Some("Arial.ttf".to_string()),
        label_font_size: 12.0,
        label_offset: 4.0,
        show_indicator: true,
        indicator_style: "chevron".to_string(),
        indicator_placement: "top".to_string(),
        indicator_color: "#FF0000".to_string(),
        indicator_size: 10.0,
        indicator_shadow: None,
        rotation: 0.0,
        opacity: 1.0,
    }
}

fn default_render_config() -> RenderConfig {
    let scene: ovrley_core::normalize::raw::SceneConfig =
        serde_json::from_value(serde_json::json!({
            "fps": 30.0,
            "start": 0.0,
            "end": 10.0,
            "width": 1920,
            "height": 1080,
            "scale": 1.0,
            "shadow_strength": 0.0,
            "shadow_distance": 0.0,
            "shadow_color": "#000000",
            "border_thickness": 0.0,
            "border_color": "#000000",
            "update_rate": 1,
            "custom_export_range_active": false,
            "composite_sync_offset": 0.0,
            "composite_video_trim_start": 0.0,
            "composite_widget_update_rate": 1
        }))
        .unwrap();
    let config = RenderConfig {
        scene,
        labels: vec![],
        values: vec![],
        plots: serde_json::Value::Object(serde_json::Map::new()),
        extra: BTreeMap::new(),
    };
    validate_render_config(config.clone()).unwrap();
    config
}

fn default_validated_scene() -> ValidatedSceneConfig {
    let config = default_render_config();
    validate_scene_config(config.scene).unwrap()
}

fn default_cache() -> HeadingWidgetCache {
    HeadingWidgetCache {
        tape_image: {
            let mut surface = create_surface(1800, 80).unwrap();
            surface.canvas().clear(skia_safe::Color::TRANSPARENT);
            surface.image_snapshot()
        },
        tape_width: 1800.0,
        x: 100.0,
        y: 200.0,
        width: 400,
        height: 80,
        pixels_per_degree: 5.0,
        show_indicator: true,
        indicator_style: "chevron".to_string(),
        indicator_placement: "top".to_string(),
        indicator_color: "#FF0000".to_string(),
        indicator_size: 10.0,
        display_type: DisplayType::Tape,
        indicator_shadow: None,
    }
}

// ── Geometry: visible ticks ───────────────────────────────────────────────

#[test]
fn visible_ticks_at_heading_zero() {
    let ticks = visible_ticks(0.0, 5.0, 200.0, 15, 3, true, true);
    assert!(!ticks.is_empty());
    assert_eq!(ticks[0].degree, 0.0);
    assert!((ticks[0].x - 0.0).abs() < 0.01);
    assert!(ticks.iter().any(|t| (t.degree - 5.0).abs() < 0.01));
    assert!(!ticks.iter().any(|t| (t.degree - 40.0).abs() < 0.01));
}

#[test]
fn visible_ticks_wraps_at_360() {
    let ticks = visible_ticks(350.0, 5.0, 100.0, 15, 3, true, true);
    let tick_0 = ticks.iter().find(|t| t.degree.abs() < 0.01);
    assert!(tick_0.is_some());
    assert!((tick_0.unwrap().x - 50.0).abs() < 0.01);
}

#[test]
fn visible_ticks_respects_show_flags() {
    let all = visible_ticks(0.0, 5.0, 200.0, 15, 3, true, true);
    let major_only = visible_ticks(0.0, 5.0, 200.0, 15, 3, true, false);
    let minor_only = visible_ticks(0.0, 5.0, 200.0, 15, 3, false, true);

    assert!(all.len() > major_only.len());
    assert!(all.len() > minor_only.len());
    assert!(major_only.iter().all(|t| t.is_major));
    // Cardinal ticks are always shown and marked as major; non-cardinal
    // ticks in the minor-only set must not be major.
    assert!(minor_only
        .iter()
        .filter(|t| !t.is_cardinal)
        .all(|t| !t.is_major));
}

#[test]
fn visible_ticks_marks_cardinals() {
    let ticks = visible_ticks(0.0, 5.0, 200.0, 15, 3, true, false);
    let tick_0 = ticks.iter().find(|t| t.degree.abs() < 0.01).unwrap();
    assert!(tick_0.is_cardinal);
    let tick_15 = ticks
        .iter()
        .find(|t| (t.degree - 15.0).abs() < 0.01)
        .unwrap();
    assert!(!tick_15.is_cardinal);
}

// ── Geometry: visible labels ──────────────────────────────────────────────

#[test]
fn visible_labels_cardinal_overrides_numeric() {
    let ticks = vec![
        TapeTick {
            degree: 0.0,
            x: 0.0,
            is_cardinal: true,
            is_major: true,
        },
        TapeTick {
            degree: 15.0,
            x: 75.0,
            is_cardinal: false,
            is_major: true,
        },
        TapeTick {
            degree: 30.0,
            x: 150.0,
            is_cardinal: false,
            is_major: true,
        },
    ];
    let labels = visible_labels(&ticks, true, true);
    assert_eq!(labels.len(), 3);
    assert_eq!(labels[0].text, "N");
    assert!(labels[0].is_major_label);
    assert_eq!(labels[1].text, "15");
    assert!(!labels[1].is_major_label);
    assert_eq!(labels[2].text, "30");
}

#[test]
fn visible_labels_respects_show_flags() {
    let ticks = vec![
        TapeTick {
            degree: 0.0,
            x: 0.0,
            is_cardinal: true,
            is_major: true,
        },
        TapeTick {
            degree: 15.0,
            x: 75.0,
            is_cardinal: false,
            is_major: true,
        },
    ];
    // Cardinal labels are always shown regardless of show flags.
    let none = visible_labels(&ticks, false, false);
    assert_eq!(none.len(), 1); // only "N" (cardinal is always shown)
    assert_eq!(none[0].text, "N");

    // show_minor_labels=true, show_major_labels=false:
    // - Cardinal (0°) → "N" (always)
    // - Non-cardinal major (15°) → not shown (not minor, major labels off)
    let minor_only = visible_labels(&ticks, true, false);
    assert_eq!(minor_only.len(), 1);
    assert_eq!(minor_only[0].text, "N");

    // show_minor_labels=false, show_major_labels=true:
    // - Cardinal (0°) → "N" (always)
    // - Non-cardinal major (15°) → "15" (major label)
    let major_only = visible_labels(&ticks, false, true);
    assert_eq!(major_only.len(), 2);
    assert_eq!(major_only[0].text, "N");
    assert_eq!(major_only[1].text, "15");
}

// ── Geometry: chevron vertices ────────────────────────────────────────────

#[test]
fn chevron_vertices_pointing_down() {
    let verts = chevron_vertices(100.0, 0.0, 20.0, true);
    assert_eq!(verts.len(), 3);
    // Base left
    assert!((verts[0].x - 88.0).abs() < f32::EPSILON);
    assert!((verts[0].y - 0.0).abs() < f32::EPSILON);
    // Base right
    assert!((verts[1].x - 112.0).abs() < f32::EPSILON);
    assert!((verts[1].y - 0.0).abs() < f32::EPSILON);
    // Apex (pointing down)
    assert!((verts[2].x - 100.0).abs() < f32::EPSILON);
    assert!((verts[2].y - 20.0).abs() < f32::EPSILON);
}

#[test]
fn chevron_vertices_pointing_up() {
    let verts = chevron_vertices(100.0, 100.0, 20.0, false);
    assert_eq!(verts.len(), 3);
    // Base left
    assert!((verts[0].x - 88.0).abs() < f32::EPSILON);
    assert!((verts[0].y - 100.0).abs() < f32::EPSILON);
    // Base right
    assert!((verts[1].x - 112.0).abs() < f32::EPSILON);
    assert!((verts[1].y - 100.0).abs() < f32::EPSILON);
    // Apex (pointing up)
    assert!((verts[2].x - 100.0).abs() < f32::EPSILON);
    assert!((verts[2].y - 80.0).abs() < f32::EPSILON);
}

// ── Prepare: cache construction ───────────────────────────────────────────

#[test]
fn prepare_heading_cache_produces_non_empty_tape() {
    let heading = default_heading();
    let mut profiler = RenderProfiler::default();

    let cache =
        prepare_heading_cache(&default_validated_scene(), &heading, &[], &mut profiler).unwrap();

    assert!((cache.tape_width - 1800.0).abs() < 1.0);
    assert_eq!(cache.width, 400);
    assert_eq!(cache.height, 80);
    assert!((cache.pixels_per_degree - 5.0).abs() < f32::EPSILON);
    assert!((cache.x - 100.0).abs() < f32::EPSILON);
    assert!((cache.y - 200.0).abs() < f32::EPSILON);
}

#[test]
fn prepare_heading_cache_with_no_labels() {
    let mut heading = default_heading();
    heading.show_minor_labels = false;
    heading.show_major_labels = false;
    heading.show_major_ticks = false;
    heading.show_minor_ticks = false;
    let mut profiler = RenderProfiler::default();

    let cache =
        prepare_heading_cache(&default_validated_scene(), &heading, &[], &mut profiler).unwrap();

    assert_eq!(cache.width, 400);
    assert_eq!(cache.height, 80);
}

#[test]
fn prepare_heading_cache_stores_indicator_config() {
    let mut heading = default_heading();
    heading.show_indicator = true;
    heading.indicator_style = "highlight_bar".to_string();
    heading.indicator_placement = "both".to_string();
    heading.indicator_color = "#00FF00".to_string();
    heading.indicator_size = 20.0;
    let mut profiler = RenderProfiler::default();

    let cache =
        prepare_heading_cache(&default_validated_scene(), &heading, &[], &mut profiler).unwrap();

    assert!(cache.show_indicator);
    assert_eq!(cache.indicator_style, "highlight_bar");
    assert_eq!(cache.indicator_placement, "both");
    assert_eq!(cache.indicator_color, "#00FF00");
    assert!((cache.indicator_size - 20.0).abs() < f32::EPSILON);
}

#[test]
fn prepare_heading_cache_indicator_size_pre_scaled() {
    let mut heading = default_heading();
    heading.indicator_size = 10.0;
    let mut profiler = RenderProfiler::default();

    let cache =
        prepare_heading_cache(&default_validated_scene(), &heading, &[], &mut profiler).unwrap();

    // With scale=1.0, indicator_size is unchanged
    assert!((cache.indicator_size - 10.0).abs() < f32::EPSILON);
}

// ── Draw: per-frame rendering ─────────────────────────────────────────────

#[test]
fn draw_heading_widget_with_chevron_indicator() {
    let cache = default_cache();
    let mut surface = create_surface(1920, 1080).unwrap();
    let canvas = surface.canvas();
    canvas.clear(skia_safe::Color::TRANSPARENT);
    let mut profiler = RenderProfiler::default();

    let report = draw_heading_widget(canvas, &cache, 90.0, &mut profiler);

    assert!(report.is_some());
    let report = report.unwrap();
    assert_eq!(report.geometry.widget_width, 400);
    assert_eq!(report.geometry.widget_height, 80);
}

#[test]
fn draw_heading_widget_with_highlight_bar() {
    let mut cache = default_cache();
    cache.indicator_style = "highlight_bar".to_string();
    cache.indicator_placement = "both".to_string();
    let mut surface = create_surface(1920, 1080).unwrap();
    let canvas = surface.canvas();
    canvas.clear(skia_safe::Color::TRANSPARENT);
    let mut profiler = RenderProfiler::default();

    let report = draw_heading_widget(canvas, &cache, 45.0, &mut profiler);

    assert!(report.is_some());
}

#[test]
fn draw_heading_widget_indicator_hidden() {
    let mut cache = default_cache();
    cache.show_indicator = false;
    let mut surface = create_surface(1920, 1080).unwrap();
    let canvas = surface.canvas();
    canvas.clear(skia_safe::Color::TRANSPARENT);
    let mut profiler = RenderProfiler::default();

    let report = draw_heading_widget(canvas, &cache, 0.0, &mut profiler);

    assert!(report.is_some());
}

#[test]
fn draw_heading_widget_chevron_placement_both() {
    let mut cache = default_cache();
    cache.indicator_placement = "both".to_string();
    let mut surface = create_surface(1920, 1080).unwrap();
    let canvas = surface.canvas();
    canvas.clear(skia_safe::Color::TRANSPARENT);
    let mut profiler = RenderProfiler::default();

    let report = draw_heading_widget(canvas, &cache, 180.0, &mut profiler);

    assert!(report.is_some());
}

#[test]
fn draw_heading_widget_wrap_at_360() {
    let cache = default_cache();
    let mut surface = create_surface(1920, 1080).unwrap();
    let canvas = surface.canvas();
    canvas.clear(skia_safe::Color::TRANSPARENT);
    let mut profiler = RenderProfiler::default();

    // Heading near 360 should wrap seamlessly
    let report = draw_heading_widget(canvas, &cache, 359.0, &mut profiler);
    assert!(report.is_some());
}

#[test]
fn draw_heading_widget_at_zero() {
    let cache = default_cache();
    let mut surface = create_surface(1920, 1080).unwrap();
    let canvas = surface.canvas();
    canvas.clear(skia_safe::Color::TRANSPARENT);
    let mut profiler = RenderProfiler::default();

    let report = draw_heading_widget(canvas, &cache, 0.0, &mut profiler);
    assert!(report.is_some());
}
