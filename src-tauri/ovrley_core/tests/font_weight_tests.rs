mod common;

use ovrley_core::commands::backend_list_system_fonts;
use ovrley_core::commands::validate_config_value;
use ovrley_core::normalize::parse_template_value;
use ovrley_core::paths::AppPaths;
use ovrley_core::render::text::{measure_text, validated_label_style};
use serde_json::json;

#[test]
fn legacy_templates_upgrade_weight_and_changed_identity_without_rewriting_input() {
    for version in [2, 3] {
        let mut config = label_config(537.0);
        config["labels"][0]
            .as_object_mut()
            .unwrap()
            .remove("font_weight");
        config["labels"][0]
            .as_object_mut()
            .unwrap()
            .remove("italic");
        config["labels"][0]
            .as_object_mut()
            .unwrap()
            .remove("letter_spacing");
        config["labels"].as_array_mut().unwrap().push(json!({
            "text": "Italic", "x": 0, "y": 0, "font": "Inter.ttf", "font_size": 32,
            "font_weight": 537, "italic": true, "letter_spacing": -1.25, "color": "#ffffff", "opacity": 1
        }));
        config["labels"][0]["font"] = json!("Inter ExtraBold.ttf");
        config["values"] = json!([{ "value": "speed", "x": 0, "y": 0 }]);
        if version == 2 {
            config.as_object_mut().unwrap().remove("rasters");
        }
        let source = json!({"format": "ovrley-template", "version": version, "config": config,
            "settings": { "globalDefaults": { "font_values": "Inter ExtraBold.ttf" } }});
        let loaded = parse_template_value(&source).unwrap();
        assert_eq!(loaded.labels[0].typography.font_weight, 400.0);
        assert!(!loaded.labels[0].typography.italic);
        assert_eq!(loaded.labels[0].typography.letter_spacing, 0.0);
        assert_eq!(loaded.labels[1].typography.letter_spacing, -1.25);
        assert!(loaded.labels[1].typography.italic);
        assert_eq!(loaded.labels[0].font.as_deref(), Some("Inter.ttf"));
        assert_eq!(loaded.values[0].font.as_deref(), Some("Inter.ttf"));
        assert!(source["config"]["labels"][0].get("font_weight").is_none());
        assert!(source["config"]["labels"][0].get("italic").is_none());
        assert!(source["config"]["labels"][0]
            .get("letter_spacing")
            .is_none());
        let saved = json!({"format": "ovrley-template", "version": 3, "config": loaded,
            "settings": { "globalDefaults": {} }});
        assert_eq!(
            parse_template_value(&saved).unwrap().labels[0]
                .typography
                .font_weight,
            400.0
        );
        assert!(
            parse_template_value(&saved).unwrap().labels[1]
                .typography
                .italic
        );
        assert_eq!(
            parse_template_value(&saved).unwrap().labels[1]
                .typography
                .letter_spacing,
            -1.25
        );
    }
}

fn label_config(weight: f32) -> serde_json::Value {
    json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [{ "text": "Weight Matters", "x": 20, "y": 20, "font": "Inter.ttf",
            "font_size": 64, "font_weight": weight, "italic": false, "letter_spacing": 0, "color": "#ffffff", "opacity": 1 }],
        "backdrops": [], "rasters": [], "values": [], "plots": []
    })
}

#[test]
fn catalog_reports_actual_per_face_weight_ranges_and_italics() {
    let paths = AppPaths::from_repo_root(common::test_config::repo_git_root());
    let catalog = backend_list_system_fonts(&paths).unwrap();
    for (id, min, max, italic) in [
        ("Inter.ttf", 100.0, 900.0, true),
        ("Teko.ttf", 300.0, 700.0, false),
        ("Oxanium.ttf", 200.0, 800.0, false),
        ("JetBrains Mono.ttf", 100.0, 800.0, true),
        ("Saira Stencil.ttf", 100.0, 900.0, true),
    ] {
        let family = catalog["recommendedFonts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|font| font["id"] == id)
            .unwrap_or_else(|| panic!("missing {id}"));
        let faces = family["faces"].as_array().unwrap();
        let normal = faces.iter().find(|face| face["style"] == "normal").unwrap();
        let weight = normal["axes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|axis| axis["tag"] == "wght")
            .unwrap();
        assert_eq!(weight["min"], min);
        assert_eq!(weight["max"], max);
        assert_eq!(faces.iter().any(|face| face["style"] == "italic"), italic);
    }
}

#[test]
fn canonical_render_rejects_missing_and_malformed_weights() {
    let mut missing = label_config(400.0);
    missing["labels"][0]
        .as_object_mut()
        .unwrap()
        .remove("font_weight");
    assert!(validate_config_value(&missing)
        .err()
        .unwrap()
        .to_string()
        .contains("font_weight"));
    for weight in [
        json!("400"),
        json!(null),
        json!(0),
        json!(1001),
        json!(1000.00001),
        json!(0.99999999),
    ] {
        let mut config = label_config(400.0);
        config["labels"][0]["font_weight"] = weight;
        assert!(validate_config_value(&config)
            .err()
            .unwrap()
            .to_string()
            .contains("font_weight"));
    }
}

#[test]
#[cfg(target_os = "windows")]
fn system_faces_are_discovered_lazily_with_real_static_weights_and_italics() {
    let paths = AppPaths::from_repo_root(common::test_config::repo_git_root());
    let catalog = backend_list_system_fonts(&paths).unwrap();
    let identity = catalog["systemFonts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|font| font["id"] == "Arial")
        .unwrap();
    assert!(identity["faces"].is_null());
    let font = ovrley_core::commands::backend_font_capabilities(&paths, "Arial").unwrap();
    let faces = font["faces"].as_array().unwrap();
    assert!(faces.iter().any(|face| face["style"] == "italic"));
    assert!(faces
        .iter()
        .any(|face| face["style"] == "normal" && face["weight"] == 700.0));
    assert!(faces
        .iter()
        .all(|face| face["axes"].as_array().unwrap().is_empty()));
}

#[test]
fn label_weight_changes_measurement_and_invalidates_static_cache() {
    use ovrley_core::debug::RenderProfiler;
    use ovrley_core::render::{LabelCacheStatus, StaticLayer};

    let paths = AppPaths::from_repo_root(common::test_config::repo_git_root());
    let render = |weight| {
        let mut input = label_config(weight);
        input["labels"][0]["text"] = json!("Cache weight sample");
        let config = validate_config_value(&input).unwrap();
        let style = validated_label_style(&config.labels[0], &config.scene, 1.0);
        let width = measure_text("Cache weight sample", &style, &paths.font_dirs)
            .unwrap()
            .width;
        let layer = StaticLayer {
            backdrops: &[],
            rasters: &[],
            labels: &config.labels,
            values: &[],
            scene: &config.scene,
        }
        .prepare(&paths, &mut RenderProfiler::default())
        .unwrap();
        (width, layer)
    };
    let (thin_width, thin) = render(100.0);
    let (middle_width, middle) = render(537.0);
    let (heavy_width, heavy) = render(900.0);
    assert!(thin_width < middle_width && middle_width < heavy_width);
    for layer in [&thin, &middle, &heavy] {
        assert!(matches!(layer.cache_status, LabelCacheStatus::Miss));
    }
    assert_ne!(thin.base_rgba, middle.base_rgba);
    assert_ne!(middle.base_rgba, heavy.base_rgba);
    let (_, repeated) = render(537.0);
    assert!(matches!(repeated.cache_status, LabelCacheStatus::Hit));
    assert_eq!(middle.base_rgba, repeated.base_rgba);
}

#[test]
fn italic_requires_a_boolean_at_render_and_saved_template_ingress() {
    let mut missing = label_config(537.0);
    missing["labels"][0]
        .as_object_mut()
        .unwrap()
        .remove("italic");
    assert!(validate_config_value(&missing)
        .err()
        .unwrap()
        .to_string()
        .contains("italic"));
    for italic in [json!(null), json!("true"), json!(1)] {
        let mut input = label_config(537.0);
        input["labels"][0]["italic"] = italic;
        assert!(validate_config_value(&input)
            .err()
            .unwrap()
            .to_string()
            .contains("italic"));
        for version in [2, 3] {
            let mut config = input.clone();
            if version == 2 {
                config.as_object_mut().unwrap().remove("rasters");
            }
            let saved = json!({"format": "ovrley-template", "version": version,
                "config": config, "settings": { "globalDefaults": {} }});
            assert!(parse_template_value(&saved)
                .err()
                .unwrap()
                .to_string()
                .contains("italic"));
        }
    }
}

#[test]
fn genuine_italic_changes_ink_bounds_and_cached_pixels_with_border_and_shadow() {
    use ovrley_core::debug::RenderProfiler;
    use ovrley_core::render::{LabelCacheStatus, StaticLayer};

    let paths = AppPaths::from_repo_root(common::test_config::repo_git_root());
    let render = |font, italic| {
        let mut input = label_config(537.0);
        input["labels"][0]["text"] = json!("ffffj Italic ink");
        input["labels"][0]["font"] = json!(font);
        input["labels"][0]["italic"] = json!(italic);
        input["scene"]["shadow_color"] = json!("#ff0000");
        input["scene"]["shadow_strength"] = json!(2);
        input["scene"]["shadow_distance"] = json!(3);
        input["scene"]["border_thickness"] = json!(1);
        let config = validate_config_value(&input).unwrap();
        let style = validated_label_style(&config.labels[0], &config.scene, 1.0);
        let measurement = measure_text(&config.labels[0].text, &style, &paths.font_dirs).unwrap();
        let layer = StaticLayer {
            backdrops: &[],
            rasters: &[],
            labels: &config.labels,
            values: &[],
            scene: &config.scene,
        }
        .prepare(&paths, &mut RenderProfiler::default())
        .unwrap();
        (measurement, layer)
    };
    let (upright_bounds, upright) = render("Inter.ttf", false);
    let (italic_bounds, italic) = render("Inter.ttf", true);
    assert!(matches!(upright.cache_status, LabelCacheStatus::Miss));
    assert!(matches!(italic.cache_status, LabelCacheStatus::Miss));
    assert_ne!(upright.base_rgba, italic.base_rgba);
    assert_ne!(upright_bounds.bounds_right, italic_bounds.bounds_right);
    let (_, repeated) = render("Inter.ttf", true);
    assert!(matches!(repeated.cache_status, LabelCacheStatus::Hit));
    assert_eq!(italic.base_rgba, repeated.base_rgba);
    for font in ["Teko.ttf", "Oxanium.ttf"] {
        let (upright_bounds, upright) = render(font, false);
        let (italic_bounds, unsupported) = render(font, true);
        assert_eq!(upright.base_rgba, unsupported.base_rgba);
        assert_eq!(upright_bounds.bounds_right, italic_bounds.bounds_right);
    }
    let normal =
        ovrley_core::fonts::resolve_typeface(&paths.font_dirs, "Inter.ttf", 537.0, false).unwrap();
    let italic =
        ovrley_core::fonts::resolve_typeface(&paths.font_dirs, "Inter.ttf", 537.0, true).unwrap();
    assert_ne!(normal.unique_id(), italic.unique_id());
    assert_eq!(
        italic.font_style().slant(),
        skia_safe::font_style::Slant::Italic
    );
}

#[test]
fn spacing_requires_a_finite_number_at_render_and_saved_template_ingress() {
    let mut missing = label_config(537.0);
    missing["labels"][0]
        .as_object_mut()
        .unwrap()
        .remove("letter_spacing");
    assert!(validate_config_value(&missing)
        .err()
        .unwrap()
        .to_string()
        .contains("letter_spacing"));
    for spacing in [json!(null), json!("0"), json!(true), json!(1e100)] {
        let mut input = label_config(537.0);
        input["labels"][0]["letter_spacing"] = spacing;
        assert!(validate_config_value(&input)
            .err()
            .unwrap()
            .to_string()
            .contains("letter_spacing"));
        for version in [2, 3] {
            let mut config = input.clone();
            if version == 2 {
                config.as_object_mut().unwrap().remove("rasters");
            }
            let saved = json!({"format": "ovrley-template", "version": version,
                "config": config, "settings": { "globalDefaults": {} }});
            assert!(parse_template_value(&saved)
                .unwrap_err()
                .to_string()
                .contains("letter_spacing"));
        }
    }
}

#[test]
fn spacing_layout_uses_grapheme_gaps_without_trailing_advance_and_scales_with_text() {
    let paths = AppPaths::from_repo_root(common::test_config::repo_git_root());
    let mut input = label_config(537.0);
    input["labels"][0]["italic"] = json!(true);
    let config = validate_config_value(&input).unwrap();
    let measure = |text: &str, spacing: f32, font_size, scale| {
        let mut label = config.labels[0].clone();
        label.typography.letter_spacing = spacing;
        label.font_size = font_size;
        let style = validated_label_style(&label, &config.scene, scale);
        measure_text(text, &style, &paths.font_dirs).unwrap()
    };
    // Four clusters: accented combining sequence, whitespace, supplementary character, accent.
    let text = "e\u{301} 😀é";
    // 12.5% of 32 px is 4 px per gap: three gaps add 12 px.
    for (spacing, extra_advance) in [(-12.5, -12.0), (12.5, 12.0)] {
        for (font_size, scale, expected) in [
            (32.0, 1.0, extra_advance),
            (64.0, 1.0, extra_advance * 2.0),
            (32.0, 2.0, extra_advance * 2.0),
        ] {
            let zero = measure(text, 0.0, font_size, scale);
            let spaced = measure(text, spacing, font_size, scale);
            assert!((spaced.width - zero.width - expected).abs() < 0.001);
        }
    }
    for text in ["", "A", "e\u{301}", "é", "😀", " "] {
        let zero = measure(text, 0.0, 64.0, 1.0);
        for spacing in [-2.5, 2.5] {
            let spaced = measure(text, spacing, 64.0, 1.0);
            assert_eq!(spaced.width, zero.width);
            assert_eq!(spaced.bounds_left, zero.bounds_left);
            assert_eq!(spaced.bounds_right, zero.bounds_right);
        }
    }
    let overlapping = measure("AB", -200.0, 64.0, 1.0);
    let first = measure("A", 0.0, 64.0, 1.0);
    let second = measure("B", 0.0, 64.0, 1.0);
    assert!(overlapping.width < 0.0);
    assert!((overlapping.bounds_left - (first.width - 128.0 + second.bounds_left)).abs() < 0.001);
    assert_eq!(overlapping.bounds_right, first.bounds_right);
}

#[test]
fn spacing_invalidates_static_label_images_with_weight_italic_border_and_shadow() {
    use ovrley_core::debug::RenderProfiler;
    use ovrley_core::render::{LabelCacheStatus, StaticLayer};
    let paths = AppPaths::from_repo_root(common::test_config::repo_git_root());
    let render = |spacing| {
        let mut input = label_config(537.0);
        input["labels"][0]["text"] = json!("Spacing cache é e\u{301}");
        input["labels"][0]["italic"] = json!(true);
        input["labels"][0]["letter_spacing"] = json!(spacing);
        input["scene"]["shadow_strength"] = json!(2);
        input["scene"]["shadow_distance"] = json!(3);
        input["scene"]["border_thickness"] = json!(1);
        let config = validate_config_value(&input).unwrap();
        assert_eq!(config.labels[0].typography.font_weight, 537.0);
        assert!(config.labels[0].typography.italic);
        StaticLayer {
            backdrops: &[],
            rasters: &[],
            labels: &config.labels,
            values: &[],
            scene: &config.scene,
        }
        .prepare(&paths, &mut RenderProfiler::default())
        .unwrap()
    };
    let zero = render(0.0);
    let positive = render(2.5);
    let negative = render(-2.5);
    for layer in [&zero, &positive, &negative] {
        assert!(matches!(layer.cache_status, LabelCacheStatus::Miss));
    }
    assert_ne!(zero.base_rgba, positive.base_rgba);
    assert_ne!(positive.base_rgba, negative.base_rgba);
    let repeated = render(2.5);
    assert!(matches!(repeated.cache_status, LabelCacheStatus::Hit));
    assert_eq!(positive.base_rgba, repeated.base_rgba);
}
