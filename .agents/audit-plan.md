# `src-tauri` Refactor Implementation Plan

Source of truth: `.agents/audit.md`
Goal: fix every maintainability finding ranked `5+` in the audit without breaking behavior.
Constraint: do not move integration tests out of `src-tauri/ovrley_core/tests`.

## What This Plan Optimizes For

1. Behavior-preserving refactors first, cleanup second.
2. One canonical source of truth for shared logic and codec metadata.
3. Smaller files with clear ownership boundaries.
4. Low-risk sequencing so an LLM can execute this in compiling slices.
5. No Rust API churn unless there is a concrete maintainability payoff.

## Non-Negotiable Safety Rules

1. Keep Tauri command names unchanged.
2. Keep serialized JSON field names unchanged unless the change is explicitly wire-compatible.
3. Keep the tests directory structure under `src-tauri/ovrley_core/tests`.
4. Prefer moving existing code into new modules over rewriting logic.
5. Do not combine structural refactors with behavior changes in the same slice.
6. Every phase must end with green verification before the next phase starts.
7. Delete temporary compatibility shims in the same phase that migrates all call sites.
8. All new or changed code must be fully documented at the module level and function level; any multi-phase or multi-layer function must also include clear phase/layer comments so the flow is understandable to a junior developer. Use existing documentation conventions and patterns for consistency.
9. If a change touches an FFmpeg-backed path that encodes more than a couple of frames, the FFmpeg-backed integration suite must be run manually before that phase is considered complete. Compile-only or `--no-run` validation is not sufficient for that change.
10. Do not widen visibility as part of a mechanical extraction. In particular, keep `video_pipeline` internals `pub(crate)` unless a later boundary-cleanup phase explicitly authorizes a visibility change.
11. After every phase and every named subphase, run `cargo test -p ovrley_core --test render_baseline_suite -- --nocapture`. If it does not pass fully, stop immediately and do not continue to the next step.

## Baseline Verification Before Any Refactor

Run these from `src-tauri/` before touching code so regressions are obvious:

```powershell
cargo fmt --check
cargo test -p ovrley_core --lib
cargo test -p ovrley_core --tests -- --test-threads=1
cargo test -p ovrley_core --test render_baseline_suite -- --nocapture
cargo check -p app --bins
cargo check --workspace --all-targets
```

If the environment cannot run the full FFmpeg-backed integration suite, use this fallback locally and rerun the full suite on a machine with FFmpeg before finishing:

```powershell
cargo test -p ovrley_core --lib
cargo test -p ovrley_core --tests --no-run
cargo test -p ovrley_core --test render_baseline_suite -- --nocapture
cargo check -p app --bins
cargo check --workspace --all-targets
```

## Recommended Phase Order

1. Shared encode runtime extraction.
2. Composite test de-duplication.
3. Codec catalog unification and FFmpeg settings cleanup.
4. Video orchestration split.
5. Render static-layer extraction.
6. Phase `6A` through `6F` widget/config/asset work, executed as separate gated slices.
7. Tauri shell/core boundary cleanup.
8. Benchmark harness consolidation and remaining test cleanup.
9. Final stale-code/doc pass and acceptance sweep.

This order matters. The high-risk encoder refactors come first because several later findings depend on those new boundaries existing.

## Mechanical Move Pattern For File-to-Directory Refactors

Use this exact sequence any time a module is being split from `foo.rs` into `foo/mod.rs` plus siblings:

1. Move `foo.rs` to `foo/mod.rs` with no logic changes.
2. Update parent `mod` declarations and imports.
3. Run `cargo check` immediately.
4. Extract one helper cluster at a time into sibling files.
5. Re-run `cargo check` after each extraction.
6. Only after the module compiles again, delete old helper definitions from `mod.rs`.

Do not mix the physical move and multiple logic extractions in one edit batch.

## Untouchable Surface and Visibility Rules

Unless a later phase explicitly says otherwise, do not rename, narrow, widen, or re-signature these cross-module surfaces while performing structural refactors:

1. `src-tauri/ovrley_core/src/encode/video.rs`
   `run_parallel_renders`, `render_video`, `CompositeRenderRequest`, `render_composite_video`, `CompositeSegmentWindow`, `composite_output_frame_windows`, `RenderController` re-export, `rendered_frame_count` re-export.
2. `src-tauri/ovrley_core/src/encode/video_pipeline.rs`
   `render_video_single` must remain `pub(crate)`.
3. `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs`
   `CompositeRenderPlan`, `derive_composite_render_plan`, `apply_composite_scene_timing`, `CompositePipelinePlan`, `render_composite_video_single`, `derive_composite_pipeline_plan`, `dense_frame_index_for_overlay`, `expected_guarded_overlay_frame_count`, `first_fractional_overrun_overlay_index`.
4. `src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs`
   `CompositeProfile`, `CompositeFfmpegSettings`, `HwAccelInfo`, `CompositeFfmpegBuildRequest`, `build_composite_ffmpeg_settings`, `fallback_profile_name`.
5. `src-tauri/ovrley_core/src/encode/codec_detect.rs`
   `AvailableCodecs`, `detect_codecs`, and the existing serialized field names on `AvailableCodecs`.
6. `src-tauri/ovrley_core/src/render/mod.rs`
   `prepare_preview_assets`, `render_preview_to_path`, `render_preview_with_report`, `render_preview_with_prepared_assets`, `render_frame_rgba`, `PreparedPreviewAssets`, `FrameRenderRequest`.
7. `src-tauri/ovrley_core/src/commands/mod.rs`
   all `backend_*` entry points and `HealthResponse`.

If a phase needs one of these surfaces to change, that phase must say so explicitly and must include downstream caller updates and verification in the same slice.

## Phase 1: Shared Encode Runtime Extraction

Audit findings covered: `#1`, visibility-boundary note from review feedback, production portion of `#2`

Risk: High

### Objective

Remove duplicated queue, buffer-pool, writer-thread, and timing-map logic from:

- `src-tauri/ovrley_core/src/encode/video_pipeline.rs`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs`

### Files To Create

- `src-tauri/ovrley_core/src/encode/pipeline_shared.rs`
- `src-tauri/ovrley_core/src/encode/video_composite_support.rs`

### Files To Edit

- `src-tauri/ovrley_core/src/encode/mod.rs`
- `src-tauri/ovrley_core/src/encode/video_pipeline.rs`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs`

### Execute This Phase In Four Ordered Substeps

#### Phase 1A: Extract Only The True Duplicates

Move only the unquestionably identical runtime pieces into `pipeline_shared.rs` first:

1. `FrameBuffer`
2. `merge_timing_maps`

Do not extract `writer_worker`, `WriterResult`, `acquire_frame_buffer`, or `queue_frame` yet.

#### Phase 1B: Write A Differences Log And Normalize Shapes Before Sharing More Code

Before any further extraction, document these current differences inside the Phase 1 work notes or commit message so the executor does not treat the helpers as line-for-line duplicates:

1. Visibility asymmetry:
   `render_video_single` is `pub(crate)` while `render_composite_video_single` is `pub`.
2. `WriterResult.written_frames` drift:
   transparent uses `u32`, composite uses `u64`.
3. `writer_worker` drift:
   transparent takes a cancel flag and records `encoder.queue_wait` plus `buffer.release_wait`; composite does not take a cancel flag and currently uses a simpler recv loop.
4. `acquire_frame_buffer` and `queue_frame` are already cancellation-aware in both files and are much closer to true duplicates than `writer_worker`.

Then normalize the remaining runtime helpers into explicitly compatible shapes before extraction:

1. Normalize written-frame storage first.
   Preferred direction: widen the transparent path internally to `u64` rather than narrowing the composite path.
2. Make writer cancellation policy explicit before extraction.
   If the composite path keeps its current "drain until sender closes" behavior, model that as an explicit strategy or option rather than hidden drift.
3. Preserve the current public visibility boundary.
   `pipeline_shared.rs` should be `pub(crate)` only, and Phase 1 must not make the transparent path more public just because the composite file has public test seams.

#### Phase 1C: Extract The Remaining Shared Runtime Helpers

Only after `1A` and `1B` are green, extract the remaining shared runtime helpers into `pipeline_shared.rs` with explicit documented parameter types:

1. `WriterResult`
2. `acquire_frame_buffer`
3. `queue_frame`
4. `writer_worker`

Do not describe this step as a line-for-line move. It is a compatibility extraction after deliberate shape alignment.

#### Phase 1D: Move Composite-Only Pure Helpers Out Of `video_composite_pipeline.rs`

Extract these pure composite helpers into `video_composite_support.rs` during Phase 1 so Phase 2 can be test-only:

1. `output_progress_for_overlay_time`
2. `verify_successful_composite_output`
3. `is_pipe_write_error`
4. `format_pipe_write_failure`
5. `stderr_tail`

Expose the module as `#[doc(hidden)] pub mod video_composite_support;` from `encode/mod.rs` so integration tests can import the real production helpers without re-implementing them.

### Implementation Notes

1. Keep `spawn_ffmpeg_process`, `spawn_composite_ffmpeg_process`, stderr monitoring, progress parsing, and render-plan logic in their current pipeline-specific files.
2. Do not introduce trait-heavy abstractions here.
3. Do not hide runtime behavior behind generic closure magic. Shared helpers must have explicit parameters and explicit docs about queueing, cancellation, and writer lifecycle.
4. Keep all current public function names and visibilities unchanged during this phase.
5. Add module docs to `pipeline_shared.rs` explaining that it owns queueing, buffer reuse, writer lifecycle, and timing-map aggregation only.
6. Add module docs to `video_composite_support.rs` explaining that it owns pure composite helper logic shared by production code and integration tests, not the composite render loop itself.

### Untouchable Surface For This Phase

1. `render_video_single` must remain `pub(crate)`.
2. `render_composite_video_single` must remain `pub`.
3. `spawn_ffmpeg_process`, `spawn_composite_ffmpeg_process`, and `derive_composite_pipeline_plan` must keep their current signatures in this phase.
4. Do not rename or move the public composite test seams listed in the untouchable-surface section; only delegate their helper internals.

### Acceptance Criteria

1. The phase is executed as `1A` -> `1B` -> `1C` -> `1D`, not as one batch edit.
2. `merge_timing_maps` and `FrameBuffer` are extracted without changing behavior.
3. The writer/result differences are normalized explicitly rather than hand-waved as identical.
4. `pipeline_shared.rs` remains internal and does not widen transparent-path visibility.
5. `video_composite_support.rs` exists before Phase 2 begins, so Phase 2 no longer has to modify `video_composite_pipeline.rs`.
6. `video_pipeline.rs` and `video_composite_pipeline.rs` both shrink materially and become easier to scan.

### Manual Verification Checklist

Because this phase touches FFmpeg-backed rendering paths, manually run the FFmpeg-backed integration suite on a machine with FFmpeg and verify all of the following:

1. A transparent encode longer than a couple of frames succeeds.
2. A composite encode longer than a couple of frames succeeds.
3. Transparent cancellation still cleans partial output correctly.
4. Composite cancellation still cleans partial output correctly.
5. Broken-pipe or early-exit diagnostics still include useful FFmpeg context.
6. Output-frame counts and overlay-frame counts remain unchanged for the same fixture inputs.

### Verification

```powershell
cargo test -p ovrley_core --lib
cargo test -p ovrley_core --test video_composite_pipeline_tests -- --test-threads=1
cargo test -p ovrley_core --tests -- --test-threads=1
```

## Phase 2: Composite Test De-Duplication

Audit findings covered: `#2`

Risk: Medium

### Objective

Stop `video_composite_pipeline_tests.rs` from being a second implementation of production logic.

### Files To Create

- `src-tauri/ovrley_core/tests/common/composite.rs`

### Files To Edit

- `src-tauri/ovrley_core/tests/video_composite_pipeline_tests.rs`
- `src-tauri/ovrley_core/tests/common/mod.rs` if needed for new shared test helpers

### Scope Guard

Phase 2 is test-only cleanup. It must not further restructure `video_composite_pipeline.rs`. The production helper extraction was completed in Phase `1D`.

### Test-Only Fixture Helpers To Move

Move the non-production test harness code into `tests/common/composite.rs`:

1. fixture render helpers
2. fixture path builders
3. FFprobe helper functions
4. argument-pair assertion helpers
5. debug summary file helpers

### Important Constraint

Keep the tests in `ovrley_core/tests`. Do not move coverage into unit tests under `src/`.

### Specific Cleanups Inside `video_composite_pipeline_tests.rs`

1. Replace local copies of the five production helpers with imports from `ovrley_core::encode::video_composite_support`.
2. Replace any planner reimplementation such as `phase4_plan(...)` with direct calls into `derive_composite_pipeline_plan(...)` or a thinner request/config builder that does not duplicate planner logic.
3. Leave only actual assertions, fixture setup, and black-box render validation in the test file.
4. Keep the test file black-box oriented: no second copy of progress math, stderr trimming, or success verification logic.

### Acceptance Criteria

1. `video_composite_pipeline_tests.rs` no longer contains duplicated production helper logic.
2. Production helpers exist in only one place.
3. The test file reads like a spec, not like a second pipeline implementation.

### Verification

```powershell
cargo test -p ovrley_core --test video_composite_pipeline_tests -- --test-threads=1
```

## Phase 3: Codec Catalog Unification

Audit findings covered: `#3`, `#9`, `#10`, plus requested transparent-profile catalog extraction

Risk: High

### Objective

Create one canonical codec/profile catalog instead of maintaining the same knowledge in multiple disconnected registries.

### Files To Create

- `src-tauri/ovrley_core/src/encode/codec_catalog.rs`
- `src-tauri/ovrley_core/src/encode/ffmpeg_transparent_profiles.rs`

### Files To Edit

- `src-tauri/ovrley_core/src/encode/mod.rs`
- `src-tauri/ovrley_core/src/encode/codec_detect.rs`
- `src-tauri/ovrley_core/src/encode/ffmpeg_composite.rs`
- `src-tauri/ovrley_core/src/encode/ffmpeg_composite_profiles.rs`
- `src-tauri/ovrley_core/src/encode/ffmpeg_settings.rs`

### Execute This Phase In Six Ordered Substeps

#### Phase 3A: Add The Catalog Without Redesigning Existing Data Models

Define two enums in `codec_catalog.rs`:

1. `TransparentCodecId`
2. `CompositeCodecId`

Back them with static metadata tables instead of handwritten match trees.

Each composite entry should define:

1. profile name
2. FFmpeg codec name
3. accepted aliases
4. overlay/filter-stack kind
5. availability rule

Each transparent entry should define:

1. codec name
2. accepted aliases
3. availability rule

During `3A`, keep `HwAccelInfo` and the probe implementation in `detect_codecs(...)` structurally unchanged. The goal of `3A` is foundation only.

#### Phase 3B: Make `AvailableCodecs` Consult The Catalog

Keep `AvailableCodecs` as the wire format returned to the frontend. Do not replace it with maps or dynamic JSON.

Add methods on `AvailableCodecs` that consult the catalog:

1. `has_transparent_codec(...)`
2. `has_composite_codec(...)`
3. any small helpers needed for hardware-family checks

Do not rename, remove, or reorder the serialized fields on `AvailableCodecs`.

#### Phase 3C: Migrate Alias Normalization And Builder Lookups

1. Migrate `ffmpeg_composite_profiles.rs` alias normalization to use catalog lookup.
2. Migrate `ffmpeg_composite.rs` free-form string matching to use the catalog.
3. Keep `build_composite_ffmpeg_settings(...)` and `composite_profile_template(...)` public surfaces unchanged during this substep.

#### Phase 3D: Collapse `HwAccelInfo` Only After `3A`-`3C` Are Green

This is the riskiest substep and must not start until `3A`, `3B`, and `3C` are all passing.

1. Collapse `HwAccelInfo` only after catalog-driven lookup is already working.
2. Keep `AvailableCodecs` as the frontend wire format.
3. Preserve the semantics of `detect_codecs(...)` output while removing duplicated internal boolean matrices.
4. The only extra context that should survive outside canonical availability is context that is not representable as a boolean capability field, such as `vaapi_device`.

#### Phase 3E: Fix `ffmpeg_settings.rs` Option-Default Duplication First

Fix the duplicated `-qscale:v` handling in the `prores_ks` branch as an isolated cleanup that does not depend on `3A`-`3D`.

Required approach:

1. Add a small helper such as `append_option_or_default(...)`.
2. Use it for `-threads`, `-profile:v`, `-qscale:v`, and any similar repeated pattern.
3. Remove the second `-qscale:v` append/default block entirely.
4. Add or update tests for override precedence and default application.

#### Phase 3F: Extract Transparent Overlay Default Profiles Out Of `ffmpeg_settings.rs`

Create `ffmpeg_transparent_profiles.rs` to own the default profile definitions for transparent overlay codecs, modeled after `ffmpeg_composite_profiles.rs`.

Required structure:

1. Add a data-shaped template type such as `TransparentProfileTemplate`.
2. Add an owned expanded type such as `TransparentProfile`.
3. Back it with a static catalog covering exactly these current transparent overlay codecs:
   `prores_ks`, `prores_ks_vulkan`, `prores_videotoolbox`, and `qtrle`.
4. Provide a lookup/normalization function such as `transparent_profile_template(...)` or `transparent_profile(...)` that expands static template data into owned arguments the builder can use.

Each transparent profile definition should own the codec-default pieces that are currently hardcoded inline in `ffmpeg_settings.rs`, including as applicable:

1. canonical codec name
2. default `pix_fmt`
3. default output extension / default container behavior
4. `hw_init_args`
5. `filters`
6. base `output_args`
7. codec-specific default values such as ProRes profile / qscale / vendor / alpha bits / slice defaults where relevant

`ffmpeg_settings.rs` should then become a thin builder that:

1. parses generic user-facing inputs such as `codec`, `loglevel`, and container override
2. looks up the transparent profile from `ffmpeg_transparent_profiles.rs`
3. expands the profile into owned defaults
4. applies user overrides with small helpers such as `append_option_or_default(...)`
5. appends `scene.ffmpeg.output_args` last as the explicit escape hatch

Important constraint:

1. `ffmpeg_settings.rs` must stop being the place where transparent codec defaults are authored.
2. It may still own generic JSON parsing, override application, and final `FfmpegSettings` assembly.
3. The new transparent profile catalog should mirror the style of `ffmpeg_composite_profiles.rs`, but it should remain scoped to transparent overlay exports and should not force `FfmpegSettings` into composite-specific shapes.

### Phase Boundaries

1. Do not touch the benchmark binaries in Phase 3. Benchmark catalog adoption belongs in Phase 8 after the catalog is stable.
2. Do not redesign `AvailableCodecs` into maps or dynamic structures.
3. Do not collapse `HwAccelInfo` before the catalog is already in use by the profile and builder layers.
4. Do not leave transparent codec defaults duplicated between `ffmpeg_settings.rs` and `ffmpeg_transparent_profiles.rs`; once `3F` is complete, the default profile data should live in the new profile module only.

### Untouchable Surface For This Phase

1. `AvailableCodecs` serialized field names must stay unchanged.
2. `detect_codecs(...)` must keep returning `CoreResult<AvailableCodecs>`.
3. `build_composite_ffmpeg_settings(...)` must keep its current public signature.
4. `composite_profile_template(...)` must keep accepting the existing string inputs during the migration.

### Acceptance Criteria

1. The phase is executed as `3A` -> `3B` -> `3C` -> `3D` -> `3E` -> `3F`, with `3E` still kept small and behavior-preserving.
2. Alias normalization lives in one module.
3. `AvailableCodecs` keeps the same serialized shape while gaining catalog-backed helper methods.
4. `HwAccelInfo` is not collapsed until the catalog has already been adopted by the builder layers.
5. The duplicated `-qscale:v` logic is gone.
6. Transparent overlay default profile data no longer lives inline inside `ffmpeg_settings.rs`; it lives in `ffmpeg_transparent_profiles.rs` in a structure that mirrors `ffmpeg_composite_profiles.rs`.

### Manual Verification Checklist

Because this phase changes codec capability and FFmpeg-setting logic, manually verify all of the following on a machine with FFmpeg:

1. The serialized shape of `AvailableCodecs` is unchanged.
2. At least one known transparent codec still runs end-to-end for more than a couple of frames.
3. At least one known composite profile still runs end-to-end for more than a couple of frames.
4. Explicit composite aliases such as profile names and codec names still resolve to the expected profiles.
5. No codec disappears from availability checks purely because of the refactor.
6. The four transparent overlay codecs still resolve to the same effective defaults as before unless an intentional default change is explicitly documented.

### Verification

```powershell
cargo test -p ovrley_core --lib
cargo check -p app --bins
cargo check --workspace --all-targets
```

## Phase 4: `encode/video.rs` Orchestration Split

Audit findings covered: `#5`

Risk: High

### Objective

Break `encode/video.rs` into focused files so it stops owning dispatch, segmentation, benchmark-only helpers, window calculation, child-controller plumbing, and cleanup logic all at once.

### Files To Create

- `src-tauri/ovrley_core/src/encode/video_parallel.rs`
- `src-tauri/ovrley_core/src/encode/video_segmented.rs`
- `src-tauri/ovrley_core/src/encode/video_windows.rs`

### Files To Edit

- `src-tauri/ovrley_core/src/encode/video.rs`
- `src-tauri/ovrley_core/src/encode/mod.rs`
- any binary or command module imports affected by moved helpers

### Target Ownership

`video.rs` should keep only:

1. public render entry points
2. `CompositeRenderRequest`
3. tiny dispatch decisions that select single-pass vs segmented paths
4. `RenderController` re-export

`video_parallel.rs` should own:

1. `run_parallel_renders`
2. `estimate_parallel_render_worker_count`
3. `estimate_composite_segment_count` if it is still only benchmark/parallel related

`video_windows.rs` should own:

1. `CompositeSegmentWindow`
2. `composite_output_frame_windows`
3. `integer_second_duration`
4. `integer_second_windows`

`video_segmented.rs` should own:

1. `render_video_segmented`
2. `render_composite_video_segmented`
3. segmented-render heuristics
4. child controller creation
5. partial output cleanup

### Important Rules

1. Do not rename the public entry points used by commands/binaries.
2. Keep segmented transparent and segmented composite behavior identical while moving code.
3. Remove stale "Phase" wording from docs/comments while touching this area.

### Untouchable Surface For This Phase

1. `render_video(...)`
2. `render_composite_video(...)`
3. `CompositeRenderRequest`
4. `run_parallel_renders(...)`
5. `CompositeSegmentWindow`
6. `composite_output_frame_windows(...)`

These may delegate to new modules, but their names, signatures, and caller-facing behavior must stay stable in Phase 4.

### Acceptance Criteria

1. `video.rs` becomes an orchestration facade instead of a god file.
2. Benchmark-only logic no longer lives next to normal render entry points.
3. The "deferred request-struct refactor" comments disappear because the structure is actually fixed.

### Manual Verification Checklist

Because this phase moves segmented and parallel render orchestration, manually verify all of the following:

1. A non-segmented transparent render still succeeds for more than a couple of frames.
2. A segmented transparent render still succeeds and stitches correctly.
3. A non-segmented composite render still succeeds for more than a couple of frames.
4. A segmented composite render still succeeds and stitches correctly.
5. Progress and cancellation semantics remain unchanged for both transparent and composite paths.

### Verification

```powershell
cargo test -p ovrley_core --lib
cargo test -p ovrley_core --tests -- --test-threads=1
cargo check --workspace --all-targets
```

## Phase 5: Render Static Layer Extraction

Audit findings covered: `#7`, `#13`

Risk: Medium

### Objective

Remove duplicated static label/icon drawing and delete dead render placeholder code.

### Files To Create

- `src-tauri/ovrley_core/src/render/static_layer.rs`

### Files To Edit

- `src-tauri/ovrley_core/src/render/mod.rs`

### Required Moves

Move these functions into `render/static_layer.rs`:

1. `cached_labels_image`
2. `prepare_base_rgba`
3. `labels_cache_key`
4. `config_has_static_metric_icons`
5. `draw_static_metric_icons`

Also add one new shared helper such as `draw_static_text_and_icons(...)` so both cached-image and base-RGBA paths call the same drawing loop.

If the `OnceLock<Mutex<HashMap<...>>>` cache moves into `render/static_layer.rs`, keep the cache private to that module and re-export only function APIs. Do not expose the cache global itself.

If `prepare_base_rgba` continues to be part of the public render surface, preserve the caller-facing path with a re-export or an unchanged public wrapper from `render/mod.rs`.

### Dead Code Removal

Delete `stub_render_response` after verifying there are no call sites.

### Acceptance Criteria

1. Static label/icon rendering exists in one implementation only.
2. `render/mod.rs` delegates static-layer concerns instead of owning every detail.
3. `stub_render_response` is removed.

### Manual Verification Checklist

Because this phase can introduce silent visual drift without compile failures, manually verify all of the following:

1. A preview frame with static labels renders identically before and after the refactor.
2. Static metric icons render identically in both cached-image and base-RGBA paths.
3. A video encode longer than a couple of frames still shows the same static labels/icons as preview output.
4. Cache hits and misses still behave correctly across distinct configs.

### Verification

```powershell
cargo test -p ovrley_core --lib
rg -n "stub_render_response" src-tauri
cargo check --workspace --all-targets
```

The final grep should return no hits.

## Phase 6: Widget Decomposition, Typed Scene Flag, and Asset Ownership

Audit findings covered: `#4`, `#6`, `#8`, `#14`

Risk: High

### Objective

Split the widget god files by responsibility, stop reading backend icons from frontend paths, remove the magic `scene.extra` flag, and correct stale transform API/docs.

### Execution Rule

Phase 6 is not one edit batch. Execute `6A`, `6B`, `6C`, `6D`, `6E`, and `6F` as six separate gated slices. Each subphase must compile and pass its verification before the next subphase starts.

### Step 6A: Typed Scene Flag

#### Files To Edit

- `src-tauri/ovrley_core/src/config/mod.rs`
- `src-tauri/ovrley_core/src/render/widgets/common.rs`
- `src-tauri/ovrley_core/src/render/widgets/route.rs` or new route submodules
- `src-tauri/ovrley_core/src/render/widgets/elevation.rs` or new elevation submodules

#### Required Change

Add an explicit field to `SceneConfig`:

```rust
#[serde(default)]
pub custom_export_range_active: Option<bool>,
```

Then replace `scene.extra.get("custom_export_range_active")` lookups with the typed field.

This change is wire-compatible because the JSON key already exists at the scene level. Serde will populate the typed field directly once it exists.

#### Acceptance Criteria

1. No widget code reads `custom_export_range_active` from `scene.extra`.
2. The behavior remains backward-compatible for existing template JSON.

#### Verification

```powershell
cargo test -p ovrley_core --lib
rg -n "get\\(\"custom_export_range_active\"\\)|custom_export_range_active.*extra" src-tauri/ovrley_core/src
```

### Step 6B: Route Widget Split

#### Physical Move

Convert `src-tauri/ovrley_core/src/render/widgets/route.rs` into:

- `src-tauri/ovrley_core/src/render/widgets/route/mod.rs`
- `src-tauri/ovrley_core/src/render/widgets/route/normalize.rs`
- `src-tauri/ovrley_core/src/render/widgets/route/prepare.rs`
- `src-tauri/ovrley_core/src/render/widgets/route/frame_state.rs`
- `src-tauri/ovrley_core/src/render/widgets/route/simplify.rs`
- `src-tauri/ovrley_core/src/render/widgets/route/draw.rs`

#### Target Ownership

1. `normalize.rs`: `normalize_route_plot` and defaults
2. `prepare.rs`: cache construction, geometry build, sample building, projection helpers
3. `frame_state.rs`: `build_route_frame_states`, prefix-point helpers, marker progress
4. `simplify.rs`: `simplify_route_samples`, downsampling, area helper
5. `draw.rs`: `draw_route_widget` only
6. `mod.rs`: re-exports and feature-level types

#### Verification

```powershell
cargo test -p ovrley_core --lib
cargo check --workspace --all-targets
```

### Step 6C: Elevation Widget Split

#### Physical Move

Convert `src-tauri/ovrley_core/src/render/widgets/elevation.rs` into:

- `src-tauri/ovrley_core/src/render/widgets/elevation/mod.rs`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs`
- `src-tauri/ovrley_core/src/render/widgets/elevation/prepare.rs`
- `src-tauri/ovrley_core/src/render/widgets/elevation/frame_state.rs`
- `src-tauri/ovrley_core/src/render/widgets/elevation/reduction.rs`
- `src-tauri/ovrley_core/src/render/widgets/elevation/draw.rs`

#### Target Ownership

1. `normalize.rs`: plot normalization and label-style defaults
2. `prepare.rs`: cache construction, geometry, remaining layer
3. `frame_state.rs`: per-frame marker/completed-path state
4. `reduction.rs`: raw point extraction, smoothing, downsampling, interpolation, simplification
5. `draw.rs`: `draw_elevation_widget` only

#### Verification

```powershell
cargo test -p ovrley_core --lib
cargo check --workspace --all-targets
```

### Step 6D: Value Widget Split

#### Physical Move

Convert `src-tauri/ovrley_core/src/render/widgets/value.rs` into:

- `src-tauri/ovrley_core/src/render/widgets/value/mod.rs`
- `src-tauri/ovrley_core/src/render/widgets/value/layout.rs`
- `src-tauri/ovrley_core/src/render/widgets/value/gradient.rs`
- `src-tauri/ovrley_core/src/render/widgets/value/icons.rs`
- `src-tauri/ovrley_core/src/render/widgets/value/svg.rs`

#### Target Ownership

1. `layout.rs`: metric text/icon/unit layout and vertical-metrics helpers
2. `gradient.rs`: gradient widget rendering and triangle math
3. `icons.rs`: icon-kind mapping, cache, icon draw code, asset lookup
4. `svg.rs`: SVG parsing, tokenization, path conversion
5. `mod.rs`: request struct, feature entry points, re-exports for existing test seams

Keep these public or `pub(crate)` re-exports stable if tests already depend on them:

1. `gradient_triangle_height`
2. `metric_vertical_metrics_text`
3. `metric_icon_top_from_value_layout`

#### Verification

```powershell
cargo test -p ovrley_core --lib
cargo check --workspace --all-targets
```

### Step 6E: Shared Icon Ownership

#### New Shared Asset Location

Create a single source of truth for metric SVGs outside the frontend component tree:

- `assets/widget-icons/widget-speed.svg`
- `assets/widget-icons/widget-heartrate.svg`
- `assets/widget-icons/widget-cadence.svg`
- `assets/widget-icons/widget-power.svg`
- `assets/widget-icons/widget-time.svg`
- `assets/widget-icons/widget-temperature.svg`

#### Required Follow-Through

1. Update backend `include_str!` paths to the shared asset directory.
2. Update frontend imports/usages to the same shared asset directory so the icons are not duplicated long-term.
3. Add a small README or manifest in `assets/widget-icons/` documenting which `MetricIconKind` maps to which file.

#### Verification

```powershell
cargo test -p ovrley_core --lib
rg -n "app/src/components/widgets/icons" src-tauri/ovrley_core/src app/src
```

### Step 6F: Transform Helper Cleanup

#### Files To Edit

- `src-tauri/ovrley_core/src/render/widgets/transform.rs`
- `src-tauri/ovrley_core/src/render/widgets/route.rs:92`
- `src-tauri/ovrley_core/src/render/widgets/elevation.rs:103`

#### Required Change

Change:

```rust
with_widget_transform(canvas, x, y, _width, _height, rotation_deg, draw)
```

to a truthful API such as:

```rust
with_widget_transform(canvas, x, y, rotation_deg, draw)
```

Update the module docs so they no longer claim clipping is happening.

If clipping is actually needed later, add a separate helper with clipping in both implementation and docs. Do not leave dead width/height parameters in place.

#### Verification

```powershell
cargo test -p ovrley_core --lib
rg -n "with_widget_transform\\(" src-tauri/ovrley_core/src/render/widgets
```

### Acceptance Criteria

1. `route`, `elevation`, and `value` stop being feature-level god files.
2. `custom_export_range_active` is typed in `SceneConfig`.
3. `ovrley_core` no longer reaches into `app/src/components/...` for icons.
4. `with_widget_transform` docs and signature match reality.

### Manual Verification Checklist

Because this phase changes visual rendering structure, manually verify all of the following:

1. Route widget geometry, marker position, and trim-window behavior still match pre-refactor output.
2. Elevation widget geometry, smoothing/downsampling behavior, and trim-window behavior still match pre-refactor output.
3. Value widgets still render text, units, gradient triangles, and icons in the same positions as before.
4. Shared metric SVG icons load correctly in both backend rendering and frontend usage.
5. Widget rotation still works after the `with_widget_transform` signature cleanup.

### Verification

```powershell
cargo test -p ovrley_core --lib
cargo test -p ovrley_core --tests -- --test-threads=1
rg -n "custom_export_range_active.*extra|get\\(\"custom_export_range_active\"\\)|app/src/components/widgets/icons|_width|_height" src-tauri/ovrley_core/src
cargo check --workspace --all-targets
```

The final grep should return no stale widget hits.

## Phase 7: Tauri Shell and Core Boundary Cleanup

Audit findings covered: `#11`, `#12`

### Objective

Thin out `src-tauri/src/lib.rs`, move non-boundary logic into focused shell modules, and remove the stale `AppPaths` re-export from the core commands module.

### Files To Create

- `src-tauri/src/runtime_paths.rs`
- `src-tauri/src/tauri_commands.rs`
- `src-tauri/src/preview_import.rs`
- `src-tauri/src/file_ops.rs`

### Files To Edit

- `src-tauri/src/lib.rs`
- `src-tauri/ovrley_core/src/commands/mod.rs`
- every caller importing `commands::AppPaths`

### `AppPaths` Migration

Replace all imports of:

```rust
use crate::commands::AppPaths;
use ovrley_core::commands::AppPaths;
```

with:

```rust
use crate::paths::AppPaths;
use ovrley_core::paths::AppPaths;
```

Known callers that must be migrated:

1. `src-tauri/ovrley_core/src/render/mod.rs`
2. `src-tauri/ovrley_core/src/render/widgets/elevation.rs`
3. `src-tauri/ovrley_core/tests/video_composite_pipeline_tests.rs`
4. `src-tauri/src/bin/render_video.rs`
5. `src-tauri/src/bin/render_preview.rs`
6. `src-tauri/src/bin/parallel_render.rs`
7. `src-tauri/src/bin/benchmark_composite.rs`
8. `src-tauri/src/bin/benchmark_transparent.rs`
9. `src-tauri/src/bin/benchmark_widget_rate.rs`

After all call sites are migrated, delete the re-export from `commands/mod.rs`.

### `src/lib.rs` Target State

Keep only these concerns in `src-tauri/src/lib.rs`:

1. `BackendState`
2. module wiring
3. `run()`
4. `tauri::generate_handler!` registration

Move the rest:

1. `source_repo_root` and `app_paths` to `runtime_paths.rs`
2. preview warning heuristics and MIME detection to `preview_import.rs`
3. template/debug file writes to `file_ops.rs`
4. repeated JSON-string wrapper commands to `tauri_commands.rs`

### Wrapper Consolidation

In `tauri_commands.rs`, add one shared serializer helper so the wrappers stop repeating:

1. call core command
2. map core error to `String`
3. `serde_json::to_string(...)`

Keep the command function names unchanged for Tauri IPC compatibility.

### Bonus Cleanup While Touching This Area

Rename stale internal phase names such as `backend_render_composite_phase3` to current names that describe behavior rather than rollout history.

### Acceptance Criteria

1. `src/lib.rs` becomes a true boundary layer.
2. No code imports `commands::AppPaths`.
3. The stale compatibility re-export is removed.

### Verification

```powershell
cargo check -p app --bins
cargo check --workspace --all-targets
rg -n "commands::AppPaths|pub use crate::paths::AppPaths" src-tauri
```

The final grep should return no hits.

## Phase 8: Benchmark Harness Consolidation and Remaining Test Cleanup

Audit findings covered: `#9`, `#15`, `#16`, `#17`

Risk: Medium

### Files To Create

- `src-tauri/src/benchmark_common.rs`

### Files To Edit

- `src-tauri/src/bin/benchmark_composite.rs`
- `src-tauri/src/bin/benchmark_transparent.rs`
- `src-tauri/src/bin/benchmark_widget_rate.rs`
- `src-tauri/ovrley_core/tests/rdp_tests.rs`
- `src-tauri/ovrley_core/tests/common/test_config.rs`
- `src-tauri/ovrley_core/tests/error_display_tests.rs`

### Benchmark Consolidation

Move only the actually shared benchmark infrastructure into `benchmark_common.rs`:

1. repeated averaging logic
2. repeated cooldown/reporting helpers
3. repeated "successful runs vs failed runs" helpers
4. shared run-metric helpers such as wall-time formatting and file-size normalization
5. codec-availability adapters that use the shared codec catalog

Do not force all three binaries into one universal serialized output type.

Keep these output-specific structs in their owning binaries if their shapes differ:

1. `BenchmarkOutput`
2. `CodecResults`
3. `CodecEntry`
4. `UpdateRateResults`
5. any video-info or render-window payloads whose fields are benchmark-specific

Recommended shared API shape for `benchmark_common.rs`:

1. `CommonRunMetrics`
   Own only the fields that are truly shared across the benchmark binaries.
2. `AverageRunMetrics`
   Own shared average calculations only.
3. `average_successful_runs(...)`
4. `summarize_run_failure(...)`
5. `sleep_between_benchmark_groups(...)`
6. `is_catalog_codec_available(...)` or small typed availability adapters

Keep `bin_common.rs` for generic path/CLI helpers. Do not overload it with benchmark-specific output structs.

### `rdp_tests.rs` Rewrite

Delete the local copies of:

1. `perpendicular_distance`
2. `rdp_simplify`

Rewrite the tests so they call:

1. `ovrley_core::rdp::perpendicular_distance`
2. `ovrley_core::rdp::simplify_rdp_indices`

Assert indices or selected points derived from the production implementation rather than mirroring the algorithm.

### `test_config.rs` Cleanup

1. Delete `test_1080p_video_path()`.
2. Update callers to use `sample_video_path()`.
3. Remove crate-wide `#![allow(dead_code)]`.
4. If some helpers are still intentionally shared but unused by a subset of test crates, add targeted `#[allow(dead_code)]` only on those items or split the registry into smaller modules.

### `ffmpeg_error_display` Fix

Replace the dead test with a real assertion against `CoreError::FfmpegNotFound`, for example:

1. verify the string contains `FFmpeg not found`
2. verify the original message text is preserved

Do not keep a test that constructs a value and asserts nothing.

### Acceptance Criteria

1. Benchmark binaries share one infrastructure layer instead of copy-pasted structs/loops, but each binary keeps its own output schema where the shapes legitimately differ.
2. `rdp_tests.rs` validates production behavior instead of forking the implementation.
3. `test_config.rs` no longer carries a duplicate alias and blanket dead-code suppression.
4. `ffmpeg_error_display` becomes a meaningful test.

### Verification

```powershell
cargo test -p ovrley_core --tests -- --test-threads=1
cargo check -p app --bins
cargo check --workspace --all-targets
```

## Phase 9: Final Stale-Code and Documentation Sweep

Audit findings covered: remaining stale wording attached to `#5`, `#11`, `#12`, `#13`, `#14`

### Objective

Remove rollout-era comments and misleading docs that no longer describe the codebase after the refactor.

### Search Targets

Run targeted searches for stale wording:

```powershell
rg -n "Phase [0-9]|partial|TODO: Remove re-export|deferred refactor|placeholder" src-tauri
```

### Required Fixes

1. Update module docs that still describe obsolete rollout phases.
2. Remove comments that say a refactor is deferred when the refactor has now landed.
3. Ensure every new module has a top-level doc comment explaining ownership boundaries.
4. Update any docs in `ffmpeg_composite.rs`, `video.rs`, `commands/mod.rs`, `transform.rs`, and `render/mod.rs` that were only accurate before this refactor.

### Acceptance Criteria

1. There are no misleading rollout-phase comments left in touched areas.
2. Module docs reflect actual ownership after the split.

## Final Acceptance Checklist

All of the following should be true before the refactor is considered done:

1. `cargo fmt --check` passes.
2. `cargo test -p ovrley_core --lib` passes.
3. `cargo test -p ovrley_core --tests -- --test-threads=1` passes.
4. `cargo test -p ovrley_core --test render_baseline_suite -- --nocapture` passes.
5. `cargo check -p app --bins` passes.
6. `cargo check --workspace --all-targets` passes.
7. Every phase and every named subphase stopped immediately if the render baseline suite failed.
8. Every phase that touched an FFmpeg-backed path longer than a couple of frames was manually validated with the FFmpeg-backed integration suite before completion.
9. No `commands::AppPaths` imports remain.
10. No `stub_render_response` symbol remains.
11. No widget code reads `custom_export_range_active` from `scene.extra`.
12. No backend code `include_str!`s icon assets from `app/src/components/...`.
13. `with_widget_transform` has no dead width/height parameters.
14. `video_composite_pipeline_tests.rs` no longer re-implements production helpers.
15. `rdp_tests.rs` no longer re-implements the RDP algorithm.
16. Benchmark codec matrices are catalog-driven, not comment-driven.
17. `ffmpeg_settings.rs` contains only one `-qscale:v` defaulting path.
18. Transparent overlay default profiles live in `ffmpeg_transparent_profiles.rs` rather than being hardcoded inline in `ffmpeg_settings.rs`.

## Audit Finding To Phase Map

| Audit # | Severity | Planned phase(s)  |
| ------- | -------: | ----------------- |
| 1       |       10 | Phase 1           |
| 2       |        9 | Phase 1D, Phase 2 |
| 3       |        9 | Phase 3           |
| 4       |        8 | Phase 6           |
| 5       |        8 | Phase 4, Phase 9  |
| 6       |        8 | Phase 6           |
| 7       |        7 | Phase 5           |
| 8       |        7 | Phase 6A          |
| 9       |        7 | Phase 3, Phase 8  |
| 10      |        7 | Phase 3           |
| 11      |        6 | Phase 7, Phase 9  |
| 12      |        6 | Phase 7           |
| 13      |        6 | Phase 5           |
| 14      |        6 | Phase 6F          |
| 15      |        6 | Phase 8           |
| 16      |        5 | Phase 8           |
| 17      |        5 | Phase 8           |

## Suggested Done-State File Shape

These are not hard line limits, but they are good guardrails for maintainability:

1. `encode/video.rs`: facade, roughly 250 lines or less
2. `render/mod.rs`: orchestration only, with static-layer logic delegated
3. `render/widgets/route/mod.rs`: thin entry point, with helpers split out
4. `render/widgets/elevation/mod.rs`: thin entry point, with helpers split out
5. `render/widgets/value/mod.rs`: thin entry point, with layout/icons/svg split out
6. `tests/video_composite_pipeline_tests.rs`: assertions plus fixture wiring only, not a second implementation

## Execution Guidance For The LLM Carrying Out This Plan

1. Execute one phase at a time.
2. After each phase, run the verification commands listed for that phase before starting the next one.
3. Prefer mechanical moves first, semantic cleanup second.
4. If a phase starts causing cross-module cycles, move shared types/functions to a neutral leaf module rather than adding back-edges between feature modules.
5. Do not invent new abstractions unless they directly remove one of the audited duplication or ownership problems.
6. Preserve public surfaces unless a narrower internal rename is enough.
7. Treat `cargo test -p ovrley_core --test render_baseline_suite -- --nocapture` as a hard gate after every phase and every named subphase; on any failure, stop and fix the regression before proceeding.

## Phase Change Summary

| Phase   | Risk (1-10) | Notes                                                                                                                                 |
| ------- | ----------: | ------------------------------------------------------------------------------------------------------------------------------------- |
| Phase 1 |           9 | Shared pipeline extraction, runtime helper normalization, cancel-policy handling, and composite helper moves.                         |
| Phase 2 |           3 | Test-only cleanup, fixture helper moves, and deletion of duplicated test-side helper implementations.                                 |
| Phase 3 |           9 | New codec catalogs, builder/profile migration, `HwAccelInfo` cleanup, and transparent-profile extraction out of `ffmpeg_settings.rs`. |
| Phase 4 |           8 | Orchestration split across new encode modules while preserving segmented/composite behavior.                                          |
| Phase 5 |           6 | Static-layer deduplication, cache/module extraction, and dead-code removal with possible visual-regression risk.                      |
| Phase 6 |           8 | Widget file splitting, typed config migration, shared asset relocation, and transform API cleanup.                                    |
| Phase 7 |           6 | Tauri shell boundary cleanup, command wrapper consolidation, and `AppPaths` ownership migration.                                      |
| Phase 8 |           5 | Benchmark infrastructure extraction plus test rewrites that remove duplicated production logic.                                       |
| Phase 9 |           2 | Final stale-comment/doc cleanup after structural work is complete.                                                                    |
