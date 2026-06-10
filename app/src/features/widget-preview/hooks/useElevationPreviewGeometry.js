import { useState, useEffect, useMemo } from 'react'
import {
  getDistanceProgressAtElapsed,
  getInterpolatedSeriesValue,
  getWindowProgressAtTime,
  resolveExportRangeWindow,
} from '@/features/overlay-editor'
import { buildElevationGeometry, hasTauriRuntime } from '@/api/backend'
import { areaToSvg, getPointAtMetricProgress, pointsToSvg } from '@/lib/geometryUtils'
import { buildElevationCompletedPoints } from '../utils/svgPreviewUtils'
import useStore from '@/store/useStore'

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

  const mockGeometry = typeof window !== 'undefined' ? window.__OVRLEY_MOCK_ELEVATION_GEOMETRY : null

  // Build the config Rust needs. The store scene lacks non-durable fields
  // (scale, shadow, border) — globalDefaults fills them. start/end are
  // overridden when an export window is active so Rust trims source points.
  const geometryConfig = useMemo(() => {
    if (!config || !activity || !hasTauriRuntime() || mockGeometry) return null
    const duration = activity?.trim_end_seconds ?? 0
    const exportWindow = resolveExportRangeWindow(activity, exportRange, false)
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
  }, [config, globalDefaults, activity, exportRange, mockGeometry, style.safeGlobalScale])

  useEffect(() => {
    if (!geometryConfig) return

    let cancelled = false
    buildElevationGeometry(geometryConfig, activity).then((geometry) => {
      if (!cancelled) setRustGeometry(geometry)
    })
    return () => {
      cancelled = true
    }
  }, [geometryConfig, activity, data])

  const effectiveGeometry = mockGeometry ?? rustGeometry
  if (!effectiveGeometry || !activity) return null

  // Rust computes at scaled resolution (scene.width × scale), but SVG
  // needs unscaled widget-local coordinates.
  const points = effectiveGeometry.points.map(([x, y]) => [x / style.safeGlobalScale, y / style.safeGlobalScale])
  const pointProgress = effectiveGeometry.progressValues

  // progress01 drives marker placement and completed polyline. Export
  // window normalizes it to 0..1 within the trimmed range.
  const exportWindow = resolveExportRangeWindow(activity, exportRange, data.show_full_activity)
  const progress01 = exportWindow.active
    ? (getWindowProgressAtTime(activity, exportWindow, previewSecond) ?? 0)
    : getDistanceProgressAtElapsed(activity, previewSecond)

  const markerPoint = getPointAtMetricProgress(points, pointProgress, progress01) || points[points.length - 1]
  const completedPoints = buildElevationCompletedPoints(points, pointProgress, progress01, markerPoint)

  // Numeric elevation for the label — interpolated from raw series at
  // current distance progress. sample_elevations preferred over raw elevation.
  const elevationValue = getInterpolatedSeriesValue(
    activity.sample_distance_progress,
    activity.sample_elevations.length ? activity.sample_elevations : activity.elevation,
    progress01,
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
