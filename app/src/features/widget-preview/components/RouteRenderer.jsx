/**
 * Renders the overlay route widget SVG preview â€” draws remaining/completed
 * polylines, shadow layers, and marker indicators for the course route.
 *
 * All data is received via props; no store access.
 *
 * @param {object} props
 * @param {object} props.widget - Widget configuration object.
 * @param {object} props.activity - Activity data with route samples.
 * @param {number} props.previewSecond - Current preview time in seconds.
 * @param {number} props.globalOpacity - Global opacity multiplier.
 * @param {number} props.globalScale - Global scale multiplier.
 * @param {object} props.sceneStyle - Scene style object (shadow, border).
 * @param {object} props.exportRange - Export range configuration.
 * @returns {JSX.Element} SVG element for route preview.
 */

import { getWidgetOpacity } from '../utils/textMeasurement'
import { getTextShadowParts } from '../utils/shadowUtils'
import { sanitizeSvgId } from '../utils/svgPreviewUtils'
import { PreviewMarkerLayers, PreviewPolylineShadow, PreviewSvgShadowBlurFilter } from './previewSvgComponents'
import { useRoutePreviewGeometry } from '../hooks/useRoutePreviewGeometry'
import { useRoutePreviewStyle } from '../hooks/useRoutePreviewStyle'

export function OverlayRouteWidget({ widget, activity, previewSecond, globalOpacity, globalScale, sceneStyle, exportRange }) {
  const data = widget.data
  const style = useRoutePreviewStyle(data, globalScale)
  const geometry = useRoutePreviewGeometry({ activity, data, exportRange, previewSecond, style })

  if (!geometry) {
    return null
  }

  const shadow = getTextShadowParts(sceneStyle)
  const shadowFilterId = sanitizeSvgId(`${widget.id}-route-shadow-blur`)

  return (
    <svg
      width={style.width}
      height={style.height}
      viewBox={`0 0 ${style.width} ${style.height}`}
      className="block h-full w-full"
      style={{ opacity: getWidgetOpacity(data, globalOpacity) }}
    >
      <PreviewSvgShadowBlurFilter id={shadowFilterId} shadow={shadow} />
      <g>
        <PreviewPolylineShadow
          points={geometry.remainingSvgPoints}
          shadow={shadow}
          blurFilterId={shadowFilterId}
          strokeWidth={style.remainingLineWidth}
          strokeOpacity={style.remainingLineOpacity}
          rotation={data.rotation ?? 0}
        />
        <polyline
          fill="none"
          stroke={style.remainingLineColor}
          strokeOpacity={style.remainingLineOpacity}
          strokeWidth={style.remainingLineWidth}
          strokeLinejoin="round"
          strokeLinecap="round"
          points={geometry.remainingSvgPoints}
        />
        <polyline
          fill="none"
          stroke={style.completedLineColor}
          strokeOpacity={style.completedLineOpacity}
          strokeWidth={style.completedLineWidth}
          strokeLinejoin="round"
          strokeLinecap="round"
          points={geometry.completedSvgPoints}
        />
        <PreviewMarkerLayers layers={style.markerLayers} x={geometry.markerPoint?.[0]} y={geometry.markerPoint?.[1]} />
      </g>
    </svg>
  )
}
