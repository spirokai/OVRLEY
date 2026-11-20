//! Labels from a tile's bytes to pixels: layout, placement and a frame drawn on the GPU.

use super::layout::SymbolKind;
use super::placement::{label_sources, place, LabelSource, LabelView, PlacedSymbol};
use super::text::{Face, FontGlyph, GlyphMetrics};
use super::{GlyphLookup, GlyphService, LabelSymbol};
use crate::gpu::style::{parse_style, ParsedStyle};
use crate::gpu::tile::prepare_tile;
use crate::gpu::web::{decoded, MapTileDecoder, SentGlyphs};
use crate::gpu::{GpuMapConfig, GpuMapRenderer, GpuPoolSizes};
use crate::mercator::world_to_lon_lat;
use crate::{
    frame_tiles, view_tiles, MapCamera, MapDataSource, MapLabelDraw, MapLayers, MapStyleBuildings,
    MapStyleId, MapStyleSpec, MapViewport, TileId, LABEL_PLACEMENTS,
};
use geozero::mvt::{tile, Message, Tile};
use serde_json::{json, Value as Json};
use std::collections::HashMap;
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

/// A `transportation_name` layer holding one named line per `(points, name)`, in a 4096-unit tile.
fn line_tile(lines: &[(&[(i32, i32)], &str)]) -> Vec<u8> {
    let features = lines
        .iter()
        .enumerate()
        .map(|(index, (points, _))| {
            let mut geometry = vec![9, zigzag(points[0].0), zigzag(points[0].1)];
            geometry.push((points.len() as u32 - 1) << 3 | 2);
            for pair in points.windows(2) {
                geometry.push(zigzag(pair[1].0 - pair[0].0));
                geometry.push(zigzag(pair[1].1 - pair[0].1));
            }
            tile::Feature {
                tags: vec![0, index as u32],
                r#type: Some(tile::GeomType::Linestring as i32),
                geometry,
                ..Default::default()
            }
        })
        .collect();
    let values = lines
        .iter()
        .map(|(_, name)| tile::Value {
            string_value: Some((*name).to_string()),
            ..Default::default()
        })
        .collect();
    Tile {
        layers: vec![tile::Layer {
            version: 2,
            name: "transportation_name".to_string(),
            features,
            keys: vec!["name".to_string()],
            values,
            extent: Some(4096),
        }],
    }
    .encode_to_vec()
}

fn style_for(source_layer: &str, layout: Json) -> Vec<u8> {
    let mut label = json!({
        "id": "label",
        "type": "symbol",
        "source": "openmaptiles",
        "source-layer": source_layer,
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
            local_names: false,
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
    let (style, symbols) = laid_out(&style_for("place", json!({})), &bytes);

    assert_eq!(symbols.len(), 1);
    let symbol = &symbols[0];
    assert_eq!(symbol.anchor, [1000.0, 1000.0]);
    assert_eq!(symbol.quads.len(), "Praha".len());
    assert_eq!(symbol.layer, style.labels[0].index);
    // 24 px text on a 24 unit em is one pixel a unit. Five glyphs of 12 units, 3 px of padding each
    // side of the quads, and `text-padding` of 2 px around the text.
    assert_eq!(symbol.font_scale, 1.0);
    let SymbolKind::Point { bounds } = symbol.kind else {
        panic!("a place is a point label");
    };
    assert_eq!(bounds[2] - bounds[0], 5.0 * 12.0 + 2.0 * 2.0);
    assert_eq!(symbol.paint.halo_width, 2.0);
    assert_eq!(symbol.paint.fill, [0, 0, 0, 255]);
    assert_eq!(symbol.paint.halo, [255, 0, 0, 255]);
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
    let view = LabelView::new(&camera_over(tile), viewport);
    // 2048 is the tile's centre: where the camera looks.
    let bytes = place_tile(&[
        (2048, 2048, "Centre"),
        (2060, 2050, "Beside"),
        (2048, 2048 + 640, "Below"),
        (4000, 2048, "Far"),
    ]);

    let place_after = |layout: Json, previous: &[u32]| {
        let (style, symbols) = laid_out(&style_for("place", layout), &bytes);
        let previous: Vec<PlacedSymbol> = previous
            .iter()
            .map(|symbol| PlacedSymbol {
                layer: symbols[*symbol as usize].layer,
                tile,
                symbol: *symbol,
                flipped: false,
            })
            .collect();
        let placed = place(
            &view,
            &style.labels,
            &[LabelSource::whole(tile, &symbols)],
            Some(&previous),
        );
        placed
            .iter()
            .map(|placed| placed.symbol)
            .collect::<Vec<_>>()
    };
    let place_with = |layout: Json| place_after(layout, &[]);
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
    // What the last placement showed goes first: Beside keeps its place, and Centre stays out.
    assert_eq!(place_after(json!({}), &[1]), [1, 2]);
}

#[test]
fn a_name_follows_its_line_and_stays_upright() {
    let tile = TileId::new(14, 8000, 5000);
    let viewport = MapViewport::new(200, 120, 1.0).unwrap();
    let view = LabelView::new(&camera_over(tile), viewport);
    let layout = json!({ "symbol-placement": "line", "text-keep-upright": true });
    // 24 px text on 8 tile units a pixel: "East" is 48 px, 384 units.
    let placed = |lines: &[(&[(i32, i32)], &str)]| {
        let (style, symbols) = laid_out(
            &style_for("transportation_name", layout.clone()),
            &line_tile(lines),
        );
        let place_with = place(
            &view,
            &style.labels,
            &[LabelSource::whole(tile, &symbols)],
            None,
        );
        (symbols, place_with)
    };

    let east = [(300, 2048), (3800, 2048)];
    let west = [(3800, 2048), (300, 2048)];
    let (symbols, placed_east) = placed(&[(&east, "East")]);
    assert!(!symbols.is_empty());
    assert!(symbols.iter().all(|symbol| symbol.anchor[1] == 2048.0));
    assert!(!placed_east.is_empty() && placed_east.iter().all(|placed| !placed.flipped));
    // The same road drawn the other way round reads the same way on screen: it is flipped.
    let (_, placed_west) = placed(&[(&west, "West")]);
    assert!(!placed_west.is_empty() && placed_west.iter().all(|placed| placed.flipped));
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
    render_tile(
        style_for("place", json!({ "text-size": 40 })),
        place_tile(&[(2048, 2048, "Praha")]),
        labels,
    )
}

/// What a renderer over the test style draws: on a 240 by 120 viewport, with labels or without.
fn config(labels: bool) -> GpuMapConfig {
    GpuMapConfig {
        spec: MapStyleSpec {
            style: MapStyleId::new("test").unwrap(),
            layers: MapLayers {
                buildings: false,
                labels,
                paths: true,
                local_names: false,
            },
        },
        viewport: MapViewport::new(240, 120, 1.0).unwrap(),
        sample_count: 1,
        style_zoom: ZOOM,
        buildings: MapStyleBuildings::None,
    }
}

fn fonts() -> GlyphService {
    GlyphService::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts/map")).unwrap()
}

/// A renderer over a source that serves one tile for every address.
fn renderer_over(
    style: Vec<u8>,
    tile_bytes: Vec<u8>,
    labels: bool,
) -> (GpuMapRenderer, TestSource, MapViewport) {
    let source = TestSource {
        style,
        tile: tile_bytes,
        glyphs: Arc::new(fonts()),
    };
    let config = config(labels);
    let viewport = config.viewport;
    let renderer = GpuMapRenderer::new(config, GpuPoolSizes::uniform(1000), &source).unwrap();
    (renderer, source, viewport)
}

/// Draws one frame at `camera` with every tile its placements need loaded.
fn draw_frame(
    renderer: &mut GpuMapRenderer,
    source: &TestSource,
    viewport: MapViewport,
    camera: &MapCamera,
    labels: &MapLabelDraw,
) -> Vec<u8> {
    renderer
        .load_view(&frame_tiles(camera, Some(labels), viewport).tiles, source)
        .unwrap();
    let mut pixels = Vec::new();
    renderer
        .render(camera, Some(labels), None, 0.0, &mut pixels)
        .unwrap();
    pixels
}

fn render_tile(style: Vec<u8>, tile_bytes: Vec<u8>, labels: bool) -> (Vec<u8>, usize) {
    let (mut renderer, source, viewport) = renderer_over(style, tile_bytes, labels);
    let camera = camera_over(TileId::new(14, 8000, 5000));
    let still = MapLabelDraw::new([camera; LABEL_PLACEMENTS], 1.0).unwrap();
    renderer
        .load_view(&view_tiles(&camera, viewport), &source)
        .unwrap();
    let rasterized = source.glyphs.rasterized();
    let pixels = draw_frame(&mut renderer, &source, viewport, &camera, &still);
    // Drawing a frame asks the glyph service for nothing.
    assert_eq!(source.glyphs.rasterized(), rasterized);
    (pixels, rasterized)
}

#[test]
fn a_frame_draws_a_labels_glyphs_and_halo_where_it_was_placed() {
    let (blank, _) = render(false);
    assert!(blank
        .as_chunks::<4>().0.iter()
        .all(|pixel| *pixel == [255, 255, 255, 255]));

    let (labelled, glyphs) = render(true);
    assert_eq!(
        glyphs,
        "Praha"
            .chars()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    );
    let columns = |is: fn(&[u8; 4]) -> bool| -> Vec<usize> {
        labelled
            .as_chunks::<4>().0.iter()
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

#[test]
fn a_flipped_name_draws_exactly_as_one_that_needed_no_flip() {
    // The same 2080 unit street from either end, named at its middle. Drawn east to west the name is
    // flipped to stay upright, so both must put the same pixels on screen.
    let style = style_for(
        "transportation_name",
        json!({ "symbol-placement": "line-center", "text-size": 40 }),
    );
    let render_street =
        |points: &[(i32, i32)]| render_tile(style.clone(), line_tile(&[(points, "Praha")]), true).0;
    let east = render_street(&[(1008, 2048), (3088, 2048)]);
    let west = render_street(&[(3088, 2048), (1008, 2048)]);

    let ink = |pixels: &[u8]| {
        pixels
            .as_chunks::<4>().0.iter()
            .filter(|pixel| pixel[0] < 60 && pixel[1] < 60 && pixel[2] < 60)
            .count()
    };
    assert!(ink(&east) > 150, "{} dark pixels", ink(&east));
    let differing = east
        .iter()
        .zip(&west)
        .filter(|(a, b)| a.abs_diff(**b) > 8)
        .count();
    assert_eq!(differing, 0);
}

fn pitched_over(tile: TileId, pitch_deg: f64) -> MapCamera {
    let [lon, lat, zoom, _, bearing, padding] = camera_over(tile).values();
    MapCamera::new(lon, lat, zoom, pitch_deg, bearing, padding).unwrap()
}

#[test]
fn placement_leaves_out_labels_behind_the_camera_and_past_the_haze() {
    let tile = TileId::new(14, 8000, 5000);
    let viewport = MapViewport::new(300, 300, 1.0).unwrap();
    let view = LabelView::new(&pitched_over(tile, 60.0), viewport);
    let (style, symbols) = laid_out(
        &style_for("place", json!({ "text-allow-overlap": true })),
        &place_tile(&[(2048, 2048, "Praha")]),
    );
    // One label at the centre of each tile in a column: the camera's own, three north of it, and one
    // south of it. At pitch 60 the perspective ratio there falls to 0.63 and then 0.597: the last one
    // north is still on screen but past the haze. The one south is behind the camera.
    let rows = [5000, 4999, 4998, 4997, 5001];
    let placed = place(
        &view,
        &style.labels,
        &rows.map(|row| LabelSource::whole(TileId::new(14, 8000, row), symbols.as_slice())),
        None,
    );
    let mut placed: Vec<i32> = placed.iter().map(|placed| placed.tile.y).collect();
    placed.sort_unstable();
    assert_eq!(placed, [4998, 4999, 5000]);
}

/// The cameras of a frame that fades from a placement at `from` to one at `to`, with only `from` before.
fn fade(from: MapCamera, to: MapCamera) -> [MapCamera; LABEL_PLACEMENTS] {
    let mut cameras = [from; LABEL_PLACEMENTS];
    cameras[LABEL_PLACEMENTS - 1] = to;
    cameras
}

/// The ink a frame has: pixels dark in every channel.
fn black_pixels(pixels: &[u8]) -> usize {
    pixels
        .as_chunks::<4>().0.iter()
        .filter(|pixel| pixel[..3].iter().all(|channel| *channel < 45))
        .count()
}

fn marked_pixels(pixels: &[u8]) -> usize {
    pixels
        .as_chunks::<4>().0.iter()
        .filter(|pixel| pixel[..3] != [255, 255, 255])
        .count()
}

#[test]
fn a_label_placed_at_one_camera_fades_in_over_the_blend() {
    let style = style_for("place", json!({ "text-size": 40 }));
    let (mut renderer, source, viewport) =
        renderer_over(style, place_tile(&[(2048, 2048, "Praha")]), true);
    let near = camera_over(TileId::new(14, 8000, 5000));
    // Three tiles east, the label of the camera's own tile is not in view, so only `near` places it.
    let away = camera_over(TileId::new(14, 8003, 5000));
    let mut frame = |blend: f32| {
        let labels = MapLabelDraw::new(fade(away, near), blend).unwrap();
        draw_frame(&mut renderer, &source, viewport, &near, &labels)
    };

    assert_eq!(marked_pixels(&frame(0.0)), 0);
    let half = frame(0.5);
    assert!(marked_pixels(&half) > 150);
    assert_eq!(black_pixels(&half), 0, "half drawn text is not black");
    let full = frame(1.0);
    assert!(black_pixels(&full) > 150);
}

#[test]
fn a_frame_draws_the_same_when_drawn_again() {
    let style = style_for("place", json!({ "text-size": 40 }));
    let tile_bytes = place_tile(&[(2048, 2048, "Praha")]);
    let near = camera_over(TileId::new(14, 8000, 5000));
    let beside = camera_over(TileId::new(14, 8001, 5000));
    let away = camera_over(TileId::new(14, 8003, 5000));
    let first = (near, MapLabelDraw::new(fade(away, near), 0.25).unwrap());
    let second = (beside, MapLabelDraw::new(fade(near, beside), 0.75).unwrap());

    let (mut renderer, source, viewport) = renderer_over(style.clone(), tile_bytes.clone(), true);
    let mut draw = |(camera, labels): &(MapCamera, MapLabelDraw)| {
        draw_frame(&mut renderer, &source, viewport, camera, labels)
    };
    let (once, other, again) = (draw(&first), draw(&second), draw(&first));
    assert_ne!(once, other);
    assert!(once == again);

    // A lone label collides with nothing, so its placement history doesn't change it.
    let (mut fresh, source, viewport) = renderer_over(style, tile_bytes, true);
    let alone = draw_frame(&mut fresh, &source, viewport, &second.0, &second.1);
    assert!(alone == other);
}

/// A style's parse, for tests that lay a tile out themselves.
fn parsed(style: &[u8]) -> ParsedStyle {
    laid_out(style, &place_tile(&[])).0
}

/// What a preview decode worker sends for a tile, read as the UI thread reads it: the tile laid out
/// with a glyph service over the font files' bytes, and the bitmap of every glyph its labels draw.
fn sent_by_a_worker(
    opened: &GlyphService,
    style: &[u8],
    tile: &[u8],
) -> (crate::gpu::tile::PreparedTile, SentGlyphs) {
    let files = Face::ALL
        .iter()
        .map(|face| opened.font_file(face.file_name()).unwrap().to_vec())
        .collect();
    decoded(
        &MapTileDecoder::new(&config(true), style, files).unwrap(),
        tile,
    )
}

#[test]
fn labels_a_decode_worker_sends_are_those_laid_out_beside_the_font_files() {
    let service = fonts();
    let names = place_tile(&[
        (1000, 1000, "Praha"),
        (2000, 1000, "תל)"),
        (3000, 1000, "東京都"),
    ]);
    let streets = line_tile(&[
        (&[(300, 2048), (3800, 2048)], "East Street"),
        (&[(300, 600), (3800, 900)], "East Street"),
    ]);
    let cases = [
        (
            style_for("place", json!({ "text-transform": "uppercase" })),
            names,
        ),
        (
            style_for("transportation_name", json!({ "symbol-placement": "line" })),
            streets,
        ),
    ];
    for (style_bytes, tile) in cases {
        let (sent, glyphs) = sent_by_a_worker(&service, &style_bytes, &tile);
        let beside_fonts = prepare_tile(&tile, &parsed(&style_bytes), Some(&service))
            .unwrap()
            .labels;
        assert!(!beside_fonts.is_empty());
        assert!(sent.labels == beside_fonts);

        // Every glyph the labels draw comes with the bitmap export makes for it, once.
        let bitmaps: HashMap<_, _> = glyphs.iter().cloned().collect();
        assert_eq!(bitmaps.len(), glyphs.len());
        for quad in beside_fonts.iter().flat_map(|symbol| &symbol.quads) {
            let made = service
                .bitmap(quad.glyph.face, quad.glyph.character)
                .unwrap();
            assert_eq!(bitmaps[&quad.glyph], *made);
        }
    }
}

#[test]
fn a_parent_tile_leaves_out_its_anchors_inside_children_whose_labels_are_in() {
    let parent = TileId::new(13, 4000, 2500);
    let northwest_child = TileId::new(14, 8000, 5000);
    let viewport = MapViewport::new(200, 120, 1.0).unwrap();
    let camera = camera_over(parent);
    let view = LabelView::new(&camera, viewport);
    let layout = style_for("place", json!({ "text-allow-overlap": true }));
    // Both of the parent's names are within 40 px of the centre, in its northwest and southeast
    // quarters. The child holds the northwest one again, at the same place.
    let (style, parent_symbols) = laid_out(
        &layout,
        &place_tile(&[(1900, 1900, "NW"), (2200, 2200, "SE")]),
    );
    let (_, child_symbols) = laid_out(&layout, &place_tile(&[(3800, 3800, "NW")]));
    let tiles = view_tiles(&camera, viewport);
    assert_eq!(tiles.len(), 4);

    let placed = |resident: &HashMap<TileId, Vec<LabelSymbol>>| {
        let mut placed: Vec<(TileId, u32)> =
            place(&view, &style.labels, &label_sources(&tiles, resident), None)
                .iter()
                .map(|placed| (placed.tile, placed.symbol))
                .collect();
        placed.sort();
        placed
    };
    // Until a child loads, the parent holds the labels of the whole view.
    let mut resident = HashMap::from([(parent, parent_symbols)]);
    assert_eq!(placed(&resident), [(parent, 0), (parent, 1)]);
    // Once the northwest child's are in, its ground is labelled by it alone.
    resident.insert(northwest_child, child_symbols);
    assert_eq!(placed(&resident), [(parent, 1), (northwest_child, 0)]);
}

#[test]
fn a_frame_draws_glyphs_a_worker_sent_as_it_draws_those_export_loaded() {
    let style = style_for("place", json!({ "text-size": 40 }));
    let tile_bytes = place_tile(&[(2048, 2048, "Praha")]);
    let (loaded, _) = render_tile(style.clone(), tile_bytes.clone(), true);

    // The preview uploads what a worker sent, and fills the atlas from the bitmaps that came with it.
    let (mut renderer, source, _) = renderer_over(style.clone(), tile_bytes.clone(), true);
    let tile = TileId::new(14, 8000, 5000);
    let (prepared, glyphs) = sent_by_a_worker(&source.glyphs, &style, &tile_bytes);
    let bitmaps: HashMap<_, _> = glyphs
        .into_iter()
        .map(|(id, bitmap)| (id, Arc::new(bitmap)))
        .collect();
    renderer.upload_tile_layers(tile, prepared).unwrap();
    renderer
        .labels
        .as_mut()
        .unwrap()
        .fill_atlas(&renderer.queue, &renderer.loaded, &[tile], &|id| {
            Ok(Arc::clone(&bitmaps[&id]))
        })
        .unwrap();
    let camera = camera_over(tile);
    let still = MapLabelDraw::new([camera; LABEL_PLACEMENTS], 1.0).unwrap();
    let mut sent_frame = Vec::new();
    renderer
        .render(&camera, Some(&still), None, 0.0, &mut sent_frame)
        .unwrap();
    assert!(sent_frame == loaded);
}
