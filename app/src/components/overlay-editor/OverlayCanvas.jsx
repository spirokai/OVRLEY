/**
 * Provides overlay editor helpers for overlay canvas.
 */

import { memo } from 'react'
import { cn } from '@/lib/utils'
import WidgetPreview from './WidgetPreview'
import { buildWidgetTransform } from './utils'

const CANVAS_BACKGROUND_COLORS = {
  black: '#000000',
  checker: '#000000',
  cream: '#f4ead2',
}

/**
 * Renders the overlay canvas widget component.
 *
 * @param {object} props - Component props.
 * @param {*} props.activity - Parsed activity data for previews or rendering.
 * @param {*} props.globalOpacity - Global opacity multiplier applied to the widget.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.globalScale - Scale factor applied to the overlay preview.
 * @param {*} props.previewSecond - Preview time in seconds.
 * @param {*} props.registerNode - Value for register node.
 * @param {*} props.handleWidgetMouseDown - Value for handle widget mouse down.
 * @returns {JSX.Element} Rendered component output.
 */
const OverlayCanvasWidget = memo(
  function OverlayCanvasWidget({
    activity,
    globalOpacity,
    widget,
    globalScale,
    previewSecond,
    sceneFont,
    sceneFontSize,
    valueFont,
    registerNode,
    handleWidgetMouseDown,
    setHoveredWidgetId,
  }) {
    const x = widget.data.x ?? 0
    const valueOffset =
      widget.category === 'values' && widget.type !== 'gradient'
        ? (widget.data.value_offset ?? 0)
        : 0
    const gradientYOffset =
      widget.type === 'gradient'
        ? Math.min(0, -(widget.data.value_offset ?? 0))
        : 0
    const y = (widget.data.y ?? 0) + valueOffset + gradientYOffset
    const isPlotWidget = widget.category === 'plots'
    const scale = isPlotWidget ? 1 : globalScale
    const rotation = widget.type === 'course' ? (widget.data.rotation ?? 0) : 0
    const width = isPlotWidget
      ? (widget.data.width ?? 0) * (globalScale || 1)
      : widget.data.width
    const height = isPlotWidget
      ? (widget.data.height ?? 0) * (globalScale || 1)
      : widget.data.height
    return (
      <div
        ref={registerNode}
        data-widget-id={widget.id}
        className="group absolute cursor-move select-none rounded-xl outline-1 outline-transparent transition-shadow hover:z-50"
        style={{
          left: x,
          top: y,
          width,
          height,
          transform: buildWidgetTransform({ scale, rotation }),
          transformOrigin: 'top left',
        }}
        onMouseDown={(event) => {
          handleWidgetMouseDown(event, widget.id)
        }}
        onMouseEnter={() => {
          setHoveredWidgetId(widget.id)
        }}
        onMouseLeave={() => {
          setHoveredWidgetId((current) =>
            current === widget.id ? null : current,
          )
        }}
      >
        <WidgetPreview
          widget={widget}
          activity={activity}
          previewSecond={previewSecond}
          globalOpacity={globalOpacity}
          globalScale={globalScale}
          sceneFont={sceneFont}
          sceneFontSize={sceneFontSize}
          valueFont={valueFont}
        />
      </div>
    )
  },
  (previousProps, nextProps) =>
    previousProps.widget === nextProps.widget &&
    previousProps.globalScale === nextProps.globalScale &&
    previousProps.globalOpacity === nextProps.globalOpacity &&
    previousProps.activity === nextProps.activity &&
    previousProps.previewSecond === nextProps.previewSecond &&
    previousProps.sceneFont === nextProps.sceneFont &&
    previousProps.sceneFontSize === nextProps.sceneFontSize &&
    previousProps.valueFont === nextProps.valueFont &&
    previousProps.registerNode === nextProps.registerNode &&
    previousProps.handleWidgetMouseDown === nextProps.handleWidgetMouseDown &&
    previousProps.setHoveredWidgetId === nextProps.setHoveredWidgetId,
)

/**
 * Renders the overlay canvas component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widgets - Widget collection in the current template.
 * @param {*} props.globalScale - Scale factor applied to the overlay preview.
 * @param {*} props.globalOpacity - Global opacity multiplier applied to the widget.
 * @param {*} props.activity - Parsed activity data for previews or rendering.
 * @param {*} props.previewSecond - Preview time in seconds.
 * @param {*} props.backgroundMode - Selected canvas background style.
 * @param {*} props.sceneSize - Numeric scene size value.
 * @param {*} props.setSceneElement - Value for set scene element.
 * @param {*} props.selectionRect - Current drag-selection rectangle.
 * @param {*} props.handleSceneMouseDown - Value for handle scene mouse down.
 * @param {*} props.handleWidgetMouseDown - Value for handle widget mouse down.
 * @param {*} props.widgetRefCallbacks - Value for widget ref callbacks.
 * @returns {JSX.Element} Rendered component output.
 */
export default function OverlayCanvas({
  widgets,
  globalScale,
  globalOpacity,
  activity,
  previewSecond,
  backgroundMode,
  sceneFont,
  sceneFontSize,
  valueFont,
  sceneSize,
  setSceneElement,
  selectionRect,
  handleSceneMouseDown,
  handleWidgetMouseDown,
  setHoveredWidgetId,
  widgetRefCallbacks,
}) {
  return (
    <div
      ref={setSceneElement}
      className="relative overflow-visible"
      onMouseDown={handleSceneMouseDown}
      style={{
        width: sceneSize.width,
        height: sceneSize.height,
      }}
    >
      <div
        className={cn(
          'pointer-events-none absolute inset-0 rounded-sm shadow-[0_5px_20px_3px_rgba(0,0,0,0.2)] border border-border/50',
          backgroundMode === 'checker' && 'bg-overlay-grid-muted',
        )}
        style={{
          backgroundColor:
            CANVAS_BACKGROUND_COLORS[backgroundMode] ||
            CANVAS_BACKGROUND_COLORS.black,
        }}
      />
      <div className="absolute inset-0 overflow-visible">
        {widgets.map((widget) => {
          return (
            <OverlayCanvasWidget
              key={widget.id}
              widget={widget}
              globalScale={globalScale}
              globalOpacity={globalOpacity}
              activity={activity}
              previewSecond={previewSecond}
              sceneFont={sceneFont}
              sceneFontSize={sceneFontSize}
              valueFont={valueFont}
              registerNode={widgetRefCallbacks[widget.id]}
              handleWidgetMouseDown={handleWidgetMouseDown}
              setHoveredWidgetId={setHoveredWidgetId}
            />
          )
        })}
      </div>
      {selectionRect ? (
        <div
          className="pointer-events-none absolute border border-primary/70 bg-primary/10"
          style={{
            left: selectionRect.x,
            top: selectionRect.y,
            width: selectionRect.width,
            height: selectionRect.height,
          }}
        />
      ) : null}
    </div>
  )
}
