//! The widget editor feature: its projection into the `WidgetEditor` global and its callbacks.

use crate::hooks::app_store_selectors::{replace_if_changed, set_if_changed};
use crate::{AppWindow, Choice, ChoiceKind, Icons, InspectorView, TextDisplayView, WidgetEditor, WidgetRowView};
use ovrley_app::features::widget_editor::{
    self as editor, font_options, open_widget_id, sidebar_view, NumericFieldOptions, OptionView, SidebarView,
};
use ovrley_app::lib::fonts::FontOption;
use ovrley_app::store::{Dispatcher, Store};
use ovrley_widgets::model::catalog::IconRef;
use serde_json::{Map, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;

fn choices(options: &[OptionView]) -> ModelRc<Choice> {
    let items: Vec<Choice> = options
        .iter()
        .map(|option| Choice { kind: ChoiceKind::Item, value: option.value.as_str().into(), label: option.label.as_str().into(), ..Default::default() })
        .collect();
    ModelRc::new(VecModel::from(items))
}

fn font_choices(fonts: &[FontOption]) -> Vec<Choice> {
    fonts.iter().map(|font| Choice { kind: ChoiceKind::Item, value: font.id.as_str().into(), label: font.name.as_str().into(), ..Default::default() }).collect()
}

fn text_display(view: &editor::TextDisplayView) -> TextDisplayView {
    let mut text = TextDisplayView {
        content_alignment: view.content_alignment.as_str().into(),
        alignments: choices(&view.alignments),
        font_size: view.font_size as f32,
        font: view.font.as_str().into(),
        color: view.color.as_str().into(),
        show_icon: view.icon.show,
        icon_color: view.icon.color.as_str().into(),
        icon_size: view.icon.size as f32,
        icon_offset_x: view.icon.offset_x.as_str().into(),
        icon_offset_y: view.icon.offset_y.as_str().into(),
        ..Default::default()
    };
    if let Some((decimals, max)) = view.decimals {
        (text.has_decimals, text.decimals, text.max_decimals) = (true, decimals as f32, max as f32);
    }
    if let Some(format) = &view.balance_format {
        (text.has_balance_format, text.balance_format) = (true, format.as_str().into());
    }
    if let Some(full) = view.show_full_distance {
        (text.has_full_distance, text.show_full_distance) = (true, full);
    }
    if let Some(coordinates) = &view.coordinates {
        text.has_coordinates = true;
        text.coordinate_unit = coordinates.format.as_str().into();
        text.coordinate_units = choices(&coordinates.formats);
        text.coordinate_format = coordinates.coordinate_format.as_str().into();
    }
    if let Some(full) = view.show_full_ascent {
        (text.has_full_ascent, text.show_full_ascent) = (true, full);
    }
    if let Some(units) = &view.units {
        text.has_units = true;
        text.units_supports_selection = units.supports_selection;
        text.units_show_toggle = units.show_toggle;
        text.units_checked = units.checked;
        if let Some(color) = &units.color {
            (text.has_unit_color, text.unit_color) = (true, color.as_str().into());
        }
        text.unit = units.unit.as_str().into();
        text.units = choices(&units.options);
    }
    text
}

fn inspector(view: &editor::InspectorView) -> InspectorView {
    let mut inspector = InspectorView {
        id: view.id.as_str().into(),
        horizontal_label: view.horizontal_label.as_str().into(),
        x: view.x.as_str().into(),
        y: view.y.as_str().into(),
        opacity: view.opacity as f32,
        ..Default::default()
    };
    if let Some(metric) = &view.metric {
        inspector.is_metric = true;
        inspector.display_types = choices(&metric.display_types);
        inspector.display_type = metric.display_type.as_str().into();
        if let Some(text) = &metric.text {
            (inspector.has_text, inspector.text) = (true, text_display(text));
        }
        if let Some(altitude) = &metric.altitude {
            inspector.has_altitude = true;
            inspector.altitude = altitude.value.as_str().into();
            inspector.altitude_placeholder = altitude.placeholder.as_str().into();
            inspector.altitude_unit = altitude.unit.as_str().into();
        }
    }
    inspector
}

/// The projection's memory: the bundled icon images, decoded once each, and the last view written,
/// so a dispatch that changes nothing here writes nothing.
#[derive(Default)]
pub struct Projection {
    icon_images: RefCell<HashMap<String, slint::Image>>,
    last: RefCell<Option<(SidebarView, Vec<FontOption>, Vec<FontOption>)>>,
}

impl Projection {
    /// A manifest icon: a lucide glyph from the `Icons` global, or a bundled SVG.
    fn icon(&self, window: &AppWindow, icon: &IconRef) -> (SharedString, slint::Image) {
        if let Some(asset_file) = &icon.asset_file {
            let image = self
                .icon_images
                .borrow_mut()
                .entry(asset_file.clone())
                .or_insert_with(|| {
                    slint::Image::load_from_svg_data(ovrley_widgets::render::icons::icon_asset_markup(asset_file).as_bytes())
                        .unwrap_or_else(|error| panic!("the bundled icon {asset_file} is not a valid SVG: {error:?}"))
                })
                .clone();
            return (SharedString::new(), image);
        }
        let icons = window.global::<Icons>();
        let glyph = match icon.name.as_deref() {
            Some("Image") => icons.get_image(),
            Some("Navigation") => icons.get_navigation(),
            Some("Presentation") => icons.get_presentation(),
            Some("Timer") => icons.get_timer(),
            Some("Type") => icons.get_type(),
            other => panic!("unsupported manifest icon {other:?}"),
        };
        (glyph, slint::Image::default())
    }

    pub fn project(&self, window: &AppWindow, store: &Store) {
        let view = sidebar_view(store);
        let font = view.inspector.as_ref().and_then(|inspector| inspector.metric.as_ref()?.text.as_ref()).map(|text| text.font.clone()).unwrap_or_default();
        let (recommended, system) = font_options(store, &font);
        let next = (view, recommended, system);
        if self.last.borrow().as_ref() == Some(&next) {
            return;
        }
        let (view, recommended, system) = &next;
        let global = window.global::<WidgetEditor>();
        let rows: Vec<WidgetRowView> = view
            .rows
            .iter()
            .map(|row| {
                let (glyph, image) = self.icon(window, &row.icon);
                WidgetRowView { id: row.id.as_str().into(), name: row.name.as_str().into(), glyph, image }
            })
            .collect();
        replace_if_changed(global.get_rows(), rows, |model| global.set_rows(model));
        set_if_changed(global.get_open_id(), view.open_widget_id.as_deref().unwrap_or_default().into(), |value| global.set_open_id(value));
        set_if_changed(global.get_has_inspector(), view.inspector.is_some(), |value| global.set_has_inspector(value));
        if let Some(view) = &view.inspector {
            global.set_inspector(inspector(view));
        }
        replace_if_changed(global.get_recommended_fonts(), font_choices(recommended), |model| global.set_recommended_fonts(model));
        replace_if_changed(global.get_system_fonts(), font_choices(system), |model| global.set_system_fonts(model));
        *self.last.borrow_mut() = Some(next);
    }
}

/// Runs an action against the open widget; its errors are bugs in what the inspector offered.
fn on_open_widget(dispatcher: &Dispatcher, action: impl FnOnce(&mut Store, &str) -> Result<(), String> + 'static) {
    dispatcher.handle().dispatch(move |store, _| {
        let Some(widget_id) = open_widget_id(store).map(str::to_string) else { return };
        if let Err(error) = action(store, &widget_id) {
            log::error!("widget editor: {error}");
        }
    });
}

fn one_field(key: &str, value: Value) -> Map<String, Value> {
    let mut patch = Map::new();
    patch.insert(key.into(), value);
    patch
}

/// The number inputs' `setNumericField` options.
fn numeric_options(key: &str) -> NumericFieldOptions {
    match key {
        "x" | "y" | "icon_offset_x" | "icon_offset_y" => NumericFieldOptions::default(),
        "starting_altitude" => NumericFieldOptions { optional: true, round: true, ..NumericFieldOptions::default() },
        other => panic!("the inspector has no number field {other}"),
    }
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let global = window.global::<WidgetEditor>();

    let dispatch = dispatcher.handle();
    global.on_toggle(move |id| {
        let id = id.to_string();
        dispatch.dispatch(move |store, _| {
            let next = (open_widget_id(store) != Some(id.as_str())).then_some(id);
            store.editor.set_selected_widget_id(next.as_deref());
        });
    });
    let dispatch = dispatcher.handle();
    global.on_delete_widget(move |id| {
        let id = id.to_string();
        dispatch.dispatch(move |store, _| {
            if let Err(error) = editor::delete_widget(store, &id) {
                log::error!("widget editor: {error}");
            }
        });
    });
    let dispatch = dispatcher.handle();
    global.on_reset_widget(move |id| {
        let id = id.to_string();
        dispatch.dispatch(move |store, _| {
            if let Err(error) = editor::reset_widget(store, &id) {
                log::error!("widget editor: {error}");
            }
        });
    });

    let handle = dispatcher.clone();
    global.on_set_number(move |key, raw| {
        let (key, raw) = (key.to_string(), raw.to_string());
        on_open_widget(&handle, move |store, id| editor::set_numeric_field(store, id, &key, &raw, numeric_options(&key)));
    });
    let handle = dispatcher.clone();
    global.on_draft(move |key, value| {
        let key = key.to_string();
        // The transparency slider runs in percent.
        let value = if key == "opacity" { f64::from(value) / 100.0 } else { f64::from(value) };
        on_open_widget(&handle, move |store, id| editor::update_widget_draft(store, id, one_field(&key, ovrley_widgets::model::document::number(value))));
    });
    let handle = dispatcher.clone();
    global.on_commit_draft(move || on_open_widget(&handle, editor::commit_widget_draft));
    let handle = dispatcher.clone();
    global.on_set_text(move |key, value| {
        let patch = one_field(&key, Value::String(value.to_string()));
        on_open_widget(&handle, move |store, id| editor::update_widget_data(store, id, patch));
    });
    let handle = dispatcher.clone();
    global.on_set_flag(move |key, value| {
        let patch = one_field(&key, Value::Bool(value));
        on_open_widget(&handle, move |store, id| editor::update_widget_data(store, id, patch));
    });
    let handle = dispatcher.clone();
    global.on_clear_field(move |key| {
        let patch = one_field(&key, Value::Null);
        on_open_widget(&handle, move |store, id| editor::update_widget_data(store, id, patch));
    });
    let handle = dispatcher.clone();
    global.on_change_font(move |font| {
        let font = font.to_string();
        on_open_widget(&handle, move |store, id| editor::change_font(store, id, &font));
    });
    let handle = dispatcher.clone();
    global.on_change_display_type(move |value| {
        let value = value.to_string();
        on_open_widget(&handle, move |store, id| editor::change_display_type(store, id, &value));
    });
    let handle = dispatcher.clone();
    global.on_change_unit(move |value| {
        let value = value.to_string();
        on_open_widget(&handle, move |store, id| editor::change_metric_unit(store, id, &value));
    });
}
