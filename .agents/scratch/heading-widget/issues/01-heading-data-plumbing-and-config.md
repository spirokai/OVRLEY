Status: ready-for-agent

# 01 — Heading Data Plumbing + Config Struct

## Parent

[Heading Tape Widget PRD](../PRD.md)

## What to build

Wire heading data end-to-end through the Rust backend pipeline and define the full widget configuration schema.

The frontend FIT and GPX parsers already extract heading and include it in the JSON payload sent to the backend. The gap is on the Rust side: heading has no schema representation, no dense activity integration, and no config struct for a heading widget.

This slice adds `MetricKind::Heading` to the metric enum, heading fields to `ParsedActivity`, `DenseSeriesReport`, `TrimmedActivity`, and `RenderDataRequirements`, wires heading through the trim→densify pipeline with forward-fill of nulls (so the widget holds last known heading during gaps), and defines the complete `HeadingWidgetConfig` struct with all ~30 parameters documented in the PRD. The config is integrated into `RenderConfig.plots` deserialization.

No rendering happens in this slice. Verification is through backend tests: serialization round-trips, data flow from parsed JSON through trimming and densification, and config deserialization.

## Acceptance criteria

- [ ] `MetricKind::Heading` variant exists and serializes/deserializes correctly
- [ ] `ParsedActivity` accepts `heading` from the JSON payload (extracted from the `extra` serde map or directly parsed)
- [ ] `DenseSeriesReport` and `TrimmedActivity` carry `heading` as `Vec<Option<f64>>`
- [ ] `RenderDataRequirements` includes `heading: bool`
- [ ] Heading flows through trim→densify: nulls are forward-filled (last known valid value carried forward)
- [ ] `HeadingWidgetConfig` struct exists with all PRD-specified fields, derives Serialize/Deserialize/Clone, and preserves `#[serde(flatten)] pub extra` for forward compatibility
- [ ] `RenderConfig.plots` deserialization recognizes a `heading` plot entry
- [ ] Backend tests pass: `cargo test` covers metric kind serde, activity data flow, and config deserialization

## Blocked by

None — can start immediately.
