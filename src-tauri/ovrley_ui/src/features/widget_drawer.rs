//! The widget drawer feature (D4): its projection into the `WidgetDrawer` global and adding.

use crate::hooks::app_store_selectors::replace_if_changed;
use crate::hooks::icons::IconCache;
use crate::{AppWindow, DrawerGroupView, DrawerItemView, DrawerOptionView, Icons, WidgetDrawer};
use ovrley_app::features::widget_drawer::{self as drawer, drawer_view};
use ovrley_app::store::{Dispatcher, Store};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::cell::RefCell;

/// The projection's memory: the icon cache and the last view written.
#[derive(Default)]
pub struct Projection {
    icons: IconCache,
    last: RefCell<Option<Vec<drawer::DrawerGroupView>>>,
}

impl Projection {
    pub fn project(&self, window: &AppWindow, store: &Store) {
        let view = drawer_view(store);
        if self.last.borrow().as_ref() == Some(&view) {
            return;
        }
        let icons = window.global::<Icons>();
        let groups: Vec<DrawerGroupView> = view
            .iter()
            .map(|group| {
                let items: Vec<DrawerItemView> = group
                    .items
                    .iter()
                    .map(|item| {
                        let (glyph, image) = self.icons.icon(&icons, &item.icon);
                        let options: Vec<DrawerOptionView> = item
                            .options
                            .iter()
                            .map(|option| {
                                let (glyph, image) = self.icons.icon(&icons, &option.icon);
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
        *self.last.borrow_mut() = Some(view);
    }
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let dispatch = dispatcher.handle();
    window.global::<WidgetDrawer>().on_add(move |key, index| {
        let key = key.to_string();
        dispatch.dispatch(move |store, services| {
            let view = drawer_view(store);
            let item = view.iter().flat_map(|group| &group.items).find(|item| item.key == key);
            let Some(option) = item.and_then(|item| item.options.get(index as usize)) else {
                log::error!("widget drawer: no option {index} of {key}");
                return;
            };
            if let Err(error) = drawer::add_from_drawer(store, services, &option.selection.clone()) {
                log::error!("widget drawer: {error}");
            }
        });
    });
}
