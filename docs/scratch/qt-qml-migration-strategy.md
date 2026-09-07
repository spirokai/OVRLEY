# OVRLEY Qt QML migration strategy

Prepared 2026-09-06 from the working tree at commit `97f549c3ff8f0388f02df185c856163e841c9be0`, including its uncommitted changes. This is the analysis requested by [qt-qml-migration.md](qt-qml-migration.md), not an implementation of the migration. No application files were changed and no builds were run.

## 1. Recommended architecture

Create `src-tauri/ovrley_qml/` as a separate native executable using Qt Quick, Qt Quick Controls and `qmetaobject-rs`. Keep the current React/Tauri executable fully operational throughout implementation. Reuse `ovrley_core` and existing shell-independent Rust functionality directly wherever practical. If substantial application/session logic genuinely needs to be shared between the React/Tauri and QML frontends, extract only those parts into a small shared Rust crate such as `src-tauri/ovrley_app/`; do not make creation of that crate a prerequisite for the migration.

The dependency direction should remain as simple as possible:

```text
React UI -> existing Tauri IPC wrappers -> existing Rust services / ovrley_core
QML UI   -> focused Rust Qt controllers -> existing Rust services / ovrley_core
                                      |
                                      +-> Qt platform services and Qt Multimedia
```

Shared application services may own editing transactions, the active project/session, source preparation, persistence coordination and playback policy where extracting them clearly reduces duplication. `ovrley_core` continues to own activity processing, normalization, widget geometry/rendering, synchronization analysis and export. Shared crates must not depend on Qt or Tauri. The two executables have independent in-memory sessions; they exchange saved files, not live state.

Implement one feature at a time, including its domain behavior and QML presentation. Keep all React features until the complete QML application passes parity acceptance. This is stricter than deleting individual features as soon as their replacements work, and preserves a usable React application throughout.

Widget preview is the **last feature implemented**. Earlier editor work uses an explicit empty preview surface plus selection bounds and manipulation handles; it does not recreate widget graphics in QML. Final integration, acceptance and React removal follow that feature.

## 2. Findings from the current code

Paths below are repository-relative. The analysis follows current source rather than older architectural descriptions.

| Area                | Observed implementation                                                                                                                                                             | Migration consequence                                                                                                             |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Shell               | `App.jsx`, `features/app-shell/hooks/useAppShellComposition.js` coordinate templates, projects, activity/video imports, exports, close guards and updates                           | Replace hook orchestration with focused Rust services; migrate visual shell early                                                 |
| State               | `store/useStore.js` combines six slices: template, editor, media, video import, render settings and layout; Immer/Zundo history is installed by `features/undo-redo/undoHistory.js` | State is broader than the older four-slice description; do not translate slices into QML stores                                   |
| Templates/widgets   | `lib/widget/standard-widgets.js` reads `assets/standard-widgets.json` and `assets/standard-metrics.json`; matching Rust modules already exist                                       | Keep manifests canonical and expose their Rust catalog to QML                                                                     |
| Durable editor data | `lib/widget/editor-state.js`, template normalization and widget resolver distinguish saved configuration, defaults and active display variants                                      | Rust needs an editable document that preserves inactive variants, not just a normalized render configuration                      |
| Projects            | `features/projects/projectOperations.js` prepares sources before applying a project; `src-tauri/src/project_file.rs` already defines and validates `.oly` archives                  | Extract existing Rust archive services; move frontend transaction/dirty-state behavior into Rust                                  |
| Video               | HTML video playback uses the Tauri loopback server and browser-specific clock/scrub hooks                                                                                           | QML loads native video through Qt Multimedia (`MediaPlayer` + `VideoOutput`); retain the server only for the React executable     |
| Timeline            | `playerTiming.js` supports negative video start, activity/video union duration and clock handoff outside the video interval                                                         | A conventional video-only player is insufficient                                                                                  |
| Sync Doctor         | Current working tree includes `useVisualSync.js`, its drawer, three commands, progress events and `ovrley_core/src/synchronization/`                                                | Include this feature; source revisions and stale-result rejection must survive drawer destruction                                 |
| Widget preview      | `features/widget-preview/` is React/SVG rendering; `OverlayCanvas.jsx` composes the editor surface                                                                                  | Replace the graphics at the very end with the Rust renderer                                                                       |
| Rust caching        | `render/static_layer.rs`, `PreparedPreviewAssets`, `prepare_base_rgba`, route/elevation preparation already cache static content                                                    | The renderer is not simply uncached or literally one layer; interactive invalidation and cache lifetime remain the real questions |
| Toolchain           | Actual manifests use Skia `0.97.2` and the Tauri package declares Rust `1.85`                                                                                                       | Do not plan against the older guide's Skia `0.75` / Rust `1.84` assumptions                                                       |

Two paths in the brief are stale: the control panel is `features/app-shell/components/ControlPanel.jsx`, and the scene-settings entry is `features/scene-settings/components/SidebarSettingsTab.jsx`, not `SceneSettingsTab.jsx`. The API file is `app/src/api/backend.js`.

## 3. State ownership and Rust contracts

### Durable state, session state and UI state

| Current owner/behavior                                                       | Target owner                                                 | Persistence and notification policy                                                                                                               |
| ---------------------------------------------------------------------------- | ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| Template config, global defaults, selected template and saved baseline       | Rust editor/template services                                | Preserve template format and inactive `display_variants`; expose typed edit operations                                                            |
| Project name/path, dirty baseline, sources, sync and export settings         | Rust project service                                         | Preserve `.oly` version 1 and its existing field names; save atomically                                                                           |
| Parsed activity, summary, telemetry provenance, video metadata               | Rust media service                                           | Large arrays stay in Rust; expose summaries and requested presentation data                                                                       |
| FPS, widget update rate, codec, bitrate, export range                        | Rust session/document                                        | One owner; derive effective render configuration without writing render-only fields into templates                                                |
| Playhead and playback transport state                                        | Rust playback/session                                        | Playback state participates in synchronization and decoder/timeline coordination; preserve existing project persistence behavior where applicable |
| Timeline viewport, zoom, pan and scroll position                             | QML presentation state, persisted through Rust when required | Keep live interaction in QML; persistence does not by itself require Rust to own presentation behavior                                            |
| Undo/redo and current edit transaction                                       | Rust editor history                                          | Match current 20-entry bound and undoable projection; exclude large media data                                                                    |
| Selection and live edits shared by sidebar/canvas                            | Rust editor session                                          | Session-only; stable existing widget IDs, begin/update/commit/cancel transaction                                                                  |
| Export and synchronization jobs                                              | Existing Rust controllers/jobs with application coordination | Survive QML page reload; immutable request snapshot and explicit terminal state                                                                   |
| Open drawer, active tab, popup visibility, hover, focus, accordion expansion | QML presentation state                                       | Persist only established preferences through the settings service                                                                                 |
| Drag pointer position, rubber-band rectangle, intermediate text entry        | Reusable QML presentation object/component                   | Convert accepted input into typed commands; no durable mutation per raw pointer event                                                             |
| Theme, language, remembered directories and window preferences               | Rust settings service; QML theme reads it                    | Reuse `ovrley-settings.json` semantics and existing keys; verify platform location                                                                |

Current undo history includes config, global defaults, aspect ratio, render settings, video offset and start/end seconds. Project dirty comparison is a different projection: editor, sources, sync, render and timeline. Do not unify these projections accidentally or make playback undoable merely because the playhead is saved in projects.

### Proposed controllers and models

Names below are proposed interfaces, not APIs already present. Keep QObjects small and backed by shared Rust services. The session composition root wires dependencies; it does not accumulate every feature method.

| Controller                   | Properties/models and commands                                                                                                                                                            |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ApplicationController`      | Startup phase, readiness, error notifications, version/distribution; close request and shutdown coordination                                                                              |
| `SettingsController`         | Validated preferences, theme, locale, remembered directories; explicit setters                                                                                                            |
| `ProjectController`          | Project path/name, dirty/busy state, project-list model, missing-source requests; new/open/save/save-as, resolve source, answer unsaved-change request                                    |
| `TemplateController`         | Template-list model, current template and saved status; load/create/save/save-as/open directory                                                                                           |
| `EditorController`           | Scene settings, selected IDs, widget-list model, undo/redo availability; add/delete/duplicate/select, change display variant, edit scene/global defaults, begin/update/commit/cancel edit |
| `MediaController`            | Activity summary/source revision, source-video metadata/revision, import phase and warnings; prepare/activate/clear source, extract embedded telemetry                                    |
| `PlaybackController`         | Playhead, range, clock owner, transport state, video offset/timezone choice, clip model; play/pause/seek/step/scrub, adjust offset/range/viewport                                         |
| `VisualSyncController`       | Analysis status, evidence/candidate models, input revisions; start/cancel/apply candidate                                                                                                 |
| `RenderController` binding   | Codec model, export phase/progress/output/error; suggest output, start/cancel/open result; wraps the existing core render controller                                                      |
| `UpdateController`           | Available version, download state/progress, install/restart actions and errors                                                                                                            |
| `TranslationController`      | Catalog revision, selected locale, key lookup with named arguments                                                                                                                        |
| `PreviewController` â€” last | Immutable scene/activity revision and playhead inputs, frame readiness/errors, render bounds; owns prepared preview session, not editor state                                             |

Use list models for projects, templates, widgets, fonts, codecs, timeline clips and sync candidates. Rows expose canonical identifiers and typed roles. Emit row-level changes instead of replacing an entire serialized session. Use typed controllers and model roles for stable application concepts, but do not require a dedicated QObject type for every widget family or display variant. Where widget schemas are inherently dynamic, use a bounded property/edit model with canonical field identifiers and typed values rather than generating a large binding class hierarchy. Do not invent a general JSON patch endpoint or one `QVariantMap` containing the whole application.

Validate file/config input once at the Rust ingress. Validate newly submitted user edit values at their command boundary, then expose validated state to consumers. Required malformed values produce actionable errors; defaults are used only for documented absence or explicit creation of a new widget/variant. External activity gaps remain governed by existing activity-data policies.

Rust's render normalization is a derived compilation step, not another independently maintained editor model. Preserve the durable variant representation and compile it through the owner into validated render assets. Audit existing aliases such as scene `updateRate`/`update_rate`; select the existing persisted contract and fix producer/owner signatures together. Do not add a QML naming scheme plus remapping helpers. Preserve existing file compatibility through the actual format ingress, not consumer-side repairs.

### Threading, transactions and shutdown

Qt-facing QObjects and list-model notifications live on the GUI thread. Run parsing, probing, archive I/O, synchronization and rendering on owned workers. Deliver results through queued notifications with operation ID and source/document revision. Reject stale completion after source replacement, cancellation or project switch. Never capture raw QML item pointers in a long-lived job.

`qmetaobject-rs` supplies QObject properties, signals, methods, models and queued callbacks, but Rust object pinning and Qt thread affinity remain explicit integration obligations. Use pinned objects that outlive their QML consumers. [qmetaobject documentation](https://docs.rs/qmetaobject/latest/qmetaobject/)

Loading a project must stage and validate the document, resolve missing files, prepare all sources, and commit the new session atomically. Cancellation or failed preparation leaves the previous session intact. Saving captures a revision: if editing continues during save, only the saved revision becomes the baseline. A drag/slider interaction commits one history entry; cancel restores the prior document.

Reuse `encode::progress::ProgressSink` for Qt delivery rather than polling or adding a second render job implementation. Reuse synchronization job cancellation/shutdown. Close processing must settle unsaved-change decisions, request cancellation, stop accepting new jobs, release playback resources and join workers before destroying their owners.

## 4. QML presentation framework

Use native QML singletons for theme tokens, spacing, typography and icons. Port semantic colors from `styles/theme.css` and layout intent from the existing components, not Tailwind utility strings. Register the same bundled fonts used by Skia; system-font selection comes from the Rust font catalog. Reuse SVG assets and shared widget manifests.

| Reusable group     | Current sources                                                   | QML equivalents                                                                                                        |
| ------------------ | ----------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Basic controls     | `components/ui/`                                                  | Styled Quick Controls buttons, switches, sliders, combo boxes, text fields, tooltips, separators, progress and dialogs |
| Form controls      | `blur-input`, font/color controls, `widgetFormControls.jsx`       | Committed numeric field, unit field, font picker, color popup, resettable field, collapsible section                   |
| Shell              | Header/title bar, ControlPanel, toolbar/drawer layout             | ApplicationWindow, layouts, tool rail, stack/tab controls, scrollable settings panel                                   |
| Feedback           | ErrorAlert, LoadingOverlay, unsaved/missing-source/update dialogs | Error banner, busy layer, reusable modal prompt and progress panel                                                     |
| Resource lists     | Projects, templates, widgets and media drawers                    | Model-backed ListView/GridView delegates and source information cards                                                  |
| Timeline           | TimelineSurface, TimelineLane, PlayerToolbar                      | Timeline ruler/lanes, clip delegate, range handles and transport controls                                              |
| Editor interaction | OverlayMoveable and gesture hooks                                 | Pointer handlers, selection overlay and transform handles, separated from widget rendering                             |

`ControlPanel.jsx` is tabs plus a scrollable settings/widget area. `VerticalToolbar.jsx` is five selectable tool buttons with tooltips. `SidebarSettingsTab.jsx` composes scene/global form sections. None presents a concrete need for Qt Widgets. Qt Quick Controls and layouts cover these requirements; choose QML dialogs or Qt native dialog facilities without introducing a QWidget application shell. [Qt Quick Controls](https://doc.qt.io/qt-6/qtquickcontrols-index.html)

Keep reusable interaction state in dedicated QML objects/controllers, and visual delegates declarative. QML handlers may dispatch actions and perform presentation calculations. Project orchestration, synchronization decisions, render configuration and reusable geometry belong in Rust. Preserve keyboard focus, shortcut suppression inside fields/dialogs, accessibility labels, scrolling and high-DPI behavior; visual similarity alone is not parity.

## 5. React hooks and utilities: disposition

| Existing group                                                             | Action                                                                                                     |
| -------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `useAppBootstrap`, `useBackendStatus`, shell composition                   | Replace with native startup/readiness and service construction; delete IPC-runtime detection at cutover    |
| `useProjectLifecycle`, document state/source recovery, `projectOperations` | Move transaction, dirty, recovery and operation-lock behavior to Rust; QML displays requests               |
| Template fetching/management/save status                                   | Rust catalog and persistence operations; QML selection/dialogs                                             |
| `useActivityImport`, `useVideoImport`, import activation                   | Rust source preparation/activation; keep parser exclusions below                                           |
| `usePlaybackEngine`, playback clock, scrub scheduler, clip timing          | Rust transport policy and Qt Multimedia position/seek boundary; QML pointer gestures and timeline painting |
| `useVisualSync`, video sync controls                                       | Rust jobs/revisions/offset decisions; QML evidence presentation                                            |
| Render workflow/dialog-derived state/config preparation                    | Rust export request construction and lifecycle; QML dialog navigation and field drafts                     |
| Widget manager, display-variant updater, draft state, selection            | Rust editor/catalog/transactions; QML focus and gesture capture                                            |
| Drag/resize/rotate/scale hooks                                             | Reuse or add Rust geometry operations; QML maps pointer input to document coordinates                      |
| Viewport/timeline gestures, drawer state, accordion autoscroll             | QML reusable presentation logic; commit persisted viewport/preferences through Rust                        |
| `useUndoRedo`                                                              | Rust history actions and availability; QML shortcut dispatch                                               |
| `useAppUpdate`, close guard                                                | Native services and application lifecycle; QML prompts                                                     |
| Widget-preview hooks, SVG helpers, DOM measurement                         | Become obsolete after the final Rust preview integration                                                   |

Before porting a utility, compare its semantics and tests with these owners:

| JavaScript utilities                                                                                    | Existing Rust reuse point / missing work                                                                                                                  |
| ------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Standard metric/widget definitions and display defaults                                                 | `standard_metrics.rs`, `standard_widgets.rs`, shared `assets/*.json`; expose existing definitions                                                         |
| Metric/time/gauge labels, altitude, lap timer formatting                                                | `render/format.rs`, `render/widgets/metric_presentation.rs`, lap-timer modules and `activity/elevation.rs`; compare missing-value/unit/rounding semantics |
| Interpolation and preview timing                                                                        | `interpolation.rs`, `activity/interpolate.rs`, `encode/fps.rs`; test gap bridging/preservation and boundary/frame rounding before reuse                   |
| Route/elevation preview geometry                                                                        | `commands/route_geometry.rs`, `commands/elevation_geometry.rs`, renderer preparation; remove JS geometry transport once preview is native                 |
| Widget resolver/draft/presentation and template normalization                                           | `normalize/`, raw config and validated types; add durable editing/variant operations where missing, not a line-by-line resolver port                      |
| Video creation time, timezone and overlap                                                               | `media/time.rs`, `media/timezone.rs`, source metadata plus new playback policy; retain local-vs-UTC camera ambiguity behavior                             |
| Codec and render-config utilities                                                                       | `encode/ffmpeg/`, `encode/pipeline/composite_plan.rs`, `output.rs`; move frontend request orchestration into the Rust owner                               |
| Project paths, snapshots, file checks                                                                   | Extract `src-tauri/src/project_file.rs` and filesystem helpers, preserving platform atomic replacement                                                    |
| Geometry utilities and widget interaction scaling                                                       | Compare `render/widgets/geometry.rs`, `transform.rs`, display-layout code; add editor geometry where rendering APIs are insufficient                      |
| Color/short time labels/timeline pixels                                                                 | Small presentation-only operations may remain QML; export-affecting formatting stays Rust                                                                 |
| `compose-refs`, class utilities, DOM helpers, cached promises, browser font/runtime glue, `previewPerf` | Delete with React; replace instrumentation at native seams where useful                                                                                   |

Do not inspect or port FIT/GPX/SRT/IGC parser internals as part of this work. Preserve their React paths until the separately planned Rust parsers exist. Existing CSV/VBO processing, MP4 telemetry extraction and Rust finalization can drive the QML migration. Native parser parity is a prerequisite for full product cutover; dropping formats or retaining a JavaScript parser sidecar would not satisfy the goal.

## 6. Frontend/backend communication inventory

The following covers `app/src/api/backend.js` and the additional registrations in `src-tauri/src/lib.rs`. Keep Tauri wire contracts functional while extracting shared implementations. QML calls typed service bindings, not these command strings or JSON-string-returning helpers.

| Existing commands                                                                                                               | Target                                                                                                                                             |
| ------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| `backend_health`, `backend_current_os`, `backend_distribution_kind`                                                             | Native application readiness/platform/distribution properties                                                                                      |
| `backend_list_system_fonts`                                                                                                     | Rust font catalog/model shared with renderer                                                                                                       |
| `backend_list_templates`, `backend_get_template`, `default_template_save_path`, `write_template_file`, `backend_open_templates` | Template service plus native dialog/system opener                                                                                                  |
| `default_project_directory`, `list_project_files`, `read_project_file`, `write_project_file`, `selected_path_is_file`           | Extracted project persistence and source-recovery service                                                                                          |
| `read_selected_file_bytes`, `write_parse_debug_file`                                                                            | Native source loading and diagnostic output; byte transport to frontend becomes unnecessary; debug writer is registered beyond the main JS wrapper |
| `backend_finalize_activity`, `backend_parse_csv_activity`, `backend_parse_vbo_activity`, `backend_extract_video_telemetry`      | Typed activity/media services retaining current core pipelines                                                                                     |
| `backend_probe_video`, `backend_prepare_preview_video`                                                                          | Native probe/metadata service; probe is registered even without a matching main JS helper                                                          |
| `backend_import_preview_video`, `backend_register_preview_video`, `backend_clear_preview_video`, `backend_get_video_state`      | Split media activation from browser-server lifecycle; QML uses native source and playback session                                                  |
| `backend_start_visual_sync`, `backend_visual_sync_status`, `backend_cancel_visual_sync`                                         | Existing analysis jobs through VisualSyncController                                                                                                |
| `backend_render`, `backend_progress`, `backend_cancel`, `backend_detect_codecs`                                                 | Typed export request and existing render controller/codec services                                                                                 |
| `backend_suggest_output_path`, `backend_open_output_directory`, `backend_open_video`                                            | Existing Rust output policy plus platform opener                                                                                                   |
| `backend_render_preview_frame`                                                                                                  | Keep current PNG diagnostic/export seam; final QML preview uses persistent in-memory assets                                                        |
| `backend_build_elevation_geometry`, `backend_build_route_geometry`                                                              | Existing renderer helpers; native preview consumes Rust geometry directly                                                                          |
| `backend_open_hevc_support`                                                                                                     | Browser-specific support flow retires after Qt Multimedia playback acceptance; native decoder failures still surface                               |

Additional transport and platform usage:

| Existing use                                                        | Replacement                                                                                                                               |
| ------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `render-progress`, `visual-sync-progress` listeners                 | Queued controller notifications with current job snapshot available on connection                                                         |
| Tauri dialog plugin in `lib/file-dialog.js` and template management | QML/native file dialogs returning selected native paths; preserve cancel, filters, extension, remembered-directory and overwrite behavior |
| Tauri path API in preferences/export output                         | Rust path operations at filesystem ingress                                                                                                |
| Tauri store plugin, `ovrley-settings.json`                          | Rust preferences service; resolve existing platform path and validate known settings                                                      |
| Tauri window controls/header dragging and close events              | Qt window state/system movement and close event coordination                                                                              |
| Native drag/drop listener in `useFileDropZone`                      | QML drop area; translate file URLs at the platform boundary                                                                               |
| `convertFileSrc`, preview URLs and HTML video                       | Native `QUrl` media source through Qt Multimedia; no local HTTP server in QML executable                                                  |
| Updater `check`, download/install callbacks and process `relaunch`  | Native update service preserving installed/portable behavior; Qt cannot simply invoke the Tauri plugin                                    |
| Webview devtools toggle in `App.jsx`                                | QML diagnostics/profiler and native logs                                                                                                  |

Remove JSON stringify/parse round trips, cycle-breaking serialization and browser-runtime fallbacks from the native design. Rust services should expose typed results even where current command helpers return `serde_json::Value`; refactor their owner when needed while the Tauri wrapper serializes the established external contract.

## 7. Gyroflow-informed native integration

### Toolchain and playback stack

Gyroflow is the primary integration reference: its bootstrap registers QML types and supplies Rust-owned objects to a QML engine. OVRLEY should adopt this ownership pattern while splitting controllers by responsibility. [Gyroflow bootstrap](https://github.com/gyroflow/gyroflow/blob/master/src/gyroflow.rs)

Gyroflow remains the primary reference for Rust/QML application structure and `VideoArea.qml` composition, but OVRLEY does not need to inherit its `qml-video-rs`/MDK playback stack. Select and pin a tested Qt minor version, Rust bindings and Rust toolchain; verify Windows MSVC and macOS architectures. Gyroflow currently uses edition 2024, so copying its complete dependency manifest would be an unnecessary toolchain change. [Gyroflow dependencies](https://github.com/gyroflow/gyroflow/blob/master/Cargo.toml)

Use Qt Multimedia for playback, with QML `MediaPlayer` and `VideoOutput` as the default implementation. OVRLEY requires normal playback and timestamp-based scrubbing, not frame-precise seeking. Verify codec playback, hardware acceleration behavior, seeking responsiveness and deployment of the required Qt Multimedia plugins on the supported platforms. [Qt MediaPlayer](https://doc.qt.io/qt-6/qml-qtmultimedia-mediaplayer.html) [Qt VideoOutput](https://doc.qt.io/qt-6/qml-qtmultimedia-videooutput.html)

### Development reload

Implement a separate proposed `pnpm dev:qml` command that selects the QML Cargo package explicitly and enables development-only filesystem QML loading. Keep `pnpm dev` and `pnpm dev:frontend` unchanged. This is a future command: it is not added by this report. Any build execution remains subject to the repository's explicit user-permission requirement.

Gyroflow's reload helper watches source files, schedules a delayed reload, re-adds changed paths, clears the component cache and recreates `App.qml` beneath an existing window. This is subtree recreation, not React-style preservation of each component's local state. [Gyroflow live reload](https://github.com/gyroflow/gyroflow/blob/master/src/ui_live_reload.cpp)

Proposed OVRLEY implementation:

1. Keep the native application, Rust services and a stable root window outside the reloadable `App.qml` subtree.
2. Watch QML, theme and development resource directories. Debounce changes; re-scan directories for new files and restore watches after atomic-save replacement.
3. Quiesce presentation callbacks, cancel active edit gestures, disconnect old views and destroy the reloadable subtree before clearing its component cache. Keep committed Rust state and jobs alive.
4. Recreate the subtree and bind it to existing controllers/models. Show compile errors in a stable development error surface and recover on the next successful save.
5. Decide QML singleton lifetime explicitly. Ordinary theme-value updates may use notified properties; singleton type/structural changes should recreate the engine or restart the process rather than mix stale and new types.
6. If the `MediaPlayer`/`VideoOutput` subtree is destroyed, restore its source/playhead from Rust after recreation; avoid promising uninterrupted playback across reload. Rust edits require recompilation/restart.

Qt documents that component-cache clearing does not replace already instantiated objects and can produce type mismatches if old instances remain. Respect this in the lifetime design; do not copy Gyroflow's delayed deletion scheme without checking its assumptions. [QQmlEngine cache lifecycle](https://doc.qt.io/qt-6/qqmlengine.html#clearComponentCache)

Reload acceptance: repeated file saves, new/deleted QML files, syntax error recovery, theme changes, active job continuity, no duplicate signal subscriptions, no leaked video resources and preserved committed document state.

### Video and activity timeline

Gyroflow's `VideoArea.qml` is useful as a reference for metadata loading, playback controls, timeline composition and video transforms. Reuse those UI and integration ideas where useful, but use Qt Multimedia (`MediaPlayer` + `VideoOutput`) for OVRLEY's playback implementation. Do not import Gyroflow's MDK-specific playback layer, stabilization, lens calibration or broad QML project orchestration. [Gyroflow VideoArea](https://github.com/gyroflow/gyroflow/blob/master/src/ui/VideoArea.qml)

Qt Multimedia provides the playback capabilities OVRLEY needs: media source loading, play/pause/stop, duration and position updates, timestamp-based seeking, playback rate and audio control, with `VideoOutput` rendering directly in the Qt Quick scene. Frame-precise seeking is not a migration requirement; prioritize responsive normal playback and basic scrubbing. [Qt MediaPlayer](https://doc.qt.io/qt-6/qml-qtmultimedia-mediaplayer.html) [Qt VideoOutput](https://doc.qt.io/qt-6/qml-qtmultimedia-videooutput.html)

Use one internal timeline coordinate: seconds relative to activity start, as in the existing frontend. At the Qt Multimedia boundary translate its millisecond positions once:

```text
timelineSecond = videoTimestampMs / 1000 + videoSyncOffsetSeconds
videoTimestampMs = (timelineSecond - videoSyncOffsetSeconds) * 1000
timelineMinimum = min(0, videoSyncOffsetSeconds) when video exists
timelineEnd = max(activityDuration, videoEnd) with existing no-media/template policy
```

Make PlaybackController the owner of transport policy. Within the visible video interval, rendered video timestamps advance the playhead. Outside that interval or in non-video background mode, use a monotonic timeline clock. Preserve the current half-open video interval and continuity at clock handoff. Do not bind playhead changes back into seeks indiscriminately; distinguish user seek requests from decoder position reports.

Retain rational source FPS metadata for export and test fractional rates. Step/seek through actual decoder frame/timestamp behavior, especially VFR; do not assume `frame / roundedFPS` identifies every decoded frame. During scrubbing keep only the latest pending request and finish with an exact seek. Associate acknowledgements with source/seek generation so obsolete callbacks cannot move the playhead.

Preserve manual offset drag, local/UTC camera timestamp choice, negative video start, export-range handles, activity/video lanes and timeline zoom/pan. Sync Doctor must preserve candidate evidence, diagnostic-only non-applicable candidates, explicit apply and stale-input rejection. Qt Multimedia replaces the browser/local-HTTP preview playback path only; keep existing ffmpeg encoding and synchronization-analysis pipelines.

## 8. Translations from the existing JSON catalogs

Keep `app/src/i18n/locales/*-translation.json` as the canonical source, including after React removal. Keeping a data-only directory under `app/` does not retain React. There are currently 15 catalogs registered by `i18n/locales.js`, with English fallback configured in `i18n/index.js`.

Recommended approach: a Rust translation service reads/embeds those exact JSON files and exposes key lookup plus named interpolation arguments. Package them from their canonical location; generated resources are build artifacts, never separately edited translation files. Use a shared data-only locale registry if registry reuse requires extraction; update React to consume that registry before native cutover.

QML bindings must explicitly depend on a notified catalog revision when calling a translation method, because a plain method call is not automatically invalidated by an internal Rust locale change. Centralize that binding convention in reusable text/form controls. Keep business errors as stable codes plus arguments where practical; translate at presentation time.

The catalogs contain `{{name}}` substitutions and the React rich-text marker `<0>{{version}}</0>` in the update message. Define a narrow rendering policy for those existing tokens: named interpolation, safe plain-text default, and an explicit presentation treatment of the update emphasis marker. Do not expose raw translated markup indiscriminately or silently strip unknown syntax. Check all catalogs for plural/context tokens and placeholder consistency before deciding the supported grammar. Missing non-English entries may use the existing English fallback; malformed catalog structure or missing required interpolation arguments must be reported.

This avoids a second maintained `.ts` catalog and an i18next runtime in QML. A generated Qt translation pipeline is possible, but adds conversion complexity for the existing JSON keys/interpolation without a demonstrated benefit here. Acceptance includes changing language without restart, long translations, CJK fonts, interpolation, rich-text treatment and fallback behavior.

## 9. Static caching investigation and final widget preview

### What already exists

`render/static_layer.rs` caches a scene-sized image of backdrops, labels and static text-metric parts in a process-wide `HashMap`. The key includes dimensions, scale and debug-formatted scene/backdrop/label/value structures. The visible implementation has no eviction policy and does not explicitly key font-resource revisions.

`prepare_base_rgba` prepares reusable base pixels for video frames. `VideoFrameRenderer::render_rgba` restores them before dynamic drawing. Route and elevation preparation separately build cached remaining-path/profile layers, marker assets and frame states. `PreparedPreviewAssets` separates preparation from rendering.

However, `commands::backend_render_preview_frame` parses inputs, validates configuration, builds dense activity and writes a new PNG for each request. Repeated use of this command is not a suitable live-preview architecture. Some internal caches survive, but expensive request-level preparation and disk output remain.

### Required investigation gate â€” before preview implementation

Use existing profiler buckets and renderer tests to establish preparation versus per-frame cost. Do not claim a performance target has already been achieved. Resolve these questions in Rust without adding QML widget graphics early:

| Change                            | Desired reuse / required investigation                                                                              |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Playhead only                     | Reuse validation, dense data, text/font resolution, static assets and geometry; update only frame-dependent content |
| Widget position/rotation/opacity  | Reuse local widget pixels where valid; determine where current preparation bakes transforms into scene-sized assets |
| Widget text/style/font            | Invalidate affected assets and bounds; include font/resource revision in correctness rules                          |
| Scene size/scale/DPI              | Recreate affected raster assets; distinguish document dimensions from display resolution                            |
| Activity/source/range/FPS change  | Rebuild only data/geometry/frame caches that actually depend on it                                                  |
| Project close or continuous edits | Bound memory and retire stale resources; avoid accumulating one full-scene image per edit                           |
| Overlapping widgets               | Preserve current renderer ordering, alpha, clipping, shadows and static/dynamic interleaving                        |

A single static image can be valid under current draw order; a universal static-under-dynamic split is not automatically correct for arbitrary stacking. Compare existing export order with React composition before choosing per-widget caches or ordered static segments. Resolve discrepancies explicitly rather than introducing stacking behavior as an incidental migration change.

Gate deliverables: measurements, documented cache ownership/keys/invalidation, memory limits, ordering decision and regression fixtures. Reuse the current renderer and make surgical cache changes where evidence requires them.

### Last feature: native widget preview

Add a persistent Rust preview session over immutable validated document/activity revisions. Expose a small in-memory frame-rendering seam reusing existing preparation/drawing code. Start with raster Skia output transferred to a Qt Quick image/texture item; this matches the current raster-oriented core and keeps GPU interoperability out of the initial contract. Measure upload cost before deciding whether a GPU path is required.

Do not use PNG files, base64, whole activity arrays or JSON on the frame path. Use bounded buffers and a latest-frame policy. Respect GUI/render/worker thread ownership and specify pixel format, premultiplied alpha, stride, color handling and DPI. A future GPU route must explicitly solve Qt rendering-backend/Skia resource lifetime; Qt Multimedia's video rendering path does not establish universal Skia interoperability.

QML draws selection and manipulation handles above the rendered overlay; it does not reimplement speed gauges, route plots or text. Obtain widget bounds from Rust layout APIs. Earlier editor phases may expose those geometry APIs without implementing preview pixels.

Parity covers text/metric, time, gradient, linear/arc gauges, heading, lean angle, G-force, lap timer, route, elevation and backdrop families, including all manifest-supported variants. Test font metrics, unit/rounding rules, sparse activity, export boundaries, transforms, transparency and overlapping widgets. Reuse existing Rust widget tests and canvas-parity fixtures where applicable; compare final preview frames with the same Rust export renderer.

## 10. Coarse folder structure

```text
src-tauri/
  Cargo.toml                    # workspace; explicitly select executable
  src/                          # existing Tauri shell until final cutover
  ovrley_core/                  # existing processing/rendering/encoding
  ovrley_app/                   # proposed shared application crate
    src/
      session.rs
      editor/                   # document, operations, history, geometry
      projects/                 # extracted archive I/O and transactions
      templates/
      media/
      playback/
      settings/
      export/
  ovrley_qml/
    Cargo.toml
    build.rs
    src/
      main.rs
      controllers/
      models/
      platform/                 # Qt paths/dialog integration, update/restart
      translations.rs
      preview/                  # added in final feature phase
    cpp/
      ui_live_reload.cpp
      quick_preview_item.cpp    # only if required by selected binding seam
    qml/
      Main.qml                  # stable window/dev error surface
      App.qml                   # reloadable application content
      theme/
      controls/
      shell/
      projects/
      templates/
      activity/
      video/
      timeline/
      sync/
      scene/
      widgets/                  # catalog and editors, not renderers
      editor/                   # interaction surface and handles
      export/
      updates/
    tests/
assets/                         # existing canonical manifests/assets
app/src/i18n/locales/            # retained canonical JSON, even after cutover
```

Add workspace membership only with package-specific commands/default-member policy that keeps ordinary Tauri development from acquiring a Qt build requirement. Keep Qt/C++ build dependencies confined to `ovrley_qml`. Shared project extraction must not require linking the Tauri `app_lib` into the Qt executable.

## 11. Ordered migration backlog and completion gates

These steps are sequential feature slices. Foundation tasks establish dependencies; they do not justify migrating several features at once. Run appropriate checks during implementation, with explicit permission before any build as required by the repository.

| Step                                  | Work                                                                                                                                       | Dependencies and completion gate                                                                                                                                                                    |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Native foundation                  | Select/pin Qt/bindings/toolchain; add separate executable/dev command, resource paths, worker notification seam and reload                 | Native window and a small model work on Windows/macOS; short Qt Multimedia playback/scrubbing feasibility check; React commands still work; no widget preview                                       |
| 2. Shared document foundation         | Extract existing project/archive types and reusable services; define editable session/transactions and history                             | Existing `.oly` and template fixtures round-trip; required invalid input fails; current Tauri contracts remain functional                                                                           |
| 3. Theme and basic controls           | Port semantic theme, fonts, translations and basic form controls                                                                           | Language change and committed-field behavior verified; malformed catalogs/settings visible                                                                                                          |
| 4. Application shell                  | Header/window controls, tool rail, drawers, control-panel tabs, error/busy/dialog surfaces, shortcuts                                      | Shell works without preview/media; close and focus behavior tested; unavailable features are explicit                                                                                               |
| 5. Templates                          | Catalog, load/create/save/save-as, saved baseline, remembered template                                                                     | Existing templates remain interchangeable with React; cancellation and invalid template do not replace current state                                                                                |
| 6. Scene settings                     | Resolution/aspect ratio, FPS/update rate, global font/style/defaults and range settings                                                    | Typed Rust operations and history; scene changes serialize correctly; video-derived options activate later                                                                                          |
| 7. Activity import                    | Native CSV/VBO, summary/removal and current Rust finalization/telemetry entry points                                                       | Import activation is atomic; metadata/gaps/provenance preserved; excluded format support remains a tracked prerequisite                                                                             |
| 8. Video import/playback              | Native paths, probe metadata, Qt Multimedia loading, transport, basic scrubbing, warnings, removal and embedded activity source lifecycle  | H.264/HEVC, rotation, metadata, reload and source replacement tested; existing Rust exporters unchanged                                                                                             |
| 9. Timeline and manual sync           | Activity/video lanes, drag offset, timezone choice, scrub/step, zoom/pan and export range                                                  | Negative offsets, gaps/out-of-video playback, fractional FPS, clock handoff and exact scrub completion pass                                                                                         |
| 10. Sync Doctor                       | Candidate/evidence presentation, job progress/cancel and explicit apply                                                                    | Source revision rejection, diagnostic-only candidates and history integration match current feature                                                                                                 |
| 11. Projects                          | Startup project browser, new/open/save/save-as, source relinking, dirty state and close guards                                             | Depends on real template/activity/video preparation; cancelled or failed open preserves session; cross-frontend round-trip passes                                                                   |
| 12. Widget catalog/editor             | Drawer/add/remove/duplicate, per-family fields and display-variant switching                                                               | Migrate each family separately: text/time/backdrop, standard text metrics, linear, arc, gradient, heading, lean angle, G-force, lap timer, route, elevation; preserve inactive settings and history |
| 13. Editor interaction                | Selection, multiselection, move/resize/scale/rotate, keyboard editing, zoom/backgrounds                                                    | Use bounds/handles and no widget graphics; one transaction per gesture, cancel semantics and scene-coordinate math pass                                                                             |
| 14. Export                            | Transparent/composite dialogs, codec capability, bitrate, range/output selection, progress/cancel/open output                              | Render real files through existing core without depending on preview; verify overwrite/cancel/error and output-range/FPS behavior                                                                   |
| 15. Native lifecycle/release services | Updates, restart, installed/portable paths, packaging configuration and diagnostics                                                        | Replace Tauri-dependent updater flow; retain distribution behavior; package Qt/Qt Multimedia plus existing ffmpeg/fonts/templates/resources                                                         |
| 16. Parser prerequisite closure       | Consume separately delivered FIT/GPX/SRT/IGC Rust support                                                                                  | No parser implementation is specified here; all currently supported formats and project reload paths must work before proceeding to final parity                                                    |
| 17. Cache gate                        | Perform the static caching investigation and required Rust corrections from section 9                                                      | Measured, bounded, correct cache model established; existing export regressions pass                                                                                                                |
| 18. Widget preview â€” LAST FEATURE   | Bind persistent Rust renderer to QML and integrate prepared assets with playhead/editor revisions                                          | All widget families and interactive edits pass visual/export parity and measured performance acceptance                                                                                             |
| 19. Cutover                           | End-to-end acceptance, approved native release validation, switch default commands/package entry point, remove React/Tauri UI dependencies | Only after all prior gates; keep canonical translations/manifests and Rust core; no React runtime remains                                                                                           |

Projects intentionally follow source import and synchronization: current hydration requires prepared activity and video, restored offsets and render/timeline settings. Shell and reusable components migrate early, but their feature content is connected only when its owner exists. Export needs valid configuration/media, not live widget-preview pixels.

## 12. Risks, verification and removal checklist

| Risk/blocker                                                       | Required resolution                                                                                                                     |
| ------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------- |
| Qt/binding/Qt Multimedia compatibility and native dependencies     | Pin and verify the full supported platform matrix in the foundation; do not upgrade the renderer merely to mirror Gyroflow              |
| Existing shell-owned Rust functionality inaccessible without Tauri | Extract project I/O, resource policy and filesystem logic; keep platform wrappers thin and independently testable                       |
| Durable editor data differs from render-normalized data            | Preserve inactive variants/default semantics and explicit derived render plans; cross-frontend file tests                               |
| Formats still parsed in JavaScript                                 | Separately delivered native parsers block complete removal; never hide missing support                                                  |
| Clock races and negative offsets                                   | One transport policy owner, explicit Qt Multimedia boundary units, generation-tagged seeks and boundary fixtures                        |
| Static cache growth and rendering order                            | Complete measured cache gate before widget preview; keep output semantics stable                                                        |
| QML reload/object lifetime                                         | Stable Rust owners, explicit destruction, reconnect tests and controlled singleton/engine restart                                       |
| Project/preferences collisions between executables                 | Independent runtime sessions; serialize or detect conflicting file writes; avoid concurrent settings overwrites during side-by-side use |
| Translations contain i18next/React syntax                          | Audit catalogs and implement only a documented shared grammar; no second maintained catalog                                             |
| Native updater/package parity                                      | Preserve installed/portable behavior and establish native update artifacts before cutover; Tauri plugin APIs are not portable services  |

For each feature, carry over behavior-focused fixtures from `app/src/tests/` into Rust service tests and Qt interaction tests as appropriate. Keep existing React checks relevant to shared changes. Particularly useful suites cover project lifecycle/snapshots, media replacement, widget draft/history behavior, player timing/scrubbing, render configuration/output paths and visual-sync candidate application. Test model insertion/removal notifications, queued job delivery, errors and cancellation at native boundaries.

Final acceptance must exercise: import each supported format; load/save/relink a project; edit every widget family; undo/redo; switch templates/language/theme; synchronize video manually and through analysis; scrub/step/play across boundaries; export transparent and composite video; cancel jobs; close with unsaved changes; reload QML in development; and launch the packaged native app without a development environment. Record preview preparation/frame/upload timings and memory over continuous edits on declared reference hardware rather than asserting unmeasured speedups.

At cutover remove React/React DOM, Zustand/Immer/Zundo frontend use, Radix/shadcn React components, Moveable, Tailwind/browser styling, Vite, React i18n runtime, frontend Tauri plugins, JSX entry points and obsolete DOM/SVG tests. The frontend is JavaScript/JSX, not TypeScript; incidental type packages do not imply a TypeScript migration. Preserve behavior fixtures that still validate shared contracts.

Remove the Tauri executable and loopback video server only after their native replacements and extracted services are complete. Rewrite development/release/portable packaging commands and CI around the native executable, retaining OVRLEY naming and existing core resources. Keep the JSON translation directory and shared manifests. No runtime webview, React code, JavaScript parser sidecar or old IPC bridge is needed in the final application.

This report verifies source-level architecture and upstream integration examples. It does not claim that Qt Multimedia playback has been tested on either platform, or that cache/performance targets have been achieved; those are explicit execution gates above.
