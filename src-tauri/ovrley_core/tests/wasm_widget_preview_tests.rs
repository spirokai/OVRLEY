//! One seam test: transported font + standalone cached surface must match native composition.
mod common;

use ovrley_core::fonts::{preview_font_resource, NativeFonts};
use ovrley_widgets::{
    model::scene::validate_scene_presentation,
    preview::PreviewSession,
    render::text::validated_value_style,
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
    request["widget"] = defaults["plot"]["definitions"]["elevation"]["defaults"].clone();
    request["widget"].as_object_mut().unwrap().extend(json!({
        "id": "elevation", "x": 150.25, "y": 90.5, "width": 280.125, "height": 145.25, "rotation": 27.0,
        "show_elevation_imperial": true, "starting_altitude": 3500, "starting_altitude_unit": "ft",
        "metric_label_offset_x": 7.25, "metric_label_offset_y": -22.5,
        "imperial_label_offset_x": 18.5, "imperial_label_offset_y": 4.25,
        "point_label": {"font": "Inter.ttf", "font_size": 19.5, "color": "#abcdef"}
    }).as_object().unwrap().clone());
    request
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
    let font: ovrley_widgets::preview::FontRequest = serde_json::from_value(json!({"font": "Inter.ttf"})).unwrap();
    let dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let bytes = preview_font_resource(&dirs, &font.font, font.font_weight, font.italic).unwrap();
    preview.fonts.register(font, &bytes).unwrap();
    preview.replace_activity(activity).unwrap();
    let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
    (preview, id)
}

fn prepared_elevation(preview: &PreviewSession, id: u32) -> std::sync::Arc<ovrley_widgets::widgets::elevation::geometry::ElevationGeometry> {
    let ovrley_widgets::preview_widget::PreviewWidget::Elevation(widget) = &preview.widgets[&id] else { panic!("expected elevation handle"); };
    widget.prepared_geometry.as_ref().unwrap().clone()
}

#[test]
fn elevation_preview_matches_native_export_labels_hover_seeking_and_activity_replacement() {
    use ovrley_core::{activity::{interpolate::densify_activity, trim::trim_activity}, debug::RenderProfiler, paths::AppPaths, render::{prepare_preview_assets, FrameSize, VideoFrameRenderer}};
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
        let requirements = config.render_data_requirements().unwrap();
        let trimmed = trim_activity(&activity, 0.25, 3.25, &requirements).unwrap();
        let seconds = vec![0.0, 0.75, 1.5, 2.2, 2.9];
        let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
        let (assets, _, _, _) = prepare_preview_assets(&paths, &config, &activity, &dense).unwrap();
        let renderer = VideoFrameRenderer::new(&paths, &dense, &assets, FrameSize {width: 700, height: 500}, 0).unwrap();
        let (mut preview, id) = elevation_preview(&request, &bytes);
        let raw = serde_json::from_value(request["widget"].clone()).unwrap();
        let scene = validate_scene_presentation(serde_json::from_value(request["scene"].clone()).unwrap()).unwrap();
        assert_eq!(config.elevation_plots[0], ovrley_widgets::widgets::elevation::validation::validate_elevation_plot(&raw, 0, &scene).unwrap());
        let mut selection = None;
        for index in [0, 1, 2, 3, 4, 2, 0] {
            let second = 0.25 + seconds[index];
            preview.render(second).unwrap();
            let prepared = prepared_elevation(&preview, id);
            assert_eq!(assets.elevation_geometry_json().unwrap()["points"], serde_json::to_value(&prepared.points).unwrap());
            let widget = &preview.widgets[&id];
            let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
            if let Some(selection) = &selection { assert_eq!(&geometry["content"], selection, "labels and hover must not move the selection frame"); }
            else { selection = Some(geometry["content"].clone()); }
            let (w, h, left, top) = (geometry["width"].as_u64().unwrap() as usize, geometry["height"].as_u64().unwrap() as usize, geometry["left"].as_f64().unwrap() as usize, geometry["top"].as_f64().unwrap() as usize);
            let mut native = vec![0; 700 * 500 * 4];
            renderer.render_rgba(index, &mut native, &mut RenderProfiler::default()).unwrap();
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
            assert!(difference <= 8, "{variant}/{full} native elevation differs at {second}: {difference}; {largest:?}");
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
    let mut widget = defaults["plot"]["definitions"]["course"]["defaults"].clone();
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
    let ovrley_widgets::preview_widget::PreviewWidget::Route(route) = &preview.widgets[&id] else { panic!("expected route handle"); };
    route.prepared_geometry.as_ref().unwrap().clone()
}

#[test]
fn route_preview_matches_native_export_and_keeps_marker_out_of_selection_bounds() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, trim::trim_activity}, debug::RenderProfiler, paths::AppPaths,
        render::{prepare_preview_assets, FrameSize, VideoFrameRenderer},
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
        let requirements = config.render_data_requirements().unwrap();
        let trimmed = trim_activity(&activity, 0.25, 3.25, &requirements).unwrap();
        let seconds = vec![0.0, 0.75, 2.2, 2.9];
        let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
        let (assets, _, _, _) = prepare_preview_assets(&paths, &config, &activity, &dense).unwrap();
        let renderer = VideoFrameRenderer::new(&paths, &dense, &assets, FrameSize {width: 700, height: 500}, 0).unwrap();
        let mut preview = PreviewSession::default();
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
            let mut native = vec![0; 700 * 500 * 4];
            renderer.render_rgba(index, &mut native, &mut RenderProfiler::default()).unwrap();
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
            assert!(max_difference <= 8, "{variant}/{full} native route differs at {second}: {max_difference}");
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
        reloaded.replace_activity(&bytes).unwrap();
        let reloaded_id = reloaded.create_widget(serde_json::from_value(committed).unwrap()).unwrap();
        reloaded.render(2.35).unwrap();
        assert_eq!(preview.widgets[&id].geometry(), reloaded.widgets[&reloaded_id].geometry());
        assert_eq!(preview.widgets[&id].pixels(), reloaded.widgets[&reloaded_id].pixels());
    }
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
        "track_filled_color": "#40e0d0", "track_filled_opacity": 0.7, "track_fill_flat": false,
        "track_fill_style": fill, "bar_count": 8, "bar_gap": 3,
        "show_min_max_labels": true, "min_max_label_font": "Teko.ttf", "min_max_label_font_size": 16,
        "min_max_label_position": if placement == "vertical" { "left" } else { "bottom" }, "min_max_label_color": "#cdefab",
        "font": "Inter.ttf", "font_size": 60, "color": "#abcdef", "opacity": 0.8,
        "show_units": true, "unit_color": "#aaaaaa", "display_unit": match metric { "speed" => "kmh", "altitude" => "ft", _ => "w" },
        "prefix": "", "suffix": "", "decimals": 0,
    });
    if metric == "altitude" {
        request["widget"]["starting_altitude"] = json!(4000);
    }
    (preview, request)
}

#[test]
fn gauges_match_native_pixels_sampling_cache_revisions_and_activity_replacement() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, trim::trim_activity},
        debug::RenderProfiler,
        paths::AppPaths,
        render::widgets::{metric_presentation::draw_metric_presentation, prepare_render_assets},
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
    for (display, metric, placement, fill) in [
        ("linear", "speed", "horizontal", "fill"),
        ("linear", "power", "vertical", "bars"),
        ("arc", "speed", "bottom-left", "fill"),
        ("arc", "power", "bottom-left", "bars"),
        ("corner", "altitude", "bottom-left", "fill"),
        ("corner", "altitude", "bottom-right", "bars"),
    ] {
        let (mut preview, mut request) = gauge_preview(display, metric, placement, fill);
        request["scene"]["opacity"] = json!(0.5);
        preview.replace_activity(&bytes).unwrap();
        let config = common::seam::validated_config_from_value(
            json!({"scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": []}),
        );
        let requirements = config.render_data_requirements().unwrap();
        let trimmed = trim_activity(&activity, 0.0, 4.0, &requirements).unwrap();
        let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
        let assets = prepare_render_assets(
            &paths,
            &config,
            &activity,
            &dense,
            &mut RenderProfiler::default(),
        )
        .unwrap();
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
            let info = ImageInfo::new(
                (
                    geometry["width"].as_i64().unwrap() as i32,
                    geometry["height"].as_i64().unwrap() as i32,
                ),
                ColorType::RGBA8888,
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
            draw_metric_presentation(
                canvas,
                &assets.values()[0],
                &dense,
                index,
                config.scene.presentation.scale,
                &paths.font_dirs,
                &mut RenderProfiler::default(),
            )
            .unwrap();
            let mut pixels = vec![0; widget.pixels().len()];
            assert!(surface.read_pixels(
                &info.with_alpha_type(AlphaType::Unpremul),
                &mut pixels,
                info.width() as usize * 4,
                (0, 0)
            ));
            assert!(
                pixels == *widget.pixels(),
                "{display}/{metric}/{placement}/{fill} pixels differ at {}",
                seconds[index]
            );
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
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
                        ovrley_core::render::widgets::types::PreparedValue::LinearGauge(widget) => {
                            ValidatedGauge::Linear(widget.validated.clone())
                        }
                        ovrley_core::render::widgets::types::PreparedValue::ArcGauge(widget) => {
                            ValidatedGauge::Arc(widget.validated.clone())
                        }
                        _ => panic!("gauge dispatch"),
                    };
                    let layout = GaugeLayout::new(
                        &widget,
                        &config.scene.presentation,
                        sampler.gauge_range(widget.metric()),
                        0.0,
                        &NativeFonts(&font_dirs),
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
fn heading_tape_matches_native_pixels_sampling_revisions_and_activity_replacement() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, trim::trim_activity},
        debug::RenderProfiler, paths::AppPaths,
        render::widgets::{prepare_render_assets, metric_presentation::draw_metric_presentation},
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
        let requirements = config.render_data_requirements().unwrap();
        let trimmed = trim_activity(&activity, 0.0, 4.0, &requirements).unwrap();
        let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
        let assets = prepare_render_assets(&paths, &config, &activity, &dense, &mut RenderProfiler::default()).unwrap();
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
            let info = ImageInfo::new((geometry["width"].as_i64().unwrap() as i32, geometry["height"].as_i64().unwrap() as i32), ColorType::RGBA8888, AlphaType::Premul, None);
            let mut surface = surfaces::raster(&info, None, None).unwrap();
            let canvas = surface.canvas();
            canvas.clear(Color::TRANSPARENT);
            canvas.translate((-geometry["left"].as_f64().unwrap() as f32, -geometry["top"].as_f64().unwrap() as f32));
            draw_metric_presentation(canvas, &assets.values()[0], &dense, index, 1.7, &paths.font_dirs, &mut RenderProfiler::default()).unwrap();
            let mut pixels = vec![0; widget.pixels().len()];
            assert!(surface.read_pixels(&info.with_alpha_type(AlphaType::Unpremul), &mut pixels, info.width() as usize * 4, (0,0)));
            assert_eq!(&pixels, widget.pixels(), "{style}/{placement} at {second}");
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
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
    let ovrley_core::render::widgets::types::PreparedValue::HeadingTape(widget) = &config.values[0] else { panic!("heading dispatch"); };
    let cache = ovrley_core::render::widgets::heading::prepare_heading_cache(&config.scene, &widget.validated, &paths.font_dirs, &mut RenderProfiler::default()).unwrap();
    let info = ImageInfo::new((400, 80), ColorType::RGBA8888, AlphaType::Premul, None);
    let mut surface = surfaces::raster(&info, None, None).unwrap();
    surface.canvas().clear(Color::BLUE);
    ovrley_core::render::widgets::heading::draw_heading_widget(surface.canvas(), &cache, 0.0, &mut RenderProfiler::default());
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
                    let ovrley_core::render::widgets::types::PreparedValue::HeadingTape(committed) = &config.values[0] else { panic!("heading dispatch"); };
                    let scene = validate_scene_presentation(serde_json::from_value(live["scene"].clone()).unwrap()).unwrap();
                    let cache = ovrley_widgets::widgets::heading::layout::prepare_heading_cache(&committed.validated, &scene, &preview.fonts).unwrap();
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
        "lap_durations_seconds": [2,1.25], "lap_durations_best_so_far_seconds": [2,1.25],
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
        "positive_delta_color": "#00ff00", "negative_delta_color": "#ff0000",
    });
    (preview, request)
}

#[test]
fn lap_timer_modes_match_native_pixels_sampling_and_resource_replacement() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, trim::trim_activity},
        debug::RenderProfiler,
        paths::AppPaths,
        render::widgets::{prepare_render_assets, lap_timer::{draw_lap_timer, lap_history, lap_sample}, types::PreparedValue},
    };
    use ovrley_widgets::{activity::ActivitySampler, widgets::lap_timer::format::lap_timer_content};
    let (mut preview, mut request) = lap_timer_preview();
    let mut activity = lap_timer_activity();
    let bytes = serde_json::to_vec(&activity).unwrap();
    let sampler = ActivitySampler::from_json(&bytes).unwrap();
    preview.replace_activity(&bytes).unwrap();
    let root = common::test_config::repo_git_root();
    let paths = AppPaths::from_runtime_roots(root.clone(), root, std::env::temp_dir());
    let seconds = vec![0.0, 0.25, 0.5, 1.121, 1.122, 2.25, 2.5, 3.375, 3.75, 4.0, 4.5, 5.0];
    for mode in ["current_lap", "best_lap", "delta", "lap_log"] {
        request["widget"]["lap_timer_mode"] = json!(mode);
        let config = common::seam::validated_config_from_value(json!({
            "scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": [],
        }));
        let requirements = config.render_data_requirements().unwrap();
        let trimmed = trim_activity(&activity, 0.0, 5.0, &requirements).unwrap();
        let dense = densify_activity(&trimmed, seconds.clone(), &requirements);
        let assets = prepare_render_assets(&paths, &config, &activity, &dense, &mut RenderProfiler::default()).unwrap();
        let PreparedValue::LapTimer(native) = &assets.values()[0] else { panic!("lap timer dispatch"); };
        let id = preview.create_widget(serde_json::from_value(request.clone()).unwrap()).unwrap();
        for index in [0,1,2,3,4,5,6,7,8,9,10,11,6,3,0] {
            let second = seconds[index];
            assert_eq!(
                lap_timer_content(&native.validated, sampler.lap_history(), sampler.lap_timer_at(second)).unwrap(),
                lap_timer_content(&native.validated, lap_history(&dense), lap_sample(&dense, index, native.validated.mode).unwrap()).unwrap(),
                "{mode} sampling at {second}",
            );
            preview.render(second).unwrap();
            let widget = &preview.widgets[&id];
            let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
            let info = ImageInfo::new((geometry["width"].as_i64().unwrap() as i32, geometry["height"].as_i64().unwrap() as i32), ColorType::RGBA8888, AlphaType::Premul, None);
            let mut surface = surfaces::raster(&info, None, None).unwrap();
            let canvas = surface.canvas();
            canvas.clear(Color::TRANSPARENT);
            canvas.translate((-geometry["left"].as_f64().unwrap() as f32, -geometry["top"].as_f64().unwrap() as f32));
            draw_lap_timer(canvas, native, &dense, index).unwrap();
            let mut pixels = vec![0; widget.pixels().len()];
            assert!(surface.read_pixels(&info.with_alpha_type(AlphaType::Unpremul), &mut pixels, info.width() as usize * 4, (0,0)));
            assert_eq!(&pixels, widget.pixels(), "{mode} at {second}");
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
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
fn g_force_preview_matches_native_pixels_and_tracks_marker_and_percentile_changes() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, schema::ParsedActivity, trim::trim_activity},
        debug::RenderProfiler,
        render::widgets::{g_force::{draw_g_force_widget, prepare_g_force_cache}, types::PreparedValue},
    };
    let fixture: Value = serde_json::from_str(include_str!("fixtures/g-force-frame-state.json")).unwrap();
    let mut activity: ParsedActivity = serde_json::from_value(fixture["activity"].clone()).unwrap();
    let (mut preview, request) = g_force_preview();
    preview.replace_activity(&serde_json::to_vec(&activity).unwrap()).unwrap();
    let config = common::seam::validated_config_from_value(json!({
        "scene": request["scene"], "values": [request["widget"]], "labels": [], "plots": [],
    }));
    let PreparedValue::GForce(widget) = &config.values[0] else { panic!("G-force dispatch"); };
    let requirements = config.render_data_requirements().unwrap();
    let trimmed = trim_activity(&activity, 0.0, 5.0, &requirements).unwrap();
    let seconds = [1.0, 1.002, 3.0];
    let dense = densify_activity(&trimmed, seconds.to_vec(), &requirements);
    let mut profiler = RenderProfiler::default();
    let cache = prepare_g_force_cache(&widget.validated, &config.scene, &activity, &dense, &mut profiler).unwrap();
    let font_dirs = vec![common::test_config::repo_git_root().join("fonts")];
    let id = preview.create_widget(serde_json::from_value(request).unwrap()).unwrap();
    for index in [0, 1, 2, 0] {
        preview.render(seconds[index]).unwrap();
        let widget = &preview.widgets[&id];
        let geometry: Value = serde_json::from_slice(widget.geometry()).unwrap();
        let info = ImageInfo::new(
            (geometry["width"].as_i64().unwrap() as i32, geometry["height"].as_i64().unwrap() as i32),
            ColorType::RGBA8888, AlphaType::Premul, None,
        );
        let mut surface = surfaces::raster(&info, None, None).unwrap();
        let canvas = surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        canvas.translate((-geometry["left"].as_f64().unwrap() as f32, -geometry["top"].as_f64().unwrap() as f32));
        draw_g_force_widget(canvas, &cache, index, &font_dirs, &mut profiler).unwrap();
        let mut pixels = vec![0; widget.pixels().len()];
        assert!(surface.read_pixels(
            &info.with_alpha_type(AlphaType::Unpremul), &mut pixels, info.width() as usize * 4, (0, 0),
        ));
        assert_eq!(&pixels, widget.pixels(), "G-force at {}", seconds[index]);
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
                "track_border_thickness": 2, "track_border_color": "#ffffff", "track_thickness": 24,
                "font": "Inter.ttf", "font_size": 60, "color": "#abcdef", "unit_color": "#aaaaaa",
                "show_units": true, "value_offset_x": 11.25, "value_offset_y": -17.5,
            },
        }),
    )
}

#[test]
fn lean_angle_preview_matches_native_export_and_tracks_signed_fill_resources_and_validation() {
    use ovrley_core::{
        activity::{interpolate::densify_activity, schema::ParsedActivity, trim::trim_activity},
        debug::RenderProfiler,
        paths::AppPaths,
        render::widgets::{draw_metric_presentation, prepare_render_assets},
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
    let requirements = config.render_data_requirements().unwrap();
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
    let mut profiler = RenderProfiler::default();
    let assets = prepare_render_assets(&paths, &config, &parsed, &dense, &mut profiler).unwrap();
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
        let info = ImageInfo::new(
            (
                geometry["width"].as_i64().unwrap() as i32,
                geometry["height"].as_i64().unwrap() as i32,
            ),
            ColorType::RGBA8888,
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
        draw_metric_presentation(
            canvas,
            &assets.values()[0],
            &dense,
            index,
            config.scene.presentation.scale,
            &paths.font_dirs,
            &mut profiler,
        )
        .unwrap();
        let mut pixels = vec![0; widget.pixels().len()];
        assert!(surface.read_pixels(
            &info.with_alpha_type(AlphaType::Unpremul),
            &mut pixels,
            info.width() as usize * 4,
            (0, 0)
        ));
        assert_eq!(&pixels, widget.pixels(), "lean-angle at {second}");
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
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
    let requirements = config.render_data_requirements().unwrap();
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
            ColorType::RGBA8888,
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
        assert!(surface.read_pixels(
            &info.with_alpha_type(AlphaType::Unpremul),
            &mut pixels,
            info.width() as usize * 4,
            (0, 0)
        ));
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
                    ColorType::RGBA8888,
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
                assert!(surface.read_pixels(
                    &info.with_alpha_type(AlphaType::Unpremul),
                    &mut pixels,
                    info.width() as usize * 4,
                    (0, 0)
                ));
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
    ovrley_core::render::text::draw_text(
        canvas, &label.text, &validated_label_style(&label, &scene, scene.scale),
        &[common::test_config::repo_git_root().join("fonts")],
    ).unwrap();
    let mut pixels = vec![0; widget.pixels().len()];
    assert!(surface.read_pixels(&info.with_alpha_type(AlphaType::Unpremul), &mut pixels, info.width() as usize * 4, (0, 0)));
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
                ColorType::RGBA8888,
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
            let read_info = info.with_alpha_type(AlphaType::Unpremul);
            assert!(surface.read_pixels(
                &read_info,
                &mut pixels,
                info.width() as usize * 4,
                (0, 0)
            ));
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
