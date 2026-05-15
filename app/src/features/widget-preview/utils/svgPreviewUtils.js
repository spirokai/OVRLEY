/**
 * Shared SVG preview utility functions used across per-widget renderers.
 */

import { getPointAtMetricProgressWithIndex, getPointAtProgress } from '@/lib/geometryUtils'

export function sanitizeSvgId(value) {
  return String(value || 'preview-shadow').replace(/[^a-zA-Z0-9_-]/g, '_')
}

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

export function pointsEqual(left, right) {
  if (!left || !right) {
    return false
  }

  return Math.hypot(right[0] - left[0], right[1] - left[1]) <= 1e-3
}

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

export function resolvePreviewStyleColor(explicitColor, inheritedColor, baseColor) {
  return explicitColor || inheritedColor || baseColor || '#ffffff'
}

export function resolvePreviewLineWidth(explicitWidth, legacyWidth) {
  const numericExplicit = Number(explicitWidth)
  if (Number.isFinite(numericExplicit)) {
    return numericExplicit
  }

  const numericLegacy = Number(legacyWidth)
  return (Number.isFinite(numericLegacy) ? numericLegacy : 1.75) * 2.5
}

export function resolveScaledPreviewLineWidth(explicitWidth, legacyWidth, globalScale) {
  const safeScale = Math.max(Number(globalScale) || 1, 0.1)
  const numericExplicit = Number(explicitWidth)

  if (Number.isFinite(numericExplicit)) {
    return numericExplicit / safeScale
  }

  return resolvePreviewLineWidth(undefined, legacyWidth)
}

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

  return markerPoints
    .map((point) => ({
      radius: Math.max(Math.sqrt(Math.max(Number(point.weight) || 80, 1)), 2),
      color: point.color || '#ffffff',
      opacity: normalizePreviewOpacity(point.opacity, 1),
    }))
    .sort((left, right) => right.radius - left.radius)
    .map((layer, index, layers) => ({
      ...layer,
      solidFill: index === layers.length - 1,
    }))
}

export function buildRouteFramePreview(points, progressValues, progress01) {
  if (!points.length) {
    return { markerPoint: null, completedPoints: [] }
  }

  const metricPoint = getPointAtMetricProgressWithIndex(points, progressValues, progress01)
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

export function buildElevationCompletedPoints(points, progressValues, progress01, markerPoint) {
  if (!points.length) {
    return []
  }

  const completedPoints = points.filter((_, index) => (progressValues[index] ?? 0) <= progress01)

  if (!completedPoints.length) {
    completedPoints.push(points[0])
  }

  if (markerPoint && !pointsEqual(completedPoints[completedPoints.length - 1], markerPoint)) {
    completedPoints.push(markerPoint)
  }

  return completedPoints
}
