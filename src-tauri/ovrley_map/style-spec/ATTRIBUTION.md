# Embedded style-spec reference

`v8.json` is the MapLibre style specification reference from `@maplibre/maplibre-gl-style-spec`, at
the version in `VERSION`: the one the frontend's maplibre-gl ships. Documentation fields (`doc`,
`example`, `sdk-support`, `units`, `requires`) are removed; everything validation reads is kept.

The validator in `src/style_spec.rs` is a port of the same package's `validateStyleMin` (`src/validate/`).

- **Source**: <https://github.com/maplibre/maplibre-style-spec>
- **License**: BSD-3-Clause, see `LICENSE.txt`. Copyright (c) 2020, MapLibre contributors.

To refresh after updating maplibre-gl, run `node scripts/prune-map-style-spec.mjs` from the
repository root.
