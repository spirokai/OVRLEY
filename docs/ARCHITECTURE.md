# OVRLEY architecture

OVRLEY is a native desktop app in Rust. It turns activity data into video overlays: a Slint window edits a document of widgets, a Skia engine draws them for the preview and the export alike, and FFmpeg encodes.

## Crates

```
ovrley_ui ──► ovrley_app ──► ovrley_services ──► ovrley_core ──► ovrley_widgets ──► ovrley_map
    │              ▲
    ├──► ovrley_mcp┘
    └──► ovrley_playback
```

A crate may also use any crate further right. `ovrley_playback` depends on no other OVRLEY crate, and only `ovrley_ui` depends on Slint.

| Crate             | Owns                                                                                                                    | Does not own                        |
| ----------------- | ----------------------------------------------------------------------------------------------------------------------- | ----------------------------------- |
| `ovrley_ui`       | The Slint markup, one projection and binding module per feature, the platform glue, the GPU device, the binary          | App logic                           |
| `ovrley_app`      | The store, the actions, the reconcile steps, undo, the service traits, translations of runtime strings                  | UI                                  |
| `ovrley_mcp`      | The tool catalog and its schemas, MCP over stdio, the socket between `--mcp` and the window                             | Logic the UI lacks                  |
| `ovrley_services` | `.oly` archives, raster images, map styles and tiles, thumbnails and filmstrips, disk caches, runtime roots, the updater | The store                           |
| `ovrley_playback` | MDK, its FFI, and the interop that puts frames into the host's wgpu textures                                            | Slint, the store                    |
| `ovrley_core`     | Activity parsing and finalization, template and render-config validation, export rendering, FFmpeg, render jobs         | Windows and preferences             |
| `ovrley_widgets`  | The widget catalog and document model, sampling, layout and Skia drawing, the preview session                           | Files and encoding                  |
| `ovrley_map`      | Cameras and styles; the wgpu renderer (`gpu` feature); the style validator (`validate` feature)                         | Fetching tiles in the app           |

## The update loop

`ovrley_app/src/store/`:

- **`Store`** holds everything the UI shows: the slices and each feature's state.
- **`Services`** holds what actions may ask for (dialogs, workers, preferences, video tools, maps, renders, updates), behind traits with fakes for tests.
- **`Dispatcher`** owns both and runs one action at a time on the UI thread. A dispatch:
  1. runs the action;
  2. runs the reconcile steps in their one fixed order (`reconcile.rs`);
  3. records at most one undo step, by comparing the undoable state before and after;
  4. runs the projections, which write Slint properties that changed.

A dispatch requested during another is queued and runs right after. Actions never block: blocking work runs on `services.workers`, and its result returns as an action through `UiPost`.

Frequent events are coalesced before they reach the loop: render and update progress keep only the newest value, and the preview renders one frame per event-loop turn however many actions asked for one.

## The UI

`crates/ovrley_ui/`:

- `ui/components/` holds OVRLEY's own controls (no std-widgets), `ui/theme.slint` the tokens and `ui/window.slint` the scaled window every window derives from.
- `ui/features/` holds each feature's markup and globals, and `src/features/` the projections that write the store into them and the callbacks that dispatch actions: `shell`, `documents` (projects and templates), `editor`, `widgets` (inspector, scene settings, drawer), `toolbar`, `video` (import dialog, seconds fields; `import.rs` and `playback.rs`), `player`, `transitions`, `sync` and `render`.
- `src/projection.rs` holds what the projections share (change-only writes, keyed models, images, icons, the Slint↔app enum conversions); `src/logic.rs` binds the component logic globals.
- `src/app.rs` composes everything and runs the event loop. `src/main.rs` starts up: logging, paths and distribution, preferences, the store, the GPU, then the window.
- `src/platform/` holds backend selection and the GPU device, window chrome, raw window events and file drops, key conversion, the traffic lights on macOS and the Wayland drop listener. The AppImage's menu entry is `ovrley_services::runtime`'s.

One wgpu device is shared by the window and the video players: Metal on macOS, DX12 on Windows, Vulkan on Linux. On a software adapter the window uses Skia's CPU rasterizer instead. Slint's Skia renderer is patched (`patches/README.md`).

## Widgets and preview

- `assets/standard-widgets.json` and `assets/standard-metrics.json` declare the widgets, metrics, displays, defaults and editor limits. `ovrley_widgets/build.rs` generates the metric types and icon tables from them.
- A document is a scene and five ordered collections of widgets. `ovrley_widgets::model::document` is the one place that edits widget data.
- A drag or resize writes gesture patches as drafts and commits on release, as one undo step.
- The preview (`ovrley_app/src/features/widget_preview.rs` over `ovrley_widgets::preview::PreviewSession`) draws each widget into its own surface, at the density it is shown at and never above export resolution.
- Export builds the same widget families, so there is one drawing path.
- Maps in the preview stream frames from a map thread and never wait for one.

## Activities and media

- Activity files: GPX, FIT, TCX, SRT, IGC, CSV, VBO, UDDF, SSRF and Subsurface XML (`ovrley_core/src/activity/`). Every parser ends in the same finalization, which derives, smooths and resamples by the manifests' policies.
- Videos: MP4, MOV and MKV. Camera telemetry is read from the file (`ovrley_core/src/media/`).
- Clips sit on one timeline by recording time. Cuts, exclusions and per-join transitions form the output program (`ovrley_app/src/lib/video_program/`).
- Playback uses two MDK players that take turns at cuts. A transition plays as a short FFmpeg-rendered proxy. The zero-copy targets are Metal on macOS, D3D11 on Windows, and OpenGL into Vulkan memory on Linux.
- Sync Doctor matches landmarks to stops, turns and places in the activity. The clip the landmarks were marked in holds the sync: a candidate places it and every other clip follows (`ovrley_app/src/features/sync/`).

## Export

1. The app builds the editor-effective document: every default materialized (`lib/template.rs`, `features/render_video/request.rs`).
2. `ovrley_core::normalize` validates it once. Missing or malformed render values fail; nothing is guessed.
3. `render_jobs` reserves the one renderer and plans the job: a transparent overlay, one combined video, or one file per video.
4. The map job downloads its tiles before the first frame, so a missing tile fails the export instead of leaving a gap.
5. Workers draw into an atlas of the widgets' tiles, and raw frames go in order to one supervised FFmpeg process (RGBA, or `yuva420p` for graphs that blend in YUV).
6. Progress returns through a sink. Cancellation is checked at frame boundaries.

The codec catalog and its detection live in `ovrley_core/src/encode/ffmpeg/`. The render dialog offers only its own grid of them, per OS (`features/render_video/codecs.rs`).

## MCP

`ovrley --mcp` is a separate process without a window. It answers `initialize` and `tools/list` from the compiled-in catalog and forwards each call over a local socket (a named pipe on Windows) to the first window that opened. There the call is one dispatch, refused unless "Allow agent edits" is on. A call that waits for a worker or for the person answers later, without a timeout. Standard output carries only the protocol: the process redirects C's stdout, because MDK prints a banner there. See [MCP.md](MCP.md).

## Files

| What                      | Where                                                                  |
| ------------------------- | ---------------------------------------------------------------------- |
| Preferences               | `<data dir>/com.ovrley.desktop/ovrley-settings.json`                   |
| Agent socket              | `<data dir>/com.ovrley.desktop/mcp/ovrley.sock`; on Linux under `$XDG_RUNTIME_DIR/com.ovrley.desktop/` when that is set; a named pipe on Windows |
| Map tiles, filmstrips     | `<cache dir>/com.ovrley.desktop/tiles`, `…/filmstrips`                 |
| User data, default output | `Documents/OVRLEY`                                                     |
| User templates            | `Documents/OVRLEY/templates`                                           |
| User map styles           | `Documents/OVRLEY/map-styles`                                          |
| Scratch files             | `Documents/OVRLEY/.runtime`                                            |

`<data dir>` and `<cache dir>` are the platform's (`dirs` crate). The preferences keys and the folders are those of the earlier Tauri releases, so an update carries them over. Templates are JSON (`ovrley-template`), projects are `.oly` archives; each has one ingress that migrates older versions and validates.

Translations are gettext catalogs in `locales/`, read by Slint's `@tr` and by `ovrley_app::i18n`.

## Packaging and updates

`scripts/package.mjs` (`pnpm build`) builds the binary, stages it with the notices and the player's libraries in `target/packaging/`, and runs cargo-packager. `ovrley_services::runtime` holds the one rule for where a package's resources are:

| Package                             | Resources                 | Player's libraries        | Updater                          |
| ----------------------------------- | ------------------------- | ------------------------- | -------------------------------- |
| Windows installer                   | beside `app.exe`          | beside it                 | on                               |
| Windows portable, Microsoft Store   | beside `OVRLEY.exe`       | beside it                 | off (`.ovrley-portable` marker)  |
| macOS `.app` (DMG)                  | `Contents/Resources`      | `Contents/Frameworks`     | on                               |
| AppImage                            | beside the binary         | `lib/` beside it          | on                               |
| Debian package                      | `/usr/lib/OVRLEY`         | `/usr/lib/OVRLEY/lib`     | off (marker)                     |
| Flatpak                             | `/app/lib/OVRLEY`         | `/app/lib/OVRLEY/lib`     | off (marker)                     |

A debug build reads the checkout instead and never checks for updates.

The updater checks `latest.json` once at startup. It verifies the package's minisign signature and the version signed into it. Windows hands over to the installer and exits; macOS and the AppImage swap in place, then the app starts the new version after its own window, preferences and socket are closed. `latest.json` also serves installed Tauri releases, so it keeps their keys. See [packaging](../packaging/README.md).
