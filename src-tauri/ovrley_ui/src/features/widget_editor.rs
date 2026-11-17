//! The widget editor feature: its projection into the `WidgetEditor` global and its callbacks.
//! The inspector is the open widget's description (D1), synced row by row by key.

use crate::hooks::app_store_selectors::{replace_if_changed, set_if_changed, sync_keyed};
use crate::hooks::icons::{glyph, IconCache};
use crate::{
    AppWindow, Choice, ChoiceKind, GradientStop, Icons, InspectorCell, InspectorCellKind, InspectorRow, PanelPart, ShapeToggleView, SwitchRowStyle, WidgetEditor,
    WidgetRowView,
};
use ovrley_app::features::widget_editor::blocks::{self, Block, BlockContent, Cell, CellKind, OptionView};
use ovrley_app::features::widget_editor::rules::Input;
use ovrley_app::features::widget_editor::{self as editor, font_options, open_widget_id, sidebar_view, SidebarView};
use ovrley_app::lib::fonts::FontOption;
use ovrley_app::store::services::Services;
use ovrley_app::store::{Dispatcher, Store};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

pub fn choices(options: &[OptionView]) -> ModelRc<Choice> {
    let items: Vec<Choice> = options
        .iter()
        .map(|option| Choice { kind: ChoiceKind::Item, value: option.value.as_str().into(), label: option.label.as_str().into(), ..Default::default() })
        .collect();
    ModelRc::new(VecModel::from(items))
}

fn font_choices(fonts: &[FontOption]) -> Vec<Choice> {
    fonts.iter().map(|font| Choice { kind: ChoiceKind::Item, value: font.id.as_str().into(), label: font.name.as_str().into(), ..Default::default() }).collect()
}

/// A switch row's style into the cell: the kind, and a `label` row's padding.
fn switch_style(out: &mut InspectorCell, style: Option<blocks::SwitchRowStyle>) {
    out.style = match style {
        None => SwitchRowStyle::None,
        Some(blocks::SwitchRowStyle::Toggle) => SwitchRowStyle::Toggle,
        Some(blocks::SwitchRowStyle::Tall) => SwitchRowStyle::Tall,
        Some(blocks::SwitchRowStyle::Label { padding: [top, right, bottom, left], label_top }) => {
            (out.padding_top, out.padding_right, out.padding_bottom, out.padding_left, out.label_top) = (top, right, bottom, left, label_top);
            SwitchRowStyle::Label
        }
    };
}

/// A cell as the `InspectorCell` struct: which fields each kind fills.
pub fn cell(icons: &Icons<'_>, cell: &Cell) -> InspectorCell {
    let mut out = InspectorCell {
        id: cell.id.as_str().into(),
        label: cell.label.as_str().into(),
        disabled: cell.disabled,
        tooltip: cell.tooltip.as_str().into(),
        end: cell.end,
        ..Default::default()
    };
    out.kind = match &cell.kind {
        CellKind::Slider { editable, value, min, max, step, scale, readout, suffix } => {
            out.editable = *editable;
            (out.value, out.minimum, out.maximum, out.step, out.scale) = (*value as f32, *min as f32, *max as f32, *step as f32, *scale as f32);
            (out.readout, out.suffix) = (readout.as_str().into(), suffix.as_str().into());
            InspectorCellKind::Slider
        }
        CellKind::Color { value } => {
            out.text = value.as_str().into();
            InspectorCellKind::Color
        }
        CellKind::Select { value, options, resettable } => {
            (out.text, out.options, out.resettable) = (value.as_str().into(), choices(options), *resettable);
            InspectorCellKind::Select
        }
        CellKind::Number { value, placeholder, suffix, resettable } => {
            (out.text, out.text2, out.suffix, out.resettable) = (value.as_str().into(), placeholder.as_str().into(), suffix.as_str().into(), *resettable);
            InspectorCellKind::Number
        }
        CellKind::Text { value, placeholder } => {
            (out.text, out.text2) = (value.as_str().into(), placeholder.as_str().into());
            InspectorCellKind::Text
        }
        CellKind::Switch { checked, label_style } => {
            out.checked = *checked;
            switch_style(&mut out, *label_style);
            InspectorCellKind::Switch
        }
        CellKind::SwitchRow { checked, style } => {
            out.checked = *checked;
            switch_style(&mut out, Some(*style));
            InspectorCellKind::SwitchRow
        }
        CellKind::Font { value, recommended, small_label } => {
            (out.text, out.options, out.small_label) = (value.as_str().into(), ModelRc::new(VecModel::from(font_choices(recommended))), *small_label);
            InspectorCellKind::Font
        }
        CellKind::ToggleGroup { value, options, labelled, multiple, pressed } => {
            let items: Vec<Choice> = options
                .iter()
                .map(|option| Choice {
                    kind: ChoiceKind::Item,
                    value: option.value.as_str().into(),
                    label: option.label.as_str().into(),
                    pressed: pressed.contains(&option.value),
                    ..Default::default()
                })
                .collect();
            (out.text, out.options, out.labelled, out.multiple) = (value.as_str().into(), ModelRc::new(VecModel::from(items)), *labelled, *multiple);
            InspectorCellKind::ToggleGroup
        }
        CellKind::Alignment { value, options } => {
            (out.text, out.options) = (value.as_str().into(), choices(options));
            InspectorCellKind::Alignment
        }
        CellKind::Italic { pressed } => {
            out.checked = *pressed;
            InspectorCellKind::Italic
        }
        CellKind::IconButton { icon, outline } => {
            (out.icon, out.outline) = (glyph(icons, icon), *outline);
            InspectorCellKind::IconButton
        }
        CellKind::Gradient { stops } => {
            let stops: Vec<GradientStop> = stops.iter().map(|stop| GradientStop { offset: stop.offset as f32, color: stop.color.as_str().into() }).collect();
            out.stops = ModelRc::new(VecModel::from(stops));
            InspectorCellKind::Gradient
        }
        CellKind::ShapeGrid { toggles } => {
            let toggles: Vec<ShapeToggleView> = toggles
                .iter()
                .map(|toggle| ShapeToggleView { key: toggle.key.as_str().into(), edge: toggle.edge, active: toggle.active, label: toggle.label.as_str().into() })
                .collect();
            out.toggles = ModelRc::new(VecModel::from(toggles));
            InspectorCellKind::ShapeGrid
        }
        CellKind::ChartSeries { metric, options, color, first, last, only } => {
            (out.text, out.options, out.text2) = (metric.as_str().into(), choices(options), color.as_str().into());
            (out.first, out.last, out.only) = (*first, *last, *only);
            InspectorCellKind::ChartSeries
        }
        CellKind::GForceAxis { axis, inverted, invert_label } => {
            (out.text, out.checked, out.text2) = (axis.as_str().into(), *inverted, invert_label.as_str().into());
            InspectorCellKind::GforceAxis
        }
        CellKind::RasterPicker { path, placeholder, button_label, loading, error } => {
            (out.text, out.text2, out.suffix, out.busy) = (path.as_str().into(), placeholder.as_str().into(), button_label.as_str().into(), *loading);
            out.readout = error.as_str().into();
            InspectorCellKind::RasterPicker
        }
        CellKind::Alert { text } => {
            out.text = text.as_str().into();
            InspectorCellKind::Alert
        }
        CellKind::WindowSeconds { text, invalid } => {
            (out.text, out.invalid) = (text.as_str().into(), *invalid);
            InspectorCellKind::WindowSeconds
        }
        CellKind::Locked { text } => {
            out.text = text.as_str().into();
            InspectorCellKind::Locked
        }
        CellKind::Note { text } => {
            out.text = text.as_str().into();
            InspectorCellKind::Note
        }
        CellKind::Empty => InspectorCellKind::Empty,
        CellKind::Tabs { value, options } => {
            (out.text, out.options) = (value.as_str().into(), choices(options));
            InspectorCellKind::Tabs
        }
    };
    out
}

fn panel(part: blocks::PanelPart) -> PanelPart {
    match part {
        blocks::PanelPart::None => PanelPart::None,
        blocks::PanelPart::First => PanelPart::First,
        blocks::PanelPart::Middle => PanelPart::Middle,
        blocks::PanelPart::Last => PanelPart::Last,
        blocks::PanelPart::Only => PanelPart::Only,
    }
}

/// A block as an `InspectorRow`, with `cells` as its cells' model.
fn row(icons: &Icons<'_>, block: &Block, cells: ModelRc<InspectorCell>) -> InspectorRow {
    let mut out = InspectorRow { key: block.key.as_str().into(), gap: block.gap, panel: panel(block.panel), cells, ..Default::default() };
    match &block.content {
        BlockContent::Heading(heading) => {
            (out.heading, out.icon, out.title) = (true, glyph(icons, heading.icon), heading.title.as_str().into());
            if let Some(trailing) = &heading.trailing {
                (out.has_trailing, out.trailing) = (true, cell(icons, trailing));
            }
        }
        BlockContent::Row(grid) => {
            (out.columns, out.column_gap, out.align_end, out.aside) = (i32::from(grid.columns), grid.column_gap, grid.align == blocks::Align::End, grid.aside);
        }
    }
    out
}

fn row_cells(block: &Block) -> &[Cell] {
    match &block.content {
        BlockContent::Row(grid) => &grid.cells,
        BlockContent::Heading(_) => &[],
    }
}

/// Keeps an inspector model in step with `next` (D1): a changed row is rewritten, but its cells
/// model stays and only its changed cells are, so a dragged slider keeps its element.
pub struct BlocksModel {
    model: Rc<VecModel<InspectorRow>>,
    last: RefCell<Vec<Block>>,
}

impl Default for BlocksModel {
    fn default() -> Self {
        Self { model: Rc::new(VecModel::default()), last: RefCell::new(Vec::new()) }
    }
}

impl BlocksModel {
    pub fn model(&self) -> ModelRc<InspectorRow> {
        ModelRc::from(self.model.clone())
    }

    pub fn sync(&self, icons: &Icons<'_>, next: &[Block]) {
        let previous = self.last.borrow();
        let model = &self.model;
        let create = |block: &Block| {
            let cells: Vec<InspectorCell> = row_cells(block).iter().map(|item| cell(icons, item)).collect();
            row(icons, block, ModelRc::new(VecModel::from(cells)))
        };
        sync_keyed(model, &previous, next, |block| &block.key, create, |index, before, after| {
            let current = model.row_data(index).expect("the synced row exists");
            let cells_model = current.cells.clone();
            let inner = cells_model.as_any().downcast_ref::<VecModel<InspectorCell>>().expect("rows hold a VecModel of cells");
            sync_keyed(inner, row_cells(before), row_cells(after), |item| &item.id, |item| cell(icons, item), |cell_index, _, item| {
                inner.set_row_data(cell_index, cell(icons, item));
            });
            let next_row = row(icons, after, cells_model);
            if next_row != current {
                model.set_row_data(index, next_row);
            }
        });
        drop(previous);
        *self.last.borrow_mut() = next.to_vec();
    }
}

/// The projection's memory: the bundled icon images, decoded once each, the inspector's model,
/// and the last view written, so a dispatch that changes nothing here writes nothing.
#[derive(Default)]
pub struct Projection {
    icons: IconCache,
    inspector: BlocksModel,
    last: RefCell<Option<(SidebarView, Vec<FontOption>)>>,
}

impl Projection {
    pub fn project(&self, window: &AppWindow, store: &Store) {
        let view = sidebar_view(store);
        let (_, system) = font_options(store, "");
        let next = (view, system);
        if self.last.borrow().as_ref() == Some(&next) {
            return;
        }
        let (view, system) = &next;
        let global = window.global::<WidgetEditor>();
        let rows: Vec<WidgetRowView> = view
            .rows
            .iter()
            .map(|row| {
                let (glyph, image) = self.icons.icon(&window.global::<Icons>(), &row.icon);
                WidgetRowView { id: row.id.as_str().into(), name: row.name.as_str().into(), glyph, image }
            })
            .collect();
        replace_if_changed(global.get_rows(), rows, |model| global.set_rows(model));
        set_if_changed(global.get_open_id(), view.open_widget_id.as_deref().unwrap_or_default().into(), |value| global.set_open_id(value));
        set_if_changed(global.get_has_inspector(), view.inspector.is_some(), |value| global.set_has_inspector(value));
        let blocks = view.inspector.as_ref().map_or(&[][..], |inspector| &inspector.blocks[..]);
        self.inspector.sync(&window.global::<Icons>(), blocks);
        let model = self.inspector.model();
        set_if_changed(global.get_inspector(), model, |model| global.set_inspector(model));
        replace_if_changed(global.get_system_fonts(), font_choices(system), |model| global.set_system_fonts(model));
        *self.last.borrow_mut() = Some(next);
    }
}

/// Runs an action against the open widget; its errors are bugs in what the inspector offered,
/// logged as Phase 3 decided.
fn on_open_widget(dispatcher: &Dispatcher, action: impl FnOnce(&mut Store, &mut Services, &str) -> Result<(), String> + 'static) {
    dispatcher.handle().dispatch(move |store, services| {
        let Some(widget_id) = open_widget_id(store).map(str::to_string) else { return };
        if let Err(error) = action(store, services, &widget_id) {
            log::error!("widget editor: {error}");
        }
    });
}

/// A field edit of the open widget (D1).
fn edit(dispatcher: &Dispatcher, id: SharedString, input: Input, draft: bool) {
    let id = id.to_string();
    on_open_widget(dispatcher, move |store, services, widget_id| editor::edit_field(store, services, widget_id, &id, input, draft));
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

    let handle = dispatcher.clone();
    global.on_draft(move |id, value| edit(&handle, id, Input::Number(f64::from(value)), true));
    let handle = dispatcher.clone();
    global.on_commit_draft(move || on_open_widget(&handle, |store, _, widget_id| editor::commit_widget_draft(store, widget_id)));
    let handle = dispatcher.clone();
    global.on_set_number(move |id, raw| edit(&handle, id, Input::Text(raw.to_string()), false));
    let handle = dispatcher.clone();
    global.on_set_text(move |id, value| edit(&handle, id, Input::Text(value.to_string()), false));
    let handle = dispatcher.clone();
    global.on_set_flag(move |id, value| edit(&handle, id, Input::Flag(value), false));
    let handle = dispatcher.clone();
    global.on_clear_field(move |id| edit(&handle, id, Input::Clear, false));
    let handle = dispatcher.clone();
    global.on_press(move |id| edit(&handle, id, Input::Press, false));
    let handle = dispatcher.clone();
    global.on_set_stops(move |id, stops| {
        let stops = stops.iter().map(|stop| blocks::StopView { offset: f64::from(stop.offset), color: stop.color.to_string() }).collect();
        edit(&handle, id, Input::Stops(stops), true);
    });
}
