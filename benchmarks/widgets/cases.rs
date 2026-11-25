//! The fixed cases: one widget per visual variant, with the editor's defaults and effective data.

use crate::editor;
use ovrley_app::features::widgets::preview::{widget_fonts, FontKey};
use ovrley_app::i18n::DEFAULT_LOCALE;
use ovrley_app::store::template::editor_effective_config;
use ovrley_widgets::model::catalog::catalog;
use ovrley_widgets::model::document::{
    create_widget_defaults, merge_widget_data, number, resolve_active_widget_data, Collection, OverlayConfig, WidgetCreationOptions,
    WidgetSelection,
};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

const FPS: u32 = 30;

/// A case's activity: its file in debug/activities and the second its playback starts at.
const FIXTURES: [(&str, &str, u32); 6] = [
    ("ride", "Evening_Ride-parse-debug.json", 60),
    ("racechrono", "sample RaceChrono-parse-debug.json", 60),
    ("racebox", "sample Racebox-parse-debug.json", 400),
    ("dive", "Shearwater-Cloud-Dive-01-parse-debug.json", 1560),
    // Three 1 km splits are complete at 520 s; the fourth completes at 550 s, inside the measured pass.
    ("splits", "Evening_Ride-parse-debug.json", 520),
    // The drone climbs from -5.6 m to 22.4 m in the measured pass, crossing zero and major ticks.
    ("flight", "Jul-14th-2026-12-45PM-Flight-Airdata-parse-debug.json", 100),
];

struct Spec {
    id: String,
    /// `None` for the empty scene.
    kind: Option<&'static str>,
    fixture: &'static str,
    display: Option<&'static str>,
    mode: Option<&'static str>,
    overrides: Value,
}

impl Spec {
    fn new(id: impl Into<String>, kind: &'static str, fixture: &'static str, overrides: Value) -> Self {
        Self { id: id.into(), kind: Some(kind), fixture, display: None, mode: None, overrides }
    }

    fn display(self, display: &'static str) -> Self {
        Self { display: Some(display), ..self }
    }

    fn mode(self, mode: &'static str) -> Self {
        Self { mode: Some(mode), ..self }
    }
}

fn fill_gradient(mut variant: Value) -> Value {
    variant["track_filled_gradient"] = json!(true);
    variant["track_filled_gradient_stops"] = json!([
        {"offset": 0, "color": "#40e0d0"},
        {"offset": 0.5, "color": "#f5d742"},
        {"offset": 1, "color": "#c65102"},
    ]);
    variant
}

fn specs(image: &str) -> Vec<Spec> {
    let mut specs = vec![
        Spec { id: "empty".into(), kind: None, fixture: "ride", display: None, mode: None, overrides: json!({}) },
        Spec::new("course", "course", "ride", json!({"width": 560, "height": 258, "show_full_activity": true})),
        Spec::new("elevation", "elevation", "ride", json!({"width": 618, "height": 163, "show_full_activity": true})),
        Spec::new("diving_profile", "diving_profile", "dive", json!({"width": 618, "height": 163, "show_full_activity": true})),
        Spec::new("rolling-chart", "rolling_chart", "ride", json!({})),
    ];
    // The defaults, then the same chart with its area fill switched off, then flat.
    let colors = ["#ffffff", "#e0103a", "#30c4d1", "#f5b700", "#7ed957"];
    let metrics = ["speed", "heartrate", "cadence", "power", "temperature"];
    for (suffix, fill) in [("", json!({})), ("-no-area", json!({"area_fill": false})), ("-flat-area", json!({"area_fill": true, "area_gradient": false}))] {
        for (name, lines) in [("1-line", 1), ("5-lines", 5)] {
            let series: Vec<Value> = metrics.iter().zip(colors).take(lines).map(|(metric, color)| json!({"metric": metric, "color": color})).collect();
            let mut overrides = fill.clone();
            overrides["series"] = json!(series);
            specs.push(Spec::new(format!("rolling-chart-{name}{suffix}"), "rolling_chart", "ride", overrides));
        }
    }
    specs.extend([
        Spec::new(
            "rolling-chart-area-gradient",
            "rolling_chart",
            "ride",
            json!({"width": 580, "height": 110, "window_seconds": 30, "area_fill": true, "area_gradient": true}),
        ),
        Spec::new("gradient", "gradient", "ride", json!({"font_size": 45, "triangle_width": 36})),
        Spec::new("time-daytime", "time", "ride", json!({"font_size": 45, "time_mode": "daytime"})),
        Spec::new("time-elapsed", "time", "ride", json!({"font_size": 45, "time_mode": "elapsed", "elapsed_origin": "activity"})),
        Spec::new("label", "label", "ride", json!({"font_size": 45, "text": "OVRLEY / AFTERNOON RIDE"})),
        Spec::new("backdrop-rectangle", "backdrop", "ride", json!({"display_variants": {"rectangle": {"width": 410, "height": 53}}})).display("rectangle"),
        Spec::new("backdrop-circle", "backdrop", "ride", json!({"display_variants": {"circle": {"diameter": 232}}})).display("circle"),
        Spec::new("raster", "raster", "ride", json!({"path": image, "width": 256, "height": 256})),
        Spec::new("speed-text", "speed", "ride", json!({"font_size": 45, "show_units": true, "display_unit": "kmh"})).display("text"),
        Spec::new("heading-tape", "heading", "ride", json!({"display_variants": {"heading_tape": {"width": 600, "height": 100}}})).display("heading_tape"),
    ]);
    for mode in ["indicator", "compass"] {
        let overrides = json!({"display_variants": {"heading_compass": {"diameter": 200, "rotation_mode": mode}}});
        specs.push(Spec::new(format!("heading-compass-{mode}"), "heading", "ride", overrides).display("heading_compass"));
    }
    for style in ["line", "arrow"] {
        let overrides = json!({"display_variants": {"heart_zones": {"width": 420, "height": 36, "indicator_style": style}}});
        specs.push(Spec::new(format!("heart-zones-{style}"), "heartrate", "ride", overrides).display("heart_zones"));
    }
    for (side, vertical_speed) in [("left", true), ("right", false)] {
        let overrides = json!({"display_variants": {"altitude_tape": {"width": 110, "height": 420, "tape_orientation": side, "show_vertical_speed": vertical_speed}}});
        specs.push(Spec::new(format!("altitude-tape-{side}"), "altitude", "flight", overrides).display("altitude_tape"));
    }
    let lean = json!({"diameter": 232, "track_thickness": 65});
    for (id, variant) in [("lean-angle", lean.clone()), ("lean-angle-gradient", fill_gradient(lean))] {
        let overrides = json!({"font_size": 45, "display_variants": {"lean_angle": variant}});
        specs.push(Spec::new(id, "lean_angle", "racechrono", overrides).display("lean_angle"));
    }
    specs.push(
        Spec::new(
            "g-force",
            "g_force",
            "racebox",
            json!({"display_variants": {"g_force": {"diameter": 232, "width": 256, "height": 256, "marker_size": 12, "label_font_size": 35}}}),
        )
        .display("g_force"),
    );
    let speed = |id: String, display: &'static str, variant: Value| {
        let overrides = json!({"show_units": true, "display_unit": "kmh", "display_variants": {display: variant}});
        Spec::new(id, "speed", "ride", overrides).display(display)
    };
    for (display, gradient) in [("linear", false), ("linear", true), ("arc", false), ("arc", true), ("corner", false), ("gt_gauge", false)] {
        let styles: &[&str] = if display == "gt_gauge" { &["fill"] } else { &["fill", "bars"] };
        for style in styles {
            let mut variant = json!({"width": 232, "height": if display == "linear" { 40 } else { 232 }, "track_fill_style": style});
            if *style == "bars" {
                variant["bar_count"] = json!(20);
                variant["bar_gap"] = json!(3);
            }
            let suffix = if gradient { "-gradient" } else { "" };
            let variant = if gradient { fill_gradient(variant) } else { variant };
            specs.push(speed(format!("speed-{display}-{style}{suffix}"), display, variant));
        }
    }
    let flat = fill_gradient(json!({"width": 232, "height": 40, "track_fill_style": "fill", "track_fill_flat": true}));
    specs.push(speed("speed-linear-fill-gradient-flat".into(), "linear", flat));
    for mode in ["current_lap", "best_lap", "delta", "lap_log"] {
        specs.push(Spec::new(format!("lap-{mode}"), "lap_timer", "racebox", json!({})).display("lap_timer").mode(mode));
    }
    specs.push(Spec::new("splits-run", "splits_run", "splits", json!({})).display("splits_run"));
    for size in [500, 1000] {
        for pitch in [0, 40, 60] {
            specs.push(Spec::new(format!("map-p{pitch}-{size}"), "map", "ride", json!({"size": size, "pitch": pitch, "follow_zoom": 17})));
        }
    }
    specs
}

/// Prints every case's id.
pub fn list() {
    for spec in specs("") {
        println!("{}", spec.id);
    }
}

/// The telemetry a case needs inside its playback window.
fn required_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "speed" => &["speed"],
        "heading" => &["heading"],
        "lean_angle" => &["lean_angle"],
        "g_force" => &["g_force_x", "g_force_y"],
        "course" | "map" => &["sample_course_points"],
        "elevation" => &["sample_elevations"],
        "diving_profile" => &["depth", "first_stop_depth"],
        "rolling_chart" => &["speed", "heartrate"],
        "gradient" => &["gradient"],
        "lap_timer" => &["lap_time_seconds", "delta_to_best_lap_seconds"],
        "splits_run" => &["distance"],
        _ => &[],
    }
}

fn load_activity(root: &Path, name: &str) -> Result<Value, String> {
    let path = root.join("debug/activities").join(name);
    let bytes = std::fs::read(&path).map_err(|error| format!("fixture {name} is not readable in debug/activities: {error}"))?;
    let mut document: Value = serde_json::from_slice(&bytes).map_err(|error| format!("{name}: {error}"))?;
    let activity = document["parsed_activity"].take();
    if activity["sample_elapsed_seconds"].as_array().is_none_or(|seconds| seconds.len() < 2) {
        return Err(format!("{name}: missing parsed activity timeline"));
    }
    Ok(activity)
}

fn course_points(activity: &Value) -> Result<Vec<(Option<f64>, Option<f64>)>, String> {
    match activity.get("sample_course_points") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(points) => serde_json::from_value(points.clone()).map_err(|error| format!("sample_course_points: {error}")),
    }
}

/// Writes `<directory>/plan.json` and the activities it names, for the cases `only` selects (with
/// the empty scene), or all of them. No rendering.
pub fn write_plan(directory: &Path, root: &Path, frames: usize, warmup: usize, only: Option<&str>) -> Result<(), String> {
    let image = root.join("assets/icons/128x128@2x.png").display().to_string();
    let selected: Vec<Spec> = specs(&image).into_iter().filter(|spec| only.is_none_or(|only| spec.id == "empty" || spec.id == only)).collect();
    if let Some(only) = only.filter(|only| !selected.iter().any(|spec| spec.id == *only)) {
        return Err(format!("Unknown case: {only}"));
    }
    let (template_scene, globals) = editor::hud_template(root)?;

    let mut activities = BTreeMap::new();
    let mut activity_paths = Map::new();
    let mut cases = Vec::new();
    let mut fonts = Vec::new();
    let mut seen_fonts: HashSet<FontKey> = HashSet::new();
    for spec in selected {
        let (_, name, start) = *FIXTURES.iter().find(|(id, _, _)| *id == spec.fixture).expect("a case names a fixture");
        if !activities.contains_key(spec.fixture) {
            let activity = load_activity(root, name).map_err(|error| format!("{}: {error}", spec.id))?;
            let path = directory.join(format!("{}.json", spec.fixture));
            std::fs::write(&path, activity.to_string()).map_err(|error| format!("{}: {error}", path.display()))?;
            activity_paths.insert(spec.fixture.into(), json!(path));
            activities.insert(spec.fixture, activity);
        }
        let activity = &activities[spec.fixture];
        let duration = activity["trim_end_seconds"].as_f64().ok_or_else(|| format!("{name}: no trim_end_seconds"))?;
        let start = f64::from(start);
        let end = start + (warmup + frames) as f64 / f64::from(FPS);
        if end > duration {
            return Err(format!("{}: playback exceeds activity duration", spec.id));
        }
        for field in required_fields(spec.kind.unwrap_or_default()) {
            let (values, seconds) = (activity[*field].as_array(), activity["sample_elapsed_seconds"].as_array());
            let present = values.zip(seconds).is_some_and(|(values, seconds)| {
                values.iter().zip(seconds).any(|(value, second)| !value.is_null() && second.as_f64().is_some_and(|second| second >= start && second < end))
            });
            if !present {
                return Err(format!("{}: no {field} telemetry in playback window", spec.id));
            }
        }

        let mut scene = template_scene.clone();
        for (key, value) in [
            ("fps", json!(FPS)),
            ("updateRate", json!(1)),
            ("start", number(start)),
            ("end", number(end)),
            ("custom_export_range_active", json!(true)),
            ("preview_duration_seconds", number(duration)),
            ("export_start_seconds", number(start)),
        ] {
            scene.insert(key.into(), value);
        }
        let mut config = OverlayConfig { scene, ..OverlayConfig::default() };
        let mut placed = None;
        if let Some(kind) = spec.kind {
            let definition = catalog().require_widget(kind)?;
            let selection = WidgetSelection {
                kind: kind.into(),
                display_type: spec.display.map(str::to_string),
                mode: spec.mode.map(str::to_string),
                ..WidgetSelection::default()
            };
            let options = WidgetCreationOptions { course_points: course_points(activity)?, scene_font_size: Some(19.5), ..Default::default() };
            let defaults = create_widget_defaults(&selection, &globals, &options).map_err(|error| format!("{}: {error}", spec.id))?;
            let mut overrides = spec.overrides.as_object().cloned().expect("overrides are an object");
            overrides.extend([("id".to_string(), json!(spec.id)), ("x".to_string(), json!(200)), ("y".to_string(), json!(200))]);
            let collection = Collection::from_name(&definition.collection).expect("the catalog names its collections");
            config.collection_mut(collection).push(merge_widget_data(Some(&defaults), &overrides));
            placed = Some((kind, collection, definition.time_dependent));
        }
        let mut effective = editor_effective_config(&config, &globals, DEFAULT_LOCALE, &editor::theme());
        let mut request = Value::Null;
        if let Some((kind, collection, _)) = placed {
            if kind == "map" {
                // A map case is named by its output pixels along each edge, in export and preview alike.
                let scale = effective.scene["scale"].as_f64().ok_or("the scene has no scale")?;
                let edge = number(spec.overrides["size"].as_f64().expect("a map case has a size") / scale);
                let map = &mut effective.collection_mut(collection)[0];
                map.insert("width".into(), edge.clone());
                map.insert("height".into(), edge);
            }
            let mut widget = resolve_active_widget_data(&effective.collection(collection)[0]);
            if kind == "raster" {
                widget.insert("resourceId".into(), json!("benchmark-image"));
            }
            for font in widget_fonts(kind, &widget) {
                if seen_fonts.insert(font.clone()) {
                    fonts.push(font.request());
                }
            }
            request = json!({"type": kind, "widget": widget, "scene": effective.scene});
        }
        cases.push(json!({
            "id": spec.id,
            "activity": spec.fixture,
            "start_frame": start as usize * FPS as usize,
            "dynamic": placed.is_some_and(|(_, _, time_dependent)| time_dependent),
            "config": effective.to_value(),
            "request": request,
            // The resource step derives the table core would hand the editor for this split length.
            "split_table": (spec.kind == Some("splits_run")).then(|| format!("split-table-{}.json", spec.id)),
            // The map renderer draws under the widget: the preview times the widget drawn over it.
            "map_layer": spec.kind == Some("map"),
        }));
    }
    let plan = json!({"root": root, "frames": frames, "warmup": warmup, "fps": FPS, "activities": activity_paths, "cases": cases, "fonts": fonts});
    let path = directory.join("plan.json");
    std::fs::write(&path, plan.to_string()).map_err(|error| format!("{}: {error}", path.display()))
}
