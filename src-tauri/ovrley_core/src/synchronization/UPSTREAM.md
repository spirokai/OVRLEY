# Gyroflow extraction and native dependencies

Upstream: https://github.com/gyroflow/gyroflow
Revision: `d918ab3594e539f25a67a9f1d2b8e042798f61f7` (inspected 2026-09-05).
Original files: `src/core/synchronization/optical_flow/opencv_pyrlk.rs` and
`src/core/synchronization/estimate_pose/find_homography.rs`.
Copyright © 2021–2022 Adrian <adrian.eddy at gmail>. GPL-3.0-or-later.
Original copyright and SPDX headers remain in the two extracted files.
OVRLEY's GPL license text is in the repository root `LICENSE.md`.

## Copied symbols and modifications

- `OFOpenCVPyrLK::detect_features`: retained good-features detection (200 corners,
  quality 0.01, minimum separation 10 px, block size 3, Harris disabled, k=0.04).
- `OFOpenCVPyrLK::optical_flow_to`: retained PyrLK (21×21 window, three pyramid
  levels, count+epsilon criteria, 30 iterations, epsilon 0.01, flags 0,
  minimum eigenvalue 1e-4), status and bounds filtering, ten-point minimum.
  Replaced unsafe image wrapping with borrowed Mats; no image crate, global cache,
  locks, usage counters, numeric method dispatch, or stabilization objects.
  Added backward tracking (1.5 px maximum error), 25% track survival, and 6/16
  occupied spatial cells. Own at most a frame pair in the analysis worker.
- `PoseFindHomography::estimate_pose`: retained point-matrix preparation and
  `find_homography_ext` RANSAC fitting (2,000 iterations, confidence 0.999).
  Replaced undistortion with centered, width/height-normalized image coordinates;
  changed the residual threshold from 0.001 to 0.005 in these new coordinates.
  Removed camera matrix, decomposition and nalgebra rotation conversion.
  Added inlier count/fraction/coverage and transform conditioning gates, then
  sampled a 5×5 displacement grid to derive image-space rates. These are proxies,
  including anisotropic image roll, not physical camera rotation or velocity.
- OpenCV failures propagate as typed errors. Poor scene support is a quality
  outcome. All new thresholds are initial engineering settings, not empirically
  calibrated matching-confidence claims.

The decoder and orchestration are OVRLEY code, not transplanted Gyroflow autosync.
No other Gyroflow sources or transitive Rust dependencies were copied.

## Native configuration

Rust binding: exactly `opencv 0.94.4` (MIT), binding generator 0.96.1 (MIT),
native OpenCV 4.11.0 (Apache-2.0), LLVM/libclang 20.1.8 (Apache-2.0 with LLVM
exceptions, development only). Bindings disable defaults and enable `imgproc`,
`video`, `calib3d`, plus binding generation; `core`, `features2d`, and `flann` are
required dependencies. No Qt, rust-cv, Gyroflow decoder, DIS or AKAZE code.

Windows setup uses OpenCV's official precompiled `opencv_world4110.dll`; the
binding surface is minimal, but that upstream DLL includes additional native
modules. Its bundled transitive notices are retained in
`tmp/motion-native/opencv/build/etc/licenses/`; retain that entire directory with
the runtime at distribution time. The OpenCV license is in the extracted
`opencv/LICENSE.txt` file. Do not assume the Rust crate's MIT license covers the DLL.

From the repository root in PowerShell:

```powershell
. ./scripts/setup-motion-native.ps1
cargo test --manifest-path src-tauri/ovrley_core/Cargo.toml --lib synchronization
```

The script downloads checksummed official archives and extracts locally without
installing system software or running a build. It sets native include/link paths,
libclang discovery and runtime DLL discovery in the current session. It requires
7-Zip (override its location with `-SevenZip`). The cache is ignored under `tmp/`.
Run any subsequently authorized development/build commands from that session.

Validated on Windows x64 with Rust 1.94.1 and MSVC 14.51.36231. The repository's
`.tool-versions` still says 1.84.0 while current dependencies require newer Rust;
this phase does not change the project-wide toolchain policy. The binding itself
declares MSRV 1.77. LLVM 18 fails with the installed MSVC headers, which require
Clang 20 or newer. Do not suppress that header compatibility check.

macOS/Linux need native OpenCV 4.11.0 and libclang discoverable through the
opencv-rust environment or pkg-config configuration. They have not been tested
in this Windows session. CI installation/cache, Windows installer DLL staging,
portable inclusion, macOS relocation/signing and clean-machine package checks
remain Phase 6. No application/package build was run for Phase 1.

## Streaming and timing contract

`analyze_video` validates required settings before launching FFmpeg, decodes
display-oriented gray frames and emits compact `MotionInterval` values to its
caller. Use 10 Hz/640 pixels as initial caller settings; there are no hidden
defaults. Phase 2 can accumulate compact fingerprints in the sink. Phase 4 owns
the job thread, cancellation token, IPC and UI.

Two bounded channels each hold at most two entries (metadata and frames). The
reader holds at most one additional image; the analysis worker holds two.
No whole-video frame or result collection exists inside this API. Cancellation
is polled every 25 ms while waiting, and between pair estimates; a native OpenCV
call is allowed to finish. Sink callbacks must return promptly. Drop kills/reaps
FFmpeg, drops the frame receiver and joins both pipe readers, including on sink
errors and unwinding.

The command uses normal FFmpeg input timestamp normalization, autorotation,
`trim`, nonduplicating `select`, scale, gray conversion and post-filter `showinfo`.
Frame ordinals and PTS come from the same invocation as rawvideo. No guessed FPS
timestamps. Window decoding currently reads/discards all preroll from the start,
preserving media time instead of using a potentially ambiguous fast seek.
The parser/command checks used bundled FFmpeg `N-125953-gd3ad8a7fee-20260803`.
Focused tests check VFR, B-frame presentation order, nonzero container start,
display rotation, cancellation and window time against ffprobe. Browser-preview
acceptance of real edit lists is still part of integrated manual testing; these
checks do not claim every edited container has been verified.

Verification: all 106 core library unit tests passed, including eight focused
synchronization tests. Rustfmt, `git diff --check` and generated-notice creation
also passed. No frontend code or activity schema was changed.
