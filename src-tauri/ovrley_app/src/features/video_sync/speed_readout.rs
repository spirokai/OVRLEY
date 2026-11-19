//! The speed readout at the top left of Sync Doctor's video screen (Phase 8 D12): one speed
//! metric widget, as React builds it (`VIDEO_SYNC_SPEED_WIDGETS`), drawn by the widget engine's
//! own session. React draws it but hides it; the port shows it (Q4: B).
//!
//! It is a fixed widget outside the document, so it has its own session instead of the editor's
//! preview host, which renders the document: one font load, the activity, and a render at the
//! playhead. It reads the external activity with origin 0 and no disabled laps, as React does.

use crate::features::widget_preview::{widget_fonts, FontKey};
use crate::store::services::Services;
use crate::store::Store;
use ovrley_core::normalize::template::normalize_global_defaults;
use ovrley_widgets::model::catalog::catalog;
use ovrley_widgets::preview::{FontRequest, PreviewSession};
use ovrley_widgets::preview_instance::PresentedGeometry;
use ovrley_widgets::render::preview::PreviewRasterScale;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

/// `VIDEO_SYNC_SPEED_FONT`.
const SPEED_FONT: &str = "JetBrains Mono.ttf";
const WIDGET_ID: &str = "video-sync-speed";

/// A font's load.
#[derive(Clone, Debug, PartialEq)]
enum FontState {
    Loading,
    Ready,
    Failed(String),
}

/// What the readout shows: its pixels (straight-alpha RGBA) and where they sit on the screen.
pub struct SpeedFrame<'a> {
    pub pixels: &'a [u8],
    pub geometry: &'a PresentedGeometry,
    pub revision: u32,
}

/// The readout's session and inputs.
#[derive(Default)]
pub struct SpeedReadout {
    session: PreviewSession,
    fonts: BTreeMap<FontKey, FontState>,
    handle: Option<u32>,
    request: Option<Value>,
    activity: Option<Arc<Vec<u8>>>,
    raster_scale: Option<f64>,
    rendered: Option<(f64, u32)>,
    presented: Option<PresentedGeometry>,
    /// A load or render failure, shown in its place (React's `speedError`).
    pub error: Option<String>,
}

impl std::fmt::Debug for SpeedReadout {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SpeedReadout")
            .field("handle", &self.handle)
            .field("error", &self.error)
            .finish()
    }
}

/// The widget's data: the catalog's speed defaults with React's overrides, resolved as the editor
/// resolves a document widget.
fn widget_data(store: &Store) -> Map<String, Value> {
    let definition = catalog()
        .require_widget("speed")
        .expect("the catalog has the speed widget");
    let mut data = definition.defaults.clone();
    for (key, value) in [
        ("id", json!(WIDGET_ID)),
        ("value", json!("speed")),
        ("x", json!(0)),
        ("y", json!(0)),
        ("font", json!(SPEED_FONT)),
        ("font_size", json!(189)),
        ("decimals", json!(1)),
        ("show_icon", json!(false)),
    ] {
        data.insert(key.into(), value);
    }
    let globals = normalize_global_defaults(Some(&store.template.global_defaults));
    let theme = store
        .theme
        .as_ref()
        .expect("ovrley_ui reads the theme before the first dispatch");
    crate::lib::template::effective_widget_data(
        ovrley_widgets::model::document::Collection::from_name(&definition.collection).expect("the speed widget's collection"),
        &data,
        &globals,
        store.i18n.language,
        theme,
    )
}

/// `VIDEO_SYNC_SPEED_SCENE` at the scene's size and the display scale: opaque, no shadow, no border.
fn scene(store: &Store, scale: f64) -> Value {
    let (width, height) = crate::features::overlay_editor::scene_size(store);
    let globals = normalize_global_defaults(Some(&store.template.global_defaults));
    let duration = store
        .media
        .activity_summary
        .as_ref()
        .map_or(0.0, |summary| summary.duration_seconds);
    json!({
        "width": ovrley_widgets::model::document::number(width),
        "height": ovrley_widgets::model::document::number(height),
        "border_color": "#000000",
        "border_thickness": 0,
        "shadow_color": "#000000",
        "shadow_strength": 0,
        "shadow_distance": 0,
        "fixed_width_digits": globals.get("fixed_width_digits").cloned().unwrap_or(Value::Null),
        "scale": scale,
        "opacity": 1,
        "export_start_seconds": 0,
        "start": 0,
        "end": duration,
        "custom_export_range_active": false,
        "preview_duration_seconds": store.editor.fallback_duration_seconds,
    })
}

fn has_speed(store: &Store) -> bool {
    store
        .media
        .parsed_activity
        .as_ref()
        .is_some_and(|imported| imported.activity.speed.iter().any(Option::is_some))
}

fn load_font(store: &mut Store, services: &mut Services, key: FontKey) {
    let readout = store
        .video_sync
        .speed
        .as_mut()
        .expect("the readout is open");
    if readout.fonts.contains_key(&key) {
        return;
    }
    readout.fonts.insert(key.clone(), FontState::Loading);
    let font_dirs = services.paths.font_dirs.clone();
    services.workers.spawn("speed-readout-font", move || {
        let request = key.request();
        let loaded = serde_json::from_value::<FontRequest>(request.clone())
            .map_err(|error| error.to_string())
            .and_then(|parsed| {
                ovrley_core::fonts::preview_font_resource(
                    &font_dirs,
                    &parsed.font,
                    parsed.font_weight,
                    parsed.italic,
                )
                .map_err(|error| error.to_string())
            });
        Box::new(move |store: &mut Store, _: &mut Services| {
            // The mode ended meanwhile.
            let Some(readout) = store.video_sync.speed.as_mut() else {
                return;
            };
            let state = match loaded.and_then(|bytes| {
                let parsed: FontRequest =
                    serde_json::from_value(request).map_err(|error| error.to_string())?;
                readout
                    .session
                    .fonts
                    .register(parsed, &bytes)
                    .map_err(|error| error.to_string())
            }) {
                Ok(()) => FontState::Ready,
                Err(error) => FontState::Failed(error),
            };
            readout.fonts.insert(key, state);
        })
    });
}

/// The reconcile step's part (D12): opens the readout with the mode, loads its fonts, and renders
/// at the playhead when the playhead, the activity or the display scale changed.
pub fn reconcile_speed_readout(store: &mut Store, services: &mut Services) {
    if !crate::features::player::video_sync_mode(store) || !has_speed(store) {
        store.video_sync.speed = None;
        return;
    }
    store
        .video_sync
        .speed
        .get_or_insert_with(SpeedReadout::default);
    let data = widget_data(store);
    let keys: Vec<FontKey> = widget_fonts("speed", &data)
        .into_iter()
        .chain([FontKey::plain(&json!(SPEED_FONT))])
        .collect();
    for key in keys.iter().cloned() {
        load_font(store, services, key);
    }
    let scale = crate::features::overlay_editor::display_scale(store);
    let raster_scale = store.overlay_editor.viewport.metrics.scale_factor.max(1.0);
    let request = json!({ "type": "speed", "widget": data, "scene": scene(store, scale), "transient": false });
    let activity = store
        .media
        .parsed_activity
        .as_ref()
        .map(|imported| imported.preview_bytes.clone());
    let second = store.editor.selected_second;
    let readout = store.video_sync.speed.as_mut().expect("opened above");
    if let Err(error) = readout.sync(&keys, request, activity, raster_scale, second) {
        readout.error = Some(error);
    }
}

impl SpeedReadout {
    fn sync(
        &mut self,
        keys: &[FontKey],
        request: Value,
        activity: Option<Arc<Vec<u8>>>,
        raster_scale: f64,
        second: f64,
    ) -> Result<(), String> {
        for key in keys {
            match self.fonts.get(key) {
                Some(FontState::Ready) => {}
                Some(FontState::Failed(error)) => return Err(error.clone()),
                _ => return Ok(()),
            }
        }
        self.error = None;
        let activity_changed = match (&self.activity, &activity) {
            (Some(current), Some(next)) => !Arc::ptr_eq(current, next),
            (None, None) => false,
            _ => true,
        };
        if activity_changed {
            self.session
                .replace_activity(activity.as_deref().map_or(&[][..], Vec::as_slice))
                .map_err(|error| error.to_string())?;
            self.activity = activity;
            self.rendered = None;
        }
        if self.raster_scale != Some(raster_scale) {
            self.session
                .set_raster_scale(
                    PreviewRasterScale::new(raster_scale as f32)
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
            self.raster_scale = Some(raster_scale);
            self.rendered = None;
        }
        if self.request.as_ref() != Some(&request) {
            let parsed =
                serde_json::from_value(request.clone()).map_err(|error| error.to_string())?;
            match self.handle {
                Some(handle) => self.session.update_widget(handle, parsed),
                None => self
                    .session
                    .create_widget(parsed)
                    .map(|handle| self.handle = Some(handle)),
            }
            .map_err(|error| error.to_string())?;
            self.request = Some(request);
            self.rendered = None;
        }
        let handle = self.handle.expect("created above");
        let revision = self.session.widgets[&handle].revision();
        if self.rendered == Some((second, revision)) {
            return Ok(());
        }
        self.session
            .render(second)
            .map_err(|error| error.to_string())?;
        let widget = &self.session.widgets[&handle];
        self.presented = widget
            .presented_geometry()
            .map_err(|error| error.to_string())?;
        self.rendered = Some((second, widget.revision()));
        Ok(())
    }

    /// What it shows now, `None` until the first render.
    pub fn frame(&self) -> Option<SpeedFrame<'_>> {
        let handle = self.handle?;
        let widget = &self.session.widgets[&handle];
        Some(SpeedFrame {
            pixels: widget.pixels(),
            geometry: self.presented.as_ref()?,
            revision: widget.pixels_revision(),
        })
    }
}
