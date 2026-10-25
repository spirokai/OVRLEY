//! One seam test: transported font + standalone cached surface must match native composition.
mod common;

use ovrley_core::fonts::{preview_font_resource, NativeFonts};
use ovrley_widgets::{
    model::scene::validate_scene_presentation,
    preview::PreviewSession,
    render::{pixels::read_surface_rgba, text::validated_value_style},
    widgets::value::preview::WidgetRequest,
    widgets::value::{
        format::{format_metric_parts, MetricSample},
        layout::{draw_metric_parts, draw_static_metric_parts_for_value},
        validation::validate_value_widget,
    },
};
use serde_json::{json, Value};
use skia_safe::{surfaces, AlphaType, Color, ColorType, ImageInfo};

fn elevation_request() -> Value {
    let defaults: Value = serde_json::from_str(include_str!("../../../assets/standard-widgets.json")).unwrap();
    let mut request = route_request();
    request["type"] = json!("elevation");
    request["widget"] = defaults["definitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|definition| definition["type"] == "elevation")
        .unwrap()["defaults"]
        .clone();
    request["widget"].as_object_mut().unwrap().extend(json!({
        "id": "elevation", "x": 150.25, "y": 90.5, "width": 280.125, "height": 145.25, "rotation": 27.0,
        "show_elevation_imperial": true, "starting_altitude": 3500, "starting_altitude_unit": "ft",
        "metric_label_offset_x": 7.25, "metric_label_offset_y": -22.5,
        "imperial_label_offset_x": 18.5, "imperial_label_offset_y": 4.25,
        "point_label": {"font": "Inter.ttf", "font_size": 19.5, "color": "#abcdef"}
    }).as_object().unwrap().clone());
    request
}

#[test]
fn diving_profile_preview_matches_export_during_playback_and_seeking() {
    use ovrley_core::{activity::{interpolate::densify_activity, trim::trim_activity}, debug::RenderProfiler, paths::AppPaths, render::render_full_frames};
    let manifest: Value = serde_json::from_str(include_str!("../../../assets/standard-widgets.json")).unwrap();
    let mut request = route_request();
    request["type"] = json!("diving_profile");
    request["widget"] = manifest["definitions"].as_array().unwrap().iter().find(|w| w["type"] == "diving_profile").unwrap()["defaults"].clone();
    request["widget"].as_object_mut().unwrap().extend(json!({
        "x": 150.25, "y": 90.5, "width": 280.125, "height": 145.25, "rotation": 0,
        "opacity": 0.8, "marker_variant": "ring", "show_depth_imperial": true, "show_full_activity": false,
        "simplify_tolerance_px": 2.5, "target_density": 1.25,
        "point_label": {"font": "Inter.ttf", "font_size": 19.5, "color": "#abcdef"}
    }).as_object().unwrap().clone());
    request["scene"]["custom_export_range_active"] = json!(true);
    request["scene"]["start"] = json!(0.25);
    request["scene"]["end"] = json!(3.75);
    let activity = ovrley_core::activity::parse_activity_json(&json!({
        "sample_elapsed_seconds": [0, 0.5, 1, 1.5, 2, 2.5, 3, 3.5, 4], "trim_end_seconds": 4,
        "sample_distance_progress": [0, 0, 0, 0, 0, 0, 0, 0, 0],
        "depth": [0, 30, 30, 18, 12, 6, 3, 3, 0], "first_stop_depth": [0, 0, 9, 9, 6, 6, 3, 0, 0]
    }).to_string()).unwrap();
    let config = common::seam::validated_config_from_value(json!({"scene": request["scene"], "values": [], "labels": [], "plots": [request["widget"]]}));
    let requirements = config.render_data_requirements();
    assert!(requirements.depth && requirements.first_stop_depth);
    let trimmed = trim_activity(&activity, 0.25, 3.75, &requirements).unwrap();
    let dense = densify_activity(&trimmed, vec![0.0, 1.5, 3.25], &requirements);
    let root = common::test_config::repo_git_root();
    let paths = AppPaths::from_runtime_roots(root.clone(), root, std::env::temp_dir());
    let frames = render_full_frames(&paths.font_dirs, &config, &activity, &dense, &(0..dense.frame_count).collect::<Vec<_>>(), &mut RenderProfiler::default()).unwrap();
    let (mut preview, id) = elevation_preview(&request, &serde_json::to_vec(&activity).unwrap());
    let mut placement = None;
    for index in [0, 1, 2, 0] {
        preview.render(0.25 + dense.frame_elapsed_seconds[index]).unwrap();
        let widget = &preview.widgets[&id];
        if let Some(previous) = &placement { assert_eq!(widget.geometry(), previous); }
        else { placement = Some(widget.geometry().clone()); }
        let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
        let (w, h, left, top) = (geometry["width"].as_u64().unwrap() as usize, geometry["height"].as_u64().unwrap() as usize,
            geometry["left"].as_f64().unwrap() as usize, geometry["top"].as_f64().unwrap() as usize);
        let native = &frames[index];
        let mut difference = 0;
        for y in 0..h {
            let native = &native[((top + y) * 700 + left) * 4..((top + y) * 700 + left + w) * 4];
            for (a, b) in native.chunks_exact(4).zip(widget.pixels()[y * w * 4..(y + 1) * w * 4].chunks_exact(4)) {
                difference = difference.max(a[3].abs_diff(b[3]) as u16);
                for c in 0..3 { difference = difference.max((a[c] as u16 * a[3] as u16).abs_diff(b[c] as u16 * b[3] as u16) / 255); }
            }
        }
        assert!(difference <= 8, "diving profile preview/export pixel difference: {difference}");
        let revision = widget.revision();
        preview.render(0.25 + dense.frame_elapsed_seconds[index]).unwrap();
        assert_eq!(preview.widgets[&id].revision(), revision);
    }
    // Playback changes the completed line and marker while reusing static presentation.
    let pixels = preview.widgets[&id].pixels().clone();
    preview.render(2.0).unwrap();
    assert_ne!(preview.widgets[&id].pixels(), &pixels);
}

fn elevation_activity() -> ovrley_core::activity::schema::ParsedActivity {
    ovrley_core::activity::parse_activity_json(&json!({
        "sample_elapsed_seconds": [0.0, 0.5, 1.5, 2.5, 3.0, 4.0], "trim_end_seconds": 4.0,
        "sample_distance_progress": [0.0, 0.2, 0.2, 0.2, 0.8, 1.0],
        "sample_elevations": [100.0, 115.0, null, 165.0, 180.0, 125.0],
        "barometric_altitude": [100.0, 115.0, null, 165.0, 180.0, 125.0],
        "elevation": [10.0, 15.0, 20.0, 30.0, 35.0, 25.0]
    }).to_string()).unwrap()
}

fn elevation_preview(request: &Value, activity: &[u8]) -> (PreviewSession, u32) {
    let mut preview = PreviewSession::default();
    // Compared against native export, which grid-fits text.
    preview.fonts.set_grid_fit(true);
    let font: ovrley_widgets::preview::FontRequest = serde_json::from_value(json!({"font": "Inter.ttf"})).unwrap();
    let dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let bytes = preview_font_resource(&dirs, &font.font, font.font_weight, font.italic).unwrap();
    preview.fonts.register(font, &bytes).unwrap();
    preview.replace_activity(activity).unwrap();
    let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
    (preview, id)
}

fn prepared_elevation(preview: &PreviewSession, id: u32) -> std::sync::Arc<ovrley_widgets::widgets::elevation::geometry::ElevationGeometry> {
    let ovrley_widgets::preview_widget::PreviewWidget::Elevation(widget) = &preview.widgets[&id].widget else { panic!("expected elevation handle"); };
    widget.prepared_geometry.as_ref().unwrap().clone()
}

/// One placed request per preview family, each in its own font-loaded session.
fn every_family_preview() -> Vec<(PreviewSession, Value)> {
    let defaults: Value = serde_json::from_str(include_str!("../../../assets/standard-widgets.json")).unwrap();
    let mut cases = vec![
        label_preview(), gradient_preview(), lean_angle_preview(), g_force_preview(),
        lap_timer_preview(), splits_run_preview(), heading_preview(), heading_compass_preview("indicator"), heading_compass_preview("compass"),
        heart_zones_preview("line"), heart_zones_preview("arrow"),
        gauge_preview("linear", "speed", "horizontal", "fill"),
        gauge_preview("arc", "speed", "horizontal", "fill"),
        gauge_preview("corner", "speed", "bottom-left", "fill"),
        gauge_preview("gt_gauge", "speed", "bottom-left", "fill"),
    ];
    for kind in ["course", "elevation", "diving_profile", "rolling_chart", "backdrop", "raster", "speed", "time"] {
        let (preview, mut request) = lean_angle_preview();
        request["type"] = json!(kind);
        request["widget"] = if matches!(kind, "speed" | "time") {
            let mut widget = common::builders::speed_value_json();
            widget["value"] = json!(kind);
            if kind == "time" {
                widget.as_object_mut().unwrap().extend(json!({
                    "format": "time-24", "hours_offset": 0, "time_mode": "elapsed",
                    "elapsed_origin": "activity", "show_hundredths": false,
                }).as_object().unwrap().clone());
            }
            widget
        } else {
            defaults["definitions"].as_array().unwrap().iter()
                .find(|definition| definition["type"] == kind).unwrap()["defaults"].clone()
        };
        request["widget"]["x"] = json!(150.25);
        request["widget"]["y"] = json!(90.5);
        request["scene"]["preview_duration_seconds"] = json!(4.0);
        request["scene"]["export_start_seconds"] = json!(0.0);
        if matches!(kind, "speed" | "time") { request["widget"]["font"] = json!("Inter.ttf"); }
        if matches!(kind, "elevation" | "diving_profile") { request["widget"]["point_label"]["font"] = json!("Inter.ttf"); }
        if kind == "rolling_chart" {
            // The editor names legends when it projects render data; templates store none.
            request["widget"]["legend_font"] = json!("Inter.ttf");
            for series in request["widget"]["series"].as_array_mut().unwrap() {
                series["label"] = json!(series["metric"].as_str().unwrap().to_uppercase());
            }
        }
        if kind == "raster" { request["widget"]["id"] = json!("raster"); }
        if kind == "backdrop" {
            request["widget"].as_object_mut().unwrap().extend(json!({
                "id": "backdrop", "width": 120, "height": 80, "corner_radius": 4,
                "round_top_left": true, "round_top_right": true, "round_bottom_left": true, "round_bottom_right": true,
            }).as_object().unwrap().clone());
        }
        cases.push((preview, request));
    }
    cases
}

#[test]
fn every_preview_family_reuses_drag_pixels_and_restores_fractional_commit() {
    for (mut preview, mut request) in every_family_preview() {
        let kind = request["type"].as_str().unwrap().to_owned();
        let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
        preview.render(1.5).unwrap();
        let pixels = preview.widgets[&id].pixels().clone();
        let storage = preview.widgets[&id].pixels().as_ptr();
        let pixels_revision = preview.widgets[&id].pixels_revision();
        let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
        let (x, y) = (request["widget"]["x"].as_f64().unwrap(), request["widget"]["y"].as_f64().unwrap());
        request["transient"] = json!(true);
        for (dx, dy) in [(12.0, -8.0), (12.125, -8.375), (-5.5, 7.25)] {
            request["widget"]["x"] = json!(x + dx);
            request["widget"]["y"] = json!(y + dy);
            let revision = preview.widgets[&id].revision();
            preview.update_widget(id, serde_json::from_value(request.clone()).unwrap()).unwrap();
            preview.render(1.5).unwrap();
            let widget = &preview.widgets[&id];
            assert_ne!(widget.revision(), revision, "{kind} must present movement");
            assert_eq!(widget.pixels_revision(), pixels_revision, "{kind} must retain its pixels revision");
            assert_eq!(widget.pixels().as_ptr(), storage, "{kind} must retain its surface storage");
            assert_eq!(widget.pixels(), &pixels, "{kind} must not redraw during dragging");
            let moved: Value = serde_json::from_slice(widget.geometry()).unwrap();
            for (field, delta) in [("left", dx), ("top", dy)] {
                assert!((moved["content"][field].as_f64().unwrap() - geometry["content"][field].as_f64().unwrap() - delta).abs() < 0.001, "{kind} content placement");
                assert!((moved[field].as_f64().unwrap() - geometry[field].as_f64().unwrap() - delta).abs() < 0.001, "{kind} shadow placement");
            }
        }
        request["transient"] = json!(false);
        preview.update_widget(id, serde_json::from_value(request.clone()).unwrap()).unwrap();
        preview.render(1.5).unwrap();
        if kind != "raster" { assert_ne!(preview.widgets[&id].pixels_revision(), pixels_revision, "{kind} must settle its fractional pixels"); }
        let fresh = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
        preview.render(1.5).unwrap();
        assert_eq!(preview.widgets[&id].pixels(), preview.widgets[&fresh].pixels(), "{kind} settled pixels must match a fresh render");
        let settled: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
        let fresh_geometry: Value = serde_json::from_slice(preview.widgets[&fresh].geometry()).unwrap();
        assert_eq!(settled, fresh_geometry, "{kind} settled geometry must match a fresh render");
    }
}

#[test]
fn every_preview_family_derives_serialized_geometry_from_typed_tile_bounds() {
    for (mut preview, mut request) in every_family_preview() {
        let kind = request["type"].as_str().unwrap().to_owned();
        let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
        preview.render(1.5).unwrap();
        let assert_derived = |preview: &PreviewSession, stage: &str| {
            let widget = &preview.widgets[&id];
            let bounds = widget.bounds().unwrap_or_else(|| panic!("{kind} {stage}: rendered tile must report bounds"));
            let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
            let field = |name: &str| geometry[name].as_f64().unwrap() as f32;
            assert_eq!((field("left"), field("top")), (bounds.left, bounds.top), "{kind} {stage}: tile origin");
            let size = (geometry["width"].as_u64().unwrap() as u32, geometry["height"].as_u64().unwrap() as u32);
            if kind == "raster" {
                // Source pixels stretch over the unrotated content frame.
                let content = (geometry["content"]["width"].as_f64().unwrap() as f32, geometry["content"]["height"].as_f64().unwrap() as f32);
                assert_eq!(content, (bounds.width, bounds.height), "{kind} {stage}: tile extent");
            } else {
                let scale = field("preview_raster_scale");
                assert_eq!(size, ((bounds.width * scale).ceil() as u32, (bounds.height * scale).ceil() as u32), "{kind} {stage}: tile extent");
            }
        };
        assert_derived(&preview, "rendered");
        let (x, y) = (request["widget"]["x"].as_f64().unwrap(), request["widget"]["y"].as_f64().unwrap());
        request["transient"] = json!(true);
        request["widget"]["x"] = json!(x + 12.125);
        request["widget"]["y"] = json!(y - 8.375);
        preview.update_widget(id, serde_json::from_value(request.clone()).unwrap()).unwrap();
        preview.render(1.5).unwrap();
        assert_derived(&preview, "dragged");
    }
}

/// Every family reads its metric from this activity; speed crosses digit-count changes.
fn every_family_activity() -> Vec<u8> {
    serde_json::to_vec(&every_family_parsed_activity()).unwrap()
}

fn every_family_parsed_activity() -> ovrley_core::activity::schema::ParsedActivity {
    let points = json!([[48.0, 12.0], [48.004, 12.002], [48.01, 12.005], [null, null], [48.013, 12.02], [48.016, 12.018], [48.02, 12.013], [48.018, 12.006], [48.015, 12.0]]);
    ovrley_core::activity::parse_activity_json(&json!({
        "sample_elapsed_seconds": [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0], "trim_end_seconds": 4.0,
        "sample_distance_progress": [0.0, 0.1, 0.2, 0.3, 0.45, 0.6, 0.75, 0.9, 1.0],
        "sample_course_points": points, "course": points,
        "sample_elevations": [100.0, 115.0, null, 165.0, 180.0, 125.0, 90.0, 95.0, 99.0],
        "barometric_altitude": [100.0, 115.0, null, 165.0, 180.0, 125.0, 90.0, 95.0, 99.0],
        "elevation": [10.0, 15.0, 20.0, 30.0, 35.0, 25.0, 5.0, 7.0, 9.0],
        "speed": [0.0, 9.0, 10.0, 99.4, 100.0, 140.0, 5.0, null, 12.0],
        "distance": [0.0, 4.0, 9.0, 13.0, null, 24.0, 29.0, 33.0, 38.0],
        "heading": [350.0, 10.0, 90.0, 180.0, 270.0, 359.0, 0.5, 45.0, 120.0],
        "heartrate": [80.0, 110.0, 130.0, 150.0, null, 170.0, 185.0, 190.0, 100.0],
        "gradient": [-12.5, -1.0, 0.0, 3.0, 9.9, 10.0, 25.0, -25.0, 1.0],
        "depth": [0.0, 4.0, 9.5, 18.0, 24.0, 21.0, 12.0, 5.0, 0.0],
        "lean_angle": [-55.0, -10.0, 0.0, 12.0, 48.0, 61.0, -61.0, 5.0, 0.0],
        "g_force_x": [0.0, 0.4, -1.2, 2.5, -0.1, 0.0, 1.0, -2.0, 0.2],
        "g_force_y": [0.0, -0.3, 0.8, -2.2, 1.5, 0.0, -1.0, 2.0, 0.1],
        "g_force_z": [1.0, 1.0, 1.1, 0.9, 1.0, 1.0, 1.0, 1.0, 1.0],
        "lap_number": [-1, 0, 0, 0, 0, 1, 1, 2, 2],
        "lap_time_seconds": [null, 0.0, 0.5, 1.0, 1.5, 0.0, 0.5, 0.0, 0.5],
        "lap_start_elapsed_seconds": [0.5, 2.5, 3.5],
        "lap_durations_seconds": [2.0, 1.0, 0.5], "lap_durations_best_so_far_seconds": [2.0, 1.0, 0.5],
        "delta_to_best_lap_seconds": [null, null, null, null, null, 0.0, -0.5, null, 0.25],
    }).to_string()).unwrap()
}

/// Request variants covering every alignment, rotation, shadows and strokes for each family.
fn bounds_variants(request: &Value) -> Vec<(String, Value)> {
    let kind = request["type"].as_str().unwrap();
    let mut variants = vec![("as configured".to_owned(), request.clone())];
    let mut styled = request.clone();
    styled["scene"].as_object_mut().unwrap().extend(json!({
        "shadow_color": "#102030", "shadow_strength": 5, "shadow_distance": 6,
        "border_color": "#ff00ff", "border_thickness": 3,
    }).as_object().unwrap().clone());
    variants.push(("shadow and stroke".to_owned(), styled));
    let mut plain = request.clone();
    plain["scene"].as_object_mut().unwrap().extend(json!({
        "shadow_strength": 0, "shadow_distance": 0, "border_thickness": 0,
    }).as_object().unwrap().clone());
    variants.push(("no shadow or stroke".to_owned(), plain));
    if request["widget"].get("content_alignment").is_some() || matches!(kind, "speed" | "time") {
        for alignment in ["left", "center", "right"] {
            let mut aligned = request.clone();
            aligned["widget"]["content_alignment"] = json!(alignment);
            variants.push((format!("{alignment} aligned"), aligned));
        }
    }
    if matches!(kind, "speed" | "time") {
        // Fixed-width digits are the scene default; proportional digits change the value's width.
        let mut proportional = request.clone();
        proportional["scene"]["fixed_width_digits"] = json!(false);
        variants.push(("proportional digits".to_owned(), proportional));
    }
    if matches!(kind, "course" | "elevation" | "diving_profile" | "raster") {
        for rotation in [37.0, 90.0, 230.0] {
            let mut rotated = request.clone();
            rotated["widget"]["rotation"] = json!(rotation);
            variants.push((format!("rotated {rotation}"), rotated));
        }
    }
    variants
}

#[test]
fn every_family_reports_layout_only_bounds_that_contain_its_drawing() {
    use ovrley_widgets::{
        activity::ActivitySampler,
        preview_widget::PreviewWidget,
        widgets::value::preview::{allocated_surfaces, TileBounds},
    };
    const MARGIN: i32 = 48;
    let activity = every_family_activity();
    let sampler = ActivitySampler::from_json(&activity).unwrap();
    // Before, inside and after the activity; samples between the timestamps change value widths.
    let seconds = [-0.5, 0.0, 0.25, 0.75, 1.0, 1.25, 2.0, 2.25, 2.75, 3.1, 3.6, 4.0, 6.0];
    for (mut preview, base) in every_family_preview() {
        preview.replace_activity(&activity).unwrap();
        for (variant, request) in bounds_variants(&base) {
            let kind = format!("{} ({variant})", request["type"].as_str().unwrap());
            let raster = request["type"] == "raster";
            let mut widget = PreviewWidget::new(serde_json::from_value(request.clone()).unwrap(), &preview.fonts, &preview.images)
                .unwrap_or_else(|error| panic!("{kind}: {error}"));
            let allocated = allocated_surfaces();
            let bounds: Vec<TileBounds> = seconds
                .iter()
                .map(|second| widget.layout_bounds(Some(&sampler), *second, &preview.fonts).unwrap())
                .collect();
            assert_eq!(allocated_surfaces(), allocated, "{kind}: layout-only bounds must not allocate a surface");
            let text_speed = request["type"] == "speed" && request["widget"]["display_type"] == "text" && variant == "proportional digits";
            if text_speed {
                let mut widths: Vec<_> = bounds.iter().map(|bounds| bounds.width.to_bits()).collect();
                widths.sort_unstable();
                widths.dedup();
                assert!(widths.len() > 3, "{kind}: proportional digits must change the value's width");
            }
            let mut drew = false;
            let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
            for (second, bounds) in seconds.iter().zip(bounds) {
                preview.render(*second).unwrap();
                if !raster {
                    // Rasters transport source pixels; their scene extent is checked by drawing below.
                    assert_eq!(preview.widgets[&id].bounds(), Some(bounds), "{kind} at {second}s: rendered tile bounds");
                }
                let (left, top) = (bounds.left.floor() as i32 - MARGIN, bounds.top.floor() as i32 - MARGIN);
                let (width, height) = (bounds.width.ceil() as i32 + 2 * MARGIN + 2, bounds.height.ceil() as i32 + 2 * MARGIN + 2);
                let info = ImageInfo::new((width, height), ColorType::RGBA8888, AlphaType::Premul, None);
                let mut surface = surfaces::raster(&info, None, None).unwrap();
                surface.canvas().clear(Color::TRANSPARENT);
                surface.canvas().translate((-left as f32, -top as f32));
                let drawn = widget.draw(surface.canvas(), Some(&sampler), *second, &preview.fonts).unwrap();
                assert_eq!(drawn, bounds, "{kind} at {second}s: drawn rectangle");
                let mut pixels = vec![0; width as usize * height as usize * 4];
                read_surface_rgba(&mut surface, &mut pixels).unwrap();
                let (right, bottom) = (bounds.left + bounds.width, bounds.top + bounds.height);
                for (index, pixel) in pixels.chunks_exact(4).enumerate() {
                    if pixel[3] == 0 {
                        continue;
                    }
                    drew = true;
                    let x = (index as i32 % width + left) as f32;
                    let y = (index as i32 / width + top) as f32;
                    assert!(
                        x + 1.0 > bounds.left && x < right && y + 1.0 > bounds.top && y < bottom,
                        "{kind} at {second}s: pixel ({x}, {y}) alpha {} lies outside {bounds:?}",
                        pixel[3],
                    );
                }
            }
            assert!(drew, "{kind}: some sample must draw pixels");
        }
    }
}

/// Golden reference: the planned atlas reassembles into the single-slot full-frame plan's frames,
/// before, inside and after the activity coverage. Workers claim frames out of order and revisit
/// equal samples, so a worker rendering frames out of order must match one rendering them in order.
fn assert_atlas_frames_match_the_full_frame(
    name: &str,
    config: &ovrley_core::normalize::ValidatedRenderConfig,
    activity: &ovrley_core::activity::schema::ParsedActivity,
) -> ovrley_core::render::AtlasPlan {
    use ovrley_core::{
        activity::build_dense_activity_report_validated,
        debug::RenderProfiler,
        paths::AppPaths,
        render::{ExportWidgets, FrameSize},
    };
    const BLANK_LEADING_FRAMES: usize = 2;
    let root = common::test_config::repo_git_root();
    let paths = AppPaths::from_runtime_roots(root.clone(), root, std::env::temp_dir());
    let dense = build_dense_activity_report_validated(activity, config).unwrap();
    let size = FrameSize { width: config.scene.presentation.width, height: config.scene.presentation.height };
    let mut profiler = RenderProfiler::default();
    let export = ExportWidgets::prepare(&paths.font_dirs, config, activity, &dense, size, BLANK_LEADING_FRAMES as u64, &mut profiler).unwrap();
    let full_frame = export.prepare_atlas(export.full_frame_atlas(), &mut profiler).unwrap();
    let frame_count = BLANK_LEADING_FRAMES + dense.frame_count + 2;
    let mut in_order = export.worker().unwrap();
    let expected: Vec<Vec<u8>> = (0..frame_count)
        .map(|frame| {
            let mut pixels = vec![0; size.rgba_len().unwrap()];
            in_order.render_atlas(frame, &full_frame, &mut pixels, &mut profiler).unwrap();
            pixels
        })
        .collect();
    let covered = BLANK_LEADING_FRAMES..BLANK_LEADING_FRAMES + dense.frame_count;
    assert!(
        covered.clone().any(|frame| expected[frame].chunks_exact(4).any(|pixel| pixel[3] > 0)),
        "{name}: covered frames must draw widgets"
    );
    let mut out_of_order = export.worker().unwrap();
    let mut actual = vec![0; size.rgba_len().unwrap()];
    for frame in (0..frame_count).rev().step_by(3).chain(0..frame_count) {
        out_of_order.render_atlas(frame, &full_frame, &mut actual, &mut profiler).unwrap();
        assert!(actual == expected[frame], "{name} frame {frame} (covered: {}): out-of-order rendering differs", covered.contains(&frame));
    }
    assert_atlas_reassembles_the_full_frame(name, &export, &(0..frame_count).collect::<Vec<_>>())
}

/// The planned atlas holds every widget's bounds of every planned frame, and compositing its
/// slots in order over a transparent frame at their scene positions gives the full-frame plan's
/// frame: within one level of anti-aliasing rounding where one slot covers a pixel, within
/// 8-bit blending rounding where slots overlap.
fn assert_atlas_reassembles_the_full_frame(
    name: &str,
    export: &ovrley_core::render::ExportWidgets,
    frames: &[usize],
) -> ovrley_core::render::AtlasPlan {
    use ovrley_core::{debug::RenderProfiler, render::AtlasPlan};
    let size = export.frame_size();
    let atlas = export.plan_atlas(frames, ovrley_core::render::SlotPlacement::Overlay, &mut RenderProfiler::default()).unwrap();
    let full_frame = export.prepare_atlas(export.full_frame_atlas(), &mut RenderProfiler::default()).unwrap();
    let prepared = export.prepare_atlas(atlas.clone(), &mut RenderProfiler::default()).unwrap();
    let mut planned = vec![false; full_frame.plan().slots()[0].layers.len()];
    for (index, slot) in atlas.slots().iter().enumerate() {
        let scene = slot.scene;
        for value in [scene.left, scene.top, scene.width(), scene.height(), slot.atlas_origin.x, slot.atlas_origin.y] {
            assert_eq!(value % 2, 0, "{name}: slot {slot:?} must be even");
        }
        assert!(slot.layers.windows(2).all(|pair| pair[0] < pair[1]), "{name}: slot {slot:?} layers in layer order");
        for layer in &slot.layers {
            assert!(!std::mem::replace(&mut planned[*layer], true), "{name}: layer {layer} planned twice");
        }
        for other in &atlas.slots()[index + 1..] {
            let disjoint = |a: skia_safe::IRect, b: skia_safe::IRect| a.right <= b.left || b.right <= a.left || a.bottom <= b.top || b.bottom <= a.top;
            assert!(
                disjoint(scene, other.scene) || slot.layers[slot.layers.len() - 1] < other.layers[0],
                "{name}: overlapping slots {slot:?} and {other:?} must overlay in layer order"
            );
            assert!(disjoint(AtlasPlan::atlas_rect(slot), AtlasPlan::atlas_rect(other)), "{name}: slots {slot:?} and {other:?} overlap in the atlas");
        }
        let rect = AtlasPlan::atlas_rect(slot);
        assert!(rect.left >= 0 && rect.top >= 0 && rect.right <= atlas.size().width as i32 && rect.bottom <= atlas.size().height as i32);
    }
    let mut worker = export.worker().unwrap();
    for frame in frames {
        for (layer, bounds) in worker.frame_bounds(*frame).unwrap().into_iter().enumerate() {
            let Some(bounds) = bounds else { continue };
            let slot = atlas.slots().iter().find(|slot| slot.layers.contains(&layer));
            let slot = slot.unwrap_or_else(|| panic!("{name} frame {frame}: visible layer {layer} has no slot"));
            let scene = slot.scene;
            assert!(
                scene.left <= bounds.left && scene.top <= bounds.top && bounds.right <= scene.right && bounds.bottom <= scene.bottom,
                "{name} frame {frame}: layer {layer} bounds {bounds:?} lie outside its slot {scene:?}"
            );
        }
    }
    let (width, height) = (size.width as usize, size.height as usize);
    let mut atlas_pixels = vec![0; atlas.size().rgba_len().unwrap()];
    let mut expected = vec![0; size.rgba_len().unwrap()];
    let mut profiler = RenderProfiler::default();
    let mut drew = false;
    for frame in frames {
        worker.render_atlas(*frame, &full_frame, &mut expected, &mut profiler).unwrap();
        worker.render_atlas(*frame, &prepared, &mut atlas_pixels, &mut profiler).unwrap();
        let mut reassembled = vec![0u8; expected.len()];
        let mut covers = vec![0u8; width * height];
        for slot in atlas.slots() {
            for row in 0..slot.scene.height() as usize {
                let y = slot.scene.top as usize + row;
                let left = slot.scene.left as usize;
                let columns = (slot.scene.width() as usize).min(width - left);
                if y >= height {
                    break;
                }
                let source = ((slot.atlas_origin.y as usize + row) * atlas.size().width as usize + slot.atlas_origin.x as usize) * 4;
                for column in 0..columns {
                    let pixel = y * width + left + column;
                    let (source, target) = (source + column * 4, pixel * 4);
                    covers[pixel] += 1;
                    source_over(&mut reassembled[target..target + 4], &atlas_pixels[source..source + 4]);
                }
            }
        }
        drew |= expected.iter().skip(3).step_by(4).any(|alpha| *alpha > 0);
        let matches = |pixel: usize| {
            let (expected, actual) = (&expected[pixel * 4..pixel * 4 + 4], &reassembled[pixel * 4..pixel * 4 + 4]);
            // Skia rasterizes a path at its shifted atlas position in float, so anti-aliased
            // coverage can round one level apart; overlapping slots add blending rounding.
            let alpha_tolerance = if covers[pixel] <= 1 { 1 } else { 2 };
            // Compare premultiplied: straight colour is unstable where alpha is near zero.
            let premultiplied = |rgba: &[u8], channel: usize| f32::from(rgba[channel]) * f32::from(rgba[3]) / 255.0;
            (expected[3].abs_diff(actual[3]) <= alpha_tolerance)
                && (0..3).all(|channel| (premultiplied(expected, channel) - premultiplied(actual, channel)).abs() <= 2.0)
        };
        if let Some(pixel) = (0..width * height).find(|pixel| !matches(*pixel)) {
            panic!(
                "{name} frame {frame}: reassembled atlas differs at ({}, {}): full frame {:?}, atlas {:?}",
                pixel % width,
                pixel / width,
                &expected[pixel * 4..pixel * 4 + 4],
                &reassembled[pixel * 4..pixel * 4 + 4]
            );
        }
    }
    assert!(drew, "{name}: the reassembled frames must show widgets");
    atlas
}

/// Straight-alpha RGBA `source` over `target`, as FFmpeg overlays one slot over the frame.
fn source_over(target: &mut [u8], source: &[u8]) {
    if target[3] == 0 {
        target.copy_from_slice(source);
        return;
    }
    let (source_alpha, target_alpha) = (f32::from(source[3]) / 255.0, f32::from(target[3]) / 255.0);
    let alpha = source_alpha + target_alpha * (1.0 - source_alpha);
    for channel in 0..3 {
        let color = f32::from(source[channel]) * source_alpha + f32::from(target[channel]) * target_alpha * (1.0 - source_alpha);
        target[channel] = (color / alpha).round() as u8;
    }
    target[3] = (alpha * 255.0).round() as u8;
}

/// Whether two slots cover a common scene pixel or one slot holds several layers.
fn overlapping_widgets_are_planned(atlas: &ovrley_core::render::AtlasPlan) -> bool {
    let slots = atlas.slots();
    slots.iter().any(|slot| slot.layers.len() > 1)
        || slots.iter().enumerate().any(|(index, slot)| {
            slots[index + 1..]
                .iter()
                .any(|other| slot.scene.left < other.scene.right && other.scene.left < slot.scene.right && slot.scene.top < other.scene.bottom && other.scene.top < slot.scene.bottom)
        })
}

#[test]
fn atlas_frames_reassemble_the_full_frames_of_the_fixture_templates() {
    use ovrley_core::activity::parse_activity_json;
    let fixtures = common::test_config::fixtures();
    let activity = parse_activity_json(&std::fs::read_to_string(fixtures.join("activity/gpx-parse-debug.json")).unwrap()).unwrap();
    for template in ["test-template-4k.json", "render-baseline-rich.json"] {
        let file: Value = serde_json::from_str(&std::fs::read_to_string(fixtures.join("config").join(template)).unwrap()).unwrap();
        let mut config = file.get("config").cloned().unwrap_or_else(|| file.clone());
        if let Some(globals) = file.pointer("/settings/globalDefaults").and_then(Value::as_object) {
            config["scene"].as_object_mut().unwrap().extend(globals.clone());
        }
        config["scene"].as_object_mut().unwrap().entry("opacity").or_insert(json!(1.0));
        // A short window keeps 4K frames cheap; a custom range exercises the plot window.
        config["scene"]["start"] = json!(600.0);
        config["scene"]["end"] = json!(600.4);
        config["scene"]["custom_export_range_active"] = json!(true);
        let config = common::seam::validated_config_from_value(config);
        let atlas = assert_atlas_frames_match_the_full_frame(template, &config, &activity);
        let frame = config.scene.presentation.width as u64 * config.scene.presentation.height as u64;
        assert!(atlas.slots().len() > 2, "{template}: planned {atlas:?}");
        assert!(overlapping_widgets_are_planned(&atlas), "{template}: overlapping widgets must share or overlap slots");
        assert!(atlas.size().width as u64 * atlas.size().height as u64 * 4 < frame * 3, "{template}: atlas {:?}", atlas.size());
    }
}

#[test]
fn atlas_planning_of_a_ten_minute_export_runs_layout_only_within_seconds() {
    use ovrley_core::{activity::{build_dense_activity_report_validated, parse_activity_json}, debug::RenderProfiler, paths::AppPaths, render::{ExportWidgets, FrameSize}};
    let fixtures = common::test_config::fixtures();
    let activity = parse_activity_json(&std::fs::read_to_string(fixtures.join("activity/gpx-parse-debug.json")).unwrap()).unwrap();
    let file: Value = serde_json::from_str(&std::fs::read_to_string(fixtures.join("config/render-baseline-rich.json")).unwrap()).unwrap();
    let mut config = file.get("config").cloned().unwrap_or_else(|| file.clone());
    config["scene"].as_object_mut().unwrap().entry("opacity").or_insert(json!(1.0));
    config["scene"]["fps"] = json!(60.0);
    config["scene"]["start"] = json!(60.0);
    config["scene"]["end"] = json!(660.0);
    config["scene"]["custom_export_range_active"] = json!(true);
    let config = common::seam::validated_config_from_value(config);
    let root = common::test_config::repo_git_root();
    let paths = AppPaths::from_runtime_roots(root.clone(), root, std::env::temp_dir());
    let dense = build_dense_activity_report_validated(&activity, &config).unwrap();
    assert!(dense.frame_count >= 36_000, "a 10-minute export at 60 fps, got {} frames", dense.frame_count);
    let size = FrameSize { width: config.scene.presentation.width, height: config.scene.presentation.height };
    let mut profiler = RenderProfiler::default();
    let export = ExportWidgets::prepare(&paths.font_dirs, &config, &activity, &dense, size, 0, &mut profiler).unwrap();
    let frames: Vec<usize> = (0..dense.frame_count).collect();
    let atlas = export.plan_atlas(&frames, ovrley_core::render::SlotPlacement::Overlay, &mut profiler).unwrap();
    let planning = &profiler.summary()["atlas.plan"];
    eprintln!("planned {} frames into {:?} in {:.0} ms", frames.len(), atlas.size(), planning.total_ms);
    // Unoptimized test builds are several times slower than the shipped export.
    let budget_ms = if cfg!(debug_assertions) { 30_000.0 } else { 3_000.0 };
    assert!(planning.total_ms < budget_ms, "planning took {:.0} ms", planning.total_ms);
    assert!(profiler.summary().contains_key("atlas.plan.pack"));
}

#[test]
fn atlas_frames_reassemble_overlapping_rotated_widgets_of_every_family_into_the_full_frame() {
    use ovrley_core::{
        normalize::{parse_config_value, validate_render_config_with_resources},
        raster::{load_embedded_raster, RasterResourceResolver, SelectedRaster},
    };
    use std::sync::Arc;
    struct Resources(Arc<SelectedRaster>);
    impl RasterResourceResolver for Resources {
        fn resolve(&self, resource_id: &str) -> Option<Arc<SelectedRaster>> {
            (resource_id == "selected").then(|| Arc::clone(&self.0))
        }
    }
    // Larger than its frame, so export resamples it; alpha varies across the image.
    let mut encoded = Vec::new();
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(400, 250, |x, y| {
        image::Rgba([(x % 256) as u8, (y % 256) as u8, 90, (40 + (x + y) % 216) as u8])
    }))
    .write_to(&mut std::io::Cursor::new(&mut encoded), image::ImageFormat::Png)
    .unwrap();
    let resources = Resources(Arc::new(load_embedded_raster("png", encoded).unwrap()));

    let (_, base) = lean_angle_preview();
    let mut scene = base["scene"].clone();
    scene["fps"] = json!(8.0);
    scene["start"] = json!(0.25);
    scene["end"] = json!(3.75);
    scene["custom_export_range_active"] = json!(true);
    let mut config = json!({ "scene": scene, "backdrops": [], "rasters": [], "labels": [], "values": [], "plots": [] });
    // A 130 px grid at scene scale 1.7 makes neighbouring widgets overlap.
    for (index, (_, request)) in every_family_preview().into_iter().enumerate() {
        let kind = request["type"].as_str().unwrap().to_owned();
        let mut widget = request["widget"].clone();
        widget["x"] = json!(30.25 + (index % 4) as f64 * 130.0);
        widget["y"] = json!(20.5 + (index / 4) as f64 * 110.0);
        let collection = match kind.as_str() {
            "label" => "labels",
            "backdrop" => {
                // The backdrop sits under every other layer.
                widget["x"] = json!(10.5);
                widget["y"] = json!(10.5);
                widget["width"] = json!(420);
                widget["height"] = json!(260);
                "backdrops"
            }
            "raster" => {
                widget.as_object_mut().unwrap().extend(json!({
                    "width": 160.0, "height": 100.0, "rotation": 15.0, "opacity": 0.8, "resourceId": "selected",
                    "path": std::env::temp_dir().join("selected.png"),
                }).as_object().unwrap().clone());
                "rasters"
            }
            "rolling_chart" => {
                widget["value"] = json!(kind);
                "plots"
            }
            "course" | "elevation" | "diving_profile" => {
                widget["value"] = json!(kind);
                widget["rotation"] = json!(match kind.as_str() { "course" => 27.0, "elevation" => 230.0, _ => 37.0 });
                "plots"
            }
            // Text values overlap only the raster and the route.
            "speed" | "time" => {
                widget["x"] = json!(if kind == "speed" { 180.25 } else { 560.25 });
                widget["y"] = json!(if kind == "speed" { 400.5 } else { 520.5 });
                "values"
            }
            _ => "values",
        };
        config[collection].as_array_mut().unwrap().push(widget);
    }
    let config = validate_render_config_with_resources(parse_config_value(&config).unwrap(), Some(&resources)).unwrap();
    assert_eq!(config.values.len(), 16, "every value family is exported");

    let atlas = assert_atlas_frames_match_the_full_frame("every family", &config, &every_family_parsed_activity());
    // Every layer is visible; neighbouring widgets either share a slot or overlay in layer order.
    let mut layers: Vec<_> = atlas.slots().iter().flat_map(|slot| slot.layers.clone()).collect();
    layers.sort_unstable();
    assert_eq!(layers, (0..23).collect::<Vec<_>>());
    assert!(overlapping_widgets_are_planned(&atlas), "every family: {atlas:?}");
}

#[test]
fn widgets_accept_the_native_font_and_raster_resolvers_used_by_export() {
    use ovrley_core::raster::{load_embedded_raster, RasterResourceResolver, SelectedRaster};
    use ovrley_widgets::preview_widget::PreviewWidget;
    use std::sync::Arc;
    struct Resources(Arc<SelectedRaster>);
    impl RasterResourceResolver for Resources {
        fn resolve(&self, resource_id: &str) -> Option<Arc<SelectedRaster>> {
            (resource_id == "selected").then(|| Arc::clone(&self.0))
        }
    }
    let mut encoded = Vec::new();
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(6, 4, |x, y| image::Rgba([x as u8 * 40, y as u8 * 60, 90, 200])))
        .write_to(&mut std::io::Cursor::new(&mut encoded), image::ImageFormat::Png)
        .unwrap();
    let selected = Arc::new(load_embedded_raster("png", encoded).unwrap());
    let resources: &dyn RasterResourceResolver = &Resources(Arc::clone(&selected));
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let fonts = NativeFonts(&font_dirs);

    let (mut raster_preview, mut raster) = lean_angle_preview();
    raster_preview.images.register("selected".into(), selected.preview_png()).unwrap();
    raster["type"] = json!("raster");
    raster["widget"] = json!({ "id":"raster", "x":20.5, "y":10.0, "width":30.0, "height":20.0, "rotation":15.0, "opacity":0.8, "path":"/image.png", "resourceId":"selected" });
    let request_scene = raster["scene"].clone();
    for (mut preview, request) in [(raster_preview, raster), label_preview()] {
        let kind = request["type"].as_str().unwrap().to_owned();
        let native = PreviewWidget::new(serde_json::from_value(request.clone()).unwrap(), &fonts, resources).unwrap();
        let id = preview.create_widget(serde_json::from_value(request).unwrap()).unwrap();
        let session = &preview.widgets[&id];
        assert_eq!(native.pixels(), session.pixels(), "{kind} pixels must not depend on the host");
        assert_eq!(native.geometry(), session.geometry(), "{kind} geometry must not depend on the host");
        assert_eq!(native.bounds(), session.bounds(), "{kind} bounds must not depend on the host");
    }
    let missing = json!({ "type":"raster", "widget": { "id":"raster", "x":0, "y":0, "width":1, "height":1, "rotation":0, "opacity":1, "path":"/image.png", "resourceId":"missing" }, "scene": request_scene });
    assert!(PreviewWidget::new(serde_json::from_value(missing).unwrap(), &fonts, resources).is_err(), "unresolved native rasters must fail loudly");
}

#[test]
fn elevation_preview_matches_export_labels_hover_seeking_and_activity_replacement() {
    use ovrley_core::{activity::{interpolate::densify_activity, trim::trim_activity}, debug::RenderProfiler, paths::AppPaths, render::render_full_frames};
    let activity = elevation_activity();
    let bytes = serde_json::to_vec(&activity).unwrap();
    let root = common::test_config::repo_git_root();
    let paths = AppPaths::from_runtime_roots(root.clone(), root, std::env::temp_dir());
    for (variant, full) in [("single", true), ("ring", false), ("halo", false)] {
        let mut request = elevation_request();
        request["widget"]["marker_variant"] = json!(variant);
        request["widget"]["show_full_activity"] = json!(full);
        request["scene"]["custom_export_range_active"] = json!(true);
        request["scene"]["start"] = json!(0.25);
        request["scene"]["end"] = json!(3.25);
        let config = common::seam::validated_config_from_value(json!({"scene": request["scene"], "values": [], "labels": [], "plots": [request["widget"]]}));
        let requirements = config.render_data_requirements();
        let trimmed = trim_activity(&activity, 0.25, 3.25, &requirements).unwrap();
        let seconds = vec![0.0, 0.75, 1.5, 2.2, 2.9];
        let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
        let frames = render_full_frames(&paths.font_dirs, &config, &activity, &dense, &(0..dense.frame_count).collect::<Vec<_>>(), &mut RenderProfiler::default()).unwrap();
        let (mut preview, id) = elevation_preview(&request, &bytes);
        let raw = serde_json::from_value(request["widget"].clone()).unwrap();
        let scene = validate_scene_presentation(serde_json::from_value(request["scene"].clone()).unwrap()).unwrap();
        assert_eq!(config.elevation_plots[0], ovrley_widgets::widgets::elevation::validation::validate_elevation_plot(&raw, 0, &scene).unwrap());
        let mut placement = None;
        for index in [0, 1, 2, 3, 4, 2, 0] {
            let second = 0.25 + seconds[index];
            preview.render(second).unwrap();
            let widget = &preview.widgets[&id];
            let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
            if let Some(placement) = &placement { assert_eq!(widget.geometry(), placement, "playback must retain raster and selection bounds"); }
            else { placement = Some(widget.geometry().clone()); }
            let (w, h, left, top) = (geometry["width"].as_u64().unwrap() as usize, geometry["height"].as_u64().unwrap() as usize, geometry["left"].as_f64().unwrap() as usize, geometry["top"].as_f64().unwrap() as usize);
            let native = &frames[index];
            let mut difference = 0;
            let mut largest = (0, 0, [0; 4], [0; 4]);
            for y in 0..h {
                let native = &native[((top + y) * 700 + left) * 4..((top + y) * 700 + left + w) * 4];
                let pixels = &widget.pixels()[y * w * 4..(y + 1) * w * 4];
                for (x, (a, b)) in native.chunks_exact(4).zip(pixels.chunks_exact(4)).enumerate() {
                    let previous = difference;
                    difference = difference.max(a[3].abs_diff(b[3]) as u16);
                    for c in 0..3 { difference = difference.max((a[c] as u16 * a[3] as u16).abs_diff(b[c] as u16 * b[3] as u16) / 255); }
                    if difference > previous { largest = (x + left, y + top, <[u8; 4]>::try_from(a).unwrap(), <[u8; 4]>::try_from(b).unwrap()); }
                }
            }
            assert!(difference <= 8, "{variant}/{full} export elevation differs at {second}: {difference}; {largest:?}");
            assert!(widget.pixels().chunks_exact(4).any(|pixel| pixel[3] > 0));
            let revision = widget.revision();
            preview.render(second).unwrap();
            assert_eq!(preview.widgets[&id].revision(), revision);
            preview.update_widget(id, serde_json::from_value(request.clone()).unwrap()).unwrap();
            preview.render(second).unwrap();
            assert_eq!(preview.widgets[&id].revision(), revision, "identical config must retain cached presentation");
        }
        let pixels = preview.widgets[&id].pixels().clone();
        preview.replace_activity(&[]).unwrap();
        preview.render(0.25).unwrap();
        assert_ne!(preview.widgets[&id].pixels(), &pixels);
        preview.replace_activity(&bytes).unwrap();
        preview.render(0.25).unwrap();
        assert_eq!(preview.widgets[&id].pixels(), &pixels);
        let mut replacement = activity.clone();
        replacement.barometric_altitude[1] = Some(210.0);
        replacement.sample_elevations[1] = Some(210.0);
        preview.replace_activity(&serde_json::to_vec(&replacement).unwrap()).unwrap();
        preview.render(0.25).unwrap();
        assert_ne!(preview.widgets[&id].pixels(), &pixels);
        preview.render(5.0).unwrap();
        assert_ne!(preview.widgets[&id].pixels(), &pixels);
    }
    let request = elevation_request();
    let (mut preview, _) = elevation_preview(&request, &bytes);
    for (field, value) in [("width", json!(0)), ("height", json!("145")), ("marker_variant", json!("unknown")), ("show_full_activity", json!("true")), ("y_scale", json!(9)), ("starting_altitude", json!("bad"))] {
        let mut invalid = request.clone();
        invalid["widget"][field] = value;
        let decoded = serde_json::from_value::<ovrley_widgets::preview_widget::WidgetRequest>(invalid);
        assert!(decoded.is_err() || preview.create_widget(decoded.unwrap()).is_err(), "malformed {field} must fail");
    }
    let mut missing = request;
    missing["widget"].as_object_mut().unwrap().remove("remaining_line_width");
    assert!(serde_json::from_value::<ovrley_widgets::preview_widget::WidgetRequest>(missing).is_err());
}

#[test]
fn elevation_areas_stop_at_the_inner_stroke_edge() {
    let mut activity = elevation_activity();
    activity.sample_elevations.fill(Some(100.0));
    activity.barometric_altitude.fill(Some(100.0));
    let bytes = serde_json::to_vec(&activity).unwrap();
    for (completed, width, second) in [(false, 24.0, 0.0), (true, 40.0, 4.0)] {
        let mut request = elevation_request();
        request["widget"].as_object_mut().unwrap().extend(json!({
            "x": 40, "y": 40, "width": 240, "height": 160, "rotation": 0, "y_scale": 0.5,
            "show_elevation_metric": false, "show_elevation_imperial": false,
            "remaining_line_width": 24, "completed_line_width": 40,
            "remaining_line_opacity": 0, "completed_line_opacity": 0, "marker_opacity": 0,
            "area_remaining_opacity": if completed { 0 } else { 100 },
            "area_completed_opacity": if completed { 100 } else { 0 }
        }).as_object().unwrap().clone());
        request["scene"].as_object_mut().unwrap().extend(json!({
            "scale": 1, "opacity": 1, "shadow_strength": 0, "shadow_distance": 0, "border_thickness": 0
        }).as_object().unwrap().clone());
        let (mut preview, id) = elevation_preview(&request, &bytes);
        preview.render(second).unwrap();
        let widget = &preview.widgets[&id];
        let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
        let left = geometry["left"].as_f64().unwrap() as usize;
        let top = geometry["top"].as_f64().unwrap() as usize;
        let raster_width = geometry["width"].as_u64().unwrap() as usize;
        let alpha = |y: usize| widget.pixels()[((y - top) * raster_width + 160 - left) * 4 + 3];
        let edge = 160 + (width * 0.5) as usize;
        assert_eq!(alpha(161), 0, "area must leave the stroke interior clear");
        assert_eq!(alpha(edge - 2), 0, "area must stop at its own stroke width");
        assert_eq!(alpha(edge + 2), 255, "area must reach the inner stroke edge");
    }
}

#[test]
fn elevation_area_gradient_fades_both_areas_from_plot_top_to_transparent_bottom() {
    let mut activity = elevation_activity();
    activity.sample_elevations.fill(Some(100.0));
    activity.barometric_altitude.fill(Some(100.0));
    let bytes = serde_json::to_vec(&activity).unwrap();
    for (gradient, completed, second) in [(false, false, 0.0), (true, false, 0.0), (false, true, 4.0), (true, true, 4.0)] {
        let mut request = elevation_request();
        request["widget"].as_object_mut().unwrap().extend(json!({
            "x": 40, "y": 40, "width": 240, "height": 160, "rotation": 0, "y_scale": 0.5,
            "show_elevation_metric": false, "show_elevation_imperial": false,
            "remaining_line_width": 1, "completed_line_width": 1,
            "remaining_line_opacity": 0, "completed_line_opacity": 0, "marker_opacity": 0,
            "area_remaining_opacity": if completed { 0 } else { 100 },
            "area_completed_opacity": if completed { 100 } else { 0 },
            "area_gradient": gradient
        }).as_object().unwrap().clone());
        request["scene"].as_object_mut().unwrap().extend(json!({
            "scale": 1, "opacity": 1, "shadow_strength": 0, "shadow_distance": 0, "border_thickness": 0
        }).as_object().unwrap().clone());
        let (mut preview, id) = elevation_preview(&request, &bytes);
        preview.render(second).unwrap();
        let widget = &preview.widgets[&id];
        let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
        let left = geometry["left"].as_f64().unwrap() as usize;
        let top = geometry["top"].as_f64().unwrap() as usize;
        let raster_width = geometry["width"].as_u64().unwrap() as usize;
        let alpha = |y: usize| widget.pixels()[((y - top) * raster_width + 160 - left) * 4 + 3] as i32;
        if gradient {
            // Plot spans y 40..200: alpha follows 255 * (200 - y) / 160 regardless of the area's own top.
            assert!((alpha(170) - 48).abs() <= 3, "gradient alpha near area top, got {}", alpha(170));
            assert!((alpha(195) - 8).abs() <= 3, "gradient alpha near plot bottom, got {}", alpha(195));
        } else {
            assert_eq!((alpha(170), alpha(195)), (255, 255), "solid area keeps uniform opacity");
        }
    }
}

#[test]
fn elevation_resize_updates_size_reuses_geometry_until_commit_and_preserves_fixed_corners() {
    use std::sync::Arc;
    let opposite = |geometry: &Value, fixed: [f64; 2]| {
        let c = &geometry["content"];
        let angle = geometry["rotation"].as_f64().unwrap().to_radians();
        let (ox, oy) = (c["width"].as_f64().unwrap() * 0.5, c["height"].as_f64().unwrap() * 0.5);
        let (dx, dy) = (c["width"].as_f64().unwrap() * fixed[0] - ox, c["height"].as_f64().unwrap() * fixed[1] - oy);
        [c["left"].as_f64().unwrap() + ox + dx * angle.cos() - dy * angle.sin(), c["top"].as_f64().unwrap() + oy + dx * angle.sin() + dy * angle.cos()]
    };
    for (fixed, flat) in [([0.0, 0.0], false), ([0.0, 1.0], false), ([1.0, 0.0], false), ([1.0, 1.0], false), ([0.5, 1.0], true)] {
        let mut activity = elevation_activity();
        if flat {
            activity.sample_elevations.fill(Some(100.0));
            activity.barometric_altitude.fill(Some(100.0));
        }
        let bytes = serde_json::to_vec(&activity).unwrap();
        let request = elevation_request();
        let (mut preview, id) = elevation_preview(&request, &bytes);
        preview.render(0.75).unwrap();
        let original = prepared_elevation(&preview, id);
        let original_pixels = preview.widgets[&id].pixels().clone();
        let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
        let point = opposite(&geometry, fixed);
        let resize_request = |size, second, commit| {
            let mut resize = request.clone();
            resize.as_object_mut().unwrap().extend(json!({"handle": id, "size": size, "fixed": fixed, "point": point, "second": second, "commit": commit}).as_object().unwrap().clone());
            serde_json::from_value(resize).unwrap()
        };
        for (size, second) in [([235.125, 230.375], 0.75), ([425.375, 120.125], 2.15)] {
            let patch = serde_json::to_value(preview.resize_widget(resize_request(size, second, false)).unwrap()).unwrap();
            let mut live = request.clone();
            live["widget"].as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
            live["transient"] = json!(true);
            preview.update_widget(id, serde_json::from_value(live).unwrap()).unwrap();
            preview.render(second).unwrap();
            assert!(Arc::ptr_eq(&original, &prepared_elevation(&preview, id)), "live resizing must retain prepared vectors");
            let live: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
            let anchored = opposite(&live, fixed);
            assert!((anchored[0] - point[0]).abs() < 0.001 && (anchored[1] - point[1]).abs() < 0.001);
            assert!((live["content"]["width"].as_f64().unwrap() - size[0]).abs() < 0.001);
            assert!((live["content"]["height"].as_f64().unwrap() - size[1]).abs() < 0.001);
            assert_ne!(preview.widgets[&id].pixels(), &original_pixels, "the resized profile must redraw immediately");
        }
        preview.update_widget(id, serde_json::from_value(request.clone()).unwrap()).unwrap();
        preview.render(0.75).unwrap();
        assert!(Arc::ptr_eq(&original, &prepared_elevation(&preview, id)));
        assert_eq!(preview.widgets[&id].pixels(), &original_pixels, "cancel restores committed pixels");
        let patch = serde_json::to_value(preview.resize_widget(resize_request([425.375, 120.125], 2.15, true)).unwrap()).unwrap();
        let prepared = prepared_elevation(&preview, id);
        assert!(!Arc::ptr_eq(&original, &prepared), "commit rebuilds profile geometry");
        assert_ne!(patch["width"].as_f64().unwrap().fract(), 0.0);
        let mut committed = request;
        committed["widget"].as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
        preview.update_widget(id, serde_json::from_value(committed.clone()).unwrap()).unwrap();
        preview.render(2.15).unwrap();
        assert!(Arc::ptr_eq(&prepared, &prepared_elevation(&preview, id)), "committed snapshot must reuse the final preparation");
        let live: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
        let anchored = opposite(&live, fixed);
        assert!((anchored[0] - point[0]).abs() < 0.001 && (anchored[1] - point[1]).abs() < 0.001);
        let (mut reloaded, reloaded_id) = elevation_preview(&committed, &bytes);
        reloaded.render(2.15).unwrap();
        assert_eq!(preview.widgets[&id].geometry(), reloaded.widgets[&reloaded_id].geometry());
        assert_eq!(preview.widgets[&id].pixels(), reloaded.widgets[&reloaded_id].pixels());
    }
}

fn route_request() -> Value {
    let defaults: Value = serde_json::from_str(include_str!("../../../assets/standard-widgets.json")).unwrap();
    let mut widget = defaults["definitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|definition| definition["type"] == "course")
        .unwrap()["defaults"]
        .clone();
    widget["id"] = json!("route");
    widget["x"] = json!(150.25);
    widget["y"] = json!(90.5);
    widget["width"] = json!(300.125);
    widget["height"] = json!(170.25);
    widget["rotation"] = json!(27.0);
    let mut scene = common::seam::explicit_scene_json();
    scene["width"] = json!(700);
    scene["height"] = json!(500);
    scene["scale"] = json!(1.25);
    scene["opacity"] = json!(0.65);
    scene["shadow_strength"] = json!(2.0);
    scene["shadow_distance"] = json!(3.0);
    scene["preview_duration_seconds"] = json!(4.0);
    json!({"type": "course", "widget": widget, "scene": scene})
}

fn route_activity() -> ovrley_core::activity::schema::ParsedActivity {
    let points = json!([[48.0, 12.0], [48.01, 12.005], [null, null], [48.013, 12.02], [48.02, 12.013]]);
    ovrley_core::activity::parse_activity_json(&json!({
        "sample_elapsed_seconds": [0.0, 0.5, 1.5, 2.5, 4.0], "trim_end_seconds": 4.0,
        "sample_distance_progress": [0.0, 0.01, 0.47, 0.8, 1.0],
        "sample_course_points": points, "course": points,
    }).to_string()).unwrap()
}

fn prepared_route(preview: &PreviewSession, id: u32) -> std::sync::Arc<ovrley_widgets::widgets::route::geometry::RouteGeometry> {
    let ovrley_widgets::preview_widget::PreviewWidget::Route(route) = &preview.widgets[&id].widget else { panic!("expected route handle"); };
    route.prepared_geometry.as_ref().unwrap().clone()
}

#[test]
fn route_preview_matches_export_and_keeps_marker_out_of_selection_bounds() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, trim::trim_activity}, debug::RenderProfiler, paths::AppPaths,
        render::render_full_frames,
    };
    let activity = route_activity();
    let bytes = serde_json::to_vec(&activity).unwrap();
    let root = common::test_config::repo_git_root();
    let paths = AppPaths::from_runtime_roots(root.clone(), root, std::env::temp_dir());
    for (variant, full) in [("single", true), ("ring", false), ("halo", false)] {
        let mut request = route_request();
        request["widget"]["marker_variant"] = json!(variant);
        request["widget"]["marker_size"] = json!(30.0);
        request["widget"]["marker_variant_diameter"] = json!(95.0);
        request["widget"]["show_full_activity"] = json!(full);
        request["scene"]["custom_export_range_active"] = json!(true);
        request["scene"]["start"] = json!(0.25);
        request["scene"]["end"] = json!(3.25);
        let config = common::seam::validated_config_from_value(json!({"scene": request["scene"], "values": [], "labels": [], "plots": [request["widget"]]}));
        let requirements = config.render_data_requirements();
        let trimmed = trim_activity(&activity, 0.25, 3.25, &requirements).unwrap();
        let seconds = vec![0.0, 0.75, 2.2, 2.9];
        let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
        let frames = render_full_frames(&paths.font_dirs, &config, &activity, &dense, &(0..dense.frame_count).collect::<Vec<_>>(), &mut RenderProfiler::default()).unwrap();
        let mut preview = PreviewSession::default();
        // Compared against native export, which grid-fits text.
        preview.fonts.set_grid_fit(true);
        preview.replace_activity(&bytes).unwrap();
        let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
        let mut selection = None;
        for index in [0, 1, 2, 3, 1, 0] {
            let second = 0.25 + seconds[index];
            preview.render(second).unwrap();
            let widget = &preview.widgets[&id];
            let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
            if let Some(selection) = &selection { assert_eq!(&geometry["content"], selection, "marker movement must not resize selection"); }
            else { selection = Some(geometry["content"].clone()); }
            let width = geometry["width"].as_u64().unwrap() as usize;
            let height = geometry["height"].as_u64().unwrap() as usize;
            let left = geometry["left"].as_f64().unwrap() as usize;
            let top = geometry["top"].as_f64().unwrap() as usize;
            let native = &frames[index];
            let mut max_difference = 0;
            for y in 0..height {
                let native_row = &native[((top + y) * 700 + left) * 4..((top + y) * 700 + left + width) * 4];
                let preview_row = &widget.pixels()[y * width * 4..(y + 1) * width * 4];
                for (a, b) in native_row.chunks_exact(4).zip(preview_row.chunks_exact(4)) {
                    max_difference = max_difference.max(a[3].abs_diff(b[3]) as u16);
                    // Native video draws into straight alpha; compare visible premultiplied color.
                    for channel in 0..3 {
                        let native = a[channel] as u16 * a[3] as u16;
                        let preview = b[channel] as u16 * b[3] as u16;
                        max_difference = max_difference.max(native.abs_diff(preview) / 255);
                    }
                }
            }
            // Allow edge quantization between native straight-alpha and preview premultiplied surfaces.
            assert!(max_difference <= 8, "{variant}/{full} export route differs at {second}: {max_difference}");
            assert!(widget.pixels().chunks_exact(4).any(|pixel| pixel[3] > 0));
            let revision = widget.revision();
            preview.render(second).unwrap();
            assert_eq!(preview.widgets[&id].revision(), revision);
        }
        let pixels = preview.widgets[&id].pixels().clone();
        preview.replace_activity(&[]).unwrap();
        preview.render(0.25).unwrap();
        assert_ne!(preview.widgets[&id].pixels(), &pixels);
        preview.replace_activity(&bytes).unwrap();
        preview.render(0.25).unwrap();
        assert_eq!(preview.widgets[&id].pixels(), &pixels);
        let mut replacement = activity.clone();
        replacement.sample_course_points[1] = (Some(48.03), Some(12.035));
        preview.replace_activity(&serde_json::to_vec(&replacement).unwrap()).unwrap();
        preview.render(0.25).unwrap();
        assert_ne!(preview.widgets[&id].pixels(), &pixels);
        for (field, value) in [("width", json!(0)), ("width", json!("300")), ("marker_variant", json!("unknown")), ("show_full_activity", json!("true"))] {
            let mut invalid = request.clone();
            invalid["widget"][field] = value;
            let request = serde_json::from_value::<ovrley_widgets::preview_widget::WidgetRequest>(invalid);
            assert!(request.is_err() || preview.create_widget(request.unwrap()).is_err());
        }
        let mut missing = request.clone();
        missing["widget"].as_object_mut().unwrap().remove("width");
        assert!(serde_json::from_value::<ovrley_widgets::preview_widget::WidgetRequest>(missing).is_err());
    }
}

#[test]
fn route_gestures_reuse_geometry_until_resize_commit_and_keep_every_opposite_corner_fixed() {
    use std::sync::Arc;
    let bytes = serde_json::to_vec(&route_activity()).unwrap();
    let opposite = |geometry: &Value, fixed: [f64; 2]| {
        let content = &geometry["content"];
        let radians = geometry["rotation"].as_f64().unwrap().to_radians();
        let ox = geometry["rotation_origin"][0].as_f64().unwrap();
        let oy = geometry["rotation_origin"][1].as_f64().unwrap();
        let dx = content["width"].as_f64().unwrap() * fixed[0] - ox;
        let dy = content["height"].as_f64().unwrap() * fixed[1] - oy;
        [content["left"].as_f64().unwrap() + ox + dx * radians.cos() - dy * radians.sin(), content["top"].as_f64().unwrap() + oy + dx * radians.sin() + dy * radians.cos()]
    };
    for fixed in [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]] {
        let request = route_request();
        let mut preview = PreviewSession::default();
        // Compared against native export, which grid-fits text.
        preview.fonts.set_grid_fit(true);
        preview.replace_activity(&bytes).unwrap();
        let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
        preview.render(0.75).unwrap();
        let original = prepared_route(&preview, id);
        let original_pixels = preview.widgets[&id].pixels().clone();
        let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
        let point = opposite(&geometry, fixed);
        for rotation in [-125.75, 0.0, 90.0] {
            let mut rotated = request.clone();
            rotated["widget"]["rotation"] = json!(rotation);
            preview.update_widget(id, serde_json::from_value(rotated).unwrap()).unwrap();
            preview.render(0.75).unwrap();
            let rotated_geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
            assert_eq!(geometry["content"]["width"], rotated_geometry["content"]["width"]);
            assert_eq!(geometry["content"]["height"], rotated_geometry["content"]["height"]);
            assert!(Arc::ptr_eq(&original, &prepared_route(&preview, id)), "rotation must not rebuild curves");
        }
        let mut translated = request.clone();
        translated["widget"]["x"] = json!(180.375);
        translated["widget"]["y"] = json!(115.25);
        preview.update_widget(id, serde_json::from_value(translated).unwrap()).unwrap();
        preview.render(0.75).unwrap();
        assert!(Arc::ptr_eq(&original, &prepared_route(&preview, id)), "drag must reuse projection and curves");
        preview.update_widget(id, serde_json::from_value(request.clone()).unwrap()).unwrap();
        preview.render(0.75).unwrap();
        for (factor, second) in [(0.625, 0.75), (1.375, 2.35)] {
            let mut resize = request.clone();
            resize.as_object_mut().unwrap().extend(json!({"handle": id, "factor": factor, "fixed": fixed, "point": point, "second": second, "commit": false}).as_object().unwrap().clone());
            let patch = serde_json::to_value(preview.resize_widget(serde_json::from_value(resize).unwrap()).unwrap()).unwrap();
            let mut live = request.clone();
            live["widget"].as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
            live["transient"] = json!(true);
            preview.update_widget(id, serde_json::from_value(live).unwrap()).unwrap();
            preview.render(second).unwrap();
            assert!(Arc::ptr_eq(&original, &prepared_route(&preview, id)), "live resize must scale existing vectors");
            let live_geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
            let anchored = opposite(&live_geometry, fixed);
            assert!((anchored[0] - point[0]).abs() < 0.001 && (anchored[1] - point[1]).abs() < 0.001);
            assert_eq!(patch["width"].as_f64().unwrap(), request["widget"]["width"].as_f64().unwrap() * factor);
        }
        // Escape restores the committed geometry without another preparation.
        preview.update_widget(id, serde_json::from_value(request.clone()).unwrap()).unwrap();
        preview.render(0.75).unwrap();
        assert!(Arc::ptr_eq(&original, &prepared_route(&preview, id)));
        assert_eq!(preview.widgets[&id].pixels(), &original_pixels);
        let mut resize = request.clone();
        resize.as_object_mut().unwrap().extend(json!({"handle": id, "factor": 1.375, "fixed": fixed, "point": point, "second": 2.35, "commit": true}).as_object().unwrap().clone());
        let patch = serde_json::to_value(preview.resize_widget(serde_json::from_value(resize).unwrap()).unwrap()).unwrap();
        let committed_geometry = prepared_route(&preview, id);
        assert!(!Arc::ptr_eq(&original, &committed_geometry), "prepare once at commit");
        let mut committed = request.clone();
        committed["widget"].as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
        preview.update_widget(id, serde_json::from_value(committed.clone()).unwrap()).unwrap();
        preview.render(2.35).unwrap();
        assert!(Arc::ptr_eq(&committed_geometry, &prepared_route(&preview, id)), "committed snapshot reuses the final preparation");
        let final_geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
        let anchored = opposite(&final_geometry, fixed);
        assert!((anchored[0] - point[0]).abs() < 0.001 && (anchored[1] - point[1]).abs() < 0.001);
        let mut reloaded = PreviewSession::default();
        // Compared against native export, which grid-fits text.
        reloaded.fonts.set_grid_fit(true);
        reloaded.replace_activity(&bytes).unwrap();
        let reloaded_id = reloaded.create_widget(serde_json::from_value(committed).unwrap()).unwrap();
        reloaded.render(2.35).unwrap();
        assert_eq!(preview.widgets[&id].geometry(), reloaded.widgets[&reloaded_id].geometry());
        assert_eq!(preview.widgets[&id].pixels(), reloaded.widgets[&reloaded_id].pixels());
    }
}

/// Preview tiles are drawn premultiplied and unpremultiplied with integer rounding, export frames
/// are drawn straight; soft shadow and anti-aliased edges round apart by a few levels.
const EXPORT_TILE_TOLERANCE: u16 = 12;

/// Largest premultiplied channel difference between a straight-alpha preview tile and the same
/// scene rectangle of a straight-alpha export frame `frame_width` pixels wide.
fn export_tile_difference(frame: &[u8], frame_width: usize, geometry: &Value, tile: &[u8]) -> u16 {
    let width = geometry["width"].as_u64().unwrap() as usize;
    let (left, top) = (geometry["left"].as_f64().unwrap() as i64, geometry["top"].as_f64().unwrap() as i64);
    let frame_height = frame.len() / 4 / frame_width;
    let mut difference = 0;
    for (index, b) in tile.chunks_exact(4).enumerate() {
        let (x, y) = (left + (index % width) as i64, top + (index / width) as i64);
        let a = if x < 0 || y < 0 || x as usize >= frame_width || y as usize >= frame_height {
            &[0; 4][..]
        } else {
            let at = (y as usize * frame_width + x as usize) * 4;
            &frame[at..at + 4]
        };
        difference = difference.max(a[3].abs_diff(b[3]) as u16);
        for c in 0..3 {
            difference = difference.max((a[c] as u16 * a[3] as u16).abs_diff(b[c] as u16 * b[3] as u16) / 255);
        }
    }
    difference
}

fn gauge_preview(
    display: &str,
    metric: &str,
    placement: &str,
    fill: &str,
) -> (PreviewSession, Value) {
    let (mut preview, mut request) = lean_angle_preview();
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    // Both native transport and WASM ingress resolve omitted metric styling in Rust.
    let font: ovrley_widgets::preview::FontRequest =
        serde_json::from_value(json!({"font": "Teko.ttf"})).unwrap();
    let bytes =
        preview_font_resource(&font_dirs, &font.font, font.font_weight, font.italic).unwrap();
    preview.fonts.register(font, &bytes).unwrap();
    request["type"] = json!(metric);
    request["widget"] = json!({
        "value": metric, "display_type": display, "x": 250.25, "y": 120.5,
        "width": 190.125, "height": if display == "linear" { 47.375 } else { 190.125 }, "rotation": 0,
        "orientation": placement, "corner_orientation": placement, "arc_angle": 220,
        "inner_widget_offset_x": 11.25, "inner_widget_offset_y": -17.5,
        "track_thickness": 18, "track_corner_radius": 6, "track_border_thickness": 2,
        "track_border_color": "#ffffff", "track_empty_color": "#222222", "track_empty_opacity": 0.5,
        "track_filled_color": "#40e0d0", "track_filled_opacity": 0.7,
        "track_filled_gradient": false, "track_filled_gradient_stops": [{ "offset": 0, "color": "#40e0d0" }, { "offset": 1, "color": "#c65102" }], "track_fill_flat": false,
        "track_fill_style": fill, "bar_count": 8, "bar_gap": 3,
        "show_min_max_labels": true, "min_max_label_font": "Teko.ttf", "min_max_label_font_size": 16,
        "min_max_label_position": if placement == "vertical" { "left" } else { "bottom" }, "min_max_label_color": "#cdefab",
        "font": "Inter.ttf", "font_size": 60, "color": "#abcdef", "opacity": 0.8,
        "show_units": true, "unit_color": "#aaaaaa", "display_unit": match metric { "speed" => "kmh", "altitude" => "ft", _ => "w" },
        "prefix": "", "suffix": "", "decimals": 0,
    });
    request["widget"].as_object_mut().unwrap().extend(
        json!({"tick_count": 11, "tick_length": 14, "tick_color": "#8c8c8c", "glow": 0.6})
            .as_object()
            .unwrap()
            .clone(),
    );
    if metric == "altitude" {
        request["widget"]["starting_altitude"] = json!(4000);
    }
    (preview, request)
}

#[test]
fn gauges_match_export_pixels_sampling_cache_revisions_and_activity_replacement() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, trim::trim_activity},
        debug::RenderProfiler,
        paths::AppPaths,
        render::render_full_frames,
    };
    let activity = ovrley_core::activity::parse_activity_json(
        r#"{
        "sample_elapsed_seconds": [0,0.5,1.5,2.5,4], "trim_end_seconds": 4,
        "speed": [null,10,null,40,null], "power": [null,400,null,100,null],
        "elevation": [10,20,30,40,50], "barometric_altitude": [800,820,840,860,880]
    }"#,
    )
    .unwrap();
    let bytes = serde_json::to_vec(&activity).unwrap();
    let root = common::test_config::repo_git_root();
    let paths = AppPaths::from_runtime_roots(root.clone(), root, std::env::temp_dir());
    let seconds = vec![0.0, 0.5, 0.501, 0.75, 1.5, 2.25, 2.5, 3.75];
    for (display, metric, placement, fill, gradient) in [
        ("linear", "speed", "horizontal", "fill", false),
        ("linear", "speed", "horizontal", "fill", true),
        ("linear", "power", "vertical", "bars", true),
        ("arc", "speed", "bottom-left", "fill", false),
        ("arc", "speed", "bottom-left", "fill", true),
        ("arc", "power", "bottom-left", "bars", true),
        ("corner", "altitude", "bottom-left", "fill", true),
        ("corner", "altitude", "bottom-right", "bars", false),
        ("gt_gauge", "speed", "bottom-left", "fill", false),
        ("gt_gauge", "power", "bottom-left", "bars", false),
    ] {
        let (mut preview, mut request) = gauge_preview(display, metric, placement, fill);
        request["widget"]["track_filled_gradient"] = json!(gradient);
        request["widget"]["track_filled_gradient_stops"] = json!([
            { "offset": 0.8, "color": "#ff0000" },
            { "offset": 0, "color": "#00ff00" },
            { "offset": 0.4, "color": "#0000ff" },
        ]);
        request["scene"]["opacity"] = json!(0.5);
        preview.replace_activity(&bytes).unwrap();
        let config = common::seam::validated_config_from_value(
            json!({"scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": []}),
        );
        let requirements = config.render_data_requirements();
        let trimmed = trim_activity(&activity, 0.0, 4.0, &requirements).unwrap();
        let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
        let frames = render_full_frames(&paths.font_dirs, &config, &activity, &dense, &(0..dense.frame_count).collect::<Vec<_>>(), &mut RenderProfiler::default()).unwrap();
        let frame_width = config.scene.presentation.width as usize;
        let id = preview
            .create_widget(serde_json::from_value(request.clone()).unwrap())
            .unwrap();
        for index in [0, 1, 2, 3, 4, 5, 6, 7, 4, 0] {
            let previous = preview.widgets[&id].revision();
            preview.render(seconds[index]).unwrap();
            let widget = &preview.widgets[&id];
            if index == 2 && metric == "speed" {
                assert_ne!(
                    widget.revision(),
                    previous,
                    "fill changes below the formatted integer"
                );
            }
            let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
            let difference = export_tile_difference(&frames[index], frame_width, &geometry, widget.pixels());
            assert!(
                difference <= EXPORT_TILE_TOLERANCE,
                "{display}/{metric}/{placement}/{fill} differs from export by {difference} at {}",
                seconds[index]
            );
            assert!(widget.pixels().chunks_exact(4).any(|pixel| pixel[3] != 0));
            let revision = widget.revision();
            preview.render(seconds[index]).unwrap();
            assert_eq!(preview.widgets[&id].revision(), revision);
        }
        preview.render(2.25).unwrap();
        let pixels = preview.widgets[&id].pixels().clone();
        preview.replace_activity(&[]).unwrap();
        preview.render(2.25).unwrap();
        assert_ne!(preview.widgets[&id].pixels(), &pixels);
        preview.replace_activity(&bytes).unwrap();
        preview.render(2.25).unwrap();
        assert_eq!(preview.widgets[&id].pixels(), &pixels);
        if metric == "speed" {
            preview.render(0.5).unwrap();
            let pixels = preview.widgets[&id].pixels().clone();
            let revision = preview.widgets[&id].revision();
            let mut replacement = activity.clone();
            replacement.speed[4] = Some(100.0);
            preview
                .replace_activity(&serde_json::to_vec(&replacement).unwrap())
                .unwrap();
            preview.render(0.5).unwrap();
            assert_ne!(preview.widgets[&id].revision(), revision);
            assert_ne!(
                preview.widgets[&id].pixels(),
                &pixels,
                "new range redraws labels even at the same current value and zero fill"
            );
        }
        for (field, invalid) in [
            ("width", json!(0)),
            ("track_border_color", json!("invalid")),
            ("track_fill_style", json!("unknown")),
            ("track_fill_style", Value::Null),
            ("show_min_max_labels", json!("true")),
        ] {
            let mut malformed = request.clone();
            malformed["widget"][field] = invalid;
            let result = serde_json::from_value(malformed)
                .map_err(|error| error.to_string())
                .and_then(|request| {
                    preview
                        .create_widget(request)
                        .map_err(|error| error.to_string())
                });
            assert!(result.is_err(), "malformed {field} must fail");
        }
        request["scene"]["opacity"] = json!(0);
        preview
            .update_widget(id, serde_json::from_value(request).unwrap())
            .unwrap();
        for second in [0.5, 2.25] {
            preview.render(second).unwrap();
            assert!(
                preview.widgets[&id]
                    .pixels()
                    .chunks_exact(4)
                    .all(|pixel| pixel[3] == 0),
                "{display}/{fill} must be transparent at zero global opacity"
            );
        }
    }
}

#[test]
fn gauge_resize_preserves_opposite_corners_and_commits_fractional_geometry_during_playback() {
    use ovrley_widgets::widgets::gauges::{
        frame::GaugeLayout, metric::sample_gauge, validation::ValidatedGauge,
    };
    let bytes =
        br#"{"sample_elapsed_seconds":[0,1,2,3],"speed":[10,40,null,20],"trim_end_seconds":3}"#;
    let sampler = ovrley_widgets::activity::ActivitySampler::from_json(bytes).unwrap();
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    for (display, placement, fill) in [
        ("linear", "horizontal", "fill"),
        ("linear", "vertical", "bars"),
        ("arc", "bottom-left", "bars"),
        ("corner", "bottom-left", "fill"),
        ("corner", "bottom-right", "bars"),
        ("gt_gauge", "bottom-left", "fill"),
    ] {
        let (mut preview, mut origin) = gauge_preview(display, "speed", placement, fill);
        origin["widget"]["track_corner_radius"] = json!(9);
        preview.replace_activity(bytes).unwrap();
        for scale in [0.75, 1.7] {
            origin["scene"]["scale"] = json!(scale);
            let id = preview
                .create_widget(serde_json::from_value(origin.clone()).unwrap())
                .unwrap();
            preview.render(0.75).unwrap();
            let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
            let bounds = &geometry["content"];
            for fixed in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]] {
                let point = [
                    bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0],
                    bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1],
                ];
                for (factor, second) in [(0.54375, 0.75), (1.234375, 2.25)] {
                    let mut resize = origin.clone();
                    if display == "linear" {
                        resize["size"] = json!([
                            bounds["width"].as_f64().unwrap() * factor,
                            bounds["height"].as_f64().unwrap() * 0.875
                        ]);
                    } else {
                        resize["factor"] = json!(factor);
                    }
                    resize["fixed"] = json!(fixed);
                    resize["point"] = json!(point);
                    resize["second"] = json!(second);
                    let patch = serde_json::to_value(
                        preview
                            .resize_widget(serde_json::from_value(resize).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                    let variant = &patch["display_variants"][display];
                    assert_ne!(variant["width"].as_f64().unwrap().fract(), 0.0);
                    if display == "linear" {
                        assert!(patch.get("font_size").is_none());
                        assert!(variant.get("track_thickness").is_none());
                    } else {
                        assert!(
                            (patch["font_size"].as_f64().unwrap() - 60.0 * factor).abs() < 0.001
                        );
                    }
                    if display == "gt_gauge" {
                        assert!((variant["tick_length"].as_f64().unwrap() - 14.0 * factor).abs() < 0.001);
                    } else {
                        assert!(variant.get("tick_length").is_none());
                    }
                    let mut live = origin.clone();
                    for (key, value) in patch.as_object().unwrap() {
                        if key != "display_variants" {
                            live["widget"][key] = value.clone();
                        }
                    }
                    for (key, value) in variant.as_object().unwrap() {
                        live["widget"][key] = value.clone();
                    }
                    preview
                        .update_widget(id, serde_json::from_value(live.clone()).unwrap())
                        .unwrap();
                    preview.render(second).unwrap();
                    let geometry: Value =
                        serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
                    let actual = &geometry["content"];
                    for (axis, (position, extent)) in [("left", "width"), ("top", "height")]
                        .into_iter()
                        .enumerate()
                    {
                        let corner = actual[position].as_f64().unwrap()
                            + actual[extent].as_f64().unwrap() * fixed[axis];
                        assert!(
                            (corner - point[axis]).abs() < 0.002,
                            "{display} fixed corner moved at {second}"
                        );
                    }
                    // Persisted variants are promoted at native ingress, with no rounding on release.
                    let mut committed = live["widget"].clone();
                    for key in variant.as_object().unwrap().keys() {
                        committed.as_object_mut().unwrap().remove(key);
                    }
                    committed["display_variants"] = patch["display_variants"].clone();
                    let config = common::seam::validated_config_from_value(
                        json!({"scene": origin["scene"], "values": [committed], "labels": [], "plots": []}),
                    );
                    let widget = match &config.values[0] {
                        ovrley_widgets::widgets::value::validation::ValidatedValue::LinearGauge(widget) => {
                            ValidatedGauge::Linear(widget.clone())
                        }
                        ovrley_widgets::widgets::value::validation::ValidatedValue::ArcGauge(widget) => {
                            ValidatedGauge::Arc(widget.clone())
                        }
                        _ => panic!("gauge dispatch"),
                    };
                    let layout = GaugeLayout::new(
                        &widget,
                        &config.scene.presentation,
                        sampler.gauge_range(widget.metric()),
                        0.0,
                        &NativeFonts(&font_dirs),
                        ovrley_widgets::render::preview::PreviewRasterScale::NATIVE,
                    )
                    .unwrap();
                    let frame = layout
                        .prepare_frame(
                            sample_gauge(&widget, Some(&sampler), second),
                            &NativeFonts(&font_dirs),
                        )
                        .unwrap();
                    let expected: Value = serde_json::from_slice(
                        &serde_json::to_vec(
                            &ovrley_widgets::widgets::value::preview::ContentBounds::from(
                                frame.content_bounds,
                            ),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                    assert_eq!(actual, &expected, "live and committed gauge bounds");
                }
            }
            preview.widgets.remove(&id);
        }
    }
}

fn heading_preview() -> (PreviewSession, Value) {
    let (preview, mut request) = lean_angle_preview();
    request["type"] = json!("heading");
    request["scene"]["opacity"] = json!(0.7);
    request["widget"] = common::builders::heading_tape_json();
    request["widget"]["label_font"] = json!("Inter.ttf");
    request["widget"]["x"] = json!(100.25);
    request["widget"]["y"] = json!(80.5);
    request["widget"]["width"] = json!(400.125);
    request["widget"]["height"] = json!(80.375);
    request["widget"]["opacity"] = json!(0.8);
    (preview, request)
}

#[test]
fn heading_tape_matches_export_pixels_sampling_revisions_and_activity_replacement() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, trim::trim_activity},
        debug::RenderProfiler, paths::AppPaths,
        render::render_full_frames,
    };
    use ovrley_widgets::activity::ActivitySampler;
    let (mut preview, mut request) = heading_preview();
    let activity = ovrley_core::activity::parse_activity_json(r#"{
        "sample_elapsed_seconds": [0,0.5,1.5,2.5,4], "trim_end_seconds": 4,
        "heading": [null,350,null,10,null]
    }"#).unwrap();
    let bytes = serde_json::to_vec(&activity).unwrap();
    let sampler = ActivitySampler::from_json(&bytes).unwrap();
    preview.replace_activity(&bytes).unwrap();
    let root = common::test_config::repo_git_root();
    let paths = AppPaths::from_runtime_roots(root.clone(), root, std::env::temp_dir());
    let seconds = vec![0.0, 0.5, 0.501, 0.75, 1.5, 2.25, 2.5, 3.75];
    assert_eq!(sampler.heading_at(0.0), Some(350.0));
    assert_eq!(sampler.heading_at(1.5), Some(0.0));
    assert_eq!(sampler.heading_at(3.75), Some(10.0));
    for (style, placement, visible) in [("chevron", "top", true), ("chevron", "bottom", true), ("chevron", "both", true), ("highlight_bar", "both", true), ("chevron", "top", false)] {
        request["widget"]["indicator_style"] = json!(style);
        request["widget"]["indicator_placement"] = json!(placement);
        request["widget"]["show_indicator"] = json!(visible);
        let config = common::seam::validated_config_from_value(json!({
            "scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": [],
        }));
        let requirements = config.render_data_requirements();
        let trimmed = trim_activity(&activity, 0.0, 4.0, &requirements).unwrap();
        let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
        let frames = render_full_frames(&paths.font_dirs, &config, &activity, &dense, &(0..dense.frame_count).collect::<Vec<_>>(), &mut RenderProfiler::default()).unwrap();
        let scene_width = config.scene.presentation.width as usize;
        let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
        for index in [0,1,2,3,4,5,6,7,4,0] {
            let second = seconds[index];
            assert_eq!(sampler.heading_at(second), dense.series.heading[index]);
            let previous = preview.widgets[&id].revision();
            let previous_pixels = (index == 2).then(|| preview.widgets[&id].pixels().clone());
            preview.render(second).unwrap();
            if index == 2 { assert_ne!(preview.widgets[&id].revision(), previous, "sub-degree tape offset changes the revision"); }
            let widget = &preview.widgets[&id];
            if let Some(previous_pixels) = previous_pixels {
                assert!(widget.pixels() != &previous_pixels, "fractional movement must change the raster below one degree and one pixel");
            }
            let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
            assert_eq!(geometry["content"]["left"], request["widget"]["x"], "tape frame does not travel horizontally");
            let frame_width = request["widget"]["width"].as_f64().unwrap() * request["scene"]["scale"].as_f64().unwrap();
            assert!((geometry["content"]["width"].as_f64().unwrap() - frame_width).abs() < 0.001, "tape width stays fixed as ticks and gaps cross its edges");
            let difference = export_tile_difference(&frames[index], scene_width, &geometry, widget.pixels());
            assert!(difference <= EXPORT_TILE_TOLERANCE, "{style}/{placement} differs from export by {difference} at {second}");
            assert!(widget.pixels().chunks_exact(4).any(|pixel| pixel[3] != 0));
            let revision = widget.revision();
            preview.render(second).unwrap();
            assert_eq!(preview.widgets[&id].revision(), revision);
        }
        preview.render(1.25).unwrap();
        let original = preview.widgets[&id].pixels().clone();
        let geometry = preview.widgets[&id].geometry().clone();
        preview.replace_activity(&[]).unwrap();
        preview.render(1.25).unwrap();
        assert_ne!(preview.widgets[&id].pixels(), &original);
        preview.replace_activity(&bytes).unwrap();
        preview.render(1.25).unwrap();
        assert_eq!(preview.widgets[&id].pixels(), &original);
        request["scene"]["shadow_strength"] = json!(0);
        preview.update_widget(id, serde_json::from_value(request.clone()).unwrap()).unwrap();
        preview.render(1.25).unwrap();
        assert_eq!(preview.widgets[&id].geometry(), &geometry, "shadows do not change selection bounds");
        request["scene"]["shadow_strength"] = json!(3);
        preview.widgets.remove(&id);
    }
    // Equal-width opaque ticks make the edge fade observable independently of glyph coverage.
    request["scene"]["scale"] = json!(1);
    request["scene"]["opacity"] = json!(1);
    request["scene"]["shadow_strength"] = json!(0);
    for (field, value) in [("x", 0), ("y", 0), ("width", 400), ("height", 80), ("opacity", 1), ("major_tick_thickness", 16), ("minor_tick_thickness", 16)] {
        request["widget"][field] = json!(value);
    }
    preview.replace_activity(&[]).unwrap();
    let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
    preview.render(0.0).unwrap();
    let pixels = preview.widgets[&id].pixels();
    let alpha = |x: usize| pixels[(25 * 400 + x) * 4 + 3];
    assert!(alpha(0) < alpha(7) && alpha(7) < alpha(25), "left edge fades inward");
    assert!(alpha(399) < alpha(392) && alpha(392) < alpha(375), "right edge fades inward");
    assert_eq!((alpha(100), alpha(200), alpha(300)), (255, 255, 255), "interior remains opaque");
    let config = common::seam::validated_config_from_value(json!({ "scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": [] }));
    let ovrley_widgets::widgets::value::validation::ValidatedValue::HeadingTape(widget) = &config.values[0] else { panic!("heading dispatch"); };
    let cache = ovrley_widgets::widgets::heading::layout::prepare_heading_cache(widget, &config.scene.presentation, &NativeFonts(&paths.font_dirs)).unwrap();
    let info = ImageInfo::new((400, 80), ColorType::RGBA8888, AlphaType::Premul, None);
    let mut surface = surfaces::raster(&info, None, None).unwrap();
    surface.canvas().clear(Color::BLUE);
    cache.prepare_frame(Some(0.0)).draw(surface.canvas(), &cache);
    let mut composited = vec![0; 400 * 80 * 4];
    assert!(surface.read_pixels(&info.with_alpha_type(AlphaType::Unpremul), &mut composited, 400 * 4, (0, 0)));
    assert!(composited.chunks_exact(4).all(|pixel| pixel[3] == 255), "fade preserves content underneath the tape");
    preview.widgets.remove(&id);
    for (field, invalid) in [("pixels_per_degree", json!(0)), ("label_font", json!("")), ("major_tick_interval", Value::Null), ("tick_alignment", json!("invalid")), ("show_indicator", json!("true"))] {
        let mut malformed = request.clone();
        malformed["widget"][field] = invalid;
        let result = serde_json::from_value(malformed).map_err(|error| error.to_string())
            .and_then(|request| preview.create_widget(request).map_err(|error| error.to_string()));
        assert!(result.is_err(), "invalid {field} must fail");
    }
}

#[test]
fn heading_tape_resize_preserves_opposite_corners_reflows_and_commits_fractional_dimensions() {
    use ovrley_widgets::widgets::heading::{geometry::heading_tape_layout, validation::validate_heading};
    let (mut preview, mut origin) = heading_preview();
    let bytes = br#"{"sample_elapsed_seconds":[0,1,2,3],"heading":[350,null,10,45],"trim_end_seconds":3}"#;
    preview.replace_activity(bytes).unwrap();
    let sampler = ovrley_widgets::activity::ActivitySampler::from_json(bytes).unwrap();
    for scale in [0.75, 1.7] {
        origin["scene"]["scale"] = json!(scale);
        for (style, placement) in [("chevron", "top"), ("chevron", "both"), ("highlight_bar", "both")] {
            origin["widget"]["indicator_style"] = json!(style);
            origin["widget"]["indicator_placement"] = json!(placement);
            let id = preview.create_widget(serde_json::from_value(origin.clone()).unwrap()).unwrap();
            preview.render(0.25).unwrap();
            let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
            let bounds = &geometry["content"];
            assert!(bounds["height"].as_f64().unwrap() <= origin["widget"]["height"].as_f64().unwrap() * scale + 0.001);
            for fixed in [[0.0,0.0], [1.0,0.0], [0.0,1.0], [1.0,1.0]] {
                let point = [bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0], bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1]];
                for second in [0.25, 1.75, 3.0, 0.5] {
                    let mut gesture = origin.clone();
                    gesture["second"] = json!(second);
                    gesture["fixed"] = json!(fixed);
                    gesture["point"] = json!(point);
                    gesture["size"] = json!([bounds["width"].as_f64().unwrap() * 1.2375, bounds["height"].as_f64().unwrap() * 0.875]);
                    let patch = serde_json::to_value(preview.resize_widget(serde_json::from_value(gesture).unwrap()).unwrap()).unwrap();
                    assert_eq!(patch.as_object().unwrap().len(), 3, "resize changes only position and the active frame variant");
                    let variant = &patch["display_variants"]["heading_tape"];
                    assert!(variant["width"].as_f64().unwrap().fract() != 0.0);
                    let mut live = origin.clone();
                    live["widget"]["x"] = patch["x"].clone();
                    live["widget"]["y"] = patch["y"].clone();
                    live["widget"]["width"] = variant["width"].clone();
                    live["widget"]["height"] = variant["height"].clone();
                    live["widget"]["display_variants"] = patch["display_variants"].clone();
                    preview.update_widget(id, serde_json::from_value(live.clone()).unwrap()).unwrap();
                    preview.render(second).unwrap();
                    let geometry = preview.widgets[&id].geometry().clone();
                    let actual: Value = serde_json::from_slice(&geometry).unwrap();
                    for axis in 0..2 {
                        let (start, extent) = if axis == 0 { ("left", "width") } else { ("top", "height") };
                        let corner = actual["content"][start].as_f64().unwrap() + actual["content"][extent].as_f64().unwrap() * fixed[axis];
                        assert!((corner - point[axis]).abs() < 0.001, "{style}/{placement} scale {scale} corner {fixed:?} at {second}");
                    }
                    // Native ingress consumes the same fractional variant after committing.
                    let config = common::seam::validated_config_from_value(json!({ "scene": live["scene"], "values": [live["widget"]], "labels": [], "plots": [] }));
                    let ovrley_widgets::widgets::value::validation::ValidatedValue::HeadingTape(committed) = &config.values[0] else { panic!("heading dispatch"); };
                    let scene = validate_scene_presentation(serde_json::from_value(live["scene"].clone()).unwrap()).unwrap();
                    let cache = ovrley_widgets::widgets::heading::layout::prepare_heading_cache(committed, &scene, &preview.fonts).unwrap();
                    let frame = cache.prepare_frame(sampler.heading_at(second));
                    let content: Value = serde_json::from_slice(&serde_json::to_vec(&ovrley_widgets::widgets::value::preview::ContentBounds::from(frame.content_bounds)).unwrap()).unwrap();
                    assert_eq!(content, actual["content"]);
                    preview.update_widget(id, serde_json::from_value(live).unwrap()).unwrap();
                    preview.render(second).unwrap();
                    assert_eq!(preview.widgets[&id].geometry(), &geometry);
                }
            }
            preview.widgets.remove(&id);
        }
    }
    for (style, visible) in [("chevron", true), ("highlight_bar", true), ("highlight_bar", false)] {
        origin["widget"]["indicator_style"] = json!(style);
        origin["widget"]["show_indicator"] = json!(visible);
        let original = validate_heading(serde_json::from_value(origin["widget"].clone()).unwrap(), 0).unwrap();
        let layout = |config: &ovrley_widgets::widgets::heading::validation::ValidatedHeading| heading_tape_layout(
            config.height, config.show_indicator, &config.indicator_style, &config.indicator_placement,
            config.indicator_size, config.major_tick_length_pct, config.label_offset, config.label_font_size,
        );
        for offset in [10.125, 0.0] {
            let mut edit = origin.clone();
            edit["second"] = json!(0.25);
            edit["fixed"] = json!([0, 0]);
            edit["point"] = json!([original.x, original.y]);
            edit["label_offset"] = json!(offset);
            let patch = serde_json::to_value(preview.resize_widget(serde_json::from_value(edit).unwrap()).unwrap()).unwrap();
            let mut committed = origin["widget"].clone();
            committed["height"] = patch["display_variants"]["heading_tape"]["height"].clone();
            committed["label_offset"] = patch["display_variants"]["heading_tape"]["label_offset"].clone();
            let committed = validate_heading(serde_json::from_value(committed).unwrap(), 0).unwrap();
            assert_eq!(committed.label_offset, offset);
            assert!((layout(&original).tick_scale_height - layout(&committed).tick_scale_height).abs() < 0.001,
                "{style}, visible {visible}: offset changes frame height without shrinking ticks");
            assert_eq!(patch["x"], json!(original.x));
            assert_eq!(patch["y"], json!(original.y));
            assert_ne!(committed.height, original.height);
        }
    }
}

fn lap_timer_activity() -> ovrley_core::activity::schema::ParsedActivity {
    ovrley_core::activity::parse_activity_json(r#"{
        "sample_elapsed_seconds": [0,0.5,1,2,2.5,3,3.75,4,5,64], "trim_end_seconds": 64,
        "lap_number": [-1,0,0,0,1,1,2,2,-1,-1],
        "lap_time_seconds": [null,0,0.5,1.5,0,0.5,0,0.25,null,null],
        "lap_start_elapsed_seconds": [0.5,2.5,3.75],
        "lap_durations_seconds": [2,1.25,60.25], "lap_durations_best_so_far_seconds": [2,1.25,1.25],
        "delta_to_best_lap_seconds": [null,null,null,null,0,-0.125,null,0.75,null,null]
    }"#).unwrap()
}

fn lap_timer_preview() -> (PreviewSession, Value) {
    let (mut preview, mut request) = lean_angle_preview();
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let weight = ovrley_widgets::model::standard_widgets::widget_font_weight();
    preview.fonts.register(
        ovrley_widgets::preview::FontRequest { font: "Teko.ttf".into(), font_weight: weight, italic: false },
        &preview_font_resource(&font_dirs, "Teko.ttf", weight, false).unwrap(),
    ).unwrap();
    request["type"] = json!("lap_timer");
    request["scene"]["end"] = json!(5);
    request["widget"] = json!({
        "value": "lap_timer", "display_type": "lap_timer", "x": 600.25, "y": 100.5,
        "font": "Inter.ttf", "font_size": 48, "color": "#abcdef", "opacity": 0.8,
        "label_font": "Teko.ttf", "label_font_size": 17.5, "label_color": "#123456",
        "show_label": true, "label": "Current Lap", "lap_timer_mode": "current_lap",
        "positive_delta_color": "#00ff00", "negative_delta_color": "#ff0000", "renumber_laps": true,
    });
    (preview, request)
}

#[test]
fn lap_timer_modes_match_export_pixels_sampling_and_resource_replacement() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, trim::trim_activity},
        debug::RenderProfiler,
        paths::AppPaths,
        render::render_full_frames,
    };
    let (mut preview, mut request) = lap_timer_preview();
    let mut activity = lap_timer_activity();
    let bytes = serde_json::to_vec(&activity).unwrap();
    preview.replace_activity(&bytes).unwrap();
    let root = common::test_config::repo_git_root();
    let paths = AppPaths::from_runtime_roots(root.clone(), root, std::env::temp_dir());
    let seconds = vec![0.0, 0.25, 0.5, 1.121, 1.122, 2.25, 2.5, 3.375, 3.75, 4.0, 4.5, 5.0];
    for mode in ["current_lap", "best_lap", "delta", "lap_log"] {
        request["widget"]["lap_timer_mode"] = json!(mode);
        let config = common::seam::validated_config_from_value(json!({
            "scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": [],
        }));
        let requirements = config.render_data_requirements();
        let trimmed = trim_activity(&activity, 0.0, 5.0, &requirements).unwrap();
        let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
        let frames = render_full_frames(&paths.font_dirs, &config, &activity, &dense, &(0..dense.frame_count).collect::<Vec<_>>(), &mut RenderProfiler::default()).unwrap();
        let scene_width = config.scene.presentation.width as usize;
        let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
        for index in [0,1,2,3,4,5,6,7,8,9,10,11,6,3,0] {
            let second = seconds[index];
            preview.render(second).unwrap();
            let widget = &preview.widgets[&id];
            let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
            let difference = export_tile_difference(&frames[index], scene_width, &geometry, widget.pixels());
            assert!(difference <= EXPORT_TILE_TOLERANCE, "{mode} differs from export by {difference} at {second}");
            assert!(widget.pixels().chunks_exact(4).any(|pixel| pixel[3] != 0));
            let revision = widget.revision();
            preview.render(second).unwrap();
            assert_eq!(preview.widgets[&id].revision(), revision);
        }
        preview.render(1.121).unwrap();
        let revision = preview.widgets[&id].revision();
        preview.render(1.122).unwrap();
        assert_eq!(preview.widgets[&id].revision(), revision, "rounded {mode} presentation is unchanged");
        preview.widgets.remove(&id);
    }
    // The current row and completed count stay equal, but replaced lap history must repaint.
    let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
    preview.render(4.0).unwrap();
    let original = preview.widgets[&id].pixels().clone();
    activity.lap_start_elapsed_seconds[0] = 0.0;
    activity.lap_number[0] = 0;
    for index in 0..4 { activity.lap_time_seconds[index] = Some(activity.sample_elapsed_seconds[index]); }
    activity.lap_durations_seconds[0] = 2.5;
    activity.lap_durations_best_so_far_seconds[0] = 2.5;
    preview.replace_activity(&serde_json::to_vec(&activity).unwrap()).unwrap();
    preview.render(4.0).unwrap();
    assert_ne!(preview.widgets[&id].pixels(), &original);
    preview.replace_activity(&[]).unwrap();
    preview.render(4.0).unwrap();
    assert_ne!(preview.widgets[&id].pixels(), &original);
    preview.replace_activity(&bytes).unwrap();
    preview.render(4.0).unwrap();
    assert_eq!(preview.widgets[&id].pixels(), &original);
    for (field, value) in [("label_font", Value::Null), ("font_size", json!(0)), ("label_color", json!("invalid")), ("show_label", Value::Null), ("lap_timer_mode", json!("unknown")), ("display_type", json!("text"))] {
        let mut malformed = request.clone();
        malformed["widget"][field] = value;
        assert!(preview.create_widget(serde_json::from_value(malformed).unwrap()).is_err(), "{field}");
    }
    request["widget"]["label_font"] = json!("Missing.ttf");
    preview.create_widget(serde_json::from_value(request).unwrap()).unwrap();
    assert!(preview.render(4.0).unwrap_err().to_string().contains("font not registered"));
}

#[test]
fn lap_timer_resize_keeps_all_opposite_corners_fixed_across_modes_and_playhead_changes() {
    let (mut preview, mut origin) = lap_timer_preview();
    preview.replace_activity(&serde_json::to_vec(&lap_timer_activity()).unwrap()).unwrap();
    let id = preview.create_widget(serde_json::from_value(origin.clone()).unwrap()).unwrap();
    for mode in ["current_lap", "best_lap", "delta", "lap_log"] {
        origin["widget"]["lap_timer_mode"] = json!(mode);
        for (scale, show_label) in [(1.7, true), (0.8, false)] {
            origin["scene"]["scale"] = json!(scale);
            origin["widget"]["show_label"] = json!(show_label);
            preview.update_widget(id, serde_json::from_value(origin.clone()).unwrap()).unwrap();
            preview.render(1.125).unwrap();
            let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
            let bounds = &geometry["content"];
            assert!(bounds["width"].as_f64().unwrap() < geometry["width"].as_f64().unwrap());
            for fixed in [[0.0,0.0], [1.0,0.0], [0.0,1.0], [1.0,1.0]] {
                let point = [bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0], bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1]];
                for (factor, second) in [(1.234375, 4.0), (0.01, 5.0), (1.234375, 0.0)] {
                    let mut gesture = origin.clone();
                    gesture.as_object_mut().unwrap().extend(json!({"factor": factor, "fixed": fixed, "point": point, "second": second}).as_object().unwrap().clone());
                    let patch = serde_json::to_value(preview.resize_widget(serde_json::from_value(gesture).unwrap()).unwrap()).unwrap();
                    assert_eq!(patch["font_size"], json!(if factor == 0.01 { 8.0 } else { 59.25 }));
                    assert_eq!(patch["label_font_size"], json!(if factor == 0.01 { 6.0 } else { 21.6015625 }));
                    let mut committed = origin.clone();
                    committed["widget"].as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
                    preview.update_widget(id, serde_json::from_value(committed.clone()).unwrap()).unwrap();
                    preview.render(second).unwrap();
                    let live = preview.widgets[&id].geometry().clone();
                    let actual: Value = serde_json::from_slice(&live).unwrap();
                    for (axis, start, size) in [(0,"left","width"), (1,"top","height")] {
                        let corner = actual["content"][start].as_f64().unwrap() + actual["content"][size].as_f64().unwrap() * fixed[axis];
                        assert!((corner - point[axis]).abs() < 0.003, "{mode} at {second}");
                    }
                    preview.update_widget(id, serde_json::from_value(committed).unwrap()).unwrap();
                    preview.render(second).unwrap();
                    assert_eq!(preview.widgets[&id].geometry(), &live, "live and committed {mode}");
                }
            }
        }
    }
}

fn splits_run_preview() -> (PreviewSession, Value) {
    let (mut preview, mut request) = lean_angle_preview();
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let weight = ovrley_widgets::model::standard_widgets::widget_font_weight();
    preview.fonts.register(
        ovrley_widgets::preview::FontRequest { font: "Teko.ttf".into(), font_weight: weight, italic: false },
        &preview_font_resource(&font_dirs, "Teko.ttf", weight, false).unwrap(),
    ).unwrap();
    request["type"] = json!("splits_run");
    request["widget"] = json!({
        "value": "splits_run", "display_type": "splits_run", "x": 600.25, "y": 100.5,
        "font": "Inter.ttf", "font_size": 30, "color": "#abcdef", "opacity": 0.8,
        "label": "Splits", "label_font": "Teko.ttf", "label_font_size": 15, "label_color": "#123456",
        "split_distance": 10, "split_unit": "m", "max_splits": 3, "visible_columns": ["split", "bar", "pace"],
        "row_gap": 6, "bar_width": 150, "track_thickness": 18, "track_corner_radius": 6,
        "track_filled_color": "#dce2e8", "track_filled_opacity": 0.9, "track_filled_gradient": true,
        "track_filled_gradient_stops": [{ "offset": 0, "color": "#40e0d0" }, { "offset": 1, "color": "#c65102" }],
        "track_border_thickness": 2, "track_border_color": "#ffffff", "track_border_opacity": 0.7,
    });
    (preview, request)
}

fn splits_run_table(activity: &ovrley_core::activity::schema::ParsedActivity) -> Vec<u8> {
    use ovrley_widgets::widgets::splits_run::table::{SplitLength, SplitUnit};
    let length = SplitLength { distance: 10.0, unit: SplitUnit::M };
    serde_json::to_vec(&ovrley_core::activity::splits::derive_split_table(activity, length)).unwrap()
}

#[test]
fn splits_run_matches_export_pixels_skips_unchanged_samples_and_rejects_malformed_fields() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, trim::trim_activity},
        debug::RenderProfiler,
        paths::AppPaths,
        render::render_full_frames,
    };
    let (mut preview, request) = splits_run_preview();
    {
        // Without an activity the table shows one placeholder row.
        let mut render = |patch: Value| {
            let mut request = request.clone();
            request["widget"].as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
            let id = preview.create_widget(serde_json::from_value(request).unwrap()).unwrap();
            preview.render(0.0).unwrap();
            let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
            let pixels = preview.widgets[&id].pixels().clone();
            preview.widgets.remove(&id);
            (geometry["content_width"].as_f64().unwrap(), pixels)
        };
        let (narrow, _) = render(json!({}));
        let (wide, _) = render(json!({ "bar_width": 250 }));
        assert!((wide - narrow - 100.0 * 1.7).abs() < 1e-3, "the bar width alone widens the table: {narrow} -> {wide}");
        let (_, placeholder) = render(json!({ "visible_columns": ["bar"] }));
        let (_, invisible) = render(json!({ "visible_columns": ["bar"], "track_filled_opacity": 0, "track_border_thickness": 0 }));
        assert_ne!(placeholder, invisible, "an empty table draws a placeholder bar");
    }
    let activity = every_family_parsed_activity();
    let table = splits_run_table(&activity);
    assert!(preview.replace_split_table(&table).is_err(), "split tables belong to a loaded activity");
    preview.replace_activity(&serde_json::to_vec(&activity).unwrap()).unwrap();
    assert!(preview.replace_split_table(b"{}").is_err(), "malformed split tables fail at ingress");
    preview.replace_split_table(&table).unwrap();
    let root = common::test_config::repo_git_root();
    let paths = AppPaths::from_runtime_roots(root.clone(), root, std::env::temp_dir());
    let seconds = vec![0.0, 0.25, 0.75, 1.0, 1.6, 2.2, 2.75, 3.1, 3.6, 4.0];
    let variants = [
        ("gradient with border", json!({})),
        ("solid without border", json!({ "track_filled_gradient": false, "track_border_thickness": 0 })),
        ("bars and pace only", json!({ "visible_columns": ["pace", "bar"] })),
        ("text only, one row", json!({ "visible_columns": ["split", "pace"], "max_splits": 1 })),
        ("short bars under a wider header", json!({ "bar_width": 4, "label": "Splits of the whole run" })),
    ];
    for (variant, patch) in variants {
        let mut request = request.clone();
        request["widget"].as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
        let config = common::seam::validated_config_from_value(json!({
            "scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": [],
        }));
        let requirements = config.render_data_requirements();
        let trimmed = trim_activity(&activity, 0.0, 4.0, &requirements).unwrap();
        let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
        let frames = render_full_frames(&paths.font_dirs, &config, &activity, &dense, &(0..dense.frame_count).collect::<Vec<_>>(), &mut RenderProfiler::default()).unwrap();
        let scene_width = config.scene.presentation.width as usize;
        let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
        let mut distinct = std::collections::HashSet::new();
        for index in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 5, 2, 0] {
            let second = seconds[index];
            preview.render(second).unwrap();
            let widget = &preview.widgets[&id];
            let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
            let difference = export_tile_difference(&frames[index], scene_width, &geometry, widget.pixels());
            assert!(difference <= EXPORT_TILE_TOLERANCE, "{variant} differs from export by {difference} at {second}");
            distinct.insert(widget.pixels().clone());
            let revision = widget.revision();
            preview.render(second).unwrap();
            assert_eq!(preview.widgets[&id].revision(), revision, "{variant}: an unchanged sample must not redraw");
        }
        assert!(distinct.len() > 3, "{variant}: completed splits and the running split must change the table");
        preview.widgets.remove(&id);
    }
    for (field, value) in [
        ("split_unit", json!("yd")), ("split_distance", json!(0)), ("visible_columns", json!([])),
        ("visible_columns", json!(["split", "split"])), ("track_border_thickness", json!(9)), ("bar_width", Value::Null),
        ("max_splits", json!(0)), ("label_font", Value::Null), ("display_type", json!("text")),
    ] {
        let mut malformed = request.clone();
        malformed["widget"][field] = value;
        assert!(preview.create_widget(serde_json::from_value(malformed).unwrap()).is_err(), "{field}");
    }
}

#[test]
fn splits_run_resize_scales_every_dimension_and_keeps_opposite_corners_fixed() {
    let (mut preview, mut origin) = splits_run_preview();
    let activity = every_family_parsed_activity();
    preview.replace_activity(&serde_json::to_vec(&activity).unwrap()).unwrap();
    preview.replace_split_table(&splits_run_table(&activity)).unwrap();
    let id = preview.create_widget(serde_json::from_value(origin.clone()).unwrap()).unwrap();
    for scale in [1.7, 0.8] {
        origin["scene"]["scale"] = json!(scale);
        preview.update_widget(id, serde_json::from_value(origin.clone()).unwrap()).unwrap();
        preview.render(2.2).unwrap();
        let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
        let bounds = &geometry["content"];
        for fixed in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]] {
            let point = [bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0], bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1]];
            for (factor, second) in [(1.25, 3.6), (0.01, 4.0), (1.25, 0.0)] {
                let mut gesture = origin.clone();
                gesture.as_object_mut().unwrap().extend(json!({"factor": factor, "fixed": fixed, "point": point, "second": second}).as_object().unwrap().clone());
                let patch = serde_json::to_value(preview.resize_widget(serde_json::from_value(gesture).unwrap()).unwrap()).unwrap();
                // The font clamp bounds the factor, so every dimension keeps its proportion.
                let applied = if factor == 0.01 { 8.0 / 30.0 } else { factor };
                for (field, configured) in [("font_size", 30.0), ("label_font_size", 15.0), ("row_gap", 6.0), ("bar_width", 150.0), ("track_thickness", 18.0), ("track_corner_radius", 6.0), ("track_border_thickness", 2.0)] {
                    assert!((patch[field].as_f64().unwrap() - configured * applied).abs() < 1e-4, "{field} at factor {factor}");
                }
                let mut committed = origin.clone();
                committed["widget"].as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
                preview.update_widget(id, serde_json::from_value(committed).unwrap()).unwrap();
                preview.render(second).unwrap();
                let actual: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
                for (axis, start, size) in [(0, "left", "width"), (1, "top", "height")] {
                    let corner = actual["content"][start].as_f64().unwrap() + actual["content"][size].as_f64().unwrap() * fixed[axis];
                    assert!((corner - point[axis]).abs() < 0.003, "scale {scale} factor {factor} at {second}");
                }
            }
        }
    }
}

fn g_force_preview() -> (PreviewSession, Value) {
    let (preview, mut request) = lean_angle_preview();
    request["type"] = json!("g_force");
    request["scene"]["end"] = json!(5);
    request["widget"] = json!({
        "value": "g_force", "display_type": "g_force", "x": 600.25, "y": 100.5,
        "width": 220.25, "height": 220.25, "diameter": 200, "opacity": 0.8,
        "fill_color": "#212121", "fill_opacity": 0.5,
        "border_thickness": 2, "border_color": "#ffffff", "border_opacity": 0.7,
        "marker_size": 12, "marker_color": "#40e0d0", "marker_opacity": 0.9,
        "axis_horizontal": "x", "axis_vertical": "y", "invert_horizontal": false, "invert_vertical": false,
        "clip_percentile": 80, "label_font": "Inter.ttf", "label_font_size": 24,
        "label_color": "#abcdef", "label_decimals": 1, "label_unit": "G", "label_unit_color": "#aaaaaa",
        "label_offset_x": 11.25, "label_offset_y": -17.5,
    });
    (preview, request)
}

#[test]
fn g_force_preview_matches_export_pixels_and_tracks_marker_and_percentile_changes() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, schema::ParsedActivity, trim::trim_activity},
        debug::RenderProfiler,
        render::render_full_frames,
    };
    let fixture: Value = serde_json::from_str(include_str!("fixtures/g-force-frame-state.json")).unwrap();
    let mut activity: ParsedActivity = serde_json::from_value(fixture["activity"].clone()).unwrap();
    let (mut preview, request) = g_force_preview();
    preview.replace_activity(&serde_json::to_vec(&activity).unwrap()).unwrap();
    let config = common::seam::validated_config_from_value(json!({
        "scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": [],
    }));
    let requirements = config.render_data_requirements();
    let trimmed = trim_activity(&activity, 0.0, 5.0, &requirements).unwrap();
    let seconds = [1.0, 1.002, 3.0];
    let dense = densify_activity(&trimmed, seconds.to_vec(), &requirements);
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let frames = render_full_frames(&font_dirs, &config, &activity, &dense, &[0, 1, 2], &mut RenderProfiler::default()).unwrap();
    let scene_width = config.scene.presentation.width as usize;
    let id = preview.create_widget(serde_json::from_value(request).unwrap()).unwrap();
    for index in [0, 1, 2, 0] {
        preview.render(seconds[index]).unwrap();
        let widget = &preview.widgets[&id];
        let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
        let difference = export_tile_difference(&frames[index], scene_width, &geometry, widget.pixels());
        assert!(difference <= EXPORT_TILE_TOLERANCE, "G-force differs from export by {difference} at {}", seconds[index]);
    }
    let revision = preview.widgets[&id].revision();
    preview.render(1.0).unwrap();
    assert_eq!(preview.widgets[&id].revision(), revision);
    let pixels = preview.widgets[&id].pixels().clone();
    activity.g_force_y[2] = Some(120.0); // Same current axes, different whole-activity clipping scale.
    preview.replace_activity(&serde_json::to_vec(&activity).unwrap()).unwrap();
    preview.render(1.0).unwrap();
    assert_ne!(preview.widgets[&id].pixels(), &pixels);
}

#[test]
fn g_force_resize_keeps_visible_corners_fixed_and_commits_fractional_variant_geometry() {
    let (mut preview, origin) = g_force_preview();
    let fixture: Value = serde_json::from_str(include_str!("fixtures/g-force-frame-state.json")).unwrap();
    preview.replace_activity(&serde_json::to_vec(&fixture["activity"]).unwrap()).unwrap();
    let id = preview.create_widget(serde_json::from_value(origin.clone()).unwrap()).unwrap();
    preview.render(1.0).unwrap();
    let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
    let bounds = &geometry["content"];
    for fixed in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]] {
        let point = [
            bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0],
            bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1],
        ];
        for second in [1.0, 3.0] {
            let mut resize = origin.clone();
            resize["factor"] = json!(1.234375);
            resize["fixed"] = json!(fixed);
            resize["point"] = json!(point);
            resize["second"] = json!(second);
            let patch = serde_json::to_value(
                preview.resize_widget(serde_json::from_value(resize).unwrap()).unwrap(),
            ).unwrap();
            let variant = &patch["display_variants"]["g_force"];
            assert_eq!(variant["width"], json!(271.87109375));
            assert_eq!(variant["height"], variant["width"]);
            let mut committed = origin.clone();
            committed["widget"]["x"] = patch["x"].clone();
            committed["widget"]["y"] = patch["y"].clone();
            for (field, value) in variant.as_object().unwrap() { committed["widget"][field] = value.clone(); }
            preview.update_widget(id, serde_json::from_value(committed).unwrap()).unwrap();
            preview.render(second).unwrap();
            let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
            for (axis, start, size) in [(0, "left", "width"), (1, "top", "height")] {
                let actual = geometry["content"][start].as_f64().unwrap() + geometry["content"][size].as_f64().unwrap() * fixed[axis];
                assert!((actual - point[axis]).abs() < 0.003);
            }
        }
    }
}

fn lean_angle_preview() -> (PreviewSession, Value) {
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let mut preview = PreviewSession::default();
    // Compared against native export, which grid-fits text.
    preview.fonts.set_grid_fit(true);
    let weight = ovrley_widgets::model::standard_widgets::widget_font_weight();
    preview
        .fonts
        .register(
            ovrley_widgets::preview::FontRequest {
                font: "Inter.ttf".into(),
                font_weight: weight,
                italic: false,
            },
            &preview_font_resource(&font_dirs, "Inter.ttf", weight, false).unwrap(),
        )
        .unwrap();
    let mut scene = common::seam::explicit_scene_json();
    scene["end"] = json!(4);
    scene["scale"] = json!(1.7);
    scene["shadow_strength"] = json!(3);
    scene["shadow_distance"] = json!(4);
    scene["border_thickness"] = json!(1.5);
    (
        preview,
        json!({
            "type": "lean_angle", "scene": scene,
            "widget": {
                "value": "lean_angle", "display_type": "lean_angle", "x": 600.25, "y": 100.5,
                "diameter": 180, "rotation": 0, "opacity": 0.8, "show_icon": false,
                "track_empty_color": "#222222", "track_empty_opacity": 0.5,
                "track_filled_color": "#40e0d0", "track_filled_opacity": 0.7,
        "track_filled_gradient": false, "track_filled_gradient_stops": [{ "offset": 0, "color": "#40e0d0" }, { "offset": 1, "color": "#c65102" }],
                "track_border_thickness": 2, "track_border_color": "#ffffff", "track_thickness": 24,
                "font": "Inter.ttf", "font_size": 60, "color": "#abcdef", "unit_color": "#aaaaaa",
                "show_units": true, "value_offset_x": 11.25, "value_offset_y": -17.5,
            },
        }),
    )
}

#[test]
fn lean_angle_preview_matches_export_and_tracks_signed_fill_resources_and_validation() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, schema::ParsedActivity, trim::trim_activity},
        debug::RenderProfiler,
        paths::AppPaths,
        render::render_full_frames,
    };
    use ovrley_widgets::activity::ActivitySampler;
    let (mut preview, request) = lean_angle_preview();
    let parsed: ParsedActivity = serde_json::from_value(json!({
        "sample_elapsed_seconds": [0,1,2,3,4], "trim_end_seconds": 4,
        "lean_angle": [null,30,null,-70,null],
    }))
    .unwrap();
    let bytes = serde_json::to_vec(&parsed).unwrap();
    let sampler = ActivitySampler::from_json(&bytes).unwrap();
    preview.replace_activity(&bytes).unwrap();
    let config = common::seam::validated_config_from_value(json!({
        "scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": [],
    }));
    let requirements = config.render_data_requirements();
    let trimmed = trim_activity(&parsed, 0.0, 4.0, &requirements).unwrap();
    let seconds = vec![0.0, 0.5, 1.0, 1.002, 2.0, 2.5, 3.0, 4.0];
    let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
    let root = common::test_config::repo_git_root();
    let paths = AppPaths {
        repo_root: root.clone(),
        font_dirs: vec![root.join("fonts")],
        debug_render_dir: std::env::temp_dir(),
        temp_dir: std::env::temp_dir(),
        bundled_templates_dirs: vec![],
        user_templates_dir: std::env::temp_dir(),
        downloads_dir: std::env::temp_dir(),
    };
    let frames = render_full_frames(&paths.font_dirs, &config, &parsed, &dense, &(0..dense.frame_count).collect::<Vec<_>>(), &mut RenderProfiler::default()).unwrap();
    let scene_width = config.scene.presentation.width as usize;
    let id = preview
        .create_widget(serde_json::from_value(request.clone()).unwrap())
        .unwrap();
    for index in [0, 1, 2, 3, 4, 5, 6, 7, 2, 0] {
        let second = seconds[index];
        assert_eq!(
            sampler.lean_angle_at(second),
            dense.series.lean_angle[index]
        );
        preview.render(second).unwrap();
        let widget = &preview.widgets[&id];
        let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
        let difference = export_tile_difference(&frames[index], scene_width, &geometry, widget.pixels());
        assert!(difference <= EXPORT_TILE_TOLERANCE, "lean-angle differs from export by {difference} at {second}");
        assert!(widget.pixels().chunks_exact(4).any(|pixel| pixel[3] != 0));
        let revision = widget.revision();
        preview.render(second).unwrap();
        assert_eq!(preview.widgets[&id].revision(), revision);
    }
    preview.render(1.0).unwrap();
    let previous = preview.widgets[&id].pixels().clone();
    let revision = preview.widgets[&id].revision();
    preview.render(1.002).unwrap();
    assert_ne!(
        preview.widgets[&id].revision(),
        revision,
        "fill changes within the same integer label"
    );
    assert_ne!(preview.widgets[&id].pixels(), &previous);
    let original = preview.widgets[&id].pixels().clone();
    preview.replace_activity(&[]).unwrap();
    preview.render(1.002).unwrap();
    assert_ne!(preview.widgets[&id].pixels(), &original);
    preview.replace_activity(&bytes).unwrap();
    preview.render(1.002).unwrap();
    assert_eq!(preview.widgets[&id].pixels(), &original);
    preview
        .replace_activity(
            br#"{"sample_elapsed_seconds":[0,4],"trim_end_seconds":4,"lean_angle":[-30,-30]}"#,
        )
        .unwrap();
    preview.render(1.002).unwrap();
    assert_ne!(
        preview.widgets[&id].pixels(),
        &original,
        "signed fill changes even for equal magnitudes"
    );
    assert_eq!(sampler.lean_angle_at(-1.0), None);
    assert_eq!(sampler.lean_angle_at(5.0), None);
    for (field, malformed) in [
        ("diameter", Value::Null),
        ("width", json!(180)),
        ("track_thickness", json!(90)),
        ("track_border_thickness", json!(12)),
        ("track_empty_color", json!("invalid")),
        ("show_units", Value::Null),
        ("value_offset_x", Value::Null),
        ("font_size", json!(0)),
        ("opacity", json!(2)),
    ] {
        let mut invalid = request.clone();
        invalid["widget"][field] = malformed;
        assert!(
            preview
                .create_widget(serde_json::from_value(invalid).unwrap())
                .is_err(),
            "{field}"
        );
    }
    let mut changed = request;
    changed["widget"]["track_filled_color"] = json!("#123456");
    let revision = preview.widgets[&id].revision();
    let pixels = preview.widgets[&id].pixels().clone();
    preview
        .update_widget(id, serde_json::from_value(changed).unwrap())
        .unwrap();
    preview.render(1.002).unwrap();
    assert_ne!(preview.widgets[&id].revision(), revision);
    assert_ne!(preview.widgets[&id].pixels(), &pixels);
}

#[test]
fn lean_angle_resize_preserves_visible_corners_across_playback_and_commits_diameter_variants() {
    let (mut preview, mut origin) = lean_angle_preview();
    preview.replace_activity(br#"{"sample_elapsed_seconds":[0,1,2,3],"lean_angle":[30,70,-30,0],"trim_end_seconds":3}"#).unwrap();
    let id = preview
        .create_widget(serde_json::from_value(origin.clone()).unwrap())
        .unwrap();
    for (scale, offset, show_units, border) in [
        (1.7, [11.25, -17.5], true, 2),
        (0.8, [200.0, 100.0], false, 0),
    ] {
        origin["scene"]["scale"] = json!(scale);
        origin["widget"]["value_offset_x"] = json!(offset[0]);
        origin["widget"]["value_offset_y"] = json!(offset[1]);
        origin["widget"]["show_units"] = json!(show_units);
        origin["widget"]["track_border_thickness"] = json!(border);
        preview
            .update_widget(id, serde_json::from_value(origin.clone()).unwrap())
            .unwrap();
        preview.render(0.5).unwrap();
        let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
        let bounds = &geometry["content"];
        assert!(bounds["width"].as_f64().unwrap() < geometry["width"].as_f64().unwrap());
        assert!(bounds["height"].as_f64().unwrap() < geometry["height"].as_f64().unwrap());
        for fixed in [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]] {
            let point = [
                bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0],
                bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1],
            ];
            for factor in [0.01, 0.55, 1.234375] {
                for second in [2.25, 3.0, 0.25] {
                    let mut gesture = origin.clone();
                    gesture["factor"] = json!(factor);
                    gesture["fixed"] = json!(fixed);
                    gesture["point"] = json!(point);
                    gesture["second"] = json!(second);
                    let patch = serde_json::to_value(
                        preview
                            .resize_widget(serde_json::from_value(gesture).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                    let variant = &patch["display_variants"]["lean_angle"];
                    assert!(patch.get("width").is_none() && patch.get("height").is_none());
                    assert!(
                        variant.get("font_size").is_none()
                            && variant.get("width").is_none()
                            && variant.get("height").is_none()
                    );
                    if factor == 1.234375 {
                        assert_eq!(variant["diameter"], json!(222.1875));
                        assert_eq!(variant["track_thickness"], json!(29.625));
                        assert_eq!(
                            variant["track_border_thickness"],
                            json!(f64::from(border) * factor)
                        );
                        assert_eq!(patch["font_size"], json!(74.0625));
                    }
                    // Match the frontend's variant merge and effective-config resolution.
                    let mut committed = origin.clone();
                    committed["widget"]
                        .as_object_mut()
                        .unwrap()
                        .extend(variant.as_object().unwrap().clone());
                    committed["widget"]
                        .as_object_mut()
                        .unwrap()
                        .extend(patch.as_object().unwrap().clone());
                    preview
                        .update_widget(id, serde_json::from_value(committed.clone()).unwrap())
                        .unwrap();
                    preview.render(second).unwrap();
                    let live_geometry = preview.widgets[&id].geometry().clone();
                    let actual: Value = serde_json::from_slice(&live_geometry).unwrap();
                    for (axis, position, size) in [(0, "left", "width"), (1, "top", "height")] {
                        let corner = actual["content"][position].as_f64().unwrap()
                            + actual["content"][size].as_f64().unwrap() * fixed[axis];
                        assert!(
                            (corner - point[axis]).abs() < 0.002,
                            "lean-angle corner moved at {second}, factor {factor}"
                        );
                    }
                    preview
                        .update_widget(id, serde_json::from_value(committed).unwrap())
                        .unwrap();
                    preview.render(second).unwrap();
                    assert_eq!(preview.widgets[&id].geometry(), &live_geometry);
                }
            }
        }
    }
}

fn gradient_preview() -> (PreviewSession, Value) {
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let mut preview = PreviewSession::default();
    // Compared against native export, which grid-fits text.
    preview.fonts.set_grid_fit(true);
    let weight = ovrley_widgets::model::standard_widgets::widget_font_weight();
    preview
        .fonts
        .register(
            ovrley_widgets::preview::FontRequest {
                font: "Inter.ttf".into(),
                font_weight: weight,
                italic: false,
            },
            &preview_font_resource(&font_dirs, "Inter.ttf", weight, false).unwrap(),
        )
        .unwrap();
    let mut scene = common::seam::explicit_scene_json();
    scene["end"] = json!(4);
    scene["scale"] = json!(1.7);
    scene["shadow_strength"] = json!(3);
    scene["shadow_distance"] = json!(4);
    scene["border_thickness"] = json!(1.5);
    (
        preview,
        json!({
            "type": "gradient", "scene": scene,
            "widget": {
                "value": "gradient", "display_type": "text", "x": 600.25, "y": 100.5,
                "font": "Inter.ttf", "font_size": 48, "color": "#abcdef", "opacity": 0.8,
                "prefix": "", "suffix": "", "decimals": 1, "show_sign": true,
                "show_triangle": true, "triangle_width": 72, "value_offset": 11.25,
                "unit_color": "#aaaaaa", "triangle_positive_color": "#40e0d0", "triangle_negative_color": "#c65102",
            },
        }),
    )
}

#[test]
fn gradient_preview_matches_native_samples_pixels_and_invalidates_continuous_triangle_changes() {
    use ovrley_core::activity::{
        interpolate::densify_activity, schema::ParsedActivity, trim::trim_activity,
    };
    use ovrley_widgets::{
        activity::ActivitySampler,
        widgets::gradient::{
            format::format_validated_gradient,
            frame::{prepare_gradient_frame, validated_gradient_style},
            validation::validate_gradient_widget,
        },
    };
    let (mut preview, mut request) = gradient_preview();
    request["scene"]["opacity"] = json!(0.5);
    let parsed: ParsedActivity = serde_json::from_value(json!({
        "sample_elapsed_seconds": [0,1,2,3,4], "trim_end_seconds": 4,
        "gradient": [null,6,null,-6,null],
    }))
    .unwrap();
    let bytes = serde_json::to_vec(&parsed).unwrap();
    let sampler = ActivitySampler::from_json(&bytes).unwrap();
    preview.replace_activity(&bytes).unwrap();
    let config = common::seam::validated_config_from_value(json!({
        "scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": [],
    }));
    let requirements = config.render_data_requirements();
    let trimmed = trim_activity(&parsed, 0.0, 4.0, &requirements).unwrap();
    let seconds = vec![0.0, 0.5, 1.0, 1.002, 2.0, 2.5, 3.0, 4.0];
    let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
    let validated = validate_gradient_widget(
        serde_json::from_value(request["widget"].clone()).unwrap(),
        0,
    )
    .unwrap();
    let scene =
        validate_scene_presentation(serde_json::from_value(request["scene"].clone()).unwrap())
            .unwrap();
    let style = validated_gradient_style(&validated, &scene, scene.scale);
    assert_eq!(style.opacity, 0.4);
    assert_eq!(style.color.a(), 102);
    let fonts = NativeFonts(&[common::test_config::repo_git_root().join("fonts")]);
    let id = preview
        .create_widget(serde_json::from_value(request.clone()).unwrap())
        .unwrap();
    // Forward playback and backward seeks include a bridged gap and missing coverage edges.
    for index in [0, 1, 2, 3, 4, 5, 6, 7, 2, 0] {
        let second = seconds[index];
        assert_eq!(sampler.gradient_at(second), dense.series.gradient[index]);
        preview.render(second).unwrap();
        let widget = &preview.widgets[&id];
        let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
        let info = ImageInfo::new(
            (
                geometry["width"].as_i64().unwrap() as i32,
                geometry["height"].as_i64().unwrap() as i32,
            ),
            ovrley_widgets::widgets::value::preview::TEXT_COLOR_TYPE,
            AlphaType::Premul,
            None,
        );
        let mut surface = surfaces::raster(&info, None, None).unwrap();
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.translate((
            -geometry["left"].as_f64().unwrap() as f32,
            -geometry["top"].as_f64().unwrap() as f32,
        ));
        prepare_gradient_frame(
            &validated,
            &style,
            dense.series.gradient[index],
            scene.scale,
            scene.opacity,
            &fonts,
        )
        .unwrap()
        .draw(canvas, &fonts)
        .unwrap();
        let mut pixels = vec![0; widget.pixels().len()];
        read_surface_rgba(&mut surface, &mut pixels).unwrap();
        assert_eq!(&pixels, widget.pixels(), "gradient at {second}");
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
        let revision = widget.revision();
        preview.render(second).unwrap();
        assert_eq!(preview.widgets[&id].revision(), revision);
    }
    assert_eq!(
        format_validated_gradient(&validated, sampler.gradient_at(1.0)),
        format_validated_gradient(&validated, sampler.gradient_at(1.002))
    );
    preview.render(1.0).unwrap();
    let revision = preview.widgets[&id].revision();
    preview.render(1.002).unwrap();
    assert_ne!(
        preview.widgets[&id].revision(),
        revision,
        "triangle changes even when the text does not"
    );
    let original = preview.widgets[&id].pixels().clone();
    preview.replace_activity(&[]).unwrap();
    preview.render(1.002).unwrap();
    assert_ne!(preview.widgets[&id].pixels(), &original);
    let missing = preview.widgets[&id].pixels().clone();
    preview.render(3.0).unwrap();
    assert_eq!(preview.widgets[&id].pixels(), &missing);
    preview.replace_activity(&bytes).unwrap();
    preview.render(1.002).unwrap();
    assert_eq!(preview.widgets[&id].pixels(), &original);
    preview
        .replace_activity(
            br#"{"sample_elapsed_seconds":[0,4],"trim_end_seconds":4,"gradient":[-3,-3]}"#,
        )
        .unwrap();
    preview.render(1.002).unwrap();
    assert_ne!(preview.widgets[&id].pixels(), &original);
    assert_eq!(sampler.gradient_at(-1.0), None);
    assert_eq!(sampler.gradient_at(5.0), None);
    for (field, malformed) in [
        ("show_triangle", Value::Null),
        ("triangle_width", json!(-1)),
        ("value_offset", Value::Null),
        ("unit_color", json!("invalid")),
        ("font_size", json!(0)),
        ("opacity", json!(2)),
        ("display_type", json!("arc")),
    ] {
        let mut invalid = request.clone();
        invalid["widget"][field] = malformed;
        assert!(
            preview
                .create_widget(serde_json::from_value(invalid).unwrap())
                .is_err(),
            "{field}"
        );
    }
    request["widget"]["triangle_positive_color"] = json!("#123456");
    request["scene"]["opacity"] = json!(0);
    let revision = preview.widgets[&id].revision();
    preview
        .update_widget(id, serde_json::from_value(request).unwrap())
        .unwrap();
    preview.render(1.002).unwrap();
    assert_ne!(preview.widgets[&id].revision(), revision);
    assert!(preview.widgets[&id].pixels().chunks_exact(4).all(|pixel| pixel[3] == 0));
}

#[test]
fn gradient_resize_preserves_all_opposite_corners_during_playback_and_commits_fractional_intrinsic_fields(
) {
    let (mut preview, mut origin) = gradient_preview();
    preview
        .replace_activity(
            br#"{"sample_elapsed_seconds":[0,1,2,3],"gradient":[6,12,-6,0],"trim_end_seconds":3}"#,
        )
        .unwrap();
    let id = preview
        .create_widget(serde_json::from_value(origin.clone()).unwrap())
        .unwrap();
    for (show_triangle, offset, scale) in
        [(true, 11.25, 1.7), (true, -17.5, 0.8), (false, 11.25, 1.7)]
    {
        origin["widget"]["show_triangle"] = json!(show_triangle);
        origin["widget"]["value_offset"] = json!(offset);
        origin["scene"]["scale"] = json!(scale);
        preview
            .update_widget(id, serde_json::from_value(origin.clone()).unwrap())
            .unwrap();
        preview.render(0.5).unwrap();
        let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
        let bounds = &geometry["content"];
        assert!(bounds["width"].as_f64().unwrap() < geometry["width"].as_f64().unwrap());
        assert!(bounds["height"].as_f64().unwrap() < geometry["height"].as_f64().unwrap());
        for fixed in [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]] {
            let point = [
                bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0],
                bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1],
            ];
            for factor in [0.55, 1.234375, 12.0] {
                for second in [2.25, 3.0, 0.25] {
                    let mut gesture = origin.clone();
                    gesture.as_object_mut().unwrap().extend(
                        json!({
                            "factor": factor, "fixed": fixed, "point": point, "second": second,
                        })
                        .as_object()
                        .unwrap()
                        .clone(),
                    );
                    let patch = serde_json::to_value(
                        preview
                            .resize_widget(serde_json::from_value(gesture).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                    assert_eq!(
                        patch.as_object().unwrap().len(),
                        5,
                        "gradient only resizes its intrinsic fields and position"
                    );
                    if factor == 1.234375 {
                        assert_eq!(patch["font_size"], json!(59.25));
                        assert_eq!(patch["triangle_width"], json!(88.875));
                        assert_eq!(patch["value_offset"].as_f64().unwrap(), offset * factor);
                    }
                    assert!(patch["font_size"].as_f64().unwrap() <= 400.0);
                    assert!(patch["triangle_width"].as_f64().unwrap() <= 600.0);
                    let mut committed = origin.clone();
                    committed["widget"]
                        .as_object_mut()
                        .unwrap()
                        .extend(patch.as_object().unwrap().clone());
                    preview
                        .update_widget(id, serde_json::from_value(committed.clone()).unwrap())
                        .unwrap();
                    preview.render(second).unwrap();
                    let live_geometry = preview.widgets[&id].geometry().clone();
                    let actual: Value = serde_json::from_slice(&live_geometry).unwrap();
                    let actual = &actual["content"];
                    for (axis, position, size) in [(0, "left", "width"), (1, "top", "height")] {
                        let corner = actual[position].as_f64().unwrap()
                            + actual[size].as_f64().unwrap() * fixed[axis];
                        assert!((corner - point[axis]).abs() < 0.002, "gradient corner moved: triangle={show_triangle} offset={offset} factor={factor} second={second}");
                    }
                    preview
                        .update_widget(id, serde_json::from_value(committed).unwrap())
                        .unwrap();
                    preview.render(second).unwrap();
                    assert_eq!(preview.widgets[&id].geometry(), &live_geometry);
                }
            }
        }
    }
}

#[test]
fn time_preview_matches_native_composition_and_resize_at_the_current_second() {
    use ovrley_widgets::widgets::time::{
        format::{format_validated_time_parts, ElapsedTimeValues},
        validation::validate_time_value,
    };
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let mut preview = PreviewSession::default();
    // Compared against native export, which grid-fits text.
    preview.fonts.set_grid_fit(true);
    let weight = ovrley_widgets::model::standard_widgets::widget_font_weight();
    preview
        .fonts
        .register(
            ovrley_widgets::preview::FontRequest {
                font: "Inter.ttf".into(),
                font_weight: weight,
                italic: false,
            },
            &preview_font_resource(&font_dirs, "Inter.ttf", weight, false).unwrap(),
        )
        .unwrap();
    let mut request = json!({"type": "time", "widget": common::builders::speed_value_json(), "scene": common::seam::explicit_scene_json()});
    request["widget"].as_object_mut().unwrap().extend(
        json!({
            "value": "time", "font": "Inter.ttf", "format": "date-time-12s", "hours_offset": 1,
            "time_mode": "daytime", "elapsed_origin": "export", "show_hundredths": true,
            "x": 600.25, "y": 100.5, "icon_offset_x": -12, "icon_offset_y": 17,
            "prefix": "[", "suffix": "]", "show_icon": true,
        })
        .as_object()
        .unwrap()
        .clone(),
    );
    request["scene"]["export_start_seconds"] = json!(10.0);
    request["scene"]["scale"] = json!(1.7);
    request["scene"]["shadow_strength"] = json!(3);
    request["scene"]["shadow_distance"] = json!(4);
    request["scene"]["border_thickness"] = json!(2);
    let activity = br#"{"sample_elapsed_seconds":[0,1,2],"time":["2025-04-21T13:05:00Z",null,"2025-04-21T13:05:02Z"],"sync_time":"2000-01-01T00:00:00Z","metadata":{"timezone":"Europe/Berlin"},"trim_end_seconds":2}"#;
    preview.replace_activity(activity).unwrap();
    let id = preview
        .create_widget(serde_json::from_value(request.clone()).unwrap())
        .unwrap();
    for (mode, origin) in [
        ("daytime", "activity"),
        ("elapsed", "activity"),
        ("elapsed", "export"),
    ] {
        request["widget"]["time_mode"] = json!(mode);
        request["widget"]["elapsed_origin"] = json!(origin);
        for alignment in ["left", "center", "right"] {
            request["widget"]["content_alignment"] = json!(alignment);
            preview
                .update_widget(id, serde_json::from_value(request.clone()).unwrap())
                .unwrap();
            for (second, timestamp) in [
                (1.5, Some("2025-04-21T13:05:01.500Z")),
                (0.25, Some("2025-04-21T13:05:00.250Z")),
                (-1.0, None),
            ] {
                preview.render(second).unwrap();
                let widget = &preview.widgets[&id];
                let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
                let validated = validate_time_value(
                    serde_json::from_value(request["widget"].clone()).unwrap(),
                    0,
                )
                .unwrap();
                let scene = validate_scene_presentation(
                    serde_json::from_value(request["scene"].clone()).unwrap(),
                )
                .unwrap();
                let style = validated_value_style(&validated.base, &scene, scene.scale);
                let parts = format_validated_time_parts(
                    &validated,
                    timestamp,
                    ElapsedTimeValues {
                        activity_seconds: second,
                        export_seconds: second - 10.0,
                    },
                    Some(chrono_tz::Europe::Berlin),
                );
                let info = ImageInfo::new(
                    (
                        geometry["width"].as_i64().unwrap() as i32,
                        geometry["height"].as_i64().unwrap() as i32,
                    ),
                    ovrley_widgets::widgets::value::preview::TEXT_COLOR_TYPE,
                    AlphaType::Premul,
                    None,
                );
                let mut surface = surfaces::raster(&info, None, None).unwrap();
                let canvas = surface.canvas();
                canvas.clear(Color::TRANSPARENT);
                canvas.translate((
                    -geometry["left"].as_f64().unwrap() as f32,
                    -geometry["top"].as_f64().unwrap() as f32,
                ));
                let fonts = NativeFonts(&font_dirs);
                let cached = draw_static_metric_parts_for_value(
                    canvas,
                    &validated.base,
                    &style,
                    scene.scale,
                    &fonts,
                )
                .unwrap();
                draw_metric_parts(
                    canvas,
                    &style,
                    &parts,
                    scene.scale,
                    &fonts,
                    cached,
                    &validated.base,
                )
                .unwrap();
                let mut pixels = vec![0; widget.pixels().len()];
                read_surface_rgba(&mut surface, &mut pixels).unwrap();
                assert_eq!(
                    &pixels,
                    widget.pixels(),
                    "{mode}/{origin}/{alignment}/{second}"
                );
                assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
                let revision = widget.revision();
                preview.render(second).unwrap();
                assert_eq!(preview.widgets[&id].revision(), revision);
            }
            let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
            let bounds = &geometry["content"];
            for fixed in [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]] {
                let point = [
                    bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0],
                    bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1],
                ];
                let mut gesture = request.clone();
                gesture.as_object_mut().unwrap().extend(
                    json!({"second": 1.75, "factor": 1.234375, "fixed": fixed, "point": point})
                        .as_object()
                        .unwrap()
                        .clone(),
                );
                let patch = serde_json::to_value(
                    preview
                        .resize_widget(serde_json::from_value(gesture).unwrap())
                        .unwrap(),
                )
                .unwrap();
                let mut committed = request.clone();
                committed["widget"]
                    .as_object_mut()
                    .unwrap()
                    .extend(patch.as_object().unwrap().clone());
                preview
                    .update_widget(id, serde_json::from_value(committed).unwrap())
                    .unwrap();
                preview.render(1.75).unwrap();
                let resized: Value =
                    serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
                let actual = &resized["content"];
                assert!(
                    (actual["left"].as_f64().unwrap()
                        + actual["width"].as_f64().unwrap() * fixed[0]
                        - point[0])
                        .abs()
                        < 0.002
                );
                assert!(
                    (actual["top"].as_f64().unwrap()
                        + actual["height"].as_f64().unwrap() * fixed[1]
                        - point[1])
                        .abs()
                        < 0.002
                );
            }
        }
    }
    request["widget"]["time_mode"] = json!("daytime");
    preview
        .update_widget(id, serde_json::from_value(request.clone()).unwrap())
        .unwrap();
    preview.render(0.5).unwrap();
    let original = preview.widgets[&id].pixels().clone();
    preview.replace_activity(&[]).unwrap();
    preview.render(0.5).unwrap();
    assert_ne!(preview.widgets[&id].pixels(), &original);
    preview.replace_activity(activity).unwrap();
    preview.render(0.5).unwrap();
    assert_eq!(preview.widgets[&id].pixels(), &original);
    for (field, malformed) in [
        ("format", json!("unknown")),
        ("elapsed_origin", json!("unknown")),
        ("time_mode", json!("unknown")),
        ("hours_offset", Value::Null),
        ("show_hundredths", Value::Null),
    ] {
        let mut invalid = request.clone();
        invalid["widget"][field] = malformed;
        assert!(
            preview
                .create_widget(serde_json::from_value(invalid).unwrap())
                .is_err(),
            "{field}"
        );
    }
    request["scene"]
        .as_object_mut()
        .unwrap()
        .remove("export_start_seconds");
    assert!(preview
        .create_widget(serde_json::from_value(request).unwrap())
        .is_err());
}

fn label_preview() -> (PreviewSession, Value) {
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let mut preview = PreviewSession::default();
    // Compared against native export, which grid-fits text.
    preview.fonts.set_grid_fit(true);
    preview.fonts.register(
        ovrley_widgets::preview::FontRequest {
            font: "Inter.ttf".into(), font_weight: 537.0, italic: true,
        },
        &preview_font_resource(&font_dirs, "Inter.ttf", 537.0, true).unwrap(),
    ).unwrap();
    let mut scene = common::seam::explicit_scene_json();
    scene["scale"] = json!(1.7);
    scene["shadow_strength"] = json!(3);
    scene["shadow_distance"] = json!(4);
    scene["border_thickness"] = json!(2);
    (preview, json!({
        "type": "label", "scene": scene,
        "widget": {
            "text": "Label e\u{301}", "x": 120.25, "y": 60.5,
            "font": "Inter.ttf", "font_size": 48, "font_weight": 537,
            "italic": true, "letter_spacing": -1.25, "color": "#abcdef", "opacity": 0.8,
        },
    }))
}

#[test]
fn label_pixels_match_native_and_only_config_changes_invalidate_the_static_revision() {
    use ovrley_widgets::widgets::label::{style::validated_label_style, validation::validate_label};
    let (mut preview, mut request) = label_preview();
    let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
    let widget = &preview.widgets[&id];
    let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
    let label = validate_label(&serde_json::from_value(request["widget"].clone()).unwrap(), 0).unwrap();
    let scene = validate_scene_presentation(serde_json::from_value(request["scene"].clone()).unwrap()).unwrap();
    let info = ImageInfo::new(
        (geometry["width"].as_i64().unwrap() as i32, geometry["height"].as_i64().unwrap() as i32),
        ColorType::RGBA8888, AlphaType::Premul, None,
    );
    let mut surface = surfaces::raster(&info, None, None).unwrap();
    let canvas = surface.canvas();
    canvas.clear(Color::TRANSPARENT);
    canvas.translate((-geometry["left"].as_f64().unwrap() as f32, -geometry["top"].as_f64().unwrap() as f32));
    ovrley_widgets::render::text::draw_text(
        canvas, &label.text, &validated_label_style(&label, &scene, scene.scale),
        &NativeFonts(&[common::test_config::repo_git_root().join("fonts")]),
    ).unwrap();
    let mut pixels = vec![0; widget.pixels().len()];
    read_surface_rgba(&mut surface, &mut pixels).unwrap();
    assert_eq!(&pixels, widget.pixels());
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
    let revision = widget.revision();
    let original_geometry = widget.geometry().clone();
    preview.replace_activity(br#"{"sample_elapsed_seconds":[0,1],"speed":[0,20],"trim_end_seconds":1}"#).unwrap();
    for second in [0.0, 1.0, 0.25, 50.0] {
        preview.render(second).unwrap();
        assert_eq!(preview.widgets[&id].revision(), revision);
        assert_eq!(preview.widgets[&id].pixels(), &pixels);
        assert_eq!(preview.widgets[&id].geometry(), &original_geometry);
    }
    preview.replace_activity(&[]).unwrap();
    preview.render(0.0).unwrap();
    assert_eq!(preview.widgets[&id].revision(), revision);
    request["widget"]["text"] = json!("Edited label");
    preview.update_widget(id, serde_json::from_value(request.clone()).unwrap()).unwrap();
    assert_ne!(preview.widgets[&id].revision(), revision);
    assert_ne!(preview.widgets[&id].pixels(), &pixels);
    request["widget"]["font_size"] = json!(0);
    assert!(preview.update_widget(id, serde_json::from_value(request).unwrap()).is_err());
}

#[test]
fn label_intrinsic_resize_preserves_every_opposite_corner_and_fractional_size() {
    let (mut preview, origin) = label_preview();
    let id = preview.create_widget(serde_json::from_value(origin.clone()).unwrap()).unwrap();
    let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
    let bounds = &geometry["content"];
    assert!(bounds["width"].as_f64().unwrap() < geometry["width"].as_f64().unwrap());
    for fixed in [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]] {
        let point = [
            bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0],
            bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1],
        ];
        let mut gesture = origin.clone();
        gesture["factor"] = json!(1.234375);
        gesture["fixed"] = json!(fixed);
        gesture["point"] = json!(point);
        gesture["second"] = json!(17.5);
        let patch = serde_json::to_value(preview.resize_widget(serde_json::from_value(gesture).unwrap()).unwrap()).unwrap();
        assert_eq!(patch["font_size"], json!(59.25));
        assert_eq!(patch.as_object().unwrap().len(), 3, "labels resize only x, y and font_size");
        let mut committed = origin.clone();
        committed["widget"].as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
        preview.update_widget(id, serde_json::from_value(committed).unwrap()).unwrap();
        let resized: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
        let actual = &resized["content"];
        let corner = [
            actual["left"].as_f64().unwrap() + actual["width"].as_f64().unwrap() * fixed[0],
            actual["top"].as_f64().unwrap() + actual["height"].as_f64().unwrap() * fixed[1],
        ];
        assert!((corner[0] - point[0]).abs() < 0.002);
        assert!((corner[1] - point[1]).abs() < 0.002);
    }
}

#[test]
fn standalone_preview_matches_native_cached_composition_and_replaces_state() {
    let root = common::test_config::repo_git_root();
    let font_dirs = vec![root.join("fonts")];
    let config: Value = serde_json::from_str(
        &std::fs::read_to_string(common::test_config::simple_config_path()).unwrap(),
    )
    .unwrap();
    let mut request =
        json!({"type": "speed", "widget": config["values"][0], "scene": common::seam::explicit_scene_json()});
    request["widget"]["font"] = json!("Inter.ttf");
    request["widget"]["x"] = json!(600.25);
    request["widget"]["y"] = json!(100.5);
    request["scene"]["shadow_strength"] = json!(3);
    request["scene"]["shadow_distance"] = json!(4);
    request["scene"]["border_thickness"] = json!(2);
    request["scene"]["opacity"] = json!(0.5);
    request["widget"]["opacity"] = json!(0.8);
    request["widget"]["color"] = json!("#abcdef");
    let mut preview = PreviewSession::default();
    // Compared against native export, which grid-fits text.
    preview.fonts.set_grid_fit(true);
    preview
        .fonts
        .register(
            ovrley_widgets::preview::FontRequest { font: "Inter.ttf".into(), font_weight: ovrley_widgets::model::standard_widgets::widget_font_weight(), italic: false },
            &preview_font_resource(&font_dirs, "Inter.ttf", ovrley_widgets::model::standard_widgets::widget_font_weight(), false).unwrap(),
        )
        .unwrap();
    preview
        .replace_activity(
            br#"{"sample_elapsed_seconds":[0,1,2],"speed":[0,null,20],"trim_end_seconds":2}"#,
        )
        .unwrap();
    let id = preview
        .create_widget(serde_json::from_value(request.clone()).unwrap())
        .unwrap();
    for alignment in ["left", "center", "right"] {
        request["widget"]["content_alignment"] = json!(alignment);
        preview
            .update_widget(id, serde_json::from_value(request.clone()).unwrap())
            .unwrap();
        for (second, speed) in [
            (0.5, Some(5.0)),
            (1.5, Some(15.0)),
            (-1.0, None),
            (0.2, Some(2.0)),
        ] {
            preview.render(second).unwrap();
            let widget = &preview.widgets[&id];
            let geometry: Value = serde_json::from_slice(&widget.geometry()).unwrap();
            let input: WidgetRequest = serde_json::from_value(request.clone()).unwrap();
            let validated = validate_value_widget(input.widget, 0).unwrap();
            let scene = validate_scene_presentation(input.scene).unwrap();
            let style = validated_value_style(&validated, &scene, scene.scale);
            assert_eq!(style.opacity, 0.4);
            assert_eq!(style.color.a(), 102);
            let info = ImageInfo::new(
                (
                    geometry["width"].as_i64().unwrap() as i32,
                    geometry["height"].as_i64().unwrap() as i32,
                ),
                ovrley_widgets::widgets::value::preview::TEXT_COLOR_TYPE,
                AlphaType::Premul,
                None,
            );
            let mut surface = surfaces::raster(&info, None, None).unwrap();
            let canvas = surface.canvas();
            canvas.clear(Color::TRANSPARENT);
            canvas.translate((
                -geometry["left"].as_f64().unwrap() as f32,
                -geometry["top"].as_f64().unwrap() as f32,
            ));
            let fonts = NativeFonts(&font_dirs);
            let cached =
                draw_static_metric_parts_for_value(canvas, &validated, &style, scene.scale, &fonts)
                    .unwrap();
            draw_metric_parts(
                canvas,
                &style,
                &format_metric_parts(
                    &validated,
                    &MetricSample {
                        numeric: speed,
                        ..MetricSample::default()
                    },
                ),
                scene.scale,
                &fonts,
                cached,
                &validated,
            )
            .unwrap();
            let mut pixels = vec![0; widget.pixels().len()];
            read_surface_rgba(&mut surface, &mut pixels).unwrap();
            assert_eq!(&pixels, widget.pixels(), "{alignment}, t={second}");
            assert!(pixels.chunks_exact(4).any(|p| p[3] != 0));
            let revision = widget.revision();
            preview.render(second).unwrap();
            assert_eq!(
                preview.widgets[&id].revision(), revision,
                "unchanged presentation reuses its pixels"
            );
        }
    }
    preview.replace_activity(&[]).unwrap();
    preview.render(0.5).unwrap();
    let empty = preview.widgets[&id].pixels().clone();
    preview.render(1.5).unwrap();
    assert_eq!(preview.widgets[&id].pixels(), &empty);
    request["scene"]["opacity"] = json!(0);
    preview.update_widget(id, serde_json::from_value(request.clone()).unwrap()).unwrap();
    preview.render(0.5).unwrap();
    assert!(preview.widgets[&id].pixels().chunks_exact(4).all(|pixel| pixel[3] == 0));
    request["widget"]["font_size"] = json!(0);
    assert!(preview
        .update_widget(id, serde_json::from_value(request).unwrap())
        .is_err());
}

#[test]
fn live_resize_keeps_the_opposite_content_corner_fixed_and_commits_the_same_geometry() {
    let root = common::test_config::repo_git_root();
    let config: Value = serde_json::from_str(
        &std::fs::read_to_string(common::test_config::simple_config_path()).unwrap(),
    )
    .unwrap();
    let mut origin =
        json!({"type": "speed", "widget": config["values"][0], "scene": common::seam::explicit_scene_json()});
    origin["widget"]["font"] = json!("Inter.ttf");
    origin["widget"]["x"] = json!(600.25);
    origin["widget"]["y"] = json!(100.5);
    origin["widget"]["icon_offset_x"] = json!(-12);
    origin["widget"]["icon_offset_y"] = json!(17);
    origin["scene"]["scale"] = json!(1.7);
    origin["scene"]["shadow_strength"] = json!(5);
    origin["scene"]["shadow_distance"] = json!(8);
    let mut preview = PreviewSession::default();
    // Compared against native export, which grid-fits text.
    preview.fonts.set_grid_fit(true);
    preview
        .fonts
        .register(
            ovrley_widgets::preview::FontRequest { font: "Inter.ttf".into(), font_weight: ovrley_widgets::model::standard_widgets::widget_font_weight(), italic: false },
            &preview_font_resource(&[root.join("fonts")], "Inter.ttf", ovrley_widgets::model::standard_widgets::widget_font_weight(), false).unwrap(),
        )
        .unwrap();
    preview
        .replace_activity(
            br#"{"sample_elapsed_seconds":[0,1,2],"speed":[0,10,40],"trim_end_seconds":2}"#,
        )
        .unwrap();
    let id = preview
        .create_widget(serde_json::from_value(origin.clone()).unwrap())
        .unwrap();
    for alignment in ["left", "center", "right"] {
        origin["widget"]["content_alignment"] = json!(alignment);
        preview
            .update_widget(id, serde_json::from_value(origin.clone()).unwrap())
            .unwrap();
        preview.render(0.5).unwrap();
        let geometry: Value = serde_json::from_slice(&preview.widgets[&id].geometry()).unwrap();
        let bounds = &geometry["content"];
        assert!(bounds["width"].as_f64().unwrap() < geometry["width"].as_f64().unwrap());
        assert!(bounds["height"].as_f64().unwrap() < geometry["height"].as_f64().unwrap());
        for fixed in [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]] {
            let point = [
                bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() * fixed[0],
                bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() * fixed[1],
            ];
            for factor in [0.55, 1.8] {
                let mut gesture = origin.clone();
                gesture["factor"] = json!(factor);
                gesture["fixed"] = json!(fixed);
                gesture["point"] = json!(point);
                gesture["second"] = json!(1.5); // Telemetry may change while the gesture is active.
                let patch = serde_json::to_value(
                    preview
                        .resize_widget(serde_json::from_value(gesture).unwrap())
                        .unwrap(),
                )
                .unwrap();
                let mut committed = origin.clone();
                committed["widget"]
                    .as_object_mut()
                    .unwrap()
                    .extend(patch.as_object().unwrap().clone());
                preview
                    .update_widget(id, serde_json::from_value(committed).unwrap())
                    .unwrap();
                preview.render(1.5).unwrap();
                let resized: Value =
                    serde_json::from_slice(&preview.widgets[&id].geometry()).unwrap();
                let actual = &resized["content"];
                let corner = [
                    actual["left"].as_f64().unwrap() + actual["width"].as_f64().unwrap() * fixed[0],
                    actual["top"].as_f64().unwrap() + actual["height"].as_f64().unwrap() * fixed[1],
                ];
                assert!(
                    (corner[0] - point[0]).abs() < 0.002,
                    "{alignment}: horizontal anchor moved"
                );
                assert!(
                    (corner[1] - point[1]).abs() < 0.002,
                    "{alignment}: vertical anchor moved"
                );
            }
        }
    }
}

fn heading_compass_preview(rotation_mode: &str) -> (PreviewSession, Value) {
    let (preview, mut request) = lean_angle_preview();
    request["type"] = json!("heading");
    request["scene"]["opacity"] = json!(0.7);
    request["widget"] = json!({
        "value": "heading", "display_type": "heading_compass", "x": 120.25, "y": 60.5,
        "diameter": 200, "rotation_mode": rotation_mode, "arc_angle": 270, "opacity": 0.8,
        "track_thickness": 10, "track_corner_radius": 5, "track_fill": "#ffffffcc",
        "indicator_size": 70, "indicator_corner_radius": 25, "indicator_color": "#ff1f3d",
        "show_cardinal_labels": true, "cardinal_font_size": 22, "cardinal_label_color": "#eeeeee",
        "font": "Inter.ttf", "show_value": true, "font_size": 48, "color": "#abcdef",
        "unit_color": "#aaaaaa", "show_units": true, "value_offset_x": 6.5, "value_offset_y": -4.25,
    });
    (preview, request)
}

#[test]
fn heading_compass_matches_export_pixels_rotates_continuously_and_validates_at_ingress() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, trim::trim_activity},
        debug::RenderProfiler, paths::AppPaths,
        render::render_full_frames,
    };
    let activity = ovrley_core::activity::parse_activity_json(r#"{
        "sample_elapsed_seconds": [0,0.5,1.5,2.5,4], "trim_end_seconds": 4,
        "heading": [null,350,null,100,null]
    }"#).unwrap();
    let bytes = serde_json::to_vec(&activity).unwrap();
    let root = common::test_config::repo_git_root();
    let paths = AppPaths::from_runtime_roots(root.clone(), root, std::env::temp_dir());
    let seconds = vec![0.0, 0.5, 0.501, 1.5, 2.0, 2.5, 3.75];
    let mut mode_pixels = Vec::new();
    for mode in ["indicator", "compass"] {
        let (mut preview, request) = heading_compass_preview(mode);
        preview.replace_activity(&bytes).unwrap();
        let config = common::seam::validated_config_from_value(json!({
            "scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": [],
        }));
        let requirements = config.render_data_requirements();
        let trimmed = trim_activity(&activity, 0.0, 4.0, &requirements).unwrap();
        let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
        let frames = render_full_frames(&paths.font_dirs, &config, &activity, &dense, &(0..dense.frame_count).collect::<Vec<_>>(), &mut RenderProfiler::default()).unwrap();
        let scene_width = config.scene.presentation.width as usize;
        let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
        for (index, second) in seconds.iter().enumerate() {
            preview.render(*second).unwrap();
            let widget = &preview.widgets[&id];
            let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
            let difference = export_tile_difference(&frames[index], scene_width, &geometry, widget.pixels());
            assert!(difference <= EXPORT_TILE_TOLERANCE, "{mode} heading compass differs from export by {difference} at {second}");
            let revision = widget.revision();
            preview.render(*second).unwrap();
            assert_eq!(preview.widgets[&id].revision(), revision, "{mode} unchanged sample re-rendered");
        }
        // Sub-degree changes keep the readout but still turn the dial.
        preview.render(0.5).unwrap();
        let previous = preview.widgets[&id].pixels().clone();
        preview.render(0.501).unwrap();
        assert_ne!(preview.widgets[&id].pixels(), &previous, "{mode} dial turns between whole degrees");
        preview.render(2.0).unwrap();
        mode_pixels.push(preview.widgets[&id].pixels().clone());
        for (field, malformed) in [
            ("diameter", Value::Null),
            ("width", json!(200)),
            ("rotation_mode", json!("spin")),
            ("arc_angle", json!(400)),
            ("track_thickness", json!(0)),
            ("track_corner_radius", json!(-1)),
            ("track_fill", json!("white")),
            ("indicator_corner_radius", json!(150)),
            ("indicator_size", Value::Null),
            ("show_cardinal_labels", Value::Null),
            ("cardinal_font_size", json!(0)),
            ("show_value", Value::Null),
            ("value_offset_y", Value::Null),
            ("opacity", json!(2)),
        ] {
            let mut invalid = request.clone();
            invalid["widget"][field] = malformed;
            assert!(preview.create_widget(serde_json::from_value(invalid).unwrap()).is_err(), "{mode} {field}");
        }
    }
    assert_ne!(mode_pixels[0], mode_pixels[1], "rotation modes turn different dial parts");
}

#[test]
fn heading_compass_resize_preserves_the_fixed_corner_and_scales_pixel_fields() {
    let (mut preview, origin) = heading_compass_preview("compass");
    preview.replace_activity(br#"{"sample_elapsed_seconds":[0,3],"heading":[20,200],"trim_end_seconds":3}"#).unwrap();
    let id = preview.create_widget(serde_json::from_value(origin.clone()).unwrap()).unwrap();
    preview.render(1.0).unwrap();
    let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
    let bounds = &geometry["content"];
    let fixed = [1.0, 1.0];
    let point = [
        bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap(),
        bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap(),
    ];
    let mut gesture = origin.clone();
    gesture["factor"] = json!(1.5);
    gesture["fixed"] = json!(fixed);
    gesture["point"] = json!(point);
    gesture["second"] = json!(1.0);
    let patch = serde_json::to_value(preview.resize_widget(serde_json::from_value(gesture).unwrap()).unwrap()).unwrap();
    let variant = &patch["display_variants"]["heading_compass"];
    assert_eq!(variant["diameter"], json!(300.0));
    assert_eq!(variant["track_thickness"], json!(15.0));
    assert_eq!(variant["indicator_size"], json!(105.0));
    assert_eq!(variant["cardinal_font_size"], json!(33.0));
    assert_eq!(patch["font_size"], json!(72.0));
    let mut committed = origin;
    committed["widget"].as_object_mut().unwrap().extend(variant.as_object().unwrap().clone());
    committed["widget"].as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
    preview.update_widget(id, serde_json::from_value(committed).unwrap()).unwrap();
    preview.render(1.0).unwrap();
    let actual: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
    for (axis, position, size) in [(0, "left", "width"), (1, "top", "height")] {
        let corner = actual["content"][position].as_f64().unwrap() + actual["content"][size].as_f64().unwrap() * fixed[axis];
        assert!((corner - point[axis]).abs() < 0.002, "heading compass corner moved on axis {axis}");
    }
}

fn heart_zones_preview(indicator_style: &str) -> (PreviewSession, Value) {
    let (mut preview, mut request) = lean_angle_preview();
    // Zone labels resolve their own weight, like the editor's per-widget font requests.
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    preview
        .fonts
        .register(
            ovrley_widgets::preview::FontRequest { font: "Inter.ttf".into(), font_weight: 800.0, italic: false },
            &preview_font_resource(&font_dirs, "Inter.ttf", 800.0, false).unwrap(),
        )
        .unwrap();
    request["type"] = json!("heartrate");
    request["scene"]["opacity"] = json!(0.9);
    request["widget"] = json!({
        "value": "heartrate", "display_type": "heart_zones", "x": 80.25, "y": 70.5,
        "width": 400, "height": 36, "opacity": 0.8, "zone_calculation": "lthr", "lthr": 160,
        "zone_colors": ["#2c4964", "#285b4f", "#596d2b", "#705227", "#9d2842"], "zone_gap": 4,
        "track_corner_radius": 8, "indicator_style": indicator_style, "indicator_size": 14,
        "indicator_color": "#ffffff", "show_label": true, "font": "Inter.ttf",
        "label_font_weight": 800, "label_color": "#bdbdbd",
    });
    (preview, request)
}

#[test]
fn heart_zones_matches_export_pixels_moves_continuously_and_validates_at_ingress() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, trim::trim_activity},
        debug::RenderProfiler, paths::AppPaths,
        render::render_full_frames,
    };
    let activity = ovrley_core::activity::parse_activity_json(r#"{
        "sample_elapsed_seconds": [0,1,2,3,4], "trim_end_seconds": 4,
        "heartrate": [90,120,150,null,185]
    }"#).unwrap();
    let bytes = serde_json::to_vec(&activity).unwrap();
    let root = common::test_config::repo_git_root();
    let paths = AppPaths::from_runtime_roots(root.clone(), root, std::env::temp_dir());
    let seconds = vec![0.0, 1.0, 1.001, 2.0, 3.0, 4.0];
    for style in ["line", "arrow"] {
        let (mut preview, request) = heart_zones_preview(style);
        preview.replace_activity(&bytes).unwrap();
        let config = common::seam::validated_config_from_value(json!({
            "scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": [],
        }));
        let requirements = config.render_data_requirements();
        let trimmed = trim_activity(&activity, 0.0, 4.0, &requirements).unwrap();
        let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
        let frames = render_full_frames(&paths.font_dirs, &config, &activity, &dense, &(0..dense.frame_count).collect::<Vec<_>>(), &mut RenderProfiler::default()).unwrap();
        let scene_width = config.scene.presentation.width as usize;
        let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
        for (index, second) in seconds.iter().enumerate() {
            preview.render(*second).unwrap();
            let widget = &preview.widgets[&id];
            let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
            let difference = export_tile_difference(&frames[index], scene_width, &geometry, widget.pixels());
            assert!(difference <= EXPORT_TILE_TOLERANCE, "{style} heart zones differs from export by {difference} at {second}");
            let revision = widget.revision();
            preview.render(*second).unwrap();
            assert_eq!(preview.widgets[&id].revision(), revision, "{style} unchanged sample re-rendered");
        }
        // Sub-bpm changes still move the indicator.
        preview.render(1.0).unwrap();
        let previous = preview.widgets[&id].pixels().clone();
        preview.render(1.001).unwrap();
        assert_ne!(preview.widgets[&id].pixels(), &previous, "{style} indicator moves between whole bpm");
        let mut manual = request.clone();
        manual["widget"]["zone_calculation"] = json!("manual");
        manual["widget"]["manual_zones"] = json!([
            {"from": 80, "to": 110}, {"from": 110, "to": 130}, {"from": 131, "to": 150},
            {"from": 150, "to": 170}, {"from": 170, "to": 190},
        ]);
        assert!(preview.create_widget(serde_json::from_value(manual.clone()).unwrap()).is_ok(), "{style} manual zones");
        manual["widget"]["manual_zones"][2]["from"] = json!(120);
        assert!(preview.create_widget(serde_json::from_value(manual).unwrap()).is_err(), "{style} overlapping manual zones");
        for (field, malformed) in [
            ("width", Value::Null),
            ("height", json!(0)),
            ("zone_calculation", json!("karvonen")),
            ("lthr", Value::Null),
            ("zone_colors", json!(["#ffffff"])),
            ("zone_gap", json!(200)),
            ("track_corner_radius", json!(-1)),
            ("indicator_style", json!("needle")),
            ("indicator_size", json!(0)),
            ("indicator_color", json!("white")),
            ("label_color", Value::Null),
            ("show_label", Value::Null),
            ("label_font_weight", json!(0)),
            ("font", json!("")),
            ("opacity", json!(2)),
        ] {
            let mut invalid = request.clone();
            invalid["widget"][field] = malformed;
            assert!(preview.create_widget(serde_json::from_value(invalid).unwrap()).is_err(), "{style} {field}");
        }
    }
}

#[test]
fn heart_zones_resize_reflows_the_track_and_preserves_the_fixed_corner() {
    let (mut preview, origin) = heart_zones_preview("arrow");
    preview.replace_activity(br#"{"sample_elapsed_seconds":[0,3],"heartrate":[100,180],"trim_end_seconds":3}"#).unwrap();
    let id = preview.create_widget(serde_json::from_value(origin.clone()).unwrap()).unwrap();
    preview.render(1.0).unwrap();
    let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
    let bounds = &geometry["content"];
    // The fixture scene renders at scale 1.7; the content rectangle is the scaled track.
    assert!((bounds["width"].as_f64().unwrap() - 680.0).abs() < 0.01);
    assert!((bounds["height"].as_f64().unwrap() - 61.2).abs() < 0.01);
    let fixed = [0.0, 1.0];
    let point = [
        bounds["left"].as_f64().unwrap(),
        bounds["top"].as_f64().unwrap() + bounds["height"].as_f64().unwrap(),
    ];
    let mut gesture = origin.clone();
    gesture["size"] = json!([520.5 * 1.7, 30.25 * 1.7]);
    gesture["fixed"] = json!(fixed);
    gesture["point"] = json!(point);
    gesture["second"] = json!(1.0);
    let patch = serde_json::to_value(preview.resize_widget(serde_json::from_value(gesture).unwrap()).unwrap()).unwrap();
    let variant = &patch["display_variants"]["heart_zones"];
    assert!((variant["width"].as_f64().unwrap() - 520.5).abs() < 0.001);
    assert!((variant["height"].as_f64().unwrap() - 30.25).abs() < 0.001);
    let mut committed = origin;
    committed["widget"].as_object_mut().unwrap().extend(variant.as_object().unwrap().clone());
    committed["widget"].as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
    preview.update_widget(id, serde_json::from_value(committed).unwrap()).unwrap();
    preview.render(1.0).unwrap();
    let actual: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
    for (axis, position, size) in [(0, "left", "width"), (1, "top", "height")] {
        let corner = actual["content"][position].as_f64().unwrap() + actual["content"][size].as_f64().unwrap() * fixed[axis];
        assert!((corner - point[axis]).abs() < 0.002, "heart zones corner moved on axis {axis}");
    }
}

fn rolling_chart_preview() -> (PreviewSession, Value) {
    let (preview, mut request) = lean_angle_preview();
    let manifest: Value = serde_json::from_str(include_str!("../../../assets/standard-widgets.json")).unwrap();
    request["type"] = json!("rolling_chart");
    request["widget"] = manifest["definitions"].as_array().unwrap().iter().find(|w| w["type"] == "rolling_chart").unwrap()["defaults"].clone();
    request["widget"].as_object_mut().unwrap().extend(json!({
        "x": 60.25, "y": 80.5, "width": 300, "height": 90, "opacity": 0.9, "window_seconds": 5,
        "area_fill": true, "area_gradient": true, "marker_variant": "ring", "marker_variant_diameter": 20,
        "legend_font": "Inter.ttf", "legend_font_size": 14,
        "series": [
            {"metric": "speed", "label": "Speed", "color": "#ffffff"},
            {"metric": "heartrate", "label": "HR", "color": "#e0103a"},
        ],
    }).as_object().unwrap().clone());
    (preview, request)
}

#[test]
fn rolling_chart_matches_export_pixels_slides_with_the_playhead_and_validates_at_ingress() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, trim::trim_activity},
        debug::RenderProfiler, paths::AppPaths,
        render::render_full_frames,
    };
    let activity = every_family_parsed_activity();
    let root = common::test_config::repo_git_root();
    let paths = AppPaths::from_runtime_roots(root.clone(), root, std::env::temp_dir());
    let seconds = vec![0.0, 1.0, 1.25, 2.5, 4.0];
    let (mut preview, request) = rolling_chart_preview();
    preview.replace_activity(&every_family_activity()).unwrap();
    let config = common::seam::validated_config_from_value(json!({
        "scene": request["scene"], "values": [], "labels": [], "plots": [request["widget"]],
    }));
    let requirements = config.render_data_requirements();
    assert!(requirements.speed && requirements.heartrate, "plotted metrics are export requirements");
    let trimmed = trim_activity(&activity, 0.0, 4.0, &requirements).unwrap();
    let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
    let frames = render_full_frames(&paths.font_dirs, &config, &activity, &dense, &(0..dense.frame_count).collect::<Vec<_>>(), &mut RenderProfiler::default()).unwrap();
    let scene_width = config.scene.presentation.width as usize;
    let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
    for (index, second) in seconds.iter().enumerate() {
        preview.render(*second).unwrap();
        let widget = &preview.widgets[&id];
        let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
        let difference = export_tile_difference(&frames[index], scene_width, &geometry, widget.pixels());
        assert!(difference <= EXPORT_TILE_TOLERANCE, "rolling chart differs from export by {difference} at {second}");
        let revision = widget.revision();
        preview.render(*second).unwrap();
        assert_eq!(preview.widgets[&id].revision(), revision, "unchanged second re-rendered");
    }
    // The window slides continuously with the playhead.
    preview.render(1.0).unwrap();
    let previous = preview.widgets[&id].pixels().clone();
    preview.render(1.05).unwrap();
    assert_ne!(preview.widgets[&id].pixels(), &previous, "lines slide between samples");
    let six: Vec<Value> = ["speed", "heartrate", "cadence", "power", "temperature", "altitude"]
        .iter()
        .map(|metric| json!({"metric": metric, "label": metric, "color": "#ffffff"}))
        .collect();
    for (field, malformed) in [
        ("series", json!([])),
        ("series", json!(six)),
        ("series", json!([{"metric": "left_right_balance", "label": "L/R", "color": "#ffffff"}])),
        ("series", json!([{"metric": "speed", "label": "A", "color": "#ffffff"}, {"metric": "speed", "label": "B", "color": "#000000"}])),
        ("series", json!([{"metric": "bogus", "label": "A", "color": "#ffffff"}])),
        ("series", json!([{"metric": "speed", "label": " ", "color": "#ffffff"}])),
        ("series", json!([{"metric": "speed", "label": "A", "color": "white"}])),
        ("window_seconds", json!(4)),
        ("window_seconds", json!(301)),
        ("line_width", json!(0)),
        ("line_opacity", json!(101)),
        ("area_opacity", json!(-1)),
        ("area_fill", Value::Null),
        ("marker_variant", json!("star")),
        ("marker_size", json!(0)),
        ("legend_font", json!("")),
        ("legend_font_size", json!(0)),
        ("legend_color", Value::Null),
        ("show_legend", Value::Null),
        ("width", json!(0)),
        ("opacity", json!(2)),
    ] {
        let mut invalid = request.clone();
        invalid["widget"][field] = malformed;
        let rejected = serde_json::from_value(invalid).map_or(true, |request| preview.create_widget(request).is_err());
        assert!(rejected, "malformed {field} must fail at ingress");
    }
}

#[test]
fn rolling_chart_resize_reflows_the_plot_and_preserves_the_fixed_corner() {
    let (mut preview, origin) = rolling_chart_preview();
    preview.replace_activity(&every_family_activity()).unwrap();
    let id = preview.create_widget(serde_json::from_value(origin.clone()).unwrap()).unwrap();
    preview.render(1.0).unwrap();
    let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
    let bounds = &geometry["content"];
    // The fixture scene renders at scale 1.7; the legend row adds 1.6 legend font sizes below the plot.
    let legend = 14.0 * 1.6;
    assert!((bounds["width"].as_f64().unwrap() - 300.0 * 1.7).abs() < 0.01);
    assert!((bounds["height"].as_f64().unwrap() - (90.0 + legend) * 1.7).abs() < 0.01);
    let fixed = [1.0, 0.0];
    let point = [
        bounds["left"].as_f64().unwrap() + bounds["width"].as_f64().unwrap(),
        bounds["top"].as_f64().unwrap(),
    ];
    let mut gesture = origin.clone();
    gesture["size"] = json!([420.5 * 1.7, (120.25 + legend) * 1.7]);
    gesture["fixed"] = json!(fixed);
    gesture["point"] = json!(point);
    gesture["second"] = json!(1.0);
    let patch = serde_json::to_value(preview.resize_widget(serde_json::from_value(gesture).unwrap()).unwrap()).unwrap();
    assert!((patch["width"].as_f64().unwrap() - 420.5).abs() < 0.001);
    assert!((patch["height"].as_f64().unwrap() - 120.25).abs() < 0.001);
    let mut committed = origin;
    committed["widget"].as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
    preview.update_widget(id, serde_json::from_value(committed).unwrap()).unwrap();
    preview.render(1.0).unwrap();
    let actual: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
    for (axis, position, size) in [(0, "left", "width"), (1, "top", "height")] {
        let corner = actual["content"][position].as_f64().unwrap() + actual["content"][size].as_f64().unwrap() * fixed[axis];
        assert!((corner - point[axis]).abs() < 0.002, "rolling chart corner moved on axis {axis}");
    }
    // A narrow plot wraps the two legend entries onto two rows; the plot keeps the remaining height.
    let mut narrow = committed_request(&preview, id);
    narrow["size"] = json!([60.0 * 1.7, (120.25 + legend) * 1.7]);
    narrow["fixed"] = json!(fixed);
    narrow["point"] = json!(point);
    narrow["second"] = json!(1.0);
    let patch = serde_json::to_value(preview.resize_widget(serde_json::from_value(narrow.clone()).unwrap()).unwrap()).unwrap();
    assert!((patch["height"].as_f64().unwrap() - (120.25 - legend)).abs() < 0.001, "wrapped legend takes a second row");
    narrow["widget"].as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
    for field in ["size", "fixed", "point", "second"] {
        narrow.as_object_mut().unwrap().remove(field);
    }
    preview.update_widget(id, serde_json::from_value(narrow).unwrap()).unwrap();
    preview.render(1.0).unwrap();
    let wrapped: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
    assert!((wrapped["content"]["height"].as_f64().unwrap() - (120.25 + legend) * 1.7).abs() < 0.01);
}

/// The request a test last committed to `id`, rebuilt from the fixture with its committed frame.
fn committed_request(preview: &PreviewSession, id: u32) -> Value {
    let (_, mut request) = rolling_chart_preview();
    let geometry: Value = serde_json::from_slice(preview.widgets[&id].geometry()).unwrap();
    let scale = 1.7;
    let legend = 14.0 * 1.6;
    request["widget"].as_object_mut().unwrap().extend(json!({
        "x": geometry["content"]["left"], "y": geometry["content"]["top"],
        "width": geometry["content"]["width"].as_f64().unwrap() / scale,
        "height": geometry["content"]["height"].as_f64().unwrap() / scale - legend,
    }).as_object().unwrap().clone());
    request
}
