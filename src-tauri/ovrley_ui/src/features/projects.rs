//! The projects feature: its projection into the `Projects` global and its callbacks.

use crate::hooks::app_store_selectors::{replace_if_changed, set_if_changed};
use crate::{
    AppWindow, MissingSourceRole, ProjectCardView, ProjectIntent, ProjectNoticeKind,
    ProjectSourceView, Projects, UnsavedAnswer,
};
use ovrley_app::features::app_shell::{self, UnsavedContinuation};
use ovrley_app::features::projects::{
    self, ProjectNotice, ProjectSourceRow, ProjectStatus, SourceRole,
};
use ovrley_app::store::{Dispatcher, Store};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer, SharedString};
use std::cell::RefCell;
use std::collections::HashMap;

fn source_view(row: &ProjectSourceRow) -> ProjectSourceView {
    ProjectSourceView {
        filename: row.filename.as_str().into(),
        path: row.path.as_str().into(),
    }
}

thread_local! {
    /// Each startup project's decoded thumbnail, by path, with the bytes it was decoded from.
    static THUMBNAILS: RefCell<HashMap<String, (Vec<u8>, Image)>> = RefCell::new(HashMap::new());
}

/// A project's thumbnail, decoded once per file.
fn thumbnail_image(path: &str, bytes: &[u8]) -> Option<Image> {
    THUMBNAILS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((cached, image)) = cache.get(path) {
            if cached == bytes {
                return Some(image.clone());
            }
        }
        let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .ok()?
            .into_rgba8();
        let pixels = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
            decoded.as_raw(),
            decoded.width(),
            decoded.height(),
        );
        let image = Image::from_rgba8(pixels);
        cache.insert(path.to_string(), (bytes.to_vec(), image.clone()));
        Some(image)
    })
}

pub fn project(window: &AppWindow, store: &Store) {
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
    set_if_changed(global.get_startup_open(), startup.open, |value| {
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
            let image = summary
                .thumbnail
                .as_deref()
                .and_then(|bytes| thumbnail_image(&summary.path, bytes));
            ProjectCardView {
                name: summary.name.as_str().into(),
                path: summary.path.as_str().into(),
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
        let role = match missing.role {
            SourceRole::Activity => MissingSourceRole::Activity,
            SourceRole::Video => MissingSourceRole::Video,
        };
        set_if_changed(global.get_missing_role(), role, |value| {
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

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let global = window.global::<Projects>();
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
        let answer = match answer {
            UnsavedAnswer::Save => app_shell::UnsavedAnswer::Save,
            UnsavedAnswer::Discard => app_shell::UnsavedAnswer::Discard,
            UnsavedAnswer::Cancel => app_shell::UnsavedAnswer::Cancel,
        };
        d.dispatch(move |store, services| {
            projects::answer_unsaved_confirm(store, services, answer)
        });
    });
}
