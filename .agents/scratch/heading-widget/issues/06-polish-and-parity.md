Status: ready-for-agent

# 06 — Polish and Parity Verification

## Parent

[Heading Tape Widget PRD](../PRD.md)

## What to build

Final verification pass: confirm frontend preview matches backend export pixel-for-pixel, handle edge cases, and ensure comprehensive test coverage.

Specific areas to verify:

- **Parity**: render the same heading widget config through both the frontend preview (SVG) and backend export (Skia → PNG). Compare outputs at multiple heading values, especially near the 0°/360° wrap boundary. Tick positions, label text, cardinal override, indicator style/placement, and shadow rendering must match.
- **Null heading data**: confirm that when heading is `None` at a frame, the tape holds the last known value and does not glitch, disappear, or jump.
- **Wrap boundary**: verify seamless rendering at heading 359° → 0° and 0° → 359° — no gaps, no doubled ticks, labels transition smoothly.
- **Shadows**: verify shadow appearance matches other graphical widgets (route, elevation) — same distance/strength/color behavior, same visual quality.
- **Test coverage gaps**: add any missing tests for geometry edge cases (heading at exact cardinal positions, heading at half-tick positions), config boundary values (negative px/°, zero height widget), and render output validation.

This is a HITL slice: manual visual inspection is required for the final parity signoff.

## Acceptance criteria

- [ ] Frontend preview and backend export produce identical visual output for the same config at multiple heading values
- [ ] Null heading data freezes the tape at the last known value (no visual glitch)
- [ ] 0°/360° wrap boundary renders seamlessly in both preview and export
- [ ] Shadow rendering matches existing graphical widget conventions
- [ ] Geometry test coverage includes edge cases: heading at cardinal positions, wrap boundary, negative offsets
- [ ] Render test coverage includes: baseline output, null data frame, wrap boundary frame
- [ ] Manual parity signoff recorded in PRD comments

## Blocked by

- [05 — Widget Editor and Drawer Registration](./05-widget-editor-and-drawer.md)
