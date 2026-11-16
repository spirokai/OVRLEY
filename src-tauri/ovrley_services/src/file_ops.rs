//! File-system operations behind the template, raster and batch-render flows.
//!
//! Owns: template save-path resolution, template file writes, and selected-raster registration.
//! Does not own: template listing, template content retrieval, or rendering — those live in
//!       `ovrley_core::commands` and the render pipeline.

use crate::raster_resources::RasterResources;
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
