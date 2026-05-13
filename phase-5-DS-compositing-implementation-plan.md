# Phase 5 — MP4 Compositing Pipeline Implementation

This document breaks Phase 5 into four independent implementation sub-phases, each with deliverables and manual tests.

> **CRITICAL**: All new pipeline logic must live in separate files. The existing `video_pipeline.rs`, `ffmpeg.rs`, and `video_debug.rs` are **sacred** and must not be modified.

---

## Sub-Phase 5A — Config Fields, Module Registration & File Stubs

**Goal**: Define the composite config fields, register new modules, create file stubs with type definitions.

### Deliverables

#### 5A.1 [MODIFY] `src-tauri/ovrley_core/src/config/mod.rs`

Add the following optional render-time-only fields to `SceneConfig`:

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

All must use `#[serde(default)]` so existing configs are unaffected. These are injected only at render time by the frontend and must never be persisted to template files.

Also add a new method to `RenderConfig`:

```rust
impl RenderConfig {
    /// Returns true when the config is for a composite render
    /// (video imported, overlay composited on top).
    pub fn is_composite(&self) -> bool {
        self.scene.composite_video_path.is_some()
    }

    /// Returns the composite widget update rate, defaulting to 1.
    pub fn composite_widget_update_rate(&self) -> u32 {
        self.scene.composite_widget_update_rate.unwrap_or(1).max(1)
    }
}
```

#### 5A.2 [NEW] `src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs` (stub)

Create the file with:

- `Fps` struct (rational number for framerate representation)
- `CompositeProfile` struct
- `CompositeFfmpegSettings` struct
- Stub function `build_composite_ffmpeg_settings()` returning an error "not implemented"
- `HwAccelInfo` struct (imported or duplicate from `codec_detect.rs`)

```rust
/// Rational framerate: num/den (e.g. 30000/1001 for 29.97 fps).
#[derive(Clone, Copy, Debug)]
pub struct Fps {
    pub num: u32,
    pub den: u32,
}

impl Fps {
    pub fn new(num: u32, den: u32) -> Self {
        Fps { num, den }.reduced()
    }

    pub fn as_f64(&self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// Format as "num/den" for FFmpeg -r argument.
    pub fn ffmpeg_arg(&self) -> String {
        format!("{}/{}", self.num, self.den)
    }

    /// Divide fps by an integer factor (e.g. for widget update rate).
    pub fn divided_by(&self, factor: u32) -> Self {
        Fps {
            num: self.num,
            den: self.den * factor,
        }.reduced()
    }

    /// Reduce fraction by GCD.
    pub fn reduced(&self) -> Self {
        let gcd = gcd(self.num, self.den);
        Fps {
            num: self.num / gcd,
            den: self.den / gcd,
        }
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Hardware acceleration availability for composite profiles.
#[derive(Clone, Debug, Default)]
pub struct HwAccelInfo {
    pub nvdec: bool,
    pub cuda: bool,
    pub qsv: bool,
    pub videotoolbox: bool,
    pub vaapi: bool,
}

/// One composite profile definition.
pub struct CompositeProfile {
    pub name: &'static str,
    pub input_args: Vec<String>,
    pub filter_complex: Option<String>,
    pub output_args: Vec<String>,
}

/// Ready-to-use FFmpeg arguments for composite mode.
pub struct CompositeFfmpegSettings {
    pub hw_init_args: Vec<String>,
    pub input_0_args: Vec<String>,
    pub input_1_args: Vec<String>,
    pub filter_complex: String,
    pub output_args: Vec<String>,
}

/// Build FFmpeg command-line arguments for a composite render.
pub fn build_composite_ffmpeg_settings(
    codec_name: &str,
    bitrate: &str,
    video_path: &std::path::Path,
    video_trim_start: f64,
    render_duration: f64,
    width: u32,
    height: u32,
    source_fps: Fps,
    overlay_pipe_fps: Fps,
    hwaccel_available: &HwAccelInfo,
) -> Result<CompositeFfmpegSettings, String> {
    Err("build_composite_ffmpeg_settings not yet implemented".to_string())
}
```

#### 5A.3 [NEW] `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs` (stub)

Create the file with a stub function:

```rust
pub(crate) fn render_composite_video_single(
    _paths: &crate::commands::AppPaths,
    _config: &crate::config::RenderConfig,
    _activity: &crate::activity::schema::ParsedActivity,
    _dense_activity: &crate::activity::schema::DenseActivityReport,
    _controller: &crate::encode::video::RenderController,
    _composite_video_path: &str,
    _composite_bitrate: &str,
    _composite_sync_offset: f64,
    _composite_video_fps_num: u32,
    _composite_video_fps_den: u32,
    _composite_video_duration: f64,
    _composite_render_duration: Option<f64>,
    _composite_video_trim_start: Option<f64>,
    _composite_widget_update_rate: Option<u32>,
) -> Result<String, String> {
    Err("render_composite_video_single not yet implemented".to_string())
}
```

#### 5A.4 [NEW] `src-tauri/ovrley_core/src/encode/video.rs` — add `render_composite_video()` entry stub

```rust
/// Entry point for composite video rendering.
/// Delegates to video_composite_pipeline::render_composite_video_single().
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
) -> Result<String, String> {
    crate::encode::video_composite_pipeline::render_composite_video_single(
        paths, config, activity, dense_activity, controller,
        composite_video_path, composite_bitrate,
        composite_sync_offset, composite_video_fps_num, composite_video_fps_den,
        composite_video_duration, composite_render_duration,
        composite_video_trim_start, composite_widget_update_rate,
    )
}
```

#### 5A.5 [MODIFY] `src-tauri/ovrley_core/src/encode/mod.rs`

```rust
pub mod ffmpeg_composite;
mod video_composite_pipeline;
```

#### 5A.6 [MODIFY] `src-tauri/ovrley_core/src/commands/mod.rs` — branch in `backend_render()`

Modify `backend_render()` to:

1. Parse config and activity as before.
2. If `config.scene.composite_video_path` is `Some`:
   - Validate all required composite fields exist.
   - Derive composite timing:
     - `source_fps = Fps { num: fps_num, den: fps_den }`
     - `overlay_pipe_fps = source_fps.divided_by(composite_widget_update_rate)`
     - `render_duration = composite_render_duration.unwrap_or(video_duration - trim_start)`
   - Override config for dense report:
     - `config.scene.start = composite_sync_offset`
     - `config.scene.end = composite_sync_offset + render_duration`
     - `config.scene.fps = overlay_pipe_fps` (as f64)
   - Build dense report with adjusted config.
   - Compute `output_frame_count = ceil(render_duration * source_fps.as_f64())`.
   - Start controller with output_frame_count.
   - Spawn thread calling `render_composite_video(...)`.
3. Otherwise, use existing transparent pipeline unchanged.

Sketch:

```rust
let mut config = parse_config_json(config_json)?;
let parsed_activity = parse_activity_json(parsed_activity_json)?;

if let Some(ref video_path) = config.scene.composite_video_path {
    let bitrate = config.scene.composite_bitrate.clone()
        .ok_or_else(|| "composite_bitrate required".to_string())?;
    let fps_num = config.scene.composite_video_fps_num
        .ok_or_else(|| "composite_video_fps_num required".to_string())?;
    let fps_den = config.scene.composite_video_fps_den
        .ok_or_else(|| "composite_video_fps_den required".to_string())?;
    let video_duration = config.scene.composite_video_duration
        .ok_or_else(|| "composite_video_duration required".to_string())?;

    let sync_offset = config.scene.composite_sync_offset.unwrap_or(0.0);
    let trim_start = config.scene.composite_video_trim_start.unwrap_or(0.0);
    let update_rate = config.scene.composite_widget_update_rate.unwrap_or(1).max(1);

    let source_fps = ffmpeg_composite::Fps::new(fps_num, fps_den);
    let overlay_pipe_fps = source_fps.divided_by(update_rate);
    let render_duration = config.scene.composite_render_duration
        .unwrap_or(video_duration - trim_start);

    // Rebuild dense report with composite-adjusted timing
    config.scene.start = sync_offset;
    config.scene.end = sync_offset + render_duration;
    config.scene.fps = overlay_pipe_fps.as_f64();

    let dense_activity = build_dense_activity_report(&parsed_activity, &config)?;
    let output_frame_count = (render_duration * source_fps.as_f64()).ceil() as u32;
    let render_id = controller.try_start(output_frame_count, "Compositing video...")?;

    let controller_clone = controller.clone();
    let paths = paths.clone();
    std::thread::spawn(move || {
        match render_composite_video(
            &paths, &config, &parsed_activity, &dense_activity,
            &controller_clone, video_path, &bitrate,
            sync_offset, fps_num, fps_den,
            video_duration,
            config.scene.composite_render_duration,
            config.scene.composite_video_trim_start,
            Some(update_rate),
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
    // Existing transparent pipeline — unchanged
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
```

**Important notes for this phase:**

- `config.scene.fps` accepts a float, but `parse_config_json()` validates it must be an integer (line 467-472 of config/mod.rs). Since composite mode sets it to a rational-derived float (e.g. `29.97002997...`), this validation will **fail**. You must either:
  - **Option A**: Relax the validation to accept fractional fps when `is_composite()` is true.
  - **Option B**: Put the composite dense report building logic **before** `parse_config_json()` — parse config manually, adjust the fps, then call `build_dense_activity_report()` directly without running through `parse_config_json()`.
  - **Option C**: Round `overlay_pipe_fps` to the nearest 3 decimal places and pass it as a float. The validation only checks `fps.fract().abs() > f64::EPSILON` — with rounding to 3 decimal places, `30000/1001 ≈ 29.970` would still fail because `29.970_f64`'s fractional part is `0.97002997...`, not zero. So this doesn't work.
  - **Recommended: Option A** — modify the validation to skip the integer-only check when composite fields are present.

### Manual Tests for 5A

1. **Config round-trip** — Parse a JSON config without composite fields → no change to existing behavior. Parse a JSON config with composite fields → fields are parsed as `Some(...)`. Parse without → fields are `None`.
2. **Fps struct** — `Fps::new(30000, 1001).as_f64()` returns `29.97002997...`. `Fps::new(60000, 1001).divided_by(2)` returns `Fps { num: 30000, den: 1001 }`. `Fps::new(30000, 1001).ffmpeg_arg()` returns `"30000/1001"`.
3. **Composite dispatch** — Call `backend_render()` without `composite_video_path` → existing transparent path used. Call with `composite_video_path` → composite branch entered, `render_composite_video_single` called (returns "not implemented" error for now).
4. **Dense report rebuild** — In composite branch, `config.scene.fps` is set to `overlay_pipe_fps` (e.g. `30000/1001 ≈ 29.97`). Verify `build_dense_activity_report` is called with this adjusted fps and that `frame_count ≈ overlay_frame_count`.
5. **No regression** — `cargo check` succeeds. Existing transparent renders still work.

---

## Sub-Phase 5B — Composite FFmpeg Settings Builder

**Goal**: Implement `ffmpeg_composite.rs` with profile definitions, profile selection, and the full `build_composite_ffmpeg_settings()` function.

### Deliverables

#### 5B.1 [MODIFY] `src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs`

Replace the stub with full profile definitions and settings builder.

**Profile definitions** (static array):

```rust
fn software_h264_profile() -> CompositeProfile { ... }
fn software_h265_profile() -> CompositeProfile { ... }
fn nvgpu_h264_profile() -> CompositeProfile { ... }
fn nvgpu_hevc_profile() -> CompositeProfile { ... }
fn nnvgpu_h264_profile() -> CompositeProfile { ... }
fn nnvgpu_hevc_profile() -> CompositeProfile { ... }
fn mac_h264_profile() -> CompositeProfile { ... }
fn mac_hevc_profile() -> CompositeProfile { ... }
fn qsv_h264_profile() -> CompositeProfile { ... }
fn qsv_hevc_profile() -> CompositeProfile { ... }
fn vaapi_h264_profile() -> CompositeProfile { ... }
fn vaapi_hevc_profile() -> CompositeProfile { ... }
```

Each profile defines:
- `input_args`: Hardware decode args placed before `-i` for input 0 (e.g. `-hwaccel nvdec`, or empty for software).
- `filter_complex`: `Some(filter_string)` for GPU overlay profiles, `None` for profiles that use CPU `overlay` (the default software filter is used).
- `output_args`: Codec-specific encoder args (e.g. `-c:v h264_nvenc -rc:v cbr -bf:v 3 -profile:v high -spatial-aq true`).

**Software profiles** (used when `filter_complex` is `None`):

```rust
// Default filter used for all CPU-overlay profiles
const SOFTWARE_FILTER: &str =
    "[0:v]setpts=PTS-STARTPTS,scale=WxH[base];\
     [1:v]setpts=PTS-STARTPTS[ovr];\
     [base][ovr]overlay=0:0:eof_action=repeat:shortest=1,format=yuv420p[out]";
```

**Profile selection function:**

```rust
pub fn select_profile(
    codec_name: &str,
    hwaccel: &HwAccelInfo,
) -> &'static CompositeProfile {
    match codec_name {
        "h264_nvenc" if hwaccel.cuda => &NNVGPU_H264,
        "h264_nvenc" => &NVGPU_H264,
        "hevc_nvenc" if hwaccel.cuda => &NNVGPU_HEVC,
        "hevc_nvenc" => &NVGPU_HEVC,
        "h264_videotoolbox" => &MAC_H264,
        "hevc_videotoolbox" => &MAC_HEVC,
        "h264_qsv" => &QSV_H264,
        "hevc_qsv" => &QSV_HEVC,
        "h264_vaapi" => &VAAPI_H264,
        "hevc_vaapi" => &VAAPI_HEVC,
        "libx264" => &SOFTWARE_H264,
        "libx265" => &SOFTWARE_H265,
        _ => &SOFTWARE_H264,  // fallback
    }
}
```

**`build_composite_ffmpeg_settings()` full implementation:**

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
) -> Result<CompositeFfmpegSettings, String> {
    let profile = select_profile(codec_name, hwaccel_available);

    // Input 0: imported video file
    let mut input_0_args = Vec::new();
    input_0_args.extend(profile.input_args.clone());
    if video_trim_start > 0.0 {
        input_0_args.push("-ss".to_string());
        input_0_args.push(video_trim_start.to_string());
    }
    input_0_args.push("-t".to_string());
    input_0_args.push(render_duration.to_string());
    input_0_args.push("-i".to_string());
    input_0_args.push(video_path.to_string_lossy().to_string());

    // Input 1: raw RGBA pipe
    let input_1_args = vec![
        "-thread_queue_size".to_string(),
        "512".to_string(),
        "-f".to_string(),
        "rawvideo".to_string(),
        "-pix_fmt".to_string(),
        "rgba".to_string(),
        "-s".to_string(),
        format!("{}x{}", width, height),
        "-r".to_string(),
        overlay_pipe_fps.ffmpeg_arg(),  // e.g. "30000/1001"
        "-i".to_string(),
        "pipe:0".to_string(),
    ];

    // Filter complex: use profile filter or default software filter
    let filter_complex = match &profile.filter_complex {
        Some(f) => f
            .replace("WxH", &format!("{}x{}", width, height))
            .replace("W", &width.to_string())
            .replace("H", &height.to_string()),
        None => SOFTWARE_FILTER
            .replace("WxH", &format!("{}x{}", width, height))
            .replace("W", &width.to_string())
            .replace("H", &height.to_string()),
    };

    // Output args: profile defaults + bitrate override
    let mut output_args = profile.output_args.clone();
    // Remove any existing -b:v from profile defaults (if present)
    output_args.retain(|arg| arg != "-b:v");
    // Insert bitrate before codec-specific flags
    output_args.push("-b:v".to_string());
    output_args.push(bitrate.to_string());  // e.g. "60M"
    // Universal flags
    output_args.push("-c:a".to_string());
    output_args.push("copy".to_string());
    output_args.push("-movflags".to_string());
    output_args.push("faststart".to_string());

    Ok(CompositeFfmpegSettings {
        hw_init_args: Vec::new(),  // populated per-profile as needed
        input_0_args,
        input_1_args,
        filter_complex,
        output_args,
    })
}
```

**Bitrate injection rule**: The user's bitrate slider value overrides any default `-b:v` in the profile. For profiles that use quality-based rate control (e.g. QSV with `-global_quality`), the `-b:v` is added alongside — ffmpeg uses the last specified rate control method and ignores the earlier one.

### Manual Tests for 5B

1. **Software H.264 profile** — Call `build_composite_ffmpeg_settings("libx264", "60M", ...)` with no hwaccel. Verify:
   - `input_0_args` contains `-t <duration> -i <path>`
   - `input_1_args` contains `-thread_queue_size 512 -f rawvideo -pix_fmt rgba -s WxH -r <overlay_pipe_fps> -i pipe:0`
   - `filter_complex` is the SOFTWARE_FILTER with WxH replaced
   - `output_args` contains `-c:v libx264 -b:v 60M -c:a copy -movflags faststart`

2. **NVENC simple profile** — Call with `"h264_nvenc"`, `hwaccel.nvdec = true`, `hwaccel.cuda = false`. Verify `input_0_args` contains `-hwaccel nvdec`.

3. **CUDA full-GPU profile** — Call with `"h264_nvenc"`, `hwaccel.cuda = true`. Verify `input_0_args` contains `-hwaccel cuda -hwaccel_output_format cuda` and `filter_complex` uses `overlay_cuda`.

4. **Rational FPS arg** — Verify `-r 30000/1001` appears in `input_1_args` when `overlay_pipe_fps = Fps::new(30000, 1001)`.

5. **Trim start** — Pass `video_trim_start = 10.5`. Verify `-ss 10.5` appears in `input_0_args`.

6. **Bitrate override** — Profile's default bitrate is replaced by user's bitrate. Verify `output_args` contains `-b:v <user_value>` and not the profile default.

7. **Filter resolution substitution** — All occurrences of `WxH`, `W`, and `H` in filter strings are replaced with actual numeric values.

---

## Sub-Phase 5C — Composite Render Pipeline

**Goal**: Implement `video_composite_pipeline.rs` with the full render loop.

### Deliverables

#### 5C.1 [MODIFY] `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs`

Full implementation of `render_composite_video_single()`. The architecture mirrors `video_pipeline::render_video_single()` but:

- Spawns ffmpeg with **two inputs** (video file + pipe:0).
- Iterates over **overlay frames** (not output frames).
- Uses the existing `render_frame_rgba()` function directly (dense report already aligned).
- Does **not** use `widget_update_rate` or `container_fps()` from the config — these are specific to the transparent pipeline.

**Step-by-step implementation:**

1. **Parse parameters**:
   ```rust
   let source_fps = Fps::new(composite_video_fps_num, composite_video_fps_den);
   let output_fps = source_fps;
   let update_rate = composite_widget_update_rate.unwrap_or(1).max(1);
   let overlay_pipe_fps = source_fps.divided_by(update_rate);
   let duration = composite_render_duration.unwrap_or(composite_video_duration - composite_video_trim_start.unwrap_or(0.0));
   let overlay_frame_count = (duration * overlay_pipe_fps.as_f64()).ceil() as u64;
   let output_frame_count = (duration * source_fps.as_f64()).ceil() as u64;
   ```

2. **Make dimensions even** (reuse `make_even` logic or duplicate it — `make_even` is private to `video_pipeline.rs`):
   ```rust
   fn make_even(value: u32) -> u32 {
       if value % 2 == 0 { value } else { value + 1 }
   }
   ```

3. **Prepare Skia assets**:
   ```rust
   let (prepared_preview_assets, label_cache_status, prepare_timings, prepare_total_ms) =
       prepare_preview_assets(paths, config, activity, dense_activity)?;
   ```

4. **Create debug dir**:
   ```rust
   let debug_dir = create_debug_dir(paths, "phase_7")?;
   ```

5. **Write prepare summary**:
   ```rust
   write_prepare_summary(&debug_dir, prepare_total_ms, &prepare_timings, label_cache_status)?;
   ```

6. **Build composite ffmpeg settings**:
   ```rust
   let ffmpeg_settings = ffmpeg_composite::build_composite_ffmpeg_settings(
       codec_name, composite_bitrate, video_path, trim_start,
       duration, width, height, source_fps, overlay_pipe_fps, &hwaccel_info,
   )?;
   ```

7. **Spawn ffmpeg process with two inputs**:
   ```rust
   fn spawn_composite_ffmpeg(
       ffmpeg_bin: &Path,
       settings: &CompositeFfmpegSettings,
       output_path: &Path,
   ) -> Result<std::process::Child, String> {
       let mut command = Command::new(ffmpeg_bin);
       suppress_child_console(&mut command);
       command.arg("-loglevel").arg("info");
       // HW init args (if any — primarily for QSV)
       command.args(&settings.hw_init_args);
       // Input 0: video file
       command.args(&settings.input_0_args);
       // Input 1: raw RGBA pipe
       command.args(&settings.input_1_args);
       // Filter complex
       command.arg("-filter_complex").arg(&settings.filter_complex);
       // Output mapping
       command.arg("-map").arg("[out]");
       command.arg("-map").arg("0:a?");
       // Output fps
       command.arg("-r").arg(&source_fps.ffmpeg_arg());
       // Output args
       command.args(&settings.output_args);
       command.arg("-y").arg(output_path);
       command.stdin(Stdio::piped());
       command.stderr(Stdio::piped());
       command.stdout(Stdio::null());
       command.spawn().map_err(|e| format!("Could not start ffmpeg: {e}"))
   }
   ```

8. **Initialize frame buffer pool** (same as transparent pipeline):
   ```rust
   const FRAME_QUEUE_SIZE: usize = 12;
   let frame_byte_len = (width as usize) * (height as usize) * 4;
   let (sender, receiver) = mpsc::sync_channel::<FrameBuffer>(FRAME_QUEUE_SIZE);
   let (free_sender, free_receiver) = mpsc::sync_channel::<FrameBuffer>(FRAME_QUEUE_SIZE + 1);
   for _ in 0..(FRAME_QUEUE_SIZE + 1) {
       free_sender.send(FrameBuffer { pixels: vec![0u8; frame_byte_len] })
           .map_err(|_| "Failed to initialize buffer pool".to_string())?;
   }
   ```

   Where `FrameBuffer` is the same struct as in `video_pipeline.rs` — either import it (if made crate-visible) or duplicate it. **Recommendation**: duplicate it to avoid modifying the sacred file.

9. **Spawn writer thread** (identical to transparent pipeline — same `writer_worker` function):
   ```rust
   let writer_thread = thread::spawn(move || {
       writer_worker(stdin, receiver, free_sender, cancel_flag_for_writer)
   });
   ```

10. **Spawn monitor thread** (identical — same `monitor_ffmpeg` function):
    ```rust
    let monitor_thread = thread::spawn(move || {
        monitor_ffmpeg(stderr, encoded_frames_for_monitor)
    });
    ```

11. **Render loop** (the core difference from transparent pipeline):
    ```rust
    let render_result = (|| -> Result<(), String> {
        for overlay_frame_index in 0..overlay_frame_count {
            if cancel_flag.load(Ordering::SeqCst) { break; }

            let video_local_time = overlay_frame_index as f64 / overlay_pipe_fps.as_f64();

            // Guard against fractional-frame overrun
            if video_local_time >= duration { break; }

            let frame_started = Instant::now();
            let mut frame_buffer = acquire_frame_buffer(&free_receiver, &cancel_flag, &mut profiler)?;

            // Dense report rebuilt for composite — frame_index == overlay_frame_index
            render_frame_rgba(
                paths, config, dense_activity,
                &prepared_preview_assets.prepared_assets,
                overlay_frame_index as usize,  // direct frame index
                scale, None,
                RenderTarget {
                    width, height,
                    pixels: frame_buffer.pixels.as_mut_slice(),
                },
                &mut profiler,
            )?;

            queue_frame(&sender, frame_buffer, &cancel_flag, &mut profiler)?;
            rendered_frames += 1;

            let frame_ms = frame_started.elapsed().as_secs_f64() * 1000.0;
            profiler.record_ms("frame.total", frame_ms);

            // Progress by output frames for user consistency
            let progress = ((video_local_time * output_fps.as_f64()).round() as u32)
                .min(output_frame_count as u32);
            let estimate = estimator.record(rendered_frames, overlay_frame_count as u32, frame_ms / 1000.0);
            controller.set_frame_progress(
                progress,
                output_frame_count as u32,
                encoded_frames.load(Ordering::SeqCst),
                estimate,
            );
        }
        Ok(())
    })();
    ```

12. **Cleanup and validation** (same pattern as transparent pipeline):
    - Drop sender, join writer/monitor threads.
    - Wait for ffmpeg child process.
    - Check exit status.
    - Verify writer frame count matches overlay_frame_count.
    - Remove output file on errors/cancellation.

13. **Write debug timing summary**:
    ```rust
    write_timing_summary_with_phase(
        &debug_dir, config, &output_path,
        "phase_7",
        output_frame_count as u32,
        dense_activity.frame_count as u32,
        rendered_frames,
        total_time_taken,
        sample_frames,
        merged_timings,
    )?;
    ```

**Functions to duplicate from `video_pipeline.rs`** (cannot import because they're private in the sacred file):

| Function | Reason |
|----------|--------|
| `FrameBuffer` struct | Buffer pool element |
| `WriterResult` struct | Writer thread return type |
| `writer_worker()` | Writes queued frames to ffmpeg stdin |
| `monitor_ffmpeg()` | Parses ffmpeg stderr for progress |
| `acquire_frame_buffer()` | Gets a buffer from the free pool |
| `queue_frame()` | Sends a buffer to the writer thread |
| `merge_timing_maps()` | Merges render/writer timing buckets |
| `ProgressEstimator` | EMA-based remaining time estimation |
| `spawn_composite_ffmpeg()` | Starts ffmpeg with two inputs |

Functions reused from crate-public API:
- `ffmpeg::resolve_ffmpeg_binary()` — public
- `ffmpeg::suppress_child_console()` — public
- `render::prepare_preview_assets()` — public
- `render::render_frame_rgba()` — public
- `video_debug::create_debug_dir()` — public (it's `pub(crate)`)
- `video_debug::write_prepare_summary()` — public
- `video_debug::write_timing_summary_with_phase()` — public

### Manual Tests for 5C

1. **Render loop produces correct frame count** — Call `render_composite_video_single()` with known params (10s video at 30000/1001 fps, update_rate=2). Verify `overlay_frame_count = ceil(10 * 15000/1001) = ceil(149.85) = 150`. Verify rendered_frames = 150.

2. **Frame index alignment** — With `scene.start = sync_offset`, `scene.fps = overlay_pipe_fps`, verify `render_frame_rgba` is called with `frame_index = overlay_frame_index`. The first frame renders at index 0, corresponding to activity time `sync_offset + 0s`.

3. **Overrun guard** — Same settings as test 1. The loop stop condition `video_local_time >= duration` (i.e. `(149) / (15000/1001) ≈ 9.94 < 10.0`) means all 150 frames render. The 151st frame would have `local_time = 150 / (15000/1001) ≈ 10.01 >= 10.0` and is correctly skipped.

4. **Buffer pool recycling** — Frames are acquired from the pool, written to pipe, returned to pool. No deadlocks. No OOM.

5. **FFmpeg process args** — Verify the spawned ffmpeg command contains all expected args: two inputs, filter_complex, output mapping, audio copy, faststart.

6. **Cancellation** — Trigger cancel mid-render. Verify ffmpeg is killed, temporary output cleaned up, no orphan files.

7. **Sample frames** — If `OVRLEY_RENDER_SAMPLE_FRAMES` is set, verify sample frames are written to `target/debug_render/phase_7/`.

---

## Sub-Phase 5D — Hardware Profiles & Integration

**Goal**: Finalize hardware profile definitions, add fallback logic, wire the full pipeline end-to-end.

### Deliverables

#### 5D.1 [MODIFY] `src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs`

Add fallback logic in `build_composite_ffmpeg_settings()`:

```rust
pub fn build_composite_ffmpeg_settings(...) -> Result<CompositeFfmpegSettings, String> {
    let profile = select_profile(codec_name, hwaccel_available);

    // If the selected profile uses a hardware filter (overlay_cuda, overlay_qsv)
    // but the filter string construction fails or is unavailable, fall back
    // to the software overlay + same encoder.
    let effective_profile = if uses_hardware_filter(profile) && !is_hardware_filter_available(profile) {
        eprintln!("WARN: Hardware overlay filter not available for {}, falling back to software overlay", profile.name);
        &SOFTWARE_PROFILE_WITH_ENCODER(codec_name)
    } else {
        profile
    };

    // ... rest of the function uses effective_profile
}
```

Define additional profiles for hardware encoder + software overlay combinations:

```rust
pub fn software_overlay_with_encoder(codec_name: &str) -> CompositeProfile {
    let encoder_args = match codec_name {
        "h264_nvenc" => vec!["-c:v", "h264_nvenc", "-rc:v", "cbr", "-bf:v", "3", "-profile:v", "high", "-spatial-aq", "true"],
        "hevc_nvenc" => vec!["-c:v", "hevc_nvenc", "-rc:v", "cbr", "-bf:v", "3", "-profile:v", "main", "-spatial-aq", "true"],
        // ... etc
        _ => vec!["-c:v", codec_name],
    };
    CompositeProfile {
        name: "software_overlay",
        input_args: vec![],  // no hw decode (or nvdec for NVENC)
        filter_complex: None,  // uses default software filter
        output_args: encoder_args.iter().map(|s| s.to_string()).collect(),
    }
}
```

The HW profile definitions themselves remain as defined in 5B. This sub-phase adds the **safety net** — if a full-GPU path fails, the pipeline degrades gracefully.

#### 5D.2 Enable `overlay_cuda` profiles with CUDA filter string

```rust
const NNVGPU_FILTER: &str =
    "[0:v]setpts=PTS-STARTPTS,scale_cuda=format=yuv420p[base];\
     [1:v]setpts=PTS-STARTPTS,format=yuva420p,hwupload[ovr];\
     [base][ovr]overlay_cuda=0:0:eof_action=repeat:shortest=1[out]";
```

Note: `overlay_cuda` may not support `eof_action` or `shortest` options. If ffmpeg rejects them, fall back to:

```rust
const NNVGPU_FILTER_FALLBACK: &str =
    "[0:v]scale_cuda=format=yuv420p[mp4_stream];\
     [1:v]format=yuva420p,hwupload[overlay_stream];\
     [mp4_stream][overlay_stream]overlay_cuda";
```

The implementation should try the full-options filter first, and if ffmpeg fails at startup, retry with the minimal filter. Handle this in the pipeline: if ffmpeg exits with error before any frames are written, re-spawn with the fallback filter string.

#### 5D.3 Enable `overlay_qsv` profiles with QSV filter string

```rust
const QSV_FILTER: &str =
    "[0:v]setpts=PTS-STARTPTS,hwupload=extra_hw_frames=64[main_hw];\
     [1:v]setpts=PTS-STARTPTS,format=bgra,hwupload=extra_hw_frames=64[overlay_hw];\
     [main_hw][overlay_hw]overlay_qsv=x=0:y=0,hwdownload,format=nv12[out]";
```

Note: QSV's `overlay_qsv` filter requires the QSV hardware device to be initialized before input. Add to `hw_init_args`:

```rust
"-init_hw_device", "qsv=hw", "-hwaccel", "qsv", "-hwaccel_output_format", "qsv"
```

### Manual Tests for 5D

1. **NVENC + CPU overlay** — Render with `h264_nvenc`, no CUDA. Verify ffmpeg uses `-hwaccel nvdec` for decode and default CPU `overlay` filter. Verify output plays correctly.

2. **CUDA full-GPU path** — Render with `h264_nvenc` + CUDA available. Verify ffmpeg uses `-hwaccel cuda`, `overlay_cuda` filter, and `h264_nvenc` encoder. Verify output.

3. **CUDA fallback** — If `overlay_cuda` fails (hypothetical: ffmpeg build lacks it), verify fallback to `scale_cuda` + CPU overlay + NVENC.

4. **QSV path** — Render with `hevc_qsv` on Intel hardware. Verify QSV device init args present, `overlay_qsv` filter used, `hwdownload` converts back to system memory for encoder.

5. **QSV fallback** — If `overlay_qsv` unavailable, verify CPU overlay + QSV encode used instead.

6. **macOS VideoToolbox** — Render with `h264_videotoolbox`. Verify `-hwaccel videotoolbox` in input args, no GPU filter (CPU overlay), Videotoolbox encoder in output args.

7. **All codec paths** — For each codec in the render dialog, verify ffmpeg produces a valid MP4 with composited overlay. Run a matrix of:
   - `libx264`, `libx265` (software)
   - `h264_nvenc`, `hevc_nvenc` (NVIDIA)
   - `h264_qsv`, `hevc_qsv` (Intel)
   - `h264_videotoolbox`, `hevc_videotoolbox` (macOS)
   - `h264_vaapi`, `hevc_vaapi` (Linux)

---

## Summary: Phase 5 Implementation Order

| Sub-Phase | Files | Depends On | Est. Effort |
|-----------|-------|------------|-------------|
| 5A — Config, stubs, dispatch | `config/mod.rs`, `ffmpeg_composite.rs`, `video_composite_pipeline.rs`, `video.rs`, `mod.rs`, `commands/mod.rs` | Nothing | Small |
| 5B — Settings builder | `ffmpeg_composite.rs` | 5A | Medium |
| 5C — Render pipeline | `video_composite_pipeline.rs` | 5A, 5B | Large |
| 5D — Hardware profiles | `ffmpeg_composite.rs` | 5B, 5C | Medium |

Each sub-phase can be implemented, compiled, and tested independently before moving to the next.

---

## File Change Summary

### New Files

| File | Phase | Purpose |
|------|-------|---------|
| `src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs` | 5A, 5B, 5D | FPS helper, profile definitions, settings builder |
| `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs` | 5A, 5C | Composite render pipeline |

### Modified Files

| File | Phase | Change |
|------|-------|--------|
| `src-tauri/ovrley_core/src/config/mod.rs` | 5A | Add `composite_*` fields to SceneConfig |
| `src-tauri/ovrley_core/src/encode/mod.rs` | 5A | Register new modules |
| `src-tauri/ovrley_core/src/encode/video.rs` | 5A | Add `render_composite_video()` entry function |
| `src-tauri/ovrley_core/src/commands/mod.rs` | 5A | Branch in `backend_render()`, build composite dense report |

### Untouched Sacred Files

| File | Status |
|------|--------|
| `src-tauri/ovrley_core/src/encode/video_pipeline.rs` | Must not be modified |
| `src-tauri/ovrley_core/src/encode/ffmpeg.rs` | Must not be modified |
| `src-tauri/ovrley_core/src/encode/video_debug.rs` | Must not be modified |
