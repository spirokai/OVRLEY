import { useMemo } from 'react'
import { getPreviewMarkerLayers, normalizePreviewOpacity } from '../utils/svgPreviewUtils'

/**
 * Builds the presentation model for the route preview renderer.
 *
 * Keeps route styling concerns in one place: viewport dimensions, resolved
 * line widths/colors/opacities, marker styling, and marker inset geometry used
 * during route normalization.
 *
 * Stages:
 * 1. Sanitize viewport dimensions and preview scale.
 * 2. Resolve line styling for both geometry and rendered strokes.
 * 3. Derive marker styling and inset radius for route normalization.
 *
 * @param {object} data - Effective route widget data.
 * @param {number} globalScale - Scene/global scale applied to the preview.
 * @returns {object} Style model consumed by the route preview renderer.
 */
export function useRoutePreviewStyle(data, globalScale) {
  return useMemo(() => {
    // Viewport and scale: ensure valid raster bounds.
    const width = Math.max(data.width, 80)
    const height = Math.max(data.height, 80)
    const safeGlobalScale = Math.max(Number(globalScale) || 1, 0.1)

    // Stroke widths: shared by geometry inset calculations and SVG rendering.
    const remainingLineWidth = Number(data.remaining_line_width)
    const completedLineWidth = Number(data.completed_line_width)

    // Marker sizing: reserve enough inset so the route stroke and marker do not clip.
    const markerSize = Number(data.marker_size)
    const markerVariantDiameter = Number(data.marker_variant_diameter)
    const routeMarkerInsetRadius = Math.max(markerSize, markerVariantDiameter * 0.5)
    const markerColor = data.marker_color
    const markerOpacity = normalizePreviewOpacity(data.marker_opacity, 1)

    return {
      width,
      height,
      safeGlobalScale,
      remainingLineWidth,
      completedLineWidth,
      remainingLineColor: data.remaining_line_color,
      completedLineColor: data.completed_line_color,
      remainingLineOpacity: normalizePreviewOpacity(data.remaining_line_opacity, 0.75),
      completedLineOpacity: normalizePreviewOpacity(data.completed_line_opacity, 1),
      routeMarkerInsetRadius,
      markerLayers: getPreviewMarkerLayers(data, markerSize, markerColor, markerOpacity),
    }
  }, [data, globalScale])
}
