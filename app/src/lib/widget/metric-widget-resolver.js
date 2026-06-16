/**
 * Default ownership:
 * - Frame geometry defaults come from the shared manifest (getDefaultFrameDimensions)
 * - Display-specific non-geometry defaults come from the manifest
 *   (getDisplayVariantNonGeometryDefaults)
 * - Text reset defaults come from TEXT_DEFAULTS in standard-widgets
 */

import { TEXT_DEFAULTS } from './standard-widgets'
import { getDefaultFrameDimensions, getDisplayVariantNonGeometryDefaults } from './standard-metrics'

/**
 * Resolves frame geometry from the 3-tier fallback chain:
 * 1. Variant config (per-display settings)
 * 2. Top-level widget data (resize handler writes)
 * 3. Manifest defaults from getDefaultFrameDimensions
 *
 * @param {object} [variantConfig] - Active display variant config.
 * @param {object} [widgetData] - Top-level widget data.
 * @param {{ width: number, height: number } | null} frameDefaults - Manifest frame defaults.
 * @returns {{ width: number, height: number, rotation: number }}
 */
function resolveFrameGeometry(variantConfig, widgetData, frameDefaults) {
  return {
    width: widgetData?.width ?? variantConfig?.width ?? frameDefaults?.width,
    height: widgetData?.height ?? variantConfig?.height ?? frameDefaults?.height,
    rotation: variantConfig?.rotation ?? widgetData?.rotation ?? 0,
  }
}

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
  const frameDefaults = getDefaultFrameDimensions(displayType)

  return {
    ...widgetData,
    ...(variantConfig || {}),
    ...resolveFrameGeometry(variantConfig, widgetData, frameDefaults),
    id: widgetData.id,
    value: widgetData.value,
    x: widgetData.x,
    y: widgetData.y,
    opacity: widgetData.opacity,
    display_type: displayType,
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
 * @returns {object} Updated widget data with the initialized variant.
 */
export function initDisplayVariant(widgetData, displayType) {
  if (!widgetData || displayType === 'text') return widgetData

  const variants = widgetData.display_variants || {}
  if (variants[displayType]) return widgetData

  const frameDefaults = getDefaultFrameDimensions(displayType)
  const nonGeometryDefaults = getDisplayVariantNonGeometryDefaults(displayType)

  if (!frameDefaults && !nonGeometryDefaults) return widgetData

  const variantDefaults = {
    ...(nonGeometryDefaults || {}),
    ...resolveFrameGeometry(null, widgetData, frameDefaults),
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
  const nonGeometryDefaults = getDisplayVariantNonGeometryDefaults(displayType)

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
  if (displayType === 'text') return geometryPatch

  const variants = widgetData.display_variants || {}
  const currentVariant = variants[displayType]
  if (!currentVariant) return geometryPatch

  return {
    ...geometryPatch,
    display_variants: {
      ...variants,
      [displayType]: {
        ...currentVariant,
        ...geometryPatch,
      },
    },
  }
}
