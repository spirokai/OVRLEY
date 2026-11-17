//! The scene settings feature (phase 5 D8): its description projected into the `SceneSettings`
//! global through the inspector's keyed rows (D1), and its edits by field id.

use crate::features::widget_editor::BlocksModel;
use crate::hooks::app_store_selectors::set_if_changed;
use crate::{AppWindow, Icons, SceneSettings};
use ovrley_app::features::scene_settings::{self as scene, describe};
use ovrley_app::features::widget_editor::blocks::Block;
use ovrley_app::features::widget_editor::rules::Input;
use ovrley_app::store::{Dispatcher, Store};
use slint::{ComponentHandle, SharedString};
use std::cell::RefCell;

/// The rows' model and the last description written.
#[derive(Default)]
pub struct Projection {
    rows: BlocksModel,
    last: RefCell<Option<Vec<Block>>>,
}

impl Projection {
    pub fn project(&self, window: &AppWindow, store: &Store) {
        let blocks = describe(store);
        if self.last.borrow().as_ref() == Some(&blocks) {
            return;
        }
        let global = window.global::<SceneSettings>();
        self.rows.sync(&window.global::<Icons>(), &blocks);
        set_if_changed(global.get_rows(), self.rows.model(), |model| global.set_rows(model));
        *self.last.borrow_mut() = Some(blocks);
    }
}

/// An edit; its errors are bugs in what the panel offered, logged as the inspector's are.
fn edit(dispatcher: &Dispatcher, id: SharedString, input: Input, draft: bool) {
    let id = id.to_string();
    dispatcher.handle().dispatch(move |store, services| {
        if let Err(error) = scene::edit(store, services, &id, input, draft) {
            log::error!("scene settings: {error}");
        }
    });
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let global = window.global::<SceneSettings>();
    let handle = dispatcher.clone();
    global.on_draft(move |id, value| edit(&handle, id, Input::Number(f64::from(value)), true));
    let handle = dispatcher.clone();
    global.on_commit_draft(move || {
        handle.handle().dispatch(|store, _| {
            if let Err(error) = scene::commit_global_defaults(store) {
                log::error!("scene settings: {error}");
            }
        })
    });
    let handle = dispatcher.clone();
    global.on_set_number(move |id, raw| edit(&handle, id, Input::Text(raw.to_string()), false));
    let handle = dispatcher.clone();
    global.on_set_text(move |id, value| edit(&handle, id, Input::Text(value.to_string()), false));
    let handle = dispatcher.clone();
    global.on_set_flag(move |id, value| edit(&handle, id, Input::Flag(value), false));
    let handle = dispatcher.clone();
    global.on_press(move |id| edit(&handle, id, Input::Press, false));
}
