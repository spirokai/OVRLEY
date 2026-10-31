//! How a finished map frame reaches the widget. Hosts implement this: `ovrley_widgets` compiles to
//! WASM and never depends on the renderer.
use crate::error::{WidgetError, WidgetResult};
use ovrley_map::{MapCamera, MapRouteDraw, MapStyleSpec, MapViewport};
use skia_safe::{images, AlphaType, ColorType, Data, Image, ImageInfo};
use std::sync::{mpsc::Receiver, Arc};

/// Everything a frame's pixels depend on.
#[derive(Clone, Debug, PartialEq)]
pub struct MapFrameRequest {
    pub spec: MapStyleSpec,
    pub camera: MapCamera,
    pub viewport: MapViewport,
    pub route: MapRouteDraw,
    /// The renderer crops the frame to this, in output pixels. A circle's is half its width.
    pub corner_radius_px: f32,
}

/// A finished frame: its premultiplied RGBA pixels, and an image reading them without a copy.
///
/// Performance: do not go back to `Data::new_copy` or a new buffer per frame. Without them, the map
/// thread went from about 6.5 to 4.5 ms per frame on the 4K benchmark.
#[derive(Clone)]
pub struct MapFramePixels {
    /// Declared first, so it drops before the pixels it reads.
    image: Image,
    pixels: Arc<Vec<u8>>,
}

impl MapFramePixels {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> WidgetResult<Self> {
        let info = ImageInfo::new(
            (width as i32, height as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        let pixels = Arc::new(pixels);
        // SAFETY: the bytes are never written while shared, and every clone of this frame holds them
        // for as long as its image. `image()` lends the image out only to draw it.
        let data = unsafe { Data::new_bytes(&pixels) };
        let image = images::raster_from_data(&info, data, width as usize * 4)
            .ok_or_else(|| WidgetError::Render("unable to wrap map frame pixels".into()))?;
        Ok(Self { image, pixels })
    }

    pub fn image(&self) -> &Image {
        &self.image
    }

    /// The buffer back for another frame, unless a clone is still reading it. Keeping a clone across
    /// frames still works, but silently allocates a new buffer every frame.
    pub fn into_pixels(self) -> Option<Vec<u8>> {
        let Self { image, pixels } = self;
        drop(image);
        Arc::into_inner(pixels)
    }
}

pub enum MapFrame {
    /// Being drawn elsewhere. The finished frame, or why it failed, arrives here.
    Drawing(Receiver<Result<MapFramePixels, String>>),
    Pending,
}

pub trait MapFrameProvider: Send + Sync {
    /// Starts the frame without waiting for it, drawing into `spare` when given one. Never blocks on
    /// the network. Export's provider never returns `Pending`. Fails only when the renderer has failed.
    /// Performance: must return before the frame is drawn, or export workers wait on the map again.
    fn frame(&self, request: &MapFrameRequest, spare: Option<Vec<u8>>) -> WidgetResult<MapFrame>;
}

/// The preview's provider. Its map is a canvas under the widget, not an image inside it, so no frame
/// ever arrives here: see `PreviewMap::set_layer_ready`.
#[derive(Default)]
pub struct PreviewMapFrames;

impl MapFrameProvider for PreviewMapFrames {
    fn frame(&self, _request: &MapFrameRequest, _spare: Option<Vec<u8>>) -> WidgetResult<MapFrame> {
        Ok(MapFrame::Pending)
    }
}
