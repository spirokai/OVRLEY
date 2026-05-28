Status: ready-for-agent

## Parent

`.agents/scratch/animated-gauges/PRD.md`

## What to build

Add `display_type` field to the `ValueConfig` struct in the Rust backend config schema. This field controls how a metric value widget is rendered: `"text"` (current behavior), `"linear"`, `"bars"`, `"arc"`, or `"corner"`.

The field must be optional with `#[serde(default)]` so that existing templates that don't include this field deserialize cleanly and default to `"text"`. No rendering changes are made in this slice — this is purely schema and deserialization plumbing.

A corresponding default value must be added to the frontend `widgetDefaults.js` so that newly created value widgets include `display_type: "text"`.

## Acceptance criteria

- [ ] `ValueConfig` in `config/mod.rs` has a new `display_type: Option<String>` (or enum) field with `#[serde(default)]`
- [ ] A `DisplayType` enum is created with variants `Text`, `Linear`, `Bars`, `Arc`, `Corner` and derives `Clone`, `Debug`, `Deserialize`, `Serialize`
- [ ] The enum's `Deserialize` implementation defaults to `Text` when the field is absent or null
- [ ] Existing templates without `display_type` parse successfully and produce `DisplayType::Text`
- [ ] Templates with `display_type: "linear"` (and other variants) parse successfully
- [ ] Unknown `display_type` values are handled gracefully (either rejected with a clear error or defaulted to `Text`)
- [ ] Frontend `widgetDefaults.js` includes `display_type: "text"` in the value widget defaults
- [ ] Rust unit test verifies deserialization round-trip for all variants and default behavior

## Blocked by

None - can start immediately
