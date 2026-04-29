import { memo } from 'react'
import {
  OverlayElevationWidget,
  OverlayMetricWidget,
  OverlayRouteWidget,
  OverlayTextWidget,
} from './widgetPreviewRenderers'

function WidgetPreview({ widget, activity, previewSecond, globalOpacity }) {
  if (widget.type === 'label') {
    return <OverlayTextWidget widget={widget} globalOpacity={globalOpacity} />
  }

  if (widget.type === 'course') {
    return (
      <OverlayRouteWidget
        widget={widget}
        activity={activity}
        previewSecond={previewSecond}
        globalOpacity={globalOpacity}
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
      />
    )
  }

  return (
    <OverlayMetricWidget
      widget={widget}
      activity={activity}
      previewSecond={previewSecond}
      globalOpacity={globalOpacity}
    />
  )
}

export default memo(
  WidgetPreview,
  (previousProps, nextProps) =>
    previousProps.widget === nextProps.widget &&
    previousProps.activity === nextProps.activity &&
    previousProps.previewSecond === nextProps.previewSecond &&
    previousProps.globalOpacity === nextProps.globalOpacity,
)
