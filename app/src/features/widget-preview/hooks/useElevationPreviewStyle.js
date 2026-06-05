import { useMemo } from 'react'
import { getPreviewFontFamily } from '../utils/textMeasurement'
import { normalizePreviewOpacity } from '../utils/svgPreviewUtils'
import { useFontMetricsVersion } from './useFontMetricsVersion'

/**
 * Builds the presentation model for the elevation preview renderer.
 *
 * This hook assumes `data` is already normalized/effective widget data and
 * derives the SVG-facing style values the renderer needs: clamped dimensions,
 * scale-adjusted stroke widths, normalized opacities, marker styling, and
 * resolved point-label font settings.
 *
 * Stages:
 * 1. Sanitize viewport dimensions and preview scale.
 * 2. Resolve line, area, marker, and label presentation values.
 * 3. Trigger font-metric loading so downstream text measurement is stable.
 *
 * @param {object} data - Effective elevation widget data.
 * @param {number} globalScale - Scene/global scale applied to the preview.
 * @returns {object} Style model consumed by the elevation preview renderer.
 */
export function useElevationPreviewStyle(data, globalScale) {
  const style = useMemo(() => {
    // Viewport and scale: keep the SVG valid.
    const width = Math.max(Number(data.width), 1)
    const height = Math.max(Number(data.height), 1)
    const safeGlobalScale = Math.max(Number(globalScale) || 1, 0.1)

    // Label typography: resolve the effective point-label font contract.
    const labelFontSize = data.point_label.font_size
    const labelFontFamily = getPreviewFontFamily(data.point_label.font || data.point_label.font_family)

    // Marker styling: derive the SVG marker radius/color/opacity inputs.
    const markerSize = Number(data.marker_size)
    const markerOpacity = normalizePreviewOpacity(data.marker_opacity, 1)

    return {
      width,
      height,
      safeGlobalScale,
      remainingLineWidth: Number(data.remaining_line_width),
      completedLineWidth: Number(data.completed_line_width),
      remainingLineOpacity: normalizePreviewOpacity(data.remaining_line_opacity, 1),
      completedLineOpacity: normalizePreviewOpacity(data.completed_line_opacity, 1),
      markerSize,
      markerColor: data.marker_color,
      markerOpacity,
      labelFontSize,
      labelFontFamily,
      remainingAreaOpacity: normalizePreviewOpacity(data.area_remaining_opacity, 0.12),
      completedAreaOpacity: normalizePreviewOpacity(data.area_completed_opacity, 0.24),
      labelColor: data.point_label.color,
      showMetricLabel: data.show_elevation_metric,
      showImperialLabel: data.show_elevation_imperial,
    }
  }, [data, globalScale])

  // Font metrics must be loaded before text measurement and baseline math run.
  useFontMetricsVersion(style.labelFontFamily, style.labelFontSize)

  return style
}
