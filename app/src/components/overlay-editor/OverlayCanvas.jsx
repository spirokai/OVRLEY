/**
 * Provides overlay editor helpers for overlay canvas.
 */

import { memo, useCallback, useLayoutEffect, useRef, useState } from 'react'
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
 * @param {*} props.displayScale - Value for display scale.
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
    displayScale,
    previewSecond,
    registerNode,
    handleWidgetMouseDown,
  }) {
    const innerNodeRef = useRef(null)
    const x = widget.data.x ?? 0
    const y = widget.data.y ?? 0
    const rotation = widget.type === 'course' ? (widget.data.rotation ?? 0) : 0
    const width = widget.data.width
    const height = widget.data.height
    const Icon = WIDGET_ICONS[widget.type] || Type
    const hasExplicitWidth = typeof width === 'number'
    const hasExplicitHeight = typeof height === 'number'
    const [visualBounds, setVisualBounds] = useState(() => ({
      width: hasExplicitWidth ? width * globalScale : undefined,
      height: hasExplicitHeight ? height * globalScale : undefined,
    }))

    useLayoutEffect(() => {
      const innerNode = innerNodeRef.current
      if (!innerNode) {
        return undefined
      }

      const syncBounds = () => {
        const nextRect = innerNode.getBoundingClientRect()
        const safeDisplayScale =
          Number.isFinite(displayScale) && displayScale > 0 ? displayScale : 1
        const nextWidth = Math.max(nextRect.width / safeDisplayScale, 0)
        const nextHeight = Math.max(nextRect.height / safeDisplayScale, 0)

        setVisualBounds((current) => {
          if (current.width === nextWidth && current.height === nextHeight) {
            return current
          }
          return {
            width: nextWidth,
            height: nextHeight,
          }
        })
      }

      syncBounds()

      if (typeof ResizeObserver === 'undefined') {
        return undefined
      }

      const resizeObserver = new ResizeObserver(() => {
        syncBounds()
      })

      resizeObserver.observe(innerNode)
      return () => resizeObserver.disconnect()
    }, [
      globalScale,
      displayScale,
      hasExplicitHeight,
      hasExplicitWidth,
      height,
      width,
      widget,
    ])

    const handleOuterRef = useCallback(
      (node) => {
        registerNode(node)
      },
      [registerNode],
    )

    return (
      <div
        ref={handleOuterRef}
        data-widget-id={widget.id}
        className="group absolute cursor-move select-none rounded-xl border border-transparent transition-shadow"
        style={{
          left: x,
          top: y,
          width: visualBounds.width,
          height: visualBounds.height,
          transform: buildWidgetTransform({ rotation }),
          transformOrigin:
            widget.type === 'course' ? 'center center' : 'top left',
          overflow: 'visible',
        }}
        onMouseDown={(event) => {
          handleWidgetMouseDown(event, widget.id)
        }}
      >
        <div className="absolute -top-9 left-0 flex items-center gap-2 rounded-full border border-border/70 bg-card/80 px-3 py-2 text-[20px] font-semibold text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100">
          <Icon className="h-6 w-6" />
          <span>{widget.type}</span>
        </div>
        <div
          ref={innerNodeRef}
          className="relative"
          style={{
            width: hasExplicitWidth ? width : 'max-content',
            height: hasExplicitHeight ? height : 'max-content',
            transform: globalScale !== 1 ? `scale(${globalScale})` : undefined,
            transformOrigin:
              widget.type === 'course' ? 'center center' : 'top left',
            overflow: 'visible',
          }}
        >
          <WidgetPreview
            widget={widget}
            activity={activity}
            previewSecond={previewSecond}
            globalOpacity={globalOpacity}
          />
        </div>
      </div>
    )
  },
  (previousProps, nextProps) =>
    previousProps.widget === nextProps.widget &&
    previousProps.globalScale === nextProps.globalScale &&
    previousProps.displayScale === nextProps.displayScale &&
    previousProps.globalOpacity === nextProps.globalOpacity &&
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
 * @param {*} props.displayScale - Value for display scale.
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
  displayScale,
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
              displayScale={displayScale}
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
