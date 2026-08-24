/**
 * @file widget-resolver – Display-variant resolution for metric and backdrop widgets.
 *
 * Both widget families use the same durable storage shape:
 *   { display_type, display_variants: { [type]: { ...type-specific fields } }, ...shared }
 *
 * This module resolves that shape into flat data for rendering/preview.
 *
 * @module widget-resolver
 */

import { TEXT_DEFAULTS, BACKDROP_CIRCLE_DEFAULTS, BACKDROP_RECTANGLE_DEFAULTS } from './standard-widgets'
import { getDefaultFrameDimensions, getDisplayTypeConfigDefaults } from './standard-metrics'
import { getLeanAngleLayout } from '@/features/widget-preview/widgets/lean-angle/geometry'

// ---------------------------------------------------------------------------
// Backdrop constants
// ---------------------------------------------------------------------------

const BACKDROP_DEFAULTS_BY_TYPE = {
  circle: BACKDROP_CIRCLE_DEFAULTS,
  rectangle: BACKDROP_RECTANGLE_DEFAULTS,
}

const BACKDROP_GEOMETRY_KEYS_BY_TYPE = {
  circle: ['diameter'],
  rectangle: ['width', 'height', 'corner_radius', 'round_top_left', 'round_top_right', 'round_bottom_left', 'round_bottom_right'],
}

function stripMetricSharedFields(variantConfig) {
  return Object.fromEntries(Object.entries(variantConfig || {}).filter(([key]) => !Object.hasOwn(TEXT_DEFAULTS, key)))
}

// ---------------------------------------------------------------------------
// Metric helpers (private)
// ---------------------------------------------------------------------------

/**
 * Resolves frame geometry from the 3-tier fallback chain:
 * 1. Variant config (per-display settings)
 * 2. Top-level widget data (resize handler writes)
 * 3. Manifest defaults from getDefaultFrameDimensions
 *
 * @param {object} [variantConfig] - Active display variant config.
 * @param {object} [widgetData] - Top-level widget data.
 * @param {{ width: number, height: number } | null} frameDefaults - Manifest frame defaults.
 * @returns {{ width?: number, height?: number, rotation: number }}
 */
function resolveFrameGeometry(variantConfig, widgetData, frameDefaults) {
  const geometry = {
    rotation: variantConfig?.rotation ?? widgetData?.rotation ?? 0,
  }

  if (frameDefaults) {
    geometry.width = variantConfig?.width ?? widgetData?.width ?? frameDefaults.width
    geometry.height = variantConfig?.height ?? widgetData?.height ?? frameDefaults.height
  }

  return geometry
}

// ---------------------------------------------------------------------------
// Backdrop resolvers
// ---------------------------------------------------------------------------

/**
 * Resolves the active backdrop display variant into flat widget data.
 *
 * @param {object} widgetData - Stored backdrop data.
 * @param {object|null} [previewOverrides] - Ephemeral preview overrides to merge.
 * @returns {object} Backdrop data with the active variant flattened.
 */
export function resolveActiveBackdropData(widgetData, previewOverrides = null) {
  if (!widgetData) return widgetData

  const displayType = widgetData.display_type
  const variantConfig = widgetData.display_variants?.[displayType] || {}

  const resolved = {
    ...widgetData,
    ...variantConfig,
    ...(widgetData.width !== undefined ? { width: widgetData.width } : {}),
    ...(widgetData.height !== undefined ? { height: widgetData.height } : {}),
    id: widgetData.id,
    x: widgetData.x,
    y: widgetData.y,
    opacity: widgetData.opacity,
    display_type: displayType,
    fill_color: widgetData.fill_color,
    fill_opacity: widgetData.fill_opacity,
    border_thickness: widgetData.border_thickness,
    border_color: widgetData.border_color,
    border_opacity: widgetData.border_opacity,
  }

  const nextResolved = previewOverrides ? { ...resolved, ...previewOverrides } : resolved
  if (displayType === 'circle') {
    const diameter = nextResolved.width ?? nextResolved.height ?? nextResolved.diameter
    return { ...nextResolved, diameter, width: diameter, height: diameter }
  }

  return nextResolved
}

/**
 * Initializes a backdrop display variant from manifest defaults when absent.
 *
 * @param {object} widgetData - Stored backdrop data.
 * @param {string} displayType - Backdrop display type to initialize.
 * @returns {object} Updated widget data with the initialized variant.
 */
export function initBackdropVariant(widgetData, displayType) {
  if (!widgetData) return widgetData

  const defaults = BACKDROP_DEFAULTS_BY_TYPE[displayType]
  const geometryKeys = BACKDROP_GEOMETRY_KEYS_BY_TYPE[displayType]
  if (!defaults || !geometryKeys) return widgetData

  const variants = widgetData.display_variants || {}
  const currentVariant = variants[displayType]
  if (currentVariant) return widgetData

  const variantDefaults = Object.fromEntries(geometryKeys.map((key) => [key, currentVariant?.[key] ?? widgetData[key] ?? defaults[key]]))

  return {
    ...widgetData,
    display_variants: {
      ...variants,
      [displayType]: variantDefaults,
    },
  }
}

// ---------------------------------------------------------------------------
// Metric resolvers
// ---------------------------------------------------------------------------

/**
 * Resolves the active metric widget data from the hybrid storage shape.
 *
 * For text display_type: returns the flat widget data as-is.
 * For non-text display_type: merges shared top-level fields with the active
 * display variant, flattening display-specific settings to the top level.
 *
 * @param {object} widgetData - The widget's stored data (hybrid shape).
 * @returns {object} Resolved active config with all fields flattened.
 */
export function resolveActiveMetricWidgetData(widgetData) {
  if (!widgetData) return widgetData

  const displayType = widgetData.display_type || 'text'

  if (displayType === 'text') {
    return widgetData
  }

  const variants = widgetData.display_variants || {}
  const variantConfig = variants[displayType]
  const resolved = {
    ...widgetData,
    ...variantConfig,
  }

  if (displayType === 'lean_angle') {
    const layout = getLeanAngleLayout(resolved)

    return {
      ...resolved,
      width: layout.width,
      height: layout.height,
    }
  }

  const frameDefaults = getDefaultFrameDimensions(displayType)

  return {
    ...resolved,
    ...resolveFrameGeometry(variantConfig, widgetData, frameDefaults),
  }
}

/**
 * Initializes a display variant from defaults if it doesn't already exist.
 * Seeds frame geometry from the manifest and non-geometry settings from
 * display-type-owned defaults.
 *
 * For boxed types without non-geometry defaults (future presentations),
 * still seeds frame geometry from the manifest so the variant is ready.
 *
 * @param {object} widgetData - The widget's stored data.
 * @param {string} displayType - The display type to initialize.
 * @param {object|null} [frameFallback=widgetData] - Explicit fallback geometry used when the variant has no frame geometry.
 * @returns {object} Updated widget data with the initialized variant.
 */
export function initDisplayVariant(widgetData, displayType, frameFallback = widgetData) {
  if (!widgetData || displayType === 'text') return widgetData

  const variants = widgetData.display_variants || {}
  const currentVariant = variants[displayType]
  const frameDefaults = getDefaultFrameDimensions(displayType)
  const nonGeometryDefaults = getDisplayTypeConfigDefaults(displayType)

  if (!frameDefaults && !nonGeometryDefaults) return widgetData

  const variantDefaults = {
    ...(nonGeometryDefaults || {}),
    ...stripMetricSharedFields(currentVariant),
    ...resolveFrameGeometry(currentVariant, frameFallback, frameDefaults),
  }

  if (!variantDefaults.min_max_label_font && widgetData.font) {
    variantDefaults.min_max_label_font = widgetData.font
  }
  if (displayType === 'g_force' && !variants[displayType] && widgetData.font) {
    variantDefaults.label_font = widgetData.font
  }

  return {
    ...widgetData,
    display_variants: {
      ...variants,
      [displayType]: variantDefaults,
    },
  }
}

/**
 * Resets the current display config to defaults while preserving all other
 * display variants and shared fields.
 *
 * For text display_type: resets text-specific fields from shared defaults.
 * For non-text display_type: resets only the active variant config.
 *
 * @param {object} widgetData - The widget's stored data.
 * @returns {object} Updated widget data with the active display config reset.
 */
export function resetCurrentDisplayConfig(widgetData) {
  if (!widgetData) return widgetData

  const displayType = widgetData.display_type || 'text'

  if (displayType === 'text') {
    return {
      ...widgetData,
      ...TEXT_DEFAULTS,
      value: widgetData.value,
      id: widgetData.id,
      font_size: widgetData.font_size,
      x: widgetData.x,
      y: widgetData.y,
    }
  }

  const frameDefaults = getDefaultFrameDimensions(displayType)
  const nonGeometryDefaults = getDisplayTypeConfigDefaults(displayType)

  if (!frameDefaults && !nonGeometryDefaults) return widgetData

  return {
    ...widgetData,
    display_variants: {
      ...widgetData.display_variants,
      [displayType]: {
        ...(nonGeometryDefaults || {}),
        ...resolveFrameGeometry(null, widgetData, frameDefaults),
      },
    },
  }
}

/**
 * Builds an update patch that writes frame geometry to both the top level
 * and the active display variant. This ensures overlay resize/rotate
 * interactions persist geometry in the durable storage shape.
 *
 * For text display_type, returns only the top-level patch.
 * For non-text display_type, also syncs width/height/rotation into
 * display_variants[display_type].
 *
 * @param {object} widgetData - The widget's current stored data.
 * @param {object} geometryPatch - Frame geometry updates ({ width, height, rotation }).
 * @returns {object} Update patch suitable for commitWidgetUpdate.
 */
export function buildFrameGeometryUpdate(widgetData, geometryPatch) {
  if (!widgetData || !geometryPatch) return geometryPatch

  const displayType = widgetData.display_type || 'text'
  if (BACKDROP_GEOMETRY_KEYS_BY_TYPE[displayType]) {
    const variants = widgetData.display_variants || {}
    const currentVariant = variants[displayType] || {}
    const variantPatch = {}

    if (displayType === 'circle') {
      variantPatch.diameter = geometryPatch.width ?? geometryPatch.height ?? currentVariant.diameter
    } else {
      variantPatch.width = geometryPatch.width
      variantPatch.height = geometryPatch.height
    }

    const patch = {
      display_variants: {
        ...variants,
        [displayType]: {
          ...currentVariant,
          ...variantPatch,
        },
      },
    }
    if (geometryPatch.x !== undefined) patch.x = geometryPatch.x
    if (geometryPatch.y !== undefined) patch.y = geometryPatch.y
    return patch
  }

  if (displayType === 'text') return geometryPatch

  if (displayType === 'lean_angle') {
    const variants = widgetData.display_variants || {}
    const currentVariant = variants[displayType] || {}
    const patch = {
      display_variants: {
        ...variants,
        [displayType]: {
          ...currentVariant,
          rotation: geometryPatch.rotation,
        },
      },
    }
    if (geometryPatch.x !== undefined) patch.x = geometryPatch.x
    if (geometryPatch.y !== undefined) patch.y = geometryPatch.y
    return patch
  }

  const variants = widgetData.display_variants || {}
  const currentVariant = variants[displayType]
  if (!currentVariant) return geometryPatch

  const variantPatch = {}
  for (const key of ['width', 'height', 'rotation']) {
    if (geometryPatch[key] !== undefined) {
      variantPatch[key] = geometryPatch[key]
    }
  }

  return {
    ...geometryPatch,
    display_variants: {
      ...variants,
      [displayType]: {
        ...currentVariant,
        ...variantPatch,
      },
    },
  }
}

/**
 * Builds the complete data update for switching a metric widget display type.
 * New variants use manifest frame defaults; existing variants restore their
 * own frame geometry. The active top-level frame remains synchronized through
 * buildFrameGeometryUpdate.
 *
 * @param {object} widgetData - The widget's current stored data.
 * @param {string} displayType - The display type to activate.
 * @returns {object} Update patch suitable for updateWidgetData.
 */
export function buildDisplayTypeChangeUpdate(widgetData, displayType) {
  const nextData = initDisplayVariant(widgetData, displayType, null)
  const frameDefaults = getDefaultFrameDimensions(displayType)

  if (!frameDefaults) {
    return {
      display_type: displayType,
      display_variants: nextData.display_variants,
    }
  }

  return {
    display_type: displayType,
    ...buildFrameGeometryUpdate(
      { ...nextData, display_type: displayType },
      resolveFrameGeometry(nextData.display_variants[displayType], null, frameDefaults),
    ),
  }
}
