import { useMemo } from 'react'
import { getInterpolatedActivityValue } from '@/features/overlay-editor'
import { getPreviewFontFamily, getWidgetOpacity } from '../utils/textMeasurement'
import { headingOffset, headingTapeLayout, visibleLabels, visibleTicks } from '../utils/headingGeometry'
import { getTextShadowParts } from '../utils/shadowUtils'
import { sanitizeSvgId } from '../utils/svgPreviewUtils'
import { useFontMetricsVersion } from './useFontMetricsVersion'

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
  const data = widget.data

  // Typography: heading labels need font metrics ready before the tape is drawn.
  const labelFontSize = data.label_font_size
  const labelFontFamily = getPreviewFontFamily(data.label_font || valueFont || sceneFont)
  useFontMetricsVersion(labelFontFamily, labelFontSize)

  return useMemo(() => {
    // Viewport and opacity: boxed heading widgets guarantee geometry; clamp only invalid transient values.
    const scale = globalScale ?? 1
    const width = Math.max(data.width, 1)
    const layout = headingTapeLayout(data)
    const bodyHeight = layout.bodyHeight
    const tickScaleHeight = layout.tickScaleHeight
    const totalHeight = layout.totalHeight
    const ppd = data.pixels_per_degree
    const opacity = getWidgetOpacity(data, globalOpacity)
    const tapeWidth = 360 * ppd

    // Heading interpolation: convert the live heading sample into tape offset.
    const heading = getInterpolatedActivityValue(activity, 'heading', previewSecond)
    const offset = headingOffset(heading, ppd, width)
    const wrappedOffset = ((offset % tapeWidth) + tapeWidth) % tapeWidth

    // Tick/label derivation: build the visible tape content for the current config.
    const ticks = visibleTicks(0, ppd, tapeWidth, data.major_tick_interval, data.minor_ticks_per_major, data.show_major_ticks, data.show_minor_ticks)
    const labels = visibleLabels(ticks, data.show_minor_labels, data.show_major_labels)

    return {
      data,
      width,
      bodyHeight,
      bodyY: layout.bodyY,
      tickScaleHeight,
      totalHeight,
      displayWidth: width * scale,
      displayHeight: totalHeight * scale,
      opacity,
      tapeWidth,
      labelFontFamily,
      ticks,
      labels,
      wrappedOffset,
      shadow: getTextShadowParts(sceneStyle),
      shadowFilterId: sanitizeSvgId(`${widget.id}-shadow`),
      clipPathId: sanitizeSvgId(`${widget.id}-clip`),
    }
  }, [activity, data, globalOpacity, globalScale, labelFontFamily, previewSecond, sceneStyle, widget.id])
}
