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

import { memo } from 'react'
import { OverlayTextWidget } from './TextRenderer'
import { OverlayMetricWidget } from './MetricRenderer'
import { OverlayRouteWidget } from './RouteRenderer'
import { OverlayElevationWidget } from './ElevationRenderer'
import { OverlayHeadingWidget } from './HeadingRenderer'
import { isBoxedDisplayType, getDisplayTypeLabel, getDefaultFrameDimensions } from '@/lib/standard-metrics'
import { resolveActiveMetricWidgetData } from '@/lib/metric-widget-resolver'

/**
 * Registry mapping boxed display_type values to their preview components.
 * New boxed presentations add their renderer here — the dispatch logic
 * is driven by the manifest's set of boxed display types, not by this map.
 */
const BOXED_PREVIEW_COMPONENTS = {
  heading_tape: OverlayHeadingWidget,
}

/**
 * Fallback for boxed display types that have a manifest definition but no
 * renderer registered yet. Renders a visible placeholder so the issue is
 * obvious rather than silently producing a null render. Frame dimensions
 * are sourced from the shared display-type manifest defaults.
 */
function UnsupportedBoxedPreview({ widget, displayType }) {
  const label = getDisplayTypeLabel(displayType)
  const defaults = getDefaultFrameDimensions(displayType)
  return (
    <div
      data-widget-id={widget.id}
      style={{ width: widget.data.width ?? defaults?.width ?? 200, height: widget.data.height ?? defaults?.height ?? 60, opacity: 0.7 }}
      className="flex items-center justify-center rounded border border-dashed border-yellow-500/50 bg-yellow-500/10 text-[10px] text-yellow-600"
    >
      {label} (preview not implemented)
    </div>
  )
}

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

  // Metric widgets: dispatch by display_type.
  const displayType = widget?.data?.display_type
  if (isBoxedDisplayType(displayType)) {
    const resolvedData = resolveActiveMetricWidgetData(widget.data)
    const resolvedWidget = { ...widget, data: resolvedData }
    const BoxedPreview = BOXED_PREVIEW_COMPONENTS[displayType]
    if (BoxedPreview) {
      return (
        <BoxedPreview
          widget={resolvedWidget}
          activity={activity}
          previewSecond={previewSecond}
          globalOpacity={globalOpacity}
          globalScale={globalScale}
          sceneFont={sceneFont}
          sceneStyle={sceneStyle}
          valueFont={valueFont}
        />
      )
    }
    // Boxed type with no renderer — show explicit fallback instead of silent null.
    return <UnsupportedBoxedPreview widget={resolvedWidget} displayType={displayType} />
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
