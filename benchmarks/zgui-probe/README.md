# zgui probe

A spike: what one presented frame costs on [zgui](https://github.com/zortax/zgui) (signal-driven,
retained renderer on wgpu), next to `crates/ovrley_ui/examples/gpu_probe.rs` on Slint. It is its own
cargo workspace, so zgui's dependencies stay out of the app's `Cargo.lock`. The dev profile is
optimised (`opt-level = 3`).

```
cargo run --manifest-path benchmarks/zgui-probe/Cargo.toml -- <blank|timeline|still|video> <fps>
```

| zgui probe                    | compare with (`cargo run -p ovrley_ui --example gpu_probe -- …`) |
| ----------------------------- | ---------------------------------------------------------------- |
| `blank 30` (whole window)     | `slint 30` with `OVRLEY_SKIA_FULL_REDRAW=1`, and `raw 30`        |
| `timeline 30` (2 px playhead) | `slint 30` (patched partial redraw), and `copy 30`               |
| `<mode> 0`                    | `slint 0`                                                        |
| `still 30` (unread signal)    | `slint 0`                                                        |
| `video` (`PROBE_VIDEO=<path>`) | `slint-video` (same `PROBE_VIDEO`)                              |

- `ZGUI_PRESENTER=native` switches zgui to its own swap chain with dirty rectangles (DX12
  `FLIP_SEQUENTIAL` + `Present1`, Vulkan `VK_KHR_incremental_present`), the counterpart of
  `gpu_probe d3d-dirty`. Unset, zgui copies its whole persistent target into a wgpu swapchain image
  every frame, like `gpu_probe copy`. Run each mode both ways.
- `video` plays the file muted through `ovrley_playback` (MDK) into one 1920×1080 texture, on a
  device opened as the app opens it and adopted by zgui (`src/video.rs`), and presents that texture
  to a `surface` element without a copy. `gpu_probe slint-video` shows the same texture the app's
  way. MDK has to be installed (`vendor/mdk-sdk`), and `ovrley_playback`'s Cargo.toml asks for wgpu
  `30.0.1` rather than `=30.0.1` so that zgui's `30.0.2` can share it.
- `PROBE_SECONDS`, `PROBE_IDLE` and `PROBE_SIZE` work as in `gpu_probe`, except that `PROBE_SIZE` is
  in logical pixels here (a 2× display needs half of `gpu_probe`'s value).
- The timeline's playhead is 96 px tall; `gpu_probe`'s spans the window, so its damage is taller.
- GPU time is measured from outside, as described in `gpu_probe`'s module comment. The pid is the
  first line the probe prints. `node benchmarks/zgui-probe/gpu-sample.mjs <probe command…>` runs
  either probe and prints the probe's and the compositor's GPU ms per frame: from ioreg on macOS
  (WindowServer), from the GPU Engine counters on Windows (dwm.exe). On Windows, name the
  executable with `.exe` (`target\debug\zgui-probe.exe`, `target\debug\examples\gpu_probe.exe`)
  and build both first.
- Every zgui mode draws on the device the app opens (Metal, DX12, Vulkan), adopted by zgui, so both
  probes run on the same backend. `ZGUI_GPU_TIMING=1` adds zgui's own GPU time per pass, damaged
  pixels and draw calls per frame.
- The window has the app's chrome (no title bar on macOS, no decorations elsewhere), as
  `gpu_probe`'s slint modes do. On macOS a borderless window made WindowServer's GPU time 5× higher,
  so `gpu_probe`'s `raw` and `copy` windows (borderless) are not comparable there.
- At the end of a run the probe prints the whole run's average (from `frames start`), besides the
  per-second lines.
- Linux: zgui uses its own Wayland backend when `WAYLAND_DISPLAY` is set, and winit otherwise. Its
  build scripts need the libxkbcommon, wayland and fontconfig development packages (pkg-config).
