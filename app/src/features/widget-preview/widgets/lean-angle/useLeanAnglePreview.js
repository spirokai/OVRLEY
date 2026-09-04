import { useId, useMemo } from 'react'
import { getInterpolatedActivityValue } from '@/features/overlay-editor/utils/overlayEditorUtils'
import { formatStandardMetricDisplay } from '../metric/format'
import { getTextShadowParts } from '../../shared/shadow'
import { getMetricWidgetLayout, getPreviewFontFamily } from '../../shared/textMeasurement'
import { useFontMetrics } from '../../shared/useFontMetrics'
import { getLeanAngleFillPath, getLeanAngleFillSweep, getLeanAngleInnerTrackPath, getLeanAngleLayout, getLeanAngleOuterTrackPath } from './geometry'

const DEGREE_UNIT_CENTERING_OFFSET_RATIO = 0.1

/**
 * Builds the lean-angle preview presentation for the current activity frame.
 * @param {{widget: object, activity: object|null, previewSecond: number, globalOpacity: number, sceneStyle: object|null}} params
 * @returns {object} Presentation model consumed by the lean-angle renderer.
 */
export function useLeanAnglePreview({ widget, activity, previewSecond, globalOpacity, sceneStyle }) {
  const maskId = useId()
  const fontFamily = getPreviewFontFamily(widget.data.font)
  useFontMetrics([{ fontFamily, fontSize: widget.data.font_size }])

  return useMemo(() => {
    const layout = getLeanAngleLayout({
      diameter: widget.data.diameter,
      track_thickness: widget.data.track_thickness,
      font_size: widget.data.font_size,
    })
    const raw = getInterpolatedActivityValue(activity, 'lean_angle', previewSecond)
    const missing = raw === null || raw === undefined
    const formatted = formatStandardMetricDisplay('lean_angle', missing ? null : Math.abs(raw), {
      ...widget.data,
      decimals: 0,
    })
    const unitText = missing || !widget.data.show_units ? '' : formatted.units
    const textLayout = getMetricWidgetLayout({
      fontSize: widget.data.font_size,
      fontFamily,
      valueText: formatted.value,
      unitText,
      showIcon: false,
      showUnits: Boolean(unitText),
      iconSize: 0,
      contentAlignment: 'left',
    })
    const degreeUnitOffset = unitText ? widget.data.font_size * DEGREE_UNIT_CENTERING_OFFSET_RATIO : 0
    const textOriginX = layout.centerX + widget.data.value_offset_x + degreeUnitOffset - textLayout.width / 2
    const textOriginY = layout.centerY + widget.data.value_offset_y - textLayout.height / 2

    return {
      maskId,
      innerTrackClipId: `${maskId}-inner-track`,
      shadow: getTextShadowParts(sceneStyle),
      shadowFilterId: `lean-angle-${widget.id}-shadow`,
      valueShadowFilterId: `lean-angle-${widget.id}-value-shadow`,
      unitShadowFilterId: `lean-angle-${widget.id}-unit-shadow`,
      layout,
      outerTrackPath: getLeanAngleOuterTrackPath(layout),
      innerTrackPath: getLeanAngleInnerTrackPath(layout, widget.data.track_border_thickness),
      fillPath: getLeanAngleFillPath(layout, raw, widget.data.track_border_thickness),
      fillSweep: getLeanAngleFillSweep(raw),
      opacity: widget.data.opacity * globalOpacity,
      valueText: formatted.value,
      unitText,
      fontFamily,
      textLayout,
      textOriginX,
      textOriginY,
    }
  }, [activity, fontFamily, globalOpacity, maskId, previewSecond, sceneStyle, widget])
}
