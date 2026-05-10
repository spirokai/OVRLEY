# MP4 Compositing — Implementation Plan

> **Scope**: Add the ability to import an `.mp4` file and render overlays composited on top of the video, producing a final `.mp4` output. The existing transparent-overlay export pipeline **must not be modified**.

---

## Phase 1 — Video Import & Store State

**Goal**: User can import an MP4 via the AppHeader. The file path is stored and available to all components.

### Deliverables

#### [NEW] `app/src/store/slices/createVideoImportSlice.js`

New Zustand slice managing all imported-video state:

```
importedVideoPath: null          // absolute path from Tauri file dialog
importedVideoDuration: null      // seconds (float), read via ffprobe
importedVideoFps: null           // fps (float)
importedVideoResolution: null    // { width, height }
importedVideoCreationTime: null  // ISO-8601 string or null
videoSyncOffsetSeconds: 0        // user-adjustable sync offset
videoSyncWarning: null           // string warning or null
```

Actions: `setImportedVideo(metadata)`, `clearImportedVideo()`, `setVideoSyncOffset(seconds)`, `setVideoSyncWarning(msg)`

#### [MODIFY] `app/src/store/useStore.js`

Import and spread `createVideoImportSlice` alongside existing slices.

#### [NEW] `app/src/lib/videoMetadata.js`

Utility that calls a new Tauri command `backend_probe_video` to run `ffprobe` and extract: duration, fps, resolution, and creation_time from metadata/tags.

#### [NEW] Tauri command — `backend_probe_video`

- [NEW] `src-tauri/ovrley_core/src/encode/video_probe.rs` — runs `ffprobe -v quiet -print_format json -show_format -show_streams <path>`, parses JSON output, returns structured metadata.
- [MODIFY] `src-tauri/ovrley_core/src/encode/mod.rs` — add `pub mod video_probe;`
- [MODIFY] `src-tauri/ovrley_core/src/commands/mod.rs` — add `backend_probe_video` function.
- [MODIFY] `src-tauri/src/lib.rs` — register `backend_probe_video` command.

#### [MODIFY] `app/src/api/backend.js`

Add `probeVideo(filePath)` function calling the new Tauri command.

#### [MODIFY] `app/src/components/AppHeader.jsx`

Add an "Import Video" button (e.g. `Film` icon from lucide) next to the activity button. On click, open a Tauri file dialog filtering for `*.mp4;*.mov;*.mkv`. On selection, call `probeVideo()`, populate the store, and attempt auto-sync (Phase 2).

When a video is imported, its FPS **overrides** the overlay FPS. The video FPS is written into `config.scene.fps` and the framerate selector in both SidebarSettingsTab and RenderVideoDialog is **disabled** with a note "Locked to video FPS (X fps)".

### Manual Tests

1. Click "Import Video" → file picker opens, filtered to video files.
2. Select a valid MP4 → store populates with duration, fps, resolution, creation time.
3. FPS selector becomes disabled and shows the video's framerate.
4. Select a corrupt file → error shown, store not modified.
5. Import again → previous video replaced.
6. Clear imported video → FPS selector re-enabled, reverts to previous value.

---

## Phase 2 — Video Time Sync

**Goal**: Automatically align imported video to the activity timeline using timestamps; provide manual offset control.

### Time Detection Strategy

The creation time is extracted via ffprobe (Phase 1) from multiple sources in priority order:

1. **`format.tags.creation_time`** — most reliable for action cams (GoPro, DJI, Insta360 write this).
2. **`streams[0].tags.creation_time`** — fallback if format-level tag is absent.
3. **`format.tags.com.apple.quicktime.creationdate`** — Apple/QuickTime metadata, often in local time with timezone.
4. **File system `mtime`/`ctime`** — last resort; unreliable if file was copied/edited.

For edited exports (Premiere, DaVinci, etc.), creation_time usually reflects export time, not original recording time. In these cases the auto-sync will fail and the user must manually set the offset.

### Auto-Sync Algorithm

Auto sync can only happen when both a video and an activity are loaded. This can happen in either order and must be idempotent. If any of the two files change, the auto-sync should be re-run.

```
videoStart = parse(creation_time)  // UTC
activityStart = parse(activitySummary.startTime)  // UTC
activityEnd = parse(activitySummary.endTime)  // UTC

offsetSeconds = (videoStart - activityStart) / 1000

if videoStart is outside [activityStart - 1h, activityEnd + 1h]:
    show warning "Video creation time is outside activity range — placed at start"
    set offset to 0  // place video at activity start
else:
    set offset to offsetSeconds
```

If creation_time cannot be determined or is outside the activity range → show warning "Could not determine video creation time or is outside the activity range — placed at start", set offset to 0 (video starts at the beginning of the activity). The user can always manually adjust the offset.

### Deliverables

#### [MODIFY] `app/src/store/slices/createVideoImportSlice.js`

Add auto-sync logic in a `computeVideoSync(activitySummary)` action called after video import or activity load.

#### [MODIFY] `app/src/components/SidebarSettingsTab.jsx`

Add a new **"Video Sync"** section (only visible when `importedVideoPath` is set). Contains:

- **Video info block**: read-only display of duration, FPS, and resolution (e.g. "12:34 min · 29.97 fps · 3840×2160").
- Read-only display of detected creation time (or "Creation Time: Unknown").
- Warning alert if sync is out-of-range or creation time unknown.
- Offset input accepting seconds to the tenth of second (`123.4` or `-123.4`) or timecode (`4:53.3` or `-4:53.3`). Parses both formats.
- A "Reset Sync" button to re-run auto-sync.

When `importedVideoPath` is set, also:

- Hide the "Custom Export Range" section (the `ExportRangeSettings` block).
- Disable the framerate selector with a note "Locked to video FPS".

### Manual Tests

1. Import video with known GoPro creation_time + load matching activity → offset auto-calculated correctly.
2. Import video from DaVinci export (creation_time = export time) → warning displayed, video placed at activity start, offset editable.
3. Import video with no creation_time → "Unknown" shown, warning displayed, video placed at activity start.
4. Video info block shows correct duration, fps, resolution.
5. Type `4:53` in offset → correctly parsed as 293 seconds.
6. FPS selector disabled, shows video FPS.
7. Export range section hidden when video imported, visible when video cleared.

---

## Phase 3 — Video Preview in Editor & Player

**Goal**: Display the imported video as canvas background and integrate with the player timeline.

### Deliverables

#### [MODIFY] `app/src/components/overlay-editor/OverlayCanvas.jsx`

When `importedVideoPath` is set, render a `<video>` element behind the widget layer as the canvas background (replacing the solid color/checker). The video is displayed at the scene resolution and synced to `previewSecond + videoSyncOffsetSeconds`.

#### [NEW] `app/src/hooks/useVideoPreview.js`

Custom hook managing the `<video>` element:

- Creates `<video>` with `src` pointing to the local file via `convertFileSrc` (Tauri asset protocol).
- Seeks to the correct frame on `previewSecond` changes.
- Pauses/plays in sync with OverlayPlayer playback state.

#### [MODIFY] `app/src/components/OverlayPlayer.jsx`

- Add a colored highlight region on the slider showing where the imported video clip covers relative to the activity timeline.
- Display the video duration next to the total activity duration.
- When video is imported, constrain playback to the video's time range.

#### [MODIFY] `app/src/components/AppHeader.jsx`

When video is imported, add a `backgroundMode: 'video'` option to the background toggle group. Auto-select it on import.

### Manual Tests

1. Import video → canvas shows video frame at current playhead position.
2. Scrub timeline → video seeks in sync.
3. Play → video plays in sync with widget preview.
4. Slider shows highlighted region for video coverage.
5. Background toggle includes "Video" option, auto-selected on import.

---

## Phase 4 — Codec Detection & Render Dialog

**Goal**: Detect available hardware codecs via ffmpeg, restructure the render dialog with grouped codec options and bitrate controls.

### Codec Detection

#### [NEW] `src-tauri/ovrley_core/src/encode/codec_detect.rs`

Runs `ffmpeg -encoders` and parses output to detect availability of:

- **Software**: `libx264`, `libx265`
- **NVIDIA**: `h264_nvenc`, `hevc_nvenc`
- **Intel QSV**: `h264_qsv`, `hevc_qsv`
- **AMD/VA-API**: `h264_vaapi`, `hevc_vaapi`
- **macOS**: `h264_videotoolbox`, `hevc_videotoolbox`

Also runs `ffmpeg -hwaccels` to detect: `cuda`, `nvdec`, `qsv`, `vaapi`, `videotoolbox`.

Returns a `AvailableCodecs` struct with boolean fields for each codec + hwaccel.

#### [NEW] Tauri command — `backend_detect_codecs`

- [MODIFY] `src-tauri/ovrley_core/src/encode/mod.rs` — add `pub mod codec_detect;`
- [MODIFY] `src-tauri/ovrley_core/src/commands/mod.rs` — add function.
- [MODIFY] `src-tauri/src/lib.rs` — register command.
- [MODIFY] `app/src/api/backend.js` — add `detectCodecs()`.

#### [MODIFY] `app/src/store/slices/createVideoImportSlice.js`

Add `availableCodecs: null` state; action `fetchAvailableCodecs()` called on app bootstrap.

#### [MODIFY] `app/src/components/RenderVideoDialog.jsx`

Restructure the codec selector into two `<SelectGroup>` sections:

**Transparent Codecs** (existing):

- ProRes (CPU), QT RLE (CPU), ProRes Vulkan (GPU), ProRes macOS
- If video is imported: entire group disabled, tooltip "Video imported — use MP4 codecs"

**MP4 Codecs** (new):

- H.264 (CPU) — `libx264`
- H.265 (CPU) — `libx265`
- H.264 NVENC (GPU) — `h264_nvenc` (if detected)
- HEVC NVENC (GPU) — `hevc_nvenc` (if detected)
- H.264 QSV (Intel) — `h264_qsv` (if detected)
- HEVC QSV (Intel) — `hevc_qsv` (if detected)
- H.264 VA-API — `h264_vaapi` (if detected, Linux only)
- HEVC VA-API — `hevc_vaapi` (if detected, Linux only)
- H.264 VideoToolbox — `h264_videotoolbox` (macOS only)
- HEVC VideoToolbox — `hevc_videotoolbox` (macOS only)
- If no video imported: entire group disabled, tooltip "Video required"

Each unavailable codec (if potentially available on the platform; e.g. on Windows there's no point showing VAAPI or VideoToolbox options and vice versa;) is shown greyed out with "Not available on this system".

#### Bitrate Slider

Add a bitrate `<Slider>` (20–100 Mbps) visible when an MP4 codec is selected. Default value is computed from **total pixel count** (`width × height`), which is orientation-agnostic — a 1080×1920 portrait and 1920×1080 landscape both equal ~2M pixels and get the same default.

Defaults are defined in a simple config constant (e.g. in a new `app/src/lib/bitrateDefaults.js`):

```js
/**
 * Bitrate defaults in Mbps, keyed by max pixel count.
 * Each entry: { maxPixels, label, h264, h265, h264Hfr, h265Hfr }
 * Hfr = high frame rate (>30 fps). Values in Mbps.
 * Entries are evaluated top-to-bottom; first match wins.
 */
export const BITRATE_BINS = [
  {
    maxPixels: 2_073_600,
    label: "1080p",
    h264: 10,
    h265: 8,
    h264Hfr: 15,
    h265Hfr: 12,
  },
  {
    maxPixels: 3_686_400,
    label: "1440p",
    h264: 30,
    h265: 20,
    h264Hfr: 45,
    h265Hfr: 30,
  },
  {
    maxPixels: 8_294_400,
    label: "4K",
    h264: 60,
    h265: 40,
    h264Hfr: 90,
    h265Hfr: 60,
  },
];

/** Fallback if resolution exceeds all bins */
export const BITRATE_FALLBACK = {
  h264: 80,
  h265: 60,
  h264Hfr: 100,
  h265Hfr: 80,
};

export function getDefaultBitrate(width, height, fps, codecName) {
  const pixels = width * height;
  const isHevc = /h265|hevc|x265/i.test(codecName);
  const isHfr = fps > 30;
  const bin =
    BITRATE_BINS.find((b) => pixels <= b.maxPixels) ?? BITRATE_FALLBACK;
  if (isHevc) return isHfr ? bin.h265Hfr : bin.h265;
  return isHfr ? bin.h264Hfr : bin.h264;
}
```

This config is easy to tweak — just edit the numbers in `BITRATE_BINS`. User can always override via the slider.

### Manual Tests

1. Open render dialog without video → MP4 group disabled with "Video required" badge.
2. Import video, open render dialog → Transparent group disabled with "Video imported" badge.
3. Only codecs detected on this system are enabled.
4. Select H.264 at 4K 30fps → bitrate defaults to 60 Mbps.
5. Select H.265 at 1080p 60fps → bitrate defaults to 12 Mbps.
6. Move bitrate slider → value updates.

---

## Phase 5 — MP4 Compositing FFmpeg Pipeline (Backend)

**Goal**: Build a parallel encoding pipeline that composites Skia-rendered overlay frames on top of the imported MP4 using ffmpeg's `filter_complex`.

> **CRITICAL**: All new code in **separate files**. Existing `video_pipeline.rs` and `ffmpeg.rs` remain untouched for transparent export.

### Architecture

The compositing pipeline uses ffmpeg with two inputs:

1. **Input 0**: The imported MP4 file (decoded by ffmpeg, optionally with hwaccel).
2. **Input 1**: Raw RGBA frames piped via stdin (Skia-rendered overlays, same as current pipeline).

FFmpeg composites them using `filter_complex`:

```
[0:v]scale=WxH[base];[1:v]format=yuva420p[overlay];[base][overlay]overlay=0:0
```

For hardware-accelerated profiles (nvenc, qsv), the filter chains from the builtin profiles in the spec are used.

### Deliverables

#### [NEW] `src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs`

New module providing `build_composite_ffmpeg_settings()`:

- Accepts: codec name, bitrate, video path, resolution, fps, hwaccel info.
- Returns: `CompositeFfmpegSettings` struct with input_args, filter_complex, output_args.
- Contains profile definitions matching the spec's builtin profiles (nvgpu, nnvgpu, mac, mac_hevc, qsv) plus software fallbacks.
- The `-movflags faststart` flag is always appended for MP4 output.

#### [NEW] `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs`

New rendering pipeline function `render_composite_video_single()`:

- Mirrors `render_video_single()` structure but spawns ffmpeg with two inputs.
- Input 0: the MP4 file path.
- Input 1: pipe:0 (stdin) for raw RGBA overlay frames.
- Uses `filter_complex` instead of `-vf`.
- Applies `-ss` and `-t` to input 0 to seek to the correct offset.
- Writes the same debug JSON timing summaries to `target/debug_render/phase_7`.
- **Frame rendering loop**:
  - Iterates through every frame of the output video (matching background FPS).
  - Uses `widget_update_rate` logic: only calls `render_frame_rgba` every $N$ frames.
  - Re-uses the previous frame buffer for intermediate writes to `pipe:0`.
- **Debug timings**: In addition to the existing Skia render and queue timing buckets, the composite pipeline should capture:
  - `ffmpeg.decode_ms` — time ffmpeg spends decoding input video frames (parsed from ffmpeg `-benchmark` or `stderr` progress output, or estimated from total wall time minus Skia render time).
  - `ffmpeg.encode_ms` — time spent encoding output frames (tracked via the writer thread's `ffmpeg.write` bucket, which measures stdin write latency as backpressure from the encoder).
  - `ffmpeg.filter_ms` — time spent in the filter_complex overlay operation (estimated as the gap between decode completion and encode start, if measurable).
  - `composite.total_ms` — **full-job wall time** from render start to final MP4 written. This includes everything: Skia drawing, ffmpeg decode, filter, encode, and file I/O. Per-frame Skia drawing time is already captured in the existing `frame.total` bucket (inherited from the transparent pipeline).
  - These are recorded in the `timing_summary.json` alongside the existing buckets (`frame.total`, `buffer.acquire_wait`, `queue.put_wait`, etc.). The phase is labeled `"phase_7"` to distinguish from transparent renders.

Key ffmpeg command structure:

```
ffmpeg -loglevel info \
  [hw_init_args...] \
  [-hwaccel ...] -ss <offset> -t <duration> -i <video.mp4> \
  -f rawvideo -s WxH -pix_fmt rgba -r <fps> -i pipe:0 \
  -filter_complex "<filter_string>" \
  -c:v <codec> -b:v <bitrate> [codec_args...] \
  -c:a copy \
  -movflags faststart \
  -y output.mp4
```

#### [MODIFY] `src-tauri/ovrley_core/src/encode/mod.rs`

Add:

```rust
pub mod ffmpeg_composite;
mod video_composite_pipeline;
```

#### [MODIFY] `src-tauri/ovrley_core/src/encode/video.rs`

Add a new `render_composite_video()` entry function that delegates to `video_composite_pipeline::render_composite_video_single()`. This is a **new function**, not a modification of `render_video()`.

#### [MODIFY] `src-tauri/ovrley_core/src/config/mod.rs`

Add optional fields to `SceneConfig`:

```rust
pub composite_video_path: Option<String>,
pub composite_bitrate: Option<String>,
pub composite_sync_offset: Option<f64>,
```

These use `#[serde(default)]` so existing configs are unaffected.

#### [MODIFY] `src-tauri/ovrley_core/src/commands/mod.rs`

In `backend_render()`: if `config.scene.composite_video_path` is `Some`, call `render_composite_video()` instead of `render_video()`.

### Manual Tests

1. Render with no video imported → existing transparent pipeline used (unchanged).
2. Render with video imported + H.264 CPU → produces a valid MP4 with overlay composited.
3. Verify audio from source video is preserved (`-c:a copy`).
4. Verify debug timing JSON written to `target/debug_render/phase_7`.
5. Render with NVENC (if available) → GPU-accelerated encoding works.
6. Cancel mid-render → cleanup works, no orphan files.

---

## Phase 6 — Frontend Render Integration

**Goal**: Wire the frontend render flow to pass composite-specific settings to the backend.

### Deliverables

#### [MODIFY] `app/src/api/renderVideo.jsx`

When `importedVideoPath` is set in the store:

- Set `config.scene.composite_video_path` to the imported path.
- Set `config.scene.composite_bitrate` to the selected bitrate (e.g., `"60M"`).
- Set `config.scene.composite_sync_offset` to the sync offset.
- Override `config.scene.ffmpeg.codec` with the selected MP4 codec.
- Override `config.scene.fps` with the imported video's FPS.
- Override export range: `start` = sync offset, `end` = sync offset + video duration.

#### [MODIFY] `app/src/hooks/useRenderWorkflow.js` (if exists) or equivalent

Ensure the render settings draft includes `exportBitrate` and that it's passed through to `renderVideo()`.

#### [MODIFY] `app/src/components/RenderProgressOverlay.jsx`

Update the progress display to say "Compositing Video" instead of "Exporting Overlay" when composite mode is active.

### Manual Tests

1. Full end-to-end: import activity → import video → adjust sync → click Render → select H.264 → set bitrate → Start Render → progress shown → MP4 produced.
2. Output MP4 plays correctly in VLC/QuickTime with overlay visible on top of video.
3. Audio from original video is present in output.
4. Cancel works correctly.
5. Error states handled gracefully (missing ffmpeg, corrupt video, etc.).

---

## Design Decisions (Resolved)

> [!NOTE]
> **Video resolution mismatch**: Show a warning when the imported video resolution differs from the overlay resolution. The user is responsible for adjusting either the video or the overlay resolution to match. No automatic scaling is performed — this keeps behavior predictable and avoids quality surprises.

> [!NOTE]
> **Audio handling**: Assume the MP4 has a single audio track. Use `-c:a copy` to passthrough. No special handling for multiple tracks or missing audio.

> [!NOTE]
> **Video container support**: The file picker accepts `.mp4`, `.mov`, and `.mkv`. FFmpeg and ffprobe handle all three identically — no code changes needed beyond the filter string. All three are supported from day one.

> [!WARNING]
> **Hardware codec testing**: GPU-accelerated codecs (NVENC, QSV, VideoToolbox, VA-API) can only be tested on machines with the corresponding hardware. The plan includes software fallbacks (libx264/libx265) that work everywhere, but HW codecs will need platform-specific testing.

> [!NOTE]
> **Widget Update Rate**: The pipeline supports a `widget_update_rate` (default 1). If set to $N > 1$, Skia only renders a new overlay frame every $N$ frames. For intermediate frames, the previous overlay buffer is re-written to the FFmpeg pipe. This preserves the background video's fluid native FPS while significantly reducing Skia rendering overhead.

> [!IMPORTANT]
> **Template persistence exclusions**: The following settings are **session-only** and must **NOT** be saved into template files:
>
> - `importedVideoPath` and all video import state
> - `videoSyncOffsetSeconds` (sync offset)
> - `composite_bitrate` (export bitrate for MP4)
> - `widget_update_rate` (ephemeral optimization)
> - MP4 codec selection (e.g., `libx264`, `h264_nvenc`)
>
> These are ephemeral, per-session values stored only in the Zustand slice (and optionally `localStorage` for convenience across page reloads). The `createTemplateSlice.js` serialization functions (`persistTemplateSettings`, `hydrateTemplateState`) must explicitly exclude them. The `SceneConfig` composite fields (`composite_video_path`, `composite_bitrate`, `composite_sync_offset`) are only injected at render time in `renderVideo.jsx` and never written to disk.

---

## File Change Summary

### New Files (8)

| File                                                           | Purpose                              |
| -------------------------------------------------------------- | ------------------------------------ |
| `app/src/store/slices/createVideoImportSlice.js`               | Zustand slice for video import state |
| `app/src/lib/videoMetadata.js`                                 | Video metadata extraction via Tauri  |
| `app/src/hooks/useVideoPreview.js`                             | Video preview sync hook              |
| `src-tauri/ovrley_core/src/encode/video_probe.rs`              | ffprobe wrapper                      |
| `src-tauri/ovrley_core/src/encode/codec_detect.rs`             | Codec/hwaccel detection              |
| `src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs`         | Composite ffmpeg settings builder    |
| `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs` | Composite render pipeline            |

### Modified Files (12)

| File                                                  | Change                               |
| ----------------------------------------------------- | ------------------------------------ |
| `app/src/store/useStore.js`                           | Add video import slice               |
| `app/src/api/backend.js`                              | Add `probeVideo()`, `detectCodecs()` |
| `app/src/api/renderVideo.jsx`                         | Add composite config fields          |
| `app/src/components/AppHeader.jsx`                    | Add "Import Video" button            |
| `app/src/components/SidebarSettingsTab.jsx`           | Add sync section, conditional range  |
| `app/src/components/OverlayPlayer.jsx`                | Video timeline highlight             |
| `app/src/components/overlay-editor/OverlayCanvas.jsx` | Video background layer               |
| `app/src/components/RenderVideoDialog.jsx`            | Grouped codecs, bitrate slider       |
| `src-tauri/ovrley_core/src/encode/mod.rs`             | Register new modules                 |
| `src-tauri/ovrley_core/src/encode/video.rs`           | Add `render_composite_video()` entry |
| `src-tauri/ovrley_core/src/config/mod.rs`             | Add composite fields to SceneConfig  |
| `src-tauri/ovrley_core/src/commands/mod.rs`           | Add probe/detect/composite commands  |
| `src-tauri/src/lib.rs`                                | Register new Tauri commands          |

### Untouched Sacred Files

- `src-tauri/ovrley_core/src/encode/video_pipeline.rs` — existing render pipeline
- `src-tauri/ovrley_core/src/encode/ffmpeg.rs` — existing ffmpeg settings (transparent codecs)
- `src-tauri/ovrley_core/src/encode/video_debug.rs` — existing debug utilities (reused, not modified)

---

## Appendix A — gopro-dashboard-overlay Builtin Profiles (Reference)

These profiles from [gopro-dashboard-overlay](https://github.com/time4tea/gopro-dashboard-overlay) are the starting point for our `ffmpeg_composite.rs` profile definitions. Each profile defines `input` (hwaccel/decode args), an optional `filter` (filter_complex string), and `output` (codec/encoding args).

### Profile: `nvgpu` — NVIDIA GPU (simple)

```json
{
  "input": ["-hwaccel", "nvdec"],
  "output": [
    "-vcodec",
    "h264_nvenc",
    "-rc:v",
    "cbr",
    "-b:v",
    "25M",
    "-bf:v",
    "3",
    "-profile:v",
    "high",
    "-spatial-aq",
    "true",
    "-movflags",
    "faststart"
  ]
}
```

| Argument              | Purpose                                                                        |
| --------------------- | ------------------------------------------------------------------------------ |
| `-hwaccel nvdec`      | Use NVIDIA hardware decoder for input video (CPU→GPU transfer for decode only) |
| `-vcodec h264_nvenc`  | Encode output with NVIDIA H.264 hardware encoder                               |
| `-rc:v cbr`           | Constant bitrate rate control — predictable file sizes                         |
| `-b:v 25M`            | Target bitrate 25 Mbps (we'll override with user slider value)                 |
| `-bf:v 3`             | 3 B-frames between reference frames — improves compression                     |
| `-profile:v high`     | H.264 High profile — best quality/compression ratio                            |
| `-spatial-aq true`    | Spatial adaptive quantization — allocates more bits to complex regions         |
| `-movflags faststart` | Moves MP4 moov atom to file start — enables progressive web playback           |

**Note**: No `filter` key — ffmpeg handles overlay compositing via default filter chain. The CPU performs the overlay operation. Good balance of simplicity and performance.

---

### Profile: `nnvgpu` — NVIDIA GPU (full hardware pipeline)

```json
{
  "input": ["-hwaccel", "cuda", "-hwaccel_output_format", "cuda"],
  "filter": "[0:v]scale_cuda=format=yuv420p[mp4_stream];[1:v]format=yuva420p,hwupload[overlay_stream];[mp4_stream][overlay_stream]overlay_cuda",
  "output": [
    "-vcodec",
    "h264_nvenc",
    "-rc:v",
    "cbr",
    "-b:v",
    "25M",
    "-bf:v",
    "3",
    "-profile:v",
    "main",
    "-spatial-aq",
    "true",
    "-movflags",
    "faststart"
  ]
}
```

| Argument                      | Purpose                                                              |
| ----------------------------- | -------------------------------------------------------------------- |
| `-hwaccel cuda`               | Use CUDA for hardware decoding (keeps frames in GPU memory)          |
| `-hwaccel_output_format cuda` | Keep decoded frames in CUDA device memory (zero-copy to encoder)     |
| `scale_cuda=format=yuv420p`   | Convert input video to yuv420p on GPU                                |
| `format=yuva420p,hwupload`    | Convert overlay to yuva420p (with alpha), upload to GPU              |
| `overlay_cuda`                | Perform overlay compositing entirely on GPU                          |
| `-profile:v main`             | H.264 Main profile (not High — required for some CUDA overlay paths) |

**Note**: This is the highest-performance path. Decode, overlay, and encode all happen on the GPU with no CPU↔GPU round-trips. Requires CUDA toolkit support in ffmpeg.

---

### Profile: `mac` — macOS VideoToolbox (H.264)

```json
{
  "input": ["-hwaccel", "videotoolbox"],
  "output": [
    "-vcodec",
    "h264_videotoolbox",
    "-b:v",
    "60M",
    "-movflags",
    "faststart"
  ]
}
```

| Argument                    | Purpose                                                                                                                                                                    |
| --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `-hwaccel videotoolbox`     | Use Apple VideoToolbox for hardware decoding                                                                                                                               |
| `-vcodec h264_videotoolbox` | Encode with Apple's hardware H.264 encoder                                                                                                                                 |
| `-b:v 60M`                  | Average bitrate mode (overridden by user slider). VT also supports `-q:v 1-100` for quality VBR, but we use `-b:v` to keep the bitrate slider universal across all codecs. |

**Note**: The original gopro-dashboard-overlay profile uses `-q:v 60` (quality-based VBR). We switch to `-b:v` so the same bitrate slider works for all codecs and aligns with YouTube's upload recommendations.

---

### Profile: `mac_hevc` — macOS VideoToolbox (HEVC)

```json
{
  "input": ["-hwaccel", "videotoolbox"],
  "output": [
    "-vcodec",
    "hevc_videotoolbox",
    "-b:v",
    "40M",
    "-movflags",
    "faststart"
  ]
}
```

Same as `mac` but using HEVC (H.265) encoder. Better compression at same quality. Uses `-b:v` (overridden by user slider).

---

### Profile: `qsv` — Intel Quick Sync Video

```json
{
  "input": [
    "-init_hw_device",
    "qsv=hw",
    "-hwaccel",
    "qsv",
    "-hwaccel_output_format",
    "qsv"
  ],
  "filter": "[0:v]hwupload=extra_hw_frames=64[main_hw];[1:v]hwupload=extra_hw_frames=64,format=qsv[overlay_hw];[main_hw][overlay_hw]overlay_qsv=x=0:y=0,hwdownload,format=nv12",
  "output": ["-vcodec", "hevc_qsv", "-global_quality", "25", "-c:a", "copy"]
}
```

| Argument                      | Purpose                                                                |
| ----------------------------- | ---------------------------------------------------------------------- |
| `-init_hw_device qsv=hw`      | Initialize Intel QSV hardware device                                   |
| `-hwaccel qsv`                | Use QSV for hardware decoding                                          |
| `-hwaccel_output_format qsv`  | Keep decoded frames in QSV device memory                               |
| `hwupload=extra_hw_frames=64` | Upload frames to QSV surface pool (64 extra frame slots for buffering) |
| `overlay_qsv=x=0:y=0`         | Perform overlay compositing on Intel GPU                               |
| `hwdownload,format=nv12`      | Download composited frame back to CPU in NV12 format                   |
| `-vcodec hevc_qsv`            | Encode with Intel QSV HEVC encoder                                     |
| `-global_quality 25`          | Quality-based encoding (1–51, lower = better quality)                  |
| `-c:a copy`                   | Passthrough audio without re-encoding                                  |

**Note**: The `hwdownload` step is required because QSV overlay output needs to be downloaded before re-encoding in some ffmpeg builds. This may cause a performance hit compared to the NVIDIA full-GPU path.

---

### Implementation Mapping

For our `ffmpeg_composite.rs`, each profile maps to a `CompositeProfile` struct:

```rust
struct CompositeProfile {
    input_args: Vec<String>,        // hwaccel flags before -i
    filter_complex: Option<String>, // if None, use default software overlay
    output_args: Vec<String>,       // codec, bitrate, quality flags
}
```

The profile is selected based on:

1. The codec chosen by the user in the render dialog (e.g., `h264_nvenc`)
2. The detected hardware capabilities from `codec_detect.rs`
3. A fallback to software overlay (`libx264`/`libx265` + CPU filter) if no hardware match

The **bitrate** (`-b:v`) is always overridden with the user's slider value. All codecs use `-b:v` for consistent behavior — including VideoToolbox, which supports average bitrate mode alongside its native `-q:v` quality mode.
