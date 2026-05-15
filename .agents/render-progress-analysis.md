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
`video_pipeline.rs:518-529` — `ProgressEstimator` uses EMA:

```rust
struct ProgressEstimator {
    ema_seconds_per_frame: Option<f64>,
    smoothing_factor: f64,
    warmup_counter: u32,
}

impl ProgressEstimator {
    fn new(smoothing: f64) -> Self {
        Self {
            ema_seconds_per_frame: None,
            smoothing_factor: smoothing,
            warmup_counter: 0,
        }
    }
}

impl Default for ProgressEstimator {
    fn default() -> Self {
        Self::new(0.85)
    }
}

fn record(&mut self, current: u32, total: u32, frame_seconds: f64) -> (Option<u64>, Option<f64>) {
    self.ema_seconds_per_frame = Some(match self.ema_seconds_per_frame {
        Some(previous) => previous * self.smoothing_factor + frame_seconds * (1.0 - self.smoothing_factor),
        None => frame_seconds,
    });
    let remaining = total.saturating_sub(current);
    let estimate = self.ema_seconds_per_frame
        .map(|avg| (avg * remaining as f64).max(0.0).round() as u64);
    let fps = self.ema_seconds_per_frame
        .filter(|&v| v > 0.0)
        .map(|avg| 1.0 / avg);
    (estimate, fps)
}
```

### Issues
1. **No warmup/cropping:** The first frame's timing initializes the EMA directly. First frames are often outliers due to shader compilation, disk caching, Skia asset preparation, etc. This skews the estimate for a significant portion of the render.
2. **Not available in composite pipeline:** The `ProgressEstimator` struct is only used in `video_pipeline.rs`, not in `video_composite_pipeline.rs`.

### Proposed Solution
Modify `ProgressEstimator` to:

1. **Skip initial frames ("crop" / warmup phase):** Ignore the first `WARMUP_FRAMES` (e.g., 5) samples for EMA, to account for asset preparation and caching overhead. During warmup, return `(None, None)` so the UI continues showing `--:--` and no FPS value. Do not average the warmup samples into the first EMA value; that would still let cold-start outliers skew the estimate.
2. **Make smoothing factor configurable via constructor:** `ProgressEstimator::new(smoothing: f64)` replaces the hardcoded constant. `Default` provides 0.85. Usage in each pipeline: `ProgressEstimator::new(0.85)` — easy to experiment with different values.
3. **Share the estimator:** Move `ProgressEstimator` to a shared module such as `src-tauri/ovrley_core/src/encode/progress.rs`, registered from `encode/mod.rs`. Do not duplicate the logic in both pipelines, and do not place it in `video.rs` unless orchestration code already grows a progress-specific module there.
4. **Handle short renders:** If `total <= WARMUP_FRAMES`, keep ETA/FPS blank during the active render and rely on completion to report `Some(0)`. This matches the desired `--:--` warmup behavior and avoids displaying misleading estimates for tiny jobs.

```rust
impl ProgressEstimator {
    const WARMUP_FRAMES: u32 = 5;

    fn new(smoothing: f64) -> Self {
        Self {
            ema_seconds_per_frame: None,
            smoothing_factor: smoothing,
            warmup_counter: 0,
        }
    }

    fn record(&mut self, current: u32, total: u32, frame_seconds: f64) -> (Option<u64>, Option<f64>) {
        // Warmup: skip cold-start samples entirely.
        self.warmup_counter += 1;
        if self.warmup_counter <= Self::WARMUP_FRAMES {
            return (None, None);
        }

        // EMA after warmup — initialize from the first post-warmup sample.
        self.ema_seconds_per_frame = Some(match self.ema_seconds_per_frame {
            Some(previous) => previous * self.smoothing_factor + frame_seconds * (1.0 - self.smoothing_factor),
            None => frame_seconds,
        });

        let remaining = total.saturating_sub(current);
        let estimate = self.ema_seconds_per_frame
            .map(|avg| (avg * remaining as f64).max(0.0).round() as u64);
        let fps = self.ema_seconds_per_frame
            .filter(|&v| v > 0.0)
            .map(|avg| 1.0 / avg);
        (estimate, fps)
    }
}
```

For composite mode, either pass an output-frame-equivalent multiplier to the estimator or convert the returned overlay FPS/ETA at the call site. The important invariant is that the UI receives output-frame-equivalent FPS and ETA because `current` and `total` are output-frame-equivalent.

---

## 4. Display Rendering FPS in RenderProgressPanel

### Problem
The UI does not show the current frame production rate. Only ETA and frame counts are shown.

### Proposed Solution

**Backend changes (Rust):**

Add `rendering_fps: Option<f64>` to the `RenderProgress` struct (`debug/mod.rs`):

```rust
pub struct RenderProgress {
    pub render_id: u64,
    pub current: u32,
    pub total: u32,
    pub encoded: u32,
    pub status: String,
    pub message: String,
    pub estimated_seconds_remaining: Option<u64>,
    pub rendering_fps: Option<f64>,  // NEW
    pub filename: Option<String>,
}
```

Set it in `ProgressEstimator::record()` alongside the EMA:

```rust
fn record(&mut self, current: u32, total: u32, frame_seconds: f64) -> (Option<u64>, Option<f64>) {
    // ... EMA logic ...
    let fps = self.ema_seconds_per_frame
        .filter(|&v| v > 0.0)
        .map(|avg| 1.0 / avg);
    (estimate, fps)
}
```

Update `set_frame_progress()` to accept and store FPS. Wire it through the composite pipeline as well.

FPS semantics:
- Transparent mode: report pipeline production FPS for final output frames.
- Composite mode: report **final output-frame-equivalent FPS**, not raw overlay frames per second. For example, if the overlay pipe produces 10 overlay frames/sec with `composite_widget_update_rate = 6`, the UI should display about 60 FPS.

Timing semantics:
- The transparent pipeline currently times render + queueing from the producer loop, while actual `stdin.write_all()` runs in the writer thread. This is usually a useful pipeline-throughput proxy because queue backpressure is included when FFmpeg cannot keep up, but it is not a pure FFmpeg encode FPS.
- If Phase 7 timing work adds `frame.total` and `ffmpeg.write` buckets for composite, use the same measured loop duration for the live estimator so debug output and UI agree.

**Frontend changes (JSX):**

In `RenderProgressPanel.jsx`, add an FPS display section alongside the ETA:

```jsx
{!isFinalizing && (
  <div className="flex items-center justify-center gap-6 pt-2">
    <div className="flex flex-col items-center">
      <div className="mb-1 flex items-center gap-1.5 text-muted-foreground">
        <span className="text-[10px] font-bold uppercase tracking-wider">Render FPS</span>
      </div>
      <span className="text-lg font-mono font-bold text-foreground">
        {renderingFps != null ? `${renderingFps.toFixed(1)}` : '--'}
      </span>
    </div>
    <div className="flex flex-col items-center">
      {/* Existing ETA display */}
    </div>
  </div>
)}
```

In `useRenderProgressPolling.js`, map `data.rendering_fps` into the store.

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

3. Add frontend/store checks:
   - `rendering_fps` from backend maps to `renderingFps`.
   - `DEFAULT_RENDER_PROGRESS` includes `renderingFps: null`.
   - The progress bar still calculates percent only from `current / total`.

## Summary

1. **ETA for compositing:** Use the shared `ProgressEstimator` in the composite pipeline and convert overlay-frame timing to final output-frame-equivalent ETA.
2. **Uneven progress:** Keep mathematically correct output-frame progress, smooth the transform-based progress bar, and report composite `encoded` in output-frame-equivalent space.
3. **Better ETA calculation:** Move the estimator to a shared module and truly skip the first warmup samples instead of averaging them into the EMA.
4. **FPS display:** Add `rendering_fps` through the backend controller, polling hook, store, and UI, with composite mode reporting final output-frame-equivalent FPS.
