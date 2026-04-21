# Backend Migration Assessment: Python to Rust + Skia/Vello

## Executive Summary
Migrating the current Python sidecar backend to a native Rust + Skia/Vello architecture is **highly viable and strongly recommended for long-term performance and maintainability**. It will eliminate the brittle Python Sidecar (Flask + PyInstaller) and the heavy dependencies (SciPy, Matplotlib, Pillow). However, it represents a **complete rewrite of the backend** rather than a refactor.

---

## 1. Current Architecture & Structure

The application currently uses a split architecture:
- **Tauri (Rust)** acts as an orchestrator and IPC bridge, exposing commands like `backend_render` and `backend_demo`.
- **Python Sidecar (Flask)** runs locally, receiving HTTP/Socket requests from Tauri.
- **Rendering Pipeline:**
  - **Parsing:** `gpxpy` extracts point data and Garmin extensions.
  - **Processing:** `numpy` and `scipy` handle data interpolation (framerate synchronization) and smoothing (Savitzky-Golay filtering for elevation/gradient).
  - **Plotting:** `matplotlib` renders complex charts (e.g., elevation profiles, course maps) to memory buffers.
  - **Compositing:** `Pillow` (PIL) composites the UI, including text, dynamic values, and the Matplotlib plots.
  - **Encoding:** Raw RGBA frames are piped directly to an `ffmpeg` subprocess `stdin` to generate ProRes/H264 video.

## 2. GPX Data Ingestion Schema

The schema heavily relies on parsing GPX data into parallel arrays, mapped over time and distance progress.
* **Core Attributes:**
  * `course` (Latitude, Longitude)
  * `elevation`
  * `time` (Datetime)
  * `speed` (Derived or recorded)
* **Garmin TrackPointExtensions (`Activity.tag_map`):**
  * `heartrate` (`hr`)
  * `cadence` (`cad`)
  * `power` (`PowerInWatts`)
  * `temperature` (`atemp`)
* **Derived Attributes:**
  * `gradient` (Calculated using distance deltas and smoothed elevation data).

**Data Transformation Flow:**
1. Parse track points into sparse arrays.
2. Smooth noisy data (e.g., `savgol_filter` on elevation).
3. Generate a dense timestamp array based on the target `fps` and `duration`.
4. Interpolate all sparse data points (`scipy.interpolate.interp1d`) to match the dense frame array perfectly.

---

## 3. Migration Viability & Approach

The transition is **highly viable** but requires substituting specialized Python data science libraries with Rust equivalents.

### 🎯 The "Easy" Wins
* **No More Sidecar:** The Tauri Rust backend can directly execute the rendering code. This removes Flask, PyInstaller, IPC overhead, and port/socket binding issues.
* **Encoding Pipeline:** Piping raw RGBA frames to an `ffmpeg` subprocess using `std::process::Command` in Rust works exactly the same as in Python (`subprocess.Popen`).
* **Performance:** Skia/Vello GPU rendering will massively outperform Pillow and Matplotlib CPU rendering, especially for 60fps video generation.

### 🚧 The Major Challenges (Requires Overhaul)
1. **Matplotlib Replacement:** 
   * **Current:** `plot.py` heavily relies on `matplotlib.pyplot` for `fill_between`, axes scaling, and path rendering.
   * **Migration:** You cannot easily plug Matplotlib into Rust. You will either need to manually draw paths and polygons using `vello`/`skia-safe` or use a Rust plotting library like `plotters` and output it to a Skia canvas.
2. **SciPy Interpolation & Smoothing:** 
   * **Current:** The code uses `scipy.interpolate.interp1d` and `scipy.signal.savgol_filter`.
   * **Migration:** You will need to implement 1D linear interpolation and a Savitzky-Golay filter in Rust. While the `ndarray` ecosystem exists, out-of-the-box equivalents for these exact functions might require writing custom algorithms.
3. **Pillow Text Rendering:**
   * **Current:** Pillow handles text layout (`multiline_textbbox`), custom font loading (`.ttf`), and font caching.
   * **Migration:** Skia (`skia-safe`) has excellent text shaping and rendering capabilities, but the API is lower-level than Pillow. Vello requires integrating with `parley` or `cosmic-text` for text layout.

---

## 4. Extent of the Overhaul

This is a **100% backend overhaul**. None of the existing Python code can be reused directly. 

**Recommended Phased Implementation Plan:**
1. **Phase 1: GPX Ingestion & Math Foundation (Rust)**
   * Use the `gpx` crate to parse the XML.
   * Implement the interpolation logic and write a custom Savitzky-Golay filter (or simpler moving average/smoothing alternative). Verify the math matches the Python output.
2. **Phase 2: Skia/Vello Prototyping**
   * Set up `vello` (or `skia-safe`) to render a single composite frame with text and simple shapes.
   * Port the font management system.
3. **Phase 3: Charting Engine**
   * Recreate the Map and Elevation profile logic using native Skia/Vello path drawing logic, completely bypassing the need for a 3rd party plotting library.
4. **Phase 4: FFmpeg Integration & Tauri Hookup**
   * Feed Skia/Vello rendered RGBA buffers into `ffmpeg` via standard input.
   * Expose as Tauri commands and remove the Python sidecar shell spawn logic.

---

## 5. Detailed Technical Q&A Summary

During the architectural exploration, several key decisions and performance estimates were established regarding the transition to Rust + Skia/Vello.

### Data Interpolation & Schema Compatibility
* **Pre-computing vs. Real-time:** Interpolation (math) should be completely separated from rendering. The dense 60fps arrays should be pre-computed *before* the render loop begins. During rendering, fetching data is an O(1) array lookup (`speed[frame_index]`), guaranteeing bottleneck-free render times.
* **Schema Fit:** The current interpolated parallel-array schema is perfectly suited for vector drawing APIs (Skia/Vello). Sparse data like `(Lat, Lon)` coordinates map directly to scaled 2D `Path` geometries, which are drawn dynamically up to the current frame's index.

### Static Composition vs. Per-Frame Rendering
* **Layered Composition (Recommended):** For video pipelines, it is most efficient to render static elements (backgrounds, borders, static text) to a single off-screen texture *once*. For each frame, this texture is copied to the canvas, and only dynamic elements (moving dots, ticking numbers) are drawn on top.
* **Redraw Every Frame (GPU approach):** Drawing static text every frame is also highly performant on GPUs (via glyph texture atlases), but Layered Composition strictly minimizes CPU/GPU work to maximize video encoding throughput.

### 4K Render Time Estimates & Bottlenecks
A 4K uncompressed frame is ~33MB. Moving this data is the ultimate bottleneck.
* **Current Python (CPU):** Takes ~60ms per frame just to draw and serialize to bytes.
* **Rust + Skia (CPU Headless):** Estimated ~10ms - 20ms per frame.
* **Rust + Vello/Skia (GPU Accelerated):** Drawing takes ~1ms - 3ms. However, copying the 33MB frame from GPU VRAM back to CPU RAM (Readback) adds ~2ms - 5ms. Total time: **5ms - 8ms per frame (120+ FPS potential).**
* **The Real Bottleneck (FFmpeg):** The pipeline will always be bottlenecked by the encoder, not the Rust graphics code.

### ProRes 4444 Absolute Bottleneck
* **Software Encoder (`prores_ks`):** CPU-bound by DCT math. Even on high-end CPUs (i9/M-series), it caps at **~50ms - 100ms per frame (10 - 20 FPS)**.
* **Hardware Encoder (`prores_videotoolbox`):** Offloads math to silicon media engines (e.g., Apple Mac). Can encode at **~15ms - 30ms per frame (30 - 60+ FPS)**.
* **Web App Environment:** A pure Web App cannot natively encode ProRes. It must use `ffmpeg.wasm` running on the CPU inside the browser sandbox, which severely limits multi-threading and SIMD vector instructions. Expect massive slowdowns (**150ms - 250ms+ per frame**) for ProRes in the browser.

### Headless Setup Difficulty & Code Size Estimates
* **Boilerplate:** CPU-backed Skia in Rust takes < 15 lines of code. GPU-backed Skia (headless Vulkan/Metal contexts) is much harder and requires libraries like `glutin` or `ash`. Vello (built on WGPU) makes headless GPU contexts significantly easier.
* **Hardware Fallbacks:** WGPU (Vello) can fallback to software rendering (like `llvmpipe`), but it is notoriously slow for compute shaders. If a robust software fallback is mandatory for non-GPU machines, Skia's highly optimized CPU rasterizer is far superior. Vello supports integrated graphics (Intel UHD/Iris, AMD APUs) flawlessly via WGPU.
* **Code Size Rewrite:** The current Python code is ~5,500 LOC. A Rust + Vello rewrite is estimated at **7,000 to 9,000 LOC**. The increase is due to explicit struct typing (config parsing), manual charting logic (replacing `matplotlib`), and text-layout boilerplate (wiring `parley` to Vello).

### The Web App Viability ("The Killer Feature")
Writing the backend in Rust + Vello/WGPU allows the exact same rendering core to compile to **WebAssembly (WASM)** and run in a user's browser via **WebGPU**. 
* GPX math and path generation run at near-native speeds in WASM.
* Vello renders 4K frames instantaneously on the user's local GPU inside the browser.
* For standard formats (H.264/HEVC), the browser's native **WebCodecs API** allows hardware-accelerated, zero-server-cost video generation directly to the user's Downloads folder. 
* *Limitation:* As noted, ProRes 4444 encoding is the only exception, requiring the slow `ffmpeg.wasm` fallback.
