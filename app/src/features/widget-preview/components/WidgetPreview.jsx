/**
 * Renders the widget preview component — dispatches to the appropriate
 * renderer based on widget type.
 */

import { memo } from 'react'
import { OverlayTextWidget } from './TextRenderer'
import { OverlayMetricWidget } from './MetricRenderer'
import { OverlayRouteWidget } from './RouteRenderer'
import { OverlayElevationWidget } from './ElevationRenderer'

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
  exportRange,
}) {
  if (widget.type === 'label') {
    return <OverlayTextWidget widget={widget} globalOpacity={globalOpacity} sceneStyle={sceneStyle} />
  }

  if (widget.type === 'course') {
    return (
      <OverlayRouteWidget
        widget={widget}
        activity={activity}
        previewSecond={previewSecond}
        globalOpacity={globalOpacity}
        globalScale={globalScale}
        sceneStyle={sceneStyle}
        exportRange={exportRange}
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
        exportRange={exportRange}
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
    previousProps.valueFont === nextProps.valueFont &&
    previousProps.exportRange === nextProps.exportRange,
)
