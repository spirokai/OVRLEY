//! The control panel's widget side: the widget list and the open widget's inspector
//! (`WidgetEditor`), the scene settings (`SceneSettings`), which use the inspector's rows, and the
//! widget drawer (`WidgetDrawer`). Inspector rows are synced by key, so an edited row keeps its
//! element, its pointer and its focus.

use crate::projection::{convert_enum, glyph, replace_if_changed, set_if_changed, sync_keyed, IconCache};
use crate::{
    AppWindow, Choice, ChoiceKind, DrawerGroupView, DrawerItemView, DrawerOptionView, GradientStop, Icons, InspectorCell, InspectorCellKind, InspectorRow,
    PanelPart, SceneSettings, ShapeToggleView, SwitchRowStyle, WidgetDrawer, WidgetEditor, WidgetRowView,
};
use ovrley_app::features::scene_settings as scene;
use ovrley_app::features::widget_drawer::{self as drawer, drawer_view, DrawerGroupView as AppDrawerGroup};
use ovrley_app::features::widget_editor::blocks::{self, Block, BlockContent, Cell, CellKind, OptionView, PanelPart as AppPanelPart};
use ovrley_app::features::widget_editor::rules::Input;
use ovrley_app::features::widget_editor::{self as editor, font_options, open_widget_id, sidebar_view, SidebarView};
use ovrley_app::lib::fonts::FontOption;
use ovrley_app::store::services::Services;
use ovrley_app::store::{DispatchHandle, Dispatcher, Store};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

convert_enum!(PanelPart <=> AppPanelPart { None, First, Middle, Last, Only });

/// The projection's memory: the bundled icon images, decoded once each, the inspector's and the
/// scene settings' models, and the last views written, so a dispatch that changes nothing here
/// writes nothing.
#[derive(Default)]
pub struct Projection {
    icons: IconCache,
    inspector: BlocksModel,
    last_sidebar: RefCell<Option<(SidebarView, Vec<FontOption>)>>,
    scene: BlocksModel,
    last_drawer: RefCell<Option<Vec<AppDrawerGroup>>>,
}

impl Projection {
    pub fn project(&self, window: &AppWindow, store: &Store) {
        let icons = window.global::<Icons>();
        self.project_sidebar(window, &icons, store);
        self.project_scene(window, &icons, store);
        self.project_drawer(window, &icons, store);
    }

    fn project_sidebar(&self, window: &AppWindow, icons: &Icons<'_>, store: &Store) {
        let (_, system) = font_options(store, "");
        let next = (sidebar_view(store), system);
        if self.last_sidebar.borrow().as_ref() == Some(&next) {
            return;
        }
        let (view, system) = &next;
        let global = window.global::<WidgetEditor>();
        let rows: Vec<WidgetRowView> = view
            .rows
            .iter()
            .map(|row| {
                let (glyph, image) = self.icons.icon(icons, &row.icon);
                WidgetRowView { id: row.id.as_str().into(), name: row.name.as_str().into(), glyph, image }
            })
            .collect();
        replace_if_changed(global.get_rows(), rows, |model| global.set_rows(model));
        set_if_changed(global.get_open_id(), view.open_widget_id.as_deref().unwrap_or_default().into(), |value| global.set_open_id(value));
        set_if_changed(global.get_has_inspector(), view.inspector.is_some(), |value| global.set_has_inspector(value));
        let blocks = view.inspector.as_ref().map_or(&[][..], |inspector| &inspector.blocks[..]);
        self.inspector.sync(icons, blocks);
        set_if_changed(global.get_inspector(), self.inspector.model(), |model| global.set_inspector(model));
        replace_if_changed(global.get_system_fonts(), font_choices(system), |model| global.set_system_fonts(model));
        *self.last_sidebar.borrow_mut() = Some(next);
    }

    fn project_scene(&self, window: &AppWindow, icons: &Icons<'_>, store: &Store) {
        let blocks = scene::describe(store);
        if self.scene.holds(&blocks) {
            return;
        }
        self.scene.sync(icons, &blocks);
        let global = window.global::<SceneSettings>();
        set_if_changed(global.get_rows(), self.scene.model(), |model| global.set_rows(model));
    }

    fn project_drawer(&self, window: &AppWindow, icons: &Icons<'_>, store: &Store) {
        let view = drawer_view(store);
        if self.last_drawer.borrow().as_ref() == Some(&view) {
            return;
        }
        let groups: Vec<DrawerGroupView> = view
            .iter()
            .map(|group| {
                let items: Vec<DrawerItemView> = group
                    .items
                    .iter()
                    .map(|item| {
                        let (glyph, image) = self.icons.icon(icons, &item.icon);
                        let options: Vec<DrawerOptionView> = item
                            .options
                            .iter()
                            .map(|option| {
                                let (glyph, image) = self.icons.icon(icons, &option.icon);
                                DrawerOptionView { label: option.label.as_str().into(), glyph, image }
                            })
                            .collect();
                        DrawerItemView {
                            key: item.key.as_str().into(),
                            short_name: item.short_name.as_str().into(),
                            glyph,
                            image,
                            available: item.available,
                            options: ModelRc::new(VecModel::from(options)),
                        }
                    })
                    .collect();
                DrawerGroupView { name: group.name.as_str().into(), items: ModelRc::new(VecModel::from(items)) }
            })
            .collect();
        let global = window.global::<WidgetDrawer>();
        replace_if_changed(global.get_groups(), groups, |model| global.set_groups(model));
        *self.last_drawer.borrow_mut() = Some(view);
    }
}

/// Keeps an inspector model in step with its blocks: a changed row is rewritten, but its cells
/// model stays and only its changed cells are, so a dragged slider keeps its element.
#[derive(Default)]
struct BlocksModel {
    model: Rc<VecModel<InspectorRow>>,
    last: RefCell<Vec<Block>>,
}

impl BlocksModel {
    fn model(&self) -> ModelRc<InspectorRow> {
        ModelRc::from(Rc::clone(&self.model))
    }

    /// Whether the model shows `blocks` already, and was written at least once.
    fn holds(&self, blocks: &[Block]) -> bool {
        self.model.row_count() > 0 && *self.last.borrow() == blocks
    }

    fn sync(&self, icons: &Icons<'_>, next: &[Block]) {
        let model = &self.model;
        let create = |block: &Block| {
            let cells: Vec<InspectorCell> = row_cells(block).iter().map(|item| cell(icons, item)).collect();
            row(icons, block, ModelRc::new(VecModel::from(cells)))
        };
        sync_keyed(model, &self.last.borrow(), next, |block| &block.key, create, |index, before, after| {
            let current = model.row_data(index).expect("the synced row exists");
            let cells = current.cells.clone();
            let inner = cells.as_any().downcast_ref::<VecModel<InspectorCell>>().expect("rows hold a VecModel of cells");
            sync_keyed(inner, row_cells(before), row_cells(after), |item| &item.id, |item| cell(icons, item), |cell_index, _, item| {
                inner.set_row_data(cell_index, cell(icons, item));
            });
            let next_row = row(icons, after, cells);
            if next_row != current {
                model.set_row_data(index, next_row);
            }
        });
        *self.last.borrow_mut() = next.to_vec();
    }
}

fn choices(options: &[OptionView]) -> ModelRc<Choice> {
    let items: Vec<Choice> =
        options.iter().map(|option| Choice { kind: ChoiceKind::Item, value: option.value.as_str().into(), label: option.label.as_str().into(), ..Default::default() }).collect();
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
fn cell(icons: &Icons<'_>, cell: &Cell) -> InspectorCell {
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
            let toggles: Vec<ShapeToggleView> =
                toggles.iter().map(|toggle| ShapeToggleView { key: toggle.key.as_str().into(), edge: toggle.edge, active: toggle.active, label: toggle.label.as_str().into() }).collect();
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

/// A block as an `InspectorRow`, with `cells` as its cells' model.
fn row(icons: &Icons<'_>, block: &Block, cells: ModelRc<InspectorCell>) -> InspectorRow {
    let mut out = InspectorRow { key: block.key.as_str().into(), gap: block.gap, panel: block.panel.into(), cells, ..Default::default() };
    match &block.content {
        BlockContent::Heading(heading) => {
            (out.heading, out.icon, out.title, out.scene_heading) = (true, glyph(icons, heading.icon), heading.title.as_str().into(), heading.scene);
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

/// Whether an edit is a draft, which a later commit records as one undo step.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Edit {
    Draft,
    Commit,
}

/// Runs an action against the open widget; its errors are bugs in what the inspector offered.
fn on_open_widget(dispatch: &DispatchHandle, action: impl FnOnce(&mut Store, &mut Services, &str) -> Result<(), String> + 'static) {
    dispatch.dispatch(move |store, services| {
        let Some(widget_id) = open_widget_id(store).map(str::to_string) else { return };
        if let Err(error) = action(store, services, &widget_id) {
            log::error!("widget editor: {error}");
        }
    });
}

/// A field edit of the open widget.
fn edit_widget(dispatch: &DispatchHandle, id: &SharedString, input: Input, edit: Edit) {
    let id = id.to_string();
    on_open_widget(dispatch, move |store, services, widget_id| editor::edit_field(store, services, widget_id, &id, input, edit == Edit::Draft));
}

/// A scene settings edit; its errors are bugs in what the panel offered.
fn edit_scene(dispatch: &DispatchHandle, id: &SharedString, input: Input, edit: Edit) {
    let id = id.to_string();
    dispatch.dispatch(move |store, services| {
        if let Err(error) = scene::edit(store, services, &id, input, edit == Edit::Draft) {
            log::error!("scene settings: {error}");
        }
    });
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    bind_widget_editor(&window.global::<WidgetEditor>(), dispatcher);
    bind_scene_settings(&window.global::<SceneSettings>(), dispatcher);
    let d = dispatcher.handle();
    window.global::<WidgetDrawer>().on_add(move |key, index| {
        let key = key.to_string();
        d.dispatch(move |store, services| {
            let view = drawer_view(store);
            let item = view.iter().flat_map(|group| &group.items).find(|item| item.key == key);
            let option = usize::try_from(index).ok().and_then(|index| item.and_then(|item| item.options.get(index)));
            let Some(option) = option else {
                log::error!("widget drawer: no option {index} of {key}");
                return;
            };
            if let Err(error) = drawer::add_from_drawer(store, services, &option.selection.clone()) {
                log::error!("widget drawer: {error}");
            }
        });
    });
}

fn bind_widget_editor(global: &WidgetEditor<'_>, dispatcher: &Dispatcher) {
    let d = dispatcher.handle();
    global.on_toggle(move |id| {
        let id = id.to_string();
        d.dispatch(move |store, _| {
            let next = (open_widget_id(store) != Some(id.as_str())).then_some(id);
            store.editor.set_selected_widget_id(next.as_deref());
        });
    });
    let d = dispatcher.handle();
    global.on_delete_widget(move |id| {
        let id = id.to_string();
        d.dispatch(move |store, _| {
            if let Err(error) = editor::delete_widget(store, &id) {
                log::error!("widget editor: {error}");
            }
        });
    });
    let d = dispatcher.handle();
    global.on_draft(move |id, value| edit_widget(&d, &id, Input::Number(value.into()), Edit::Draft));
    let d = dispatcher.handle();
    global.on_commit_draft(move || on_open_widget(&d, |store, _, widget_id| editor::commit_widget_draft(store, widget_id)));
    let d = dispatcher.handle();
    global.on_set_number(move |id, raw| edit_widget(&d, &id, Input::Text(raw.to_string()), Edit::Commit));
    let d = dispatcher.handle();
    global.on_set_text(move |id, value| edit_widget(&d, &id, Input::Text(value.to_string()), Edit::Commit));
    let d = dispatcher.handle();
    global.on_set_flag(move |id, value| edit_widget(&d, &id, Input::Flag(value), Edit::Commit));
    let d = dispatcher.handle();
    global.on_clear_field(move |id| edit_widget(&d, &id, Input::Clear, Edit::Commit));
    let d = dispatcher.handle();
    global.on_press(move |id| edit_widget(&d, &id, Input::Press, Edit::Commit));
    let d = dispatcher.handle();
    global.on_set_stops(move |id, stops| {
        let stops = stops.iter().map(|stop| blocks::StopView { offset: stop.offset.into(), color: stop.color.to_string() }).collect();
        edit_widget(&d, &id, Input::Stops(stops), Edit::Draft);
    });
}

fn bind_scene_settings(global: &SceneSettings<'_>, dispatcher: &Dispatcher) {
    let d = dispatcher.handle();
    global.on_draft(move |id, value| edit_scene(&d, &id, Input::Number(value.into()), Edit::Draft));
    let d = dispatcher.handle();
    global.on_commit_draft(move || {
        d.dispatch(|store, _| {
            if let Err(error) = scene::commit_global_defaults(store) {
                log::error!("scene settings: {error}");
            }
        });
    });
    let d = dispatcher.handle();
    global.on_set_number(move |id, raw| edit_scene(&d, &id, Input::Text(raw.to_string()), Edit::Commit));
    let d = dispatcher.handle();
    global.on_set_text(move |id, value| edit_scene(&d, &id, Input::Text(value.to_string()), Edit::Commit));
    let d = dispatcher.handle();
    global.on_set_flag(move |id, value| edit_scene(&d, &id, Input::Flag(value), Edit::Commit));
    let d = dispatcher.handle();
    global.on_press(move |id| edit_scene(&d, &id, Input::Press, Edit::Commit));
}
