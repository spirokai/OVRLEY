# Widget Icons

The SVG icons of the widgets, metrics, displays and display modes, for the editor and the export alike. The manifests are the index: `assets/standard-metrics.json` and `assets/standard-widgets.json` name each file under an `icon.assetFile`. Files are named `widget-<type>.svg`, `display-type-<display>.svg` and `display-mode-<mode>.svg`.

## Taken from Lucide

Recorded provenance (ISC; see `THIRD_PARTY_NOTICES.txt`). This is not a list of every icon.

| SVG File | Lucide icon |
| -------- | ----------- |
| `widget-speed.svg` | `Gauge` |
| `widget-heartrate.svg` | `Heart` |
| `widget-cadence.svg` | `RefreshCw` |
| `widget-power.svg` | `Zap` |
| `widget-time.svg` | `Clock3` |
| `widget-temperature.svg` | `Thermometer` |
| `widget-pace.svg` | `Footprints` |
| `widget-air-pressure.svg` | `Wind` |
| `widget-left-right-balance.svg` | `Scale` |
| `widget-stride-length.svg` | `Ruler` |
| `widget-stroke-rate.svg` | `Waves` |
| `widget-vertical-speed.svg` | `TrendingUp` |
| `widget-vertical-ratio.svg` | `Percent` |
| `widget-core-temperature.svg` | `Thermometer` |
| `widget-satellite.svg` | `Satellite` |
| `widget-house.svg` | `House` |
| `widget-arrow-up-narrow-wide.svg` | `ArrowUpNarrowWide` |

## Usage Notes

- These SVGs use a deliberately small subset (paths, lines, circles, `fill="currentColor"`, `fill="none"`, and optional per-element `stroke-width`) compatible with the Rust icon parser in `crates/ovrley_widgets/src/render/svg.rs`.
- `crates/ovrley_widgets/build.rs` embeds the icons the manifests reference at compile time, so the editor's preview and the export draw the same files.
- Do not add inline styles, complex transforms, or unsupported SVG elements without updating the Rust parser.
