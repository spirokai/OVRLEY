# Shared widget manifests

standard-widgets.json defines standalone widgets and shared defaults.
standard-metrics.json defines metric widgets, display modes, and activity-series metadata.

A metric's activity.sources lists numeric source columns in priority order. activity.requires
adds supporting columns, such as coordinates for distance to home. Time and lap display modes
declare their dependencies in the mode's activity array.

activity.column declares the activity column associated with that definition. It extends
activitySeries.numericDefaults; interpolation comes from the definition's existing
interpolation field. unit describes source values before display conversion, so speed's
source unit is mps even when its display unit is kmh.

Column options describe whether extraction exposes the column (input), whether row input
contains it (rawSample), whether finalization produces it (finalize), its permitted
smoothing method, idle-gap values (idleValue: preserve, zero, or missing), and whether sampling
must stay within observed coverage (limitToObserved).
Missing external samples remain optional throughout preparation.

Supporting columns without their own widget, including barometric altitude and G-force axes,
live in activitySeries.auxiliary. Structural input columns and ordered availability attributes
also live in activitySeries. activity.coverageSource declares an availability alias when a
widget advertises the coverage of another column.

The shared Rust crate generates metric identities, typed activity fields, input-column lists,
and numeric assembly operations from these manifests during compilation. Generic extraction,
validation, ranges, trimming, densification, and preview sampling consume those declarations.
Adding a scalar metric using an existing renderer does not require new per-widget tables in
those stages. New derivation formulas, source-format decoding, and renderer implementations
remain Rust code.
