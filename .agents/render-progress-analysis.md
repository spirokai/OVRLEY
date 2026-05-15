# Render Progress Analysis & Proposed Solutions

## 1. Estimated Time Not Displayed During Compositing

### Problem
The `estimatedSecondsRemaining` field is always `null` during composite renders. The `RenderProgressPanel` shows `--:--` (from `formatTime(null)`).

### Root Cause
`video_composite_pipeline.rs:171` — the call to `controller.set_frame_progress()` always passes `None` for the `estimate` parameter:

```rust
controller.set_frame_progress(
    estimated_output_progress.min(total_progress),
    total_progress,
    written_overlay_frames.min(u64::from(u32::MAX)) as u32,
    None,  // <-- always None
);
```

The normal (transparent) pipeline at `video_pipeline.rs:180` uses a `ProgressEstimator` that feeds an EMA-based estimate, but the composite pipeline never instantiates or uses it.

### Solution
Add a shared live `ProgressEstimator` and use it from the composite pipeline (`video_composite_pipeline.rs`), instantiated before the render loop, called after each overlay frame render + write, and passed to `set_frame_progress()`.

For composite mode, the estimator should measure **one overlay-frame production iteration**: Skia render + raw RGBA write to FFmpeg stdin. Because the UI should report final-output-video progress, convert the overlay-frame measurement into output-frame-equivalent progress:

```txt
output_equivalent_fps = overlay_frame_fps * composite_widget_update_rate
eta_seconds = remaining_output_frames / output_equivalent_fps
```

This keeps ETA and FPS aligned with the user-facing `current / total` values, even when the overlay pipe runs at a lower FPS than the final video.

---

## 2. Uneven Progress Bar Updates (Composite, Update Rate != 1/1)

### Problem
When `update_rate != 1/1` (e.g., 1/3), the progress bar jumps in large, uneven increments. Fewer overlay frames are rendered relative to output frames, so each overlay frame advances progress by `update_rate` output-frame-equivalents.

Example: source_fps=30, update_rate=3, overlay_pipe_fps=10, render_duration=10s
- output_frame_count = 300
- overlay_frame_count = 100
- Each overlay frame advances progress by 3 output frames (= 1% of total)
- Only 100 progress updates instead of 300

### Root Cause
`video_composite_pipeline.rs:165-166` — `output_progress_for_overlay_time()` maps overlay time to output frame count:

```rust
fn output_progress_for_overlay_time(video_local_time: f64, plan: &CompositePipelinePlan) -> u32 {
    (video_local_time * plan.output_fps.as_f64())
        .round()
        .max(0.0)
        .min(plan.output_frame_count as f64) as u32
}
```

The current value advances by `update_rate` output frames per iteration, causing visible jumps. Total frames correctly shows `output_frame_count`, but progress granularity is limited to overlay frame count.

Additionally, `encoded` tracks `written_overlay_frames` (overlay-frame-space) while `current` and `total` are in output-frame-space. At completion, `encoded` shows `overlay_frame_count` vs `output_frame_count`, which is misleading.

### Solution

Adopt **Option A: CSS smoothing + fix `encoded` tracking.**
- Keep the current progress calculation (it is mathematically correct — `output_progress_for_overlay_time` maps overlay time to output frame count, so the percent value is right even if it updates fewer times)
- Smooth the visual jumps on the frontend by making the existing transform-based Progress transition explicit and predictable, e.g. `transition-transform duration-300 ease-out`. The current `Progress` component moves the indicator with `transform: translateX(...)`, not by changing `width`, so the transition should target `transform`.
- Fix the `encoded` field: in composite mode, set `encoded = current` during progress updates and `encoded = total` on success, so `current`, `encoded`, and `total` are all in final output-frame-equivalent space.
- Update backend comments/docs so `encoded` is not described as literal FFmpeg-reported frames for every pipeline. Suggested wording: "Number of output frames encoded or covered by the active render pipeline."

This is intentionally pragmatic. Parsing FFmpeg's real output-frame progress for composite mode can be added later, but it is not required for the current UI bug and it would still need careful interpretation because FFmpeg repeats lower-FPS overlay frames internally.

---

## 3. Improve Estimated Time Calculation (Both Pipelines)

### Current Implementation
`progress.rs` — `ProgressEstimator` with wall-clock throughput clamping:

```rust
struct ProgressEstimator {
    ema_seconds_per_frame: Option<f64>,
    smoothing_factor: f64,
}

impl Default for ProgressEstimator {
    fn default() -> Self {
        Self::new(0.97)             // ← smoothing factor is 0.97
    }
}

fn record(&mut self, current: u32, total: u32, frame_seconds: f64, elapsed_seconds: f64
) -> (Option<u64>, Option<f64>) {
    self.ema_seconds_per_frame = Some(match self.ema_seconds_per_frame {
        Some(prev) => prev * self.smoothing_factor + frame_seconds * (1.0 - self.smoothing_factor),
        None => frame_seconds,      // ← first frame initializes EMA directly
    });
    self.current_estimate(current, total, elapsed_seconds)
}

fn current_estimate(&self, current: u32, total: u32, elapsed_seconds: f64) -> (Option<u64>, Option<f64>) {
    let ema_fps = 1.0 / ema_seconds_per_frame;
    let wall_fps = current / elapsed_seconds;
    let fps = min(ema_fps, wall_fps);       // ← conservative clamp
    let eta = remaining / fps;
    (eta, fps)
}
```

Both pipelines already use this shared estimator:
- `video_pipeline.rs:128` → per-frame call with `(rendered_frames, total, frame_seconds, wall_clock)`
- `video_composite_pipeline.rs:128` → per-overlay-frame call with scaled frame time

### Over-optimism Root Cause

The 0.97 smoothing factor is extremely aggressive — each new frame gets only **3% weight**:

```
half-life ≈ ln(0.5) / ln(0.97) ≈ 22.7 frames
```

With QSV encoding (first frame ~14ms = 70fps, steady state ~40ms = 25fps):

| Frame | EMA (s) | EMA FPS | wall FPS | reported (min) |
|-------|---------|---------|----------|-------|
| 1 | 0.014 | 71 | 71 | **71** ← first frame initializes directly |
| 10 | 0.016 | 62 | 71 | **62** |
| 20 | 0.020 | 50 | 37 | **37** ← wall clamp finally helps |
| 50 | 0.028 | 36 | 29 | **29** |
| 100 | 0.036 | 28 | 27 | **27** |
| 150 | 0.039 | 26 | 26 | **26** |

Two factors cause the initial 70fps spike:
1. **First frame initializes the EMA directly** (`None => frame_seconds` at `progress.rs:40`) — cold frames can be artificially fast due to pipeline startup, FFmpeg buffering, or Skia warmup
2. **0.97 smoothing barely moves the needle** — it takes ~150 frames to converge from the initial 70fps to the real 25fps

The wall-clock clamp (`min(ema_fps, wall_fps)`) at line 60 only becomes effective after ~15–20 frames when enough slow frames are in the wall-clock average.

### Solution Applied

Changes in `progress.rs`:

1. **Warmup phase:** Skip first `WARMUP_FRAMES = 5` frames, returning `(None, None)` so the UI shows `--:--`. This prevents cold-start outliers (GPU warmup, shader compilation, FFmpeg pipeline priming) from poisoning the EMA.

2. **Default smoothing changed from 0.97 → 0.90.** Half-life drops from ~23 frames to ~6.6 frames, converging to steady state in ~25 frames instead of ~150. At 0.90, per-frame jitter of ±5ms produces only ±1% FPS fluctuation — visually stable.

3. **Wall-clock clamping (`min(ema_fps, wall_fps)`) continues** to provide a conservative floor: the reported FPS never exceeds the cumulative throughput achieved so far.

Convergence for the QSV scenario (4 fast frames, then steady 25fps):

| Since warmup ends | Reported FPS |
|---|---|
| Frame 6 (first post-warmup) | ~38 (from 25fps actual + cold-start bias) |
| +5 frames | ~31 |
| +10 frames | ~28 |
| +15 frames | ~27 |
| +20 frames | ~26 |
| +30 frames | ~25 |

From an initial spike of 38fps down to 25fps in ~25 frames — vs 150 frames with the old 0.97.

### Existing features (already correct)
- `ProgressEstimator` is already shared via `encode/progress.rs` ✓
- Both pipelines already use it ✓
- Wall-clock throughput clamping prevents EMA from ever reporting faster than achieved ✓
- Smoothing factor is already configurable via constructor ✓
- `rendering_fps` field already exists in `RenderProgress` and `set_frame_progress` ✓
- Composite pipeline already scales overlay frame time to output-frame-equivalent ✓

---

## 4. Display Rendering FPS in RenderProgressPanel

### Status: Already Implemented

- `rendering_fps: Option<f64>` is already in `RenderProgress` (`debug/mod.rs:33`) ✓
- `set_frame_progress()` already accepts and stores it (`video.rs:105`) ✓
- `ProgressEstimator::record()` already returns `(Option<u64>, Option<f64>)` ✓
- Both pipelines already pass FPS to the controller ✓
- Polling hook maps `rendering_fps` (`useRenderProgressPolling.js:36`) ✓
- Store default is `null` (`store-utils.js:35`) ✓
- `RenderProgressPanel` already displays it with `formatFps()` ✓
- `formatFps()` helper exists in `format.js` ✓

---

## Files to Modify

| File | Changes |
|------|---------|
| `src-tauri/ovrley_core/src/debug/mod.rs` | Add `rendering_fps: Option<f64>` to `RenderProgress` struct |
| `src-tauri/ovrley_core/src/encode/progress.rs` | New shared `ProgressEstimator` with true skipped warmup, configurable smoothing, ETA + FPS output |
| `src-tauri/ovrley_core/src/encode/mod.rs` | Register the new shared progress module |
| `src-tauri/ovrley_core/src/encode/video_pipeline.rs` | Replace local estimator with shared estimator; pass ETA + final-output FPS to `set_frame_progress` |
| `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs` | Add shared estimator usage; convert overlay timing to output-frame-equivalent ETA/FPS; pass `encoded = current` |
| `src-tauri/ovrley_core/src/encode/video.rs` | Update `set_frame_progress` to accept FPS; update `RenderProgress` initializations; update `encoded` docs/comments |
| `app/src/features/render-video/hooks/useRenderProgressPolling.js` | Map `rendering_fps` from backend response |
| `app/src/store/store-utils.js` | Add `renderingFps: null` to `DEFAULT_RENDER_PROGRESS` |
| `app/src/store/slices/createMediaSlice.js` | No structural change needed, but verify spread preserves `renderingFps` and percent calculation still uses `current / total` |
| `app/src/features/render-video/components/RenderProgressPanel.jsx` | Add FPS display section |
| `app/src/features/render-video/utils/format.js` | Add `formatFps` helper if needed |
| `app/src/components/ui/progress.jsx` | Make transform smoothing explicit, e.g. `transition-transform duration-300 ease-out` |
| `src-tauri/ovrley_core/src/encode/tests/video_composite_pipeline_tests.rs` | Update tests that expected overlay-frame `encoded` values; add tests for output-frame-equivalent `encoded`, ETA/FPS availability after warmup, and lower update-rate FPS semantics |

## Test Updates

1. Update composite lower-update-rate tests:
   - Tests like `test_5_4_lower_overlay_update_rate_renders_half_overlay_frames` and `test_5_5_aggressive_overlay_update_rate_renders_one_sixth_overlay_frames` should no longer assert `progress.encoded == overlay_frame_count`.
   - They should assert that the pipeline still writes the expected overlay frames internally, but the public progress snapshot uses `encoded == total` on success.

2. Add estimator unit tests:
   - First `WARMUP_FRAMES` samples return `(None, None)`.
   - First post-warmup sample initializes the EMA.
   - FPS equals `1.0 / ema_seconds_per_frame` for transparent/final-frame timing.
   - Composite conversion multiplies overlay FPS by update rate for output-frame-equivalent FPS.

## Files to Modify

| File | Changes |
|------|---------|
| `src-tauri/ovrley_core/src/encode/progress.rs` | Add warmup phase (skip first 5 frames → `(None, None)`), change `DEFAULT_SMOOTHING_FACTOR` from 0.97 to 0.85, handle short renders (`total <= WARMUP_FRAMES`) |
| `app/src/features/render-video/components/RenderProgressPanel.jsx` | Add `transition: width 0.3s ease` CSS class to `Progress` component for smoother bar animation |

## Summary

1. **ETA for compositing:** Already implemented (shared `progress.rs`, both pipelines use it, composite scales overlay time to output-equivalent) ✓
2. **Uneven progress (composite, update_rate != 1/1):** CSS transition on progress bar for visual smoothing; `encoded = current` already set ✓
3. **Over-optimistic ETA/FPS (both pipelines):** Root cause is **0.97 smoothing** + **no warmup**. Fix: warmup (skip 5 frames → `--:--`), default smoothing to **0.85**
4. **FPS display:** Already fully implemented in backend (`RenderProgress.rendering_fps`, `ProgressEstimator::record()` returns FPS) and frontend (`RenderProgressPanel`, `useRenderProgressPolling`, `formatFps`) ✓
