# Widget Icons

Shared metric SVG icons used by both the Rust backend (`include_str!`) and the React frontend.

Each file maps to one `MetricIconKind`:

| SVG File                  | MetricIconKind | MetricKind    |
| ------------------------- | -------------- | ------------- |
| `widget-speed.svg`        | `Gauge`        | `Speed`       |
| `widget-heartrate.svg`    | `Heart`        | `Heartrate`   |
| `widget-cadence.svg`      | `RefreshCw`    | `Cadence`     |
| `widget-power.svg`        | `Zap`          | `Power`       |
| `widget-time.svg`         | `Clock3`       | `Time`        |
| `widget-temperature.svg`  | `Thermometer`  | `Temperature` |

## Usage Notes

- These SVGs use a deliberately small subset (paths, lines, circles) compatible with
  the Rust icon parser in `ovrley_core/src/render/widgets/value/svg.rs`.
- The Rust backend embeds them at compile time via `include_str!`.
- The frontend imports them via `?raw` query suffix for SVG markup access.
- Do not add inline styles, complex transforms, or unsupported SVG elements
  without updating the Rust parser.

## Source

Originally located at `app/src/components/widgets/icons/`. These six metric icons
are shared between backend and frontend. Other widget icons (course, elevation,
gradient, label) remain frontend-only in the original location.
