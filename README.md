<div align="center">
  <img src="app/public/logo.svg" alt="OVRLEY Logo" height="60" />
</div>

<br />

<div align="center">
  <img src="docs/hero.avif" alt="OVRLEY Dashboard Interface" width="100%" />
  <img src="docs/hero2.avif" alt="OVRLEY Dashboard Interface" width="100%" />
  <img src="docs/gauge-showcase.gif" alt="OVRLEY Gauge Showcase" width="100%" />
</div>

<br />

<div align="center">
  <h1><a href="https://www.ovrley.cc">WWW.OVRLEY.CC</a></h1>
  <br/>
  <br/>
  <p>OVRLEY turns .fit, .gpx, .srt, .igc, .csv, .vbo, and embedded .mp4 telemetry from activities, flights, motorsports, and cameras into fully customizable overlays for your videos. Its visual editor lets you design the overlay while a fast native rendering engine handles the final export.</p>
</div>

## Features

- **Broad Import Support**: Import `.fit`, `.gpx`, `.srt`, `.igc`, `.csv`, and `.vbo` activity files, or use telemetry embedded in supported camera videos. OVRLEY works with data from fitness devices, drones, action cameras, and motorsport loggers.
- **Interactive Editor**: Drag-and-drop visual editor with live previews, reusable templates, undo/redo, centering guides, and data-availability indicators.
- **Full Customization**: Shape every widget with your own styling, color, font, and size.
- **40+ Widgets**: Show speed, heart rate, power, cadence, G-force, lean angle, RPM, throttle, brake position, lap times, GPS position, ascent, camera settings, and much more.
- **Highly Customizable Animated Gauges**: Display your data using linear, arc, or corner gauges with bar, fill, and segmented-bar styles. Dedicated G-force and lean-angle gauges are also included. Customize their size, colors, borders, labels, angles, and other visual details.
- **Lap Timing**: Show the current lap, best lap, live time difference, or a complete lap log from supported CSV and VBO files.
- **Transparent Overlays**: Export with alpha channel (in real-time speed) for maximum flexibility in post-production. Use in Premiere Pro, DaVinci Resolve, Final Cut Pro, or any editor that supports layered video.
- **Final Video Export**: Export your final video footage with overlays baked in - no separate compositing step required. Multiple codecs, hardware-accelerated pipelines, and custom bitrate.
- **Hardware-Accelerated Export**: Use supported NVIDIA, AMD, Intel, or Apple hardware to drastically speed up video export.
- **Real-Time Preview**: Preview at up to 4K/30 fps directly inside the app before you export.
- **Flexible Video Sync**: Automatically sync telemetry using camera timestamps, GPS timestamps or supported timestamps in video filenames. You can also adjust the sync by multiple approaches. Negative offsets are supported when the video starts before the telemetry.
- **Video-Editing Timeline**: See video and telemetry as separate tracks, move through them frame by frame, zoom and pan, and choose exactly which part to export.
- **Keyboard Shortcuts**: Use shortcuts for playback, editing, timeline navigation, and other common actions. The help button in the top toolbar shows the full list.
- **Automatic Updates**: Installer builds can check for new versions at startup and update with your permission.
- **100% Free & Offline**: No subscriptions. All data processing, rendering, and encoding happen locally on your machine. Your data never leaves your computer.
- **Windows, macOS, and Linux**: Choose an installer build with automatic updates or one of the available portable/manual packages.

> ### Using OVRLEY: Download the package for your operating system from the [latest release](https://github.com/spirokai/OVRLEY/releases/latest). Windows offers an installer and a portable ZIP, macOS uses a DMG, and Linux offers an AppImage and a DEB package. Installer, DMG, and AppImage builds can notify you when an update is available.

## Supported Files

- **Fitness and outdoor activities:** `.fit` and `.gpx`
- **Action cameras and drones:** `.srt` files and telemetry embedded in supported `.mp4` videos, including footage from DJI, GoPro, and Insta360 devices
- **Flight logs:** `.igc`
- **Motorsport and custom data logs:** `.csv` and `.vbo`
- **Video footage:** `.mp4`, `.mov`, and `.mkv`

CSV support includes common exports from AiM, RaceChrono, RaceBox, LapLegend, TrackAddict, AirData, and Torque Pro. CSV and VBO files do not have one universal layout, so please report a file that OVRLEY does not recognize correctly.

## Project Summary

The codebase utilizes a dual-stack architecture to maximize both the user experience and computational performance:

- **Frontend Interface**: A React application built with Vite (`app`), providing a seamless editing environment for overlay positioning, aesthetic customization, and metric synchronization.
- **Core Engine**: A standalone Rust backend (`ovrley_core`) utilizing Skia for frame-perfect 2D graphics rendering and FFmpeg for high-speed video compositing.
- **Desktop Shell**: A lightweight Tauri wrapper that bridges the web-based interface with the native Rust core, granting access to the local file system and hardware resources.

## Export & Codecs

OVRLEY supports both exporting transparent overlays as well as final footage with overlays baked in. It offers multiple export pipelines for different hardware and workflow needs:

**Transparent Overlays**

- `ProRes CPU`: The safest general recommendation. High quality and excellent compatibility with professional editors like Premiere Pro or DaVinci Resolve.
- `ProRes Vulkan`: A newer, GPU-accelerated option for ProRes. Frees up your CPU for other tasks, though hardware/driver compatibility may vary.
- `QT RLE`: Offers significantly smaller file sizes and extremely fast export times, though with slightly lower compatibility in some basic video players.

**MP4 Exports (overlays baked in)**

- `H.264/H.265`: Industry-standard codecs for maximum compatibility. Custom bitrate setting to balance quality and file size.

**Hardware-Acceleration Options**: `NVENC/CUDA/AMF/QSV/VideoToolbox` hardware-accelerated encoding options to take full advantage of your system and GPU to speed up export times.

## Dependencies

- **Node.js 24 and pnpm 10.25.0**: one workspace and root lockfile. Frontend tooling uses **Vite+** (`vp`) for development, builds, linting, formatting, and Vitest tests.
- **Rust via rustup**: native toolchain listed in [`.tool-versions`](.tool-versions), plus **Rust 1.98.1** with the `wasm32-unknown-emscripten` target for browser preview bindings.
- **Tauri v2 platform prerequisites**: native compiler and webview development tools (MSVC Build Tools and WebView2 on Windows, Xcode Command Line Tools on macOS, system development packages on Linux).
- **Emscripten 6.0.9** installed and activated through emsdk, plus **Python 3.10+** and **Git**.
- **FFmpeg 8.1+ full build**: downloaded automatically into `vendor/ffmpeg` during installation.

The prebuilt Skia WASM archive ships in [`vendor/skia-wasm`](vendor/skia-wasm). Ordinary bindings builds reuse it; GN, Ninja, and libclang are only needed to rebuild Skia from source. Native Skia uses upstream prebuilt binaries.

## Dev Installation

After cloning, install the dependencies above and activate Emscripten 6.0.9 in your emsdk installation. Run the following from the repository root:

```bash
rustup toolchain install 1.98.1 --profile minimal --target wasm32-unknown-emscripten
pnpm install --frozen-lockfile
```

Copy [`.env.example`](.env.example) to `.env.local` and set `EMSDK` to your SDK directory. Set `PYTHON` if the Python on PATH is older than 3.10. Both the Tauri launcher and WASM scripts load this file; shell variables take precedence. `LIBCLANG_PATH` is only needed for rebuilding Skia.

Use pnpm for all packages; no separate install in `app/` is needed.

## Development

Run commands from the repository root:

```bash
pnpm dev                       # Tauri app with frontend hot reload
pnpm dev:frontend              # Frontend server only
pnpm wasm:build                # Build JS/WASM bindings using stored Skia
```

The Vite plugin builds WASM bindings on first preview access and rebuilds/reloads when renderer sources change. Production frontend builds also generate and include the bindings.

### Rebuilding Skia from source

Only needed when updating the stored Skia bundle. In addition to the tools above, install **LLVM/libclang** and set `LIBCLANG_PATH` (LLVM 23.1.0 is verified). The script fetches **GN** and pinned Skia sources/dependencies. It downloads **Ninja 1.13.1** on Windows; on other platforms install Ninja or set `NINJA` to its executable. Network access and several GB of disk space are required.

Configure tool paths in the root `.env.local`: `EMSDK` for the SDK directory, `LIBCLANG_PATH` for the directory containing libclang, and optional `PYTHON` and `NINJA` executable paths. See [`.env.example`](.env.example); GN is managed by the script.

```bash
pnpm wasm:skia:build  # Compile Skia and regenerate vendor/skia-wasm
pnpm wasm:build       # Verify bindings consume the regenerated bundle
```

See the [WASM setup guide](src-tauri/wasm-bindings/README.md) for toolchain details.

## Available Test Suites

The Rust backend includes several extra test suites covering rendering correctness, cross-pipeline parity, and encoding performance:

| Suite                | Purpose                                                                                        | Docs                                                         |
| -------------------- | ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| **Render Baselines** | Pixel-level regression tests — rendered frames and videos compared against committed baselines | [`BASELINES.md`](src-tauri/ovrley_core/BASELINES.md)         |
| **Canvas Parity**    | Rust Skia vs browser SVG pixel comparison with SSIM scoring and diff images                    | [`CANVAS_PARITY.md`](src-tauri/ovrley_core/CANVAS_PARITY.md) |
| **Benchmarks**       | Real-render encoding benchmarks measuring job time and file size per codec/hardware profile    | [`BENCHMARKS.md`](src-tauri/ovrley_core/BENCHMARKS.md)       |

## Building & Compatibility

To compile a production release:

```bash
pnpm run build
```

**Portable Distribution:** The build process bypasses traditional system installers. Instead, it generates a standalone, portable application packaged neatly within a `.zip` file. This allows for immediate extraction and execution without elevated system privileges.

---

## Acknowledgements

Big credit goes to [@walkersutton](https://github.com/walkersutton) and his [Cyclemetry](https://github.com/walkersutton/cyclemetry) repository, which was the original inspiration and trigger for this project. Without his initial work, OVRLEY likely would not have happened.
