# Widget Icons

Shared metric SVG icons used by both the Rust backend (`include_str!`) and the React frontend.

## Active Shared Metric Icons

| SVG File                 | MetricIconKind | MetricKind    |
| ------------------------ | -------------- | ------------- |
| `widget-speed.svg`       | `Gauge`        | `Speed`       |
| `widget-heartrate.svg`   | `Heart`        | `Heartrate`   |
| `widget-cadence.svg`     | `RefreshCw`    | `Cadence`     |
| `widget-power.svg`       | `Zap`          | `Power`       |
| `widget-time.svg`        | `Clock3`       | `Time`        |
| `widget-temperature.svg` | `Thermometer`  | `Temperature` |

## Planned Standard Metric Icon Catalog

| SVG File                            | Widget Type             | Source Type | Source Name    |
| ----------------------------------- | ----------------------- | ----------- | -------------- |
| `widget-pace.svg`                   | `pace`                  | `lucide`    | `Footprints`   |
| `widget-air-pressure.svg`           | `air_pressure`          | `lucide`    | `Wind`         |
| `widget-left-right-balance.svg`     | `left_right_balance`    | `lucide`    | `Scale`        |
| `widget-stride-length.svg`          | `stride_length`         | `lucide`    | `Ruler`        |
| `widget-stroke-rate.svg`            | `stroke_rate`           | `lucide`    | `Waves`        |
| `widget-vertical-speed.svg`         | `vertical_speed`        | `lucide`    | `TrendingUp`   |
| `widget-vertical-ratio.svg`         | `vertical_ratio`        | `lucide`    | `Percent`      |
| `widget-core-temperature.svg`       | `core_temperature`      | `lucide`    | `Thermometer`  |
| `widget-g-force.svg`                | `g_force`               | `custom`    |                |
| `widget-ground-contact-time.svg`    | `ground_contact_time`   | `custom`    |                |
| `widget-torque.svg`                 | `torque`                | `custom`    |                |
| `widget-gear-position.svg`          | `gear_position`         | `custom`    |                |

## Usage Notes

- These SVGs use a deliberately small subset (paths, lines, circles, `fill="currentColor"`, `fill="none"`, and optional per-element `stroke-width`) compatible with the Rust icon parser in `ovrley_core/src/render/widgets/value/svg.rs`.
- The Rust backend embeds shared metric icons at compile time via `include_str!`.
- The frontend imports shared metric icons via the `?raw` query suffix for SVG markup access.
- Do not add inline styles, complex transforms, or unsupported SVG elements without updating the Rust parser.

## Source

The active shared metric icons were originally moved out of `app/src/components/widgets/icons/` so backend export and frontend preview render from the same files.

The planned standard metric catalog lives here now as well, so future widget work extends the same shared asset registry instead of introducing a second icon system.
