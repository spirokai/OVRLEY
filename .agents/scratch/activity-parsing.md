# Activity Parsing Architecture — Frontend

## Entry & Routing
**File:** `import-activity.js` — `importActivityFile(file, storeActions)`

Routes by extension via `parseActivityFile()`:
- `.fit` → `fit-parser.js` (external `fit-file-parser` library)
- `.gpx` → inline `parseGpxActivityFile()` (DOMParser-based XML)
- `.srt` → `srt-parser.js` (custom DJI subtitle parser)

Each format parser returns `{ parsedActivity, debugPayload }`.

---

## Three Format Parsers

### 1. FIT (`fit-parser.js`)
- Uses `fit-file-parser` npm library to decode binary FIT
- Maps ~25 record fields to a normalized raw sample object (lat, lon, altitude, speed, cadence, power, heartrate, heading, etc.) via `safeNumber()` guards and `getOptionalRecordValue()` key aliases
- Extracts session-level metadata (sport, sub_sport, total times)
- Calls `finalizeParsedActivity()` **without special options** → default gap filling, per-sample rate derivation, standard gradient

### 2. GPX (inline in `import-activity.js:108-182`)
- `DOMParser` → `parseFromString(text, 'application/xml')`
- Iterates `<trkpt>` elements: `lat`/`lon` attributes, `<ele>`, `<time>`, and `<extensions>` children (parses arbitrary leaf XML into key-value map with normalized keys)
- Reads metric aliases via `readTrackPointMetric()` with extensive fallback key lists (e.g. `['hr', 'heartrate', 'heart_rate']`)
- Calls `finalizeParsedActivity()` **with `{ useLegacyGpxDerivations: true }`** → adjacent-sample gradient (not 5m-window), per-sample rate

### 3. SRT (`srt-parser.js`)
- DJI subtitle format. Splits on blank lines → extracts cue index, timing (`HH:MM:SS,mmm --> ...`), and body lines

**Format A** (bracketed `[key: value]` telemetry, detected by regex):
- Extracts timestamp line (`2025-07-23 10:21:41.694`)
- `parseFormatABracketedFields()` — regex-based kv extraction within brackets
- Fields: `latitude`, `longitude`, `abs_alt`, `iso`, `fnum`, `shutter`, `focal_len`, `ev`, `ct` (color temp)

**Format B** (legacy line-oriented, `HOME(...)`, `GPS(...)`, `ISO:`, `BAROMETER:` lines):
- `HOME()` → timestamp
- `GPS()` → lat/lon/alt
- `ISO: Shutter: EV: Fnum:` → camera data
- `BAROMETER:` → barometric altitude fallback

- Calls `finalizeParsedActivity()` **with `{ skipIdleGapFill: true, useWindowedRate: true, rateWindowSeconds: 1 }`** → no zero-fill, **windowed** rate derivation (looks back 1s instead of adjacent sample, needed because GPS updates at ~6-10 Hz but SRT cue rate is ~30 Hz)

---

## Post-Processing Pipeline (`parser.js`: `finalizeParsedActivity()`)

### Step 1 — Idle Gap Filling (`gap-utils.js:insertIdleGapSamples`)
- Estimates **recording interval** via lower-half median of consecutive elapsed/timestamp deltas (capped at 10s)
- **Gap threshold** = max(3s, interval × 3)
- **Stationary threshold** = max(5m, interval × 2.5)
- If elapsed delta > threshold AND distance delta ≤ stationary threshold → inserts zero-filled synthetic samples:
  - `speed=0, cadence=0, power=0, strokeRate=0, verticalSpeed=0, gForce=0, gradient=0`, syntheticIdle flag
  - Timestamps interpolated proportionally
- Returns `{ rawSamples, gapDebug }` with gap metadata

### Step 2 — Series Building
| Series | Function | Source |
|--------|----------|--------|
| `courseSeries` | `buildCourseSeries()` | `[lat, lon]` pairs from samples |
| `timeSeries` | `buildTimeSeries()` | ISO timestamps |
| `distanceSeries` | `buildDistanceSeries()` | Direct `distance` field, falls back to haversine cumulative |
| `elapsedSeries` | `buildElapsedSeries()` | Explicit `elapsedSeconds`, falls back to timestamp delta from origin |
| `progressSeries` | `buildProgressSeries()` | `distance[i] / totalDistance` |
| `elevationBaseSeries` | direct map | `sample.elevation` |

### Step 3 — Metric Series Derivation (`metric-series.js:deriveActivityMetricSeries`)

**Direct metrics** (extracted from raw samples, 23 fields): `air_pressure`, `altitude`, `cadence`, `core_temperature`, `distance`, `elevation`, `g_force`, `gear_position`, `gradient`, `ground_contact_time`, `heading`, `heartrate`, `left_right_balance`, `pace`, `power`, `speed`, `stride_length`, `stroke_rate`, `temperature`, `torque`, `vertical_oscillation`, `vertical_speed`, and camera fields (`iso`, `aperture`, `shutter_speed`, `focal_length`, `ev`, `color_temperature`)

**Derived metrics** (computed from direct/other derived):

| Metric | Derivation | Notes |
|--------|-----------|-------|
| **speed** | `deriveNumericRateSeries(distance, elapsed)` or `deriveWindowedRateSeries(distance, elapsed, windowSec=1)` | Per-sample diff (default) vs 1s lookback window (SRT) |
| **heading** | `deriveHeadingSeries()` → `smoothHeadingSeriesCircularEma()` | Min-distance lookback baseline (2m half, 4m full), then circular EMA (`α=0.05`) on sin/cos unit vectors to avoid 0/360 wrap |
| **gradient** | `deriveGradientSeries()` or `deriveLegacyGradientSeries()` | Standard: 5m sliding window on smoothed elevation (Savitzky-Golay radius 2). Legacy (GPX): adjacent-sample diff with SG [-2,3,6,7,6,3,-2] filtering |
| **vertical_speed** | `deriveNumericRateSeries(elevation, elapsed)` or `deriveWindowedRateSeries(...)` | Same per-sample vs windowed split as speed |
| **pace** | `derivePaceSeries(speed)` | `1000 / speed` (seconds per km) |
| **torque** | `deriveTorqueSeries(power, cadence)` | `power / (2π × cadence / 60)` |

**Combination logic** (`combineSeries`): For each metric, the final series is computed as:
- Direct field (from file) preferred, derived as fallback (for speed, heading, gradient, pace, torque, vertical_speed)
- Source tracked per metric: `'direct'`, `'derived'`, `'mixed'`, or `'missing'`

### Step 4 — Final Assembly
- `buildValidAttributes()` — presence check for CORE_ACTIVITY_ATTRIBUTES: `[cadence, course, elevation, gradient, heartrate, power, speed, time, temperature]`
- `buildExtendedAttributes()` — same for EXTENDED list
- `buildMetricCoverage()` — count of non-null values per metric
- Flat `parsedActivity` object with all series arrays, metadata, units, coverage, gaps

---

## Pure Helpers (`parse-helpers.js`)
All side-effect-free functions suitable for backend move:
- `isFiniteNumber()`, `roundValue()`, `safeNumber()`, `safeTimestamp()`
- `haversineDistanceMeters()` — spherical distance
- `calculateBearingDegrees()` — bearing from `[lat, lon]` pair

---

## Separation Seam Candidates

| Layer | Files | Browser deps? | Move-ready? |
|-------|-------|--------------|-------------|
| **Pure helpers** | `parse-helpers.js` | No | ✅ Move verbatim |
| **Gap utils** | `gap-utils.js` | No (uses `Date` but pure logic) | ✅ Move verbatim |
| **Metric derivation** | `metric-series.js` | No | ✅ Move verbatim |
| **Parser orchestrator** | `parser.js` | No (uses `Date()` only for debug timestamp) | ✅ Move (strip `debugPayload` or keep) |
| **FIT parser** | `fit-parser.js` | Yes (`File.arrayBuffer()`) | Need Node-compatible FIT library or adapt input |
| **GPX parser** | inline in `import-activity.js` | Yes (`DOMParser`) | Need XML parser (e.g. `fast-xml-parser`) |
| **SRT parser** | `srt-parser.js` | No (pure text parsing) | ✅ Move verbatim |
| **Import orchestration** | `import-activity.js` | Yes (`File`, store actions) | Stay on frontend; thin wrapper |

### Recommended Seam
Move `parse-helpers.js`, `gap-utils.js`, `metric-series.js`, `parser.js`, and `srt-parser.js` to backend as-is.
For FIT and GPX, write backend adapters that produce the same normalized `rawSamples[]` shape consumed by `finalizeParsedActivity()`.
`import-activity.js` stays on frontend as a thin upload → call backend → load store glue.
