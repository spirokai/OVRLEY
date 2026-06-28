# MP4 Telemetry Integration Plan v3

## Goal

Integrate [telemetry-parser](https://github.com/AdrianEddy/telemetry-parser)
(Rust crate) for MP4 telemetry extraction while preserving the existing app
seams around preview import, activity finalization, and render preparation.
Two purposes:

1. Primary source for MP4 sync-relevant timestamps and embedded telemetry
   (GPS, camera settings, IMU).
2. Supplement the existing video probe path with telemetry-parser data, using
   ffprobe only as a salvage fallback when telemetry-parser fails or returns
   incomplete metadata needed by the app.

## Decisions

| #   | Decision                                                                                       | Rationale                                                                                                                                                                                                                                                |
| --- | ---------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D1  | Pin dependency to a reviewed telemetry-parser commit SHA                                       | Reproducible builds; avoids silent breakage from upstream HEAD. The exact SHA must be verified at implementation time instead of described as "latest" in the plan.                                                                                   |
| D2  | Preserve native telemetry cadence through extraction                                           | Avoids baking one scene FPS into the imported activity too early; the existing Rust trim+dense pipeline remains the canonical frame-alignment seam.                                                                                                   |
| D3  | Forward-backward moving average (FBMA) for smoothing                                           | Simple (~20 lines), zero-phase, no coefficient computation; upgrade path to Butterworth if insufficient                                                                                                                                                  |
| D4  | Camera settings (ISO, aperture, shutter, focal length, EV, color temp) excluded from smoothing | These are discrete values; smoothing would blur intentional jumps                                                                                                                                                                                        |
| D5  | GPS `unix_timestamp` inference is treated as primary sync-time source                          | The timestamp is only used to synchronize telemetry/video with activity data; do not model it as authoritative file creation metadata.                                                                                                                |
| D6  | ffprobe is a salvage fallback for missing sync/preview metadata                                | Handles non-GPS cameras, re-encoded videos, and preview-only fields that telemetry-parser may not expose (codec profile, pixel format, audio presence, bit depth, container format).                                                                |
| D7  | Rust returns normalized MP4 raw samples plus sync metadata, not fully finalized `ParsedActivity` | Keeps one canonical finalization path across FIT/GPX/SRT/MP4, avoids duplicating derivation rules, and still lets Rust own dense-telemetry extraction and smoothing where it is cheapest.                                                           |
| D8  | FIT/GPX always supersedes extracted telemetry                                                  | FIT/GPX is user-intent data; telemetry supplements only missing fields                                                                                                                                                                                   |
| D9  | Telemetry extraction runs automatically on video import                                        | User confirmed; no separate "extract" button needed                                                                                                                                                                                                      |
| D10 | GPS speed/track/altitude used as recorded (not derived from coordinates)                       | These are GPS chip Doppler/heading/altimeter readings, already firmware-smoothed                                                                                                                                                                         |
| D11 | Check `GpsData.is_acquired` before using GPS values                                            | Skip samples where GPS signal is lost; emit `null` in those positions                                                                                                                                                                                    |
| D12 | Frontend `finalizeParsedActivity()` remains canonical for now                                  | MP4 joins the existing import/finalization seam instead of creating a second source-specific activity assembly pipeline.                                                                                                                                |
| D13 | Source video parsing lives in `media/`, not `encode/`                                         | Probing and embedded telemetry extraction inspect imported source media; `encode/` remains focused on producing rendered output via FFmpeg.                                                                                                             |
| D14 | Smoothing horizons are top-of-file constants                                                  | Keeps telemetry tuning visible and avoids hidden magic numbers inside smoothing code.                                                                                                                                                                    |

## Telemetry-Parser Schema (Verified)

`tags.rs` is a macro-definition file included by `tags_impl.rs`. The actual schema
lives in `src/tags_impl.rs`:

- `GroupId` enum: GPS, Gyroscope, Accelerometer, Magnetometer, Exposure, Lens, …
- `TagId` enum: ISOValue, ShutterSpeed, IrisFStop, FocalLength, FrameRate, …
- `TagValue` enum: typed wrappers (u8..f64, String, bool, Vec_Vector3_i16,
  Vec_GpsData, …)
- `TagDescription`: `{ group, id, native_id, description, value }`
- `TagMap = BTreeMap<TagId, TagDescription>`
- `GroupedTagMap = BTreeMap<GroupId, TagMap>`
- `GpsData`: `{ is_acquired, unix_timestamp, lat, lon, speed, track, altitude }`
- `SampleInfo`: `{ sample_index, track_index, timestamp_ms, duration_ms,
video_rotation, tag_map }`

## Architecture

```
[User imports video]
    │
    ▼
backend_import_preview_video()
    │ 1. Probe with telemetry-parser first
    │ 2. Salvage missing preview/sync metadata from ffprobe if needed
    │ 3. Register in HTTP video server
    │ 4. Return metadata + preview_url
    │
    ▼
backend_extract_video_telemetry()  ← NEW command, called automatically
    │ 1. Parse file with telemetry-parser
    │ 2. Extract GPS/camera/IMU at native rate
    │ 3. Apply FBMA smoothing to dense continuous series only
    │ 4. Infer primary sync time from GPS when available
    │ 5. Return normalized MP4 raw samples + sync metadata
    │
    ▼
Frontend: finalize MP4 telemetry through the existing activity pipeline
    │ Reuse `finalizeParsedActivity()` (or a close sibling) as the canonical
    │ assembler for:
    │ - distance / progress
    │ - gradient / pace / heading fallback rules
    │ - metric units / coverage / valid attributes / extended attributes
    │ - canonical `ParsedActivity` shape
    │
    ▼
[User imports FIT/GPX] (optional, takes precedence)
    │ → Merge at the canonical ParsedActivity layer, not as a parallel
    │   late-render-only telemetry object
    │
    ▼
[Rust: trim + densify → DenseActivityReport]
    │ Existing frame-alignment seam remains authoritative
    │
    ▼
[Render: per-frame overlay]
```

## Canonical Boundary

The sustainable split is:

- Rust `media/` owns source video metadata and embedded telemetry parsing.
- Rust owns MP4-specific extraction, dense-series smoothing, and sync-time
  inference.
- Rust `encode/` remains the output-video encoding subsystem: render
  orchestration, FFmpeg command construction, segmented output, progress, and
  codec detection.
- JavaScript continues to own canonical activity finalization for all source
  formats in this phase.
- The existing Rust render-prep path continues to own scene trim and
  frame-rate densification.

This intentionally avoids introducing a second finalized-activity pipeline
that only MP4 uses.

## Telemetry → Raw Sample Mapping

The extraction step produces normalized raw samples for the frontend
finalization seam, not a fully assembled `ParsedActivity` object.

| telemetry-parser source         | Raw sample field             | Unit conversion                | Smoothing |
| ------------------------------- | ---------------------------- | ------------------------------ | --------- |
| `GPS.lat/lon`                   | `latitude`, `longitude`      | direct                         | no        |
| `GPS.altitude`                  | `altitude`, `elevation`      | meters                         | FBMA      |
| `GPS.speed`                     | `speed`                      | km/h → m/s (÷3.6)              | FBMA      |
| `GPS.track`                     | `heading`                    | degrees                        | FBMA      |
| `GPS.unix_timestamp`            | `timestamp`, `sync_start`    | unix → ISO 8601                | no        |
| `Exposure.ISOValue`             | `iso`                        | direct                         | **no**    |
| `Exposure.IrisFStop`            | `aperture`                   | direct                         | **no**    |
| `Exposure.ShutterSpeed`         | `shutterSpeed`               | direct                         | **no**    |
| `Lens.FocalLength`              | `focalLength`                | mm                             | **no**    |
| Accelerometer magnitude         | `gForce`                     | √(x²+y²+z²), subtract 1g       | FBMA      |

### MP4 Raw Sample JSON Shape (Rust Output)

Rust returns a normalized payload tailored for the existing frontend activity
finalizer:

```json
{
  "fileName": "GOPR0123.MP4",
  "fileFormat": "mp4_telemetry",
  "syncTime": "2025-07-23T10:21:41Z",
  "metadata": {
    "camera_type": "GoPro",
    "camera_model": "HERO 12",
    "telemetry_sample_count": 18000
  },
  "rawSamples": [
    {
      "timestamp": "2025-07-23T10:21:41Z",
      "latitude": 48.123,
      "longitude": 11.456,
      "altitude": 520.0,
      "elevation": 520.0,
      "speed": 5.2,
      "heading": 180.0,
      "iso": 100,
      "aperture": 2.8,
      "shutterSpeed": 0.0025,
      "focalLength": 24.0,
      "gForce": 0.1
    }
  ]
}
```

The frontend then calls `finalizeParsedActivity()` with MP4-specific options
such as skipping idle gap fill and preferring source-provided dense metrics.

## Sync Time Extraction

The inferred GPS timestamp is the preferred sync reference, not file
provenance metadata:

```rust
fn extract_sync_time(samples: &[SampleInfo]) -> Option<String> {
    for sample in samples {
        if let Some(ref tag_map) = sample.tag_map {
            if let Some(gps_map) = tag_map.get(&GroupId::GPS) {
                if let Some(tag) = gps_map.get(&TagId::Data) {
                    if let TagValue::Vec_GpsData(gps_vec) = &tag.value {
                        if let Some(first_gps) = gps_vec.get().first() {
                            if first_gps.is_acquired {
                                let start_unix = first_gps.unix_timestamp
                                    - sample.timestamp_ms / 1000.0;
                                return Some(unix_to_rfc3339(start_unix));
                            }
                        }
                    }
                }
            }
        }
    }
    None  // Caller may salvage a sync fallback from ffprobe if needed
}
```

### Survivability

| Scenario                      | GPS sync-time method                                | ffprobe salvage method         |
| ----------------------------- | --------------------------------------------------- | ------------------------------ |
| Original camera file          | Accurate                                            | Often acceptable               |
| Simple cut (`ffmpeg -c copy`) | Accurate (timestamps preserved in metadata track)   | Usually preserved              |
| Re-encode (Premiere, DaVinci) | **Lost** (metadata track stripped)                  | Often overwritten to export time |
| Trim start removed            | Reflects cut point (correct for remaining clip sync) | May reflect cut or original    |

## Smoothing: Forward-Backward Moving Average

```rust
const GPS_SPEED_SMOOTHING_SECONDS: f64 = 0.5;
const GPS_ALTITUDE_SMOOTHING_SECONDS: f64 = 1.0;
const GPS_HEADING_SMOOTHING_SECONDS: f64 = 0.5;
const G_FORCE_SMOOTHING_SECONDS: f64 = 1.0;

fn moving_average(data: &[Option<f64>], window: usize) -> Vec<Option<f64>> {
    // Standard centered moving average, null-aware
}

fn zero_phase_smooth(data: &[Option<f64>], window: usize) -> Vec<Option<f64>> {
    let forward = moving_average(data, window);
    let reversed: Vec<_> = forward.into_iter().rev().collect();
    let backward = moving_average(&reversed, window);
    backward.into_iter().rev().collect()
}
```

### Window Sizes

Declare these as top-of-file constants in `media/mp4_telemetry.rs`, not inline
literals inside `smooth_series()`.

| Metric       | Window                    | Rationale                             |
| ------------ | ------------------------- | ------------------------------------- |
| GPS speed    | ~0.5s worth of samples    | Already firmware-smoothed; light pass |
| GPS altitude | ~1.0s worth of samples    | GPS altitude is noisier than speed    |
| GPS heading  | ~0.5s worth of samples    | Preserve sharp turns                  |
| gForce       | ~1.0s worth of samples    | Accelerometer is high-frequency noise |

### Upgrade Path to Butterworth

If FBMA is insufficient (noise leaks through due to side lobes), upgrade to
Butterworth order-2 filtfilt with explicit cutoff frequency. Same function
signature, swap internals only. Documented but not implemented initially.

---

## Phase A: Add Dependency

### Step A1: Edit `src-tauri/ovrley_core/Cargo.toml`

Add after the existing `skia-safe` dependency:

```toml
telemetry-parser = { git = "https://github.com/AdrianEddy/telemetry-parser.git", rev = "fd9a73e" }
```

### Step A2: Verify it compiles

```bash
cd src-tauri/ovrley_core && cargo check
```

The crate uses `edition = "2024"` which requires Rust 1.85+. Your workspace
already specifies `rust-version = "1.85"` — verify the installed toolchain
matches. If not, `rustup update stable`.

### Step A3: Resolve dependency conflicts

`telemetry-parser` pulls in `mp4parse` (custom fork), `byteorder`, `chrono`,
`serde`, `serde_json`, `log`, `memchr`, `prost`, `csv`, `half`. Most are already
in the dependency tree. Run `cargo tree -d` to check for version conflicts and
resolve with `[patch]` if needed.

---

## Phase B: Metadata Probe (Telemetry-Parser First, FFprobe Salvage Fallback)

### Step B1: Create `src-tauri/ovrley_core/src/media/`

Create a new source-media module:

- `src-tauri/ovrley_core/src/media/mod.rs`
- `src-tauri/ovrley_core/src/media/source_video_metadata.rs`
- `src-tauri/ovrley_core/src/media/video_probe.rs` (move existing
  `encode/video_probe.rs` here)
- `src-tauri/ovrley_core/src/media/mp4_telemetry.rs`

Initial `media/mod.rs`:

```rust
//! Source media probing and embedded telemetry extraction.
//!
//! This module reads imported source media. It does not own rendering,
//! output encoding, or FFmpeg encode pipelines.

/// MP4/MOV metadata extraction via ffprobe.
pub mod video_probe;
/// Shared source video metadata contract.
pub mod source_video_metadata;
/// MP4 embedded telemetry extraction via telemetry-parser.
pub mod mp4_telemetry;

pub use source_video_metadata::{Resolution, SourceVideoMetadata};
```

Register the module in `src-tauri/ovrley_core/src/lib.rs`:

```rust
/// Source media probing and embedded telemetry extraction.
pub mod media;
```

After moving `video_probe.rs`, update references from
`crate::encode::video_probe` to `crate::media::video_probe`.

The moved `video_probe.rs` may keep using
`crate::encode::ffmpeg::{configure_ffmpeg_command, resolve_ffmpeg_binary}` for
now. A later cleanup can move FFmpeg binary discovery out of `encode/` if both
probing and encoding continue to share it.

### Step B2: Define shared source video metadata

Create `src-tauri/ovrley_core/src/media/source_video_metadata.rs`.
Both ffprobe and telemetry-parser populate this type directly; do not keep
probe-specific metadata structs plus converters.

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceVideoMetadata {
    pub path: String,
    pub duration: Option<f64>,
    pub fps: Option<f64>,
    pub fps_num: Option<u32>,
    pub fps_den: Option<u32>,
    pub resolution: Option<Resolution>,
    pub creation_time: Option<String>, // legacy frontend compatibility
    pub sync_time: Option<String>,     // canonical sync hint
    pub codec_name: Option<String>,
    pub codec_long_name: Option<String>,
    pub codec_profile: Option<String>,
    pub pix_fmt: Option<String>,
    pub bits_per_raw_sample: Option<u32>,
    pub has_audio: bool,
    pub container_format: Option<String>,
    pub rotation_degrees: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u64,
    pub height: u64,
}
```

Update `media/mod.rs`:

```rust
/// MP4 embedded telemetry and telemetry-first metadata extraction.
pub mod mp4_telemetry;
/// Shared source video metadata contract.
pub mod source_video_metadata;
/// MP4/MOV metadata extraction via ffprobe.
pub mod video_probe;

pub use source_video_metadata::{Resolution, SourceVideoMetadata};
```

### Step B3: Move `video_probe.rs` to return `SourceVideoMetadata`

Move `src-tauri/ovrley_core/src/encode/video_probe.rs` to
`src-tauri/ovrley_core/src/media/video_probe.rs` and change
`probe_video()` to return `CoreResult<SourceVideoMetadata>` directly.

The ffprobe path should set:

- `creation_time` from the existing ffprobe priority chain.
- `sync_time` to the same value as a salvage sync fallback.

Keep `creation_time` only for current frontend compatibility. The canonical
field for sync is `sync_time`.

### Step B4: Create `src-tauri/ovrley_core/src/media/mp4_telemetry.rs`

Module doc comment:

```rust
//! Source video metadata and telemetry extraction via telemetry-parser.
//!
//! Owns: `probe_video_metadata()` for telemetry-parser-first source-video
//! metadata reads. Later phases add dense GPS/camera/IMU extraction from the
//! same MP4 telemetry source.
//! Does not own: ffprobe binary discovery (see [`crate::encode::ffmpeg`]),
//! activity parsing/finalization, rendering, or output encoding.
//!
//! Allowed dependencies: `crate::error`, `crate::encode::fps`, `crate::media`,
//! `telemetry_parser`, and `std`.
//! Forbidden dependencies: `crate::commands`, `crate::render`.
```

Implement `probe_video_metadata()` so it returns `CoreResult<SourceVideoMetadata>`.

```rust
use crate::encode::fps::Fps;
use crate::error::{CoreError, CoreResult};
use crate::media::{Resolution, SourceVideoMetadata};
use std::fs::File;
use std::path::Path;

pub fn probe_video_metadata(file_path: &str) -> CoreResult<SourceVideoMetadata> {
    let path = Path::new(file_path);
    let file_size = std::fs::metadata(path)
        .map_err(|source| CoreError::Io { path: path.to_path_buf(), source })?
        .len() as usize;

    let mut stream = File::open(path)
        .map_err(|source| CoreError::Io { path: path.to_path_buf(), source })?;

    let vm = telemetry_parser::util::get_video_metadata(&mut stream, file_size)
        .map_err(|error| CoreError::Encode(format!("telemetry-parser metadata error: {error}")))?;

    let (fps_num, fps_den) = rational_fps_parts(vm.fps);

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
        has_audio: false,
        container_format: None,
        rotation_degrees: Some(vm.rotation),
    })
}
```

### Step B5: Reuse existing FPS rationalization

Do not add a separate `compute_rational_fps()` helper. Use the shared
`crate::encode::fps::Fps::from_f64_fallback()` table and expose only a thin
local adapter if needed:

```rust
pub fn rational_fps_parts(fps: f64) -> (Option<u32>, Option<u32>) {
    match Fps::from_f64_fallback(fps) {
        Ok(fps) => (Some(fps.num), Some(fps.den)),
        Err(_) => (None, None),
    }
}
```

Extend `Fps::from_f64_fallback()` to cover the Phase B metadata-probe rates:
23.976, 24, 25, 29.97, 30, 48, 50, 59.94, 60, and 120, with an integer
rounding fallback for other positive finite values.

### Step B6: Update `backend_probe_video()` in `src-tauri/ovrley_core/src/commands/mod.rs`

Replace the current implementation (lines 497-501):

```rust
pub fn backend_probe_video(paths: &AppPaths, file_path: &str) -> CoreResult<Value> {
    match crate::media::mp4_telemetry::probe_video_metadata(file_path) {
        Ok(metadata) => {
            let metadata = if needs_ffprobe_salvage(&metadata) {
                match crate::media::video_probe::probe_video(&paths.repo_root, file_path) {
                    Ok(ffprobe_metadata) => merge_ffprobe_metadata(metadata, ffprobe_metadata),
                    Err(e) => {
                        log::warn!("ffprobe fallback failed: {e}");
                        metadata
                    }
                }
            } else { metadata };
            serde_json::to_value(&metadata).map_err(CoreError::Serialization)
        }
        Err(e) => {
            log::warn!("telemetry-parser probe failed for {file_path}: {e}, falling back to ffprobe");
            let metadata = crate::media::video_probe::probe_video(&paths.repo_root, file_path)?;
            serde_json::to_value(&metadata).map_err(CoreError::Serialization)
        }
    }
}
```

`needs_ffprobe_salvage()` and `merge_ffprobe_metadata()` should both operate on
`SourceVideoMetadata`. Do not introduce converter functions between ffprobe and
telemetry metadata shapes.

---

## Phase C: Telemetry Extraction → Normalized MP4 Raw Samples

### Step C1: Define extraction internals

In `media/mp4_telemetry.rs`:

```rust
/// Intermediate sample at native telemetry rate, before final frontend assembly.
struct NativeSample {
    timestamp_ms: f64,
    timestamp: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    altitude: Option<f64>,
    speed: Option<f64>,        // m/s (converted from GPS km/h)
    heading: Option<f64>,      // degrees
    iso: Option<f64>,
    aperture: Option<f64>,
    shutter_speed: Option<f64>,
    focal_length: Option<f64>,
    ev: Option<f64>,
    color_temperature: Option<f64>,
    g_force: Option<f64>,
}
```

### Step C2: Implement `extract_telemetry()`

Returns `Option<serde_json::Value>` containing normalized MP4 raw samples and
sync metadata. Returns `None` if no telemetry found.

```rust
use chrono::{TimeZone, Utc};

pub fn extract_telemetry(file_path: &str) -> CoreResult<Option<serde_json::Value>> {
    let path = Path::new(file_path);
    let file_size = std::fs::metadata(path)
        .map_err(|e| CoreError::Io { path: path.to_path_buf(), source: e })?
        .len() as usize;

    let mut stream = File::open(path)
        .map_err(|e| CoreError::Io { path: path.to_path_buf(), source: e })?;

    let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let input = Input::from_stream(&mut stream, file_size, path, |_| {}, cancel_flag)
        .map_err(|e| CoreError::Encode(format!("telemetry-parser parse error: {e}")))?;

    let samples = match input.samples {
        Some(s) if !s.is_empty() => s,
        _ => return Ok(None),
    };

    let camera_type = input.camera_type().to_string();
    let camera_model = input.camera_model().cloned();

    // ── 1. Extract native samples ──
    let native = extract_native_samples(&samples);
    if native.is_empty() { return Ok(None); }

    // ── 2. Smooth dense continuous telemetry series in place ──
    let mut normalized = native;
    smooth_series(&mut normalized);

    // ── 3. Infer sync time ──
    let sync_time = extract_sync_time_from_samples(&samples);

    // ── 4. Convert to raw sample payload for frontend finalization ──
    let raw_samples: Vec<_> = normalized.iter().map(|sample| {
        serde_json::json!({
            "timestamp": sample.timestamp,
            "latitude": sample.latitude,
            "longitude": sample.longitude,
            "altitude": sample.altitude,
            "elevation": sample.altitude,
            "speed": sample.speed,
            "heading": sample.heading,
            "iso": sample.iso,
            "aperture": sample.aperture,
            "shutterSpeed": sample.shutter_speed,
            "focalLength": sample.focal_length,
            "ev": sample.ev,
            "colorTemperature": sample.color_temperature,
            "gForce": sample.g_force,
        })
    }).collect();

    let payload = serde_json::json!({
        "fileName": Path::new(file_path).file_name().map(|n| n.to_string_lossy().to_string()),
        "fileFormat": "mp4_telemetry",
        "syncTime": sync_time,
        "metadata": {
            "camera_type": camera_type,
            "camera_model": camera_model,
            "telemetry_sample_count": raw_samples.len(),
        },
        "rawSamples": raw_samples,
    });

    Ok(Some(payload))
}
```

### Step C3: Implement `extract_native_samples()`

```rust
fn extract_native_samples(samples: &[telemetry_parser::SampleInfo]) -> Vec<NativeSample> {
    let mut result = Vec::with_capacity(samples.len());
    for sample in samples {
        let tag_map = match &sample.tag_map {
            Some(m) => m, None => continue,
        };
        let mut ns = NativeSample {
            timestamp_ms: sample.timestamp_ms,
            timestamp: None, latitude: None, longitude: None, altitude: None,
            speed: None, heading: None, iso: None, aperture: None,
            shutter_speed: None, focal_length: None, ev: None,
            color_temperature: None, g_force: None,
        };
        // GPS
        if let Some(gps_map) = tag_map.get(&GroupId::GPS) {
            if let Some(tag) = gps_map.get(&TagId::Data) {
                if let TagValue::Vec_GpsData(gps_vec) = &tag.value {
                    if let Some(gps) = gps_vec.get().first() {
                        if gps.is_acquired {
                            ns.latitude = Some(gps.lat);
                            ns.longitude = Some(gps.lon);
                            ns.altitude = Some(gps.altitude);
                            ns.speed = Some(gps.speed / 3.6);
                            ns.heading = Some(gps.track);
                            ns.timestamp = Some(unix_to_rfc3339(gps.unix_timestamp));
                        }
                    }
                }
            }
        }
        // Exposure
        if let Some(exp_map) = tag_map.get(&GroupId::Exposure) {
            ns.iso = extract_tag_f64(exp_map, &TagId::ISOValue);
            ns.aperture = extract_tag_f64(exp_map, &TagId::IrisFStop);
            ns.shutter_speed = extract_tag_f64(exp_map, &TagId::ShutterSpeed)
                .or_else(|| extract_tag_f64(exp_map, &TagId::ExposureTime));
        }
        // Lens
        if let Some(lens_map) = tag_map.get(&GroupId::Lens) {
            ns.focal_length = extract_tag_f64(lens_map, &TagId::FocalLength);
        }
        // Accelerometer → gForce
        if let Some(accl_map) = tag_map.get(&GroupId::Accelerometer) {
            if let Some(tag) = accl_map.get(&TagId::Data) {
                match &tag.value {
                    TagValue::Vec_Vector3_i16(arr) => {
                        if let Some(last) = arr.get().last() {
                            ns.g_force = Some(compute_g_force_i16(last, accl_map));
                        }
                    }
                    TagValue::Vec_TimeVector3_f64(arr) => {
                        if let Some(last) = arr.get().last() {
                            ns.g_force = Some(compute_g_force_f64(last));
                        }
                    }
                    _ => {}
                }
            }
        }
        result.push(ns);
    }
    result
}
```

### Step C4: Implement helper functions

```rust
fn extract_tag_f64(map: &TagMap, id: &TagId) -> Option<f64> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::u8(v)  => Some(*v.get() as f64),
        TagValue::u16(v) => Some(*v.get() as f64),
        TagValue::u32(v) => Some(*v.get() as f64),
        TagValue::i16(v) => Some(*v.get() as f64),
        TagValue::i32(v) => Some(*v.get() as f64),
        TagValue::f32(v) => Some(*v.get() as f64),
        TagValue::f64(v) => Some(*v.get()),
        _ => None,
    }
}

fn compute_g_force_i16(v: &Vector3<i16>, map: &TagMap) -> f64 {
    let scale = extract_tag_f64(map, &TagId::Scale).unwrap_or(1.0);
    let unit_factor = match map.get_t::<String>(TagId::Unit).map(|s| s.as_str()) {
        Some("g") => 1.0,
        Some("m/s²") => 1.0 / 9.80665,
        _ => 1.0,
    };
    let x = v.x as f64 / scale * unit_factor;
    let y = v.y as f64 / scale * unit_factor;
    let z = v.z as f64 / scale * unit_factor;
    ((x * x + y * y + z * z).sqrt() - 1.0).abs()
}

fn compute_g_force_f64(v: &TimeVector3<f64>) -> f64 {
    ((v.x * v.x + v.y * v.y + v.z * v.z).sqrt() - 1.0).abs()
}

fn unix_to_rfc3339(unix_ts: f64) -> String {
    let secs = unix_ts as i64;
    let nanos = ((unix_ts - secs as f64) * 1_000_000_000.0) as u32;
    Utc.timestamp_opt(secs, nanos).single().map(|dt| dt.to_rfc3339()).unwrap_or_default()
}

fn extract_sync_time_from_samples(samples: &[telemetry_parser::SampleInfo]) -> Option<String> {
    for sample in samples {
        if let Some(ref tag_map) = sample.tag_map {
            if let Some(gps_map) = tag_map.get(&GroupId::GPS) {
                if let Some(tag) = gps_map.get(&TagId::Data) {
                    if let TagValue::Vec_GpsData(gps_vec) = &tag.value {
                        if let Some(gps) = gps_vec.get().first() {
                            if gps.is_acquired {
                                return Some(unix_to_rfc3339(gps.unix_timestamp - sample.timestamp_ms / 1000.0));
                            }
                        }
                    }
                }
            }
        }
    }
    None
}
```

---

## Phase D: Interpolation, Smoothing, Derived Metrics

### Step D1: Keep smoothing only; do not pre-densify to video FPS

The earlier version proposed resampling MP4 telemetry to video frame rate
inside the extractor. That is intentionally removed.

- Extraction keeps native telemetry cadence.
- The existing frontend finalizer computes canonical distance/progress/derived
  metrics.
- The existing Rust render-prep path remains the only frame-densification seam.

### Step D2: Implement FBMA smoothing

```rust
const GPS_SPEED_SMOOTHING_SECONDS: f64 = 0.5;
const GPS_ALTITUDE_SMOOTHING_SECONDS: f64 = 1.0;
const GPS_HEADING_SMOOTHING_SECONDS: f64 = 0.5;
const G_FORCE_SMOOTHING_SECONDS: f64 = 1.0;

fn moving_average(data: &[Option<f64>], window: usize) -> Vec<Option<f64>> {
    if window == 0 || data.is_empty() { return data.to_vec(); }
    let half = window / 2;
    let mut result = Vec::with_capacity(data.len());
    for i in 0..data.len() {
        let start = i.saturating_sub(half);
        let end = (i + half + 1).min(data.len());
        let mut sum = 0.0;
        let mut count = 0;
        for j in start..end {
            if let Some(v) = data[j] { sum += v; count += 1; }
        }
        result.push(if count > 0 { Some(sum / count as f64) } else { None });
    }
    result
}

fn zero_phase_smooth(data: &[Option<f64>], window: usize) -> Vec<Option<f64>> {
    let forward = moving_average(data, window);
    let reversed: Vec<_> = forward.into_iter().rev().collect();
    let backward = moving_average(&reversed, window);
    backward.into_iter().rev().collect()
}

fn smoothing_window_for_seconds(sample_timestamps_ms: &[f64], seconds: f64) -> usize {
    // Estimate a representative samples-per-second rate from timestamps and
    // convert the desired smoothing horizon into a window length.
}

fn smooth_series(samples: &mut [NativeSample]) {
    let timestamps: Vec<_> = samples.iter().map(|s| s.timestamp_ms).collect();
    let altitude: Vec<_> = samples.iter().map(|s| s.altitude).collect();
    let speed: Vec<_> = samples.iter().map(|s| s.speed).collect();
    let heading: Vec<_> = samples.iter().map(|s| s.heading).collect();
    let g_force: Vec<_> = samples.iter().map(|s| s.g_force).collect();

    let altitude_window = smoothing_window_for_seconds(&timestamps, GPS_ALTITUDE_SMOOTHING_SECONDS);
    let speed_window = smoothing_window_for_seconds(&timestamps, GPS_SPEED_SMOOTHING_SECONDS);
    let heading_window = smoothing_window_for_seconds(&timestamps, GPS_HEADING_SMOOTHING_SECONDS);
    let g_force_window = smoothing_window_for_seconds(&timestamps, G_FORCE_SMOOTHING_SECONDS);

    let sa = zero_phase_smooth(&altitude, altitude_window.max(1));
    let ss = zero_phase_smooth(&speed, speed_window.max(1));
    let sh = zero_phase_smooth(&heading, heading_window.max(1));
    let sg = zero_phase_smooth(&g_force, g_force_window.max(1));

    for i in 0..samples.len() {
        samples[i].altitude = sa[i];
        samples[i].speed = ss[i];
        samples[i].heading = sh[i];
        samples[i].g_force = sg[i];
    }
}
```

### Step D3: Reuse existing frontend derivations

Do not add Rust-only distance/gradient/pace derivation helpers for MP4 in this
phase. Those remain in the canonical frontend finalization seam so all source
formats share the same rules.

---

## Phase E: Tauri Command Wiring

### Step E1: Add `backend_extract_video_telemetry` to `commands/mod.rs`

```rust
pub fn backend_extract_video_telemetry(file_path: &str) -> CoreResult<Value> {
    match crate::media::mp4_telemetry::extract_telemetry(file_path)? {
        Some(activity_json) => Ok(activity_json),
        None => Ok(serde_json::Value::Null),
    }
}
```

### Step E2: Add Tauri command in `src-tauri/src/tauri_commands.rs`

```rust
#[tauri::command]
pub(crate) async fn backend_extract_video_telemetry(
    file_path: String,
) -> Result<String, String> {
    let result = commands::backend_extract_video_telemetry(&file_path)
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&result).map_err(|e| e.to_string())
}
```

### Step E3: Register in `src-tauri/src/lib.rs`

Add to the `invoke_handler` list after `backend_probe_video`:

```rust
tauri_commands::backend_extract_video_telemetry,
```

---

## Phase F: Frontend Integration

### Step F1: Add IPC wrapper in `app/src/api/backend.js`

```javascript
/**
 * Extracts embedded telemetry from a video file as normalized MP4 raw samples.
 *
 * @param {string} path - Absolute path to the source video file.
 * @returns {Promise<object|null>} Promise resolving to normalized payload or null.
 */
export async function extractVideoTelemetry(path) {
  const result = await invokeCommand("backend_extract_video_telemetry", { filePath: path });
  return typeof result === "string" ? JSON.parse(result) : result;
}
```

### Step F2: Add state to `createVideoImportSlice.js`

Add new fields for the raw MP4 telemetry payload and the finalized telemetry
activity derived from it.

Also rename the existing video timestamp field from `importedVideoCreationTime`
to `importedVideoSyncTime` and update `computeVideoSync()` plus any callers to
read `metadata.syncTime` instead of `metadata.creationTime`:

```javascript
videoTelemetryRaw: null,             // normalized Rust payload: { fileName, fileFormat, syncTime, metadata, rawSamples }
videoTelemetryParsedActivity: null,  // ParsedActivity produced via finalizeParsedActivity()
videoTelemetrySyncTime: null,        // ISO-8601 sync-time inferred from GPS or salvaged metadata
```

Add setter:

```javascript
setVideoTelemetry: ({ raw, parsedActivity }) => set({
  videoTelemetryRaw: raw,
  videoTelemetryParsedActivity: parsedActivity,
  videoTelemetrySyncTime: raw?.syncTime ?? null,
}),
```

Update `clearImportedVideo` to also clear:

```javascript
videoTelemetryRaw: null,
videoTelemetryParsedActivity: null,
videoTelemetrySyncTime: null,
```

### Step F3: Update `useVideoImport.js` to call extraction

After `importPreviewVideo()` succeeds, kick off telemetry extraction guarded by
the current import path or import ID to avoid stale async writes:

```javascript
import { extractVideoTelemetry, clearPreviewVideo, importPreviewVideo } from "@/api/backend";
import { finalizeParsedActivity } from "@/lib/activity/parser";

// Inside handleImportVideo, after setImportedVideo(metadata):
const currentImportPath = selected;
extractVideoTelemetry(selected)
  .then((result) => {
    if (!result) return;
    if (useStore.getState().importedVideoPath !== currentImportPath) return;

    const { parsedActivity } = finalizeParsedActivity({
      fileName: result.fileName,
      fileFormat: result.fileFormat,
      metadata: result.metadata,
      rawSamples: result.rawSamples,
      options: {
        skipIdleGapFill: true,
        useWindowedRate: true,
        rateWindowSeconds: 1,
      },
    });

    setVideoTelemetry({ raw: result, parsedActivity });

    // Sync-time is a sync hint, not provenance metadata.
    if (result.syncTime && !metadata.syncTime) {
      const updatedMetadata = { ...metadata, syncTime: result.syncTime };
      setImportedVideo(updatedMetadata);
    }
  })
  .catch((err) => console.warn("Telemetry extraction failed:", err));
```

### Step F4: Merge strategy (when FIT/GPX also imported)

When the user imports FIT/GPX after video telemetry, merge from known source
objects into a disposable canonical `ParsedActivity`. Do not treat the merged
object itself as provenance-aware. Once metrics have been copied into one
`ParsedActivity`, the app cannot reliably tell whether a value originally came
from FIT/GPX or MP4 telemetry unless the source objects are still kept
separately.

Recommended flow:

- Keep source-level activities separate in state:
  - `importedActivityParsedActivity` (or the existing non-video parsed activity
    store field) owns FIT/GPX/SRT user activity imports.
  - `videoTelemetryParsedActivity` owns MP4 embedded telemetry finalized from
    `videoTelemetryRaw`.
  - `parsedActivity` / `activitySummary` is derived output only.
- Rebuild the merged `parsedActivity` whenever either source changes. Do not
  mutate an existing merged object in place, because that loses source
  provenance and can leave stale MP4-filled values after a later activity
  import.
- If the user imports a new FIT/GPX/SRT activity, replace the non-video source
  activity and recompute the merged activity against the current video
  telemetry source, if any.
- If the user clears or replaces the preview video, clear
  `videoTelemetryRaw` / `videoTelemetryParsedActivity` and recompute from the
  non-video source alone.
- If only MP4 telemetry exists, `parsedActivity` may be derived directly from
  `videoTelemetryParsedActivity`. If both exist, FIT/GPX/SRT remains the primary
  activity source.
- For each metric series during recompute: if FIT/GPX/SRT has a non-null value,
  keep it; otherwise fill from MP4 telemetry. Camera-only metrics such as ISO,
  aperture, shutter, focal length, EV, color temperature, and g-force naturally
  come from MP4 when absent from the activity source.
- Keep one derived `activitySummary` / `parsedActivity` pair so the rest of the
  app continues to operate on a single canonical activity object, but never use
  that pair as the source of truth for future merges.

This should reuse the existing `combineSeries()` pattern in
`metric-series.js`, but the merge happens before render, not only inside the
render payload builder.

### Step F5: Expand video metadata display

Update `app/src/features/scene-settings/components/VideoSyncSection.jsx` so the
video section shows richer source metadata when available. This is display-only
metadata; it must not become sync/provenance logic.

Recommended display fields:

- Existing summary row: duration, fps, resolution.
- Source timestamp row: use `importedVideoSyncTime` / `metadata.syncTime` once
  F2 renames the old creation-time field; label it as sync/source time rather
  than authoritative file creation time.
- Camera row, if available:
  - manufacturer/source family from MP4 telemetry `metadata.camera_type` or
    source probe metadata if a manufacturer field is added later.
  - model from MP4 telemetry `metadata.camera_model` and any later camera
    model field.
- Codec row, if available:
  - `codecName` only from `SourceVideoMetadata`.
  - keep the display compact and do not surface the longer codec detail bundle
    in this phase.
- Bitrate row, if available:
  - show video/container bitrate only when the backend exposes it.
  - Do not infer bitrate from file size and duration in the UI unless that is
    explicitly added as an approximate field; exact stream bitrate should come
    from ffprobe/source metadata.

Data-shape note:

- `SourceVideoMetadata` already carries codec/profile/pixel-format/bit-depth
  fields, but F5 should only render `codecName` from that set. The UI must
  render manufacturer, camera model, and bitrate too when the backend exposes
  them, but those values come from the source metadata contract rather than the
  codec detail bundle.
- `extract_telemetry()` currently exposes MP4 telemetry metadata as
  `camera_type` and `camera_model`.
- If Phase F requires bitrate/manufacturer/model in the UI, add those fields to
  the backend metadata contract first instead of deriving them ad hoc inside
  `VideoSyncSection.jsx`.
- `VideoSyncSection.jsx` should accept one metadata object or explicit optional
  props for these display values; avoid reaching directly into unrelated store
  state from the presentational component.

---

## Phase G: Testing

### Step G1: Unit tests in `media/mp4_telemetry.rs`

`#[cfg(test)] mod tests` block:

- `test_rational_fps_parts()` / shared `Fps::from_f64_fallback()` coverage —
  23.976, 24, 25, 29.97, 30, 48, 50, 59.94, 60, 120, and integer fallback
- `test_moving_average()` — null handling, window sizes
- `test_zero_phase_smooth()` — zero delay, smoothing behavior
- `test_smoothing_window_for_seconds()` — cadence-aware window sizing
- `test_extract_sync_time_from_samples()` — GPS timestamp inference
- `test_unix_to_rfc3339()` — timestamp conversion
- `test_extract_native_samples()` — expected field mapping from telemetry-parser tags

### Step G2: Integration tests

`src-tauri/ovrley_core/tests/telemetry_tests.rs`:

- `test_probe_video_metadata_with_gopro()` — if fixture available
- `test_extract_telemetry_with_gopro()` — verify normalized raw payload shape
- `test_extract_telemetry_no_telemetry()` — verify returns None
- frontend test: finalize MP4 telemetry payload into canonical `ParsedActivity`
- frontend test: merge FIT/GPX over telemetry with expected precedence

### Step G3: Manual testing

Test with sample videos:

- GoPro HERO 5+ (GPS + camera settings)
- Sony a7 series (GPS + camera settings)
- DJI drone (GPS + camera settings)
- Insta360 (GPS + camera settings)
- Screen recording (no telemetry — verify fallback)

---

## Unanswered Questions

None.

### Resolved Questions

- **Canonical finalization boundary**: MP4 telemetry joins the existing
  finalization seam. Rust does extraction + smoothing; JavaScript still builds
  canonical `ParsedActivity` for all source formats in this phase.

- **Merge strategy**: Merge at the store/canonical-activity layer, not only at
  render time. The rest of the app continues to consume one `parsedActivity`.

- **Partial telemetry**: Rust returns sparse normalized raw samples with nulls
  for missing values. Frontend finalization and merge logic handle gaps
  gracefully.

- **gForce baseline subtraction**: Subtract 1g from accelerometer magnitude.
  The exported `gForce` value should represent net force above/below gravity,
  matching the existing `ParsedActivity.g_force` semantics.

- **No embedded telemetry**: `extract_telemetry()` returns `None`. The frontend
  handles this gracefully: no activity is imported from the video, and the user
  can still import FIT/GPX separately.

- **FBMA window declarations**: Keep the initial smoothing horizons hard-coded
  as named top-of-file constants in `media/mp4_telemetry.rs`:
  `GPS_SPEED_SMOOTHING_SECONDS`, `GPS_ALTITUDE_SMOOTHING_SECONDS`,
  `GPS_HEADING_SMOOTHING_SECONDS`, and `G_FORCE_SMOOTHING_SECONDS`.
