/**
 * @file Standard Widget Constants
 *
 * All constants derived from the canonical manifest files at
 * `assets/standard-metrics.json` and `assets/standard-widgets.json`.
 * Every constant export is a frozen object/array keyed off the manifest.
 *
 * @module standard-widgets
 */

import standardWidgetsManifest from '../../../../assets/standard-widgets.json'
import standardMetricsManifest from '../../../../assets/standard-metrics.json'

// ---------------------------------------------------------------------------
// Plot widget defaults (assets/standard-widgets.json)
// ---------------------------------------------------------------------------

/** Defaults for course plot widgets. */
export const COURSE_PLOT_DEFAULTS = Object.freeze({ ...standardWidgetsManifest.plot.definitions.course.defaults })

/** Defaults for elevation plot widgets. */
export const ELEVATION_PLOT_DEFAULTS = Object.freeze({ ...standardWidgetsManifest.plot.definitions.elevation.defaults })

/** Defaults for gradient metric value widgets. */
export const GRADIENT_DEFAULTS = Object.freeze({ ...standardWidgetsManifest.gradient.definitions.gradient.defaults })

// ---------------------------------------------------------------------------
// Metric definitions (assets/standard-metrics.json)
// ---------------------------------------------------------------------------

/** Map of type -> definition for O(1) lookups. */
export const STANDARD_METRIC_DEFINITIONS = Object.freeze(
  Object.fromEntries(standardMetricsManifest.definitions.map((definition) => [definition.type, Object.freeze(definition)])),
)

const standardWidgetTypeDefinitions = {}
const pendingManifestEntries = [standardWidgetsManifest]

while (pendingManifestEntries.length > 0) {
  const entry = pendingManifestEntries.pop()
  if (!entry || Array.isArray(entry) || typeof entry !== 'object') continue

  const metadataFields = ['type', 'nameKey', 'shortNameKey', 'category']
  const carriesWidgetMetadata = metadataFields.some((field) => Object.hasOwn(entry, field))
  if (carriesWidgetMetadata) {
    if (metadataFields.some((field) => !entry[field])) throw new Error(`Incomplete standard widget metadata: ${entry.type ?? 'missing type'}`)
    if (Object.hasOwn(standardWidgetTypeDefinitions, entry.type)) throw new Error(`Duplicate standard widget type: ${entry.type}`)
    standardWidgetTypeDefinitions[entry.type] = Object.freeze(entry)
    continue
  }

  pendingManifestEntries.push(...Object.values(entry))
}

const widgetTypeDefinitions = { ...standardWidgetTypeDefinitions, ...STANDARD_METRIC_DEFINITIONS }

for (const [type, definition] of Object.entries(widgetTypeDefinitions)) {
  if (!definition.nameKey || !definition.shortNameKey || !Object.hasOwn(standardWidgetsManifest.categories, definition.category)) {
    throw new Error(`Invalid widget type metadata: ${type}`)
  }
}

/** Canonical UI metadata and behavior definition for every supported widget type. */
export const WIDGET_TYPE_DEFINITIONS = Object.freeze(widgetTypeDefinitions)

/** Ordered map of category ID to translation key. */
export const WIDGET_CATEGORY_NAME_KEYS = Object.freeze({ ...standardWidgetsManifest.categories })

/**
 * Resolve a supported widget type through the canonical catalog.
 * @param {string} type - Canonical widget type.
 * @returns {object} Widget type definition.
 */
export function requireWidgetTypeDefinition(type) {
  const definition = WIDGET_TYPE_DEFINITIONS[type]
  if (!definition) throw new Error(`Unknown widget type: ${type}`)
  return definition
}

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
 * Each definition includes: `labelKey`, `layoutMode` ("intrinsic" | "boxed"),
 * and, for explicitly sized boxed presentations, `defaultFrameWidth` and
 * `defaultFrameHeight`.
 */
export const DISPLAY_TYPE_DEFINITIONS = Object.freeze(
  Object.fromEntries(Object.entries(standardMetricsManifest.displayTypes.definitions).map(([key, def]) => [key, Object.freeze(def)])),
)

for (const [displayType, definition] of Object.entries(DISPLAY_TYPE_DEFINITIONS)) {
  if (!definition.labelKey) throw new Error(`Display type is missing labelKey: ${displayType}`)
}

/** Map of display_type value -> translation key for dropdown menus. */
export const DISPLAY_TYPE_LABEL_KEYS = Object.freeze(
  Object.fromEntries(Object.entries(DISPLAY_TYPE_DEFINITIONS).map(([key, definition]) => [key, definition.labelKey])),
)

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
export const TEXT_LABEL_DEFAULTS = Object.freeze({ ...standardWidgetsManifest.label.definitions.label.defaults })

/** Default values for the "heading_tape" display variant. */
export const HEADING_TAPE_DEFAULTS = Object.freeze(standardMetricsManifest.displayTypes.definitions.heading_tape.defaults)

/** Flat defaults and readout modes for the intrinsic lap timer display. */
export const LAP_TIMER_DEFAULTS = Object.freeze({ ...standardMetricsManifest.displayTypes.definitions.lap_timer.defaults })
export const LAP_TIMER_MODES = Object.freeze(
  standardMetricsManifest.displayTypes.definitions.lap_timer.modes.map((mode) =>
    Object.freeze({ font_size: LAP_TIMER_DEFAULTS.font_size, label_font_size: LAP_TIMER_DEFAULTS.label_font_size, ...mode }),
  ),
)

// ---------------------------------------------------------------------------
// Backdrop display type constants (assets/standard-widgets.json)
// ---------------------------------------------------------------------------

/** Map of backdrop display_type value -> definition object. */
export const BACKDROP_TYPE_DEFINITIONS = Object.freeze(
  Object.fromEntries(Object.entries(standardWidgetsManifest.backdrops.definitions).map(([key, definition]) => [key, Object.freeze(definition)])),
)

for (const [displayType, definition] of Object.entries(BACKDROP_TYPE_DEFINITIONS)) {
  if (!definition.labelKey) throw new Error(`Backdrop type is missing labelKey: ${displayType}`)
}

/** Map of backdrop display_type value -> translation key. */
export const BACKDROP_TYPE_LABEL_KEYS = Object.freeze(
  Object.fromEntries(Object.entries(BACKDROP_TYPE_DEFINITIONS).map(([key, definition]) => [key, definition.labelKey])),
)

/** The default backdrop display type list. */
export const BACKDROP_DEFAULT_DISPLAY_TYPES = Object.freeze([...standardWidgetsManifest.backdrops.defaults])

/** Defaults for circle backdrop widgets. */
export const BACKDROP_CIRCLE_DEFAULTS = Object.freeze({ ...standardWidgetsManifest.backdrops.definitions.circle.defaults })

/** Defaults for rectangle backdrop widgets. */
export const BACKDROP_RECTANGLE_DEFAULTS = Object.freeze({ ...standardWidgetsManifest.backdrops.definitions.rectangle.defaults })

/**
 * Build the {value, label} option list for a backdrop display_type dropdown.
 * @param {import('i18next').TFunction} translate - Translation function.
 * @returns {Array<{value: string, label: string}>}
 */
export function getBackdropTypeOptions(translate) {
  return Object.entries(BACKDROP_TYPE_DEFINITIONS).map(([value, definition]) => ({
    value,
    label: translate(definition.labelKey),
  }))
}

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
          ...(definition?.defaults ?? {}),
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
