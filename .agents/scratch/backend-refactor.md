/*
Backend Refactor Plan: Metric Presentation Resolution
=====================================================

Goal
----

Refactor the Rust backend so metric widget rendering follows the same shape as
the frontend:

1. Parse raw incoming widget config
2. Normalize and resolve one active presentation at the backend boundary
3. Render from fully normalized active config

The backend should optimize for:

- aggressive normalization at the boundary
- one centralized default-resolution seam
- manifest-driven shared defaults wherever possible
- minimal "should never happen" safety guards in renderers
- no scattered business-logic fallbacks in draw/prepare modules


Preferred Architecture
----------------------

The backend should converge on one deep metric-presentation seam:

- raw config parsing remains permissive
- one backend resolver validates + normalizes + seeds defaults
- prepare/draw modules consume resolved structs only

Preferred resolved model:

- `ResolvedMetricWidgetConfig`
- `ResolvedMetricPresentationConfig`
- presentation-specific resolved structs such as:
  - `ResolvedTextMetricConfig`
  - `ResolvedHeadingTapeConfig`

`type` / `MetricKind` selects the data source.
`display_type` selects the presentation.

Route and elevation remain separate true graphical widgets and should not be
pulled into this metric-presentation contract.


Current Backend Gaps To Eliminate
---------------------------------

Today the backend is closer to:

- permissive parse into `ValueConfig`
- ad hoc conversion into other config types via JSON round-trip
- draw-time fallback chains
- prepare-time special-casing by metric kind + `display_type`
- render-time fallback from boxed presentation back to text

Concrete examples:

- `ValueConfig::to_heading_widget_config()` in
  `src-tauri/ovrley_core/src/config/mod.rs`
- `prepare_render_assets()` special-casing heading tape in
  `src-tauri/ovrley_core/src/render/widgets/mod.rs`
- raw `ValueConfig` dispatch in
  `src-tauri/ovrley_core/src/render/widgets/metric_presentation.rs`
- boxed-presentation fallback-to-text behavior in
  `src-tauri/ovrley_core/src/render/mod.rs`
- inline defaults in `heading/prepare.rs`

Those should be replaced by one normalization/resolution boundary.


Normalization Boundary
----------------------

Create one backend-owned resolver seam that runs before draw code.

That seam should do all of the following:

- parse raw config shape
- normalize missing `display_type` to `text`
- validate `display_type` against `MetricKind`
- resolve the active metric presentation
- read active variant config from `display_variants[display_type]` for boxed presentations
- seed missing presentation-specific fields from centralized defaults
- apply manifest frame defaults
- resolve shared inherited fields
- produce a fully normalized active config struct

This seam should be the only place that owns normal business defaults.


Default Ownership
-----------------

Shared manifest defaults from `assets/standard-metrics.json`
-------------------------------------------------------------

These should remain owned by the shared contract:

- allowed `display_type` values per metric kind
- `layoutMode`
- boxed vs intrinsic semantics
- default frame width
- default frame height

The backend should keep using `src-tauri/ovrley_core/src/standard_metrics.rs`
as the adapter over that manifest.

Backend-local defaults
----------------------

These should live in one backend defaults/resolver seam for now:

- heading tape tick defaults
- heading tape label defaults
- heading tape indicator defaults
- any presentation-specific non-geometry defaults not yet formalized in the shared contract

Examples that should move out of renderers:

- heading tape fallback tick colors
- default label font size
- default label offset
- default indicator style/placement
- default indicator size

Principle:

- shared manifest owns presentation contract semantics
- backend resolver owns presentation-specific runtime defaults


What Should Remain Defensive
----------------------------

Keep these checks:

- early validation for malformed input
- early validation for unsupported `display_type` / metric-kind combinations
- early validation for backend-unimplemented presentations
- minimal "should never happen" guards where internal safety matters
- cache/type mismatch guards if a corrupted internal state would otherwise panic

Do not keep:

- draw-time `unwrap_or` chains for ordinary config values
- prepare-time repeated "if missing use X" defaults
- fallback from boxed presentation to text in normal control flow
- repeated metric-kind / presentation validation in renderers


Concrete Module Plan
--------------------

1. `assets/standard-metrics.json`
---------------------------------

No major ownership change needed.

Keep this file as the source of truth for:

- display type vocabulary
- layout mode
- default frame dimensions
- supported display types per metric kind

Do not move heading-tape-specific tick/indicator defaults here yet unless the
frontend also adopts them as shared cross-layer presentation contract values.


2. `src-tauri/ovrley_core/src/standard_metrics.rs`
--------------------------------------------------

Keep and extend this as the shared display-type contract adapter.

Existing helpers already point in the right direction:

- `display_type_definition`
- `display_type_layout_mode`
- `default_frame_dimensions`
- `supported_display_types`
- `is_display_type_supported`

Recommended additions:

- one helper that returns a resolved display contract for `(MetricKind, display_type)`
- one helper that distinguishes:
  - known display type
  - allowed-for-metric
  - boxed/intrinsic
  - default frame dimensions

Reason:

The new resolver should not need to manually combine 3-4 helper calls and
re-encode policy each time.


3. `src-tauri/ovrley_core/src/types.rs`
---------------------------------------

Change `DisplayType` deserialization behavior.

Current problem:

- unknown strings deserialize directly to `DisplayType::Text`

That is convenient for backward compatibility, but it hides malformed or
unsupported input before the normalization boundary can validate it properly.

Preferred change:

- keep `DisplayType` as the canonical enum
- stop using it as the raw parse type for incoming widget config

Instead:

- parse raw `display_type` as `Option<String>` in a raw DTO
- normalize missing / `null` -> `"text"`
- reject unknown strings in the resolver with a clear config error

Important distinction:

- missing `display_type` should still normalize to text
- malformed or unknown explicit values should not silently become text anymore


4. `src-tauri/ovrley_core/src/config/mod.rs`
--------------------------------------------

This file needs the biggest contract split.

Current issues:

- `ValueConfig` is too close to renderer needs
- `ValueConfig::to_heading_widget_config()` uses a JSON round-trip
- `HeadingWidgetConfig` currently owns many runtime defaults directly via serde defaults
- the backend config model does not understand the frontend hybrid shape with
  `display_variants`

Preferred changes:

Introduce raw config DTOs:

- `RawValueConfig`
- `RawMetricDisplayVariants`
- `RawHeadingTapeConfig` or equivalent presentation-specific raw struct

Raw DTO requirements:

- permissive enough to parse the frontend hybrid storage shape
- presentation-specific fields optional
- `display_type` parsed as optional string, not final enum
- boxed frame geometry can be read from either top level or active variant

Introduce resolved config types:

- `ResolvedMetricWidgetConfig`
- `ResolvedMetricPresentationConfig`
- `ResolvedHeadingTapeConfig`

Remove or deprecate:

- `ValueConfig::to_heading_widget_config()`

Do not serialize one config into JSON just to deserialize into another typed
config. That is exactly the seam the resolver should replace.

Shift ownership:

- raw config types describe incoming shape
- resolved config types describe renderer input


5. New module: `src-tauri/ovrley_core/src/render/widgets/metric_presentation/resolve.rs`
-----------------------------------------------------------------------------------------

Create a dedicated metric-presentation resolver module.

Recommended API:

- `resolve_metric_widget_config(raw: &RawValueConfig, scene: &SceneConfig) -> CoreResult<ResolvedMetricWidgetConfig>`

Responsibilities:

- normalize missing display type to text
- validate display type is known
- validate display type is allowed for this metric kind
- validate backend supports this presentation today
- determine whether the presentation is intrinsic or boxed from the shared manifest
- resolve active variant config for non-text presentations
- apply manifest frame defaults for boxed layouts
- apply backend-local non-geometry defaults
- resolve shared inherited values such as opacity / scale / fonts where needed
- produce one concrete resolved active presentation

This module should mirror the role played by the frontend resolver in:

- `app/src/lib/metric-widget-resolver.js`

Backend ownership should conceptually match frontend ownership:

- shared contract for frame semantics
- local resolver for presentation-specific defaults
- one resolved active shape for render consumers


6. New module: `src-tauri/ovrley_core/src/render/widgets/metric_presentation/defaults.rs`
------------------------------------------------------------------------------------------

Create one backend-owned defaults seam for presentation-specific non-geometry defaults.

This should own defaults for:

- heading tape tick intervals
- minor/major tick visibility
- tick lengths and thickness
- label visibility
- label offsets
- indicator style
- indicator placement
- indicator visibility
- indicator size
- any other heading-tape-specific non-geometry defaults

Reason:

These defaults currently leak through serde defaults and inline renderer
fallbacks. They should become explicit backend-owned defaults invoked only by
the resolver.


7. `src-tauri/ovrley_core/src/render/widgets/mod.rs`
----------------------------------------------------

Refactor `prepare_render_assets()` so it works from resolved metric widget
configs instead of raw `ValueConfig`.

Preferred flow:

- parse / resolve metric widgets once up front
- store resolved metric widget configs in prepared assets
- prepare any presentation caches from resolved presentation configs

Replace current special case:

- `if value.value == MetricKind::Heading && value.display_type == DisplayType::Tape`

with:

- match on `ResolvedMetricPresentationConfig`

Example:

- `ResolvedMetricPresentationConfig::HeadingTape(resolved)` ->
  prepare heading tape cache

Route and elevation remain unchanged here except as architectural reference:

- normalize first
- prepare from normalized
- draw from normalized


8. `src-tauri/ovrley_core/src/render/widgets/types.rs`
------------------------------------------------------

Extend prepared render state to store resolved metric widget information.

Recommended additions:

- `resolved_metric_widgets: Vec<ResolvedMetricWidgetConfig>`

Potential future direction:

- key presentation caches by stable widget identity if available
- keep index-based mapping for now if that aligns with current report plumbing

Also consider whether `HeadingWidgetCache` should keep only draw/runtime data,
not duplicated business defaults already guaranteed by resolved config.


9. `src-tauri/ovrley_core/src/render/widgets/metric_presentation.rs`
--------------------------------------------------------------------

Refactor this file into a strict resolved-presentation dispatcher.

Current issue:

- it dispatches over raw `ValueConfig`
- it returns `None` for many normal business cases
- it re-checks state that should have been validated earlier

Preferred direction:

- dispatch over `ResolvedMetricPresentationConfig`
- boxed presentation draw entrypoint should assume it got a valid resolved config

Recommended shape:

- `prepare_metric_presentation_cache(...)`
- `draw_metric_presentation(..., resolved: &ResolvedMetricWidgetConfig, ...)`

The dispatcher should stop encoding policy like:

- "non-heading heading_tape returns None"
- "text cache returns None"
- "future boxed type returns None"

Those should become boundary-level validation / implementation-support errors,
not normal runtime branches in draw logic.


10. `src-tauri/ovrley_core/src/render/widgets/heading/prepare.rs`
-----------------------------------------------------------------

Refactor to consume `ResolvedHeadingTapeConfig`.

Move these defaults out of prepare logic:

- fallback tick colors
- fallback label colors
- default font size
- default label offset
- default indicator color
- default indicator size

After refactor, this file should do:

- tape-surface allocation
- geometry calculations
- font resolution
- static tape rendering
- cache construction

It should not own business fallback/default policy.


11. `src-tauri/ovrley_core/src/render/widgets/heading/draw.rs`
--------------------------------------------------------------

Keep this module focused on compositing and indicator drawing.

It should assume:

- the presentation is resolved
- the cache matches the resolved presentation
- required fields already exist

Allow only minimal safety checks such as:

- corrupted cache shape
- invalid tape width / degenerate geometry

It should stop acting as a normal fallback layer.


12. `src-tauri/ovrley_core/src/render/widgets/value/mod.rs`
------------------------------------------------------------

Current problem:

- intrinsic text and boxed presentation routing are entangled
- boxed values are "handled" here but not drawn here

Preferred direction:

- keep this module for intrinsic text metric rendering
- boxed presentations should bypass this module after resolution

Desired behavior:

- `ResolvedMetricPresentationConfig::Text(...)` -> handled here
- boxed resolved presentations -> handled by metric presentation pipeline

This module should stop being a half-dispatch layer for boxed render paths.


13. `src-tauri/ovrley_core/src/render/mod.rs`
---------------------------------------------

This is the main runtime behavior shift.

Current behavior:

- collect boxed values
- attempt boxed draw
- fall back to generic text if boxed renderer returns `None`

Preferred behavior:

- frame rendering operates on already-resolved metric widget configs
- text presentations draw on the intrinsic metric path
- boxed presentations draw on the presentation path
- malformed or unsupported boxed presentations should fail before frame drawing
- valid-but-not-yet-implemented presentations should fail in resolver/preparation,
  not silently render as text

Opinionated recommendation:

Remove render-time business fallback from boxed presentation to text.

If a user selected `heading_tape`, `linear`, `bars`, `arc`, or `corner`, the
backend should either:

- render that presentation correctly, or
- fail clearly before draw

It should not silently choose a different presentation.


14. `src-tauri/ovrley_core/src/render/widgets/route/normalize.rs`
-----------------------------------------------------------------

Use route normalization as the model.

Route already demonstrates the target architecture:

- normalize once
- prepare from normalized
- draw from normalized

Metric presentations should gain an equivalent path:

- resolve once
- prepare from resolved
- draw from resolved

No large route changes are required; this module is mainly a reference point for
how deep normalization should simplify callers.


15. `src-tauri/ovrley_core/src/render/widgets/route/prepare.rs`
----------------------------------------------------------------

No main behavioral change required.

Keep using it as a reference for:

- "expensive work happens after normalization"
- "prepare assumes normalized config"

The metric-presentation refactor should try to feel as clean as route prepare
feels today.


16. `src-tauri/ovrley_core/src/render/widgets/route/draw.rs`
-------------------------------------------------------------

No main behavioral change required.

Keep using it as a reference for:

- draw receiving already-prepared, normalized data
- no repeated ownership of defaults in draw code


Behavioral Rules After Refactor
-------------------------------

What backend code should do:

- normalize aggressively at the boundary
- centralize defaults in one resolver/defaults seam
- consume one resolved active presentation
- derive boxed/intrinsic semantics from the shared manifest
- reject malformed or unsupported input early

What backend code should stop doing:

- serializing `ValueConfig` into JSON to build other configs
- keeping presentation defaults inside serde defaults and draw modules simultaneously
- branching on raw `MetricKind` + `display_type` all over the render path
- silently falling back from boxed presentation to text
- duplicating frame-default logic across prepare/draw modules
- re-validating ordinary business cases inside renderers


Testing Plan
------------

1. Shared contract tests
------------------------

Files:

- `src-tauri/ovrley_core/tests/standard_metrics_display_type_tests.rs`

Add / keep tests proving:

- display type definitions load from manifest
- allowed display types per metric kind are correct
- boxed/intrinsic semantics are manifest-driven
- boxed frame defaults come from the manifest


2. Raw parse / boundary validation tests
----------------------------------------

Files:

- `src-tauri/ovrley_core/tests/display_type_tests.rs`
- new resolver tests file if needed

Change expectations:

- omitted `display_type` normalizes to text
- `display_type: null` normalizes to text
- explicit unknown `display_type` should fail resolver validation
- explicit unsupported `display_type` for a metric kind should fail validation
- valid manifest display type that backend does not implement yet should fail
  at resolver/preparation boundary, not silently render as text


3. Resolver tests
-----------------

Add new unit/integration tests around the new resolver seam.

Required cases:

- resolves text presentation without touching boxed defaults
- resolves heading tape active presentation from `display_variants.heading_tape`
- applies manifest width/height defaults when boxed geometry is missing
- prefers active variant geometry over top-level geometry when appropriate
- seeds heading-tape-specific non-geometry defaults centrally
- preserves shared top-level fields like `x`, `y`, `opacity`, `value`, `display_type`
- rejects malformed variant payloads clearly


4. Heading preparation tests
----------------------------

Files:

- existing heading tests
- possibly new resolver + heading prepare tests

Add tests proving:

- `heading/prepare.rs` consumes resolved config
- no inline default fallback is required for ordinary fields
- prepared cache reflects resolved dimensions and resolved presentation fields


5. Metric presentation dispatch tests
-------------------------------------

Files:

- `src-tauri/ovrley_core/tests/metric_presentation_tests.rs`

Update these tests to match the new architecture:

- dispatch over resolved presentation config, not raw `ValueConfig`, where possible
- heading tape renders successfully from resolved config
- multiple resolved boxed widgets preserve identity in reports
- impossible-state guards are tiny and internal, not business fallback

Tests that should be removed or rewritten:

- assertions that future boxed display types simply return `None`
- assertions that normal boxed render failures silently degrade to text


6. Full render integration tests
--------------------------------

Add preview/render tests for:

- heading text renders through intrinsic text path
- heading tape renders through boxed metric presentation path
- hybrid storage shape with `display_variants` resolves correctly before render
- unsupported boxed presentation fails before frame draw
- omitted boxed width/height uses manifest defaults


Migration / Sequencing Plan
---------------------------

Recommended order of implementation:

1. Introduce raw DTOs and resolved config types
2. Add the new metric-presentation resolver and defaults modules
3. Update config parsing to preserve unknown explicit display types for validation
4. Refactor heading tape to resolve through the new seam
5. Update `prepare_render_assets()` to prepare from resolved presentations
6. Update frame rendering to consume resolved presentations and remove boxed->text fallback
7. Update tests to assert early validation and resolved rendering behavior
8. Only then extend the same seam to future boxed metric presentations

This sequencing keeps heading tape as the first real boxed presentation while
building the generalized backend seam for `linear`, `bars`, `arc`, and `corner`.


Opinionated Answers To The Design Questions
-------------------------------------------

Where should fallback values come from?

- from exactly two places:
  - the shared manifest for shared presentation contract values
  - one backend resolver/defaults seam for backend-local presentation defaults

Which defaults should come from the shared JSON manifest?

- allowed display types per metric kind
- layout mode
- boxed vs intrinsic semantics
- default frame width
- default frame height

Which defaults, if any, must remain backend-local for now?

- heading-tape-specific tick defaults
- heading-tape-specific label defaults
- heading-tape-specific indicator defaults
- any other presentation-specific non-geometry defaults not yet formalized in
  the shared contract

How should backend ownership mirror frontend ownership?

- one shared manifest contract
- one backend active-presentation resolver
- one backend-owned presentation-defaults seam
- renderers that consume resolved active presentation only

How should backend rendering move toward consuming one resolved active presentation?

- resolve widget config before prepare
- store resolved metric widgets in prepared assets
- prepare caches from resolved presentation configs
- draw from resolved presentation configs
- remove raw mixed-config branching from renderers


Success Criteria
----------------

The refactor is complete when:

- renderers consume resolved active presentation structs
- heading tape defaults no longer live in scattered fallback chains
- manifest frame defaults are applied centrally
- backend validation rejects unsupported/malformed presentation input early
- render modules stop silently choosing text when a boxed presentation was requested
- route/elevation remain separate and unaffected
- adding a future boxed metric presentation means:
  - add shared manifest support if needed
  - add backend defaults if needed
  - add one resolved presentation variant
  - plug into one metric-presentation seam
  - no scattered fallback edits across draw/prepare modules
*/
