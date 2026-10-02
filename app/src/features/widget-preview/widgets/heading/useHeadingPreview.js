import { useMemo } from 'react'
import { getInterpolatedActivityValue } from '@/features/overlay-editor/utils/overlayEditorUtils'
import { getPreviewFontFamily, getWidgetOpacity } from '../../shared/textMeasurement'
import { headingOffset, headingTapeLayout, visibleLabels, visibleTicks } from './geometry'
import { getTextShadowParts } from '../../shared/shadow'
import { sanitizeSvgId } from '../../shared/svgPreviewUtils'
import { useFontMetrics } from '../../shared/useFontMetrics'

/**
 * Builds the preview model for the heading-tape renderer.
 *
 * Centralizes heading-specific sizing, font/shadow setup, tape offset math,
 * tick/label derivation, and reusable SVG ids so the renderer can focus on
 * drawing the tape and indicator.
 *
 * Stages:
 * 1. Resolve font metrics and viewport sizing.
 * 2. Interpolate the current heading and derive tape offset.
 * 3. Build visible ticks/labels and shared SVG identifiers.
 *
 * @param {object} params - Heading preview inputs.
 * @param {object} params.widget - Effective heading widget.
 * @param {object|null} params.activity - Activity data with heading series.
 * @param {number} params.previewSecond - Current preview timestamp in seconds.
 * @param {number} params.globalOpacity - Global opacity multiplier.
 * @param {number} params.globalScale - Global scale multiplier.
 * @param {string} params.sceneFont - Scene-level font family.
 * @param {string} params.valueFont - Value-font override.
 * @param {object} params.sceneStyle - Scene style object.
 * @returns {object} Preview model consumed by the heading preview renderer.
 */
export function useHeadingPreviewModel({ widget, activity, previewSecond, globalOpacity, globalScale, sceneFont, valueFont, sceneStyle }) {
  // Typography: heading labels need font metrics ready before the tape is drawn.
  const labelFontFamily = getPreviewFontFamily(widget.data.label_font ?? valueFont ?? sceneFont)
  useFontMetrics([{ fontId: widget.data.label_font ?? valueFont ?? sceneFont, fontSize: widget.data.label_font_size }])

  return useMemo(() => {
    // Viewport and opacity: boxed heading widgets guarantee geometry; clamp only invalid transient values.
    const scale = globalScale ?? 1
    const layout = headingTapeLayout(widget.data)
    const tapeWidth = 360 * widget.data.pixels_per_degree

    // Heading interpolation: convert the live heading sample into tape offset.
    const heading = getInterpolatedActivityValue(activity, 'heading', previewSecond)
    const offset = headingOffset(heading, widget.data.pixels_per_degree, widget.data.width)
    const wrappedOffset = ((offset % tapeWidth) + tapeWidth) % tapeWidth

    // Tick/label derivation: build the visible tape content for the current config.
    const ticks = visibleTicks(
      0,
      widget.data.pixels_per_degree,
      tapeWidth,
      widget.data.major_tick_interval,
      widget.data.minor_ticks_per_major,
      widget.data.show_major_ticks,
      widget.data.show_minor_ticks,
    )
    const labels = visibleLabels(ticks, widget.data.show_minor_labels, widget.data.show_major_labels)

    return {
      bodyHeight: layout.bodyHeight,
      bodyY: layout.bodyY,
      tickScaleHeight: layout.tickScaleHeight,
      totalHeight: layout.totalHeight,
      displayWidth: widget.data.width * scale,
      displayHeight: layout.totalHeight * scale,
      opacity: getWidgetOpacity(widget.data, globalOpacity),
      tapeWidth,
      labelFontFamily,
      ticks,
      labels,
      wrappedOffset,
      shadow: getTextShadowParts(sceneStyle),
      shadowFilterId: sanitizeSvgId(`${widget.id}-shadow`),
      clipPathId: sanitizeSvgId(`${widget.id}-clip`),
    }
  }, [activity, globalOpacity, globalScale, labelFontFamily, previewSecond, sceneStyle, widget])
}
