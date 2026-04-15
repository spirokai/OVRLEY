# Fusion Telemetry Overlay Project Instructions

## Purpose

Build a new project that replaces the current pre-rendered overlay movie workflow with a DaVinci Resolve Fusion-native workflow.

The new project has two major parts:

1. A React-based `.fit` / `.gpx` parser and normalizer that produces telemetry data and precomputed geometry optimized for use in DaVinci Resolve.
2. A generator that emits a Fusion `.setting` macro template and related assets so the overlay is rendered inside Resolve during final export, with centralized styling and per-clip ride selection.

This document is intended to be used as the implementation plan and working instructions for that project.

## Locked Decisions

The following product decisions are fixed:

1. The Resolve artifact is a Fusion `.setting` macro.
2. A companion Lua script or Fuse helper is allowed and expected where needed for runtime data processing and Inspector behavior.
3. Export one normalized JSON file per ride.
4. `rideId` is the stable ride identity; the Inspector should prefer a dropdown when practical.
5. The path widget is a simple stylized line route with no basemap tiles.

## Feasibility Note

Assume the target artifact is a Fusion `.setting` file, not a generic `.settings` file. DaVinci Resolve Fusion macros are commonly stored as `.setting` files.

Also assume that a pure `.setting` macro may not be sufficient for all requirements if it must:

1. Browse to an arbitrary external JSON file from the Inspector.
2. Parse ride data and feed processed values into the macro at runtime.
3. Populate ride choices dynamically in the Inspector.
4. Execute a true `Sync` button that copies global defaults into local sections.

The architecture should therefore assume a lightweight companion script or Fuse component for runtime data loading and Inspector behavior. Do not force a macro-only solution if it produces a brittle or unmaintainable design.

## Remaining Open Questions

These questions remain open and should be answered before implementation is finalized:

1. Confirm whether the elevation profile and path are expected to animate only a position indicator, with the underlying geometry precomputed and static.
2. Confirm whether the final target is Windows-first, macOS-first, or both for editor-side installation and font behavior.
3. Confirm whether `.fit` support is mandatory in the first milestone or may follow `.gpx` if parser complexity requires staging.

If any of the above remain unanswered, proceed with the assumptions defined in the next section.

## Default Assumptions

1. Use Fusion `.setting` as the primary target format.
2. Require a small companion Lua or Fuse helper for runtime JSON access and Inspector integration where needed.
3. Export one normalized JSON file per ride.
4. Use a stable `rideId` string as the canonical selector identity; expose a dropdown in the Inspector when runtime support allows it.
5. Render a stylized path only, with no basemap tiles.
6. Precompute path and elevation geometry outside Resolve; Resolve should only draw static geometry and animate the current position/value.
7. Support Windows and macOS.
8. Implement `.gpx` first if needed, but design the parser abstraction so `.fit` can be added without schema changes.

## Product Goal

The user should be able to:

1. Load one or more `.fit` and `.gpx` rides in a React application.
2. Normalize the telemetry to an export timeline, with `30 Hz` as the default and `60 Hz` as an optional high-smoothness mode.
3. Export a Resolve-ready data package.
4. Add a Fusion macro to a clip or timeline in DaVinci Resolve.
5. Pick the source ride for that clip.
6. Adjust global and per-gauge styling in the Inspector.
7. Render the final edit in one Resolve export pass without separately pre-rendering an overlay movie.

## Non-Goals

1. Do not rebuild full video editing features.
2. Do not generate a standalone overlay video asset as the primary workflow.
3. Do not attempt an OFX plugin in this project.
4. Do not build a heavy map renderer or tile downloader.
5. Do not depend on custom fonts in milestone one.

## High-Level Architecture

The project should be split into four layers.

### Layer 1: React Telemetry Workbench

Responsibilities:

1. File import for `.gpx` and `.fit`.
2. Parse raw telemetry.
3. Normalize telemetry to a common schema.
4. Precompute geometry for map path and elevation profile.
5. Surface statistics such as min and max values.
6. Export a Resolve-ready package.

### Layer 2: Normalized Data Package

Responsibilities:

1. Store all rides in a stable schema.
2. Preserve 60 samples per second for the primary time series.
3. Store extrema, metadata, and precomputed graphics geometry.
4. Make lookup by ride ID and timeline time simple.

### Layer 3: Fusion Macro Generator

Responsibilities:

1. Emit one or more Fusion `.setting` files.
2. Define Inspector controls grouped into pages or tabs.
3. Expose global settings and per-gauge settings.
4. Bind Fusion nodes to normalized data.

### Layer 4: Resolve Runtime Helper

Responsibilities:

1. Read JSON at runtime.
2. Resolve `rideId` selection and timeline offset.
3. Feed computed values into macro controls or expressions.
4. Implement actions such as `Sync` and dropdown population that cannot be done with a static macro.

## Recommended Tech Stack

### Frontend App

1. React with TypeScript.
2. Vite for bundling.
3. Zustand or Redux Toolkit for app state.
4. Zod for schema validation.
5. Web Workers for parsing and preprocessing large files.
6. `fit-file-parser` or another maintained FIT parser with browser support.
7. A GPX parser with extension support, or a custom XML parser if telemetry extensions are needed.

### Data Export

1. JSON as the canonical export format.
2. Optional secondary artifacts such as SVG, simplified polyline JSON, or pre-bucketed profile arrays.

### Fusion Side

1. Fusion `.setting` macro as the default delivery target.
2. Required Lua script or Fuse helper for runtime JSON access and Inspector logic where Fusion macros alone are insufficient.

## Deliverables

The project should produce the following deliverables.

### Deliverable A: Parser App

1. Import one or multiple `.fit` and `.gpx` files.
2. Display ride metadata and telemetry coverage.
3. Preview the simplified path and elevation profile.
4. Export a normalized data package.

### Deliverable B: Data Package

Each ride export package should contain:

1. One ride JSON file as the primary normalized data file.
2. Optional generated icons if Fusion needs explicit assets.
3. Optional static SVG or point lists for path/profile if the Fusion side performs better with those assets.

### Deliverable C: Fusion Macro Package

The package should contain:

1. `CyclemetryOverlay.setting` or equivalent.
2. Optional helper script or Fuse if needed.
3. Installation instructions for Resolve.
4. A sample package with test rides.

## Normalized Data Requirements

The parser must extract at minimum:

1. Power
2. Heart rate
3. Speed
4. Cadence
5. Gradient
6. GPS coordinates
7. Elevation
8. Timestamp

The parser must also compute:

1. Minimum value per metric
2. Maximum value per metric
3. Total duration
4. Start timestamp
5. End timestamp
6. Distance traveled
7. Optional smoothing metadata if used

### Required Sampling Behavior

Default to `30 Hz` export sampling for Resolve runtime data. Offer `60 Hz` as an optional high-smoothness mode for 50/60 fps timelines or motion-sensitive overlays.

This means:

1. The canonical export timeline is sampled at a configurable `sampleRateHz`.
2. The default `sampleRateHz` is `30`.
3. An optional `sampleRateHz` of `60` is supported.
4. Raw files may have lower or uneven sampling rates.
5. The parser must interpolate or forward-fill carefully depending on metric semantics.
6. Resolve runtime lookup should use the exported sample rate directly and should not do its own interpolation.

### Metric Semantics

Use the following normalization rules.

1. Power: numeric, watts, interpolate linearly when reasonable.
2. Heart rate: numeric, bpm, interpolate linearly with optional clamp.
3. Speed: meters per second in canonical storage, convert to km/h or mph at presentation time.
4. Cadence: numeric, rpm, interpolate linearly or hold short gaps.
5. Gradient: canonical numeric percent, derived from smoothed elevation and distance delta.
6. Elevation: meters in canonical storage.
7. GPS path: canonical latitude and longitude plus derived local projected coordinates.

## Telemetry Processing Pipeline

### Step 1: Parse Raw Source Files

For `.gpx`:

1. Parse timestamps, coordinates, elevation, and extensions.
2. Read power, heart rate, cadence, and temperature if present in extensions.
3. Handle missing extension values safely.

For `.fit`:

1. Parse record messages.
2. Extract timestamped power, heart rate, cadence, speed, position, and altitude.
3. Normalize vendor-specific field naming before entering the canonical schema.

### Step 2: Build Canonical Raw Samples

Build a time-ordered raw sample array with a structure similar to:

```json
{
  "t": 12.5,
  "lat": 50.123,
  "lon": 14.456,
  "elevationM": 215.4,
  "powerW": 287,
  "heartRateBpm": 164,
  "cadenceRpm": 91,
  "speedMps": 10.8
}
```

### Step 3: Normalize Time Base to Export Sample Rate

1. Create a canonical timeline from `0` to `durationSeconds` in steps of `1 / sampleRateHz`.
2. For each metric, sample the raw data onto that timeline.
3. Use metric-aware interpolation.

Recommended interpolation rules:

1. Timestamps: exact timeline steps.
2. Position: linear interpolation in projected XY space, not directly in latitude/longitude if possible.
3. Elevation: linearly interpolate after smoothing.
4. Power, heart rate, cadence: linearly interpolate for short gaps, hold value for very short missing spans, use `null` only when data is truly absent for long periods.
5. Speed: prefer source speed if present; otherwise derive from distance delta over time.

### Step 4: Compute Derived Metrics

Compute the following derived metrics after normalization.

1. `distanceM`: cumulative path distance.
2. `gradientPct`: computed from smoothed elevation and horizontal distance.
3. `elapsedSeconds`: exact normalized time.

### Gradient Algorithm

Gradient is noisy if calculated from raw elevation. Use this algorithm:

1. Smooth elevation with a short rolling median or Savitzky-Golay filter.
2. Compute horizontal distance over a centered window rather than a frame-to-frame delta.
3. Use a window based on traveled distance, not just sample count.
4. Recommended default: calculate grade over a 5 m to 15 m lookback/lookahead window.
5. Clamp absurd spikes caused by GPS or elevation noise.

Recommended formula:

$$
gradientPct = 100 \times \frac{\Delta elevationM}{\Delta horizontalDistanceM}
$$

Guardrails:

1. If horizontal distance is below a small threshold, hold previous gradient.
2. Clamp final output to a sane range such as `-30` to `30` unless domain testing justifies wider bounds.

### Step 5: Compute Extrema

For each ride, compute:

1. `minPowerW`, `maxPowerW`
2. `minHeartRateBpm`, `maxHeartRateBpm`
3. `minCadenceRpm`, `maxCadenceRpm`
4. `minSpeedMps`, `maxSpeedMps`
5. `minGradientPct`, `maxGradientPct`
6. `minElevationM`, `maxElevationM`

These should be stored explicitly in the export package so Fusion does not have to compute them.

## Map Geometry Precomputation

The path should be pre-generated from GPS coordinates outside Resolve.

### Projection Strategy

Do not draw directly in latitude and longitude.

1. Convert GPS coordinates to a local projected XY coordinate system.
2. Use a simple local tangent-plane approximation or equirectangular projection centered on the ride bounds.
3. Normalize projected coordinates into a `0..1` box for template layout.

Suggested approach:

1. Compute the geographic center of the ride.
2. Convert lat/lon to local meters.
3. Preserve cumulative distance index mapping between original samples and projected path points.

### Simplification Strategy for Path

The route path should be simplified for rendering performance while keeping motion believable.

Recommended algorithm:

1. First simplify geometry using Ramer-Douglas-Peucker on the normalized XY path.
2. Use tolerance in output-space pixels, not geographic units.
3. Preserve key anchor points such as start, end, and sharp turns.
4. Keep a mapping from normalized sample time to nearest simplified segment position.

Suggested tolerance starting points:

1. For a 200 px to 600 px route widget, start with a tolerance equivalent to `0.5 px` to `1.5 px` in the target widget.
2. Never simplify so aggressively that hairpins or major switchbacks collapse.

### Path Output Structure

Store:

1. Simplified XY polyline points.
2. Bounding box.
3. Cumulative distance per simplified point if useful.
4. A per-frame or per-sample normalized position along the path.

The easiest runtime path animation is to store, for every exported sample:

1. `pathProgress01`
2. Optional `pathX01`
3. Optional `pathY01`

That lets Resolve animate the position indicator without recomputing geometry traversal.

## Elevation Profile Precomputation

The elevation profile should also be pre-generated outside Resolve.

### Basic Strategy

1. Use distance on X.
2. Use elevation on Y.
3. Normalize into a `0..1` box.

### Simplification Strategy

The elevation profile should not preserve all exported telemetry samples if the output chart is only a few hundred pixels wide.

Recommended approach:

1. Build a high-resolution canonical profile from normalized data.
2. Downsample using one of the following:
   - Largest-Triangle-Three-Buckets for visual fidelity
   - Min/max bucket downsampling for performance and preservation of peaks
3. Prefer min/max bucket downsampling when the chart is filled and a moving cursor is drawn over it.

Recommended bucket rule:

1. Determine intended chart width in pixels.
2. Create roughly `2 x widthPx` buckets or fewer.
3. Preserve local extrema in each bucket.

Suggested final export structure:

1. Simplified profile points for drawing the full path.
2. Per-sample `profileProgress01` so the cursor can move cheaply.
3. Optional per-sample `profileY01` for the current altitude point.

## Canonical Export Schema

The canonical export should be stable, versioned, compact, and represent exactly one ride per JSON file.

Use an array-based schema for runtime telemetry, not an array of per-sample objects. Repeating object keys at every sample wastes space and makes large rides slower to load.

The export should therefore separate:

1. Ride metadata and extrema.
2. Static graphics geometry.
3. Compact parallel arrays for time-sampled telemetry.

Suggested top-level structure:

```json
{
  "schemaVersion": 1,
  "appVersion": "0.1.0",
  "generatedAt": "2026-04-13T12:00:00Z",
  "sampleRateHz": 30,
  "rideId": "ride-001",
  "label": "Morning Ride",
  "source": {
    "fileName": "ride.fit",
    "format": "fit"
  },
  "metadata": {
    "durationSeconds": 3600.0,
    "distanceM": 42195.2,
    "startTime": "2026-04-13T08:15:00Z"
  },
  "extrema": {
    "powerW": { "min": 0, "max": 981 },
    "heartRateBpm": { "min": 82, "max": 186 },
    "cadenceRpm": { "min": 0, "max": 128 },
    "speedMps": { "min": 0, "max": 18.4 },
    "gradientPct": { "min": -14.2, "max": 18.9 },
    "elevationM": { "min": 202.1, "max": 487.3 }
  },
  "telemetry": {
    "timeSeconds": [0.0, 0.0333, 0.0667],
    "powerW": [0, 3, 7],
    "heartRateBpm": [85, 85, 85],
    "cadenceRpm": [0, 0, 1],
    "speedMps": [0.0, 0.1, 0.2],
    "gradientPct": [0.0, 0.0, 0.1],
    "elevationM": [215.2, 215.2, 215.21],
    "distanceM": [0.0, 0.03, 0.07],
    "pathProgress01": [0.0, 0.0002, 0.0005],
    "pathX01": [0.123, 0.1235, 0.124],
    "pathY01": [0.842, 0.8417, 0.8413],
    "profileProgress01": [0.0, 0.0002, 0.0005],
    "profileY01": [0.611, 0.611, 0.6109]
  },
  "graphics": {
    "path": {
      "points01": [
        [0.1, 0.8],
        [0.11, 0.79]
      ],
      "bbox": { "minX": 0, "minY": 0, "maxX": 1, "maxY": 1 }
    },
    "elevationProfile": {
      "points01": [
        [0.0, 0.6],
        [0.01, 0.59]
      ]
    }
  }
}
```

## React App Functional Plan

### Core Screens

Implement at least these screens or panels.

1. Import panel
2. Ride list panel
3. Ride detail and validation panel
4. Geometry preview panel for path and elevation profile
5. Export panel

### Import Workflow

1. User drops one or multiple `.fit` / `.gpx` files.
2. Files are parsed in a Web Worker.
3. The app reports parsing status, detected metrics, duration, and missing fields.
4. The user can rename rides and exclude rides from export.
5. The user can choose export sample rate, with `30 Hz` as default and `60 Hz` as optional.

### Validation Workflow

The app should show for each ride:

1. Which metrics are present.
2. Which metrics were derived rather than source-provided.
3. Min and max values.
4. Duration and sample count.
5. Any warnings such as missing HR, unstable GPS, or sparse elevation.

### Export Workflow

1. User chooses export destination format.
2. App generates normalized JSON.
3. App generates Fusion `.setting` package from the current style preset.
4. App offers download as a zip package.

## Fusion Template Architecture

The Fusion side should be built as a stylized macro, not as a procedural full redraw of complex geometry every frame.

### Design Principle

Fusion should do the minimum amount of per-frame work necessary.

That means:

1. All telemetry normalization is done outside Resolve.
2. Path and elevation geometry are precomputed outside Resolve.
3. Fusion should mostly animate text values, progress states, and indicator positions.
4. Avoid expensive blur, glow, and recomputed shape logic where simpler precomputed assets will do.

### Proposed Macro Structure

Use one top-level Fusion macro called something like `CyclemetryOverlay`.

Inside it, organize subgroups or logical sections for:

1. Global controls
2. Speed gauge
3. Heart rate gauge
4. Power gauge
5. Cadence gauge
6. Gradient gauge
7. Path widget
8. Elevation profile widget

If node complexity grows too much, split these into reusable sub-macros and assemble them in a single root macro.

### Data Binding Strategy

Do not start Fusion development with live JSON ingestion.

The first Fusion milestone must be a UI-only macro prototype that uses dummy values and no external data file. The purpose of that phase is to validate:

1. Overall visual styling.
2. Inspector page structure and control ergonomics.
3. Inheritance and override behavior for styling controls.
4. Node layout and macro maintainability.
5. Whether the chosen Fusion implementation approach is practical before runtime data wiring is introduced.

Only after the UI-only macro behaves correctly with dummy values should the project move on to JSON ingestion and runtime data binding.

Implement data lookup with one of these approaches, in order of preference:

1. Companion helper reads JSON and exposes current frame values to the macro.
2. Export precomputed keyframe splines or time-sampled lookup tables directly into the `.setting` package.
3. Macro expressions read a static data block if the amount of embedded data is manageable.

Do not depend on Fusion recomputing interpolation, path simplification, or gradient derivation.

## Inspector Control Plan

Expose controls in the Inspector using Fusion `UserControls` pages or equivalent grouping.

### Global Controls

Must include:

1. Browse to JSON file
2. Font numbers plus size
3. Font title plus size
4. Default icon color, default white
5. Default text color, default white
6. Default title color, default white
7. Units `kmh` / `mph`, default `kmh`
8. Default gauge size, default `200 x 200 px`
9. Global overlay scale range `0.1` to `4`
10. Offset in seconds with single decimal precision, default `0`
11. Sync button
12. Shadow color for text, default light grey
13. Outline color for text, default transparent or none
14. Ride selector for current clip

### Numeric Gauge Controls

Each of `speed`, `hr`, `power`, and `cadence` gets its own Inspector page or tab.

Each must include:

1. Display on or off, default on
2. Icon browse is not needed because icon is baked in
3. Icon size
4. Icon horizontal offset
5. Icon vertical offset
6. Icon on or off, default on
7. Icon color, default inherits from Global
8. Text color, default inherits from Global
9. Title color, default inherits from Global
10. Display units on or off, default on
11. Display title on or off, default on
12. Display title top or bottom, default top
13. Display dashed progress bar on or off, default on
14. Number of bars
15. Bar width
16. Bar height
17. Bar spacing
18. Glow on or off

### Gradient Gauge Controls

Gradient is a numeric text plus a triangle below or above with angle corresponding to gradient.

Must include:

1. Display on or off, default on
2. Display label on or off, default on
3. Display label top or bottom, default top
4. Text size
5. Number size
6. Width, default `200 px`

### Path Controls

Must include:

1. Display on or off, default on
2. Position indicator color, default inherits from Global text color
3. Position indicator size, default `40 px`
4. Path color completed, default inherits from Global text color
5. Path color to go, default light grey
6. Path width, default `10 px`
7. Rotate, default `0 deg`

### Elevation Profile Controls

Must include:

1. Display on or off, default on
2. Position indicator color, default inherits from Global text color
3. Position indicator size, default `40 px`
4. Fill color, default inherits from Global text color
5. Fill opacity, default `0.7`
6. Path color completed, default inherits from Global text color
7. Path color to go, default light grey
8. Path width, default `10 px`
9. Display altitude in meters checkbox, default on
10. Display altitude in feet checkbox, default off
11. Text size

## Global Override and Sync Model

The user requested centralized styling plus per-section controls.

Implement the settings model with inheritance, not destructive copying.

Recommended model:

1. Each per-gauge color or size control can either inherit from Global or override locally.
2. A `Use Global` toggle is safer than copying values.
3. The `Sync` button should only force-refresh inherited fields or re-read source data.

If a literal copy-style sync is required, only implement it if Fusion scripting support is available. Do not fake a sync button with unreliable macro-only logic.

## Ride Selection Model

Each clip should be able to choose which ride JSON is the source and use that file's `rideId` as the canonical identity.

Recommended implementation:

1. Each ride is exported as its own JSON file.
2. Each ride file has a stable `rideId` and human-readable `label`.
3. Inspector should expose a dropdown as the primary ride selector if the runtime helper can populate it reliably.
4. A `rideId` text field may exist as an advanced fallback, but not as the primary UX.
5. The `Browse to JSON file` control remains the source-file chooser for direct linking to a specific ride package.

At render time:

1. Resolve frame time is mapped to clip-local time.
2. Clip-local time plus user `offsetSeconds` selects a telemetry sample.
3. The selected ride file and its `rideId` determine the sample arrays and geometry.

## Time Mapping Model

The overlay values must be deterministic and easy to reason about.

For a given frame:

1. Get clip-local time from Resolve.
2. Add `offsetSeconds`.
3. Clamp between `0` and ride duration.
4. Convert to sample index:

$$
sampleIndex = round((clipLocalTime + offsetSeconds) \times sampleRateHz)
$$

5. Fetch all current values from the compact telemetry arrays at `telemetry.metricName[sampleIndex]`.

This model is simple, stable, and avoids runtime interpolation inside Resolve.

## Gauge Rendering Strategy

### Numeric Gauges

Each numeric gauge should be built from:

1. A title element
2. A numeric text element
3. Optional units text
4. A baked icon
5. A dashed progress bar made from repeated rectangles or similar shapes

Progress bar value should be normalized from the ride extrema.

Recommended normalized value formula:

$$
progress01 = \frac{value - minValue}{maxValue - minValue}
$$

Guard against zero ranges by forcing `progress01 = 0`.

### Gradient Gauge

Use:

1. Numeric gradient text
2. Optional label
3. A triangle indicator rotated based on gradient angle

Do not map gradient directly to angle one-to-one. Use a bounded mapping, for example:

1. Clamp gradient to a design range such as `-15%` to `15%`
2. Map to a triangle angle range such as `-45 deg` to `45 deg`

### Path Widget

Use:

1. Precomputed static path geometry
2. Two visual states for completed and remaining path
3. A moving position indicator

Avoid recomputing path traversal from raw GPS in Fusion.

### Elevation Profile Widget

Use:

1. Precomputed profile geometry
2. Optional fill under the completed profile area
3. A moving position indicator
4. Optional text for current altitude in meters and feet

## Performance Strategy

The project succeeds only if Resolve-side rendering remains materially cheaper than the current external Python render.

### Rules

1. Precompute everything possible outside Resolve.
2. Avoid runtime parsing of huge files every frame.
3. Cache static geometry and baked icons.
4. Keep glowing or soft-shadow effects optional.
5. Avoid heavy blur nodes in default presets.
6. Prefer simple shape animation over procedural redraws.
7. Use sampled lookup at the exported `sampleRateHz` rather than arbitrary timestamp interpolation in Fusion.
8. Default to `30 Hz` export to keep runtime files smaller; use `60 Hz` only when the extra smoothness is justified.

### Specific Optimizations

1. Normalize and simplify path geometry before export.
2. Downsample elevation profile based on intended widget width.
3. Store per-sample normalized indicator coordinates.
4. Pre-store extrema instead of calculating them in Resolve.
5. Use Text+ only where necessary and keep styled text logic simple.

## Implementation Milestones

### Milestone 1: Project Skeleton

1. Set up React plus TypeScript app.
2. Create parser abstraction for `.gpx` and `.fit`.
3. Define canonical schemas with validation.
4. Add worker-based import pipeline.

### Milestone 2: GPX and FIT Normalization

1. Parse `.gpx` successfully.
2. Parse `.fit` successfully.
3. Produce canonical raw samples.
4. Normalize to export sample rate, with `30 Hz` default and `60 Hz` optional.
5. Compute extrema and derived fields.

### Milestone 3: Geometry Pipeline

1. Project GPS to XY.
2. Simplify route geometry.
3. Build elevation profile geometry.
4. Store per-sample path and profile progress coordinates.
5. Render previews in the React app.

### Milestone 4: Export Package

1. Export stable JSON package.
2. Export icons and optional static assets.
3. Add package versioning.

### Milestone 5: UI-Only Fusion Macro Prototype

1. Build one numeric gauge end-to-end with dummy values only.
2. Validate Inspector control grouping and page structure.
3. Validate styling controls, inheritance, and override behavior.
4. Validate macro layout, node organization, and maintainability in Resolve.
5. User-test that single gauge and gather feedback on readability, styling, control ergonomics, and expected customization behavior.
6. Confirm that the single-gauge visual language is approved before any runtime data wiring begins.

### Milestone 6: Runtime Data Binding Prototype

1. Wire source file browsing, ride selection, and offset.
2. Connect one numeric gauge to real ride data.
3. Connect one path widget or one elevation widget to real ride data.
4. Validate timing, offset behavior, and runtime responsiveness in Resolve.
5. User-test the single-gauge real-data prototype before expanding the macro surface.
6. Confirm the data binding approach works in Resolve.

### Milestone 7: Full Overlay Macro

1. Only after the single-gauge UI and single-gauge data-bound prototype are approved, add the remaining numeric gauges.
2. Add gradient gauge.
3. Add path widget.
4. Add elevation profile widget.
5. Extend inheritance and overrides across the full control surface.

### Milestone 8: Packaging and QA

1. Install macro in Resolve on Windows.
2. Install macro in Resolve on macOS.
3. Validate fonts and color controls.
4. Validate multiple ride packages in one Resolve project.
5. Validate offset behavior and clip syncing.

## Acceptance Criteria

The project is complete when all of the following are true.

1. User can import `.gpx` and `.fit` rides in the React app.
2. Exported JSON contains normalized telemetry arrays at the chosen export sample rate and includes extrema.
3. Exported package contains path and elevation profile geometry optimized for Resolve.
4. Resolve macro loads with usable Inspector controls.
5. User can set a source data file and pick a ride per clip, with dropdown-first selection behavior.
6. User can adjust global and per-gauge styles in the Inspector.
7. Overlay values track clip time plus offset correctly.
8. Path and elevation position indicators animate correctly.
9. Final workflow requires only one Resolve export pass.

## Risks and Mitigations

### Risk 1: Pure Macro Cannot Handle Runtime Data Loading

Mitigation:

1. Add a small companion Lua or Fuse component.
2. Keep the macro responsible for presentation only.

### Risk 2: FIT Parsing in Browser Is Inconsistent

Mitigation:

1. Evaluate parser libraries early.
2. Lock a canonical adapter layer.
3. Include a corpus of real FIT files from multiple devices.

### Risk 3: Fusion Render Cost Is Still Too High

Mitigation:

1. Keep geometry static and precomputed.
2. Avoid expensive default effects.
3. Profile each widget independently.

### Risk 4: Inspector UX Becomes Too Crowded

Mitigation:

1. Group controls by page.
2. Use clear defaults.
3. Prefer inheritance over duplicated controls where possible.

## Recommended First Prototype

Do not start by building the entire overlay.

Start with this smallest meaningful slice:

1. Build one Fusion macro with dummy values only.
2. Validate styling, layout, and Inspector user controls in Resolve.
3. Build and polish one numeric gauge only, with no JSON ingestion.
4. User-test that one gauge and approve its styling and controls.
5. Only after the single gauge is approved, parse one `.gpx` ride.
6. Normalize speed, heart rate, cadence, power, gradient, and elevation to `30 Hz`.
7. Precompute path geometry and elevation profile.
8. Export one JSON package.
9. Plug real ride data into the already-approved single gauge.
10. Validate that clip time drives the overlay correctly in Resolve.
11. User-test the single-gauge real-data version.

Only after that works reliably and user feedback is positive should the project expand to all gauge types and the full Inspector model.

## Execution Guidance for the Next Agent or Team

1. Treat the UI-only Fusion macro as the first technical spike; prove styling, Inspector ergonomics, and macro structure on a single gauge before touching runtime data binding.
2. Lock the export schema early and version it.
3. Keep presentation concerns out of the parser layer.
4. Keep parser-derived units canonical and perform unit conversion in the presentation layer.
5. Profile the geometry simplification and preview pipeline with long rides before building Resolve packaging.
6. Add an explicit user-feedback gate after the single-gauge UI prototype and another after the single-gauge real-data prototype.
7. After the UI-only macro is approved, treat runtime data binding as the next technical spike and validate it on that same single gauge before scaling out.
8. Build and validate one widget end-to-end before expanding horizontally across all widgets.
