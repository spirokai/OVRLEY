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
│   ├── build.rs                  # Windows msvcprt link + Tauri build
│   ├── src/
│   │   ├── main.rs               # Binary entry point
│   │   ├── lib.rs                # Tauri app wiring, 18 IPC commands, BackendState
│   │   ├── bin_common.rs          # Shared CLI argument parsing
│   │   ├── benchmark_common.rs    # Shared benchmark metrics printing
│   │   ├── tauri_commands.rs      # Tauri #[command] wrappers → ovrley_core
│   │   ├── file_ops.rs            # Template file read/write commands
│   │   ├── preview_import.rs      # Video preview import logic
│   │   ├── runtime_paths.rs       # Platform resource path resolution
│   │   ├── video_server.rs        # Local HTTP server for video preview
│   │   └── bin/                   # CLI tools
│   │       ├── render_video.rs    #   Video render
│   │       ├── render_preview.rs  #   Preview PNG generation
│   │       ├── validate_activity.rs # Activity validation
│   │       ├── parallel_render.rs #   Diagnostic parallel benchmark
│   │       ├── benchmark_widget_rate.rs | Benchmark: widget update rates
│   │       ├── benchmark_transparent.rs| Benchmark: transparent codecs
│   │       └── benchmark_composite.rs  | Benchmark: composite codecs
│   │
│   ├── tests/
│   │   └── video_server_tests.rs  # Video server integration tests
│   │
│   ├── ovrley_core/               # Standalone Rust crate
│   │   ├── src/
│   │   │   ├── lib.rs             #   Crate root, module declarations, re-exports
│   │   │   ├── types.rs           #   MetricKind enum (cross-cutting domain type)
│   │   │   ├── error.rs           #   CoreError enum + CoreResult alias
│   │   │   ├── interpolation.rs   #   Linear interpolation utilities
│   │   │   ├── rdp.rs             #   Ramer-Douglas-Peucker simplification
│   │   │   ├── paths.rs           #   AppPaths: font/template/output dirs
│   │   │   ├── activity/          #   Activity ingestion & densification
│   │   │   │   ├── schema.rs      #     ParsedActivity, DenseActivityReport
│   │   │   │   ├── trim.rs        #     Scene-window trimming
│   │   │   │   └── interpolate.rs #     Frame-rate densification
│   │   │   ├── config/mod.rs      #   RenderConfig schema, validation, plots
│   │   │   ├── commands/mod.rs    #   Backend command implementations
│   │   │   ├── debug/mod.rs       #   RenderProgress, RenderProfiler
│   │   │   ├── render/            #   Skia overlay rendering
│   │   │   │   ├── mod.rs         #     prepare_preview_assets, render_frame_rgba
│   │   │   │   ├── surface.rs     #     Skia surface create/wrap/encode
│   │   │   │   ├── text.rs        #     Font resolution, text drawing
│   │   │   │   ├── format.rs      #     Metric formatting, unit conversion
│   │   │   │   ├── static_layer.rs#     Cached static label layer
│   │   │   │   └── widgets/       #     Widget rendering (route, elevation, value)
│   │   │   │       ├── common.rs  #       Polyline, area, marker, transform
│   │   │   │       ├── types.rs   #       Cache types, geometry types
│   │   │   │       ├── geometry.rs#       Interior segment geometry helpers
│   │   │   │       ├── marker.rs  #       Marker circle drawing
│   │   │   │       ├── polyline.rs#       Polyline drawing (painting)
│   │   │   │       ├── transform.rs#      Coordinate transform utilities
│   │   │   │       ├── route/     #       Route map widget
│   │   │   │       ├── elevation/ #       Elevation profile widget
│   │   │   │       ├── value/     #       Metric value widget
│   │   │   │       └── tests/     #       Widget unit tests
│   │   │   └── encode/            #   FFmpeg video encoding pipelines
│   │   │       ├── mod.rs         #     Module organization
│   │   │       ├── ffmpeg.rs      #     Binary discovery, ffmpeg arg builders
│   │   │       ├── ffmpeg_settings.rs    # Unified encoding settings type
│   │   │       ├── ffmpeg_composite.rs   # Composite MP4 ffmpeg args
│   │   │       ├── ffmpeg_composite_profiles.rs  # Composite encoder profiles
│   │   │       ├── ffmpeg_transparent_profiles.rs # Transparent encoder profiles
│   │   │       ├── fps.rs         #     Rational FPS type
│   │   │       ├── progress.rs    #     RenderProgress state machine
│   │   │       ├── codec_detect.rs#     Encoder availability probing
│   │   │       ├── codec_catalog.rs#     Known codec definitions
│   │   │       ├── video_probe.rs #     ffprobe metadata extraction
│   │   │       ├── video.rs       #     RenderController, dispatch, orchestration
│   │   │       ├── video_pipeline.rs      # Single-pass render (transparent)
│   │   │       ├── video_parallel.rs      # Parallel segment rendering
│   │   │       ├── video_segmented.rs     # Segmented render + concat
│   │   │       ├── video_windows.rs       # Windows-specific encode helpers
│   │   │       ├── video_debug.rs         # Debug artifact generation
│   │   │       ├── video_composite_pipeline.rs # Composite render loop
│   │   │       ├── video_composite_support.rs  # Composite timing/plan helpers
│   │   │       ├── video_composite_debug.rs    # Composite debug artifacts
│   │   │       └── pipeline_shared.rs     # Shared encode pipeline types
│   │   └── tests/                 # Integration tests
│   │       ├── common/            #   Shared test fixtures & helpers
│   │       ├── activity_tests.rs  #   Activity parse/trim/densify
│   │       ├── commands_tests.rs  #   Backend command logic
│   │       ├── config_tests.rs    #   RenderConfig validation
│   │       ├── cancellation_tests.rs  # Render cancellation
│   │       ├── codec_detect_tests.rs  # Encoder detection
│   │       ├── composite_profile_tests.rs
│   │       ├── error_display_tests.rs
│   │       ├── ffmpeg_composite_tests.rs
│   │       ├── ffmpeg_settings_tests.rs
│   │       ├── format_tests.rs
│   │       ├── fps_tests.rs
│   │       ├── metric_kind_behavior_tests.rs
│   │       ├── metric_kind_serde_tests.rs
│   │       ├── progress_tests.rs
│   │       ├── rdp_tests.rs
│   │       ├── render_baseline_suite.rs  # PNG baseline render tests
│   │       ├── value_widget_tests.rs
│   │       ├── video_composite_pipeline_tests.rs
│   │       ├── video_probe_tests.rs
│   │       └── video_tests.rs
│   │
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
┌─────────────────────────────────────────┐
│  Tauri Shell (WebView)                  │
│  ┌───────────────────────────────────┐  │
│  │  React 19 App                     │  │
│  │  ┌──────────┐ ┌──────────────┐    │  │
│  │  │  Shell   │ │  Editor      │    │  │
│  │  │ (header, │ │ (canvas,     │    │  │
│  │  │  tabs,   │ │  moveable,   │    │  │
│  │  │  dialogs)│ │  player)     │    │  │
│  │  └────┬─────┘ └──────┬───────┘    │  │
│  │       └──────┬───────┘            │  │
│  │              ▼                    │  │
│  │    ┌─────────────────┐            │  │
│  │    │  Zustand Store  │            │  │
│  │    │  (4 slices)     │            │  │
│  │    └────────┬────────┘            │  │
│  │             │                     │  │
│  │    ┌────────▼────────┐            │  │
│  │    │  api/backend.js │            │  │
│  │    │  (Tauri IPC)    │            │  │
│  │    └────────┬────────┘            │  │
│  └─────────────┼─────────────────────┘  │
└────────────────┼────────────────────────┘
                 │ Tauri IPC (JSON)
┌────────────────▼─────────────────────────┐
│  Rust Backend                            │
│  ┌───────────────────────────────────┐   │
│  │  src-tauri/ shell layer           │   │
│  │  ├── lib.rs       app wiring      │   │
│  │  ├── tauri_commands.rs  #[command]│   │
│  │  ├── file_ops.rs  template IO     │   │
│  │  ├── preview_import.rs  preview   │   │
│  │  ├── runtime_paths.rs  resources  │   │
│  │  └── video_server.rs  HTTP server │   │
│  └──────────────┬────────────────────┘   │
│                 │                        │
│  ┌──────────────▼────────────────────┐   │
│  │  ovrley_core crate                │   │
│  │  ┌──────────┐  ┌──────────┐       │   │
│  │  │ types    │  │ error    │       │   │
│  │  │ MetricKind│  │CoreError│       │   │
│  │  └──────────┘  └──────────┘       │   │
│  │  ┌──────────┐  ┌──────────┐       │   │
│  │  │interpol. │  │ rdp      │       │   │
│  │  │ paths    │  │ activity │       │   │
│  │  └──────────┘  └─────┬────┘       │   │
│  │  ┌──────────┐        │            │   │
│  │  │ config   │────────┘            │   │
│  │  └────┬─────┘                     │   │
│  │       │                           │   │
│  │  ┌────▼─────────────────┐         │   │
│  │  │ render/ (Skia)       │         │   │
│  │  │ ├── surface/text     │         │   │
│  │  │ ├── format           │         │   │
│  │  │ ├── static_layer     │         │   │
│  │  │ └── widgets/         │         │   │
│  │  │   ├── route/ (LTTB+RDP)        │   │
│  │  │   ├── elevation/ (SG+RDP)      │   │
│  │  │   └── value/ (icons+gradient)  │   │
│  │  └──────────┬──────────┘         │   │
│  │             │                     │   │
│  │  ┌──────────▼──────────┐         │   │
│  │  │ encode/ (FFmpeg)    │         │   │
│  │  │ ├── video/controller│         │   │
│  │  │ ├── pipeline        │         │   │
│  │  │ ├── composite       │         │   │
│  │  │ ├── segmented       │         │   │
│  │  │ ├── parallel        │         │   │
│  │  │ ├── codec_detect    │         │   │
│  │  │ ├── video_probe     │         │   │
│  │  │ └── debug/progress  │         │   │
│  │  └─────────────────────┘         │   │
│  └──────────────────────────────────┘   │
└──────────────────────────────────────────┘
```

### Process Model

- **Frontend (WebView):** React 19 app rendered in Tauri's WebView. Communicates with Rust via JSON-over-IPC (`invoke`).
- **Backend (Rust):** Tauri v2 process. The shell layer (`src-tauri/src/`) registers IPC commands, manages `BackendState`, and delegates to `ovrley_core`. Domain logic — rendering, encoding, activity processing, config validation — lives in `ovrley_core` and is Tauri-agnostic.
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
    │   │        └── video_composite_pipeline::render_composite_video_single()
    │   │            (or video_segmented::render_composite_video_segmented for parallels)
    │   │
    │   └── NO  → video::render_video()
    │             ├── should_parallelize_qtrle() ?
    │             │   ├── YES → video_segmented::render_video_segmented()
    │             │   │          → video_parallel::run_parallel_renders()
    │             │   │          → ffmpeg concat stitch
    │             │   └── NO  → video_pipeline::render_video_single()
    │             │
    │             ▼
    │         render_video_single()
    │
    ▼
Shared pipeline:
    ├── 1. Parse config (config::parse_config_json)
    ├── 2. Parse activity (activity::parse_activity_json)
    ├── 3. Trim activity to scene window (activity::trim::trim_activity)
    ├── 4. Densify activity to frame rate (activity::interpolate::densify_activity)
    ├── 5. Prepare Skia assets (render::prepare_preview_assets)
    │      ├── Cached labels image (render::static_layer)
    │      ├── Route widget: GPS projection → LTTB downsample → RDP simplify
    │      │   (route/normalize.rs → route/prepare.rs → route/simplify.rs)
    │      └── Elevation widget: SG smooth → RDP with min/max preservation
    │          (elevation/normalize.rs → elevation/prepare.rs → elevation/reduction.rs)
    ├── 6. Spawn FFmpeg subprocess (encode/ffmpeg.rs)
    ├── 7. Render loop: for each frame →
    │      ├── render::render_frame_to_surface()
    │      │   ├── Blit static labels layer (render::static_layer)
    │      │   ├── Draw metric values (value/layout.rs: icon + value + unit)
    │      │   ├── Draw route widget (route/draw.rs: remaining + completed + marker)
    │      │   └── Draw elevation widget (elevation/draw.rs: remaining + completed + marker + labels)
    │      └── Write RGBA bytes to FFmpeg stdin
    ├── 8. Monitor FFmpeg progress (parse stderr for frame=)
    ├── 9. Wait for FFmpeg to finish
    └── 10. Validate output, write timing/debug summaries
```

### 6.3 FFmpeg Integration

**Discovery** (`encode/ffmpeg.rs`):

- Search order: `OVRLEY_FFMPEG` env → `FFMPEG_BINARY` env → `vendor/ffmpeg/bin/` → PATH

**Encoding settings** (`encode/ffmpeg_settings.rs`):

- `FfmpegSettings` struct: unified builder for transparent and composite pipelines
- Pixel format selection, encoder flags, hardware acceleration (Vulkan, VideoToolbox, NVENC, QSV, VAAPI)
- `FfmpegSettingsBuilder` chains: codec-specific presets, bitrate, container format

**Transparent encoding** (`encode/video_pipeline.rs`):

- Raw RGBA piped via stdin: `ffmpeg -f rawvideo -pix_fmt rgba -s WxH -r fps -i -`
- Encoder profiles in `encode/ffmpeg_transparent_profiles.rs`
- Encoder: ProRes (`prores_ks`, `prores_ks_vulkan`, `prores_videotoolbox`) or QTRLE (`qtrle`)
- Vulkan acceleration: `-init_hw_device vulkan=vk -filter_hw_device vk -vf format=yuva444p10le,hwupload`

**Composite MP4 encoding** (`encode/video_composite_pipeline.rs`):

- Two inputs: source video (file) + overlay (pipe:0 raw RGBA)
- Encoder profiles in `encode/ffmpeg_composite_profiles.rs`
- FFmpeg args built in `encode/ffmpeg_composite.rs`
- Filter complex: `[0:v]setpts=PTS-STARTPTS,scale=W:H[base];[1:v]setpts=PTS-STARTPTS[ovr];[base][ovr]overlay=0:0:eof_action=repeat:shortest=1,format=yuv420p[out]`
- Output: H.264/H.265 with `-movflags +faststart`, audio copied

**QTRLE parallelism** (`encode/video.rs`, `encode/video_segmented.rs`, `encode/video_parallel.rs`):

- For QTRLE codec with >= 2 second duration: split into second-aligned segments
- Segments rendered in parallel threads via `run_parallel_renders()`
- Stitched with ffmpeg concat demuxer

**Codec detection** (`encode/codec_detect.rs`):

- Runs short ffmpeg encode tests for 20+ codec/hardware combinations
- Codec definitions in `encode/codec_catalog.rs`
- Reports booleans back to frontend for UI filtering

**Video probing** (`encode/video_probe.rs`):

- ffprobe metadata: dimensions, duration, FPS, codec, bitrate, pixel format
- Creates activity timestamps from video for composite sync

**Progress tracking** (`encode/progress.rs`, `debug/mod.rs`):

- `RenderProgress`: current frame, elapsed, ETA, time per frame, message
- `RenderProfiler`: fine-grained timing buckets per pipeline phase
- `RenderController`: shared state machine (Idle → Running → Done/Cancelled)

### 6.4 Activity Processing (Rust Side)

The Rust backend receives already-parsed activity JSON from the frontend (the JS-side parser extracts raw samples). The Rust side does:

1. **Parse** (`activity/mod.rs`): Accepts production or debug payload JSON, deserializes into `ParsedActivity` via `activity/schema.rs`.
2. **Trim** (`activity/trim.rs`): Validate scene window against activity duration, interpolate boundary samples, produce `TrimmedActivity` with scene-local timeline and only the required telemetry series.
3. **Densify** (`activity/interpolate.rs`): Convert uneven samples into frame-aligned dense series using linear interpolation with edge clamping via shared `interpolation.rs` utilities.
4. **Report**: `DenseActivityReport` with per-frame telemetry for every scene frame.

### 6.5 Widget Rendering (Skia)

**Text** (`render/text.rs`):

- Font resolution: bundled path → directories → system FontMgr → fallback
- Font cache: process-lifetime `OnceLock<Mutex<HashMap>>` by family name
- Text drawing: shadow (drop-shadow filter) → stroke (border) → fill

**Cache / shared types** (`render/widgets/types.rs`, `render/widgets/transform.rs`):

- `WidgetCacheEntry`: pre-rendered Skia Image + dirty flag
- Coordinate transforms: canvas ↔ widget-local space
- `render/static_layer.rs`: once-per-render cached label layer

**Route map** (`render/widgets/route/`):

1. `normalize.rs`: Project GPS coords to 2D via equirectangular projection at mean latitude; normalize to 0..1
2. `prepare.rs`: Downsample via LTTB (Largest-Triangle-Three-Buckets); `polyline.rs` renders cached polylines
3. `simplify.rs`: Simplify via RDP (Ramer-Douglas-Peucker) using shared `rdp.rs`
4. `frame_state.rs`: Per-frame state: completion fraction, marker position
5. `draw.rs`: Per frame: draw remaining route → completed route prefix → marker circle (`marker.rs`)
6. `geometry.rs`: Interior segment geometry for completed path clipping

**Elevation profile** (`render/widgets/elevation/`):

1. `normalize.rs`: Smooth via Savitzky-Golay filter (11-point kernel); normalize elevation and distance to 0..1
2. `prepare.rs`: Project normalized points to widget pixel bounds
3. `reduction.rs`: Simplify RDP with preserved min/max elevation points (visibility-critical)
4. `frame_state.rs`: Per-frame state: completed/remaining cut point, marker position
5. `draw.rs`: Per frame: draw remaining area+line → completed area fill → completed line → marker → metric/imperial labels

**Metric values** (`render/widgets/value/`):

- `icons.rs`: SVG icons parsed from `include_str!` embedded SVGs (path, line, circle primitives)
- `svg.rs`: SVG path string → Skia Path conversion (tokenizer + emitter)
- `gradient.rs`: Triangle indicator showing slope direction + magnitude
- `layout.rs`: Value text + unit text + icon layout with shadow/border, line breaks, overflow
- `format.rs` (render root): Unit conversion (metric/imperial), decimal rounding, time formatting

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

## 10. Key Patterns & Conventions

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
- **Module layering** — `tauri_commands.rs` (Tauri `#[command]` wrappers) → `ovrley_core::commands` (framework-agnostic logic) → domain modules (activity, config, render, encode)
- **Shared utilities** — `types.rs` (MetricKind), `error.rs` (CoreError), `interpolation.rs`, `rdp.rs`, `paths.rs` (AppPaths) live at crate root as leaf dependencies
- **Render loop** — acquires surface, renders RGBA, writes to FFmpeg stdin pipe
- **Composite pipeline** — two-input FFmpeg: source video file + raw pipe overlay
- **Parallel QTRLE** — segment-based parallelism with FFmpeg concat stitch, governed by `video.rs` / `video_segmented.rs` / `video_parallel.rs`
- **FFmpeg subprocess** — rawvideo via stdin, stderr parsing for progress
- **Cancellation** — cooperative; render loops check `RenderController` at frame boundaries

### Data Formats

- **Templates** — JSON `ovrley-template` v1 format
- **Activity** — JSON serialized from JS parser, Rust-side trim + densify
- **Frames** — raw RGBA (u8, 4 bytes/pixel) between Rust and FFmpeg
- **Preview** — PNG via Skia encode, returned as base64 over IPC

---

## 11. Known Architectural Notes

1. **Widget rendering is duplicated** — JSX SVG preview vs. Rust Skia render. Expect minor visual discrepancies. Rust output is authoritative.
2. **Composite timing is the most complex part** — involves mapping between 3 time domains: video time, activity time, and overlay frame index.
3. **QTRLE parallel render** — only activates for >= 2 second integer-second durations, uses `logical_cores / 4` workers.
4. **No test framework for frontend** — all testing is manual. Rust has unit tests in `render/widgets/tests/` and integration tests in `ovrley_core/tests/` and `src-tauri/tests/`.
5. **Browser fallback** — the frontend has a fallback path for running outside Tauri (browser dev mode), using local file APIs instead of Tauri IPC.
6. **CSS zoom** — the editor shell supports zoom via `--app-scale` CSS variable (0.35x–4x).
