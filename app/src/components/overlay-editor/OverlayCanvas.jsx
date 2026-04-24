import { memo } from 'react'
import { Type } from 'lucide-react'
import { cn } from '@/lib/utils'
import { WIDGET_ICONS } from './constants'
import WidgetPreview from './WidgetPreview'
import { buildWidgetTransform } from './utils'

const OverlayCanvasWidget = memo(
  function OverlayCanvasWidget({
    activity,
    globalOpacity,
    widget,
    globalScale,
    previewSecond,
    registerNode,
    handleWidgetMouseDown,
  }) {
    const x = widget.data.x ?? 0
    const y = widget.data.y ?? 0
    const scale = globalScale
    const rotation = widget.type === 'course' ? (widget.data.rotation ?? 0) : 0
    const width = widget.data.width
    const height = widget.data.height
    const Icon = WIDGET_ICONS[widget.type] || Type

    return (
      <div
        ref={registerNode}
        data-widget-id={widget.id}
        className="group absolute cursor-move select-none rounded-xl border border-transparent transition-shadow"
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
        <div className="absolute -top-7 left-0 flex items-center gap-1 rounded-full border border-border/70 bg-card/80 px-2 py-1 text-[10px] font-semibold text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100">
          <Icon className="h-3 w-3" />
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
    previousProps.activity === nextProps.activity &&
    previousProps.previewSecond === nextProps.previewSecond &&
    previousProps.registerNode === nextProps.registerNode &&
    previousProps.handleWidgetMouseDown === nextProps.handleWidgetMouseDown,
)

export default function OverlayCanvas({
  widgets,
  globalScale,
  globalOpacity,
  activity,
  previewSecond,
  backgroundMode,
  sceneSize,
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
          'pointer-events-none absolute inset-0 rounded-md shadow-[0_5px_20px_3px_rgba(0,0,0,0.2)]',
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
