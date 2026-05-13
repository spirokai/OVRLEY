# Phase 5 — MP4 Compositing FFmpeg Pipeline — Detailed Implementation Plan

## 0. Purpose and Non-Negotiable Constraints

### Goal

Build a parallel backend MP4 compositing pipeline that renders transparent Skia overlay frames and composites them over an imported MP4/MOV/MKV video using FFmpeg `filter_complex`.

The existing transparent overlay export pipeline must remain untouched.

Conceptually, the new pipeline is:

```txt
Imported video frame
        +
Transparent Skia overlay frame
        ↓
FFmpeg overlay filter
        ↓
Final composited frame
        ↓
H.264 / H.265 encoder
        ↓
Output MP4
```

### Absolute constraints

The following existing files are sacred and must not be modified:

```txt
src-tauri/ovrley_core/src/encode/video_pipeline.rs
src-tauri/ovrley_core/src/encode/ffmpeg.rs
src-tauri/ovrley_core/src/encode/video_debug.rs
```

All new pipeline logic must live in separate files. You are explicitly allowed to import/reuse functions from the sacred files if possible.

Composite mode must be a parallel backend path, not a mutation of the existing transparent export path.

### Core behavioral rules

1. The imported source video FPS is the master clock in MP4 compositing mode.
2. The final MP4 output FPS must equal the imported video FPS.
3. Fractional NTSC rates must not be rounded:
   - `24000/1001` must not become `24`.
   - `30000/1001` must not become `30`.
   - `60000/1001` must not become `60`.
4. The overlay pipe FPS is derived from the source FPS and widget update rate:

```txt
overlay_pipe_fps = source_video_fps / composite_widget_update_rate
```

5. In composite mode, `config.scene.fps` must not override the output video FPS.
6. In composite mode, the dense activity report should be rebuilt using composite-specific scene timing:

```txt
scene.start = composite_sync_offset
scene.end   = composite_sync_offset + render_duration
scene.fps   = source_video_fps / composite_widget_update_rate
```

7. `composite_sync_offset` means activity time at which video time `0` begins. It must not be used as FFmpeg `-ss` seek.
8. A separate `composite_video_trim_start` field is required for video seek/trim behavior.
9. FFmpeg must receive the raw RGBA overlay stream through `pipe:0`.
10. FFmpeg should hold/repeat lower-FPS overlay frames between overlay updates.
11. Rust should render/write one frame per overlay-frame timestamp, not one frame per output-video timestamp.
12. Audio should be copied from the source video when present using optional audio mapping.
13. Full respect the "Key Implementation Warnings" in this plan.

---

## 1. Target Architecture

### 1.1 FFmpeg input model

The composite FFmpeg command uses two inputs.

| Input   | Source              | Description                                                               |
| ------- | ------------------- | ------------------------------------------------------------------------- |
| Input 0 | Imported video file | MP4/MOV/MKV decoded by FFmpeg, optionally with hardware acceleration      |
| Input 1 | Raw RGBA pipe       | Transparent overlay frames rendered by Skia and streamed through `pipe:0` |

### 1.2 Default software filter graph

The first robust implementation should use the CPU software overlay path:

```txt
[0:v]setpts=PTS-STARTPTS,scale=WxH[base];[1:v]setpts=PTS-STARTPTS[ovr];[base][ovr]overlay=0:0:eof_action=repeat:shortest=1,format=yuv420p[out]
```

This graph:

- Normalizes both timelines to start at zero.
- Scales the imported video to the overlay resolution.
- Uses the lower-FPS overlay stream as the overlay input.
- Repeats the last overlay frame until the next overlay timestamp arrives.
- Ends when the shortest stream ends.
- Converts the output to `yuv420p` for broad MP4 playback compatibility.
- Labels the final video stream as `[out]`.

### 1.3 Output mapping

Always explicitly map the filtered video stream and optional audio:

```bash
-map "[out]" -map 0:a?
```

Use:

```bash
-c:a copy
```

This should preserve source audio when present and avoid failure when no audio track exists.

---

## 2. Implementation Phases Overview

The work should be split into independent phases. Each phase should produce a usable, reviewable deliverable and include manual tests before moving to the next phase.

| Phase    | Scope                                               | Main Deliverable                                                       |
| -------- | --------------------------------------------------- | ---------------------------------------------------------------------- |
| Phase 1  | Composite data model and rational FPS helpers       | Config fields and `Fps` helper                                         |
| Phase 2  | FFmpeg composite argument builder                   | New `ffmpeg_composite.rs` module                                       |
| Phase 3  | Composite command branching and dense-report timing | Composite branch in `backend_render()`                                 |
| Phase 4  | Composite render entry point and pipeline shell     | New `render_composite_video()` and `video_composite_pipeline.rs` shell |
| Phase 5  | Overlay render loop and pipe writer                 | Functional software H.264 composite render                             |
| Phase 6  | Audio handling, cancellation, cleanup, and progress | Production-safe render control behavior                                |
| Phase 7  | Debug timings and diagnostics                       | Phase 7 timing summary output                                          |
| Phase 8  | Hardware encoder profiles and fallback strategy     | NVENC/QSV/VideoToolbox/VAAPI profile selection                         |
| Phase 9  | Optional full-GPU filter paths                      | CUDA/QSV experimental paths with safe fallback                         |
| Phase 10 | End-to-end validation matrix                        | Complete manual regression suite                                       |

---

# Phase 1 — Composite Data Model and Rational FPS Foundation

## Objective

Add the render-time configuration fields and FPS representation required by the compositing pipeline without changing the transparent render path.

## Files involved

### Modified files

```txt
src-tauri/ovrley_core/src/config/mod.rs
```

### New files

None required in this phase, unless a small FPS helper is placed in a new utility module.

Recommended location if separated:

```txt
src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs
```

or:

```txt
src-tauri/ovrley_core/src/encode/fps.rs
```

If an independent `fps.rs` is added, register it in:

```txt
src-tauri/ovrley_core/src/encode/mod.rs
```

## Implementation tasks

### 1. Add optional composite fields to `SceneConfig`

Add the following optional fields:

```rust
#[serde(default)]
pub composite_video_path: Option<String>,

#[serde(default)]
pub composite_bitrate: Option<String>,

#[serde(default)]
pub composite_sync_offset: Option<f64>,

#[serde(default)]
pub composite_video_fps_num: Option<u32>,

#[serde(default)]
pub composite_video_fps_den: Option<u32>,

#[serde(default)]
pub composite_video_duration: Option<f64>,

#[serde(default)]
pub composite_render_duration: Option<f64>,

#[serde(default)]
pub composite_video_trim_start: Option<f64>,

#[serde(default)]
pub composite_widget_update_rate: Option<u32>,
```

### 2. Treat these fields as render-time-only

These fields are injected by the frontend only at render time.

They must not be persisted into template files.

Implementation should confirm whether template serialization already strips transient fields. If it does not, add explicit serialization behavior or template-save filtering so these fields are excluded from saved templates.

### 3. Prefer rational FPS fields

Use:

```txt
composite_video_fps_num
composite_video_fps_den
```

instead of float-only FPS fields.

The backend should prefer rational FPS values from ffprobe or frontend metadata.

### 4. Add an `Fps` helper

Implement a small helper struct:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fps {
    pub num: u32,
    pub den: u32,
}

impl Fps {
    pub fn new(num: u32, den: u32) -> Result<Self, String> {
        if num == 0 {
            return Err("FPS numerator must be greater than zero".to_string());
        }
        if den == 0 {
            return Err("FPS denominator must be greater than zero".to_string());
        }
        Ok(Self { num, den }.reduced())
    }

    pub fn as_f64(&self) -> f64 {
        self.num as f64 / self.den as f64
    }

    pub fn ffmpeg_arg(&self) -> String {
        format!("{}/{}", self.num, self.den)
    }

    pub fn divided_by(&self, factor: u32) -> Result<Fps, String> {
        if factor == 0 {
            return Err("FPS division factor must be greater than zero".to_string());
        }
        Ok(Fps {
            num: self.num,
            den: self.den.saturating_mul(factor),
        }
        .reduced())
    }

    pub fn reduced(&self) -> Fps {
        let gcd = gcd_u32(self.num, self.den);
        Fps {
            num: self.num / gcd,
            den: self.den / gcd,
        }
    }
}
```

Add a simple `gcd_u32()` helper.

### 5. Add approximate float-to-rational conversion only as fallback

If the frontend initially only provides float FPS, the backend may convert common rates:

| Approximate float | Rational     |
| ----------------- | ------------ |
| `23.976`          | `24000/1001` |
| `29.97`           | `30000/1001` |
| `59.94`           | `60000/1001` |
| `25.0`            | `25/1`       |
| `30.0`            | `30/1`       |
| `60.0`            | `60/1`       |

This fallback should be isolated and not used when rational numerator/denominator fields are available.

### 6. Decide how composite FPS is represented internally

If `SceneConfig.fps` currently supports only integer FPS, do not force fractional values into it incorrectly.

Choose one of the following strategies:

#### Preferred strategy

Extend `SceneConfig.fps` or related timing logic to support rational or float FPS for composite mode.

#### Safer compatibility strategy

Keep persisted `scene.fps` unchanged and build an internal composite render config used only for dense-report generation.

The internal config should carry:

```txt
scene.start = composite_sync_offset
scene.end   = composite_sync_offset + render_duration
scene.fps   = overlay_pipe_fps as f64 or supported representation
```

## Deliverables

- Optional composite render-time fields added to `SceneConfig`.
- Render-time-only behavior documented or enforced.
- `Fps` helper implemented with:
  - validation,
  - reduction,
  - `as_f64()`,
  - `ffmpeg_arg()`,
  - `divided_by()`.
- Fallback float-to-rational conversion implemented if needed.
- No changes to the transparent render behavior.

## Manual tests after Phase 1

### Test 1.1 — Existing transparent config still parses

1. Use an existing transparent-render template/config with no `composite_*` fields.
2. Parse it through the backend.
3. Confirm parsing succeeds.
4. Confirm default values for all composite fields are `None`.

Expected result:

```txt
Existing transparent configs remain compatible.
```

### Test 1.2 — Composite config parses with all fields

1. Add the following render-time fields to a test config:

```json
{
  "composite_video_path": "test.mp4",
  "composite_bitrate": "60M",
  "composite_sync_offset": 300.0,
  "composite_video_fps_num": 30000,
  "composite_video_fps_den": 1001,
  "composite_video_duration": 20.0,
  "composite_render_duration": 10.0,
  "composite_video_trim_start": 0.0,
  "composite_widget_update_rate": 2
}
```

2. Parse it.
3. Confirm all fields are available in `config.scene`.

Expected result:

```txt
Composite render-time config parses successfully.
```

### Test 1.3 — Rational FPS is preserved

Create:

```rust
Fps::new(30000, 1001)
```

Confirm:

```txt
as_f64() ≈ 29.97002997
ffmpeg_arg() == "30000/1001"
```

Expected result:

```txt
29.97-style rates are represented as rationals, not rounded integers.
```

### Test 1.4 — Overlay FPS division

Create:

```rust
source_fps = 60000/1001
update_rate = 2
```

Confirm:

```txt
overlay_pipe_fps = 30000/1001
```

Then test:

```rust
source_fps = 60000/1001
update_rate = 6
```

Confirm:

```txt
overlay_pipe_fps = 10000/1001
```

Expected result:

```txt
Overlay pipe FPS is derived exactly from source FPS and update rate.
```

### Test 1.5 — Update rate zero protection

1. Set `composite_widget_update_rate = 0`.
2. Confirm backend clamps to `1` or rejects with a clear validation error.

Expected result:

```txt
No division by zero is possible.
```

---

# Phase 2 — Composite FFmpeg Argument Builder

## Objective

Create a new module that builds FFmpeg arguments for MP4 compositing mode without touching the existing FFmpeg builder.

## Files involved

### New file

```txt
src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs
```

### Modified file

```txt
src-tauri/ovrley_core/src/encode/mod.rs
```

## Implementation tasks

### 1. Register the new module

In:

```txt
src-tauri/ovrley_core/src/encode/mod.rs
```

add:

```rust
pub mod ffmpeg_composite;
```

If the composite pipeline module is added later, do not add it yet unless needed.

### 2. Define `CompositeProfile`

Add:

```rust
pub struct CompositeProfile {
    pub name: &'static str,
    pub input_args: Vec<String>,
    pub filter_complex: Option<String>,
    pub output_args: Vec<String>,
}
```

This struct represents profile-specific differences such as codec, hardware decode arguments, filter graph, and output encoder flags.

### 3. Define `CompositeFfmpegSettings`

Add:

```rust
pub struct CompositeFfmpegSettings {
    pub hw_init_args: Vec<String>,
    pub input_0_args: Vec<String>,
    pub input_1_args: Vec<String>,
    pub filter_complex: String,
    pub output_args: Vec<String>,
}
```

This struct should contain all argument groups needed to spawn FFmpeg.

### 4. Define or import `HwAccelInfo`

Use an existing hardware capability representation if one exists.

If not available yet, add a minimal placeholder for Phase 2:

```rust
pub struct HwAccelInfo {
    pub nvenc_available: bool,
    pub cuda_filters_available: bool,
    pub qsv_available: bool,
    pub qsv_filters_available: bool,
    pub videotoolbox_available: bool,
    pub vaapi_available: bool,
}
```

This can be replaced with real detection later.

### 5. Implement `build_composite_ffmpeg_settings()`

Suggested signature:

```rust
pub fn build_composite_ffmpeg_settings(
    codec_name: &str,
    bitrate: &str,
    video_path: &Path,
    video_trim_start: f64,
    render_duration: f64,
    width: u32,
    height: u32,
    source_fps: Fps,
    overlay_pipe_fps: Fps,
    hwaccel_available: &HwAccelInfo,
) -> Result<CompositeFfmpegSettings, String>
```

### 6. Validate inputs

Reject invalid values early:

- empty `codec_name`,
- empty `bitrate`,
- missing or empty video path,
- `render_duration <= 0.0`,
- `video_trim_start < 0.0`,
- `width == 0`,
- `height == 0`,
- invalid FPS numerator/denominator.

### 7. Build input 0 arguments

Default no-trim case:

```bash
-t <render_duration> -i <video_path>
```

Trim case:

```bash
-ss <composite_video_trim_start> -t <render_duration> -i <video_path>
```

Important:

```txt
Never add -ss <composite_sync_offset>.
```

Only `composite_video_trim_start` may become `-ss`.

### 8. Build input 1 raw RGBA pipe arguments

Input 1 must be:

```bash
-thread_queue_size 512 -f rawvideo -pix_fmt rgba -s <width>x<height> -r <overlay_pipe_fps> -i pipe:0
```

Example:

```bash
-thread_queue_size 512 -f rawvideo -pix_fmt rgba -s 3840x2160 -r 30000/1001 -i pipe:0
```

Requirements:

- `-thread_queue_size 512` must appear before the rawvideo pipe input.
- `-r` must use the overlay pipe FPS, not the output/source FPS.
- FPS should be formatted as a rational string when possible.

### 9. Build default software filter graph

For Phase 2, implement the robust software filter graph:

```txt
[0:v]setpts=PTS-STARTPTS,scale=<width>:<height>[base];[1:v]setpts=PTS-STARTPTS[ovr];[base][ovr]overlay=0:0:eof_action=repeat:shortest=1,format=yuv420p[out]
```

Do not add unnecessary `format=rgba` to Input 1 in the software path because the raw input already declares `-pix_fmt rgba`.

### 10. Build output arguments

Output arguments must include:

```bash
-map "[out]" -map 0:a? -r <source_video_fps> -c:v <codec> -b:v <bitrate> -c:a copy -movflags faststart -y <output_path>
```

At this stage, `output_path` can be appended by the caller if the existing spawning architecture expects that.

If included in this builder, make the output path explicit in the function signature.

### 11. Preserve source FPS as output FPS

Use:

```bash
-r <source_video_fps>
```

with rational string formatting:

```txt
30000/1001
60000/1001
24000/1001
```

Never use rounded float/int values.

### 12. Keep bitrate user-controlled

The user's selected bitrate must be passed through:

```bash
-b:v <bitrate>
```

Examples:

```bash
-b:v 20M
-b:v 60M
-b:v 100M
```

This setting should override profile defaults.

## Deliverables

- New `ffmpeg_composite.rs` file.
- `CompositeProfile` struct.
- `CompositeFfmpegSettings` struct.
- `Fps` helper available to this module.
- `build_composite_ffmpeg_settings()` implemented for software overlay paths.
- Default H.264/H.265 software command argument support.
- `-thread_queue_size 512` added before rawvideo pipe input.
- Output mapping implemented with `-map "[out]" -map 0:a?`.
- Audio copy argument implemented with `-c:a copy`.
- Output FPS preserved as rational source FPS.
- No changes to the sacred FFmpeg file.

## Manual tests after Phase 2

### Test 2.1 — Build command for 29.97 fps source

Inputs:

```txt
source_fps = 30000/1001
overlay_pipe_fps = 30000/1001
codec = libx264
bitrate = 60M
width = 3840
height = 2160
render_duration = 10.0
trim_start = 0.0
```

Inspect built arguments.

Expected result:

```txt
Input 1 contains: -r 30000/1001
Output args contain: -r 30000/1001
No value is rounded to 30.
```

### Test 2.2 — Build command with lower overlay update rate

Inputs:

```txt
source_fps = 60000/1001
overlay_pipe_fps = 30000/1001
codec = libx264
bitrate = 60M
```

Expected result:

```txt
Input 1 contains: -r 30000/1001
Output args contain: -r 60000/1001
```

### Test 2.3 — Confirm no sync offset seek appears

Build command for:

```txt
composite_sync_offset = 300
composite_video_trim_start = 0
```

Expected result:

```txt
No argument pair "-ss 300" exists.
```

### Test 2.4 — Confirm trim seek appears only for trim

Build command for:

```txt
composite_video_trim_start = 10
```

Expected result:

```txt
Input 0 contains: -ss 10
Input 0 contains: -t <render_duration>
Input 0 contains: -i <video_path>
```

### Test 2.5 — Confirm rawvideo pipe input shape

Inspect command.

Expected result:

```txt
-thread_queue_size 512 -f rawvideo -pix_fmt rgba -s 3840x2160 -r <overlay_pipe_fps> -i pipe:0
```

### Test 2.6 — Confirm filter graph labels output

Inspect filter graph.

Expected result:

```txt
Filter graph contains [out].
Output args contain -map [out].
```

### Test 2.7 — Confirm optional audio map

Inspect output args.

Expected result:

```txt
-map 0:a? is present.
-c:a copy is present.
```

---

# Phase 3 — Composite Branching and Dense-Report Timing in `backend_render()`

## Objective

Modify the backend render command so it detects composite mode before dense activity report generation, derives composite timing, builds the dense report at overlay pipe FPS, and then dispatches to the composite pipeline.

## Files involved

### Modified file

```txt
src-tauri/ovrley_core/src/commands/mod.rs
```

## Implementation tasks

### 1. Parse config and activity as before

Keep existing parsing behavior:

```rust
let mut config = parse_config_json(config_json)?;
let parsed_activity = parse_activity_json(parsed_activity_json)?;
```

### 2. Branch before dense activity report generation

Currently, the transparent path may build dense activity before rendering.

Composite mode must branch before dense activity generation because the dense report timing must use:

```txt
scene.start = composite_sync_offset
scene.end   = composite_sync_offset + render_duration
scene.fps   = overlay_pipe_fps
```

### 3. Detect composite mode

Composite mode is active when:

```rust
config.scene.composite_video_path.is_some()
```

If absent, follow the existing transparent pipeline unchanged.

### 4. Validate required composite fields

When `composite_video_path` is present, validate:

```txt
composite_bitrate
composite_video_fps_num
composite_video_fps_den
composite_video_duration
```

Optional fields and defaults:

| Field                          | Default                       |
| ------------------------------ | ----------------------------- |
| `composite_sync_offset`        | `0.0`                         |
| `composite_video_trim_start`   | `0.0`                         |
| `composite_widget_update_rate` | `1`                           |
| `composite_render_duration`    | `video_duration - trim_start` |

Reject:

- missing bitrate,
- missing FPS numerator,
- missing FPS denominator,
- denominator `0`,
- numerator `0`,
- missing video duration,
- `video_duration <= 0`,
- `trim_start < 0`,
- `trim_start >= video_duration`,
- computed `render_duration <= 0`,
- `sync_offset < 0` if negative activity time is unsupported,
- update rate `0` unless clamped to `1`.

### 5. Derive source FPS

Build:

```rust
let source_fps = Fps::new(fps_num, fps_den)?;
```

This is both:

```txt
source_video_fps
output_fps
```

### 6. Derive overlay pipe FPS

Use:

```rust
let update_rate = config.scene.composite_widget_update_rate.unwrap_or(1).max(1);
let overlay_pipe_fps = source_fps.divided_by(update_rate)?;
```

### 7. Derive render duration

Use explicit override if provided:

```rust
let render_duration = config.scene.composite_render_duration
    .unwrap_or(video_duration - trim_start);
```

If clamping to remaining activity duration is implemented, compute it explicitly:

```rust
render_duration = min(
    video_duration - trim_start,
    activity_duration - composite_sync_offset,
)
```

and store/pass it as `composite_render_duration`.

Do not implicitly reinterpret `composite_sync_offset`.

### 8. Create composite-specific config for dense report

Either mutate a local clone or build an internal composite render config.

Required timing:

```rust
config.scene.start = sync_offset;
config.scene.end = sync_offset + render_duration;
config.scene.fps = overlay_pipe_fps.as_f64_or_supported_representation();
```

If `scene.fps` is integer-only, use an internal config/timing representation rather than lossy rounding.

### 9. Build dense activity report with adjusted timing

Call:

```rust
let dense_activity = build_dense_activity_report(&parsed_activity, &config)?;
```

using the composite-adjusted timing.

The intended relationship is:

```txt
dense_activity.frame_count ≈ overlay_frame_count
dense frame i → overlay pipe frame i
```

### 10. Compute progress total

Compute final output frame count:

```rust
let output_frame_count = (render_duration * source_fps.as_f64()).ceil() as u32;
```

Use output-frame progress for user-facing progress.

Use overlay-frame count for internal render metrics.

### 11. Start render controller

Use a composite-specific status message:

```rust
let render_id = controller.try_start(output_frame_count, "Compositing video...")?;
```

Ensure this does not affect the transparent path.

### 12. Dispatch to composite render function

Call the new function that will be added in Phase 4:

```rust
render_composite_video(
    paths,
    &config,
    &parsed_activity,
    &dense_activity,
    controller,
    video_path,
    &bitrate,
    sync_offset,
    fps_num,
    fps_den,
    video_duration,
    Some(render_duration),
    Some(trim_start),
    Some(update_rate),
)
```

### 13. Preserve transparent path exactly

If no `composite_video_path` is present, preserve the previous sequence:

1. Build dense report using existing scene timing.
2. Start transparent render.
3. Use the existing transparent pipeline.

## Deliverables

- `backend_render()` branches before dense report generation.
- Composite mode required fields are validated.
- Composite timing is derived correctly.
- Dense report is built using composite-adjusted scene timing.
- User-facing progress total uses final output frame count.
- Transparent mode follows the previous path unchanged.

## Manual tests after Phase 3

### Test 3.1 — Transparent render branch unchanged

1. Run a normal transparent overlay export with no `composite_video_path`.
2. Confirm it still builds dense activity using the original config timing.
3. Confirm the existing transparent output is generated successfully.

Expected result:

```txt
Transparent export behavior is unchanged.
```

### Test 3.2 — Composite branch activates only when video path is present

1. Add `composite_video_path` to the config.
2. Run `backend_render()`.
3. Confirm the composite branch is selected.

Expected result:

```txt
Composite render path is selected when composite_video_path is present.
```

### Test 3.3 — Missing bitrate validation

1. Provide `composite_video_path` but omit `composite_bitrate`.
2. Run render.

Expected result:

```txt
Render fails early with a clear error: composite_bitrate required for composite render.
```

### Test 3.4 — Missing FPS validation

1. Provide `composite_video_path` but omit `composite_video_fps_num` or `composite_video_fps_den`.
2. Run render.

Expected result:

```txt
Render fails early with a clear FPS error.
```

### Test 3.5 — Dense report timing for sync offset

Inputs:

```txt
composite_sync_offset = 300
render_duration = 10
source_fps = 30000/1001
update_rate = 1
```

Expected composite scene timing:

```txt
scene.start = 300
scene.end ≈ 310
scene.fps = 30000/1001 or equivalent supported representation
```

Expected result:

```txt
Dense activity report starts at activity time 300s.
```

### Test 3.6 — Dense report timing for lower overlay update rate

Inputs:

```txt
source_fps = 60000/1001
update_rate = 2
render_duration = 10
```

Expected:

```txt
scene.fps = 30000/1001 or equivalent
```

Expected result:

```txt
Dense activity report frame count approximately matches 10 * 30000/1001.
```

### Test 3.7 — Render duration with trim

Inputs:

```txt
video_duration = 60
trim_start = 10
composite_render_duration = None
```

Expected:

```txt
render_duration = 50
```

### Test 3.8 — Reject impossible trim

Inputs:

```txt
video_duration = 60
trim_start = 60
```

Expected result:

```txt
Render fails early because trim_start leaves no usable video duration.
```

---

# Phase 4 — Composite Render Entry Point and Pipeline Shell

## Objective

Add the public composite render entry function and a new composite pipeline module without implementing the full render loop yet.

## Files involved

### New file

```txt
src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs
```

### Modified files

```txt
src-tauri/ovrley_core/src/encode/mod.rs
src-tauri/ovrley_core/src/encode/video.rs
```

## Implementation tasks

### 1. Register the new composite pipeline module

In:

```txt
src-tauri/ovrley_core/src/encode/mod.rs
```

add:

```rust
mod video_composite_pipeline;
```

### 2. Add new entry function in `video.rs`

Add:

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
    composite_video_fps_num: u32,
    composite_video_fps_den: u32,
    composite_video_duration: f64,
    composite_render_duration: Option<f64>,
    composite_video_trim_start: Option<f64>,
    composite_widget_update_rate: Option<u32>,
) -> Result<String, String>
```

This function should delegate to:

```rust
video_composite_pipeline::render_composite_video_single(...)
```

Do not modify `render_video()`.

### 3. Add `render_composite_video_single()` shell

In:

```txt
src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs
```

add:

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
    composite_video_fps_num: u32,
    composite_video_fps_den: u32,
    composite_video_duration: f64,
    composite_render_duration: Option<f64>,
    composite_video_trim_start: Option<f64>,
    composite_widget_update_rate: Option<u32>,
) -> Result<String, String>
```

### 4. Derive basic composite values inside the shell

Compute:

```txt
source_fps
output_fps = source_fps
overlay_pipe_fps = source_fps / composite_widget_update_rate
render_duration
overlay_frame_count
output_frame_count
```

Use:

```rust
let overlay_frame_count = (render_duration * overlay_pipe_fps.as_f64()).ceil() as u64;
let output_frame_count = (render_duration * source_fps.as_f64()).ceil() as u64;
```

### 5. Add guard against fractional-frame overrun in design

Even if the render loop is not implemented yet, define the loop condition that will be used:

```rust
let video_local_time = overlay_frame_index as f64 / overlay_pipe_fps.as_f64();
if video_local_time >= render_duration {
    break;
}
```

### 6. Build FFmpeg settings using Phase 2 builder

Call:

```rust
build_composite_ffmpeg_settings(...)
```

For now, this can be logged/debugged or returned in a dry-run mode if full spawning is not implemented yet.

### 7. Prepare output path strategy

Define where the output MP4 will be written.

Use the existing output naming conventions if available.

If not, temporarily use a clear composite output name such as:

```txt
<render_id>_composited.mp4
```

or:

```txt
output_composited.mp4
```

The final path must be returned as `Result<String, String>`.

### 8. Do not spawn FFmpeg yet unless all dependencies are ready

This phase may be a shell/dry-run phase.

It should compile and allow the composite branch to reach the new pipeline without touching the transparent path.

## Deliverables

- `video_composite_pipeline.rs` created.
- `render_composite_video()` added to `video.rs`.
- `render_video()` remains unchanged.
- Composite pipeline shell derives all timing values.
- Composite pipeline shell can build FFmpeg settings.
- Composite branch can dispatch into the new function.
- Code compiles.

## Manual tests after Phase 4

### Test 4.1 — Project compiles

Run:

```bash
cargo check
```

Expected result:

```txt
No compile errors.
```

### Test 4.2 — Transparent render still works

Run a known-good transparent render.

Expected result:

```txt
Output is generated exactly as before.
```

### Test 4.3 — Composite branch reaches shell

Run composite config with:

```txt
source_fps = 30000/1001
update_rate = 2
render_duration = 10
```

Expected derived values:

```txt
source_fps = 30000/1001
output_fps = 30000/1001
overlay_pipe_fps = 15000/1001
overlay_frame_count ≈ ceil(10 * 15000/1001)
output_frame_count ≈ ceil(10 * 30000/1001)
```

### Test 4.4 — FFmpeg settings are built in shell

Inspect logs or debug output.

Expected result:

```txt
Composite pipeline shell builds a two-input FFmpeg command configuration.
```

### Test 4.5 — No sacred files modified

Check git diff.

Expected result:

```txt
No changes to video_pipeline.rs, ffmpeg.rs, or video_debug.rs.
```

---

# Phase 5 — Functional Software Composite Render Loop

## Objective

Implement the first complete working MP4 composite pipeline using CPU overlay filtering and software H.264/H.265 encoding.

## Files involved

### Modified files

```txt
src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs
src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs
```

No sacred files may be modified.

## Implementation tasks

### 1. Reuse existing render preparation mechanisms

Prepare Skia assets using the same mechanisms as the transparent pipeline wherever possible.

This may include:

- loading fonts,
- preparing widget assets,
- preparing route/path data,
- preparing image assets,
- preparing reusable buffers,
- preparing renderer state,
- creating profiler/timing structures.

If existing helper functions are private to the transparent pipeline, do not modify the sacred files directly. Instead:

- reuse public helpers where available,
- duplicate minimal setup logic in the new composite file if necessary,
- or extract shared helpers only into new non-sacred files if this can be done without changing sacred files.

### 2. Spawn FFmpeg with two inputs

Use the settings from `build_composite_ffmpeg_settings()`.

The command must include:

```bash
-i <video_path>
-f rawvideo -pix_fmt rgba -s <width>x<height> -r <overlay_pipe_fps> -i pipe:0
-filter_complex "...overlay..."
-map "[out]" -map 0:a?
-r <source_video_fps>
-c:v <codec> -b:v <bitrate>
-c:a copy
-movflags faststart
-y <output_path>
```

### 3. Open FFmpeg stdin for raw RGBA frames

Ensure FFmpeg is spawned with piped stdin.

Rust must write raw RGBA bytes directly to FFmpeg stdin.

Each frame must be exactly:

```txt
width * height * 4 bytes
```

### 4. Render overlay frames, not output frames

Loop over overlay frame indices:

```rust
let mut overlay_frame_index: u64 = 0;

loop {
    if cancel_flag.load(Ordering::SeqCst) {
        break;
    }

    let video_local_time = overlay_frame_index as f64 / overlay_pipe_fps.as_f64();

    if video_local_time >= render_duration {
        break;
    }

    let activity_time = composite_sync_offset + video_local_time;

    let dense_frame_index = if dense_report_matches_composite_window {
        overlay_frame_index as usize
    } else {
        let idx = ((activity_time - config.scene.start) * config.scene.fps as f64).floor();
        if idx < 0.0 {
            return Err("Composite overlay frame is before dense activity range".to_string());
        }
        idx as usize
    };

    render_frame_rgba(..., dense_frame_index, ...)?;
    write_frame_to_ffmpeg_stdin(...)?;

    overlay_frame_index += 1;
}
```

### 5. Prefer direct dense-frame mapping

Because Phase 3 should rebuild dense activity specifically for composite mode, the preferred mapping is:

```txt
overlay frame j → dense frame j
```

This is valid when:

```txt
config.scene.start = composite_sync_offset
config.scene.end   = composite_sync_offset + render_duration
config.scene.fps   = overlay_pipe_fps
```

### 6. Add robust fallback dense-frame mapping

If the dense report does not match the composite window exactly, use:

```txt
dense_frame_index = floor((activity_time - config.scene.start) * config.scene.fps)
```

This formula must subtract `scene.start`.

Do not assume dense data starts at activity time zero.

### 7. Validate dense frame index

Before rendering:

- reject negative index,
- reject index beyond dense report length,
- or clamp only if that matches existing renderer semantics.

Preferred behavior for first implementation:

```txt
Reject out-of-range dense frame index with a clear error.
```

### 8. Guard against fractional-frame overrun

Do not render a frame when:

```txt
video_local_time >= render_duration
```

This prevents an extra tail frame when:

```txt
ceil(render_duration * overlay_pipe_fps)
```

creates a count that includes a timestamp outside the requested duration.

### 9. Let FFmpeg hold overlay frames

Do not duplicate cached RGBA frames to reach output FPS.

The optimized architecture is:

```txt
full-FPS source video
+
lower-FPS overlay stream
+
FFmpeg overlay filter holds overlay frames between updates
```

not:

```txt
full-FPS source video
+
full-FPS overlay stream with duplicated cached buffers
```

### 10. Implement simple writer behavior

For the first working implementation, it is acceptable to write frames directly to FFmpeg stdin from the render loop.

If existing architecture expects a writer thread and queue, implement:

- reusable frame buffer acquisition,
- render into buffer,
- queue frame,
- writer thread writes to FFmpeg stdin.

The preferred final architecture includes:

- render thread,
- bounded queue,
- writer thread,
- stderr monitor thread.

### 11. Wait for FFmpeg completion

After all overlay frames are written:

1. Close FFmpeg stdin.
2. Wait for FFmpeg process to exit.
3. Read/collect stderr.
4. If exit status is non-zero, return a clear error including relevant FFmpeg stderr.
5. If exit status is zero, return output path.

### 12. Support `libx264` first

Start with:

```txt
codec = libx264
```

Then add:

```txt
codec = libx265
```

Keep hardware encoders for later phases.

## Deliverables

- Functional software H.264 composite render path.
- Optional software H.265 composite render path.
- FFmpeg process spawned with two inputs.
- Rust writes raw RGBA overlay frames to FFmpeg stdin.
- Render loop iterates over overlay frames only.
- Dense frame mapping works for composite-adjusted dense reports.
- Fractional-frame overrun guard implemented.
- FFmpeg exit handling implemented.
- Output MP4 returned on success.

## Manual tests after Phase 5

### Test 5.1 — Basic software H.264 composite

Inputs:

```txt
codec = libx264
source video = short MP4
source_fps = source native FPS
update_rate = 1
bitrate = 20M or 60M
```

Expected result:

```txt
Output MP4 is created.
Overlay is visible.
Output plays in VLC.
Output plays in QuickTime if available.
```

### Test 5.2 — Verify output FPS for 29.97 source

Input:

```txt
source_fps = 30000/1001
```

Run:

```bash
ffprobe -v error -select_streams v:0 -show_entries stream=r_frame_rate,avg_frame_rate -of default=noprint_wrappers=1 output.mp4
```

Expected result:

```txt
r_frame_rate or avg_frame_rate reports 30000/1001.
No value is rounded to 30/1.
```

### Test 5.3 — Verify output FPS for 59.94 source

Input:

```txt
source_fps = 60000/1001
```

Expected ffprobe result:

```txt
60000/1001
```

not:

```txt
60/1
```

### Test 5.4 — Lower overlay update rate

Inputs:

```txt
source_fps = 60000/1001
update_rate = 2
overlay_pipe_fps = 30000/1001
```

Expected result:

```txt
Rust renders approximately half as many overlay frames as final output frames.
Final output FPS remains 60000/1001.
Overlay motion updates at the lower overlay rate.
```

### Test 5.5 — Aggressive overlay update rate

Inputs:

```txt
source_fps = 60000/1001
update_rate = 6
overlay_pipe_fps = 10000/1001
```

Expected result:

```txt
Rust renders approximately one sixth as many overlay frames as final output frames.
Final output FPS remains 60000/1001.
No duplicated raw RGBA frames are written by Rust.
```

### Test 5.6 — Sync offset semantics

Scenario:

```txt
Activity starts at 10:00.
Video starts at 10:05.
composite_sync_offset = 300 seconds.
```

Expected result:

```txt
Video starts from its first frame.
Overlay at video time 0 uses activity timestamp 300s.
FFmpeg does not seek 300 seconds into the video.
```

Inspect FFmpeg command.

Expected:

```txt
No -ss 300 argument exists.
```

### Test 5.7 — Fractional duration edge case

Use:

```txt
render_duration = a value that does not multiply cleanly by overlay_pipe_fps
```

Expected result:

```txt
The loop does not render any frame where video_local_time >= render_duration.
Output duration is correct.
No extra visual tail frame appears.
```

### Test 5.8 — Video without audio

Use a source MP4 with no audio track.

Expected result:

```txt
Render succeeds because -map 0:a? is optional.
```

### Test 5.9 — Video with audio

Use a source MP4 with an audio track.

Expected result:

```txt
Output MP4 contains the original audio track.
Audio is copied, not re-encoded.
```

### Test 5.10 — Invalid dense frame range

Force a config where dense report timing does not cover the requested composite sync offset.

Expected result:

```txt
Render fails with a clear dense-frame range error.
No panic occurs.
```

---

# Phase 6 — Cancellation, Cleanup, Progress, and Robust Process Handling

## Objective

Make the working composite render path production-safe with cancellation, progress reporting, queue behavior, and cleanup.

## Files involved

### Modified file

```txt
src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs
```

Possibly:

```txt
src-tauri/ovrley_core/src/commands/mod.rs
```

only if controller wiring needs refinement.

## Implementation tasks

### 1. Implement cancellation checks

The overlay render loop must check cancellation regularly:

```rust
if cancel_flag.load(Ordering::SeqCst) {
    break;
}
```

Cancellation should:

- stop rendering new frames,
- close FFmpeg stdin,
- terminate the FFmpeg process,
- clean up partial output if appropriate,
- return a cancellation result/error consistent with existing backend behavior.

### 2. Terminate FFmpeg on cancellation

When cancellation is requested:

1. Stop queueing new frames.
2. Drop/close stdin.
3. Attempt graceful FFmpeg termination if supported.
4. Kill process if it does not exit promptly.
5. Wait for process cleanup.

### 3. Remove orphan temporary outputs

If render is canceled or FFmpeg exits unsuccessfully, remove incomplete temporary output files unless existing project behavior keeps failed outputs for debugging.

If failed output is kept, name or place it clearly as failed/debug output.

### 4. Use output-frame progress for user-facing progress

Although the loop renders overlay frames, user-facing progress should map to final output frames.

Recommended:

```rust
let progress_output_frame =
    ((video_local_time * output_fps.as_f64()).round() as u32)
        .min(output_frame_count);

controller.set_frame_progress(progress_output_frame);
```

or:

```rust
progress_current = min(output_frame_count, overlay_frame_index * composite_widget_update_rate)
```

Prefer the time-based calculation when using rational FPS.

### 5. Track overlay frame count internally

Internal metrics should track:

```txt
overlay_frame_count_rendered
overlay_frame_count_written
output_frame_count
```

### 6. Add stderr monitor thread

Spawn a thread to continuously read FFmpeg stderr.

This prevents stderr pipe backpressure and allows useful diagnostics.

Collected stderr should be included in error messages when FFmpeg fails.

### 7. Add writer thread and bounded queue if needed

For better throughput and consistency with the transparent renderer, use:

- reusable frame buffers,
- a bounded queue,
- a writer thread,
- cancellation-aware send/receive logic.

Track timings for:

```txt
queue.put_wait
encoder.queue_wait
ffmpeg.write
buffer.acquire_wait
buffer.release_wait
```

### 8. Handle broken pipe clearly

If FFmpeg exits early, writing to stdin may produce a broken pipe.

Return a clear error that includes:

- FFmpeg exit status if available,
- relevant stderr,
- the fact that pipe writing failed because FFmpeg terminated.

### 9. Ensure output finalization

On success:

- close stdin,
- wait for FFmpeg,
- confirm output file exists,
- optionally confirm non-zero file size,
- return output path.

## Deliverables

- Cancellation support implemented.
- FFmpeg process termination implemented.
- Partial output cleanup behavior implemented.
- User-facing progress maps to final output frames.
- FFmpeg stderr monitor implemented.
- Broken-pipe handling implemented.
- Success path confirms output file creation.

## Manual tests after Phase 6

### Test 6.1 — Cancel mid-render

1. Start a composite render on a video long enough to cancel.
2. Cancel during rendering.

Expected result:

```txt
Rendering stops quickly.
FFmpeg process is terminated.
No orphan FFmpeg process remains.
Partial output is removed or clearly handled according to project policy.
UI/controller reports cancellation cleanly.
```

### Test 6.2 — Progress reaches completion

Run a short complete render.

Expected result:

```txt
Progress starts near 0.
Progress advances based on final output frames.
Progress reaches the expected total at completion.
```

### Test 6.3 — Progress with lower overlay FPS

Inputs:

```txt
source_fps = 60000/1001
update_rate = 6
```

Expected result:

```txt
Progress still reflects final output video duration/frames, not only the lower overlay frame count.
```

### Test 6.4 — FFmpeg failure reports useful error

Force an invalid codec or output path.

Expected result:

```txt
Render fails with a clear error containing useful FFmpeg stderr.
No panic occurs.
```

### Test 6.5 — Broken pipe scenario

Force FFmpeg to exit early, for example by using an invalid filter graph during a debug run.

Expected result:

```txt
Rust reports FFmpeg pipe/write failure clearly.
Collected stderr explains the underlying FFmpeg problem.
```

### Test 6.6 — Output file exists on success

After successful render, verify:

```txt
Output file exists.
Output file size is greater than zero.
Output opens in a media player.
```

---

# Phase 7 — Debug Timings and Diagnostics

## Objective

Write composite render timing summaries to a new debug phase directory without modifying the existing debug module.

## Files involved

### Modified file

```txt
src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs
```

### New file, optional

```txt
src-tauri/ovrley_core/src/encode/video_composite_debug.rs
```

Only add this if keeping debug helpers separate is cleaner.

Do not modify:

```txt
src-tauri/ovrley_core/src/encode/video_debug.rs
```

## Implementation tasks

### 1. Create composite debug output directory

Write timing summaries to:

```txt
target/debug_render/phase_7
```

### 2. Write timing summary JSON

Create:

```txt
target/debug_render/phase_7/timing_summary.json
```

The JSON should include:

```json
{
  "phase": "phase_7",
  "mode": "mp4_composite",
  "source_fps": "60000/1001",
  "overlay_pipe_fps": "30000/1001",
  "widget_update_rate": 2,
  "render_duration": 10.0,
  "overlay_frame_count": 300,
  "output_frame_count": 600,
  "total_ms": 12345.0
}
```

Exact values will vary.

### 3. Reuse existing timing buckets where practical

Track:

| Bucket                     | Description                                        |
| -------------------------- | -------------------------------------------------- |
| `frame.total`              | Total time per overlay frame render loop iteration |
| `buffer.acquire_wait`      | Time waiting for a reusable frame buffer           |
| `buffer.release_wait`      | Time returning a buffer to the pool                |
| `queue.put_wait`           | Time waiting to enqueue frame for writer           |
| `encoder.queue_wait`       | Writer waiting for queued frame                    |
| `ffmpeg.write`             | Time writing raw RGBA frame bytes to FFmpeg stdin  |
| `debug.sample_frame_write` | Optional debug sample-frame write timing           |

### 4. Add composite-specific buckets

Track:

| Bucket                          | Description                                   |
| ------------------------------- | --------------------------------------------- |
| `composite.overlay_frame_count` | Number of overlay frames rendered and written |
| `composite.output_frame_count`  | Number of final output video frames           |
| `composite.source_fps`          | Source/output FPS as rational string          |
| `composite.overlay_pipe_fps`    | Overlay pipe FPS as rational string           |
| `composite.widget_update_rate`  | Overlay update-rate divisor                   |
| `composite.total_ms`            | Full wall-clock render time                   |

### 5. Treat FFmpeg internal timings carefully

Optional estimated buckets:

| Bucket             | Meaning                                              |
| ------------------ | ---------------------------------------------------- |
| `ffmpeg.decode_ms` | Estimated decode time or parsed benchmark timing     |
| `ffmpeg.encode_ms` | Stdin write backpressure proxy, not pure encode time |
| `ffmpeg.filter_ms` | Estimated filter time, not directly isolated         |

Do not overstate these as exact unless FFmpeg provides reliable benchmark data.

### 6. Include command diagnostics

Include the final FFmpeg argument list in the debug output if safe.

Avoid logging sensitive file paths if project policy requires path redaction.

At minimum, log:

```txt
codec
bitrate
source_fps
overlay_pipe_fps
filter_complex
input resolution
render_duration
trim_start
sync_offset
```

## Deliverables

- `target/debug_render/phase_7/timing_summary.json` generated for composite renders.
- Existing timing buckets reused where practical.
- Composite-specific timing buckets added.
- FFmpeg timing estimates are labeled as estimates.
- Debug output does not require modifying `video_debug.rs`.

## Manual tests after Phase 7

### Test 7.1 — Timing summary exists

Run a composite render.

Expected file:

```txt
target/debug_render/phase_7/timing_summary.json
```

### Test 7.2 — Phase marker is correct

Inspect JSON.

Expected:

```json
"phase": "phase_7"
```

### Test 7.3 — FPS values are recorded as rationals

For a `60000/1001` source with update rate `2`, expected:

```json
"source_fps": "60000/1001"
"overlay_pipe_fps": "30000/1001"
"widget_update_rate": 2
```

### Test 7.4 — Frame counts are recorded

Expected fields:

```json
"overlay_frame_count": <number>
"output_frame_count": <number>
```

For update rate `2`, overlay frame count should be approximately half of output frame count.

### Test 7.5 — Total wall time is recorded

Expected field:

```json
"total_ms": <positive number>
```

### Test 7.6 — Existing transparent debug output unaffected

Run a transparent render.

Expected result:

```txt
Existing transparent debug behavior remains unchanged.
Composite phase_7 debug output is only created for composite renders.
```

---

# Phase 8 — Hardware Encoder Profiles and Safe Fallbacks

## Objective

Add hardware encoder support while keeping the robust CPU overlay path as the default fallback.

## Files involved

### Modified file

```txt
src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs
```

Possibly:

```txt
src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs
```

if profile selection is partly done there.

## Implementation tasks

### 1. Add profile selection strategy

Profiles should be selected based on:

1. user-selected codec,
2. detected hardware capabilities (this is done by codec_detect.rs so user should not be able to pick incompatible codec),
3. whether required FFmpeg filters are available,
4. safe fallback behavior.

### 2. Add recommended profiles

Implement profile records for:

| Profile         | Codec               | Decode              | Overlay Filter          | Notes                         |
| --------------- | ------------------- | ------------------- | ----------------------- | ----------------------------- |
| `software_h264` | `libx264`           | CPU                 | CPU `overlay`           | Universal fallback            |
| `software_h265` | `libx265`           | CPU                 | CPU `overlay`           | Universal fallback            |
| `nvgpu_h264`    | `h264_nvenc`        | `nvdec` or CPU      | CPU `overlay`           | Hardware encode, simpler path |
| `nvgpu_hevc`    | `hevc_nvenc`        | `nvdec` or CPU      | CPU `overlay`           | Hardware encode, simpler path |
| `mac_h264`      | `h264_videotoolbox` | VideoToolbox or CPU | CPU `overlay`           | macOS H.264                   |
| `mac_hevc`      | `hevc_videotoolbox` | VideoToolbox or CPU | CPU `overlay`           | macOS HEVC                    |
| `qsv_h264`      | `h264_qsv`          | QSV or CPU          | CPU `overlay` initially | Intel H.264                   |
| `qsv_hevc`      | `hevc_qsv`          | QSV or CPU          | CPU `overlay` initially | Intel HEVC                    |
| `vaapi_h264`    | `h264_vaapi`        | VAAPI or CPU        | CPU initially           | Linux only                    |
| `vaapi_hevc`    | `hevc_vaapi`        | VAAPI or CPU        | CPU initially           | Linux only                    |

### 3. Keep CPU overlay path as default

The first hardware implementation should generally be:

```txt
CPU decode/filter + hardware encode
```

or:

```txt
hardware decode + CPU filter + hardware encode
```

Do not require full-GPU overlay filters for hardware encoder support.

### 4. Add NVENC simple path

For NVIDIA simple hardware encode:

```txt
codec = h264_nvenc or hevc_nvenc
filter = CPU overlay
```

Possible decode choices:

- CPU decode for maximum compatibility.
- `-hwaccel nvdec` if available and stable.

If hardware decode creates pixel-format issues with CPU overlay, fall back to CPU decode.

### 5. Add VideoToolbox simple path

For macOS:

```txt
codec = h264_videotoolbox or hevc_videotoolbox
filter = CPU overlay
```

Use bitrate control:

```bash
-b:v <bitrate>
```

### 6. Add QSV simple path

For Intel:

```txt
codec = h264_qsv or hevc_qsv
filter = CPU overlay initially
```

If QSV encode requires specific pixel format, add the required final format conversion.

### 7. Add VAAPI placeholder or Linux path

For Linux VAAPI:

```txt
codec = h264_vaapi or hevc_vaapi
```

VAAPI may require:

```txt
format=nv12,hwupload
```

Because this can be platform-specific, keep fallback clear.

### 8. Enforce bitrate override

For every profile, user bitrate must be passed as:

```bash
-b:v <bitrate>
```

Profile defaults must not override the user's selected bitrate.

### 9.Capability detection or capability input

The working codecs are already detected by codec_detect.rs and stored in zustand store. The user will then select desired codec/acceleration combo which must be passed into the backed.

### 10. Add fallback messages

If a hardware profile is requested but unavailable, either:

- fail clearly if user explicitly selected it, or
- fall back to software if automatic mode selected it.

Messages should explain:

```txt
Requested hardware encoder/filter is unavailable; falling back to software overlay + available encoder.
```

or:

```txt
Requested hardware encoder is unavailable.
```

## Deliverables

- Hardware profile selection implemented.
- Software profiles remain universal fallback.
- NVENC simple paths implemented.
- VideoToolbox simple paths implemented.
- QSV simple paths implemented if available.
- VAAPI support added or clearly deferred behind profile availability.
- User bitrate respected across all profiles.
- Fallback behavior implemented and logged.

## Manual tests after Phase 8

### Test 8.1 — Software H.264 fallback

Select:

```txt
codec = libx264
```

Expected result:

```txt
Render succeeds using CPU overlay and libx264.
```

### Test 8.2 — Software H.265 fallback

Select:

```txt
codec = libx265
```

Expected result:

```txt
Render succeeds using CPU overlay and libx265.
```

### Test 8.3 — NVENC simple path

On NVIDIA hardware with NVENC-capable FFmpeg, select:

```txt
codec = h264_nvenc
```

Expected result:

```txt
Render succeeds.
Overlay is visible.
Output FPS is preserved.
CPU overlay filter is used unless full CUDA path is explicitly selected later.
```

### Test 8.4 — NVENC HEVC simple path

On NVIDIA hardware, select:

```txt
codec = hevc_nvenc
```

Expected result:

```txt
Render succeeds or fails with a clear hardware/codec availability error.
```

### Test 8.5 — VideoToolbox H.264

On macOS, select:

```txt
codec = h264_videotoolbox
```

Expected result:

```txt
Render succeeds.
Output MP4 plays in QuickTime.
```

### Test 8.6 — VideoToolbox HEVC

On macOS, select:

```txt
codec = hevc_videotoolbox
```

Expected result:

```txt
Render succeeds or reports a clear VideoToolbox HEVC support error.
```

### Test 8.7 — QSV H.264

On Intel hardware with QSV-enabled FFmpeg, select:

```txt
codec = h264_qsv
```

Expected result:

```txt
Render succeeds using the safe CPU overlay path, or fails with a clear QSV availability error.
```

### Test 8.8 — Hardware unavailable fallback

Request a hardware codec on a machine where it is unavailable.

Expected result for automatic mode:

```txt
Renderer falls back to a safe available profile and logs the fallback.
```

Expected result for explicit hardware-only mode:

```txt
Renderer fails clearly and explains the requested hardware path is unavailable.
```

### Test 8.9 — Bitrate override

Render with:

```txt
bitrate = 10M
```

Then render with:

```txt
bitrate = 60M
```

Inspect FFmpeg command.

Expected result:

```txt
-b:v 10M and -b:v 60M are passed respectively for every profile.
```

---

# Phase 9 — Optional Full-GPU Filter Paths

## Objective

Add experimental full-GPU overlay paths for CUDA and QSV where available, while keeping safe software overlay fallback mandatory.

## Files involved

### Modified file

```txt
src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs
```

Possibly:

```txt
src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs
```

## Implementation tasks

### 1. Add CUDA full-GPU profiles

Add:

| Profile       | Codec        | Decode | Overlay Filter |
| ------------- | ------------ | ------ | -------------- |
| `nnvgpu_h264` | `h264_nvenc` | CUDA   | `overlay_cuda` |
| `nnvgpu_hevc` | `hevc_nvenc` | CUDA   | `overlay_cuda` |

### 2. Check CUDA filter availability

Before selecting CUDA path, confirm FFmpeg supports:

```txt
overlay_cuda
scale_cuda
hwupload
```

If unavailable, do not select the profile.

### 3. Use CUDA filter graph as a starting point

Candidate graph:

```txt
[0:v]setpts=PTS-STARTPTS,scale_cuda=format=yuv420p[base];[1:v]setpts=PTS-STARTPTS,format=yuva420p,hwupload[ovr];[base][ovr]overlay_cuda=0:0:eof_action=repeat:shortest=1[out]
```

This may require adjustment depending on FFmpeg build support and alpha-format compatibility.

### 4. Fall back if CUDA graph fails

If `overlay_cuda` does not accept the exact options or alpha format, the implementation must fall back to:

```txt
software overlay + NVENC encode
```

Do not let CUDA fragility block hardware encoding entirely.

### 5. Add QSV full-GPU profiles

Add optional QSV overlay path:

```txt
[0:v]setpts=PTS-STARTPTS,hwupload=extra_hw_frames=64[main_hw];[1:v]setpts=PTS-STARTPTS,format=bgra,hwupload=extra_hw_frames=64[overlay_hw];[main_hw][overlay_hw]overlay_qsv=x=0:y=0,hwdownload,format=nv12[out]
```

This may be hardware, driver, and FFmpeg-build dependent.

### 6. Check QSV filter availability

Before selecting QSV full-GPU path, confirm FFmpeg supports:

```txt
overlay_qsv
hwupload
hwdownload
```

### 7. Keep experimental profiles opt-in

Full-GPU paths should not become the default until tested widely.

Recommended UX/profile behavior:

```txt
nvgpu_h264      = safe NVIDIA hardware encode with CPU overlay
nnvgpu_h264     = experimental full CUDA path
qsv_h264        = safe Intel hardware encode with CPU overlay
qsv_full_h264   = experimental QSV overlay path
```

### 8. Capture detailed diagnostics on failure

If full-GPU path fails, log:

- selected profile,
- full filter graph,
- FFmpeg stderr,
- fallback profile used, if any.

## Deliverables

- Optional CUDA full-GPU profiles added.
- Optional QSV full-GPU profiles added.
- Filter availability checks added.
- Full-GPU paths are opt-in or carefully gated.
- Software overlay fallback remains mandatory.
- Detailed failure diagnostics implemented.

## Manual tests after Phase 9

### Test 9.1 — CUDA filter availability check

On an NVIDIA machine, run FFmpeg filter detection.

Expected result:

```txt
Renderer only enables full CUDA profile if overlay_cuda and required CUDA filters exist.
```

### Test 9.2 — Full CUDA H.264 render

Select:

```txt
profile = nnvgpu_h264
codec = h264_nvenc
```

Expected result:

```txt
If supported, render succeeds and overlay is visible.
If unsupported, renderer falls back to software overlay + NVENC or reports a clear error according to selected fallback policy.
```

### Test 9.3 — Full CUDA HEVC render

Select:

```txt
profile = nnvgpu_hevc
codec = hevc_nvenc
```

Expected result:

```txt
Success or clear fallback/error behavior.
```

### Test 9.4 — CUDA alpha correctness

Use overlay widgets with transparent regions, semi-transparent shadows, and solid text.

Expected result:

```txt
Alpha compositing looks correct.
No black boxes around transparent overlay areas.
No incorrect premultiplication artifacts.
```

### Test 9.5 — QSV filter availability check

On Intel hardware, check for:

```txt
overlay_qsv
```

Expected result:

```txt
Full QSV overlay profile is only enabled when required filters are available.
```

### Test 9.6 — QSV overlay render

Select experimental QSV overlay profile.

Expected result:

```txt
Render succeeds or falls back cleanly to CPU overlay + QSV encode.
```

### Test 9.7 — Full-GPU fallback path

Force CUDA or QSV filter failure.

Expected result:

```txt
Fallback path is available.
Diagnostic output explains why full-GPU path failed.
```

---

# Phase 10 — Full End-to-End Validation and Regression Suite

## Objective

Validate the entire MP4 compositing pipeline across frame rates, update rates, sync offsets, trim behavior, audio, cancellation, debug output, and hardware profiles.

## Files involved

No required code changes unless tests reveal bugs.

## Manual test matrix

### Test 10.1 — 29.97 fps source video

Input:

```txt
source_video_fps = 30000/1001
```

Expected result:

```txt
Output remains 30000/1001.
Output is not rounded to 30.
```

Verify with:

```bash
ffprobe -v error -select_streams v:0 -show_entries stream=r_frame_rate,avg_frame_rate -of default=noprint_wrappers=1 output.mp4
```

### Test 10.2 — 59.94 fps source video

Input:

```txt
source_video_fps = 60000/1001
```

Expected result:

```txt
Output remains 60000/1001.
Output is not rounded to 60.
```

### Test 10.3 — 23.976 fps source video

Input:

```txt
source_video_fps = 24000/1001
```

Expected result:

```txt
Output remains 24000/1001.
Output is not rounded to 24.
```

### Test 10.4 — Lower overlay update rate

Inputs:

```txt
source_fps = 60000/1001
composite_widget_update_rate = 2
```

Expected result:

```txt
overlay_pipe_fps = 30000/1001
Rust renders/writes approximately half as many overlay frames as final output frames.
Dense activity report is rebuilt at 30000/1001 or equivalent supported representation.
Final output remains 60000/1001.
```

### Test 10.5 — Aggressive overlay update rate

Inputs:

```txt
source_fps = 60000/1001
composite_widget_update_rate = 6
```

Expected result:

```txt
overlay_pipe_fps = 10000/1001
Final output remains 60000/1001.
Dense activity report is rebuilt at 10000/1001 or equivalent supported representation.
```

### Test 10.6 — Sync offset

Scenario:

```txt
Activity starts at 10:00.
Video starts at 10:05.
composite_sync_offset = 300.
```

Expected result:

```txt
Video begins from its first frame.
Overlay renders activity timestamp 300s at video time 0s.
FFmpeg does not seek 300 seconds into the video.
Dense report uses scene.start = 300.
```

### Test 10.7 — Optional video trim

Inputs:

```txt
composite_video_trim_start = 10
composite_sync_offset = 300
```

Expected result:

```txt
FFmpeg seeks 10 seconds into the source video.
Overlay activity time at output start is composite_sync_offset + 0 unless UI explicitly defines different behavior.
```

Command expectation:

```txt
-ss 10 is present.
-ss 300 is not present.
```

### Test 10.8 — Audio preservation

Use a source video with audio.

Expected result:

```txt
Output MP4 contains the original audio track.
```

Verify with:

```bash
ffprobe -v error -select_streams a -show_entries stream=codec_name -of default=noprint_wrappers=1 output.mp4
```

### Test 10.9 — No-audio input

Use a source video without audio.

Expected result:

```txt
Output render succeeds because -map 0:a? is optional.
```

### Test 10.10 — Software H.264

Select:

```txt
codec = libx264
```

Expected result:

```txt
Composited overlay is visible.
Output plays in VLC.
Output plays in QuickTime.
```

### Test 10.11 — Software H.265

Select:

```txt
codec = libx265
```

Expected result:

```txt
Composited overlay is visible.
Output plays in VLC.
QuickTime compatibility depends on platform/codec support.
```

### Test 10.12 — NVENC simple path

Select:

```txt
codec = h264_nvenc or hevc_nvenc
```

Expected result:

```txt
CPU overlay filter path succeeds with hardware encode.
Output FPS and audio behavior remain correct.
```

### Test 10.13 — Full CUDA path

Select experimental full CUDA path only if CUDA FFmpeg filters are available.

Expected result:

```txt
If overlay_cuda works, render succeeds.
If overlay_cuda fails, fallback path is available and documented.
```

### Test 10.14 — QSV path

Test on Intel hardware.

Expected result:

```txt
QSV encode works with CPU overlay, or failure is reported clearly.
If overlay_qsv is tested and fails, fallback path is available.
```

### Test 10.15 — Cancel mid-render

Start a long render and cancel.

Expected result:

```txt
FFmpeg process is terminated.
No orphan temporary output remains unless intentionally preserved for debugging.
No orphan FFmpeg process remains.
Controller reports cancellation cleanly.
```

### Test 10.16 — No video imported

Run backend render with no `composite_video_path`.

Expected result:

```txt
Existing transparent pipeline is used unchanged.
```

### Test 10.17 — Debug timings

Run a composite render.

Expected result:

```txt
target/debug_render/phase_7/timing_summary.json exists.
"phase" is "phase_7".
FPS values, overlay frame count, output frame count, widget update rate, and total wall time are recorded.
```

### Test 10.18 — Fractional overlay duration edge case

Use a render duration that does not multiply cleanly by overlay pipe FPS.

Expected result:

```txt
The loop does not render a frame with video_local_time >= render_duration.
FFmpeg output duration is correct.
No extra visual tail frame appears.
```

### Test 10.19 — Thread queue robustness

Inputs:

```txt
source_fps = 60000/1001
update_rate = 6 or 10
```

Expected result:

```txt
FFmpeg does not emit avoidable thread queue starvation warnings for the pipe input.
-thread_queue_size 512 is present before the rawvideo pipe input.
```

### Test 10.20 — Resolution mismatch behavior

Use source video resolution that does not match overlay resolution.

Expected result if scaling is enabled:

```txt
Source video is scaled to overlay resolution.
Output dimensions match overlay dimensions.
```

Expected result if mismatch rejection is chosen later:

```txt
Render fails early with a clear resolution mismatch error.
```

For this plan, the default software filter scales source video to overlay resolution.

---

# Final File Change Summary

## New files

| File                                                           | Purpose                                                             |
| -------------------------------------------------------------- | ------------------------------------------------------------------- |
| `src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs`         | Composite FFmpeg argument/profile builder                           |
| `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs` | Composite render pipeline                                           |
| `src-tauri/ovrley_core/src/encode/video_composite_debug.rs`    | Optional composite-only debug helpers                               |
| `src-tauri/ovrley_core/src/encode/fps.rs`                      | Optional rational FPS helper if not placed in `ffmpeg_composite.rs` |

## Modified files

| File                                        | Change                                                                                                                    |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `src-tauri/ovrley_core/src/encode/mod.rs`   | Register new composite modules                                                                                            |
| `src-tauri/ovrley_core/src/encode/video.rs` | Add new `render_composite_video()` entry function; do not modify `render_video()`                                         |
| `src-tauri/ovrley_core/src/config/mod.rs`   | Add render-time-only `composite_*` fields                                                                                 |
| `src-tauri/ovrley_core/src/commands/mod.rs` | Branch to composite pipeline when `composite_video_path` is present and build dense report with adjusted composite timing |

## Untouched sacred files

| File                                                 | Status               |
| ---------------------------------------------------- | -------------------- |
| `src-tauri/ovrley_core/src/encode/video_pipeline.rs` | Must not be modified |
| `src-tauri/ovrley_core/src/encode/ffmpeg.rs`         | Must not be modified |
| `src-tauri/ovrley_core/src/encode/video_debug.rs`    | Must not be modified |

---

# Key Implementation Warnings

## Never round source FPS

Do not do this:

```rust
let output_fps = video_fps.ceil() as u32;
```

This is incorrect for composite mode.

Use rational values such as:

```txt
24000/1001
30000/1001
60000/1001
25/1
30/1
60/1
```

## Never use sync offset as FFmpeg seek

Do not do this:

```bash
-ss <composite_sync_offset>
```

Use only:

```txt
composite_video_trim_start
```

for FFmpeg seek/trim behavior.

## Rebuild dense activity for composite mode

Composite mode should build the dense activity report using:

```txt
scene.start = composite_sync_offset
scene.end   = composite_sync_offset + render_duration
scene.fps   = source_video_fps / composite_widget_update_rate
```

The intended relationship is:

```txt
one dense/interpolated data frame
=
one Skia-rendered overlay frame
=
one raw RGBA frame written to FFmpeg pipe:0
```

## Account for `scene.start` in fallback frame mapping

If dense data is not rebuilt exactly for the composite window, map time to dense frame index with:

```txt
dense_frame_index = floor((activity_time - scene.start) * scene.fps)
```

Do not assume dense activity starts at zero.

## Guard against fractional frame overrun

Do not blindly trust:

```rust
ceil(render_duration * overlay_pipe_fps)
```

without checking timestamps.

The render loop should stop when:

```rust
video_local_time >= render_duration
```

## Use lower-FPS overlay pipe correctly

The optimized architecture is:

```txt
full-FPS source video
+
lower-FPS overlay stream
+
FFmpeg overlay filter holds overlay frames between updates
```

The renderer should not write duplicate cached RGBA frames just to match output FPS.

## Add thread queue size for overlay pipe

Always place this before the rawvideo pipe input:

```bash
-thread_queue_size 512
```

This is especially useful when overlay FPS is much lower than source video FPS.

## Keep transparent pipeline untouched

Composite mode is a new backend path.

Existing transparent export behavior must remain unchanged.
