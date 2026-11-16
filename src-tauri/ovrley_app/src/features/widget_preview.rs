//! The native preview host (D8, D9): the port of `previewSession.js`, `previewScene.js` and
//! `useWasmWidgets`, over `ovrley_widgets::preview::PreviewSession` directly instead of the WASM
//! ABI.
//!
//! - **Ownership:** the host is hook state in the store. It owns the session, one handle per widget
//!   with the last request applied to it, the resources (fonts, raster images, split tables, the
//!   activity), the raster scale and the disabled laps.
//! - **Frames:** reconcile step 3 marks the host dirty when one of its inputs changed; `ovrley_ui`
//!   then dispatches one [`preview_frame`] for the next event-loop turn, which syncs (create,
//!   update, destroy), renders at the playhead and records what each widget presents. A burst of
//!   actions is one frame.
//! - **Resources** load on workers and come back as actions. A widget is created only once its
//!   fonts are ready; a time-dependent widget waits for the activity; a raster waits for its image;
//!   a resource error is the canvas error. A document change destroys every widget, because ids
//!   repeat across documents.

use crate::features::overlay_editor::{raster_scale, scene_size};
use crate::lib::activity::{has_activity_coverage, resolve_activity_clip_id, resolve_activity_context, ActivityContext, ActivityTrim};
use crate::store::slices::media::ImportedActivity;
use crate::lib::template::{effective_widget_data, ThemeColors};
use crate::lib::utils::clamp;
use crate::lib::video_program::SecondsRange;
use crate::lib::widget::build_config_widgets;
use crate::store::services::Services;
use crate::store::Store;
use ovrley_core::normalize::template::normalize_global_defaults;
use ovrley_widgets::model::catalog::catalog;
use ovrley_widgets::model::document::number;
use ovrley_widgets::model::document::{
    merge_widget_data, resolve_active_widget_data, OverlayConfig,
};
use ovrley_widgets::preview::{FontRequest, PreviewSession};
use ovrley_widgets::preview_instance::PresentedGeometry;
use ovrley_widgets::render::preview::PreviewRasterScale;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

/// A font the preview needs: `widgetFonts`' request, as its identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontKey {
    pub font: String,
    /// `font_weight` and `italic` only where the widget styles its text (labels, zone labels).
    pub font_weight: Option<String>,
    pub italic: Option<bool>,
}

impl FontKey {
    fn plain(font: &Value) -> Self {
        Self {
            font: font.as_str().unwrap_or_default().to_string(),
            font_weight: None,
            italic: None,
        }
    }

    fn request(&self) -> Value {
        let mut request = Map::new();
        request.insert("font".into(), Value::String(self.font.clone()));
        if let Some(weight) = &self.font_weight {
            request.insert(
                "font_weight".into(),
                serde_json::from_str(weight).expect("a weight key is a JSON number"),
            );
        }
        if let Some(italic) = self.italic {
            request.insert("italic".into(), Value::Bool(italic));
        }
        Value::Object(request)
    }
}

/// A resource's state.
#[derive(Clone, Debug, PartialEq)]
enum Resource {
    Loading,
    Ready,
    Failed(String),
}

/// `widgetFonts`: the font resources a widget's effective data needs.
pub fn widget_fonts(kind: &str, data: &Map<String, Value>) -> Vec<FontKey> {
    let field = |name: &str| data.get(name).cloned().unwrap_or(Value::Null);
    let flag = |name: &str| data.get(name).and_then(Value::as_bool).unwrap_or(false);
    let point_label_font = || {
        data.get("point_label")
            .and_then(|label| label.get("font"))
            .cloned()
            .unwrap_or(Value::Null)
    };
    match kind {
        "backdrop" | "raster" | "course" => return Vec::new(),
        "map" => return vec![FontKey::plain(&field("font"))],
        "diving_profile" => {
            return if flag("show_depth_metric") || flag("show_depth_imperial") {
                vec![FontKey::plain(&point_label_font())]
            } else {
                Vec::new()
            }
        }
        "rolling_chart" => {
            return if flag("show_legend") {
                vec![FontKey::plain(&field("legend_font"))]
            } else {
                Vec::new()
            }
        }
        "elevation" => {
            return if flag("show_elevation_metric") || flag("show_elevation_imperial") {
                vec![FontKey::plain(&point_label_font())]
            } else {
                Vec::new()
            }
        }
        _ => {}
    }
    let display = data
        .get("display_type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if matches!(display, "linear" | "arc" | "corner" | "gt_gauge") {
        let mut fonts = if display == "linear" {
            Vec::new()
        } else {
            vec![FontKey::plain(&field("font"))]
        };
        if flag("show_min_max_labels") {
            fonts.push(FontKey::plain(&field("min_max_label_font")));
        }
        return fonts;
    }
    if display == "altitude_tape" {
        return vec![
            FontKey::plain(&field("font")),
            FontKey::plain(&field("label_font")),
        ];
    }
    if display == "heart_zones" {
        return if flag("show_label") {
            vec![FontKey {
                font_weight: data.get("label_font_weight").map(Value::to_string),
                ..FontKey::plain(&field("font"))
            }]
        } else {
            Vec::new()
        };
    }
    let font = if kind == "g_force" || display == "heading_tape" {
        field("label_font")
    } else {
        field("font")
    };
    let value_font = if kind == "label" {
        FontKey {
            font_weight: data.get("font_weight").map(Value::to_string),
            italic: data.get("italic").and_then(Value::as_bool),
            ..FontKey::plain(&font)
        }
    } else {
        FontKey::plain(&font)
    };
    if kind == "lap_timer" || kind == "splits_run" {
        vec![value_font, FontKey::plain(&field("label_font"))]
    } else {
        vec![value_font]
    }
}

/// The trimmed activity duration (always present natively), never negative.
fn activity_duration(activity: Option<&ovrley_core::activity::schema::ParsedActivity>) -> f64 {
    let duration = activity.map_or(0.0, |activity| activity.trim_end_seconds);
    if duration.is_finite() {
        duration.max(0.0)
    } else {
        0.0
    }
}

/// `resolvePreviewWindow`'s result: the activity window widgets read in the preview.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewWindow {
    pub export_start_second: f64,
    pub start_second: f64,
    pub end_second: f64,
    pub is_scoped: bool,
}

/// `resolvePreviewWindow`: the telemetry clip's own duration, else the interval covered by the
/// videos, else the activity trim, clamped to the activity. A range outside it scopes nothing.
pub fn resolve_preview_window(
    activity_duration_seconds: f64,
    activity_trim: Option<ActivityTrim>,
    activity_clip_duration_seconds: Option<f64>,
    video_bounds: Option<SecondsRange>,
) -> PreviewWindow {
    let range = match (activity_clip_duration_seconds, video_bounds) {
        (Some(duration), _) => Some(SecondsRange { start_second: 0.0, end_second: duration }),
        (None, Some(bounds)) => Some(bounds),
        (None, None) => activity_trim.map(|trim| SecondsRange { start_second: trim.start_second, end_second: trim.end_second }),
    };
    let whole = |export_start_second| PreviewWindow { export_start_second, start_second: 0.0, end_second: activity_duration_seconds, is_scoped: false };
    let Some(range) = range else { return whole(0.0) };
    let start_second = clamp(range.start_second, 0.0, activity_duration_seconds);
    let end_second = clamp(range.end_second, 0.0, activity_duration_seconds);
    if end_second <= start_second {
        return whole(range.start_second);
    }
    PreviewWindow { export_start_second: range.start_second, start_second, end_second, is_scoped: true }
}

/// What drives the preview's widgets at the playhead (`useActivityContext` and
/// `usePlayheadActivityCoverage` over the displayed timeline).
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewActivity {
    pub context: Option<ActivityContext>,
    /// The duration of the clip supplying embedded telemetry; `None` with an external activity.
    pub clip_duration_seconds: Option<f64>,
    /// Embedded telemetry carries no lap selection, as in the export.
    pub disabled_laps: Vec<usize>,
    /// Frames without activity coverage show raster widgets only, as in the export.
    pub coverage: bool,
}

impl PreviewActivity {
    pub fn activity(&self) -> Option<&ImportedActivity> {
        self.context.as_ref().and_then(|context| context.activity.as_ref())
    }

    /// The timeline second of activity second 0.
    pub fn origin_second(&self) -> f64 {
        self.context.as_ref().map_or(0.0, |context| context.origin_second)
    }
}

pub fn preview_activity(store: &Store) -> PreviewActivity {
    let displayed = &store.player.timeline.timeline().displayed;
    let second = store.editor.selected_second;
    let parsed = store.media.parsed_activity.as_ref();
    let clip = if parsed.is_none() {
        resolve_activity_clip_id(&displayed.program, second).and_then(|id| displayed.clips.iter().find(|clip| clip.id() == id))
    } else {
        None
    };
    let context = resolve_activity_context(parsed, store.media.activity_summary.as_ref(), clip);
    let from_clip = context.as_ref().is_some_and(|context| context.video_id.is_some());
    PreviewActivity {
        clip_duration_seconds: clip.filter(|_| from_clip).map(|clip| clip.duration_seconds()),
        disabled_laps: if from_clip { Vec::new() } else { store.media.disabled_laps.clone() },
        coverage: has_activity_coverage(parsed, store.media.activity_summary.as_ref(), &displayed.program, &displayed.clips, second),
        context,
    }
}

/// The preview window of the store: the telemetry clip, else the committed videos, else the trim.
pub fn store_preview_window(store: &Store, activity: &PreviewActivity) -> PreviewWindow {
    let duration = activity_duration(activity.activity().map(|imported| imported.activity.as_ref()));
    resolve_preview_window(duration, store.media.activity_trim, activity.clip_duration_seconds, store.player.timeline.timeline().committed.bounds)
}

/// `buildWasmScene`: the preview scene from the global defaults, the scene size, the preview
/// window and the fallback duration.
pub fn preview_scene(globals: &Map<String, Value>, scene: (f64, f64), window: PreviewWindow, fallback_duration: f64) -> Value {
    let global = |key: &str| globals.get(key).cloned().unwrap_or(Value::Null);
    json!({
        "width": number(scene.0),
        "height": number(scene.1),
        "border_color": global("border_color"),
        "border_thickness": global("border_thickness"),
        "shadow_color": global("shadow_color"),
        "shadow_strength": global("shadow_strength"),
        "shadow_distance": global("shadow_distance"),
        "fixed_width_digits": global("fixed_width_digits"),
        "scale": global("scale"),
        "opacity": global("opacity"),
        "export_start_seconds": window.export_start_second,
        "start": window.start_second,
        "end": window.end_second,
        "custom_export_range_active": window.is_scoped,
        "preview_duration_seconds": fallback_duration,
    })
}

/// One widget as the preview takes it: its type and its effective data with drafts and the
/// running gesture's patch applied.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewWidget {
    pub id: String,
    pub kind: String,
    pub data: Map<String, Value>,
}

fn time_dependent(kind: &str) -> bool {
    catalog()
        .widget(kind)
        .is_some_and(|definition| definition.time_dependent)
}

/// The effective widgets the preview shows, in document order.
pub fn preview_widgets(store: &Store, theme: &ThemeColors) -> Vec<PreviewWidget> {
    let globals = normalize_global_defaults(Some(&store.template.global_defaults));
    let language = store.i18n.language;
    build_config_widgets(&store.editor.config)
        .into_iter()
        .map(|widget| {
            let effective =
                effective_widget_data(widget.collection, &widget.data, &globals, language, theme);
            // `applyWidgetDrafts`, then the gesture: patches over the effective data, resolved again.
            let mut data = effective;
            for patch in [
                store.drafts.widget_draft(&widget.id),
                store.drafts.gesture_patch(&widget.id),
            ]
            .into_iter()
            .flatten()
            {
                data = resolve_active_widget_data(&merge_widget_data(Some(&data), patch));
            }
            PreviewWidget {
                id: widget.id,
                kind: widget.kind,
                data,
            }
        })
        .collect()
}

/// What one widget presents after a frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Presented {
    pub handle: u32,
    pub kind: String,
    pub revision: u32,
    pub pixels_revision: u32,
    pub geometry: PresentedGeometry,
}

struct Instance {
    handle: u32,
    kind: String,
    /// The request last applied, compared before updating (`instance.config`).
    request: Value,
    revision: Option<u32>,
}

/// The inputs a frame depends on, compared to mark the host dirty (D9).
#[derive(Clone, Debug, PartialEq)]
struct FrameInputs {
    config: Option<*const OverlayConfig>,
    global_defaults: Option<*const Map<String, Value>>,
    drafts_revision: u64,
    language: crate::i18n::Locale,
    raster_scale: f64,
    activity: Option<*const Vec<u8>>,
    disabled_laps: Vec<usize>,
    second: f64,
    document_revision: u64,
    fallback_duration: f64,
    window: PreviewWindow,
}

/// `createPreviewSession`'s state, natively.
pub struct PreviewHost {
    session: PreviewSession,
    document_revision: u64,
    instances: BTreeMap<String, Instance>,
    /// Font bytes by request, kept across document changes so a new session does not reload them.
    font_bytes: HashMap<FontKey, Arc<Vec<u8>>>,
    fonts: HashMap<FontKey, Resource>,
    images: HashMap<String, Resource>,
    split_tables: HashSet<String>,
    applied_raster_scale: Option<f64>,
    activity: Option<Arc<Vec<u8>>>,
    disabled_laps: Vec<usize>,
    presented: BTreeMap<String, Presented>,
    /// The canvas error: a resource or render failure.
    pub error: Option<String>,
    /// Whether activity covers the playhead; without it only rasters show (phase 4 D14).
    pub activity_coverage: bool,
    dirty: bool,
    seen: Option<FrameInputs>,
}

impl Default for PreviewHost {
    fn default() -> Self {
        Self {
            session: PreviewSession::default(),
            document_revision: 0,
            instances: BTreeMap::new(),
            font_bytes: HashMap::new(),
            fonts: HashMap::new(),
            images: HashMap::new(),
            split_tables: HashSet::new(),
            applied_raster_scale: None,
            activity: None,
            disabled_laps: Vec::new(),
            presented: BTreeMap::new(),
            error: None,
            activity_coverage: true,
            dirty: true,
            seen: None,
        }
    }
}

impl std::fmt::Debug for PreviewHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreviewHost")
            .field("widgets", &self.instances.len())
            .field("error", &self.error)
            .field("dirty", &self.dirty)
            .finish()
    }
}

impl PreviewHost {
    /// Whether a frame should run.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// What each widget presents, by id.
    pub fn presented(&self) -> &BTreeMap<String, Presented> {
        &self.presented
    }

    /// A presented widget's straight-alpha RGBA pixels (D10).
    pub fn pixels(&self, handle: u32) -> &[u8] {
        self.session.widgets[&handle].pixels()
    }

    /// `PreviewSession::resize_widget` for one widget: the Rust resize policy turns a size or factor
    /// and a fixed anchor into the widget's data patch. Profiles and maps measure their prepared
    /// vectors, so their request names their handle.
    pub fn resize_widget(&mut self, widget_id: &str, mut request: Map<String, Value>) -> Result<Map<String, Value>, String> {
        let kind = request.get("type").and_then(Value::as_str).unwrap_or_default().to_string();
        if matches!(kind.as_str(), "course" | "elevation" | "diving_profile" | "map") {
            let handle = self.instances.get(widget_id).map(|instance| instance.handle).ok_or_else(|| format!("{widget_id} has not rendered yet"))?;
            request.insert("handle".into(), Value::from(handle));
        }
        let request = serde_json::from_value(Value::Object(request)).map_err(|error| error.to_string())?;
        let update = self.session.resize_widget(request).map_err(|error| error.to_string())?;
        match serde_json::to_value(update).map_err(|error| error.to_string())? {
            Value::Object(patch) => Ok(patch),
            other => Err(format!("a resize answered {other}")),
        }
    }

    /// The scene of a widget's last request, which a resize starts from.
    pub fn request_scene(&self, widget_id: &str) -> Option<&Value> {
        self.instances.get(widget_id).map(|instance| &instance.request["scene"])
    }

    /// The editor never grid-fits text, so resizing scales it continuously.
    fn new_session(&mut self) {
        self.session = PreviewSession::default();
        self.instances.clear();
        self.presented.clear();
        self.applied_raster_scale = None;
        self.split_tables.clear();
        for (key, bytes) in &self.font_bytes {
            let request: FontRequest =
                serde_json::from_value(key.request()).expect("a stored font request parses");
            if let Err(error) = self.session.fonts.register(request, bytes) {
                self.fonts
                    .insert(key.clone(), Resource::Failed(error.to_string()));
            }
        }
        self.images.clear();
        self.activity = None;
    }
}

/// Reconcile step: marks the host dirty when an input of the frame changed (D9).
pub fn reconcile_widget_preview(store: &mut Store) {
    let activity = preview_activity(store);
    let inputs = FrameInputs {
        config: Some(Rc::as_ptr(&store.editor.config)),
        global_defaults: Some(Rc::as_ptr(&store.template.global_defaults)),
        drafts_revision: store.drafts.revision(),
        language: store.i18n.language,
        raster_scale: raster_scale(store),
        activity: activity.activity().map(|imported| Arc::as_ptr(&imported.preview_bytes)),
        disabled_laps: activity.disabled_laps.clone(),
        second: store.editor.selected_second - activity.origin_second(),
        document_revision: store.editor.editor_document_revision,
        fallback_duration: store.editor.fallback_duration_seconds,
        window: store_preview_window(store, &activity),
    };
    store.preview.activity_coverage = activity.coverage;
    if store.preview.seen.as_ref() != Some(&inputs) {
        store.preview.seen = Some(inputs);
        store.preview.dirty = true;
    }
}

fn load_font(store: &mut Store, services: &mut Services, key: FontKey) {
    if store.preview.fonts.contains_key(&key) {
        return;
    }
    store.preview.fonts.insert(key.clone(), Resource::Loading);
    let font_dirs = services.paths.font_dirs.clone();
    services.workers.spawn("preview-font", move || {
        let request: Result<FontRequest, String> =
            serde_json::from_value(key.request()).map_err(|error| error.to_string());
        let loaded = request.and_then(|request| {
            ovrley_core::fonts::preview_font_resource(
                &font_dirs,
                &request.font,
                request.font_weight,
                request.italic,
            )
            .map_err(|error| error.to_string())
        });
        Box::new(move |store: &mut Store, _: &mut Services| {
            let state = match loaded {
                Ok(bytes) => {
                    let request: FontRequest =
                        serde_json::from_value(key.request()).expect("checked on the worker");
                    match store.preview.session.fonts.register(request, &bytes) {
                        Ok(()) => {
                            store
                                .preview
                                .font_bytes
                                .insert(key.clone(), Arc::new(bytes));
                            Resource::Ready
                        }
                        Err(error) => Resource::Failed(error.to_string()),
                    }
                }
                Err(error) => Resource::Failed(error),
            };
            store.preview.fonts.insert(key, state);
            store.preview.dirty = true;
        })
    });
}

fn load_image(store: &mut Store, services: &mut Services, id: &str) {
    if store.preview.images.contains_key(id) {
        return;
    }
    use ovrley_core::raster::RasterResourceResolver;
    let state = match services.rasters.resolve(id) {
        Some(image) => match store
            .preview
            .session
            .images
            .register(id.to_string(), image.preview_png())
        {
            Ok(()) => Resource::Ready,
            Err(error) => Resource::Failed(error.to_string()),
        },
        None => Resource::Failed("Raster image resource is unavailable".into()),
    };
    store.preview.images.insert(id.to_string(), state);
}

fn load_split_tables(store: &mut Store, services: &mut Services, widgets: &[PreviewWidget], activity: &PreviewActivity) {
    let Some(imported) = activity.activity().cloned() else {
        return;
    };
    for widget in widgets.iter().filter(|widget| widget.kind == "splits_run") {
        let length = json!({ "distance": widget.data.get("split_distance"), "unit": widget.data.get("split_unit") });
        let key = length.to_string();
        if !store.preview.split_tables.insert(key) {
            continue;
        }
        let activity = Arc::clone(&imported.activity);
        let bytes_identity = Arc::clone(&imported.preview_bytes);
        services.workers.spawn("preview-splits", move || {
            let table = serde_json::from_value::<
                ovrley_widgets::widgets::splits_run::table::SplitLength,
            >(length)
            .map_err(|error| error.to_string())
            .and_then(|length| {
                serde_json::to_vec(&ovrley_core::activity::splits::derive_split_table(
                    &activity, length,
                ))
                .map_err(|error| error.to_string())
            });
            Box::new(move |store: &mut Store, _: &mut Services| {
                let current = store
                    .preview
                    .activity
                    .as_ref()
                    .is_some_and(|bytes| Arc::ptr_eq(bytes, &bytes_identity));
                if !current {
                    return;
                }
                match table.and_then(|bytes| {
                    store
                        .preview
                        .session
                        .replace_split_table(&bytes)
                        .map_err(|error| error.to_string())
                }) {
                    Ok(()) => {
                        for instance in store
                            .preview
                            .instances
                            .values_mut()
                            .filter(|instance| instance.kind == "splits_run")
                        {
                            instance.revision = None;
                        }
                    }
                    Err(error) => store.preview.error = Some(error),
                }
                store.preview.dirty = true;
            })
        });
    }
}

/// One preview frame: sync, render at the playhead and record what each widget presents. Errors
/// become the canvas error.
pub fn preview_frame(store: &mut Store, services: &mut Services) {
    store.preview.dirty = false;
    let theme = store
        .theme
        .clone()
        .expect("ovrley_ui reads the theme before the first dispatch");
    let widgets = preview_widgets(store, &theme);
    if let Err(error) = sync(store, services, &widgets) {
        store.preview.error = Some(error);
        return;
    }
    store.preview.error = None;
}

/// Runs the frame an action just made pending inside that action, for an answer that reads what
/// the widgets present (an agent's `widget_move`). The frame `ovrley_ui` schedules then finds the
/// host clean. A widget still waiting for a resource keeps its last presentation.
pub fn present_pending_frame(store: &mut Store, services: &mut Services) {
    reconcile_widget_preview(store);
    if store.preview.is_dirty() {
        preview_frame(store, services);
    }
}

fn sync(
    store: &mut Store,
    services: &mut Services,
    widgets: &[PreviewWidget],
) -> Result<(), String> {
    // A replaced document starts a fresh session: widget ids only identify widgets within one.
    if store.preview.document_revision != store.editor.editor_document_revision {
        store.preview.document_revision = store.editor.editor_document_revision;
        store.preview.new_session();
    }
    // The activity: its bytes replace the session's; time-dependent widgets present again.
    let preview_activity = preview_activity(store);
    let activity = preview_activity.activity().map(|imported| Arc::clone(&imported.preview_bytes));
    let activity_changed = match (&store.preview.activity, &activity) {
        (Some(current), Some(next)) => !Arc::ptr_eq(current, next),
        (None, None) => false,
        _ => true,
    };
    if activity_changed {
        store.preview.split_tables.clear();
        let bytes = activity.as_deref().map_or(&[][..], Vec::as_slice);
        store
            .preview
            .session
            .replace_activity(bytes)
            .map_err(|error| error.to_string())?;
        store.preview.activity = activity;
        for instance in store
            .preview
            .instances
            .values_mut()
            .filter(|instance| time_dependent(&instance.kind))
        {
            instance.revision = None;
        }
    }
    if store.preview.disabled_laps != preview_activity.disabled_laps {
        store.preview.disabled_laps = preview_activity.disabled_laps.clone();
        store
            .preview
            .session
            .select_laps(store.preview.disabled_laps.clone())
            .map_err(|error| error.to_string())?;
        for instance in store
            .preview
            .instances
            .values_mut()
            .filter(|instance| time_dependent(&instance.kind))
        {
            instance.revision = None;
        }
    }
    load_split_tables(store, services, widgets, &preview_activity);

    let globals = normalize_global_defaults(Some(&store.template.global_defaults));
    let default_font = FontKey::plain(globals.get("font_values").unwrap_or(&Value::Null));
    for widget in widgets {
        for font in widget_fonts(&widget.kind, &widget.data) {
            load_font(store, services, font);
        }
        if widget.kind == "raster" {
            if let Some(id) = widget.data.get("resourceId").and_then(Value::as_str) {
                load_image(store, services, id);
            }
        }
    }
    load_font(store, services, default_font.clone());
    if let Some(Resource::Failed(error)) = store.preview.fonts.get(&default_font) {
        return Err(error.clone());
    }

    // Widgets no longer in the document.
    let active: HashSet<&str> = widgets.iter().map(|widget| widget.id.as_str()).collect();
    let removed: Vec<String> = store
        .preview
        .instances
        .keys()
        .filter(|id| !active.contains(id.as_str()))
        .cloned()
        .collect();
    for id in removed {
        let instance = store.preview.instances.remove(&id).expect("listed above");
        store.preview.session.widgets.remove(&instance.handle);
        store.preview.presented.remove(&id);
    }

    let scale = raster_scale(store);
    if store.preview.applied_raster_scale != Some(scale) {
        let raster_scale =
            PreviewRasterScale::new(scale as f32).map_err(|error| error.to_string())?;
        store
            .preview
            .session
            .set_raster_scale(raster_scale)
            .map_err(|error| error.to_string())?;
        store.preview.applied_raster_scale = Some(scale);
    }

    let scene = preview_scene(&globals, scene_size(store), store_preview_window(store, &preview_activity), store.editor.fallback_duration_seconds);
    let activity_ready = store.preview.activity.is_some() || preview_activity.activity().is_none();
    for widget in widgets {
        // A raster waits for its image and hides meanwhile.
        if widget.kind == "raster" {
            if let Some(id) = widget.data.get("resourceId").and_then(Value::as_str) {
                match store.preview.images.get(id) {
                    Some(Resource::Failed(error)) => return Err(error.clone()),
                    Some(Resource::Ready) => {}
                    _ => {
                        if let Some(instance) = store.preview.instances.remove(&widget.id) {
                            store.preview.session.widgets.remove(&instance.handle);
                        }
                        store.preview.presented.remove(&widget.id);
                        continue;
                    }
                }
            }
        }
        let mut fonts_ready = true;
        for font in widget_fonts(&widget.kind, &widget.data) {
            match store.preview.fonts.get(&font) {
                Some(Resource::Failed(error)) => return Err(error.clone()),
                Some(Resource::Ready) => {}
                _ => fonts_ready = false,
            }
        }
        if !fonts_ready || (time_dependent(&widget.kind) && !activity_ready) {
            continue;
        }
        let transient = store.overlay_editor.is_transient(&widget.id, &widget.kind);
        let request = json!({ "type": widget.kind, "widget": widget.data, "scene": scene, "transient": transient });
        match store.preview.instances.get_mut(&widget.id) {
            Some(instance) if instance.request == request => {}
            Some(instance) => {
                let parsed =
                    serde_json::from_value(request.clone()).map_err(|error| error.to_string())?;
                store
                    .preview
                    .session
                    .update_widget(instance.handle, parsed)
                    .map_err(|error| error.to_string())?;
                instance.request = request;
            }
            None => {
                let parsed =
                    serde_json::from_value(request.clone()).map_err(|error| error.to_string())?;
                let handle = store
                    .preview
                    .session
                    .create_widget(parsed)
                    .map_err(|error| error.to_string())?;
                store.preview.instances.insert(
                    widget.id.clone(),
                    Instance {
                        handle,
                        kind: widget.kind.clone(),
                        request,
                        revision: None,
                    },
                );
            }
        }
    }

    if activity_ready && widgets.iter().any(|widget| time_dependent(&widget.kind)) {
        store
            .preview
            .session
            .render(store.editor.selected_second - preview_activity.origin_second())
            .map_err(|error| error.to_string())?;
    }

    let PreviewHost {
        session,
        instances,
        presented,
        ..
    } = &mut store.preview;
    for (id, instance) in instances.iter_mut() {
        let widget = &session.widgets[&instance.handle];
        let revision = widget.revision();
        if instance.revision == Some(revision) {
            continue;
        }
        instance.revision = Some(revision);
        match widget
            .presented_geometry()
            .map_err(|error| error.to_string())?
        {
            Some(geometry) => {
                presented.insert(
                    id.clone(),
                    Presented {
                        handle: instance.handle,
                        kind: instance.kind.clone(),
                        revision,
                        pixels_revision: widget.pixels_revision(),
                        geometry,
                    },
                );
            }
            None => {
                presented.remove(id);
            }
        }
    }
    Ok(())
}

/// `WIDGET_LAYERS`: backdrops, maps, rasters, then profiles, then everything else.
pub fn widget_layer(kind: &str) -> u8 {
    match kind {
        "backdrop" => 1,
        "map" => 2,
        "raster" => 3,
        "course" | "elevation" | "diving_profile" => 4,
        _ => 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::services::fakes::{fake_services, Fakes};
    use crate::store::Dispatcher;

    fn theme() -> ThemeColors {
        ThemeColors {
            ice: "#86aaaa".into(),
            aqua: "#40e0d0".into(),
        }
    }

    fn dispatcher() -> (Dispatcher, Fakes) {
        let (parts, fakes) = fake_services(json!({}));
        let store = Store { theme: Some(theme()), ..Store::default() };
        (Dispatcher::new(store, parts), fakes)
    }

    /// Runs frames and worker results until the host is idle.
    fn settle(dispatcher: &Dispatcher, fakes: &Fakes) {
        for _ in 0..400 {
            fakes.ui_post.drain(dispatcher);
            if dispatcher.with_store(|store| store.preview.is_dirty()) {
                dispatcher.dispatch(preview_frame);
                continue;
            }
            let pending = fakes.ui_post.posted.lock().unwrap().len();
            if pending == 0
                && dispatcher.with_store(|store| {
                    store
                        .preview
                        .fonts
                        .values()
                        .all(|font| *font != Resource::Loading)
                })
            {
                dispatcher.dispatch(preview_frame);
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the preview did not settle");
    }

    fn with_document(dispatcher: &Dispatcher, document: Value) {
        dispatcher.dispatch(move |store, _| {
            store
                .history
                .set_mode(crate::features::undo_redo::HistoryMode::Reset);
            store
                .editor
                .set_config(OverlayConfig::from_value(&document).unwrap());
        });
    }

    #[test]
    fn fonts_follow_the_widget_family() {
        let map = |value: Value| value.as_object().unwrap().clone();
        assert!(widget_fonts("backdrop", &map(json!({}))).is_empty());
        let fonts = widget_fonts(
            "lap_timer",
            &map(json!({"display_type": "lap_timer", "font": "A.ttf", "label_font": "B.ttf"})),
        );
        assert_eq!(
            fonts
                .iter()
                .map(|font| font.font.as_str())
                .collect::<Vec<_>>(),
            ["A.ttf", "B.ttf"]
        );
        let label = widget_fonts(
            "label",
            &map(json!({"font": "A.ttf", "font_weight": 537, "italic": true})),
        );
        assert_eq!(
            label[0].request(),
            json!({"font": "A.ttf", "font_weight": 537, "italic": true})
        );
        assert!(widget_fonts(
            "speed",
            &map(json!({"display_type": "linear", "show_min_max_labels": false}))
        )
        .is_empty());
    }

    #[test]
    fn the_scene_takes_the_globals_and_the_whole_activity_window() {
        let globals = normalize_global_defaults(None);
        let scene = preview_scene(&globals, (1920.0, 1080.0), resolve_preview_window(0.0, None, None, None), 73.0);
        assert_eq!(scene["width"], 1920);
        assert_eq!(scene["preview_duration_seconds"], 73.0);
        assert_eq!(scene["custom_export_range_active"], false);
        assert_eq!(scene["end"], 0.0);
        let scene = preview_scene(&globals, (1920.0, 1080.0), resolve_preview_window(90.0, None, None, None), 73.0);
        assert_eq!((scene["start"].as_f64(), scene["end"].as_f64()), (Some(0.0), Some(90.0)));
        let trim = Some(ActivityTrim { start_second: 10.0, end_second: 40.0 });
        let scene = preview_scene(&globals, (1920.0, 1080.0), resolve_preview_window(90.0, trim, None, None), 73.0);
        assert_eq!((scene["export_start_seconds"].as_f64(), scene["end"].as_f64(), scene["custom_export_range_active"].as_bool()), (Some(10.0), Some(40.0), Some(true)));
        assert_eq!(scene["scale"], globals["scale"]);
    }

    #[test]
    fn the_preview_window_prefers_the_telemetry_clip_then_the_videos_then_the_trim() {
        let trim = Some(ActivityTrim { start_second: 5.0, end_second: 20.0 });
        let bounds = Some(SecondsRange { start_second: -10.0, end_second: 30.0 });
        let window = |start_second, end_second| PreviewWindow { export_start_second: start_second, start_second, end_second, is_scoped: true };
        assert_eq!(resolve_preview_window(60.0, trim, Some(12.0), bounds), window(0.0, 12.0));
        assert_eq!(
            resolve_preview_window(60.0, trim, None, bounds),
            PreviewWindow { export_start_second: -10.0, start_second: 0.0, end_second: 30.0, is_scoped: true },
            "clamped to the activity, the export starts with the videos"
        );
        assert_eq!(resolve_preview_window(60.0, trim, None, None), window(5.0, 20.0));
        let outside = Some(SecondsRange { start_second: 70.0, end_second: 90.0 });
        assert_eq!(
            resolve_preview_window(60.0, None, None, outside),
            PreviewWindow { export_start_second: 70.0, start_second: 0.0, end_second: 60.0, is_scoped: false },
            "a range outside the activity scopes nothing"
        );
    }

    #[test]
    fn widgets_appear_once_their_fonts_load_and_leave_with_the_document() {
        let (dispatcher, fakes) = dispatcher();
        let mut backdrop = ovrley_widgets::model::document::create_widget_defaults(
            &ovrley_widgets::model::document::WidgetSelection { kind: "backdrop".into(), ..Default::default() },
            &ovrley_core::normalize::template::template_manifest().globals,
            &Default::default(),
        )
        .unwrap();
        backdrop.insert("id".into(), json!("box"));
        with_document(
            &dispatcher,
            json!({"scene": {"width": 1920, "height": 1080}, "backdrops": [backdrop],
                "labels": [{"id": "title", "text": "Ride", "x": 100, "y": 100, "font_size": 40, "font": "Inter.ttf", "font_weight": 400, "italic": false, "letter_spacing": 0}]}),
        );
        dispatcher.dispatch(preview_frame);
        assert!(
            dispatcher.with_store(|store| !store.preview.presented().contains_key("title")),
            "the label waits for its font"
        );
        settle(&dispatcher, &fakes);
        dispatcher.with_store(|store| {
            assert_eq!(store.preview.error, None);
            let title = &store.preview.presented()["title"];
            assert!(title.geometry.widget.content.width > 0.0);
            assert_eq!(
                store.preview.pixels(title.handle).len() as u32,
                title.geometry.widget.width * title.geometry.widget.height * 4
            );
            assert!(store.preview.presented().contains_key("box"));
        });
        with_document(
            &dispatcher,
            json!({"scene": {"width": 1920, "height": 1080}}),
        );
        settle(&dispatcher, &fakes);
        assert!(dispatcher.with_store(|store| store.preview.presented().is_empty()));
    }

    #[test]
    fn pixels_are_kept_until_their_revision_changes() {
        let (dispatcher, fakes) = dispatcher();
        with_document(
            &dispatcher,
            json!({"scene": {"width": 1920, "height": 1080}, "labels": [{"id": "t", "text": "A", "x": 0, "y": 0, "font_size": 40, "font": "Inter.ttf", "font_weight": 400, "italic": false, "letter_spacing": 0}]}),
        );
        settle(&dispatcher, &fakes);
        let first = dispatcher.with_store(|store| store.preview.presented()["t"].pixels_revision);
        dispatcher.dispatch(|store, _| {
            store
                .editor
                .edit_config(|config| config.labels[0].insert("x".into(), json!(40)));
        });
        settle(&dispatcher, &fakes);
        let moved = dispatcher.with_store(|store| store.preview.presented()["t"].clone());
        assert_eq!(
            moved.pixels_revision, first,
            "a whole-pixel move keeps the pixels"
        );
        dispatcher.dispatch(|store, _| {
            store
                .editor
                .edit_config(|config| config.labels[0].insert("text".into(), json!("B")));
        });
        settle(&dispatcher, &fakes);
        assert_ne!(
            dispatcher.with_store(|store| store.preview.presented()["t"].pixels_revision),
            first
        );
    }

    #[test]
    fn a_pending_frame_presents_within_the_action() {
        let (dispatcher, fakes) = dispatcher();
        with_document(
            &dispatcher,
            json!({"scene": {"width": 1920, "height": 1080}, "labels": [{"id": "t", "text": "A", "x": 0, "y": 0, "font_size": 40, "font": "Inter.ttf", "font_weight": 400, "italic": false, "letter_spacing": 0}]}),
        );
        settle(&dispatcher, &fakes);
        let before = dispatcher.with_store(|store| store.preview.presented()["t"].geometry.widget.content.left);
        let after = dispatcher
            .dispatch_with_result(|store, services| {
                store
                    .editor
                    .edit_config(|config| config.labels[0].insert("x".into(), json!(40)));
                present_pending_frame(store, services);
                store.preview.presented()["t"].geometry.widget.content.left
            })
            .unwrap();
        assert_eq!(after - before, 40.0);
        assert!(!dispatcher.with_store(|store| store.preview.is_dirty()), "the scheduled frame has nothing left to do");
    }

    #[test]
    fn an_unknown_font_is_the_canvas_error() {
        let (dispatcher, fakes) = dispatcher();
        with_document(
            &dispatcher,
            json!({"scene": {"width": 1920, "height": 1080}, "labels": [{"id": "t", "text": "A", "x": 0, "y": 0, "font": "Missing Font.ttf", "font_weight": 400, "italic": false, "letter_spacing": 0}]}),
        );
        settle(&dispatcher, &fakes);
        assert!(dispatcher
            .with_store(|store| store.preview.error.clone())
            .is_some());
    }

    #[test]
    fn an_action_marks_the_host_dirty_only_when_an_input_changed() {
        let (dispatcher, fakes) = dispatcher();
        settle(&dispatcher, &fakes);
        dispatcher.dispatch(|store, _| store.editor_shell.set_editor_grid_visible(true));
        assert!(
            !dispatcher.with_store(|store| store.preview.is_dirty()),
            "the grid is not an input"
        );
        dispatcher.dispatch(|store, _| store.editor.set_selected_second(3.0));
        assert!(
            dispatcher.with_store(|store| store.preview.is_dirty()),
            "the playhead is"
        );
    }

    #[test]
    fn layers_stack_as_export_does() {
        assert!(
            widget_layer("backdrop") < widget_layer("map")
                && widget_layer("raster") < widget_layer("course")
                && widget_layer("course") < widget_layer("speed")
        );
    }
}
