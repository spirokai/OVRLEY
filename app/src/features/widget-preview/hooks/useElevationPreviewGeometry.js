import { useState, useEffect, useMemo } from 'react'
import {
  getDistanceProgressAtElapsed,
  getInterpolatedSeriesValue,
  getWindowProgressAtTime,
  resolveExportRangeWindow,
} from '@/features/overlay-editor'
import { buildElevationGeometry, hasTauriRuntime } from '@/api/backend'
import { areaToSvg, findPointAtProgress, pointsToSvg } from '@/lib/geometryUtils'
import { buildPlaceholderElevationPreviewGeometry } from '../utils/placeholderPlotGeometry'
import { buildElevationCompletedPoints } from '../utils/svgPreviewUtils'
import useStore from '@/store/useStore'

function normalizeElevationGeometry(geometry) {
  if (!geometry || !Array.isArray(geometry.points) || !Array.isArray(geometry.progressValues)) {
    return null
  }

  if (!Array.isArray(geometry.elapsedFractions) || !Array.isArray(geometry.dataRange) || geometry.dataRange.length !== 2) {
    return null
  }

  return geometry
}

function projectElevationValueToSvgY(elevationValue, dataRange, height, yScale) {
  const [minElevation, maxElevation] = Array.isArray(dataRange) ? dataRange : [NaN, NaN]
  if (!Number.isFinite(elevationValue) || !Number.isFinite(minElevation) || !Number.isFinite(maxElevation)) {
    return null
  }

  const safeHeight = Math.max(Number(height) || 0, 1)
  const safeYScale = Number(yScale) || 1
  const span = Math.max(maxElevation - minElevation, 1e-9)
  const normalized = (elevationValue - minElevation) / span
  const centered = Math.min(Math.max((normalized - 0.5) * safeYScale + 0.5, 0), 1)

  return safeHeight - safeHeight * centered
}

/**
 * Builds the geometry model for the elevation preview renderer.
 *
 * Rust handles the expensive geometry pipeline (smoothing, downsampling,
 * projection, RDP simplification) via IPC. This hook consumes the result
 * and performs cheap per-frame operations locally (marker interpolation,
 * completed polyline, SVG paths) that must run at 30fps.
 *
 * For canvas-parity testing, window.__OVRLEY_MOCK_ELEVATION_GEOMETRY
 * injects pre-computed Rust geometry so Skia and SVG use identical data.
 *
 * @param {object} params
 * @param {object} params.activity - Activity data with elevation samples.
 * @param {object} params.data - Effective elevation widget data.
 * @param {object} params.exportRange - Active export-range selection.
 * @param {number} params.previewSecond - Current preview timestamp in seconds.
 * @param {object} params.style - Style model returned by useElevationPreviewStyle.
 * @returns {object|null} Geometry model for the renderer, or null while loading.
 */
export function useElevationPreviewGeometry({ activity, data, exportRange, previewSecond, style }) {
  const [rustGeometry, setRustGeometry] = useState(null)
  const config = useStore((state) => state.config)
  const globalDefaults = useStore((state) => state.globalDefaults)
  const fallbackDurationSeconds = useStore((state) => state.fallbackDurationSeconds)

  const mockGeometry = typeof window !== 'undefined' ? window.__OVRLEY_MOCK_ELEVATION_GEOMETRY : null
  const exportWindow = useMemo(
    () => resolveExportRangeWindow(activity, exportRange, data.show_full_activity),
    [activity, exportRange, data.show_full_activity],
  )

  // Build the config Rust needs. The store scene lacks non-durable fields
  // (scale, shadow, border); globalDefaults fills them. start/end are
  // overridden when an export window is active so Rust trims source points.
  const geometryConfig = useMemo(() => {
    if (!config || !activity || !hasTauriRuntime() || mockGeometry) return null
    const duration = activity?.trim_end_seconds ?? 0
    const { updateRate, start, end, ...sceneRest } = config.scene

    return {
      ...config,
      scene: {
        ...globalDefaults,
        ...sceneRest,
        scale: style.safeGlobalScale,
        update_rate: updateRate,
        start: exportWindow.active ? exportWindow.start : (start ?? 0),
        end: exportWindow.active ? exportWindow.end : (end ?? duration),
        custom_export_range_active: exportWindow.active,
      },
    }
  }, [config, globalDefaults, activity, exportWindow, mockGeometry, style.safeGlobalScale])

  useEffect(() => {
    if (!geometryConfig) return

    let cancelled = false
    buildElevationGeometry(geometryConfig, activity).then((geometry) => {
      if (!cancelled) setRustGeometry(geometry)
    })
    return () => {
      cancelled = true
    }
  }, [geometryConfig, activity])

  if (!activity) {
    return buildPlaceholderElevationPreviewGeometry({
      width: style.width,
      height: style.height,
      previewSecond,
      fallbackDurationSeconds,
    })
  }

  const effectiveGeometry = normalizeElevationGeometry(mockGeometry ?? rustGeometry)
  if (!effectiveGeometry || !activity) return null

  // Rust computes at scaled resolution, but SVG needs widget-local coordinates.
  const points = effectiveGeometry.points.map(([x, y]) => [x / style.safeGlobalScale, y / style.safeGlobalScale])
  const lastPoint = points[points.length - 1] ?? null

  // Keep marker x distance-based so it stays put during hover/stop segments.
  const progress01 = exportWindow.active
    ? (getWindowProgressAtTime(activity, exportWindow, previewSecond) ?? 0)
    : getDistanceProgressAtElapsed(activity, previewSecond)

  // Completed profile fill is chronological, normalized to the same scoped duration
  // Rust used when building elapsedFractions.
  const sourceDuration = exportWindow.active ? exportWindow.end - exportWindow.start : activity.sample_elapsed_seconds?.at(-1) || 1
  const elapsedWindowStart = exportWindow.active ? exportWindow.start : 0
  const frameElapsedFraction = Math.min(Math.max((previewSecond - elapsedWindowStart) / Math.max(sourceDuration, 1e-9), 0), 1)

  const metricHit = findPointAtProgress(points, effectiveGeometry.progressValues, progress01)
  const elevationSeries = activity.sample_elevations.length ? activity.sample_elevations : activity.elevation
  const elevationValue = getInterpolatedSeriesValue(activity.sample_elapsed_seconds, elevationSeries, previewSecond)
  const markerX = metricHit?.point?.[0] ?? lastPoint?.[0] ?? null
  const markerY = projectElevationValueToSvgY(elevationValue, effectiveGeometry.dataRange, style.height, data.y_scale)
  const markerPoint = Number.isFinite(markerX) && Number.isFinite(markerY) ? [markerX, markerY] : null
  const completedPoints = buildElevationCompletedPoints(
    points,
    effectiveGeometry.progressValues,
    effectiveGeometry.elapsedFractions,
    progress01,
    frameElapsedFraction,
  )

  return {
    markerPoint,
    elevationValue,
    remainingSvgPoints: pointsToSvg(points),
    completedSvgPoints: pointsToSvg(completedPoints),
    areaSvgPoints: areaToSvg(points, style.width, style.height, null),
    completedAreaSvgPoints: areaToSvg(completedPoints, style.width, style.height, null),
  }
}
