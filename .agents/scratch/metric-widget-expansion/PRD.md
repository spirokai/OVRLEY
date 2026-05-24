Status: ready-for-agent

# Metric Widget Expansion and Standardization

## Problem Statement

OVRLEY already parses or derives more telemetry than the overlay editor can currently turn into metric value widgets. The editor and renderer support a small set of standard metric widgets today, but adding the planned widget set with the current per-type branching would increase duplication across activity parsing, widget creation, editor controls, preview formatting, Rust rendering, icon management, and template serialization.

At the same time, the current unit-selection schema is inconsistent. Existing standard metric widgets mix legacy unit fields and legacy fallback behavior, while the planned widget set needs richer unit choices, clearer formatting rules, and a reliable frontend/backend contract. The result is that OVRLEY cannot yet expose the planned metric widget catalog in a scalable way, and the current schema would make future additions increasingly expensive and error-prone.

## Solution

Extend OVRLEY's standard metric widget system so the editor, preview renderer, and Rust renderer can support a larger catalog of metric value widgets through a shared metadata-driven layer.

Wave 1 will add the already parsed or derived standard metric widgets:

- `pace`
- `g_force`
- `air_pressure`
- `ground_contact_time`
- `left_right_balance`
- `stride_length`
- `stroke_rate`
- `torque`
- `vertical_speed`

Wave 2 will add the parser-backed but not yet extracted standard metric widgets:

- `gear_position`
- `vertical_ratio`
- `core_temperature`

The implementation will keep explicit widget types per metric, but standardize how standard metric widgets are defined, configured, formatted, rendered, and serialized. Standard metric value widgets will use `display_unit` as the canonical unit-selection field. The old mixed unit schema is intentionally replaced, the template version is intentionally bumped, and older template versions are intentionally rejected on load.

All Wave 1 widgets will appear in the widget drawer regardless of activity coverage. If the loaded activity does not provide a metric, the widget will render a placeholder while preserving layout. Advanced graphical widgets are explicitly excluded from this PRD.

The icon plan is also part of the feature contract. The following standard metric widgets use extracted Lucide SVGs as their shared assets:

- `pace` -> `Footprints`
- `air_pressure` -> `Wind`
- `left_right_balance` -> `Scale`
- `stride_length` -> `Ruler`
- `stroke_rate` -> `Waves`
- `vertical_speed` -> `TrendingUp`
- `vertical_ratio` -> `Percent`
- `core_temperature` -> `Thermometer`

The following standard metric widgets require custom shared SVG assets that fit seamlessly with the Lucide icon family:

- `g_force`
- `ground_contact_time`
- `torque`
- `gear_position`

All shared standard metric widget icon assets live canonically in `assets/widget-icons/`. Extracted Lucide icons for these widgets must be committed there as SVG files and consumed from there by both preview and export. They must not remain runtime-only `lucide-react` component references for the widget system.

## User Stories

1. As an overlay editor user, I want to add Pace as a standard metric widget, so that I can show running or swimming pace in my overlay.
2. As an overlay editor user, I want Pace to default to `m:ss min/km`, so that the overlay matches common endurance-sport expectations.
3. As an overlay editor user, I want Pace to support `min/km` and `min/mi`, so that I can choose the unit that matches my audience.
4. As an overlay editor user, I want to add G-Force as a standard metric widget, so that I can show dynamic load in motor, cycling, ski, or flight footage.
5. As an overlay editor user, I want G-Force to support both `g` and `m/s²`, so that I can choose between the familiar sport unit and the SI unit.
6. As an overlay editor user, I want to add Air Pressure as a standard metric widget, so that I can show barometric conditions in outdoor overlays.
7. As an overlay editor user, I want Air Pressure to support `hPa`, `mbar`, `inHg`, and `mmHg`, so that I can match regional expectations.
8. As an overlay editor user, I want to add Ground Contact Time as a standard metric widget, so that I can show running dynamics data in the overlay.
9. As an overlay editor user, I want Ground Contact Time to render in milliseconds, so that the metric is immediately legible.
10. As an overlay editor user, I want to add Left/Right Balance as a standard metric widget, so that I can visualize asymmetry and balance from my activity data.
11. As an overlay editor user, I want Left/Right Balance to offer several display formats, so that I can pick the most readable style for my layout.
12. As an overlay editor user, I want Left/Right Balance to default to `52% / 48%`, so that the widget is clear without additional setup.
13. As an overlay editor user, I want Left/Right Balance to support `52 / 48`, `L52 / R48`, `52% / 48%`, and `52L / 48R`, so that I can choose the exact format I prefer.
14. As an overlay editor user, I want Left/Right Balance to behave like a normal standard metric widget, so that it can use the same icon, typography, positioning, and unit controls model as the other standard metric widgets.
15. As an overlay editor user, I want to add Stride Length as a standard metric widget, so that I can show running efficiency data in the overlay.
16. As an overlay editor user, I want Stride Length to support `m`, `cm`, `ft`, and `in`, so that I can choose the most natural unit for my audience.
17. As an overlay editor user, I want to add Stroke Rate as a standard metric widget, so that I can show swim or rowing cadence in the overlay.
18. As an overlay editor user, I want Stroke Rate to render in `spm`, so that it matches the expected sport-specific convention.
19. As an overlay editor user, I want to add Torque as a standard metric widget, so that I can show cycling torque in the overlay.
20. As an overlay editor user, I want Torque to render as a standard metric widget with consistent icon, value, and unit behavior, so that it feels native beside the existing widgets.
21. As an overlay editor user, I want to add Vertical Speed as a standard metric widget, so that I can show climb or descent rate in paragliding, hiking, or cycling footage.
22. As an overlay editor user, I want Vertical Speed to support `m/s`, `ft/min`, and `m/h`, so that I can use the unit system most familiar to my audience.
23. As an overlay editor user, I want all Wave 1 widgets to appear in the widget drawer even before I load an activity, so that I can build templates ahead of time.
24. As an overlay editor user, I want widgets with missing data to render placeholders instead of disappearing, so that my layout stays stable across activities.
25. As an overlay editor user, I want standard metric widgets old and new to share one coherent unit-selection model, so that configuring widgets feels consistent.
26. As an overlay editor user, I want standard metric widgets to use a canonical `display_unit` field, so that templates have a simpler and more predictable schema.
27. As an overlay editor user, I want my new standard metric widgets to use the same editor patterns as the existing standard metric widgets, so that learning one widget helps me use the others.
28. As an overlay editor user, I want the widgets that benefit from fractional values to offer a simple `0 / 1 decimal` control, so that I can improve readability without micromanaging formatting.
29. As an overlay editor user, I want the widgets that do not benefit from fractional values to avoid unnecessary decimal controls, so that the widget editor stays clean.
30. As an overlay editor user, I want all Wave 1 standard metric widgets to have final production-quality icons, so that templates do not need visual rework later.
31. As an overlay editor user, I want Lucide-based icons to be extracted into shared SVG assets, so that preview and backend render use the same icon source.
32. As an overlay editor user, I want custom metric icons to match the Lucide visual language, so that the expanded widget set feels cohesive.
33. As an overlay editor user, I want custom icons to render the same in preview and export, so that I can trust what I see in the editor.
34. As an overlay editor user, I want Wave 2 metrics to work when the parser provides data and to show placeholders otherwise, so that I can still use the widgets without format-specific surprises.
35. As an overlay editor user, I want Gear Position, Vertical Ratio, and Core Temperature to follow the same standard metric widget model as Wave 1, so that future additions do not feel like special cases.
36. As an overlay editor user, I want the template format version to clearly signal the new schema, so that broken older templates fail fast instead of behaving unpredictably.
37. As a template author, I want old and new standard metric widgets to be defined by one shared system, so that the widget catalog can grow without introducing one-off behavior for each metric.
38. As an OVRLEY developer, I want the standard metric widget system to be metadata-driven, so that adding future metric widgets requires less duplicated code across frontend and backend.
39. As an OVRLEY developer, I want Left/Right Balance normalized during parsing, so that the widget layer works with a clean scalar series instead of parser-specific object shapes.
40. As an OVRLEY developer, I want the frontend preview and Rust renderer to share the same unit and placeholder rules, so that exported overlays match the editor.

## Implementation Decisions

- OVRLEY will keep explicit widget types per metric. The system will not pivot to a generic single metric widget with an arbitrary data-source selector.
- The feature is split into two waves. Wave 1 covers the already parsed or derived standard metric widgets. Wave 2 covers the standard metric widgets that require parser extraction work.
- Advanced graphical widgets are out of scope. This PRD only covers standard metric value widgets.
- A shared metadata-driven standard metric widget layer will be introduced and will cover both the existing standard metric widgets and the new standard metric widgets. This layer will not replace the specialized treatment of non-standard widget types such as Gradient and Time unless that falls out naturally.
- The metadata-driven layer should be treated as a deep module. It should encapsulate widget labels, placeholder rules, unit choices, default units, decimal-toggle eligibility, formatting modes, icon bindings, and editor behavior behind a small, stable interface.
- Standard metric widgets will continue to use explicit type ids that match the activity metric keys exactly wherever that mapping exists.
- The canonical unit-selection field for standard metric value widgets will be `display_unit`.
- `display_unit` replaces the old mixed unit-selection schema for standard metric value widgets. Legacy fields such as the generic legacy unit field and the old widget-specific unit fields are intentionally removed from the standard metric widget schema rather than preserved as compatibility aliases.
- The schema cleanup is a deliberate breaking change. Older saved templates are not preserved through runtime compatibility shims or import migration.
- The template file version will be bumped, and older template versions will be rejected on load rather than loosely accepted.
- `display_unit` is only for standard metric value widgets. Plot widgets, including elevation-related plot labeling, keep their separate schema and are not pulled into this unit-field cleanup.
- All standard metric widgets remain available in the widget drawer regardless of whether the currently loaded activity contains that metric.
- Missing telemetry for a configured widget renders a placeholder rather than conditionally hiding the widget.
- Wave 1 defaults are metric-first.
- Wave 1 default display units and formats are fixed as follows:
  `pace` -> `min/km`
  `g_force` -> `g`
  `air_pressure` -> `hPa`
  `ground_contact_time` -> `ms`
  `left_right_balance` -> `52% / 48%`
  `stride_length` -> `m`
  `stroke_rate` -> `spm`
  `torque` -> `Nm`
  `vertical_speed` -> `m/s`
- Wave 1 unit sets are shipped in full rather than as reduced first-pass subsets.
- Pace defaults to `m:ss min/km`.
- Pace supports `min/km` and `min/mi`.
- Left/Right Balance uses a single value-string layout in Wave 1 rather than a specialized split renderer.
- Left/Right Balance offers these formats: `52 / 48`, `L52 / R48`, `52% / 48%`, `52L / 48R`.
- Left/Right Balance defaults to `52% / 48%`.
- The placeholder for `left_right_balance` is format-aware and defaults to a single-string empty form such as `-- / --`.
- Left/Right Balance is normalized at parse time into a canonical left-percent scalar series. Formatter and renderer logic derive all supported display formats from that scalar.
- Left/Right Balance normalization is a second deep module opportunity. Parser-specific balance object shapes should be collapsed before the widget system sees them.
- The simple `0 / 1 decimal` editor control reuses the existing `decimals` field instead of introducing a new boolean or display-specific field.
- The `0 / 1 decimal` control is only exposed for metrics where fractional display carries signal. Pace and Left/Right Balance do not use this control. Fixed-integer metrics that do not meaningfully benefit from fractions do not use this control.
- The metrics that expose the `0 / 1 decimal` control are:
  `g_force`
  `stride_length`
  `torque`
  `vertical_speed`
- The Wave 1 metrics that do not expose the `0 / 1 decimal` control are:
  `pace`
  `air_pressure`
  `ground_contact_time`
  `stroke_rate`
  `left_right_balance`
- Final production-quality icons are part of the Wave 1 definition of done. Temporary placeholder icons are not acceptable.
- When a planned icon has a suitable Lucide source, the SVG must be extracted into a shared asset file rather than referenced only as a frontend component.
- Custom icons are only created when needed and must visually fit the Lucide icon family.
- Shared standard metric widget icons live in `assets/widget-icons/` so both the React preview renderer and the Rust export renderer read from the same source of truth.
- Wave 1 custom icons must fit the current backend SVG subset rather than expanding the backend SVG parser as part of this feature.
- The icon mapping for the planned standard metric widgets is fixed as follows:
  `pace` -> `Footprints`
  `air_pressure` -> `Wind`
  `left_right_balance` -> `Scale`
  `stride_length` -> `Ruler`
  `stroke_rate` -> `Waves`
  `vertical_speed` -> `TrendingUp`
  `vertical_ratio` -> `Percent`
  `core_temperature` -> `Thermometer`
  `g_force` -> custom
  `ground_contact_time` -> custom
  `torque` -> custom
  `gear_position` -> custom
- The shared standard metric widget icon catalog is another deep module opportunity. The rest of the system should consume icon definitions through a stable registry rather than through scattered hardcoded mappings.
- Wave 2 is allowed to be parser-source-dependent. If a metric is available in FIT but not in most GPX activities, the widget still exists and renders a placeholder when the activity lacks the metric.
- Wave 2 metrics are `gear_position`, `vertical_ratio`, and `core_temperature`.
- Wave 2 default display units are fixed as follows:
  `gear_position` -> unitless
  `vertical_ratio` -> `%`
  `core_temperature` -> `°C`
- Existing standard metric widgets should be folded into the new metadata-driven path so OVRLEY does not end up with two standard metric systems side by side.
- The existing standard metric widgets that must be covered by the shared metadata-driven path are:
  `speed`
  `heartrate`
  `cadence`
  `power`
  `temperature`
- `time` and `gradient` may remain specialized paths rather than being forced into the shared standard metric widget path.
- The implementation should preserve the current explicit distinction between standard metric value widgets and plot widgets. The refactor expands the standard metric value widget family; it does not collapse the entire widget system into one universal abstraction.

## Testing Decisions

- Good tests should verify external behavior and stable contracts rather than internal implementation details. That means asserting formatted output, schema validation, render-data requirements, placeholder behavior, icon asset compatibility, template version rejection, and parser normalization results rather than the exact internal branching structure.
- The shared standard metric widget metadata layer should be tested as a stable contract. Tests should cover unit choices, default display units, decimal-toggle availability, placeholder behavior, and balance-format configuration for representative existing and new standard metric widgets.
- Activity parsing and metric normalization should be tested for the new parser-time balance normalization and for any Wave 2 parser extraction additions. The important behavior is the shape and meaning of the emitted metric series, not the internal parser plumbing.
- Frontend standard metric formatting and preview-model behavior should be tested for Pace, Left/Right Balance, unit conversion, placeholder output, and decimal control behavior.
- Backend config and rendering-format behavior should be tested for the expanded `MetricKind` coverage, `display_unit` handling, template version rejection, render-data requirement derivation, placeholder formatting, and standard metric display-part output.
- Backend value-widget rendering should be tested with focused behavior checks for representative new standard metric widgets and with baseline render coverage for at least one text-heavy metric, one unit-converting metric, and Left/Right Balance.
- Shared metric icon handling should be tested through asset compatibility and rendering behavior, with emphasis on confirming that the canonical shared SVG assets render consistently in both preview and export.
- Prior art for frontend tests already exists in the widget drawer and store tests using Vitest and Testing Library. Similar behavior-first tests should be used for the standard metric widget editor and preview logic.
- Prior art for backend tests already exists in metric formatting tests, config tests, activity tests, value widget tests, metric kind serde tests, and baseline render tests. The new work should extend those patterns rather than inventing a different testing style.
- Manual verification should still be part of signoff for icon appearance, widget editor ergonomics, preview-to-export parity, and the intentional template-version rejection behavior.

## Out of Scope

- Advanced graphical widgets, including heading or compass-style widgets and vector G-meter widgets
- Conditional widget visibility based on activity coverage
- A generic metric widget with an arbitrary metric selector
- Runtime migration support for older templates
- Schema cleanup for plot-widget unit systems
- A specialized split renderer for Left/Right Balance in Wave 1
- Search, categorization, or filtering inside the widget drawer
- Changes to non-standard widget families beyond the minimum integration needed for the shared standard metric widget layer

## Further Notes

- This PRD intentionally supersedes the earlier idea of strict backwards compatibility for existing templates. The team has explicitly chosen a schema break, a template version bump, and fast rejection of older template versions.
- The current project docs about testing are stale. The codebase already has both frontend Vitest coverage and Rust test coverage, so this feature should expand automated tests instead of assuming manual-only verification.
- The implementation should be planned so that Wave 1 is independently shippable even if Wave 2 parser extraction work lands later.
