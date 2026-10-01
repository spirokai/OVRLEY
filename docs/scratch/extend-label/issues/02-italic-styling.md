---
Status: ready-for-agent
---

# 02 — Add italic styling to labels

**What to build:** Let users toggle italic styling for a label. The saved setting must affect the JSX preview and Rust output consistently, including shadows, borders, and selection bounds.

**Blocked by:** None — can start immediately. This property can be implemented and verified independently of font weight and letter spacing. The user supplies any replacement font assets manually.

**IMPORTANT:** Do not introduce parallel or duplicate workflows, validations or state. Extend the existing infrastructure, validation and migration to support the new property.

- [ ] Add the canonical required boolean property `italic`, defaulting to false, to label defaults and durable configuration.
- [ ] Add an accessible, translated italic toggle to label typography controls using the existing widget update, commit, and undo behavior. Keep stateful presentation logic in hooks and computation in utilities.
- [ ] Use genuine italic support when the selected font provides it through an italic face or an `ital`/`slnt` variation. Define how the toggle selects an italic or slanted instance from that font's capabilities. Variable weight support alone must not be treated as italic support.
- [ ] When genuine italic support is absent, apply an explicit synthetic-slant policy consistently in browser preview and Rust output. This is intentional presentation behavior, not recovery from malformed configuration. Test its visual agreement rather than assuming the renderers synthesize identical results.
- [ ] Apply the same italic settings to SVG fill, border, shadow, canvas measurement, and font readiness requests. Selection bounds include italic overhangs and refresh after font loading.
- [ ] Carry italic styling through Rust ingress validation to measurement and drawing. Font/variation caches and static label image caches distinguish upright and italic instances.
- [ ] Validate the property once at ingress. Canonical render input requires a boolean; saved input with a malformed present value fails loudly. Consumers do not coerce or default validated fields.
- [ ] Extend the existing project migration to v3 rather than introducing v4. Also upgrade development projects already stamped v3 and existing v2 templates at their load boundaries. Add false only when the legacy property is absent, preserve explicit true/false, and reject malformed present values. Migration is idempotent, does not rewrite files on read, and subsequent saves include the property explicitly.
- [ ] Update bundled template configurations and canonical test fixtures to include the property. Changing the toggle preserves any existing weight and spacing values; where those properties have landed, verify their combinations.
- [ ] Verify upright/italic output, actual italic or slant support, the synthetic policy, overhang bounds, shadows/borders, save/load, legacy migrations, malformed input rejection, and cache invalidation. Run relevant frontend and Rust tests and frontend lint; do not run a build without the user's permission.
