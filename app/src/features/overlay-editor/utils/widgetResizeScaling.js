/**
 * Pure widget-content scaling policies used by resize and scale interactions.
 *
 * Moveable owns pointer geometry; this module owns how a widget's dimensional
 * data follows that geometry. Presentation-specific strategies keep the
 * interaction hooks free from display-variant field knowledge.
 */

import { clamp } from '@/lib/utils'
import { isBackdropWidget } from '@/lib/widget/display-type-behavior'
import { buildFrameGeometryUpdate, resolveActiveMetricWidgetData } from '@/lib/widget/widget-resolver'

function scaleNumber(value, scaleFactor, { min = -Infinity, max = Infinity, round = true } = {}) {
  const scaledValue = value * scaleFactor
  const roundedValue = round ? Math.round(scaledValue) : scaledValue

  return clamp(roundedValue, min, max)
}

const GAUGE_DISPLAY_TYPES = new Set(['arc', 'corner', 'lean_angle'])
const G_FORCE_DISPLAY_TYPE = 'g_force'

function isLeanAngleDisplayType(displayType) {
  return displayType === 'lean_angle'
}

/**
 * Returns whether a display type uses the uniform content-resize policy.
 *
 * @param {string} displayType - Persisted display type.
 * @returns {boolean} Whether the display type scales its dimensional content with the frame.
 */
export function isUniformResizeDisplayType(displayType) {
  return GAUGE_DISPLAY_TYPES.has(displayType) || displayType === G_FORCE_DISPLAY_TYPE
}

function isGauge(widget) {
  return GAUGE_DISPLAY_TYPES.has(widget?.data?.display_type)
}

function isGForce(widget) {
  return widget?.data?.display_type === G_FORCE_DISPLAY_TYPE
}

function captureVariantResizeOrigin(widget, data) {
  const displayType = widget.data.display_type

  return {
    displayType,
    data,
    variant: widget.data.display_variants[displayType],
  }
}

function buildMarkerResizeContentDraft(origin, scaleFactor) {
  if (origin.markerSize === null) return {}
  return { marker_size: scaleNumber(origin.markerSize, scaleFactor, { min: 0, max: 400 }) }
}

function buildGaugeResizeContentDraft(origin, scaleFactor, { round = true } = {}) {
  const { data } = origin
  const trackThickness = scaleNumber(data.track_thickness, scaleFactor, { min: 1, max: 100, round })
  const trackCornerRadius = scaleNumber(data.track_corner_radius, scaleFactor, {
    min: 0,
    max: trackThickness * 0.5,
    round,
  })

  return {
    font_size: scaleNumber(data.font_size, scaleFactor, { min: 8, max: 400, round }),
    display_variants: {
      [origin.displayType]: {
        ...origin.variant,
        track_thickness: trackThickness,
        track_corner_radius: trackCornerRadius,
        track_border_thickness: scaleNumber(data.track_border_thickness, scaleFactor, { min: 0, max: 24, round }),
        inner_widget_offset_x: scaleNumber(data.inner_widget_offset_x, scaleFactor, { min: -10_000, max: 10_000, round }),
        inner_widget_offset_y: scaleNumber(data.inner_widget_offset_y, scaleFactor, { min: -10_000, max: 10_000, round }),
        min_max_label_font_size: scaleNumber(data.min_max_label_font_size, scaleFactor, { min: 6, max: 50, round }),
      },
    },
  }
}

function buildLeanAngleResizeContentDraft(origin, scaleFactor, { round = true } = {}) {
  const { data } = origin
  const diameter = scaleNumber(data.diameter, scaleFactor, { min: 8, max: 10_000, round })
  const trackThickness = scaleNumber(data.track_thickness, scaleFactor, {
    min: 1,
    max: diameter * 0.5 - Number.EPSILON,
    round,
  })
  const borderThickness = scaleNumber(data.track_border_thickness, scaleFactor, {
    min: 0,
    max: Math.max(0, trackThickness * 0.5 - Number.EPSILON),
    round,
  })

  return {
    font_size: scaleNumber(data.font_size, scaleFactor, { min: 8, max: 400, round }),
    display_variants: {
      [origin.displayType]: {
        ...origin.variant,
        diameter,
        track_thickness: trackThickness,
        track_border_thickness: borderThickness,
        value_offset_x: scaleNumber(data.value_offset_x, scaleFactor, { min: -10_000, max: 10_000, round }),
        value_offset_y: scaleNumber(data.value_offset_y, scaleFactor, { min: -10_000, max: 10_000, round }),
      },
    },
  }
}

function buildGForceResizeContentDraft(origin, scaleFactor, { round = true } = {}) {
  const { data } = origin
  const diameter = scaleNumber(data.diameter, scaleFactor, { min: 8, max: 10_000, round })

  return {
    display_variants: {
      [origin.displayType]: {
        ...origin.variant,
        diameter,
        border_thickness: scaleNumber(data.border_thickness, scaleFactor, { min: 0, max: (diameter - 1) * 0.5, round }),
        marker_size: scaleNumber(data.marker_size, scaleFactor, { min: 1, max: diameter, round }),
        label_font_size: scaleNumber(data.label_font_size, scaleFactor, { min: 8, max: 400, round }),
        label_offset_x: scaleNumber(data.label_offset_x, scaleFactor, { min: -10_000, max: 10_000, round }),
        label_offset_y: scaleNumber(data.label_offset_y, scaleFactor, { min: -10_000, max: 10_000, round }),
      },
    },
  }
}

function getResizeScaleFactor(origin, width, height) {
  if (isLeanAngleDisplayType(origin.displayType)) return width / origin.width
  return (width / origin.width + height / origin.height) * 0.5
}

function buildResizeContentDraft(origin, scaleFactor, options) {
  if (origin.displayType === G_FORCE_DISPLAY_TYPE) return buildGForceResizeContentDraft(origin, scaleFactor, options)
  if (isLeanAngleDisplayType(origin.displayType)) return buildLeanAngleResizeContentDraft(origin, scaleFactor, options)
  if (GAUGE_DISPLAY_TYPES.has(origin.displayType)) return buildGaugeResizeContentDraft(origin, scaleFactor, options)
  return buildMarkerResizeContentDraft(origin, scaleFactor)
}

function lockResizeFrame(origin, framePatch, { round = false } = {}) {
  if (origin.displayType === G_FORCE_DISPLAY_TYPE) {
    return {
      ...framePatch,
      height: round ? Math.round(framePatch.width) : framePatch.width,
    }
  }

  return framePatch
}

/**
 * Captures all data needed to produce resize updates from a widget frame.
 *
 * @param {object|null} widget - Selected editor widget.
 * @param {object} [frameData] - Resolved active frame data, when the caller has it.
 * @returns {object|null} Resize origin or null when the widget has no data.
 */
export function captureResizeOrigin(widget, frameData = resolveActiveMetricWidgetData(widget?.data)) {
  if (!widget?.data) return null

  const origin = {
    widgetData: widget.data,
    width: frameData.width,
    height: frameData.height,
    markerSize: widget.data.marker_size ?? null,
  }

  if (isGauge(widget) || isGForce(widget)) return { ...origin, ...captureVariantResizeOrigin(widget, frameData) }
  return origin
}

/**
 * Merges a content draft with the normal frame update while preserving the
 * durable display-variant shape. Lean-angle frame dimensions remain derived
 * and therefore are never included in the commit patch.
 *
 * @param {object|null} widgetData - Current stored widget data.
 * @param {object} framePatch - Position/frame update.
 * @param {object} contentDraft - Presentation-specific scaled content.
 * @returns {object} Commit-ready widget update patch.
 */
function mergeResizeUpdate(widgetData, framePatch, contentDraft = {}) {
  const frameUpdate =
    widgetData.display_type === 'lean_angle' ? { x: framePatch.x, y: framePatch.y } : buildFrameGeometryUpdate(widgetData, framePatch)
  const { display_variants: contentVariants, ...topLevelContent } = contentDraft

  if (!contentVariants) {
    return { ...frameUpdate, ...topLevelContent }
  }

  const baseVariants = widgetData?.display_variants || {}
  const frameVariants = frameUpdate.display_variants || {}
  const variantKeys = new Set([...Object.keys(baseVariants), ...Object.keys(contentVariants), ...Object.keys(frameVariants)])
  const displayVariants = {}

  variantKeys.forEach((variantKey) => {
    const mergedVariant = {
      ...(baseVariants[variantKey] || {}),
      ...(contentVariants[variantKey] || {}),
    }

    const frameVariant = frameVariants[variantKey] || {}
    for (const key of ['width', 'height', 'rotation']) {
      if (Object.hasOwn(frameVariant, key)) {
        mergedVariant[key] = frameVariant[key]
      }
    }

    displayVariants[variantKey] = mergedVariant
  })

  return {
    ...frameUpdate,
    ...topLevelContent,
    display_variants: displayVariants,
  }
}

/**
 * Builds a complete resize update from a frame patch, using the same content
 * scaling and durable geometry merge as a resize-handle commit.
 *
 * @param {object} origin - Resize origin from captureResizeOrigin.
 * @param {object} framePatch - Updated frame geometry.
 * @param {object} [options]
 * @param {boolean} [options.round=false] - Round scaled content for persistence.
 * @returns {object} Commit-ready widget update patch.
 */
export function buildResizeUpdate(origin, framePatch, { round = false } = {}) {
  const lockedFramePatch = lockResizeFrame(origin, framePatch, { round })
  const scaleFactor = getResizeScaleFactor(origin, lockedFramePatch.width, lockedFramePatch.height)
  const contentDraft = buildResizeContentDraft(origin, scaleFactor, { round })

  return mergeResizeUpdate(origin.widgetData, lockedFramePatch, contentDraft)
}

/**
 * Builds the render-ready data update for an active frame resize.
 *
 * @param {object} origin - Resize origin from captureResizeOrigin.
 * @param {object} framePatch - Updated frame geometry.
 * @param {object} widget - Widget definition being resized.
 * @returns {object} Resolved live widget data.
 */
export function buildLiveResizeUpdate(origin, framePatch, widget) {
  const resizeUpdate = buildResizeUpdate(origin, framePatch, { round: false })

  return isBackdropWidget(widget)
    ? { ...resizeUpdate, width: framePatch.width, height: framePatch.height }
    : resolveActiveMetricWidgetData({ ...origin.widgetData, ...resizeUpdate })
}

/**
 * Builds the same persisted update produced by a ratio-preserving resize
 * handle, with its target width supplied as one Size value.
 *
 * @param {object} widget - Widget definition being resized.
 * @param {number} size - Target frame width in widget coordinates.
 * @returns {object|null} Commit-ready widget update patch, or null for invalid geometry.
 */
export function buildUniformResizeUpdate(widget, size) {
  const origin = captureResizeOrigin(widget)
  if (!origin || !Number.isFinite(size)) return null

  if (origin.displayType === 'lean_angle') {
    const contentDraft = buildLeanAngleResizeContentDraft(origin, size / origin.data.diameter, { round: true })
    return mergeResizeUpdate(origin.widgetData, { x: origin.widgetData.x, y: origin.widgetData.y }, contentDraft)
  }

  const framePatch = {
    width: Math.round(size),
    height: origin.displayType === G_FORCE_DISPLAY_TYPE ? Math.round(size) : Math.round(origin.height * (size / origin.width)),
  }

  return buildResizeUpdate(origin, framePatch, { round: true })
}

/**
 * Builds a draft for the existing uniform scale interaction used by intrinsic
 * metric, label, and gradient widgets.
 *
 * @param {object} data - Normalized widget data at interaction start.
 * @param {number} scaleFactor - Uniform scale multiplier.
 * @param {object} widget - Widget definition.
 * @param {object} [options]
 * @param {boolean} [options.round=true] - Round values for persistence.
 * @returns {object} Draft with scaled properties.
 */
export function buildScaleDraft(data, scaleFactor, widget, { round = true } = {}) {
  if (widget?.data?.display_type === G_FORCE_DISPLAY_TYPE) {
    const origin = {
      displayType: G_FORCE_DISPLAY_TYPE,
      data,
      variant: widget.data.display_variants?.[G_FORCE_DISPLAY_TYPE] ?? {},
    }
    const contentDraft = buildGForceResizeContentDraft(origin, scaleFactor, { round })
    const width = scaleNumber(data.width, scaleFactor, { min: 8, max: 10_000, round })
    const height = scaleNumber(data.height, scaleFactor, { min: 8, max: 10_000, round })

    return {
      width,
      height,
      display_variants: {
        ...(widget.data.display_variants || {}),
        [G_FORCE_DISPLAY_TYPE]: {
          ...contentDraft.display_variants[G_FORCE_DISPLAY_TYPE],
          width,
          height,
        },
      },
    }
  }

  const nextFontSize = scaleNumber(data.font_size, scaleFactor, { min: 8, max: 400, round })
  const nextDraft = {
    font_size: nextFontSize,
  }

  if (widget?.data?.display_type === 'lap_timer') {
    nextDraft.label_font_size = scaleNumber(data.label_font_size, scaleFactor, { min: 6, max: 400, round })
  }

  if (widget?.category === 'values' && widget.type !== 'gradient') {
    Object.assign(nextDraft, {
      icon_size: scaleNumber(data.icon_size, scaleFactor, { min: 0, max: 400, round }),
      icon_offset_x: scaleNumber(data.icon_offset_x, scaleFactor, { round }),
      icon_offset_y: scaleNumber(data.icon_offset_y, scaleFactor, { round }),
    })
  }

  if (widget?.type === 'gradient') {
    Object.assign(nextDraft, {
      triangle_width: scaleNumber(data.triangle_width, scaleFactor, { min: 0, max: 600, round }),
      value_offset: scaleNumber(data.value_offset, scaleFactor, { round }),
    })
  }

  return nextDraft
}
