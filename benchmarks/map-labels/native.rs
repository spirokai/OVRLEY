//! Map label benchmark, native side. `--plan` writes the cases both backends replay (cameras, label
//! placements and tiles per frame) and needs no tiles. `--run` replays them through the export path:
//! `MapTileSizer` measures every tile (which lays labels out and rasterizes their glyphs), then
//! `GpuMapRenderer` loads each frame's view and renders it with a blocking readback.
use ovrley_map::gpu::labels::{Face, GlyphId, GlyphService};
use ovrley_map::gpu::{GpuMapConfig, GpuMapRenderer, GpuPoolSizes, MapTileSizer};
use ovrley_map::mercator::{lon_lat_to_world, world_to_lon_lat};
use ovrley_map::{
    frame_tiles, read_style_file, MapCamera, MapDataSource, MapLabelDraw, MapLayers, MapRouteDraw,
    MapRouteLine, MapRoutePoint, MapRouteStyle, MapStyleBuildings, MapStyleId, MapStyleSpec,
    MapViewport, TileId, LABEL_PLACEMENTS, TILE_SIZE,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use std::{fs, thread};

const FPS: f64 = 30.0;
const STYLE: &str = "liberty";
/// The map widget's defaults (`assets/standard-widgets.json`).
const MSAA_SAMPLES: u32 = 4;
const FOLLOW_ZOOM: f64 = 16.0;
const FOLLOW_PITCH: f64 = 50.0;
const FOLLOW_TOP_PADDING: f64 = 0.45;
const LINE_WIDTH: f32 = 6.0;
/// `ovrley_widgets::widgets::map::camera`: a fixed camera fits the route into 500 units with an 8% margin.
const FIXED_ROUTE_SPAN: f64 = 500.0 * (1.0 - 0.08 * 2.0);
const LABEL_STEP_SECONDS: f64 = 0.3;
/// Chase speed along the route.
const SPEED_MPS: f64 = 15.0;
/// Distance either side of a position whose chord gives the rider-up bearing.
const BEARING_REACH_M: f64 = 20.0;
const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// Waypoints as `lon, lat`. Dense urban: a loop through central Prague. Rural: farmland and small villages
/// in the Bohemian-Moravian Highlands. CJK urban: a loop around Shinjuku, Tokyo, with Japanese names.
const AREAS: [(&str, &[(f64, f64)]); 3] = [
    (
        "rural",
        &[
            (15.9020, 49.5480),
            (15.9150, 49.5530),
            (15.9290, 49.5560),
            (15.9420, 49.5620),
            (15.9530, 49.5700),
            (15.9640, 49.5760),
            (15.9790, 49.5790),
            (15.9900, 49.5730),
            (15.9960, 49.5630),
        ],
    ),
    (
        "urban",
        &[
            (14.4310, 50.0790),
            (14.4240, 50.0840),
            (14.4210, 50.0875),
            (14.4165, 50.0860),
            (14.4135, 50.0862),
            (14.4085, 50.0864),
            (14.4035, 50.0878),
            (14.4120, 50.0895),
            (14.4150, 50.0900),
            (14.4190, 50.0900),
            (14.4280, 50.0880),
            (14.4350, 50.0830),
            (14.4310, 50.0790),
        ],
    ),
    (
        "urban-cjk",
        &[
            (139.7003, 35.6896),
            (139.7020, 35.6830),
            (139.7115, 35.6812),
            (139.7203, 35.6801),
            (139.7302, 35.6860),
            (139.7357, 35.6911),
            (139.7230, 35.6930),
            (139.7046, 35.6909),
            (139.7003, 35.6896),
        ],
    ),
];
const SIZES: [u32; 2] = [400, 1000];
const CAMERAS: [&str; 2] = ["top-down", "chase"];

#[derive(Serialize, Deserialize)]
struct Plan {
    frames: usize,
    warmup: usize,
    style: String,
    style_buildings: MapStyleBuildings,
    cases: Vec<Case>,
    /// Every tile any case needs, as `[z, x, y]`.
    tiles: Vec<[u32; 3]>,
}

/// One area, size and camera. Each runs once without labels and once with them.
#[derive(Serialize, Deserialize)]
struct Case {
    id: String,
    area: String,
    size: u32,
    camera: String,
    zoom: f64,
    pixel_ratio: f32,
    /// `world_x, world_y, along_metres` per vertex, for a chase camera's route.
    route: Option<Vec<[f64; 3]>>,
    route_style: [f32; 9],
    frames: Vec<Frame>,
}

#[derive(Serialize, Deserialize)]
struct Frame {
    camera: [f64; 6],
    /// Label placement cameras, oldest first, then the blend.
    labels: Vec<f64>,
    travelled_metres: f32,
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["--plan", out, frames, warmup] => plan(
            Path::new(out),
            frames.parse().map_err(|_| "frames must be an integer")?,
            warmup.parse().map_err(|_| "warmup must be an integer")?,
        ),
        ["--run", plan, tiles, out] => run(Path::new(plan), Path::new(tiles), Path::new(out)),
        _ => Err("Usage: benchmark_map_labels --plan <out.json> <frames> <warmup> | --run <plan.json> <tiles dir> <out.json>".into()),
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn style_bytes() -> Result<Vec<u8>, String> {
    fs::read(root().join(format!("assets/map-styles/{STYLE}.json"))).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------------
// Plan

/// A route resampled every metre-ish, with cumulative distance.
struct Track {
    world: Vec<(f64, f64)>,
    along: Vec<f64>,
}

fn metres(a: (f64, f64), b: (f64, f64)) -> f64 {
    let ((lon1, lat1), (lon2, lat2)) = (world_to_lon_lat(a.0, a.1), world_to_lon_lat(b.0, b.1));
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = (p2 - p1, (lon2 - lon1).to_radians());
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

impl Track {
    fn new(waypoints: &[(f64, f64)]) -> Self {
        let mut world = Vec::new();
        for pair in waypoints.windows(2) {
            let (a, b) = (
                lon_lat_to_world(pair[0].0, pair[0].1),
                lon_lat_to_world(pair[1].0, pair[1].1),
            );
            let steps = (metres(a, b) / 5.0).ceil().max(1.0) as usize;
            for step in 0..steps {
                let t = step as f64 / steps as f64;
                world.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
            }
        }
        let last = waypoints[waypoints.len() - 1];
        world.push(lon_lat_to_world(last.0, last.1));
        let mut along = vec![0.0];
        for pair in world.windows(2) {
            along.push(along.last().unwrap() + metres(pair[0], pair[1]));
        }
        Self { world, along }
    }

    fn length(&self) -> f64 {
        *self.along.last().unwrap()
    }

    fn at(&self, distance: f64) -> (f64, f64) {
        let distance = distance.clamp(0.0, self.length());
        let index = self
            .along
            .partition_point(|&along| along <= distance)
            .clamp(1, self.world.len() - 1);
        let (a, b) = (self.world[index - 1], self.world[index]);
        let span = self.along[index] - self.along[index - 1];
        let t = if span > 0.0 {
            (distance - self.along[index - 1]) / span
        } else {
            0.0
        };
        (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
    }

    /// Clockwise from north.
    fn bearing(&self, distance: f64) -> f64 {
        let (a, b) = (
            self.at(distance - BEARING_REACH_M),
            self.at(distance + BEARING_REACH_M),
        );
        // World y grows southwards.
        (b.0 - a.0).atan2(a.1 - b.1).to_degrees()
    }

    fn chase_camera(&self, second: f64) -> Result<MapCamera, String> {
        let distance = (second * SPEED_MPS).rem_euclid(self.length());
        let world = self.at(distance);
        let (lon, lat) = world_to_lon_lat(world.0, world.1);
        MapCamera::new(
            lon,
            lat,
            FOLLOW_ZOOM,
            FOLLOW_PITCH,
            self.bearing(distance),
            FOLLOW_TOP_PADDING,
        )
    }

    /// `ovrley_widgets`' fixed camera at zoom offset 0 and no pan.
    fn fixed_camera(&self) -> Result<MapCamera, String> {
        let (mut min, mut max) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
        for point in &self.world {
            min = (min.0.min(point.0), min.1.min(point.1));
            max = (max.0.max(point.0), max.1.max(point.1));
        }
        let longest = (max.0 - min.0).max(max.1 - min.1);
        let zoom = (FIXED_ROUTE_SPAN / (longest * f64::from(TILE_SIZE)))
            .log2()
            .clamp(0.0, 19.0);
        let (lon, lat) = world_to_lon_lat((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0);
        MapCamera::new(lon, lat, zoom, 0.0, 0.0, 0.0)
    }

    /// `ovrley_widgets`' `WidgetCamera::label_draw` for a follow camera.
    fn chase_labels(&self, second: f64) -> Result<MapLabelDraw, String> {
        let steps = second / LABEL_STEP_SECONDS;
        let step = steps.floor();
        let at = |back: usize| self.chase_camera((step - back as f64) * LABEL_STEP_SECONDS);
        let mut cameras = [at(0)?; LABEL_PLACEMENTS];
        for (back, camera) in (1..LABEL_PLACEMENTS).rev().zip(&mut cameras) {
            *camera = at(back)?;
        }
        MapLabelDraw::new(cameras, (steps - step) as f32)
    }
}

fn plan(out: &Path, frames: usize, warmup: usize) -> Result<(), String> {
    let (info, _) = read_style_file(MapStyleId::new(STYLE)?, &style_bytes()?)
        .map_err(|errors| errors.join("; "))?;
    let mut cases = Vec::new();
    let mut all_tiles = BTreeSet::new();
    // Starts a little after the route's start, so the label placements' history is on the route.
    let start = 2.0;
    for (area, waypoints) in AREAS {
        let track = Track::new(waypoints);
        eprintln!("[plan] {area}: route {:.0} m", track.length());
        for size in SIZES {
            let viewport = MapViewport::new(size, size, 1.0)?;
            for camera_mode in CAMERAS {
                let chase = camera_mode == "chase";
                let fixed = track.fixed_camera()?;
                let mut case_frames = Vec::with_capacity(frames + warmup);
                for index in 0..frames + warmup {
                    let second = start + index as f64 / FPS;
                    let (camera, labels) = if chase {
                        (track.chase_camera(second)?, track.chase_labels(second)?)
                    } else {
                        (fixed, MapLabelDraw::new([fixed; LABEL_PLACEMENTS], 1.0)?)
                    };
                    let frame = frame_tiles(&camera, Some(&labels), viewport);
                    all_tiles.extend(frame.tiles);
                    let mut label_values: Vec<f64> = labels
                        .cameras()
                        .iter()
                        .flat_map(|camera| camera.values())
                        .collect();
                    label_values.push(f64::from(labels.blend()));
                    case_frames.push(Frame {
                        camera: camera.values(),
                        labels: label_values,
                        travelled_metres: ((second * SPEED_MPS).rem_euclid(track.length())) as f32,
                    });
                }
                let zoom = if chase { FOLLOW_ZOOM } else { fixed.zoom() };
                cases.push(Case {
                    id: format!("{area}-{size}-{camera_mode}"),
                    area: area.into(),
                    size,
                    camera: camera_mode.into(),
                    zoom,
                    pixel_ratio: 1.0,
                    route: chase.then(|| {
                        track
                            .world
                            .iter()
                            .zip(&track.along)
                            .map(|(world, along)| [world.0, world.1, *along])
                            .collect()
                    }),
                    // The widget's defaults: #ff4d1f at 100%, #1f7bff at 60%, 6 px.
                    route_style: [1.0, 0.302, 0.122, 1.0, 0.122, 0.482, 1.0, 0.6, LINE_WIDTH],
                    frames: case_frames,
                });
            }
        }
    }
    let plan = Plan {
        frames,
        warmup,
        style: STYLE.into(),
        style_buildings: info.buildings,
        cases,
        tiles: all_tiles
            .into_iter()
            .map(|tile| [u32::from(tile.z), tile.x as u32, tile.y as u32])
            .collect(),
    };
    eprintln!(
        "[plan] {} cases, {} tiles",
        plan.cases.len(),
        plan.tiles.len()
    );
    fs::write(out, serde_json::to_vec(&plan).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------------
// Run

struct Source {
    style: Vec<u8>,
    tiles: HashMap<TileId, Vec<u8>>,
    glyphs: Arc<GlyphService>,
}

impl MapDataSource for Source {
    fn style(&self, _style: &MapStyleId) -> Result<Vec<u8>, String> {
        Ok(self.style.clone())
    }

    fn tile(&self, z: u8, x: u32, y: u32) -> Result<Vec<u8>, String> {
        self.tiles
            .get(&TileId::new(z, x as i32, y as i32))
            .cloned()
            .ok_or_else(|| format!("Map tile {z}/{x}/{y} was not fetched; run --fetch"))
    }

    fn glyphs(&self) -> Arc<GlyphService> {
        self.glyphs.clone()
    }
}

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn camera(values: &[f64]) -> Result<MapCamera, String> {
    MapCamera::new(
        values[0], values[1], values[2], values[3], values[4], values[5],
    )
}

fn label_draw(values: &[f64]) -> Result<MapLabelDraw, String> {
    let cameras: Vec<MapCamera> = values[..values.len() - 1]
        .chunks_exact(6)
        .map(camera)
        .collect::<Result<_, _>>()?;
    MapLabelDraw::new(
        cameras
            .try_into()
            .map_err(|_| "wrong number of label cameras")?,
        values[values.len() - 1] as f32,
    )
}

fn run(plan_path: &Path, tiles_dir: &Path, out: &Path) -> Result<(), String> {
    let plan: Plan = serde_json::from_slice(&fs::read(plan_path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut tiles = HashMap::new();
    for [z, x, y] in &plan.tiles {
        let path = tiles_dir.join(format!("{z}/{x}/{y}.pbf"));
        let bytes =
            fs::read(&path).map_err(|error| format!("{}: {error}; run --fetch", path.display()))?;
        tiles.insert(TileId::new(*z as u8, *x as i32, *y as i32), bytes);
    }
    let fonts = root().join("fonts/map");
    let style = style_bytes()?;
    let mut results = Vec::new();
    let mut adapter = String::new();
    for case in &plan.cases {
        // Labels off first, then on, for each case.
        for labels in [false, true] {
            eprintln!("[native] {} labels={labels}", case.id);
            let result = run_case(
                &plan,
                case,
                labels,
                &style,
                &tiles,
                &fonts,
                &mut adapter,
                out,
            )?;
            results.push(result);
        }
    }
    let output = json!({
        "backend": "native",
        "adapter": adapter,
        "threads": thread::available_parallelism().map_or(1, usize::from),
        "results": results,
    });
    fs::write(
        out,
        serde_json::to_vec_pretty(&output).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn run_case(
    plan: &Plan,
    case: &Case,
    labels: bool,
    style: &[u8],
    tiles: &HashMap<TileId, Vec<u8>>,
    fonts: &Path,
    adapter: &mut String,
    out: &Path,
) -> Result<serde_json::Value, String> {
    let viewport = MapViewport::new(case.size, case.size, case.pixel_ratio)?;
    let spec = MapStyleSpec {
        style: MapStyleId::new(&plan.style)?,
        layers: MapLayers {
            buildings: true,
            labels,
            paths: true,
            local_names: false,
        },
    };
    // A fresh glyph service, so every case rasterizes its glyphs cold, as an export's first job does.
    let started = Instant::now();
    let glyphs = Arc::new(GlyphService::open(fonts)?);
    let glyph_open_ms = ms(started);
    let source = Source {
        style: style.to_vec(),
        tiles: tiles.clone(),
        glyphs: glyphs.clone(),
    };

    // The frames' labels and views, as export's frame requests carry them.
    let frames: Vec<(MapCamera, Option<MapLabelDraw>)> = case
        .frames
        .iter()
        .map(|frame| {
            Ok((
                camera(&frame.camera)?,
                labels.then(|| label_draw(&frame.labels)).transpose()?,
            ))
        })
        .collect::<Result<_, String>>()?;
    let mut views: Vec<Vec<TileId>> = Vec::new();
    for (camera, label_draw) in &frames {
        let view = frame_tiles(camera, label_draw.as_ref(), viewport).tiles;
        if views.last() != Some(&view) {
            views.push(view);
        }
    }
    let case_tiles: Vec<TileId> = views
        .iter()
        .flatten()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    // Export's preparation, single-threaded so it is comparable across cases: export spreads it over
    // every core. The second pass finds every glyph made, so the difference is rasterization.
    let config = GpuMapConfig {
        spec,
        viewport,
        sample_count: MSAA_SAMPLES,
        style_zoom: case.zoom,
        buildings: plan.style_buildings.clone(),
    };
    let sizer = MapTileSizer::new(&config, &source)?;
    let measure = |tile: &TileId| sizer.measure(&tiles[tile]);
    let started = Instant::now();
    let measured: Vec<_> = case_tiles.iter().map(measure).collect::<Result<_, _>>()?;
    let measure_cold_ms = ms(started);
    let rasterized = glyphs.rasterized();
    let started = Instant::now();
    for tile in &case_tiles {
        measure(tile)?;
    }
    let measure_warm_ms = ms(started);
    let job_glyphs: BTreeSet<GlyphId> = measured
        .iter()
        .flat_map(|m| m.glyphs.iter().copied())
        .collect();
    let sizes: HashMap<TileId, GpuPoolSizes> = case_tiles
        .iter()
        .copied()
        .zip(measured.iter().map(|m| m.pools))
        .collect();

    let started = Instant::now();
    let mut renderer =
        GpuMapRenderer::new(config, GpuPoolSizes::for_views(&sizes, &views), &source)?;
    let renderer_new_ms = ms(started);
    *adapter = renderer.adapter_name().to_string();
    let started = Instant::now();
    renderer.preload_glyphs(&job_glyphs, &source)?;
    let preload_glyphs_ms = ms(started);

    let route_line = case
        .route
        .as_ref()
        .map(|points| {
            MapRouteLine::new(
                points
                    .iter()
                    .map(|p| MapRoutePoint {
                        world_x: p[0],
                        world_y: p[1],
                        along_metres: p[2] as f32,
                    })
                    .collect(),
            )
            .map(Arc::new)
        })
        .transpose()?;
    let s = case.route_style;
    let route_style = MapRouteStyle {
        completed_color: [s[0], s[1], s[2], s[3]],
        remaining_color: [s[4], s[5], s[6], s[7]],
        width_px: s[8] * case.pixel_ratio,
    };

    // Each glyph of the job made again on a fresh service, timed by face: CJK glyphs are made at 2x.
    let fresh = GlyphService::open(fonts)?;
    let (mut cjk_ms, mut cjk, mut other_ms, mut other) = (0.0, 0, 0.0, 0);
    for glyph in &job_glyphs {
        let started = Instant::now();
        fresh.bitmap(glyph.face, glyph.character)?;
        if glyph.face == Face::CjkPack {
            (cjk_ms, cjk) = (cjk_ms + ms(started), cjk + 1);
        } else {
            (other_ms, other) = (other_ms + ms(started), other + 1);
        }
    }
    // Export makes every glyph before its first frame, so frames should make none.
    let made_before_frames = glyphs.rasterized();

    let mut pixels = Vec::new();
    let (mut load_ms, mut render_ms) = (Vec::new(), Vec::new());
    let mut first_load_ms = 0.0;
    for (index, ((camera, label_draw), frame)) in frames.iter().zip(&case.frames).enumerate() {
        let route = route_line.as_ref().map(|line| MapRouteDraw {
            line: line.clone(),
            style: route_style,
            travelled_metres: frame.travelled_metres,
        });
        let started = Instant::now();
        renderer.load_view(
            &frame_tiles(camera, label_draw.as_ref(), viewport).tiles,
            &source,
        )?;
        let load = ms(started);
        let started = Instant::now();
        renderer.render(
            camera,
            label_draw.as_ref(),
            route.as_ref(),
            0.0,
            &mut pixels,
        )?;
        let render = ms(started);
        if index == 0 {
            first_load_ms = load;
        }
        if index >= plan.warmup {
            load_ms.push(load);
            render_ms.push(render);
        }
    }
    // The last frame, straight RGBA, to check what was drawn.
    let snapshot = out.with_file_name(format!("native-{}-labels-{labels}.rgba", case.id));
    fs::write(&snapshot, &pixels).map_err(|e| e.to_string())?;
    Ok(json!({
        "case": case.id,
        "area": case.area,
        "size": case.size,
        "camera": case.camera,
        "labels": labels,
        "tiles": case_tiles.len(),
        "views": views.len(),
        "prepare": {
            "glyph_open_ms": glyph_open_ms,
            "measure_cold_ms": measure_cold_ms,
            "measure_warm_ms": measure_warm_ms,
            "glyphs_rasterized": rasterized,
            "job_glyphs": job_glyphs.len(),
            "renderer_new_ms": renderer_new_ms,
            "preload_glyphs_ms": preload_glyphs_ms,
            "first_load_view_ms": first_load_ms,
        },
        "frames": {
            "load_view_ms": load_ms,
            "render_ms": render_ms,
        },
        "glyph_faces": {
            "cjk": cjk,
            "cjk_ms": cjk_ms,
            "other": other,
            "other_ms": other_ms,
        },
        "glyphs_made_during_frames": glyphs.rasterized() - made_before_frames,
        "atlas_clears": renderer.atlas_clears(),
        "pool_clears": renderer.pool_clears(),
    }))
}
