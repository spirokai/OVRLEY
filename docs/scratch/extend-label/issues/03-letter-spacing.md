---
Status: ready-for-human
---

# 03 — Add letter spacing to labels

**What to build:** Let users increase or decrease spacing between a label's letters. The saved spacing must affect the JSX preview, selection bounds, Rust preview, and exported overlay consistently.

**Blocked by:** None — can start immediately. This property can be implemented and verified independently of font weight and italic styling.

**IMPORTANT:** Do not introduce parallel or duplicate workflows, validations or state. Extend the existing infrastructure, validation and migration to support the new property.

- [x] Add the canonical required numeric property `letter_spacing`, defaulting to 0, to label defaults and durable configuration. Define its units as a percentage of font size; font-size changes and scene scaling scale spacing together with the text.
- [x] Add an accessible, translated letter-spacing slider with a percentage readout. Support positive, zero, negative, and fractional values through the existing widget update, live preview, commit, and undo behavior. Keep stateful presentation logic in hooks and computation in utilities.
- [x] Define one spacing convention for browser and Rust layout, including whether a trailing advance is counted. Preserve the distinction between text advances and visible ink bounds; empty and single-character labels have no inter-character gaps.
- [x] Apply the same spacing to SVG fill, border, and shadow layers, and to canvas measurement. Selection bounds and baseline calculations describe the rendered text, including tightly overlapping glyphs at negative spacing.
- [x] Position text in Rust using the same spacing convention and scene scale. Measurement, fill, border, and shadow reuse the same laid-out glyph positions rather than applying spacing only during drawing.
- [x] Preserve Unicode text: do not split UTF-16 code units or insert spacing inside combining-character clusters. Include whitespace, accented text, and supplementary characters in verification, and retain the existing text layout behavior at zero spacing.
- [x] Verify that static label image caching distinguishes different spacing values. Changing spacing preserves any existing weight and italic values; where those properties have landed, verify their combinations.
- [x] Validate the property once at ingress. Canonical render input requires a finite number; malformed present saved values, including strings and null, fail loudly. Consumers do not coerce or default validated fields.
- [x] Extend the existing project migration to v3 rather than introducing v4. Also upgrade development projects already stamped v3 and existing v2 templates at their load boundaries. Add 0 only when the legacy property is absent, preserve valid explicit spacing, and reject malformed present values. Migration is idempotent, does not rewrite files on read, and subsequent saves include the property explicitly.
- [x] Update bundled template configurations and canonical test fixtures to include the property.
- [x] Verify zero/positive/negative/fractional spacing, scene scaling, empty and single-character text, Unicode cases, selection bounds, shadows/borders, save/load, legacy migrations, malformed input rejection, and cache invalidation.
- [x] Implement ONLY crucial and essential tests, and update the existing tests where necesssary. Do not spam tests. Run relevant frontend and Rust tests and frontend lint; do not run a build without the user's permission.

## Implementation status — 2026-10-02

Implemented; status is ready-for-human for manual verification. The translated, accessible letter-spacing slider uses the existing live draft, commit, and undo paths, with a fractional percentage readout, a range of -100% to 100%, and 0.1 percentage-point steps. `NumberField` is unchanged.

The canonical required field is `letter_spacing`, stored as a percentage of font size per the user's revised requirement. Both renderers calculate the pixel gap as `font_size * letter_spacing / 100`, then apply scene scale. For example, 10% gives a 6 px gap at font size 60 and a 12 px gap at font size 120. Changing font size or resizing preserves the stored percentage. No conversion migration is needed because this is still a development feature with no existing spacing data.

Zero retains whole-string layout. Nonzero spacing positions intact extended grapheme clusters: each run starts after the preceding run's advance plus the pixel gap, with no trailing gap. Empty and single-cluster text have no gaps. Ink bounds are the union of positioned runs, independently of the signed total advance, so strongly negative spacing still produces correct selection bounds and baselines. Whitespace contributes advance without ink.

SVG fill, border, and shadow reuse canvas-measured run positions. Rust measurement and drawing share one layout function; drawing reuses one Skia text blob across all paint layers. Preview and export share the static layer, whose existing cache key includes the validated spacing field.

Existing v3 migration and v2 template load boundaries add 0 only for absent legacy spacing. Explicit values survive save/load; malformed present values fail at ingress. Project reads leave archive bytes unchanged. Bundled templates and canonical render fixtures include explicit spacing.

Initial validation passed: 45 focused frontend tests across six files; 29 Rust typography/configuration tests; two project typography migration/archive tests; frontend lint; and git diff --check. Project tests used a temporary TAURI_CONFIG override excluding packaging resources because THIRD_PARTY_NOTICES.txt is absent. The percentage update passed 11 focused frontend tests across preview, editor, and undo behavior; 10 Rust typography tests, including different font sizes and scene scaling; frontend lint; and git diff --check. No build, full test suite, pixel parity tests, or code review were run.
