//! Group drags and resizes through the editor's draft actions and the native preview, without a
//! window. Each frame dispatches its pointer moves, each through the whole update loop, then
//! redraws the preview once, as the window schedules it. One process measures one case.

#[path = "../editor.rs"]
mod editor;

use ovrley_app::features::canvas::interaction::{
    content_rect, handles, pointer_down, pointer_move, pointer_up, scene_to_stage, Handle, PointerModifiers,
};
use ovrley_app::features::canvas::set_viewport_metrics;
use ovrley_app::features::canvas::viewport::ViewportMetrics;
use ovrley_app::features::history::HistoryMode;
use ovrley_app::features::widgets::preview::{preview_frame, Presented};
use ovrley_app::store::services::fakes::{fake_services, Fakes};
use ovrley_app::store::media::{ActivitySource, ImportedActivity};
use ovrley_app::store::{Dispatcher, Store};
use ovrley_widgets::model::catalog::catalog;
use ovrley_widgets::model::document::{create_widget_defaults, merge_widget_data, Collection, OverlayConfig, WidgetCreationOptions, WidgetSelection};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

const USAGE: &str =
    "Usage: benchmark_interactions <root> <output.json> <widgets> <frames> <warmup> <burst> text|mixed drag|resize";
const RIDE: &str = "crates/ovrley_core/tests/fixtures/activity/test-ride.fit";
const PLAYHEAD_SECOND: f64 = 60.0;

const MIXED: [(&str, Option<&str>); 15] = [
    ("speed", Some("text")),
    ("time", Some("text")),
    ("label", None),
    ("speed", Some("linear")),
    ("speed", Some("arc")),
    ("backdrop", Some("rectangle")),
    ("gradient", None),
    ("speed", Some("text")),
    ("heading", Some("heading_tape")),
    ("course", None),
    ("speed", Some("corner")),
    ("label", None),
    ("backdrop", Some("circle")),
    ("elevation", None),
    ("speed", Some("text")),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    Drag,
    Resize,
}

fn fixture(scene: Map<String, Value>, globals: &Map<String, Value>, count: usize, mixed: bool, imported: &ImportedActivity) -> Result<OverlayConfig, String> {
    let options = WidgetCreationOptions {
        course_points: imported.activity.sample_course_points.clone(),
        scene_font_size: Some(19.5),
        ..Default::default()
    };
    let mut config = OverlayConfig { scene, ..OverlayConfig::default() };
    for index in 0..count {
        let (kind, display) = if mixed { MIXED[index % MIXED.len()] } else { ("speed", Some("text")) };
        let selection = WidgetSelection { kind: kind.into(), display_type: display.map(str::to_string), ..Default::default() };
        let defaults = create_widget_defaults(&selection, globals, &options)?;
        let mut overrides = json!({"id": format!("profile-{index}"), "x": 200 + index % 5 * 600, "y": 200 + index / 5 * 500});
        match (kind, display) {
            ("label", _) => {
                overrides["text"] = json!(format!("OVRLEY {index}"));
                overrides["font_size"] = json!(45);
            }
            ("speed", Some("text")) => overrides["font_size"] = json!(45),
            ("course", _) => {
                overrides["width"] = json!(230);
                overrides["height"] = json!(140);
                overrides["show_full_activity"] = json!(true);
            }
            ("elevation", _) => {
                overrides["width"] = json!(250);
                overrides["height"] = json!(130);
                overrides["show_full_activity"] = json!(true);
            }
            _ => {}
        }
        let collection = Collection::from_name(&catalog().require_widget(kind)?.collection).expect("the catalog names its collections");
        config
            .collection_mut(collection)
            .push(merge_widget_data(Some(&defaults), overrides.as_object().expect("overrides are an object")));
    }
    Ok(config)
}

#[derive(Default)]
struct Measurement {
    update_ms: Vec<f64>,
    preview_ms: Vec<f64>,
    frame_ms: Vec<f64>,
    redraws: u64,
    redrawn_bytes: u64,
    first_presentation_ms: f64,
    release_ms: f64,
}

struct Editor {
    dispatcher: Dispatcher,
    fakes: Fakes,
    count: usize,
}

fn milliseconds(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

impl Editor {
    fn open(root: &Path, output: &Path, count: usize, mixed: bool) -> Result<Self, String> {
        let (scene, globals) = editor::hud_template(root)?;
        let (parts, fakes) = fake_services(json!({}));
        let path = root.join(RIDE).display().to_string();
        // The import's debug dump (debug builds) lands beside the results.
        let directory = output.parent().ok_or("the output has no folder")?.to_path_buf();
        let paths = ovrley_core::paths::AppPaths::from_runtime_roots(directory.clone(), directory.clone(), directory.join("runtime"));
        let response = ovrley_core::activity::import_activity_path(&paths, &path).map_err(|error| error.to_string())?;
        let imported = ImportedActivity::from_response(response)?;
        let config = fixture(scene, &globals, count, mixed, &imported)?;
        let dispatcher = Dispatcher::new(Store { theme: Some(editor::theme()), ..Store::default() }, parts);
        dispatcher.dispatch(move |store, _| {
            store.history.set_mode(HistoryMode::Reset);
            store.template.global_defaults = Rc::new(globals);
            ovrley_app::features::shell::activate_parsed_activity(store, ActivitySource::File { path }, imported);
            store.editor.set_config(config);
            store.editor.set_selected_second(PLAYHEAD_SECOND);
            // A 1600 × 1000 editor at density 1.
            set_viewport_metrics(
                store,
                ViewportMetrics {
                    editor_width: 1600.0,
                    editor_height: 1000.0,
                    scroll_width: 1600.0,
                    scroll_height: 1000.0,
                    padding: 16.0,
                    scale_factor: 1.0,
                    rem: 16.0,
                },
            );
            let ids = store.editor.config.widget_ids();
            store.editor.set_widget_selection(&ids, ids.last().map(String::as_str));
        });
        let editor = Self { dispatcher, fakes, count };
        editor.settle()?;
        Ok(editor)
    }

    /// Runs frames and worker results until every widget shows.
    fn settle(&self) -> Result<(), String> {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(30) {
            self.fakes.ui_post.drain(&self.dispatcher);
            self.frame();
            if let Some(error) = self.dispatcher.with_store(|store| store.preview.error.clone()) {
                return Err(error);
            }
            if self.presented().len() == self.count && self.fakes.ui_post.posted.lock().unwrap().is_empty() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Err("Timed out waiting for all widgets to render".into())
    }

    /// The preview's frame, if an action asked for one.
    fn frame(&self) {
        if self.dispatcher.with_store(|store| store.preview.is_dirty()) {
            self.dispatcher.dispatch(preview_frame);
        }
    }

    fn presented(&self) -> BTreeMap<String, Presented> {
        self.dispatcher.with_store(|store| store.preview.presented().clone())
    }

    fn move_to(&self, point: (f64, f64)) -> f64 {
        let started = Instant::now();
        self.dispatcher.dispatch(move |store, _| pointer_move(store, point, PointerModifiers::default()));
        milliseconds(started)
    }

    /// The press point, and the group's height over its width, on the stage.
    fn grip(&self, operation: Operation) -> Result<((f64, f64), f64), String> {
        self.dispatcher.with_store(|store| {
            let corner = |direction| {
                handles(store)
                    .into_iter()
                    .find(|(_, handle)| *handle == Handle::Resize { direction })
                    .map(|(point, _)| scene_to_stage(store, point))
                    .ok_or("the group shows no resize handles")
            };
            let (start, end) = (corner((-1.0, -1.0))?, corner((1.0, 1.0))?);
            let point = match operation {
                Operation::Resize => end,
                Operation::Drag => {
                    let first = content_rect(store, "profile-0").ok_or("the first widget is not presented")?;
                    scene_to_stage(store, (first.left + first.width / 2.0, first.top + first.height / 2.0))
                }
            };
            Ok((point, (end.1 - start.1) / (end.0 - start.0)))
        })
    }

    fn gesture(&self, operation: Operation, frames: usize, burst: usize) -> Result<Measurement, String> {
        let document = || self.dispatcher.with_store(|store| Rc::clone(&store.editor.config));
        let committed = document();
        let before = self.presented();
        let ((x, y), ratio) = self.grip(operation)?;
        self.dispatcher.dispatch(move |store, _| pointer_down(store, (x, y), PointerModifiers::default()));
        self.frame();

        let mut measured = Measurement::default();
        let started = Instant::now();
        self.move_to((x + 12.0, y + 8.0));
        self.frame();
        measured.first_presentation_ms = milliseconds(started);
        let first = self.presented();
        let changed = before.iter().filter(|(id, old)| first.get(*id).is_some_and(|new| new.geometry != old.geometry)).count();
        if changed != self.count {
            return Err(format!("the gesture presented {changed}/{} widgets; refusing to report an incomplete group", self.count));
        }

        let mut last = (x + 12.0, y + 8.0);
        let mut shown = first;
        for index in 0..frames {
            let mut frame_ms = 0.0;
            for event in 0..burst {
                let phase = (index * burst + event + 1) as f64 / (frames * burst) as f64 * std::f64::consts::TAU;
                let (dx, dy) = match operation {
                    Operation::Drag => (35.0 + phase.sin() * 70.0, 20.0 + (phase * 2.0).sin() * 40.0),
                    Operation::Resize => {
                        let dx = 20.0 + phase.sin() * 55.0;
                        (dx, dx * ratio)
                    }
                };
                last = (x + dx, y + dy);
                let update = self.move_to(last);
                measured.update_ms.push(update);
                frame_ms += update;
            }
            let started = Instant::now();
            self.frame();
            let preview = milliseconds(started);
            measured.preview_ms.push(preview);
            measured.frame_ms.push(frame_ms + preview);

            let next = self.presented();
            for (id, widget) in &next {
                if shown.get(id).is_some_and(|old| old.pixels_revision != widget.pixels_revision) {
                    measured.redraws += 1;
                    measured.redrawn_bytes += u64::from(widget.geometry.width) * u64::from(widget.geometry.height) * 4;
                }
            }
            shown = next;
        }
        if let Some(error) = self.dispatcher.with_store(|store| store.preview.error.clone()) {
            return Err(error);
        }
        if !Rc::ptr_eq(&committed, &document()) {
            return Err("the document changed before the release".into());
        }

        let started = Instant::now();
        self.dispatcher.dispatch(move |store, _| pointer_up(store, last, PointerModifiers::default()));
        self.frame();
        measured.release_ms = milliseconds(started);
        if Rc::ptr_eq(&committed, &document()) {
            return Err("the release did not commit the gesture".into());
        }
        self.settle()?;
        Ok(measured)
    }
}

fn main() -> Result<(), String> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [root, output, widgets, frames, warmup, burst, workload, operation] = arguments.as_slice() else {
        return Err(USAGE.into());
    };
    let integer = |name: &str, text: &str, least: usize| {
        text.parse::<usize>().ok().filter(|value| *value >= least).ok_or_else(|| format!("{name} must be an integer >= {least}"))
    };
    let (count, frames, warmup, burst) =
        (integer("widgets", widgets, 2)?, integer("frames", frames, 1)?, integer("warmup", warmup, 0)?, integer("burst", burst, 1)?);
    let mixed = match workload.as_str() {
        "text" => false,
        "mixed" => true,
        _ => return Err(USAGE.into()),
    };
    let gesture = match operation.as_str() {
        "drag" => Operation::Drag,
        "resize" => Operation::Resize,
        _ => return Err(USAGE.into()),
    };
    let output = PathBuf::from(output);
    let editor = Editor::open(Path::new(root), &output, count, mixed)?;
    if warmup > 0 {
        editor.gesture(gesture, warmup, burst)?;
    }
    let measured = editor.gesture(gesture, frames, burst)?;
    let result = json!({
        "id": format!("{workload}-{operation}"),
        "widgets": count,
        "inputs": frames * burst,
        "first_presentation_ms": measured.first_presentation_ms,
        "release_ms": measured.release_ms,
        "frame_ms": measured.frame_ms,
        "stages": {"update": measured.update_ms, "preview": measured.preview_ms},
        "counters": {"redraws": measured.redraws, "redrawn_bytes": measured.redrawn_bytes},
    });
    std::fs::write(&output, result.to_string()).map_err(|error| format!("{}: {error}", output.display()))
}
