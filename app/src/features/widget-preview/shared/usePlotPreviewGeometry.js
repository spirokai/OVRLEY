import { useEffect, useMemo, useRef, useState } from 'react'
import { hasTauriRuntime } from '@/api/backend'
import { resolveExportRangeWindow } from '@/features/overlay-editor/utils/exportRange'
import { pointsToSvg, areaToSvg } from '@/lib/geometryUtils'
import useStore from '@/store/useStore'

const ROUTE_GEOMETRY_FIELDS = [
  'width',
  'height',
  'simplify_tolerance_px',
  'target_density',
  'completed_line_width',
  'remaining_line_width',
  'marker_size',
  'marker_variant_diameter',
  'show_full_activity',
]
const ELEVATION_GEOMETRY_FIELDS = ['width', 'height', 'y_scale', 'simplify_tolerance_px', 'target_density', 'show_full_activity']

function normalizePlotDimensions(plotData) {
  return { ...plotData, width: Math.round(plotData.width), height: Math.round(plotData.height) }
}

function getPlotGeometrySignature(data, plotType) {
  const fields = plotType === 'course' ? ROUTE_GEOMETRY_FIELDS : ELEVATION_GEOMETRY_FIELDS
  const width = Math.round(data.width)
  const height = Math.round(data.height)
  return JSON.stringify(
    fields.map((field) => {
      if (field === 'width') return [field, width]
      if (field === 'height') return [field, height]
      return [field, data[field]]
    }),
  )
}

function replaceGeometryPlot(config, plotType, plotData) {
  const plotIndex = config.plots.findIndex((plot) => plot.id === plotData.id || plot.value === plotType)
  if (plotIndex === -1) return config

  const plots = [...config.plots]
  plots[plotIndex] = { ...plots[plotIndex], ...plotData }
  return { ...config, plots }
}

function buildPlotGeometryConfig({ config, globalDefaults, activity, exportWindow, globalScale, plotType, plotData, widgetUpdateRate }) {
  if (!config || !activity || !hasTauriRuntime()) return null

  const duration = activity.trim_end_seconds
  const { start, end, updateRate: _updateRate, ...sceneRest } = config.scene
  const geometryConfig = replaceGeometryPlot(config, plotType, normalizePlotDimensions(plotData))

  return {
    ...geometryConfig,
    scene: {
      ...globalDefaults,
      ...sceneRest,
      scale: globalScale,
      update_rate: widgetUpdateRate,
      start: exportWindow.active ? exportWindow.start : (start ?? 0),
      end: exportWindow.active ? exportWindow.end : (end ?? duration),
      custom_export_range_active: exportWindow.active,
    },
  }
}

/**
 * Owns shared Rust geometry preparation and static SVG serialization for plot previews.
 *
 * @param {object} params
 * @param {object|null} params.activity - Stable parsed activity.
 * @param {object} params.data - Effective plot widget data.
 * @param {object|null} params.exportRange - Active export-range selection.
 * @param {object} params.style - Plot style containing globalScale.
 * @param {'course'|'elevation'} params.plotType - Backend plot type.
 * @param {function} params.buildGeometry - Rust geometry command.
 * @param {string} params.mockGeometryKey - Window key for test geometry.
 * @param {boolean} [params.includeArea=false] - Whether to serialize a static area path.
 * @returns {object} Shared geometry state for the plot-specific frame hook.
 */
export function usePlotPreviewGeometry({ activity, data, exportRange, style, plotType, buildGeometry, mockGeometryKey, includeArea = false }) {
  const [rustGeometry, setRustGeometry] = useState(null)
  const config = useStore((state) => state.config)
  const globalDefaults = useStore((state) => state.globalDefaults)
  const fallbackDurationSeconds = useStore((state) => state.fallbackDurationSeconds)
  const widgetUpdateRate = useStore((state) => state.renderSettings.widgetUpdateRate)
  const plotWidth = Math.round(data.width)
  const plotHeight = Math.round(data.height)
  const exportWindow = useMemo(
    () => resolveExportRangeWindow(activity, exportRange, data.show_full_activity),
    [activity, data.show_full_activity, exportRange],
  )
  const geometrySignature = getPlotGeometrySignature(data, plotType)
  const latestInputsRef = useRef(null)

  useEffect(() => {
    latestInputsRef.current = {
      activity,
      config,
      plotData: data,
      exportWindow,
      globalDefaults,
      globalScale: style.globalScale,
      plotType,
      widgetUpdateRate,
    }
  }, [activity, config, data, exportWindow, globalDefaults, plotType, style.globalScale, widgetUpdateRate])

  useEffect(() => {
    const geometryConfig = buildPlotGeometryConfig(latestInputsRef.current)
    if (!geometryConfig) return undefined

    if (typeof window !== 'undefined' && window[mockGeometryKey]) {
      setRustGeometry(window[mockGeometryKey])
      return undefined
    }

    let cancelled = false
    buildGeometry(geometryConfig, activity).then((geometry) => {
      if (!cancelled) setRustGeometry(geometry)
    })
    return () => {
      cancelled = true
    }
  }, [
    activity,
    buildGeometry,
    config?.scene?.end,
    config?.scene?.start,
    exportWindow,
    geometrySignature,
    mockGeometryKey,
    style.globalScale,
    widgetUpdateRate,
  ])

  const points = useMemo(
    () =>
      rustGeometry
        ? rustGeometry.points.map(([x, y]) => [(x * plotWidth) / rustGeometry.widgetWidth, (y * plotHeight) / rustGeometry.widgetHeight])
        : null,
    [plotHeight, plotWidth, rustGeometry],
  )
  const remainingSvgPoints = useMemo(() => (points ? pointsToSvg(points) : null), [points])
  const areaSvgPoints = useMemo(
    () => (includeArea && points ? areaToSvg(points, plotWidth, plotHeight, null) : null),
    [plotHeight, plotWidth, includeArea, points],
  )

  return { areaSvgPoints, exportWindow, fallbackDurationSeconds, points, remainingSvgPoints, rustGeometry }
}
