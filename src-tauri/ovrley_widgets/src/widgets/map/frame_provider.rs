//! How a finished map frame reaches the widget. Hosts implement this: `ovrley_widgets` compiles to
//! WASM and never depends on the renderer.
use crate::error::{WidgetError, WidgetResult};
use ovrley_map::{MapCamera, MapRouteDraw, MapStyleSpec, MapViewport};
use skia_safe::{images, AlphaType, ColorType, Data, Image, ImageInfo};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Everything a frame's pixels depend on.
#[derive(Clone, Debug, PartialEq)]
pub struct MapFrameRequest {
    pub spec: MapStyleSpec,
    pub camera: MapCamera,
    pub viewport: MapViewport,
    pub route: MapRouteDraw,
}

pub enum MapFrame {
    Ready(Image),
    Pending,
}

pub trait MapFrameProvider: Send + Sync {
    /// Never blocks on the network. Export's provider never returns `Pending`. Fails only when the
    /// renderer itself fails.
    fn frame(&self, request: &MapFrameRequest) -> WidgetResult<MapFrame>;
}

/// Wraps finished RGBA pixels, copying them once.
pub fn map_frame_image(width: u32, height: u32, pixels: &[u8]) -> WidgetResult<Image> {
    let info = ImageInfo::new(
        (width as i32, height as i32),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    images::raster_from_data(&info, Data::new_copy(pixels), width as usize * 4)
        .ok_or_else(|| WidgetError::Render("unable to wrap map frame pixels".into()))
}

/// The preview's provider: frames delivered through the session, keyed by output size. Nothing
/// delivers frames in the app yet, so its widgets stay `Pending`. Delivery adds the camera to the
/// key, so a frame can't land under a different camera.
#[derive(Default)]
pub struct UploadedMapFrames(Mutex<BTreeMap<(u32, u32), Image>>);

impl UploadedMapFrames {
    pub fn upload(&self, width: u32, height: u32, pixels: &[u8]) -> WidgetResult<()> {
        let expected = width as usize * height as usize * 4;
        if width == 0 || height == 0 || pixels.len() != expected {
            return Err(WidgetError::Config(format!(
                "map frame of {width}x{height} needs {expected} bytes, got {}",
                pixels.len()
            )));
        }
        let image = map_frame_image(width, height, pixels)?;
        self.0
            .lock()
            .map_err(|error| WidgetError::Render(error.to_string()))?
            .insert((width, height), image);
        Ok(())
    }
}

impl MapFrameProvider for UploadedMapFrames {
    fn frame(&self, request: &MapFrameRequest) -> WidgetResult<MapFrame> {
        let frames = self
            .0
            .lock()
            .map_err(|error| WidgetError::Render(error.to_string()))?;
        let key = (request.viewport.width_px(), request.viewport.height_px());
        Ok(frames
            .get(&key)
            .map_or(MapFrame::Pending, |image| MapFrame::Ready(image.clone())))
    }
}
