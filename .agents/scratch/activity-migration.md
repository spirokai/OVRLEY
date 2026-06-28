# Activity Parsing Migration Plan

Move the shared post-processing pipeline (idle gap fill, series building, metric
derivation, smoothing, final assembly) from the frontend to the Rust backend,
creating a single canonical finalization path for all four input formats. The
frontend becomes a thin extraction + upload layer.

Companion document: [activity-parsing.md](./activity-parsing.md) describes the
current frontend implementation in detail.

## 1. Target Architecture

```
                          EXTRACTION (format-specific, stays where it is)
                          ─────────────────────────────────────────────
 FIT  (frontend, JS)      GPX  (frontend, JS)        SRT  (frontend, JS)        MP4  (backend, Rust — NOT WIRED YET)
 fit-file-parser         DOMParser + extensions      text regex                  telemetry-parser + DJI AC004
   │                       │                          │                          │
   ▼                       ▼                          ▼                          ▼
 rawSamples[]             rawSamples[]               rawSamples[]               rawSamples[]  ← AFTER special
 (camelCase, ~27 fields)  (camelCase, ~26 fields)     (camelCase, ~12 fields)      pre-treatment:
                                                                    │                 zero-phase smoothing +
                                                                    │                 closest-in-time culling +
                                                                    │                 unit normalization
                                                                    ▼
                          SHARED CONTRACT (new): RawActivity
                          ─────────────────────────────────────────
                          { file_name, file_format, metadata, raw_samples[], options }
                                                                    │
                                                                    ▼
                          SHARED POST-PROCESSING (moves to backend Rust)
                          ──────────────────────────────────────────
1. Idle gap fill          (gap-utils → Rust)
                           2. Series building        (distance, elapsed, course, time, progress)
                           3. Metric series derivation (speed, heading, gradient, vertical_speed, pace, torque)
                           4. Optional per-metric smoothing  ← per-metric { enabled, method, window_seconds }
                           5. Final assembly → ParsedActivity
                                                                    │
                                                                    ▼
                          ParsedActivity (existing Rust schema.rs)
                          ─────────────────────────────────────────
                          feeds render / route / elevation geometry as today
```

### The seam
- **Extraction** produces a normalized `RawActivity` (raw samples + options) and
  nothing else. No derivation, no smoothing, no assembly.
- **Shared post-processing** owns everything from idle gap fill through to the
  final `ParsedActivity`. It lives in the backend and is format-agnostic.
- **MP4 pre-treatment** is a separate stage owned by the Rust extraction code
  (`media/mp4_telemetry/`), performed BEFORE the raw samples enter the shared
  path. It is out of scope for this migration — the MP4 path is not wired in
  yet and must not be touched in this work.

## 2. The RawActivity Contract

A new intermediate JSON shape sent from frontend to backend (and produced
internally by MP4 extraction in the future). It is the minimal input to the
shared pipeline.

```ts
{
  file_name: string,
  file_format: 'fit' | 'gpx' | 'srt' | 'mp4',
  metadata: { [key: string]: any },          // format-specific (sport, creator, camera_type, …)
  raw_samples: RawSample[],                  // normalized, see below
  options: {
    skip_idle_gap_fill: boolean,              // SRT: true, others: false
    // No top-level smoothing flag — opt-in is per metric via the `smoothing` map below.
    smoothing: {                              // per-metric; absent key = not smoothed
      [metric: string]: {
        enabled: boolean,
        method: 'zero_phase_ma' | 'circular_ema',
        window_seconds: number                // horizon; MA → window width. EMA ignores this (α hardcoded 0.05).
      }
    }
  }
}
```

The backend is a dumb executor: it applies exactly what each metric entry says
and picks the algorithm from `method`. No per-format constants in the backend.
Parsers mount the table appropriate to their format quality (SRT opts in, FIT/GPX
send `{}` today). Future formats can override without touching backend code.

If `smoothing` is omitted (or has no matching metric key), no smoothing is applied
to that metric. Absent flag = no smoothing, always.

### RawSample shape (snake_case — matches backend convention)
Renamed from the current camelCase frontend shape to match Rust field names and
the existing `NativeSample`. Parsers produce this directly so no conversion layer
is needed.

| Field | Type | FIT | GPX | SRT | MP4 (future) |
|-------|------|-----|-----|-----|---------------|
| `timestamp` | string\|null (ISO) | ✓ | ✓ | ✓ | ✓ |
| `elapsed_seconds` | number\|null | ✓ | — | ✓ | — |
| `latitude` | number\|null | ✓ | ✓ | ✓ | ✓ |
| `longitude` | number\|null | ✓ | ✓ | ✓ | ✓ |
| `elevation` | number\|null | ✓ | ✓ | ✓ | ✓ |
| `altitude` | number\|null | ✓ | ✓ | ✓ | ✓ |
| `speed` | number\|null | ✓ | ✓ | — | ✓ |
| `heading` | number\|null | ✓ | ✓ | — | ✓ |
| `heartrate` | number\|null | ✓ | ✓ | — | — |
| `cadence` | number\|null | ✓ | ✓ | — | — |
| `power` | number\|null | ✓ | ✓ | — | — |
| `temperature` | number\|null | ✓ | ✓ | — | — |
| `gradient` | number\|null | ✓ | ✓ | — | — |
| `pace` | number\|null | ✓ | ✓ | — | — |
| `distance` | number\|null | ✓ | ✓ | — | — |
| `g_force` | number\|null | ✓ | ✓ | — | ✓ |
| `vertical_speed` | number\|null | ✓ | ✓ | — | — |
| `torque` | number\|null | ✓ | ✓ | — | — |
| `stroke_rate` | number\|null | ✓ | ✓ | — | — |
| `stride_length` | number\|null | ✓ | ✓ | — | — |
| `vertical_oscillation` | number\|null | ✓ | ✓ | — | — |
| `ground_contact_time` | number\|null | ✓ | ✓ | — | — |
| `left_right_balance` | number\|null | ✓ | ✓ | — | — |
| `core_temperature` | number\|null | ✓ | ✓ | — | — |
| `air_pressure` | number\|null | ✓ | ✓ | — | — |
| `gear_position` | number\|null | ✓ | ✓ | — | — |
| `iso` | number\|null | — | — | ✓ | ✓ |
| `aperture` | number\|null | — | — | ✓ | ✓ |
| `shutter_speed` | number\|null | — | — | ✓ | ✓ |
| `focal_length` | number\|null | — | — | ✓ | ✓ |
| `ev` | number\|null | — | — | ✓ | ✓ |
| `color_temperature` | number\|null | — | — | ✓ | ✓ |

Missing fields are `null`; the shared pipeline handles nulls via the existing
`safe_number` / `Option<f64>` semantics.

## 3. What Moves to the Backend

Port these from `app/src/lib/activity/` to a new Rust module
`ovrley_core/src/activity/finalize.rs` (or `activity/process.rs`):

| JS source file | Rust target | Contents |
|----------------|-------------|----------|
| `parse-helpers.js` | `activity/math.rs` (reuse existing `telemetry_math.rs`)| `isFiniteNumber`, `roundValue`, `safeNumber`, `safeTimestamp`, `haversineDistanceMeters`, `calculateBearingDegrees` |
| `gap-utils.js` | `activity/finalize/gap.rs` | `insertIdleGapSamples`, `estimateRecordingIntervalSeconds`, `buildDistanceSeries`, `buildElapsedSeries`, `buildProgressSeries` |
| `metric-series.js` | `activity/finalize/metrics.rs` | `deriveActivityMetricSeries`, `deriveGradientSeries`, `deriveHeadingSeries`, `smoothHeadingSeriesCircularEma`, `deriveNumericRateSeries`, `deriveWindowedRateSeries`, `derivePaceSeries`, `deriveTorqueSeries`, `combineSeries`, `buildMetricCoverage` |
| `parser.js` | `activity/finalize.rs` | `finalizeParsedActivity` orchestrator, `CORE_ACTIVITY_ATTRIBUTES`, `EXTENDED_ACTIVITY_ATTRIBUTES`, `METRIC_UNITS` |
| (NEW) | `activity/finalize/smoothing.rs` | Zero-lag moving-average smoothing pass — see §5 |

Notes:
- `telemetry_math.rs` already has `haversine_distance` and `finite_f64`. Extend
  it with `bearing_degrees` and rounding helpers rather than duplicating.
- The Rust `ParsedActivity` schema (`schema.rs`) is unchanged — it already
  accepts every field the JS pipeline produces (extras via `flatten` BTreeMap).
- The legacy `frame_*` empty arrays and `trim_*` fields stay on `ParsedActivity`
  and are populated as today.

## 4. Per-Format Frontend Changes

### FIT (`fit-parser.js`)
- Keep the `fit-file-parser` library call (browser-side, no Rust equivalent).
- Change the output: instead of calling `finalizeParsedActivity()`, return a
  `RawActivity` with snake_case `raw_samples[]` and `options: { skip_idle_gap_fill: false, smoothing: { heading: { enabled: true, method: 'circular_ema', window_seconds: 0.5 } } }`
  (heading EMA only, mirroring today's behavior).
- Rename camelCase sample keys to snake_case at the map site.
- Drop `useLegacyGpxDerivations` — irrelevant; every format uses the standard
  5m-window gradient derivation.

### GPX (inline in `import-activity.js`)
- Keep `DOMParser`. Extract `parseGpxActivityFile` into its own
  `app/src/lib/activity/gpx-parser.js` for symmetry with the others.
- Return a `RawActivity` with snake_case `raw_samples[]` and
  `options: { skip_idle_gap_fill: false, smoothing: { heading: { enabled: true, method: 'circular_ema', window_seconds: 0.5 } } }`
  (heading EMA only; gradient uses the standard derivation per D-A).
- Rename `readTrackPointMetric` outputs to snake_case.
- No `use_legacy_gpx_derivations` flag — see D-A.

### SRT (`srt-parser.js`)
- Already pure text parsing. Return a `RawActivity` with snake_case
  `raw_samples[]` and a smoothing table — the only format that opts in today.
  Windowed-rate derivation (the old `use_windowed_rate` + `rate_window_seconds:1`)
  is replaced by the per-metric smoothing map: SRT's `speed` and `vertical_speed`
  are now derived with the standard per-sample differencing and then smoothed
  via `zero_phase_ma` with `window_seconds: 0.5` and `1.0` respectively.
- Rename `elapsedSeconds` → `elapsed_seconds`, `shutterSpeed` → `shutter_speed`,
  `focalLength` → `focal_length`, `colorTemperature` → `color_temperature`.
- SRT's default `options.smoothing`:
  ```json
  {
    "speed":          { "enabled": true, "method": "zero_phase_ma", "window_seconds": 0.5 },
    "vertical_speed": { "enabled": true, "method": "zero_phase_ma", "window_seconds": 1.0 },
    "elevation":      { "enabled": true, "method": "zero_phase_ma", "window_seconds": 1.0 },
    "heading":        { "enabled": true, "method": "circular_ema",  "window_seconds": 0.5 }
  }
  ```
  (Mirrors MP4's Rust `smoothing.rs` horizon constants.)

### `import-activity.js`
- Replace `parseActivityFile()` with a dispatcher that returns `RawActivity`.
- Replace the `finalizeParsedActivity()` call with a Tauri command call:
  `backend_finalize_activity(rawActivityJson)` → returns `ParsedActivity`.
- Remove `persistDebugPayload` from the frontend (move debug writing to the
  backend command, or keep as a separate optional call).
- `loadActivityIntoStore` receives the backend-assembled `ParsedActivity`
  unchanged — the store contract is identical.

## 5. Per-Metric Smoothing Pass

A new optional smoothing pass applied AFTER metric derivation, BEFORE final
assembly. Inspired by the MP4 Rust `smoothing.rs` (forward-backward centered
moving average, zero-phase).

### Why
FIT/GPX/SRT currently have no holistic smoothing pass; only per-derivation
smoothing exists (elevation moving average for gradient, circular EMA for
heading). High-frequency FIT or noisy GPX can produce jittery widgets. The
zero-lag pass gives map/course widgets stable values without time-shifting
events (no lag like a one-pole filter).

### Design
- Opt-in is per-metric via `options.smoothing[metric]`. No top-level flag.
- If `smoothing` is absent or the metric has no entry, no smoothing is applied.
- Two `method` values supported, both applying the existing smoothing primitives:
  - `zero_phase_ma`: linear forward-backward centered moving average. The
    `window_seconds` is the half-window horizon in seconds; the actual sample
    count is derived from observed cadence (median delta) via
    `smoothing_window_for_seconds`.
  - `circular_ema`: exponential moving average on sin/cos unit vectors to avoid
    the 0°/360° wrap, then `atan2` back to degrees. The smoothing factor α is
    **hardcoded at 0.05** for every format, matching the legacy JS
    (`smoothHeadingSeriesCircularEma` at `metric-series.js:262`). `window_seconds`
    is ignored for this method.
- Discrete fields (gear_position, iso, aperture, shutter_speed, focal_length,
  ev, color_temperature, left_right_balance) are never smoothed, even if a
  parser mistakenly lists them.
- Continuous candidates: `elevation`, `speed`, `vertical_speed`, `g_force`,
  `gradient`, `pace`. `heading` is also continuous but always uses
  `circular_ema` (never `zero_phase_ma` — the linear MA will produce wrap
  artifacts at 0/360).
- Gradient keeps its in-derivation radius-2 elevation moving average
  (inside `deriveGradientSeries`) for every format. This is independent of the
  per-metric smoothing pass; it is not configurable today and stays as-is.
- Heading circular EMA today runs for every format in the legacy code. After
  migration it runs only when the format opts in via `smoothing.heading` (SRT
  only today). FIT/GPX get unsmoothed derived heading — a visible behavior
  change for those formats.

### SRT default smoothing table
SRT is the only format that opts in today. Derived from MP4's Rust
`smoothing.rs` horizons (`smoothing.rs:19-22`). Drops the old windowed-rate
derivation (`use_windowed_rate` + `rate_window_seconds`) — SRT's `speed` and
`vertical_speed` are now derived with standard per-sample differencing and then
smoothed via this pass, matching FIT/GPX for derivation but adding the
smoothing SRT needs because its ~30 Hz cue cadence amplifies sensor noise.

### Implementation note
Reuse the Rust smoothing primitives from `media/mp4_telemetry/smoothing.rs`
(`moving_average`, `zero_phase_smooth`, `smoothing_window_for_seconds`) by
hoisting them into a shared `activity/finalize/smoothing.rs` module so MP4
pre-treatment and the shared pass import from one place. Add a `circular_ema`
helper in the same module (already exists in JS `metric-series.js`; port it).
Do **not** modify the MP4 call sites in this migration — only refactor the
function locations so both consumers import from the same place.

## 6. Migration Phases

### Phase 0 — No-behavior-change scaffolding (backend)
1. Create `ovrley_core/src/activity/finalize/` module tree.
2. Port `parse-helpers.js` → extend `activity/math.rs` (bearing + rounding).
3. Port `gap-utils.js` → `activity/finalize/gap.rs`.
4. Port `metric-series.js` → `activity/finalize/metrics.rs` (gradient uses ONLY
   the standard 5m-window derivation; drop the legacy path).
5. Port `parser.js` (`finalizeParsedActivity`) → `activity/finalize.rs`.
6. Add `RawActivity` struct to `activity/schema.rs` (mirrors §2, incl. the
   `options.smoothing` map).
7. Add new Tauri command `backend_finalize_activity(raw_activity_json)` in
   `commands/mod.rs` + `tauri_commands.rs` + `lib.rs`. It returns
   `{ parsed_activity, debug_payload }` but `debug_payload` is populated only
   in dev builds (`cfg!(debug_assertions)`); in release it is `null`.
8. Add a Rust unit test that feeds a known `RawActivity` (captured from a
   current-frontend FIT parse) and compares the output `ParsedActivity` field
   by field against a captured current-frontend `ParsedActivity`. The expected
   payload must be regenerated with the standard gradient derivation (no
   legacy), since the gradient change is intentional — the parity guard checks
   against the **post-migration** expected shape, not the pre-migration JS output.

### Phase 1 — Per-metric smoothing (backend)
1. Hoist `moving_average`, `zero_phase_smooth`,
   `smoothing_window_for_seconds` from `media/mp4_telemetry/smoothing.rs` into
   `activity/finalize/smoothing.rs`. Re-export from the original location so
   the MP4 path is untouched.
2. Add a `circular_ema` helper in the same module (port from JS
   `metric-series.js:smoothHeadingSeriesCircularEma`).
3. Wire the per-metric smoothing pass into `finalize_parsed_activity` after
   metric derivation, reading entries from `options.smoothing`. Reject discrete
   metrics (gear_position, iso, aperture, shutter_speed, focal_length, ev,
   color_temperature, left_right_balance) even if a parser lists them.
4. Tests: empty `smoothing` = no-op (matches FIT/GPX); SRT-style table
   actually smooths; circular EMA handles a 350°→10° turn without wrap glitch.

### Phase 2 — Frontend parser output shape change
1. Rename sample fields to snake_case in `fit-parser.js`, `gpx-parser.js`
   (extracted), `srt-parser.js`.
2. Replace each parser's `finalizeParsedActivity()` call with a `RawActivity`
   return, including each format's `options.smoothing` map:
   - FIT/GPX: `{ smoothing: { heading: { enabled: true, method: 'circular_ema', window_seconds: 0.5 } } }`
     (heading EMA only — matches current behavior).
   - SRT: the §5 default table.
3. Update `import-activity.js` to call `backend_finalize_activity` instead of
   the local pipeline.
4. Drop `use_windowed_rate` / `rate_window_seconds` / `use_legacy_gpx_derivations`
   use sites — replaced by the smoothing map and the unified gradient path.
5. Delete `gap-utils.js`, `metric-series.js`, `parser.js`, `parse-helpers.js`
   from the frontend (now backend-owned). Keep `import-activity.js` as the
   thin orchestrator.
6. Frontend parity test: import the same file before and after the change,
   diff the resulting `ParsedActivity`. Gradient values for GPX will differ
   (standard vs legacy derivation) — this is **expected**, see D-A.

### Phase 3 — Cleanup
1. Remove the now-dead frontend debug-payload write path (backend owns it now).
2. Update `activity-parsing.md` to reflect the new architecture.

## 7. Decisions (resolved)

### D-A: `use_legacy_gpx_derivations` — dropped
The standard 5m-window gradient derivation (`deriveGradientSeries`) is more
physically truthful than the legacy adjacent-sample + Savitzky-Golay path
(`deriveLegacyGradientSeries`). After the migration, **every format uses the
standard derivation** and the legacy function is not ported to Rust.

Behavior change for GPX users: gradient values will differ from pre-migration
output. This is intentional — the standard derivation is the correct one. No
flag to opt back in.

### D-B: Debug payload — backend-owned, dev-only
Backend returns `{ parsed_activity, debug_payload }`. `debug_payload` is
populated only in dev builds (`cfg!(debug_assertions)`); in release it is
`null`. The frontend persists the dev payload to disk via the existing
`backend.writeParseDebugFile` path, unchanged.

### D-C: Smoothing — per-metric opt-in, configurable
- No top-level smoothing flag. Opt-in is per metric via `options.smoothing`.
- If `smoothing` is omitted or has no entry for a metric, that metric is not
  smoothed.
- Each entry carries `{ enabled, method, window_seconds }`. Methods:
  `zero_phase_ma` (linear, window = half-width in seconds) or
  `circular_ema` (heading-only to avoid wrap, `window_seconds` = EMA time
  constant τ).
- Disabled for discrete metrics regardless of opt-in.
- Defaults after migration:
  - FIT: `{}` (no smoothing — trusted direct fields).
  - GPX: `{}` (no smoothing — consistent with today's holistic smoothing, which
    was none).
  - SRT: the §5 table (speed, vertical_speed, elevation via `zero_phase_ma`;
    heading via `circular_ema`). Replaces the old `use_windowed_rate` flag.
  - MP4 (future): pre-smoothed in `media/` pre-treatment; will send `{}`.

### Behavior changes to flag
1. **GPX `gradient`**: legacy adjacent-sample + Savitzky-Golay derivation
   removed; GPX now uses the standard 5m-window path like every other format.
   This is the only permitted JSON diff in the testing protocol.
2. **FIT/GPX `heading`**: circular EMA is preserved (every format opts in via
   `smoothing.heading`, `window_seconds: 0.5`). No diff expected.

## 7b. Testing Protocol
Golden-master comparison against pre-migration `ParsedActivity.json` fixtures.
For each fixture (FIT and GPX only — SRT not tested in this migration):

1. Re-run the same source file through the new backend pipeline.
2. Diff the new `ParsedActivity.json` against the stored old one.
3. Pass criteria:
   - **FIT**: every field must match bit-for-bit.
   - **GPX**: only `gradient` may differ. Any other field diff is a regression.

The protocol validates that porting `parse-helpers`, `gap-utils`, `metric-series`,
and `parser.js` to Rust preserves numerics for the unchanged parts of the
algorithm, and that the only intentional behavioral change (GPX gradient path)
is isolated to one field.

## 8. What Does NOT Change
- `ParsedActivity` Rust schema (`schema.rs`) — field set and serde contract
  stay identical.
- Render pipeline, `build_dense_activity_report_validated`, trim, densify,
  route/elevation geometry — all consume `ParsedActivity` as today.
- MP4 extraction path (`media/mp4_telemetry/`, `dji_ac004/`) — not wired, not
  touched in this migration. Its special pre-treatment (smoothing + culling +
  normalization) stays as-is and will later emit a `RawActivity` to feed the
  shared backend pipeline.
- `import-activity.js` remains the frontend import orchestrator (store sync,
  duration sync).
- `app/src/api/backend.js` — gains one new function (`finalizeActivity`), the
  rest unchanged.
## Appendix A — Smoothing primitive reuse
Current MP4 media/mp4_telemetry/smoothing.rs owns:
smooth_series, moving_average, zero_phase_smooth,
smoothing_window_for_seconds, and the four horizon constants
(GPS_SPEED_SMOOTHING_SECONDS=0.5, GPS_ALTITUDE=1.0, GPS_HEADING=0.5,
G_FORCE=1.0).

Migration plan for those symbols:

| Symbol | Destination | MP4 keeps using via |
|--------|-------------|---------------------|
| moving_average | ctivity/finalize/smoothing.rs | re-export |
| zero_phase_smooth | same | re-export |
| smoothing_window_for_seconds | same | re-export |
| smooth_series (MP4-specific entrypoint) | stays in media/mp4_telemetry/ | unchanged |
| Horizon constants | stay as MP4 caller-side defaults | unchanged; SRT defaults come from its parser table in JS |

The shared per-metric pass reads window_seconds from
options.smoothing[metric] and translates it to a sample count via the shared
smoothing_window_for_seconds. The MP4 path keeps using its hard-coded
constants — those SRT-style horizons live in the SRT parser, not the Rust
constants.