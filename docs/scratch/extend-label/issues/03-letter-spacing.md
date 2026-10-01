---
Status: ready-for-agent
---

# 03 — Add letter spacing to labels

**What to build:** Let users increase or decrease spacing between a label's letters. The saved spacing must affect the JSX preview, selection bounds, Rust preview, and exported overlay consistently.

**Blocked by:** None — can start immediately. This property can be implemented and verified independently of font weight and italic styling.

**IMPORTANT:** Do not introduce parallel or duplicate workflows, validations or state. Extend the existing infrastructure, validation and migration to support the new property.

- [ ] Add the canonical required numeric property `letter_spacing`, defaulting to 0, to label defaults and durable configuration. Define its units as unscaled overlay pixels; scene scaling scales spacing together with the text.
- [ ] Add an accessible, translated numeric letter-spacing control with a pixel readout. Support positive, zero, negative, and fractional values through the existing widget update, live preview, commit, and undo behavior. Keep stateful presentation logic in hooks and computation in utilities.
- [ ] Define one spacing convention for browser and Rust layout, including whether a trailing advance is counted. Preserve the distinction between text advances and visible ink bounds; empty and single-character labels have no inter-character gaps.
- [ ] Apply the same spacing to SVG fill, border, and shadow layers, and to canvas measurement. Selection bounds and baseline calculations describe the rendered text, including tightly overlapping glyphs at negative spacing.
- [ ] Position text in Rust using the same spacing convention and scene scale. Measurement, fill, border, and shadow reuse the same laid-out glyph positions rather than applying spacing only during drawing.
- [ ] Preserve Unicode text: do not split UTF-16 code units or insert spacing inside combining-character clusters. Include whitespace, accented text, and supplementary characters in verification, and retain the existing text layout behavior at zero spacing.
- [ ] Verify that static label image caching distinguishes different spacing values. Changing spacing preserves any existing weight and italic values; where those properties have landed, verify their combinations.
- [ ] Validate the property once at ingress. Canonical render input requires a finite number; malformed present saved values, including strings and null, fail loudly. Consumers do not coerce or default validated fields.
- [ ] Extend the existing project migration to v3 rather than introducing v4. Also upgrade development projects already stamped v3 and existing v2 templates at their load boundaries. Add 0 only when the legacy property is absent, preserve valid explicit spacing, and reject malformed present values. Migration is idempotent, does not rewrite files on read, and subsequent saves include the property explicitly.
- [ ] Update bundled template configurations and canonical test fixtures to include the property.
- [ ] Verify zero/positive/negative/fractional spacing, scene scaling, empty and single-character text, Unicode cases, selection bounds, shadows/borders, save/load, legacy migrations, malformed input rejection, and cache invalidation. Compare JSX and Rust text extents. Run relevant frontend and Rust tests and frontend lint; do not run a build without the user's permission.
