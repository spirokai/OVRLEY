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

mod common;

use ovrley_core::debug::RenderProfiler;
use ovrley_core::normalize::raw::RenderConfig;
use ovrley_core::normalize::{
    validate_render_config, validate_scene_config, ValidatedSceneConfig,
};
use ovrley_core::render::surface::{create_surface, native_n32_image_info};
use ovrley_core::render::text::resolve_font;
use ovrley_core::render::widgets::heading::{draw_heading_widget, prepare_heading_cache};
use ovrley_widgets::widgets::heading::geometry::{
    chevron_vertices, heading_label_baseline, heading_tape_layout, heading_tick_position,
    visible_labels, visible_ticks, TapeTick,
};
use ovrley_widgets::widgets::heading::{layout::HeadingCache, validation::ValidatedHeading};
use ovrley_widgets::render::text::origin_x_for_centered_text;
use std::collections::BTreeMap;

// ── Geometry helpers ──────────────────────────────────────────────────────

// ── Fixtures ──────────────────────────────────────────────────────────────

fn default_heading() -> ValidatedHeading {
    ValidatedHeading {
        x: 100.0,
        y: 200.0,
        width: 400.0,
        height: 80.0,
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
        label_font: "Arial.ttf".to_string(),
        label_font_size: 12.0,
        label_offset: 4.0,
        show_indicator: true,
        indicator_style: "chevron".to_string(),
        indicator_placement: "top".to_string(),
        indicator_color: "#FF0000".to_string(),
        indicator_size: 10.0,
        opacity: 1.0,
    }
}

fn default_render_config() -> RenderConfig {
    let scene: ovrley_core::normalize::raw::SceneConfig =
        serde_json::from_value(common::builders::scene_json()).unwrap();
    let config = RenderConfig {
        scene,
        backdrops: vec![],
        rasters: vec![],
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

fn default_cache() -> HeadingCache {
    prepare_heading_cache(&default_validated_scene(), &default_heading(),
        &[common::test_config::repo_git_root().join("fonts")], &mut RenderProfiler::default()).unwrap()
}

fn heading_pixels(cache: &HeadingCache, heading: f32) -> Vec<u8> {
    let width = cache.frame.width().ceil() as u32;
    let height = cache.frame.height().ceil() as u32;
    let mut surface = create_surface(width, height).unwrap();
    let canvas = surface.canvas();
    canvas.clear(skia_safe::Color::TRANSPARENT);
    canvas.translate((-cache.frame.left, -cache.frame.top));
    draw_heading_widget(canvas, cache, heading, &mut RenderProfiler::default());
    let mut pixels = vec![0; width as usize * height as usize * 4];
    assert!(surface.read_pixels(&native_n32_image_info(width, height), &mut pixels, width as usize * 4, (0, 0)));
    pixels
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

#[test]
fn centered_heading_labels_use_tick_center_as_visual_center() {
    let workspace_root = common::test_config::repo_git_root();
    let font_dirs = vec![workspace_root.join("fonts")];
    let font = resolve_font(&font_dirs, Some("JetBrains Mono.ttf"), 36.0).unwrap();
    let center_x = 120.0;
    let text = "NE";

    let (_, naive_bounds) = font.measure_str(text, None);
    let naive_center = center_x + (naive_bounds.left + naive_bounds.right) * 0.5;
    assert!(
        (naive_center - center_x).abs() > 0.1,
        "naive draw_str x should not already center glyph bounds"
    );

    let draw_x = origin_x_for_centered_text(text, center_x, &font);
    let (_, bounds) = font.measure_str(text, None);
    let rendered_center = draw_x + (bounds.left + bounds.right) * 0.5;

    assert!(
        (rendered_center - center_x).abs() < 0.001,
        "expected centered bounds at {center_x}, got {rendered_center}"
    );
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
fn heading_tape_layout_adds_only_visible_chevron_slots() {
    let top = heading_tape_layout(80.0, true, "chevron", "top", 10.0, 40.0, 4.0, 12.0);
    assert!((top.body_y - 15.0).abs() < f32::EPSILON);
    assert!((top.body_height - 65.0).abs() < f32::EPSILON);
    assert!((top.tick_scale_height - 115.0).abs() < 0.001);
    assert!((top.total_height - 80.0).abs() < f32::EPSILON);
    assert!(top.has_top_chevron);
    assert!(!top.has_bottom_chevron);

    let bottom = heading_tape_layout(80.0, true, "chevron", "bottom", 10.0, 40.0, 4.0, 12.0);
    assert!((bottom.body_y - 0.0).abs() < f32::EPSILON);
    assert!((bottom.body_height - 65.0).abs() < f32::EPSILON);
    assert!((bottom.tick_scale_height - 115.0).abs() < 0.001);
    assert!((bottom.total_height - 80.0).abs() < f32::EPSILON);
    assert!(!bottom.has_top_chevron);
    assert!(bottom.has_bottom_chevron);

    let both = heading_tape_layout(80.0, true, "chevron", "both", 10.0, 40.0, 4.0, 12.0);
    assert!((both.body_y - 15.0).abs() < f32::EPSILON);
    assert!((both.body_height - 50.0).abs() < f32::EPSILON);
    assert!((both.tick_scale_height - 77.5).abs() < 0.001);
    assert!((both.total_height - 80.0).abs() < f32::EPSILON);
}

#[test]
fn heading_tape_layout_adds_body_margin_for_highlight_bars() {
    let layout = heading_tape_layout(80.0, true, "highlight_bar", "both", 10.0, 40.0, 4.0, 12.0);

    assert!((layout.body_y - 8.0).abs() < f32::EPSILON);
    assert!((layout.body_height - 64.0).abs() < f32::EPSILON);
    assert!((layout.tick_scale_height - 112.5).abs() < 0.001);
    assert!((layout.total_height - 80.0).abs() < f32::EPSILON);
    assert!(!layout.has_top_chevron);
    assert!(!layout.has_bottom_chevron);
}

#[test]
fn heading_tick_alignment_affects_minor_ticks_only() {
    let major = heading_tick_position(80.0, 40.0, 20.0, "centered", true);
    let minor_centered = heading_tick_position(80.0, 40.0, 20.0, "centered", false);
    let minor_below = heading_tick_position(80.0, 40.0, 20.0, "below", false);

    assert!((major.0 - 0.0).abs() < f32::EPSILON);
    assert!((major.1 - 32.0).abs() < f32::EPSILON);
    assert!((minor_centered.0 - 8.0).abs() < f32::EPSILON);
    assert!((minor_centered.1 - 16.0).abs() < f32::EPSILON);
    assert!((minor_below.0 - 0.0).abs() < f32::EPSILON);
    assert!((minor_below.1 - 16.0).abs() < f32::EPSILON);
}

#[test]
fn heading_label_baseline_uses_fixed_offset_from_major_tick_bottom() {
    let baseline = heading_label_baseline(80.0, 40.0, 4.0, 12.0);
    assert!((baseline - 48.0).abs() < f32::EPSILON);
}

#[test]
fn prepare_heading_cache_produces_non_empty_tape() {
    let heading = default_heading();
    let mut profiler = RenderProfiler::default();

    let cache = prepare_heading_cache(
        &default_validated_scene(),
        &heading,
        &[common::test_config::repo_git_root().join("fonts")],
        &mut profiler,
    )
    .unwrap();

    assert!((cache.tape_width - 1800.0).abs() < 1.0);
    assert_eq!(cache.frame.width(), 400.0);
    assert_eq!(cache.frame.height(), 80.0);
    assert!((cache.tape_body_y - 15.0).abs() < f32::EPSILON);
    assert!((cache.tape_body_height - 65.0).abs() < f32::EPSILON);
    assert!((cache.pixels_per_degree - 5.0).abs() < f32::EPSILON);
    assert!((cache.frame.left - 100.0).abs() < f32::EPSILON);
    assert!((cache.frame.top - 200.0).abs() < f32::EPSILON);
}

#[test]
fn prepare_heading_cache_with_no_labels() {
    let mut heading = default_heading();
    heading.show_minor_labels = false;
    heading.show_major_labels = false;
    heading.show_major_ticks = false;
    heading.show_minor_ticks = false;
    let mut profiler = RenderProfiler::default();

    let cache = prepare_heading_cache(
        &default_validated_scene(),
        &heading,
        &[common::test_config::repo_git_root().join("fonts")],
        &mut profiler,
    )
    .unwrap();

    assert_eq!(cache.frame.width(), 400.0);
    assert_eq!(cache.frame.height(), 80.0);
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

    let cache = prepare_heading_cache(
        &default_validated_scene(),
        &heading,
        &[common::test_config::repo_git_root().join("fonts")],
        &mut profiler,
    )
    .unwrap();

    assert!((cache.prepare_frame(Some(0.0)).content_bounds.height() - 80.0).abs() < f32::EPSILON);
    assert!((cache.tape_body_y - 8.0).abs() < f32::EPSILON);
    assert!((cache.tape_body_height - 64.0).abs() < f32::EPSILON);

    for style in ["highlight_bar", "chevron"] {
        heading.indicator_style = style.to_string();
        let cache = prepare_heading_cache(
            &default_validated_scene(), &heading,
            &[common::test_config::repo_git_root().join("fonts")], &mut profiler,
        ).unwrap();
        let pixels = heading_pixels(&cache, 0.0);
        // Ignore the centre indicator and faded sides when measuring the painted tape.
        let rows: Vec<_> = (0..80).filter(|y| (50..350).filter(|x| !(180..220).contains(x))
            .any(|x| pixels[(y * 400 + x) * 4 + 3] != 0)).collect();
        let centre = (rows[0] + rows[rows.len() - 1]) as f32 * 0.5;
        assert!((centre - 39.5).abs() <= 1.0, "{style}: painted tape is vertically centred, got {centre}");
    }
}

#[test]
fn prepare_heading_cache_indicator_size_pre_scaled() {
    let mut heading = default_heading();
    heading.indicator_size = 10.0;
    let mut profiler = RenderProfiler::default();

    let cache = prepare_heading_cache(
        &default_validated_scene(),
        &heading,
        &[common::test_config::repo_git_root().join("fonts")],
        &mut profiler,
    )
    .unwrap();

    // With scale=1.0, indicator_size is unchanged
    assert!((cache.tape_body_y - 15.0).abs() < f32::EPSILON);
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
fn north_tick_keeps_its_full_stroke_while_crossing_the_repeat_seam() {
    let mut heading = default_heading();
    heading.show_indicator = false;
    heading.show_major_ticks = false;
    heading.show_minor_ticks = false;
    heading.major_tick_thickness = 4.0;
    for (pixels_per_degree, scale) in [(5.0, 1.0), (5.001, 1.3)] {
        heading.pixels_per_degree = pixels_per_degree;
        let mut scene = default_validated_scene();
        scene.presentation.scale = scale;
        let cache = prepare_heading_cache(&scene, &heading,
            &[common::test_config::repo_git_root().join("fonts")], &mut RenderProfiler::default()).unwrap();
        let width = cache.frame.width().ceil() as usize;
        let centre = width / 2;
        let tick_alpha = |degrees| {
            let pixels = heading_pixels(&cache, degrees);
            (centre - 10..centre + 10).map(|x| pixels[(10 * width + x) * 4 + 3] as i32).sum::<i32>()
        };
        for degrees in [359.9, 359.95, 0.0, 0.05, 0.1] {
            let north = tick_alpha(degrees);
            let east = tick_alpha((degrees + 90.0) % 360.0);
            let expected = 4.0 * scale * 255.0;
            assert!((north as f32 - expected).abs() <= 4.0, "north lost stroke coverage at {degrees}: {north}");
            assert!((north - east).abs() <= 4, "north differs from east at {degrees}: {north}/{east}");
        }
    }
}
