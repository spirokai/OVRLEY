/**
 * Provides overlay editor helpers for widget preview.
 */

import { memo } from 'react'
import {
  OverlayElevationWidget,
  OverlayMetricWidget,
  OverlayRouteWidget,
  OverlayTextWidget,
} from './widgetPreviewRenderers'

/**
 * Renders the widget preview component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.activity - Parsed activity data for previews or rendering.
 * @param {*} props.previewSecond - Preview time in seconds.
 * @param {*} props.globalOpacity - Global opacity multiplier applied to the widget.
 * @returns {JSX.Element} Rendered component output.
 */
function WidgetPreview({
  widget,
  activity,
  previewSecond,
  globalOpacity,
  globalScale,
  metricPreviewModel,
  sceneFont,
  sceneFontSize,
  sceneStyle,
  valueFont,
}) {
  if (widget.type === 'label') {
    return (
      <OverlayTextWidget
        widget={widget}
        globalOpacity={globalOpacity}
        sceneStyle={sceneStyle}
      />
    )
  }

  if (widget.type === 'course') {
    return (
      <OverlayRouteWidget
        widget={widget}
        activity={activity}
        previewSecond={previewSecond}
        globalOpacity={globalOpacity}
        globalScale={globalScale}
        sceneFont={sceneFont}
        sceneFontSize={sceneFontSize}
        sceneStyle={sceneStyle}
        valueFont={valueFont}
      />
    )
  }

  if (widget.type === 'elevation') {
    return (
      <OverlayElevationWidget
        widget={widget}
        activity={activity}
        previewSecond={previewSecond}
        globalOpacity={globalOpacity}
        globalScale={globalScale}
        sceneFont={sceneFont}
        sceneFontSize={sceneFontSize}
        sceneStyle={sceneStyle}
        valueFont={valueFont}
      />
    )
  }

  return (
    <OverlayMetricWidget
      widget={widget}
      activity={activity}
      previewSecond={previewSecond}
      globalOpacity={globalOpacity}
      globalScale={globalScale}
      metricPreviewModel={metricPreviewModel}
      sceneStyle={sceneStyle}
    />
  )
}

export default memo(
  WidgetPreview,
  (previousProps, nextProps) =>
    previousProps.widget === nextProps.widget &&
    previousProps.activity === nextProps.activity &&
    previousProps.previewSecond === nextProps.previewSecond &&
    previousProps.globalOpacity === nextProps.globalOpacity &&
    previousProps.globalScale === nextProps.globalScale &&
    previousProps.metricPreviewModel === nextProps.metricPreviewModel &&
    previousProps.sceneFont === nextProps.sceneFont &&
    previousProps.sceneFontSize === nextProps.sceneFontSize &&
    previousProps.sceneStyle === nextProps.sceneStyle &&
    previousProps.valueFont === nextProps.valueFont,
)
