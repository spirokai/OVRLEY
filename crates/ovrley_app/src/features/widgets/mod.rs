//! Widgets: the widgets tab (the widget list and its open item), the widget actions, the widget
//! drawer, the preview host, and every editor as a declarative description that `ovrley_ui`
//! renders with one component. Descriptions translate their labels from the locale catalog.

use blocks::Block;
use ovrley_core::activity::schema::ParsedActivity;
use ovrley_core::fonts::FontFamily;
use ovrley_widgets::model::catalog::{catalog, AltitudeSource, IconRef};
use ovrley_widgets::model::document::{convert_altitude_value, create_widget_defaults, font_family_name, js_round, merge_widget_data, number, resolve_active_widget_data, widget_id, Collection, OverlayConfig, WidgetCreationOptions, WidgetData, WidgetSelection};
use ovrley_widgets::model::telemetry::{ActivitySeries, NumericActivity};
use rules::{parse_integer, parse_number_text, Input, Patch, RuleContext};
use sections::Ctx;
use serde_json::{Map, Value};
use crate::features::canvas::interaction::{content_rect, resize_draft_patch, rotation_draft_patch};
use crate::format::compare_text;
use crate::i18n::{catalog as translations, Locale};
use crate::store::Store;
use crate::store::services::Services;

pub mod blocks;
pub mod displays;
pub mod drawer;
pub mod editors;
pub mod preview;
pub mod raster;
pub mod rules;
pub mod sections;

/// The widgets tab's state beyond the store, and the font catalog.
#[derive(Debug, Default)]
pub struct WidgetsState {
    /// The open item, held while a marquee runs.
    held_open_widget_id: Option<String>,
    pub fonts: FontCatalogState,
    /// The image selection's error and loading state, per raster widget.
    pub raster: std::collections::BTreeMap<String, raster::RasterSelectionState>,
    /// The window-seconds input's invalid draft, per chart widget.
    pub window_drafts: std::collections::BTreeMap<String, String>,
}

/// The session's font catalog, loaded once on a worker, and the families' capabilities label
/// typography reads, loaded on demand.
#[derive(Debug, Default)]
pub struct FontCatalogState {
    requested: bool,
    pub recommended: Vec<FontOption>,
    pub system: Vec<FontOption>,
    /// Capabilities by font id: `None` while loading.
    capabilities: std::collections::HashMap<String, Option<std::sync::Arc<ovrley_core::fonts::FontFamily>>>,
}

impl FontCatalogState {
    /// A family's capabilities, once loaded.
    pub fn capabilities(&self, id: &str) -> Option<&ovrley_core::fonts::FontFamily> {
        self.capabilities.get(id).and_then(Option::as_deref)
    }

    /// Whether a family's capabilities are loading or loaded; a failed load is forgotten.
    pub fn capabilities_requested(&self, id: &str) -> bool {
        self.capabilities.contains_key(id)
    }
}

/// The capabilities of font `id`, loaded once: a worker loads them, and a failure goes to the error
/// alert.
pub fn request_font_capabilities(store: &mut Store, services: &mut Services, id: &str) {
    if store.widgets.fonts.capabilities.contains_key(id) {
        return;
    }
    store.widgets.fonts.capabilities.insert(id.to_string(), None);
    let font_dirs = services.paths.font_dirs.clone();
    let id = id.to_string();
    services.workers.spawn("font-capabilities", move || {
        let loaded = ovrley_core::fonts::font_capabilities(&font_dirs, &id);
        Box::new(move |store: &mut Store, _: &mut Services| match loaded {
            Ok(family) => {
                store.widgets.fonts.capabilities.insert(id, Some(std::sync::Arc::new(family)));
            }
            Err(error) => {
                // Forgotten, so a later edit asks again; a waiting font edit is dropped.
                store.widgets.fonts.capabilities.remove(&id);
                store.media.set_error_message(error.to_string());
            }
        })
    });
}

/// Reconcile step: the open label's font capabilities are requested.
pub fn reconcile_label_typography(store: &mut Store, services: &mut Services) {
    let Some(id) = open_widget_id(store).map(str::to_string) else { return };
    let Ok((collection, _)) = store.editor.config.find_widget(&id) else { return };
    if collection != Collection::Labels {
        return;
    }
    let font = store.editor.config.widget(&id).ok().and_then(|data| data.get("font")).and_then(Value::as_str).map(str::to_string);
    if let Some(font) = font {
        request_font_capabilities(store, services, &font);
    }
}

/// Reconcile step: the open item follows the selection, except while a marquee runs, when it keeps
/// the last value seen before.
pub fn reconcile_open_widget(store: &mut Store) {
    if !store.editor.is_marquee_selecting {
        store.widgets.held_open_widget_id = store.editor.selected_widget_id.clone();
    }
}

/// The accordion's open item.
pub fn open_widget_id(store: &Store) -> Option<&str> {
    if store.editor.is_marquee_selecting {
        store.widgets.held_open_widget_id.as_deref()
    } else {
        store.editor.selected_widget_id.as_deref()
    }
}

/// Loads the font catalog once per session: a failure is logged and the select stays empty.
pub fn load_font_catalog(store: &mut Store, services: &mut Services) {
    if std::mem::replace(&mut store.widgets.fonts.requested, true) {
        return;
    }
    let font_dirs = services.paths.font_dirs.clone();
    services.workers.spawn("font-catalog", move || {
        let catalog = ovrley_core::fonts::font_catalog(&font_dirs);
        Box::new(move |store: &mut Store, _: &mut Services| match catalog {
            Ok(catalog) => {
                store.widgets.fonts.recommended = catalog.recommended_fonts.iter().map(FontOption::from).collect();
                store.widgets.fonts.system = catalog.system_fonts.iter().map(FontOption::from).collect();
            }
            Err(error) => log::warn!("Failed to load available fonts: {error}"),
        })
    });
}

// ── The view model ──────────────────────────────────────────────────────────────────────────────

/// One row of the widget list.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetRowView {
    pub id: String,
    pub name: String,
    pub icon: IconRef,
}

/// The open widget's inspector: its description.
#[derive(Clone, Debug, PartialEq)]
pub struct InspectorView {
    pub id: String,
    pub blocks: Vec<Block>,
}

/// What the widgets tab shows.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarView {
    pub rows: Vec<WidgetRowView>,
    pub open_widget_id: Option<String>,
    pub inspector: Option<InspectorView>,
}

/// The sidebar's widgets: document widgets with their live drafts
/// applied, named and ordered for the sidebar.
fn sidebar_widgets(store: &Store) -> Vec<ConfigWidget> {
    let mut widgets = build_config_widgets(&store.editor.config);
    for widget in &mut widgets {
        // A draft's widget is merged and resolved; others stay as stored.
        if let Some(draft) = store.drafts.widget_draft(&widget.id) {
            widget.data = resolve_active_widget_data(&merge_widget_data(Some(&widget.data), draft));
        }
    }
    group_widgets_for_sidebar(&widgets, store.i18n.language)
}

/// The widget as its inspector shows it: drafts applied, then the live canvas gesture (only the
/// open inspector follows it).
fn inspector_widget(store: &Store, committed: &ConfigWidget) -> ConfigWidget {
    let mut widget = committed.clone();
    if let Some(patch) = store.drafts.gesture_patch(&widget.id) {
        widget.data = resolve_active_widget_data(&merge_widget_data(Some(&widget.data), patch));
    }
    widget
}

/// The widget `widget_id` as its inspector shows it.
fn inspected(store: &Store, widget_id: &str) -> Result<ConfigWidget, String> {
    let widgets = sidebar_widgets(store);
    let committed = widgets.iter().find(|widget| widget.id == widget_id).ok_or_else(|| format!("No widget {widget_id}"))?;
    Ok(inspector_widget(store, committed))
}

/// The position section, then the widget's editor, 1.5rem apart.
pub fn describe(store: &Store, widget: &ConfigWidget) -> Vec<Block> {
    let ctx = Ctx::new(store, widget);
    let mut nodes = vec![sections::position_section(&ctx)];
    let definition = catalog().require_widget(&widget.kind).expect("listed widgets are in the catalog");
    // The type's editor wins; otherwise a metric widget gets the metric editor.
    if let Some(editor) = editors::type_editor(&ctx) {
        nodes.extend(editor);
    } else if definition.metric {
        nodes.extend(displays::metric_widget_editor(&ctx));
    }
    blocks::flatten(1.5, nodes)
}

/// The widgets tab's view: the rows, the open item and its inspector.
pub fn sidebar_view(store: &Store) -> SidebarView {
    let widgets = sidebar_widgets(store);
    let open = open_widget_id(store).map(str::to_string);
    let inspector = open.as_deref().and_then(|id| widgets.iter().find(|widget| widget.id == id)).map(|committed| {
        let widget = inspector_widget(store, committed);
        InspectorView { id: widget.id.clone(), blocks: describe(store, &widget) }
    });
    SidebarView {
        rows: widgets
            .iter()
            .map(|widget| WidgetRowView {
                id: widget.id.clone(),
                name: widget.name.clone(),
                icon: catalog().require_widget(&widget.kind).expect("listed widgets are in the catalog").icon.clone(),
            })
            .collect(),
        open_widget_id: open,
        inspector,
    }
}

/// Whether an inspector edit is a draft, which a later commit records as one undo step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// A slider's live value.
    Draft,
    Commit,
}

/// An inspector edit: the field is looked up in the widget's description, and its rule turns
/// the input into a patch.
pub fn edit_field(store: &mut Store, services: &mut Services, widget_id: &str, field_id: &str, input: Input, edit: Edit) -> Result<(), String> {
    let widget = inspected(store, widget_id)?;
    let blocks = describe(store, &widget);
    let cell = blocks::find(&blocks, field_id).ok_or_else(|| format!("The {} inspector has no field {field_id}", widget.kind))?;
    if cell.disabled {
        return Err(format!("{field_id} is disabled"));
    }
    let context = RuleContext { data: &widget.data, kind: &widget.kind, unit_system: store.editor.unit_system };
    match rules::patch(cell, input, &context)? {
        Patch::Data(patch) if edit == Edit::Draft => update_widget_draft(store, widget_id, patch),
        Patch::Data(patch) => {
            store.widgets.window_drafts.remove(widget_id);
            update_widget_data(store, widget_id, patch)
        }
        Patch::Numeric { key, raw, options } => set_numeric_field(store, widget_id, &key, &raw, options),
        Patch::Reset => reset_widget(store, widget_id),
        Patch::Draft(patch) => update_widget_draft(store, widget_id, patch),
        Patch::Nothing => {
            store.widgets.window_drafts.remove(widget_id);
            Ok(())
        }
        Patch::InvalidWindow(text) => {
            store.widgets.window_drafts.insert(widget_id.to_string(), text);
            Ok(())
        }
        Patch::PickRaster => {
            raster::select_raster_image(store, services, widget_id);
            Ok(())
        }
    }
}

/// The font select's two lists for the open widget.
pub fn font_options(store: &Store, value: &str) -> (Vec<FontOption>, Vec<FontOption>) {
    font_select_options(value, &store.widgets.fonts.recommended, &store.widgets.fonts.system)
}

// ── Widget actions ─────────────────────────────────────────────────────────────────────────────

fn text(data: &Map<String, Value>, key: &str) -> String {
    data.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn float(data: &Map<String, Value>, key: &str) -> f64 {
    data.get(key).and_then(Value::as_f64).unwrap_or_default()
}

fn alignment_factor(alignment: &str) -> Result<f64, String> {
    match alignment {
        "left" => Ok(0.0),
        "center" => Ok(0.5),
        "right" => Ok(1.0),
        other => Err(format!("Unsupported content alignment: {other}")),
    }
}

/// Applies a patch to a widget as one undo step. A content alignment change
/// keeps the content in place by shifting x by the presented content width. A map switching to the
/// top-down camera starts with its whole route in view.
pub fn update_widget_data(store: &mut Store, widget_id: &str, updates: Map<String, Value>) -> Result<(), String> {
    let data = store.editor.config.widget(widget_id)?.clone();
    let mut updates = updates;
    let (collection, _) = store.editor.config.find_widget(widget_id)?;
    if collection.widget_type(&data)? == "map" && updates.get("camera_mode").and_then(Value::as_str) == Some("fixed") && text(&data, "camera_mode") != "fixed" {
        let theme = store.theme.clone().ok_or("The editor has not started yet.")?;
        let effective = crate::features::widgets::preview::preview_widgets(store, &theme)
            .into_iter()
            .find(|widget| widget.id == widget_id)
            .ok_or_else(|| format!("Unknown widget ID: {widget_id}"))?
            .data;
        let fit = crate::features::canvas::interaction::map_view_patch(store, widget_id, Some(merge_widget_data(Some(&effective), &updates)), Value::String("fit".into()))?;
        updates.extend(fit);
    }
    if let Some(next) = updates.get("content_alignment").cloned() {
        if Some(&next) != data.get("content_alignment") {
            let current = alignment_factor(&text(&data, "content_alignment"))?;
            let next = alignment_factor(next.as_str().unwrap_or_default())?;
            let width = content_rect(store, widget_id).ok_or("Alignment change requires rendered content width")?.width;
            updates.insert("x".into(), number(float(&data, "x") + (next - current) * width));
        }
    }
    store.editor.edit_config(|config| config.update_widgets(&[(widget_id.to_string(), updates)]))
}

/// A live inspector edit, outside the history. A rotation goes through the
/// canvas's rotation, and a heading tape's label offset through the resize policy, so the draft
/// carries the geometry they move.
pub fn update_widget_draft(store: &mut Store, widget_id: &str, updates: Map<String, Value>) -> Result<(), String> {
    let data = store.editor.config.widget(widget_id)?;
    let tape_offset = (text(data, "display_type") == "heading_tape")
        .then(|| updates.get("display_variants").and_then(|variants| variants.get("heading_tape")).and_then(|tape| tape.get("label_offset")).cloned())
        .flatten();
    let patch = if let Some(rotation) = updates.get("rotation").and_then(Value::as_f64) {
        let mut patch = updates.clone();
        patch.extend(rotation_draft_patch(store, widget_id, rotation)?);
        patch
    } else if let Some(offset) = tape_offset {
        let mut geometry = Map::new();
        geometry.insert("label_offset".into(), offset);
        resize_draft_patch(store, widget_id, geometry)?
    } else {
        updates
    };
    store.drafts.set_widget_draft(widget_id, &patch);
    Ok(())
}

/// The widget's draft becomes one undo step.
pub fn commit_widget_draft(store: &mut Store, widget_id: &str) -> Result<(), String> {
    if let Some(draft) = store.drafts.widget_draft(widget_id).cloned() {
        store.editor.edit_config(|config| config.update_widgets(&[(widget_id.to_string(), draft)]))?;
    }
    store.drafts.clear_widget_drafts(&[widget_id.to_string()]);
    Ok(())
}

/// How a number field stores its text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumericFieldOptions {
    pub fallback: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// An empty input clears the field (`null`).
    pub optional: bool,
    pub integer: bool,
    pub round: bool,
}

impl Default for NumericFieldOptions {
    fn default() -> Self {
        Self { fallback: 0.0, min: None, max: None, optional: false, integer: true, round: false }
    }
}

/// Parses an input, clamps it to its range and updates the widget.
pub fn set_numeric_field(store: &mut Store, widget_id: &str, key: &str, raw: &str, options: NumericFieldOptions) -> Result<(), String> {
    let mut patch = Map::new();
    if options.optional && raw.is_empty() {
        patch.insert(key.into(), Value::Null);
        return update_widget_data(store, widget_id, patch);
    }
    let mut parsed = if options.integer { parse_integer(raw, options.fallback)? } else { parse_number_text(raw) };
    if options.round {
        parsed = js_round(parse_number_text(raw));
    }
    if !parsed.is_finite() {
        return Err(format!("Invalid numeric value for {key}"));
    }
    let clamped = parsed.max(options.min.unwrap_or(parsed)).min(options.max.unwrap_or(parsed));
    patch.insert(key.into(), number(clamped));
    update_widget_data(store, widget_id, patch)
}

/// Fresh defaults for the selection, appended to the type's collection, then selected. New course
/// plots fit the activity at the playhead. The scene font size comes from the global defaults
/// here, from the scene in `reset_widget`.
///
/// A top-down map starts with its whole route in view, as switching to the top-down camera does,
/// in the same undo step. Its route is measured on the preview, so the new map presents first; a
/// map that cannot present yet (its font still loading) keeps the defaults' view.
pub fn add_widget(store: &mut Store, services: &mut Services, selection: &WidgetSelection) -> Result<String, String> {
    let definition = catalog().require_widget(&selection.kind)?;
    let collection = Collection::from_name(&definition.collection).ok_or_else(|| format!("{} has no collection {}", selection.kind, definition.collection))?;
    let preview_activity = crate::features::widgets::preview::preview_activity(store);
    let course_points = preview_activity.activity().map(|imported| imported.activity.sample_course_points.clone()).unwrap_or_default();
    let globals = &store.template.global_defaults;
    let options = WidgetCreationOptions { course_points, scene_font_size: globals.get("font_size").and_then(Value::as_f64), unit_system: store.editor.unit_system };
    let data = create_widget_defaults(selection, globals, &options)?;
    let top_down_map = selection.kind == "map" && data.get("camera_mode").and_then(Value::as_str) == Some("fixed");
    let id = store.editor.edit_config(|config| config.add_widget(collection, data));
    store.editor.set_selected_widget_id(Some(&id));
    if top_down_map {
        crate::features::widgets::preview::present_pending_frame(store, services);
        match crate::features::canvas::interaction::map_view_patch(store, &id, None, Value::String("fit".into())) {
            Ok(fit) => store.editor.edit_config(|config| config.update_widgets(&[(id.clone(), fit)]))?,
            Err(error) => log::warn!("The new map keeps its default view: {error}"),
        }
    }
    Ok(id)
}

pub fn delete_widget(store: &mut Store, widget_id: &str) -> Result<(), String> {
    store.editor.config.widget(widget_id)?;
    let ids = [widget_id.to_string()];
    store.editor.edit_config(|config| config.delete_widgets(&ids));
    Ok(())
}

/// The manifest's reset selection as a drawer selection of `kind`.
fn reset_selection(kind: &str) -> WidgetSelection {
    let definition = catalog().require_widget(kind).expect("listed widgets are in the catalog");
    let mut selection = WidgetSelection { kind: kind.into(), ..WidgetSelection::default() };
    for (key, value) in &definition.reset_selection {
        match (key.as_str(), value) {
            ("displayType", Value::String(display)) => selection.display_type = Some(display.clone()),
            ("preset", Value::Object(preset)) => selection.preset = preset.clone(),
            (_, Value::String(mode)) => selection.mode = Some(mode.clone()),
            _ => panic!("{kind}'s resetSelection has an unsupported field {key}"),
        }
    }
    selection
}

/// Replaces the widget's data with fresh defaults for its type.
pub fn reset_widget(store: &mut Store, widget_id: &str) -> Result<(), String> {
    let (collection, _) = store.editor.config.find_widget(widget_id)?;
    let kind = collection.widget_type(store.editor.config.widget(widget_id)?)?;
    let options = WidgetCreationOptions {
        scene_font_size: store.editor.config.scene.get("font_size").and_then(Value::as_f64),
        unit_system: store.editor.unit_system,
        ..WidgetCreationOptions::default()
    };
    let data = create_widget_defaults(&reset_selection(&kind), &store.template.global_defaults, &options)?;
    store.editor.edit_config(|config| config.replace_widget(widget_id, &data))
}

/// The rule input of an agent's value for `cell`: what the control would report.
fn agent_input(cell: &blocks::Cell, value: &Value) -> Result<Input, String> {
    use blocks::CellKind;
    Ok(match (value, &cell.kind) {
        (Value::Null, _) => Input::Clear,
        (Value::Bool(flag), _) => Input::Flag(*flag),
        (Value::Number(number), CellKind::Number { .. } | CellKind::WindowSeconds { .. }) => Input::Text(crate::format::plain_number(number.as_f64().unwrap_or_default())),
        (Value::Number(number), _) => Input::Number(number.as_f64().unwrap_or_default()),
        (Value::String(text), _) => Input::Text(text.clone()),
        (other, _) => return Err(format!("{} takes a single value, not {other}", cell.id)),
    })
}

/// An agent's field patch through the inspector's rules: a field the inspector edits with a
/// named rule (bar fill style, map shape, track geometry, …) goes through it, so the widget stays
/// as consistent as the UI leaves it; other fields patch as they are. One patch, one undo step.
fn ruled_patch(store: &Store, widget_id: &str, fields: Map<String, Value>) -> Result<Map<String, Value>, String> {
    let widget = inspected(store, widget_id)?;
    let blocks = describe(store, &widget);
    let context = RuleContext { data: &widget.data, kind: &widget.kind, unit_system: store.editor.unit_system };
    // Each leaf: a top-level field, or a display variant's field.
    let mut leaves: Vec<(String, Value)> = Vec::new();
    for (key, value) in fields {
        match (key.as_str(), value) {
            ("display_variants", Value::Object(variants)) => {
                for (display, fields) in variants {
                    match fields {
                        Value::Object(fields) => leaves.extend(fields.into_iter().map(|(field, value)| (format!("display_variants.{display}.{field}"), value))),
                        other => leaves.push((format!("display_variants.{display}"), other)),
                    }
                }
            }
            (_, value) => leaves.push((key, value)),
        }
    }
    let mut patch = Map::new();
    for (path, value) in leaves {
        let cell = blocks::find(&blocks, &path).filter(|cell| matches!(cell.rule, blocks::Rule::Named(_)));
        // A choice takes one of its options, as the select offers them.
        if let Some(blocks::CellKind::Select { options, .. } | blocks::CellKind::ToggleGroup { options, .. }) = cell.map(|cell| &cell.kind) {
            if !options.iter().any(|option| Some(option.value.as_str()) == value.as_str()) {
                let values: Vec<&str> = options.iter().map(|option| option.value.as_str()).collect();
                return Err(format!("{path} takes one of: {}.", values.join(", ")));
            }
        }
        let next = match cell {
            Some(cell) => match rules::patch(cell, agent_input(cell, &value)?, &context)? {
                Patch::Data(next) | Patch::Draft(next) => next,
                Patch::Nothing | Patch::InvalidWindow(_) => return Err(format!("The editor does not accept {value} for {path}.")),
                Patch::Numeric { .. } | Patch::Reset | Patch::PickRaster => return Err(format!("{path} cannot be set this way.")),
            },
            None => rules::path_patch(&widget.data, &path, value),
        };
        patch = merge_widget_data(Some(&patch), &next);
    }
    Ok(patch)
}

/// The fields the open widget's editor offers, as agents read them (`widget_get`): each
/// field's id, kind, label, range and options, in English.
pub fn editable_fields(store: &Store, widget_id: &str) -> Result<Vec<Value>, String> {
    use blocks::CellKind;
    let widget = inspected(store, widget_id)?;
    let ctx = Ctx { locale: crate::i18n::Locale::En, ..Ctx::new(store, &widget) };
    let mut nodes = vec![sections::position_section(&ctx)];
    if let Some(editor) = editors::type_editor(&ctx) {
        nodes.extend(editor);
    } else if catalog().require_widget(&widget.kind)?.metric {
        nodes.extend(displays::metric_widget_editor(&ctx));
    }
    let blocks = blocks::flatten(1.5, nodes);
    let options = |options: &[blocks::OptionView]| options.iter().map(|option| serde_json::json!({"value": option.value, "label": option.label})).collect::<Vec<_>>();
    Ok(blocks::cells(&blocks)
        .filter(|cell| !matches!(cell.kind, CellKind::Empty | CellKind::IconButton { .. } | CellKind::Alert { .. } | CellKind::RasterPicker { .. }))
        .map(|cell| {
            let mut field = serde_json::json!({"id": cell.id, "label": cell.label, "disabled": cell.disabled});
            let (kind, extra) = match &cell.kind {
                // In the field's own units: Transparency's percent slider stores 0–1.
                CellKind::Slider { min, max, step, .. } => {
                    let divisor = if let blocks::Rule::Scaled(divisor) = cell.rule { divisor } else { 1.0 };
                    ("number", serde_json::json!({"min": number(*min / divisor), "max": number(*max / divisor), "step": number(*step / divisor)}))
                }
                CellKind::Number { .. } => ("number", Value::Null),
                CellKind::Color { .. } => ("color", Value::Null),
                CellKind::Select { options: items, .. } | CellKind::ToggleGroup { options: items, .. } | CellKind::Tabs { options: items, .. } => ("choice", serde_json::json!({"options": options(items)})),
                CellKind::Font { .. } => ("font", Value::Null),
                CellKind::Text { .. } | CellKind::WindowSeconds { .. } => ("text", Value::Null),
                CellKind::Switch { .. } | CellKind::SwitchRow { .. } | CellKind::Italic { .. } => ("flag", Value::Null),
                CellKind::Alignment { options: items, .. } => ("choice", serde_json::json!({"options": options(items)})),
                CellKind::Gradient { .. } => ("gradient stops", Value::Null),
                CellKind::ShapeGrid { .. } => ("shape", Value::Null),
                CellKind::ChartSeries { .. } => ("series row", Value::Null),
                CellKind::GForceAxis { .. } => ("axis", Value::Null),
                _ => ("other", Value::Null),
            };
            field["kind"] = Value::String(kind.into());
            if let Value::Object(extra) = extra {
                field.as_object_mut().expect("a field is an object").extend(extra);
            }
            field
        })
        .collect())
}

/// An agent's widget data patch: checked against the type's fields, then
/// validated by the durable normalization and by the renderer. An invalid patch changes nothing and
/// fails with the reason.
pub fn update_widget_fields(store: &mut Store, services: &mut Services, widget_id: &str, fields: Map<String, Value>) -> Result<(), String> {
    let (collection, _) = store.editor.config.find_widget(widget_id)?;
    let kind = collection.widget_type(store.editor.config.widget(widget_id)?)?;
    let definition = catalog().require_widget(&kind)?;
    let type_field = catalog().collection(collection.name()).and_then(|entry| entry.type_field.clone());
    for key in fields.keys() {
        if key == "id" || Some(key) == type_field.as_ref() {
            return Err(format!("{key} cannot be changed; delete the widget and add another instead."));
        }
        if !definition.config_keys.contains(key) {
            return Err(format!("{kind} widgets have no field {key}. Their fields: {}.", definition.config_keys.join(", ")));
        }
    }
    let patch = ruled_patch(store, widget_id, fields)?;
    let previous = std::rc::Rc::clone(&store.editor.config);
    let had_error = store.preview.error.is_some();
    update_widget_data(store, widget_id, patch)?;
    let durable = ovrley_core::normalize::template::create_durable_template_state(&store.editor.config, Some(&store.template.global_defaults));
    let rendered = durable.map_err(|error| error.to_string()).and_then(|_| {
        crate::features::widgets::preview::present_pending_frame(store, services);
        match &store.preview.error {
            Some(error) if !had_error => Err(error.clone()),
            _ => Ok(()),
        }
    });
    if let Err(error) = rendered {
        store.editor.replace_config(previous);
        crate::features::widgets::preview::present_pending_frame(store, services);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::features::history::{undo, HistoryMode};
    use crate::features::widgets::preview::preview_frame;
    use crate::store::template::ThemeColors;
    use crate::store::services::fakes::{fake_services, Fakes};
    use crate::store::Dispatcher;
    use ovrley_core::normalize::template::template_manifest;
    use ovrley_widgets::model::document::{OverlayConfig, UnitSystem};
    use serde_json::json;

    pub(crate) fn defaults(kind: &str, display_type: Option<&str>) -> Map<String, Value> {
        let selection = WidgetSelection { kind: kind.into(), display_type: display_type.map(str::to_string), ..WidgetSelection::default() };
        create_widget_defaults(&selection, &template_manifest().globals, &WidgetCreationOptions::default()).unwrap()
    }

    pub(crate) fn value(kind: &str, id: &str, x: f64) -> Value {
        let mut data = defaults(kind, None);
        data.insert("id".into(), json!(id));
        data.insert("x".into(), json!(x));
        Value::Object(data)
    }

    /// A dispatcher over `values`, with the preview's frames run until its fonts are ready.
    pub(crate) fn editor(values: Vec<Value>) -> (Dispatcher, Fakes) {
        editor_with(json!({"values": values}))
    }

    /// A dispatcher over a document of `collections` (`{"labels": [...], ...}`), its preview
    /// frames run until every widget is presented.
    pub(crate) fn editor_with(collections: Value) -> (Dispatcher, Fakes) {
        let (parts, fakes) = fake_services(json!({}));
        let store = Store { theme: Some(ThemeColors { ice: "#86aaaa".into(), aqua: "#40e0d0".into(), teal: "#005b5b".into(), accent: "#ad5013".into() }), ..Store::default() };
        let dispatcher = Dispatcher::new(store, parts);
        dispatcher.dispatch(move |store, _| {
            store.history.set_mode(HistoryMode::Reset);
            let mut document = collections;
            document["scene"] = json!({"width": 1000, "height": 500});
            store.editor.set_config(OverlayConfig::from_value(&document).unwrap());
        });
        for _ in 0..400 {
            fakes.ui_post.drain(&dispatcher);
            dispatcher.dispatch(preview_frame);
            let presented = dispatcher.with_store(|store| store.editor.config.widget_ids().iter().all(|id| store.preview.presented().contains_key(id)));
            if presented {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        (dispatcher, fakes)
    }

    pub(crate) fn patch(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    pub(crate) fn widget<R>(dispatcher: &Dispatcher, id: &str, read: impl FnOnce(&Map<String, Value>) -> R) -> R {
        dispatcher.with_store(|store| read(store.editor.config.widget(id).unwrap()))
    }

    /// Opens `id`'s inspector and returns its blocks.
    pub(crate) fn inspector(dispatcher: &Dispatcher, id: &'static str) -> Vec<Block> {
        dispatcher.dispatch(move |store, _| store.editor.set_selected_widget_id(Some(id)));
        dispatcher.with_store(|store| sidebar_view(store).inspector.unwrap().blocks)
    }

    /// The cell `field` of `id`'s inspector.
    pub(crate) fn cell(dispatcher: &Dispatcher, id: &'static str, field: &str) -> Option<blocks::Cell> {
        blocks::find(&inspector(dispatcher, id), field).cloned()
    }

    pub(crate) fn slider_value(cell: &blocks::Cell) -> (f64, f64) {
        match &cell.kind {
            blocks::CellKind::Slider { value, max, .. } => (*value, *max),
            other => panic!("{} is not a slider: {other:?}", cell.id),
        }
    }

    pub(crate) fn select_options(cell: &blocks::Cell) -> Vec<blocks::OptionView> {
        match &cell.kind {
            blocks::CellKind::Select { options, .. } => options.clone(),
            other => panic!("{} is not a select: {other:?}", cell.id),
        }
    }

    pub(crate) fn edit(dispatcher: &Dispatcher, id: &'static str, field: &'static str, input: Input) {
        dispatcher.dispatch(move |store, services| edit_field(store, services, id, field, input, Edit::Commit).unwrap());
    }

    #[test]
    fn slider_drafts_preview_live_and_commit_as_one_undo_step() {
        let (dispatcher, _) = editor(vec![value("speed", "speed-0", 300.0)]);
        let initial = widget(&dispatcher, "speed-0", |data| data["font_size"].clone());
        dispatcher.dispatch(|store, _| {
            update_widget_draft(store, "speed-0", patch(json!({"font_size": 50}))).unwrap();
        });
        dispatcher.dispatch(|store, _| {
            update_widget_draft(store, "speed-0", patch(json!({"font_size": 60.375}))).unwrap();
        });
        assert_eq!(widget(&dispatcher, "speed-0", |data| data["font_size"].clone()), initial, "drafts leave the document alone");
        assert_eq!(slider_value(&cell(&dispatcher, "speed-0", "font_size").unwrap()).0, 60.375, "the inspector shows the draft");
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 0);

        dispatcher.dispatch(|store, _| commit_widget_draft(store, "speed-0").unwrap());
        assert_eq!(widget(&dispatcher, "speed-0", |data| data["font_size"].clone()), json!(60.375));
        dispatcher.with_store(|store| {
            assert_eq!(store.history.past.len(), 1);
            assert!(store.drafts.widget_draft("speed-0").is_none());
        });
        dispatcher.dispatch(|store, _| {
            undo(store);
        });
        assert_eq!(widget(&dispatcher, "speed-0", |data| data["font_size"].clone()), initial);
    }

    #[test]
    fn an_alignment_change_compensates_x_from_the_presented_width() {
        let (dispatcher, _) = editor(vec![value("speed", "speed-0", 300.0)]);
        let width = dispatcher.with_store(|store| content_rect(store, "speed-0").unwrap().width);
        dispatcher.dispatch(|store, _| update_widget_data(store, "speed-0", patch(json!({"content_alignment": "right"}))).unwrap());
        widget(&dispatcher, "speed-0", |data| {
            assert_eq!(data["content_alignment"], "right");
            assert_eq!(data["x"].as_f64().unwrap(), 300.0 + width);
        });
    }

    #[test]
    fn numeric_fields_parse_clamp_and_clear() {
        let (dispatcher, _) = editor(vec![value("altitude", "alt", 10.0)]);
        dispatcher.dispatch(|store, _| set_numeric_field(store, "alt", "x", "12.7", NumericFieldOptions::default()).unwrap());
        assert_eq!(widget(&dispatcher, "alt", |data| data["x"].clone()), json!(12), "integers truncate");
        dispatcher.dispatch(|store, _| set_numeric_field(store, "alt", "x", "", NumericFieldOptions::default()).unwrap());
        assert_eq!(widget(&dispatcher, "alt", |data| data["x"].clone()), json!(0), "an empty integer is its fallback");
        dispatcher.dispatch(|store, _| {
            let options = NumericFieldOptions { min: Some(0.0), max: Some(100.0), ..NumericFieldOptions::default() };
            set_numeric_field(store, "alt", "y", "250", options).unwrap();
        });
        assert_eq!(widget(&dispatcher, "alt", |data| data["y"].clone()), json!(100));
        let optional = NumericFieldOptions { optional: true, round: true, ..NumericFieldOptions::default() };
        dispatcher.dispatch(move |store, _| set_numeric_field(store, "alt", "starting_altitude", "99.5", optional).unwrap());
        assert_eq!(widget(&dispatcher, "alt", |data| data["starting_altitude"].clone()), json!(100));
        dispatcher.dispatch(move |store, _| set_numeric_field(store, "alt", "starting_altitude", "", optional).unwrap());
        assert_eq!(widget(&dispatcher, "alt", |data| data["starting_altitude"].clone()), Value::Null);
        let invalid = dispatcher.dispatch_with_result(|store, _| set_numeric_field(store, "alt", "x", "abc", NumericFieldOptions::default())).unwrap();
        assert_eq!(invalid.unwrap_err(), "Invalid integer input: abc");
    }

    #[test]
    fn the_text_display_shows_each_types_controls() {
        let kinds = [
            "speed", "g_force", "distance", "stride_length", "torque", "vertical_speed", "pace", "air_pressure", "ground_contact_time", "stroke_rate",
            "vertical_oscillation", "left_right_balance", "gps_coordinates", "total_ascent",
        ];
        let values: Vec<Value> = kinds.iter().map(|kind| value(kind, kind, 0.0)).collect();
        let (dispatcher, _) = editor(values);
        let has = |id: &'static str, field: &str| cell(&dispatcher, id, field).is_some();
        for kind in ["speed", "g_force", "distance", "stride_length", "torque", "vertical_speed", "vertical_oscillation"] {
            assert!(has(kind, "decimals"), "{kind} has a decimal control");
        }
        for kind in ["pace", "air_pressure", "ground_contact_time", "stroke_rate", "left_right_balance"] {
            assert!(!has(kind, "decimals"), "{kind} has no decimal control");
        }
        assert_eq!(slider_value(&cell(&dispatcher, "speed", "decimals").unwrap()), (0.0, 1.0), "speed's slider runs 0 to 1");
        let full_distance = cell(&dispatcher, "distance", "show_full_distance").unwrap();
        assert!(matches!(full_distance.kind, blocks::CellKind::SwitchRow { checked: true, .. }), "full distance defaults to on");
        assert!(matches!(cell(&dispatcher, "left_right_balance", "balance_format").unwrap().kind, blocks::CellKind::Select { ref value, .. } if value == "percent_label"));
        assert!(!has("speed", "balance_format"));
        assert!(has("total_ascent", "show_full_ascent"));

        let formats: Vec<_> = select_options(&cell(&dispatcher, "gps_coordinates", "display_unit").unwrap()).into_iter().map(|option| option.label).collect();
        assert_eq!(formats, ["Latitude", "Longitude", "Both"]);
        assert!(!has("gps_coordinates", "show_units"), "coordinates have no units toggle");
        assert!(has("speed", "show_units"));
        assert_eq!(select_options(&cell(&dispatcher, "speed", "display_unit").unwrap())[0], blocks::OptionView::new("kmh", "km/h"));

        assert!(select_options(&cell(&dispatcher, "speed", "display_type").unwrap()).len() > 1, "speed offers several displays");
        assert_eq!(cell(&dispatcher, "speed", "x").unwrap().label, "Horizontal Anchor");
    }

    #[test]
    fn altitude_units_convert_calibration_and_show_the_activity_start() {
        let (dispatcher, _) = editor(vec![value("altitude", "alt", 0.0)]);
        dispatcher.dispatch(|store, _| store.editor.set_selected_widget_id(Some("alt")));
        edit(&dispatcher, "alt", "display_unit", Input::Text("ft".into()));
        widget(&dispatcher, "alt", |data| assert_eq!((data["display_unit"].clone(), data["starting_altitude"].clone()), (json!("ft"), Value::Null)));
        edit(&dispatcher, "alt", "display_unit", Input::Text("m".into()));
        edit(&dispatcher, "alt", "starting_altitude", Input::Text("100".into()));
        edit(&dispatcher, "alt", "display_unit", Input::Text("ft".into()));
        assert_eq!(widget(&dispatcher, "alt", |data| data["starting_altitude"].clone()), json!(328));
        let altitude = cell(&dispatcher, "alt", "starting_altitude").unwrap();
        assert!(matches!(&altitude.kind, blocks::CellKind::Number { value, suffix, resettable: true, .. } if value == "328" && suffix == "ft"));
        edit(&dispatcher, "alt", "starting_altitude", Input::Clear);
        assert_eq!(widget(&dispatcher, "alt", |data| data["starting_altitude"].clone()), Value::Null);
    }

    #[test]
    fn the_open_item_holds_while_a_marquee_runs() {
        let (dispatcher, _) = editor(vec![value("speed", "a", 0.0), value("heartrate", "b", 0.0)]);
        dispatcher.dispatch(|store, _| store.editor.set_selected_widget_id(Some("a")));
        dispatcher.dispatch(|store, _| {
            store.editor.set_marquee_selecting(true);
            store.editor.set_selected_widget_id(Some("b"));
        });
        assert_eq!(dispatcher.with_store(|store| open_widget_id(store).map(str::to_string)), Some("a".into()));
        dispatcher.dispatch(|store, _| store.editor.set_marquee_selecting(false));
        assert_eq!(dispatcher.with_store(|store| sidebar_view(store).open_widget_id), Some("b".into()));
    }

    #[test]
    fn reset_restores_defaults_and_delete_removes_the_widget() {
        let (dispatcher, _) = editor(vec![value("speed", "a", 0.0)]);
        dispatcher.dispatch(|store, _| update_widget_data(store, "a", patch(json!({"x": 400, "show_icon": false}))).unwrap());
        dispatcher.dispatch(|store, _| reset_widget(store, "a").unwrap());
        let expected = defaults("speed", None);
        widget(&dispatcher, "a", |data| {
            assert_eq!(data["x"], expected["x"]);
            assert_eq!(data["show_icon"], expected["show_icon"]);
            assert_eq!(data["id"], "a");
        });
        dispatcher.dispatch(|store, _| delete_widget(store, "a").unwrap());
        assert!(dispatcher.with_store(|store| sidebar_view(store).rows.is_empty()));
    }

    #[test]
    fn display_type_changes_and_unit_systems_follow_the_catalog() {
        let (dispatcher, _) = editor(vec![value("speed", "a", 0.0)]);
        dispatcher.dispatch(|store, _| store.editor.initialize_unit_system(UnitSystem::Imperial));
        edit(&dispatcher, "a", "display_type", Input::Text("linear".into()));
        widget(&dispatcher, "a", |data| {
            assert_eq!(data["display_type"], "linear");
            assert!(data["display_variants"]["linear"].is_object());
        });
    }

    #[test]
    fn agent_patches_are_checked_and_an_invalid_one_changes_nothing() {
        let (mut dispatcher, _) = editor(vec![value("speed", "a", 0.0)]);
        let run = |dispatcher: &mut Dispatcher, fields: Value| {
            dispatcher.dispatch_with_result(move |store, services| update_widget_fields(store, services, "a", patch(fields))).unwrap()
        };
        assert!(run(&mut dispatcher, json!({"nonsense": 1})).unwrap_err().contains("no field nonsense"));
        assert!(run(&mut dispatcher, json!({"value": "cadence"})).unwrap_err().contains("cannot be changed"));
        let before = dispatcher.with_store(|store| store.history.past.len());
        assert!(run(&mut dispatcher, json!({"display_type": "nonsense"})).is_err());
        dispatcher.with_store(|store| {
            assert_eq!(store.editor.config.widget("a").unwrap()["display_type"], "text");
            assert_eq!(store.history.past.len(), before, "a refused patch leaves no undo step");
        });
        run(&mut dispatcher, json!({"decimals": 1})).unwrap();
        assert_eq!(widget(&dispatcher, "a", |data| data["decimals"].clone()), json!(1));
    }

    fn config(value: Value) -> OverlayConfig {
        OverlayConfig::from_value(&value).unwrap()
    }

    #[test]
    fn widgets_list_in_collection_order_with_their_names() {
        let widgets = build_config_widgets(&config(json!({
            "plots": [{"id": "p", "value": "course"}], "values": [{"id": "v", "value": "speed"}],
            "labels": [{"id": "l", "text": "Label"}], "backdrops": [{"id": "b", "display_type": "rectangle"}]
        })));
        assert_eq!(widgets.iter().map(|widget| widget.collection).collect::<Vec<_>>(), [Collection::Backdrops, Collection::Labels, Collection::Values, Collection::Plots]);
        assert_eq!(widgets[1].name, "Label");
        assert_eq!(widgets[2].kind, "speed");
    }

    #[test]
    fn the_sidebar_groups_by_type_and_sorts_by_priority_and_name() {
        let widgets = build_config_widgets(&config(json!({
            "backdrops": [{"id": "b", "display_type": "rectangle"}],
            "labels": [{"id": "l1", "text": "Beta"}, {"id": "l2", "text": "Alpha"}],
            "values": [{"id": "s", "value": "speed"}, {"id": "h", "value": "heartrate"}]
        })));
        let sorted = group_widgets_for_sidebar(&widgets, Locale::En);
        assert_eq!(sorted[0].id, "b", "backdrops have the lowest priority");
        let labels: Vec<_> = sorted.iter().filter(|widget| widget.kind == "label").map(|widget| widget.name.as_str()).collect();
        assert_eq!(labels, ["Beta", "Alpha"], "label text stays, in document order");
        let speed = sorted.iter().find(|widget| widget.id == "s").unwrap();
        assert_eq!(speed.name, widget_type_name("speed", Locale::En));
    }

    #[test]
    fn attribute_labels_fall_back_to_title_case() {
        assert_eq!(activity_attribute_label("speed", Locale::En), widget_type_name("speed", Locale::En));
        assert_eq!(activity_attribute_label("gps_fix_quality_xyz", Locale::En), "GPS Fix Quality Xyz");
    }

    #[test]
    fn altitude_placeholders_come_from_the_first_activity_value() {
        let activity = ParsedActivity {
            elevation: vec![None, Some(100.0)],
            ..Default::default()
        };
        let widgets = build_config_widgets(&config(json!({"values": [{"id": "a", "value": "altitude", "display_unit": "ft"}]})));
        assert_eq!(starting_altitude_placeholder(&widgets[0], Some(&activity)), Some(328.0));
        assert_eq!(starting_altitude_placeholder(&widgets[0], None), None);
    }

    fn option(id: &str, name: &str) -> FontOption {
        FontOption { id: id.into(), name: name.into() }
    }

    #[test]
    fn select_options_keep_an_unavailable_selection_and_drop_duplicate_system_names() {
        let recommended = [option("Inter.ttf", "Inter")];
        let system = [option("sys:Inter", "Inter"), option("sys:Arial", "Arial")];
        let (bundled, others) = font_select_options("Gone.ttf", &recommended, &system);
        assert_eq!(bundled, [option("Gone.ttf", "Gone"), option("Inter.ttf", "Inter")]);
        assert_eq!(others, [option("sys:Arial", "Arial")]);
        let (bundled, _) = font_select_options("sys:Arial", &recommended, &system);
        assert_eq!(bundled, recommended);
    }

    #[test]
    fn collects_every_referenced_font_once() {
        let value = json!({"globalDefaults": {"font_text": "A.ttf", "font_values": "B.ttf"},
            "values": [{"font": "B.ttf", "display_variants": {"linear": {"min_max_label_font": "C.ttf"}}}], "labels": [{"font": null}]});
        assert_eq!(collect_font_ids(&value), ["A.ttf", "B.ttf", "C.ttf"]);
    }
}

/// The keys the descriptions translate in Rust are checked against the English catalog, as
/// `ovrley_ui/tests/translations.rs` checks every `@tr`. A key no locale has shows its code
/// default; those are listed here so a new one is a decision.
#[cfg(test)]
mod translation_keys {
    use crate::i18n::{catalog, Locale};

    /// Keys translated with a code default the locale files lack.
    const CODE_DEFAULTS: &[&str] = &[
        "components.gradient",
        "widget-editor.alignCenter",
        "widget-editor.alignLeft",
        "widget-editor.alignRight",
        "widget-editor.contentAlignment",
        "widget-editor.dimensions",
        "widget-editor.horizontalAnchor",
    ];

    #[test]
    fn every_key_a_description_translates_is_in_the_english_catalog() {
        let sources = [
            include_str!("blocks.rs"),
            include_str!("displays.rs"),
            include_str!("editors.rs"),
            include_str!("mod.rs"),
            include_str!("raster.rs"),
            include_str!("rules.rs"),
            include_str!("sections.rs"),
            include_str!("../scene.rs"),
            include_str!("../maps.rs"),
            include_str!("drawer.rs"),
        ];
        let key = regex::Regex::new(r#""((?:widget-editor|scene-settings|widgets|raster|overlay-editor|app-shell|components|toolbar)\.[A-Za-z0-9_.]+)""#).unwrap();
        let mut missing: Vec<String> = sources
            .iter()
            .flat_map(|source| key.captures_iter(source).map(|found| found[1].to_string()).collect::<Vec<_>>())
            .filter(|key| !key.ends_with('.') && !key.ends_with(".rs") && catalog().t_or(Locale::En, key, "\u{0}") == "\u{0}" && !CODE_DEFAULTS.contains(&key.as_str()))
            .collect();
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "keys missing from the English catalog: {missing:?}");
    }
}

/// One widget of the document, as the editor lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigWidget {
    pub id: String,
    pub kind: String,
    pub collection: Collection,
    /// The name field's value (a label's text), else the type.
    pub name: String,
    pub data: WidgetData,
}

/// Every widget in document order. Ingress validated each type.
pub fn build_config_widgets(config: &OverlayConfig) -> Vec<ConfigWidget> {
    config
        .widgets()
        .map(|(collection, data)| {
            let kind = collection.widget_type(data).expect("ingress validated widget types");
            let definition = catalog().require_widget(&kind).expect("ingress validated widget types");
            let name = match &definition.name_field {
                None => kind.clone(),
                Some(field) => data.get(field).and_then(Value::as_str).unwrap_or_default().to_string(),
            };
            ConfigWidget {
                id: widget_id(data).to_string(),
                kind,
                collection,
                name,
                data: data.clone(),
            }
        })
        .collect()
}

pub fn widget_type_name(kind: &str, locale: Locale) -> String {
    let definition = catalog().require_widget(kind).expect("a listed widget type is in the catalog");
    translations().t(locale, &definition.name_key).to_string()
}

/// A widget's name, else the attribute's own label, else the
/// attribute title-cased.
pub fn activity_attribute_label(attribute: &str, locale: Locale) -> String {
    if let Some(definition) = catalog().widget(attribute) {
        return translations().t(locale, &definition.name_key).to_string();
    }
    let words: Vec<String> = attribute
        .split('_')
        .map(|part| {
            if part == "gps" {
                return "GPS".to_string();
            }
            let mut chars = part.chars();
            chars.next().map(|first| first.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
        })
        .collect();
    let default = words.join(" ");
    translations().t_or(locale, &format!("widgets.activityAttributes.{attribute}"), &default).to_string()
}

/// Widgets grouped by type, groups ordered by sidebar priority then name;
/// a widget without a name field takes its type's name.
pub fn group_widgets_for_sidebar(widgets: &[ConfigWidget], locale: Locale) -> Vec<ConfigWidget> {
    struct Group {
        name: String,
        priority: i64,
        widgets: Vec<ConfigWidget>,
    }
    let mut groups: Vec<(String, Group)> = Vec::new();
    for widget in widgets {
        let definition = catalog().require_widget(&widget.kind).expect("listed widgets are in the catalog");
        let type_name = widget_type_name(&widget.kind, locale);
        let index = groups.iter().position(|(kind, _)| *kind == widget.kind).unwrap_or_else(|| {
            groups.push((
                widget.kind.clone(),
                Group {
                    name: type_name.clone(),
                    priority: definition.sidebar_priority,
                    widgets: Vec::new(),
                },
            ));
            groups.len() - 1
        });
        let mut named = widget.clone();
        if definition.name_field.is_none() {
            named.name = type_name;
        }
        groups[index].1.widgets.push(named);
    }
    groups.sort_by(|(_, left), (_, right)| left.priority.cmp(&right.priority).then_with(|| compare_text(&left.name, &right.name)));
    groups.into_iter().flat_map(|(_, group)| group.widgets).collect()
}

/// A numeric activity column by its manifest name. The manifest names only numeric columns and
/// `sample_elevations`.
fn activity_series<'a>(activity: &'a ParsedActivity, field: &str) -> &'a [Option<f64>] {
    if field == "sample_elevations" {
        return &activity.sample_elevations;
    }
    let series: ActivitySeries = serde_json::from_value(Value::String(field.to_string())).unwrap_or_else(|_| panic!("altitude source {field} is an activity column"));
    activity.numeric_series(series)
}

/// The first altitude of the source's series, in meters.
pub fn starting_altitude(activity: Option<&ParsedActivity>, source: &AltitudeSource) -> Option<f64> {
    let activity = activity?;
    for series in &source.series {
        let values = activity_series(activity, &series.field);
        if values.is_empty() {
            continue;
        }
        if let Some(first) = values.iter().flatten().next() {
            return Some(*first);
        }
        if !series.require_value {
            return None;
        }
    }
    None
}

/// The activity's starting altitude in the
/// widget's unit, when the widget has an altitude source and no starting altitude of its own.
pub fn starting_altitude_placeholder(widget: &ConfigWidget, activity: Option<&ParsedActivity>) -> Option<f64> {
    let source = catalog().widget(&widget.kind)?.altitude_source.as_ref()?;
    if widget.data.get("starting_altitude").is_some_and(|value| !value.is_null()) {
        return None;
    }
    let meters = starting_altitude(activity, source)?;
    let unit = widget.data.get(&source.unit_field).and_then(Value::as_str).unwrap_or("m");
    Some(js_round(convert_altitude_value(meters, "m", unit)))
}

/// One option of the font select.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontOption {
    pub id: String,
    pub name: String,
}

impl From<&FontFamily> for FontOption {
    fn from(family: &FontFamily) -> Self {
        Self {
            id: family.id.clone(),
            name: family.name.clone(),
        }
    }
}

/// The bundled options (with an unavailable saved selection first) and the
/// system families whose names the bundled list does not already have.
pub fn font_select_options(value: &str, recommended: &[FontOption], system: &[FontOption]) -> (Vec<FontOption>, Vec<FontOption>) {
    let mut known = value.is_empty();
    let mut names = std::collections::HashSet::new();
    for font in recommended {
        known |= font.id == value;
        names.insert(font.name.as_str());
    }
    let mut filtered = Vec::new();
    for font in system {
        known |= font.id == value;
        if !names.contains(font.name.as_str()) {
            filtered.push(font.clone());
        }
    }
    let recommended_options = if known {
        recommended.to_vec()
    } else {
        let name = font_family_name(value);
        let mut options = vec![FontOption {
            id: value.to_string(),
            name: if name.is_empty() { "Custom font".into() } else { name },
        }];
        options.extend(recommended.iter().cloned());
        options
    };
    (recommended_options, filtered)
}

const FONT_KEYS: [&str; 5] = ["font", "label_font", "min_max_label_font", "font_text", "font_values"];

/// Every font id a value references, inactive variants included, in first-seen
/// order.
pub fn collect_font_ids(value: &Value) -> Vec<String> {
    fn walk(value: &Value, ids: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, item) in map {
                    match item {
                        Value::String(id) if FONT_KEYS.contains(&key.as_str()) => {
                            if !ids.contains(id) {
                                ids.push(id.clone());
                            }
                        }
                        Value::Object(_) | Value::Array(_) => walk(item, ids),
                        _ => {}
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, ids)),
            _ => {}
        }
    }
    let mut ids = Vec::new();
    walk(value, &mut ids);
    ids
}
