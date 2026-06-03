/**
 * Renders the overlay canvas component with grouped props.
 */

import { memo, useEffect, useRef } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import { AlertTriangle } from 'lucide-react'
import { cn } from '@/lib/utils'
import { getEditorGridSize } from '../utils/overlayEditorUtils'
import { buildMetricWidgetPreviewModel, buildTextWidgetPreviewModel, WidgetPreview } from '@/features/widget-preview'
import { useFontMetricsVersion } from '@/features/widget-preview/hooks/useFontMetricsVersion'
import { getPreviewFontFamily } from '@/features/widget-preview/utils/textMeasurement'
import { CANVAS_BACKGROUND_COLORS } from '../data/overlayEditorConstants'
import { useVideoPreview } from '@/features/video-preview'
import useStore from '@/store/useStore'
import { resolveWidgetRenderGeometry } from '../utils/widgetRenderGeometry'

/**
 * Canvas overlay grid — draws a teal-colored grid on an HTML canvas element
 * positioned above the scene background. Uses device pixel ratio for crisp rendering.
 * Re-draws whenever scene size or display scale changes.
 */
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

const OverlayCanvasWidget = memo(
  function OverlayCanvasWidget({
    widget,
    preview,
    globalScale,
    globalOpacity,
    activity,
    previewSecond,
    sceneFont,
    sceneFontSize,
    sceneStyle,
    valueFont,
    exportRange,
    registerNode,
    handleWidgetMouseDown,
    setHoveredWidgetId,
  }) {
    const widgetFontSize = widget.data.font_size ?? 60
    const widgetFontFamily = getPreviewFontFamily(widget.data.font || widget.data.font_family)
    useFontMetricsVersion(widgetFontFamily, widgetFontSize)

    const metricPreviewModel = buildMetricWidgetPreviewModel({
      widget,
      activity,
      previewSecond,
    })
    const metricVisualBounds = metricPreviewModel?.visualBounds ?? null
    const textPreviewModel = buildTextWidgetPreviewModel({ widget })
    const visualBounds = metricVisualBounds ?? textPreviewModel?.visualBounds ?? null
    const renderGeometry = resolveWidgetRenderGeometry(widget, visualBounds, globalScale, preview)

    return (
      <div
        ref={registerNode}
        data-widget-id={widget.id}
        data-widget-bounds-left={visualBounds?.minX ?? 0}
        data-widget-bounds-top={visualBounds?.minY ?? 0}
        data-widget-bounds-right={visualBounds?.maxX ?? 0}
        data-widget-bounds-bottom={visualBounds?.maxY ?? 0}
        className="group absolute cursor-move select-none rounded-xl outline-1 outline-transparent transition-shadow hover:z-50"
        style={{
          left: renderGeometry.left,
          top: renderGeometry.top,
          width: renderGeometry.width,
          height: renderGeometry.height,
          transform: renderGeometry.transform,
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
          textPreviewModel={textPreviewModel}
          sceneFont={sceneFont}
          sceneFontSize={sceneFontSize}
          sceneStyle={sceneStyle}
          valueFont={valueFont}
          exportRange={exportRange}
        />
      </div>
    )
  },
  (previousProps, nextProps) =>
    previousProps.widget === nextProps.widget &&
    previousProps.preview === nextProps.preview &&
    previousProps.globalScale === nextProps.globalScale &&
    previousProps.globalOpacity === nextProps.globalOpacity &&
    previousProps.activity === nextProps.activity &&
    previousProps.previewSecond === nextProps.previewSecond &&
    previousProps.sceneFont === nextProps.sceneFont &&
    previousProps.sceneFontSize === nextProps.sceneFontSize &&
    previousProps.sceneStyle === nextProps.sceneStyle &&
    previousProps.valueFont === nextProps.valueFont &&
    previousProps.exportRange === nextProps.exportRange &&
    previousProps.registerNode === nextProps.registerNode &&
    previousProps.handleWidgetMouseDown === nextProps.handleWidgetMouseDown &&
    previousProps.setHoveredWidgetId === nextProps.setHoveredWidgetId,
)

/**
 * Renders the scene background (color or video), widget previews, and selection rectangle.
 * Delegates scene-level state to grouped props to minimize re-renders.
 *
 * @param {object} props
 * @param {object} props.sceneProps - { sceneFont, sceneFontSize, sceneStyle, valueFont, sceneSize }
 * @param {object} props.displayProps - { displayScale, globalScale, globalOpacity, backgroundMode, gridVisible }
 * @param {object} props.dataProps - { widgets, activity, previewSecond, exportRange }
 * @param {object} props.callbacks - { setSceneElement, handleWidgetMouseDown, setHoveredWidgetId, widgetRefCallbacks }
 * @returns {JSX.Element} Rendered component output.
 */
export default function OverlayCanvas({ sceneProps, displayProps, dataProps, callbacks }) {
  const { sceneFont, sceneFontSize, sceneStyle, valueFont, sceneSize } = sceneProps
  const { displayScale, globalScale, globalOpacity, backgroundMode, gridVisible } = displayProps
  const { widgets, activity, previewSecond, exportRange } = dataProps
  const { setSceneElement, handleWidgetMouseDown, setHoveredWidgetId, widgetRefCallbacks } = callbacks
  const videoRef = useRef(null)
  const importedBackgroundImagePath = useStore((state) => state.importedBackgroundImagePath)
  const { videoSrc, importId, isOutOfRange, videoPreviewMessages } = useVideoPreview(videoRef, backgroundMode === 'video')
  const hasTransparentBackground = backgroundMode === 'transparent'
  const backgroundImageSrc = importedBackgroundImagePath ? convertFileSrc(importedBackgroundImagePath) : ''

  return (
    <div
      ref={setSceneElement}
      data-testid="overlay-scene"
      className="relative overflow-visible"
      style={{
        width: sceneSize.width,
        height: sceneSize.height,
      }}
    >
      {!hasTransparentBackground ? (
        <div
          className={cn(
            'pointer-events-none absolute inset-0 rounded-sm shadow-[0_5px_20px_3px_rgba(0,0,0,0.2)] border border-border/50',
            backgroundMode === 'checker' && !gridVisible && 'bg-overlay-grid-muted',
          )}
          style={{
            backgroundColor: CANVAS_BACKGROUND_COLORS[backgroundMode] || CANVAS_BACKGROUND_COLORS.black,
          }}
        />
      ) : null}
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
      {backgroundMode === 'image' && backgroundImageSrc ? (
        <img src={backgroundImageSrc} alt="" className="pointer-events-none absolute inset-0 h-full w-full object-cover" draggable="false" />
      ) : null}
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
      <div data-testid="widget-layer" className="absolute inset-0 overflow-visible">
        {widgets.map((widget) => {
          return (
            <OverlayCanvasWidget
              key={widget.id}
              widget={widget}
              preview={dataProps.widgetPreviews?.[widget.id] ?? null}
              globalScale={globalScale}
              globalOpacity={globalOpacity}
              activity={activity}
              previewSecond={previewSecond}
              sceneFont={sceneFont}
              sceneFontSize={sceneFontSize}
              sceneStyle={sceneStyle}
              valueFont={valueFont}
              exportRange={exportRange}
              registerNode={widgetRefCallbacks[widget.id]}
              handleWidgetMouseDown={handleWidgetMouseDown}
              setHoveredWidgetId={setHoveredWidgetId}
            />
          )
        })}
      </div>
    </div>
  )
}
