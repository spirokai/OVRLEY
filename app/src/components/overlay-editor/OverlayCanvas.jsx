/**
 * Provides overlay editor helpers for overlay canvas.
 */

import { memo } from 'react'
import { Type } from 'lucide-react'
import { cn } from '@/lib/utils'
import { WIDGET_ICONS } from './constants'
import WidgetPreview from './WidgetPreview'
import { buildWidgetTransform } from './utils'

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
    displayScale,
    globalOpacity,
    widget,
    globalScale,
    previewSecond,
    registerNode,
    handleWidgetMouseDown,
  }) {
    const x = widget.data.x ?? 0
    const valueOffset =
      widget.category === 'values' && widget.type !== 'gradient'
        ? (widget.data.value_offset ?? 0)
        : 0
    const y = (widget.data.y ?? 0) + valueOffset
    const scale = globalScale
    const rotation = widget.type === 'course' ? (widget.data.rotation ?? 0) : 0
    const width = widget.data.width
    const height = widget.data.height
    const Icon = WIDGET_ICONS[widget.type] || Type
    // Counter-scale the badge so it always appears at a fixed screen size,
    // regardless of the globalScale and displayScale transforms on ancestors.
    const badgeScale = 0.8 / ((displayScale || 1) * (globalScale || 1))

    return (
      <div
        ref={registerNode}
        data-widget-id={widget.id}
        className="group absolute cursor-move select-none rounded-xl border border-transparent transition-shadow hover:z-50"
        style={{
          left: x,
          top: y,
          width,
          height,
          transform: buildWidgetTransform({ scale, rotation }),
          transformOrigin:
            widget.type === 'course' ? 'center center' : 'top left',
        }}
        onMouseDown={(event) => {
          handleWidgetMouseDown(event, widget.id)
        }}
      >
        <div
          className="absolute left-0 flex items-center gap-[0.4rem] rounded-full border border-border/70 bg-card/80 px-[0.5rem] py-[0.2rem] font-semibold text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100"
          style={{
            bottom: '100%',
            marginBottom: 4,
            fontSize: '0.85rem',
            transform: `scale(${badgeScale})`,
            transformOrigin: 'bottom left',
          }}
        >
          <Icon className="h-[0.9rem] w-[0.9rem]" />
          <span>{widget.type}</span>
        </div>
        <WidgetPreview
          widget={widget}
          activity={activity}
          previewSecond={previewSecond}
          globalOpacity={globalOpacity}
        />
      </div>
    )
  },
  (previousProps, nextProps) =>
    previousProps.widget === nextProps.widget &&
    previousProps.globalScale === nextProps.globalScale &&
    previousProps.globalOpacity === nextProps.globalOpacity &&
    previousProps.displayScale === nextProps.displayScale &&
    previousProps.activity === nextProps.activity &&
    previousProps.previewSecond === nextProps.previewSecond &&
    previousProps.registerNode === nextProps.registerNode &&
    previousProps.handleWidgetMouseDown === nextProps.handleWidgetMouseDown,
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
  sceneSize,
  displayScale,
  setSceneElement,
  selectionRect,
  handleSceneMouseDown,
  handleWidgetMouseDown,
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
          'pointer-events-none absolute inset-0 rounded-sm shadow-[0_5px_20px_3px_rgba(0,0,0,0.2)] border border-red-500',
          backgroundMode === 'checker' && 'bg-overlay-grid-muted',
        )}
        style={{ backgroundColor: '#000000' }}
      />
      <div className="absolute inset-0 overflow-visible">
        {widgets.map((widget) => {
          return (
            <OverlayCanvasWidget
              key={widget.id}
              widget={widget}
              globalScale={globalScale}
              globalOpacity={globalOpacity}
              displayScale={displayScale}
              activity={activity}
              previewSecond={previewSecond}
              registerNode={widgetRefCallbacks[widget.id]}
              handleWidgetMouseDown={handleWidgetMouseDown}
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
