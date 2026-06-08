Status: ready-for-agent

# SRT Activity Import + Camera Metrics + Hold Interpolation

## Problem Statement

OVRLEY currently supports FIT and GPX activity import on the frontend, normalizes those sources into a canonical `parsedActivity` payload, and sends that payload to the Rust backend for trimming, densification, preview rendering, and video rendering.

We now need to support DJI-style SRT telemetry import and extend the standard metric system so camera telemetry can be treated as first-class value widgets.

This work has four tightly-coupled parts:

1. Add an SRT parser on the frontend that converts subtitle cues into the same finalized parsed activity format used by FIT/GPX.
2. Add new metric widget types for camera telemetry plus `altitude`, wired end-to-end through frontend preview and Rust rendering.
3. Add manifest-driven interpolation policy so some metrics interpolate linearly while others hold the last known value.
4. Add manifest-driven unit policy so unitless metrics never expose unit controls and single-unit metrics still preview/render correctly even when no unit dropdown exists.

The key architectural constraint is that the current system is time-based, not sample-index-based. SRT telemetry must preserve real cue timestamps, even when they are dense, irregular, or slightly offset from integer seconds. The implementation must not assume exact 30 fps.

## Goals

- Parse `.srt` activity telemetry into the same canonical `parsedActivity` shape as FIT/GPX.
- Preserve real elapsed times from SRT cue start times, including millisecond offsets like `2.001` and `3.002`.
- Add first-class standard metric widget support for:
  - `altitude`
  - `iso`
  - `aperture`
  - `shutter_speed`
  - `focal_length`
  - `ev`
  - `color_temperature`
- Support derived GPS metrics from SRT telemetry by mapping absolute altitude into the same elevation/derivation path used by existing sources.
- Introduce a manifest key that controls interpolation mode per metric.
- Ensure both frontend preview and Rust backend densification/rendering honor the same interpolation mode.
- Ensure unitless metrics expose no units toggle, no units selection UI, and no unit text.
- Ensure single-unit selectable metrics still work correctly when the frontend never exposes a dropdown.
- Ensure altitude behaves as a normal standard metric widget, separate from the existing elevation plot widget.

## Non-Goals

- Do not redesign the activity pipeline around a separate frame-series contract.
- Do not add a generic arbitrary-metric system in this slice.
- Do not pre-resample SRT telemetry onto a synthetic exact 30 fps grid at parse time.
- Do not add a generic “ingest all unknown SRT fields” path.
- Do not remove the backend requirement that standard metric widgets send explicit formatting fields unless a local refactor makes that unavoidable.
- Do not solve multi-file SRT merge/synchronization strategy in this slice; this plan assumes one imported SRT telemetry source at a time.

## Locked Product Decisions

These are confirmed requirements.

1. `altitude` supports user-selectable units via normal `supportedDisplayUnits`.
2. `aperture` is displayed as `F/1.7`.
3. `focal_length` supports units, but only one unit (`mm`); the user may toggle units on/off, but there should be no dropdown because there is only one supported unit.
4. `ev` and `ct` are included in this slice.
5. `ct` means color temperature and is always represented in Kelvin.
6. `shutter_speed` is always displayed in reciprocal style.
7. `unitsMode` only needs `selectable` and `hidden`.
8. If a metric has exactly one supported display unit and `unitsMode: selectable`, preview/rendering must still work even though the frontend does not expose a unit dropdown.
9. `heading` should use linear interpolation, not hold interpolation.

## Core Decisions

### 1. Treat SRT as a native sample timeline, not a fake frame grid

Each subtitle cue becomes one raw sample. The cue start time is the authoritative `elapsedSeconds` value. The embedded wall-clock timestamp line is the authoritative `timestamp` value.

Do not derive elapsed time from frame count. Do not derive elapsed time by assuming fixed `1/30` cadence. The SRT sample timeline must preserve the actual source timing, including cadence drift and non-integer seconds.

### 2. Keep SRT in `sample_*` series, not `frame_*`

The frontend already interpolates from `sample_elapsed_seconds` during preview. The backend already trims and densifies from sample series during render.

For this slice:

- populate `sample_elapsed_seconds`
- leave `frame_elapsed_seconds`, `frame_timestamps`, and `frame_distance_progress` unused on the frontend side
- let Rust densification continue to build dense per-frame render data from trimmed sample series

### 3. Make interpolation policy manifest-driven

Interpolation type must become part of the shared standard metric manifest so both sides can read the same policy:

- `linear`
- `hold`

Intended policy:

- `altitude`: `linear`
- `speed`: existing linear behavior
- `vertical_speed`: existing linear behavior
- `heading`: `linear`
- `iso`: `hold`
- `aperture`: `hold`
- `shutter_speed`: `hold`
- `focal_length`: `hold`
- `ev`: `hold`
- `color_temperature`: `hold`

### 4. Units policy is minimal and manifest-driven

The manifest only needs:

- `selectable`
- `hidden`

Semantics:

- `selectable`: the metric participates in the normal unit pipeline. If there is more than one supported unit, the frontend may show a selector. If there is exactly one supported unit, the frontend should not expose a dropdown, but preview/rendering must still work with the explicit/default unit.
- `hidden`: no units toggle, no units selector, no unit text rendered.

## Representative SRT Formats

The parser must support both DJI SRT formats represented by:

- [DJI-sample1.SRT](/e:/Github/cyclemetry-reloaded/docs/DJI-sample1.SRT)
- [DJI-sample2.SRT](/e:/Github/cyclemetry-reloaded/docs/DJI-sample2.SRT)

### Format A: dense bracketed telemetry format

This is the newer dense format shown in `DJI-sample1.SRT`:

1. Subtitle index
2. Cue timing, e.g. `00:00:02,001 --> 00:00:02,035`
3. Body lines, including:
   - `FrameCnt: 61, DiffTime: 34ms`
   - `2025-07-23 10:21:41.694`
   - bracketed key-value fields such as:
     - `[iso: 200]`
     - `[shutter: 1/3200.0]`
     - `[fnum: 1.7]`
     - `[focal_len: 24.00]`
     - `[latitude: 51.118062]`
     - `[longitude: 88.083302]`
     - `[rel_alt: 20.000 abs_alt: 864.309]`
     - `[ev: 0]`
     - `[ct: 5491]`

### Format B: legacy line-oriented telemetry format

This is the older line-oriented format shown in `DJI-sample2.SRT`:

1. Subtitle index
2. Cue timing, e.g. `00:00:09,000 --> 00:00:10,000`
3. Body lines, including:
   - `HOME(149.0251,-20.2532) 2017.08.05 14:12:00`
   - `GPS(149.0251,-20.2532,27) BAROMETER:13.0`
   - `ISO:100 Shutter:60 EV: Fnum:2.2`

Parsing expectations for Format B:

- parse the timestamp from the `HOME(...) YYYY.MM.DD HH:MM:SS` line
- parse latitude, longitude, and one altitude-like value from the `GPS(...)` tuple
- parse `BAROMETER` separately when present
- parse camera fields from the compact `ISO:... Shutter:... EV:... Fnum:...` line
- tolerate missing values such as blank `EV:`

The parser must tolerate:

- HTML-like wrappers such as `<font size="28">...</font>`
- bracket groups with one or more key/value pairs
- line-oriented key/value text without brackets
- variable cue cadence
- missing fields in some cues

## Output Contract

The SRT parser must feed `finalizeParsedActivity()` and produce a canonical payload shaped like FIT/GPX outputs.

New top-level canonical series to add:

- `altitude`
- `iso`
- `aperture`
- `shutter_speed`
- `focal_length`
- `ev`
- `color_temperature`

Those series must be included consistently in:

- frontend parsed activity object
- backend `ParsedActivity`
- backend `TrimmedActivity`
- backend `DenseSeriesReport`
- metric/widget requirement plumbing

Important pre-existing gap:

- the frontend already carries `altitude` in finalized parsed activity and already lists it in frontend extended metadata
- Rust `ParsedActivity` currently does not deserialize `altitude` into a real field
- today it falls into the backend `extra` catch-all and is never trimmed, densified, or rendered
- this plan must explicitly close that Rust-side gap or altitude widgets will not work

## New Standard Metrics

Add these metric definitions to the shared manifest:

- `altitude`
- `iso`
- `aperture`
- `shutter_speed`
- `focal_length`
- `ev`
- `color_temperature`

### Required manifest metadata

Each standard metric definition must support these keys:

- `type`
- `current`
- `label`
- `defaultDisplayUnit`
- `supportedDisplayUnits`
- `showUnitsByDefault`
- `formatter`
- `icon`
- `interpolation`
- `unitsMode`

For each new metric definition, the implementing agent must explicitly fill in:

- `label`
- `formatter`
- `icon`
- `defaultDisplayUnit`
- `supportedDisplayUnits`
- `showUnitsByDefault`
- `interpolation`
- `unitsMode`

### New manifest keys

#### `interpolation`

String enum:

- `linear`
- `hold`

#### `unitsMode`

String enum:

- `selectable`
- `hidden`

### Metric policies

- `altitude`
  - `label`: explicit human-readable label required
  - `formatter`: explicit formatter required
  - `icon`: explicit icon asset required
  - `defaultDisplayUnit`: explicit value required
  - `supportedDisplayUnits`: explicit array required
  - `showUnitsByDefault`: explicit boolean required
  - `interpolation`: `linear`
  - `unitsMode`: `selectable`
- `iso`
  - `label`: explicit human-readable label required
  - `formatter`: explicit formatter required
  - `icon`: explicit icon asset required
  - `defaultDisplayUnit`: explicit value required
  - `supportedDisplayUnits`: explicit array required
  - `showUnitsByDefault`: explicit boolean required
  - `interpolation`: `hold`
  - `unitsMode`: `hidden`
- `aperture`
  - `label`: explicit human-readable label required
  - `formatter`: explicit formatter required
  - `icon`: explicit icon asset required
  - `defaultDisplayUnit`: explicit value required
  - `supportedDisplayUnits`: explicit array required
  - `showUnitsByDefault`: explicit boolean required
  - `interpolation`: `hold`
  - `unitsMode`: `hidden`
- `shutter_speed`
  - `label`: explicit human-readable label required
  - `formatter`: explicit formatter required
  - `icon`: explicit icon asset required
  - `defaultDisplayUnit`: explicit value required
  - `supportedDisplayUnits`: explicit array required
  - `showUnitsByDefault`: explicit boolean required
  - `interpolation`: `hold`
  - `unitsMode`: `hidden`
- `focal_length`
  - `label`: explicit human-readable label required
  - `formatter`: explicit formatter required
  - `icon`: explicit icon asset required
  - `defaultDisplayUnit`: explicit value required
  - `supportedDisplayUnits`: explicit array required
  - `showUnitsByDefault`: explicit boolean required
  - `interpolation`: `hold`
  - `unitsMode`: `selectable`
- `ev`
  - `label`: explicit human-readable label required
  - `formatter`: explicit formatter required
  - `icon`: explicit icon asset required
  - `defaultDisplayUnit`: explicit value required
  - `supportedDisplayUnits`: explicit array required
  - `showUnitsByDefault`: explicit boolean required
  - `interpolation`: `hold`
  - `unitsMode`: `hidden`
- `color_temperature`
  - `label`: explicit human-readable label required
  - `formatter`: explicit formatter required
  - `icon`: explicit icon asset required
  - `defaultDisplayUnit`: explicit value required
  - `supportedDisplayUnits`: explicit array required
  - `showUnitsByDefault`: explicit boolean required
  - `interpolation`: `hold`
  - `unitsMode`: `selectable`

## Parsing Design

### Frontend source parser

Create a new parser module:

- `app/src/lib/activity/srt-parser.js`

Responsibilities:

1. Parse subtitle cues from raw file text.
2. Extract cue start time as elapsed seconds.
3. Extract the absolute timestamp line.
4. Extract bracketed telemetry fields.
5. Normalize each cue into one raw sample object.
6. Pass those raw samples into `finalizeParsedActivity()`.

### Dispatch integration

Update:

- `app/src/lib/activity/import-activity.js`

Current behavior routes `.fit` to FIT parser and everything else to GPX. Replace this with explicit source detection:

- `.fit` -> FIT parser
- `.gpx` -> GPX parser
- `.srt` -> SRT parser
- anything else -> clear error

### SRT raw sample mapping

Each cue should map into a raw sample using the same field naming convention as existing parsers:

- `elapsedSeconds`: cue start time in seconds
- `timestamp`: absolute timestamp line converted to ISO 8601 if possible
- `latitude`
- `longitude`
- `altitude`: from `abs_alt`
- `elevation`: from `abs_alt`
- `iso`
- `aperture`: from `fnum`
- `shutterSpeed`: numeric seconds parsed from shutter string
- `focalLength`: from `focal_len`
- `ev`
- `colorTemperature`: from `ct`

Be explicit about the shutter naming chain:

- SRT field: `shutter`
- raw sample field: `shutterSpeed`
- finalized canonical series: `shutter_speed`

Optional metadata worth preserving at file-level or debug payload level:

- frame count
- diff time
- color mode
- relative altitude

### Timestamp handling

SRT absolute timestamps are not RFC 3339 in the sample. Convert them into ISO format before passing them downstream if possible.

The important invariant is that elapsed timing stays correct. If timezone treatment is ambiguous, preserve elapsed seconds as the source of truth and treat absolute timestamps as best-effort metadata.

### Shutter parsing

Store shutter speed numerically in seconds.

Examples:

- `1/3200.0` -> `0.0003125`
- `0.5` -> `0.5`

Keep formatting concerns separate from storage. The stored series must be numeric.

For this slice, the parser must explicitly support:

- reciprocal forms like `1/3200.0` and `1/50`
- decimal-second forms like `0.5`

Unsupported or ambiguous shutter forms must not fail the import. Parse them as `null`, keep the activity valid, and preserve enough debug context for diagnosis.

### Gap insertion policy

SRT is dense telemetry. Existing idle-gap logic was designed around sparse GPS activity logs.

Recommended implementation:

- add `options.skipIdleGapFill`
- make SRT call `finalizeParsedActivity({ ..., options: { skipIdleGapFill: true } })`

Note: `finalizeParsedActivity()` does not currently accept an options object. Introducing `skipIdleGapFill` requires a small signature change and corresponding updates at the FIT and GPX call sites.

## Frontend Canonical Activity Changes

Update:

- `app/src/lib/activity/parser.js`
- `app/src/lib/activity/metric-series.js`
- helper files as needed

### Add canonical output series

Extend the canonical parsed activity object with:

- `altitude`
- `iso`
- `aperture`
- `shutter_speed`
- `focal_length`
- `ev`
- `color_temperature`

### Extend metric metadata

Classification note:

- `altitude` is already present in the frontend parsed-activity metadata (`EXTENDED_ACTIVITY_ATTRIBUTES` and `METRIC_UNITS`).
- For this slice, all newly added SRT/camera metrics belong in `EXTENDED_ACTIVITY_ATTRIBUTES`.
- None of the new camera metrics should be added to `CORE_ACTIVITY_ATTRIBUTES`.

Update:

- `EXTENDED_ACTIVITY_ATTRIBUTES`
- `METRIC_UNITS`
- coverage calculation
- serialized output

Suggested internal/source units:

- `altitude: 'm'`
- `iso: 'iso'`
- `aperture: 'fnum'`
- `shutter_speed: 'seconds'`
- `focal_length: 'mm'`
- `ev: 'ev'`
- `color_temperature: 'kelvin'`

### Metric series derivation

In `metric-series.js`, add direct metric extraction for the new raw sample fields.

No derived fallback is needed for camera metrics.

Altitude should continue to participate in existing derivations indirectly by also feeding `elevation` for SRT.

### SRT derivation behavior

The current derivation seam already supports:

- derived `speed` from distance / elapsed
- derived `heading` from course
- derived `vertical_speed` from elevation / elapsed

That means SRT only needs to provide:

- course coordinates
- absolute altitude mapped to `elevation`
- real elapsed time

to benefit from existing derived GPS metrics.

If some SRT cues are missing GPS or altitude fields, that is acceptable for this slice. Existing null-handling behavior for partially missing source data is sufficient; derived metrics may be locally missing for those cues and no synthetic GPS backfill is required.

## Frontend Preview Interpolation + Formatting

Update:

- `app/src/lib/standard-metrics.js`
- `app/src/features/overlay-editor/utils/overlayEditorUtils.js`
- `app/src/features/widget-preview/utils/formatUtils.js`
- `app/src/features/overlay-editor/data/overlayEditorConfig.js`

### Interpolation behavior

Current state: `getInterpolatedActivityValue()` always performs numeric interpolation.

Current frontend gap:

- the frontend does not currently have a real hold-interpolation path for any metric
- the backend currently has a special-case hold-like path only in densification
- this slice must add frontend hold interpolation for hold metrics explicitly; do not assume heading already behaves that way in preview

Required change:

- `linear` -> current interpolation logic
- `hold` -> return last known value at or before `elapsedSecond`

This should be metric-aware, not widget-aware.

### Formatting behavior

Required display expectations:

- `altitude`
  - numeric display like existing scalar metrics
- `iso`
  - integer, no unit text
- `aperture`
  - display as `F/x.x`
- `shutter_speed`
  - always render numeric seconds as reciprocal shutter text, e.g. `1/3200`
- `focal_length`
  - numeric display, optional `mm` suffix when `show_units` is on
- `ev`
  - numeric display, no unit text, with explicit sign support for positive and negative values
- `color_temperature`
  - numeric display, optional `K` suffix when `show_units` is on

### Default preview fallback data

Extend `DEFAULT_ACTIVITY_PREVIEW` so the new metrics have safe fallback values when no parsed activity is loaded.

## Backend Schema + Densification Changes

Update:

- `src-tauri/ovrley_core/src/types.rs`
- `src-tauri/ovrley_core/src/standard_metrics.rs`
- `src-tauri/ovrley_core/src/activity/schema.rs`
- `src-tauri/ovrley_core/src/activity/interpolate.rs`
- `src-tauri/ovrley_core/src/normalize/mod.rs`
- `src-tauri/ovrley_core/src/normalize/value.rs`
- Rust metric icon/render enum files as needed for the new metric icons

### Metric enum

Add `MetricKind` variants:

- `Altitude`
- `Iso`
- `Aperture`
- `ShutterSpeed`
- `FocalLength`
- `Ev`
- `ColorTemperature`

Preserve exact serde names:

- `"altitude"`
- `"iso"`
- `"aperture"`
- `"shutter_speed"`
- `"focal_length"`
- `"ev"`
- `"color_temperature"`

### Standard metric manifest loader

Extend Rust manifest parsing to understand:

- the new metrics
- `interpolation`
- `unitsMode`

Expose helpers like:

- `standard_metric_interpolation(kind)`
- `standard_metric_units_mode(kind)`

### Parsed activity schema

Extend `ParsedActivity` with:

- `altitude: NumericSeries`
- `iso: NumericSeries`
- `aperture: NumericSeries`
- `shutter_speed: NumericSeries`
- `focal_length: NumericSeries`
- `ev: NumericSeries`
- `color_temperature: NumericSeries`

Extend `TrimmedActivity` and `DenseSeriesReport` with the same fields.

### Render data requirements

Extend `RenderDataRequirements` derivation so requesting any of these widgets causes the corresponding series to be trimmed and densified.

This is not just a match-arm update. Add one `RenderDataRequirements` field per new metric and wire each field through `render_data_requirements()`.

### Densification

The backend currently has one special-case hold-like path for heading.

Refactor that into a reusable policy-based mechanism:

- add interpolation metadata to the Rust standard metric definition
- add a `standard_metric_interpolation(kind)` accessor
- replace the current hardcoded special case with generic metric-aware densification keyed off manifest policy
- linear densify
- hold densify

The implementation may be match-driven or table-driven, but it should no longer rely on a heading-only branch.

Then use manifest policy per metric.

Expected policies:

- `heading`: linear
- `iso`: hold
- `aperture`: hold
- `shutter_speed`: hold
- `focal_length`: hold
- `ev`: hold
- `color_temperature`: hold
- `altitude`: linear

The backend must stay consistent with frontend preview interpolation.

## Units Policy Enforcement

This needs both frontend and backend enforcement.

### Frontend editor

Update:

- `app/src/features/widget-editor/components/MetricWidgetEditor.jsx`
- `app/src/features/widget-editor/components/widgetEditorSections.jsx`

For metrics with `unitsMode: hidden`:

- do not render the units section
- do not render the units toggle
- do not render the unit selector
- do not render unit color controls

For `unitsMode: selectable` with exactly one supported display unit:

- render the units toggle if that metric allows units on/off
- do not render a unit dropdown
- use the single supported unit implicitly

### Frontend defaults and template state

For `unitsMode: hidden` metrics, materialize defaults like:

- `show_units: false`
- `display_unit: <manifest default>`
- `unit_color: '#ffffff'` or the usual default if still required structurally

The user must never see or edit these fields for `unitsMode: hidden`.

### Backend validation

Add policy enforcement:

- if `unitsMode == hidden`, `show_units` must be false
- if `unitsMode == hidden`, rendered unit text must always be empty
- if `unitsMode == selectable` and exactly one supported unit exists, preview/rendering must still succeed even if the UI never exposed a dropdown

## Altitude as a Standard Metric

Altitude is distinct from:

- `elevation` plot widget
- `sample_elevations`

Altitude must be added as a standard metric widget type and treated like speed/power/etc. in:

- frontend widget drawer
- frontend widget editor
- frontend preview formatting
- backend metric enum
- backend value formatter
- backend dense report

Altitude does not replace elevation. It is an additional scalar metric series.

## Independent Phases

Each phase should be independently landable and testable.

### Phase 1: Shared Manifest + Metric Catalog Extension

Files:

- `assets/standard-metrics.json`
- `app/src/lib/standard-widgets.js`
- `app/src/lib/standard-metrics.js`
- `src-tauri/ovrley_core/src/standard_metrics.rs`
- `src-tauri/ovrley_core/src/types.rs`

Deliverables:

- new metric definitions
- manifest parsing for `interpolation`
- manifest parsing for `unitsMode`
- helper accessors on both JS and Rust sides
- all approved metrics represented in the shared manifest

Acceptance criteria:

- [x] Shared manifest includes `altitude`, `iso`, `aperture`, `shutter_speed`, `focal_length`, `ev`, and `color_temperature`
- [x] Shared manifest uses only `unitsMode: selectable | hidden`
- [x] JS manifest adapters can read `interpolation` and `unitsMode`
- [x] Rust manifest adapters can read `interpolation` and `unitsMode`
- [x] Rust `MetricKind` supports all 7 new metric IDs
- [x] Existing standard metric consumers keep working after the manifest change

Manual tests:

- [x] Open the app and verify the widget drawer still loads without runtime errors
- [x] Confirm no existing standard metric preview crashes because of the new manifest keys
- [x] Confirm the new metrics appear in the metric catalog once labels are wired

### Phase 2: Frontend SRT Parser + Canonical Parsed Activity Output

Files:

- `app/src/lib/activity/srt-parser.js` (new)
- `app/src/lib/activity/import-activity.js`
- `app/src/lib/activity/parser.js`
- `app/src/lib/activity/metric-series.js`
- `app/src/lib/activity/gap-utils.js`

Deliverables:

- `.srt` dispatch
- cue parsing
- raw sample mapping
- canonical parsed activity output
- SRT gap-fill bypass

Acceptance criteria:

- [x] `.srt` files import through the frontend activity pipeline
- [x] Cue start times populate `sample_elapsed_seconds` with millisecond precision preserved
- [x] `abs_alt` populates both `altitude` and `elevation`
- [x] `ct` populates `color_temperature`
- [x] `ev` populates `ev`
- [x] Shutter values are stored numerically in seconds
- [x] SRT parsing bypasses idle-gap insertion
- [x] The finalized parsed activity shape matches FIT/GPX conventions closely enough for the existing backend/import flow

Manual tests:

- [x] Import the representative SRT sample and inspect the parse debug JSON
- [x] Verify `sample_elapsed_seconds` contains values like `2.001`, `3.002`
- [x] Verify `altitude`, `iso`, `aperture`, `shutter_speed`, `focal_length`, `ev`, and `color_temperature` arrays exist and align with `sample_elapsed_seconds`
- [x] Verify `speed`, `heading`, and `vertical_speed` can still be derived from the imported SRT activity

> **Post-implementation (windowed rate derivation):** SRT GPS data is sparse relative to cue cadence (GPS updates ~6-10 Hz vs cue data at ~30 Hz). `finalizeParsedActivity` now accepts `options.useWindowedRate` which routes speed and vertical_speed through `deriveWindowedRateSeries()` — a 1-second lookback window instead of per-sample differencing. Only the SRT parser passes this flag; FIT/GPX keep existing per-sample rate derivation unchanged.

### Phase 3: Frontend Widget Catalog + Units UI Plumbing

Files:

- `app/src/lib/standard-widgets.js`
- `app/src/lib/widget-icons.jsx`
- `app/src/lib/widget-icon-data.js`
- widget icon asset maps if needed
- `app/src/features/widget-editor/hooks/useWidgetManager.js`
- `app/src/features/widget-editor/components/MetricWidgetEditor.jsx`
- `app/src/features/widget-editor/components/widgetEditorSections.jsx`
- `app/src/features/widget-editor/utils/widgetUtils.js` (audit required; update if default metric widget generation needs explicit handling)

Deliverables:

- new widget types discoverable in drawer/quick add
- default widget creation works
- units UI hidden for `unitsMode: hidden`
- single-unit `selectable` metrics do not expose a dropdown
- icon import/parsing and widget defaults audited for the 7 new metrics

Acceptance criteria:

- [x] New metrics can be added as widgets from the frontend catalog
- [x] `altitude` behaves like a normal standard metric widget
- [x] `iso`, `aperture`, `shutter_speed`, and `ev` expose no units controls
- [x] `focal_length` and `color_temperature` can toggle units on/off
- [x] `focal_length` and `color_temperature` do not expose a unit dropdown when only one unit is supported

Manual tests:

- [x] Add each new metric widget from the drawer/quick menu
- [x] Open the widget editor for each and verify the expected unit controls appear or do not appear
- [x] Toggle units on/off for `focal_length` and `color_temperature`
- [x] Confirm there is no unit dropdown for a single-unit selectable metric

### Phase 4: Frontend Preview Interpolation + Formatting

Files:

- `app/src/features/overlay-editor/utils/overlayEditorUtils.js`
- `app/src/features/widget-preview/utils/formatUtils.js`
- preview tests for parity as needed

Deliverables:

- metric-aware `linear` vs `hold` interpolation
- altitude formatting
- camera metric formatting
- reciprocal shutter formatting
- `F/x.x` aperture formatting
- `K` color temperature formatting when units are shown
- safe default preview fallback values for the new metrics

Acceptance criteria:

- [x] Frontend preview uses manifest-driven `linear` vs `hold`
- [x] `shutter_speed` always renders as reciprocal style in preview
- [x] `aperture` renders as `F/x.x` in preview
- [x] `focal_length` renders with optional `mm`
- [x] `color_temperature` renders with optional `K`
- [x] Unitless metrics render no unit text
- [x] Preview fallback data exists for all new metrics when no activity is loaded

Manual tests:

- [x] Scrub between two different ISO values and confirm the preview holds rather than interpolates
- [x] Scrub between two altitude values and confirm the preview interpolates smoothly
- [x] Verify shutter text stays reciprocal throughout playback/scrubbing
- [x] Verify turning units off for `focal_length` and `color_temperature` removes the suffix cleanly

### Phase 5: Backend Schema + Normalization + Requirements

Files:

- `src-tauri/ovrley_core/src/activity/schema.rs`
- `src-tauri/ovrley_core/src/normalize/mod.rs`
- `src-tauri/ovrley_core/src/normalize/value.rs`
- `src-tauri/ovrley_core/src/standard_metrics.rs`

Deliverables:

- parsed/trimmed/dense schema support
- requirement plumbing
- units policy validation
- support for `ev` and `color_temperature`

Acceptance criteria:

- [x] `ParsedActivity`, `TrimmedActivity`, and dense schema structs carry the new series
- [x] Render data requirements request the new series when widgets use them
- [x] `unitsMode: hidden` is enforced in the frontend defaults chain and UI; backend manifest requires explicit `unitsMode` per definition
- [x] Single-unit selectable metrics remain valid and renderable

Manual tests:

- [x] Run Rust tests covering config validation and metric kind serde
- [x] Verify a config containing each new metric survives the normalization seam
- [x] Verify manifest rejects definitions missing `unitsMode` or `interpolation`

### Phase 6: Backend Densification + Render Formatting

Files:

- `src-tauri/ovrley_core/src/activity/interpolate.rs`
- `src-tauri/ovrley_core/src/render/format.rs`
- `src-tauri/ovrley_core/src/render/widgets/value/icons.rs`
- Rust icon enums such as `MetricIconAssetKey` / `MetricIconKind`
- renderer modules that switch over `MetricKind` as needed

Deliverables:

- manifest-driven hold interpolation
- dense series generation for new metrics
- altitude rendering as standard metric
- camera metric formatting in renderer
- reciprocal shutter formatting in Rust render
- `F/x.x` aperture formatting in Rust render
- optional `mm` / `K` unit rendering for single-unit selectable metrics

Acceptance criteria:

- [x] Rust densification honors manifest `linear` vs `hold`
- [x] New metrics appear in dense activity where required
- [x] Rust renderer formats shutter reciprocally
- [x] Rust renderer formats aperture as `F/x.x`
- [x] Rust renderer shows/hides `mm` and `K` based on `show_units`
- [x] Preview/render parity is maintained for the new metrics

> **Note:** The frontend SRT parser applies windowed rate derivation (1-second lookback) before producing the `speed` and `vertical_speed` series. The backend receives these as pre-computed numeric arrays — do NOT re-derive rates from raw GPS coordinates in Rust. Densification should interpolate the received arrays as-is.

Manual tests:

- [ ] Render preview frames for each new metric and compare with frontend preview
- [ ] Render a short export using an SRT activity and visually inspect the new widgets
- [ ] Verify no-unit metrics render without empty gaps or broken layout where units would have appeared

## Global Acceptance Criteria

- [ ] `.srt` files import successfully through the frontend activity import flow
- [ ] SRT cue start times populate `sample_elapsed_seconds` with millisecond precision preserved from the source
- [ ] The finalized parsed activity for SRT matches the same structural contract used by FIT/GPX
- [ ] SRT `abs_alt` populates `altitude` and also feeds `elevation` so derived metrics can work
- [ ] SRT `ct` populates `color_temperature` in Kelvin
- [ ] SRT `ev` populates `ev`
- [ ] SRT telemetry supports existing derived GPS metrics such as `speed`, `heading`, and `vertical_speed`
- [ ] `altitude`, `iso`, `aperture`, `shutter_speed`, `focal_length`, `ev`, and `color_temperature` are recognized as standard metric widget types in both frontend and backend
- [ ] The shared manifest contains interpolation metadata and unit-policy metadata for standard metrics
- [ ] Frontend preview interpolation respects manifest `linear` vs `hold`
- [ ] Rust densification respects manifest `linear` vs `hold`
- [ ] Frontend preview and Rust rendering format the new metrics consistently
- [ ] No-unit metrics expose no unit controls in the editor UI
- [ ] No-unit metrics render no unit text in preview or final render
- [ ] Single-unit selectable metrics render correctly whether units are on or off, without requiring a dropdown
- [ ] `shutter_speed` always renders in reciprocal style
- [ ] `aperture` renders as `F/x.x`
- [ ] Altitude is available as a standard metric widget and renders correctly
- [ ] Existing FIT/GPX parsing continues to work
- [ ] Existing non-camera widgets keep their current unit and interpolation behavior unless explicitly changed by the shared manifest

## Testing Plan

### Frontend tests

Add or update tests for:

- SRT parser happy path
- SRT parser missing-field tolerance
- shutter reciprocal parsing
- preserved elapsed times like `2.001`
- `hold` interpolation in preview
- `linear` interpolation still working for altitude/speed
- no-units editor behavior
- single-unit selectable metric editor behavior
- manifest adapter behavior for interpolation + units policy
- aperture/shutter/ct formatting behavior

Likely test locations:

- `app/src/tests/lib/activity/...`
- `app/src/tests/features/widget-preview/...`
- `app/src/tests/features/widget-editor/...`
- `app/src/tests/features/standard-metrics/...`

### Rust tests

Add or update tests for:

- new `MetricKind` serde variants
- standard metric manifest loading for new fields
- parsed activity deserialization with new series
- densify linear vs hold behavior
- unit policy validation in value widget normalization
- renderer format output for new metrics
- parity-oriented formatting tests where applicable
- single-unit selectable metrics with `show_units` true/false

Likely test locations:

- `src-tauri/ovrley_core/tests/metric_kind_serde_tests.rs`
- `src-tauri/ovrley_core/tests/wave1_format_tests.rs`
- `src-tauri/ovrley_core/tests/metric_presentation_tests.rs`
- interpolation-focused tests in `activity_tests.rs` or adjacent files

## Risks and Failure Modes

### 1. Shutter speed stored as string

If `shutter_speed` is stored as `"1/3200.0"` instead of numeric seconds, interpolation and standard metric formatting pipelines will break or fork unnecessarily.

Mitigation:

- normalize at parse time to numeric seconds

### 2. No-unit policy only enforced in UI

If unitless behavior is only hidden in the frontend editor, templates or future migrations may still produce visible or invalid unit behavior.

Mitigation:

- enforce no-unit policy in Rust normalization and Rust formatting too

### 3. Frontend and backend interpolation drift

If preview uses hold interpolation but backend densification stays linear, preview/render parity will break.

Mitigation:

- read interpolation policy from the shared manifest on both sides

### 4. Gap filler mutates dense SRT

Existing idle-gap insertion may create synthetic samples or distort dense telemetry if reused blindly.

Mitigation:

- skip idle-gap insertion for SRT

### 5. Single-unit selectable metrics break because no dropdown exists

If preview/render paths assume a visible unit selector is the only way to materialize a display unit, metrics like `focal_length` and `color_temperature` may format incorrectly.

Mitigation:

- always resolve display unit from widget data or manifest default, even if the UI never exposed a selector

### 6. Time precision loss

If elapsed seconds are rounded too aggressively, subtle drift can appear in dense telemetry.

Mitigation:

- preserve source millisecond timing through parse and interpolation
- avoid deriving from frame count

## Implementation Order

Recommended order:

1. Phase 1: Shared Manifest + Metric Catalog Extension
2. Phase 2: Frontend SRT Parser + Canonical Parsed Activity Output
3. Phase 3: Frontend Widget Catalog + Units UI Plumbing
4. Phase 4: Frontend Preview Interpolation + Formatting
5. Phase 5: Backend Schema + Normalization + Requirements
6. Phase 6: Backend Densification + Render Formatting
7. Add or repair tests alongside each phase if possible; otherwise complete the remaining test debt at the end

This order minimizes rework because:

- manifest decisions come first
- parser shape is defined before widget usage
- interpolation policy is shared before formatting/render parity is tested
- frontend preview is stabilized before Rust parity work

## Final Notes for the Implementing Agent

- Do not invent a new parallel activity contract if the current FIT/GPX-style normalized shape can be extended cleanly.
- Preserve existing behavior for all current metrics unless the shared manifest explicitly changes it.
- Keep preview and render parity as a first-class concern; any new formatting or interpolation branch added on one side should be mirrored on the other side in the same slice.
- Prefer explicit policy-driven behavior over scattered metric-name conditionals when possible.
- The product decisions in the “Locked Product Decisions” section are confirmed requirements and should not be treated as open questions unless the user explicitly changes them later.
