# `src-tauri` Rust Audit

Scope: reviewed all 77 `.rs` files under `src-tauri`.
Filter: only maintainability findings ranked `5/10` or higher are included.
Note: the deliberate choice to keep tests under `ovrley_core/tests` was respected and is not listed as a finding.

## Findings

1. Severity `10/10` - The transparent and composite render pipelines are partially forked copies of the same infrastructure.
Type: duplicate code; should be abstracted/refactored/reorganized.
Files: `src-tauri/ovrley_core/src/encode/video_pipeline.rs:385`, `src-tauri/ovrley_core/src/encode/video_pipeline.rs:424`, `src-tauri/ovrley_core/src/encode/video_pipeline.rs:481`, `src-tauri/ovrley_core/src/encode/video_pipeline.rs:565`, `src-tauri/ovrley_core/src/encode/video_pipeline.rs:585`, `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:671`, `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:844`, `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:871`, `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:900`, `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:929`
Why this is high severity: buffer-pool management, queue backpressure, writer threads, timing-map merging, and cancellation-aware buffer acquisition now exist in two large implementations with small behavioral drift. Future fixes in one path are very likely to be missed in the other.

2. Severity `9/10` - `video_composite_pipeline_tests.rs` has become a second implementation of production logic instead of a test suite.
Type: duplicate code; unnecessarily verbose code; should be reorganized.
Files: `src-tauri/ovrley_core/tests/video_composite_pipeline_tests.rs:534`, `src-tauri/ovrley_core/tests/video_composite_pipeline_tests.rs:651`, `src-tauri/ovrley_core/tests/video_composite_pipeline_tests.rs:1036`, `src-tauri/ovrley_core/tests/video_composite_pipeline_tests.rs:1043`, `src-tauri/ovrley_core/tests/video_composite_pipeline_tests.rs:1059`, `src-tauri/ovrley_core/tests/video_composite_pipeline_tests.rs:1069`, `src-tauri/ovrley_core/tests/video_composite_pipeline_tests.rs:1093`, `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:479`, `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:517`, `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:535`, `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:549`, `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:783`
Why this is high severity: the 1000+ line test file duplicates private production helpers like output-progress math, success verification, broken-pipe detection, and stderr formatting. That makes the tests easy to keep green while production and tests silently drift apart.

3. Severity `9/10` - Composite codec knowledge is duplicated across multiple disconnected registries.
Type: duplicate code; code that should be abstracted/refactored/reorganized.
Files: `src-tauri/ovrley_core/src/encode/codec_detect.rs:54`, `src-tauri/ovrley_core/src/encode/codec_detect.rs:106`, `src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs:71`, `src-tauri/ovrley_core/src/encode/ffmpeg_composite_profiles.rs:46`, `src-tauri/ovrley_core/src/encode/ffmpeg_composite_profiles.rs:227`
Why this is high severity: `AvailableCodecs`, `HwAccelInfo`, the hard-coded probe list in `detect_codecs`, and the built-in composite profile catalog all describe the same encoder/filter surface in different shapes. Adding or renaming a codec requires touching several files by hand, which is exactly how stale capability matrices appear.

4. Severity `8/10` - The widget layer is split by feature, but each feature file still owns too many responsibilities.
Type: unnecessarily verbose code; poorly organized code; should be split into different files.
Files: `src-tauri/ovrley_core/src/render/widgets/route.rs:139`, `src-tauri/ovrley_core/src/render/widgets/route.rs:217`, `src-tauri/ovrley_core/src/render/widgets/route.rs:308`, `src-tauri/ovrley_core/src/render/widgets/elevation.rs:293`, `src-tauri/ovrley_core/src/render/widgets/elevation.rs:481`, `src-tauri/ovrley_core/src/render/widgets/elevation.rs:529`, `src-tauri/ovrley_core/src/render/widgets/value.rs:261`, `src-tauri/ovrley_core/src/render/widgets/value.rs:570`, `src-tauri/ovrley_core/src/render/widgets/value.rs:612`
Why this is high severity: route, elevation, and value widgets each combine normalization, geometry preparation, cache construction, draw-time composition, and in the value case asset loading plus SVG parsing. These are feature-level god files and will be difficult to hand off safely.

5. Severity `8/10` - `encode/video.rs` mixes unrelated orchestration concerns and still carries deferred refactors in production code.
Type: code that should be abstracted/refactored/reorganized; code that should live in a different file.
Files: `src-tauri/ovrley_core/src/encode/video.rs:33`, `src-tauri/ovrley_core/src/encode/video.rs:160`, `src-tauri/ovrley_core/src/encode/video.rs:181`, `src-tauri/ovrley_core/src/encode/video.rs:270`, `src-tauri/ovrley_core/src/encode/video.rs:273`, `src-tauri/ovrley_core/src/encode/video.rs:605`
Why this is high severity: the file owns public render dispatch, transparent segmentation, composite segmentation, benchmark-only parallel rendering, child-controller wiring, and progress aggregation. The comments explicitly note deferred request-struct refactors, which is a sign the module boundary is already straining.

6. Severity `8/10` - The core renderer depends directly on frontend asset paths and embeds a custom SVG parser in `value.rs`.
Type: code that should live in a different file; code that should be abstracted/refactored/reorganized.
Files: `src-tauri/ovrley_core/src/render/widgets/value.rs:570`, `src-tauri/ovrley_core/src/render/widgets/value.rs:612`, `src-tauri/ovrley_core/src/render/widgets/value.rs:642`, `src-tauri/ovrley_core/src/render/widgets/value.rs:664`
Why this is high severity: `ovrley_core` reaches into `../../app/src/components/...` via `include_str!` and also owns a hand-rolled SVG path parser. That tightly couples backend rendering to the frontend folder layout and hides asset concerns inside a text/widget implementation file.

7. Severity `7/10` - Static label/icon rendering is duplicated in two production paths.
Type: duplicate code; should be abstracted/refactored.
Files: `src-tauri/ovrley_core/src/render/mod.rs:469`, `src-tauri/ovrley_core/src/render/mod.rs:495`, `src-tauri/ovrley_core/src/render/mod.rs:520`, `src-tauri/ovrley_core/src/render/mod.rs:537`, `src-tauri/ovrley_core/src/render/mod.rs:571`
Why this matters: `cached_labels_image` and `prepare_base_rgba` both build the same static label/icon layer with nearly identical loops. That is an easy place for cache and non-cache rendering to diverge visually.

8. Severity `7/10` - `custom_export_range_active` is a hidden magic flag living in `scene.extra` instead of the typed config schema.
Type: poorly documented code; code that should live in a different file.
Files: `src-tauri/ovrley_core/src/render/widgets/common.rs:97`, `src-tauri/ovrley_core/src/render/widgets/route.rs:318`, `src-tauri/ovrley_core/src/render/widgets/route.rs:356`, `src-tauri/ovrley_core/src/render/widgets/elevation.rs:538`, `src-tauri/ovrley_core/src/render/widgets/elevation.rs:639`
Why this matters: a behaviorally important flag is effectively smuggled through `extra` JSON. That makes the feature hard to discover, hard to validate, and easy to break during template/schema work.

9. Severity `7/10` - The benchmark binaries are largely copy-pasted harnesses, and some of the codec matrices are visibly stale/disabled.
Type: duplicate code; stale code; unnecessarily verbose code.
Files: `src-tauri/src/bin/benchmark_composite.rs:17`, `src-tauri/src/bin/benchmark_composite.rs:64`, `src-tauri/src/bin/benchmark_composite.rs:160`, `src-tauri/src/bin/benchmark_transparent.rs:30`, `src-tauri/src/bin/benchmark_transparent.rs:53`, `src-tauri/src/bin/benchmark_transparent.rs:125`, `src-tauri/src/bin/benchmark_widget_rate.rs:16`, `src-tauri/src/bin/benchmark_widget_rate.rs:52`, `src-tauri/src/bin/benchmark_widget_rate.rs:141`
Why this matters: three large binaries repeat nearly the same CLI parsing, loop structure, result structs, cooldown handling, and JSON reporting. On top of that, benchmark codec lists are partly commented out or hard-disabled in helper matches, which is a classic stale-code smell.

10. Severity `7/10` - `ffmpeg_settings.rs` contains duplicated `-qscale:v` handling in the `prores_ks` branch.
Type: duplicate code; stale code.
Files: `src-tauri/ovrley_core/src/encode/ffmpeg_settings.rs:82`, `src-tauri/ovrley_core/src/encode/ffmpeg_settings.rs:87`, `src-tauri/ovrley_core/src/encode/ffmpeg_settings.rs:96`, `src-tauri/ovrley_core/src/encode/ffmpeg_settings.rs:101`
Why this matters: the same option append/default logic appears twice in one branch. It currently collapses to "do nothing extra" because the first copy wins, which makes the second copy pure maintenance noise and a future bug trap.

11. Severity `6/10` - The Tauri shell crate still owns backend logic that should not live in the UI boundary layer.
Type: code that should live in a different file; unnecessarily verbose code.
Files: `src-tauri/src/lib.rs:75`, `src-tauri/src/lib.rs:242`, `src-tauri/src/lib.rs:286`, `src-tauri/src/lib.rs:310`, `src-tauri/src/lib.rs:320`, `src-tauri/src/lib.rs:336`
Why this matters: `src/lib.rs` is mostly a command-registration boundary, but it also contains preview warning heuristics, MIME detection, template file writes, parse-debug writes, and a long series of almost-identical JSON wrapper commands. That makes the shell layer fatter than its own module docs claim.

12. Severity `6/10` - `commands/mod.rs` still exposes a stale compatibility re-export for `AppPaths`.
Type: stale code; code in the wrong file.
Files: `src-tauri/ovrley_core/src/commands/mod.rs:29`, `src-tauri/ovrley_core/src/commands/mod.rs:30`
Why this matters: the file itself says the re-export should have been removed in "Phase 6". Leaving this around keeps the wrong ownership boundary alive and makes future path-related imports harder to clean up.

13. Severity `6/10` - `stub_render_response` is an obsolete unused placeholder still sitting in production code.
Type: obsolete/unused code.
Files: `src-tauri/ovrley_core/src/render/mod.rs:584`
Why this matters: it advertises an old "Phase 3 partial" state that no longer matches the codebase, and there are no call sites left for it. This is precisely the kind of stale artifact that confuses a production handoff.

14. Severity `6/10` - `with_widget_transform` has a stale API and misleading module docs.
Type: poorly documented code; stale code.
Files: `src-tauri/ovrley_core/src/render/widgets/transform.rs:1`, `src-tauri/ovrley_core/src/render/widgets/transform.rs:20`, `src-tauri/ovrley_core/src/render/widgets/transform.rs:46`
Why this matters: the module header says `with_widget_transform` handles clipping, but the implementation only translates and rotates. The `_width` and `_height` parameters are dead weight left behind by that older API shape.

15. Severity `6/10` - `rdp_tests.rs` duplicates the production RDP algorithm instead of validating the real implementation.
Type: duplicate code; code that should be abstracted/refactored.
Files: `src-tauri/ovrley_core/tests/rdp_tests.rs:103`, `src-tauri/ovrley_core/tests/rdp_tests.rs:115`, `src-tauri/ovrley_core/src/rdp.rs:16`, `src-tauri/ovrley_core/src/rdp.rs:33`
Why this matters: the test file re-implements `perpendicular_distance` and `rdp_simplify`, so a production regression can be mirrored in tests without being caught. Behavior-spec tests are useful, but not when they fork the implementation they claim to guard.

16. Severity `5/10` - The shared test fixture registry still carries redundant aliases and broad dead-code suppression.
Type: duplicate code; stale code.
Files: `src-tauri/ovrley_core/tests/common/test_config.rs:41`, `src-tauri/ovrley_core/tests/common/test_config.rs:73`, `src-tauri/ovrley_core/tests/common/test_config.rs:77`
Why this matters: `sample_video_path()` and `test_1080p_video_path()` return the same path, and the file relies on crate-wide `#![allow(dead_code)]` to suppress registry sprawl. That is manageable today, but it is already a sign the helper catalog wants a tighter surface.

17. Severity `5/10` - `ffmpeg_error_display` is effectively a dead test.
Type: obsolete/unused code.
Files: `src-tauri/ovrley_core/tests/error_display_tests.rs:61`
Why this matters: the test creates a `CoreError::FfmpegNotFound` value and asserts nothing about it. It adds maintenance surface without adding coverage.
