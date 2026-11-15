//! Port of `lib/file-dialog.js`: native file pickers with remembered directories.
//!
//! The pickers go through the `dialogs` service. Their answer comes back on the UI thread as an
//! action: the remembered directory is updated there, then `then` runs with the selection. The
//! preference rules are React's:
//! - a remembered directory overrides `default_path`;
//! - a cancelled dialog changes nothing;
//! - a malformed remembered directory fails the request.
//!
//! `selectBrowserFile` and `fileFromSelectedPath` are webview-only and not ported.

use crate::store::services::{DialogFilter, FileDialogRequest, Services};
use crate::store::Store;
use serde_json::Value;
use std::path::Path;

const LAST_RENDER_OUTPUT_DIR_KEY: &str = "last-render-output-dir";

/// The final path component of a native path, either separator.
pub fn filename_from_selected_path(path: &str) -> Option<&str> {
    path.split(['/', '\\'])
        .rfind(|component| !component.is_empty())
}

/// The parent directory of a native path, either separator; empty when there is none.
pub fn directory_from_selected_path(path: &str) -> &str {
    path.rfind(['/', '\\']).map_or("", |index| &path[..index])
}

/// Joins a filename to a directory with the directory's own separator.
///
/// # Panics
/// When either is empty: callers pass a remembered directory and a filename they checked.
pub fn path_in_directory(directory: &str, filename: &str) -> String {
    assert!(
        !directory.is_empty() && !filename.is_empty(),
        "Directory and filename are required"
    );
    let separator = if directory.contains('\\') { '\\' } else { '/' };
    let directory = directory.strip_suffix(['/', '\\']).unwrap_or(directory);
    format!("{directory}{separator}{filename}")
}

/// GTK file-chooser patterns are case-sensitive, so `*.mp4` hides `CLIP.MP4`. Lowercase stays
/// first so save dialogs keep appending the canonical extension.
pub fn case_insensitive_filters(filters: &[DialogFilter]) -> Vec<DialogFilter> {
    filters
        .iter()
        .map(|filter| {
            let mut extensions: Vec<String> = Vec::with_capacity(filter.extensions.len() * 2);
            for extension in filter
                .extensions
                .iter()
                .flat_map(|extension| [extension.clone(), extension.to_uppercase()])
            {
                if !extensions.contains(&extension) {
                    extensions.push(extension);
                }
            }
            DialogFilter {
                name: filter.name.clone(),
                extensions,
            }
        })
        .collect()
}

/// Options shared by the open pickers.
#[derive(Clone, Copy, Debug, Default)]
pub struct DialogOptions<'a> {
    pub default_path: Option<&'a str>,
    /// The preference that remembers the last directory used.
    pub last_directory_key: Option<&'static str>,
}

/// What to do with a picker's answer, on the UI thread, after the directory is remembered.
pub type Then = Box<dyn FnOnce(&mut Store, &mut Services, Option<String>)>;

fn remembered_directory(
    services: &Services,
    key: Option<&'static str>,
) -> Result<Option<String>, String> {
    let Some(key) = key else { return Ok(None) };
    Ok(services
        .preferences
        .get_optional_path(key)?
        .map(|path| path.to_string_lossy().into_owned()))
}

fn show(
    services: &Services,
    request: FileDialogRequest,
    last_directory_key: Option<&'static str>,
    remember: fn(&str) -> String,
    then: Then,
) {
    let dispatch = services.dispatch.clone();
    services.dialogs.show(
        request,
        Box::new(move |selected| {
            dispatch.dispatch(move |store, services| {
                if let (Some(selected), Some(key)) = (&selected, last_directory_key) {
                    services
                        .preferences
                        .set(key, Value::String(remember(selected)));
                }
                then(store, services, selected);
            });
        }),
    );
}

/// `openSinglePath`: one file of the given types.
pub fn open_single_path(
    services: &Services,
    filters: &[DialogFilter],
    options: DialogOptions,
    then: Then,
) -> Result<(), String> {
    let default_path = remembered_directory(services, options.last_directory_key)?
        .or(options.default_path.map(str::to_string));
    let request = FileDialogRequest::OpenFile {
        filters: case_insensitive_filters(filters),
        default_path,
    };
    show(
        services,
        request,
        options.last_directory_key,
        |selected| directory_from_selected_path(selected).to_string(),
        then,
    );
    Ok(())
}

/// `openDirectoryPath`: one directory, remembered as itself.
pub fn open_directory_path(
    services: &Services,
    options: DialogOptions,
    then: Then,
) -> Result<(), String> {
    let default_path = remembered_directory(services, options.last_directory_key)?
        .or(options.default_path.map(str::to_string));
    show(
        services,
        FileDialogRequest::OpenDirectory { default_path },
        options.last_directory_key,
        str::to_string,
        then,
    );
    Ok(())
}

/// `saveSinglePath`: a complete render or save target with one allowed extension. A remembered
/// directory keeps the default path's filename.
pub fn save_single_path(
    services: &Services,
    default_path: &str,
    extension: &str,
    filter_name: Option<&str>,
    last_directory_key: Option<&'static str>,
    then: Then,
) -> Result<(), String> {
    let mut resolved_default_path = default_path.to_string();
    if let (Some(directory), Some(filename)) = (
        remembered_directory(services, last_directory_key)?,
        filename_from_selected_path(default_path),
    ) {
        resolved_default_path = path_in_directory(&directory, filename);
    }
    let filter_name = filter_name.map_or_else(|| extension.to_uppercase(), str::to_string);
    let request = FileDialogRequest::Save {
        default_path: resolved_default_path,
        filters: case_insensitive_filters(&[DialogFilter {
            name: filter_name,
            extensions: vec![extension.to_string()],
        }]),
    };
    show(
        services,
        request,
        last_directory_key,
        |selected| directory_from_selected_path(selected).to_string(),
        then,
    );
    Ok(())
}

/// `loadRememberedRenderDirectory`: the optional remembered absolute render directory.
pub fn load_remembered_render_directory(services: &Services) -> Result<Option<String>, String> {
    match services.preferences.get(LAST_RENDER_OUTPUT_DIR_KEY) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value))
            if !value.trim().is_empty() && Path::new(value).is_absolute() =>
        {
            Ok(Some(value.clone()))
        }
        Some(_) => Err("The remembered render output directory is malformed.".into()),
    }
}

/// `rememberAcceptedRenderOutput`: remembers the directory of an accepted output path. Write
/// failures are logged by the preferences writer, as React logged them.
pub fn remember_accepted_render_output(services: &mut Services, output_path: &str) {
    let directory = Path::new(output_path)
        .parent()
        .map_or_else(String::new, |parent| parent.to_string_lossy().into_owned());
    services
        .preferences
        .set(LAST_RENDER_OUTPUT_DIR_KEY, Value::String(directory));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::services::fakes::fake_services;
    use crate::store::Dispatcher;
    use serde_json::json;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A platform absolute path, since remembered directories must be absolute here.
    fn absolute(windows: &str, unix: &str) -> String {
        if cfg!(windows) {
            windows.into()
        } else {
            unix.into()
        }
    }

    fn project_filter() -> Vec<DialogFilter> {
        vec![DialogFilter::new("OVRLEY Project", &["oly"])]
    }

    #[test]
    fn project_open_and_save_as_share_one_persistent_directory_preference() {
        let remembered = absolute("D:\\OVRLEY Projects", "/OVRLEY Projects");
        let opened = absolute("E:\\Events\\Race.oly", "/Events/Race.oly");
        let saved = absolute("F:\\Archive\\Race.oly", "/Archive/Race.oly");
        let (parts, fakes) = fake_services(json!({"last-project-dir": remembered}));
        let dispatcher = Dispatcher::new(Default::default(), parts);
        let selections = Rc::new(RefCell::new(Vec::new()));

        let log = selections.clone();
        dispatcher.dispatch(move |_, services| {
            let options = DialogOptions {
                default_path: Some("C:\\Users\\test\\Documents\\OVRLEY\\projects"),
                last_directory_key: Some("last-project-dir"),
            };
            open_single_path(
                services,
                &project_filter(),
                options,
                Box::new(move |_, _, selected| log.borrow_mut().push(selected)),
            )
            .unwrap();
        });
        assert_eq!(
            fakes.dialogs.requests(),
            [FileDialogRequest::OpenFile {
                filters: vec![DialogFilter::new("OVRLEY Project", &["oly", "OLY"])],
                default_path: Some(remembered.clone()),
            }]
        );
        fakes.dialogs.answer(Some(&opened));

        let log = selections.clone();
        dispatcher.dispatch(move |_, services| {
            save_single_path(
                services,
                "C:\\Users\\test\\Documents\\OVRLEY\\projects\\Race.oly",
                "oly",
                Some("OVRLEY Project"),
                Some("last-project-dir"),
                Box::new(move |_, _, selected| log.borrow_mut().push(selected)),
            )
            .unwrap();
        });
        assert_eq!(
            fakes.dialogs.requests(),
            [FileDialogRequest::Save {
                default_path: path_in_directory(directory_from_selected_path(&opened), "Race.oly"),
                filters: vec![DialogFilter::new("OVRLEY Project", &["oly", "OLY"])],
            }]
        );
        fakes.dialogs.answer(Some(&saved));

        let remembered_directories: Vec<_> = fakes
            .preferences
            .saves
            .borrow()
            .iter()
            .map(|saved| saved["last-project-dir"].clone())
            .collect();
        assert_eq!(
            remembered_directories,
            [
                json!(directory_from_selected_path(&opened)),
                json!(directory_from_selected_path(&saved))
            ]
        );
        assert_eq!(*selections.borrow(), [Some(opened), Some(saved)]);
    }

    #[test]
    fn cancelled_project_dialogs_do_not_change_the_remembered_directory() {
        let remembered = absolute("D:\\OVRLEY Projects", "/OVRLEY Projects");
        let (parts, fakes) = fake_services(json!({"last-project-dir": remembered}));
        let dispatcher = Dispatcher::new(Default::default(), parts);

        dispatcher.dispatch(|_, services| {
            let options = DialogOptions {
                default_path: None,
                last_directory_key: Some("last-project-dir"),
            };
            open_single_path(services, &project_filter(), options, Box::new(|_, _, _| {})).unwrap();
            save_single_path(
                services,
                "/x/Race.oly",
                "oly",
                Some("OVRLEY Project"),
                Some("last-project-dir"),
                Box::new(|_, _, _| {}),
            )
            .unwrap();
        });
        fakes.dialogs.answer(None);
        fakes.dialogs.answer(None);

        assert!(fakes.preferences.saves.borrow().is_empty());
    }

    #[test]
    fn dialog_filters_match_uppercase_extensions_on_case_sensitive_pickers() {
        assert_eq!(
            case_insensitive_filters(&[DialogFilter::new("Video", &["mp4", "mov"])]),
            [DialogFilter::new("Video", &["mp4", "MP4", "mov", "MOV"])]
        );
    }

    #[test]
    fn a_malformed_remembered_directory_fails_the_request() {
        let (parts, fakes) = fake_services(json!({"last-project-dir": "relative/dir"}));
        let dispatcher = Dispatcher::new(Default::default(), parts);
        let result = dispatcher
            .dispatch_with_result(|_, services| {
                let options = DialogOptions {
                    default_path: None,
                    last_directory_key: Some("last-project-dir"),
                };
                open_single_path(services, &project_filter(), options, Box::new(|_, _, _| {}))
            })
            .unwrap();
        assert_eq!(
            result.unwrap_err(),
            "Preference \"last-project-dir\" must be an absolute path"
        );
        assert!(fakes.dialogs.requests().is_empty());
    }

    #[test]
    fn path_helpers_handle_both_separators() {
        assert_eq!(filename_from_selected_path("C:\\a\\b.oly"), Some("b.oly"));
        assert_eq!(filename_from_selected_path("/a/b/"), Some("b"));
        assert_eq!(filename_from_selected_path(""), None);
        assert_eq!(directory_from_selected_path("C:\\a\\b.oly"), "C:\\a");
        assert_eq!(directory_from_selected_path("b.oly"), "");
        assert_eq!(path_in_directory("C:\\a\\", "b.oly"), "C:\\a\\b.oly");
        assert_eq!(path_in_directory("/a", "b.oly"), "/a/b.oly");
    }

    #[test]
    fn remembered_render_directory_rules() {
        let absolute_dir = absolute("C:\\Renders", "/renders");
        let (parts, _) = fake_services(json!({}));
        let dispatcher = Dispatcher::new(Default::default(), parts);
        let output = Path::new(&absolute_dir)
            .join("out.mp4")
            .to_string_lossy()
            .into_owned();
        let result = dispatcher
            .dispatch_with_result(move |_, services| {
                let before = load_remembered_render_directory(services);
                remember_accepted_render_output(services, &output);
                (before, load_remembered_render_directory(services))
            })
            .unwrap();
        assert_eq!(result, (Ok(None), Ok(Some(absolute_dir))));

        let (parts, _) = fake_services(json!({"last-render-output-dir": "renders"}));
        let dispatcher = Dispatcher::new(Default::default(), parts);
        assert!(dispatcher
            .dispatch_with_result(|_, services| load_remembered_render_directory(services))
            .unwrap()
            .is_err());
    }
}
