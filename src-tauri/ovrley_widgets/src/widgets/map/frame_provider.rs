//! How a finished map frame reaches the widget. Hosts implement this: `ovrley_widgets` compiles to
//! WASM and never depends on the renderer.
use crate::error::{WidgetError, WidgetResult};
use ovrley_map::{MapCamera, MapRouteDraw, MapStyleSpec, MapViewport};
use skia_safe::{images, AlphaType, ColorType, Data, Image, ImageInfo};

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

/// The preview's provider. Its map is a canvas under the widget, not an image inside it, so no frame
/// ever arrives here: see `PreviewMap::set_layer_ready`.
#[derive(Default)]
pub struct PreviewMapFrames;

impl MapFrameProvider for PreviewMapFrames {
    fn frame(&self, _request: &MapFrameRequest) -> WidgetResult<MapFrame> {
        Ok(MapFrame::Pending)
    }
}
