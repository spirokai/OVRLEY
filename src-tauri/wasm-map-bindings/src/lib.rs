//! External boundary between `ovrley_map::gpu` and the page. `MapLayer` owns one canvas's renderer on
//! the UI thread, and `TileDecoder` turns tile bytes into geometry in a worker.
use ovrley_map::gpu::{GpuMapConfig, GpuMapRenderer, MapTileDecoder};
use ovrley_map::{
    MapCamera, MapLabelDraw, MapLayers, MapRouteDraw, MapRouteLine, MapRoutePoint, MapRouteStyle,
    MapStyleBuildings, MapStyleId, MapStyleSpec, MapViewport, TileId, LABEL_PLACEMENTS,
};
use serde::Deserialize;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

/// The widget chose these, so they are not user input here. Keep in step with `PreviewMap::publish_view`.
#[derive(Deserialize)]
struct RendererConfig {
    style: MapStyleId,
    /// The style's 3D buildings, from the catalog.
    style_buildings: MapStyleBuildings,
    buildings: bool,
    labels: bool,
    paths: bool,
    local_names: bool,
    width: u32,
    height: u32,
    pixel_ratio: f32,
    msaa_samples: u32,
    zoom: f64,
}

/// `lon, lat, zoom, pitch, bearing, top padding`, from the widget's view.
const CAMERA_VALUES: usize = 6;
/// The cameras of a frame's label placements, oldest first, then the blend between the last two. Empty
/// when the frame draws no labels.
const LABEL_VALUES: usize = LABEL_PLACEMENTS * CAMERA_VALUES + 1;
/// Completed rgba, remaining rgba, then the shared width.
const ROUTE_STYLE_VALUES: usize = 9;
/// `world_x, world_y, along_metres` per vertex.
const ROUTE_POINT_VALUES: usize = 3;

fn js_error(message: String) -> JsValue {
    JsValue::from_str(&message)
}

fn parse_config(json: &str) -> Result<GpuMapConfig, String> {
    let config: RendererConfig = serde_json::from_str(json).map_err(|error| error.to_string())?;
    Ok(GpuMapConfig {
        spec: MapStyleSpec {
            style: config.style,
            layers: MapLayers {
                buildings: config.buildings,
                labels: config.labels,
                paths: config.paths,
                local_names: config.local_names,
            },
        },
        viewport: MapViewport::new(config.width, config.height, config.pixel_ratio)?,
        sample_count: config.msaa_samples,
        style_zoom: config.zoom,
        buildings: config.style_buildings,
    })
}

fn parse_camera(values: &[f64]) -> Result<MapCamera, String> {
    let &[lon, lat, zoom, pitch, bearing, padding] = values else {
        return Err(format!(
            "A map camera has {CAMERA_VALUES} values, got {}",
            values.len()
        ));
    };
    MapCamera::new(lon, lat, zoom, pitch, bearing, padding)
}

/// `None` for an empty array: the frame draws no labels.
fn parse_labels(values: &[f64]) -> Result<Option<MapLabelDraw>, String> {
    if values.is_empty() {
        return Ok(None);
    }
    if values.len() != LABEL_VALUES {
        return Err(format!(
            "A map label draw has {LABEL_VALUES} values, got {}",
            values.len()
        ));
    }
    let (cameras, blend) = values.split_at(LABEL_VALUES - 1);
    let cameras = cameras
        .chunks_exact(CAMERA_VALUES)
        .map(parse_camera)
        .collect::<Result<Vec<_>, _>>()?;
    let cameras: [MapCamera; LABEL_PLACEMENTS] = cameras
        .try_into()
        .expect("LABEL_VALUES holds LABEL_PLACEMENTS cameras");
    MapLabelDraw::new(cameras, blend[0] as f32).map(Some)
}

#[wasm_bindgen]
pub struct MapLayer {
    renderer: GpuMapRenderer,
    route: Option<Arc<MapRouteLine>>,
}

#[wasm_bindgen]
impl MapLayer {
    /// Rejects without a hardware GPU or WebGL2, and the widget keeps its placeholder.
    pub async fn create(
        canvas: web_sys::HtmlCanvasElement,
        config: String,
        style: Vec<u8>,
    ) -> Result<MapLayer, JsValue> {
        console_error_panic_hook::set_once();
        let config = parse_config(&config).map_err(js_error)?;
        let renderer = GpuMapRenderer::for_canvas(config, &style, canvas)
            .await
            .map_err(js_error)?;
        Ok(Self {
            renderer,
            route: None,
        })
    }

    pub fn set_route(&mut self, points: &[f64]) -> Result<(), JsValue> {
        if points.len() % ROUTE_POINT_VALUES != 0 {
            return Err(js_error(format!(
                "A map route has {ROUTE_POINT_VALUES} values per vertex, got {}",
                points.len()
            )));
        }
        let points = points
            .chunks_exact(ROUTE_POINT_VALUES)
            .map(|point| MapRoutePoint {
                world_x: point[0],
                world_y: point[1],
                along_metres: point[2] as f32,
            })
            .collect();
        self.route = Some(Arc::new(MapRouteLine::new(points).map_err(js_error)?));
        Ok(())
    }

    /// The `z, x, y` of each tile this camera, and the cameras of its label placements, see that aren't
    /// on the GPU yet.
    pub fn missing(&mut self, camera: &[f64], labels: &[f64]) -> Result<Vec<u32>, JsValue> {
        let camera = parse_camera(camera).map_err(js_error)?;
        let labels = parse_labels(labels).map_err(js_error)?;
        Ok(self
            .renderer
            .missing_tiles(&camera, labels.as_ref())
            .iter()
            .flat_map(|tile| [u32::from(tile.z), tile.x as u32, tile.y as u32])
            .collect())
    }

    /// Uploads what [`TileDecoder::decode`] returned: a tile's roads, buildings and labels.
    pub fn upload(&mut self, z: u8, x: u32, y: u32, tile: &[u8]) -> Result<(), JsValue> {
        self.renderer
            .upload_tile(TileId::new(z, x as i32, y as i32), tile)
            .map_err(js_error)
    }

    /// Draws at a new size from the next frame, keeping every tile.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), JsValue> {
        self.renderer.resize(width, height).map_err(js_error)
    }

    /// The map alone, for a top-down widget that strokes its route over it. `labels` is the cameras of
    /// the label placements and their blend, or empty for none.
    pub fn draw(
        &mut self,
        camera: &[f64],
        labels: &[f64],
        corner_radius_px: f32,
    ) -> Result<(), JsValue> {
        let camera = parse_camera(camera).map_err(js_error)?;
        let labels = parse_labels(labels).map_err(js_error)?;
        self.renderer
            .present(&camera, labels.as_ref(), None, corner_radius_px)
            .map_err(js_error)
    }

    /// The map with its route under the buildings, for a chase camera.
    pub fn draw_with_route(
        &mut self,
        camera: &[f64],
        labels: &[f64],
        route_style: &[f32],
        travelled_metres: f32,
        corner_radius_px: f32,
    ) -> Result<(), JsValue> {
        let camera = parse_camera(camera).map_err(js_error)?;
        let labels = parse_labels(labels).map_err(js_error)?;
        let line = self
            .route
            .clone()
            .ok_or_else(|| js_error("A map layer drew before its route".to_string()))?;
        let &[c0, c1, c2, c3, r0, r1, r2, r3, width_px] = route_style else {
            return Err(js_error(format!(
                "A map route style has {ROUTE_STYLE_VALUES} values, got {}",
                route_style.len()
            )));
        };
        let route = MapRouteDraw {
            line,
            style: MapRouteStyle {
                completed_color: [c0, c1, c2, c3],
                remaining_color: [r0, r1, r2, r3],
                width_px,
            },
            travelled_metres,
        };
        self.renderer
            .present(&camera, labels.as_ref(), Some(&route), corner_radius_px)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
pub struct TileDecoder(MapTileDecoder);

#[wasm_bindgen]
impl TileDecoder {
    /// The bundled font files a decoder for this style takes, in order, by the names the shell serves
    /// them under. Empty when the style draws no labels.
    pub fn font_files(config: &str, style: &[u8]) -> Result<Vec<String>, JsValue> {
        let config = parse_config(config).map_err(js_error)?;
        Ok(MapTileDecoder::font_files(&config, style)
            .map_err(js_error)?
            .into_iter()
            .map(str::to_owned)
            .collect())
    }

    /// `fonts` holds the bytes of [`Self::font_files`], in order.
    #[wasm_bindgen(constructor)]
    pub fn new(
        config: &str,
        style: &[u8],
        fonts: Vec<js_sys::Uint8Array>,
    ) -> Result<TileDecoder, JsValue> {
        console_error_panic_hook::set_once();
        let config = parse_config(config).map_err(js_error)?;
        let fonts = fonts.iter().map(js_sys::Uint8Array::to_vec).collect();
        MapTileDecoder::new(&config, style, fonts)
            .map(Self)
            .map_err(js_error)
    }

    /// A tile's roads, buildings and labels, for [`MapLayer::upload`].
    pub fn decode(&self, tile: &[u8]) -> Result<Vec<u8>, JsValue> {
        self.0.decode(tile).map_err(js_error)
    }
}
