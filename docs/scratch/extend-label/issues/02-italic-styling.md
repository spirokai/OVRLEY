---
Status: ready-for-human
---

# 02 — Add italic styling to labels

**What to build:** Let users toggle italic styling for a label. The saved setting must affect the JSX preview and Rust output consistently, including shadows, borders, and selection bounds.

**Blocked by:** None. Implementation is ready for human verification. The user supplies any replacement font assets manually.

**IMPORTANT:** Do not introduce parallel or duplicate workflows, validations or state. Extend the existing infrastructure, validation and migration to support the new property.

- [x] Add the canonical required boolean property `italic`, defaulting to false, to label defaults and durable configuration.
- [x] Add an accessible, translated italic toggle to label typography controls using the existing widget update, commit, and undo behavior. Keep stateful presentation logic in hooks and computation in utilities.
- [x] Prefer genuine italic support through an italic face or an `ital`/`slnt` variation. Also allow OS-advertised simulated slants, with browser style synthesis for the JSX preview. Variable weight support alone does not enable italic. The accessible, translated toggle uses the compact alignment-toggle styling and remains disabled for families without any advertised italic/slant capability.
- [x] For OS-simulated slants, register the upright source and let the browser synthesize italic; Rust uses the Windows/Skia simulated face. Families without advertised italic/slant support render upright, and switching to one clears `italic` in the same committed action. This policy reflects the user's latest authorization and supersedes the earlier prohibition on synthesis.
- [x] Apply the same italic settings to SVG fill, border, shadow, canvas measurement, and font readiness requests. Selection bounds include italic overhangs and refresh after font loading.
- [x] Carry italic styling through Rust ingress validation to measurement and drawing. Font/variation caches and static label image caches distinguish upright and italic instances.
- [x] Validate the property once at ingress. Canonical render input requires a boolean; saved input with a malformed present value fails loudly. Consumers do not coerce or default validated fields.
- [x] Extend the existing project migration to v3 rather than introducing v4. Also upgrade development projects already stamped v3 and existing v2 templates at their load boundaries. Add false only when the legacy property is absent, preserve explicit true/false, and reject malformed present values. Migration is idempotent, does not rewrite files on read, and subsequent saves include the property explicitly.
- [x] Update bundled template configurations and canonical test fixtures to include the property. Changing the toggle preserves any existing weight and spacing values; where those properties have landed, verify their combinations.
- [x] Verify the original implementation's upright/italic output, genuine support, unsupported-font behavior, overhang bounds, shadows/borders, save/load, legacy migrations, malformed input rejection, and cache invalidation. Investigate Bebas Neue with font metadata and browser geometry probes. Pixel parity testing is excluded by the user.
- [x] Implement only crucial and essential tests, and update the existing tests where necesssary. Do not spam tests. Run relevant frontend and Rust tests and frontend lint; do not run a build without the user's permission.
- [ ] Confirm Bebas Neue's synthesized italic in the app after reloading cached browser font registrations. The final synthesis change has not been tested, at the user's request.

## Implementation status — 2026-10-02

Implemented; status is ready-for-human pending manual confirmation of Bebas Neue after an app reload. Label defaults, durable configuration, v3 migration, preview measurement/font readiness, Rust rendering, and caches carry the required boolean `italic`.

Genuine italic faces and `ital` variations are preferred over oblique faces and `slnt` variations. `ital` selects 1; `slnt` selects −12° (or +12° for a positive-only axis), clamped to the font's range. Other axes keep their defaults.

Bebas Neue's installed font is regular-only; Windows supplies simulated slants. Browser registration skips slanted system entries sharing an upright face's local identity without an `ital`/`slnt` variation. Their toggle remains enabled, and the browser synthesizes from the upright source while Rust uses the Windows/Skia simulated face. SVG fill, border, shadow, and canvas measurement permit style synthesis; browser synthetic bold remains disabled. Fonts without advertised italic/slant support keep a disabled toggle and render upright. Synthesized angles can differ between renderers.

The original implementation passed focused frontend, Rust typography/configuration, and project migration tests plus frontend lint. Tauri tests excluded packaging resources because `THIRD_PARTY_NOTICES.txt` is missing. The final synthesis change passed focused lint and formatting; tests were not run for it at the user's request. No pixel parity tests, build, full test suite, or code review were run. Reload the app before manual verification to refresh cached font registrations.
