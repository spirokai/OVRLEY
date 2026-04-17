# Cyclemetry Reloaded - Agent Configuration

## Project Overview

Cyclemetry is a desktop application designed to create stunning telemetry video overlays from GPX (and eventually FIT) data. It allows users to visualize route tracking, elevation profiles, and rich metrics like speed, power, heart rate, cadence, gradient, and temperature on top of their videos.

The application is structured as a monorepo consisting of:

1. **Frontend (`app/`)**: A React-based web application providing a drag-and-drop overlay designer.
2. **Backend (`backend/`)**: A Python sidecar that processes telemetry data and renders the final video overlay.
3. **App Wrapper (`src-tauri/`)**: A Rust/Tauri desktop application shell that bundles the frontend and manages the Python sidecar.

Currently undergoing major refactoring:

- **Frontend Refactor**: Moving to a fully reactive, drag-and-drop UI editor using React, Immer, and Zustand.
- **Backend Refactor**: Moving away from slow, per-frame Matplotlib rendering to a precomputed geometry and cached layer compositing model for significantly faster export times.

## Tech Stack & Packages Used

### Frontend (`app/`)

- **Core**: React 19, TypeScript, Vite
- **Styling**: Tailwind CSS v4, `clsx`, `tailwind-merge`, `tw-animate-css`
- **UI Components**: Radix UI primitives, Lucide React (icons)
- **State Management**: Zustand, Immer (planned per refactor docs)
- **Desktop Integration**: `@tauri-apps/api`, `@tauri-apps/plugin-shell`, `@tauri-apps/plugin-dialog`
- **Linting/Formatting**: ESLint v9, Prettier

### Backend (`backend/`)

- **Core**: Python 3.11+, managed via `uv`
- **Web Server/API**: Flask, Flask-CORS, Waitress/Gunicorn, Gevent/Websockets
- **Data Processing**: `gpxpy`, `numpy`, `scipy`, `tsmoothie`, `simdkalman`
- **Video/Graphics Processing**: `imageio-ffmpeg`, `matplotlib` (being deprecated for frame loops), `Pillow` (PIL) for new compositing pipeline, `fonttools`
- **Packaging**: PyInstaller (for compiling the sidecar binary)
- **Linting/Formatting**: `ruff`

### App Wrapper (`src-tauri/`)

- **Core**: Rust, Tauri v2 CLI

## Build & Test Commands

Run these commands from the repository root:

- **Development**:
  - `pnpm dev` - Runs both frontend & backend concurrently (Development Mode - TCP).
  - `pnpm dev:frontend` - Runs only the Vite frontend dev server.
  - `pnpm dev:backend` - Runs only the Python backend via `uv`.
- **Testing Production (Unix Socket Mode)**:
  - `pnpm buildtest` - Builds the Python sidecar and runs Tauri in dev mode with the compiled binary.
- **Building**:
  - `pnpm build` - Tauri build process.
  - `pnpm build:sidecar` - Compiles the Python backend into a single executable using PyInstaller.
- **Code Quality**:
  - `pnpm lint` - Runs both frontend ESLint and backend Ruff linting.
  - `pnpm format` - Runs both frontend Prettier and backend Ruff formatting.

## Code Style Guidelines and Practices

### General

- Prefer using `pnpm` for package management in the root and frontend.
- Prefer using `uv` for Python dependency management and running backend scripts.

### Frontend

- **React**: Use functional components and hooks. Rely on Zustand for global state to manage the reactive overlay builder.
- **Styling**: Use Tailwind CSS utility classes. Avoid custom CSS files where Tailwind suffices.
- **UI Elements**: Build upon Radix UI primitives for accessible, headless components.

### Backend

- **Formatting**: Strictly follow `ruff` defaults (equivalent to Black): 88 line length, 4 space indents, double quotes for strings.
- **Architecture**:
  - **Refactor Rule**: Matplotlib must NOT be used in the per-frame rendering loop.
  - All static layers (backgrounds, completed routes, text fonts) must be cached and composited using Pillow (`PIL`).
  - Precompute widget-local geometry before the frame loop begins.
  - Debugging: Emit visual debug artifacts (e.g., `debug_render/`) when working on rendering pipelines rather than rendering full videos.

## Security Constraints

- **Local Communication**: The Tauri frontend and Python sidecar communicate over local TCP ports or Unix domain sockets. Ensure these are tightly bound (`127.0.0.1` or secure socket permissions) to prevent local network exposure.
- **Sidecar Execution**: The Python backend is bundled as a PyInstaller executable. Ensure that only the intended binary is executed by Tauri to prevent arbitrary code execution vulnerabilities.
- **macOS App Signing**: Currently not signed with an Apple Developer Account. Users must bypass Gatekeeper using `xattr -cr /Applications/Cyclemetry.app`. Be mindful of this when handling executable permissions or modifying the build process.
- **File Access**: The app parses local `.fit` and `.gpx` files and writes large video files. Ensure paths are validated and sanitized to prevent directory traversal or accidental overwrites.

## AI Agent Directives

When working in this repository:

1. **Always check the current architectural refactoring state.** If working on rendering, refer strictly to `strategy.md`. If working on the frontend, refer to `frontend-refactor.md`.
2. **Prioritize Performance in Python:** The main bottleneck is frame generation. Avoid any repetitive tasks (like I/O, font loading, complex object instantiation) inside frame loops.
3. **Use the specified tools:** Use `pnpm` and `uv` exclusively. Do not generate `requirements.txt` or use `npm/yarn` unless explicitly requested.
4. **Cross-Platform Awareness:** While Windows is the primary target right now, maintain code compatibility with macOS (e.g., path separators, socket types).
