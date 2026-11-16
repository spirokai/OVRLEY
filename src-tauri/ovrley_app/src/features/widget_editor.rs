//! Port of `features/widget-editor`, Phase 3 part: `SidebarWidgetsTab` (the widget list and its
//! open item), `PositionSection`, `MetricWidgetEditor` with `TextDisplaySection`, and
//! `useWidgetManager`'s actions. The other editors and display sections arrive in Phase 5.
//!
//! The view model here is what the components read; `ovrley_ui` copies it into the `WidgetEditor`
//! global. Labels the locale files have are `@tr` in the markup; the ones built from the manifests
//! (display types, units) and those React writes as defaults only are translated here.

use crate::features::overlay_editor::interaction::{content_rect, resize_draft_patch, rotation_draft_patch};
use crate::i18n::{catalog as translations, Locale};
use crate::lib::fonts::{font_selection, font_select_options, FontOption};
use crate::lib::widget::{build_config_widgets, group_widgets_for_sidebar, starting_altitude_placeholder, ConfigWidget};
use crate::store::services::Services;
use crate::store::Store;
use ovrley_widgets::model::catalog::{catalog, IconRef, UnitOption};
use ovrley_widgets::model::document::{
    build_display_type_change_update, build_metric_unit_update, create_widget_defaults, js_round, merge_widget_data, number, resolve_active_widget_data,
    Collection, WidgetCreationOptions, WidgetSelection,
};
use serde_json::{Map, Value};

/// `SidebarWidgetsTab`'s state beyond the store, and the font catalog (D16).
#[derive(Debug, Default)]
pub struct WidgetEditorState {
    /// `useHeldValue(selectedWidgetId, isMarqueeSelecting)`'s memory.
    held_open_widget_id: Option<String>,
    pub fonts: FontCatalogState,
}

/// `useAvailableFonts`: the session's font catalog, loaded once on a worker.
#[derive(Debug, Default)]
pub struct FontCatalogState {
    requested: bool,
    pub recommended: Vec<FontOption>,
    pub system: Vec<FontOption>,
}

/// Reconcile step: the open item follows the selection, except while a marquee runs, when it keeps
/// the last value seen before.
pub fn reconcile_open_widget(store: &mut Store) {
    if !store.editor.is_marquee_selecting {
        store.widget_editor.held_open_widget_id = store.editor.selected_widget_id.clone();
    }
}

/// The accordion's open item (`useHeldValue`).
pub fn open_widget_id(store: &Store) -> Option<&str> {
    if store.editor.is_marquee_selecting {
        store.widget_editor.held_open_widget_id.as_deref()
    } else {
        store.editor.selected_widget_id.as_deref()
    }
}

/// `getFontCatalog`, once per session: a failure is logged and the select stays empty, as
/// `useAvailableFonts` warns and keeps its empty lists.
pub fn load_font_catalog(store: &mut Store, services: &mut Services) {
    if std::mem::replace(&mut store.widget_editor.fonts.requested, true) {
        return;
    }
    let font_dirs = services.paths.font_dirs.clone();
    services.workers.spawn("font-catalog", move || {
        let catalog = ovrley_core::fonts::font_catalog(&font_dirs);
        Box::new(move |store: &mut Store, _: &mut Services| match catalog {
            Ok(catalog) => {
                store.widget_editor.fonts.recommended = catalog.recommended_fonts.iter().map(FontOption::from).collect();
                store.widget_editor.fonts.system = catalog.system_fonts.iter().map(FontOption::from).collect();
            }
            Err(error) => log::warn!("Failed to load available fonts: {error}"),
        })
    });
}

// ── The view model ──────────────────────────────────────────────────────────────────────────────

/// A select or toggle-group option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionView {
    pub value: String,
    pub label: String,
}

impl OptionView {
    fn new(value: &str, label: &str) -> Self {
        Self { value: value.into(), label: label.into() }
    }
}

/// One row of the widget list.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetRowView {
    pub id: String,
    pub name: String,
    pub icon: IconRef,
}

/// `PositionSection` and the editor of the open widget (`WidgetInspector`).
#[derive(Clone, Debug, PartialEq)]
pub struct InspectorView {
    pub id: String,
    /// "Horizontal Anchor" for text values, else "Horizontal Position".
    pub horizontal_label: String,
    /// The number fields' text: whole numbers, as `NumberField` rounds them.
    pub x: String,
    pub y: String,
    /// The transparency slider, in percent.
    pub opacity: f64,
    /// `MetricWidgetEditor`, for metric widgets.
    pub metric: Option<MetricEditorView>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetricEditorView {
    /// The display type select's options; it shows only with more than one.
    pub display_types: Vec<OptionView>,
    pub display_type: String,
    /// `TextDisplaySection`, for the text display. The other displays arrive in Phase 5.
    pub text: Option<TextDisplayView>,
    /// "Altitude at start", for altitude widgets.
    pub altitude: Option<AltitudeView>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AltitudeView {
    /// Empty without a configured starting altitude.
    pub value: String,
    /// The activity's starting altitude in the widget's unit; empty without one.
    pub placeholder: String,
    pub unit: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextDisplayView {
    pub content_alignment: String,
    /// The alignment control's items, labelled for screen readers.
    pub alignments: Vec<OptionView>,
    pub font_size: f64,
    pub font: String,
    pub color: String,
    /// The decimals slider's value and maximum, for widgets with a decimal control.
    pub decimals: Option<(f64, f64)>,
    pub balance_format: Option<String>,
    pub show_full_distance: Option<bool>,
    pub coordinates: Option<CoordinatesView>,
    pub show_full_ascent: Option<bool>,
    pub icon: IconSectionView,
    /// `UnitsControlRow`, unless the type hides its units.
    pub units: Option<UnitsRowView>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CoordinatesView {
    pub format: String,
    pub formats: Vec<OptionView>,
    pub coordinate_format: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IconSectionView {
    pub show: bool,
    pub color: String,
    pub size: f64,
    pub offset_x: String,
    pub offset_y: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnitsRowView {
    /// "Units" when the type offers several, else "Unit".
    pub supports_selection: bool,
    pub show_toggle: bool,
    pub checked: bool,
    /// The unit colour field shows when the widget has a unit colour.
    pub color: Option<String>,
    pub unit: String,
    /// The unit select's options; empty hides it.
    pub options: Vec<OptionView>,
}

/// What `SidebarWidgetsTab` shows.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarView {
    pub rows: Vec<WidgetRowView>,
    pub open_widget_id: Option<String>,
    pub inspector: Option<InspectorView>,
}

fn text(data: &Map<String, Value>, key: &str) -> String {
    data.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn float(data: &Map<String, Value>, key: &str) -> f64 {
    data.get(key).and_then(Value::as_f64).unwrap_or_default()
}

/// `NumberField`'s shown value: empty for an absent value, else `Math.round(value)`.
fn whole_number_text(value: Option<&Value>) -> String {
    match value.and_then(Value::as_f64) {
        // `+ 0.0` turns JavaScript's `-0` into `0`, as `String(-0)` does.
        Some(number) => format!("{}", js_round(number) + 0.0),
        None => String::new(),
    }
}

/// A unit option's label: its own label, else its translated key.
fn unit_label(option: &UnitOption, locale: Locale) -> String {
    match (&option.label, &option.label_key) {
        (Some(label), _) => label.clone(),
        (None, Some(key)) => translations().t_or(locale, key, option.default_label.as_deref().unwrap_or_default()).to_string(),
        (None, None) => panic!("unit option {} has no label", option.value),
    }
}

/// The sidebar's widgets (`useWidgetManager`'s `widgets`): document widgets with their live drafts
/// applied, named and ordered for the sidebar.
fn sidebar_widgets(store: &Store) -> Vec<ConfigWidget> {
    let mut widgets = build_config_widgets(&store.editor.config);
    for widget in &mut widgets {
        // `applyWidgetDrafts`: a draft's widget is merged and resolved; others stay as stored.
        if let Some(draft) = store.drafts.widget_draft(&widget.id) {
            widget.data = resolve_active_widget_data(&merge_widget_data(Some(&widget.data), draft));
        }
    }
    group_widgets_for_sidebar(&widgets, store.i18n.language)
}

fn text_display_view(store: &Store, widget: &ConfigWidget) -> TextDisplayView {
    let locale = store.i18n.language;
    let theme = store.theme.as_ref().expect("ovrley_ui reads the theme before the first dispatch");
    let definition = catalog().require_widget(&widget.kind).expect("listed widgets are in the catalog");
    let data = &widget.data;
    let units_mode = definition.units_mode.as_deref().unwrap_or("hidden");
    let supports_selection = definition.supported_display_units.len() > 1;
    let is_coordinates = widget.kind == "gps_coordinates";
    let alignment = |value: &str, key: &str, default: &str| OptionView::new(value, translations().t_or(locale, key, default));
    TextDisplayView {
        content_alignment: text(data, "content_alignment"),
        alignments: vec![
            alignment("left", "widget-editor.alignLeft", "Align left"),
            alignment("center", "widget-editor.alignCenter", "Align center"),
            alignment("right", "widget-editor.alignRight", "Align right"),
        ],
        font_size: float(data, "font_size"),
        font: text(data, "font"),
        color: data.get("color").and_then(Value::as_str).filter(|color| !color.is_empty()).unwrap_or(&theme.ice).to_string(),
        decimals: definition.has_decimal_control.then(|| (float(data, "decimals"), f64::from(definition.max_decimals))),
        balance_format: (definition.formatter.as_deref() == Some("balance")).then(|| text(data, "balance_format")),
        show_full_distance: (widget.kind == "distance").then(|| data.get("show_full_distance").and_then(Value::as_bool).unwrap_or(true)),
        coordinates: is_coordinates.then(|| CoordinatesView {
            format: text(data, "display_unit"),
            formats: definition.supported_display_units.iter().map(|option| OptionView { value: option.value.clone(), label: unit_label(option, locale) }).collect(),
            coordinate_format: text(data, "coordinate_format"),
        }),
        show_full_ascent: (widget.kind == "total_ascent").then(|| data.get("show_full_ascent").and_then(Value::as_bool).unwrap_or_default()),
        icon: IconSectionView {
            show: data.get("show_icon").and_then(Value::as_bool).unwrap_or_default(),
            color: data.get("icon_color").and_then(Value::as_str).filter(|color| !color.is_empty()).unwrap_or(&theme.aqua).to_string(),
            size: float(data, "icon_size"),
            offset_x: whole_number_text(data.get("icon_offset_x")),
            offset_y: whole_number_text(data.get("icon_offset_y")),
        },
        units: (units_mode != "hidden").then(|| UnitsRowView {
            supports_selection,
            show_toggle: !is_coordinates,
            checked: data.get("show_units").and_then(Value::as_bool).unwrap_or_default(),
            color: data.get("unit_color").map(|color| color.as_str().unwrap_or_default().to_string()),
            unit: text(data, "display_unit"),
            options: if !is_coordinates && supports_selection {
                definition.supported_display_units.iter().map(|option| OptionView { value: option.value.clone(), label: unit_label(option, locale) }).collect()
            } else {
                Vec::new()
            },
        }),
    }
}

fn metric_editor_view(store: &Store, widget: &ConfigWidget) -> Option<MetricEditorView> {
    let catalog = catalog();
    let definition = catalog.require_widget(&widget.kind).expect("listed widgets are in the catalog");
    if !definition.metric {
        return None;
    }
    let locale = store.i18n.language;
    let display_type = text(&widget.data, "display_type");
    let activity = store.media.parsed_activity.as_ref().map(|imported| imported.activity.as_ref());
    Some(MetricEditorView {
        display_types: definition
            .display_types
            .iter()
            .map(|value| OptionView::new(value, translations().t(locale, definition.display_type_label_key(catalog, value))))
            .collect(),
        text: (display_type == "text").then(|| text_display_view(store, widget)),
        display_type,
        altitude: (widget.kind == "altitude").then(|| AltitudeView {
            value: whole_number_text(widget.data.get("starting_altitude").filter(|value| !value.is_null())),
            placeholder: starting_altitude_placeholder(widget, activity).map(|value| format!("{value}")).unwrap_or_default(),
            unit: text(&widget.data, "display_unit"),
        }),
    })
}

fn inspector_view(store: &Store, committed: &ConfigWidget) -> InspectorView {
    let locale = store.i18n.language;
    // `useGestureWidget`: only the open inspector follows the live canvas gesture.
    let mut widget = committed.clone();
    if let Some(patch) = store.drafts.gesture_patch(&widget.id) {
        widget.data = resolve_active_widget_data(&merge_widget_data(Some(&widget.data), patch));
    }
    let data = &widget.data;
    let uses_horizontal_anchor = widget.collection == Collection::Values && text(data, "display_type") == "text" && widget.kind != "gradient" && widget.kind != "lap_timer";
    InspectorView {
        id: widget.id.clone(),
        horizontal_label: if uses_horizontal_anchor {
            translations().t_or(locale, "widget-editor.horizontalAnchor", "Horizontal Anchor").to_string()
        } else {
            translations().t(locale, "widget-editor.horizontalPosition").to_string()
        },
        x: whole_number_text(data.get("x")),
        y: whole_number_text(data.get("y")),
        opacity: js_round(float(data, "opacity") * 100.0),
        metric: metric_editor_view(store, &widget),
    }
}

/// `SidebarWidgetsTab`'s view: the rows, the open item and its inspector.
pub fn sidebar_view(store: &Store) -> SidebarView {
    let widgets = sidebar_widgets(store);
    let open = open_widget_id(store).map(str::to_string);
    let inspector = open.as_deref().and_then(|id| widgets.iter().find(|widget| widget.id == id)).map(|widget| inspector_view(store, widget));
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

/// The font select's two lists for the open widget (`getFontSelectOptions`).
pub fn font_options(store: &Store, value: &str) -> (Vec<FontOption>, Vec<FontOption>) {
    font_select_options(value, &store.widget_editor.fonts.recommended, &store.widget_editor.fonts.system)
}

// ── `useWidgetManager`'s actions ────────────────────────────────────────────────────────────────

/// `CONTENT_ALIGNMENT_FACTORS`.
fn alignment_factor(alignment: &str) -> Result<f64, String> {
    match alignment {
        "left" => Ok(0.0),
        "center" => Ok(0.5),
        "right" => Ok(1.0),
        other => Err(format!("Unsupported content alignment: {other}")),
    }
}

/// `updateWidgetData`: applies a patch to a widget as one undo step. A content alignment change
/// keeps the content in place by shifting x by the presented content width. (A map switching to the
/// fixed camera fits its route; that rule arrives with the map editor in Phase 5.)
pub fn update_widget_data(store: &mut Store, widget_id: &str, updates: Map<String, Value>) -> Result<(), String> {
    let data = store.editor.config.widget(widget_id)?.clone();
    let mut updates = updates;
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

/// `updateWidgetDraft`: a live inspector edit, outside the history. A rotation goes through the
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

/// `commitWidgetDraft`: the widget's draft becomes one undo step.
pub fn commit_widget_draft(store: &mut Store, widget_id: &str) -> Result<(), String> {
    if let Some(draft) = store.drafts.widget_draft(widget_id).cloned() {
        store.editor.edit_config(|config| config.update_widgets(&[(widget_id.to_string(), draft)]))?;
    }
    store.drafts.clear_widget_drafts(&[widget_id.to_string()]);
    Ok(())
}

/// `setNumericField`'s options.
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

/// `Number(text)` for an editor input.
fn js_number(text: &str) -> f64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    trimmed.parse().unwrap_or(f64::NAN)
}

/// `parseInteger`.
fn parse_integer(text: &str, fallback: f64) -> Result<f64, String> {
    if text.is_empty() {
        return Ok(fallback);
    }
    let parsed = js_number(text);
    if !parsed.is_finite() {
        return Err(format!("Invalid integer input: {text}"));
    }
    Ok(parsed.trunc())
}

/// `setNumericField`: parses an input, clamps it to its range and updates the widget.
pub fn set_numeric_field(store: &mut Store, widget_id: &str, key: &str, raw: &str, options: NumericFieldOptions) -> Result<(), String> {
    let mut patch = Map::new();
    if options.optional && raw.is_empty() {
        patch.insert(key.into(), Value::Null);
        return update_widget_data(store, widget_id, patch);
    }
    let mut parsed = if options.integer { parse_integer(raw, options.fallback)? } else { js_number(raw) };
    if options.round {
        parsed = js_round(js_number(raw));
    }
    if !parsed.is_finite() {
        return Err(format!("Invalid numeric value for {key}"));
    }
    let clamped = parsed.max(options.min.unwrap_or(parsed)).min(options.max.unwrap_or(parsed));
    patch.insert(key.into(), number(clamped));
    update_widget_data(store, widget_id, patch)
}

/// `deleteWidget`.
pub fn delete_widget(store: &mut Store, widget_id: &str) -> Result<(), String> {
    store.editor.config.widget(widget_id)?;
    let ids = [widget_id.to_string()];
    store.editor.edit_config(|config| config.delete_widgets(&ids));
    Ok(())
}

/// The manifest's `resetSelection` as a drawer selection of `kind`.
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

/// `resetWidget`: replaces the widget's data with fresh defaults for its type.
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

/// `handleDisplayTypeChange`.
pub fn change_display_type(store: &mut Store, widget_id: &str, display_type: &str) -> Result<(), String> {
    let data = store.editor.config.widget(widget_id)?;
    let patch = build_display_type_change_update(data, display_type, store.editor.unit_system);
    update_widget_data(store, widget_id, patch)
}

/// `TextDisplaySection`'s `handleUnitChange`: altitude widgets convert their starting altitude.
pub fn change_metric_unit(store: &mut Store, widget_id: &str, unit: &str) -> Result<(), String> {
    let (collection, _) = store.editor.config.find_widget(widget_id)?;
    let data = store.editor.config.widget(widget_id)?;
    let kind = collection.widget_type(data)?;
    let patch = build_metric_unit_update(&kind, data.get("starting_altitude").unwrap_or(&Value::Null), &text(data, "display_unit"), unit);
    update_widget_data(store, widget_id, patch)
}

/// `changeFont`: `font` and its `font_family`.
pub fn change_font(store: &mut Store, widget_id: &str, font: &str) -> Result<(), String> {
    update_widget_data(store, widget_id, font_selection(font))
}

/// The MCP form of `updateWidgetData` (Q2): a field patch, checked against the type's fields, then
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
    let previous = std::rc::Rc::clone(&store.editor.config);
    let had_error = store.preview.error.is_some();
    update_widget_data(store, widget_id, fields)?;
    let durable = ovrley_core::normalize::template::create_durable_template_state(&store.editor.config, Some(&store.template.global_defaults));
    let rendered = durable.map_err(|error| error.to_string()).and_then(|_| {
        crate::features::widget_preview::present_pending_frame(store, services);
        match &store.preview.error {
            Some(error) if !had_error => Err(error.clone()),
            _ => Ok(()),
        }
    });
    if let Err(error) = rendered {
        store.editor.replace_config(previous);
        crate::features::widget_preview::present_pending_frame(store, services);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::undo_redo::{undo, HistoryMode};
    use crate::features::widget_preview::preview_frame;
    use crate::lib::template::ThemeColors;
    use crate::store::services::fakes::{fake_services, Fakes};
    use crate::store::Dispatcher;
    use ovrley_core::normalize::template::template_manifest;
    use ovrley_widgets::model::document::{OverlayConfig, UnitSystem};
    use serde_json::json;

    fn defaults(kind: &str, display_type: Option<&str>) -> Map<String, Value> {
        let selection = WidgetSelection { kind: kind.into(), display_type: display_type.map(str::to_string), ..WidgetSelection::default() };
        create_widget_defaults(&selection, &template_manifest().globals, &WidgetCreationOptions::default()).unwrap()
    }

    fn value(kind: &str, id: &str, x: f64) -> Value {
        let mut data = defaults(kind, None);
        data.insert("id".into(), json!(id));
        data.insert("x".into(), json!(x));
        Value::Object(data)
    }

    /// A dispatcher over `values`, with the preview's frames run until its fonts are ready.
    fn editor(values: Vec<Value>) -> (Dispatcher, Fakes) {
        let (parts, fakes) = fake_services(json!({}));
        let mut store = Store::default();
        store.theme = Some(ThemeColors { ice: "#86aaaa".into(), aqua: "#40e0d0".into() });
        let dispatcher = Dispatcher::new(store, parts);
        dispatcher.dispatch(move |store, _| {
            store.history.set_mode(HistoryMode::Reset);
            store.editor.set_config(OverlayConfig::from_value(&json!({"scene": {"width": 1000, "height": 500}, "values": values})).unwrap());
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

    fn patch(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    fn widget<R>(dispatcher: &Dispatcher, id: &str, read: impl FnOnce(&Map<String, Value>) -> R) -> R {
        dispatcher.with_store(|store| read(store.editor.config.widget(id).unwrap()))
    }

    fn inspector(dispatcher: &Dispatcher, id: &'static str) -> InspectorView {
        dispatcher.dispatch(move |store, _| store.editor.set_selected_widget_id(Some(id)));
        dispatcher.with_store(|store| sidebar_view(store).inspector.unwrap())
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
        assert_eq!(inspector(&dispatcher, "speed-0").metric.unwrap().text.unwrap().font_size, 60.375, "the inspector shows the draft");
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
        let text = |id: &'static str| inspector(&dispatcher, id).metric.unwrap().text.unwrap();
        for kind in ["speed", "g_force", "distance", "stride_length", "torque", "vertical_speed", "vertical_oscillation"] {
            assert!(text(kind).decimals.is_some(), "{kind} has a decimal control");
        }
        for kind in ["pace", "air_pressure", "ground_contact_time", "stroke_rate", "left_right_balance"] {
            assert!(text(kind).decimals.is_none(), "{kind} has no decimal control");
        }
        assert_eq!(text("speed").decimals, Some((0.0, 1.0)), "speed's slider runs 0 to 1");
        assert_eq!(text("distance").show_full_distance, Some(true), "full distance defaults to on");
        assert_eq!(text("left_right_balance").balance_format.as_deref(), Some("percent_label"));
        assert_eq!(text("speed").balance_format, None);
        assert!(text("total_ascent").show_full_ascent.is_some());

        let coordinates = text("gps_coordinates");
        let formats: Vec<_> = coordinates.coordinates.as_ref().unwrap().formats.iter().map(|option| option.label.as_str()).collect();
        assert_eq!(formats, ["Latitude", "Longitude", "Both"]);
        let units = coordinates.units.unwrap();
        assert!(!units.show_toggle && units.options.is_empty(), "coordinates have no units toggle or unit select");
        let speed_units = text("speed").units.unwrap();
        assert!(speed_units.supports_selection && speed_units.show_toggle);
        assert_eq!(speed_units.options[0], OptionView::new("kmh", "km/h"));

        let speed = inspector(&dispatcher, "speed").metric.unwrap();
        assert!(speed.display_types.len() > 1, "speed offers several displays");
        assert_eq!(inspector(&dispatcher, "speed").horizontal_label, "Horizontal Anchor");
    }

    #[test]
    fn altitude_units_convert_calibration_and_show_the_activity_start() {
        let (dispatcher, _) = editor(vec![value("altitude", "alt", 0.0)]);
        dispatcher.dispatch(|store, _| change_metric_unit(store, "alt", "ft").unwrap());
        widget(&dispatcher, "alt", |data| assert_eq!((data["display_unit"].clone(), data["starting_altitude"].clone()), (json!("ft"), Value::Null)));
        dispatcher.dispatch(|store, _| change_metric_unit(store, "alt", "m").unwrap());
        dispatcher.dispatch(|store, _| update_widget_data(store, "alt", patch(json!({"starting_altitude": 100}))).unwrap());
        dispatcher.dispatch(|store, _| change_metric_unit(store, "alt", "ft").unwrap());
        assert_eq!(widget(&dispatcher, "alt", |data| data["starting_altitude"].clone()), json!(328));
        let altitude = inspector(&dispatcher, "alt").metric.unwrap().altitude.unwrap();
        assert_eq!((altitude.value.as_str(), altitude.unit.as_str()), ("328", "ft"));
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
        dispatcher.dispatch(|store, _| change_display_type(store, "a", "linear").unwrap());
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
}
