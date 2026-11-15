//! The template-manager feature: its projection into the `TemplateManager` global and its
//! callbacks. Dialogs open here, before the actions, which take paths (D5).

use crate::hooks::app_store_selectors::set_if_changed;
use crate::{AppWindow, Choice, ChoiceKind, TemplateManager, UnsavedAnswer};
use ovrley_app::features::app_shell;
use ovrley_app::features::template_manager::{self as manager, template_groups};
use ovrley_app::i18n::catalog;
use ovrley_app::store::slices::template::{TemplateKind, TemplateSource};
use ovrley_app::store::{Dispatcher, Store};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

fn choice(kind: ChoiceKind, value: &str, label: &str) -> Choice {
    Choice {
        kind,
        value: value.into(),
        label: label.into(),
        ..Default::default()
    }
}

/// The select's entries: each resolution group's label and separator, then its templates.
fn choices(store: &Store) -> Vec<Choice> {
    let locale = store.i18n.language;
    let unknown = catalog().t(locale, "app-shell.unknownResolution");
    let mut entries = Vec::new();
    for group in template_groups(&store.template.templates, unknown) {
        entries.push(choice(ChoiceKind::Label, "", &group.label));
        entries.push(choice(ChoiceKind::Separator, "", ""));
        for template in group.templates {
            let label = match template.kind {
                TemplateKind::User => format!("{} (User)", template.name),
                TemplateKind::BuiltIn => template.name.clone(),
            };
            entries.push(choice(ChoiceKind::Item, &template.id, &label));
        }
    }
    entries
}

fn models_equal(current: &ModelRc<Choice>, next: &[Choice]) -> bool {
    current.row_count() == next.len() && current.iter().zip(next).all(|(left, right)| &left == right)
}

pub fn project(window: &AppWindow, store: &Store) {
    let global = window.global::<TemplateManager>();
    let next = choices(store);
    if !models_equal(&global.get_choices(), &next) {
        global.set_choices(ModelRc::new(VecModel::from(next)));
    }
    let (selected, file_name) = match &store.template.loaded_template_source {
        Some(TemplateSource::Bundled { template_id }) => (template_id.clone(), String::new()),
        Some(TemplateSource::File { path }) => (String::new(), manager::filename_from_path(Some(path))),
        None => (String::new(), String::new()),
    };
    set_if_changed(global.get_selected(), SharedString::from(selected), |value| global.set_selected(value));
    set_if_changed(global.get_file_name(), SharedString::from(file_name), |value| global.set_file_name(value));
    set_if_changed(global.get_can_save(), manager::save_status(store).shows(), |value| global.set_can_save(value));
    set_if_changed(global.get_selector_open_serial(), store.template_manager.selector_open_serial, |value| global.set_selector_open_serial(value));
    let confirm_open = matches!(store.unsaved_confirm.continuation(), Some(app_shell::UnsavedContinuation::NewTemplate { .. }));
    set_if_changed(global.get_confirm_open(), confirm_open, |value| global.set_confirm_open(value));
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let global = window.global::<TemplateManager>();
    let dispatch = dispatcher.handle();
    global.on_select_template(move |id| {
        let id = id.to_string();
        dispatch.dispatch(move |store, services| manager::load_template(store, services, id, Box::new(|_, _, _| {})));
    });
    let dispatch = dispatcher.handle();
    global.on_new_template(move || dispatch.dispatch(|store, services| manager::create_new_template_request(store, services, None)));
    let dispatch = dispatcher.handle();
    global.on_import_template(move || dispatch.dispatch(manager::open_import_template_dialog));
    let dispatch = dispatcher.handle();
    global.on_save_template(move || dispatch.dispatch(|store, services| manager::open_save_template_dialog(store, services, Box::new(|_, _, _| {}))));
    let dispatch = dispatcher.handle();
    global.on_answer_confirm(move |answer| {
        let answer = match answer {
            UnsavedAnswer::Save => app_shell::UnsavedAnswer::Save,
            UnsavedAnswer::Discard => app_shell::UnsavedAnswer::Discard,
            UnsavedAnswer::Cancel => app_shell::UnsavedAnswer::Cancel,
        };
        dispatch.dispatch(move |store, services| manager::answer_unsaved_confirm(store, services, answer));
    });
}
