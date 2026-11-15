//! The toolbar feature: its projection into the `Toolbar` global and its callbacks.

use crate::hooks::app_store_selectors::set_if_changed;
use crate::{AppWindow, DrawerTool, Toolbar};
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
}
