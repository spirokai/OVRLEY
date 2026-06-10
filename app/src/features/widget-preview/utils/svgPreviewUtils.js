/**
 * Shared SVG preview utility functions used across per-widget renderers.
 */

import { findPointAtProgress, getPointAtProgress } from '@/lib/geometryUtils'

/**
 * Sanitizes a string for use as an SVG element ID.
 *
 * Replaces all non-alphanumeric characters (except hyphens and underscores)
 * with underscores to prevent invalid SVG id attributes.
 *
 * @param {string} value - Raw ID string.
 * @returns {string} Sanitized ID safe for SVG use.
 */
export function sanitizeSvgId(value) {
  return String(value || 'preview-shadow').replace(/[^a-zA-Z0-9_-]/g, '_')
}

/**
 * Normalizes a shadow color string, splitting 8-digit hex into separate color and opacity components.
 *
 * For 8-character hex values, the last two digits are treated as alpha and combined
 * with the explicit opacity parameter. For all other formats, opacity is passed through.
 *
 * @param {string} color - Raw color string (hex, named, etc.).
 * @param {number} [opacity=1] - Additional opacity multiplier.
 * @returns {{ color: string, opacity: number }} Normalized hex color and clamped opacity.
 */
export function normalizeSvgShadowColor(color, opacity = 1) {
  const rawColor = String(color || '').trim()
  const hex = rawColor.startsWith('#') ? rawColor.slice(1) : rawColor
  const safeOpacity = Math.min(Math.max(Number(opacity) || 0, 0), 1)

  if (/^[0-9a-fA-F]{8}$/.test(hex)) {
    const alpha = parseInt(hex.slice(6, 8), 16) / 255
    return {
      color: `#${hex.slice(0, 6)}`,
      opacity: alpha * safeOpacity,
    }
  }

  return {
    color: rawColor || '#000000',
    opacity: safeOpacity,
  }
}

/**
 * Checks whether two 2D points are approximately equal within a small epsilon.
 *
 * @param {number[]|null|undefined} left - First point [x, y].
 * @param {number[]|null|undefined} right - Second point [x, y].
 * @returns {boolean} True if both points are within 1e-3 Euclidean distance.
 */
export function pointsEqual(left, right) {
  if (!left || !right) {
    return false
  }

  return Math.hypot(right[0] - left[0], right[1] - left[1]) <= 1e-3
}

/**
 * Normalizes an opacity value handling both percentage (0–100) and decimal (0–1) ranges.
 *
 * Values > 1 are treated as percentages and divided by 100.
 *
 * @param {number|null|undefined} value - Raw opacity value.
 * @param {number} fallback - Fallback opacity if value is null, undefined, or non-finite.
 * @returns {number} Clamped opacity in the 0–1 range.
 */
export function normalizePreviewOpacity(value, fallback) {
  if (value === null || value === undefined) {
    return fallback
  }

  const numericValue = Number(value)
  if (!Number.isFinite(numericValue)) {
    return fallback
  }

  return numericValue > 1 ? Math.min(Math.max(numericValue / 100, 0), 1) : Math.min(Math.max(numericValue, 0), 1)
}

/**
 * Resolves a display color from explicit, inherited, or base fallback values.
 *
 * Precedence: explicitColor > inheritedColor > baseColor.
 *
 * @param {string|null|undefined} explicitColor - Widget-level color override.
 * @param {string|null|undefined} inheritedColor - Inherited line/group color.
 * @param {string} [baseColor='#ffffff'] - Base fallback color.
 * @returns {string} Resolved color string.
 */
export function resolvePreviewStyleColor(explicitColor, inheritedColor, baseColor) {
  return explicitColor || inheritedColor || baseColor || '#ffffff'
}

/**
 * Resolves a line width value from explicit or legacy widget settings.
 *
 * Legacy widths are multiplied by 2.5 to match the Skia renderer's scaling convention.
 *
 * @param {number|null|undefined} explicitWidth - Explicit line width value.
 * @param {number|null|undefined} legacyWidth - Legacy line width (multiplied by 2.5 as fallback).
 * @returns {number} Resolved line width in pixels.
 */
export function resolvePreviewLineWidth(explicitWidth, legacyWidth) {
  const numericExplicit = Number(explicitWidth)
  if (Number.isFinite(numericExplicit)) {
    return numericExplicit
  }

  const numericLegacy = Number(legacyWidth)
  return (Number.isFinite(numericLegacy) ? numericLegacy : 1.75) * 2.5
}

/**
 * Resolves a line width accounting for global SVG scale.
 *
 * Explicit widths are divided by the global scale to produce an unscaled preview value.
 * Legacy widths fall through to resolvePreviewLineWidth without scaling.
 *
 * @param {number|null|undefined} explicitWidth - Explicit line width.
 * @param {number|null|undefined} legacyWidth - Legacy line width.
 * @param {number} globalScale - Global scale factor to un-scale.
 * @returns {number} Resolved line width in unscaled preview coordinates.
 */
export function resolveScaledPreviewLineWidth(explicitWidth, legacyWidth, globalScale) {
  const safeScale = Math.max(Number(globalScale) || 1, 0.1)
  const numericExplicit = Number(explicitWidth)

  if (Number.isFinite(numericExplicit)) {
    return numericExplicit / safeScale
  }

  return resolvePreviewLineWidth(undefined, legacyWidth)
}

function normalizeMarkerVariant(value) {
  return value === 'ring' || value === 'halo' ? value : 'single'
}

function resolveMarkerVariantDiameter(widgetData, fallbackRadius) {
  const configuredDiameter = Number(widgetData.marker_variant_diameter)
  if (Number.isFinite(configuredDiameter) && configuredDiameter >= 0) {
    return configuredDiameter
  }

  return Math.max(fallbackRadius * 2 + 8, 8)
}

const METRIC_PROGRESS_EPSILON = 1e-6

function metricProgressEqual(left, right) {
  return Number.isFinite(left) && Number.isFinite(right) && Math.abs(left - right) <= METRIC_PROGRESS_EPSILON
}

function findDuplicateProgressRun(progressValues, targetProgress, anchorIndex) {
  const safeAnchorIndex = Math.max(Math.min(anchorIndex, progressValues.length - 1), 0)
  const anchorProgress = Number(progressValues[safeAnchorIndex])

  if (!metricProgressEqual(anchorProgress, targetProgress)) {
    return null
  }

  let start = safeAnchorIndex
  let end = safeAnchorIndex

  while (start > 0 && metricProgressEqual(progressValues[start - 1], anchorProgress)) {
    start -= 1
  }

  while (end + 1 < progressValues.length && metricProgressEqual(progressValues[end + 1], anchorProgress)) {
    end += 1
  }

  return end > start ? { start, end } : null
}

/**
 * Builds the marker layer definitions for a widget's position indicator.
 *
 * Processes widget data points into sorted circle layers (largest radius first),
 * with the innermost layer rendered as a solid fill. Falls back to a single
 * default marker when no custom points are configured.
 *
 * @param {object} widgetData - Widget configuration with optional points array.
 * @param {number} fallbackRadius - Default marker radius if no points defined.
 * @param {string} fallbackColor - Default marker color.
 * @param {number} fallbackOpacity - Default marker opacity.
 * @returns {Array<{radius: number, color: string, opacity: number, solidFill: boolean}>} Sorted marker layers.
 */
export function getPreviewMarkerLayers(widgetData, fallbackRadius, fallbackColor, fallbackOpacity) {
  const sourcePoints = Array.isArray(widgetData.points) ? widgetData.points : []
  const markerPoints = sourcePoints.length
    ? sourcePoints
    : [
        {
          weight: fallbackRadius ** 2,
          color: fallbackColor,
          opacity: fallbackOpacity,
        },
      ]

  const layers = markerPoints
    .map((point) => ({
      radius: Math.max(Math.sqrt(Math.max(Number(point.weight) || 80, 1)), 2),
      color: point.color || '#ffffff',
      opacity: normalizePreviewOpacity(point.opacity, 1),
    }))
    .sort((left, right) => right.radius - left.radius)
    .map((layer, index, layers) => ({
      ...layer,
      solidFill: index === layers.length - 1,
      strokeWidth: index === layers.length - 1 ? undefined : Math.min(Math.max(Math.round(layer.radius * 0.18), 1), 3),
    }))

  const markerVariant = normalizeMarkerVariant(widgetData.marker_variant)
  const variantRadius = Math.max(resolveMarkerVariantDiameter(widgetData, fallbackRadius) * 0.5, 0)

  if (markerVariant === 'ring' && variantRadius > 0) {
    layers.unshift({
      radius: variantRadius,
      color: fallbackColor,
      opacity: fallbackOpacity,
      solidFill: false,
      strokeWidth: 1.5,
    })
  }

  if (markerVariant === 'halo' && variantRadius > 0) {
    layers.unshift({
      radius: variantRadius,
      color: fallbackColor,
      opacity: Math.min(Math.max(fallbackOpacity * 0.35, 0), 1),
      solidFill: true,
      strokeWidth: undefined,
    })
  }

  return layers
}

/**
 * Builds the route frame preview state — determines the marker point and
 * completed segment points from route geometry at a given progress value.
 *
 * Uses metric-distance-based interpolation with fallback to uniform progress,
 * ensuring the marker lands at the correct position along the route.
 *
 * @param {number[][]} points - Route SVG points.
 * @param {number[]} progressValues - Per-point progress values (0–1).
 * @param {number} progress01 - Current progress (0–1).
 * @returns {{ markerPoint: number[]|null, completedPoints: number[][] }} Marker position and completed polyline points.
 */
export function buildRouteFramePreview(points, progressValues, progress01) {
  if (!points.length) {
    return { markerPoint: null, completedPoints: [] }
  }

  const metricPoint = findPointAtProgress(points, progressValues, progress01)
  const markerPoint = metricPoint?.point || getPointAtProgress(points, progress01) || points[points.length - 1]
  const lastPoint = points[points.length - 1]
  let completedPoints =
    markerPoint && pointsEqual(lastPoint, markerPoint) ? [...points] : points.slice(0, Math.min(metricPoint?.index ?? 0, points.length))

  if (!completedPoints.length) {
    completedPoints = [points[0]]
  }

  if (markerPoint && !pointsEqual(completedPoints[completedPoints.length - 1], markerPoint)) {
    completedPoints.push(markerPoint)
  }

  return { markerPoint, completedPoints }
}

/**
 * Builds the completed elevation polyline points for the current frame.
 *
 * Ordinary motion should behave like the route widget: the completed path ends
 * at the distance-based marker position on the geometry. When the activity is in
 * a duplicate-progress run (hover/stop with vertical motion), the path must fill
 * chronologically within that run using elapsed fractions while still staying at
 * the current x-position.
 *
 * @param {number[][]} points - Elevation SVG points.
 * @param {number[]} progressValues - Per-point metric progress values (0–1).
 * @param {number[]} elapsedFractions - Per-point elapsed fractions (0–1).
 * @param {number} progress01 - Current distance progress (0–1).
 * @param {number} frameElapsedFraction - Current frame elapsed fraction (0–1).
 * @returns {number[][]} Points for the completed (ridden) portion of the elevation profile.
 */
export function buildElevationCompletedPoints(points, progressValues, elapsedFractions, progress01, frameElapsedFraction) {
  if (!points.length) {
    return []
  }

  const metricHit = findPointAtProgress(points, progressValues, progress01)
  const metricIndex = metricHit?.index ?? points.length - 1
  const duplicateRun = findDuplicateProgressRun(progressValues, progress01, metricIndex)
  let completedPoints = []
  let completedEndpoint = metricHit?.point || points[points.length - 1]

  if (duplicateRun) {
    completedPoints = points.slice(0, duplicateRun.start)

    for (let index = duplicateRun.start; index <= duplicateRun.end; index += 1) {
      if ((elapsedFractions[index] ?? 0) < frameElapsedFraction) {
        completedPoints.push(points[index])
      }
    }

    const runPoints = points.slice(duplicateRun.start, duplicateRun.end + 1)
    const runElapsedFractions = elapsedFractions.slice(duplicateRun.start, duplicateRun.end + 1)
    completedEndpoint = findPointAtProgress(runPoints, runElapsedFractions, frameElapsedFraction)?.point || runPoints[runPoints.length - 1]
  } else {
    completedPoints = points.slice(0, Math.min(metricIndex, points.length))
  }

  if (!completedPoints.length) {
    completedPoints.push(points[0])
  }

  if (completedEndpoint && !pointsEqual(completedPoints[completedPoints.length - 1], completedEndpoint)) {
    completedPoints.push(completedEndpoint)
  }

  return completedPoints
}
