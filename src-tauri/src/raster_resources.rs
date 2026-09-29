//! Immutable selected raster resources shared by editor history and render jobs.

use base64::Engine;
use ovrley_core::raster::{RasterResourceResolver, SelectedRaster};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Default)]
pub(crate) struct RasterResources {
    // Retain every selected version for session undo/redo. Jobs hold their own
    // Arc snapshots; dropping app state releases the store's ownership.
    images: Mutex<HashMap<String, Arc<SelectedRaster>>>,
}

impl RasterResources {
    pub(crate) fn insert(&self, image: SelectedRaster) -> String {
        let id = Uuid::new_v4().to_string();
        self.images
            .lock()
            .unwrap()
            .insert(id.clone(), Arc::new(image));
        id
    }

    pub(crate) fn preview_png_base64(&self, resource_id: &str) -> Result<String, String> {
        let image = self
            .resolve(resource_id)
            .ok_or_else(|| "Raster image resource is unavailable".to_string())?;
        Ok(base64::engine::general_purpose::STANDARD.encode(image.preview_png()))
    }

    pub(crate) fn owned_assets(
        &self,
        handles: HashMap<String, String>,
    ) -> Result<HashMap<String, Arc<SelectedRaster>>, String> {
        handles
            .into_iter()
            .map(|(widget_id, handle)| {
                let image = self.resolve(&handle).ok_or_else(|| {
                    format!("Raster {widget_id} has no loaded image resource [raster_error:missing_resource:{widget_id}]")
                })?;
                Ok((widget_id, image))
            })
            .collect()
    }
}

impl RasterResourceResolver for RasterResources {
    fn resolve(&self, resource_id: &str) -> Option<Arc<SelectedRaster>> {
        self.images.lock().unwrap().get(resource_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_keeps_both_immutable_versions_available() {
        let resources = RasterResources::default();
        let encoded = base64::engine::general_purpose::STANDARD
            .decode(
                "Qk06AAAAAAAAADYAAAAoAAAAAQAAAAEAAAABABgAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAD/AA==",
            )
            .unwrap();
        let first = resources
            .insert(ovrley_core::raster::load_embedded_raster("bmp", encoded.clone()).unwrap());
        let original = resources.resolve(&first).unwrap();
        let second = resources
            .insert(ovrley_core::raster::load_embedded_raster("bmp", encoded.clone()).unwrap());

        assert_ne!(first, second);
        assert!(Arc::ptr_eq(&original, &resources.resolve(&first).unwrap()));
        assert!(std::ptr::eq(
            original.preview_png(),
            resources.resolve(&first).unwrap().preview_png()
        ));
        assert_eq!(resources.resolve(&second).unwrap().encoded_bytes(), encoded);
    }
}
