//! Throwaway Phase 1 gate for the map tile rasterizer. Delete it, and the `image`
//! dependency in Cargo.toml, once the gate has been judged.
//!
//! Usage: `map_tile_gate <cache_dir> <output_dir>`
//! Run with `--release`; timings from a debug build mean nothing.

#[allow(dead_code)]
#[path = "../map_tile_service.rs"]
mod map_tile_service;

use image::ExtendedColorType;
use map_tile_service::MapTileService;
use ovrley_core::MapDataSource;
use ovrley_map::mercator::{lon_lat_to_world, world_to_tile};
use ovrley_map::raster::{MapRasterizer, RasterizedTile};
use ovrley_map::style::translate;
use ovrley_map::{tile_pixels, MapLayers, MapStyle, MapStyleSpec, TileKey};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const PLACES: [(&str, f64, f64); 6] = [
    ("dense_zurich", 8.5417, 47.3769),
    ("motorway_limmattal", 8.4060, 47.4110),
    ("rural_furka", 8.4150, 46.5720),
    ("nonlatin_tokyo", 139.7671, 35.6812),
    ("nonlatin_cairo", 31.2357, 30.0444),
    ("ocean_atlantic", -40.0, 40.0),
];
const ZOOMS: [u8; 5] = [14, 15, 16, 17, 18];
const BLOCK: u32 = 3;

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (Some(cache_dir), Some(output_dir)) = (args.next(), args.next()) else {
        return Err("Usage: map_tile_gate <cache_dir> <output_dir>".to_string());
    };
    let output_dir = PathBuf::from(output_dir);
    fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;

    let service = MapTileService::for_application_cache(PathBuf::from(cache_dir))?;
    let mut report = String::new();

    translation_report(&service, &output_dir, &mut report)?;
    prefetch(&service)?;
    block_timings(&service, &output_dir, &mut report)?;
    ratio_timings(&service, &mut report)?;
    tile_size_comparison(&service, &output_dir, &mut report)?;
    layer_toggles(&service, &output_dir)?;
    parallel_timing(&service, &mut report)?;

    print!("{report}");
    fs::write(output_dir.join("report.txt"), report).map_err(|error| error.to_string())
}

fn all_layers(style: MapStyle) -> MapStyleSpec {
    MapStyleSpec {
        style,
        layers: MapLayers::ALL,
    }
}

/// Translation cost and warnings. Warnings go to a file per style.
fn translation_report(
    service: &MapTileService,
    output_dir: &Path,
    report: &mut String,
) -> Result<(), String> {
    writeln!(report, "== translation").unwrap();
    for style in MapStyle::ALL {
        let bytes = service.style(style)?;
        let started = Instant::now();
        let translated = translate(&bytes, MapLayers::ALL)?;
        writeln!(
            report,
            "{:<9} translate {:>6.1} ms  recipe {:>7} bytes  opaque {}  warnings {}",
            style.name(),
            started.elapsed().as_secs_f64() * 1000.0,
            translated.recipe.len(),
            translated.opaque,
            translated.warnings.len(),
        )
        .unwrap();
        fs::write(
            output_dir.join(format!("warnings_{}.txt", style.name())),
            translated.warnings.join("\n"),
        )
        .map_err(|error| error.to_string())?;

        let rasterizer = MapRasterizer::new();
        let (x, y) = place_tile(PLACES[0], 16);
        let started = Instant::now();
        rasterizer.render_tile(TileKey::new(all_layers(style), 16, x, y, 1000)?, service)?;
        writeln!(
            report,
            "{:<9} first render with renderer build {:>7.1} ms",
            "",
            started.elapsed().as_secs_f64() * 1000.0
        )
        .unwrap();
    }
    Ok(())
}

fn place_tile(place: (&str, f64, f64), z: u8) -> (u32, u32) {
    let (world_x, world_y) = lon_lat_to_world(place.1, place.2);
    world_to_tile(world_x, world_y, z)
}

/// Puts every data tile on disk first, so timings below never include the network.
fn prefetch(service: &MapTileService) -> Result<(), String> {
    for place in PLACES {
        let (x, y) = place_tile(place, 14);
        for data_y in y.saturating_sub(2)..=y + 2 {
            for data_x in x.saturating_sub(2)..=x + 2 {
                service.tile(14, data_x, data_y)?;
            }
        }
    }
    Ok(())
}

/// A block of distinct tiles per place, zoom and style: distinct because ezu's
/// render cache would flatter repeat renders of one tile. Also writes the block
/// as a mosaic, which is what shows seams and cross-tile labels.
fn block_timings(
    service: &MapTileService,
    output_dir: &Path,
    report: &mut String,
) -> Result<(), String> {
    writeln!(report, "\n== ms per tile, ratio 1, {BLOCK}x{BLOCK} block").unwrap();
    writeln!(
        report,
        "{:<18} {:<9} {:>3} {:>8} {:>8} {:>8}  alpha_ok",
        "place", "style", "z", "median", "max", "min"
    )
    .unwrap();
    let rasterizer = MapRasterizer::new();
    for place in PLACES {
        for style in MapStyle::ALL {
            for z in ZOOMS {
                let (center_x, center_y) = place_tile(place, z);
                let mut times = Vec::new();
                let mut tiles = Vec::new();
                let mut alpha_ok = true;
                for row in 0..BLOCK {
                    for column in 0..BLOCK {
                        let key = TileKey::new(
                            all_layers(style),
                            z,
                            center_x + column - BLOCK / 2,
                            center_y + row - BLOCK / 2,
                            1000,
                        )?;
                        let started = Instant::now();
                        let tile = rasterizer.render_tile(key, service)?;
                        times.push(started.elapsed().as_secs_f64() * 1000.0);
                        alpha_ok &=
                            !tile.opaque || tile.pixels.chunks_exact(4).all(|p| p[3] == 255);
                        tiles.push(tile);
                    }
                }
                times.sort_by(f64::total_cmp);
                writeln!(
                    report,
                    "{:<18} {:<9} {z:>3} {:>8.1} {:>8.1} {:>8.1}  {alpha_ok}",
                    place.0,
                    style.name(),
                    times[times.len() / 2],
                    times[times.len() - 1],
                    times[0],
                )
                .unwrap();
                write_mosaic(
                    &tiles,
                    BLOCK,
                    &output_dir.join(format!("{}_{}_z{z}.png", place.0, style.name())),
                )?;
            }
        }
    }
    Ok(())
}

fn write_mosaic(tiles: &[RasterizedTile], columns: u32, path: &Path) -> Result<(), String> {
    let size = tiles[0].size;
    let rows = tiles.len() as u32 / columns;
    let (width, height) = (size * columns, size * rows);
    let mut pixels = vec![0_u8; (width * height * 4) as usize];
    for (index, tile) in tiles.iter().enumerate() {
        let (column, row) = (index as u32 % columns, index as u32 / columns);
        for line in 0..size {
            let from = (line * size * 4) as usize;
            let to = (((row * size + line) * width + column * size) * 4) as usize;
            let length = (size * 4) as usize;
            pixels[to..to + length].copy_from_slice(&tile.pixels[from..from + length]);
        }
    }
    image::save_buffer(path, &pixels, width, height, ExtendedColorType::Rgba8)
        .map_err(|error| error.to_string())
}

/// Cost and texture size at the pixel ratios a scaled scene can reach.
fn ratio_timings(service: &MapTileService, report: &mut String) -> Result<(), String> {
    writeln!(
        report,
        "\n== pixel ratio extremes, dense, distinct tile per render"
    )
    .unwrap();
    writeln!(
        report,
        "{:<9} {:>3} {:>6} {:>6} {:>9} {:>8}",
        "style", "z", "ratio", "edge", "MB", "ms"
    )
    .unwrap();
    let rasterizer = MapRasterizer::new();
    for style in MapStyle::ALL {
        for z in [16, 18] {
            let (x, y) = place_tile(PLACES[0], z);
            for (offset, ratio_milli) in [1000_u16, 1500, 2000, 3000, 4000].into_iter().enumerate()
            {
                let key = TileKey::new(
                    all_layers(style),
                    z,
                    x + offset as u32 + 2,
                    y + 2,
                    ratio_milli,
                )?;
                let started = Instant::now();
                let tile = rasterizer.render_tile(key, service)?;
                assert_eq!(tile.size, tile_pixels(ratio_milli));
                writeln!(
                    report,
                    "{:<9} {z:>3} {:>6.2} {:>6} {:>9.1} {:>8.1}",
                    style.name(),
                    f32::from(ratio_milli) / 1000.0,
                    tile.size,
                    tile.pixels.len() as f64 / 1_048_576.0,
                    started.elapsed().as_secs_f64() * 1000.0,
                )
                .unwrap();
            }
        }
    }
    Ok(())
}

/// One 512 px tile against the four 256 px tiles that cover the same ground.
/// The style's zoom expressions read the tile zoom, so the 256 px version
/// evaluates them one level deeper. The images show what that does.
fn tile_size_comparison(
    service: &MapTileService,
    output_dir: &Path,
    report: &mut String,
) -> Result<(), String> {
    writeln!(report, "\n== tile size 512 vs 256, same ground, ratio 2").unwrap();
    let rasterizer = MapRasterizer::new();
    for style in [MapStyle::Liberty, MapStyle::Positron] {
        for view_zoom in [15_u8, 17] {
            let (x, y) = place_tile(PLACES[0], view_zoom);
            let spec = all_layers(style);

            let started = Instant::now();
            let large =
                rasterizer.render_tile(TileKey::new(spec, view_zoom, x, y, 2000)?, service)?;
            let large_ms = started.elapsed().as_secs_f64() * 1000.0;

            let started = Instant::now();
            let mut small = Vec::new();
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let key = TileKey::new(spec, view_zoom + 1, x * 2 + dx, y * 2 + dy, 2000)?;
                small.push(rasterizer.render_tile_sized(key, 256, service)?);
            }
            let small_ms = started.elapsed().as_secs_f64() * 1000.0;

            writeln!(
                report,
                "{:<9} view z{view_zoom}: 1x512 {large_ms:>7.1} ms {:>5.1} MB | 4x256 {small_ms:>7.1} ms {:>5.1} MB",
                style.name(),
                large.pixels.len() as f64 / 1_048_576.0,
                small.iter().map(|tile| tile.pixels.len()).sum::<usize>() as f64 / 1_048_576.0,
            )
            .unwrap();
            write_mosaic(
                std::slice::from_ref(&large),
                1,
                &output_dir.join(format!("tilesize_{}_z{view_zoom}_512.png", style.name())),
            )?;
            write_mosaic(
                &small,
                2,
                &output_dir.join(format!("tilesize_{}_z{view_zoom}_256.png", style.name())),
            )?;
        }
    }
    Ok(())
}

/// One image per toggle turned off, to check each group hides what it should.
fn layer_toggles(service: &MapTileService, output_dir: &Path) -> Result<(), String> {
    let toggles = [
        (
            "no_roads",
            MapLayers {
                roads: false,
                ..MapLayers::ALL
            },
        ),
        (
            "no_paths",
            MapLayers {
                paths: false,
                ..MapLayers::ALL
            },
        ),
        (
            "no_water",
            MapLayers {
                water: false,
                ..MapLayers::ALL
            },
        ),
        (
            "no_buildings",
            MapLayers {
                buildings: false,
                ..MapLayers::ALL
            },
        ),
        (
            "no_labels",
            MapLayers {
                labels: false,
                ..MapLayers::ALL
            },
        ),
    ];
    let rasterizer = MapRasterizer::new();
    let z = 16;
    let (x, y) = place_tile(PLACES[0], z);
    for (name, layers) in toggles {
        let spec = MapStyleSpec {
            style: MapStyle::Liberty,
            layers,
        };
        let tile = rasterizer.render_tile(TileKey::new(spec, z, x, y, 1000)?, service)?;
        write_mosaic(
            std::slice::from_ref(&tile),
            1,
            &output_dir.join(format!("toggle_{name}.png")),
        )?;
    }
    Ok(())
}

/// Renders on several threads over one rasterizer, as export workers will.
fn parallel_timing(service: &MapTileService, report: &mut String) -> Result<(), String> {
    writeln!(
        report,
        "\n== parallel, liberty, dense z17, 16 distinct tiles"
    )
    .unwrap();
    let z = 17;
    let (x, y) = place_tile(PLACES[0], z);
    let keys = (0..16)
        .map(|index| {
            TileKey::new(
                all_layers(MapStyle::Liberty),
                z,
                x + index % 4 + 8,
                y + index / 4 + 8,
                1000,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    for threads in [1_usize, 4, 8] {
        let rasterizer = MapRasterizer::new();
        // Build the renderer outside the measurement.
        rasterizer.render_tile(
            TileKey::new(all_layers(MapStyle::Liberty), z, x, y, 1000)?,
            service,
        )?;
        let started = Instant::now();
        std::thread::scope(|scope| {
            for chunk in keys.chunks(keys.len().div_ceil(threads)) {
                let rasterizer = &rasterizer;
                scope.spawn(move || {
                    for key in chunk {
                        rasterizer
                            .render_tile(*key, service)
                            .expect("parallel render");
                    }
                });
            }
        });
        writeln!(
            report,
            "{threads} threads: {:>7.1} ms total",
            started.elapsed().as_secs_f64() * 1000.0
        )
        .unwrap();
    }
    Ok(())
}
