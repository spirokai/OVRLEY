/**
 * Provides overlay editor helpers for overlay canvas.
 */

import { memo, useEffect, useRef } from 'react'
import { AlertTriangle } from 'lucide-react'
import { cn } from '@/lib/utils'
import { getEditorGridSize } from './constants'
import { getWidgetSceneOrigin } from './overlayEditorHelpers'
import { buildMetricWidgetPreviewModel } from './metricWidgetPreviewModel'
import WidgetPreview from './WidgetPreview'
import { buildWidgetTransform } from './utils'
import { useVideoPreview } from '@/features/video-preview'

const CANVAS_BACKGROUND_COLORS = {
  black: '#000000',
  checker: '#000000',
  white: '#f4ead2',
}

const CanvasGrid = memo(function CanvasGrid({ displayScale, sceneSize }) {
  const canvasRef = useRef(null)
  const sceneGridSize = getEditorGridSize(sceneSize)
  const displayWidth = sceneSize.width * displayScale
  const displayHeight = sceneSize.height * displayScale

  useEffect(() => {
    const canvas = canvasRef.current
    const context = canvas?.getContext('2d')

    if (!canvas || !context || displayWidth <= 0 || displayHeight <= 0) {
      return
    }

    const pixelRatio = window.devicePixelRatio || 1
    const bitmapWidth = Math.max(1, Math.round(displayWidth * pixelRatio))
    const bitmapHeight = Math.max(1, Math.round(displayHeight * pixelRatio))

    canvas.width = bitmapWidth
    canvas.height = bitmapHeight
    canvas.style.width = `${displayWidth}px`
    canvas.style.height = `${displayHeight}px`

    context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0)
    context.clearRect(0, 0, displayWidth, displayHeight)
    context.strokeStyle = '#003836'
    context.lineWidth = 1
    context.beginPath()

    for (let x = 0; x <= sceneSize.width; x += sceneGridSize) {
      const displayX = Math.min(Math.round(x * displayScale) + 0.5, Math.max(0.5, Math.round(displayWidth) - 0.5))
      context.moveTo(displayX, 0)
      context.lineTo(displayX, displayHeight)
    }

    for (let y = 0; y <= sceneSize.height; y += sceneGridSize) {
      const displayY = Math.min(Math.round(y * displayScale) + 0.5, Math.max(0.5, Math.round(displayHeight) - 0.5))
      context.moveTo(0, displayY)
      context.lineTo(displayWidth, displayY)
    }

    context.stroke()
  }, [displayHeight, displayScale, displayWidth, sceneGridSize, sceneSize])

  return (
    <canvas
      ref={canvasRef}
      className="pointer-events-none absolute inset-0"
      aria-hidden="true"
      style={{
        transform: `scale(${1 / displayScale})`,
        transformOrigin: 'top left',
      }}
    />
  )
})

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
    sceneStyle,
    valueFont,
    registerNode,
    handleWidgetMouseDown,
    setHoveredWidgetId,
  }) {
    const metricPreviewModel = buildMetricWidgetPreviewModel({
      widget,
      activity,
      previewSecond,
    })
    const metricVisualBounds = metricPreviewModel?.visualBounds ?? null
    const isPlotWidget = widget.category === 'plots'
    const origin = getWidgetSceneOrigin(widget, null, metricVisualBounds, {
      boundsScale: isPlotWidget ? 1 : globalScale,
    })
    const scale = isPlotWidget ? 1 : globalScale
    const rotation = widget.type === 'course' ? (widget.data.rotation ?? 0) : 0
    const width = isPlotWidget ? (widget.data.width ?? 0) * (globalScale || 1) : (metricVisualBounds?.width ?? widget.data.width)
    const height = isPlotWidget ? (widget.data.height ?? 0) * (globalScale || 1) : (metricVisualBounds?.height ?? widget.data.height)
    return (
      <div
        ref={registerNode}
        data-widget-id={widget.id}
        data-widget-bounds-left={metricVisualBounds?.minX ?? 0}
        data-widget-bounds-top={metricVisualBounds?.minY ?? 0}
        data-widget-bounds-right={metricVisualBounds?.maxX ?? 0}
        data-widget-bounds-bottom={metricVisualBounds?.maxY ?? 0}
        className="group absolute cursor-move select-none rounded-xl outline-1 outline-transparent transition-shadow hover:z-50"
        style={{
          left: origin.x,
          top: origin.y,
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
          setHoveredWidgetId((current) => (current === widget.id ? null : current))
        }}
      >
        <WidgetPreview
          widget={widget}
          activity={activity}
          previewSecond={previewSecond}
          globalOpacity={globalOpacity}
          globalScale={globalScale}
          metricPreviewModel={metricPreviewModel}
          sceneFont={sceneFont}
          sceneFontSize={sceneFontSize}
          sceneStyle={sceneStyle}
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
    previousProps.sceneStyle === nextProps.sceneStyle &&
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
 * @param {*} props.displayScale - Scale factor applied to the scene display.
 * @param {*} props.globalOpacity - Global opacity multiplier applied to the widget.
 * @param {*} props.activity - Parsed activity data for previews or rendering.
 * @param {*} props.previewSecond - Preview time in seconds.
 * @param {*} props.backgroundMode - Selected canvas background style.
 * @param {*} props.gridVisible - Whether to show the editor grid overlay.
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
  displayScale,
  globalScale,
  globalOpacity,
  activity,
  previewSecond,
  backgroundMode,
  gridVisible,
  sceneFont,
  sceneFontSize,
  sceneStyle,
  valueFont,
  sceneSize,
  setSceneElement,
  selectionRect,
  handleSceneMouseDown,
  handleWidgetMouseDown,
  setHoveredWidgetId,
  widgetRefCallbacks,
}) {
  const videoRef = useRef(null)
  const { videoSrc, importId, isOutOfRange, videoPreviewMessages } = useVideoPreview(videoRef, backgroundMode === 'video')

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
          backgroundMode === 'checker' && !gridVisible && 'bg-overlay-grid-muted',
        )}
        style={{
          backgroundColor: CANVAS_BACKGROUND_COLORS[backgroundMode] || CANVAS_BACKGROUND_COLORS.black,
        }}
      />
      {backgroundMode === 'video' && videoSrc && (
        <video
          key={importId ?? 'no-video'}
          ref={videoRef}
          src={videoSrc}
          className={cn('pointer-events-none absolute inset-0 h-full w-full object-cover', isOutOfRange ? 'opacity-20' : 'opacity-100')}
          preload="metadata"
          muted
          playsInline
          onError={(e) => console.error('[OverlayCanvas] Video Error:', e)}
        />
      )}
      {backgroundMode === 'video' && videoPreviewMessages.length > 0 ? (
        <div
          className="pointer-events-none absolute left-3 right-3 top-3 z-20 flex max-w-xl items-start gap-2 rounded-md border border-amber-400/40 bg-black/75 px-3 py-2 text-xs leading-snug text-amber-100 shadow-lg"
          aria-live="polite"
        >
          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <div className="space-y-1">
            {videoPreviewMessages.slice(0, 2).map((message) => (
              <p key={message}>{message}</p>
            ))}
          </div>
        </div>
      ) : null}
      {gridVisible ? <CanvasGrid displayScale={displayScale} sceneSize={sceneSize} /> : null}
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
              sceneStyle={sceneStyle}
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
