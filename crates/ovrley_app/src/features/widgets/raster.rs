//! Raster images: the image dialog, loading the chosen image on a worker into the session's raster
//! resources, and the coded errors the editor shows.

use std::path::PathBuf;
use serde_json::Value;
use crate::dialogs::{open_single_path, DialogOptions};
use crate::i18n::catalog as translations;
use crate::store::Store;
use crate::store::services::{DialogFilter, Services};

/// What follows a load, on the UI thread: whether the widget took the image, else the error's
/// English message (an agent's call waits for it).
pub type Then = Box<dyn FnOnce(&mut Store, Result<(), String>) + Send>;

/// The image selection's state for one raster widget.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RasterSelectionState {
    /// The last selection's error code, until the next selection starts.
    pub error_code: Option<&'static str>,
    pub loading: bool,
}

/// A raster error code's message key under `raster.errors`.
fn error_key(code: &str) -> &'static str {
    match code {
        "relative_path" => "relativePath",
        "missing" => "missing",
        "unreadable" => "unreadable",
        "unsupported_type" => "unsupportedType",
        "encoded_size" => "encodedSize",
        "resolution" => "resolution",
        "decode" => "decode",
        "missing_asset" => "missingAsset",
        "corrupt_asset" => "corruptAsset",
        other => panic!("unknown raster error code {other}"),
    }
}

/// The message of a raster error code.
pub fn error_message(store: &Store, code: &str) -> String {
    translations()
        .t(
            store.i18n.language,
            &format!("raster.errors.{}", error_key(code)),
        )
        .to_string()
}

/// The error the editor shows: the last selection's, else the template load's resource error.
pub fn raster_error(store: &Store, widget_id: &str) -> Option<String> {
    let selection = store
        .widgets
        .raster
        .get(widget_id)
        .and_then(|state| state.error_code);
    let resource = store
        .editor
        .config
        .rasters
        .iter()
        .find(|raster| raster.get("id").and_then(Value::as_str) == Some(widget_id))
        .and_then(|raster| raster.get("resourceErrorCode"))
        .and_then(Value::as_str);
    selection
        .or(resource)
        .map(|code| error_message(store, code))
}

pub fn is_loading(store: &Store, widget_id: &str) -> bool {
    store
        .widgets
        .raster
        .get(widget_id)
        .is_some_and(|state| state.loading)
}

/// The dialog (`Image files`: png, jpeg, jpg, bmp, tiff), then the image loads on a
/// worker and replaces the widget's as one edit. A cancelled dialog changes nothing.
pub fn select_raster_image(store: &mut Store, services: &mut Services, widget_id: &str) {
    store.widgets.raster.insert(
        widget_id.to_string(),
        RasterSelectionState {
            error_code: None,
            loading: true,
        },
    );
    let filter = DialogFilter {
        name: translations()
            .t(store.i18n.language, "raster.imageFiles")
            .to_string(),
        extensions: ["png", "jpeg", "jpg", "bmp", "tiff"]
            .map(String::from)
            .to_vec(),
    };
    let widget_id = widget_id.to_string();
    let opened = open_single_path(
        services,
        &[filter],
        DialogOptions::default(),
        Box::new(move |store, services, selected| match selected {
            Some(path) => load_raster_image(store, services, &widget_id, path, None),
            None => finish(store, &widget_id, None),
        }),
    );
    if let Err(error) = opened {
        store
            .media
            .set_error_message(format!("Image selection failed: {error}"));
    }
}

fn finish(store: &mut Store, widget_id: &str, error_code: Option<&'static str>) {
    store.widgets.raster.insert(
        widget_id.to_string(),
        RasterSelectionState {
            error_code,
            loading: false,
        },
    );
}

/// Loads `path` on a worker; back on the UI thread the widget takes it or
/// shows the coded error. `widget_raster_image_set` calls this with the agent's path.
pub fn load_raster_image(
    store: &mut Store,
    services: &mut Services,
    widget_id: &str,
    path: String,
    then: Option<Then>,
) {
    store.widgets.raster.insert(
        widget_id.to_string(),
        RasterSelectionState {
            error_code: None,
            loading: true,
        },
    );
    let resources = services.rasters.clone();
    let widget_id = widget_id.to_string();
    services.workers.spawn("raster-load", move || {
        let loaded = resources.load_selected(&PathBuf::from(&path));
        Box::new(move |store: &mut Store, _: &mut Services| match loaded {
            Ok(image) => {
                finish(store, &widget_id, None);
                let set = store.editor.set_raster_image(
                    &widget_id,
                    &path,
                    image.width,
                    image.height,
                    &image.resource_id,
                );
                match then {
                    Some(then) => then(store, set),
                    None => {
                        if let Err(error) = set {
                            log::error!("raster image: {error}");
                        }
                    }
                }
            }
            Err(error) => {
                finish(store, &widget_id, Some(error.code()));
                if let Some(then) = then {
                    let message = translations().t(crate::i18n::Locale::En, &format!("raster.errors.{}", error_key(error.code()))).to_string();
                    then(store, Err(message));
                }
            }
        })
    });
}
