# Phase 5 — MP4 Compositing FFmpeg Pipeline (Backend) — Refined

**Goal**: Build a parallel backend encoding pipeline that composites Skia-rendered transparent overlay frames on top of an imported MP4/MOV/MKV video using FFmpeg `filter_complex`.

The existing transparent overlay export pipeline must remain untouched.

> **CRITICAL**: All new pipeline logic must live in separate files. The existing `video_pipeline.rs`, `ffmpeg.rs`, and `video_debug.rs` files are sacred and must not be modified.

---

## 1. Core Architecture

The compositing pipeline uses FFmpeg with two inputs:

| Input   | Source              | Description                                                               |
| ------- | ------------------- | ------------------------------------------------------------------------- |
| Input 0 | Imported video file | MP4/MOV/MKV decoded by FFmpeg, optionally with hardware acceleration      |
| Input 1 | Raw RGBA pipe       | Transparent overlay frames rendered by Skia and streamed through `pipe:0` |

FFmpeg composites these two streams using `filter_complex`, then encodes the final video to MP4 using the selected H.264/H.265 codec.

Conceptually:

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

---

## 2. Framerate Strategy

### 2.1 Source Video FPS Is the Master Clock

In MP4 compositing mode, the final output FPS must always equal the imported video's native FPS.

Do **not** round `29.97` to `30`, `59.94` to `60`, or `23.976` to `24`.

Instead, preserve the source video FPS as accurately as possible, preferably as a rational value from ffprobe:

| Common Display FPS | Rational FPS |
| ------------------ | ------------ |
| 23.976             | `24000/1001` |
| 29.97              | `30000/1001` |
| 59.94              | `60000/1001` |
| 25                 | `25/1`       |
| 30                 | `30/1`       |
| 60                 | `60/1`       |

Definitions:

```txt
source_video_fps = native FPS of imported video, preferably rational
output_fps       = source_video_fps
```

The final composited MP4 should therefore have the same FPS as the imported video.

### 2.2 Overlay Stream FPS Is Derived from Widget Update Rate

The overlay stream does not need to run at full source-video FPS.

Instead of writing one raw RGBA overlay frame for every output video frame and reusing cached buffers, the composite pipeline should reduce the overlay pipe FPS directly.

```txt
overlay_pipe_fps = source_video_fps / composite_widget_update_rate
```

Examples:

| Source Video FPS | `composite_widget_update_rate` | Overlay Pipe FPS | Final Output FPS |
| ---------------- | -----------------------------: | ---------------- | ---------------- |
| `60000/1001`     |                              1 | `60000/1001`     | `60000/1001`     |
| `60000/1001`     |                              2 | `30000/1001`     | `60000/1001`     |
| `60000/1001`     |                              3 | `20000/1001`     | `60000/1001`     |
| `30000/1001`     |                              2 | `15000/1001`     | `30000/1001`     |
| `25/1`           |                              5 | `5/1`            | `25/1`           |

This means:

```txt
source video frame 0  @ 0.000s  + overlay frame 0
source video frame 1  @ 0.017s  + overlay frame 0
source video frame 2  @ 0.033s  + overlay frame 1
source video frame 3  @ 0.050s  + overlay frame 1
...
```

for a `59.94 fps` source with `composite_widget_update_rate = 2`.

### 2.3 Why This Strategy Is Preferred

This strategy saves both expensive Skia rendering work and raw RGBA pipe bandwidth.

A full-resolution 4K RGBA frame is approximately:

```txt
3840 × 2160 × 4 = ~33 MB/frame
```

At `59.94 fps`, writing one overlay frame per video frame would require roughly:

```txt
~2 GB/s raw pipe bandwidth
```

Reducing the overlay pipe FPS directly avoids writing duplicate cached RGBA frames. FFmpeg holds each overlay frame until the next overlay frame timestamp is reached.

### 2.4 Role of `config.scene.fps`

In transparent overlay export mode, `config.scene.fps` remains the overlay/container FPS.

In MP4 compositing mode, the imported source video is the master clock. The final output FPS is always the source video FPS.

`config.scene.fps` must not override the source video FPS.

However, `config.scene.fps` is still important in composite mode: it represents the overlay data interpolation FPS used to build the dense activity report for the composite render.

In composite mode, `config.scene.fps` should be locked to the derived overlay pipe FPS:

```txt
config.scene.fps = overlay_pipe_fps
config.scene.fps = source_video_fps / composite_widget_update_rate
```

This means the dense activity report should be rebuilt/rerun for the composite render using the derived overlay FPS. With this setup, the mapping becomes direct:

```txt
overlay frame j → dense frame index j → activity time composite_sync_offset + j / overlay_pipe_fps
```

The goal is:

```text
one dense/interpolated data frame
=
one Skia-rendered overlay frame
=
one raw RGBA frame written to FFmpeg pipe:0
```

This avoids generating unused dense/interpolated data and avoids awkward sampling from a dense report built at a different FPS.

Recommended composite-specific control:

```txt
composite_widget_update_rate
```

This is a positive integer decimation factor relative to source-video FPS.

| Field                          | Transparent Pipeline                  | Composite Pipeline                                                                     |
| ------------------------------ | ------------------------------------- | -------------------------------------------------------------------------------------- |
| `config.scene.fps`             | Output/container FPS                  | Overlay interpolation FPS, locked to `source_video_fps / composite_widget_update_rate` |
| `config.update_rate`           | Existing transparent-frame decimation | Not used for MP4 compositing                                                           |
| `config.container_fps()`       | Transparent output FPS                | Not used for MP4 compositing                                                           |
| `composite_video_fps`          | Not used                              | Source video FPS, exact/rational if possible                                           |
| `composite_widget_update_rate` | Not used                              | Divides source FPS to get overlay pipe FPS and `config.scene.fps`                      |

### 2.5 Composite Dense Report Rebuild

The composite backend should build the dense activity report using composite-specific scene timing before rendering.

For composite mode, the effective scene timing should be:

```txt
config.scene.start = composite_sync_offset
config.scene.end   = composite_sync_offset + render_duration
config.scene.fps   = source_video_fps / composite_widget_update_rate
```

This ensures the dense report contains exactly the overlay frames that the composite pipe will send to FFmpeg.

If the frontend already injects these adjusted values before calling `backend_render()`, the backend should validate them.

If the frontend does not inject them, the backend composite branch should derive an internal composite render config before calling `build_dense_activity_report()`.

The important relationship is:

```txt
dense_activity.frame_count ≈ overlay_frame_count
```

and:

```txt
dense frame i → overlay pipe frame i
```

This is cleaner than building a dense report at 24/30/60 fps and then sampling it at a different fractional overlay pipe FPS such as `30000/1001`.

---

## 3. Sync and Timeline Semantics

### 3.1 Sync Offset Meaning

`composite_sync_offset` means:

```txt
The activity timestamp at which video time 0 begins.
```

Example:

```txt
Activity starts at 10:00
Video starts at 10:05
composite_sync_offset = 300 seconds
```

This means:

```txt
video time 0.0s aligns with activity time 300.0s
```

It does **not** mean:

```txt
seek 300 seconds into the video file
```

Therefore, `composite_sync_offset` must normally affect the overlay timestamp, not FFmpeg's `-ss` seek value.

### 3.2 Video Local Time and Activity Time

For overlay frame `j`:

```txt
video_local_time = j / overlay_pipe_fps
activity_time    = composite_sync_offset + video_local_time
```

Skia renders the overlay for `activity_time`.

FFmpeg composites that overlay frame over the video at `video_local_time`.

If the dense report was rebuilt specifically for composite mode using:

```txt
config.scene.start = composite_sync_offset
config.scene.end   = composite_sync_offset + render_duration
config.scene.fps   = overlay_pipe_fps
```

then overlay frame `j` maps directly to dense frame `j`.

In that case, no manual time-to-frame-index conversion is needed in the composite render loop:

```txt
overlay frame j → dense report frame j → render_frame_rgba(frame_index = j, ...)
```

If the dense report was not rebuilt with these exact composite values, conversion must explicitly account for `config.scene.start`:

```txt
dense_relative_time = activity_time - config.scene.start
dense_frame_index   = floor(dense_relative_time * config.scene.fps)
```

So the robust formula is:

```txt
dense_frame_index = floor((activity_time - config.scene.start) * config.scene.fps)
```

When `config.scene.start = composite_sync_offset`, this collapses to:

```txt
dense_frame_index = floor(video_local_time * config.scene.fps)
```

and when `config.scene.fps = overlay_pipe_fps`, this further collapses to:

```txt
dense_frame_index = overlay_frame_index
```

### 3.3 Optional Video Trim Start

If the app later supports trimming/seeking within the imported video file, this should be represented as a separate field:

```txt
composite_video_trim_start
```

Only this value should be passed to FFmpeg `-ss`.

Default:

```txt
composite_video_trim_start = 0.0
```

Do not overload `composite_sync_offset` as a video seek value.

### 3.4 Render Duration

For the first implementation, render duration should normally be the usable video duration after trim:

```txt
render_duration = composite_video_duration - composite_video_trim_start
```

Optionally clamp to remaining activity duration:

```txt
render_duration = min(
  composite_video_duration - composite_video_trim_start,
  activity_duration - composite_sync_offset
)
```

If this clamp is used, it should be computed explicitly and passed as:

```txt
composite_render_duration
```

Avoid implicit reinterpretation of `composite_sync_offset`.

---

## 4. FFmpeg Command Structure

### 4.1 Input 0: Imported Video

Input 0 is the source video file.

For the default no-trim case:

```bash
-t <render_duration> -i <video_path>
```

If video trim is supported:

```bash
-ss <composite_video_trim_start> -t <render_duration> -i <video_path>
```

Hardware decode arguments may be inserted before the input depending on profile:

```bash
-hwaccel nvdec
-hwaccel cuda -hwaccel_output_format cuda
-hwaccel videotoolbox
-init_hw_device qsv=hw -hwaccel qsv -hwaccel_output_format qsv
```

### 4.2 Input 1: Raw RGBA Overlay Stream

Input 1 is a raw RGBA overlay stream written by Rust/Skia to stdin.

```bash
-thread_queue_size 512 -f rawvideo -pix_fmt rgba -s <width>x<height> -r <overlay_pipe_fps> -i pipe:0
```

Important:

- The raw RGBA stream has no timestamps embedded.
- FFmpeg assigns timestamps based on frame order and `-r <overlay_pipe_fps>`.
- Therefore, `overlay_pipe_fps` must be precise and should be passed as a rational string when possible.
- `-thread_queue_size 512` is a cheap robustness measure that helps avoid demuxer starvation warnings or queue pressure on some FFmpeg builds, especially when the overlay pipe FPS is much lower than the source video FPS.

Examples:

```bash
-r 30000/1001
-r 15000/1001
-r 25/1
-r 5/1
```

### 4.3 Software Filter Graph

Recommended default software filter:

```txt
[0:v]setpts=PTS-STARTPTS,scale=WxH[base];
[1:v]setpts=PTS-STARTPTS[ovr];
[base][ovr]overlay=0:0:eof_action=repeat:shortest=1,format=yuv420p[out]
```

Single-line form:

```txt
[0:v]setpts=PTS-STARTPTS,scale=WxH[base];[1:v]setpts=PTS-STARTPTS[ovr];[base][ovr]overlay=0:0:eof_action=repeat:shortest=1,format=yuv420p[out]
```

Notes:

- `setpts=PTS-STARTPTS` normalizes both timelines to start at zero.
- `scale=WxH` ensures the base video matches overlay resolution.
- If resolution mismatch should be disallowed, this scale can be omitted and validation should reject mismatches before render.
- `eof_action=repeat` tells FFmpeg to keep using the last overlay frame if the overlay stream ends slightly early.
- `shortest=1` ends the overlay operation when the shortest stream ends.
- `format=yuv420p` gives broad MP4 player compatibility for H.264/H.265 output.
- The output is explicitly labeled `[out]`.
- `format=rgba` on Input 1 is redundant in the software path because the raw input already declares `-pix_fmt rgba`. It is harmless but unnecessary. Explicit format conversion remains relevant in hardware paths such as CUDA/QSV where `format=yuva420p`, `format=bgra`, `hwupload`, or other conversions may be required.

### 4.4 Output Mapping

When using `filter_complex`, explicitly map the filtered video and optional audio:

```bash
-map "[out]" -map 0:a?
```

The optional `?` prevents failure when the input video has no audio track.

Output arguments:

```bash
-c:v <codec> -b:v <bitrate> [codec_specific_args...] -c:a copy -movflags faststart -y <output_path>
```

The output FPS should be preserved as the source video FPS:

```bash
-r <source_video_fps>
```

Place this after the filter graph and before the output path.

### 4.5 Complete Software Command Template

```bash
ffmpeg -loglevel info \
  [hw_init_args...] \
  [input_0_hwaccel_args...] \
  [-ss <composite_video_trim_start>] \
  -t <render_duration> \
  -i <video_path> \
  -thread_queue_size 512 \
  -f rawvideo -pix_fmt rgba -s <width>x<height> -r <overlay_pipe_fps> -i pipe:0 \
  -filter_complex "[0:v]setpts=PTS-STARTPTS,scale=<width>:<height>[base];[1:v]setpts=PTS-STARTPTS[ovr];[base][ovr]overlay=0:0:eof_action=repeat:shortest=1,format=yuv420p[out]" \
  -map "[out]" -map 0:a? \
  -r <source_video_fps> \
  -c:v <codec> -b:v <bitrate> [codec_args...] \
  -c:a copy \
  -movflags faststart \
  -y <output_path>
```

---

## 5. Hardware Profile Strategy

Hardware profiles should be defined in `ffmpeg_composite.rs` and selected based on:

1. User-selected codec.
2. Detected hardware capabilities.
3. Whether the required FFmpeg filters are available.
4. Safe fallback to software compositing and hardware encoding where appropriate.

### 5.1 Profile Struct

```rust
pub struct CompositeProfile {
    pub name: &'static str,
    pub input_args: Vec<String>,
    pub filter_complex: Option<String>,
    pub output_args: Vec<String>,
}
```

### 5.2 Settings Struct

```rust
pub struct CompositeFfmpegSettings {
    pub hw_init_args: Vec<String>,
    pub input_0_args: Vec<String>,
    pub input_1_args: Vec<String>,
    pub filter_complex: String,
    pub output_args: Vec<String>,
}
```

### 5.3 Recommended Profiles

| Profile         | Codec               | Decode         | Overlay Filter                                   | Notes                         |
| --------------- | ------------------- | -------------- | ------------------------------------------------ | ----------------------------- |
| `software_h264` | `libx264`           | CPU            | CPU `overlay`                                    | Universal fallback            |
| `software_h265` | `libx265`           | CPU            | CPU `overlay`                                    | Universal fallback            |
| `nvgpu_h264`    | `h264_nvenc`        | `nvdec` or CPU | CPU `overlay`                                    | Hardware encode, simpler path |
| `nvgpu_hevc`    | `hevc_nvenc`        | `nvdec` or CPU | CPU `overlay`                                    | Hardware encode, simpler path |
| `nnvgpu_h264`   | `h264_nvenc`        | CUDA           | `overlay_cuda`                                   | Full GPU path if available    |
| `nnvgpu_hevc`   | `hevc_nvenc`        | CUDA           | `overlay_cuda`                                   | Full GPU path if available    |
| `mac_h264`      | `h264_videotoolbox` | VideoToolbox   | CPU `overlay`                                    | macOS H.264                   |
| `mac_hevc`      | `hevc_videotoolbox` | VideoToolbox   | CPU `overlay`                                    | macOS HEVC                    |
| `qsv_h264`      | `h264_qsv`          | QSV            | `overlay_qsv` if reliable, otherwise CPU overlay | Intel H.264                   |
| `qsv_hevc`      | `hevc_qsv`          | QSV            | `overlay_qsv` if reliable, otherwise CPU overlay | Intel HEVC                    |
| `vaapi_h264`    | `h264_vaapi`        | VAAPI          | CPU or VAAPI path later                          | Linux only                    |
| `vaapi_hevc`    | `hevc_vaapi`        | VAAPI          | CPU or VAAPI path later                          | Linux only                    |

### 5.4 Default First Implementation Recommendation

Start with robust CPU overlay filter paths even when using hardware encoders.

That means:

```txt
CPU decode/filter + hardware encode
```

or:

```txt
hardware decode + CPU filter + hardware encode
```

The full-GPU `overlay_cuda` and `overlay_qsv` paths are useful but more fragile because they depend on exact FFmpeg build support and pixel-format compatibility.

Implementation should allow these profiles, but the fallback path must be solid.

### 5.5 CUDA Filter Example

If full CUDA path is selected and available:

```txt
[0:v]setpts=PTS-STARTPTS,scale_cuda=format=yuv420p[base];
[1:v]setpts=PTS-STARTPTS,format=yuva420p,hwupload[ovr];
[base][ovr]overlay_cuda=0:0:eof_action=repeat:shortest=1[out]
```

This may require adjustment depending on FFmpeg build support. If `overlay_cuda` does not accept the exact options or alpha format, fall back to software overlay plus NVENC.

### 5.6 QSV Filter Example

If QSV overlay is selected and available:

```txt
[0:v]setpts=PTS-STARTPTS,hwupload=extra_hw_frames=64[main_hw];
[1:v]setpts=PTS-STARTPTS,format=bgra,hwupload=extra_hw_frames=64[overlay_hw];
[main_hw][overlay_hw]overlay_qsv=x=0:y=0,hwdownload,format=nv12[out]
```

This may be hardware/driver/build dependent. If unreliable, fall back to software overlay and QSV encode.

### 5.7 Bitrate Handling

The user's selected bitrate is injected through:

```bash
-b:v <bitrate>
```

For example:

```bash
-b:v 60M
```

This should override any default bitrate in a profile.

All MP4 codecs use bitrate-based control for this phase to keep the frontend bitrate slider universal, including VideoToolbox.

---

## 6. Rust Render Loop

### 6.1 Loop Over Overlay Frames, Not Output Frames

Because the overlay pipe FPS may be lower than the output video FPS, Rust should iterate over overlay frames.

Definitions:

```txt
source_video_fps_rational
output_fps = source_video_fps_rational
overlay_pipe_fps = source_video_fps_rational / composite_widget_update_rate
config.scene.fps = overlay_pipe_fps
overlay_frame_count ≈ ceil(render_duration * overlay_pipe_fps)
```

For each overlay frame `j`:

```txt
video_local_time = j / overlay_pipe_fps
activity_time = composite_sync_offset + video_local_time
```

Skia renders the overlay for `activity_time`.

If the dense report was rebuilt for the composite render with:

```txt
config.scene.start = composite_sync_offset
config.scene.end   = composite_sync_offset + render_duration
config.scene.fps   = overlay_pipe_fps
```

then:

```txt
dense_frame_index = j
```

Otherwise, the robust conversion is:

```txt
dense_frame_index = floor((activity_time - config.scene.start) * config.scene.fps)
```

The implementation should clamp or reject out-of-range dense frame indices rather than silently reading invalid data.

### 6.2 Pseudocode

```rust
let source_fps = composite_video_fps_rational;
let output_fps = source_fps;
let update_rate = config.scene.composite_widget_update_rate.unwrap_or(1).max(1);
let overlay_pipe_fps = source_fps / update_rate;

// Composite mode should use a dense report rebuilt with:
// config.scene.start = composite_sync_offset
// config.scene.end   = composite_sync_offset + render_duration
// config.scene.fps   = overlay_pipe_fps

let mut ffmpeg = spawn_composite_ffmpeg(
    video_path,
    render_duration,
    output_fps,
    overlay_pipe_fps,
    width,
    height,
    codec,
    bitrate,
)?;

let mut overlay_frame_index: u64 = 0;

loop {
    if cancel_flag.load(Ordering::SeqCst) {
        break;
    }

    let video_local_time = overlay_frame_index as f64 / overlay_pipe_fps.as_f64();

    // Guard against fractional-frame overrun caused by ceil-style frame counts.
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

    let mut frame_buffer = acquire_frame_buffer(...)?;

    render_frame_rgba(
        paths,
        config,
        dense_activity,
        &prepared_assets,
        dense_frame_index,
        scale,
        None,
        RenderTarget {
            width,
            height,
            pixels: frame_buffer.pixels.as_mut_slice(),
        },
        &mut profiler,
    )?;

    queue_frame(&sender, frame_buffer, &cancel_flag, &mut profiler)?;

    let progress_output_frame =
        ((video_local_time * output_fps.as_f64()).round() as u32)
            .min(output_frame_count);

    controller.set_frame_progress(progress_output_frame);

    overlay_frame_index += 1;
}
```

### 6.3 Important Renderer Requirement

The existing renderer may currently be frame-index based:

```rust
render_frame_rgba(..., frame_index, ...)
```

For composite mode, the preferred abstraction is to rebuild the dense activity report so that:

```txt
dense frame i = overlay pipe frame i
```

using:

```txt
config.scene.start = composite_sync_offset
config.scene.end   = composite_sync_offset + render_duration
config.scene.fps   = overlay_pipe_fps
```

Then the existing frame-index renderer can be reused directly:

```txt
render_frame_rgba(..., overlay_frame_index, ...)
```

If the dense report cannot be rebuilt this way, add a helper in the new composite pipeline that converts `activity_time_seconds` to the expected dense-activity frame index:

```txt
dense_frame_index = floor((activity_time_seconds - config.scene.start) * config.scene.fps)
```

This conversion must respect how telemetry interpolation is already represented in `DenseActivityReport`.

Do not modify the existing transparent pipeline behavior.

### 6.4 Progress Reporting

Because Rust writes only overlay frames, progress can be tracked in one of two ways:

Option A — progress by overlay frames:

```txt
progress_total = overlay_frame_count
```

Option B — progress by final video frames:

```txt
progress_total = output_frame_count
progress_current = min(output_frame_count, overlay_frame_index * composite_widget_update_rate)
```

Option B may feel more consistent to users because the final MP4 has `output_frame_count` frames.

Recommended:

```txt
Use final output frames for user-facing progress.
Use overlay frame count for internal render-loop metrics.
```

---

## 7. Deliverables

### 7.1 [NEW] `src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs`

New module responsible for building FFmpeg arguments for composite mode.

Responsibilities:

- Represent FPS values as rational strings where possible.
- Build input 0 args for the imported video.
- Build input 1 args for the raw RGBA overlay pipe.
- Add `-thread_queue_size 512` before the raw overlay pipe input.
- Select a composite profile based on codec and hardware availability.
- Build `filter_complex`.
- Add explicit output mapping:
  - `-map "[out]"`
  - `-map 0:a?`
- Add:
  - `-r <source_video_fps>`
  - `-c:v <codec>`
  - `-b:v <bitrate>`
  - `-c:a copy`
  - `-movflags faststart`
- Avoid adding `-ss <composite_sync_offset>`. Use `-ss` only for a separate video trim field.

Suggested function:

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

Where `Fps` can be a small local helper struct:

```rust
pub struct Fps {
    pub num: u32,
    pub den: u32,
}

impl Fps {
    pub fn as_f64(&self) -> f64 {
        self.num as f64 / self.den as f64
    }

    pub fn ffmpeg_arg(&self) -> String {
        format!("{}/{}", self.num, self.den)
    }

    pub fn divided_by(&self, factor: u32) -> Fps {
        Fps {
            num: self.num,
            den: self.den * factor,
        }.reduced()
    }
}
```

### 7.2 [NEW] `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs`

New module responsible for the composite rendering pipeline.

Responsibilities:

1. Parse composite parameters from config.
2. Compute:
   - `source_fps`
   - `output_fps = source_fps`
   - `overlay_pipe_fps = source_fps / composite_widget_update_rate`
   - `render_duration`
   - `overlay_frame_count`
   - `output_frame_count`
3. Build or validate a composite-specific dense activity report using:
   - `config.scene.start = composite_sync_offset`
   - `config.scene.end = composite_sync_offset + render_duration`
   - `config.scene.fps = overlay_pipe_fps`
4. Prepare Skia assets using the same mechanisms as the transparent pipeline where possible.
5. Build FFmpeg settings using `ffmpeg_composite`.
6. Spawn FFmpeg with two inputs.
7. Spawn writer thread for overlay frames.
8. Spawn stderr/progress monitor thread.
9. Render and queue one overlay frame per overlay-frame timestamp.
10. Guard the overlay loop against fractional-frame overrun:
    - stop if `video_local_time >= render_duration`
11. Let FFmpeg hold/repeat overlay frames during compositing.
12. Handle cancellation and cleanup.
13. Write composite debug timing summary to `target/debug_render/phase_7`.

Suggested function:

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

Note:

- If `dense_activity` is already built before the composite branch, `backend_render()` may need to be adjusted so composite mode can derive the composite scene timing before building the dense report.
- The existing transparent path must remain unchanged.

### 7.3 [MODIFY] `src-tauri/ovrley_core/src/encode/mod.rs`

Add:

```rust
pub mod ffmpeg_composite;
mod video_composite_pipeline;
```

### 7.4 [MODIFY] `src-tauri/ovrley_core/src/encode/video.rs`

Add a new entry function:

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

This is a new function. Do not modify `render_video()`.

### 7.5 [MODIFY] `src-tauri/ovrley_core/src/config/mod.rs`

Add optional render-time-only fields to `SceneConfig`:

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

Notes:

- These fields are injected only at render time by the frontend.
- They must not be persisted to template files.
- `composite_video_fps_num` and `composite_video_fps_den` are preferred over a lossy float-only FPS field.
- If only float FPS is available initially, convert common values to rational in the backend:
  - approximately `23.976` → `24000/1001`
  - approximately `29.97` → `30000/1001`
  - approximately `59.94` → `60000/1001`
- In composite mode, validation must allow `config.scene.fps` to be fractional/rational-derived because values such as `30000/1001`, `15000/1001`, `10000/1001`, or `15000/2002` may occur depending on source FPS and update rate.
- If `SceneConfig.fps` is currently integer-only, either:
  - extend it to support rational/float FPS for composite mode, or
  - keep the persisted scene FPS unchanged and use an internal composite render config with rational overlay FPS only for dense-report generation.

### 7.6 [MODIFY] `src-tauri/ovrley_core/src/commands/mod.rs`

In `backend_render()`:

1. Parse config.
2. Parse activity.
3. If `config.scene.composite_video_path` is `Some`, validate all required composite fields.
4. Derive composite timing:
   - `source_fps`
   - `overlay_pipe_fps`
   - `render_duration`
   - `scene.start = composite_sync_offset`
   - `scene.end = composite_sync_offset + render_duration`
   - `scene.fps = overlay_pipe_fps`
5. Build the dense activity report using the composite-adjusted scene timing.
6. Start composite render.
7. Otherwise, build the dense activity report and follow the existing transparent pipeline unchanged.

Sketch:

```rust
let mut config = parse_config_json(config_json)?;
let parsed_activity = parse_activity_json(parsed_activity_json)?;

// Branch before building dense activity report, because composite mode needs
// a dense report built at overlay_pipe_fps for the composite render window.
if let Some(ref video_path) = config.scene.composite_video_path {
    let bitrate = config.scene.composite_bitrate.clone()
        .ok_or_else(|| "composite_bitrate required for composite render".to_string())?;

    let fps_num = config.scene.composite_video_fps_num
        .ok_or_else(|| "composite_video_fps_num required for composite render".to_string())?;

    let fps_den = config.scene.composite_video_fps_den
        .ok_or_else(|| "composite_video_fps_den required for composite render".to_string())?;

    let video_duration = config.scene.composite_video_duration
        .ok_or_else(|| "composite_video_duration required for composite render".to_string())?;

    let sync_offset = config.scene.composite_sync_offset.unwrap_or(0.0);
    let trim_start = config.scene.composite_video_trim_start.unwrap_or(0.0);
    let update_rate = config.scene.composite_widget_update_rate.unwrap_or(1).max(1);

    let source_fps = Fps { num: fps_num, den: fps_den }.reduced();
    let overlay_pipe_fps = source_fps.divided_by(update_rate).reduced();

    let render_duration = config.scene.composite_render_duration
        .unwrap_or(video_duration - trim_start);

    // Composite-specific dense report timing.
    config.scene.start = sync_offset;
    config.scene.end = sync_offset + render_duration;
    config.scene.fps = overlay_pipe_fps.as_f64_or_supported_representation();

    let dense_activity = build_dense_activity_report(&parsed_activity, &config)?;

    let output_frame_count = (render_duration * source_fps.as_f64()).ceil() as u32;

    let render_id = controller.try_start(output_frame_count, "Compositing video...")?;

    // Spawn thread and call render_composite_video(...)
} else {
    let dense_activity = build_dense_activity_report(&parsed_activity, &config)?;

    // Existing transparent pipeline unchanged
}
```

---

## 8. Debug Timings

Composite renders write timing summaries to:

```txt
target/debug_render/phase_7
```

The summary should include:

### Existing Buckets Reused Where Practical

| Bucket                     | Description                                        |
| -------------------------- | -------------------------------------------------- |
| `frame.total`              | Total time per overlay frame render loop iteration |
| `buffer.acquire_wait`      | Time waiting for a reusable frame buffer           |
| `buffer.release_wait`      | Time returning a buffer to the pool                |
| `queue.put_wait`           | Time waiting to enqueue frame for writer           |
| `encoder.queue_wait`       | Writer waiting for queued frame                    |
| `ffmpeg.write`             | Time writing raw RGBA frame bytes to FFmpeg stdin  |
| `debug.sample_frame_write` | Optional debug sample-frame write timing           |

### Composite-Specific Buckets

| Bucket                          | Description                                   |
| ------------------------------- | --------------------------------------------- |
| `composite.overlay_frame_count` | Number of overlay frames rendered and written |
| `composite.output_frame_count`  | Number of final output video frames           |
| `composite.source_fps`          | Source/output FPS as rational string          |
| `composite.overlay_pipe_fps`    | Overlay pipe FPS as rational string           |
| `composite.widget_update_rate`  | Overlay update-rate divisor                   |
| `composite.total_ms`            | Full wall-clock render time                   |

### Optional / Estimated FFmpeg Buckets

The following are useful but should be treated as estimates unless directly measurable:

| Bucket             | Description                                          |
| ------------------ | ---------------------------------------------------- |
| `ffmpeg.decode_ms` | Estimated decode time or parsed benchmark timing     |
| `ffmpeg.encode_ms` | Stdin write backpressure proxy, not pure encode time |
| `ffmpeg.filter_ms` | Estimated filter time, not directly isolated         |

Do not overstate these as exact measurements unless FFmpeg provides reliable benchmark data.

---

## 9. Audio Handling

Use explicit optional audio mapping:

```bash
-map "[out]" -map 0:a? -c:a copy
```

Assumptions:

- Source may have zero or one audio track for the first implementation.
- If no audio exists, output should still succeed.
- Audio is copied without filtering or re-encoding.
- If audio codec/container compatibility fails, report the FFmpeg error clearly.
- Multi-track audio handling can be added later.

Manual test wording:

```txt
Verify output MP4 contains the original audio track.
```

not:

```txt
Verify output MP3 contains original audio track.
```

---

## 10. Manual Tests

1. **29.97 fps source video**
   - Input: `30000/1001`
   - Output should remain `30000/1001`, not `30`.
   - Verify with ffprobe.

2. **59.94 fps source video**
   - Input: `60000/1001`
   - Output should remain `60000/1001`, not `60`.
   - Verify with ffprobe.

3. **23.976 fps source video**
   - Input: `24000/1001`
   - Output should remain `24000/1001`, not `24`.

4. **Lower overlay update rate**
   - Source FPS: `60000/1001`
   - `composite_widget_update_rate = 2`
   - Overlay pipe FPS should be `30000/1001`.
   - Rust should render/write approximately half as many overlay frames as final output frames.
   - Dense activity report should be rebuilt at `30000/1001` or equivalent supported representation.

5. **More aggressive overlay update rate**
   - Source FPS: `60000/1001`
   - `composite_widget_update_rate = 6`
   - Overlay pipe FPS should be `10000/1001`.
   - Final output should still be `60000/1001`.
   - Dense activity report should be rebuilt at `10000/1001` or equivalent supported representation.

6. **Sync offset**
   - Activity starts at `10:00`.
   - Video starts at `10:05`.
   - `composite_sync_offset = 300`.
   - Video should begin from its first frame.
   - Overlay should render activity timestamp `300s` at video time `0s`.
   - FFmpeg should not seek 300 seconds into the video.
   - Dense report should use `scene.start = 300`.

7. **Optional video trim**
   - If `composite_video_trim_start = 10`, FFmpeg should seek 10 seconds into the source video.
   - Overlay activity time at output start should still be `composite_sync_offset + 0`, unless a separate UI decision says trim should also affect activity time.

8. **Audio preservation**
   - Output MP4 should contain the original audio track when present.
   - Video without audio should render successfully because of `-map 0:a?`.

9. **Software H.264**
   - Render with `libx264`.
   - Verify composited overlay is visible and output plays in VLC/QuickTime.

10. **NVENC simple path**
    - Render with `h264_nvenc` or `hevc_nvenc`.
    - Use CPU overlay filter first.
    - Verify output succeeds.

11. **Full CUDA path**
    - Test only if CUDA FFmpeg filters are available.
    - If `overlay_cuda` fails, fallback path should be available.

12. **QSV path**
    - Test on Intel hardware.
    - If `overlay_qsv` fails, fallback path should be available.

13. **Cancel mid-render**
    - Verify FFmpeg process is terminated.
    - Verify no orphan temporary output remains.

14. **No video imported**
    - `backend_render()` should use the existing transparent pipeline unchanged.

15. **Debug timings**
    - Verify `target/debug_render/phase_7/timing_summary.json` exists.
    - Verify `"phase": "phase_7"`.
    - Verify FPS, overlay frame count, output frame count, and total wall time are recorded.

16. **Fractional overlay duration edge case**
    - Use a render duration that does not multiply cleanly by overlay pipe FPS.
    - Verify the loop does not render a frame with `video_local_time >= render_duration`.
    - Verify FFmpeg output duration is correct and no extra visual tail frame appears.

17. **Thread queue robustness**
    - Use a low overlay update rate, e.g. source `60000/1001`, update rate `6` or `10`.
    - Verify FFmpeg does not emit avoidable thread queue starvation warnings for the pipe input.
    - Confirm `-thread_queue_size 512` is present before the rawvideo pipe input.

---

## 11. File Change Summary

### New Files

| File                                                           | Purpose                                   |
| -------------------------------------------------------------- | ----------------------------------------- |
| `src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs`         | Composite FFmpeg argument/profile builder |
| `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs` | Composite render pipeline                 |

### Modified Files

| File                                        | Change                                                                                                                    |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `src-tauri/ovrley_core/src/encode/mod.rs`   | Register new composite modules                                                                                            |
| `src-tauri/ovrley_core/src/encode/video.rs` | Add new `render_composite_video()` entry function                                                                         |
| `src-tauri/ovrley_core/src/config/mod.rs`   | Add render-time-only `composite_*` fields                                                                                 |
| `src-tauri/ovrley_core/src/commands/mod.rs` | Branch to composite pipeline when `composite_video_path` is present and build composite dense report with adjusted timing |

### Untouched Sacred Files

| File                                                 | Status               |
| ---------------------------------------------------- | -------------------- |
| `src-tauri/ovrley_core/src/encode/video_pipeline.rs` | Must not be modified |
| `src-tauri/ovrley_core/src/encode/ffmpeg.rs`         | Must not be modified |
| `src-tauri/ovrley_core/src/encode/video_debug.rs`    | Must not be modified |

---

## 12. Implementation Notes / Warnings

### Do Not Round Source FPS

Never do:

```rust
let output_fps = video_fps.ceil() as u32;
```

This is incorrect for composite mode.

### Do Not Use Sync Offset as FFmpeg Seek

Never do this unless `sync_offset` has been explicitly redefined as a video trim offset:

```bash
-ss <composite_sync_offset>
```

Use a separate field:

```txt
composite_video_trim_start
```

### Prefer Rational FPS Everywhere

Use:

```txt
30000/1001
60000/1001
24000/1001
```

instead of:

```txt
29.97
59.94
23.976
```

where possible.

### Let FFmpeg Hold Lower-FPS Overlay Frames

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

### Rebuild Dense Activity for Composite Mode

In composite mode, the dense activity report should be rebuilt using:

```txt
scene.start = composite_sync_offset
scene.end   = composite_sync_offset + render_duration
scene.fps   = source_video_fps / composite_widget_update_rate
```

This ensures there is no unused dense data and no mismatch between dense frames and overlay pipe frames.

### Account for `scene.start` in Frame Mapping

If dense data is not rebuilt exactly for the composite window, any time-to-frame-index conversion must subtract `scene.start`:

```txt
dense_frame_index = floor((activity_time - scene.start) * scene.fps)
```

Do not assume the dense report starts at activity time `0`.

### Guard Against Fractional Frame Overrun

Do not blindly trust:

```rust
ceil(render_duration * overlay_pipe_fps)
```

without checking frame timestamps.

The render loop should stop when:

```rust
video_local_time >= render_duration
```

### Add Thread Queue Size for Overlay Pipe

Use:

```bash
-thread_queue_size 512
```

before the rawvideo pipe input.

This is especially helpful when the overlay stream FPS is much lower than the source video FPS.

### Keep Transparent Pipeline Untouched

Composite mode is a parallel backend path. Transparent export behavior should remain unchanged.
