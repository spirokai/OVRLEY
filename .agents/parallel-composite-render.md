# Parallel Chunked Composite Render — Implementation Plan

## Goal

Split a composite video render into N time segments, render+encode each segment in parallel across CPU cores, then stitch them with FFmpeg stream copy. This scales throughput nearly linearly with available cores.

## Current bottleneck

The render loop runs at ~10.6ms/frame on a single CPU core. On an 8-core machine, 1 core is fully loaded while 7 are mostly idle. Parallel chunking lets 4 cores each render ¼ of the frames simultaneously.

## Why not use the existing `render_video_segmented_qtrle`?

That function is hardcoded for the transparent pipeline (`render_video_single`, qtrle codec, `config.scene.start/end` windowing). The composite pipeline has different parameters (`composite_render_duration`, `composite_video_trim_start`, source video path, overlay params). It needs its own segmentation function.

## Existing infrastructure (reusable)

All of these exist in `video.rs` and `video_debug.rs`:

| Component | Location | Reuse |
|-----------|----------|-------|
| `concat_video_segments` | `video_debug.rs:73` | ✅ Direct reuse — accepts filename list, runs FFmpeg concat |
| `child_render_controller` | `video.rs:555` | ✅ Direct reuse — shares parent cancel flag |
| `cleanup_segment_outputs` | `video.rs:569` | ✅ Direct reuse |
| `estimate_parallel_render_worker_count` | `video.rs:252` | ✅ Direct reuse |
| Progress aggregation pattern | `video.rs:414-455` | Adapt — sum current/encoded across segments |
| Cancel propagation pattern | `video.rs:448-450` | Adapt — stop all segments on any error |

## New code needed

### 1. `render_composite_video_segmented` in `video.rs`

New function parallel to `render_video_segmented_qtrle` but tailored for composite parameters.

**Signature:**
```rust
fn render_composite_video_segmented(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    controller: &RenderController,
    composite_video_path: &str,
    composite_bitrate: &str,
    composite_sync_offset: f64,
    composite_video_fps_num: u32,
    composite_video_fps_den: u32,
    composite_video_duration: f64,
    composite_render_duration: f64,
    composite_video_trim_start: f64,
    composite_widget_update_rate: u32,
) -> Result<String, String>
```

**Steps:**

#### 1a. Determine segment count

```rust
let segment_count = estimate_parallel_render_worker_count(
    composite_render_duration.ceil() as usize
).max(1);
if segment_count < 2 {
    return render_composite_video_single(..., composite_render_duration, ...);
}
```

#### 1b. Compute segment windows

Unlike `render_video_segmented_qtrle` which uses `config.scene.start/end`, composite segments are defined by splitting `composite_render_duration` into equal chunks. Base segment duration:

```rust
let segment_duration = composite_render_duration / segment_count as f64;
let segments: Vec<(f64, f64)> = (0..segment_count)
    .map(|i| {
        let start = i as f64 * segment_duration;
        let end = ((i + 1) as f64 * segment_duration).min(composite_render_duration);
        (start, end)
    })
    .collect();
```

Each segment gets:
- `composite_render_duration = segment_end - segment_start`
- `composite_video_trim_start = parent_trim_start + segment_start`
- Same `dense_activity` (composite uses time-based frame mapping, not dense frame indices)
- Same `composite_sync_offset` (shifts with trim adjust)
- Same `composite_video_path`, `composite_bitrate`, FPS params

#### 1c. Spawn parallel render threads

Same pattern as `render_video_segmented_qtrle` lines 390-410:

```rust
let (tx, rx) = mpsc::channel::<SegmentEvent>();
let mut handles = Vec::with_capacity(segment_count);

for (index, (seg_start, seg_end)) in segments.iter().enumerate() {
    let tx = tx.clone();
    let segment_controller = child_render_controller(
        /* total frames for this segment */,
        &child_cancel_flag,
    );
    let segment_paths = paths.clone();
    let segment_config = config.clone();
    let segment_activity = activity.clone();
    let segment_dense = dense_activity.clone();
    let segment_video_path = composite_video_path.to_string();
    let segment_bitrate = composite_bitrate.to_string();

    let handle = thread::spawn(move || {
        let result = render_composite_video_single(
            &segment_paths,
            &segment_config,
            &segment_activity,
            &segment_dense,
            &segment_controller,
            &segment_video_path,
            &segment_bitrate,
            composite_sync_offset,   // same offset
            composite_video_fps_num,
            composite_video_fps_den,
            composite_video_duration, // full video duration (for FFmpeg -t)
            Some(seg_end - seg_start), // segment render duration
            Some(parent_trim_start + seg_start), // segment trim
            Some(composite_widget_update_rate),
        );
        let _ = tx.send(SegmentEvent::Completed(index, result));
    });
    handles.push(handle);
}
```

#### 1d. Aggregate progress

Same pattern as `render_video_segmented_qtrle` lines 417-455. Poll `segment_controllers` every 200ms, sum `current`/`encoded`, propagate to parent controller.

#### 1e. Stitch

After all segments succeed, call `concat_video_segments` with the segment filenames. Clean up temp segment files via `cleanup_segment_outputs`.

### 2. Update `render_composite_video` entry point

Current (line 296):
```rust
pub fn render_composite_video(...) -> Result<String, String> {
    render_composite_video_single(...)
}
```

Add segmentation gating, similar to `render_video`:

```rust
pub fn render_composite_video(...) -> Result<String, String> {
    if should_parallelize_composite(
        composite_render_duration,
        composite_widget_update_rate,
        composite_video_fps_num,
    ) {
        return render_composite_video_segmented(
            paths, config, activity, dense_activity, controller,
            composite_video_path, composite_bitrate,
            composite_sync_offset,
            composite_video_fps_num, composite_video_fps_den,
            composite_video_duration, composite_render_duration.unwrap_or(...),
            composite_video_trim_start.unwrap_or(0.0),
            composite_widget_update_rate.unwrap_or(1),
        );
    }
    render_composite_video_single(...)
}
```

The `should_parallelize_composite` gate:
```rust
fn should_parallelize_composite(
    render_duration: Option<f64>,
    update_rate: Option<u32>,
    fps_num: u32,
) -> bool {
    // Only parallelize when there are enough frames to make it worth it.
    // At least 2 seconds of video at the overlay FPS.
    let duration = render_duration.unwrap_or(0.0);
    let overlay_fps = fps_num as f64 / update_rate.unwrap_or(1) as f64;
    let total_frames = (duration * overlay_fps).ceil() as u32;
    total_frames >= 120 // at least ~4 seconds at 30fps
        && estimate_parallel_render_worker_count(duration.ceil() as usize) >= 2
}
```

### 3. Sync offset adjustment

`composite_sync_offset` currently shifts the entire render timeline. In segmented mode, each segment inherits the same offset — no change needed because the trim already advances the video position correctly.

Wait: sync offset needs careful consideration. Currently:
```
activity_time = composite_sync_offset + video_local_time
video_local_time = overlay_frame_index / overlay_pipe_fps
```

When segment N starts at `seg_start` seconds into the render, `video_local_time` for its first frame is `seg_start`. The activity time becomes `composite_sync_offset + seg_start` — which is correct because the activity data at that time matches what the source video shows.

So sync offset is preserved: each segment starts at the correct activity time because `composite_sync_offset` is the same and `composite_video_trim_start` advances by `seg_start`.

### 4. Frame count verification per segment

Each segment's `render_composite_video_single` call returns its own frame count check. The stitch happens after all segments report success. No additional frame count check needed at the aggregate level — `concat_video_segments` verifies the final file exists and is non-empty.

### 5. Temporary segment file cleanup

Segment filenames follow the existing naming convention (`video_composited_{timestamp}.mp4`). After stitching, all segment files are removed via `cleanup_segment_outputs`.

## Changes by file

| File | Change |
|------|--------|
| `ovrley_core/src/encode/video.rs` | Add `render_composite_video_segmented`, update `render_composite_video`, add `should_parallelize_composite` |
| (none else) | All infrastructure already exists |

## Implementation order

1. Add `should_parallelize_composite` gate function
2. Add `render_composite_video_segmented` function
3. Wire it into `render_composite_video` entry point
4. Test with 30-second render, 4 segments, verify output matches single-segment
5. Benchmark speedup

## Expected speedup

- Single segment: ~11s for 30s video (2.7x real-time)
- 4 segments on 8-core CPU: ~3-4s (7.5-10x real-time)
- Stitch overhead: ~0.1-0.3s (stream copy, negligible)
- Memory: N segments × ~75 MB buffer pool

## Risk areas

- **Activity frame index misalignment**: If `composite_sync_offset + segment_start` produces a frame index outside the dense activity range, the segment fails. The existing `dense_frame_index_for_overlay` function already validates this — same validation applies to segments.
- **FFmpeg HW device per segment**: Each `spawn_composite_ffmpeg_process` call creates its own QSV device. On Windows, `-init_hw_device dxva2=dx` creates a shared DXVA2 device. Multiple concurrent instances may compete for GPU resources. Test with 2-3 segments first.
- **Audio**: The first segment gets audio (`-map 0:a?`). Subsequent segments won't find audio (it's already consumed by the first FFmpeg call's input). `concat_video_segments` with `-c copy` handles this correctly — it concatenates audio streams from segment 1 and drops audio-only segments.
- **Non-integer durations**: If `composite_render_duration` isn't evenly divisible, the last segment gets the remainder. The fractional overrun guard in `render_composite_video_single` already handles this.
