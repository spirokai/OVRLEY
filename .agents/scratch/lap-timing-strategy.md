# Lap Timing Extraction Strategy

## Goal

Fold lap timing into the existing native activity parsing pipeline so the canonical `ParsedActivity` output gains the minimum lap-timing data needed for live overlay:

- Current lap live timing (`lap_time_seconds`)
- Live delta of the current lap versus the current best lap (`delta_to_best_lap_seconds`)
- Session-best lap duration as a single scalar (`best_lap_time_seconds`)
- Compact per-lap metadata so both renderer and frontend can read best-lap-so-far and lap-log values without repeated per-frame scans (`lap_durations_seconds`, `lap_durations_best_so_far_seconds`)

The output remains the same JSON `ParsedActivity` shape the renderer already consumes; lap data is added as new aligned arrays plus compact metadata.

## Scope

Only the CSV and VBO fixtures under `src-tauri/ovrley_core/tests/fixtures/activity/`.

## Fixture inventory

| File                             | Format                        | Time column(s)                                                        | Lap boundary source                                                        | Notes                                                                                                    |
| -------------------------------- | ----------------------------- | --------------------------------------------------------------------- | -------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `VBO-test.vbo`                   | RaceChrono Pro VBO            | `time` (HHMMSS.ss)                                                    | `[laptiming]` start/split markers + interpolation of start/finish crossing | No per-row lap number. Time is GPS time-of-day.                                                          |
| `Amozoc - TrackAddict.csv`       | TrackAddict                   | `Time` (session elapsed, s), `UTC Time`                               | Explicit `Lap` column + `Start Point` comment                              | Also has `Predicted Lap Time` and `Predicted vs Best Lap`; we ignore those and compute our own best lap. |
| `sample AiM.csv`                 | AiM                           | `Time` (session elapsed, s)                                           | `Beacon Markers` header values are elapsed times of lap boundaries         | `Segment Times` header gives per-lap durations; ignored.                                                 |
| `sample LapLegend.csv`           | Lap Legend                    | `Time` (session elapsed, s)                                           | Explicit `Lap` column                                                      | Also has `Lap Number` and `Sector Number`; sector is ignored.                                            |
| `sample Racebox.csv`             | RaceBox                       | `Time` (session elapsed, s)                                           | Explicit `Lap` column                                                      | Single lap visible in sample.                                                                            |
| `sample RaceChrono.csv`          | RaceChrono Pro v9.1.3         | `Time (s)` (absolute epoch-ish), `Elapsed time (s)` (session elapsed) | Explicit `Lap #` column                                                    | Dynamic columns; `Trap name` marks splits but is ignored.                                                |
| `session_20260713_185859_v1.csv` | RaceChrono v10.2.3 (per-lap)  | `Timestamp (s)` (time-of-day in seconds)                              | Explicit `Lap #` column                                                    | `Lap #` is `N/A` before first lap. `Trap name` ignored.                                                  |
| `session_20260713_185859_v2.csv` | RaceChrono v10.2.3 (format 2) | `Time (s)` (absolute), `Elapsed time (s)` (session elapsed)           | Explicit `Lap #` column                                                    | `Lap #` is blank before first lap.                                                                       |

## Observations

1. **Elapsed time is already normalized by the existing pipeline.**
   - The CSV importer produces `sample_elapsed_seconds` from `Time`, `Elapsed time (s)`, `UTC Time`, RFC 3339 timestamps, or preamble-derived timestamps.
   - The VBO importer already rebases time-of-day to zero-based elapsed seconds.
   - Lap timing consumes the existing `sample_elapsed_seconds` series.

2. **Lap boundaries come from three distinct sources.**
   - **Per-row column:** `Lap`, `Lap #`, `Lap Number`.
   - **Header metadata:** AiM `Beacon Markers` (elapsed times of start/finish crossings).
   - **Geospatial markers:** VBO `[laptiming]` section defines start/finish points; lap boundaries are inferred from crossing those points.

3. **Best-lap data is sometimes already hinted.**
   - TrackAddict has `Predicted vs Best Lap`.
   - AiM has `Segment Times` (lap durations).
     We still compute best lap ourselves from completed laps so the overlay is consistent across formats.

## Integration with the existing pipeline

The existing backend path is:

```
CSV/VBO source
   -> format-specific extractor -> ActivityColumns
   -> finalize_activity_columns -> ParsedActivity
   -> trim_activity             -> TrimmedActivity
   -> densify_activity          -> DenseActivityReport
```

Lap timing is added at three seams:

1. **Extraction seam** (`ActivityColumns`)
   - New optional source column: `lap_number`.
   - New metadata carriers for implicit boundaries: `lap_beacon_markers`, `lap_timing_markers`.
2. **Finalization seam** (`finalize.rs`)
   - Derive lap numbers when the source did not provide them.
   - Compute `lap_time_seconds` and `delta_to_best_lap_seconds`.
   - Emit `best_lap_time_seconds` as a single scalar in metadata.
3. **Trim/densify seam** (`trim.rs`, `interpolate.rs`)
   - Carry the new per-sample series through trim and densify so frame-aligned overlays can read them.

## Schema additions

### `ActivityColumns` (internal canonical input)

```rust
pub struct ActivityColumns {
    // ... existing fields ...
    pub lap_number: LapNumberSeries,        // canonical source lap numbers (empty if absent)
    pub lap_markers: LapMarkers,            // metadata for implicit boundary detection
    // ...
}

pub type LapNumberSeries = Vec<Option<i64>>; // -1 out-lap, otherwise contiguous and 0-based

pub enum LapMarkers {
    None,
    BeaconMarkers(Vec<f64>),               // AiM: elapsed seconds of start/finish crossings
    TimingMarkers(Vec<TimingMarker>),      // VBO: start/finish lat/lon points
}

pub struct TimingMarker {
    pub kind: TimingMarkerKind,             // Start or Split; only Start closes and opens laps
    pub latitude_a: f64,                    // first endpoint of the timing line
    pub longitude_a: f64,
    pub latitude_b: f64,                    // second endpoint of the timing line
    pub longitude_b: f64,
}
```

### `ParsedActivity` (JSON output)

New aligned arrays (same length as `sample_elapsed_seconds`):

```json
{
  "lap_number": [-1, -1, 0, 0, 1, 1, ...],
  "lap_time_seconds": [null, null, 0.0, 0.04, 0.0, 0.04, ...],
  "delta_to_best_lap_seconds": [null, null, null, null, 0.0, -0.12, ...]
}
```

New metadata fields:

```json
{
  "best_lap_time_seconds": 93.456,
  "lap_durations_seconds": [94.12, 93.456, 95.002],
  "lap_durations_best_so_far_seconds": [94.12, 93.456, 93.456]
}
```

`lap_durations_seconds` is the duration of each completed lap (0-based lap index). `lap_durations_best_so_far_seconds` is the prefix-min of that array, giving the best completed lap duration up to and including each lap. Both arrays are scoped to the active trim window so that partial out-laps and in-laps are excluded. Consumers use these compact arrays for best-lap-so-far and lap-log lookups instead of scanning per-sample series.

### `TrimmedActivity` and `DenseSeriesReport`

Add matching `lap_number`, `lap_time_seconds`, and `delta_to_best_lap_seconds` fields so the renderer can request and receive lap-timing series per frame.

## Column semantics

| Column                      | Meaning                                                                                                                              | Nullability |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ----------- |
| `lap_number`                | Lap number, 0-based. Records before the first start/finish crossing are `-1` (out-lap).                                              | Never null. |
| `lap_time_seconds`          | Seconds since the start of the current `lap_number`. Null during out-lap (`lap_number == -1`).                                       | Nullable.   |
| `delta_to_best_lap_seconds` | `lap_time_seconds - reference_best_lap_time_at_this_distance`. Null until at least one lap is completed and a reference path exists. | Nullable.   |

| Metadata                            | Meaning                                                                                            | Nullability                   |
| ----------------------------------- | -------------------------------------------------------------------------------------------------- | ----------------------------- |
| `best_lap_time_seconds`             | Duration of the fastest completed lap in the whole session.                                        | Nullable if no lap completes. |
| `lap_durations_seconds`             | Array of completed-lap durations, indexed by 0-based lap number. Scoped to the active trim window. | Empty if no lap completes.    |
| `lap_durations_best_so_far_seconds` | Prefix-min of `lap_durations_seconds`; `best_so_far[n]` is the fastest lap among laps `0..=n`.     | Empty if no lap completes.    |

## Delta computation

The preferred reference is **distance**, which the finalizer already computes from source distance or from GPS course:

1. For each completed lap, build a function `lap_time_for_distance(d)`.
2. The _best lap_ is the completed lap with the minimum total duration.
3. At every record, `delta = lap_time_seconds - interpolate(best_lap_path, distance)`.

If the current distance exceeds the best lap's max distance, delta is `null`. Before any lap completes, delta is `null`.

The current lap is **not** included when selecting the reference lap.

## Source-specific extraction changes

### CSV importer (`src/activity/csv/`)

1. **Header aliases** (in `parser.rs`)
   - `lap`, `lap #`, `lap number` -> `Metric::LapNumber`
2. **Column assembly** (in `columns.rs`)
    - Select the lap column the same way other metrics are selected.
    - Parse lap numbers as finite floats; treat `N/A`, blank, `null` as `None`.
    - Normalize source lap labels once at extraction into contiguous 0-based integers; source values at or below zero represent the out-lap (`-1`).
3. **AiM special case**
   - The existing preamble parser already swallows `Beacon Markers` and `Segment Times` as transient metadata.
   - Capture the `Beacon Markers` value into `ActivityColumns.lap_markers` instead of discarding it.

### VBO importer (`src/activity/vbo/`)

1. **Section parsing** (in `mod.rs`)
    - Parse `[laptiming]` section rows as a marker kind followed by two longitude/latitude endpoint pairs defining a finite timing line.
   - Store markers in `ActivityColumns.lap_markers`.
2. **No change to timeline/distance**; the existing elapsed and distance series are reused.

## Finalization algorithm

A new `derive_lap_timing` step runs after the existing elapsed/distance series are finalized:

```
input: elapsed_seconds[], distance[], course[], lap_number_source[], lap_markers
output: lap_number[], lap_time_seconds[], delta_to_best_lap_seconds[]
        + metadata best_lap_time_seconds, lap_durations_seconds, lap_durations_best_so_far_seconds

1. Resolve lap_number[]:
    - If lap_number_source has any Some values, use it directly.
      Map absence before the first timed lap to `-1` and hold the current lap across later absent samples.
   - Else if BeaconMarkers present, bin each sample by elapsed time range.
   - Else if TimingMarkers present, detect start/finish crossings in sample order.
   - Else all lap_number = -1 and all derived lap fields are null.

2. Resolve lap_time_seconds[]:
    - Explicit source transitions use the transition sample's elapsed time.
    - AiM uses the exact beacon elapsed time; VBO interpolates the crossing time within the intersecting route segment.
    - lap_time_seconds[i] = elapsed_seconds[i] - lap_start_elapsed of current lap.
    - For lap_number == -1, value is null.

3. Resolve completed lap durations:
    - For each pair of consecutive boundaries, duration = next_boundary_elapsed - current_boundary_elapsed.
    - The final observed lap remains incomplete because no subsequent boundary closes it.

4. Metadata lap_durations_seconds and lap_durations_best_so_far_seconds:
   - `lap_durations_seconds[n]` = duration of lap n.
   - `lap_durations_best_so_far_seconds[n]` = min(`lap_durations_seconds[0..=n]`).

5. Metadata best_lap_time_seconds:
   - Minimum completed-lap duration across the whole session (= last element of `lap_durations_best_so_far_seconds`, or None if empty).

6. Compute delta_to_best_lap_seconds[]:
   - For the best completed lap so far, build (distance, lap_time_seconds) points.
   - For each sample with lap_number >= 0, if distance is Some and within best-lap distance range,
     delta = current lap_time_seconds - interpolated best-lap lap_time_seconds at that distance.
   - The reference lap is updated as new laps complete.
```

## Trim/densify propagation

- `trim.rs`: add `lap_number`, `lap_time_seconds`, and `delta_to_best_lap_seconds` to `TrimmedActivity`. Trim them alongside other numeric series. `lap_number` uses hold interpolation and aligned series retain session lap IDs. Scope the compact metadata arrays to the active trim window by omitting laps whose opening or closing boundary lies outside the trim.
- `interpolate.rs`: add the three series to `DenseSeriesReport` and wire them through `densify_activity` using the existing `RenderDataRequirements` gating pattern. Carry the scoped per-lap metadata through so the renderer receives it.
- `normalize/mod.rs`: add boolean flags to `RenderDataRequirements` for the new lap series.

> **TODO:** Replace the temporary `MissingSamplePolicy::Preserve` handling with a lap-aware interpolation helper. It must interpolate only when adjacent samples belong to the same lap, never interpolate across a lap-time reset, and retain `None` whenever either delta endpoint is unavailable. `Preserve` is only an interim improvement because it treats missing numeric endpoints as zero.

## Suggested implementation order

1. **Schema plumbing**
   - Add fields to `RawSample`, `ActivityColumns`, `ParsedActivity`, `TrimmedActivity`, `DenseSeriesReport`, and `RenderDataRequirements`.
   - Add compact per-lap metadata to `ParsedActivity` and `DenseSeriesReport`.
2. **CSV header alias**
   - Teach `csv/parser.rs` to recognize `lap`/`lap #`/`lap number`.
   - Populate `ActivityColumns.lap_number` in `csv/columns.rs`.
3. **Finalizer derivation**
   - Implement `derive_lap_timing` in `activity/finalize.rs`.
   - Produce the new `ParsedActivity` fields and metadata.
4. **VBO `[laptiming]` parsing**
   - Parse the section in `activity/vbo/mod.rs` and populate `lap_markers`.
   - Reuse the finalizer's crossing detection.
5. **AiM beacon capture**
   - Preserve `Beacon Markers` into `lap_markers` instead of discarding it.
6. **Trim/densify wiring**
   - Carry the new series through `trim.rs` and `interpolate.rs`.
7. **Tests**
   - Add unit tests for `derive_lap_timing` with synthetic data.
   - Add integration tests in `csv_activity.rs` and `vbo_activity.rs` asserting correct `lap_number`, `lap_time_seconds`, `delta_to_best_lap_seconds`, `lap_durations_seconds`, and `lap_durations_best_so_far_seconds` for each fixture.

## Open questions

1. Should partial first/last laps (out-lap, in-lap) be eligible for "session best"? _(Resolved: no. The per-lap metadata arrays are scoped to the active trim window; only laps that fully complete within the trim are included.)_
2. If a source already provides a predicted-vs-best value (TrackAddict), should the overlay ever prefer it over our own computed delta? _(Resolved: yes)_
3. How should the VBO crossing detector treat markers with the same lat/lon duplicated on consecutive `[laptiming]` rows? _(Unclear question; elaborate with example prior to implementation.)_

## Testing

- After implementing the finalizer, create a script that will parse all csv/vbo fixtures in tests/fixtures/activity, run it and assess if all required metrics are extracted properly.
