# Native N32/BGRA Input Plan

Goal: stop treating the raw Skia render buffer as hardcoded `RGBA` and instead feed FFmpeg the actual native Skia 32-bit pixel layout (`N32`, which is `BGRA` on our current Windows path).

## Scope

- Keep output codec settings unchanged.
- Do not change crop behavior.
- Apply the same raw-input change to both transparent export and composite export.

## Plan

1. Centralize the raw render format metadata.
   - In [src-tauri/ovrley_core/src/render/surface.rs](/H:/tools/cyclemetry/src-tauri/ovrley_core/src/render/surface.rs), replace the hardcoded `ColorType::RGBA8888` image info with a native `N32`/platform-mapped definition.
   - Add one small helper that returns the matching FFmpeg raw input `pix_fmt` string for the same buffer layout.

2. Remove RGBA-specific assumptions from the render path.
   - Update comments and naming around `RenderTarget`, `render_frame_rgba`, and `prepare_base_rgba` in [src-tauri/ovrley_core/src/render/mod.rs](/H:/tools/cyclemetry/src-tauri/ovrley_core/src/render/mod.rs) so they describe a native 32-bit raw buffer instead of promising RGBA byte order.
   - Rename functions only if the churn stays small; otherwise keep names and fix the comments first.

3. Switch transparent export to the native input format.
   - In [src-tauri/ovrley_core/src/encode/video_pipeline.rs](/H:/tools/cyclemetry/src-tauri/ovrley_core/src/encode/video_pipeline.rs), replace the default stdin `pix_fmt` of `rgba` with the helper from `surface.rs`.
   - Make the debug sample-frame path use the same raw input format so diagnostics still match the real encode path.

4. Mirror the change in composite export.
   - In [src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs](/H:/tools/cyclemetry/src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs), replace the hardcoded raw overlay input `rgba` with the same helper-driven format.
   - Check [src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs](/H:/tools/cyclemetry/src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs) for comments and debug output that still assume RGBA.

5. Leave encoder output formats alone.
   - Do not change `prores_ks`, `qtrle`, or composite output `pix_fmt` handling in [src-tauri/ovrley_core/src/encode/ffmpeg.rs](/H:/tools/cyclemetry/src-tauri/ovrley_core/src/encode/ffmpeg.rs).
   - This change is only about removing an unnecessary raw-input swizzle/conversion step before encoding.

6. Verify with one narrow test pass.
   - Render one transparent export and one composite export.
   - Confirm colors/alpha are correct, sample-frame debug output still looks right, and FFmpeg no longer sees the input as `rgba` on the updated path.

## Main Risk

The risky part is not FFmpeg, it is channel order mismatch. If the Skia buffer layout and FFmpeg `pix_fmt` string disagree, colors will swap immediately, so visual validation should catch regressions quickly.
