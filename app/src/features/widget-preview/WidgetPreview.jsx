/**
 * WidgetPreview — renders the appropriate preview component based on widget type
 * and display_type.
 *
 * Non-metric widgets (label, course, elevation) dispatch by widget.type.
 * Metric widgets dispatch by display_type: intrinsic text uses the standard
 * metric preview, boxed presentations use their presentation-specific preview.
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

import { createElement, memo } from 'react'
import { OverlayTextWidget } from './widgets/text/TextPreview'
import { OverlayMetricWidget } from './widgets/metric/MetricPreview'
import { OverlayLapTimerWidget } from './widgets/lap-timer/LapTimerPreview'
import { OverlayRouteWidget } from './widgets/route/RoutePreview'
import { OverlayElevationWidget } from './widgets/elevation/ElevationPreview'
import { OverlayHeadingWidget } from './widgets/heading/HeadingPreview'
import { OverlayLinearGaugeWidget } from './widgets/linear-gauge/LinearGaugePreview'
import { OverlayArcGaugeWidget } from './widgets/arc-gauge/ArcGaugePreview'
import { OverlayLeanAngleWidget } from './widgets/lean-angle/LeanAnglePreview'
import { OverlayGForceWidget } from './widgets/g-force/GForcePreview'
import OverlayBackdropWidget from './widgets/backdrop/BackdropPreview'
import { isBoxedDisplayType } from '@/lib/widget/standard-metrics'

const BOXED_PREVIEW_COMPONENTS = {
  heading_tape: OverlayHeadingWidget,
  linear: OverlayLinearGaugeWidget,
  arc: OverlayArcGaugeWidget,
  corner: OverlayArcGaugeWidget,
  g_force: OverlayGForceWidget,
}

function getBoxedPreviewComponent(displayType) {
  const component = BOXED_PREVIEW_COMPONENTS[displayType]
  if (!component) throw new Error(`Missing widget preview renderer for boxed display type: ${displayType}`)
  return component
}

/**
 * Dispatches a widget to the preview renderer for its type and presentation.
 * @param {object} props - Widget, activity, presentation models, and scene preview state.
 * @returns {JSX.Element} The selected widget preview.
 */
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
  if (widget.type === 'backdrop') {
    return <OverlayBackdropWidget widget={widget} globalOpacity={globalOpacity} globalScale={globalScale} />
  }

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

  if (widget.data.display_type === 'lean_angle') {
    return (
      <OverlayLeanAngleWidget
        widget={widget}
        activity={activity}
        previewSecond={previewSecond}
        globalOpacity={globalOpacity}
        globalScale={globalScale}
        sceneStyle={sceneStyle}
      />
    )
  }

  if (widget.data.display_type === 'lap_timer') {
    return (
      <OverlayLapTimerWidget
        widget={widget}
        activity={activity}
        previewSecond={previewSecond}
        globalOpacity={globalOpacity}
        metricPreviewModel={metricPreviewModel}
        sceneStyle={sceneStyle}
      />
    )
  }

  // Metric widgets: dispatch by display_type.
  if (isBoxedDisplayType(widget.data.display_type)) {
    const BoxedPreview = getBoxedPreviewComponent(widget.data.display_type)
    return createElement(BoxedPreview, {
      widget,
      activity,
      previewSecond,
      globalOpacity,
      globalScale,
      sceneFont,
      sceneStyle,
      valueFont,
    })
    // Boxed type with no renderer — show explicit fallback instead of silent null.
  }

  // Intrinsic text presentation.
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
