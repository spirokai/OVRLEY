# MP4 Compositing — Phase Implementation Prompts

Use these prompts to trigger implementation of each phase. Copy the relevant phase prompt into a new conversation.

---

## Phase 1 — Video Import & Store State

```
Implement Phase 1 of the MP4 compositing feature in full scope as defined in @mp4-plan.md (section "Phase 1 — Video Import & Store State").

Also read @mp4-compositing.md for full project context.

Key rules:
- Do NOT modify any existing files unless the plan explicitly marks them as [MODIFY].
- Do NOT touch the existing transparent overlay export pipeline (video_pipeline.rs, ffmpeg.rs, video_debug.rs are sacred).
- Video import state (path, duration, fps, resolution, creation_time, sync offset) is SESSION-ONLY — never persisted to template files.
- When a video is imported, its FPS overrides config.scene.fps and the framerate selector must be disabled.
- File picker must accept .mp4, .mov, .mkv.
- The ffprobe command should extract creation_time from multiple sources in priority order: format.tags.creation_time, streams[0].tags.creation_time, format.tags.com.apple.quicktime.creationdate.

Deliverables:
1. [NEW] app/src/store/slices/createVideoImportSlice.js — Zustand slice
2. [MODIFY] app/src/store/useStore.js — register the new slice
3. [NEW] app/src/lib/videoMetadata.js — frontend utility calling backend_probe_video
4. [NEW] src-tauri/ovrley_core/src/encode/video_probe.rs — ffprobe wrapper
5. [MODIFY] src-tauri/ovrley_core/src/encode/mod.rs — add pub mod video_probe
6. [MODIFY] src-tauri/ovrley_core/src/commands/mod.rs — add backend_probe_video function
7. [MODIFY] src-tauri/src/lib.rs — register backend_probe_video command
8. [MODIFY] app/src/api/backend.js — add probeVideo(filePath)
9. [MODIFY] app/src/components/AppHeader.jsx — add "Import Video" button with Film icon

After implementation, verify the Rust code compiles with `cargo check` from src-tauri/.
```

---

## Phase 2 — Video Time Sync

```
Implement Phase 2 of the MP4 compositing feature in full scope as defined in @mp4-plan.md (section "Phase 2 — Video Time Sync").

Also read @mp4-compositing.md for full project context.

Key rules:
- Do NOT modify any existing files unless the plan explicitly marks them as [MODIFY].
- Sync offset is SESSION-ONLY — never saved to template files.
- Auto-sync algorithm: compare video creation_time to activitySummary.startTime/endTime. If creation_time is unknown OR outside [activityStart, activityEnd ], set offset to 0 (place video at activity start) and show a warning. Otherwise compute offsetSeconds = (videoStart - activityStart) / 1000.
- The "Video Sync" section in SidebarSettingsTab must show: video info block (duration, fps, resolution), detected creation time (or "Unknown"), warning if applicable, offset input accepting both seconds and timecode formats, and a "Reset Sync" button.
- When video is imported: hide ExportRangeSettings, disable framerate selector with "Locked to video FPS" note.

Deliverables:
1. [MODIFY] app/src/store/slices/createVideoImportSlice.js — add computeVideoSync(activitySummary) action
2. [MODIFY] app/src/components/SidebarSettingsTab.jsx — add "Video Sync" section, hide export range, disable FPS selector

Do not modify any backend Rust files in this phase.
```

---

## Phase 3 — Video Preview in Editor & Player

```
Implement Phase 3 of the MP4 compositing feature in full scope as defined in @mp4-plan.md (section "Phase 3 — Video Preview in Editor & Player").

Also read @mp4-compositing.md for full project context.

Key rules:
- Do NOT modify any existing files unless the plan explicitly marks them as [MODIFY].
- Use Tauri's convertFileSrc (asset protocol) to create a src URL for the <video> element.
- The video seeks to previewSecond + videoSyncOffsetSeconds on scrub.
- Video plays/pauses in sync with the OverlayPlayer playback state.
- The video replaces the canvas background (checker/black/white) when imported.
- Add a 'video' option to the background mode toggle in AppHeader, auto-selected on import.

Deliverables:
1. [MODIFY] app/src/components/overlay-editor/OverlayCanvas.jsx — render <video> behind widget layer
2. [NEW] app/src/hooks/useVideoPreview.js — manages video element, seeking, play/pause sync
3. [MODIFY] app/src/components/OverlayPlayer.jsx — add highlighted region on slider for video coverage
4. [MODIFY] app/src/components/AppHeader.jsx — add 'video' background mode option

Do not modify any backend Rust files in this phase.
```

---

## Phase 4 — Codec Detection & Render Dialog

```
Implement Phase 4 of the MP4 compositing feature in full scope as defined in @mp4-plan.md (section "Phase 4 — Codec Detection & Render Dialog").

Also read @mp4-compositing.md for full project context.

Key rules:
- Do NOT modify any existing files unless the plan explicitly marks them as [MODIFY].
- Do NOT touch the existing transparent overlay export pipeline.
- Codec detection runs `ffmpeg -encoders` and `ffmpeg -hwaccels`, parses text output.
- Codec/bitrate settings are SESSION-ONLY — never saved to template files.
- RenderVideoDialog codec selector must have two <SelectGroup> sections: "Transparent Codecs" (existing, disabled when video imported) and "MP4 Codecs" (new, disabled when no video imported).
- Unavailable codecs shown greyed out with "Not available on this system".
- Bitrate slider (20-100 Mbps) visible only when MP4 codec selected.
- Bitrate defaults defined in app/src/lib/bitrateDefaults.js using the BITRATE_BINS config from the plan. Use getDefaultBitrate(width, height, fps, codecName) for lookup.
- All codecs use -b:v (bitrate) for rate control — including VideoToolbox, which supports average bitrate mode. This keeps the bitrate slider universal.

Deliverables:
1. [NEW] src-tauri/ovrley_core/src/encode/codec_detect.rs — codec/hwaccel detection
2. [MODIFY] src-tauri/ovrley_core/src/encode/mod.rs — add pub mod codec_detect
3. [MODIFY] src-tauri/ovrley_core/src/commands/mod.rs — add backend_detect_codecs
4. [MODIFY] src-tauri/src/lib.rs — register backend_detect_codecs command
5. [MODIFY] app/src/api/backend.js — add detectCodecs()
6. [MODIFY] app/src/store/slices/createVideoImportSlice.js — add availableCodecs state
7. [NEW] app/src/lib/bitrateDefaults.js — BITRATE_BINS config + getDefaultBitrate()
8. [MODIFY] app/src/components/RenderVideoDialog.jsx — grouped codecs, bitrate slider

After implementation, verify the Rust code compiles with `cargo check` from src-tauri/.
```

---

## Phase 5 — MP4 Compositing FFmpeg Pipeline

```
Implement Phase 5 of the MP4 compositing feature in full scope as defined in @mp4-plan.md (section "Phase 5 — MP4 Compositing FFmpeg Pipeline (Backend)").

Also read @mp4-compositing.md for full project context. Pay special attention to Appendix A in mp4-plan.md which contains the annotated gopro-dashboard-overlay builtin profiles — use these as the starting point for profile definitions.

Key rules:
- CRITICAL: All new code must be in SEPARATE FILES. Do NOT modify video_pipeline.rs, ffmpeg.rs, or video_debug.rs.
- The compositing pipeline uses ffmpeg with two inputs: Input 0 = imported MP4 (decoded by ffmpeg), Input 1 = pipe:0 stdin (raw RGBA overlay frames from Skia).
- Use filter_complex for overlay compositing.
- Implement profiles matching Appendix A: nvgpu, nnvgpu, mac, mac_hevc, qsv, plus software fallbacks (libx264/libx265 with CPU filter).
- Apply -ss and -t to input 0 for sync offset.
- Always append -movflags faststart and -c:a copy.
- All codecs use -b:v (bitrate) for rate control, including VideoToolbox. The bitrate slider is universal.
- **Frame rendering loop**:
  - Iterates through every frame of the output video (matching background FPS).
  - Implement `widget_update_rate` logic: only call `render_frame_rgba` every $N$ frames.
  - Re-use/re-pipe the previous frame buffer for intermediate writes to `pipe:0`.
- Debug timings must include: ffmpeg.decode_ms, ffmpeg.encode_ms, ffmpeg.filter_ms, composite.total_ms, plus all existing frame.total and queue buckets. Write to target/debug_render/phase_7.
- The render_composite_video() function in video.rs is a NEW function, not a modification of render_video().
- Add composite fields to SceneConfig: composite_video_path, composite_bitrate, composite_sync_offset, widget_update_rate (all Option with #[serde(default)]).
- In backend_render(): if composite_video_path is Some, call render_composite_video() instead of render_video().

Deliverables:
1. [NEW] src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs — composite profile builder
2. [NEW] src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs — composite render pipeline
3. [MODIFY] src-tauri/ovrley_core/src/encode/mod.rs — register new modules
4. [MODIFY] src-tauri/ovrley_core/src/encode/video.rs — add render_composite_video() entry
5. [MODIFY] src-tauri/ovrley_core/src/config/mod.rs — add composite fields to SceneConfig
6. [MODIFY] src-tauri/ovrley_core/src/commands/mod.rs — branch on composite_video_path

After implementation, verify the Rust code compiles with `cargo check` from src-tauri/.
```

---

## Phase 6 — Frontend Render Integration

```
Implement Phase 6 of the MP4 compositing feature in full scope as defined in @mp4-plan.md (section "Phase 6 — Frontend Render Integration").

Also read @mp4-compositing.md for full project context.

Key rules:
- Do NOT modify any existing files unless the plan explicitly marks them as [MODIFY].
- Composite config fields (composite_video_path, composite_bitrate, composite_sync_offset) are ONLY injected at render time in renderVideo.jsx — never written to disk or template files.
- When importedVideoPath is set in the store, renderVideo.jsx must:
  - Set config.scene.composite_video_path to the imported path
  - Set config.scene.composite_bitrate to selected bitrate (e.g. "60M")
  - Set config.scene.composite_sync_offset to the sync offset
  - Set config.scene.widget_update_rate to the store's value (default 1)
  - Override config.scene.ffmpeg.codec with the selected MP4 codec
  - Override config.scene.fps with the imported video's FPS
  - Override export range: start = sync offset, end = sync offset + video duration

Deliverables:
1. [MODIFY] app/src/api/renderVideo.jsx — inject composite config at render time
2. [MODIFY] app/src/hooks/useRenderWorkflow.js (or equivalent) — pass exportBitrate through
3. [MODIFY] app/src/components/RenderProgressOverlay.jsx — show "Compositing Video" label when composite

Do not modify any backend Rust files in this phase.
```
