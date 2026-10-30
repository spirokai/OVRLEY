//! Throwaway gate for the GPU map renderer (`ovrley_map_gpu`). Delete it once judged.
//!
//! Usage: `map_gpu_gate <cache_dir> <output_dir> <all|timings|stills|clip|fuzz|trace|gaps|widget|widget-smoke> [runs] [frames]`
//! Build: `cargo build --release -p app --bin map_gpu_gate --features map-gpu-gate`
//! Timings from a debug build mean nothing. Check other load before trusting them, and look at
//! the stills: a blank frame reads as very fast.

#[allow(dead_code)]
#[path = "../map_tile_service.rs"]
mod map_tile_service;

use image::ExtendedColorType;
use map_tile_service::MapTileService;
use ovrley_map::mercator::lon_lat_to_world;
use ovrley_map::{
    view_tiles, MapCamera, MapDataSource, MapLayers, MapSprite, MapStyle, MapStyleSpec,
    MapViewport, TileId, ViewPadding, ViewState,
};
use ovrley_map_gpu::{
    Buildings, FrameStats, GpuMapConfig, GpuMapRenderer, TileLoadReport, DEFAULT_POOL_ENTRIES,
};
use ovrley_core::fonts::preview_font_resource;
use ovrley_widgets::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::{
        scene::{validate_scene_presentation, ScenePresentationConfig, ValidatedScenePresentation},
        standard_widgets::widget_font_weight,
    },
    preview::PreviewFonts,
    preview_widget::{PreviewWidget, ValidatedWidget},
    render::pixels::read_surface_rgba,
    widgets::map::{
        config::MapConfig,
        frame_provider::{
            map_frame_image, MapFrame, MapFrameProvider, MapFrameRequest, UploadedMapFrames,
        },
        validation::validate_map_widget,
    },
};
use skia_safe::{surfaces, Color, Image, Surface};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
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
            "Usage: map_gpu_gate <cache_dir> <output_dir> <all|timings|stills|clip|fuzz|trace|gaps|widget|widget-smoke> [runs] [frames]"
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
    if mode == "widget" || mode == "widget-smoke" {
        widget_gate(&service, &output_dir, mode == "widget-smoke")?;
    }
    if mode == "gaps" {
        padding_check()?;
        pixel_ratio_check(&service, &output_dir)?;
        coverage(&service, &output_dir)?;
    }
    Ok(())
}

/// Gap 1: the camera's centre must land at `(0.5 + ratio / 2) * height`, at any pitch and bearing.
fn padding_check() -> Result<(), String> {
    let (lat, lon) = PATH[0];
    let (world_x, world_y) = lon_lat_to_world(lon, lat);
    for (ratio, pitch, bearing) in [
        (0.0, 0.0, 0.0),
        (0.0, 60.0, 30.0),
        (0.45, 0.0, 0.0),
        (0.45, 40.0, 120.0),
        (0.45, 60.0, 250.0),
    ] {
        let camera = MapCamera::new(lon, lat, ZOOM, pitch, bearing, ratio)?;
        let view = ViewState::new(&camera, MapViewport::new(800, 400, 1.0)?);
        let clip = view.ground_projection().clip(world_x, world_y);
        let x = (clip.x / clip.w * 0.5 + 0.5) * 800.0;
        let y = (0.5 - clip.y / clip.w * 0.5) * 400.0;
        println!(
            "padding {ratio} pitch {pitch} bearing {bearing}: centre at ({x:.3}, {y:.3}) of 800x400, expected ({:.3}, {:.3})",
            400.0,
            (0.5 + ratio * 0.5) * 400.0,
        );
    }
    Ok(())
}

fn renderer_for(
    service: &MapTileService,
    style: MapStyle,
    camera: &MapCamera,
    viewport: MapViewport,
) -> Result<(GpuMapRenderer, Vec<TileId>, Vec<TileLoadReport>), String> {
    let tiles = view_tiles(camera, viewport, ViewPadding::Tight);
    let (renderer, reports) = loaded_renderer(
        service,
        GpuMapConfig {
            spec: MapStyleSpec {
                style,
                layers: MapLayers::ALL,
            },
            viewport,
            sample_count: 4,
            style_zoom: camera.zoom(),
            pool_entries: DEFAULT_POOL_ENTRIES,
            buildings: Buildings::for_style(style),
        },
        &tiles,
    )?;
    Ok((renderer, tiles, reports))
}

/// Gap 2: a 2x frame downscaled must match the 1x frame of the same camera.
fn pixel_ratio_check(service: &MapTileService, output_dir: &Path) -> Result<(), String> {
    const SIZE: u32 = 500;
    for (style, pitch) in [
        (MapStyle::Liberty, 60.0),
        (MapStyle::Liberty, 0.0),
        (MapStyle::Positron, 40.0),
        (MapStyle::Dark, 60.0),
    ] {
        let (lat, lon) = at(250.0);
        let camera = MapCamera::new(lon, lat, ZOOM, pitch, 35.0, 0.45)?;
        let (mut single, single_tiles, _) =
            renderer_for(service, style, &camera, MapViewport::new(SIZE, SIZE, 1.0)?)?;
        let one = single.render(&camera, None)?.to_vec();
        let (mut double, double_tiles, _) = renderer_for(
            service,
            style,
            &camera,
            MapViewport::new(SIZE * 2, SIZE * 2, 2.0)?,
        )?;
        let two = double.render(&camera, None)?.to_vec();
        let down = box_downscale(&two, SIZE * 2, SIZE);
        let total: u64 = one
            .iter()
            .zip(&down)
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        let big = one
            .chunks_exact(4)
            .zip(down.chunks_exact(4))
            .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 48))
            .count();
        println!(
            "ratio {} pitch {pitch}: tiles 1x {} 2x {} same {}, mean abs diff {:.3} of 255, {:.2}% of pixels differ by more than 48",
            style.name(),
            single_tiles.len(),
            double_tiles.len(),
            single_tiles == double_tiles,
            total as f64 / one.len() as f64,
            big as f64 * 100.0 / f64::from(SIZE * SIZE),
        );
        let name = format!("ratio_{}_p{pitch:.0}", style.name());
        save_rgba(&output_dir.join(format!("{name}_1x.png")), &one, SIZE, SIZE)?;
        save_rgba(&output_dir.join(format!("{name}_2x.png")), &two, SIZE * 2, SIZE * 2)?;
        save_rgba(&output_dir.join(format!("{name}_2x_down.png")), &down, SIZE, SIZE)?;
    }
    Ok(())
}

fn box_downscale(pixels: &[u8], from: u32, to: u32) -> Vec<u8> {
    let factor = (from / to) as usize;
    let (from, to) = (from as usize, to as usize);
    let mut out = vec![0; to * to * 4];
    for y in 0..to {
        for x in 0..to {
            for channel in 0..4 {
                let mut sum = 0_u32;
                for dy in 0..factor {
                    for dx in 0..factor {
                        sum += u32::from(pixels[((y * factor + dy) * from + x * factor + dx) * 4 + channel]);
                    }
                }
                out[(y * to + x) * 4 + channel] = (sum / (factor * factor) as u32) as u8;
            }
        }
    }
    out
}

fn save_rgba(path: &Path, pixels: &[u8], width: u32, height: u32) -> Result<(), String> {
    image::save_buffer(path, pixels, width, height, ExtendedColorType::Rgba8)
        .map_err(|error| error.to_string())
}

/// Gap 3: what nothing has rendered yet. Every view must load without eviction.
fn coverage(service: &MapTileService, output_dir: &Path) -> Result<(), String> {
    /// Dense Zurich, and rural farmland and forest in the Emmental.
    const PLACES: [(&str, f64, f64); 2] = [("zurich", 47.3722, 8.5388), ("rural", 46.95, 7.78)];
    const ZOOMS: [f64; 6] = [10.0, 12.0, 14.0, 15.5, 17.0, 18.0];
    const SHAPES: [(&str, u32, u32); 3] = [("square", 700, 700), ("wide", 1200, 400), ("tall", 400, 1200)];
    let dir = output_dir.join("coverage");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    println!("place style zoom shape pitch padding | tiles evicted failed load_ms frame_ms vertices draws");
    let mut views = Vec::new();
    for zoom in ZOOMS {
        for shape in SHAPES {
            views.push((zoom, shape, 0.0, 0.0));
        }
    }
    // The follow camera's real views, including its deepest reach up the frame.
    for zoom in [14.0, 15.5, 17.0] {
        for pitch in [40.0, 60.0] {
            for shape in SHAPES {
                views.push((zoom, shape, pitch, 0.45));
            }
        }
    }
    for (place, lat, lon) in PLACES {
        for style in MapStyle::ALL {
            for &view in &views {
                if let Err(error) = coverage_view(service, &dir, place, lat, lon, style, view) {
                    println!(
                        "FAILED {place} {} z{} {} p{} pad{}: {error}",
                        style.name(),
                        view.0,
                        view.1 .0,
                        view.2,
                        view.3,
                    );
                }
            }
        }
    }
    Ok(())
}

/// One coverage view. Evictions and tile failures are recorded, not fatal, so a run shows them all.
fn coverage_view(
    service: &MapTileService,
    dir: &Path,
    place: &str,
    lat: f64,
    lon: f64,
    style: MapStyle,
    (zoom, (shape, width, height), pitch, padding): (f64, (&str, u32, u32), f64, f64),
) -> Result<(), String> {
    let camera = MapCamera::new(lon, lat, zoom, pitch, 30.0, padding)?;
    let viewport = MapViewport::new(width, height, 1.0)?;
    let tiles = view_tiles(&camera, viewport, ViewPadding::Tight);
    let started = Instant::now();
    let mut renderer = GpuMapRenderer::new(
        GpuMapConfig {
            spec: MapStyleSpec {
                style,
                layers: MapLayers::ALL,
            },
            viewport,
            sample_count: 4,
            style_zoom: zoom,
            pool_entries: DEFAULT_POOL_ENTRIES,
            buildings: Buildings::for_style(style),
        },
        service,
    )?;
    let reports = renderer.load_tiles(&tiles, service)?;
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    renderer.render(&camera, None)?;
    let started = Instant::now();
    let pixels = renderer.render(&camera, None)?.to_vec();
    let frame_ms = started.elapsed().as_secs_f64() * 1000.0;
    let stats = renderer.last_stats();
    let vertices: usize = reports
        .iter()
        .map(|report| report.ground_vertices + report.extrusion_vertices)
        .sum();
    let failed = reports
        .iter()
        .filter(|report| report.error.is_some())
        .count();
    println!(
        "{place} {} z{zoom} {shape} p{pitch} pad{padding} | {} {} {failed} {load_ms:.0} {frame_ms:.2} {vertices} {}+{}",
        style.name(),
        tiles.len(),
        renderer.evicted_layers(),
        stats.layer_draws,
        stats.extrusion_draws,
    );
    save_rgba(
        &dir.join(format!(
            "{place}_{}_z{zoom}_{shape}_p{pitch:.0}.png",
            style.name()
        )),
        &pixels,
        width,
        height,
    )
}

const WIDGET_ACTIVITY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../debug/activities/Evening_Ride-parse-debug.json"
);
const WIDGET_FONT: &str = "Inter.ttf";
const WIDGET_FONT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../fonts");
/// Scene units across the widget, drawn at scene scale 1 and 2, so 1000 px is the same map sharper.
const WIDGET_WIDTH: f32 = 500.0;
const WIDGET_RANGE_METERS: f32 = 400.0;
const WIDGET_START_SECOND: f64 = 900.0;
const WIDGET_FPS: f64 = 30.0;
const WIDGET_FRAMES: usize = 300;

#[derive(Default)]
struct FrameTimes {
    load_ms: Vec<f64>,
    render_ms: Vec<f64>,
    wrap_ms: Vec<f64>,
}

/// The real provider's shape: one renderer per spec, zoom and viewport, tiles loaded before drawing.
struct GateFrames {
    service: MapTileService,
    renderer: Mutex<Option<(GpuMapConfig, GpuMapRenderer)>>,
    times: Mutex<FrameTimes>,
    tile_failures: AtomicUsize,
}

impl GateFrames {
    fn new(service: &MapTileService) -> Self {
        Self {
            service: service.clone(),
            renderer: Mutex::new(None),
            times: Mutex::new(FrameTimes::default()),
            tile_failures: AtomicUsize::new(0),
        }
    }

    fn evicted_layers(&self) -> Result<u64, String> {
        let state = self.renderer.lock().map_err(|error| error.to_string())?;
        Ok(state
            .as_ref()
            .map_or(0, |(_, renderer)| renderer.evicted_layers()))
    }

    fn frame_pixels(&self, request: &MapFrameRequest) -> Result<Image, String> {
        let mut state = self.renderer.lock().map_err(|error| error.to_string())?;
        let same = state.as_ref().is_some_and(|(config, _)| {
            config.spec == request.spec
                && config.viewport == request.viewport
                && config.style_zoom == request.camera.zoom()
        });
        if !same {
            let config = GpuMapConfig {
                spec: request.spec,
                viewport: request.viewport,
                sample_count: 4,
                style_zoom: request.camera.zoom(),
                pool_entries: DEFAULT_POOL_ENTRIES,
                buildings: Buildings::for_style(request.spec.style),
            };
            let renderer = GpuMapRenderer::new(config.clone(), &self.service)?;
            *state = Some((config, renderer));
        }
        let (_, renderer) = state.as_mut().ok_or("Renderer was just created")?;
        let tiles = view_tiles(&request.camera, request.viewport, ViewPadding::Tight);
        let started = Instant::now();
        let reports = renderer.load_tiles(&tiles, &self.service)?;
        let load_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.tile_failures.fetch_add(
            reports.iter().filter(|report| report.error.is_some()).count(),
            Ordering::Relaxed,
        );
        let started = Instant::now();
        let pixels = renderer.render(&request.camera, Some(&request.route))?;
        let render_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let image = map_frame_image(
            request.viewport.width_px(),
            request.viewport.height_px(),
            pixels,
        )
        .map_err(|error| error.to_string())?;
        let wrap_ms = started.elapsed().as_secs_f64() * 1000.0;
        let mut times = self.times.lock().map_err(|error| error.to_string())?;
        times.load_ms.push(load_ms);
        times.render_ms.push(render_ms);
        times.wrap_ms.push(wrap_ms);
        Ok(image)
    }
}

impl MapFrameProvider for GateFrames {
    fn frame(&self, request: &MapFrameRequest) -> WidgetResult<MapFrame> {
        self.frame_pixels(request)
            .map(MapFrame::Ready)
            .map_err(WidgetError::Render)
    }
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn widget_config(style: MapStyle, camera_mode: &str, orientation: &str, pitch: f32) -> serde_json::Value {
    serde_json::json!({
        "value": "map", "x": 0.0, "y": 0.0, "width": WIDGET_WIDTH, "height": WIDGET_WIDTH,
        "font": WIDGET_FONT, "map_style": style.name(),
        "show_roads": true, "show_paths": true, "show_water": true, "show_buildings": true,
        "camera_mode": camera_mode, "orientation": orientation, "pitch": pitch,
        "range_meters": WIDGET_RANGE_METERS, "simplify_tolerance_px": 0.5,
        "show_full_activity": true,
        "completed_line_width": 5.0, "completed_line_color": "#ff4d1f", "completed_line_opacity": 1.0,
        "remaining_line_width": 4.0, "remaining_line_color": "#1f7bff", "remaining_line_opacity": 0.9,
        "marker_variant": "halo", "marker_variant_diameter": 22.0, "marker_size": 6.0,
        "marker_color": "#ff4d1f", "marker_opacity": 1.0,
    })
}

fn widget_scene(scale: f32) -> Result<ValidatedScenePresentation, String> {
    let scene: ScenePresentationConfig = serde_json::from_value(serde_json::json!({
        "width": 3840, "height": 2160, "scale": scale, "opacity": 1.0,
        "shadow_color": "#000000", "shadow_strength": 0.0, "shadow_distance": 0.0,
        "border_color": "#000000", "border_thickness": 0.0,
    }))
    .map_err(|error| error.to_string())?;
    validate_scene_presentation(scene).map_err(|error| error.to_string())
}

struct WidgetRun<'a> {
    fonts: &'a PreviewFonts,
    activity: &'a ActivitySampler,
    duration: f64,
}

impl WidgetRun<'_> {
    fn build(
        &self,
        config: &serde_json::Value,
        scale: f32,
        frames: Arc<dyn MapFrameProvider>,
    ) -> Result<(PreviewWidget, Surface), String> {
        let config: MapConfig =
            serde_json::from_value(config.clone()).map_err(|error| error.to_string())?;
        let validated = validate_map_widget(&config, 0).map_err(|error| error.to_string())?;
        let widget = PreviewWidget::from_validated(
            ValidatedWidget::Map {
                widget: &validated,
                frames,
            },
            &widget_scene(scale)?,
            &WidgetHost::export(None, self.duration, 0.0),
            self.fonts,
        )
        .map_err(|error| error.to_string())?;
        let size = (WIDGET_WIDTH * scale) as i32;
        let surface =
            surfaces::raster_n32_premul((size, size)).ok_or("Unable to allocate the gate surface")?;
        Ok((widget, surface))
    }

    fn draw(
        &self,
        widget: &mut PreviewWidget,
        surface: &mut Surface,
        activity: Option<&ActivitySampler>,
        second: f64,
        pixels: &mut [u8],
    ) -> Result<(), String> {
        surface.canvas().clear(Color::TRANSPARENT);
        widget
            .draw(surface.canvas(), activity, second, self.fonts)
            .map_err(|error| error.to_string())?;
        read_surface_rgba(surface, pixels).map_err(|error| error.to_string())
    }
}

/// Gate A: the real widget with the real renderer, as stills and a clip per configuration.
fn widget_gate(service: &MapTileService, output_dir: &Path, smoke: bool) -> Result<(), String> {
    let dir = output_dir.join("widget");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let document: serde_json::Value = serde_json::from_slice(
        &fs::read(WIDGET_ACTIVITY).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let parsed = document
        .get("parsed_activity")
        .ok_or("Activity has no parsed_activity")?;
    let duration = parsed["trim_end_seconds"]
        .as_f64()
        .ok_or("Activity has no trim_end_seconds")?;
    let activity = ActivitySampler::from_json(
        &serde_json::to_vec(parsed).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let mut fonts = PreviewFonts::default();
    fonts.set_grid_fit(true);
    fonts
        .register(
            serde_json::from_value(serde_json::json!({ "font": WIDGET_FONT }))
                .map_err(|error| error.to_string())?,
            &preview_font_resource(
                &[PathBuf::from(WIDGET_FONT_DIR)],
                WIDGET_FONT,
                widget_font_weight(),
                false,
            )
            .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    let run = WidgetRun {
        fonts: &fonts,
        activity: &activity,
        duration,
    };

    // The two states without a frame.
    let pending: Arc<dyn MapFrameProvider> = Arc::new(UploadedMapFrames::default());
    let config = widget_config(MapStyle::Fiord, "follow", "rider_up", 40.0);
    let (mut widget, mut surface) = run.build(&config, 1.0, pending)?;
    let mut pixels = vec![0; (WIDGET_WIDTH * WIDGET_WIDTH * 4.0) as usize];
    run.draw(&mut widget, &mut surface, Some(run.activity), WIDGET_START_SECOND, &mut pixels)?;
    save_rgba(&dir.join("state_pending_fiord.png"), &pixels, WIDGET_WIDTH as u32, WIDGET_WIDTH as u32)?;
    let pending: Arc<dyn MapFrameProvider> = Arc::new(UploadedMapFrames::default());
    let (mut widget, mut surface) = run.build(&config, 1.0, pending)?;
    run.draw(&mut widget, &mut surface, None, 30.0, &mut pixels)?;
    save_rgba(&dir.join("state_no_activity_fiord.png"), &pixels, WIDGET_WIDTH as u32, WIDGET_WIDTH as u32)?;

    let cameras: Vec<(String, serde_json::Value)> = if smoke {
        vec![(
            "rider_up_p60".to_string(),
            widget_config(MapStyle::Liberty, "follow", "rider_up", 60.0),
        )]
    } else {
        let mut cameras = vec![(
            "fixed".to_string(),
            widget_config(MapStyle::Liberty, "fixed", "north_up", 0.0),
        )];
        for orientation in ["north_up", "rider_up"] {
            for pitch in [0.0, 40.0, 60.0] {
                cameras.push((
                    format!("{orientation}_p{pitch:.0}"),
                    widget_config(MapStyle::Liberty, "follow", orientation, pitch),
                ));
            }
        }
        cameras
    };
    let styles: &[MapStyle] = if smoke {
        &[MapStyle::Liberty]
    } else {
        &MapStyle::ALL
    };
    let frames_per_clip = if smoke { 90 } else { WIDGET_FRAMES };
    println!("style size camera | load render wrap widget ms (medians) | evicted failed setup_ms");
    for &style in styles {
        for scale in [1.0_f32, 2.0] {
            let size = (WIDGET_WIDTH * scale) as u32;
            for (name, base) in &cameras {
                let mut config = base.clone();
                config["map_style"] = serde_json::json!(style.name());
                let frames = Arc::new(GateFrames::new(service));
                let (mut widget, mut surface) = run.build(&config, scale, frames.clone())?;
                let stem = format!("{}_{size}_{name}", style.name());
                let mut ffmpeg = Command::new("ffmpeg")
                    .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s"])
                    .arg(format!("{size}x{size}"))
                    .args(["-r", "30", "-i", "-", "-c:v", "libx264", "-crf", "18", "-pix_fmt", "yuv420p"])
                    .arg(dir.join(format!("{stem}.mp4")))
                    .stdin(Stdio::piped())
                    .spawn()
                    .map_err(|error| format!("Failed to start ffmpeg: {error}"))?;
                let mut stdin = ffmpeg.stdin.take().ok_or("ffmpeg has no stdin")?;
                let mut pixels = vec![0; size as usize * size as usize * 4];
                let mut total_ms = Vec::with_capacity(frames_per_clip);
                let setup = Instant::now();
                let mut setup_ms = 0.0;
                for frame in 0..frames_per_clip {
                    let second = WIDGET_START_SECOND + frame as f64 / WIDGET_FPS;
                    let started = Instant::now();
                    run.draw(&mut widget, &mut surface, Some(run.activity), second, &mut pixels)?;
                    total_ms.push(started.elapsed().as_secs_f64() * 1000.0);
                    if frame == 0 {
                        setup_ms = setup.elapsed().as_secs_f64() * 1000.0;
                    }
                    if frame % (frames_per_clip / 2) == 0 || frame + 1 == frames_per_clip {
                        save_rgba(&dir.join(format!("{stem}_f{frame}.png")), &pixels, size, size)?;
                    }
                    stdin.write_all(&pixels).map_err(|error| error.to_string())?;
                }
                drop(stdin);
                let status = ffmpeg.wait().map_err(|error| error.to_string())?;
                if !status.success() {
                    return Err(format!("ffmpeg failed on {stem}: {status}"));
                }
                let mut times = frames.times.lock().map_err(|error| error.to_string())?;
                // The widget's own cost, including reading the surface back for the clip.
                let mut own: Vec<f64> = total_ms
                    .iter()
                    .enumerate()
                    .map(|(index, total)| {
                        total
                            - times.load_ms.get(index).unwrap_or(&0.0)
                            - times.render_ms.get(index).unwrap_or(&0.0)
                            - times.wrap_ms.get(index).unwrap_or(&0.0)
                    })
                    .collect();
                println!(
                    "{} {size} {name} | {:.2} {:.2} {:.2} {:.2} | {} {} {setup_ms:.0}",
                    style.name(),
                    median(&mut times.load_ms),
                    median(&mut times.render_ms),
                    median(&mut times.wrap_ms),
                    median(&mut own),
                    frames.evicted_layers()?,
                    frames.tile_failures.load(Ordering::Relaxed),
                );
            }
        }
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
    MapCamera::new(lon, lat, ZOOM, PITCH, heading(distance), 0.0)
}

fn frame_distance(frame: usize) -> f64 {
    frame as f64 * SPEED_METRES_PER_FRAME + START_METRES
}

/// Every tile the camera sees along `frames`, so timed frames never upload.
fn tiles_along_path(frames: usize, size: u32) -> Result<Vec<TileId>, String> {
    let mut tiles = BTreeSet::new();
    for frame in 0..frames {
        let camera = camera_at(frame_distance(frame))?;
        tiles.extend(view_tiles(&camera, square(size)?, ViewPadding::Tight));
    }
    Ok(tiles.into_iter().collect())
}

fn square(size: u32) -> Result<MapViewport, String> {
    MapViewport::new(size, size, 1.0)
}

fn config(style: MapStyle, size: u32, samples: u32, buildings: bool) -> GpuMapConfig {
    GpuMapConfig {
        spec: MapStyleSpec {
            style,
            layers: MapLayers::ALL,
        },
        viewport: MapViewport::new(size, size, 1.0).expect("gate sizes are valid"),
        sample_count: samples,
        style_zoom: ZOOM,
        pool_entries: DEFAULT_POOL_ENTRIES,
        buildings: if buildings {
            Buildings::for_style(style)
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
            renderer.render(&camera, None)?;
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
                .flat_map(|camera| {
                    view_tiles(
                        camera,
                        MapViewport::new(size, size, 1.0).expect("gate sizes are valid"),
                        ViewPadding::Tight,
                    )
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            for buildings in [false, true] {
                let (mut renderer, _) =
                    loaded_renderer(service, config(style, size, samples, buildings), &tiles)?;
                println!("{} on {}", style.name(), renderer.adapter_name());
                for distance in STILL_DISTANCES {
                    let pixels = renderer.render(&camera_at(distance)?, None)?;
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
            let pixels = renderer.render(&camera_at(frame_distance(frame))?, None)?;
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
    let real = view_tiles(&camera, square(1000)?, ViewPadding::Tight)[0];
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
    renderer.render(&camera, None)?;
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
