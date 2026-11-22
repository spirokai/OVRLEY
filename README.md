<div align="center">
  <img src="assets/logo.svg" alt="OVRLEY Logo" height="60" />
</div>

<br />

<div align="center">
  <img src="docs/hero.avif" alt="OVRLEY Dashboard Interface" width="100%" />
  <img src="docs/hero2.avif" alt="OVRLEY Dashboard Interface" width="100%" />
  <img src="docs/widgets-showcase.avif" alt="OVRLEY Widget Showcase" width="100%" />
  <img src="docs/maps-showcase.avif" alt="OVRLEY Map Showcase" width="100%" />
</div>

<br />

<div align="center">
  <h1><a href="https://www.ovrley.cc">WWW.OVRLEY.CC</a></h1>
  <br/>
  <br/>
  <p>OVRLEY turns .fit, .gpx, .tcx, .srt, .igc, .csv, .vbo, dive logs, and embedded .mp4 telemetry from activities, flights, dives, motorsports, and cameras into fully customizable overlays for your videos. Its visual editor lets you design the overlay while a fast native rendering engine handles the final export.</p>
</div>

## Features

- **Broad Import Support**: Import `.fit`, `.gpx`, `.tcx`, `.srt`, `.igc`, `.csv`, `.vbo`, and dive log files, or use telemetry embedded in supported camera videos. OVRLEY works with data from fitness devices, dive computers, drones, action cameras, and motorsport loggers.
- **Interactive Editor**: Drag-and-drop visual editor with live previews, reusable templates, undo/redo, centering guides, and data-availability indicators.
- **Full Customization**: Shape every widget with your own styling, color, gradient fills, font, and size.
- **50+ Widgets**: Show speed, heart rate and heart zones, power, cadence, heading compass, G-force, lean angle, RPM, throttle, brake position, lap times, GPS position, ascent, running splits, diving profile, camera settings, and much more.
- **Charts & Splits**: Rolling time charts with up to 5 fully customizable metric graphs, and a running splits table with a live-updating current split.
- **Highly Customizable Animated Gauges**: Display your data using linear, arc, GT-styled arc, or corner gauges with bar, fill, and segmented-bar styles. Dedicated G-force and lean-angle gauges are also included. Customize their size, colors, gradients, borders, labels, angles, and other visual details.
- **Lap Timing**: Show the current lap, best lap, live time difference, or a complete lap log. Individual laps can be toggled on or off without editing the activity file.
- **Scuba Diving**: Dedicated dive metric widgets and a dive profile showing depth and the decompression ceiling.
- **Transparent Overlays**: Export with alpha channel (in real-time speed) for maximum flexibility in post-production. Use in Premiere Pro, DaVinci Resolve, Final Cut Pro, or any editor that supports layered video.
- **Final Video Export**: Export your final video footage with overlays baked in - no separate compositing step required. Multiple codecs, hardware-accelerated pipelines, and custom bitrate.
- **Hardware-Accelerated Export**: Use supported NVIDIA, AMD, Intel, or Apple hardware (and VA-API on Linux) to drastically speed up video export. A typical cycling overlay exports at around 100-150 fps (H264@4K, RTX 3070-4070).
- **Real-Time Preview**: Preview inside the app at up to the export's resolution, with the widgets updating at the project's frame rate divided by their update rate. The preview draws the widgets with the same rendering engine as the final export.
- **Flexible Video Sync**: Automatically sync telemetry using camera timestamps, GPS timestamps or supported timestamps in video filenames. You can also adjust the sync by multiple approaches. Negative offsets are supported when the video starts before the telemetry.
- **Video-Editing Timeline**: See video and telemetry as separate tracks, move through them frame by frame, zoom and pan, and choose exactly which part to export.
- **Multiple Videos**: Load many clips at once, for example camera files split at a size limit or separate recordings. OVRLEY places every clip by its recording time, also before or after the activity, warns about overlaps and clips that differ in resolution or frame rate, and previews them continuously.
- **Deleted Parts and Transitions**: Cut unwanted parts out of any clip and choose a transition for every join between clips or around a deleted part from 58 animated `xfade` transitions. Clips recorded back to back join seamlessly by default.
- **One Video or Separate Files**: Export all clips together as one video with their transitions, or each clip to its own file, as final footage or transparent overlays.
- **Keyboard Shortcuts**: Use shortcuts for playback, editing, timeline navigation, and other common actions. Settings lists them all under Shortcuts.
- **Automatic Updates**: Installer builds can check for new versions at startup and update with your permission.
- **100% Free & Local**: No subscriptions. All data processing, rendering, and encoding happen on your machine, and your files are never uploaded. Two features use the network: maps download OpenFreeMap tiles for the area they show, and builds that update themselves check GitHub for updates.
- **Windows, macOS (Apple Silicon), and Linux**: Choose an installer build with automatic updates or one of the available portable/manual packages.

> ### Using OVRLEY: Download the package for your operating system from the [latest release](https://github.com/spirokai/OVRLEY/releases/latest). Windows offers an installer and a portable ZIP, macOS uses a DMG, and Linux offers an AppImage, a DEB package, and a Flatpak. Installer, DMG, and AppImage builds can notify you when an update is available.

## Supported Files

- **Fitness and outdoor activities:** `.fit`, `.gpx`, and `.tcx`
- **Action cameras and drones:** `.srt` files and telemetry embedded in supported `.mp4` videos, including footage from DJI, GoPro, and Insta360 devices
- **Flight logs:** `.igc`
- **Dive logs:** `.fit`, `.uddf`, `.ssrf`, and `.xml`
- **Motorsport and custom data logs:** `.csv` and `.vbo`
- **Video footage:** `.mp4`, `.mov`, and `.mkv`

CSV support includes common exports from AiM, RaceChrono, RaceBox, LapLegend, TrackAddict, AirData, and Torque Pro. CSV and VBO files do not have one universal layout, so please report a file that OVRLEY does not recognize correctly.

## Project Summary

OVRLEY is a native Rust application. Its crates are in `crates/`:

| Crate              | Role                                                                                    |
| ------------------ | --------------------------------------------------------------------------------------- |
| `ovrley_ui`        | The window: the Slint interface, the glue to the app's logic, and the `ovrley` binary   |
| `ovrley_app`       | The app's state, actions and logic, without any UI                                      |
| `ovrley_services`  | Desktop services: projects, rasters, map tiles and styles, video frames, the updater   |
| `ovrley_mcp`       | The MCP server for AI agents                                                            |
| `ovrley_playback`  | Video playback with MDK, into the window's GPU textures                                 |
| `ovrley_core`      | Activity parsing, validation, the export renderer and FFmpeg encoding                   |
| `ovrley_widgets`   | The widgets, drawn with Skia, for the preview and the export alike                      |
| `ovrley_map`       | The map renderer, on wgpu                                                               |

[ARCHITECTURE.md](docs/ARCHITECTURE.md) explains how they work together, and [MCP.md](docs/MCP.md) how to let an AI agent drive the app.

## Export & Codecs

OVRLEY supports both exporting transparent overlays as well as final footage with overlays baked in. It offers multiple export pipelines for different hardware and workflow needs:

**Transparent Overlays**

- `ProRes CPU`: The safest general recommendation. High quality and excellent compatibility with professional editors like Premiere Pro or DaVinci Resolve.
- `ProRes VideoToolbox`: Apple's hardware ProRes encoder, on macOS.
- `ProRes Vulkan`: A newer, GPU-accelerated option for ProRes. Frees up your CPU for other tasks, though hardware/driver compatibility may vary.
- `QT RLE`: Offers significantly smaller file sizes and extremely fast export times, though with slightly lower compatibility in some basic video players.

**MP4 Exports (overlays baked in)**

- `H.264/H.265`: Industry-standard codecs for maximum compatibility. Custom bitrate setting to balance quality and file size.

**Hardware-Acceleration Options**, where the system supports them:

| System  | H.264 and H.265                                          | ProRes                |
| ------- | -------------------------------------------------------- | --------------------- |
| Windows | NVIDIA NVENC (with or without CUDA), AMD AMF, Intel QSV  | Vulkan                |
| macOS   | VideoToolbox                                             | VideoToolbox, Vulkan  |
| Linux   | NVIDIA NVENC (with or without CUDA), VA-API              | Vulkan                |

CPU export is always offered.

## Dependencies

- **Rust via rustup**: rustup installs the toolchain pinned in [`rust-toolchain.toml`](rust-toolchain.toml).
- **A C++17 compiler and linker**: the MSVC Build Tools on Windows, the Xcode Command Line Tools on macOS, `build-essential` on Linux. Linux also needs `pkg-config` and the development packages of fontconfig and FreeType, and the system libraries that the player links; the release workflow ([`workflow_linux.yml`](.github/workflows/workflow_linux.yml)) installs the full list.
- **Node.js 24 and pnpm 10.25.0**, for the scripts: installation, packaging, releases and benchmarks.
- **FFmpeg** and the **MDK SDK** (video playback): `pnpm install` downloads both into `vendor/`. On Windows, the SDK is a 7z archive: Windows' own `tar` or 7-Zip extracts it.

Skia comes from rust-skia's prebuilt binaries.

## Dev Installation

Run from the repository root:

```bash
pnpm install --frozen-lockfile
```

An MDK SDK kept elsewhere is used through `MDK_SDK`, set in the shell that runs cargo (see [`.env.example`](.env.example)).

## Development

```bash
pnpm dev:native        # The app
pnpm dev:native:live   # The app with Slint live preview: saving a .slint file reloads the UI
pnpm showcase:native   # The component showcase
```

`pnpm dev:native:live` sets its environment variable in POSIX shell syntax, so on Windows run it from such a shell.

## Available Test Suites

```bash
cargo test --workspace --features ovrley_ui/dev-windows   # Everything that is not ignored
pnpm test:native-ui                                       # The app, services, MCP and UI component tests
```

| Suite                | Purpose                                                                                  | Docs                                                |
| -------------------- | ---------------------------------------------------------------------------------------- | --------------------------------------------------- |
| **Render Baselines** | Rendered frames and videos compared with committed baselines (ignored; run by hand)      | [`BASELINES.md`](crates/ovrley_core/BASELINES.md)   |

## Benchmarks

| Benchmark                   | Measures                                                         | Docs                                                    |
| --------------------------- | ---------------------------------------------------------------- | ------------------------------------------------------- |
| **Encoding**                | Job time and file size per codec, and per widget update rate     | [`BENCHMARKS.md`](crates/ovrley_core/BENCHMARKS.md)     |
| **Widgets**                 | Each widget's preview and export cost                            | [`widgets`](benchmarks/widgets/README.md)               |
| **Widget interactions**     | Group drags and resizes: the update loop and the preview redraw  | [`widget-interactions`](benchmarks/widget-interactions/README.md) |
| **Map labels**              | What map labels cost in export                                   | [`map-labels`](benchmarks/map-labels/README.md)         |
| **Overlay pipeline**        | Where composite export time goes in FFmpeg                       | [`run.mjs`](benchmarks/overlay-pipeline/run.mjs)        |

## Building & Compatibility

Packaging needs cargo-packager at the pinned revision:

```bash
pnpm install:packager
pnpm build
```

| System  | `pnpm build` makes                                                                               |
| ------- | ------------------------------------------------------------------------------------------------ |
| Windows | The NSIS installer and the portable ZIP                                                          |
| macOS   | `OVRLEY.app` and the portable ZIP; with a signing key also the updater's `.app.tar.gz`           |
| Linux   | The Debian package, without the no-update marker that its release workflow adds                  |

The DMG, the AppImage, the Flatpak and the Microsoft Store package are built by the release workflows. See [packaging](packaging/README.md).

---

## Acknowledgements

Big credit goes to [@walkersutton](https://github.com/walkersutton) and his [Cyclemetry](https://github.com/walkersutton/cyclemetry) repository, which was the original inspiration and trigger for this project. Without his initial work, OVRLEY likely would not have happened.
