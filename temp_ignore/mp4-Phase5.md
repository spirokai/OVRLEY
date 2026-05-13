# Phase 5 — MP4 Compositing FFmpeg Pipeline (Backend) — REVISED

**Goal**: Build a parallel encoding pipeline that composites Skia-rendered overlay frames on top of an imported MP4 video using ffmpeg's `filter_complex`, **correctly handling the inherent FPS mismatch** between the video source and the overlay stream.

> **CRITICAL**: All new code must be in **separate files**. The existing `video_pipeline.rs`, `ffmpeg.rs`, and `video_debug.rs` files are **sacred** — they drive the transparent overlay export and must not be modified.

---

## 1. The FPS Mismatch Problem

The composite pipeline must handle **three distinct framerates**, which are almost never equal:

| Stream | Source | Typical Values |
|--------|--------|----------------|
| **Video FPS** (`video_fps`) | Native framerate of the imported MP4 file, detected by ffprobe in Phase 1 | 29.97, 59.94, 23.976, 25, 30, 60 |
| **Overlay FPS** (`overlay_fps`) | User-chosen integer fps for the overlay widget data. Data sampled at 1 Hz and interpolated to this rate. | 24, 30, 60 |
| **Output FPS** (`output_fps`) | Framerate of the final composited MP4 | Must be deliberately chosen |

The original Phase 5 plan stated *"Iterates through every frame of the output video (matching background FPS)"* — this is ambiguous when video FPS and overlay FPS differ (which is the common case). The revised pipeline makes the strategy explicit.

### Resolution Strategy

**The output FPS is set to the video's native FPS (rounded up to integer).** The overlay frames are delivered at `output_fps` rate but only re-rendered by Skia when the overlay's frame index changes. This:

- Preserves the source video's motion smoothness (no frame dropping on the video).
- Avoids resampling the video, which would introduce motion artifacts.
- Renders the overlay no more often than its intended fps, saving CPU.
- Takes advantage of the fact that overlay data is already heavily interpolated from 1 Hz telemetry — consecutive overlay frames at 60 fps differ by imperceptible deltas, so capping to video fps is visually lossless.

| Scenario | Video FPS | Overlay FPS | Output FPS | Skia Renders per Second | Frame Cache Reuse |
|----------|-----------|-------------|------------|------------------------|-------------------|
| Overlay ≤ Video | 59.94 | 30 | 60 | 30 (every 2nd output frame) | Every other frame |
| Overlay ≤ Video | 29.97 | 24 | 30 | 24 (most frames, skip ~1 in 5) | Occasional |
| Overlay ≥ Video | 29.97 | 60 | 30 | 30 (every output frame, capped) | None |
| Overlay = Video | 29.97 | 30 | 30 | 30 (every output frame) | None |

---

## 2. FPS Matching Algorithm

### 2.1 Pipeline Framerate Definitions

```
video_fps     = imported_video_fps                  // float, e.g. 29.97
output_fps    = video_fps.ceil() as u32              // integer, e.g. 30
overlay_fps   = config.scene.fps                     // integer, e.g. 30 (user-chosen, unchanged)
video_duration = imported_video_duration              // seconds, float
total_output_frames = (video_duration * output_fps).ceil() as u64
```

### 2.2 Output Frame → Overlay Frame Mapping

For each output frame at index `i`:

```
current_time = i / output_fps as f64
overlay_frame_index = (current_time * overlay_fps).floor() as usize
```

The overlay is re-rendered by Skia only when `overlay_frame_index` changes from the previous iteration. When it stays the same, the cached overlay buffer is reused.

### 2.3 Fractional Video FPS Handling

Fractional framerates (29.97, 59.94, 23.976) are rounded **up** to the nearest integer:

| Detected FPS | Output FPS | Rationale |
|--------------|------------|-----------|
| 23.976 | 24 | Rounds up — no content loss |
| 29.97 | 30 | Rounds up — industry standard |
| 59.94 | 60 | Rounds up — industry standard |
| 25 | 25 | Already integer |
| 30 | 30 | Already integer |
| 60 | 60 | Already integer |

Ceil rounding causes the output to have slightly more frames than the source video. ffmpeg handles this gracefully by repeating the last video frame if the input stream runs out before the output duration is reached. The alternative (floor or round) would truncate the final fraction of a second of video, which is worse.

### 2.4 Interaction with `config.scene.fps` and `config.update_rate`

**Important: `config.scene.fps` is NOT overridden by video fps.** It retains the user-chosen overlay fps value. The composite pipeline reads both independently:

| Config Field | Transparent Pipeline | Composite Pipeline |
|---|---|---|
| `config.scene.fps` | Overlay frame rate = output fps | Overlay frame rate only |
| `config.update_rate` | Divides container fps | **Ignored** — composite uses its own update logic |
| `config.container_fps()` | `fps / update_rate` | **Ignored** — output fps is `ceil(video_fps)` |
| `config.widget_update_rate()` | Frame decimation factor | **Ignored** — composite uses overlay_frame_index comparison |

The frontend (Phase 6) passes the video fps as a **separate** composite parameter (`composite_video_fps`), not by overriding `config.scene.fps`.

---

## 3. FFmpeg Command Structure

### 3.1 Two-Input Architecture

The compositing pipeline spawns ffmpeg with two inputs:

**Input 0**: The imported MP4 video file, decoded by ffmpeg (optionally with hardware acceleration).

```
-hwaccel <method> -ss <sync_offset> -t <duration> -i <video_path>
```

- `-ss` seeks to the sync offset within the video.
- `-t` limits the input to the render duration.
- `-hwaccel` is set per profile (nvdec, cuda, qsv, videotoolbox, or omitted for software).

**Input 1**: Raw RGBA frames piped via stdin, at the **output fps** (video fps rounded up).

```
-f rawvideo -s WxH -pix_fmt rgba -r <output_fps> -i pipe:0
```

- `-r <output_fps>` tells ffmpeg to expect frames at this rate.
- The Rust pipeline writes one overlay frame (cached or freshly rendered) for each output frame.

### 3.2 filter_complex

The two inputs are composited using ffmpeg's `overlay` filter:

**Software (default)**:
```
[0:v]scale=WxH[base];[1:v]format=yuva420p[overlay];[base][overlay]overlay=0:0
```

**CUDA (nnvgpu profile)**:
```
[0:v]scale_cuda=format=yuv420p[mp4_stream];[1:v]format=yuva420p,hwupload[overlay_stream];[mp4_stream][overlay_stream]overlay_cuda
```

**QSV (Intel)**:
```
[0:v]hwupload=extra_hw_frames=64[main_hw];[1:v]hwupload=extra_hw_frames=64,format=qsv[overlay_hw];[main_hw][overlay_hw]overlay_qsv=x=0:y=0,hwdownload,format=nv12
```

### 3.3 Output Arguments

```
-c:v <codec> -b:v <bitrate> [codec_specific_args...] -c:a copy -movflags faststart -y <output_path>
```

- `-c:a copy` preserves the original video's audio track without re-encoding.
- `-movflags faststart` enables progressive playback.
- Bitrate is overridden by the user's slider value.

### 3.4 Complete Command Template

```
ffmpeg -loglevel info \
  [hw_init_args...] \
  [-hwaccel ...] -ss <offset> -t <duration> -i <video.mp4> \
  -f rawvideo -s WxH -pix_fmt rgba -r <output_fps> -i pipe:0 \
  -filter_complex "<filter_string>" \
  -c:v <codec> -b:v <bitrate> [codec_args...] \
  -c:a copy \
  -movflags faststart \
  -y output.mp4
```

---

## 4. Frame Rendering Loop (Revised)

The main loop differs fundamentally from the transparent pipeline:

### Pseudocode

```
let output_fps = ceil(video_fps);
let overlay_fps = config.scene.fps;  // as-is, not overridden
let total_frames = ceil(video_duration * output_fps);
let total_overlay_frames = ceil(video_duration * overlay_fps);

// Initialize frame cache
let mut cached_overlay: Vec<u8> = vec![0u8; width * height * 4];
let mut last_overlay_index: Option<usize> = None;

// ffmpeg process: two inputs (video file + pipe:0)
let mut ffmpeg = spawn_composite_ffmpeg(...)?;

for output_frame_index in 0..total_frames {
    if cancel_flag.load(Ordering::SeqCst) { break; }

    let frame_started = Instant::now();
    let current_time = output_frame_index as f64 / output_fps as f64;
    let overlay_index = (current_time * overlay_fps as f64).floor() as usize;

    // Acquire a buffer from the pool
    let mut frame_buffer = acquire_frame_buffer(...)?;

    if Some(overlay_index) != last_overlay_index {
        // Re-render overlay
        render_frame_rgba(
            paths, config, dense_activity, &prepared_assets,
            overlay_index.min(total_overlay_frames - 1),
            scale, None,
            RenderTarget { width, height, pixels: frame_buffer.pixels.as_mut_slice() },
            &mut profiler,
        )?;
        // Update cache
        cached_overlay.copy_from_slice(&frame_buffer.pixels);
        last_overlay_index = Some(overlay_index);
    } else {
        // Reuse cached overlay — no Skia render
        frame_buffer.pixels.copy_from_slice(&cached_overlay);
    }

    // Write to ffmpeg pipe:0 (encoder thread handles actual IO)
    queue_frame(&sender, frame_buffer, &cancel_flag, &mut profiler)?;

    let frame_ms = frame_started.elapsed().as_secs_f64() * 1000.0;
    profiler.record_ms("frame.total", frame_ms);

    // Update progress
    controller.set_frame_progress(...);
}
```

### Key Differences from Transparent Pipeline

| Aspect | Transparent Pipeline | Composite Pipeline |
|---|---|---|
| Total frames | `((layout_frames - 1) / update_rate) + 1` | `ceil(video_duration * output_fps)` |
| Skia render | Every output frame | Only when `overlay_index` changes |
| Frame cache | Not needed | Cached overlay buffer for reuse |
| ffmpeg inputs | 1 (pipe:0 only) | 2 (video file + pipe:0) |
| Output fps used | `config.container_fps()` | `ceil(video_fps)` |
| Progress total | `rendered_frame_count` | `total_frames` |

### Frame Cache Behavior by Scenario

```
Video: 59.94 fps → output 60 fps, Overlay: 30 fps
  Frame   0: t=0.000s  overlay_idx=0  → RENDER overlay, cache it
  Frame   1: t=0.017s  overlay_idx=0  → REUSE cached overlay
  Frame   2: t=0.033s  overlay_idx=1  → RENDER overlay, cache it
  Frame   3: t=0.050s  overlay_idx=1  → REUSE cached overlay
  ...

Video: 29.97 fps → output 30 fps, Overlay: 60 fps
  Frame   0: t=0.000s  overlay_idx=0  → RENDER
  Frame   1: t=0.033s  overlay_idx=1  → RENDER (capped, no reuse)
  Frame   2: t=0.067s  overlay_idx=2  → RENDER
  ...

Video: 29.97 fps → output 30 fps, Overlay: 24 fps
  Frame   0: t=0.000s  overlay_idx=0  → RENDER
  Frame   1: t=0.033s  overlay_idx=0  → REUSE  (no new overlay data yet)
  Frame   2: t=0.067s  overlay_idx=1  → RENDER
  Frame   3: t=0.100s  overlay_idx=2  → RENDER (overlay caught up)
  Frame   4: t=0.133s  overlay_idx=3  → RENDER
  Frame   5: t=0.167s  overlay_idx=4  → RENDER
```

---

## 5. Deliverables

### 5.1 [NEW] `src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs`

New module providing `build_composite_ffmpeg_settings()`:

**Input parameters:**
- `codec_name: &str` — e.g., `"h264_nvenc"`, `"libx264"`
- `bitrate: &str` — e.g., `"60M"`
- `video_path: &Path` — path to the imported MP4
- `sync_offset: f64` — seconds to seek into the video (`-ss`)
- `render_duration: f64` — seconds to render (`-t`)
- `width: u32, height: u32` — output resolution
- `output_fps: u32` — `ceil(video_fps)`
- `hwaccel_available: &HwAccelInfo` — detected hardware capabilities

**Returns:**
```rust
pub struct CompositeFfmpegSettings {
    pub input_0_args: Vec<String>,      // hwaccel + -ss + -t + -i for video file
    pub input_1_args: Vec<String>,      // -f rawvideo -s -pix_fmt -r -i pipe:0
    pub filter_complex: String,         // filter_complex string
    pub output_args: Vec<String>,       // -c:v -b:v -c:a copy -movflags faststart ...
    pub hw_init_args: Vec<String>,      // device init args (qsv only)
}
```

**Profile definitions** (matching Appendix A in `mp4-plan.md`):

| Profile Name | Codec | HW Decode | Filter | Notes |
|---|---|---|---|---|
| `software_h264` | `libx264` | None | CPU overlay | Universal fallback |
| `software_h265` | `libx265` | None | CPU overlay | Universal fallback |
| `nvgpu` | `h264_nvenc` | `nvdec` | CPU overlay | Good perf, no GPU filter needed |
| `nnvgpu` | `h264_nvenc` | `cuda` | `overlay_cuda` | Full GPU pipeline |
| `nnvgpu_hevc` | `hevc_nvenc` | `cuda` | `overlay_cuda` | Full GPU pipeline, HEVC |
| `mac` | `h264_videotoolbox` | `videotoolbox` | CPU overlay | macOS H.264 |
| `mac_hevc` | `hevc_videotoolbox` | `videotoolbox` | CPU overlay | macOS HEVC |
| `qsv` | `hevc_qsv` | `qsv` | `overlay_qsv` | Intel GPU pipeline |
| `qsv_h264` | `h264_qsv` | `qsv` | `overlay_qsv` | Intel GPU pipeline, H.264 |

Profile selection logic:

```rust
fn select_profile(codec_name: &str, hwaccel: &HwAccelInfo) -> &'static CompositeProfile {
    match codec_name {
        "h264_nvenc" if hwaccel.cuda => &PROFILES.nnvgpu,
        "h264_nvenc" => &PROFILES.nvgpu,
        "hevc_nvenc" if hwaccel.cuda => &PROFILES.nnvgpu_hevc,
        "hevc_nvenc" => &PROFILES.nvgpu_hevc,  // nvdec + hevc_nvenc
        "h264_videotoolbox" => &PROFILES.mac,
        "hevc_videotoolbox" => &PROFILES.mac_hevc,
        "h264_qsv" => &PROFILES.qsv_h264,
        "hevc_qsv" => &PROFILES.qsv,
        "libx264" => &PROFILES.software_h264,
        "libx265" => &PROFILES.software_h265,
        _ => &PROFILES.software_h264,  // fallback
    }
}
```

Bitrate override: The user's selected bitrate is injected via `-b:v` on the output args, replacing any default `-b:v` or `-global_quality` from the profile.

### 5.2 [NEW] `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs`

New module providing `render_composite_video_single()`:

**Function signature:**
```rust
pub(crate) fn render_composite_video_single(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    controller: &RenderController,
    composite_video_path: &str,
    composite_bitrate: &str,
    composite_sync_offset: f64,
    composite_video_fps: f64,
    composite_video_duration: f64,
) -> Result<String, String>
```

**Pipeline structure** (mirrors `video_pipeline::render_video_single` but with two-input ffmpeg):

1. **Parse composite parameters**: `output_fps = ceil(composite_video_fps)`, `total_frames = ceil(composite_video_duration * output_fps)`, `overlay_fps = config.scene.fps`.
2. **Prepare Skia assets**: same as transparent pipeline via `prepare_preview_assets()`.
3. **Build composite ffmpeg settings**: call `ffmpeg_composite::build_composite_ffmpeg_settings()`.
4. **Spawn ffmpeg** with two inputs:
   - Input 0: video file with `-ss offset -t duration`
   - Input 1: `pipe:0` with `-r output_fps`
   - `-filter_complex` for overlay compositing
5. **Spawn writer thread** (same as transparent pipeline — writes frames to stdin).
6. **Spawn monitor thread** (same as transparent pipeline — parses stderr for progress).
7. **Render loop** (as described in Section 4 above) with:
   - Frame buffer pool (same 12-buffer pool as transparent pipeline).
   - Cached overlay buffer for re-use.
   - `overlay_index` comparison for Skia render gating.
8. **Cleanup**: wait for ffmpeg, validate output, handle cancellation.
9. **Debug output**: write timing summary to `target/debug_render/phase_7`.

### 5.3 [MODIFY] `src-tauri/ovrley_core/src/encode/mod.rs`

Add:
```rust
pub mod ffmpeg_composite;
mod video_composite_pipeline;
```

### 5.4 [MODIFY] `src-tauri/ovrley_core/src/encode/video.rs`

Add a new `render_composite_video()` entry function:
```rust
pub fn render_composite_video(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    controller: &RenderController,
    composite_video_path: &str,
    composite_bitrate: &str,
    composite_sync_offset: f64,
    composite_video_fps: f64,
    composite_video_duration: f64,
) -> Result<String, String>
```

This function does NOT modify `render_video()`. It delegates directly to `video_composite_pipeline::render_composite_video_single()`.

### 5.5 [MODIFY] `src-tauri/ovrley_core/src/config/mod.rs`

Add fields to `SceneConfig`:
```rust
#[serde(default)]
pub composite_video_path: Option<String>,
#[serde(default)]
pub composite_bitrate: Option<String>,
#[serde(default)]
pub composite_sync_offset: Option<f64>,
#[serde(default)]
pub composite_video_fps: Option<f64>,
#[serde(default)]
pub composite_video_duration: Option<f64>,
```

All `#[serde(default)]` so existing configs are not affected. These are only injected at render time by the frontend (Phase 6), never persisted to template files.

Also update `parse_config_json()` validation: the `fps` check that requires integer fps with clean update_rate division still applies for transparent renders. For composite renders, the fps is the overlay fps (user-chosen integer), which already satisfies this check.

### 5.6 [MODIFY] `src-tauri/ovrley_core/src/commands/mod.rs`

In `backend_render()`:
```rust
pub fn backend_render(
    paths: &AppPaths,
    controller: &RenderController,
    config_json: &str,
    parsed_activity_json: &str,
) -> Result<Value, String> {
    let config = parse_config_json(config_json)?;
    let parsed_activity = parse_activity_json(parsed_activity_json)?;
    let dense_activity = build_dense_activity_report(&parsed_activity, &config)?;

    // Branch: composite mode or transparent mode
    if let Some(ref video_path) = config.scene.composite_video_path {
        let bitrate = config.scene.composite_bitrate.clone()
            .ok_or_else(|| "composite_bitrate required for composite render".to_string())?;
        let sync_offset = config.scene.composite_sync_offset.unwrap_or(0.0);
        let video_fps = config.scene.composite_video_fps
            .ok_or_else(|| "composite_video_fps required for composite render".to_string())?;
        let video_duration = config.scene.composite_video_duration
            .ok_or_else(|| "composite_video_duration required for composite render".to_string())?;

        let output_fps = video_fps.ceil() as u32;
        let total_frames = (video_duration * output_fps as f64).ceil() as u32;
        let render_id = controller.try_start(total_frames, "Compositing video...")?;

        let controller_clone = controller.clone();
        let paths = paths.clone();
        std::thread::spawn(move || {
            match render_composite_video(
                &paths, &config, &parsed_activity, &dense_activity,
                &controller_clone, video_path, &bitrate, sync_offset,
                video_fps, video_duration,
            ) {
                Ok(filename) => controller_clone.finish_success(filename),
                Err(error) => {
                    let cancelled = error.to_lowercase().contains("cancelled");
                    controller_clone.finish_error(error, cancelled);
                }
            }
        });

        Ok(json!({ "started": true, "render_id": render_id }))
    } else {
        // Existing transparent pipeline (unchanged)
        let output_frame_count = rendered_frame_count(
            dense_activity.frame_count,
            config.widget_update_rate() as usize,
        );
        let render_id = controller.try_start(output_frame_count as u32, "Preparing render assets...")?;
        let controller_clone = controller.clone();
        let paths = paths.clone();
        std::thread::spawn(move || {
            match render_video(&paths, &config, &parsed_activity, &dense_activity, &controller_clone) {
                Ok(filename) => controller_clone.finish_success(filename),
                Err(error) => {
                    let cancelled = error.to_lowercase().contains("cancelled");
                    controller_clone.finish_error(error, cancelled);
                }
            }
        });
        Ok(json!({ "started": true, "render_id": render_id }))
    }
}
```

---

## 6. Debug Timings

The composite pipeline writes timing summaries to `target/debug_render/phase_7` (distinct from transparent exports which go to `phase_6`). Timings include:

### Existing Buckets (inherited from transparent pipeline)

| Bucket | Source | Description |
|--------|--------|-------------|
| `frame.total` | Render loop | Wall time per output frame (including cache reuse) |
| `buffer.acquire_wait` | Render loop | Time waiting for a free buffer from the pool |
| `buffer.release_wait` | Writer thread | Time returning a buffer to the pool |
| `queue.put_wait` | Render loop | Time waiting to enqueue a frame for the writer |
| `encoder.queue_wait` | Writer thread | Time waiting for a frame from the renderer |
| `ffmpeg.write` | Writer thread | Time writing frame bytes to ffmpeg stdin |
| `debug.sample_frame_write` | Render loop | Time writing sample frames for debugging |

### New Composite Buckets

| Bucket | Source | Description |
|--------|--------|-------------|
| `composite.overlay_skipped` | Render loop | Count of frames where cached overlay was reused (no Skia render) |
| `composite.overlay_rendered` | Render loop | Count of frames where Skia rendered a new overlay |
| `composite.total_ms` | Top level | Full wall-clock time from start to finish |
| `ffmpeg.decode_ms` | Estimated | Time ffmpeg spends decoding input video (parsed from `-benchmark` stderr or estimated as total_wall - skia_time - encode_time) |
| `ffmpeg.encode_ms` | Writer thread | Time spent encoding (stdin write latency as backpressure proxy) |
| `ffmpeg.filter_ms` | Estimated | Time in filter_complex (total_decode - encode - remaining) |

The summary JSON format matches the existing structure with `"phase": "phase_7"`.

---

## 7. Audio Handling

Audio is passthrough via `-c:a copy`. The pipeline assumes:
- The imported MP4 has at most one audio track.
- The audio is in a format compatible with the output container (MP4).
- No re-encoding or audio filtering is performed.

If the video has no audio track, ffmpeg silently skips the `-c:a copy` (no error).

---

## 8. Sync Offset Timing

### Offset Applied to Input 0 Only

The `-ss` and `-t` arguments are applied only to Input 0 (the video file):
```
-ss <sync_offset> -t <render_duration> -i <video.mp4>
```

This means:
- The video starts playing from `sync_offset` seconds into the file.
- Only `render_duration` seconds of video are decoded.

**The overlay frames (Input 1) are NOT offset via `-ss`**. Instead, the overlay timing uses `composite_video_duration` and `output_fps` directly. The overlay always starts at activity time 0 and covers the full render duration. The sync offset shifts which part of the video aligns with the overlay.

The effective timeline:
```
Video time 0 = activity time sync_offset
Video frame at time t = overlay frame at time t
```

### Render Duration

`render_duration` is set to `min(video_duration, activity_duration - sync_offset)` — the shorter of the available video and the remaining activity after sync. The frontend (Phase 6) computes this value and passes it.

---

## 9. File Change Summary

### New Files (2)

| File | Purpose |
|------|---------|
| `src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs` | Composite ffmpeg settings builder, profile definitions |
| `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs` | Composite render pipeline with FPS-aware frame loop |

### Modified Files (4)

| File | Change |
|------|--------|
| `src-tauri/ovrley_core/src/encode/mod.rs` | Add `pub mod ffmpeg_composite;` and `mod video_composite_pipeline;` |
| `src-tauri/ovrley_core/src/encode/video.rs` | Add `render_composite_video()` entry function |
| `src-tauri/ovrley_core/src/config/mod.rs` | Add `composite_*` fields to `SceneConfig` (all `Option`, `#[serde(default)]`) |
| `src-tauri/ovrley_core/src/commands/mod.rs` | Branch on `composite_video_path` in `backend_render()` |

### Untouched Sacred Files

- `src-tauri/ovrley_core/src/encode/video_pipeline.rs` — existing render pipeline
- `src-tauri/ovrley_core/src/encode/ffmpeg.rs` — existing ffmpeg settings
- `src-tauri/ovrley_core/src/encode/video_debug.rs` — existing debug utilities

---

## 10. Manual Tests

1. **Video at 29.97 fps, overlay at 30 fps**: Render with H.264 CPU. Verify output is 30 fps with overlay composited on each frame. Frame stutters not perceptible.

2. **Video at 59.94 fps, overlay at 30 fps**: Render with H.264 NVENC. Verify output is 60 fps. Overlay updates every 2nd frame, cached frame used for intermediate frames.

3. **Video at 29.97 fps, overlay at 60 fps**: Render with H.265 CPU. Verify output is 30 fps. Overlay rendered every frame (capped at video rate).

4. **Video at 23.976 fps, overlay at 24 fps**: Render. Verify output is 24 fps. Nearly 1:1 mapping.

5. **Audio preservation**: Verify output MP3 contains original audio track (`-c:a copy`).

6. **Sync offset**: Import video with 10s offset. Verify video starts at 10s mark, overlay aligned correctly.

7. **Debug timings**: Verify `target/debug_render/phase_7/timing_summary.json` contains all composite-specific buckets and `"phase": "phase_7"`.

8. **FFmpeg profiles**: Test each profile path (software, nvgpu, nnvgpu, mac, mac_hevc, qsv) on available hardware.

9. **Cancel mid-render**: Verify no orphan files remain.

10. **No video imported**: Verify `backend_render` follows transparent pipeline (no change in behavior).
