Status: ready-for-agent

# Formalize `display_type` Across Metric Widgets

## Problem Statement

OVRLEY is moving toward a widget model where one metric can be shown in multiple visual presentations selected by the user through `display_type`. The current implementation already hints at this direction, but the contract is still shallow and inconsistent across the editor, preview, backend renderer, and stored widget config.

Today, `display_type` exists, but its meaning is interpreted in several ad hoc ways:

- The shared standard-metric manifest defines which display types exist and which metric kinds allow them.
- The frontend uses a mix of widget `type`, `display_type`, and special-case helpers to decide editor behavior, preview routing, and resize semantics.
- The backend has a `DisplayType` enum, but metric rendering is still split between the value-widget path and separate specialized widget paths.
- The metric widget config shape is flat, so display-specific fields for one presentation are mixed into the same config record as fields for other presentations.

This works for the first experimental boxed presentation, but it will become increasingly brittle as more boxed metric presentations arrive (`linear`, `bars`, `arc`, `corner`, and future additions). The current design relies on inactive fields being ignored by whichever presentation is active, which weakens locality, makes defaults ambiguous, complicates normalization, and creates multiple seams where `display_type` can drift semantically between frontend and backend.

The user wants `display_type` to become the formal presentation contract for metric widgets, without introducing unnecessary abstraction, without preserving legacy/old template behavior, and with a clear strategy for how per-presentation config should be stored and consumed.

## Solution

Formalize `display_type` as the single presentation selector for metric widgets across the shared manifest, the frontend, the backend, and the stored widget config.

The implementation keeps the mental model deliberately simple:

- `type` tells the system which telemetry series the widget represents.
- `display_type` tells the system how that metric is presented.

The formalization has three coordinated parts:

1. **Shared `display_type` contract**
   - Extend the shared display-type manifest so each display type is defined as a real presentation contract, not just a label string.
   - At minimum, each display type declares whether it is `intrinsic` or `boxed`, and may optionally define default frame dimensions for boxed presentations.
   - Both frontend and backend consume this manifest-derived definition rather than rediscovering `display_type` semantics in local helpers.

2. **Frontend presentation formalization**
   - Replace ad hoc special-casing with display-type-driven editor, preview, and overlay behavior.
   - Move from a flat widget config to a nested per-display config model that preserves state for each display type without clearing inactive variants on switch.
   - Introduce one resolver that materializes the active metric widget config for preview and export, so render layers consume one active presentation rather than the nested storage shape.

3. **Backend presentation formalization**
   - Make `DisplayType` the dispatch seam for metric widget rendering.
   - Keep route and elevation as distinct non-metric graphical widgets.
   - Move boxed metric presentations toward a unified metric rendering path rather than allowing each new display type to escape into separate specialized render flows.

This keeps the user-facing model minimal while making the implementation deep and testable:

- One persisted selector: `display_type`
- One source-of-truth contract for what each display type means
- One storage model that preserves per-display settings
- One active-config resolver for render consumers
- One backend metric presentation dispatch seam

## User Stories

1. As an overlay editor user, I want one metric widget to support multiple visual presentations through `display_type`, so that I can switch how a metric is shown without creating separate widget families.
2. As an overlay editor user, I want the meaning of each `display_type` to be consistent across the editor, preview, and export, so that the same presentation behaves the same everywhere.
3. As an overlay editor user, I want `text` to remain the standard intrinsic metric presentation, so that classic value widgets keep their expected behavior.
4. As an overlay editor user, I want boxed presentations such as `heading_tape`, `linear`, `bars`, `arc`, and `corner` to behave as boxed widgets, so that they can be resized like bounded visual frames rather than intrinsic text blocks.
5. As an overlay editor user, I want the widget editor to route by `display_type`, so that changing the presentation changes the relevant editing controls automatically.
6. As an overlay editor user, I want the overlay preview to route by `display_type`, so that switching a metric to a boxed presentation renders the corresponding visualization instead of a text fallback.
7. As an overlay editor user, I want render/export to route by `display_type`, so that the exported output uses the same presentation contract as the editor.
8. As an overlay editor user, I want each metric kind to advertise only the display types it supports, so that I never see unsupported presentation choices in the editor.
9. As an overlay editor user, I want the allowed display types to come from the shared manifest, so that the frontend and backend agree on presentation availability.
10. As an overlay editor user, I want the behavior of each display type to be defined centrally, so that presentation semantics do not depend on scattered conditionals.
11. As an overlay editor user, I want boxed versus intrinsic behavior to be a formal property of the display type, so that resize and layout rules are derived from the active presentation rather than inferred indirectly.
12. As an overlay editor user, I want per-display settings to be preserved when I switch between presentations, so that returning to a previous presentation restores the settings I already tuned.
13. As an overlay editor user, I do not want the app to clear my old display-specific settings automatically when I switch display type, so that variant switching is non-destructive.
14. As an overlay editor user, I want each display type to initialize its own defaults the first time I switch to it, so that every presentation starts from a sensible baseline.
15. As an overlay editor user, I want reset behavior to be explicit, so that I can reset the current display config without losing all other display-specific settings for the widget.
16. As an overlay editor user, I want common metric settings to stay shared across presentations, so that source-data choices and truly shared styling do not need to be reconfigured per variant.
17. As an overlay editor user, I want display-specific settings to live under their own display-specific config, so that tape, arc, bar, and text settings do not pollute each other.
18. As an overlay editor user, I want position and scene placement to remain stable while I switch presentations, so that I can explore variants without the widget jumping unexpectedly.
19. As an overlay editor user, I want boxed presentations to retain width, height, and other frame settings when I switch away and back, so that layout tuning is preserved.
20. As an overlay editor user, I want text presentations to retain typography and icon settings when I switch away and back, so that I do not lose the text variant I already designed.
21. As an overlay editor user, I want a clean separation between stored nested presentation config and active render config, so that the editor can preserve all variants while preview/export consume only the active one.
22. As an overlay editor user, I want the active metric widget config to be resolved in one place before preview/export, so that all render consumers receive the same normalized shape.
23. As an overlay editor user, I want the same display-type contract to support current heading tape work and future gauge presentations, so that the architecture scales without repeating the same refactor per variant.
24. As an overlay editor user, I want route and elevation to remain independent graphical widgets, so that true non-metric plots are not forced into the metric presentation model.
25. As an overlay editor user, I want metric presentations to remain metric widgets regardless of whether they are intrinsic or boxed, so that the system does not blur scalar metric widgets with true plot widgets.
26. As a developer, I want the frontend to stop depending on heuristics like “non-text means plot-like”, so that new display types do not require fragile special cases in multiple modules.
27. As a developer, I want the shared standard-metric contract to include presentation metadata beyond labels, so that the meaning of a display type is available to every layer that needs it.
28. As a developer, I want the backend metric renderer to dispatch on `DisplayType`, so that each new metric presentation can plug into the same metric rendering seam.
29. As a developer, I want the backend to stop using “return true but render nothing here” as a long-term presentation escape hatch, so that presentation ownership remains localized.
30. As a developer, I want the nested config shape to clarify which settings are common and which belong only to one presentation, so that the schema is easier to reason about and validate.
31. As a template author working in the current dev-only phase, I want the final schema to be explicit and stable, so that templates produced after this refactor have a clear long-term structure.
32. As a developer, I want one formal resolver from nested widget storage to active render config, so that editor state, preview state, and export state do not drift semantically.
33. As a developer, I want defaults to be defined per display type rather than mixed into type-wide defaults, so that adding `linear`, `bars`, `arc`, or `corner` does not enlarge one flat config bag indefinitely.
34. As a developer, I want display-type normalization and validation to be shared between frontend and backend conceptually, so that unsupported display types fail predictably and valid ones behave identically.
35. As a developer, I want the architecture to preserve the simple product story—`type` is the data, `display_type` is the presentation—so that future implementation work does not overcomplicate the user model.

## Implementation Decisions

### Core Contract

- `display_type` remains the single presentation selector for metric widgets.
- `type` remains the metric/data-source selector.
- No additional persisted selector is introduced for preview/editor/backend dispatch.
- The architecture must not add unnecessary abstraction layers that duplicate the role already played by `display_type`.

### Shared Display-Type Manifest

- The shared standard-metric manifest already owns:
  - the vocabulary of display types
  - the label for each display type
  - the default set of display types available to metric widgets
  - per-metric overrides restricting which display types are supported
- This manifest will be extended so each display type has a formal definition, not only a label.
- The display-type definition must include:
  - a user-facing label
  - a layout mode describing whether the presentation is `intrinsic` or `boxed`
  - optional default frame geometry for boxed presentations
- The manifest remains the shared source of truth consumed by both frontend and backend.

### Frontend Formalization

- The frontend must stop treating `display_type` as a loose string interpreted differently across modules.
- One central display-type definition module will expose the semantic meaning of each display type.
- All display-type-driven behavior must derive from this definition module rather than ad hoc conditionals.

This central definition layer is expected to answer at least:

- what label a display type uses
- whether the display type is intrinsic or boxed
- which default frame geometry a boxed display type should use
- which display types are allowed for a given metric kind

### Frontend Dispatch Rules

- Editor behavior must dispatch primarily by `display_type`.
- Preview behavior must dispatch primarily by `display_type`.
- Overlay resize/layout behavior must dispatch primarily by `display_type`.
- The frontend should stop modeling boxed metric presentations as “plot-like” in a heuristic sense.
- Instead, boxed versus intrinsic behavior is derived from the formal display-type contract.

Expected interpretation:

- `text` is intrinsic.
- `heading_tape`, `linear`, `bars`, `arc`, and `corner` are boxed.

### Frontend Storage Model

- The flat metric widget config shape is no longer sufficient for multiple display types.
- The widget config will be reworked so each metric widget stores:
  - shared/common metric data
  - the active `display_type`
  - a nested per-display config record keyed by display type
- This nested structure will preserve separate config for each display type instead of mixing all presentation-specific fields into one flat bag.

### Frontend Common vs Display-Specific Ownership

- A field belongs in shared/common config only if it has the same meaning across presentations.
- Fields that are only meaningful for a single presentation must be nested under that presentation’s config.

Expected shared/common ownership includes:

- metric binding
- active `display_type`
- scene position
- widget opacity
- shared unit selection when it truly applies across presentations
- any prefix/suffix only if they remain semantically shared across all supported presentations

Expected display-specific ownership includes:

- text typography, icon, decimals, and unit text styling under text presentation config
- width, height, rotation, and other frame geometry under boxed presentation config
- presentation-specific geometry and styling under each boxed presentation config

### Frontend Defaults

- Shared metric defaults remain separate from display-specific defaults.
- Display-specific defaults must be owned per display type rather than mixed into type-wide defaults.
- A metric widget initializes the active display config from the corresponding display-type defaults when that display is first selected.
- The current pattern of mixing tape settings into the same default record as text metric settings is not the target design.

### Frontend Variant Switching

- Switching `display_type` must be non-destructive.
- The app must not clear the outgoing display config when the user switches to a new presentation.
- When switching to a new presentation:
  - activate the new `display_type`
  - create that display’s nested config only if it does not already exist
  - preserve all previously edited configs for other display types
- Reset remains an explicit user action.
- The product may later expose:
  - reset current display config
  - reset all display configs for a widget
- Automatic clearing during switch is not acceptable.

### Active Metric Widget Resolver

- Frontend preview and export-facing code should not consume the nested storage shape directly.
- Introduce one resolver that materializes the active metric widget config from:
  - common widget config
  - active `display_type`
  - nested config for that display type
- This resolver becomes the seam between editor storage and render consumption.
- All preview and export preparation logic should consume the resolved active metric widget shape.

### Frontend Widget Editor Composition

- The generic metric editor path remains the right home for the shared metric controls.
- `text` presentation continues to use the generic metric editor controls.
- Boxed presentations contribute only their own presentation-specific editor sections.
- The editor should not duplicate the generic metric editor logic inside each metric-specific presentation editor.

### Frontend Preview Model

- The intrinsic text preview model remains the text/metric presentation path.
- Boxed metric presentations should bypass the intrinsic metric preview model and instead use their own presentation-specific preview path.
- This bypass should be driven by formal display-type meaning, not by type-specific heading exceptions.

### Backend Formalization

- The backend `DisplayType` enum remains the presentation selector for metric widgets.
- The backend must formalize metric presentation rendering around:
  - metric/data selection by `MetricKind`
  - presentation selection by `DisplayType`
- Route and elevation remain distinct non-metric graphical widgets.
- Metric presentations must not be modeled as separate widget families once generalized boxed metric presentations arrive.

### Backend Render Dispatch

- The long-term backend render seam for metric widgets is the metric rendering pipeline dispatching by `DisplayType`.
- The backend should stop relying on a special case where the value-widget path acknowledges a presentation but does not own rendering it.
- The current heading-tape path is acceptable as a first step, but not as the long-term pattern for future metric presentations.
- Boxed metric presentations should ultimately live behind one metric presentation dispatch seam rather than each creating its own top-level render escape hatch.

### Backend Shared Contract Consumption

- The backend standard-metric manifest loader already exposes the allowed display types and labels.
- It will be extended to expose the behavioral meaning of display types needed by backend rendering.
- At minimum, backend consumers should be able to derive:
  - whether a display type is intrinsic or boxed
  - any default frame geometry the presentation implies
- The backend must continue validating whether a display type is permitted for a given metric kind from the shared manifest contract.

### Backend Config Shape

- The backend config model should move toward consuming the active metric presentation rather than proliferating one specialized config struct per boxed metric presentation.
- This does not require every renderer to understand nested frontend storage.
- The backend should receive or derive the resolved active metric widget config, not the full nested presentation state for inactive variants.
- The long-term backend design keeps the storage model and active render model distinct:
  - storage model may preserve all display variants
  - runtime render model consumes only the active resolved presentation

### Relationship to Existing Graphical Widgets

- Route and elevation remain true separate graphical widgets outside the metric-presentation system.
- Metric presentations are not “plots” even when they are boxed.
- The system should stop overloading plot semantics for boxed metric presentations.
- The intended distinction is:
  - metric widget with intrinsic presentation
  - metric widget with boxed presentation
  - true non-metric route/elevation graphical widgets

### No Legacy or Backward Compatibility Requirement

- This PRD assumes there is no need to preserve old or previously shipped schemas for this work.
- The implementation may perform a clean schema break or one-time migration in the dev branch.
- The design should optimize for the long-term presentation model, not for preserving temporary intermediate shapes.

### Major Modules to Build or Modify

- Shared standard-metric manifest and manifest readers, so display-type definitions become formalized rather than label-only.
- A frontend display-type definition module, so presentation meaning is centralized.
- Metric widget defaults and template normalization modules, so defaults and durable keys are display-type-owned instead of mixed into one flat record.
- A nested metric widget config resolver module, so preview/export consume one active presentation shape.
- Widget editor dispatch and presentation editor modules, so generic metric editing and display-specific editing are composed cleanly.
- Widget preview dispatch and presentation preview modules, so preview routing depends on the active display type.
- Overlay interaction/layout modules, so resize and layout behavior depend on boxed versus intrinsic presentation semantics.
- Backend standard-metric manifest support, so backend render logic can consume the same display-type contract.
- Backend metric rendering dispatch, so all metric presentations move toward one `DisplayType`-driven render seam.

### Deep Modules to Prefer

- A **display-type definition module** that encapsulates label, boxed/intrinsic semantics, and default frame behavior behind a small interface.
- A **metric widget resolver module** that turns nested widget storage into one active render-ready config.
- A **backend metric presentation dispatcher** that hides display-type-specific rendering behind one stable metric-rendering seam.

These modules should be deep:

- callers ask simple questions
- the module owns the behavioral meaning
- future display-type additions extend the definition/adapter tables rather than multiplying conditionals in callers

## Testing Decisions

- Good tests should verify externally visible behavior and contract boundaries, not internal implementation details.
- The test surface is the formal presentation contract: given a metric kind and `display_type`, the system should expose the right editor behavior, preview behavior, stored defaults, and backend render dispatch behavior.

### Frontend Tests

- Test the shared display-type definition behavior:
  - label lookup
  - allowed display types per metric kind
  - intrinsic versus boxed semantics
  - default frame behavior for boxed presentations
- Test nested metric widget default creation:
  - new metric widget starts with the correct default active presentation
  - first switch to a boxed presentation seeds that display config from the correct defaults
  - switching back restores the prior display config rather than recreating it
- Test variant switching behavior:
  - changing `display_type` does not clear prior display-specific config
  - reset current display affects only the active presentation config
- Test active metric widget resolution:
  - resolved active config contains shared fields plus only the active display’s fields
  - preview consumers receive the same resolved shape regardless of how many inactive display configs exist
- Test editor dispatch:
  - text uses the generic metric editor path
  - boxed presentations use display-specific editor sections
- Test preview dispatch:
  - intrinsic presentations use the text/metric preview path
  - boxed presentations use the corresponding boxed preview path
- Test overlay behavior:
  - boxed presentations are resizable/bounded
  - intrinsic text presentations keep intrinsic layout behavior

### Backend Tests

- Test manifest-driven display-type support:
  - allowed display types per metric kind
  - label and formal display-type definition loading
- Test config parsing:
  - `DisplayType` deserializes correctly for all supported presentation kinds
  - unsupported display types are rejected or normalized per the agreed contract
- Test backend metric presentation dispatch:
  - each supported `DisplayType` for a metric kind routes to the correct metric presentation path
  - the metric pipeline owns active metric presentation dispatch rather than silently skipping boxed presentations
- Test resolved active metric config consumption:
  - backend rendering sees the active presentation fields it needs and ignores inactive presentation configs by construction
- Test route/elevation separation:
  - true graphical widgets remain separate from metric presentation dispatch

### Prior Art

- Frontend tests should follow the existing Vitest and Testing Library patterns already used for widget editors, widget previews, defaults, and widget behavior helpers.
- Backend tests should follow the existing Rust unit and integration test patterns used for config parsing, metric formatting, widget rendering, and render baseline behavior.

## Out of Scope

- Implementing the visual rendering details of every future boxed metric presentation.
- Replacing route or elevation with the metric presentation model.
- Adding new user-facing presentation types beyond the currently discussed set.
- Final visual design decisions for each future presentation’s geometry and styling.
- Template migration tooling for external released templates, since backward compatibility is explicitly not a requirement here.
- Reworking unrelated widget families that do not participate in the metric `display_type` model.
- Introducing separate persisted dispatch selectors for preview, editor, and backend in addition to `display_type`.

## Further Notes

- The product model should stay simple: `value` is the metric, `display_type` is the presentation.
- The implementation may be layered internally, but it should not leak a more complicated conceptual model back into the user-facing or persisted widget contract.
- The current work around heading tape is useful as the first real boxed presentation, but the formalization should treat it as one member of a general metric presentation system.
- The storage model and runtime render model should be intentionally different:
  - storage preserves all presentation variants for a widget
  - runtime consumers operate on the resolved active presentation only
- The frontend and backend should converge on the same presentation vocabulary and semantics from the shared manifest, even if their concrete rendering implementations remain separate.

## 4-Phase Implementation Plan

### Phase 1: Shared `display_type` Contract Formalization

**Goal**

- Make the meaning of each `display_type` explicit in the shared metric manifest and expose the same semantics to the frontend and Rust backend.

**Steps**

1. Extend the shared metric manifest so each `display_type` has a first-class definition.
2. Add shared metadata for each `display_type`, at minimum:
   - human-readable label
   - layout mode: `intrinsic` (default text/metric/value) or `boxed` (plots, tapes, and future framed presentations)
   - optional default frame dimensions for boxed presentations but in SHARED_VALUES and similar variables in widgetDefaults
3. Keep the existing allowed-display-types-per-metric contract, but source labels and behavior from the formal definitions rather than loose string handling.
4. Update the frontend standard-metrics loader so callers can resolve:
   - all display type definitions
   - one display type definition by key
   - whether a display type is boxed or intrinsic
   - default frame geometry for a display type when available
5. Update the Rust standard-metrics loader to parse and expose the same display-type semantics for backend use.
6. Add targeted tests on both sides so the shared manifest is validated and malformed or incomplete display type definitions fail loudly.

**Acceptance Criteria**

- `display_type` behavior is defined centrally in shared data instead of inferred ad hoc from strings.
- Frontend code can answer "is this display boxed?" without metric-specific heuristics.
- Rust code can answer the same question from the parsed shared manifest.
- Allowed display types per metric still work as before, but now rely on the formalized definitions.
- Tests cover manifest parsing and basic display-type helper behavior.

**Modified Files**

- `assets/standard-metrics.json`
- `app/src/lib/standard-metrics.js`
- `src-tauri/ovrley_core/src/standard_metrics.rs`
- related frontend tests for standard-metrics helpers
- related Rust tests for standard-metrics parsing

### Phase 2: Frontend Behavior and Dispatch Cleanup

**Goal**

- Make frontend editor, preview, and layout behavior derive from `display_type` semantics instead of scattered special cases and "plot-like" heuristics.

**Steps**

1. Introduce a small display-type behavior helper layer on the frontend backed by the shared manifest definitions.
2. Replace existing "plot-like" or "non-text means plot" checks with `display_type`-driven layout behavior.
3. Update preview dispatch so metric widget rendering chooses the presentation path from `display_type`.
4. Update preview model generation so active metric preview data is prepared according to the current `display_type`.
5. Update editor dispatch so generic metric editing remains on the text path while presentation-specific editor sections are selected by `display_type`.
6. Keep route and elevation on their existing separate graphical paths; only metric presentation behavior should move under `display_type`.
7. Add or update tests for widget behavior, preview dispatch, and sidebar/editor dispatch.

**Acceptance Criteria**

- No frontend behavior relies on "plot-like" terminology or the heuristic that any non-text metric is plot-based.
- Preview dispatch for metric widgets is controlled by `display_type`.
- Widget sizing and layout mode are controlled by the formal display-type definition.
- Editor dispatch uses `display_type` to choose presentation-specific controls.
- Existing route and elevation flows remain unaffected.

**Modified Files**

- `app/src/lib/widget-behavior.js`
- `app/src/features/widget-preview/components/WidgetPreview.jsx`
- `app/src/features/widget-preview/utils/metricWidgetPreviewModel.js`
- `app/src/features/widget-editor/components/SidebarWidgetsTab.jsx`
- `app/src/features/widget-editor/components/HeadingWidgetEditor.jsx` or its eventual replacement path
- related preview/editor behavior tests

### Phase 3: Frontend Hybrid Config Shape and Active-Display Resolution

**Goal**

- Replace the flat mixed config shape with a hybrid model: keep text-compatible fields flat, move non-text presentation config into nested variant buckets, and keep display-type switching non-destructive.

**Steps**

1. Define the new frontend storage shape for metric widgets:
   - shared or common metric fields
   - active `display_type`
   - existing text-compatible fields kept flat for template compatibility
   - nested non-text config buckets under a dedicated container such as `display_variants`
2. Decide which current fields stay shared, which remain part of the flat text-compatible shape, and which move under non-text display-specific buckets.
3. Move non-text display-specific defaults out of metric-type-owned defaults and into display-type-owned defaults.
4. Move non-text display-specific durable key ownership out of metric-type-owned key lists and into display-type-owned key definitions.
5. Add a helper that initializes a non-text display bucket from defaults the first time a widget switches to that `display_type`.
6. Update variant-switching behavior so changing `display_type`:
   - preserves previous display-specific settings
   - initializes the new non-text display config if absent
   - never silently clears other display configs
7. Add explicit reset actions for the current display config if reset behavior is desired.
8. Add an active metric widget resolver that:
   - reads flat text-compatible fields when `display_type` is `text`
   - reads shared fields plus the active nested non-text bucket for non-text presentations
   - produces the effective config consumed by preview and export
9. Update normalization, template persistence, and editor state code to use the hybrid shape and active resolver.
10. Add targeted tests for:
    - switching between display types
    - default initialization
    - persistence of inactive display configs
    - active-config resolution

**Acceptance Criteria**

- Metric widgets no longer store all non-text display-specific fields as one flat mixed bag.
- Existing text-oriented templates remain compatible without requiring a full text-config migration.
- Switching `display_type` does not discard per-display edits.
- Text continues to read and write the flat compatible fields, while non-text displays own their nested defaults and durable keys.
- Preview and export paths can consume a resolved active metric widget without understanding the hybrid storage shape directly.
- Tests prove that switching from one display type to another and back preserves prior settings.

**Modified Files**

- `app/src/features/widget-editor/data/widgetDefaults.js`
- `app/src/features/template-manager/data/templateConstants.js`
- `app/src/features/widget-editor/utils/widgetUtils.js`
- metric widget normalization helpers
- editor state and update helpers for metric widgets
- preview and export config resolution helpers
- related tests for defaults, persistence, and switching behavior

### Phase 4: Rust Backend Rendering Consolidation

**Goal**

- Make Rust metric rendering dispatch primarily on `DisplayType` inside the metric or value widget pipeline so metric presentations are handled as presentations, not side widgets.

**Steps**

1. Identify the current split points where boxed metric presentations escape the normal metric render path.
2. Refactor backend metric rendering so the main metric or value pipeline dispatches on:
   - metric kind for data sourcing
   - `DisplayType` for presentation rendering
3. Keep route and elevation as separate true graphical widget families.
4. Move heading-tape-like metric presentations behind the same metric presentation seam instead of separate special render passes where possible.
5. Update cached or static rendering preparation so presentation-specific caches are owned by the active metric presentation path.
6. Make backend config resolution consume the active presentation config cleanly rather than relying on a flat bag of loosely related fields.
7. Add backend tests for:
   - `DisplayType` dispatch
   - boxed vs intrinsic metric presentations
   - metric presentation preparation and caching
   - rendering of heading tape through the metric presentation pipeline

**Acceptance Criteria**

- Rust metric rendering dispatch is centered on `DisplayType` for presentation selection.
- Boxed metric presentations no longer rely on ad hoc side paths outside the core metric rendering seam.
- Route and elevation remain separate and unaffected.
- Backend tests cover the active metric presentation flow and prevent regressions as more boxed display types are added.

**Modified Files**

- `src-tauri/ovrley_core/src/types.rs`
- `src-tauri/ovrley_core/src/config/mod.rs`
- `src-tauri/ovrley_core/src/render/mod.rs`
- `src-tauri/ovrley_core/src/render/widgets/mod.rs`
- `src-tauri/ovrley_core/src/render/widgets/value/mod.rs`
- metric presentation-specific render or prepare modules under `src-tauri/ovrley_core/src/render/widgets/`
- related Rust render and config tests

## Storage Model Revision

The storage-model decision for this refactor is a hybrid compatibility-preserving shape rather than fully nesting every display type.

- `display_type` remains the active presentation selector.
- Existing text-compatible fields remain flat in metric widget config so current text-oriented templates do not need a full migration.
- Non-text presentations such as `heading_tape`, `linear`, `bars`, `arc`, and `corner` should store their display-specific config in a nested container such as `display_variants`.
- Truly shared fields should remain top-level.
- Switching between display types must never clear inactive variant config automatically.
- The resolver used by preview, export, and backend handoff should flatten the active presentation at runtime:
  - `text` resolves from the flat compatible fields
  - non-text presentations resolve from shared fields plus `display_variants[display_type]`

This hybrid model is the preferred path because it captures the ownership benefits of nested special variants while preserving compatibility with existing text-oriented templates.
