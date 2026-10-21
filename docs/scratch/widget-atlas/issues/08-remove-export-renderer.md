# 08 — Remove the export's own widget renderer

**What to build:** The export's separate full-frame drawing path and its full-frame static layer are deleted. Preview and export draw every widget through one engine. The golden reference that compared engine output to the old renderer becomes a comparison between atlas output and the single-slot full-frame output, so coverage of the export stays intact.

**Blocked by:** 05 — CPU-overlay encoders export through the atlas

**Status:** ready-for-agent

- [ ] No export code draws widgets outside the shared widget engine.
- [ ] Preview/native parity tests that only guarded the two implementations are removed or rewritten against the single engine.
- [ ] Golden tests compare atlas frames against full-frame plan frames for the fixture templates.
- [ ] All composite, transparent and preview tests pass, and the wasm build compiles.
