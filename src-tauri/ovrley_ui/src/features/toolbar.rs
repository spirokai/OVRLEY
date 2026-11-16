//! The toolbar feature: its projection into the `Toolbar` global and its callbacks.

use crate::hooks::app_store_selectors::set_if_changed;
use crate::{ActivityLapView, ActivityRowView, AppWindow, DrawerTool, Toolbar};
use ovrley_app::features::app_shell;
use ovrley_app::features::toolbar::activity_drawer_view;
use slint::{Model, ModelRc, SharedString, VecModel};
use ovrley_app::store::slices::layout::LeftDrawerTool;
use ovrley_app::store::{Dispatcher, Store};
use slint::ComponentHandle;

fn to_slint(tool: LeftDrawerTool) -> DrawerTool {
    match tool {
        LeftDrawerTool::Projects => DrawerTool::Projects,
        LeftDrawerTool::Activity => DrawerTool::Activity,
        LeftDrawerTool::Video => DrawerTool::Video,
        LeftDrawerTool::VideoSync => DrawerTool::VideoSync,
        LeftDrawerTool::Widgets => DrawerTool::Widgets,
    }
}

fn from_slint(tool: DrawerTool) -> LeftDrawerTool {
    match tool {
        DrawerTool::Projects => LeftDrawerTool::Projects,
        DrawerTool::Activity => LeftDrawerTool::Activity,
        DrawerTool::Video => LeftDrawerTool::Video,
        DrawerTool::VideoSync => LeftDrawerTool::VideoSync,
        DrawerTool::Widgets => LeftDrawerTool::Widgets,
    }
}

pub fn project(window: &AppWindow, store: &Store) {
    let toolbar = window.global::<Toolbar>();
    let layout = &store.layout;
    set_if_changed(toolbar.get_active_tool(), to_slint(layout.active_left_drawer_tool), |value| toolbar.set_active_tool(value));
    set_if_changed(toolbar.get_visible(), layout.left_drawer_visible, |value| toolbar.set_visible(value));
    set_if_changed(toolbar.get_pinned(), layout.left_drawer_pinned, |value| toolbar.set_pinned(value));
    project_activity_drawer(&toolbar, store);
}

/// Writes `next` into the model. Rows of an unchanged length update in place, so the repeater
/// keeps their elements and their animations play (a lap's switch slides instead of jumping).
fn replace_if_changed<T: Clone + PartialEq + 'static>(current: ModelRc<T>, next: Vec<T>, set: impl FnOnce(ModelRc<T>)) {
    if current.row_count() != next.len() {
        set(ModelRc::new(VecModel::from(next)));
        return;
    }
    for (row, value) in next.into_iter().enumerate() {
        if current.row_data(row).as_ref() != Some(&value) {
            current.set_row_data(row, value);
        }
    }
}

fn project_activity_drawer(toolbar: &Toolbar<'_>, store: &Store) {
    let view = activity_drawer_view(store);
    set_if_changed(toolbar.get_has_activity(), view.is_some(), |value| toolbar.set_has_activity(value));
    let Some(view) = view else { return };
    set_if_changed(toolbar.get_activity_file_name(), SharedString::from(view.file_name), |value| toolbar.set_activity_file_name(value));
    set_if_changed(toolbar.get_can_delete_activity(), view.can_delete, |value| toolbar.set_can_delete_activity(value));
    set_if_changed(toolbar.get_activity_format(), SharedString::from(view.format_label), |value| toolbar.set_activity_format(value));
    let rows = view.metadata.into_iter().map(|row| ActivityRowView { label: row.label.into(), value: row.value.into() }).collect();
    replace_if_changed(toolbar.get_activity_metadata(), rows, |model| toolbar.set_activity_metadata(model));
    replace_if_changed(toolbar.get_activity_extracted(), view.extracted.into_iter().map(SharedString::from).collect(), |model| toolbar.set_activity_extracted(model));
    replace_if_changed(toolbar.get_activity_derived(), view.derived.into_iter().map(SharedString::from).collect(), |model| toolbar.set_activity_derived(model));
    let laps = view
        .laps
        .into_iter()
        .map(|lap| ActivityLapView {
            id: lap.id as i32,
            number: lap.number.into(),
            duration: lap.duration.into(),
            start: lap.start.into(),
            enabled: lap.enabled,
        })
        .collect();
    replace_if_changed(toolbar.get_activity_laps(), laps, |model| toolbar.set_activity_laps(model));
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let toolbar = window.global::<Toolbar>();
    let dispatch = dispatcher.handle();
    toolbar.on_select_tool(move |tool| {
        let tool = from_slint(tool);
        dispatch.dispatch(move |store, _| store.layout.select_left_drawer_tool(tool));
    });
    let dispatch = dispatcher.handle();
    toolbar.on_set_pinned(move |pinned| dispatch.dispatch(move |store, _| store.layout.set_left_drawer_pinned(pinned)));
    let dispatch = dispatcher.handle();
    toolbar.on_dismiss_overlay(move || dispatch.dispatch(|store, _| store.layout.dismiss_left_drawer_overlay()));
    let dispatch = dispatcher.handle();
    toolbar.on_load_activity(move || dispatch.dispatch(app_shell::open_activity_dialog));
    let dispatch = dispatcher.handle();
    toolbar.on_delete_activity(move || {
        dispatch.dispatch(|store, _| {
            if let Err(error) = app_shell::delete_activity(store) {
                log::error!("{error}");
            }
        })
    });
    let dispatch = dispatcher.handle();
    toolbar.on_set_lap_enabled(move |id, enabled| {
        // The table lists the activity's lap ids, which are never negative.
        let id = usize::try_from(id).expect("lap ids are not negative");
        dispatch.dispatch(move |store, _| store.media.set_lap_enabled(id, enabled));
    });
}
