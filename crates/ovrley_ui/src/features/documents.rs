//! The documents the user opens and saves: projects (the `Projects` global: the file menu, the
//! startup projects, missing sources) and templates (the `TemplateManager` global: the header's
//! template select). Both ask the unsaved-changes question before they replace the document.

use crate::projection::{convert_enum, replace_if_changed, set_if_changed, PngImages};
use crate::{
    AppWindow, Choice, ChoiceKind, MissingSourceRole, ProjectCardView, ProjectIntent,
    ProjectNoticeKind, ProjectSourceView, Projects, TemplateManager, UnsavedAnswer,
};
use ovrley_app::features::app_shell::{UnsavedAnswer as AppUnsavedAnswer, UnsavedContinuation};
use ovrley_app::features::projects::{
    self, ProjectNotice, ProjectSourceRow, ProjectStatus, SourceRole,
};
use ovrley_app::features::template_manager::{self as manager, template_groups};
use ovrley_app::i18n::catalog;
use ovrley_app::store::slices::template::TemplateSource;
use ovrley_app::store::{Dispatcher, Store};
use ovrley_core::commands::TemplateKind;
use slint::{ComponentHandle, SharedString};

convert_enum!(UnsavedAnswer <=> AppUnsavedAnswer { Save, Discard, Cancel });
convert_enum!(MissingSourceRole <=> SourceRole { Activity, Video });

/// The startup projects' thumbnails, by path.
#[derive(Default)]
pub struct Projection {
    thumbnails: PngImages,
}

impl Projection {
    pub fn project(&self, window: &AppWindow, store: &Store) {
        self.project_projects(window, store);
        project_templates(window, store);
    }

    fn project_projects(&self, window: &AppWindow, store: &Store) {
        let global = window.global::<Projects>();
        let status = projects::project_status(store);
        set_if_changed(global.get_busy(), projects::is_busy(store), |value| {
            global.set_busy(value)
        });
        set_if_changed(global.get_loading(), store.projects.loading, |value| {
            global.set_loading(value)
        });
        set_if_changed(
            global.get_save_disabled(),
            status == ProjectStatus::Saved,
            |value| global.set_save_disabled(value),
        );
        set_if_changed(
            global.get_status_shown(),
            status.has_unsaved_changes(),
            |value| global.set_status_shown(value),
        );
        set_if_changed(
            global.get_status_modified(),
            status == ProjectStatus::Modified,
            |value| global.set_status_modified(value),
        );

        let drawer = projects::projects_drawer_view(store);
        set_if_changed(
            global.get_has_project(),
            drawer.project.is_some(),
            |value| global.set_has_project(value),
        );
        if let Some(project) = &drawer.project {
            set_if_changed(global.get_project(), source_view(project), |value| {
                global.set_project(value)
            });
        }
        set_if_changed(
            global.get_has_activity(),
            drawer.activity.is_some(),
            |value| global.set_has_activity(value),
        );
        if let Some(activity) = &drawer.activity {
            set_if_changed(global.get_activity(), source_view(activity), |value| {
                global.set_activity(value)
            });
        }
        replace_if_changed(
            global.get_videos(),
            drawer.videos.iter().map(source_view).collect(),
            |model| global.set_videos(model),
        );

        let startup = &store.projects.startup;
        // The update prompt hides the startup dialog while it shows.
        let startup_open = startup.open && !store.app_update.is_open();
        set_if_changed(global.get_startup_open(), startup_open, |value| {
            global.set_startup_open(value)
        });
        set_if_changed(
            global.get_startup_opening(),
            startup.opening_path.is_some(),
            |value| global.set_startup_opening(value),
        );
        let cards = startup
            .projects
            .iter()
            .map(|summary| {
                let path = summary.path.to_string_lossy();
                let image = summary
                    .thumbnail
                    .as_deref()
                    .and_then(|bytes| self.thumbnails.image(&path, bytes));
                ProjectCardView {
                    name: summary.name.as_str().into(),
                    path: path.as_ref().into(),
                    has_thumbnail: image.is_some(),
                    thumbnail: image.unwrap_or_default(),
                }
            })
            .collect();
        replace_if_changed(global.get_startup_projects(), cards, |model| {
            global.set_startup_projects(model)
        });

        let missing = store.projects.missing_source.as_ref();
        set_if_changed(global.get_missing_open(), missing.is_some(), |value| {
            global.set_missing_open(value)
        });
        if let Some(missing) = missing {
            set_if_changed(global.get_missing_role(), missing.role.into(), |value| {
                global.set_missing_role(value)
            });
            set_if_changed(
                global.get_missing_file_name(),
                SharedString::from(missing.file_name.as_str()),
                |value| global.set_missing_file_name(value),
            );
        }

        let (notice, detail) = match &store.projects.notice {
            None => (ProjectNoticeKind::None, ""),
            Some(ProjectNotice::FormatUpgraded { backup_path }) => {
                (ProjectNoticeKind::FormatUpgraded, backup_path.as_str())
            }
            Some(ProjectNotice::LegacyBatchFolder { folder }) => {
                (ProjectNoticeKind::LegacyBatchFolder, folder.as_str())
            }
        };
        set_if_changed(global.get_notice(), notice, |value| {
            global.set_notice(value)
        });
        set_if_changed(
            global.get_notice_detail(),
            SharedString::from(detail),
            |value| global.set_notice_detail(value),
        );

        let intent = match store.unsaved_confirm.continuation() {
            Some(UnsavedContinuation::NewProject { .. }) => Some(ProjectIntent::NewProject),
            Some(UnsavedContinuation::OpenProject { .. }) => Some(ProjectIntent::OpenProject),
            Some(UnsavedContinuation::CloseWindow) => Some(ProjectIntent::CloseWindow),
            _ => None,
        };
        set_if_changed(global.get_confirm_open(), intent.is_some(), |value| {
            global.set_confirm_open(value)
        });
        if let Some(intent) = intent {
            set_if_changed(global.get_confirm_intent(), intent, |value| {
                global.set_confirm_intent(value)
            });
        }
    }
}

fn source_view(row: &ProjectSourceRow) -> ProjectSourceView {
    ProjectSourceView {
        filename: row.filename.as_str().into(),
        path: row.path.as_str().into(),
    }
}

fn choice(kind: ChoiceKind, value: &str, label: &str) -> Choice {
    Choice {
        kind,
        value: value.into(),
        label: label.into(),
        ..Default::default()
    }
}

/// The template select's entries: each resolution group's label and separator, then its templates.
fn template_choices(store: &Store) -> Vec<Choice> {
    let unknown = catalog().t(store.i18n.language, "app-shell.unknownResolution");
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

fn project_templates(window: &AppWindow, store: &Store) {
    let global = window.global::<TemplateManager>();
    replace_if_changed(global.get_choices(), template_choices(store), |model| {
        global.set_choices(model)
    });
    let (selected, file_name) = match &store.template.loaded_template_source {
        Some(TemplateSource::Bundled { template_id }) => (template_id.clone(), String::new()),
        Some(TemplateSource::File { path }) => {
            (String::new(), manager::filename_from_path(Some(path)))
        }
        None => (String::new(), String::new()),
    };
    set_if_changed(
        global.get_selected(),
        SharedString::from(selected),
        |value| global.set_selected(value),
    );
    set_if_changed(
        global.get_file_name(),
        SharedString::from(file_name),
        |value| global.set_file_name(value),
    );
    set_if_changed(
        global.get_can_save(),
        manager::save_status(store).shows(),
        |value| global.set_can_save(value),
    );
    set_if_changed(
        global.get_selector_open_serial(),
        store.template_manager.selector_open_serial,
        |value| global.set_selector_open_serial(value),
    );
    let confirm_open = matches!(
        store.unsaved_confirm.continuation(),
        Some(UnsavedContinuation::NewTemplate { .. })
    );
    set_if_changed(global.get_confirm_open(), confirm_open, |value| {
        global.set_confirm_open(value)
    });
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    bind_projects(&window.global::<Projects>(), dispatcher);
    bind_templates(&window.global::<TemplateManager>(), dispatcher);
}

fn bind_projects(global: &Projects<'_>, dispatcher: &Dispatcher) {
    let d = dispatcher.handle();
    global.on_new_project(move || {
        d.dispatch(|store, services| projects::new_project(store, services, None, None))
    });
    let d = dispatcher.handle();
    global.on_open_project(move || d.dispatch(projects::open_project_dialog));
    let d = dispatcher.handle();
    global.on_save_project(move || {
        d.dispatch(|store, services| {
            projects::save_project(store, services, false, None, Box::new(|_, _, _| {}))
        })
    });
    let d = dispatcher.handle();
    global.on_save_project_as(move || {
        d.dispatch(|store, services| {
            projects::save_project(store, services, true, None, Box::new(|_, _, _| {}))
        })
    });
    let d = dispatcher.handle();
    global
        .on_startup_dismiss(move || d.dispatch(|store, _| projects::dismiss_startup_dialog(store)));
    let d = dispatcher.handle();
    global.on_startup_new(move || d.dispatch(projects::startup_new_project));
    let d = dispatcher.handle();
    global.on_startup_open_project(move |path| {
        d.dispatch(move |store, services| {
            projects::startup_open_project(store, services, path.to_string())
        })
    });
    let d = dispatcher.handle();
    global.on_locate_source(move || d.dispatch(projects::locate_missing_source));
    let d = dispatcher.handle();
    global.on_load_anyway(move || d.dispatch(projects::skip_missing_source));
    let d = dispatcher.handle();
    global.on_cancel_missing(move || d.dispatch(|store, _| projects::cancel_missing_source(store)));
    let d = dispatcher.handle();
    global.on_close_notice(move || d.dispatch(|store, _| store.projects.notice = None));
    let d = dispatcher.handle();
    global.on_answer_confirm(move |answer| {
        d.dispatch(move |store, services| {
            projects::answer_unsaved_confirm(store, services, answer.into())
        })
    });
}

fn bind_templates(global: &TemplateManager<'_>, dispatcher: &Dispatcher) {
    let d = dispatcher.handle();
    global.on_select_template(move |id| {
        let id = id.to_string();
        d.dispatch(move |store, services| {
            manager::load_template(store, services, id, Box::new(|_, _, _| {}))
        });
    });
    let d = dispatcher.handle();
    global.on_new_template(move || {
        d.dispatch(|store, services| manager::create_new_template_request(store, services, None))
    });
    let d = dispatcher.handle();
    global.on_import_template(move || d.dispatch(manager::open_import_template_dialog));
    let d = dispatcher.handle();
    global.on_save_template(move || {
        d.dispatch(|store, services| {
            manager::open_save_template_dialog(store, services, Box::new(|_, _, _| {}))
        })
    });
    let d = dispatcher.handle();
    global.on_answer_confirm(move |answer| {
        d.dispatch(move |store, services| {
            manager::answer_unsaved_confirm(store, services, answer.into())
        })
    });
}
