Status: ready-for-agent

## Parent

`.agents/scratch/animated-gauges/PRD.md`

## What to build

Add `display_type` field to the `ValueConfig` struct in the Rust backend config schema. This field controls how a metric value widget is rendered: `"text"` (current behavior), `"linear"`, `"bars"`, `"arc"`, `"corner"`, `"heading_tape"` will be default options for metric widgets. HOWEVER, not all metric widgets will allow all these types, the default scenario will be that a metric widget, unless state otherwise, will allow text/linear/bars/arc/coners. Others will allow different combos that need to be defined separately somwhere. This will ensure we don't define every widget separately, only the "exceptions". Do not invent any new widget types. All these are subtypes of the existing "value" widget, and the frontend will conditionally render different SVG structures based on the `display_type` field. Ignore 'heading' widget for now, we are focusing on plumbing so we can rewire the 'heading' widget to use this new field in a future slice.

The field must be optional with `#[serde(default)]` so that existing templates that don't include this field deserialize cleanly and default to `"text"`. No rendering changes are made in this slice — this is purely schema and deserialization plumbing.

A corresponding default value must be added to the frontend `widgetDefaults.js` so that newly created value widgets include `display_type: "text"`.

## Acceptance criteria

- [x] `ValueConfig` in `config/mod.rs` has a new `display_type: Option<String>` (or enum) field with `#[serde(default)]`
- [x] A `DisplayType` enum is created with variants `Text`, `Linear`, `Bars`, `Arc`, `Corner`, `Tape` and derives `Clone`, `Debug`, `Deserialize`, `Serialize`
- [x] The enum's `Deserialize` implementation defaults to `Text` when the field is absent or null
- [x] Existing templates without `display_type` parse successfully and produce `DisplayType::Text`
- [x] Templates with `display_type: "linear"` (and other variants) parse successfully
- [x] Unknown `display_type` values are handled gracefully (defaulted to `Text`)
- [x] Frontend `widgetDefaults.js` includes `display_type: "text"` in the value widget defaults
- [x] Rust unit test verifies deserialization round-trip for all variants and default behavior

## Blocked by

None - can start immediately
