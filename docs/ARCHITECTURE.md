# OVRLEY — Project Context

## 1. Project Overview

**OVRLEY** (`0.1.0`, GPL-3.0) is a cross-platform desktop application that turns `.fit` / `.gpx` GPS activity data and `.srt` telemetry subtitles into **customizable video overlays**. Users load an activity file, arrange widgets (speed, heart rate, elevation, route map, time, gradient, etc.) on a scene canvas, optionally import a source video for compositing, and render the result as ProRes, QTRLE, or H.264/H.265 MP4.

**Key capabilities:**

- 10+ widget types: metric values (speed, HR, power, cadence, temperature, time, gradient), text labels, route map, elevation profile
- Widget editing: drag, resize, rotate, grid snap via `react-moveable`
- Activity extraction: FIT (via `fit-file-parser` JS library), GPX (via DOMParser), and SRT (text parser), with shared Rust finalization
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
│   │   ├── api/                  # Tauri IPC bridge (backend.js)
│   │   ├── store/                # Zustand state (5 slices)
│   │   ├── hooks/                # Shared hooks (selectors, useFpsMode)
│   │   ├── lib/                  # Utility library
│   │   │   ├── activity/         #   Activity extraction + backend finalization bridge
│   │   │   ├── utils.js          #   isInteractiveElement, clamp, cn
│   │   │   ├── update-rate.js    #   FPS normalization, rate options
│   │   │   ├── widget-config.js  #   Widget CRUD operations
│   │   │   └── ...               #   color, geometry, interpolation, template, etc.
│   │   ├── features/             # 10 feature modules
│   │   │   ├── app-shell/        #   Shell: header, title bar, bootstrap, activity import, backend status
│   │   │   ├── overlay-editor/   #   Editor canvas, moveable, widget selection, keyboard
│   │   │   ├── player/           #   Playback engine, timeline, shortcuts
│   │   │   ├── render-video/     #   Render dialog, codec selection, progress, workflow
│   │   │   ├── scene-settings/   #   Resolution, FPS, video sync, global defaults
│   │   │   ├── template-manager/ #   Template CRUD, community templates, save status
│   │   │   ├── video-preview/    #   Video import, preview playback, sync
│   │   │   ├── widget-drawer/    #   Widget quick-add sidebar panel
│   │   │   ├── widget-editor/    #   Widget property editors, sidebar tab
│   │   │   └── widget-preview/   #   SVG widget preview renderers
│   │   ├── components/           # Shared React components
│   │   │   ├── ui/               #   shadcn/ui primitives (Radix-based, 19 components)
│   │   │   └── widgets/          #   Widget SVG icons
│   │   └── tests/                # Vitest tests (50 suites, 320 tests)
│   ├── package.json
│   ├── vite.config.js
│   └── eslint.config.js
│
├── src-tauri/                    # Tauri v2 desktop shell
│   ├── build.rs                  # Windows msvcprt link + Tauri build
│   ├── Cargo.toml                # Workspace root + app package
│   ├── tauri.conf.json
│   │
│   ├── src/                       # Tauri shell layer (app_lib crate)
│   │   ├── main.rs                #   Binary entry point
│   │   ├── lib.rs                 #   Tauri app wiring, 18 IPC commands, BackendState
│   │   ├── tauri_commands.rs      #   Tauri #[command] wrappers → ovrley_core
│   │   ├── file_ops.rs            #   Template file read/write commands
│   │   ├── preview_import.rs      #   Video preview import logic
│   │   ├── runtime_paths.rs       #   Platform resource path resolution
│   │   ├── video_server.rs        #   Local HTTP server for video preview
│   │   └── video_server_tests.rs  #   Video server integration tests
│   │
│   ├── ovrley_core/               # Standalone domain library crate
│   │   ├── Cargo.toml
│   │   ├── BENCHMARKS.md
│   │   ├── BASELINES.md
│   │   └── src/
│   │       ├── lib.rs             #   Crate root, module declarations, re-exports
│   │       ├── types.rs           #   MetricKind enum (cross-cutting domain type)
│   │       ├── error.rs           #   CoreError enum + CoreResult alias
│   │       ├── interpolation.rs   #   Linear interpolation utilities
│   │       ├── rdp.rs             #   Ramer-Douglas-Peucker simplification
│   │       ├── paths.rs           #   AppPaths: font/template/output dirs
│   │       ├── bin_common.rs       #   Shared CLI argument parsing
│   │       ├── benchmark_common.rs #   Shared benchmark infrastructure
│   │       ├── bin/               #   CLI diagnostic binaries
│   │       │   ├── render_video.rs    #     Video render
│   │       │   ├── render_preview.rs  #     Preview PNG generation
│   │       │   ├── validate_activity.rs #  Activity validation
│   │       │   ├── parallel_render.rs #     Diagnostic parallel benchmark
│   │       │   ├── benchmark_widget_rate.rs |  Widget update rate benchmark
│   │       │   ├── benchmark_transparent.rs|  Transparent codec benchmark
│   │       │   └── benchmark_composite.rs  |  Composite codec benchmark
│   │       ├── activity/          #   Activity ingestion & densification
│   │       │   ├── schema.rs      #     ParsedActivity, DenseActivityReport
│   │       │   ├── trim.rs        #     Scene-window trimming
│   │       │   └── interpolate.rs #     Frame-rate densification
│   │       ├── commands/         #   Backend command implementations
│   │       │   ├── mod.rs        #     Module organization + shared helpers
│   │       │   ├── elevation_geometry.rs  # Elevation geometry IPC command
│   │       │   └── route_geometry.rs      # Route geometry IPC command
│   │       ├── normalize/         #   Config validation seam
│   │       │   ├── mod.rs         #     validate_render_config, ValidatedRenderConfig
│   │       │   ├── raw/           #     Raw types + parsing (private)
│   │       │   ├── helpers.rs     #     Shared validation helpers
│   │       │   ├── scene.rs       #     SceneConfig validation
│   │       │   ├── value.rs       #     ValueConfig validation
│   │       │   ├── gradient.rs    #     Gradient widget validation
│   │       │   ├── heading.rs     #     Heading widget validation
│   │       │   ├── label.rs       #     Label validation
│   │       │   ├── route.rs       #     Route plot validation
│   │       │   ├── elevation.rs   #     Elevation plot validation
│   │       │   └── time.rs        #     Time value validation
│   │       ├── debug/mod.rs       #   RenderProgress, RenderProfiler
│   │       ├── render/            #   Skia overlay rendering
│   │       │   ├── mod.rs         #     prepare_preview_assets, render_frame_rgba
│   │       │   ├── surface.rs     #     Skia surface create/wrap/encode
│   │       │   ├── text.rs        #     Font resolution, text drawing
│   │       │   ├── format.rs      #     Metric formatting, unit conversion
│   │       │   ├── static_layer.rs#     Cached static label layer
│   │       │   └── widgets/       #     Widget rendering (route, elevation, value)
│   │       │       ├── common.rs  #       Polyline, area, marker, transform
│   │       │       ├── types.rs   #       Cache types, geometry types
│   │       │       ├── geometry.rs#       Interior segment geometry helpers
│   │       │       ├── marker.rs  #       Marker circle drawing
│   │       │       ├── polyline.rs#       Polyline drawing (painting)
│   │       │       ├── transform.rs#      Coordinate transform utilities
│   │       │       ├── route/     #       Route map widget
│   │       │       ├── elevation/ #       Elevation profile widget
│   │       │       ├── value/     #       Metric value widget
│   │       │       └── tests/     #       Widget unit tests
│   │       └── encode/            #   FFmpeg video encoding pipelines
│   │           ├── mod.rs         #     Module organization
│   │           ├── ffmpeg.rs      #     Binary discovery, ffmpeg arg builders
│   │           ├── ffmpeg_settings.rs    # Unified encoding settings type
│   │           ├── ffmpeg_composite.rs   # Composite MP4 ffmpeg args
│   │           ├── ffmpeg_composite_profiles.rs  # Composite encoder profiles
│   │           ├── ffmpeg_transparent_profiles.rs # Transparent encoder profiles
│   │           ├── fps.rs         #     Rational FPS type
│   │           ├── progress.rs    #     RenderProgress state machine
│   │           ├── codec_detect.rs#     Encoder availability probing
│   │           ├── codec_catalog.rs#     Known codec definitions
│   │           ├── video.rs       #     RenderController, dispatch, orchestration
│   │           ├── video_pipeline.rs      # Single-pass render (transparent)
│   │           ├── video_parallel.rs      # Parallel segment rendering
│   │           ├── video_segmented.rs     # Segmented render + concat
│   │           ├── video_windows.rs       # Windows-specific encode helpers
│   │           ├── video_debug.rs         # Debug artifact generation
│   │           ├── video_composite_pipeline.rs # Composite render loop
│   │           ├── video_composite_support.rs  # Composite timing/plan helpers
│   │           ├── video_composite_debug.rs    # Composite debug artifacts
│   │           └── pipeline_shared.rs     # Shared encode pipeline types
│   │       ├── media/             #   Imported source media parsing
│   │       │   ├── mod.rs         #     Source media module organization
│   │       │   ├── source_video_metadata.rs # Shared video metadata contract
│   │       │   ├── video_probe.rs #     ffprobe metadata extraction
│   │       │   └── mp4_telemetry/ #   telemetry-parser extraction + column assembly
│   │
│   └── tests/                     # ovrley_core integration tests
│       ├── common/                #   Shared test fixtures & helpers
│       ├── activity_tests.rs      #   Activity parse/trim/densify
│       ├── commands_tests.rs      #   Backend command logic
│       ├── config_tests.rs        #   RenderConfig validation
│       ├── cancellation_tests.rs  #   Render cancellation
│       ├── codec_detect_tests.rs  #   Encoder detection
│       ├── composite_profile_tests.rs
│       ├── error_display_tests.rs
│       ├── ffmpeg_composite_tests.rs
│       ├── ffmpeg_settings_tests.rs
│       ├── format_tests.rs
│       ├── fps_tests.rs
│       ├── metric_kind_behavior_tests.rs
│       ├── metric_kind_serde_tests.rs
│       ├── progress_tests.rs
│       ├── rdp_tests.rs
│       ├── render_baseline_suite.rs  # PNG baseline render tests
│       ├── value_widget_tests.rs
│       ├── video_composite_pipeline_tests.rs
│       ├── video_probe_tests.rs
│       └── video_tests.rs
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
| Rendering (Rust)   | skia-safe         | 0.97.2                                      |
| Video encoding     | FFmpeg            | 8.1+ (via subprocess)                       |
| Activity parsing   | fit-file-parser   | browser                                     |
| Drag/resize        | react-moveable    | latest                                      |
| Icons              | lucide-react      | latest                                      |
| Package manager    | pnpm              | 10.25.0                                     |
| Rust edition       | 2021              | 1.85.0 minimum toolchain                    |
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
│  │    │  (5 slices)     │            │  │
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
│  │  │ normalize│────────┘            │   │
│  │  │  (seam)  │                     │   │
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
3. **Geometry is computed in Rust:** Widget geometry (route Mercator projection + LTTB downsampling + RDP simplification, elevation smoothing + downsampling + simplification) is computed once in Rust via IPC commands (`backend_build_route_geometry`, `backend_build_elevation_geometry`). The JS hooks consume pre-built geometry for 30fps preview rendering with zero IPC latency. Per-frame operations (marker interpolation, completed segment splitting, SVG path materialization) remain local in JS.
4. **Cached static layers:** Labels and static widget backgrounds are rendered once and cached as Skia images. Only dynamic metric values + marker positions are redrawn per frame.
5. **Composite video timing is tricky:** Overlay FPS = source video FPS / update_rate. Timing mapping goes: overlay_frame -> video_local_time -> activity_time -> dense_frame_index.
6. **Normalization seam — zero backend defaults:** All raw config types (`SceneConfig`, `ValueConfig`, etc.) and parsing functions live in `normalize::raw` (a **private** submodule). No code outside `normalize/` can access raw types. The only public entry point is `validate_render_config()`, which converts raw types into validated types. The backend owns **zero** render-affecting defaults — missing fields are rejected, not filled in. The frontend must materialise every value before sending.

---

## 5. Frontend Architecture

### 5.1 State Management (Zustand)

The Zustand store is composed of 5 slices, merged via `create()` with `immer` + `subscribeWithSelector` middleware:

```
useStore
├── createEditorSlice
│   # Preview playback, widget selection, editor config, draft state, selectedSecond
├── createLayoutSlice
│   # Widget drawer open/close toggle
├── createMediaSlice
│   # Activity summary, render progress, errors, parsedActivity, videoFilename, activityFilename
├── createTemplateSlice
│   # Templates, export settings, global defaults, aspectRatio, updateRate, config hydration
└── createVideoImportSlice
    # Imported video metadata, sync offset, codec availability, platformOs
```

Store access from components goes through selector hooks in `hooks/useAppStoreSelectors.js`, which use `useShallow` to prevent unnecessary re-renders. Non-React utility functions must NOT call `useStore.getState()` — they receive state via parameters from their hook callers. React hooks may use `useStore.getState()` for imperative state reads (e.g. inside callbacks and effects).

### 5.2 Component Architecture

Components follow a **container/presentational pattern** via hooks:

```
App.jsx
├── useAppBootstrap                 # On mount: fetch platform, templates, codecs    (app-shell)
├── useActivityImport               # File input for .fit/.gpx/.srt activity import  (app-shell)
├── useTemplateManagement           # CRUD lifecycle for templates                   (template-manager)
├── useRenderWorkflow               # Render dialog orchestration                    (render-video)
├── useEditorShellState             # Zoom, grid, background mode                    (app-shell)
├── useVideoImport                  # Video file import dialog                       (video-preview)
├── useBackendStatus                # Backend health polling (2s interval)           (app-shell)
│
├── TitleBar                        # Custom window decorations
├── AppHeader                       # Main toolbar
├── ControlPanel                    # Sidebar with Settings/Widgets tabs
│   ├── SidebarSettingsTab          # Scene settings, video sync, global defaults    (scene-settings)
│   └── SidebarWidgetsTab           # Widget quick-add + property editors            (widget-editor)
├── OverlayEditor                   # Main editor canvas                             (overlay-editor)
│   ├── OverlayCanvas               # Scene compositing (grid, video bg, widgets)
│   ├── OverlayMoveable             # Drag/resize/rotate/scale wrapper
│   ├── WidgetPreview               # Routes to correct renderer by type             (widget-preview)
│   └── useOverlayEditorState       # Central editor state hook                      (overlay-editor)
├── OverlayPlayer                   # Timeline playback controls                     (player)
├── RenderVideoDialog               # Export settings + progress panel               (render-video)
├── ErrorAlert                      # Toast error notification
└── LoadingOverlay                  # Activity spinner
```

### 5.3 Feature Modules

10 feature modules each with their own `data/`, `utils/`, `hooks/`, and/or `components/` subdirectories:

| Feature | Purpose |
|---|---|
| **`app-shell/`** | App header, title bar, control panel, loading overlay, error alert, backend health polling, activity import, app bootstrap, editor shell state (zoom/grid/background) |
| **`overlay-editor/`** | Main editor canvas, moveable (drag/resize/rotate), widget selection, editor keyboard shortcuts, viewport management, drag/rotate/scale/resize handlers |
| **`player/`** | Overlay player component, playback engine (timeline + video-backed RAF loops), keyboard shortcuts (Space/Arrow keys), timeline scrubbing |
| **`render-video/`** | Render dialog: codec selection (ProRes, QTRLE, H.264, H.265), hardware acceleration (NVENC, QSV, VAAPI, VideoToolbox, Vulkan), bitrate slider, FPS mode, export range, progress polling, render workflow orchestration |
| **`scene-settings/`** | Sidebar settings tab: aspect ratio, resolution, FPS, widget update rate, video sync offset, global font/color/opacity defaults |
| **`template-manager/`** | Template CRUD, community template loading, template save status tracking, new-template dialog |
| **`video-preview/`** | Video file import, local HTTP preview server, video playback clock, scrub scheduling, sync with activity timeline |
| **`widget-drawer/`** | Widget quick-add sidebar panel (button grid) |
| **`widget-editor/`** | Per-widget property editors: metric value, text, time, gradient, elevation, route map, heading. Sidebar widgets tab composition. |
| **`widget-preview/`** | SVG widget preview renderers: metric, text, route, elevation, heading. Geometry computation, text measurement, shadow rendering. |

Shared cross-feature utilities live in `lib/` (e.g. `isInteractiveElement` in `utils.js`) and `hooks/` (e.g. `useFpsMode`).

### 5.4 Widget System

Widgets are stored in the `config` object as arrays: `config.labels[]`, `config.values[]`, `config.plots[]`. Each widget has a unique ID format: `{label|value|plot}-{index}`.

**Widget CRUD** (`lib/widget-config.js`):

- `buildConfigWidgets` — creates flat widget list from config
- `groupWidgetsForSidebar` — groups by type
- `findWidgetById`, `updateWidgetInConfig`, `deleteWidgetInConfig`

**10 widget types** and their preview renderers (`features/widget-preview/components/`):

| Widget Type                      | Renderer                | Editor                       | Geometry Source |
| -------------------------------- | ----------------------- | ---------------------------- | --------------- |
| text                             | `TextRenderer`          | `TextWidgetEditor`           | N/A             |
| speed, heartrate, power, cadence | `MetricRenderer`        | `MetricWidgetEditor`         | N/A             |
| time                             | `MetricRenderer`        | `TimeWidgetEditor`           | N/A             |
| temperature                      | `MetricRenderer`        | `TemperatureWidgetEditor`    | N/A             |
| gradient                         | `MetricRenderer` (triangle) | `GradientWidgetEditor`   | N/A             |
| heading                          | `HeadingRenderer`       | `HeadingWidgetEditor`        | N/A             |
| route_map                        | `RouteRenderer`         | `RouteMapWidgetEditor`       | Rust IPC        |
| elevation                        | `ElevationRenderer`     | `ElevationWidgetEditor`      | Rust IPC        |

**Geometry IPC pattern** (route and elevation widgets):

The route and elevation widgets compute their geometry in Rust and expose it via IPC commands. The JS hooks consume this geometry for 30fps preview rendering:

```
User moves slider / preview second changes
  → JS hook calls buildRouteGeometry() / buildElevationGeometry() via IPC
  → Rust runs geometry pipeline (fast — hundreds of points)
  → Rust returns { points, progressValues, bbox, sourcePointCount, simplification }
  → JS hook stores geometry in state
  → JS renders preview at 30fps using local interpolation (zero latency)
```

Per-frame operations (marker interpolation, completed segment splitting, SVG path materialization) remain local in JS for performance. The geometry is computed once when parameters change, not per-frame.

### 5.5 Activity Import Pipeline

```
.fit/.gpx/.srt file
    │
    ▼
import-activity.js                        # File type detection + parser dispatch
    │
    ├── .fit → fit-parser.js              # fit-file-parser library
    ├── .gpx → gpx-parser.js              # DOMParser + extensions
    └── .srt → srt-parser.js              # DJI subtitle telemetry
    │
    ▼
RawActivity                               # file_name, file_format, metadata, raw_samples, options
    ├── Snake-case raw samples in canonical units
    ├── Parser-selected idle-gap behavior
    └── Per-metric smoothing requests
    │
    ▼
backend.finalizeActivity()                # Tauri IPC: backend_finalize_activity
    └── Sends RawActivity JSON to Rust
    │
    ▼
Rust activity::finalize                   # shared post-processing
    ├── Idle gap fill
    ├── Elapsed/distance/course/time/progress series
    ├── Metric derivation
    ├── Optional per-metric smoothing
    └── Returns { parsed_activity, debug_payload }
Zustand (activitySummary, parsedActivity)
```

The frontend owns browser-visible FIT/GPX/SRT extraction into `RawActivity`. MP4 telemetry is extracted in Rust by `media/mp4_telemetry/`, smoothed at native cadence where required, aligned into `ActivityColumns`, and sent directly into the same shared finalizer core. Rust owns canonical finalization from optional idle-gap fill through derived metrics, opt-in smoothing, `sync_time` metadata, debug payload generation, and final `ParsedActivity` assembly. The `importActivityFile()` function accepts `storeActions` as a required parameter (no implicit `useStore.getState()` fallback).

### 5.6 Video Preview System

Imported source videos are served by a custom local HTTP server (`video_server.rs`) for native `<video>` element playback:

```
User selects video
    │
    ▼
useVideoImport.js → backend.importPreviewVideo(path)
    │                          (features/video-preview/hooks/)
    ▼
lib.rs: backend_import_preview_video
    ├── Assigns UUIDv4 import_id
    ├── Registers file in VideoServerHandle
    └── Returns preview_url (http://127.0.0.1:PORT/video/<uuid>)
    │
    ▼
useVideoPreview.js
    ├── Sets <video> src to preview_url
    ├── Handles play/pause/scrub via useVideoPlaybackClock
    ├── Syncs with activity timeline via videoSyncOffsetSeconds
    └── Debounces seek requests for performance
```

---

## 6. Backend Architecture

### 6.1 Tauri IPC Commands (21 total)

All defined in `lib.rs` and implemented in `ovrley_core/src/commands/mod.rs`:

| Command                        | Purpose                                       |
| ------------------------------ | --------------------------------------------- |
| `backend_health`               | Health check + FFmpeg path                    |
| `backend_current_os`           | OS string ("windows", "macos")                |
| `backend_list_system_fonts`    | Skia FontMgr font listing                     |
| `backend_finalize_activity`    | Finalize RawActivity into ParsedActivity      |
| `backend_render`               | Start video render (transparent or composite) |
| `backend_render_preview_frame` | Render one transparent preview PNG (debug)    |
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
| `backend_build_elevation_geometry` | Build elevation widget geometry via IPC   |
| `backend_build_route_geometry` | Build route widget geometry via IPC           |
| `default_template_save_path`   | User template path                            |
| `write_template_file`          | Write template JSON to disk (validated)       |
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
    ├── 1. Parse config JSON → raw types (normalize::raw::parse_config_json)
    ├── 2. Validate through normalization seam (normalize::validate_render_config)
    │      └── Validates every field; rejects missing/out-of-range values
    │          Zero backend-owned defaults — frontend must send everything
    ├── 3. Parse activity (activity::parse_activity_json)
    ├── 4. Trim activity to scene window (activity::trim::trim_activity)
    ├── 5. Densify activity to frame rate (activity::interpolate::densify_activity)
    ├── 6. Prepare Skia assets (render::prepare_preview_assets)
    │      ├── Cached labels image (render::static_layer)
    │      ├── Route widget: GPS projection → LTTB downsample → RDP simplify
    │      │   (route/normalize.rs → route/prepare.rs → route/simplify.rs)
    │      └── Elevation widget: SG smooth → RDP with min/max preservation
    │          (elevation/normalize.rs → elevation/prepare.rs → elevation/reduction.rs)
    ├── 7. Spawn FFmpeg subprocess (encode/ffmpeg.rs)
    ├── 8. Render loop: for each frame →
    │      ├── render::render_frame_to_surface()
    │      │   ├── Blit static labels layer (render::static_layer)
    │      │   ├── Draw metric values (value/layout.rs: icon + value + unit)
    │      │   ├── Draw route widget (route/draw.rs: remaining + completed + marker)
    │      │   └── Draw elevation widget (elevation/draw.rs: remaining + completed + marker + labels)
    │      └── Write RGBA bytes to FFmpeg stdin
    ├── 9. Monitor FFmpeg progress (parse stderr for frame=)
    ├── 10. Wait for FFmpeg to finish
    └── 11. Validate output, write timing/debug summaries

Geometry IPC Commands (used by frontend preview):
    ├── backend_build_elevation_geometry
    │   └── commands::elevation_geometry::build_elevation_geometry_command
    │       ├── Parse config + activity JSON
    │       ├── Build elevation source points (trim + project)
    │       ├── Normalize elevation plot (scale dimensions)
    │       ├── Build elevation geometry (smooth → downsample → simplify)
    │       └── Return { points, progressValues, bbox, sourcePointCount, simplification }
    │
    └── backend_build_route_geometry
        └── commands::route_geometry::build_route_geometry_command
            ├── Parse config + activity JSON
            ├── Build route samples (trim + Mercator projection)
            ├── Normalize route plot (scale dimensions)
            ├── Build route geometry (fit → downsample LTTB → simplify RDP)
            └── Return { points, progressValues, bbox, sourcePointCount, simplification }
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

### 6.4 Source Media Probing

**Video probing** (`media/video_probe.rs`):

- ffprobe metadata: dimensions, duration, FPS, codec, bitrate, pixel format
- Returns the shared `SourceVideoMetadata` shape
- Uses creation-time metadata only as a sync fallback

**MP4 telemetry metadata and activity extraction** (`media/mp4_telemetry/`):

- telemetry-parser metadata: dimensions, duration, FPS, rotation
- Returns the shared `SourceVideoMetadata` shape
- Runs before ffprobe for video import metadata
- ffprobe salvages codec/audio/container/sync fields that telemetry-parser does
  not expose
- Activity extraction uses `telemetry-parser` tag maps first, with a DJI AC004
  fallback for files whose parser output is incomplete for that camera path
- MP4 extraction normalizes vendor telemetry into `NativeSample`, smooths
  continuous MP4-native streams before culling, aligns GPS/IMU/camera streams
  into `ActivityColumns`, then calls the shared activity finalizer
- GPS/course anchoring requires usable latitude/longitude. Speed, heading,
  altitude, or timestamps alone do not create a route or GPS-derived timeline
- MP4 does not assemble `ParsedActivity` directly; `activity/finalize.rs` owns
  gap handling, metric derivation, metadata enrichment, and final assembly

**Progress tracking** (`encode/progress.rs`, `debug/mod.rs`):

- `RenderProgress`: current frame, elapsed, ETA, time per frame, message
- `RenderProfiler`: fine-grained timing buckets per pipeline phase
- `RenderController`: shared state machine (Idle → Running → Done/Cancelled)

### 6.5 Activity Processing (Rust Side)

All activity sources converge on the shared Rust finalizer before preview or render. Frontend FIT/GPX/SRT parsers send `RawActivity` to `backend_finalize_activity`; MP4 telemetry is extracted in Rust, converted to `ActivityColumns`, and finalized through the same core path.

1. **Frontend raw activity input** (`activity/schema.rs`): `RawActivity` carries `file_name`, `file_format`, `metadata`, `raw_samples`, and parser-selected options. FIT/GPX/SRT parsers emit canonical units and can request per-metric smoothing.
2. **Backend MP4 column input** (`media/mp4_telemetry/`): MP4 extraction produces `NativeSample` rows, preserves MP4-specific pre-cull smoothing for continuous streams, aligns GPS/IMU/camera cadences into `ActivityColumns`, and sets `skip_idle_gap_fill: true` because MP4 uses a video-derived or GPS-derived cadence.
3. **Finalize** (`activity/finalize.rs`): `RawActivity` is gap-filled first when requested, then converted to `ActivityColumns`. `ActivityColumns` input goes directly into the same finalizer core. The finalizer builds elapsed/time/course/distance/progress series, derives missing metrics, applies opt-in per-metric smoothing after derivation, enriches metadata, and assembles `ParsedActivity`.
4. **Sync naming** (`activity/schema.rs`): `sync_time` is the canonical activity start/sync field. `source_start_time` is retired, and `metadata.start_time` is stripped during finalization.
5. **Debug payload** (`activity/finalize.rs`): Dev builds return `{ parsed_activity, debug_payload }`; release builds return `debug_payload: null`. The frontend persists the payload through `write_parse_debug_file` when present.
6. **Parse existing activity JSON** (`activity/mod.rs`): Render and geometry commands accept production or debug payload JSON and deserialize `ParsedActivity` via `activity/schema.rs`.
7. **Trim** (`activity/trim.rs`): Validate scene window against activity duration, interpolate boundary samples, produce `TrimmedActivity` with scene-local timeline and only the required telemetry series.
8. **Densify** (`activity/interpolate.rs`): Convert uneven samples into frame-aligned dense series using linear interpolation with edge clamping via shared `interpolation.rs` utilities.
9. **Report**: `DenseActivityReport` with per-frame telemetry for every scene frame.

### 6.6 Widget Rendering (Skia)

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
7. `commands/route_geometry.rs`: IPC command `backend_build_route_geometry` exposes the geometry pipeline to the frontend

**Elevation profile** (`render/widgets/elevation/`):

1. `normalize.rs`: Smooth via Savitzky-Golay filter (11-point kernel); normalize elevation and distance to 0..1
2. `prepare.rs`: Project normalized points to widget pixel bounds
3. `reduction.rs`: Simplify RDP with preserved min/max elevation points (visibility-critical)
4. `frame_state.rs`: Per-frame state: completed/remaining cut point, marker position
5. `draw.rs`: Per frame: draw remaining area+line → completed area fill → completed line → marker → metric/imperial labels
6. `commands/elevation_geometry.rs`: IPC command `backend_build_elevation_geometry` exposes the geometry pipeline to the frontend

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
File picker (.fit/.gpx/.srt)
    │
    ▼
lib/activity/import-activity.js
    ├── Frontend parser extracts RawActivity
    └── backend_finalize_activity returns ParsedActivity
    │
    ▼
Zustand: activitySummary, parsedActivity set
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
render-video.js → build final config
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
└── Video sync: compares video sync_time vs activity timestamps
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
    └── Tauri bundles: portable ZIP (Windows), app bundle (macOS)
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

Templates are JSON files following the `ovrley-template` format (v2):

```json
{
  "format": "ovrley-template",
  "version": 2,
  "name": "Template Name",
  "savedAt": "ISO-8601",
  "config": {
    "scene": { "width": 1920, "height": 1080, "fps": 30, "start": 0, "end": 3600, ... },
    "labels": [{ "text": "...", "x": 100, "y": 200, ... }],
    "values": [{ "value": "speed", "x": 100, "y": 300, ... }],
    "plots": [{ "type": "route_map", ... }, { "type": "elevation", ... }]
  },
  "settings": {
    "globalDefaults": { "font", "color", "opacity", "scale", ... },
    "updateRate", "exportRange", "exportCodec", "aspectRatio"
  }
}
```

**Validation:** Template writes go through `write_template_file` which validates the full config through the normalization seam before writing to disk. The backend rejects any template with missing or invalid fields — the frontend must provide complete configs.

**Resolution:** Built-in templates in `templates/`, user templates in `Documents/OVRLEY/`. Deduplication by filename (user wins over built-in).

---

## 10. Key Patterns & Conventions

### Frontend

- **No TypeScript** — JSDoc for type documentation on exported functions
- **Zustand** — global state with Immer, `subscribeWithSelector`, 5 slices, selector hooks with `useShallow`
- **No `useStore.getState()` in utilities** — non-React functions (e.g. `renderVideo()`, `importActivityFile()`) receive store state via parameters from their hook callers. Only React hooks and store slices may call `useStore.getState()`.
- **Container hooks** — extract store access, side effects, and derived state from components
- **Presentational components** — receive grouped props, minimal logic
- **shadcn/ui** — Radix-based primitives in `components/ui/`
- **Feature folders** — `data/` (constants), `utils/` (pure functions), `hooks/`, `components/`
- **Shared hooks** — `useFpsMode` (FPS mode selector logic shared between scene-settings and render-video)
- **Shared utilities** — `isInteractiveElement` (input-focus guard shared between player keyboard and editor keyboard shortcuts)

### Rust

- **skia-safe 0.97.2** — `binary-cache` feature enabled; mutable path construction now uses Skia's `PathBuilder` APIs where older releases allowed editing `Path` directly
- **Process-lifetime caches** — `OnceLock<Mutex<HashMap>>` for fonts, label images
- **Module layering** — `tauri_commands.rs` (Tauri `#[command]` wrappers) → `ovrley_core::commands` (framework-agnostic logic) → domain modules (activity, media, render, encode)
- **Normalization seam** — all raw config types and parsing live in `normalize::raw` (private submodule). The only public entry point is `normalize::validate_render_config()`. No code outside `normalize/` can access raw types. The backend owns zero render-affecting defaults — the frontend must materialise every value before sending.
- **Shared utilities** — `types.rs` (MetricKind), `error.rs` (CoreError), `interpolation.rs`, `rdp.rs`, `paths.rs` (AppPaths) live at crate root as leaf dependencies
- **Render loop** — acquires surface, renders RGBA, writes to FFmpeg stdin pipe
- **Composite pipeline** — two-input FFmpeg: source video file + raw pipe overlay
- **Parallel QTRLE** — segment-based parallelism with FFmpeg concat stitch, governed by `video.rs` / `video_segmented.rs` / `video_parallel.rs`
- **FFmpeg subprocess** — rawvideo via stdin, stderr parsing for progress
- **Cancellation** — cooperative; render loops check `RenderController` at frame boundaries

### Data Formats

- **Templates** — JSON `ovrley-template` v1 format
- **Activity** — FIT/GPX/SRT frontend parsers emit `RawActivity`; MP4 Rust extraction emits `ActivityColumns`; Rust finalizes both to `ParsedActivity`, then trims + densifies
- **Frames** — raw RGBA (u8, 4 bytes/pixel) between Rust and FFmpeg
- **Preview** — PNG via Skia encode, returned as base64 over IPC

---

## 11. Known Architectural Notes

1. **Geometry is computed in Rust** — Route and elevation widget geometry (Mercator projection, LTTB downsampling, RDP simplification, Savitzky-Golay smoothing) is computed once in Rust via IPC commands (`backend_build_route_geometry`, `backend_build_elevation_geometry`). The JS hooks consume pre-built geometry for 30fps preview rendering. Per-frame operations (marker interpolation, completed segment splitting, SVG path materialization) remain local in JS. This eliminates the duplicated JS geometry pipelines (`routeGeometry.js`, `elevationGeometry.js`) and ensures WYSIWYG parity between preview and final render.
2. **Canvas-parity testing** — The Rust `PreparedPreviewAssets` exposes `elevation_geometry_json()` and `route_geometry_json()` methods that serialize geometry for Playwright tests. The test script injects `window.__OVRLEY_MOCK_ELEVATION_GEOMETRY` and `window.__OVRLEY_MOCK_ROUTE_GEOMETRY` so the frontend uses identical geometry to Skia rendering.
3. **Composite timing is the most complex part** — involves mapping between 3 time domains: video time, activity time, and overlay frame index.
4. **QTRLE parallel render** — only activates for >= 2 second integer-second durations, uses `logical_cores / 4` workers.
5. **Frontend testing** — 50 Vitest test suites (320 tests) in `app/src/tests/`. Rust has widget unit tests in `ovrley_core/src/render/widgets/tests/`, video server tests in `src-tauri/src/video_server_tests.rs`, and integration tests in `ovrley_core/tests/`. CLI benchmark binaries live in `ovrley_core/src/bin/` (`cargo run -p ovrley_core --bin <name>`).
6. **Browser fallback** — the frontend has a fallback path for running outside Tauri (browser dev mode), using local file APIs instead of Tauri IPC.
7. **CSS zoom** — the editor shell supports zoom via `--app-scale` CSS variable (0.35x–4x).
