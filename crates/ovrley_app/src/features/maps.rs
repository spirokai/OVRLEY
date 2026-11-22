//! Maps (phase 5 D9, D10, D11): the installed styles, the preview's frame thread, the GPU alert,
//! the map view mode, the background tile download and the tile cache row.
//!
//! - **Styles** are read at startup with the map services; [`initialize`] copies their catalog into
//!   the store for the style select (`useMapStyleSelect`) and probes the adapter on a worker.
//! - **Frames** come from one map thread, started with the preview's first map ([`frame_factory`]).
//!   Its wake-ups mark the preview dirty, so the next frame collects what arrived.
//! - **The tile cache row** (`useMapTileCache`) lives here too, with the rest of the maps' services.

use crate::features::overlay_editor::interaction::{hit_widget, map_view_patch, stage_to_scene, PointerModifiers};
use crate::features::overlay_editor::WheelInput;
use crate::features::widget_editor::blocks::OptionView;
use crate::i18n::{catalog as translations, Locale};
use crate::store::services::{MapAdapterSupport, MapFrameFactory, MapStyleCatalog, Services};
use crate::store::Store;
use ovrley_widgets::model::document::merge_widget_data;
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A wheel's zoom commits once the wheel has been still this long (`WHEEL_COMMIT_DELAY_MS`).
pub const WHEEL_COMMIT_DELAY: Duration = Duration::from_millis(300);

/// A resize or a run of slider edits only downloads the tiles it settles on (`SETTLE_MS`).
pub const DOWNLOAD_SETTLE: Duration = Duration::from_millis(500);

/// The map fields that decide which tiles a route needs (`TILE_FIELDS`). Colours and styles share
/// the same tiles.
const TILE_FIELDS: [&str; 10] = ["width", "height", "camera_mode", "orientation", "pitch", "follow_zoom", "fixed_zoom", "fixed_pan_x", "fixed_pan_y", "show_full_activity"];

/// The maps' hook state.
#[derive(Default)]
pub struct MapsState {
    /// The installed styles, once [`initialize`] has copied them.
    pub styles: Option<MapStyleCatalog>,
    /// Which adapter the map renderer would open, once the probe has answered.
    pub adapter: Option<MapAdapterSupport>,
    /// The preview's map frames, started with its first map.
    frames: Option<Arc<dyn MapFrameFactory>>,
    /// The map view mode's gesture (D10).
    pub view: MapViewState,
    /// The background route download (`createMapDownload`).
    download: MapDownloadState,
    /// Settings → Map tile cache (D11).
    pub tile_cache: MapTileCacheState,
}

/// `useMapTileCache`'s state.
#[derive(Debug, Default)]
pub struct MapTileCacheState {
    /// The cache's usage, once loaded.
    pub usage: Option<crate::store::services::CacheUsage>,
    /// The slider's value while it drags, and until the command it committed answers.
    pub draft_limit: Option<u64>,
    /// Whether a cache command is running; it disables Clear.
    pub pending: bool,
    /// The commands given while one runs, since the slider stays live meanwhile. They run one after
    /// another, so the newest is also the last to reach the cache and the settings file.
    queued: VecDeque<Option<u64>>,
}

impl MapTileCacheState {
    /// The slider's limit: the draft while dragging, else the cache's.
    pub fn limit(&self) -> Option<u64> {
        self.draft_limit.or(self.usage.map(|usage| usage.limit_bytes))
    }
}

/// What the background download last asked for, and the request waiting for the settle.
#[derive(Default)]
struct MapDownloadState {
    requested: String,
    waiting: Option<(Instant, crate::store::services::MapPrepareRequest)>,
    running: ovrley_services::maps::MapDownload,
}

impl std::fmt::Debug for MapDownloadState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("MapDownloadState").field("requested", &!self.requested.is_empty()).field("waiting", &self.waiting.as_ref().map(|(at, _)| *at)).finish()
    }
}

/// `useMapViewGesture`'s state: the map the mode is on, the uncommitted patch and a pan drag.
#[derive(Debug, Default)]
pub struct MapViewState {
    /// The map in view mode, as the last reconcile saw it, so its patch commits when it ends.
    active: Option<String>,
    pending: Option<PendingView>,
    /// A pan drag: the data it started from and its scene point.
    drag: Option<(Map<String, Value>, (f64, f64))>,
}

/// The uncommitted patch, with the data it applies to, since a wheel tick can arrive before the
/// draft renders; and when a wheel's zoom commits.
#[derive(Debug)]
struct PendingView {
    widget_id: String,
    data: Map<String, Value>,
    patch: Map<String, Value>,
    wheel_commit: Option<Instant>,
}

impl std::fmt::Debug for MapsState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MapsState")
            .field(
                "styles",
                &self.styles.as_ref().map(|styles| styles.styles().len()),
            )
            .field("adapter", &self.adapter)
            .field("frames", &self.frames.is_some())
            .finish()
    }
}

/// At startup: the style catalog for the style select, and the adapter probe on a worker (the GPU
/// alert reads it; until it answers, the alert stays hidden).
pub fn initialize(store: &mut Store, services: &mut Services) {
    store.maps.styles = Some(services.maps.styles().clone());
    let maps = services.maps.clone();
    services.workers.spawn("map-adapter-probe", move || {
        let adapter = maps.probe_adapter();
        Box::new(move |store, _| store.maps.adapter = Some(adapter))
    });
}

/// The preview's map frames, starting the map thread the first time. Its wake-ups mark the preview
/// dirty, so the next preview frame collects what arrived.
pub fn frame_factory(
    store: &mut Store,
    services: &mut Services,
) -> Result<Arc<dyn MapFrameFactory>, String> {
    if let Some(frames) = &store.maps.frames {
        return Ok(frames.clone());
    }
    let post = services.ui_post.clone();
    let frames = services.maps.frames(Arc::new(move || {
        post.post(Box::new(|store, _| store.preview.mark_dirty()))
    }))?;
    store.maps.frames = Some(frames.clone());
    Ok(frames)
}

/// `useMapStyleSelect`: the installed styles, and the one a map with `map_style` draws (a style that
/// is not installed draws the first). Until the styles load, there are none.
pub fn map_style_select(store: &Store, map_style: Option<&str>) -> (Vec<OptionView>, String) {
    let Some(styles) = &store.maps.styles else {
        return (Vec::new(), String::new());
    };
    let styles = styles.styles();
    let options = styles
        .iter()
        .map(|style| OptionView::new(style.id.as_str(), style.label.clone()))
        .collect();
    let value = styles
        .iter()
        .find(|style| Some(style.id.as_str()) == map_style)
        .unwrap_or(&styles[0])
        .id
        .as_str()
        .to_string();
    (options, value)
}

/// `mapBuildingsAvailable`: seen from straight above a building is only its roof, so buildings
/// need a pitched chase camera.
pub fn map_buildings_available(data: &Map<String, Value>) -> bool {
    data.get("camera_mode").and_then(Value::as_str) == Some("follow")
        && data
            .get("pitch")
            .and_then(Value::as_f64)
            .is_some_and(|pitch| pitch > 0.0)
}

/// The tile map editor's GPU warning: none on a hardware adapter or before the probe answered.
pub fn gpu_alert(store: &Store, locale: Locale) -> Option<String> {
    let (key, default) = match store.maps.adapter? {
        MapAdapterSupport::Hardware => return None,
        MapAdapterSupport::Software => (
            "widget-editor.mapWebgl2Software",
            "No GPU found. The map draws in software, so its preview and export may be slow.",
        ),
        MapAdapterSupport::Unavailable => (
            "widget-editor.mapWebgl2Unavailable",
            "WebGL2 isn't available on this system, so the map can't be previewed.",
        ),
    };
    Some(translations().t_or(locale, key, default).to_string())
}

// ── The map view mode (D10) ─────────────────────────────────────────────────────────────────────

/// The map in view mode: the single selection, when it is the map the mode was entered on.
pub fn map_view_widget(store: &Store) -> Option<&str> {
    let id = store.editor.map_view_widget_id.as_deref()?;
    (store.editor.selected_widget_ids.len() == 1 && store.editor.selected_widget_ids[0] == id).then_some(id)
}

/// Whether the effective widget `id` draws a top-down camera, which a drag pans.
fn can_pan(data: &Map<String, Value>) -> bool {
    data.get("camera_mode").and_then(Value::as_str) == Some("fixed")
}

fn effective_data(store: &Store, widget_id: &str) -> Option<Map<String, Value>> {
    let theme = store.theme.clone()?;
    crate::features::widget_preview::preview_widgets(store, &theme).into_iter().find(|widget| widget.id == widget_id).map(|widget| widget.data)
}

/// A double-click on the canvas enters the view mode of the map under the pointer.
pub fn double_click(store: &mut Store, stage_point: (f64, f64)) {
    let Some(widget_id) = hit_widget(store, stage_to_scene(store, stage_point)) else { return };
    let is_map = store.editor.config.find_widget(&widget_id).ok().and_then(|(collection, _)| store.editor.config.widget(&widget_id).ok().map(|data| collection.widget_type(data))).and_then(Result::ok).is_some_and(|kind| kind == "map");
    if is_map {
        store.editor.set_map_view_widget_id(Some(&widget_id));
    }
}

/// The data the next gesture applies to: the pending patch's, or the widget's.
fn current_data(store: &Store, widget_id: &str) -> Option<Map<String, Value>> {
    match &store.maps.view.pending {
        Some(pending) if pending.widget_id == widget_id => Some(pending.data.clone()),
        _ => effective_data(store, widget_id),
    }
}

/// Previews `gesture` on `data` as a draft, adding its patch to the pending one.
fn preview(store: &mut Store, widget_id: &str, data: Map<String, Value>, gesture: Value) -> Result<(), String> {
    let patch = map_view_patch(store, widget_id, Some(data.clone()), gesture)?;
    let pending = store.maps.view.pending.take().filter(|pending| pending.widget_id == widget_id);
    let (base, mut total, wheel_commit) = match pending {
        Some(pending) => (pending.data, pending.patch, pending.wheel_commit),
        None => (data, Map::new(), None),
    };
    total.extend(patch.clone());
    store.drafts.set_widget_draft(widget_id, &total);
    store.maps.view.pending = Some(PendingView { widget_id: widget_id.into(), data: merge_widget_data(Some(&base), &patch), patch: total, wheel_commit });
    Ok(())
}

/// Commits the pending patch as one undo step. A deleted map leaves nothing to commit to.
fn commit(store: &mut Store) {
    let Some(pending) = store.maps.view.pending.take() else { return };
    store.drafts.clear_widget_drafts(std::slice::from_ref(&pending.widget_id));
    if store.editor.config.widget(&pending.widget_id).is_ok() {
        if let Err(error) = store.editor.edit_config(|config| config.update_widgets(&[(pending.widget_id.clone(), pending.patch)])) {
            log::error!("The map view could not commit: {error}");
        }
    }
}

/// A press while a map is in view mode. On the map it starts a pan (top down only) and nothing
/// else: no move and no marquee. Returns whether the press was the map's.
pub fn pointer_down(store: &mut Store, stage_point: (f64, f64)) -> bool {
    let Some(widget_id) = map_view_widget(store).map(str::to_string) else { return false };
    let scene_point = stage_to_scene(store, stage_point);
    if hit_widget(store, scene_point).as_deref() != Some(widget_id.as_str()) {
        return false;
    }
    if let Some(pending) = &mut store.maps.view.pending {
        pending.wheel_commit = None;
    }
    if let Some(data) = current_data(store, &widget_id).filter(can_pan) {
        store.maps.view.drag = Some((data, scene_point));
    }
    true
}

/// A pointer move during a pan. Returns whether a pan is running.
pub fn pointer_move(store: &mut Store, stage_point: (f64, f64)) -> bool {
    let (Some(widget_id), Some((start, origin))) = (map_view_widget(store).map(str::to_string), store.maps.view.drag.clone()) else { return false };
    let point = stage_to_scene(store, stage_point);
    if let Err(error) = preview(store, &widget_id, start, json!({ "pan": [point.0 - origin.0, point.1 - origin.1] })) {
        log::error!("The map view could not pan: {error}");
    }
    true
}

/// The release ends a pan and commits it. Returns whether a pan ran.
pub fn pointer_up(store: &mut Store) -> bool {
    if store.maps.view.drag.take().is_none() {
        return false;
    }
    commit(store);
    true
}

/// An unmodified wheel over the map in view mode zooms at the pointer; modified wheels keep zooming
/// and scrolling the canvas. Returns whether the map took the wheel.
pub fn wheel(store: &mut Store, wheel: &WheelInput, modifiers: PointerModifiers, now: Instant) -> bool {
    if modifiers.ctrl || modifiers.meta || modifiers.alt || modifiers.shift || wheel.delta_y == 0.0 {
        return false;
    }
    let Some(widget_id) = map_view_widget(store).map(str::to_string) else { return false };
    let viewport = &store.overlay_editor.viewport;
    let point = stage_to_scene(store, (wheel.cursor.0 + viewport.scroll_x, wheel.cursor.1 + viewport.scroll_y));
    if hit_widget(store, point).as_deref() != Some(widget_id.as_str()) {
        return false;
    }
    let Some(data) = current_data(store, &widget_id) else { return false };
    match preview(store, &widget_id, data, json!({ "zoom": { "wheel_delta": wheel.delta_y, "point": [point.0, point.1] } })) {
        Ok(()) => {
            if let Some(pending) = &mut store.maps.view.pending {
                pending.wheel_commit = Some(now + WHEEL_COMMIT_DELAY);
            }
        }
        Err(error) => log::error!("The map view could not zoom: {error}"),
    }
    true
}

/// Escape leaves the view mode wherever focus is, unless a dialog or popup is open. Returns
/// whether it did.
pub fn escape(store: &mut Store) -> bool {
    if map_view_widget(store).is_none() || crate::features::overlay_editor::keyboard::overlay_open(store) {
        return false;
    }
    store.editor.set_map_view_widget_id(None);
    true
}

/// The badge's Reset button: the whole route back in view, as one undo step.
pub fn reset_map_view(store: &mut Store, widget_id: &str) -> Result<(), String> {
    let fit = map_view_patch(store, widget_id, None, Value::String("fit".into()))?;
    store.editor.edit_config(|config| config.update_widgets(&[(widget_id.to_string(), fit)]))
}

/// When the maps next need a turn of the event loop: a wheel's zoom commit, or the settled download.
pub fn next_deadline(store: &Store) -> Option<Instant> {
    let wheel = store.maps.view.pending.as_ref().and_then(|pending| pending.wheel_commit);
    let download = store.maps.download.waiting.as_ref().map(|(at, _)| *at);
    // Sync Doctor's maps: their animations, the settle and the frame throttle (Phase 8 D5).
    let sync = crate::features::video_sync::maps::next_deadline(store, Instant::now());
    wheel.into_iter().chain(download).chain(sync).min()
}

/// The deadline's turn: commits a wheel's zoom once the wheel has been still, and starts the
/// download once the maps' tile fields have.
pub fn tick(store: &mut Store, services: &mut Services, now: Instant) {
    if store.maps.view.pending.as_ref().and_then(|pending| pending.wheel_commit).is_some_and(|at| at <= now) {
        commit(store);
    }
    if store.maps.download.waiting.as_ref().is_some_and(|(at, _)| *at <= now) {
        let (_, request) = store.maps.download.waiting.take().expect("checked above");
        let cancelled = store.maps.download.running.restart();
        let maps = services.maps.clone();
        services.workers.spawn("map-route-download", move || {
            // Only a head start, since frames fetch their tiles on demand, so failures are logged only.
            if let Err(error) = maps.prepare_tiles(request, &cancelled) {
                log::warn!("Map route download: {error}");
            }
            Box::new(|_, _| {})
        });
    }
}

/// Keeps the background route download on what the preview's maps need (`mapDownload.update`):
/// a change of their tile fields, the scene, the density or the activity downloads after the
/// settle; no map or no activity cancels a running download.
pub fn update_download(store: &mut Store, maps: &[&Map<String, Value>], scene: &Value, raster_scale: f64, activity: Option<&Arc<Vec<u8>>>) {
    let download = &mut store.maps.download;
    let Some(activity) = activity.filter(|_| !maps.is_empty()) else {
        download.waiting = None;
        if !download.requested.is_empty() {
            download.running.restart();
            download.requested.clear();
        }
        return;
    };
    let fields: Vec<Vec<Value>> = maps.iter().map(|map| TILE_FIELDS.iter().map(|field| map.get(*field).cloned().unwrap_or(Value::Null)).collect()).collect();
    let wanted = json!([Arc::as_ptr(activity) as usize, scene, raster_scale, fields]).to_string();
    if wanted == download.requested {
        return;
    }
    let request = (|| -> Result<_, String> {
        Ok(crate::store::services::MapPrepareRequest {
            activity: activity.as_ref().clone(),
            widgets: maps.iter().map(|map| serde_json::from_value(Value::Object((*map).clone()))).collect::<Result<_, _>>().map_err(|error| error.to_string())?,
            scene: serde_json::from_value(scene.clone()).map_err(|error| error.to_string())?,
            raster_scale: raster_scale as f32,
        })
    })();
    download.requested = wanted;
    match request {
        Ok(request) => download.waiting = Some((Instant::now() + DOWNLOAD_SETTLE, request)),
        Err(error) => log::warn!("Map route download: {error}"),
    }
}

/// Reconcile step (D10): the mode ends when its map is gone or no longer the single selection, and
/// what it had not committed commits then.
pub fn reconcile_map_view(store: &mut Store) {
    let current = map_view_widget(store).filter(|id| store.editor.config.widget(id).is_ok()).map(str::to_string);
    if store.maps.view.active != current {
        store.maps.view.drag = None;
        commit(store);
        store.maps.view.active = current.clone();
    }
    if current.is_none() && store.editor.map_view_widget_id.is_some() {
        store.editor.set_map_view_widget_id(None);
    }
}

// ── Settings → Map tile cache (D11) ─────────────────────────────────────────────────────────────

/// The row showed: its usage loads on a worker. A failure goes to the error alert.
pub fn load_tile_cache(_: &mut Store, services: &mut Services) {
    let maps = services.maps.clone();
    services.workers.spawn("map-tile-cache-usage", move || {
        let usage = maps.tile_cache_usage();
        Box::new(move |store, _| match usage {
            Ok(usage) => store.maps.tile_cache.usage = Some(usage),
            Err(error) => store.media.set_error_message(error),
        })
    });
}

/// The slider moves.
pub fn draft_tile_cache_limit(store: &mut Store, limit: u64) {
    store.maps.tile_cache.draft_limit = Some(limit);
}

/// Runs a cache command on a worker, after the ones given before it. Only the newest command's
/// answer shows; a failure goes to the error alert. A limit that applied is saved.
fn run_tile_cache_command(store: &mut Store, services: &mut Services, limit: Option<u64>) {
    let cache = &mut store.maps.tile_cache;
    if cache.pending {
        cache.queued.push_back(limit);
        return;
    }
    cache.pending = true;
    start_tile_cache_command(services, limit);
}

fn start_tile_cache_command(services: &mut Services, limit: Option<u64>) {
    let maps = services.maps.clone();
    services.workers.spawn("map-tile-cache", move || {
        let result = match limit {
            Some(limit) => maps.set_tile_cache_limit(limit),
            None => maps.clear_tile_cache(),
        };
        Box::new(move |store, services| {
            if let (Ok(usage), Some(_)) = (&result, limit) {
                services.preferences.set(crate::store::services::MAP_TILE_CACHE_LIMIT_KEY, Value::from(usage.limit_bytes));
            }
            let next = store.maps.tile_cache.queued.pop_front();
            match result {
                Ok(usage) if next.is_none() => store.maps.tile_cache.usage = Some(usage),
                Ok(_) => {}
                Err(error) => store.media.set_error_message(error),
            }
            if let Some(next) = next {
                start_tile_cache_command(services, next);
            } else {
                let cache = &mut store.maps.tile_cache;
                cache.draft_limit = None;
                cache.pending = false;
            }
        })
    });
}

/// The slider is released at `limit`.
pub fn commit_tile_cache_limit(store: &mut Store, services: &mut Services, limit: u64) {
    run_tile_cache_command(store, services, Some(limit));
}

/// Clear cache.
pub fn clear_tile_cache(store: &mut Store, services: &mut Services) {
    run_tile_cache_command(store, services, None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::overlay_editor::interaction::{content_rect, scene_to_stage};
    use crate::features::overlay_editor::viewport::ViewportMetrics;
    use crate::features::widget_editor::blocks::CellKind;
    use crate::features::widget_editor::rules::Input;
    use crate::features::widget_editor::tests::{cell, editor_with, patch, widget};
    use crate::features::widget_editor::{edit_field, update_widget_data};
    use crate::store::services::fakes::Fakes;
    use crate::store::Dispatcher;
    use ovrley_core::normalize::template::template_manifest;
    use ovrley_widgets::model::catalog::catalog;
    use ovrley_widgets::model::document::{create_widget_defaults, WidgetCreationOptions, WidgetSelection};

    /// A tile map `m`, presented, with the editor's viewport measured and the map selected.
    fn map_editor() -> (Dispatcher, Fakes) {
        let selection = WidgetSelection { kind: "map".into(), ..WidgetSelection::default() };
        let mut data = create_widget_defaults(&selection, &template_manifest().globals, &WidgetCreationOptions::default()).unwrap();
        data.insert("id".into(), json!("m"));
        let collection = catalog().require_widget("map").unwrap().collection.clone();
        let (dispatcher, fakes) = editor_with(json!({ collection.as_str(): [data] }));
        // The first font load reads the system's fonts, which can outlast the editor's own wait.
        for _ in 0..2000 {
            if dispatcher.with_store(|store| store.preview.presented().contains_key("m")) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
            fakes.ui_post.drain(&dispatcher);
            dispatcher.dispatch(crate::features::widget_preview::preview_frame);
        }
        assert!(dispatcher.with_store(|store| store.preview.presented().contains_key("m")), "the map never rendered");
        dispatcher.dispatch(|store, _| {
            crate::features::overlay_editor::set_viewport_metrics(
                store,
                ViewportMetrics { editor_width: 1992.0, editor_height: 1152.0, scroll_width: 1992.0, scroll_height: 1104.0, padding: 16.0, scale_factor: 2.0, rem: 16.0 },
            );
            store.editor.set_widget_selection(&["m".to_string()], Some("m"));
        });
        (dispatcher, fakes)
    }

    /// Runs worker results until none are left.
    fn drain(dispatcher: &Dispatcher, fakes: &Fakes) {
        for _ in 0..2000 {
            std::thread::sleep(Duration::from_millis(5));
            if fakes.ui_post.posted.lock().unwrap().is_empty() {
                continue;
            }
            fakes.ui_post.drain(dispatcher);
            return;
        }
        panic!("no worker answered");
    }

    /// Runs `action` as a dispatch and returns its answer.
    fn mutate<R: 'static>(dispatcher: &Dispatcher, action: impl FnOnce(&mut Store) -> R + 'static) -> R {
        let answer = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot = answer.clone();
        dispatcher.dispatch(move |store, _| *slot.borrow_mut() = Some(action(store)));
        let answer = answer.borrow_mut().take().expect("the action ran");
        answer
    }

    fn edit(dispatcher: &Dispatcher, field: &'static str, input: Input) {
        dispatcher.dispatch(move |store, services| edit_field(store, services, "m", field, input, false).unwrap());
    }

    /// The map's centre in stage pixels.
    fn centre(dispatcher: &Dispatcher) -> (f64, f64) {
        dispatcher.with_store(|store| {
            let rect = content_rect(store, "m").unwrap();
            scene_to_stage(store, (rect.left + rect.width / 2.0, rect.top + rect.height / 2.0))
        })
    }

    #[test]
    fn the_map_editor_offers_the_installed_styles_and_warns_without_a_gpu() {
        let (dispatcher, fakes) = map_editor();
        assert!(matches!(cell(&dispatcher, "m", "map_style").unwrap().kind, CellKind::Select { ref options, .. } if options.is_empty()), "no styles before they load");
        *fakes.maps.adapter.lock().unwrap() = MapAdapterSupport::Software;
        dispatcher.dispatch(initialize);
        drain(&dispatcher, &fakes);
        let CellKind::Select { options, value, .. } = cell(&dispatcher, "m", "map_style").unwrap().kind else { panic!() };
        assert_eq!((options.len(), value.as_str()), (5, "liberty"));
        dispatcher.dispatch(|store, _| update_widget_data(store, "m", patch(json!({"map_style": "missing"}))).unwrap());
        assert!(matches!(cell(&dispatcher, "m", "map_style").unwrap().kind, CellKind::Select { ref value, .. } if value == "liberty"), "a missing style draws the first");
        assert!(matches!(cell(&dispatcher, "m", "gpu_alert").unwrap().kind, CellKind::Alert { ref text } if text.starts_with("No GPU found")));
        dispatcher.dispatch(|store, _| store.maps.adapter = Some(MapAdapterSupport::Hardware));
        assert!(cell(&dispatcher, "m", "gpu_alert").is_none());
    }

    #[test]
    fn a_circle_map_is_square_and_buildings_need_a_pitched_chase_camera() {
        let (dispatcher, _) = map_editor();
        dispatcher.dispatch(|store, _| update_widget_data(store, "m", patch(json!({"width": 300, "height": 200}))).unwrap());
        edit(&dispatcher, "shape", Input::Text("circle".into()));
        widget(&dispatcher, "m", |data| assert_eq!((data["shape"].clone(), data["width"].clone(), data["height"].clone()), (json!("circle"), json!(200), json!(200))));
        assert!(cell(&dispatcher, "m", "corner_radius").is_none(), "a circle has no corner radius");
        edit(&dispatcher, "width", Input::Text("4".into()));
        widget(&dispatcher, "m", |data| assert_eq!((data["width"].clone(), data["height"].clone()), (json!(8), json!(8)), "at least the smallest map edge"));

        assert!(!cell(&dispatcher, "m", "show_buildings").unwrap().disabled);
        dispatcher.dispatch(|store, _| update_widget_data(store, "m", patch(json!({"pitch": 0}))).unwrap());
        let buildings = cell(&dispatcher, "m", "show_buildings").unwrap();
        assert!(buildings.disabled && matches!(buildings.kind, CellKind::SwitchRow { checked: false, .. }), "seen from above a building is its roof");
        dispatcher.dispatch(|store, _| update_widget_data(store, "m", patch(json!({"show_labels": false}))).unwrap());
        assert!(cell(&dispatcher, "m", "local_names").unwrap().disabled);
    }

    #[test]
    fn switching_to_top_down_fits_the_route_as_one_undo_step() {
        let (dispatcher, _) = map_editor();
        dispatcher.dispatch(|store, _| update_widget_data(store, "m", patch(json!({"fixed_pan_x": 0.25, "fixed_pan_y": -0.1}))).unwrap());
        let before = dispatcher.with_store(|store| store.history.past.len());
        edit(&dispatcher, "camera_mode", Input::Text("fixed".into()));
        widget(&dispatcher, "m", |data| assert_eq!((data["camera_mode"].clone(), data["fixed_pan_x"].clone(), data["fixed_pan_y"].clone()), (json!("fixed"), json!(0), json!(0))));
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), before + 1);
    }

    #[test]
    fn the_view_mode_zooms_commits_once_and_ends_with_escape_or_the_selection() {
        let (dispatcher, _) = map_editor();
        let point = centre(&dispatcher);
        dispatcher.dispatch(move |store, _| double_click(store, point));
        assert_eq!(dispatcher.with_store(|store| map_view_widget(store).map(str::to_string)), Some("m".into()));

        let zoom = widget(&dispatcher, "m", |data| data["follow_zoom"].as_f64().unwrap());
        let before = dispatcher.with_store(|store| store.history.past.len());
        let start = Instant::now();
        let cursor = dispatcher.with_store(|store| (point.0 - store.overlay_editor.viewport.scroll_x, point.1 - store.overlay_editor.viewport.scroll_y));
        let took = mutate(&dispatcher, move |store| wheel(store, &WheelInput { delta_x: 0.0, delta_y: -100.0, cursor }, PointerModifiers::default(), start));
        assert!(took, "an unmodified wheel over the map zooms it");
        assert!(dispatcher.with_store(|store| store.drafts.widget_draft("m").is_some()), "the zoom previews as a draft");
        assert_eq!(widget(&dispatcher, "m", |data| data["follow_zoom"].as_f64().unwrap()), zoom);
        assert!(!mutate(&dispatcher, move |store| wheel(store, &WheelInput { delta_x: 0.0, delta_y: -100.0, cursor }, PointerModifiers { ctrl: true, ..PointerModifiers::default() }, start)), "a modified wheel stays the canvas's");
        dispatcher.dispatch(move |store, services| tick(store, services, start + WHEEL_COMMIT_DELAY));
        assert!(widget(&dispatcher, "m", |data| data["follow_zoom"].as_f64().unwrap()) > zoom);
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), before + 1, "one undo step");
        assert!(dispatcher.with_store(|store| store.drafts.widget_draft("m").is_none()));

        assert!(mutate(&dispatcher, escape));
        assert_eq!(dispatcher.with_store(|store| store.editor.map_view_widget_id.clone()), None);
        dispatcher.dispatch(move |store, _| double_click(store, point));
        dispatcher.dispatch(|store, _| store.editor.set_widget_selection(&[], None));
        assert_eq!(dispatcher.with_store(|store| store.editor.map_view_widget_id.clone()), None, "deselecting ends the mode");
    }

    #[test]
    fn the_route_download_waits_for_the_settle_and_no_map_cancels_it() {
        let (dispatcher, fakes) = map_editor();
        let activity = Arc::new(b"{}".to_vec());
        let request = {
            let activity = activity.clone();
            move |store: &mut Store, _: &mut Services| {
                let data = crate::features::widget_preview::preview_widgets(store, store.theme.as_ref().unwrap()).remove(0).data;
                let scene = store.preview.request_scene("m").unwrap().clone();
                update_download(store, &[&data], &scene, 1.0, Some(&activity));
            }
        };
        dispatcher.dispatch(request.clone());
        let deadline = dispatcher.with_store(next_deadline).expect("a download waits for the settle");
        dispatcher.dispatch(request);
        assert_eq!(dispatcher.with_store(next_deadline), Some(deadline), "an unchanged request keeps its timer");
        dispatcher.dispatch(move |store, services| tick(store, services, deadline));
        for _ in 0..2000 {
            if !fakes.maps.prepared.lock().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(*fakes.maps.prepared.lock().unwrap(), [1]);
        dispatcher.dispatch(|store, _| update_download(store, &[], &Value::Null, 1.0, None));
        assert_eq!(dispatcher.with_store(next_deadline), None);
    }

    #[test]
    fn the_tile_cache_row_applies_only_the_newest_command_and_saves_the_limit() {
        let (dispatcher, fakes) = map_editor();
        dispatcher.dispatch(load_tile_cache);
        drain(&dispatcher, &fakes);
        assert_eq!(dispatcher.with_store(|store| store.maps.tile_cache.limit()), Some(512 << 20));
        dispatcher.dispatch(|store, _| draft_tile_cache_limit(store, 1 << 30));
        assert_eq!(dispatcher.with_store(|store| store.maps.tile_cache.limit()), Some(1 << 30), "the slider drafts");
        dispatcher.dispatch(|store, services| commit_tile_cache_limit(store, services, 1 << 30));
        dispatcher.dispatch(|store, services| commit_tile_cache_limit(store, services, 768 << 20));
        assert!(dispatcher.with_store(|store| store.maps.tile_cache.pending));
        for _ in 0..2000 {
            fakes.ui_post.drain(&dispatcher);
            if !dispatcher.with_store(|store| store.maps.tile_cache.pending) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        dispatcher.with_store(|store| {
            assert_eq!(store.maps.tile_cache.limit(), Some(768 << 20), "only the newest command applies");
            assert_eq!(store.maps.tile_cache.draft_limit, None);
        });
        let saved = fakes.preferences.saves.borrow().last().and_then(|values| values.get(crate::store::services::MAP_TILE_CACHE_LIMIT_KEY).cloned());
        assert_eq!(saved, Some(json!(768u64 << 20)));
        dispatcher.dispatch(clear_tile_cache);
        drain(&dispatcher, &fakes);
        assert!(fakes.maps.calls.lock().unwrap().contains(&"clear".to_string()));
    }

    #[test]
    fn a_top_down_map_from_the_drawer_starts_fitted_to_the_route_in_one_undo_step() {
        let (dispatcher, fakes) = map_editor();
        let ride = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ovrley_core/tests/fixtures/activity/test-ride.fit").to_string_lossy().into_owned();
        dispatcher.dispatch(move |store, services| crate::features::app_shell::import_activity(store, services, ride, None));
        for _ in 0..2000 {
            fakes.ui_post.drain(&dispatcher);
            dispatcher.dispatch(crate::features::widget_preview::preview_frame);
            if dispatcher.with_store(|store| !store.media.is_processing && store.preview.presented().contains_key("m")) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let before = dispatcher.with_store(|store| store.history.past.len());
        let added = mutate(&dispatcher, |store| store.editor.config.widget_ids().len());
        let selection = WidgetSelection { kind: "map".into(), preset: patch(json!({"camera_mode": "fixed"})), ..WidgetSelection::default() };
        let id = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
        let slot = id.clone();
        dispatcher.dispatch(move |store, services| *slot.borrow_mut() = crate::features::widget_drawer::add_from_drawer(store, services, &selection).unwrap());
        let id = id.borrow().clone();
        dispatcher.with_store(|store| {
            assert_eq!(store.editor.config.widget_ids().len(), added + 1);
            let data = store.editor.config.widget(&id).unwrap();
            assert_ne!(data["fixed_zoom"], json!(14), "the zoom fits the route");
            assert_eq!(store.history.past.len(), before + 1, "one undo step");
        });
    }
}
