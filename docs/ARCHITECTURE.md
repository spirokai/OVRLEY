# OVRLEY — Architecture Guide

## 1. Project Overview

**OVRLEY** (`0.1.0`, GPL-3.0) is a cross-platform desktop application that turns `.fit` / `.gpx` GPS activity data (cycling, running, etc.) into **customizable video overlays**. Users load an activity file, arrange widgets (speed, heart rate, elevation, route map, time, gradient, etc.) on a scene canvas, optionally import a source video for compositing, and render the result as ProRes, QTRLE, or H.264/H.265 MP4.

**Key capabilities:**

- 10+ widget types: metric values (speed, HR, power, cadence, temperature, time, gradient), text labels, route map, elevation profile
- Widget editing: drag, resize, rotate, grid snap via `react-moveable`
- Activity parsing: FIT (via `fit-file-parser` JS library) and GPX (via DOMParser)
- Transparent overlay export (ProRes/QTRLE) for external compositing
- Direct MP4 compositing: overlay rendered over imported source video
- 100% offline, no cloud dependencies

---

## 2. Repository Structure

```
cyclemetry/
├── app/                          # React 19 + Vite frontend (JSX, no TypeScript)
│   ├── src/
│   │   ├── main.jsx              # Entry point
│   │   ├── App.jsx               # Shell composition
│   │   ├── index.css             # Tailwind CSS 4 + dark theme + custom fonts
│   │   ├── api/                  # Tauri IPC bridge + activity parsing
│   │   ├── store/                # Zustand state (4 slices)
│   │   ├── hooks/                # Shared hooks (selectors, workflows, lifecycle)
│   │   ├── lib/                  # Utility library (config, template, color, etc.)
│   │   ├── features/             # Feature modules (scene-settings, render-video)
│   │   └── components/           # React components
│   │       ├── ui/               # shadcn/ui primitives (Radix-based)
│   │       ├── overlay-editor/   # Editor canvas, moveable, widget renderers
│   │       └── ...               # Shell, player, sidebar, editors
│   ├── package.json
│   ├── vite.config.js
│   └── eslint.config.js
│
├── src-tauri/                    # Tauri v2 desktop shell
│   ├── src/
│   │   ├── main.rs               # Entry point
│   │   ├── lib.rs                # Tauri app setup, 18 IPC commands
│   │   └── video_server.rs       # Local HTTP server for video preview
│   ├── src/bin/                  # CLI tools (render, preview, validate)
│   ├── ovrley_core/              # Standalone Rust crate (~13,500 LOC)
│   │   └── src/
│   │       ├── activity/         # Activity parsing, trim, interpolation
│   │       ├── config/           # Template/render config schema
│   │       ├── commands/         # Backend command logic (non-Tauri)
│   │       ├── debug/            # Progress reporting & timing profiler
│   │       ├── render/           # Skia overlay rendering
│   │       │   └── widgets/      # Route, elevation, value widget drawing
│   │       └── encode/           # FFmpeg video encoding pipelines
│   ├── Cargo.toml
│   └── tauri.conf.json
│
├── templates/                    # Bundled template JSON files
├── fonts/                        # Bundled fonts (Evogria, Furore)
├── vendor/ffmpeg/                # FFmpeg binaries (downloaded by postinstall)
├── scripts/                      # Build & packaging scripts
├── .github/workflows/            # CI/CD (release, semantic-release)
└── .agents/                      # Agent development guides
```

---

## 3. Technology Stack

| Layer              | Technology        | Version                                     |
| ------------------ | ----------------- | ------------------------------------------- |
| Frontend framework | React             | 19.x                                        |
| Build tool         | Vite              | 7.x                                         |
| CSS                | Tailwind CSS      | 4.x                                         |
| State management   | Zustand           | 5.x (with `immer`, `subscribeWithSelector`) |
| UI primitives      | shadcn/ui + Radix | latest                                      |
| Desktop shell      | Tauri             | 2.9.x                                       |
| Rendering (Rust)   | skia-safe         | 0.75                                        |
| Video encoding     | FFmpeg            | 8.1+ (via subprocess)                       |
| Activity parsing   | fit-file-parser   | browser                                     |
| Drag/resize        | react-moveable    | latest                                      |
| Icons              | lucide-react      | latest                                      |
| Package manager    | pnpm              | 10.25.0                                     |
| Rust edition       | 2021              | 1.84.0                                      |
| Immutable updates  | immer             | via Zustand middleware                      |

---

## 4. Architecture Overview

OVRLEY follows a **two-process desktop architecture** via Tauri v2:

```
┌─────────────────────────────────────┐
│  Tauri Shell (WebView)              │
│  ┌───────────────────────────────┐  │
│  │  React 19 App                 │  │
│  │  ┌──────────┐ ┌────────────┐  │  │
│  │  │  Shell   │ │  Editor    │  │  │
│  │  │ (header, │ │ (canvas,   │  │  │
│  │  │  tabs,   │ │  moveable, │  │  │
│  │  │  dialogs)│ │  player)   │  │  │
│  │  └────┬─────┘ └─────┬──────┘  │  │
│  │       └──────┬──────┘         │  │
│  │              ▼                │  │
│  │    ┌─────────────────┐        │  │
│  │    │  Zustand Store  │        │  │
│  │    │  (4 slices)     │        │  │
│  │    └────────┬────────┘        │  │
│  │             │                 │  │
│  │    ┌────────▼────────┐        │  │
│  │    │  api/backend.js │        │  │
│  │    │  (Tauri IPC)    │        │  │
│  │    └────────┬────────┘        │  │
│  └─────────────┼─────────────────┘  │
└────────────────┼────────────────────┘
                 │ Tauri IPC (JSON)
┌────────────────▼─────────────────────┐
│  Rust Backend                        │
│  ┌──────────────────────────────┐    │
│  │  lib.rs (Tauri command       │    │
│  │  handler, 18 commands)       │    │
│  └──────────┬───────────────────┘    │
│             │                        │
│  ┌──────────▼───────────────────┐    │
│  │  ovrley_core crate           │    │
│  │  ┌────────┐ ┌─────────┐      │    │
│  │  │activity│ │ config  │      │    │
│  │  │ (parse,│ │ (schema)│      │    │
│  │  │ trim,  │ └────┬────┘      │    │
│  │  │ interp)│      │           │    │
│  │  └───┬────┘      │           │    │
│  │      └─────┬─────┘           │    │
│  │            ▼                 │    │
│  │  ┌───────────────────┐       │    │
│  │  │  render/ (Skia)   │       │    │
│  │  │  text, format,    │       │    │
│  │  │  widgets (route,  │       │    │
│  │  │  elevation, value)│       │    │
│  │  └────────┬──────────┘       │    │
│  │           │                  │    │
│  │  ┌────────▼──────────┐       │    │
│  │  │  encode/ (FFmpeg) │       │    │
│  │  │  video_pipeline,  │       │    │
│  │  │  composite, codec │       │    │
│  │  │  probe, debug     │       │    │
│  │  └───────────────────┘       │    │
│  └──────────────────────────────┘    │
│                                      │
│  ┌──────────────────────────────┐    │
│  │  video_server.rs             │    │
│  │  (Local HTTP server for      │    │
│  │   video preview in <video>)  │    │
│  └──────────────────────────────┘    │
└──────────────────────────────────────┘
```

### Process Model

- **Frontend (WebView):** React 19 app rendered in Tauri's WebView. Communicates with Rust via JSON-over-IPC (`invoke`).
- **Backend (Rust):** Tauri v2 process. Handles IPC commands, runs Skia overlay rendering, spawns FFmpeg subprocesses for encoding, serves video preview files via local HTTP.
- **FFmpeg subprocess:** Spawned by Rust for encoding. Raw RGBA frames piped via stdin for transparent codecs; two-input pipe + file for composite MP4.

### Key Architectural Decisions

1. **Skia for rendering, not HTML Canvas:** Overlays are rendered in Rust using Skia, mirroring what will be in the final video. The frontend's `WidgetPreview` components render SVG approximations for preview only.
2. **No TypeScript:** The frontend is plain JSX. Type documentation is via JSDoc.
3. **Widget rendering is duplicated:** The JSX SVG preview renderers (`widgetPreviewRenderers.jsx`) approximately match the Rust Skia renderers (`render/widgets/*.rs`). Minor discrepancies exist — the Rust output is authoritative.
4. **Cached static layers:** Labels and static widget backgrounds are rendered once and cached as Skia images. Only dynamic metric values + marker positions are redrawn per frame.
5. **Composite video timing is tricky:** Overlay FPS = source video FPS / update_rate. Timing mapping goes: overlay_frame -> video_local_time -> activity_time -> dense_frame_index.

---

## 5. Frontend Architecture

### 5.1 State Management (Zustand)

The Zustand store is composed of 4 slices, merged via `create()` with `immer` + `subscribeWithSelector` middleware:

```
useStore
├── createEditorSlice
│   # Config, preview playback, widget selection, autoRender, editor, selectedWidgetId, previewPlaybackState, config
├── createMediaSlice
│   # Activity, render status, errors, activitySummary, renderingVideo, errorMessage, renderProgress
├── createTemplateSlice
│   # Templates, export settings, global defaults, templates, loadedTemplateFilename, updateRate, exportRange, globalDefaults
└── createVideoImportSlice
    # Imported video metadata, sync offset, codecs, importedVideoPath, videoSyncOffsetSeconds, availableCodecs
```

Store access from components goes through selector hooks in `hooks/useAppStoreSelectors.js`, which use `useShallow` to prevent unnecessary re-renders.

### 5.2 Component Architecture

Components follow a **container/presentational pattern** via hooks:

```
App.jsx
├── useAppBootstrap         # On mount: fetch platform, templates, codecs
├── useActivityImport       # File input for .gpx/.fit parsing
├── useTemplateManagement   # CRUD lifecycle for templates
├── useRenderWorkflow       # Render dialog orchestration
├── useEditorShellState     # Zoom, grid, background mode
├── useVideoImport          # Video file import dialog
├── useBackendStatus        # Backend health polling (2s interval)
│
├── TitleBar                # Custom window decorations
├── AppHeader               # Main toolbar
├── ControlPanel            # Sidebar with Settings/Widgets tabs
│   ├── SidebarSettingsTab  # Scene settings, video sync, global defaults
│   └── SidebarWidgetsTab   # Widget quick-add + property editors
├── OverlayEditor           # Main editor canvas
│   ├── OverlayCanvas       # Scene compositing (grid, video bg, widgets)
│   ├── OverlayMoveable     # Drag/resize/rotate/scale wrapper
│   ├── WidgetPreview       # Routes to correct renderer by type
│   └── useOverlayEditorState  # Central editor state hook
├── OverlayPlayer           # Timeline playback controls
├── RenderVideoDialog       # Export settings + progress panel
├── ErrorAlert              # Toast error notification
└── LoadingOverlay          # Activity spinner
```

### 5.3 Feature Modules

Well-defined features with their own `data/`, `utils/`, `hooks/`, and `components/`:

- **`features/scene-settings/`** — Sidebar settings tab: aspect ratio, resolution, FPS, widget update rate, video sync offset, global font/color/opacity defaults.
- **`features/render-video/`** — Render dialog: codec selection (ProRes, QTRLE, H.264, H.265), hardware acceleration (NVENC, QSV, VAAPI, VideoToolbox, Vulkan), bitrate slider, FPS mode, export range, progress polling.

### 5.4 Widget System

Widgets are stored in the `config` object as arrays: `config.labels[]`, `config.values[]`, `config.plots[]`. Each widget has a unique ID format: `{label|value|plot}-{index}`.

**Widget CRUD** (`lib/widget-config.js`):

- `buildConfigWidgets` — creates flat widget list from config
- `groupWidgetsForSidebar` — groups by type
- `findWidgetById`, `updateWidgetInConfig`, `deleteWidgetInConfig`

**10 widget types** and their preview renderers (`components/widgetPreviewRenderers.jsx`):

| Widget Type                      | Renderer                                   | Editor                    |
| -------------------------------- | ------------------------------------------ | ------------------------- |
| text                             | `OverlayTextWidget`                        | `TextWidgetEditor`        |
| speed, heartrate, power, cadence | `OverlayMetricWidget`                      | `MetricWidgetEditor`      |
| time                             | `OverlayMetricWidget`                      | `TimeWidgetEditor`        |
| temperature                      | `OverlayMetricWidget`                      | `TemperatureWidgetEditor` |
| gradient                         | `OverlayMetricWidget` (triangle indicator) | `GradientWidgetEditor`    |
| route_map                        | `OverlayRouteWidget`                       | `RouteMapWidgetEditor`    |
| elevation                        | `OverlayElevationWidget`                   | `ElevationWidgetEditor`   |

### 5.5 Activity Parsing Pipeline

```
.gpx/.fit file
    │
    ▼
gpxUtils.jsx                        # File type detection + parser dispatch
    │
    ├── .fit → fitParserUtils.js    # fit-file-parser library
    └── .gpx → DOMParser            # Native XML parsing
    │
    ▼
activityParserUtils.js              # finalizeParsedActivity()
    ├── Build elapsed/distance/course series
    ├── Insert idle gap samples (stationary detection)
    ├── Compute metric series (speed from distance, gradient from elevation, etc.)
    └── Returns { parsedActivity, debugPayload }
    │
    ▼
activityMetricSeries.js             # deriveActivityMetricSeries()
    ├── Speed from distance
    ├── Gradient from elevation (Savitzky-Golay smoothing)
    ├── Heading from course
    ├── Pace from speed, vertical speed
    └── Torque from power/cadence
    │
    ▼
activityCache.js                       # In-memory cache
Zustand (activitySummary)
```

### 5.6 Video Preview System

Imported source videos are served by a custom local HTTP server (`video_server.rs`) for native `<video>` element playback:

```
User selects video
    │
    ▼
useVideoImport.js → backend.importPreviewVideo(path)
    │
    ▼
lib.rs: backend_import_preview_video
    ├── Assigns UUIDv4 import_id
    ├── Registers file in VideoServerHandle
    └── Returns preview_url (http://127.0.0.1:PORT/video/<uuid>)
    │
    ▼
useVideoPreview.jsx
    ├── Sets <video> src to preview_url
    ├── Handles play/pause/scrub via useVideoPlaybackClock
    ├── Syncs with activity timeline via videoSyncOffsetSeconds
    └── Debounces seek requests for performance
```

---

## 6. Backend Architecture

### 6.1 Tauri IPC Commands (18 total)

All defined in `lib.rs` and implemented in `ovrley_core/src/commands/mod.rs`:

| Command                        | Purpose                                       |
| ------------------------------ | --------------------------------------------- |
| `backend_health`               | Health check + FFmpeg path                    |
| `backend_current_os`           | OS string ("windows", "macos")                |
| `backend_list_system_fonts`    | Skia FontMgr font listing                     |
| `backend_render`               | Start video render (transparent or composite) |
| `backend_progress`             | Poll render progress                          |
| `backend_cancel`               | Cancel active render                          |
| `backend_open_downloads`       | Open output folder                            |
| `backend_open_video`           | Open specific output video                    |
| `backend_list_templates`       | List bundled + user templates                 |
| `backend_get_template`         | Read template JSON                            |
| `backend_probe_video`          | ffprobe metadata extraction                   |
| `backend_import_preview_video` | Register video for preview                    |
| `backend_clear_preview_video`  | Clear preview registration                    |
| `backend_get_video_state`      | Diagnostic server state                       |
| `backend_detect_codecs`        | Probe encoder availability                    |
| `default_template_save_path`   | User template path                            |
| `write_template_file`          | Write template JSON to disk                   |
| `write_parse_debug_file`       | Write debug file                              |

### 6.2 Render Pipeline (Skia → FFmpeg)

```
Render Request (config + activity JSON)
    │
    ▼
commands::backend_render
    ├── is_composite_render() ?
    │   ├── YES → derive_composite_render_plan()
    │   │        → apply_composite_scene_timing()
    │   │        → video::render_composite_video()
    │   │        → video_composite_pipeline::render_composite_video_single()
    │   │
    │   └── NO  → video::render_video()
    │             ├── should_parallelize_qtrle() ?
    │             │   ├── YES → segmented render with parallel threads
    │             │   │          → ffmpeg concat stitch
    │             │   └── NO  → video_pipeline::render_video_single()
    │             │
    │             ▼
    │         render_video_single()
    │
    ▼
Shared pipeline:
    ├── 1. Parse config & activity
    ├── 2. Trim activity to scene window (activity::trim)
    ├── 3. Densify activity to frame rate (activity::interpolate)
    ├── 4. Prepare Skia assets (render::prepare_preview_assets)
    │      ├── Cached labels image (static)
    │      ├── Route widget cache (LTTB downsampled + RDP simplified)
    │      └── Elevation widget cache (SG-smoothed + RDP simplified)
    ├── 5. Spawn FFmpeg subprocess
    ├── 6. Render loop: for each frame →
    │      ├── render_frame_rgba() → render_frame_to_surface()
    │      │   ├── Blit static labels layer
    │      │   ├── Draw metric values (icon + value + unit text)
    │      │   ├── Draw route widget (remaining + completed + marker)
    │      │   └── Draw elevation widget (remaining + completed + marker + labels)
    │      └── Write RGBA bytes to FFmpeg stdin
    ├── 7. Monitor FFmpeg progress (parse stderr for frame=)
    ├── 8. Wait for FFmpeg to finish
    └── 9. Validate output, write timing summary
```

### 6.3 FFmpeg Integration

**Discovery** (`encode/ffmpeg.rs`):

- Search order: `OVRLEY_FFMPEG` env → `FFMPEG_BINARY` env → `vendor/ffmpeg/bin/` → PATH

**Transparent encoding** (`encode/video_pipeline.rs`):

- Raw RGBA piped via stdin: `ffmpeg -f rawvideo -pix_fmt rgba -s WxH -r fps -i -`
- Encoder: ProRes (`prores_ks`, `prores_ks_vulkan`, `prores_videotoolbox`) or QTRLE (`qtrle`)
- Vulkan acceleration: `-init_hw_device vulkan=vk -filter_hw_device vk -vf format=yuva444p10le,hwupload`

**Composite MP4 encoding** (`encode/video_composite_pipeline.rs`):

- Two inputs: source video (file) + overlay (pipe:0 raw RGBA)
- Filter complex: `[0:v]setpts=PTS-STARTPTS,scale=W:H[base];[1:v]setpts=PTS-STARTPTS[ovr];[base][ovr]overlay=0:0:eof_action=repeat:shortest=1,format=yuv420p[out]`
- Output: H.264 (libx264) with `-movflags +faststart`, audio copied

**QTRLE parallelism** (`encode/video.rs`):

- For QTRLE codec with >= 2 second duration: split into second-aligned segments, render in parallel threads, stitch with ffmpeg concat demuxer.

**Codec detection** (`encode/codec_detect.rs`):

- Runs short ffmpeg encode tests for 20+ codec/hardware combinations.
- Reports booleans back to frontend for UI filtering.

### 6.4 Activity Processing (Rust Side)

The Rust backend receives already-parsed activity JSON from the frontend (the JS-side parser extracts raw samples). The Rust side does:

1. **Trim** (`activity/trim.rs`): Validate scene window, interpolate boundary samples, produce `TrimmedActivity` with scene-local timeline.
2. **Densify** (`activity/interpolate.rs`): Convert uneven samples into frame-aligned dense series using linear interpolation with edge clamping.
3. **Report**: `DenseActivityReport` with per-frame telemetry for every scene frame.

### 6.5 Widget Rendering (Skia)

**Text** (`render/text.rs`):

- Font resolution: bundled path → directories → system FontMgr → fallback
- Font cache: process-lifetime `OnceLock<Mutex<HashMap>>` by family name
- Text drawing: shadow (drop-shadow filter) → stroke (border) → fill

**Route** (`render/widgets/route.rs`):

1. Project GPS coords to 2D via equirectangular projection at mean latitude
2. Downsample via LTTB (Largest-Triangle-Three-Buckets)
3. Simplify via RDP (Ramer-Douglas-Peucker)
4. Cache "remaining route" layer as Skia Image (unchanged across frames)
5. Per frame: draw remaining route → completed route prefix → marker circle

**Elevation** (`render/widgets/elevation.rs`):

1. Smooth via Savitzky-Golay filter (11-point kernel)
2. Project (normalize + y_scale + fit to widget bounds)
3. Simplify via RDP with preserved min/max points
4. Cache "remaining" area + line as Skia Image
5. Per frame: draw remaining → completed area fill → completed line → marker → metric/imperial labels

**Metric Values** (`render/widgets/value.rs`):

- SVG icons parsed from `include_str!` embedded SVGs (path, line, circle primitives)
- Gradient type: triangle indicator showing slope direction + magnitude
- Value + unit text layout with shadow/border

---

## 7. Data Flow & Key Workflows

### 7.1 Activity Import → Preview

```
[User clicks "Open Activity"]
    │
    ▼
File picker (.gpx/.fit)
    │
    ▼
gpxUtils.jsx → activityParserUtils → activityMetricSeries
    │
    ▼
Zustand: activitySummary set
    │
    ▼
Rust: backend_render (preview mode)
    ├── trim + densify activity
    └── Skia renders preview PNG → returned to frontend
    │
    ▼
OverlayCanvas displays widget previews
OverlayPlayer enables timeline scrubbing
```

### 7.2 Widget Editing

```
[User drags widget on canvas]
    │
    ▼
OverlayMoveable → onDrag
    │
    ▼
useOverlayEditorState → draft state (ref + live styles)
    │
    ▼
[drag ends] → commitWidgetMove()
    │
    ▼
Zustand: config updated via updateWidgetInConfig
    │
    ▼
OverlayCanvas re-renders with new widget position
```

### 7.3 Template Lifecycle

```
[Load app]
    │
    ▼
backend_list_templates → bundled/user scan → deduplicate → rendered in AppHeader dropdown
    │
    ▼
[Select template] → backend_get_template → parse JSON
    │
    ▼
hydrateTemplateState() → createTemplateState() → normalizeTemplateConfig()
    │
    ▼
Zustand: config, globalDefaults, export settings populated
    │
    ▼
OverlayEditor renders all widgets from config
```

### 7.4 Video Render Export

```
[User clicks "Render"]
    │
    ▼
RenderVideoDialog opens → user selects codec, bitrate, FPS, etc.
    │
    ▼
[User clicks "Start Render"]
    │
    ▼
renderVideo.jsx → build final config
    ├── Apply global defaults
    ├── Override with export settings (FPS, update rate, codec)
    ├── Set composite_video_path if MP4 mode
    └── Invoke backend.renderVideo(config, activity)
    │
    ▼
Rust: backend_render command
    ├── derive_composite_render_plan() (if composite)
    ├── trim + densify activity at output FPS
    ├── Render loop → pipe frames to FFmpeg
    └── Monitor progress
    │
    ▼
Frontend polls backend_progress every 500ms
    │
    ▼
RenderProgressPanel shows progress bar + ETA
    │
    ▼
On completion: backend_open_video → OS file manager opens output
```

### 7.5 Video Import + Sync

```
[User imports video]
    │
    ▼
VideoServerHandle.set_video(path)
    ├── Starts HTTP server on random port (if not running)
    └── Returns preview URL
    │
    ▼
useVideoPreview sets <video src>
├── useVideoPlaybackClock drives timeline
└── Video sync: compares video creation_time vs activity timestamps
    ├── computeVideoSync() → auto-sets videoSyncOffsetSeconds
    └── During render: composite_video_path + sync_offset applied
```

---

## 8. Configuration & Build

### 8.1 Dev Commands

```bash
pnpm dev              # Full dev: Vite + Tauri window
pnpm dev:frontend     # Vite dev server only (port 5173)
pnpm build            # Production build (Tauri + portable ZIP)
pnpm lint             # ESLint (flat config, Prettier integration)
pnpm format           # Prettier
pnpm release          # semantic-release
```

### 8.2 Environment Variables

- `OVRLEY_FFMPEG` / `FFMPEG_BINARY` — override FFmpeg binary path
- `OVRLEY_SAMPLE_FRAMES` — enables sample frame PNG generation during render (debug)
- `ovrley:store-devtools` — localStorage key to enable Zustand devtools

### 8.3 Build Pipeline

```
pnpm install
    ├── postinstall: scripts/install-ffmpeg.mjs downloads FFmpeg 8.1+ to vendor/ffmpeg/
    └── pnpm --filter app install

pnpm tauri build
    ├── Vite builds app/ → app/dist/
    ├── Cargo builds src-tauri/ (Rust compilation)
    └── Tauri bundles: NSIS/MSI (Windows), DMG (macOS)
        └── scripts/package-portable.mjs: creates standalone ZIP
            ├── OVRLEY binary
            ├── vendor/ffmpeg/
            ├── fonts/
            ├── templates/
            └── THIRD_PARTY_NOTICES.txt
```

### 8.4 CI/CD

- **Release workflow** (manual trigger): Builds Windows + macOS bundles, creates GitHub release with artifacts.
- **Semantic-release workflow**: Auto-tags pushes to `skia-render-backend` branch, no build step.

---

## 9. Template System

Templates are JSON files following the `ovrley-template` format (v1):

```json
{
  "format": "ovrley-template",
  "version": 1,
  "name": "Template Name",
  "savedAt": "ISO-8601",
  "config": {
    "scene": { "width": 1920, "height": 1080, "fps": 30, "start": 0, "end": 3600, ... },
    "labels": [{ "key": "text", "text": "...", "x": 100, "y": 200, ... }],
    "values": [{ "key": "speed", "x": 100, "y": 300, ... }],
    "plots": [{ "type": "route_map", ... }, { "type": "elevation", ... }]
  },
  "settings": {
    "globalDefaults": { "font", "color", "opacity", "scale", ... },
    "updateRate", "exportRange", "exportCodec", "aspectRatio"
  }
}
```

**Resolution:** Built-in templates in `templates/`, user templates in `Documents/OVRLEY/`. Deduplication by filename (user wins over built-in).

---

## 10. Complete Directory Map

```
H:\tools\cyclemetry\
│
├── app/
│   ├── src/
│   │   ├── main.jsx                           Entry point
│   │   ├── App.jsx                            Shell composition
│   │   ├── index.css                          Global styles + theme
│   │   │
│   │   ├── api/
│   │   │   ├── backend.js                     Tauri IPC bridge (18 commands)
│   │   │   ├── gpxUtils.jsx                   Activity file parse orchestrator
│   │   │   ├── fitParserUtils.js              FIT file parser
│   │   │   ├── activityParserUtils.js         Activity data finalization
│   │   │   ├── activityMetricSeries.js        Metric derivation
│   │   │   ├── activityGapUtils.js            Idle gap filling
│   │   │   ├── activityCache.js               In-memory cache
│   │   │   └── renderVideo.jsx                Render payload builder
│   │   │
│   │   ├── store/
│   │   │   ├── useStore.js                    Zustand store creation
│   │   │   ├── store-utils.js                 Persistence/serialization helpers
│   │   │   └── slices/
│   │   │       ├── createEditorSlice.js       Editor config, playback, selection
│   │   │       ├── createMediaSlice.js        Activity, render status, errors
│   │   │       ├── createTemplateSlice.js     Template CRUD, export settings
│   │   │       └── createVideoImportSlice.js  Video import metadata, sync
│   │   │
│   │   ├── hooks/
│   │   │   ├── useAppStoreSelectors.js        Centralized shallow selectors
│   │   │   ├── useAppBootstrap.js             Init: platform, templates, codecs
│   │   │   ├── useActivityImport.js           Activity file picker
│   │   │   ├── useBackendStatus.js            Health polling
│   │   │   ├── useEditorShellState.js         Zoom, grid, background mode
│   │   │   ├── useTemplateManagement.js       Template lifecycle
│   │   │   ├── useVideoImport.js              Video import workflow
│   │   │   ├── useVideoPreview.js             <video> element sync
│   │   │   ├── useVideoPlaybackClock.js       Frame scheduling
│   │   │   ├── useAvailableFonts.js           System font listing
│   │   │   ├── use-isomorphic-layout-effect.js
│   │   │   ├── use-lazy-ref.js
│   │   │   └── use-as-ref.js
│   │   │
│   │   ├── lib/
│   │   │   ├── utils.js                       cn() class merge
│   │   │   ├── compose-refs.js                React 19 ref composition
│   │   │   ├── color-utils.js                 Hex color normalization
│   │   │   ├── config-utils.js                Config resolution + defaults
│   │   │   ├── template-snapshot.js           Template serialize/deserialize
│   │   │   ├── widget-config.js               Widget CRUD utilities
│   │   │   ├── update-rate.js                 FPS divisor math
│   │   │   ├── export-range.js                Time range helpers
│   │   │   ├── theme.js                       CSS variable access
│   │   │   ├── fonts.js                       Font naming utilities
│   │   │   ├── previewPerf.js                 Dev perf counters
│   │   │   └── bitrateDefaults.js             Bitrate presets by resolution
│   │   │
│   │   ├── features/
│   │   │   ├── scene-settings/
│   │   │   │   ├── index.js                   Public API exports
│   │   │   │   ├── data/sceneSettingsConstants.js
│   │   │   │   ├── utils/sceneSettingsUtils.js
│   │   │   │   ├── hooks/useSceneSettingsState.js
│   │   │   │   └── components/
│   │   │   │       ├── SidebarSettingsTab.jsx
│   │   │   │       ├── OverlaySettingsSection.jsx
│   │   │   │       ├── VideoSyncSection.jsx
│   │   │   │       └── GlobalSettingsSection.jsx
│   │   │   │
│   │   │   └── render-video/
│   │   │       ├── index.js                   Public API exports
│   │   │       ├── data/renderConstants.js    Codec/format definitions
│   │   │       ├── utils/
│   │   │       │   ├── codecUtils.js          Codec selection logic
│   │   │       │   └── format.js              Time formatting
│   │   │       ├── hooks/
│   │   │       │   ├── useRenderWorkflow.js
│   │   │       │   ├── useRenderDialogState.js
│   │   │       │   ├── useRenderVideoDialogState.js
│   │   │       │   ├── useRenderVideoDerivedState.js
│   │   │       │   ├── useRenderVideoEffects.js
│   │   │       │   ├── useRenderProgressPolling.js
│   │   │       │   └── useRenderCompletion.js
│   │   │       └── components/
│   │   │           ├── RenderVideoDialog.jsx
│   │   │           ├── RenderProgressPanel.jsx
│   │   │           └── ExportRangeSettings.jsx
│   │   │
│   │   └── components/
│   │       ├── ui/                            shadcn/ui primitives (20 files)
│   │       ├── overlay-editor/
│   │       │   ├── useOverlayEditorState.js
│   │       │   ├── useWidgetDraftState.js
│   │       │   ├── createOverlayMoveableHandlers.js
│   │       │   ├── createOverlayPointerHandlers.js
│   │       │   ├── overlayEditorHelpers.js
│   │       │   ├── utils.js
│   │       │   ├── geometryUtils.js           Route/elevation geometry math
│   │       │   ├── metricTextUtils.js         Canvas2D text measurement
│   │       │   ├── metricWidgetPreviewModel.js
│   │       │   ├── metricWidgetAssets.js
│   │       │   ├── previewInterpolation.js
│   │       │   ├── constants.js
│   │       │   ├── OverlayEditor.jsx
│   │       │   ├── OverlayCanvas.jsx
│   │       │   ├── OverlayMoveable.jsx
│   │       │   ├── WidgetPreview.jsx
│   │       │   └── widgetPreviewRenderers.jsx SVG widget renderers (~1040 LOC)
│   │       ├── OverlayPlayer.jsx
│   │       ├── AppHeader.jsx
│   │       ├── TitleBar.jsx
│   │       ├── ControlPanel.jsx
│   │       ├── ErrorAlert.jsx
│   │       ├── LoadingOverlay.jsx
│   │       ├── NewTemplateConfirmDialog.jsx
│   │       ├── ActivitySection.jsx
│   │       ├── SidebarWidgetsTab.jsx
│   │       ├── widgetFormControls.jsx         Shared form field components
│   │       ├── widgetEditorSections.jsx       Font/Position/Dimensions/Icon sections
│   │       ├── widgetDefinitions.js           Widget type config + defaults
│   │       ├── TextWidgetEditor.jsx
│   │       ├── MetricWidgetEditor.jsx
│   │       ├── TimeWidgetEditor.jsx
│   │       ├── TemperatureWidgetEditor.jsx
│   │       ├── GradientWidgetEditor.jsx
│   │       ├── RouteMapWidgetEditor.jsx
│   │       └── ElevationWidgetEditor.jsx
│   │
│   ├── index.html
│   ├── package.json
│   ├── vite.config.js
│   ├── eslint.config.js
│   └── jsconfig.json
│
├── src-tauri/
│   ├── src/
│   │   ├── main.rs                            Entry point (6 lines)
│   │   ├── lib.rs                             Tauri setup + 18 IPC commands
│   │   └── video_server.rs                    Local HTTP preview server
│   │
│   ├── src/bin/
│   │   ├── render_video.rs                    CLI video renderer
│   │   ├── render_preview.rs                  CLI preview PNG generator
│   │   ├── validate_activity.rs               CLI activity validator
│   │   └── parallel_render.rs                 Diagnostic parallel benchmark
│   │
│   ├── ovrley_core/src/
│   │   ├── lib.rs                             Crate root (re-exports)
│   │   │
│   │   ├── activity/
│   │   │   ├── mod.rs                         Parse + build_dense_activity_report
│   │   │   ├── schema.rs                      ParsedActivity, DenseActivityReport
│   │   │   ├── trim.rs                        Scene window trim + boundary interp
│   │   │   └── interpolate.rs                 Frame-rate densification
│   │   │
│   │   ├── config/mod.rs                      RenderConfig schema + validation
│   │   ├── commands/mod.rs                    Backend command implementations
│   │   ├── debug/mod.rs                       RenderProgress + RenderProfiler
│   │   │
│   │   ├── render/
│   │   │   ├── mod.rs                         prepare_preview_assets, render_frame_rgba
│   │   │   ├── surface.rs                     Skia surface create/wrap/encode
│   │   │   ├── text.rs                        Font resolution + text drawing
│   │   │   ├── format.rs                      Metric formatting + unit conversion
│   │   │   └── widgets/
│   │   │       ├── mod.rs                     prepare_render_assets (re-exports)
│   │   │       ├── types.rs                   Cache types + geometry types
│   │   │       ├── common.rs                  Polyline, area, marker, transform
│   │   │       ├── value.rs                   Metric icons + gradient triangle
│   │   │       ├── route.rs                   Route map (LTTB + RDP)
│   │   │       └── elevation.rs               Elevation profile (SG + RDP)
│   │   │
│   │   └── encode/
│   │       ├── mod.rs                         Sub-module organization
│   │       ├── fps.rs                         Rational FPS type
│   │       ├── ffmpeg.rs                      Binary discovery + codec settings
│   │       ├── ffmpeg_composite.rs            Composite MP4 FFmpeg args
│   │       ├── codec_detect.rs                Encoder availability probing
│   │       ├── video_probe.rs                 ffprobe metadata extraction
│   │       ├── video.rs                       RenderController + orchestration
│   │       ├── video_pipeline.rs              Single-pass render loop
│   │       ├── video_composite_pipeline.rs    Composite render loop
│   │       └── video_debug.rs                 Debug artifacts
│   │
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── capabilities/default.json
│
├── templates/                                 11 bundled template JSON files
├── fonts/                                     Bundled fonts (Evogria, Furore)
├── vendor/ffmpeg/                             FFmpeg binaries (auto-downloaded)
├── scripts/
│   ├── tauri.mjs                              Tauri build wrapper
│   ├── package-portable.mjs                   Portable ZIP packaging
│   └── install-ffmpeg.mjs                     FFmpeg download postinstall
│
├── .github/workflows/
│   ├── release.yml                            Manual release build
│   └── semantic-release.yml                   Auto-tag on branch push
│
├── .agents/
│   ├── AGENTS.md                              Agent development guide
│   └── refactor-guide.md                      React refactoring standards
│
├── package.json                               Workspace root
├── pnpm-workspace.yaml                        pnpm workspace config
├── .releaserc.json                            semantic-release config
├── .tool-versions                             Runtime version pinning
└── README.md
```

---

## 11. Key Patterns & Conventions

### Frontend

- **No TypeScript** — JSDoc for type documentation on exported functions
- **Zustand** — global state with Immer, `subscribeWithSelector`, 4 slices, selector hooks with `useShallow`
- **Container hooks** — extract store access, side effects, and derived state from components
- **Presentational components** — receive grouped props, minimal logic
- **shadcn/ui** — Radix-based primitives in `components/ui/`
- **Feature folders** — `data/` (constants), `utils/` (pure functions), `hooks/`, `components/`

### Rust

- **skia-safe 0.75** — `binary-cache` feature enabled
- **Process-lifetime caches** — `OnceLock<Mutex<HashMap>>` for fonts, label images
- **Render loop** — acquires buffer from pool, renders RGBA, queues to FFmpeg writer thread
- **Composite pipeline** — two-input FFmpeg: source video file + raw pipe overlay
- **Parallel QTRLE** — segment-based parallelism with FFmpeg concat stitch
- **FFmpeg subprocess** — rawvideo via stdin, stderr parsing for progress

### Data Formats

- **Templates** — JSON `ovrley-template` v1 format
- **Activity** — JSON serialized from JS parser, Rust-side trim + densify
- **Frames** — raw RGBA (u8, 4 bytes/pixel) between Rust and FFmpeg
- **Preview** — PNG via Skia encode, returned as base64 over IPC

---

## 12. Known Architectural Notes

1. **Widget rendering is duplicated** — JSX SVG preview vs. Rust Skia render. Expect minor visual discrepancies. Rust output is authoritative.
2. **Composite timing is the most complex part** — involves mapping between 3 time domains: video time, activity time, and overlay frame index.
3. **QTRLE parallel render** — only activates for >= 2 second integer-second durations, uses `logical_cores / 4` workers.
4. **No test framework for frontend** — all testing is manual. Rust has some unit tests in `encode/tests/` and `commands/tests/`.
5. **Browser fallback** — the frontend has a fallback path for running outside Tauri (browser dev mode), using local file APIs instead of Tauri IPC.
6. **CSS zoom** — the editor shell supports zoom via `--app-scale` CSS variable (0.35x–4x).
