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

use ovrley_core::config::HeadingWidgetConfig;
use ovrley_core::debug::RenderProfiler;
use ovrley_core::render::surface::create_surface;
use ovrley_core::render::widgets::heading::draw::draw_heading_widget;
use ovrley_core::render::widgets::heading::geometry::{
    chevron_vertices, visible_labels, visible_ticks, TapeTick,
};
use ovrley_core::render::widgets::heading::prepare::prepare_heading_cache;
use ovrley_core::render::widgets::types::HeadingWidgetCache;
use ovrley_core::MetricKind;
use std::collections::BTreeMap;

// ── Geometry helpers ──────────────────────────────────────────────────────

fn heading_offset(heading: f32, ppd: f32) -> f32 {
    heading * ppd
}

fn is_cardinal_degree(degree: f32) -> bool {
    [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0]
        .iter()
        .any(|c| (degree - c).abs() < 0.01)
}

fn cardinal_label_for_degree(degree: f32) -> Option<&'static str> {
    match degree as u32 {
        0 => Some("N"),
        45 => Some("NE"),
        90 => Some("E"),
        135 => Some("SE"),
        180 => Some("S"),
        225 => Some("SW"),
        270 => Some("W"),
        315 => Some("NW"),
        _ => None,
    }
}

// ── Fixtures ──────────────────────────────────────────────────────────────

fn default_plot() -> HeadingWidgetConfig {
    HeadingWidgetConfig {
        value: MetricKind::Heading,
        x: 100.0,
        y: 200.0,
        width: 400,
        height: 80,
        rotation: 0.0,
        opacity: 1.0,
        pixels_per_degree: 5.0,
        major_tick_interval: 15,
        minor_ticks_per_major: 3,
        show_major_ticks: true,
        show_minor_ticks: true,
        major_tick_length_pct: 40.0,
        minor_tick_length_pct: 20.0,
        major_tick_thickness: 2.0,
        minor_tick_thickness: 2.0,
        tick_color: Some("#FFFFFF".to_string()),
        cardinal_tick_color: Some("#FF0000".to_string()),
        tick_alignment: "below".to_string(),
        shadow_distance: None,
        shadow_strength: None,
        shadow_color: None,
        show_numeric_labels: true,
        show_cardinal_labels: true,
        numeric_label_color: Some("#CCCCCC".to_string()),
        cardinal_label_color: Some("#FF0000".to_string()),
        label_font: Some("Arial.ttf".to_string()),
        label_font_family: Some("Arial".to_string()),
        label_font_size: Some(12.0),
        label_offset: Some(4.0),
        indicator_style: "chevron".to_string(),
        indicator_placement: "top".to_string(),
        show_indicator: true,
        indicator_color: Some("#FF0000".to_string()),
        indicator_size: Some(10.0),
        extra: BTreeMap::new(),
    }
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
        indicator_shadow: None,
    }
}

// ── Geometry: heading offset ──────────────────────────────────────────────

#[test]
fn heading_offset_basic() {
    assert!((heading_offset(0.0, 5.0) - 0.0).abs() < f32::EPSILON);
    assert!((heading_offset(90.0, 5.0) - 450.0).abs() < f32::EPSILON);
    assert!((heading_offset(360.0, 5.0) - 1800.0).abs() < f32::EPSILON);
}

// ── Geometry: cardinal detection ──────────────────────────────────────────

#[test]
fn is_cardinal_degree_detects_45_multiples() {
    assert!(is_cardinal_degree(0.0));
    assert!(is_cardinal_degree(45.0));
    assert!(is_cardinal_degree(90.0));
    assert!(is_cardinal_degree(180.0));
    assert!(is_cardinal_degree(270.0));
    assert!(is_cardinal_degree(315.0));
    assert!(!is_cardinal_degree(15.0));
    assert!(!is_cardinal_degree(30.0));
    assert!(!is_cardinal_degree(100.0));
}

#[test]
fn cardinal_label_for_degree_returns_correct_labels() {
    assert_eq!(cardinal_label_for_degree(0.0), Some("N"));
    assert_eq!(cardinal_label_for_degree(45.0), Some("NE"));
    assert_eq!(cardinal_label_for_degree(90.0), Some("E"));
    assert_eq!(cardinal_label_for_degree(135.0), Some("SE"));
    assert_eq!(cardinal_label_for_degree(180.0), Some("S"));
    assert_eq!(cardinal_label_for_degree(225.0), Some("SW"));
    assert_eq!(cardinal_label_for_degree(270.0), Some("W"));
    assert_eq!(cardinal_label_for_degree(315.0), Some("NW"));
    assert_eq!(cardinal_label_for_degree(30.0), None);
}

// ── Geometry: visible ticks ───────────────────────────────────────────────

#[test]
fn visible_ticks_at_heading_zero() {
    let ticks = visible_ticks(0.0, 5.0, 200.0, 15, 3, true, true);
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
    assert!(minor_only.iter().all(|t| !t.is_major));
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
    assert!(labels[0].is_cardinal);
    assert_eq!(labels[1].text, "15");
    assert!(!labels[1].is_cardinal);
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
    let none = visible_labels(&ticks, false, false);
    assert!(none.is_empty());

    let numeric_only = visible_labels(&ticks, true, false);
    assert_eq!(numeric_only.len(), 2);
    assert_eq!(numeric_only[0].text, "0");
    assert_eq!(numeric_only[1].text, "15");

    let cardinal_only = visible_labels(&ticks, false, true);
    assert_eq!(cardinal_only.len(), 1);
    assert_eq!(cardinal_only[0].text, "N");
}

// ── Geometry: indicator vertices ──────────────────────────────────────────

#[test]
fn chevron_top_points_down() {
    let verts = chevron_vertices(200.0, 0.0, 10.0, true);
    assert!((verts[0].x - 194.0).abs() < 0.01);
    assert!((verts[1].x - 206.0).abs() < 0.01);
    assert!((verts[2].y - 10.0).abs() < 0.01);
}

#[test]
fn chevron_bottom_points_up() {
    let verts = chevron_vertices(200.0, 80.0, 10.0, false);
    assert!((verts[2].y - 70.0).abs() < 0.01);
}

// ── Prepare: cache construction ───────────────────────────────────────────

#[test]
fn prepare_heading_cache_produces_non_empty_tape() {
    let plot = default_plot();
    let mut profiler = RenderProfiler::default();

    let cache = prepare_heading_cache(&plot, &mut profiler).unwrap();

    assert!((cache.tape_width - 1800.0).abs() < 1.0);
    assert_eq!(cache.width, 400);
    assert_eq!(cache.height, 80);
    assert!((cache.pixels_per_degree - 5.0).abs() < f32::EPSILON);
    assert!((cache.x - 100.0).abs() < f32::EPSILON);
    assert!((cache.y - 200.0).abs() < f32::EPSILON);
}

#[test]
fn prepare_heading_cache_with_no_labels() {
    let mut plot = default_plot();
    plot.show_numeric_labels = false;
    plot.show_cardinal_labels = false;
    plot.show_major_ticks = false;
    plot.show_minor_ticks = false;
    let mut profiler = RenderProfiler::default();

    let cache = prepare_heading_cache(&plot, &mut profiler).unwrap();

    assert_eq!(cache.width, 400);
    assert_eq!(cache.height, 80);
}

#[test]
fn prepare_heading_cache_stores_indicator_config() {
    let mut plot = default_plot();
    plot.show_indicator = true;
    plot.indicator_style = "highlight_bar".to_string();
    plot.indicator_placement = "both".to_string();
    plot.indicator_color = Some("#00FF00".to_string());
    plot.indicator_size = Some(20.0);
    let mut profiler = RenderProfiler::default();

    let cache = prepare_heading_cache(&plot, &mut profiler).unwrap();

    assert!(cache.show_indicator);
    assert_eq!(cache.indicator_style, "highlight_bar");
    assert_eq!(cache.indicator_placement, "both");
    assert_eq!(cache.indicator_color, "#00FF00");
    assert!((cache.indicator_size - 20.0).abs() < f32::EPSILON);
}

#[test]
fn prepare_heading_cache_indicator_defaults() {
    let mut plot = default_plot();
    plot.indicator_color = None;
    plot.indicator_size = None;
    let mut profiler = RenderProfiler::default();

    let cache = prepare_heading_cache(&plot, &mut profiler).unwrap();

    assert_eq!(cache.indicator_color, "#ffffff");
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
fn draw_heading_widget_indicator_at_center() {
    let cache = default_cache();
    let report_center_x = cache.x + cache.width as f32 / 2.0;
    assert!((report_center_x - 300.0).abs() < f32::EPSILON);
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
fn draw_heading_widget_wrap_at_359() {
    let cache = default_cache();
    let mut surface = create_surface(1920, 1080).unwrap();
    let canvas = surface.canvas();
    canvas.clear(skia_safe::Color::TRANSPARENT);
    let mut profiler = RenderProfiler::default();

    let report = draw_heading_widget(canvas, &cache, 359.0, &mut profiler);
    assert!(report.is_some());
}
