import { useState, useEffect, useMemo } from 'react'
import { getDistanceProgressAtElapsed, getWindowProgressAtTime, resolveExportRangeWindow } from '@/features/overlay-editor'
import { buildRouteGeometry, hasTauriRuntime } from '@/api/backend'
import { pointsToSvg } from '@/lib/geometryUtils'
import { buildRouteFramePreview } from '../utils/svgPreviewUtils'
import useStore from '@/store/useStore'

/**
 * Builds the geometry model for the route preview renderer.
 *
 * Rust handles the expensive geometry pipeline (Mercator projection,
 * LTTB downsampling, RDP simplification, widget fitting) via IPC.
 * This hook consumes the result and performs cheap per-frame operations
 * locally (marker interpolation, completed segment, SVG paths) that
 * must run at 30fps.
 *
 * For canvas-parity testing, window.__OVRLEY_MOCK_ROUTE_GEOMETRY
 * injects pre-computed Rust geometry so Skia and SVG use identical data.
 *
 * @param {object} params
 * @param {object} params.activity - Activity data with route samples.
 * @param {object} params.data - Effective route widget data.
 * @param {object} params.exportRange - Active export-range selection.
 * @param {number} params.previewSecond - Current preview timestamp in seconds.
 * @param {object} params.style - Style model returned by useRoutePreviewStyle.
 * @returns {object|null} Geometry model for the renderer, or null while loading.
 */
export function useRoutePreviewGeometry({ activity, data, exportRange, previewSecond, style }) {
  const [rustGeometry, setRustGeometry] = useState(null)
  const config = useStore((state) => state.config)
  const globalDefaults = useStore((state) => state.globalDefaults)

  const mockGeometry = typeof window !== 'undefined' ? window.__OVRLEY_MOCK_ROUTE_GEOMETRY : null

  // Build the config Rust needs. The store scene lacks non-durable fields
  // (scale, shadow, border) — globalDefaults fills them. start/end are
  // overridden when an export window is active so Rust trims source points.
  const geometryConfig = useMemo(() => {
    if (!config || !activity || !hasTauriRuntime() || mockGeometry) return null
    const duration = activity?.trim_end_seconds ?? 0
    const exportWindow = resolveExportRangeWindow(activity, exportRange, data.show_full_activity)
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
  }, [config, globalDefaults, activity, exportRange, mockGeometry, style.safeGlobalScale, data.show_full_activity])

  useEffect(() => {
    if (!geometryConfig) return

    let cancelled = false
    buildRouteGeometry(geometryConfig, activity).then((geometry) => {
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

  const { markerPoint, completedPoints } = buildRouteFramePreview(points, pointProgress, progress01)

  return {
    markerPoint,
    remainingSvgPoints: pointsToSvg(points),
    completedSvgPoints: pointsToSvg(completedPoints),
  }
}
