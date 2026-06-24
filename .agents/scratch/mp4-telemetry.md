# MP4 Telemetry Integration Plan v2

## Goal

Supersede the current ffprobe-based video metadata extraction with
[telemetry-parser](https://github.com/AdrianEddy/telemetry-parser) (Rust crate).
Two purposes:

1. Primary metadata source for video import (resolution, fps, creation time,
   codec, rotation) — fall back to ffprobe only when telemetry-parser fails.
2. Extract embedded telemetry (GPS, camera settings, IMU) from video files and
   remap to the internal `ParsedActivity` schema.

## Decisions

| #   | Decision                                                                                       | Rationale                                                                                                                                                                                                                                                |
| --- | ---------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D1  | Pin dependency to commit `fd9a73e` (Jun 11 2026, latest)                                       | Reproducible builds; avoids silent breakage from upstream HEAD                                                                                                                                                                                           |
| D2  | Video framerate as target sample rate                                                          | Overlay renders at video FPS; one telemetry value per frame simplifies densification                                                                                                                                                                     |
| D3  | Forward-backward moving average (FBMA) for smoothing                                           | Simple (~20 lines), zero-phase, no coefficient computation; upgrade path to Butterworth if insufficient                                                                                                                                                  |
| D4  | Camera settings (ISO, aperture, shutter, focal length, EV, color temp) excluded from smoothing | These are discrete values; smoothing would blur intentional jumps                                                                                                                                                                                        |
| D5  | GPS `unix_timestamp` inference as primary creation-time source                                 | Survives simple cuts (metadata track preserved); most accurate for GPS cameras                                                                                                                                                                           |
| D6  | ffprobe fallback for creation time                                                             | Handles non-GPS cameras and re-encoded videos where metadata track is stripped                                                                                                                                                                           |
| D7  | Rust outputs complete `ParsedActivity` JSON directly (flat arrays), not raw samples            | Flat arrays are ~2.5x more compact than raw sample objects; avoids redundant JS re-processing; GPS speed/heading already available — no need to re-derive; frontend does NOT call `finalizeParsedActivity()` — Rust already computed all derived metrics |
| D8  | FIT/GPX always supersedes extracted telemetry                                                  | FIT/GPX is user-intent data; telemetry supplements only missing fields                                                                                                                                                                                   |
| D9  | Telemetry extraction runs automatically on video import                                        | User confirmed; no separate "extract" button needed                                                                                                                                                                                                      |
| D10 | GPS speed/track/altitude used as recorded (not derived from coordinates)                       | These are GPS chip Doppler/heading/altimeter readings, already firmware-smoothed                                                                                                                                                                         |
| D11 | Check `GpsData.is_acquired` before using GPS values                                            | Skip samples where GPS signal is lost; emit `null` in those positions                                                                                                                                                                                    |

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
    │ 1. telemetry-parser → VideoMetadata (fps, resolution, etc.)
    │ 2. Register in HTTP video server
    │ 3. Return metadata + preview_url
    │
    ▼
backend_extract_video_telemetry()  ← NEW command, called automatically
    │ 1. Parse file with telemetry-parser
    │ 2. Get video FPS
    │ 3. Extract GPS/camera/IMU at native rate
    │ 4. Interpolate to video frame rate
    │ 5. Apply FBMA smoothing (GPS only, not camera settings)
    │ 6. Compute derived metrics (distance, gradient, pace)
    │ 7. Infer creation_time from GPS
    │ 8. Return ParsedActivity-compatible JSON (flat arrays)
    │
    ▼
Frontend: store as videoTelemetry (same ParsedActivity shape as FIT/GPX)
    │ NO finalizeParsedActivity() call — Rust output is already complete
    │ NO useWindowedRate — smoothing already done in Rust
    │ Only add: metricUnits, coverage, validAttributes (cheap O(n) passes)
    │
    ▼
[User imports FIT/GPX] (optional, takes precedence)
    │ → Merge: FIT/GPX values override telemetry values
    │
    ▼
[Rust: trim + densify → DenseActivityReport]
    │ Densification is near-pass-through (already at frame rate)
    │
    ▼
[Render: per-frame overlay]
```

## Telemetry → ParsedActivity Mapping

| telemetry-parser source         | ParsedActivity field        | Unit conversion                | Smoothing |
| ------------------------------- | --------------------------- | ------------------------------ | --------- |
| `GPS.lat/lon`                   | `course: [(lat, lon), ...]` | direct                         | no        |
| `GPS.altitude`                  | `altitude`, `elevation`     | meters                         | FBMA      |
| `GPS.speed`                     | `speed`                     | km/h → m/s (÷3.6)              | FBMA      |
| `GPS.track`                     | `heading`                   | degrees                        | FBMA      |
| `GPS.unix_timestamp`            | `time`, `source_start_time` | unix → ISO 8601                | no        |
| Derived from course points      | `sample_distance_progress`  | haversine → normalized 0..1    | no        |
| Derived from elevation/distance | `gradient`                  | (Δelevation / Δdistance) × 100 | FBMA      |
| Derived from speed              | `pace`                      | 1000 / speed (s/km)            | no        |
| `Exposure.ISOValue`             | `iso`                       | direct                         | **no**    |
| `Exposure.IrisFStop`            | `aperture`                  | direct                         | **no**    |
| `Exposure.ShutterSpeed`         | `shutter_speed`             | direct                         | **no**    |
| `Lens.FocalLength`              | `focal_length`              | mm                             | **no**    |
| Accelerometer magnitude         | `g_force`                   | √(x²+y²+z²), subtract 1g       | FBMA      |

### ParsedActivity JSON Shape (Rust Output)

Flat arrays — compact, fast to parse, matches existing schema exactly:

```json
{
  "file_name": "GOPR0123.MP4",
  "file_format": "mp4_telemetry",
  "metadata": {
    "duration_seconds": 600.0,
    "start_time": "2025-07-23T10:21:41Z",
    "end_time": "2025-07-23T10:31:41Z",
    "total_distance_m": 1500.0,
    "sample_count": 18000,
    "camera_type": "GoPro",
    "camera_model": "HERO 12"
  },
  "source_start_time": "2025-07-23T10:21:41Z",
  "sample_elapsed_seconds": [0.0, 0.033, 0.067, ...],
  "sample_distance_progress": [0.0, 0.0001, 0.0003, ...],
  "sample_course_points": [[48.123, 11.456], [48.124, 11.457], ...],
  "sample_elevations": [520.0, 520.1, 520.2, ...],
  "trim_start_seconds": 0,
  "trim_end_seconds": 600.0,
  "course": [[48.123, 11.456], [48.124, 11.457], ...],
  "elevation": [520.0, 520.1, 520.2, ...],
  "speed": [5.2, 5.3, 5.1, ...],
  "heading": [180.0, 180.1, 180.2, ...],
  "gradient": [0.5, 0.3, -0.2, ...],
  "pace": [192.3, 188.7, 196.1, ...],
  "time": ["2025-07-23T10:21:41Z", "2025-07-23T10:21:41.033Z", ...],
  "iso": [100, 100, 200, ...],
  "aperture": [2.8, 2.8, 2.8, ...],
  "shutter_speed": [0.0025, 0.0025, 0.005, ...],
  "focal_length": [24.0, 24.0, 24.0, ...],
  "altitude": [520.0, 520.1, 520.2, ...],
  "g_force": [0.1, 0.2, 0.05, ...],
  "heartrate": [],
  "cadence": [],
  "power": [],
  "temperature": [],
  "ev": [],
  "color_temperature": [],
  "air_pressure": [],
  "ground_contact_time": [],
  "left_right_balance": [],
  "stride_length": [],
  "stroke_rate": [],
  "torque": [],
  "vertical_speed": [],
  "gear_position": [],
  "vertical_ratio": [],
  "vertical_oscillation": [],
  "core_temperature": []
}
```

Note: Empty arrays `[]` for metrics not available from the camera (heartrate,
cadence, power, etc.) — the frontend `combineSeries()` pattern fills these from
FIT/GPX if available.

## Creation Time Extraction

```rust
fn extract_creation_time(samples: &[SampleInfo]) -> Option<String> {
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
    None  // Caller falls back to ffprobe
}
```

### Survivability

| Scenario                      | GPS method                                        | ffprobe method                |
| ----------------------------- | ------------------------------------------------- | ----------------------------- |
| Original camera file          | Accurate                                          | Accurate                      |
| Simple cut (`ffmpeg -c copy`) | Accurate (timestamps preserved in metadata track) | Usually preserved             |
| Re-encode (Premiere, DaVinci) | **Lost** (metadata track stripped)                | Overwritten to re-export time |
| Trim start removed            | Reflects cut point (correct for remaining video)  | May reflect cut or original   |

## Smoothing: Forward-Backward Moving Average

```rust
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

| Metric       | Window       | Rationale                             |
| ------------ | ------------ | ------------------------------------- |
| GPS speed    | fps/2 (0.5s) | Already firmware-smoothed; light pass |
| GPS altitude | fps (1s)     | GPS altitude is noisier than speed    |
| GPS heading  | fps/2 (0.5s) | Preserve sharp turns                  |
| gForce       | fps (1s)     | Accelerometer is high-frequency noise |

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

## Phase B: Metadata Probe (Telemetry-Parser Primary, FFprobe Fallback)

### Step B1: Create `src-tauri/ovrley_core/src/encode/telemetry.rs`

Module doc comment:

```rust
//! Video metadata and telemetry extraction via telemetry-parser.
//!
//! Owns: `probe_video_metadata()` (supersedes ffprobe for basic metadata),
//!       `extract_telemetry()` (extracts GPS/camera/IMU as ParsedActivity),
//!       and zero-phase smoothing helpers.
//! Does not own: ffprobe binary discovery (see [`crate::encode::ffmpeg`]),
//!       activity parsing pipeline (frontend `finalizeParsedActivity()`).
//!
//! Allowed dependencies: `crate::error`, `crate::activity::schema`, `serde`,
//!       `serde_json`, `chrono`, `telemetry_parser`, `std`.
//! Forbidden dependencies: `crate::commands`, `crate::render`.
//!
//! ## Thread Safety
//! Single-threaded. Reads file synchronously via telemetry-parser. No shared
//! mutable state.
//!
//! ## Performance
//! Called once per imported video. telemetry-parser reads only header/footer
//! bytes (5-500 MB depending on file size), not the full file. Typical wall
//! time < 2 seconds for 1080p files with telemetry.
```

### Step B2: Define `TelemetryVideoMetadata` struct

This struct must serialize to the same JSON shape as the existing
`VideoMetadata` in `video_probe.rs` (camelCase), so the frontend receives
identical fields regardless of which backend produced them.

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryVideoMetadata {
    pub path: String,
    pub duration: Option<f64>,
    pub fps: Option<f64>,
    pub fps_num: Option<u32>,
    pub fps_den: Option<u32>,
    pub resolution: Option<Resolution>,
    pub creation_time: Option<String>,
    pub codec_name: Option<String>,
    pub codec_long_name: Option<String>,
    pub codec_profile: Option<String>,
    pub pix_fmt: Option<String>,
    pub bits_per_raw_sample: Option<u32>,
    pub has_audio: bool,
    pub container_format: Option<String>,
    pub rotation_degrees: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u64,
    pub height: u64,
}
```

### Step B3: Implement `probe_video_metadata()`

```rust
use crate::error::{CoreError, CoreResult};
use std::fs::File;
use std::path::Path;
use telemetry_parser::Input;

pub fn probe_video_metadata(file_path: &str) -> CoreResult<TelemetryVideoMetadata> {
    let path = Path::new(file_path);
    let file_size = std::fs::metadata(path)
        .map_err(|e| CoreError::Io { path: path.to_path_buf(), source: e })?
        .len() as usize;

    let mut stream = File::open(path)
        .map_err(|e| CoreError::Io { path: path.to_path_buf(), source: e })?;

    let vm = telemetry_parser::util::get_video_metadata(&mut stream, file_size)
        .map_err(|e| CoreError::Encode(format!("telemetry-parser metadata error: {e}")))?;

    let (fps_num, fps_den) = compute_rational_fps(vm.fps);

    Ok(TelemetryVideoMetadata {
        path: file_path.to_string(),
        duration: Some(vm.duration_s),
        fps: Some(vm.fps),
        fps_num,
        fps_den,
        resolution: Some(Resolution {
            width: vm.width as u64,
            height: vm.height as u64,
        }),
        creation_time: None, // Filled by caller via ffprobe fallback or GPS inference
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

### Step B4: Implement `compute_rational_fps()`

```rust
fn compute_rational_fps(fps: f64) -> (Option<u32>, Option<u32>) {
    if fps <= 0.0 || !fps.is_finite() {
        return (None, None);
    }
    let candidates = [
        (24000, 1001), (24, 1), (25, 1), (30000, 1001), (30, 1),
        (48, 1), (50, 1), (60000, 1001), (60, 1), (120, 1),
    ];
    for (num, den) in candidates {
        let candidate = num as f64 / den as f64;
        if (fps - candidate).abs() < 0.01 {
            return (Some(num), Some(den));
        }
    }
    let rounded = fps.round() as u32;
    if rounded > 0 { (Some(rounded), Some(1)) } else { (None, None) }
}
```

### Step B5: Register module in `src-tauri/ovrley_core/src/encode/mod.rs`

Add after `pub mod video_probe;`:

```rust
/// Video metadata and telemetry extraction via telemetry-parser.
pub mod telemetry;
```

### Step B6: Update `backend_probe_video()` in `src-tauri/ovrley_core/src/commands/mod.rs`

Replace the current implementation (lines 497-501):

```rust
pub fn backend_probe_video(paths: &AppPaths, file_path: &str) -> CoreResult<Value> {
    match crate::encode::telemetry::probe_video_metadata(file_path) {
        Ok(metadata) => {
            let metadata = if metadata.creation_time.is_none() {
                match crate::encode::video_probe::probe_video(&paths.repo_root, file_path) {
                    Ok(ffprobe_md) => {
                        let mut md = metadata;
                        md.creation_time = ffprobe_md.creation_time;
                        if md.codec_name.is_none() { md.codec_name = ffprobe_md.codec_name; }
                        if md.codec_long_name.is_none() { md.codec_long_name = ffprobe_md.codec_long_name; }
                        if md.codec_profile.is_none() { md.codec_profile = ffprobe_md.codec_profile; }
                        if md.pix_fmt.is_none() { md.pix_fmt = ffprobe_md.pix_fmt; }
                        if md.bits_per_raw_sample.is_none() { md.bits_per_raw_sample = ffprobe_md.bits_per_raw_sample; }
                        md.has_audio = ffprobe_md.has_audio;
                        if md.container_format.is_none() { md.container_format = ffprobe_md.container_format; }
                        md
                    }
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
            let metadata = crate::encode::video_probe::probe_video(&paths.repo_root, file_path)?;
            serde_json::to_value(&metadata).map_err(CoreError::Serialization)
        }
    }
}
```

---

## Phase C: Telemetry Extraction → ParsedActivity

### Step C1: Define extraction internals

In `telemetry.rs`:

```rust
/// Intermediate sample at native telemetry rate, before interpolation.
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

Returns `Option<serde_json::Value>` — the `ParsedActivity`-compatible JSON
ready for the frontend store. Returns `None` if no telemetry found.

```rust
use crate::activity::schema::ParsedActivity;
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

    let mut stream2 = File::open(path)
        .map_err(|e| CoreError::Io { path: path.to_path_buf(), source: e })?;
    let vm = telemetry_parser::util::get_video_metadata(&mut stream2, file_size)
        .map_err(|e| CoreError::Encode(format!("telemetry-parser metadata error: {e}")))?;
    let target_fps = vm.fps;
    if target_fps <= 0.0 || !target_fps.is_finite() { return Ok(None); }

    let camera_type = input.camera_type().to_string();
    let camera_model = input.camera_model().cloned();

    // ── 1. Extract native samples ──
    let native = extract_native_samples(&samples);
    if native.is_empty() { return Ok(None); }

    // ── 2. Infer creation time ──
    let creation_time = extract_creation_time_from_samples(&samples);

    // ── 3. Resample to video frame rate ──
    let target_interval_ms = 1000.0 / target_fps;
    let total_duration_ms = samples.last().map(|s| s.timestamp_ms + s.duration_ms).unwrap_or(0.0);
    let frame_count = (total_duration_ms / target_interval_ms).ceil() as usize;

    let mut resampled = Vec::with_capacity(frame_count);
    for i in 0..frame_count {
        let target_ms = i as f64 * target_interval_ms;
        resampled.push(interpolate_at_timestamp(target_ms, &native));
    }

    // ── 4. Smooth GPS/IMU series ──
    let fps_usize = target_fps.round() as usize;
    smooth_series(&mut resampled, fps_usize);

    // ── 5. Build flat arrays ──
    let elapsed: Vec<f64> = (0..frame_count).map(|i| i as f64 / target_fps).collect();
    let course: Vec<(Option<f64>, Option<f64>)> = resampled.iter().map(|s| (s.latitude, s.longitude)).collect();
    let elevation: Vec<Option<f64>> = resampled.iter().map(|s| s.altitude).collect();
    let speed: Vec<Option<f64>> = resampled.iter().map(|s| s.speed).collect();
    let heading: Vec<Option<f64>> = resampled.iter().map(|s| s.heading).collect();
    let time: Vec<Option<String>> = resampled.iter().map(|s| s.timestamp.clone()).collect();
    let iso: Vec<Option<f64>> = resampled.iter().map(|s| s.iso).collect();
    let aperture: Vec<Option<f64>> = resampled.iter().map(|s| s.aperture).collect();
    let shutter_speed: Vec<Option<f64>> = resampled.iter().map(|s| s.shutter_speed).collect();
    let focal_length: Vec<Option<f64>> = resampled.iter().map(|s| s.focal_length).collect();
    let g_force: Vec<Option<f64>> = resampled.iter().map(|s| s.g_force).collect();

    // ── 6. Compute derived metrics ──
    let distance_series = compute_distance_series(&course);
    let total_distance = distance_series.last().copied().unwrap_or(0.0);
    let distance_progress: Vec<f64> = if total_distance > 0.0 {
        distance_series.iter().map(|d| d / total_distance).collect()
    } else {
        vec![0.0; frame_count]
    };
    let gradient = compute_gradient_series(&elevation, &distance_series);
    let pace: Vec<Option<f64>> = speed.iter().map(|s| {
        s.and_then(|v| if v > 0.0 { Some(1000.0 / v) } else { None })
    }).collect();

    // ── 7. Build ParsedActivity JSON ──
    let start_time = time.iter().find(|t| t.is_some()).and_then(|t| t.clone());
    let end_time = time.iter().rfind(|t| t.is_some()).and_then(|t| t.clone());
    let duration_seconds = elapsed.last().copied().unwrap_or(0.0);

    let mut activity = serde_json::json!({
        "file_name": Path::new(file_path).file_name().map(|n| n.to_string_lossy().to_string()),
        "file_format": "mp4_telemetry",
        "metadata": {
            "duration_seconds": duration_seconds,
            "start_time": start_time,
            "end_time": end_time,
            "total_distance_m": total_distance,
            "sample_count": frame_count,
            "camera_type": camera_type,
            "camera_model": camera_model,
        },
        "source_start_time": start_time,
        "sample_elapsed_seconds": elapsed,
        "sample_distance_progress": distance_progress,
        "sample_course_points": course,
        "sample_elevations": elevation,
        "trim_start_seconds": 0.0,
        "trim_end_seconds": duration_seconds,
        "course": course,
        "elevation": elevation,
        "speed": speed,
        "heading": heading,
        "gradient": gradient,
        "pace": pace,
        "time": time,
        "iso": iso,
        "aperture": aperture,
        "shutter_speed": shutter_speed,
        "focal_length": focal_length,
        "altitude": elevation,
        "g_force": g_force,
        // Empty arrays for metrics not available from video telemetry
        "heartrate": [],
        "cadence": [],
        "power": [],
        "temperature": [],
        "ev": [],
        "color_temperature": [],
        "air_pressure": [],
        "ground_contact_time": [],
        "left_right_balance": [],
        "stride_length": [],
        "stroke_rate": [],
        "torque": [],
        "vertical_speed": [],
        "gear_position": [],
        "vertical_ratio": [],
        "vertical_oscillation": [],
        "core_temperature": [],
    });

    Ok(Some(activity))
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

fn extract_creation_time_from_samples(samples: &[telemetry_parser::SampleInfo]) -> Option<String> {
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

### Step D1: Implement `interpolate_at_timestamp()`

```rust
fn interpolate_at_timestamp(target_ms: f64, samples: &[NativeSample]) -> NativeSample {
    if samples.is_empty() { return NativeSample::default_at(target_ms); }
    let idx = samples.partition_point(|s| s.timestamp_ms <= target_ms);
    if idx == 0 { return samples[0].clone(); }
    if idx >= samples.len() { return samples.last().unwrap().clone(); }
    let before = &samples[idx - 1];
    let after = &samples[idx];
    let dt = after.timestamp_ms - before.timestamp_ms;
    if dt <= 0.0 { return before.clone(); }
    let t = (target_ms - before.timestamp_ms) / dt;
    NativeSample {
        timestamp_ms: target_ms,
        timestamp: if t < 0.5 { before.timestamp.clone() } else { after.timestamp.clone() },
        latitude: lerp(before.latitude, after.latitude, t),
        longitude: lerp(before.longitude, after.longitude, t),
        altitude: lerp(before.altitude, after.altitude, t),
        speed: lerp(before.speed, after.speed, t),
        heading: lerp_heading(before.heading, after.heading, t),
        iso: if t < 0.5 { before.iso } else { after.iso },
        aperture: if t < 0.5 { before.aperture } else { after.aperture },
        shutter_speed: if t < 0.5 { before.shutter_speed } else { after.shutter_speed },
        focal_length: if t < 0.5 { before.focal_length } else { after.focal_length },
        ev: lerp(before.ev, after.ev, t),
        color_temperature: if t < 0.5 { before.color_temperature } else { after.color_temperature },
        g_force: lerp(before.g_force, after.g_force, t),
    }
}

fn lerp(a: Option<f64>, b: Option<f64>, t: f64) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a + (b - a) * t),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn lerp_heading(a: Option<f64>, b: Option<f64>, t: f64) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let mut diff = b - a;
            if diff > 180.0 { diff -= 360.0; }
            if diff < -180.0 { diff += 360.0; }
            Some((a + diff * t + 360.0) % 360.0)
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}
```

### Step D2: Implement FBMA smoothing

```rust
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

fn smooth_series(samples: &mut [NativeSample], fps: usize) {
    let altitude: Vec<_> = samples.iter().map(|s| s.altitude).collect();
    let speed: Vec<_> = samples.iter().map(|s| s.speed).collect();
    let heading: Vec<_> = samples.iter().map(|s| s.heading).collect();
    let g_force: Vec<_> = samples.iter().map(|s| s.g_force).collect();

    let sa = zero_phase_smooth(&altitude, fps.max(1));
    let ss = zero_phase_smooth(&speed, (fps / 2).max(1));
    let sh = zero_phase_smooth(&heading, (fps / 2).max(1));
    let sg = zero_phase_smooth(&g_force, fps.max(1));

    for i in 0..samples.len() {
        samples[i].altitude = sa[i];
        samples[i].speed = ss[i];
        samples[i].heading = sh[i];
        samples[i].g_force = sg[i];
    }
}
```

### Step D3: Implement `compute_distance_series()`

Haversine distance accumulation from lat/lon course points:

```rust
fn haversine_meters(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6_371_000.0; // Earth radius in meters
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2) + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    R * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())
}

fn compute_distance_series(course: &[(Option<f64>, Option<f64>)]) -> Vec<f64> {
    let mut distances = Vec::with_capacity(course.len());
    let mut cumulative = 0.0;
    distances.push(0.0);
    for i in 1..course.len() {
        match (course[i - 1], course[i]) {
            ((Some(lat1), Some(lon1)), (Some(lat2), Some(lon2))) => {
                cumulative += haversine_meters(lat1, lon1, lat2, lon2);
            }
            _ => {}
        }
        distances.push(cumulative);
    }
    distances
}
```

### Step D4: Implement `compute_gradient_series()`

```rust
fn compute_gradient_series(elevation: &[Option<f64>], distance: &[f64]) -> Vec<Option<f64>> {
    let len = elevation.len();
    let mut gradient = Vec::with_capacity(len);
    for i in 0..len {
        // Look back ~5m and forward ~5m for gradient
        let mut left = i;
        while left > 0 && distance[i] - distance[left] < 5.0 { left -= 1; }
        let mut right = i;
        while right < len - 1 && distance[right] - distance[i] < 5.0 { right += 1; }
        match (elevation[left], elevation[right]) {
            (Some(el), Some(er)) => {
                let horiz = distance[right] - distance[left];
                if horiz >= 1.0 {
                    let g = ((er - el) / horiz * 100.0).clamp(-30.0, 30.0);
                    gradient.push(Some((g * 1000.0).round() / 1000.0));
                } else {
                    gradient.push(gradient.last().and_then(|v| *v));
                }
            }
            _ => gradient.push(gradient.last().and_then(|v| *v)),
        }
    }
    gradient
}
```

---

## Phase E: Tauri Command Wiring

### Step E1: Add `backend_extract_video_telemetry` to `commands/mod.rs`

```rust
pub fn backend_extract_video_telemetry(file_path: &str) -> CoreResult<Value> {
    match crate::encode::telemetry::extract_telemetry(file_path)? {
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
 * Extracts embedded telemetry from a video file as a ParsedActivity.
 *
 * @param {string} path - Absolute path to the source video file.
 * @returns {Promise<object|null>} Promise resolving to ParsedActivity or null.
 */
export async function extractVideoTelemetry(path) {
  const result = await invokeCommand("backend_extract_video_telemetry", { filePath: path });
  return typeof result === "string" ? JSON.parse(result) : result;
}
```

### Step F2: Add state to `createVideoImportSlice.js`

Add new fields. Note: `videoTelemetry` is a complete `ParsedActivity` from Rust —
all derived metrics (speed, heading, gradient, pace, distance) are already computed.
No `finalizeParsedActivity()` call needed. No `useWindowedRate` smoothing needed.

```javascript
videoTelemetry: null,              // ParsedActivity from telemetry extraction (complete, no JS processing needed)
videoTelemetryCreationTime: null,  // ISO-8601 from GPS inference
```

Add setter:

```javascript
setVideoTelemetry: (result) => set({
  videoTelemetry: result,
  videoTelemetryCreationTime: result?.source_start_time ?? null,
}),
```

Update `clearImportedVideo` to also clear:

```javascript
videoTelemetry: null,
videoTelemetryCreationTime: null,
```

### Step F3: Update `useVideoImport.js` to call extraction

After `importPreviewVideo()` succeeds, fire-and-forget telemetry extraction:

```javascript
import { extractVideoTelemetry, clearPreviewVideo, importPreviewVideo } from "@/api/backend";

// Inside handleImportVideo, after setImportedVideo(metadata):
extractVideoTelemetry(selected)
  .then((result) => {
    if (result) {
      setVideoTelemetry(result);
      // If GPS-inferred creation time available and ffprobe didn't provide one,
      // update creation time for sync
      if (result.source_start_time && !metadata.creationTime) {
        // Update store with GPS-inferred creation time
        const updatedMetadata = { ...metadata, creationTime: result.source_start_time };
        setImportedVideo(updatedMetadata);
      }
    }
  })
  .catch((err) => console.warn("Telemetry extraction failed:", err));
```

### Step F4: Merge strategy (when FIT/GPX also imported)

When the user imports FIT/GPX after video telemetry, the merge happens in the
render payload builder. For each metric series:

- If FIT/GPX has non-null values → use FIT/GPX
- Otherwise → use video telemetry values

This is the existing `combineSeries()` pattern in `metric-series.js`. The
`videoTelemetry` object from the store is passed alongside `importedActivity`
and merged before rendering.

---

## Phase G: Testing

### Step G1: Unit tests in `telemetry.rs`

`#[cfg(test)] mod tests` block:

- `test_compute_rational_fps()` — 23.976, 24, 25, 29.97, 30, 59.94, 60
- `test_moving_average()` — null handling, window sizes
- `test_zero_phase_smooth()` — zero delay, smoothing behavior
- `test_lerp()` — basic interpolation
- `test_lerp_heading()` — 0°/360° wraparound
- `test_interpolate_at_timestamp()` — mock NativeSample data
- `test_extract_creation_time_from_samples()` — GPS timestamp inference
- `test_unix_to_rfc3339()` — timestamp conversion
- `test_haversine_meters()` — known coordinate pairs
- `test_compute_distance_series()` — cumulative distance
- `test_compute_gradient_series()` — elevation/distance gradient

### Step G2: Integration tests

`src-tauri/ovrley_core/tests/telemetry_tests.rs`:

- `test_probe_video_metadata_with_gopro()` — if fixture available
- `test_extract_telemetry_with_gopro()` — verify ParsedActivity shape
- `test_extract_telemetry_no_telemetry()` — verify returns None

### Step G3: Manual testing

Test with sample videos:

- GoPro HERO 5+ (GPS + camera settings)
- Sony a7 series (GPS + camera settings)
- DJI drone (GPS + camera settings)
- Insta360 (GPS + camera settings)
- Screen recording (no telemetry — verify fallback)

---

## Unanswered Questions

1. **Merge strategy on frontend**: When both telemetry and FIT/GPX are present,
   should we merge at the store level (single combined ParsedActivity) or keep
   them separate and merge at render time? The current store has a single
   `importedActivity` — likely need to add a `videoTelemetry` slice and merge
   when building the render payload.

2. **gForce baseline subtraction**: Accelerometer reads ~1g at rest (gravity).
   Should we subtract 1g to get net g-force, or expose the raw magnitude? The
   existing `g_force` field in `ParsedActivity` is described as "multiples of
   Earth gravity" which suggests net force (subtract 1g).

3. **Edge-case: no telemetry at all**: Some videos (screen recordings, webcams)
   have no embedded telemetry. `extract_telemetry()` returns `None`. The frontend
   should handle this gracefully — no activity imported, user can still import
   FIT/GPX separately. Confirm this is expected behavior?

4. **Window size for FBMA**: The proposed window sizes (fps/2 for speed/heading,
   fps for altitude/gForce) are starting points. Should these be configurable
   or hard-coded initially?

### Resolved Questions

- **Interpolation method (was Q3)**: Linear interpolation for continuous values
  (speed, altitude, heading, gForce), hold-last-value for discrete values (ISO,
  aperture, shutter speed, focal length, color temperature). ~2ms overhead for
  60-min footage — negligible vs ~1-3s telemetry-parser parsing.

- **Partial telemetry (was Q5)**: Rust returns null for missing fields, empty
  arrays `[]` for metrics not available from the camera (heartrate, cadence,
  power, etc.). Frontend merge logic handles gracefully.

- **Multiple GPS samples per frame (was Q7)**: Linear interpolation handles
  this naturally — interpolates between surrounding GPS samples at each frame
  timestamp. No special handling needed.
