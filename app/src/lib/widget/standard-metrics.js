/**
 * @file Standard Metric Helpers
 *
 * Pure lookup functions for the standard metric catalog. All constants are
 * owned by `@/lib/standard-widgets`; this module imports only what it needs
 * and exposes no derived constants — just functions.
 *
 * @module standard-metrics
 */

import {
  STANDARD_METRIC_DEFINITIONS,
  DISPLAY_TYPE_DEFINITIONS,
  DISPLAY_TYPE_LABELS,
  DISPLAY_TYPE_OVERRIDES,
  DEFAULT_DISPLAY_TYPES,
} from './standard-widgets'

// ---------------------------------------------------------------------------
// Display type helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Metric definition helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Interpolation + units policy
// ---------------------------------------------------------------------------

/**
 * Look up the interpolation policy for a standard metric type.
 * @param {string} type - metric type string
 * @returns {'linear' | 'hold' | null} the interpolation mode, or `null` if not found
 */
export function getStandardMetricInterpolation(type) {
  const definition = getStandardMetricDefinition(type)
  return definition?.interpolation ?? null
}

/**
 * Look up the units mode policy for a standard metric type.
 * @param {string} type - metric type string
 * @returns {'selectable' | 'hidden' | null} the units mode, or `null` if not found
 */
export function getStandardMetricUnitsMode(type) {
  const definition = getStandardMetricDefinition(type)
  return definition?.unitsMode ?? null
}
