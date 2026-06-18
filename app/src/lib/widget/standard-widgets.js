/**
 * @file Standard Widget Constants
 *
 * All constants derived from the canonical manifest files at
 * `assets/standard-metrics.json` and `assets/standard-widgets.json`.
 * Every export is a frozen object/array keyed off the manifest — no
 * helper functions, no runtime logic beyond construction.
 *
 * @module standard-widgets
 */

import standardWidgetsManifest from '../../../../assets/standard-widgets.json'
import standardMetricsManifest from '../../../../assets/standard-metrics.json'

// ---------------------------------------------------------------------------
// Plot widget defaults (assets/standard-widgets.json)
// ---------------------------------------------------------------------------

/** Defaults for course plot widgets. */
export const COURSE_PLOT_DEFAULTS = Object.freeze({ ...standardWidgetsManifest.plot.course })

/** Defaults for elevation plot widgets. */
export const ELEVATION_PLOT_DEFAULTS = Object.freeze({ ...standardWidgetsManifest.plot.elevation })

/** Defaults for gradient metric value widgets. */
export const GRADIENT_DEFAULTS = Object.freeze({ ...standardWidgetsManifest.gradient })

// ---------------------------------------------------------------------------
// Metric definitions (assets/standard-metrics.json)
// ---------------------------------------------------------------------------

/** Map of type -> definition for O(1) lookups. */
export const STANDARD_METRIC_DEFINITIONS = Object.freeze(
  Object.fromEntries(standardMetricsManifest.definitions.map((definition) => [definition.type, Object.freeze(definition)])),
)

/** Metric types marked as `current` — actively shipping widget types. */
export const CURRENT_STANDARD_METRIC_WIDGET_TYPES = Object.freeze(
  standardMetricsManifest.definitions.filter((definition) => definition.current).map((definition) => definition.type),
)

/** Every metric type defined in the manifest (both current and planned). */
export const STANDARD_METRIC_WIDGET_TYPES = Object.freeze(standardMetricsManifest.definitions.map((definition) => definition.type))

// ---------------------------------------------------------------------------
// Display type constants (assets/standard-metrics.json)
// ---------------------------------------------------------------------------

/**
 * Map of display_type value -> definition object.
 * Each definition includes: `label`, `layoutMode` ("intrinsic" | "boxed"),
 * and for boxed presentations: `defaultFrameWidth`, `defaultFrameHeight`.
 */
export const DISPLAY_TYPE_DEFINITIONS = Object.freeze(
  Object.fromEntries(Object.entries(standardMetricsManifest.displayTypes.definitions).map(([key, def]) => [key, Object.freeze(def)])),
)

/** Map of display_type value -> human-readable label for dropdown menus. */
export const DISPLAY_TYPE_LABELS = Object.freeze(Object.fromEntries(Object.entries(DISPLAY_TYPE_DEFINITIONS).map(([key, def]) => [key, def.label])))

/** The default set of display types available to all metric value widgets. */
export const DEFAULT_DISPLAY_TYPES = Object.freeze([...standardMetricsManifest.displayTypes.defaults])

/** Per-metric overrides that restrict which display types are permitted. */
export const DISPLAY_TYPE_OVERRIDES = Object.freeze({ ...standardMetricsManifest.displayTypes.overrides })

// ---------------------------------------------------------------------------
// Text display defaults (assets/standard-metrics.json)
// ---------------------------------------------------------------------------

const _textDef = standardMetricsManifest.displayTypes.definitions.text

/** Flat default values for the "text" display type (value widgets). */
export const TEXT_DEFAULTS = Object.freeze(_textDef.defaults)

/** Default font sizes keyed by metric type for text display. */
export const TEXT_FONT_SIZES = Object.freeze(_textDef.fontSizeByType)

/** Default fields for label widgets. */
export const TEXT_LABEL_DEFAULTS = Object.freeze(standardWidgetsManifest.label)

/** Default values for the "heading_tape" display variant. */
export const HEADING_TAPE_DEFAULTS = Object.freeze(standardMetricsManifest.displayTypes.definitions.heading_tape.defaults)

// ---------------------------------------------------------------------------
// Derived metric-type defaults
// ---------------------------------------------------------------------------

/**
 * Base defaults computed from each standard metric definition.
 * Each entry provides `show_units` and `display_unit` derived from the
 * manifest's `showUnitsByDefault` and `defaultDisplayUnit`.
 */
export const METRIC_TYPE_BASE_DEFAULTS = Object.freeze(
  Object.fromEntries(
    STANDARD_METRIC_WIDGET_TYPES.map((type) => {
      const definition = STANDARD_METRIC_DEFINITIONS[type]
      return [
        type,
        Object.freeze({
          show_units: definition?.showUnitsByDefault ?? false,
          display_unit: definition?.defaultDisplayUnit,
        }),
      ]
    }),
  ),
)

export const METRIC_TYPE_OVERRIDES = Object.freeze({ ..._textDef.metricTypeOverrides })

/**
 * Combined metric type defaults: base defaults from each metric definition
 * overlaid with text display type overrides from the manifest and
 * gradient-specific defaults.
 */
export const TYPE_DEFAULTS = Object.freeze({
  ...METRIC_TYPE_BASE_DEFAULTS,
  ...Object.fromEntries(
    Object.entries(METRIC_TYPE_OVERRIDES).map(([type, override]) => [type, Object.freeze({ ...METRIC_TYPE_BASE_DEFAULTS[type], ...override })]),
  ),
  gradient: GRADIENT_DEFAULTS,
})
