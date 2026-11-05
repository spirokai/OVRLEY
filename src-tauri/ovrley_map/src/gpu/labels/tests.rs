//! Labels from a tile's bytes to pixels: layout, placement and a frame drawn on the GPU.

use super::placement::{place, LabelView};
use super::text::{Face, FontGlyph, GlyphMetrics};
use super::{GlyphLookup, GlyphService, LabelSymbol};
use crate::gpu::style::{parse_style, ParsedStyle};
use crate::gpu::tile::prepare_tile;
use crate::gpu::{GpuMapConfig, GpuMapRenderer, GpuPoolSizes};
use crate::mercator::world_to_lon_lat;
use crate::{
    view_tiles, MapCamera, MapDataSource, MapLayers, MapStyleBuildings, MapStyleId, MapStyleSpec,
    MapViewport, TileId, ViewState,
};
use geozero::mvt::{tile, Message, Tile};
use serde_json::{json, Value as Json};
use std::path::Path;
use std::sync::Arc;

const ZOOM: f64 = 14.0;

fn zigzag(value: i32) -> u32 {
    ((value << 1) ^ (value >> 31)) as u32
}

/// A `place` layer holding one named point per `(x, y, name)`, in a 4096-unit tile.
fn place_tile(points: &[(i32, i32, &str)]) -> Vec<u8> {
    let features = points
        .iter()
        .enumerate()
        .map(|(index, (x, y, _))| tile::Feature {
            tags: vec![0, index as u32],
            r#type: Some(tile::GeomType::Point as i32),
            geometry: vec![9, zigzag(*x), zigzag(*y)],
            ..Default::default()
        })
        .collect();
    let values = points
        .iter()
        .map(|(_, _, name)| tile::Value {
            string_value: Some((*name).to_string()),
            ..Default::default()
        })
        .collect();
    Tile {
        layers: vec![tile::Layer {
            version: 2,
            name: "place".to_string(),
            features,
            keys: vec!["name".to_string()],
            values,
            extent: Some(4096),
        }],
    }
    .encode_to_vec()
}

fn label_style(layout: Json) -> Vec<u8> {
    let mut label = json!({
        "id": "label",
        "type": "symbol",
        "source": "openmaptiles",
        "source-layer": "place",
        "layout": { "text-field": ["get", "name"], "text-font": ["Noto Sans Bold"], "text-size": 24 },
        "paint": { "text-color": "#000000", "text-halo-color": "#ff0000", "text-halo-width": 2 },
    });
    for (name, value) in layout.as_object().unwrap() {
        label["layout"][name] = value.clone();
    }
    serde_json::to_vec(&json!({
        "version": 8,
        "sources": { "openmaptiles": { "type": "vector", "url": "unused" } },
        "layers": [
            { "id": "background", "type": "background", "paint": { "background-color": "#ffffff" } },
            label,
        ],
    }))
    .unwrap()
}

/// Every glyph is a 10 by 14 unit box, with an advance of 12.
struct BoxGlyphs;

impl GlyphLookup for BoxGlyphs {
    fn glyph(&self, _: &str, character: char) -> Result<Option<FontGlyph>, String> {
        Ok((character != '?').then_some(FontGlyph {
            face: Face::NotoSansBold,
            bold: false,
            metrics: GlyphMetrics {
                width: 10.0,
                height: 14.0,
                left: 1.0,
                top: 8.0,
                advance: 12.0,
            },
        }))
    }
}

/// A style's parse and the symbols a tile lays out under it.
fn laid_out(style: &[u8], bytes: &[u8]) -> (ParsedStyle, Vec<LabelSymbol>) {
    let style = parse_style(
        style,
        MapLayers {
            buildings: false,
            labels: true,
            paths: true,
        },
        ZOOM,
        1.0,
        &MapStyleBuildings::None,
    )
    .unwrap();
    let symbols = prepare_tile(bytes, &style, Some(&BoxGlyphs))
        .unwrap()
        .labels;
    (style, symbols)
}

#[test]
fn each_point_in_the_tile_gets_one_symbol_and_a_name_no_face_covers_drops_its_label() {
    let bytes = place_tile(&[
        (1000, 1000, "Praha"),
        (5000, 1000, "Outside"),
        (-1, 1000, "Outside"),
        (2000, 2000, "Brno?"),
    ]);
    let (style, symbols) = laid_out(&label_style(json!({})), &bytes);

    assert_eq!(symbols.len(), 1);
    let symbol = &symbols[0];
    assert_eq!(symbol.anchor, [1000.0, 1000.0]);
    assert_eq!(symbol.quads.len(), "Praha".len());
    assert_eq!(symbol.layer, style.labels[0].index);
    // 24 px text on a 24 unit em is one pixel a unit. Five glyphs of 12 units, 3 px of padding each
    // side of the quads, and `text-padding` of 2 px around the text.
    assert_eq!(symbol.font_scale, 1.0);
    assert_eq!(symbol.bounds[2] - symbol.bounds[0], 5.0 * 12.0 + 2.0 * 2.0);
    assert_eq!(symbol.halo_width, 2.0);
    assert_eq!(symbol.fill, [0, 0, 0, 255]);
    assert_eq!(symbol.halo, [255, 0, 0, 255]);
}

fn camera_over(tile: TileId) -> MapCamera {
    let tiles = 2_f64.powi(i32::from(tile.z));
    let (lon, lat) = world_to_lon_lat(
        (f64::from(tile.x) + 0.5) / tiles,
        (f64::from(tile.y) + 0.5) / tiles,
    );
    MapCamera::new(lon, lat, ZOOM, 0.0, 0.0, 0.0).unwrap()
}

#[test]
fn placement_keeps_the_first_of_two_overlapping_labels_and_culls_what_is_far_off_screen() {
    let tile = TileId::new(14, 8000, 5000);
    let viewport = MapViewport::new(200, 120, 1.0).unwrap();
    let view = LabelView::new(&ViewState::new(&camera_over(tile), viewport), 200, 120);
    // 2048 is the tile's centre: where the camera looks.
    let bytes = place_tile(&[
        (2048, 2048, "Centre"),
        (2060, 2050, "Beside"),
        (2048, 2048 + 640, "Below"),
        (4000, 2048, "Far"),
    ]);

    let place_with = |layout: Json| {
        let (style, symbols) = laid_out(&label_style(layout), &bytes);
        let placed = place(&view, &style.labels, &[(tile, &symbols)]);
        placed
            .iter()
            .map(|placed| placed.symbol)
            .collect::<Vec<_>>()
    };
    // Layout order decides: Centre, then Beside collides with it, Below is 80 px down and clears it,
    // and Far is 244 px right, past the 100 px that labels are still placed beyond the viewport.
    assert_eq!(place_with(json!({})), [0, 2]);
    assert_eq!(place_with(json!({ "text-allow-overlap": true })), [0, 1, 2]);
    // Sort keys reorder them: now Beside goes first and Centre is the one left out.
    assert_eq!(
        place_with(
            json!({ "symbol-sort-key": ["case", ["==", ["get", "name"], "Beside"], -1, 0] })
        ),
        [1, 2]
    );
}

/// The bundled fonts, and a source that serves one tile for every address.
struct TestSource {
    style: Vec<u8>,
    tile: Vec<u8>,
    glyphs: Arc<GlyphService>,
}

impl MapDataSource for TestSource {
    fn style(&self, _: &MapStyleId) -> Result<Vec<u8>, String> {
        Ok(self.style.clone())
    }

    fn tile(&self, _: u8, _: u32, _: u32) -> Result<Vec<u8>, String> {
        Ok(self.tile.clone())
    }

    fn glyphs(&self) -> Arc<GlyphService> {
        self.glyphs.clone()
    }
}

/// Renders a frame centred on the tile's one label, with labels on or off.
fn render(labels: bool) -> (Vec<u8>, usize) {
    let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts/map");
    let source = TestSource {
        style: label_style(json!({ "text-size": 40 })),
        tile: place_tile(&[(2048, 2048, "Praha")]),
        glyphs: Arc::new(GlyphService::open(&fonts).unwrap()),
    };
    let tile = TileId::new(14, 8000, 5000);
    let viewport = MapViewport::new(240, 120, 1.0).unwrap();
    let camera = camera_over(tile);
    let mut renderer = GpuMapRenderer::new(
        GpuMapConfig {
            spec: MapStyleSpec {
                style: MapStyleId::new("test").unwrap(),
                layers: MapLayers {
                    buildings: false,
                    labels,
                    paths: true,
                },
            },
            viewport,
            sample_count: 1,
            style_zoom: ZOOM,
            pools: GpuPoolSizes::uniform(1000),
            buildings: MapStyleBuildings::None,
        },
        &source,
    )
    .unwrap();
    renderer
        .load_view(&view_tiles(&camera, viewport), &source)
        .unwrap();
    let rasterized = source.glyphs.rasterized();
    let mut pixels = Vec::new();
    renderer.render(&camera, None, 0.0, &mut pixels).unwrap();
    // Drawing a frame asks the glyph service for nothing.
    assert_eq!(source.glyphs.rasterized(), rasterized);
    (pixels, rasterized)
}

#[test]
fn a_frame_draws_a_labels_glyphs_and_halo_where_it_was_placed() {
    let (blank, _) = render(false);
    assert!(blank
        .chunks_exact(4)
        .all(|pixel| pixel == [255, 255, 255, 255]));

    let (labelled, glyphs) = render(true);
    assert_eq!(
        glyphs,
        "Praha"
            .chars()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    );
    let columns = |is: fn(&[u8]) -> bool| -> Vec<usize> {
        labelled
            .chunks_exact(4)
            .enumerate()
            .filter(|(_, pixel)| is(pixel))
            .map(|(at, _)| at % 240)
            .collect()
    };
    let ink = columns(|pixel| pixel[0] < 60 && pixel[1] < 60 && pixel[2] < 60);
    let halo = columns(|pixel| pixel[0] > 200 && pixel[1] < 120 && pixel[2] < 120);
    assert!(ink.len() > 150, "{} dark pixels", ink.len());
    assert!(halo.len() > 150, "{} red pixels", halo.len());
    // The word is centred on the screen, with margins either side.
    let (first, last) = (ink.iter().min().unwrap(), ink.iter().max().unwrap());
    assert!(*first > 30 && *last < 210, "ink spans {first}..{last}");
    assert!((120_i64 - (*first + *last) as i64 / 2).abs() < 8);
}
