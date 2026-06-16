//! DisplayType-driven metric presentation dispatch tests.
//!
//! Verifies that [`draw_metric_presentation`] correctly dispatches metric
//! widget rendering based on `DisplayType`:
//! - `Text` returns `None` (intrinsic text is handled by the value module)
//! - `Tape` with a heading cache and Heading metric draws successfully
//! - `Tape` without a cache returns `None`
//! - `Tape` with a Text-mode cache returns `None` (presentation switched away)
//! - `Tape` with a non-Heading metric returns `None`
//! - Future boxed display types (`Linear`, `Bars`, `Arc`, `Corner`) return `None`
//!
//! ## Type
//! Integration test. Uses the public `ovrley_core::render::widgets::metric_presentation`
//! API with Skia surface rendering.
//!
//! ## Regressions guarded
//! - Heading tape not rendering through the metric presentation pipeline
//! - DisplayType dispatch returning wrong variant for Text
//! - Future display types crashing instead of returning None

mod common;

use ovrley_core::activity::schema::{DenseActivityReport, DenseSeriesReport};
use ovrley_core::debug::RenderProfiler;
use ovrley_core::normalize::raw::ValueConfig;
use ovrley_core::render::surface::create_surface;
use ovrley_core::render::text::ResolvedTextStyle;
use ovrley_core::render::widgets::metric_presentation::draw_metric_presentation;
use ovrley_core::render::widgets::types::{HeadingWidgetCache, PresentationCache};
use ovrley_core::types::{DisplayType, MetricKind};
use skia_safe::Color;
use std::collections::BTreeMap;

// ── Fixtures ──────────────────────────────────────────────────────────────

fn default_value_config(display_type: DisplayType) -> ValueConfig {
    ValueConfig {
        value: MetricKind::Heading,
        x: 100.0,
        y: 200.0,
        font: None,
        font_family: None,
        font_size: None,
        color: None,
        opacity: None,
        suffix: None,
        prefix: None,
        unit: None,
        hours_offset: None,
        time_format: None,
        format: None,
        decimal_rounding: None,
        decimals: None,
        show_icon: None,
        icon_color: None,
        icon_size: None,
        icon_offset_x: None,
        icon_offset_y: None,
        show_units: None,
        unit_color: None,
        display_unit: None,
        balance_format: None,
        value_offset: None,
        triangle_positive_color: None,
        triangle_negative_color: None,
        show_sign: None,
        show_triangle: None,
        triangle_width: None,
        shadow_color: None,
        shadow_strength: None,
        shadow_distance: None,
        border_color: None,
        border_thickness: None,
        border_strength: None,
        border_distance: None,
        display_type,
        width: None,
        height: None,
        rotation: None,
        orientation: None,
        track_corner_radius: None,
        track_border_thickness: None,
        track_border_color: None,
        track_empty_color: None,
        track_empty_opacity: None,
        track_filled_color: None,
        track_filled_opacity: None,
        track_fill_flat: None,
        show_min_max_labels: None,
        min_max_label_font: None,
        min_max_label_font_size: None,
        min_max_label_color: None,
        extra: BTreeMap::new(),
    }
}

fn default_heading_cache(display_type: DisplayType) -> HeadingWidgetCache {
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
        display_type,
        indicator_shadow: None,
    }
}

fn presentation_caches_with(
    idx: usize,
    display_type: DisplayType,
) -> BTreeMap<usize, PresentationCache> {
    let mut caches = BTreeMap::new();
    caches.insert(
        idx,
        PresentationCache::HeadingTape(default_heading_cache(display_type)),
    );
    caches
}

fn empty_caches() -> BTreeMap<usize, PresentationCache> {
    BTreeMap::new()
}

fn empty_dense_series() -> DenseSeriesReport {
    let mut s = common::builders::empty_dense_series();
    s.heading = vec![Some(90.0)];
    s
}

fn default_dense_activity() -> DenseActivityReport {
    DenseActivityReport {
        frame_count: 1,
        frame_elapsed_seconds: vec![0.0],
        frame_distance_progress: vec![Some(0.0)],
        series: empty_dense_series(),
    }
}

fn default_style() -> ResolvedTextStyle {
    ResolvedTextStyle {
        x: 0.0,
        y: 0.0,
        font_name: None,
        font_size: 60.0,
        line_height: 55.2,
        color: Color::WHITE,
        opacity: 1.0,
        shadow_color: None,
        shadow_strength: 0.0,
        shadow_distance: 0.0,
        border_color: None,
        border_thickness: 0.0,
    }
}

// ── DisplayType dispatch ──────────────────────────────────────────────────

#[test]
fn text_display_type_returns_none_from_presentation_dispatch() {
    let value = default_value_config(DisplayType::Text);
    let dense = default_dense_activity();
    let style = default_style();
    let mut surface = create_surface(1920, 1080).unwrap();
    let mut profiler = RenderProfiler::default();
    let caches = empty_caches();

    let result = draw_metric_presentation(
        surface.canvas(),
        value.value,
        value.display_type,
        &style,
        &dense,
        0,
        1.0,
        &[],
        &caches,
        0,
        &mut profiler,
    );

    assert!(
        result.is_none(),
        "Text display type should return None from presentation dispatch"
    );
}

#[test]
fn tape_display_type_with_heading_cache_draws_successfully() {
    let value = default_value_config(DisplayType::Tape);
    let dense = default_dense_activity();
    let style = default_style();
    let caches = presentation_caches_with(0, DisplayType::Tape);
    let mut surface = create_surface(1920, 1080).unwrap();
    let mut profiler = RenderProfiler::default();

    let result = draw_metric_presentation(
        surface.canvas(),
        value.value,
        value.display_type,
        &style,
        &dense,
        0,
        1.0,
        &[],
        &caches,
        0,
        &mut profiler,
    );

    assert!(
        result.is_some(),
        "Tape display with heading cache should produce a report"
    );
    let report = result.unwrap();
    assert_eq!(report.geometry.widget_width, 400);
    assert_eq!(report.geometry.widget_height, 80);
}

#[test]
fn tape_display_type_without_cache_returns_none() {
    let value = default_value_config(DisplayType::Tape);
    let dense = default_dense_activity();
    let style = default_style();
    let mut surface = create_surface(1920, 1080).unwrap();
    let mut profiler = RenderProfiler::default();
    let caches = empty_caches();

    let result = draw_metric_presentation(
        surface.canvas(),
        value.value,
        value.display_type,
        &style,
        &dense,
        0,
        1.0,
        &[],
        &caches,
        0,
        &mut profiler,
    );

    assert!(
        result.is_none(),
        "Tape display without cache should return None"
    );
}

#[test]
fn tape_display_type_with_text_cache_returns_none() {
    let value = default_value_config(DisplayType::Tape);
    let dense = default_dense_activity();
    let style = default_style();
    let caches = presentation_caches_with(0, DisplayType::Text);
    let mut surface = create_surface(1920, 1080).unwrap();
    let mut profiler = RenderProfiler::default();

    let result = draw_metric_presentation(
        surface.canvas(),
        value.value,
        value.display_type,
        &style,
        &dense,
        0,
        1.0,
        &[],
        &caches,
        0,
        &mut profiler,
    );

    assert!(
        result.is_none(),
        "Tape display with Text cache should return None (presentation switched away)"
    );
}

#[test]
fn tape_display_type_with_non_heading_metric_returns_none() {
    let mut value = default_value_config(DisplayType::Tape);
    value.value = MetricKind::Speed;
    let dense = default_dense_activity();
    let style = default_style();
    let caches = presentation_caches_with(0, DisplayType::Tape);
    let mut surface = create_surface(1920, 1080).unwrap();
    let mut profiler = RenderProfiler::default();

    let result = draw_metric_presentation(
        surface.canvas(),
        value.value,
        value.display_type,
        &style,
        &dense,
        0,
        1.0,
        &[],
        &caches,
        0,
        &mut profiler,
    );

    assert!(
        result.is_none(),
        "Tape display with non-heading metric should return None"
    );
}

#[test]
fn future_boxed_display_types_return_none() {
    let dense = default_dense_activity();
    let style = default_style();
    let mut surface = create_surface(1920, 1080).unwrap();
    let mut profiler = RenderProfiler::default();
    let caches = empty_caches();

    for display_type in [
        DisplayType::Linear,
        DisplayType::Bars,
        DisplayType::Arc,
        DisplayType::Corner,
    ] {
        let value = default_value_config(display_type);
        let result = draw_metric_presentation(
            surface.canvas(),
            value.value,
            value.display_type,
            &style,
            &dense,
            0,
            1.0,
            &[],
            &caches,
            0,
            &mut profiler,
        );
        assert!(
            result.is_none(),
            "Future display type {display_type:?} should return None until implemented"
        );
    }
}

fn full_heading_tape_config(x: i32, y: i32) -> serde_json::Value {
    full_heading_tape_config_sized(x, y, 200, 60)
}

fn full_heading_tape_config_sized(x: i32, y: i32, width: u32, height: u32) -> serde_json::Value {
    serde_json::json!({
        "value": "heading",
        "x": x,
        "y": y,
        "display_type": "heading_tape",
        "width": width,
        "height": height,
        "pixels_per_degree": 5.0,
        "major_tick_interval": 45,
        "minor_ticks_per_major": 4,
        "show_major_ticks": true,
        "show_minor_ticks": true,
        "major_tick_length_pct": 0.3,
        "minor_tick_length_pct": 0.15,
        "major_tick_thickness": 2.0,
        "minor_tick_thickness": 1.0,
        "tick_color": "#ffffff",
        "cardinal_tick_color": "#ff0000",
        "tick_alignment": "below",
        "show_minor_labels": false,
        "show_major_labels": true,
        "label_color": "#ffffff",
        "cardinal_label_color": "#ff0000",
        "label_font_size": 12.0,
        "label_offset": 4.0,
        "show_indicator": true,
        "indicator_style": "chevron",
        "indicator_placement": "top",
        "indicator_color": "#FF0000",
        "indicator_size": 10.0,
        "rotation": 0.0,
        "opacity": 1.0
    })
}

fn full_speed_text_config(x: i32, y: i32) -> serde_json::Value {
    serde_json::json!({
        "value": "speed",
        "x": x,
        "y": y,
        "display_type": "text",
        "font": "Arial.ttf",
        "font_size": 32.0,
        "color": "#ffffff",
        "opacity": 1.0,
        "show_icon": true,
        "icon_color": "#ffffff",
        "icon_size": 45.0,
        "icon_offset_x": 0.0,
        "icon_offset_y": 0.0,
        "show_units": true,
        "unit_color": "#ffffff",
        "display_unit": "kmh",
        "prefix": "",
        "suffix": "",
        "decimals": 0
    })
}

// ── Asset preparation: per-index cache allocation ────────────────────────

#[test]
fn prepare_assets_distinct_caches_per_value_index() {
    use ovrley_core::activity::schema::ParsedActivity;
    use ovrley_core::debug::RenderProfiler;
    use ovrley_core::normalize::raw::RenderConfig;
    use ovrley_core::normalize::validate_render_config;
    use ovrley_core::paths::AppPaths;
    use ovrley_core::render::widgets::prepare_render_assets;
    use ovrley_core::render::widgets::types::PresentationCache;
    use std::path::PathBuf;

    let heading_tape_at_pos_0 = full_heading_tape_config(10, 20);

    let speed_text_at_pos_1 = serde_json::from_value(full_speed_text_config(100, 200)).unwrap();

    let heading_tape_at_pos_2 = full_heading_tape_config(400, 20);

    let config = RenderConfig {
        scene: serde_json::from_value(common::builders::scene_json()).unwrap(),
        labels: vec![],
        values: vec![
            serde_json::from_value(heading_tape_at_pos_0).unwrap(),
            speed_text_at_pos_1,
            serde_json::from_value(heading_tape_at_pos_2).unwrap(),
        ],
        plots: serde_json::Value::Object(serde_json::Map::new()),
        extra: BTreeMap::new(),
    };
    let config = validate_render_config(config).unwrap();
    let activity: ParsedActivity = serde_json::from_value(serde_json::json!({})).unwrap();
    let dense = ovrley_core::activity::schema::DenseActivityReport {
        frame_count: 1,
        frame_elapsed_seconds: vec![0.0],
        frame_distance_progress: vec![Some(0.0)],
        series: empty_dense_series(),
    };
    let paths = AppPaths {
        repo_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        font_dirs: vec![],
        debug_render_dir: std::env::temp_dir(),
        temp_dir: std::env::temp_dir(),
        bundled_templates_dirs: vec![],
        user_templates_dir: std::env::temp_dir(),
        downloads_dir: std::env::temp_dir(),
    };
    let mut profiler = RenderProfiler::default();

    let assets = prepare_render_assets(&paths, &config, &activity, &dense, &mut profiler).unwrap();

    assert_eq!(
        assets.presentation_caches.len(),
        2,
        "Two heading_tape values should produce two distinct caches"
    );

    let cache_0 = assets.presentation_caches.get(&0);
    let cache_2 = assets.presentation_caches.get(&2);
    assert!(cache_0.is_some(), "Cache should exist at index 0");
    assert!(cache_2.is_some(), "Cache should exist at index 2");
    assert!(
        assets.presentation_caches.get(&1).is_none(),
        "No cache at index 1 (speed_text)"
    );

    if let (Some(PresentationCache::HeadingTape(c0)), Some(PresentationCache::HeadingTape(c2))) =
        (cache_0, cache_2)
    {
        assert_ne!(
            c0.x, c2.x,
            "Caches at different indices should reflect different value positions"
        );
        assert_eq!(c0.x, 10.0, "Index-0 cache should use index-0 position");
        assert_eq!(c2.x, 400.0, "Index-2 cache should use index-2 position");
    }
}

// ── Full-frame: boxed-widget report collection ────────────────────────────

#[test]
fn render_preserves_multiple_boxed_reports() {
    use ovrley_core::activity::schema::ParsedActivity;
    use ovrley_core::normalize::raw::RenderConfig;
    use ovrley_core::normalize::validate_render_config;
    use ovrley_core::paths::AppPaths;
    use ovrley_core::render::render_preview_with_report;
    use std::path::PathBuf;

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();

    let heading_tape = serde_json::from_value(full_heading_tape_config(10, 20)).unwrap();

    let speed_text = serde_json::from_value(full_speed_text_config(400, 20)).unwrap();

    let mut scene = common::builders::scene_json();
    scene["width"] = serde_json::json!(800);
    scene["height"] = serde_json::json!(200);
    let config = RenderConfig {
        scene: serde_json::from_value(scene).unwrap(),
        labels: vec![],
        values: vec![heading_tape, speed_text],
        plots: serde_json::Value::Object(serde_json::Map::new()),
        extra: BTreeMap::new(),
    };
    let config = validate_render_config(config).unwrap();
    let activity: ParsedActivity = serde_json::from_value(serde_json::json!({})).unwrap();
    let mut series = empty_dense_series();
    series.speed = vec![Some(10.0)];
    let dense = ovrley_core::activity::schema::DenseActivityReport {
        frame_count: 1,
        frame_elapsed_seconds: vec![0.0],
        frame_distance_progress: vec![Some(0.0)],
        series,
    };
    let fonts_dir = workspace_root.join("fonts");
    let paths = AppPaths {
        repo_root: workspace_root.clone(),
        font_dirs: if fonts_dir.is_dir() {
            vec![fonts_dir]
        } else {
            vec![]
        },
        debug_render_dir: std::env::temp_dir(),
        temp_dir: std::env::temp_dir(),
        bundled_templates_dirs: vec![],
        user_templates_dir: std::env::temp_dir(),
        downloads_dir: std::env::temp_dir(),
    };
    let out_path = std::env::temp_dir().join("metric_pres_test_report.png");

    let result = render_preview_with_report(&paths, &config, &activity, &dense, 0.0, &out_path);

    assert!(result.is_ok(), "Full-frame render should succeed");
    let report = result.unwrap();

    assert!(
        report.metric_presentations.len() == 1,
        "Exactly one boxed presentation should report here: heading_tape renders as a boxed metric presentation while speed text remains intrinsic; got {}",
        report.metric_presentations.len()
    );
    let presentation = &report.metric_presentations[0];
    assert_eq!(
        presentation.value_idx, 0,
        "Heading tape should map to value index 0"
    );
    assert_eq!(
        presentation.metric_kind,
        MetricKind::Heading,
        "Reported boxed presentation should identify the heading metric"
    );
    assert_eq!(
        presentation.display_type,
        DisplayType::Tape,
        "Reported boxed presentation should identify heading_tape"
    );
    assert_eq!(
        presentation.widget.geometry.widget_width, 200,
        "Reported widget geometry should match configured heading-tape width"
    );
    assert_eq!(
        presentation.widget.geometry.widget_height, 60,
        "Reported widget geometry should match configured heading-tape height"
    );

    let _ = std::fs::remove_file(&out_path);
}

#[test]
fn render_reports_multiple_heading_tapes_with_identity() {
    use ovrley_core::activity::schema::ParsedActivity;
    use ovrley_core::normalize::raw::RenderConfig;
    use ovrley_core::normalize::validate_render_config;
    use ovrley_core::paths::AppPaths;
    use ovrley_core::render::render_preview_with_report;
    use std::path::PathBuf;

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();

    let heading_tape_left = serde_json::from_value(full_heading_tape_config(10, 20)).unwrap();

    let heading_tape_right =
        serde_json::from_value(full_heading_tape_config_sized(400, 20, 180, 50)).unwrap();

    let mut scene = common::builders::scene_json();
    scene["width"] = serde_json::json!(800);
    scene["height"] = serde_json::json!(200);
    let config = RenderConfig {
        scene: serde_json::from_value(scene).unwrap(),
        labels: vec![],
        values: vec![heading_tape_left, heading_tape_right],
        plots: serde_json::Value::Object(serde_json::Map::new()),
        extra: BTreeMap::new(),
    };
    let config = validate_render_config(config).unwrap();
    let activity: ParsedActivity = serde_json::from_value(serde_json::json!({})).unwrap();
    let dense = ovrley_core::activity::schema::DenseActivityReport {
        frame_count: 1,
        frame_elapsed_seconds: vec![0.0],
        frame_distance_progress: vec![Some(0.0)],
        series: empty_dense_series(),
    };
    let fonts_dir = workspace_root.join("fonts");
    let paths = AppPaths {
        repo_root: workspace_root.clone(),
        font_dirs: if fonts_dir.is_dir() {
            vec![fonts_dir]
        } else {
            vec![]
        },
        debug_render_dir: std::env::temp_dir(),
        temp_dir: std::env::temp_dir(),
        bundled_templates_dirs: vec![],
        user_templates_dir: std::env::temp_dir(),
        downloads_dir: std::env::temp_dir(),
    };
    let out_path = std::env::temp_dir().join("metric_pres_multi_report.png");

    let result = render_preview_with_report(&paths, &config, &activity, &dense, 0.0, &out_path);

    assert!(result.is_ok(), "Full-frame render should succeed");
    let report = result.unwrap();

    assert_eq!(
        report.metric_presentations.len(),
        2,
        "Two heading_tape widgets should yield two metric presentation reports"
    );
    assert_eq!(report.metric_presentations[0].value_idx, 0);
    assert_eq!(report.metric_presentations[1].value_idx, 1);
    assert_eq!(
        report.metric_presentations[0].display_type,
        DisplayType::Tape
    );
    assert_eq!(
        report.metric_presentations[1].display_type,
        DisplayType::Tape
    );
    assert_eq!(
        report.metric_presentations[0].widget.geometry.widget_width,
        200
    );
    assert_eq!(
        report.metric_presentations[1].widget.geometry.widget_width,
        180
    );

    let _ = std::fs::remove_file(&out_path);
}
