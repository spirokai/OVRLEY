//! MP4 embedded telemetry extraction via telemetry-parser.
//!
//! Telemetry-parser reads the camera-native metadata track (GoPro GPMF, DJI
//! protobuf, Insta360 time-scalars, etc.) and exposes it as vendor-agnostic
//! tag maps in [`GroupId`]/[`TagId`] buckets. This module converts those maps
//! into columnar telemetry for shared activity finalization. The pipeline:
//!
//! 1. `extract_native_samples()` converts tag maps into a [`NativeSample`] vec.
//! 2. `smooth_series()` applies zero-phase smoothing to GPS/IMU fields.
//! 3. `build_activity_columns()` aligns all metrics to GPS-cadence timestamps
//!    (or video FPS as fallback) via closest-in-time matching, then the shared
//!    activity finalizer derives gaps, metrics, and the final [`ParsedActivity`].
//!
//! The [`ParsedActivity`] carries raw GPS course points and elevation -
//! **not** the simplified widget geometries. Route polylines and elevation
//! plots are computed separately by [`crate::commands::route_geometry`] and
//! [`crate::commands::elevation_geometry`] (LTTB downsampling, RDP
//! simplification, equirectangular projection).
//!
//! Owns: `probe_video_metadata()`, [`extract_activity`].
//! Does not own: ffprobe binary discovery (see [`crate::encode::ffmpeg`]),
//!       activity interpolation/densification (see [`crate::activity`]),
//!       route/elevation geometry (see [`crate::commands`]),
//!       rendering or output encoding.
//!
//! # Sub-modules
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`extraction`] | Tag-map to [`NativeSample`] conversion, GPS/camera/IMU expansion |
//! | [`tags`] | Typed accessors for telemetry-parser `TagValue` variants |
//! | [`vendor`] | Vendor-specific camera metadata (GoPro, Insta360, DJI JSON) |
//! | [`smoothing`] | Zero-phase moving-average smoothing for continuous series |
//! | [`activity`] | Columnar telemetry assembly with closest-in-time alignment |

mod activity;
mod extraction;
mod smoothing;
mod tags;
mod vendor;

use std::fs::File;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use telemetry_parser::tags_impl::{GroupId, TagId};
use telemetry_parser::util::SampleInfo;
use telemetry_parser::Input;

use crate::activity::finalize::write_activity_debug_file;
use crate::encode::fps::Fps;
use crate::error::{CoreError, CoreResult};
use crate::media::native_sample::{NativeSample, TelemetrySeriesCounts};
use crate::media::time::{
    gps_unix_seconds_to_video_start_rfc3339, gpsu_millis_to_video_start_rfc3339,
};
use crate::media::{dji_ac004, Resolution, SourceVideoMetadata};

use tags::{extract_tag_u64, gps5_fix_is_usable, GOPRO_GPSU_TAG};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Probes source-video metadata with telemetry-parser.
pub fn probe_video_metadata(file_path: &str) -> CoreResult<SourceVideoMetadata> {
    let path = Path::new(file_path);
    let file_size = std::fs::metadata(path)
        .map_err(|source| CoreError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .len() as usize;

    let mut stream = File::open(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let vm = telemetry_parser::util::get_video_metadata(&mut stream, file_size)
        .map_err(|error| CoreError::Encode(format!("telemetry-parser metadata error: {error}")))?;

    let (fps_num, fps_den) = rational_fps_parts(vm.fps);

    let (camera_type, camera_model) = match File::open(path) {
        Ok(mut stream) => {
            let cancel_flag = Arc::new(AtomicBool::new(false));
            match Input::from_stream(&mut stream, file_size, path, |_| {}, cancel_flag) {
                Ok(input) => (
                    Some(input.camera_type().to_string()),
                    input.camera_model().cloned(),
                ),
                Err(_) => (None, None),
            }
        }
        Err(_) => (None, None),
    };

    Ok(SourceVideoMetadata {
        path: file_path.to_string(),
        duration: positive_f64(vm.duration_s),
        fps: positive_f64(vm.fps),
        fps_num,
        fps_den,
        resolution: Some(Resolution {
            width: vm.width as u64,
            height: vm.height as u64,
        }),
        creation_time: None,
        sync_time: None,
        codec_name: None,
        codec_long_name: None,
        codec_profile: None,
        pix_fmt: None,
        bits_per_raw_sample: None,
        bit_rate: None,
        has_audio: false,
        container_format: None,
        rotation_degrees: Some(vm.rotation),
        camera_type,
        camera_model,
    })
}

// ---------------------------------------------------------------------------
// Shared extraction pipeline
// ---------------------------------------------------------------------------

/// Source that produced the normalized native samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TelemetrySource {
    TelemetryParser,
    DjiAc004Fallback,
}

impl TelemetrySource {
    fn as_str(self) -> &'static str {
        match self {
            Self::TelemetryParser => "telemetry_parser",
            Self::DjiAc004Fallback => "dji_ac004_fallback",
        }
    }
}

/// Timeline selected by the activity adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimelineKind {
    GpsAnchored,
    VideoDerived,
}

impl TimelineKind {
    fn from_counts(counts: TelemetrySeriesCounts) -> Self {
        if counts.gps > 0 {
            Self::GpsAnchored
        } else {
            Self::VideoDerived
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::GpsAnchored => "gps_anchored",
            Self::VideoDerived => "video_derived",
        }
    }
}

/// Internal extraction result consumed by output adapters.
struct Mp4TelemetryExtraction {
    samples: Vec<NativeSample>,
    counts: TelemetrySeriesCounts,
    source: TelemetrySource,
    timeline: TimelineKind,
    camera_type: String,
    camera_model: Option<String>,
    sync_time: Option<String>,
    file_name: Option<String>,
}

/// Shared core: opens the file, runs telemetry-parser, handles the DJI AC004
/// fallback, extracts native samples, smooths GPS/IMU series, and records the
/// provenance consumed by the activity adapter.
fn extract_telemetry_data(
    repo_root: &Path,
    file_path: &str,
) -> CoreResult<Option<Mp4TelemetryExtraction>> {
    let path = Path::new(file_path);
    let file_size = std::fs::metadata(path)
        .map_err(|source| CoreError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .len() as usize;

    let mut stream = File::open(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let input = match Input::from_stream(&mut stream, file_size, path, |_| {}, cancel_flag) {
        Ok(input) => input,
        Err(error) => {
            log::info!("telemetry-parser cannot parse {file_path}: {error}");
            return Ok(None);
        }
    };
    dump_raw_telemetry_parser_output(file_path, &input.samples);

    let camera_type = input.camera_type().to_string();
    let camera_model = input.camera_model().cloned();
    let file_name = path.file_name().map(|n| n.to_string_lossy().to_string());

    // Try telemetry-parser first, fall back to DJI AC004 when needed
    let (mut samples, sync_time, camera_model, source) = match input.samples {
        Some(ref parser_samples) if !parser_samples.is_empty() => {
            let has_gps_group = parser_samples.iter().any(|s| {
                s.tag_map
                    .as_ref()
                    .is_some_and(|m| m.contains_key(&GroupId::GPS))
            });

            let extracted = extraction::extract_native_samples(parser_samples);

            if extracted.is_empty() || (camera_type == "DJI" && !has_gps_group) {
                let Some(dji) = dji_normalized_samples(repo_root, path)? else {
                    return Ok(None);
                };
                (
                    dji.samples,
                    dji.sync_time,
                    dji.device_name,
                    TelemetrySource::DjiAc004Fallback,
                )
            } else {
                let sync = extract_sync_time_from_samples(parser_samples);
                (
                    extracted,
                    sync,
                    camera_model,
                    TelemetrySource::TelemetryParser,
                )
            }
        }
        _ => {
            let Some(dji) = dji_normalized_samples(repo_root, path)? else {
                return Ok(None);
            };
            (
                dji.samples,
                dji.sync_time,
                dji.device_name,
                TelemetrySource::DjiAc004Fallback,
            )
        }
    };

    if samples.is_empty() {
        return Ok(None);
    }
    smoothing::smooth_series(&mut samples);
    let counts = count_series(&samples);

    Ok(Some(Mp4TelemetryExtraction {
        samples,
        counts,
        source,
        timeline: TimelineKind::from_counts(counts),
        camera_type,
        camera_model,
        sync_time,
        file_name,
    }))
}

fn count_series(samples: &[NativeSample]) -> TelemetrySeriesCounts {
    TelemetrySeriesCounts {
        gps: samples
            .iter()
            .filter(|sample| sample.has_gps_payload())
            .count(),
        imu: samples
            .iter()
            .filter(|sample| sample.g_force.is_some())
            .count(),
        camera: samples
            .iter()
            .filter(|sample| sample.has_camera_payload())
            .count(),
    }
}

/// Result from the DJI AC004 fallback decoder.
struct DjiFallbackResult {
    samples: Vec<NativeSample>,
    sync_time: Option<String>,
    device_name: Option<String>,
}

/// Runs the DJI AC004 fallback decoder and returns normalized samples + metadata.
fn dji_normalized_samples(repo_root: &Path, path: &Path) -> CoreResult<Option<DjiFallbackResult>> {
    let Some(telemetry) = dji_ac004::extract_from_video(repo_root, path)? else {
        return Ok(None);
    };

    let samples: Vec<_> = telemetry
        .samples
        .iter()
        .map(|sample| NativeSample {
            timestamp_ms: sample.timestamp_ms,
            timestamp: Some(sample.timestamp.clone()),
            latitude: Some(sample.latitude),
            longitude: Some(sample.longitude),
            altitude: Some(sample.altitude),
            speed: Some(sample.speed),
            heading: sample.heading,
            g_force: sample.g_force,
            ..NativeSample::default()
        })
        .collect();

    if samples.is_empty() {
        return Ok(None);
    }

    Ok(Some(DjiFallbackResult {
        samples,
        sync_time: telemetry.sync_time,
        device_name: telemetry.device_name,
    }))
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Extracts MP4 telemetry into a [`ParsedActivity`] with a single timeline.
///
/// The returned activity aligns all metrics (GPS, IMU, camera) to a single
/// `sample_elapsed_seconds` timeline keyed to GPS timestamps (or video FPS
/// when GPS is absent). Closest-in-time matching picks the IMU and camera
/// value for each anchor point - see shared activity finalization.
///
/// `fps` and `duration_s` come from [`probe_video_metadata`] and are only
/// used as a fallback when the file has no GPS data.
///
/// The returned [`ParsedActivity`] carries raw course points and elevation.
/// Route and elevation widget geometries are computed separately by
/// [`crate::commands::route_geometry`] and
/// [`crate::commands::elevation_geometry`].
pub fn extract_activity(
    repo_root: &Path,
    file_path: &str,
    fps: f64,
    duration_s: f64,
) -> CoreResult<Option<crate::activity::finalize::FinalizeActivityResponse>> {
    let Some(result) = extract_telemetry_data(repo_root, file_path)? else {
        if cfg!(debug_assertions) {
            let stem = Path::new(file_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("video");
            let debug_payload = serde_json::json!({
                "generated_at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                "file_name": Path::new(file_path).file_name().map(|n| n.to_string_lossy().to_string()),
                "file_format": "mp4",
                "telemetry": null,
                "reason": "no embedded telemetry found by telemetry-parser or DJI fallback",
            });
            write_activity_debug_file(
                repo_root,
                Some(&format!("{stem}-telemetry-extraction")),
                &debug_payload,
            );
        }
        return Ok(None);
    };

    let columns = activity::build_activity_columns(
        &result.samples,
        fps,
        duration_s,
        result.file_name,
        &result.camera_type,
        result.camera_model,
        result.sync_time,
        result.source.as_str(),
        result.timeline.as_str(),
        result.counts,
    );

    crate::activity::finalize::finalize_activity_columns(&columns, Some(repo_root)).map(Some)
}

/// Converts floating FPS metadata into the shared rational representation.
pub fn rational_fps_parts(fps: f64) -> (Option<u32>, Option<u32>) {
    match Fps::from_f64_fallback(fps) {
        Ok(fps) => (Some(fps.num), Some(fps.den)),
        Err(_) => (None, None),
    }
}

// Sync time
// ---------------------------------------------------------------------------

/// Infers video start time from the first acquired GPS timestamp.
fn extract_sync_time_from_samples(samples: &[SampleInfo]) -> Option<String> {
    for sample in samples {
        let Some(tag_map) = &sample.tag_map else {
            continue;
        };
        let Some(gps_map) = tag_map.get(&GroupId::GPS) else {
            continue;
        };
        let Some(tag) = gps_map.get(&TagId::Data) else {
            continue;
        };

        match &tag.value {
            telemetry_parser::tags_impl::TagValue::Vec_GpsData(gps_values) => {
                let Some(gps) = gps_values.get().first().filter(|gps| gps.is_acquired) else {
                    continue;
                };
                return Some(gps_unix_seconds_to_video_start_rfc3339(
                    gps.unix_timestamp,
                    sample.timestamp_ms,
                ));
            }
            telemetry_parser::tags_impl::TagValue::Vec_Vec_i32(rows) if !rows.get().is_empty() => {
                if gps5_fix_is_usable(gps_map) {
                    if let Some(unix_ms) = extract_tag_u64(gps_map, &TagId::Unknown(GOPRO_GPSU_TAG))
                    {
                        let first_row_ms = extract_tag_u64(gps_map, &TagId::TimestampUs)
                            .map(|stmp| stmp as f64 / 1000.0)
                            .unwrap_or(sample.timestamp_ms);
                        return Some(gpsu_millis_to_video_start_rfc3339(unix_ms, first_row_ms));
                    }
                }
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Debug output
// ---------------------------------------------------------------------------

/// Writes a debug dump of the raw telemetry-parser output.
///
/// Only active in debug builds. The file is written to
/// `debug/mp4telemetry/{stem}-telemetry-parser-raw.txt` for offline
/// inspection when diagnosing extraction issues.
#[cfg(debug_assertions)]
fn dump_raw_telemetry_parser_output(file_path: &str, samples: &Option<Vec<SampleInfo>>) {
    let debug_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .join("debug")
        .join("mp4telemetry");

    if let Err(error) = std::fs::create_dir_all(&debug_dir) {
        log::warn!("failed to create MP4 telemetry debug directory: {error}");
        return;
    }

    let stem = Path::new(file_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("video");
    let filename = format!("{stem}-telemetry-parser-raw.txt");
    let output_path = debug_dir.join(filename);

    if let Err(error) = std::fs::write(output_path, format!("{samples:#?}")) {
        log::warn!("failed to write MP4 telemetry parser debug output: {error}");
    }
}

#[cfg(not(debug_assertions))]
fn dump_raw_telemetry_parser_output(_file_path: &str, _samples: &Option<Vec<SampleInfo>>) {}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Treats non-positive probe values as absent.
fn positive_f64(value: f64) -> Option<f64> {
    (value.is_finite() && value > 0.0).then_some(value)
}
