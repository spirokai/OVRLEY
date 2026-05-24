/**
 * @file Standard Metric Widget Builder
 *
 * Bridge between the canonical metric catalog at `assets/standard-metrics.json`
 * and the frontend widget system. Loads all metric definitions at import time
 * and exposes pure lookup functions — no state, no side effects.
 *
 * Each definition in the manifest describes:
 * - `type` — unique key string
 * - `current` — `true` for shipping widgets, `false` for planned/future types
 * - `label` — human-readable name
 * - `defaultDisplayUnit` — fallback unit value
 * - `supportedDisplayUnits` — array of `{ value, label, renderLabel? }`
 * - `showUnitsByDefault` — whether to show the unit label out of the box
 * - `formatter` — key into the formatting system
 * - `icon` — `{ source, assetFile, name? }`
 *
 * @module standard-metrics
 */

import standardMetricsManifest from '../../../assets/standard-metrics.json'

/** Map of type -> definition for O(1) lookups. */
const STANDARD_METRIC_DEFINITIONS = Object.fromEntries(standardMetricsManifest.definitions.map((definition) => [definition.type, definition]))

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
