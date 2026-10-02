---
Status: ready-for-human
---

# 01 — Add variable font weight control to labels

**What to build:** Let users adjust a label's font weight with a slider and numeric readout. The selected weight must be saved and applied consistently to the JSX preview, selection bounds, Rust preview, and exported overlay. New and migrated labels use regular weight 400.

**Blocked by:** None — can start immediately. The user supplies replacement font assets manually; downloading or replacing fonts is outside this ticket. Final verification against those assets requires their availability.

**IMPORTANT:** Do not introduce parallel or duplicate workflows, validations or state. Extend the existing infrastructure, validation and migration to support the new property.

- [x] Add the canonical required property `font_weight`, defaulting to 400, to label defaults and durable configuration. Do not add a separate `bold` property.
- [x] Detect capabilities separately for each resolved font face. Read the variable font's `wght` axis minimum, default, and maximum through Skia's variation metadata APIs; do not infer ranges from filenames or assume every font supports 100–900.
- [x] Extend the existing font catalog with one canonical capability shape and update its consumers. Discover bundled font capabilities when the app first requests its font catalog and cache them for the session. Resolve system font capabilities when needed. Discovery must not depend on a build step or run on every rendered frame; restarting the app refreshes replaced assets.
- [x] Configure browser font registration from the same font identity and supported range used by Rust. Preserve existing font references where replacements represent the same family; explicitly migrate changed identities without runtime aliases.
- [x] Show a weight slider for variable fonts with a visible weight axis, using the detected minimum and maximum, step 1, and a numeric readout. Static fonts keep the weight slider visible but disabled and expose supported face weights through the selector. Changing font explicitly updates an unsupported current weight as part of that user action.
- [x] Use the existing widget update, live preview, commit, and undo behavior. Keep stateful presentation logic in hooks and computation in utilities.
- [x] Apply the requested weight to SVG text and every shadow/border layer. Canvas measurement and font readiness requests use the same weight; selection bounds refresh after the requested font becomes available.
- [x] Resolve the requested `wght` variation in Rust for both measurement and drawing. Keep other variation axes consistent between browser and Rust, including optical sizing. Cache resolved variations by font identity and relevant variation coordinates, and verify that static label image caching distinguishes different weights.
- [x] Validate the property once at each genuine ingress boundary. Reject missing fields in canonical render input and malformed present values in saved input, including strings, null, and invalid numeric weights. Consumers use the validated property without coercion or repair defaults. Any supported-face matching for static fonts is an explicit font-resolution policy shared by both renderers.
- [x] Extend the existing project migration to v3 rather than introducing v4. Also upgrade development projects already stamped v3 and existing v2 templates at their load boundaries. Add 400 only when the legacy property is absent, preserve valid explicit values, and reject malformed present values. Migration is idempotent, does not rewrite files on read, and subsequent saves include the property explicitly.
- [x] Update bundled template configurations and canonical test fixtures to include the property. Replacement variable font assets intended to support the regular default must include weight 400 in their range.
- [ ] Verify differing per-font ranges, an intermediate weight, endpoint weights, font switching, save/load, legacy migrations, malformed input rejection, selection bounds, and cache invalidation. Check JSX and Rust output using the same replacement font assets. Run relevant frontend and Rust tests and frontend lint; do not run a build without the user's permission.

## Implementation notes — 2026-10-01

Implementation complete. Focused frontend tests, font catalog/rendering and configuration tests, project migration tests, frontend lint, and the Rust/Tauri compile check passed. The compile check excluded packaging resources because the existing `THIRD_PARTY_NOTICES.txt` resource is missing; no build was run.

The final verification checkbox remains open only for visual JSX/Rust output comparison. Rendering parity tests were excluded at the user's request, and the new parity helpers were removed. No code-review was run. Tests were trimmed to the critical feature contracts.

Bundled families are discovered from font metadata without an `assets/fonts.json` inventory. Existing same-family IDs remain stable; saved `Inter ExtraBold.ttf` references migrate to `Inter.ttf`.

Following the backup-font audit, non-label widget text uses a trial weight of 700. Browser drawing, measurement, font readiness, and Rust rendering share `typography.widgetFontWeight` in `assets/standard-widgets.json`. Label widgets retain their explicit selected weight and regular 400 default.
