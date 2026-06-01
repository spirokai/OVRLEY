/**
 * WidgetPreview — renders the appropriate preview component based on widget type.
 *
 * Dispatches to OverlayTextWidget, OverlayRouteWidget, OverlayElevationWidget,
 * or OverlayMetricWidget depending on `widget.type`.
 *
 * Memoized with a custom comparator that checks all individual props to avoid
 * unnecessary re-renders during playback scrubbing.
 *
 * @param {object} props
 * @param {object} props.widget - Widget configuration object.
 * @param {object} [props.activity] - Activity data.
 * @param {number} [props.previewSecond] - Current preview time in seconds.
 * @param {number} [props.globalOpacity] - Global opacity multiplier.
 * @param {number} [props.globalScale] - Global scale multiplier.
 * @param {object} [props.metricPreviewModel] - Precomputed metric preview model.
 * @param {object} [props.textPreviewModel] - Precomputed text preview model.
 * @param {string} [props.sceneFont] - Scene-level font family.
 * @param {number} [props.sceneFontSize] - Scene-level font size.
 * @param {object} [props.sceneStyle] - Scene style object.
 * @param {string} [props.valueFont] - Value font family override.
 * @param {object} [props.exportRange] - Export range configuration.
 * @returns {JSX.Element|null} Widget preview component.
 */

import { memo } from 'react'
import { OverlayTextWidget } from './TextRenderer'
import { OverlayMetricWidget } from './MetricRenderer'
import { OverlayRouteWidget } from './RouteRenderer'
import { OverlayElevationWidget } from './ElevationRenderer'
import { OverlayHeadingWidget } from './HeadingRenderer'
import { isHeadingTapeWidget } from '@/lib/widget-behavior'

function WidgetPreview({
  widget,
  activity,
  previewSecond,
  globalOpacity,
  globalScale,
  metricPreviewModel,
  textPreviewModel,
  sceneFont,
  sceneFontSize,
  sceneStyle,
  valueFont,
  exportRange,
}) {
  if (widget.type === 'label') {
    return <OverlayTextWidget widget={widget} globalOpacity={globalOpacity} sceneStyle={sceneStyle} textPreviewModel={textPreviewModel} />
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

  if (isHeadingTapeWidget(widget)) {
    return (
      <OverlayHeadingWidget
        widget={widget}
        activity={activity}
        previewSecond={previewSecond}
        globalOpacity={globalOpacity}
        sceneFont={sceneFont}
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
    previousProps.textPreviewModel === nextProps.textPreviewModel &&
    previousProps.sceneFont === nextProps.sceneFont &&
    previousProps.sceneFontSize === nextProps.sceneFontSize &&
    previousProps.sceneStyle === nextProps.sceneStyle &&
    previousProps.valueFont === nextProps.valueFont &&
    previousProps.exportRange === nextProps.exportRange,
)
