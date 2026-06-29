# MP4 Telemetry Finalizer Wiring Plan

## Goal

Wire `src-tauri/ovrley_core/src/media/mp4_telemetry` into the single backend
activity finalizer so MP4 telemetry follows the same final `ParsedActivity`
construction path as FIT/GPX/SRT, while preserving MP4-specific pre-treatment.

## Decisions

### D-A: Canonical timestamp name is `sync_time`

`sync_time` is the canonical field name across backend, frontend, store, and
render code.

Meaning: the absolute timestamp corresponding to activity/video time zero. It is
the timestamp used to align telemetry time with video time.

Rules:

- Add `sync_time` to `ParsedActivity`.
- `activity::finalize` computes:
  ```text
  sync_time = metadata.sync_time ?? first valid sample timestamp
  ```
- MP4 adapter must preserve extracted MP4 sync time as `metadata.sync_time`.
- FIT/GPX/SRT can omit `metadata.sync_time`; the activity finalizer derives it
  from the first valid sample timestamp.
- `source_start_time` is deprecated. Do not add new logic against it. Migrate
  frontend/store/render consumers to `sync_time`, then remove `source_start_time`
  once call sites are gone.

### D-B: There is one activity finalizer

The single finalization pipeline is `activity::finalize`.

It owns:

- idle gap fill
- elapsed/distance/course/time/progress series
- metric derivation
- optional per-metric smoothing
- final `ParsedActivity` assembly
- debug payload generation

Use the name **activity finalizer** for this pipeline. Avoid introducing
additional "finalizer" names.

### D-C: MP4 media owns MP4-domain pre-treatment

MP4 media extraction remains responsible for camera/video-specific processing
that must happen before generic activity finalization.

It owns:

- telemetry-parser / DJI AC004 extraction
- native stream normalization into `NativeSample`
- pre-cull smoothing of continuous native streams
- aligning GPS/IMU/camera domains to a single activity timeline
- closest-in-time selection
- hold-forward for discrete camera settings

It must not own final `ParsedActivity` assembly.

### D-D: Preserve pre-cull smoothing

Current `media/mp4_telemetry/smoothing.rs::smooth_series()` must remain before
closest-in-time culling.

Reason: if we cull first, we discard the high-rate samples needed for useful
zero-phase smoothing.

MP4 should send finalizer options:

```json
{
  "skip_idle_gap_fill": true,
  "smoothing": {}
}
```

This avoids synthetic idle insertion and avoids double-smoothing after MP4 has
already smoothed continuous native streams before culling.

### D-E: Hold-forward is MP4 adapter behavior, not generic finalizer behavior

Media currently carries forward some values while aligning telemetry domains.
This is not the same as idle gap insertion.

Preserve hold-forward for discrete camera step fields:

- `iso`
- `aperture`
- `shutter_speed`
- `focal_length`
- `ev`
- `color_temperature`

Do not add generic finalizer carry-forward for all formats. FIT/GPX/SRT missing
values should usually remain missing unless derivation fills them. Continuous
MP4 fields should not be blindly held forward by the generic finalizer.

### D-F: Long-term finalizer input should be columnar

`ParsedActivity` is already columnar, and MP4 alignment naturally produces
columns. Large multi-hour 30fps telemetry is more efficient as columnar data:
fewer objects, fewer repeated field names, fewer null fields, and less reshaping
inside the finalizer.

Do not make row-oriented `RawActivity` the permanent internal shape.

Incremental path:

- Keep frontend FIT/GPX/SRT row `RawActivity` contract for now.
- Add an internal columnar finalizer core.
- Convert row `RawActivity` to columns at the finalizer boundary.
- Let MP4 emit columns directly after alignment.

## Target Architecture

```text
FIT/GPX/SRT frontend extraction
    -> RawActivity rows
    -> activity::finalize converts rows to ActivityColumns

MP4 media extraction
    -> NativeSample streams
    -> pre-cull smoothing
    -> closest-in-time timeline alignment
    -> hold-forward discrete camera fields
    -> ActivityColumns directly

ActivityColumns
    -> activity::finalize core
    -> ParsedActivity
```

## Implementation Steps

### 1. Introduce canonical `sync_time`

- Add `sync_time: Option<String>` to `ParsedActivity`.
- Update `activity::finalize` to compute canonical sync time:
  ```text
  metadata.sync_time if valid string else first valid sample timestamp
  ```
- Write `parsed_activity.sync_time = computed_sync_time`.
- Update frontend/store/render consumers to use `sync_time`.
- Stop writing new code against `source_start_time`.
- Remove or deprecate `source_start_time` after consumers are migrated.
- Update debug payload expectations and docs to use `sync_time`.

### 2. Add internal columnar raw activity type

Add an activity-owned type, for example:

```rust
pub struct ActivityColumns {
    pub file_name: String,
    pub file_format: String,
    pub metadata: serde_json::Value,
    pub options: RawActivityOptions,
    pub elapsed_seconds: Vec<Option<f64>>,
    pub timestamp: Vec<Option<String>>,
    pub latitude: Vec<Option<f64>>,
    pub longitude: Vec<Option<f64>>,
    pub elevation: Vec<Option<f64>>,
    pub altitude: Vec<Option<f64>>,
    pub speed: Vec<Option<f64>>,
    pub heading: Vec<Option<f64>>,
    pub heartrate: Vec<Option<f64>>,
    pub cadence: Vec<Option<f64>>,
    pub power: Vec<Option<f64>>,
    pub temperature: Vec<Option<f64>>,
    pub gradient: Vec<Option<f64>>,
    pub pace: Vec<Option<f64>>,
    pub distance: Vec<Option<f64>>,
    pub g_force: Vec<Option<f64>>,
    pub vertical_speed: Vec<Option<f64>>,
    pub torque: Vec<Option<f64>>,
    pub stroke_rate: Vec<Option<f64>>,
    pub stride_length: Vec<Option<f64>>,
    pub vertical_oscillation: Vec<Option<f64>>,
    pub ground_contact_time: Vec<Option<f64>>,
    pub left_right_balance: Vec<Option<f64>>,
    pub core_temperature: Vec<Option<f64>>,
    pub air_pressure: Vec<Option<f64>>,
    pub gear_position: Vec<Option<f64>>,
    pub iso: Vec<Option<f64>>,
    pub aperture: Vec<Option<f64>>,
    pub shutter_speed: Vec<Option<f64>>,
    pub focal_length: Vec<Option<f64>>,
    pub ev: Vec<Option<f64>>,
    pub color_temperature: Vec<Option<f64>>,
}
```

Requirements:

- Represent every metric currently supported by the finalizer.
- Validate equal column lengths at construction or finalizer entry.
- Keep `ActivityColumns` owned by `activity`, not by `media`.

### 3. Convert existing row `RawActivity` to columns

- Add an explicit converter, e.g.:
  ```rust
  ActivityColumns::from_raw_activity(&RawActivity) -> CoreResult<ActivityColumns>
  ```
- Keep `finalize_raw_activity_json` accepting current frontend row payloads.
- Internally, route row input through:
  ```rust
  let columns = ActivityColumns::from_raw_activity(&raw_activity)?;
  finalize_activity_columns(&columns)
  ```

### 4. Move finalizer core to columns

Refactor current `finalize_with_debug` into a columnar core, e.g.:

```rust
fn finalize_activity_columns(columns: &ActivityColumns) -> FinalizedActivity
```

The core should:

- read series directly from columns
- build course/time/elapsed/distance/progress series
- derive metrics from columns
- apply per-metric smoothing
- assemble `ParsedActivity`

Preserve FIT/GPX/SRT output behavior by comparing existing golden references.

### 5. Refactor metric direct extraction

- Replace `direct_metrics(raw_samples, ...)` with column-based direct metric
  collection.
- Ensure missing metrics remain all-null columns.
- Preserve derivations:
  - `distance` from course
  - `speed` from distance + elapsed
  - `heading` from course + distance
  - `gradient` from elevation + distance
  - `vertical_speed` from elevation + elapsed
  - `pace` from speed
  - `torque` from power + cadence

### 6. Create MP4 activity adapter

Replace `media/mp4_telemetry/activity.rs::build_parsed_activity` with an adapter
that outputs `ActivityColumns`, not `ParsedActivity`.

Keep the adapter near MP4 extraction because the alignment rules are
MP4-domain-specific:

```text
media/mp4_telemetry/activity.rs
    build_activity_columns(...)
```

Adapter responsibilities:

- choose GPS-anchored timeline when GPS exists
- use video-derived timeline only when GPS is absent
- align IMU and camera domains by closest-in-time
- hold-forward discrete camera fields independently
- put GPS altitude into both:
  - `altitude`
  - `elevation`
- set `elapsed_seconds = timestamp_ms / 1000.0`
- preserve timestamps where available
- include MP4 metadata:
  - `camera_type`
  - `camera_model`
  - `telemetry_source`
  - `timeline_kind`
  - telemetry sample counts
  - `sync_time`

### 7. Remove MP4 final assembly duplication

Delete or shrink MP4-specific code that currently duplicates finalizer work:

- distance computation
- distance progress computation
- `ParsedActivity` assembly
- empty metric vector assembly
- `sample_elevations` handling

Those responsibilities should belong only to `activity::finalize`.

### 8. Wire MP4 extraction through the activity finalizer

Change `mp4_telemetry::extract_activity` from:

```text
extract -> smooth_series -> build_parsed_activity -> ParsedActivity
```

to:

```text
extract -> smooth_series -> build_activity_columns -> activity::finalize::finalize_activity_columns -> ParsedActivity
```

Keep the public return type for command compatibility:

```rust
CoreResult<Option<ParsedActivity>>
```

### 9. Set MP4 finalizer options

MP4 adapter should use:

```rust
RawActivityOptions {
    skip_idle_gap_fill: true,
    smoothing: BTreeMap::new(),
}
```

Rationale:

- MP4 already has video-time-aligned telemetry.
- MP4 continuous streams were smoothed before culling.
- Finalizer should derive missing metrics but not insert synthetic idle samples
  or double-smooth.

### 10. Verify missing MP4 metric derivation

Add tests where MP4 columns omit direct:

- `speed`
- `heading`
- `vertical_speed`
- `gradient`

Assert finalizer derives them from course/elevation/distance/elapsed.

Also test that direct `speed`/`heading` are preserved when available and
fallback derivation fills only missing values.

### 11. Preserve discrete camera behavior

Add adapter-level tests for sparse camera samples:

- ISO at `t=0` and `t=10` holds across aligned samples until changed.
- Shutter/aperture/focal/EV/color temperature hold independently.
- Finalizer does not smooth or derive discrete camera fields.

### 12. Preserve pre-cull smoothing

Add an MP4 adapter-level test proving smoothing occurs before culling:

- create high-rate noisy source samples
- align to a lower-rate timeline
- assert aligned values reflect the smoothed series rather than raw nearest
  spikes

### 13. Remove or update old MP4 activity tests

Any tests asserting that `media/mp4_telemetry/activity.rs` directly builds
`ParsedActivity` must be rewritten around:

- adapter output columns
- finalizer output `ParsedActivity`

### 14. Golden/parity checks

- Existing FIT/GPX/SRT references should remain valid after the columnar
  refactor.
- MP4 before/after should preserve intended semantics:
  - camera fields
  - route/elevation
  - distance/progress
  - `sync_time`
  - derived speed/heading/gradient/vertical_speed

### 15. Documentation

Update `docs/ARCHITECTURE.md`:

- MP4 media is extraction/pre-treatment/alignment only.
- `activity::finalize` is the single finalization path.
- `sync_time` is canonical.
- `source_start_time` is deprecated/removed.

Update any scratch docs or ADRs still used for migration tracking.

## Risks / Watch Points

- `source_start_time` may have hidden consumers in frontend, render, or tests.
  Search and migrate all to `sync_time`.
- MP4 `sync_time` semantics must be preserved exactly because it anchors
  video/activity alignment.
- Holding GPS values forward may hide missing GPS gaps. Decide explicitly
  whether MP4 aligned GPS columns should carry last GPS or leave nulls when no
  nearby GPS exists.
- Finalizer idle gap fill should not run for MP4 unless we define what "idle
  gap" means for video-derived telemetry.
- `sample_elevations` should be populated from finalized `elevation`; MP4
  currently leaves it empty.
- Column lengths must be strictly validated to prevent subtle render
  interpolation bugs.
- If the frontend later emits columnar payloads, IPC/debug JSON size will drop
  substantially, but that should be a separate migration after MP4 wiring is
  stable.
