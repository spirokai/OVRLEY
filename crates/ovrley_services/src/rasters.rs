//! The images raster widgets show: each selected or embedded image, registered once under a
//! resource id, immutable, and shared by editor history and render jobs.

use ovrley_core::raster::{RasterResourceResolver, SelectedRaster};
use ovrley_widgets::widgets::raster::RasterError;
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::Path;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Every image registered this session. Clones share the registry.
#[derive(Clone, Debug, Default)]
pub struct RasterResources {
    // Every version stays registered for undo and redo. Jobs hold their own `Arc`s; dropping the
    // app state releases the registry's.
    images: Arc<Mutex<HashMap<String, Arc<SelectedRaster>>>>,
}

/// A user-selected image, registered: its oriented size and its resource id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RasterSelection {
    pub width: u32,
    pub height: u32,
    pub resource_id: String,
}

impl RasterResources {
    /// Registers `image` and returns its new resource id.
    pub fn insert(&self, image: SelectedRaster) -> String {
        let id = Uuid::new_v4().to_string();
        self.images.lock().expect("raster resources lock poisoned").insert(id.clone(), Arc::new(image));
        id
    }

    /// Validates the image at `path` a user selected and registers it.
    ///
    /// # Errors
    ///
    /// Why the file is not an image a raster widget can show.
    pub fn load_selected(&self, path: &Path) -> Result<RasterSelection, RasterError> {
        let raster = ovrley_core::raster::load_selected_raster(path)?;
        let (width, height) = raster.dimensions();
        Ok(RasterSelection { width, height, resource_id: self.insert(raster) })
    }

    /// The image of each widget, by `widget id → resource id`.
    ///
    /// # Errors
    ///
    /// When a resource id is not registered.
    pub fn owned_assets<S: BuildHasher>(
        &self,
        handles: HashMap<String, String, S>,
    ) -> Result<HashMap<String, Arc<SelectedRaster>>, String> {
        handles
            .into_iter()
            .map(|(widget_id, handle)| {
                let image = self.resolve(&handle).ok_or_else(|| format!("Raster {widget_id} has no loaded image resource"))?;
                Ok((widget_id, image))
            })
            .collect()
    }
}

impl RasterResourceResolver for RasterResources {
    fn resolve(&self, resource_id: &str) -> Option<Arc<SelectedRaster>> {
        self.images.lock().expect("raster resources lock poisoned").get(resource_id).cloned()
    }
}

/// A 1×1 24-bit BMP with one red pixel at byte 54.
#[cfg(test)]
pub(crate) const TEST_BMP: [u8; 58] = [
    66, 77, 58, 0, 0, 0, 0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 24, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 0,
];

#[cfg(test)]
mod tests {
    use super::*;
    use ovrley_core::raster::load_embedded_raster;

    #[test]
    fn replacement_keeps_both_immutable_versions_available() {
        let resources = RasterResources::default();
        let first = resources.insert(load_embedded_raster("bmp", TEST_BMP.to_vec()).unwrap());
        let original = resources.resolve(&first).unwrap();
        let second = resources.insert(load_embedded_raster("bmp", TEST_BMP.to_vec()).unwrap());

        assert_ne!(first, second);
        assert!(Arc::ptr_eq(&original, &resources.resolve(&first).unwrap()));
        assert!(std::ptr::eq(original.preview_png(), resources.resolve(&first).unwrap().preview_png()));
        assert_eq!(resources.resolve(&second).unwrap().encoded_bytes(), TEST_BMP);
    }
}
