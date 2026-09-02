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
  DISPLAY_TYPE_LABEL_KEYS,
  DISPLAY_TYPE_OVERRIDES,
  DEFAULT_DISPLAY_TYPES,
  TEXT_DEFAULTS,
} from './standard-widgets'

const METRIC_SHARED_DEFAULT_KEYS = new Set(Object.keys(TEXT_DEFAULTS))

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
 * @param {import('i18next').TFunction} translate - Translation function.
 * @returns {string} Translated display type label.
 */
export function getDisplayTypeLabel(displayType, translate) {
  const labelKey = DISPLAY_TYPE_LABEL_KEYS[displayType]
  if (!labelKey) throw new Error(`Unknown display type: ${displayType}`)
  return translate(labelKey)
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
  if (definition.defaultFrameWidth === undefined || definition.defaultFrameHeight === undefined) return null
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
 * @param {import('i18next').TFunction} translate - Translation function.
 * @returns {Array<{value: string, label: string}>}
 */
export function getDisplayTypeOptions(metricType, translate) {
  return getSupportedDisplayTypes(metricType).map((value) => ({
    value,
    label: getDisplayTypeLabel(value, translate),
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
 * Falls back through `renderLabel` → `label` → `defaultLabel` → empty string.
 * @param {string} type — metric type string
 * @param {string} [displayUnit] — the unit to look up; if falsy, uses the definition's default
 * @returns {string} the display label (e.g. "KM/H", "BPM", "°C")
 */
export function getStandardMetricUnitLabel(type, displayUnit) {
  const definition = getStandardMetricDefinition(type)
  const resolvedUnit = displayUnit || definition?.defaultDisplayUnit
  const option = definition?.supportedDisplayUnits.find((candidate) => candidate.value === resolvedUnit)
  return option?.renderLabel ?? option?.label ?? option?.defaultLabel ?? ''
}

/**
 * Look up display-specific non-geometry defaults for a given display type.
 * For boxed types (like heading_tape), returns the flat defaults object
 * (tick colors, font sizes, etc.) excluding frame dimensions.
 * For intrinsic types, returns null (consumers read TEXT_DEFAULTS directly).
 * @param {string} displayType - display_type key
 * @returns {object|null} defaults object, or `null` if none defined
 */
export function getDisplayTypeConfigDefaults(displayType) {
  const definition = DISPLAY_TYPE_DEFINITIONS[displayType]
  if (!definition?.defaults || definition.layoutMode === 'intrinsic') return null
  return Object.fromEntries(Object.entries(definition.defaults).filter(([key]) => !METRIC_SHARED_DEFAULT_KEYS.has(key)))
}

/**
 * Returns the initial shared value-font size for a display type, when it
 * supplies one. Unlike display config defaults, this value is not persisted
 * into the display variant and therefore remains editable as `font_size`.
 *
 * @param {string} displayType - display_type key
 * @returns {number|null} configured default font size, or null
 */
export function getDisplayTypeDefaultFontSize(displayType) {
  const fontSize = DISPLAY_TYPE_DEFINITIONS[displayType]?.defaults?.font_size
  return Number.isFinite(fontSize) ? fontSize : null
}

// ---------------------------------------------------------------------------
// Interpolation + units policy
// ---------------------------------------------------------------------------

/**
 * Look up the interpolation policy for a standard metric type.
 * @param {string} type - metric type string
 * @returns {'linear' | 'hold' | 'preserve' | null} the interpolation mode, or `null` if not found
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

/**
 * Converts a canonical standard-metric value to a display unit.
 * @param {string} type
 * @param {number} value
 * @param {string|null} displayUnit
 * @returns {number}
 */
export function convertStandardMetricValue(type, value, displayUnit) {
  switch (type) {
    case 'speed':
      if (displayUnit === 'mph' || displayUnit === 'imperial') return value * 2.23694
      if (displayUnit === 'kn') return value * 1.943844
      return displayUnit === 'mps' ? value : value * 3.6
    case 'temperature':
    case 'core_temperature':
      return displayUnit === 'fahrenheit' ? (value * 9) / 5 + 32 : value
    case 'pace':
      return displayUnit === 'min_per_mi' ? value * 1.609344 : value
    case 'distance':
    case 'distance_to_home':
      if (displayUnit === 'km') return value / 1000
      if (displayUnit === 'mi') return value / 1609.344
      return displayUnit === 'ft' ? value * 3.28084 : value
    case 'g_force':
      return displayUnit === 'mps2' ? value * 9.80665 : value
    case 'air_pressure':
      if (displayUnit === 'inhg') return value * 29.5299830714
      if (displayUnit === 'mmhg') return value * 750.061561303
      return value * 1000
    case 'stride_length':
      if (displayUnit === 'cm') return value * 100
      if (displayUnit === 'ft') return value * 3.28084
      return displayUnit === 'in' ? value * 39.3701 : value
    case 'vertical_speed':
      if (displayUnit === 'ftmin') return value * 196.850394
      if (displayUnit === 'ftph') return value * 11811.02364
      return displayUnit === 'mph_vertical' ? value * 3600 : value
    case 'altitude':
    case 'total_ascent':
      return displayUnit === 'ft' ? value * 3.28084 : value
    case 'vertical_oscillation':
      return displayUnit === 'cm' ? value / 10 : value
    default:
      return value
  }
}
