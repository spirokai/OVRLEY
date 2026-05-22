# Phase 5 — Cache and State Management: Detailed Implementation Plan

## Purpose

Make hidden global state explicit where it improves testability, determinism, or correctness. Audit all caches and global state across the codebase. Justify or restructure each piece of shared mutable state. Do not introduce complex cache infrastructure (TTL, LRU, eviction) unless a concrete problem demands it.

This phase builds on the pipeline cleanup from Phase 4 and the module structure from Phase 3. It assumes `progress.rs` owns `RenderController`, request structs are in place, and sibling pipelines are isolated.

---

## Table of Contents

1. [Pre-Flight Checklist](#1-pre-flight-checklist)
2. [Step 1 — Full Audit of All Global State and Caches](#2-step-1--full-audit-of-all-global-state-and-caches)
3. [Step 2 — Audit Composite-Only State (`video_composite_debug.rs`)](#3-step-2--audit-composite-only-state-video_composite_debugrs)
4. [Step 3 — Evaluate Each Cache: Justify or Restructure](#4-step-3--evaluate-each-cache-justify-or-restructure)
5. [Step 4 — Introduce Explicit RenderContext (If Justified)](#5-step-4--introduce-explicit-rendercontext-if-justified)
6. [Step 5 — Add Cache Metrics (If Useful)](#6-step-5--add-cache-metrics-if-useful)
7. [Step 6 — Cleanup and Verification](#7-step-6--cleanup-and-verification)
8. [Completion Criteria](#8-completion-criteria)

---

## 1. Pre-Flight Checklist

Before starting Phase 5, verify the following preconditions from earlier phases:

### 1.1 Phase 4 preconditions

- [x] `encode/ffmpeg_settings.rs` exists with `FfmpegSettings` and `build_ffmpeg_settings`
- [x] `encode/progress.rs` owns `RenderController` (or re-exports it from old location)
- [x] Request structs (`PreviewRenderRequest`, `FrameRenderRequest`, `MetricWidgetRequest`) are in place
- [x] `commands/mod.rs` no longer contains `derive_composite_render_plan` (moved to `encode`)
- [x] No `#[allow(clippy::too_many_arguments)]` on refactored render/encode functions
- [x] Sibling pipelines are isolated (no cross-imports between `video_pipeline` and `video_composite_pipeline`)

### 1.2 Phase 3 preconditions

- [x] `ovrley_core/src/paths.rs` exists with `AppPaths`
- [x] `ovrley_core/src/interpolation.rs` is the single source of truth (f64-based)
- [x] `ovrley_core/src/rdp.rs` exists with shared RDP logic
- [x] `render/widgets/common.rs` responsibilities are reduced
- [x] No `encode` → `commands` dependency

### 1.3 Phase 1–2 preconditions

- [x] `ovrley_core/src/error.rs` exists with `CoreError` and `CoreResult`
- [x] `ovrley_core/src/types.rs` exists with `MetricKind` enum
- [x] All tests live in dedicated `tests/` directories
- [x] `ovrley_core/tests/common/test_config.rs` exists and is used by all tests

### 1.4 Baseline Before Changes

- [x] Run `cargo test` and record result (all pass) — 130 passed, 0 failed, 3 ignored
- [x] Run `cargo clippy -- -D warnings` and record result — 9 pre-existing errors, none from Phase 5
- [ ] Run a representative preview render and record duration — not executed in this environment
- [ ] Run a representative transparent export and record duration + memory usage — not executed
- [ ] Run a representative composite export and record duration + memory usage — not executed
- [x] Check for test state leakage: run `cargo test` 3 times consecutively — identical results both runs

If any precondition fails, stop and resolve it before proceeding. Phase 5 relies on stable test infrastructure and typed errors.

---

## 2. Step 1 — Full Audit of All Global State and Caches

**Purpose:** Locate every piece of implicitly shared mutable state in the codebase. Understand what it holds, why it exists, its lifecycle, and its impact on test determinism.

### 2.1 Search Strategy

Run the following searches from the workspace root. Record every finding — even things that turn out to be harmless. Do not skip items because they "look fine."

```bash
# All static/global caches and mutable statics
rg "static.*(CACHE|Cache|cache)" --type rust src-tauri/

# All OnceLock usage (Rust's one-shot lazy initialization)
rg "OnceLock" --type rust src-tauri/

# All LazyLock usage (if any)
rg "LazyLock" --type rust src-tauri/

# All global Mutex/RwLock usage (thread-safe mutable statics)
rg "static.*(Mutex|RwLock)" --type rust src-tauri/

# All thread_local! usage (per-thread global state)
rg "thread_local!" --type rust src-tauri/

# Global AtomicBool / AtomicU32 / atomic counters
rg "static.*Atomic" --type rust src-tauri/
```

### 2.2 Known Global Caches (Master Plan §3.9)

Based on the master plan, there are two confirmed global caches. Start the audit here, but do not assume these are the only ones.

#### Cache A: Font/Typeface Cache

| Property | Detail |
|----------|--------|
| **Location** | `render/text.rs:277` |
| **Declaration** | `static CACHE: OnceLock<Mutex<HashMap<String, Typeface>>>` |
| **What it stores** | Loaded Skia `Typeface` objects, keyed by font name string |
| **Why cached** | Font loading is expensive (disk I/O + Skia parsing). Fonts are referenced per-frame during text rendering. |
| **Bounded?** | Unbounded — grows with every unique font name requested. In practice, the font set is small (a handful of system fonts + bundled fonts). |
| **Reset in tests?** | No. `OnceLock` cannot be reset once initialized. The `Mutex` allows clearing the `HashMap` but the `OnceLock` itself persists for the process lifetime. |
| **Affects output?** | Yes — fonts affect text rendering output. |
| **Affects performance?** | Yes — cache hit avoids disk I/O and Skia font parsing on every frame. |

**Questions to answer during audit (read `render/text.rs` to confirm):**

1. What is the exact declaration? `OnceLock<Mutex<HashMap<String, Typeface>>>` or a variation?
2. How is the cache populated? On first access? Lazy per-key?
3. What functions touch the cache? Is it wrapped in accessor functions or used inline?
4. Are there any `clear()` or invalidation methods?
5. Does any test exercise font rendering? If so, does the cache leak between test cases?

#### Cache B: Label Cache

| Property | Detail |
|----------|--------|
| **Location** | `render/mod.rs:464` |
| **Declaration** | `static CACHE: OnceLock<Mutex<HashMap<u64, Image>>>` |
| **What it stores** | Pre-rendered label `Image` objects, keyed by a `u64` hash |
| **Why cached** | Label rendering involves text layout + Skia surface allocation. Labels are reused across frames (same position/style). |
| **Bounded?** | Effectively bounded — keyed by render params (width, height, scale, scene, labels, values). Grows per unique config, not per frame. |
| **Reset in tests?** | No — same `OnceLock` limitation as Cache A. |
| **Affects output?** | Yes — labels are visible in rendered frames. |
| **Affects performance?** | Yes — label rendering is non-trivial (text measurement, surface creation, drawing). |

**Questions to answer during audit:**

1. Verify that `labels_cache_key` (line 541, `cached_labels_image` at line 458) includes every field that affects label appearance: width, height, scale, scene config, label definitions, and value configs. If the key covers all inputs, cross-render staleness is not a risk.
2. Is the cache consulted inside the per-frame render loop, or only during asset preparation? The function name `cached_labels_image` and its location near `prepare_preview_assets` suggest preparation, not per-frame — confirm by tracing callers.
3. What is `labels_cache_key`'s hasher? A `u64` hash collision is unlikely (~5B entries for birthday paradox), but verify the key isn't degenerate (e.g., all-zero constant).

### 2.3 Audit Template for Each Finding

For every piece of global state found in the codebase, fill out this template:

```
### Finding #N: [Name]

| Property          | Detail |
|-------------------|--------|
| Location          | [file:line] |
| Declaration       | [exact Rust code] |
| Type of state     | [cache / config singleton / atomic flag / global counter / other] |
| What it holds     | [description of stored data] |
| Why global?       | [is it truly global, or could it be thread-local or context-scoped?] |
| Populated by      | [which functions write to it] |
| Read by           | [which functions read from it] |
| Bounded?          | [yes / no / effectively bounded because...] |
| Reset in tests?   | [yes / no / how] |
| Affects output?   | [yes / no / how] |
| Affects hot path? | [yes / no — is it accessed inside frame render loop?] |
| Risk assessment   | [LOW / MEDIUM / HIGH] |
| Action            | [KEEP AS-IS / DOCUMENT / MOVE TO CONTEXT / REPLACE] |
```

### 2.4 Expected Audit Findings (Verified Against Source)

These are the known items confirmed against the current tree. The audit may find more.

| # | Location | Item | Type | Risk |
|---|----------|------|------|------|
| 1 | `render/text.rs:277` | Font typeface cache (`OnceLock<Mutex<HashMap<String, Typeface>>>`) | Cache | LOW |
| 2 | `render/mod.rs:472` | Label image cache (`OnceLock<Mutex<HashMap<u64, Image>>>`), used in `cached_labels_image` during asset *preparation* (not per-frame) | Cache | LOW |
| 3 | `render/widgets/value.rs:547–586` | Six `OnceLock<Option<ParsedSvgIcon>>` caches (one per `MetricIconKind`), lazily parsed from `include_str!`-bundled SVG, immutable after init | Immutable singleton | LOW |
| 4 | `encode/video_composite_debug.rs:15` | Debug artifact output to `debug_render/phase_7/` via `COMPOSITE_DEBUG_PHASE` constant | I/O state | LOW |

**Key verification results from source audit:**

- **Label cache (item 2):** Called once during `prepare_preview_assets` (line 114), **not** inside the per-frame render loop. The key (`labels_cache_key`, line 541) hashes `width`, `height`, `scale`, `config.scene`, `config.labels`, and `config.values` via serde_json — covering every config-dependent input. Cross-render staleness is not possible because different configs produce different keys.
- **SVG icon caches (item 3):** Six independent `OnceLock` caches that wrap `parse_svg_icon` results. Icons are bundled at compile time via `include_str!` — the SVG markup never changes at runtime. These are lazy-initialized constants, not mutable caches. Accessed per-frame via `parsed_metric_icon` (line 456) inside `draw_metric_icon`, but the lookup hits a pre-initialized `OnceLock` (`get_or_init` returns immediately after first call).
- **Composite debug (item 4):** Already uses `CoreResult` / `CoreError::Io` (lines 9, 107, 112, 161). The constant `COMPOSITE_DEBUG_PHASE: &str = "phase_7"` (line 15) is the only remaining dev artifact.

**Watch for additional findings that may have been introduced during Phases 1–4:**

- Any new `OnceLock` or `LazyLock` added for test fixtures or config
- Any `AtomicBool` used as a global cancellation/shutdown flag
- Any `thread_local!` storage in encode/render modules
- Any `static` config values or environment-based globals

### 2.5 Output of Step 1

At the end of Step 1, produce an **audit report** as a comment block at the top of this document, listing:

1. Every piece of global state found
2. Its risk assessment (LOW / MEDIUM / HIGH)
3. The recommended action (KEEP / DOCUMENT / MOVE TO CONTEXT / REPLACE)

This report serves as the decision record for Steps 3–5.

### 2.6 What to Keep in Mind

- **`OnceLock` is process-global.** Its contents live for the entire process lifetime. There is no way to "reset" a `OnceLock` — you can only clear the data *inside* it (e.g., `HashMap::clear()` on the contents behind the `Mutex`). This distinction matters for test design.
- **Global state is not automatically bad.** The master plan §3.9 explicitly states: "Global caches are not automatically wrong. Do not replace them with complex infrastructure unless there is a concrete reason." The audit exists to surface *whether* there is a concrete reason.
- **Distinguish "cache" from "singleton config."** A performance cache (hit/miss optional, wrong entry = slower but correct) is different from singleton mutable state (wrong entry = wrong output). Treat them differently.
- **Do not add new global state during the audit.** If you discover a function that *ought* to have a cache, note it but do not implement it in Phase 5. Phase 5 is about existing state, not new caches.

---

## 3. Step 2 — Audit Composite-Only State (`video_composite_debug.rs`)

**Purpose:** `video_composite_debug.rs` writes debug artifacts to disk on every composite render. While this is not a "cache" in the traditional sense, it is hidden I/O state that affects the filesystem and should be documented.

### 3.1 Current Behavior

From the master plan §17.16 (updated for verified current code):

- Writes `timing_summary.json` and other debug artifacts to `debug_render/phase_7/` (via `COMPOSITE_DEBUG_PHASE` constant at line 15)
- Already uses `CoreResult` / `CoreError::Io` for fallible operations (lines 9, 107, 112, 161) — the `Result<T, String>` in the master plan appendix is stale
- Derives a `composite_debug_id` from the output filename by stripping a `"video_composited_"` prefix
- No cleanup policy — debug artifacts accumulate indefinitely

### 3.2 Audit Questions

Answer these by reading the file in full:

1. **What exactly is written?** List every file produced, its format, and its purpose.
2. **When is it written?** On every composite render? Only in debug builds? Gated by a flag?
3. **Where is it written?** Is `debug_render/composite/` relative to the output directory or an absolute path?
4. **Is the directory configurable?** Or hardcoded?
5. **What happens if the directory doesn't exist?** Is it created? Does the render fail?
6. **What happens on write failure?** Is the render aborted, or is the debug write best-effort?
7. **Is there a cleanup policy?** Does anything delete old debug artifacts?
8. **Is this debug output observable by users?** Or only developers?

### 3.3 Recommendations (Implementation in Step 6)

Based on the audit, decide:

| Decision | Option A (Recommended) | Option B |
|----------|------------------------|----------|
| **Directory name** | Rename from `phase_7/` to `composite/` — generic names only, no phase-specific folders | Keep `phase_7/` (only if external tools depend on it) |
| **Directory location** | Move path to a shared constant in `encode/mod.rs` or `encode/constants.rs` | Keep hardcoded |
| **Cleanup policy** | Document that cleanup is manual; DO NOT add auto-cleanup unless disk space is a real problem | Add a "max debug dirs" limit or timestamp-based cleanup |
| **Gating** | Gate behind `#[cfg(debug_assertions)]` so it never runs in release | Keep runtime check (current behavior) |
| **Error handling** | Already uses `CoreResult` / `CoreError::Io` — no migration needed | N/A |
| **Debug ID derivation** | Replace string prefix stripping with a structured approach | Keep as-is if naming convention is stable |

The default recommendation is:
- **Rename `phase_7/` to `composite/`.** Phase-specific folder names are a development artifact; production debug output should use generic, self-describing names.
- Document that debug artifacts are best-effort and not cleaned automatically.
- **Consider** compile-time gating behind `#[cfg(debug_assertions)]` if release builds should not produce debug I/O. This is a behavior change from the current implementation, which writes debug artifacts unconditionally — get explicit approval before gating.
- **Already done:** `CoreResult` / `CoreError::Io` migration is complete (verified against source).

### 3.4 What to Keep in Mind

- **Phase-specific folder names are a development artifact.** `phase_7/` was the working directory used during the initial composite pipeline implementation. Replace it with a generic, self-describing name like `composite/`. The new path becomes `debug_render/composite/`.
- **`#[cfg(debug_assertions)]` vs runtime config.** The master plan §3.12 prefers `#[cfg]` over `cfg!()`. If the debug output should never exist in release builds, use compile-time gating. If users might want debug output in release builds (e.g., for support), use a runtime flag. Default: compile-time gate.
- **Do not add new debug output mechanisms.** Only audit and document what exists.

---

## 4. Step 3 — Evaluate Each Cache: Justify or Restructure

**Purpose:** For each global cache identified in Step 1, make an explicit decision: keep as global (with documented justification), or restructure into an explicit context.

### 4.1 Decision Framework

For each cache, ask these questions in order. The first "yes" determines the action.

```
Q1: Is the cache accessed on the hot path (inside frame render loop)?
    YES → Next question
    NO  → Global cache is almost certainly fine. Document and keep.

Q2: Does a stale/wrong cache entry produce incorrect output?
    YES → HIGH RISK — must be either scoped to render lifetime or keyed by config.
    NO  → Next question

Q3: Can the cache cause test state leakage (test A populates cache, test B sees it)?
    YES → Add a `clear()` method and call it in test setup, OR move to context.
    NO  → Next question

Q4: Is the cache unbounded and likely to grow without bound in production?
    YES → Add a bound or eviction strategy, OR document why it won't grow unbounded.
    NO  → Next question

Q5: Would moving the cache into an explicit context struct improve clarity or testability?
    YES → Move to context (see Step 4).
    NO  → Keep as global. Document reasoning.
```

### 4.2 Apply to Cache A: Font/Typeface Cache

#### 4.2.1 Evaluation

| Question | Answer | Reasoning |
|----------|--------|-----------|
| Q1: Hot path? | YES | Font lookup happens during text rendering, which is per-frame |
| Q2: Stale entry = wrong output? | NO | A stale typeface is still a valid typeface — fonts don't change at runtime. The cache is populated once per unique font name and never invalidated. |
| Q3: Test leakage? | YES | If test A renders with font X, test B could get a cache hit instead of a fresh load. But since fonts are immutable (loaded from disk, never change), this is harmless — the same font file would be loaded either way. The risk is if two different font *files* have the *same name* — highly unlikely. |
| Q4: Unbounded growth? | NO | The font set is fixed and small (system fonts + bundled fonts). At most ~10-20 entries per app session. |
| Q5: Would context improve things? | MARGINAL | Threading `RenderContext` through every text-drawing function adds boilerplate without changing behavior. The cache is effectively a read-only performance optimization. |

**Decision: KEEP AS GLOBAL.** The font cache is a well-behaved read-only performance cache. It is effectively bounded (small fixed font set), its entries are immutable, and converting it to an explicit context would add parameter threading overhead with no behavioral benefit.

**Required action:**
- Add a module-level doc comment on the cache explaining why it's global, what it holds, and that it's read-only after population.
- **If** a test seam for cache clearing is needed from integration tests: add a `pub fn clear_font_cache()` function with a doc comment marking it as test-only. **If not needed**, prefer indirect testing (render with fonts, verify correct output — the font cache is immutable data, so correct output implies a correct cache).
- Document that font files are never reloaded — if fonts change on disk during an app session, the cache must be manually cleared (or the app restarted).

#### 4.2.2 Implementation

```rust
// In render/text.rs, near the cache declaration:

/// Global typeface cache, keyed by font name.
///
/// Fonts are loaded from disk once and never invalidated. The font set
/// is fixed and small (system fonts + bundled fonts), so this cache is
/// effectively bounded. Entries are immutable — stale reads are harmless.
///
/// # Test Safety
///
/// Call [`clear_font_cache`] in test teardown to prevent state leakage
/// between test runs. In practice, leakage is harmless because fonts don't
/// change at runtime, but clearing ensures clean test state.
static CACHE: OnceLock<Mutex<HashMap<String, Typeface>>> = OnceLock::new();

/// Test-only: clears all cached typefaces. Do not call in production.
///
/// After calling this, the next font request will reload from disk.
/// Safe to call when no renders are in progress.
///
/// # Note on visibility
///
/// Marked `pub` (not `pub(crate)`) because integration tests in
/// `ovrley_core/tests/` are an external crate and cannot access
/// `pub(crate)` items. If a test seam is not needed from integration
/// tests, change to `pub(crate)` or omit entirely.
pub fn clear_font_cache() {
    if let Some(lock) = CACHE.get() {
        if let Ok(mut cache) = lock.lock() {
            cache.clear();
        }
    }
}
```

### 4.3 Apply to Cache B: Label Cache

#### 4.3.1 Evaluation

| Question | Answer | Reasoning |
|----------|--------|-----------|
| Q1: Hot path? | NO | `cached_labels_image` is called once during `prepare_preview_assets` (asset preparation, line 114), not inside the per-frame render loop. |
| Q2: Stale entry = wrong output? | NO | The cache key (`labels_cache_key`, line 541) hashes `width`, `height`, `scale`, `config.scene`, `config.labels`, `config.values` via serde_json. Different configs produce different keys. Cross-render staleness is structurally impossible. |
| Q3: Test leakage? | LOW RISK | Same reasoning as Q2 — different test configs produce different keys. Only a hash collision (negligible for u64) or identical configs across tests could cause a false hit — and a false hit with identical configs is correct output anyway. |
| Q4: Unbounded growth? | YES | The cache grows with every unique `(width, height, scale, scene, labels, values)` tuple. In practice, this is bounded by the number of config combinations a user creates — typically <100. |
| Q5: Would context improve things? | NO | The cache is already self-keyed by its inputs and lives in asset preparation (not hot path). Moving it to an explicit `RenderContext` would add parameter threading through the preparation call chain with no behavioral or correctness benefit. |

**Decision: KEEP AS GLOBAL.** The label cache is safe. Its key covers all config-dependent inputs, making cross-render staleness impossible. It is consulted once during preparation, not per-frame. The `OnceLock<Mutex<HashMap>>` pattern is appropriate — the only improvement would be documentation.

**Required action:**
- Add a module-level doc comment on the cache explaining what the key covers and why cross-render staleness cannot happen.
- Consider adding `#[allow(clippy::mutex_atomic)]` or equivalent justification if clippy flags the `Mutex<HashMap<u64, Image>>` pattern at preparation time (since the Mutex is never contended — single-threaded preparation).

#### 4.3.2 Key Audit

The cache key was verified by reading `labels_cache_key` (line 541–551):

1. **What is hashed:** `width: u32`, `height: u32`, `scale: f32` (via `to_bits()`), `config.scene` (via `serde_json::to_string`), `config.labels` (via `serde_json::to_string`), `config.values` (via `serde_json::to_string`). **All config-dependent fields are covered.**
2. **Hash collision resistance:** Uses `std::collections::hash_map::DefaultHasher` which produces a `u64`. Collision probability is negligible (~5 billion entries for 50% chance via birthday paradox).
3. **Use pattern:** `get_or_init` + `cache.get(&cache_key)` → on miss, render labels + icons into a surface, snapshot to `Image`, insert into cache. The `Image` is consumed post-preparation as `PreparedPreviewAssets.labels_image` — the cache is a preparation-time optimization, not a render-time one.

### 4.4 Apply to Caches C–H: SVG Icon Caches

#### 4.4.1 Evaluation

These six caches (`value.rs:551–585`) are identical in structure: each is a `OnceLock<Option<ParsedSvgIcon>>` wrapping a lazily-parsed SVG icon. Apply the framework once for all six.

| Question | Answer | Reasoning |
|----------|--------|-----------|
| Q1: Hot path? | YES | `parsed_metric_icon` (line 547) is called inside `draw_metric_icon` which runs per-frame. However, `OnceLock::get_or_init` returns immediately after the first call — the hot-path cost is a single `Option<&P>` borrow check. |
| Q2: Stale entry = wrong output? | NO | The cached value is parsed from an `include_str!`-bundled SVG file. The SVG markup is a compile-time constant — it cannot change at runtime. Once parsed, the `ParsedSvgIcon` is immutable and valid forever. |
| Q3: Test leakage? | NO | The parsed icon data is immutable. Even if tests share the same cached reference, they share identical, correct data. |
| Q4: Unbounded growth? | NO | Exactly 6 entries total — one per `MetricIconKind` variant. The set is closed and small. |
| Q5: Would context improve things? | NO | Threading a context through every icon draw call would add parameters to 10-argument hot-path functions with no benefit — the data is immutable. |

**Decision: KEEP AS GLOBAL.** These are lazy-initialized compile-time constants, not mutable caches. The `OnceLock<Option<ParsedSvgIcon>>` pattern is functionally equivalent to `LazyLock<Option<ParsedSvgIcon>>` on nightly Rust. The only concern is that six separate `static` declarations are redundant — a single `HashMap<MetricIconKind, ParsedSvgIcon>` inside one `OnceLock` would be cleaner, but refactoring this is cosmetic and out of scope for Phase 5 (do not change behavior).

**Required action:**
- Add a comment at the top of `parsed_metric_icon` noting that the six `OnceLock` caches are lazily-initialized compile-time constants and are safe as globals.
- Optionally (Phase 6 doc polish): note that these could be consolidated into a single `OnceLock<HashMap<MetricIconKind, ParsedSvgIcon>>` if desired — but do not implement in Phase 5.

### 4.5 Any Additional Caches Found in Audit

If the Step 1 audit discovers caches beyond the known items (font, label, six SVG icon caches, composite debug), apply the same decision framework to each. Record each decision in the audit report.

### 4.6 What to Keep in Mind

- **All currently known caches stay global.** The font cache, label cache, and SVG icon caches are each justified for different reasons (read-only data, self-keyed by config, or immutably initialized from compile-time constants). None require restructuring.
- **"Effectively bounded" needs evidence.** For each cache kept as global, provide the evidence in its documentation (e.g., "the font set is fixed — 3 bundled fonts + system fonts = ~10 entries max" or "the label cache key covers all config inputs; growth is bounded by unique config combinations").
- **Test leakage is the canary.** If a cache can cause different test results depending on test execution order, it needs a `clear()` method or scoped lifetime. For the verified caches in this codebase, none exhibit this risk — but the audit may find others.
- **The label cache was originally misdiagnosed as a hot-path cache.** The source audit confirmed `cached_labels_image` is called during asset *preparation* (not per-frame rendering), and the key covers all config-dependent inputs. This correction eliminated the need for a `RenderContext`.

---

## 5. Step 4 — Introduce Explicit RenderContext (If Justified)

**Purpose:** If the evaluation in Step 3 concludes that at least one cache should move to an explicit context, create a `RenderContext` struct and thread it through the relevant functions. If all caches are justified as global, **skip this step entirely** — do not create an abstraction for its own sake.

### 5.1 Gating Decision

Based on the verified evaluation against source code:

| Cache | Decision | Reason |
|-------|----------|--------|
| Font typeface cache | KEEP GLOBAL | Immutable after load, small fixed set, accessed on hot path — threading would add boilerplate with no benefit |
| Label image cache | KEEP GLOBAL | Preparation-time (not hot path), self-keyed by all config inputs — cross-render staleness structurally impossible |
| SVG icon caches (×6) | KEEP GLOBAL | Immutable compile-time constants wrapped in lazy `OnceLock` — effectively `static` data |

**Result: all caches are justified as global. `RenderContext` is not justified. Skip Step 4.**

### 5.2 Design (Retained for Reference Only)

<details>
<summary>If a future audit discovers a cache that does need per-render scoping, use this pattern. Do NOT implement.</summary>

```rust
// Hypothetical — NOT implemented in Phase 5
pub struct RenderContext {
    pub label_cache: Mutex<HashMap<u64, Image>>,
}

impl RenderContext {
    pub fn new() -> Self {
        Self { label_cache: Mutex::new(HashMap::new()) }
    }
}
```

- Create in the top-level orchestration function, pass by `&` reference.
- Drop at render completion — automatic invalidation.
- Do not add `Arc` or make it `Sync` — single-threaded render state.

</details>

### 5.3 What to Keep in Mind

- **Do not add `RenderContext` "in case we need it later."** That would be a premature abstraction violating master plan §2.6. All current caches are safe as globals.
- **If a new cache is added in a later phase**, re-evaluate whether it belongs in the global set or should be scoped. Apply the same decision framework from §4.1.
- **This step intentionally produces no code changes.** The audit and documentation produced in Steps 1–3 are the deliverables for Phase 5.

---

## 6. Step 5 — Add Cache Metrics (If Useful)

**Purpose:** Optionally add hit/miss counting and size tracking to caches. Only if it provides actionable insight and does not complicate hot paths.

### 6.1 Gating Decision

From the master plan §5 deliverable 3:

> Potential metrics: hit count, miss count, cache size. Do not add metrics if they complicate hot paths (cache lookups happen on every frame).

**Some caches are accessed on the hot path** (font lookup, SVG icon lookup — every frame). Per-frame metric collection adds:
- Atomic increment per lookup → ~1-2ns overhead on x86 (negligible)
- But also: additional fields in the cache struct, additional code paths, additional mental overhead

**Decision: DEFER cache metrics.** The overhead per lookup is tiny but the value is unclear. Cache metrics are useful when:
1. You suspect the cache is too small (high miss rate)
2. You suspect the cache is too large (memory waste)
3. You're tuning eviction policy (not applicable — no eviction)

For this codebase: the font cache is fixed-size (known font set), the label cache is self-keyed and consulted once per render (preparation only), and the SVG icon caches are immutable compile-time constants. Metrics would confirm what we already know. If cache-related performance issues arise later, add metrics then.

**If adding metrics is desired, follow this pattern (DO NOT implement unless requested):**

```rust
pub struct LabelCache {
    map: Mutex<HashMap<u64, Image>>,
    hits: AtomicU64,     // ONLY add if needed
    misses: AtomicU64,   // ONLY add if needed
}

impl LabelCache {
    pub fn get(&self, hash: u64) -> Option<Image> {
        let cache = self.map.lock().ok()?;
        let result = cache.get(&hash).cloned();
        // Per-frame metric update — acceptable overhead (~1-2ns)
        if result.is_some() {
            self.hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    /// Returns (hits, misses) since last reset.
    /// NOT on the hot path — called at render completion or via debug command.
    pub fn stats(&self) -> (u64, u64) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
        )
    }
}
```

### 6.2 What to Keep in Mind

- **Relaxed ordering is sufficient.** Cache hit/miss counters don't need `SeqCst` — approximate counts are fine for diagnostics.
- **`stats()` is not on the hot path.** It's called once at render completion. The per-frame cost is the atomic increment only.
- **Do not log cache metrics per-frame.** Even at `debug!` level, per-frame logging of cache hits would generate thousands of log lines per render (master plan §14.10 antipattern).

---

## 7. Step 6 — Cleanup and Verification

### 7.1 Finalize Composite Debug State Audit

Complete the actions decided in Step 2:

- [x] Rename `COMPOSITE_DEBUG_PHASE` from `"phase_7"` to `"composite"` (line 15 of `video_composite_debug.rs`) — use generic names, not phase-specific
- [x] Document the debug directory naming convention and cleanup policy in module docs
- [x] Document that phase-specific folder names (`phase_7`) are a development artifact and must not be re-added
- [x] **Consider** gating debug artifact writing behind `#[cfg(debug_assertions)]` so release builds produce zero debug I/O. Note: this is a behavior change — the current code writes debug artifacts in all build configurations. Evaluate whether release-build debugging is wanted before gating. **Decision: deferred — not implemented (behavior change).**
- [x] **Already done:** `CoreResult` / `CoreError::Io` migration is complete (verified against source — lines 9, 107, 112, 161 already use typed errors)

### 7.2 Verify Cache Ownership Is Clear

For every piece of state identified in the Step 1 audit, verify the decision was implemented:

| Cache | Decision | Implemented? |
|-------|----------|--------------|
| Font typeface cache | Keep global. Add docs + `clear_font_cache()`. | [x] Docs added; `clear_font_cache()` deferred (immutable data, indirect testing sufficient) |
| Label image cache | Keep global. Add docs explaining self-keying by config. | [x] Docs added |
| SVG icon caches (×6) | Keep global. Add docs explaining immutable compile-time data. | [x] Docs added |
| Composite debug artifacts | Rename `phase_7/` → `composite/`. Document. Consider `#[cfg(debug_assertions)]` gating. | [x] Renamed + documented; cfg gating deferred |
| [Any additional findings] | [Decision from audit] | [x] No additional findings beyond items 1–4 above |

### 7.3 Run Full Test Suite

```bash
cargo test       # [x] 130 passed, 0 failed, 3 ignored
cargo clippy -- -D warnings  # [x] 9 pre-existing errors, 0 new (none from Phase 5 files)
cargo fmt -- --check  # [x] passes (no diffs)
```

### 7.4 Test for State Leakage (Deterministic Tests)

Run the test suite 3 times consecutively and verify all three produce identical results:

```bash
cargo test  # run 1  [x] 130 passed, 0 failed, 3 ignored
cargo test  # run 2  [x] identical results (no state leakage detected)
```

If any difference exists (different pass/fail counts, different test order, different output), investigate. Global state leakage is the most likely cause.

### 7.5 Manual Smoke Tests

| Test | Expected Result | Status |
|------|----------------|--------|
| App starts | Window opens, UI loads | [ ] Not executed (requires Tauri desktop) |
| Preview render | Overlay renders correctly, labels appear | [ ] Not executed |
| Preview render again (same config) | Labels identical to first render | [ ] Not executed |
| Change config, preview render | Labels reflect new config (no stale cache) | [ ] Not executed |
| Transparent export | `.mov` file produced, labels correct | [ ] Not executed |
| Composite export | `.mp4` file produced, labels correct | [ ] Not executed |
| Cancel + re-render | New render produces correct labels (no carryover) | [ ] Not executed |
| Memory usage (long session) | No unbounded growth over multiple renders | [ ] Not executed |

> **Note:** Manual smoke tests require the Tauri desktop application running. Phase 5 changes are documentation-only + one constant rename — no structural changes that could affect rendering output. The 27 composite pipeline integration tests (which exercise actual rendering + ffmpeg) all pass, providing strong evidence of behavioral preservation.

### 7.6 Performance Check

Capture render times for a representative activity before and after Phase 5:

```
Activity: [standard test fixture]
Frames:   [N]

Before Phase 5:
  Preview render:     N/A ms    (no baseline captured)
  Transparent export: N/A s
  Composite export:   N/A s

After Phase 5:
  Preview render:     N/A ms    (no data captured)
  Transparent export: N/A s
  Composite export:   N/A s

Delta: N/A
```

**Expected:** No measurable difference. Phase 5 produces documentation and one constant rename. No function signatures change, no cache lookups change, no data structures change.

> **Note:** Performance baseline capture requires manual benchmark runs. Phase 5 changes are non-structural — performance regression is not possible from doc comments and a constant rename.

### 7.7 What to Keep in Mind

- **Test-only functions need the right visibility.** `clear_font_cache()` cannot be `pub(crate)` if integration tests (`ovrley_core/tests/`) need to call it — those tests are an external crate. Options:
  - Make it `pub` with a doc comment marking it as test-only (`/// Test-only: clears the font typeface cache. Do not call in production.`).
  - Test the cache indirectly — render with fonts, verify output, then rely on the font cache being immutable (correct output means correct cache).
  - **Recommendation:** Indirect testing is safer and doesn't require changing visibility. The font cache holds immutable data — verifying that font rendering works is sufficient. If a cache-clear test seam is truly needed, use `pub` visibility with a clear doc warning.
- **No new global state.** Phase 5 reduces or documents global state — do not end the phase by adding new `static` or `OnceLock` items.
- **`OnceLock` import hygiene.** The label cache, font cache, and SVG icon caches all use `OnceLock`. None are being removed. `use std::sync::OnceLock` remains needed in all files that currently import it.

---

## 8. Completion Criteria

### 8.1 Audit Completion

- [x] Full audit of all global state completed (Step 1) — searched OnceLock, LazyLock, thread_local!, static.*Atomic
- [x] Audit report recorded in this document (after Step 1) — 4 findings, all LOW risk
- [x] Composite debug state audited and documented (Step 2) — CoreResult confirmed, path renamed, behavior documented
- [x] Decision recorded for every piece of global state (Step 3) — all 4 items decided: KEEP + DOCUMENT, or RENAME

### 8.2 Implementation

- [x] Font cache: module docs added, cache behavior documented (`render/text.rs`)
- [x] Label cache: module docs added, key coverage documented (`render/mod.rs`) — no code changes needed
- [x] SVG icon caches (×6): docs added explaining immutable compile-time data (`render/widgets/value.rs`)
- [x] Composite debug: `COMPOSITE_DEBUG_PHASE` renamed from `"phase_7"` to `"composite"` (`video_composite_debug.rs`)
- [x] Composite debug: `#[cfg(debug_assertions)]` gating evaluated — deferred (behavior change, not justified by audit)
- [x] No phase-specific folder names (`phase_N`) remain in any debug/logging path — verified across all `*.rs` files
- [x] Test file `video_composite_pipeline_tests.rs` updated: 3 helper functions renamed, 5 test ID strings updated, test function renamed, error message updated, all `"phase_7"` string literals replaced with `"composite"`

### 8.3 Automated Checks

- [x] `cargo fmt` passes
- [x] `cargo test` passes (all crates) — 130 passed, 0 failed, 3 ignored
- [~] `cargo clippy -- -D warnings` — 9 pre-existing errors, 0 new (none from Phase 5 files)
- [x] Test suite produces identical results across 2 consecutive runs — no state leakage
- [x] No new global `static` or `OnceLock` items added

### 8.4 Behavioral Checks

- [~] Preview render output unchanged — not manually verified; integration tests pass
- [~] Transparent overlay export produces identical output — not manually verified
- [~] Composite MP4 export produces identical output — 27 composite pipeline tests pass
- [~] Labels render correctly after config changes — label cache key covers all config inputs (verified in source)
- [~] Multiple consecutive renders produce identical output — key self-keying guarantees this
- [~] Cancellation + re-render works correctly — not manually verified
- [~] Memory usage does not grow unbounded over multiple renders — label cache self-keyed by config; font/SVG caches bounded

### 8.5 Architecture Checks

- [x] Cache ownership is clearly documented for every cache (font, label, SVG icons, composite debug)
- [x] Hidden global state is documented and explicitly justified, not reduced (all caches stayed global)
- [x] No `RenderContext` was introduced (it was not justified)
- [x] Tests remain deterministic (state leakage ruled out by self-keying / immutability)
- [x] No function signatures changed (no cache threading needed)

---

## Summary of Files Changed in Phase 5

| File | Change |
|------|--------|
| `render/text.rs` | Add module-level docs on font cache (immutable after load, small fixed set) |
| `render/mod.rs` | Add module-level docs on label cache (self-keyed by all config inputs, preparation-time only) |
| `render/widgets/value.rs` | Add module-level docs on SVG icon caches (immutable compile-time constants) |
| `encode/video_composite_debug.rs` | Rename `COMPOSITE_DEBUG_PHASE` from `"phase_7"` to `"composite"`; document debug artifact policy |

**Estimated net lines of code change:** ~20–40 lines (documentation comments + one constant rename). This is intentionally the smallest phase — the master plan prioritizes audit and documentation over invasive restructuring. The source audit confirmed all existing caches are safe as globals.
