# Plan v2: Eliminate Backend-Only Output Drift Without Over-Specifying Config

## Status: Draft / Proposed replacement for v1

---

## 1. Goal

The real problem is not "all fallbacks are bad".

The real problem is:

1. The backend can produce pixels or encoded output that differ from the user's intended config because it applies **backend-only defaults**.
2. Some missing or invalid fields are accepted too deep in the pipeline, so the render succeeds with the wrong output instead of failing early.
3. The current ownership of defaults is split across frontend manifests, frontend normalization, serde defaults, and Rust renderer helpers.

This v2 plan changes the target:

> We do **not** remove every fallback.
> We remove only the fallbacks that can silently change user-visible output or encoding behavior relative to the validated config contract.

**Non-negotiable rules**

- DO NOT jump into the common "centralize all defaults in Rust" trap. This plan is the exact opposite: the backend should have zero ownership of render-affecting defaults.
- Define validating contract for the standard metric text/value slice, with zero backend-owned semantic defaults.
- NEVER encode backend-side constants or defaults.
- NEVER assume the intent or desired outcome of the user submitting the contract to Rust.
- Do not inlcude missing values in the contract.
- Reject missing output-affecting fields with explicit error messages.
- Failing because the user omitted a field that can change output is the right outcome.

That means:

- No backend-only styling defaults that alter render output.
- No backend-only encoding defaults that alter codec/fps/resolution/range/output timing.
- No renderer-side inheritance chains that invent missing values.
- But we **do keep** legitimate defaults in one canonical normalization layer, and we **do keep** runtime/environment fallbacks that are not part of the user-authored render contract.

---

## 2. New Design Principle

### 2.1 Single Source of Truth

There should be exactly one place where render-affecting defaults are materialized:

- **Normalization seam**

Everything after normalization should consume a shape where output-relevant fields are already explicit.

That means:

- Frontend/editor state may remain ergonomic and sparse.
- Saved templates may remain durable and compact.
- The backend should receive either:
  1. a fully normalized render config, or
  2. a raw template config that the backend normalizes using the exact same canonical defaults as the frontend.

But it must not do ad-hoc last-mile fallback decisions inside draw code.

### 2.2 Distinguish Three Kinds of Defaults

Not all defaults are the same:

1. **Contract defaults**
   - Legitimate product defaults.
   - Example: new speed widget defaults to `display_unit = "kmh"`.
   - These belong in manifests / normalization.

2. **Runtime heuristics**
   - Non-user-authored operational decisions.
   - Example: thread count, hardware probing, temp paths.
   - These can remain as runtime fallbacks.

3. **Silent drift defaults**
   - Renderer/encoder invents values because config is missing something.
   - Example: route color falls back to white in Rust even though frontend did not explicitly provide it.
   - These must be removed from render/encode code.

The v1 plan treated all three categories too similarly.

---

## 3. Scope: What Must Be Guaranteed Explicit Before Render Starts

The normalization seam must guarantee explicit values for all fields that can change:

### 3.1 Visual output

- colors
- opacities
- font names
- font sizes
- line widths
- marker settings
- icon settings
- heading widget settings
- gradient widget settings
- route/elevation normalization inputs
- display_type-specific variant settings

### 3.2 Output geometry and timing

- scene width
- scene height
- scene fps
- scene scale if it affects backend render sizing
- scene start/end or equivalent export window
- widget update rate if it affects frame cadence

### 3.3 Encoding decisions

- codec/profile/container when user-meaningful
- target render duration
- composite trim/sync/range inputs
- any output resolution / fps conversion settings

If a field changes pixels, frame count, timestamps, container, or selected codec path, it must be explicit before render/encode execution.

---

## 4. What Should Stay Optional

These should **not** be forced into explicit template JSON if they are not part of user intent:

### 4.1 Runtime/environment concerns

- thread count
- ffmpeg discovery
- QSV/CUDA/VAAPI probe behavior
- temp/download/debug paths
- log verbosity defaults

### 4.2 Data-absence presentation

- `"--"` for missing telemetry values
- interpolation fallbacks for sparse activity data
- empty collections such as `labels: []`, `values: []`, `plots: []`

### 4.3 Final-resort platform fallback

- font resolver last-resort typeface fallback

But this last one should be treated as:

- diagnostic fallback
- warning-worthy
- not part of the intended styling contract

---

## 5. Replace "Everything Required in Schema" With "Validated Explicit Render Contract"

This is the key simplification.

The v1 plan pushed too hard toward:

- removing most `#[serde(default)]`
- making many config fields required in raw JSON
- pushing every implicit value into persisted files

That is unnecessary.

Instead:

### 5.1 Backend entry must already contain explicit user-intent fields

The editor already owns:

- defaults
- suggestions
- widget creation presets
- migrations
- effective config materialization

So the backend should make **zero assumptions** about output or user intent.

That means:

- if a field can change the output, it must already be explicit by the time the backend receives the config
- if it is missing at backend entry, that is an error
- the backend must not "helpfully" fill it in

Examples of fields that must already be explicit:

- `display_unit`
- codec/profile/container when they affect output semantics
- fps / update cadence
- resolution / scale
- scene start / end or resolved export window
- output-affecting styling

### 5.2 Keep raw template schema ergonomic where possible

Templates can stay compact and user-facing.

They do **not** need every heading tick size, every gradient triangle width, every icon color, every derived display field serialized explicitly if those values come from canonical manifests.

But the backend-facing render contract must still be explicit.

That means there may be a distinction between:

- durable saved template shape
- effective editor state
- backend execution contract

The backend only cares about the last one.

### 5.3 Introduce a validated backend input type

Create an explicit seam:

```rust
pub struct ValidatedRenderConfig {
    pub scene: ValidatedScene,
    pub labels: Vec<ValidatedLabel>,
    pub values: Vec<ValidatedValue>,
    pub plots: Vec<ValidatedPlot>,
    pub encode: ValidatedEncodeOptions,
}
```

Each validated/normalized struct should already contain explicit output-relevant fields.

After that:

- render modules stop reading `Option<T>` for output-relevant data
- encode modules stop reading `Option<T>` for output-relevant data
- no `expect("guaranteed by validation")` scattered through draw code

### 5.4 Validation should validate explicit intent, not invent it

The seam should:

1. parse raw config
2. verify that all output-affecting semantic fields are already explicit
3. flatten / normalize structure
4. validate impossible/unsupported states
5. hand only validated normalized types to renderer/encoder

That is cleaner than:

1. parse raw config
2. silently materialize semantic omissions
3. keep `Option`s everywhere
4. `expect(...)` later

### 5.5 The seam may derive mechanical values, but not semantic intent

The backend seam may still do work, but only of the right kind.

Allowed:

- flatten shapes
- validate invariants
- derive mechanical values from explicit inputs

Forbidden:

- choosing missing semantic values
- selecting output-affecting defaults on behalf of the user

Examples:

Allowed:

- parse `"#ff0000"` into an RGBA color
- convert opacity percentage into normalized float
- flatten `display_variants.heading_tape` into the active heading-tape config
- convert degrees to radians
- convert explicit fps data into a validated `Fps` struct
- compute render duration from explicit `scene.start` and `scene.end`
- build a resolved text style struct from explicit `font`, `font_size`, `color`

Forbidden:

- `display_unit` missing -> choose `"kmh"`
- `icon_color` missing -> choose `"#ffffff"`
- codec missing -> choose `"libx264"`
- resolution missing -> choose `1920x1080`
- scene end missing -> assume full activity range

### 5.6 What "invariants" means

Invariants are rules that must be true for the config to be renderable, but are not guesses about user intent.

Examples:

- `width > 0`
- `height > 0`
- `fps > 0`
- `scene.start <= scene.end`
- opacity in valid range
- color string parses successfully
- `display_type` is supported
- `heading_tape.width` / `height` exist when heading tape is active
- simplify tolerance is non-negative
- target density is positive
- codec value is one the backend supports

These are validation rules, not defaults.

---

## 6. Canonical Default Ownership

### 6.1 Source of defaults

Render-affecting defaults should come from one canonical source **upstream of backend execution**:

- shared manifests if possible
- otherwise a dedicated normalization module

They should **not** be mined from `unwrap_or(...)` calls in Rust implementation code.

Important distinction:

- upstream canonical defaults may exist in editor creation flow, migration logic, or manifest-backed effective state
- backend execution must not apply those defaults late if the field is still missing

### 6.2 Recommended ownership

Use this precedence:

1. User-authored widget/config values
2. Editor-owned explicit materialization of product defaults
3. Explicit validation error if still unresolved

Not this:

1. User-authored value
2. backend draw helper fallback
3. backend color helper fallback
4. serde default
5. hardcoded Rust constant

### 6.3 Shared contract

If frontend and backend both need normalization logic, the contract should be shared by:

- shared manifest data
- mirrored normalization semantics
- fixture parity tests at the seam level

The draw code should not be where agreement is negotiated.

---

## 7. Revised Work Plan

### Phase 1: Classify by Drift Risk, Not by Syntax

Do **not** start with a spreadsheet of every `unwrap_or`.

Instead classify fallback sites into:

1. **Output-drift critical**
   - changes pixels, geometry, fps, resolution, codec, range, timing
2. **Data absence**
   - placeholder output for missing telemetry
3. **Runtime/environment**
   - thread counts, hardware detection, paths
4. **Diagnostic last resort**
   - typeface resolver fallback

Deliverable:

- a compact inventory of only category 1 sites

That is the actionable set.

### Phase 2: Build Normalized Backend Contract

[IMPORTANT] Define validating contract for the standard metric text/value slice, with zero backend-owned semantic defaults. Do NOT encode backend-side constants or defaults. Do NOT assume user's intent or desired outcome. Do not inlcude missing values in the contract. Instead, reject missing output-affecting fields with explicit error messages.

Add a seam module, for example:

```rust
pub fn normalize_and_validate_render_config(
    raw: RenderConfig
) -> CoreResult<ValidatedRenderConfig>
```

Responsibilities:

1. verify output-affecting semantic fields are already explicit
2. resolve inheritance chains into explicit values
3. flatten structural variants into execution-friendly structs
4. reject unsupported states with field paths
5. produce non-optional render/encode structs

This is where:

- plot colors become explicit
- heading tape settings become explicit
- route/elevation marker settings become explicit
- encode options become explicit

This is **not** where:

- missing units get chosen
- missing codec gets selected
- missing resolution gets guessed

### Phase 3: Cut Renderer Off From Raw Config

Change renderer entry points to accept validated normalized types only.

Examples:

- `ResolvedTextStyle` should already be complete
- route/elevation normalize modules should consume explicit normalized structs
- value widget layout should consume explicit icon/unit/gradient settings

Goal:

- remove backend-only draw-time fallbacks
- keep rendering code simple

### Phase 4: Cut Encoder Off From Raw Config

Same principle for encoding:

- explicit resolution
- explicit fps/update cadence
- explicit range/timing
- explicit codec/profile/container where product-owned

Keep runtime heuristics outside this contract.

### Phase 5: Delete Drift-Producing Helpers

After normalized types exist, remove helpers whose purpose was to invent missing values:

- color inheritance fallback chains
- legacy line width fallback shims
- synthesized marker defaults when config should already be explicit

But only after the seam has absorbed that responsibility.

### Phase 6: Migration

Migrate templates from old sparse semantics to the new normalized contract behavior.

Important:

- migration defaults must come from canonical manifests / normalization rules
- not from grep-ing `unwrap_or(...)`

If template v3 is needed, the migration function should live at the seam.

---

## 8. Concrete Policy By Area

### 8.1 Styling

Policy:

- user-visible styling must be explicit in `ValidatedRenderConfig`
- raw templates may remain sparse
- draw code must not invent colors/sizes/fonts/opacities

### 8.2 Codecs / container / output format

Policy:

- if the user or product selection can change the actual encoded output, it must be explicit before encode starts
- hardware capability fallback may still choose execution strategy, but must not silently change the requested semantic output

Example:

- choosing software fallback for the same requested codec profile can be acceptable
- silently switching to a different codec because config omitted one is not

### 8.3 FPS / resolution / scale

Policy:

- render resolution and fps are explicit contract data
- no backend defaulting to `1920x1080` or `1.0` once render begins

### 8.4 Scene start/end and export range

Policy:

- render/encode entry points must receive an explicit resolved window
- no downstream fallback from missing range to implicit whole-activity render unless that decision was already made in normalization

### 8.5 Missing telemetry

Policy:

- placeholder output remains allowed
- this is not config drift

---

## 9. What v2 Intentionally Does Not Do

This plan does **not** do the following:

1. It does not make every render-affecting field required in raw JSON.
2. It does not remove every `#[serde(default)]`.
3. It does not ban all fallbacks everywhere.
4. It does not require a giant spreadsheet of all fallback syntax sites.
5. It does not use `expect(...)` as the main safety mechanism in draw code.
6. It does not allow the backend seam to choose missing semantic values.

Those were the main sources of overengineering in v1.

---

## 10. Recommended First Slice

Implement one vertical slice first:

1. normalize + validate standard metric text widgets
2. pass explicit normalized value widget structs into Rust render code
3. delete icon/unit/gradient styling fallbacks in that path
4. confirm backend output matches frontend-intended defaults

Then do:

5. route/elevation plots
6. heading tape
7. encode contract

This proves the architecture before a repo-wide rewrite.

---

## 11. Success Criteria

The work is successful when:

1. The backend render/encode code no longer invents missing output-affecting values.
2. All output-affecting intent is already explicit at backend entry.
3. Frontend intention and backend output match for styling, codec, fps, resolution, and render window.
4. Runtime/environment heuristics still exist where appropriate, without contaminating the render contract.
5. Renderer modules consume validated, mostly non-optional types instead of raw sparse config.

---

## 12. Short Version

If v1 was:

> "remove all silent fallbacks"

v2 is:

> "remove only backend-side fallbacks that can silently change intended output, and require semantic intent to be explicit before backend execution"

That achieves the goal without turning the config format into an implementation dump.
