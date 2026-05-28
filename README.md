<div align="center">
  <img src="app/public/logo.svg" alt="OVRLEY Logo" height="90" />
</div>

<br />

<div align="center">
  <img src="docs/hero.png" alt="OVRLEY Dashboard Interface" width="100%" />
</div>

<br />

OVRLEY turns .fit and .gpx activity data into customizable overlays for action-cam videos. It features an intuitive, WYSIWYG editor backed by a robust, hardware-accelerated native rendering engine.

## Features

- **Universal Import**: Supports `.fit` and `.gpx` files from any device or service (Strava, Garmin, etc.). Works for any activity: running, cycling, skiing, sailing, motorcycling, and more.
- **Interactive Editor**: Drag-and-drop editor for live layout editing and template sharing.
- **Full Customization**: Shape every widget with your own styling, color, font, and size.
- **23+ Widgets**: Speed, heartrate, power, cadence, date, temperature, elevation profile, map route, and more.
- **Transparent Overlays**: Export with alpha channel for maximum flexibility in post-production. Use in Premiere Pro, DaVinci Resolve, Final Cut Pro, or any editor that supports layered video.
- **Direct Final Video Export**: Export your video footage with overlays baked in; no separate compositing step required. Choose from multiple codecs and hardware-accelerated pipelines, customize bitrate, and more.
- **Hardware-Accelerated Export**: Export pipelines using GPU acceleration for maximum export speed.
- **Preview & Sync**: Preview your footage directly inside the app in full quality. Automatically syncs activity to your video, with manual offset possible.
- **100% Free & Offline**: No subscriptions. All data processing, rendering, and encoding happen locally on your machine. Your data never leaves your computer.
- **Cross-Platform & Portable**: Runs on Windows, macOS, and Linux. No installation required; just download and run.

> ## Using OVRLEY: Download the exe/app file from latest release and just run it - no installation or setup required.

## Project Summary

The codebase utilizes a modern, dual-stack architecture to maximize both the user experience and computational performance:

- **Frontend Interface**: A React application built with Vite (`app`), providing a seamless editing environment for overlay positioning, aesthetic customization, and metric synchronization.
- **Core Engine**: A standalone Rust backend (`ovrley_core`) utilizing Skia for frame-perfect 2D graphics rendering and FFmpeg for high-speed video compositing.
- **Desktop Shell**: A lightweight Tauri wrapper that bridges the web-based interface with the native Rust core, granting access to the local file system and hardware resources.

## Export & Codecs

OVRLEY supports both exporting transparent overlays as well as final footage with overlays baked in. It offers multiple export pipelines tailored to your hardware and workflow needs:

Transparent Overlays:

- **ProRes CPU**: The safest general recommendation. High quality and excellent compatibility with professional editors like Premiere Pro or DaVinci Resolve.
- **ProRes Vulkan**: A newer, GPU-accelerated option for ProRes. Frees up your CPU for other tasks, though hardware/driver compatibility may vary.
- **QT RLE**: Offers significantly smaller file sizes and extremely fast export times, though with slightly lower compatibility in some basic video players.

MP4 Exports (overlays baked in):

- **H.264/H.265**: Industry-standard codecs for maximum compatibility. Custom bitrate settings allow you to balance quality and file size.

Hardware-Acceleration Options:
**NVENC/AMF/QSV/VideoToolbox**: Hardware-accelerated encoding options to take full advantage of your system and GPU to speed up export times.

## Dependencies

Compiling and running OVRLEY requires the following system-level toolchains:

- **[Node.js](https://nodejs.org/) & [pnpm](https://pnpm.io/)** (Package management and script execution)
- **[Rust Toolchain](https://rust-lang.org/tools/install/)** (For compiling the Tauri shell and core engine)
- **[FFmpeg 8.1+ (Full Build)](https://ffmpeg.org/download.html/)** (For video manipulation; **installed automatically**)

> **Important:** The rendering engine requires an FFmpeg 8.1+ **Full Build** located within the `vendor/ffmpeg` directory to ensure compatibility with advanced hardware encoders. This is automatically handled by the installation process, but you can manually place your own build there.

## Dev Installation

The repository setup is automated. When you install the Node dependencies, a custom `postinstall` script will automatically download and place the required FFmpeg 8.1 binaries into the correct vendor directory.

```bash
# Installs packages and automatically provisions the FFmpeg dependency
pnpm install
```

## Development

To launch the application locally with hot-module reloading enabled:

```bash
# Starts the frontend server alongside the native Tauri window
pnpm dev
```

## Building & Compatibility

OVRLEY is optimized for desktop environments and is designed to be run as a standalone application.

To compile a production release:

```bash
pnpm build
```

**Portable Distribution:** The build process bypasses traditional system installers. Instead, it generates a standalone, portable application packaged neatly within a `.zip` file. This allows for immediate extraction and execution without elevated system privileges.

---

## Acknowledgements

Big credit goes to [@walkersutton](https://github.com/walkersutton) and his [Cyclemetry](https://github.com/walkersutton/cyclemetry) repository, which was the original inspiration and trigger for this project. Without his initial work, OVRLEY likely would not have happened.
