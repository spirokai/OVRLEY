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
- **10+ Widgets**: Speed, heartrate, power, cadence, date, temperature, elevation profile, map route, and more.
- **100% Free & Offline**: No subscriptions. All data processing, rendering, and encoding happen locally on your machine. Your data never leaves your computer.

---

## Project Summary

The codebase utilizes a modern, dual-stack architecture to maximize both the user experience and computational performance:

- **Frontend Interface**: A React application built with Vite (`app`), providing a seamless editing environment for overlay positioning, aesthetic customization, and metric synchronization.
- **Core Engine**: A standalone Rust backend (`ovrley_core`) utilizing Skia for frame-perfect 2D graphics rendering and FFmpeg for high-speed video compositing.
- **Desktop Shell**: A lightweight Tauri wrapper that bridges the web-based interface with the native Rust core, granting access to the local file system and hardware resources.

## Export & Codecs

OVRLEY supports video transparency and offers multiple export pipelines tailored to your hardware and workflow needs:

- **ProRes CPU**: The safest general recommendation. High quality and excellent compatibility with professional editors like Premiere Pro or DaVinci Resolve.
- **ProRes Vulkan**: A newer, GPU-accelerated option for ProRes. Frees up your CPU for other tasks, though hardware/driver compatibility may vary.
- **ProRes VideoToolbox**: A highly optimized, hardware-accelerated pipeline exclusively for macOS.
- **QT RLE**: Offers significantly smaller file sizes and extremely fast export times, though with slightly lower compatibility in some basic video players.

## Dependencies

Compiling and running OVRLEY requires the following system-level toolchains:

- **[Node.js](https://nodejs.org/) & [pnpm](https://pnpm.io/)** (Package management and script execution)
- **[Rust Toolchain](https://rustup.rs/)** (For compiling the Tauri shell and core engine)
- **[FFmpeg 8.1+ (Full Build)](https://ffmpeg.org/)** (For video manipulation; **installed automatically**)

> **Important:** The rendering engine requires an FFmpeg 8.1+ **Full Build** located within the `vendor/ffmpeg` directory to ensure compatibility with advanced hardware encoders. This is automatically handled by the installation process, but you can manually place your own build there.

## Installation

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
