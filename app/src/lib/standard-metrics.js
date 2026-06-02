/**
 * @file Standard Metric Widget Builder
 *
 * Bridge between the canonical metric catalog at `assets/standard-metrics.json`
 * and the frontend widget system. Loads all metric definitions and display-type
 * definitions at import time and exposes pure lookup functions — no state, no
 * side effects.
 *
 * Each metric definition in the manifest describes:
 * - `type` — unique key string
 * - `current` — `true` for shipping widgets, `false` for planned/future types
 * - `label` — human-readable name
 * - `defaultDisplayUnit` — fallback unit value
 * - `supportedDisplayUnits` — array of `{ value, label, renderLabel? }`
 * - `showUnitsByDefault` — whether to show the unit label out of the box
 * - `formatter` — key into the formatting system
 * - `icon` — `{ source, assetFile, name? }`
 *
 * Each display-type definition in the manifest describes:
 * - `label` — human-readable name
 * - `layoutMode` — `"intrinsic"` (text/metric) or `"boxed"` (framed presentation)
 * - `defaultFrameWidth` / `defaultFrameHeight` — optional defaults for boxed types
 *
 * @module standard-metrics
 */

import standardMetricsManifest from '../../../assets/standard-metrics.json'
import { GRADIENT_DEFAULTS } from '@/lib/standard-widgets'

/** Map of type -> definition for O(1) lookups. */
const STANDARD_METRIC_DEFINITIONS = Object.fromEntries(standardMetricsManifest.definitions.map((definition) => [definition.type, definition]))

// ---------------------------------------------------------------------------
// Display type definitions (shared with backend via assets/standard-metrics.json)
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
export const DISPLAY_TYPE_LABELS = Object.fromEntries(Object.entries(DISPLAY_TYPE_DEFINITIONS).map(([key, def]) => [key, def.label]))

/** The default set of display types available to all metric value widgets. */
export const DEFAULT_DISPLAY_TYPES = Object.freeze([...standardMetricsManifest.displayTypes.defaults])

/** Per-metric overrides that restrict which display types are permitted. */
const DISPLAY_TYPE_OVERRIDES = Object.freeze({ ...standardMetricsManifest.displayTypes.overrides })

/**
 * Look up the full definition object for a display_type value.
 * @param {string} displayType - display_type key (e.g. "text", "linear")
 * @returns {object|null} the definition, or `null` if unknown
 */
export function getDisplayTypeDefinition(displayType) {
  return DISPLAY_TYPE_DEFINITIONS[displayType] ?? null
}

/**
 * Look up the human-readable label for a display_type value.
 * @param {string} displayType - display_type key (e.g. "text", "linear")
 * @returns {string} the label, or the key unchanged if unknown
 */
export function getDisplayTypeLabel(displayType) {
  return DISPLAY_TYPE_LABELS[displayType] ?? displayType
}

/**
 * Check whether a display_type uses boxed (framed) layout rather than intrinsic text layout.
 * @param {string} displayType - display_type key
 * @returns {boolean} `true` if the display type is boxed
 */
export function isBoxedDisplayType(displayType) {
  const definition = getDisplayTypeDefinition(displayType)
  return definition?.layoutMode === 'boxed'
}

/**
 * Return the default frame dimensions for a boxed display type.
 * @param {string} displayType - display_type key
 * @returns {{ width: number, height: number } | null} default frame size, or `null` for intrinsic or unknown types
 */
export function getDefaultFrameDimensions(displayType) {
  const definition = getDisplayTypeDefinition(displayType)
  if (!definition || definition.layoutMode !== 'boxed') return null
  return { width: definition.defaultFrameWidth, height: definition.defaultFrameHeight }
}

/**
 * Return the set of valid display_type values for a given metric type.
 * Falls back to the global defaults if no override is present.
 * @param {string} metricType - metric type string (e.g. "speed", "heading")
 * @returns {string[]} array of permitted display_type values
 */
export function getSupportedDisplayTypes(metricType) {
  if (Object.hasOwn(DISPLAY_TYPE_OVERRIDES, metricType)) {
    return DISPLAY_TYPE_OVERRIDES[metricType]
  }
  return DEFAULT_DISPLAY_TYPES
}

/**
 * Build the {value, label} option list for a display_type dropdown.
 * @param {string} metricType - metric type string
 * @returns {Array<{value: string, label: string}>}
 */
export function getDisplayTypeOptions(metricType) {
  return getSupportedDisplayTypes(metricType).map((value) => ({
    value,
    label: getDisplayTypeLabel(value),
  }))
}

/**
 * Metric types marked as `current` — actively shipping widget types.
 * @type {string[]}
 */
const CURRENT_STANDARD_METRIC_WIDGET_TYPES = standardMetricsManifest.definitions
  .filter((definition) => definition.current)
  .map((definition) => definition.type)

export { CURRENT_STANDARD_METRIC_WIDGET_TYPES }

/**
 * Every metric type defined in the manifest (both current and planned).
 * @type {string[]}
 */
export const STANDARD_METRIC_WIDGET_TYPES = standardMetricsManifest.definitions.map((definition) => definition.type)

/**
 * Check whether a widget type is a known standard metric type.
 * @param {string} type — widget type string to check
 * @returns {boolean} `true` if the type exists in the manifest
 */
export function isStandardMetricWidgetType(type) {
  return Object.hasOwn(STANDARD_METRIC_DEFINITIONS, type)
}

/**
 * Look up the full definition object for a standard metric type.
 * @param {string} type — metric type string
 * @returns {object|null} the definition object, or `null` if not found
 */
export function getStandardMetricDefinition(type) {
  return STANDARD_METRIC_DEFINITIONS[type] ?? null
}

/**
 * Resolve the display unit for a widget instance.
 * Prefers the widget's persisted `display_unit`, falls back to the
 * definition's `defaultDisplayUnit`, then `null`.
 * @param {string} type — metric type string
 * @param {object} [widgetData={}] — widget instance data (may contain `display_unit`)
 * @returns {string|null} the resolved display unit value
 */
export function getStandardMetricDisplayUnit(type, widgetData = {}) {
  const definition = getStandardMetricDefinition(type)
  return widgetData.display_unit || definition?.defaultDisplayUnit || null
}

/**
 * List all supported display unit options for a metric type.
 * @param {string} type — metric type string
 * @returns {Array<{value: string, label: string, renderLabel?: string}>}
 */
export function getStandardMetricUnitOptions(type) {
  return getStandardMetricDefinition(type)?.supportedDisplayUnits ?? []
}

/**
 * Get the rendered label for a given display unit in a metric type.
 * Falls back through `renderLabel` → `label` → empty string.
 * @param {string} type — metric type string
 * @param {string} [displayUnit] — the unit to look up; if falsy, uses the definition's default
 * @returns {string} the display label (e.g. "KM/H", "BPM", "°C")
 */
export function getStandardMetricUnitLabel(type, displayUnit) {
  const definition = getStandardMetricDefinition(type)
  const resolvedUnit = displayUnit || definition?.defaultDisplayUnit
  const option = definition?.supportedDisplayUnits.find((candidate) => candidate.value === resolvedUnit)
  return option?.renderLabel ?? option?.label ?? ''
}

// ---------------------------------------------------------------------------
// Widget defaults owned by display type definitions
// ---------------------------------------------------------------------------

const _textDef = standardMetricsManifest.displayTypes.definitions.text

/**
 * Flat default values for the "text" display type (value widgets).
 * Spread directly into factory-created widget data.
 * Sourced from displayTypes.definitions.text.defaults in the manifest.
 * @type {object}
 */
export const TEXT_DEFAULTS = Object.freeze(_textDef.defaults)

/**
 * Default font sizes keyed by metric type for text display.
 * Sourced from displayTypes.definitions.text.fontSizeByType in the manifest.
 * @type {{ time: number, gradient: number, heading: number, default: number }}
 */
export const TEXT_FONT_SIZES = Object.freeze(_textDef.fontSizeByType)

/**
 * Default fields for label widgets (text display type).
 * Sourced from displayTypes.definitions.text.labelDefaults in the manifest.
 * @type {{ x: number, y: number, font_size: number, text: string }}
 */
export const TEXT_LABEL_DEFAULTS = Object.freeze(_textDef.labelDefaults)

/**
 * Default values for the "heading_tape" display variant.
 * Sourced from displayTypes.definitions.heading_tape.defaults in the manifest.
 * @type {object}
 */
export const HEADING_TAPE_DEFAULTS = Object.freeze(standardMetricsManifest.displayTypes.definitions.heading_tape.defaults)

/**
 * Base defaults computed from each standard metric definition.
 * Each entry provides `show_units` and `display_unit` derived from the
 * manifest's `showUnitsByDefault` and `defaultDisplayUnit`.
 * `unit_color` is not here — it is a shared text display default
 * (TEXT_DEFAULTS.unit_color).
 * @type {Record<string, {show_units: boolean, display_unit: string|null}>}
 */
export const METRIC_TYPE_BASE_DEFAULTS = Object.freeze(
  Object.fromEntries(
    STANDARD_METRIC_WIDGET_TYPES.map((type) => {
      const definition = getStandardMetricDefinition(type)
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

const _metricTypeOverrides = _textDef.metricTypeOverrides || {}

/**
 * Combined metric type defaults: base defaults from each metric definition
 * overlaid with text display type overrides from the manifest and
 * gradient-specific defaults from standard-widgets.
 *
 * Each override entry is shallow-merged over its base — adding a new
 * metricTypeOverrides entry in the manifest automatically flows through.
 *
 * @type {Record<string, object>}
 */
export const TYPE_DEFAULTS = Object.freeze({
  ...METRIC_TYPE_BASE_DEFAULTS,
  ...Object.fromEntries(
    Object.entries(_metricTypeOverrides).map(([type, override]) => [type, Object.freeze({ ...METRIC_TYPE_BASE_DEFAULTS[type], ...override })]),
  ),
  gradient: GRADIENT_DEFAULTS,
})

/**
 * Look up display-specific non-geometry defaults for a given display type.
 * For boxed types (like heading_tape), returns the flat defaults object.
 * For intrinsic types, returns null (consumers read TEXT_DEFAULTS directly).
 * @param {string} displayType - display_type key
 * @returns {object|null} defaults object, or `null` if none defined
 */
export function getDisplayVariantNonGeometryDefaults(displayType) {
  const definition = DISPLAY_TYPE_DEFINITIONS[displayType]
  if (!definition?.defaults || definition.layoutMode === 'intrinsic') return null
  return definition.defaults
}
