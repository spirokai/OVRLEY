//! Throwaway gate for the GPU map renderer (`ovrley_map_gpu`). Delete it once judged.
//!
//! Usage: `map_gpu_gate <cache_dir> <output_dir> <all|timings|stills|clip|fuzz|trace> [runs] [frames]`
//! Build: `cargo build --release -p app --bin map_gpu_gate --features map-gpu-gate`
//! Timings from a debug build mean nothing. Check other load before trusting them, and look at
//! the stills: a blank frame reads as very fast.

#[allow(dead_code)]
#[path = "../map_tile_service.rs"]
mod map_tile_service;

use image::ExtendedColorType;
use map_tile_service::MapTileService;
use ovrley_map::{MapDataSource, MapLayers, MapSprite, MapStyle, MapStyleSpec};
use ovrley_map_gpu::{
    view_tiles, Buildings, FrameStats, GpuMapConfig, GpuMapRenderer, MapCamera, TileId,
    TileLoadReport, ViewPadding, DEFAULT_POOL_ENTRIES,
};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

/// The spike's camera path, so numbers compare directly with its baselines.
const PATH: [(f64, f64); 5] = [
    (47.3769, 8.5395),
    (47.3745, 8.5392),
    (47.3722, 8.5388),
    (47.3712, 8.5365),
    (47.3690, 8.5350),
];
const ZOOM: f64 = 17.0;
const PITCH: f64 = 60.0;
const FPS: f64 = 30.0;
const SPEED_METRES_PER_FRAME: f64 = 30.0 / 3.6 / FPS;
const START_METRES: f64 = 5.0;
const HEADING_WINDOW_METRES: f64 = 30.0;

const STYLES: [MapStyle; 4] = [
    MapStyle::Positron,
    MapStyle::Liberty,
    MapStyle::Bright,
    MapStyle::Dark,
];
const SIZES: [u32; 2] = [500, 1000];
const SAMPLE_COUNTS: [u32; 2] = [1, 4];
const DEFAULT_RUNS: usize = 3;
const DEFAULT_FRAMES: usize = 1500;
const CLIP_FRAMES: usize = 300;
const STILL_DISTANCES: [f64; 3] = [5.0, 250.0, 700.0];

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (Some(cache_dir), Some(output_dir), Some(mode)) = (args.next(), args.next(), args.next())
    else {
        return Err(
            "Usage: map_gpu_gate <cache_dir> <output_dir> <all|timings|stills|clip|fuzz|trace> [runs] [frames]"
                .to_string(),
        );
    };
    let runs = parse_count(args.next(), DEFAULT_RUNS)?;
    let frames = parse_count(args.next(), DEFAULT_FRAMES)?;
    let output_dir = PathBuf::from(output_dir);
    fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
    let service = MapTileService::for_application_cache(PathBuf::from(cache_dir))?;

    if mode == "all" || mode == "stills" {
        stills(&service, &output_dir)?;
    }
    if mode == "all" || mode == "timings" {
        let report = timings(&service, runs, frames)?;
        fs::write(output_dir.join("timings.txt"), report).map_err(|error| error.to_string())?;
    }
    if mode == "all" || mode == "clip" {
        clips(&service, &output_dir)?;
    }
    if mode == "all" || mode == "fuzz" {
        fuzz(&service)?;
    }
    if mode == "trace" {
        trace(&service, frames)?;
    }
    Ok(())
}

fn parse_count(value: Option<String>, default: usize) -> Result<usize, String> {
    value.map_or(Ok(default), |value| {
        value
            .parse()
            .map_err(|error| format!("Bad count {value}: {error}"))
    })
}

fn metres(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((b.1 - a.1) * 111320.0 * 47.37_f64.to_radians().cos()).hypot((b.0 - a.0) * 110540.0)
}

fn at(mut distance: f64) -> (f64, f64) {
    for i in 0..PATH.len() - 1 {
        let length = metres(PATH[i], PATH[i + 1]);
        if distance <= length || i + 2 == PATH.len() {
            let k = distance / length;
            return (
                PATH[i].0 + (PATH[i + 1].0 - PATH[i].0) * k,
                PATH[i].1 + (PATH[i + 1].1 - PATH[i].1) * k,
            );
        }
        distance -= length;
    }
    PATH[0]
}

fn heading(distance: f64) -> f64 {
    let (a, b) = (
        at((distance - HEADING_WINDOW_METRES).max(0.0)),
        at(distance + HEADING_WINDOW_METRES),
    );
    ((b.1 - a.1) * 47.37_f64.to_radians().cos())
        .atan2(b.0 - a.0)
        .to_degrees()
}

fn camera_at(distance: f64) -> Result<MapCamera, String> {
    let (lat, lon) = at(distance);
    MapCamera::new(lon, lat, ZOOM, PITCH, heading(distance))
}

fn frame_distance(frame: usize) -> f64 {
    frame as f64 * SPEED_METRES_PER_FRAME + START_METRES
}

/// Every tile the camera sees along `frames`, so timed frames never upload.
fn tiles_along_path(frames: usize, size: u32) -> Result<Vec<TileId>, String> {
    let mut tiles = BTreeSet::new();
    for frame in 0..frames {
        let camera = camera_at(frame_distance(frame))?;
        tiles.extend(view_tiles(&camera, size, size, ViewPadding::Tight));
    }
    Ok(tiles.into_iter().collect())
}

/// Liberty has real 3D buildings. The others get the `building` source layer with a fixed
/// extrusion, without touching the styles.
fn buildings_for(style: MapStyle) -> Buildings {
    let color = match style {
        MapStyle::Liberty => return Buildings::FromStyle,
        MapStyle::Positron => "rgb(228,228,222)",
        MapStyle::Bright | MapStyle::Fiord => "hsl(35,8%,85%)",
        MapStyle::Dark => "rgb(44,44,47)",
    };
    Buildings::Fixed(serde_json::json!({
        "id": "gate-building-3d",
        "type": "fill-extrusion",
        "source": "openmaptiles",
        "source-layer": "building",
        "minzoom": 14,
        "filter": ["!", ["has", "hide_3d"]],
        "paint": {
            "fill-extrusion-color": color,
            "fill-extrusion-height": ["get", "render_height"],
            "fill-extrusion-base": ["get", "render_min_height"],
        },
    }))
}

fn config(style: MapStyle, size: u32, samples: u32, buildings: bool) -> GpuMapConfig {
    GpuMapConfig {
        spec: MapStyleSpec {
            style,
            layers: MapLayers::ALL,
        },
        width: size,
        height: size,
        sample_count: samples,
        style_zoom: ZOOM,
        pool_entries: DEFAULT_POOL_ENTRIES,
        buildings: if buildings {
            buildings_for(style)
        } else {
            Buildings::None
        },
    }
}

/// A renderer with the whole path on the GPU. Fails if the timed frames could upload or miss data.
fn loaded_renderer(
    service: &MapTileService,
    config: GpuMapConfig,
    tiles: &[TileId],
) -> Result<(GpuMapRenderer, Vec<TileLoadReport>), String> {
    let mut renderer = GpuMapRenderer::new(config, service as &dyn MapDataSource)?;
    let reports = renderer.load_tiles(tiles, service)?;
    if let Some(failed) = reports.iter().find(|report| report.error.is_some()) {
        return Err(format!("Tile load failed: {failed:?}"));
    }
    if renderer.evicted_layers() > 0 {
        return Err(format!(
            "Pools too small: {} layers evicted while loading {} tiles",
            renderer.evicted_layers(),
            tiles.len()
        ));
    }
    Ok((renderer, reports))
}

struct RunResult {
    median: f64,
    p95: f64,
    max: f64,
}

fn time_frames(
    renderer: &mut GpuMapRenderer,
    frames: usize,
    with_copy: bool,
) -> Result<RunResult, String> {
    let mut times = Vec::with_capacity(frames);
    for frame in 0..frames {
        let camera = camera_at(frame_distance(frame))?;
        let started = Instant::now();
        if with_copy {
            renderer.render(&camera)?;
        } else {
            renderer.render_without_copy(&camera)?;
        }
        times.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    Ok(RunResult {
        median: times[times.len() / 2],
        p95: times[times.len() * 95 / 100],
        max: times[times.len() - 1],
    })
}

/// The full matrix. Each run opens its own device, which is what the device-loss count measures.
fn timings(service: &MapTileService, runs: usize, frames: usize) -> Result<String, String> {
    let mut report = String::new();
    let mut device_opens = 0;
    let mut device_failures = 0;
    macro_rules! say {
        ($($arg:tt)*) => {{
            let line = format!($($arg)*);
            println!("{line}");
            writeln!(report, "{line}").unwrap();
        }};
    }
    let path_tiles = SIZES
        .iter()
        .map(|size| tiles_along_path(frames, *size))
        .collect::<Result<Vec<_>, _>>()?;
    for (size, tiles) in SIZES.iter().zip(&path_tiles) {
        say!("{size} px: {} tiles along the path", tiles.len());
    }
    let probe = GpuMapRenderer::new(config(MapStyle::Liberty, 500, 1, true), service)?;
    say!(
        "adapter {}, pools {} MB",
        probe.adapter_name(),
        probe.pool_bytes() / 1_000_000
    );
    drop(probe);
    say!("ms per frame over {frames} frames, {runs} runs each, blocking readback. copy = pixels copied out; map = mapped only, as upstream's headless did");
    say!(
        "{:<9} {:>5} {:>4} {:>4} | {:>7} {:>7} {:>7} | {:>7} {:>7} | draws (tiles, layers, extrusions)",
        "style", "size", "bldg", "msaa", "map med", "map p95", "map max", "cpy med", "cpy p95"
    );

    let mut prepare = Vec::new();
    for style in STYLES {
        for (&size, tiles) in SIZES.iter().zip(&path_tiles) {
            for buildings in [false, true] {
                for samples in SAMPLE_COUNTS {
                    let mut map_runs = Vec::new();
                    let mut copy_runs = Vec::new();
                    let mut stats = FrameStats::default();
                    for _ in 0..runs {
                        device_opens += 1;
                        let result = (|| {
                            let (mut renderer, reports) = loaded_renderer(
                                service,
                                config(style, size, samples, buildings),
                                tiles,
                            )?;
                            let map = time_frames(&mut renderer, frames, false)?;
                            let copy = time_frames(&mut renderer, frames, true)?;
                            Ok::<_, String>((renderer.last_stats(), reports, map, copy))
                        })();
                        match result {
                            Ok((frame_stats, reports, map, copy)) => {
                                stats = frame_stats;
                                if samples == 1 {
                                    prepare.extend(
                                        reports
                                            .into_iter()
                                            .map(|report| (style, buildings, report)),
                                    );
                                }
                                map_runs.push(map);
                                copy_runs.push(copy);
                            }
                            Err(error) => {
                                device_failures += 1;
                                say!(
                                    "FAILED {} {size} {buildings} {samples}: {error}",
                                    style.name()
                                );
                            }
                        }
                    }
                    if map_runs.is_empty() {
                        continue;
                    }
                    let mid = |runs: &[RunResult], read: fn(&RunResult) -> f64| {
                        let mut values: Vec<f64> = runs.iter().map(read).collect();
                        values.sort_by(f64::total_cmp);
                        values[values.len() / 2]
                    };
                    let worst = map_runs.iter().map(|run| run.max).fold(0.0, f64::max);
                    say!(
                        "{:<9} {size:>5} {:>4} {samples:>4} | {:>7.2} {:>7.2} {worst:>7.2} | {:>7.2} {:>7.2} | {} {} {}",
                        style.name(),
                        if buildings { "yes" } else { "no" },
                        mid(&map_runs, |run| run.median),
                        mid(&map_runs, |run| run.p95),
                        mid(&copy_runs, |run| run.median),
                        mid(&copy_runs, |run| run.p95),
                        stats.tiles,
                        stats.layer_draws,
                        stats.extrusion_draws,
                    );
                }
            }
        }
    }

    say!("\nper-tile prepare, decode and tessellate on one worker, ms");
    say!(
        "{:<9} {:>4} {:>6} {:>8} {:>8} {:>8} {:>9} {:>9}",
        "style",
        "bldg",
        "tiles",
        "median",
        "max",
        "upload",
        "ground v",
        "bldg v"
    );
    for style in STYLES {
        for buildings in [false, true] {
            let mut rows: Vec<&TileLoadReport> = prepare
                .iter()
                .filter(|(s, b, _)| *s == style && *b == buildings)
                .map(|(_, _, report)| report)
                .collect();
            if rows.is_empty() {
                continue;
            }
            rows.sort_by(|a, b| a.prepare_ms.total_cmp(&b.prepare_ms));
            let densest = rows.iter().map(|r| r.ground_vertices).max().unwrap_or(0);
            let densest_buildings = rows.iter().map(|r| r.extrusion_vertices).max().unwrap_or(0);
            let upload = rows.iter().map(|r| r.upload_ms).fold(0.0, f64::max);
            say!(
                "{:<9} {:>4} {:>6} {:>8.1} {:>8.1} {upload:>8.1} {densest:>9} {densest_buildings:>9}",
                style.name(),
                if buildings { "yes" } else { "no" },
                rows.len(),
                rows[rows.len() / 2].prepare_ms,
                rows[rows.len() - 1].prepare_ms,
            );
        }
    }
    say!("\ndevices opened {device_opens}, failed runs {device_failures}");
    Ok(report)
}

/// Stills along the path, plus the arguments that draw the same camera with MapLibre Native.
fn stills(service: &MapTileService, output_dir: &Path) -> Result<(), String> {
    let mut manifest = String::new();
    for style in STYLES {
        for (&size, samples) in SIZES.iter().zip([1, 4]) {
            let tiles = STILL_DISTANCES
                .iter()
                .map(|distance| camera_at(*distance))
                .collect::<Result<Vec<_>, _>>()?
                .iter()
                .flat_map(|camera| view_tiles(camera, size, size, ViewPadding::Tight))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            for buildings in [false, true] {
                let (mut renderer, _) =
                    loaded_renderer(service, config(style, size, samples, buildings), &tiles)?;
                println!("{} on {}", style.name(), renderer.adapter_name());
                for distance in STILL_DISTANCES {
                    let pixels = renderer.render(&camera_at(distance)?)?;
                    let name = format!(
                        "{}_{size}_{}_d{distance:.0}.png",
                        style.name(),
                        if buildings { "3d" } else { "flat" },
                    );
                    image::save_buffer(
                        output_dir.join(&name),
                        pixels,
                        size,
                        size,
                        ExtendedColorType::Rgba8,
                    )
                    .map_err(|error| error.to_string())?;
                    let (lat, lon) = at(distance);
                    writeln!(
                        manifest,
                        "{name} {} {lat} {lon} {ZOOM} {PITCH} {} {size} {}",
                        style.name(),
                        heading(distance),
                        u8::from(!buildings),
                    )
                    .unwrap();
                }
            }
        }
    }
    fs::write(output_dir.join("reference_cameras.txt"), manifest).map_err(|error| error.to_string())
}

/// A ten-second clip per style at 1000 px with buildings and MSAA, for judging by eye.
fn clips(service: &MapTileService, output_dir: &Path) -> Result<(), String> {
    const SIZE: u32 = 1000;
    let tiles = tiles_along_path(CLIP_FRAMES, SIZE)?;
    for style in STYLES {
        let (mut renderer, _) = loaded_renderer(service, config(style, SIZE, 4, true), &tiles)?;
        let path = output_dir.join(format!("{}_{SIZE}_3d.mp4", style.name()));
        let mut ffmpeg = Command::new("ffmpeg")
            .args([
                "-y",
                "-loglevel",
                "error",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "-s",
            ])
            .arg(format!("{SIZE}x{SIZE}"))
            .args([
                "-r", "30", "-i", "-", "-c:v", "libx264", "-crf", "18", "-pix_fmt", "yuv420p",
            ])
            .arg(&path)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Failed to start ffmpeg: {error}"))?;
        let mut stdin = ffmpeg.stdin.take().ok_or("ffmpeg has no stdin")?;
        for frame in 0..CLIP_FRAMES {
            let pixels = renderer.render(&camera_at(frame_distance(frame))?)?;
            stdin.write_all(pixels).map_err(|error| error.to_string())?;
        }
        drop(stdin);
        let status = ffmpeg.wait().map_err(|error| error.to_string())?;
        if !status.success() {
            return Err(format!("ffmpeg failed on {}: {status}", path.display()));
        }
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// Real tiles with corrupted bytes, keyed by a made-up tile address.
struct CorruptedTiles<'a> {
    service: &'a MapTileService,
    bytes: Vec<Vec<u8>>,
}

impl MapDataSource for CorruptedTiles<'_> {
    /// The real style with one unreadable layer added, as external data could carry.
    fn style(&self, style: MapStyle) -> Result<Vec<u8>, String> {
        let mut document: serde_json::Value = serde_json::from_slice(&self.service.style(style)?)
            .map_err(|error| error.to_string())?;
        document["layers"]
            .as_array_mut()
            .ok_or("Style has no layers")?
            .push(serde_json::json!({
                "id": "gate-malformed",
                "type": "fill",
                "source": "openmaptiles",
                "source-layer": "water",
                "paint": { "fill-color": ["no-such-operator"] },
            }));
        serde_json::to_vec(&document).map_err(|error| error.to_string())
    }

    fn sprite(&self, style: MapStyle) -> Result<MapSprite, String> {
        self.service.sprite(style)
    }

    fn tile(&self, _z: u8, x: u32, _y: u32) -> Result<Vec<u8>, String> {
        self.bytes
            .get(x as usize)
            .cloned()
            .ok_or_else(|| format!("No corrupted tile {x}"))
    }
}

/// Tile data is external, so nothing may panic on it: truncated tiles, flipped bytes, and a
/// malformed style layer must be skipped. Renders afterwards, because the survivors draw too.
fn fuzz(service: &MapTileService) -> Result<(), String> {
    const VARIANTS: u32 = 400;
    let camera = camera_at(START_METRES)?;
    let real = view_tiles(&camera, 1000, 1000, ViewPadding::Tight)[0];
    let original = service.tile(real.z, real.x as u32, real.y as u32)?;
    let mut seed = 0x2545_f491_u32;
    let mut random = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let mut bytes = Vec::with_capacity(VARIANTS as usize);
    for variant in 0..VARIANTS {
        let mut tile = original.clone();
        match variant % 4 {
            0 => tile.truncate(random() as usize % tile.len()),
            1 => {
                for _ in 0..8 {
                    let at = random() as usize % tile.len();
                    tile[at] ^= 1 << (random() % 8);
                }
            }
            2 => {
                for _ in 0..200 {
                    let at = random() as usize % tile.len();
                    tile[at] = random() as u8;
                }
            }
            _ => tile = (0..tile.len().min(4096)).map(|_| random() as u8).collect(),
        }
        bytes.push(tile);
    }
    let source = CorruptedTiles { service, bytes };

    let mut renderer = GpuMapRenderer::new(config(MapStyle::Liberty, 500, 1, true), &source)?;
    let tiles: Vec<TileId> = (0..VARIANTS as i32)
        .map(|x| TileId::new(14, x, 0))
        .collect();
    let reports = renderer.load_tiles(&tiles, &source)?;
    let failed = reports
        .iter()
        .filter(|report| report.error.is_some())
        .count();
    renderer.render(&camera)?;
    println!(
        "fuzz: {VARIANTS} corrupted tiles without a panic: {} loaded, {failed} rejected, {} ground vertices kept",
        reports.len() - failed,
        reports.iter().map(|report| report.ground_vertices).sum::<usize>(),
    );
    Ok(())
}

/// Where slow frames come from: how they spread over time, at one setting known for a high p95
/// and one known for a low p95.
fn trace(service: &MapTileService, frames: usize) -> Result<(), String> {
    const SLOW_FACTOR: f64 = 1.6;
    for (size, samples) in [(1000, 4), (1000, 1), (500, 4)] {
        for pass in 0..2 {
            let tiles = tiles_along_path(frames, size)?;
            let (mut renderer, _) = loaded_renderer(
                service,
                config(MapStyle::Liberty, size, samples, true),
                &tiles,
            )?;
            let run_started = Instant::now();
            let mut samples_ms = Vec::with_capacity(frames);
            for frame in 0..frames {
                let camera = camera_at(frame_distance(frame))?;
                let started = Instant::now();
                renderer.render_without_copy(&camera)?;
                samples_ms.push((
                    run_started.elapsed().as_secs_f64() * 1000.0,
                    started.elapsed().as_secs_f64() * 1000.0,
                ));
            }
            let mut sorted: Vec<f64> = samples_ms.iter().map(|sample| sample.1).collect();
            sorted.sort_by(f64::total_cmp);
            let median = sorted[frames / 2];
            let slow: Vec<(usize, f64, f64)> = samples_ms
                .iter()
                .enumerate()
                .filter(|(_, sample)| sample.1 > median * SLOW_FACTOR)
                .map(|(frame, sample)| (frame, sample.0, sample.1))
                .collect();
            let longest = slow
                .iter()
                .fold(
                    (0_usize, 0_usize, 0_usize),
                    |(best, run, last), (frame, _, _)| {
                        let run = if *frame == last + 1 { run + 1 } else { 1 };
                        (best.max(run), run, *frame)
                    },
                )
                .0;
            let tenths: Vec<usize> = (0..10)
                .map(|tenth| {
                    slow.iter()
                        .filter(|(frame, _, _)| frame * 10 / frames == tenth)
                        .count()
                })
                .collect();
            println!(
                "{size} px msaa {samples} pass {pass}: median {median:.2} p95 {:.2} p99 {:.2} max {:.2}; {} slow frames, longest streak {longest}, by tenth of the run {tenths:?}",
                sorted[frames * 95 / 100],
                sorted[frames * 99 / 100],
                sorted[frames - 1],
                slow.len(),
            );
        }
    }
    Ok(())
}
