//! File-system operations behind the template, raster and batch-render flows.
//!
//! Owns: template save-path resolution, template file writes, selected-raster registration and
//!       batch video directory listing.
//! Does not own: template listing, template content retrieval, or rendering — those live in
//!       `ovrley_core::commands` and the render pipeline.

use crate::raster_resources::RasterResources;
use ovrley_core::error::RenderPathError;
use ovrley_widgets::widgets::raster::error::RasterError;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Returns the default save path for a user template under the user data root.
pub fn default_template_save_path(user_data_root: &Path, filename: &str) -> Result<PathBuf, String> {
    let directory = user_data_root.join("templates");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    Ok(directory.join(filename))
}

/// Writes a user template file, creating parent directories as needed.
///
/// Validates the template content before writing — invalid JSON or config that fails the
/// normalization seam is rejected with an error.
pub fn write_template_file(path: &Path, contents: &str) -> Result<(), String> {
    ovrley_core::commands::validate_template_contents(contents).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, contents).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RasterSelection {
    pub width: u32,
    pub height: u32,
    pub resource_id: String,
}

#[derive(Serialize)]
pub struct RasterSelectionError {
    pub code: &'static str,
    pub message: String,
}

impl From<RasterError> for RasterSelectionError {
    fn from(error: RasterError) -> Self {
        Self {
            code: error.code(),
            message: error.to_string(),
        }
    }
}

/// Validates a user-selected bitmap and registers its immutable oriented preview.
pub fn load_selected_raster(resources: &RasterResources, path: &Path) -> Result<RasterSelection, RasterSelectionError> {
    let raster = ovrley_core::raster::load_selected_raster(path).map_err(RasterSelectionError::from)?;
    let (width, height) = raster.dimensions();
    let resource_id = resources.insert(raster);
    Ok(RasterSelection {
        width,
        height,
        resource_id,
    })
}

const BATCH_VIDEO_EXTENSIONS: [&str; 3] = ["mp4", "mov", "mkv"];

/// Lists supported video files directly inside a directory, sorted by name.
///
/// Used by the batch-render folder picker — does not recurse into subdirectories.
pub fn list_directory_video_files(directory: &Path) -> Result<Vec<String>, RenderPathError> {
    if !directory.is_absolute() {
        return Err(RenderPathError::VideoDirectoryInvalid {
            path: directory.display().to_string(),
        });
    }

    let mut paths = Vec::new();
    for entry in std::fs::read_dir(directory).map_err(|error| RenderPathError::video_directory_io(directory, error))? {
        let entry = entry.map_err(|error| RenderPathError::video_directory_io(directory, error))?;
        let path = entry.path();
        let is_video = entry
            .file_type()
            .map_err(|error| RenderPathError::video_directory_io(directory, error))?
            .is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| BATCH_VIDEO_EXTENSIONS.iter().any(|candidate| extension.eq_ignore_ascii_case(candidate)));
        if is_video {
            paths.push(path.to_string_lossy().to_string());
        }
    }

    paths.sort_by_key(|left| left.to_lowercase());
    Ok(paths)
}
