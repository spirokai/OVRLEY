/**
 * Renders the overlay canvas component with grouped props.
 */

import { memo, useEffect, useRef } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import { cn } from '@/lib/utils'
import { getEditorGridSize } from '../utils/overlayEditorUtils'
import { WidgetPreview } from '@/features/widget-preview'
import { CANVAS_BACKGROUND_COLORS } from '../data/overlayEditorConstants'
import { useVideoPreview } from '@/features/video-preview'
import { syncVideoCurrentTime } from '@/features/video-preview/utils/videoPreviewPlayback'
import useStore from '@/store/useStore'
import HevcPlaybackPlaceholder from './HevcPlaybackPlaceholder'

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

function FrozenVideoFrame({ className, importId, second, src }) {
  const videoRef = useRef(null)

  useEffect(() => {
    const video = videoRef.current
    if (!video || !src || second === null) {
      return undefined
    }

    const syncFrozenFrame = () => {
      const duration = Number(video.duration)
      if (!Number.isFinite(duration) || duration <= 0) {
        return
      }

      if (!video.paused) {
        video.pause()
      }

      syncVideoCurrentTime(video, second)
    }

    syncFrozenFrame()
    video.addEventListener('loadedmetadata', syncFrozenFrame)
    video.addEventListener('canplay', syncFrozenFrame)

    return () => {
      video.removeEventListener('loadedmetadata', syncFrozenFrame)
      video.removeEventListener('canplay', syncFrozenFrame)
    }
  }, [second, src])

  return (
    <video
      key={`${importId ?? 'no-video'}-frozen`}
      ref={videoRef}
      src={src}
      className={className}
      preload="auto"
      playsInline
      muted
      aria-hidden="true"
    />
  )
}

const OverlayCanvasWidget = memo(
  function OverlayCanvasWidget({
    widget,
    globalScale,
    globalOpacity,
    activity,
    previewSecond,
    metricPreviewModel,
    textPreviewModel,
    renderGeometryModel,
    sceneFont,
    sceneFontSize,
    sceneStyle,
    valueFont,
    exportRange,
    registerNode,
    handleWidgetMouseDown,
    setHoveredWidgetId,
  }) {
    const visualBounds = renderGeometryModel.visualBounds
    const renderGeometry = renderGeometryModel.renderGeometry

    return (
      <div
        ref={registerNode}
        data-widget-id={widget.id}
        data-widget-bounds-left={visualBounds?.minX ?? 0}
        data-widget-bounds-top={visualBounds?.minY ?? 0}
        data-widget-bounds-right={visualBounds?.maxX ?? 0}
        data-widget-bounds-bottom={visualBounds?.maxY ?? 0}
        data-widget-content-width={metricPreviewModel ? metricPreviewModel.metricLayout.width * globalScale : undefined}
        className={cn(
          'group absolute cursor-move select-none rounded-xl outline-1 outline-transparent transition-shadow',
          widget.category === 'backdrops' && 'z-1',
          widget.category === 'labels' && 'z-20',
          widget.category === 'plots' && 'z-2',
          widget.category === 'values' && 'z-10',
        )}
        style={{
          left: renderGeometry.left,
          top: renderGeometry.top,
          width: renderGeometry.width,
          height: renderGeometry.height,
          transform: renderGeometry.transform,
          transformOrigin: renderGeometry.transformOrigin,
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
    previousProps.globalScale === nextProps.globalScale &&
    previousProps.globalOpacity === nextProps.globalOpacity &&
    previousProps.activity === nextProps.activity &&
    (previousProps.widget.type === 'label' || previousProps.widget.type === 'backdrop' || previousProps.previewSecond === nextProps.previewSecond) &&
    previousProps.metricPreviewModel === nextProps.metricPreviewModel &&
    previousProps.textPreviewModel === nextProps.textPreviewModel &&
    previousProps.renderGeometryModel === nextProps.renderGeometryModel &&
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
 * @param {object} props.dataProps - { widgets, activity, previewSecond, metricPreviewModels, textPreviewModels, renderGeometryModels, exportRange }
 * @param {object} props.callbacks - { setSceneElement, handleWidgetMouseDown, setHoveredWidgetId, widgetRefCallbacks }
 * @returns {JSX.Element} Rendered component output.
 */
export default function OverlayCanvas({ sceneProps, displayProps, dataProps, callbacks }) {
  const { sceneFont, sceneFontSize, sceneStyle, valueFont, sceneSize } = sceneProps
  const { displayScale, globalScale, globalOpacity, backgroundMode, gridVisible } = displayProps
  const { widgets, activity, previewSecond, metricPreviewModels, textPreviewModels, renderGeometryModels, exportRange } = dataProps
  const { setSceneElement, handleWidgetMouseDown, setHoveredWidgetId, widgetRefCallbacks } = callbacks
  const videoRef = useRef(null)
  const isVideoMuted = useStore((state) => state.isVideoMuted)
  const importedBackgroundImagePath = useStore((state) => state.importedBackgroundImagePath)
  const platformOs = useStore((state) => state.platformOs)
  const { videoSrc, importId, frozenFrameSecond, isOutOfRange, hevcPlaybackWarning, openVideoPreviewHelp, videoPreviewHelpAvailable } =
    useVideoPreview(videoRef, backgroundMode === 'video')
  const hasHevcPlaybackError = Boolean(hevcPlaybackWarning)
  const hasTransparentBackground = backgroundMode === 'transparent'
  const backgroundImageSrc = importedBackgroundImagePath ? convertFileSrc(importedBackgroundImagePath) : ''
  const videoBackgroundClassName = cn('pointer-events-none absolute inset-0 h-full w-full object-cover', isOutOfRange ? 'opacity-20' : 'opacity-100')

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
      {backgroundMode === 'video' && videoSrc && !hasHevcPlaybackError ? (
        <video
          key={importId ?? 'no-video'}
          ref={videoRef}
          src={videoSrc}
          className={videoBackgroundClassName}
          preload="metadata"
          playsInline
          muted={isVideoMuted}
          onError={(e) => console.error('[OverlayCanvas] Video Error:', e)}
        />
      ) : null}
      {backgroundMode === 'video' && videoSrc && frozenFrameSecond !== null && !hasHevcPlaybackError ? (
        <FrozenVideoFrame className={videoBackgroundClassName} importId={importId} second={frozenFrameSecond} src={videoSrc} />
      ) : null}
      {backgroundMode === 'image' && backgroundImageSrc ? (
        <img src={backgroundImageSrc} alt="" className="pointer-events-none absolute inset-0 h-full w-full object-cover" draggable="false" />
      ) : null}
      {gridVisible ? <CanvasGrid displayScale={displayScale} sceneSize={sceneSize} /> : null}
      <div data-testid="widget-layer" className="absolute inset-0 overflow-visible">
        {widgets.map((widget) => {
          return (
            <OverlayCanvasWidget
              key={widget.id}
              widget={widget}
              globalScale={globalScale}
              globalOpacity={globalOpacity}
              activity={activity}
              previewSecond={previewSecond}
              metricPreviewModel={metricPreviewModels[widget.id] ?? null}
              textPreviewModel={textPreviewModels[widget.id] ?? null}
              renderGeometryModel={renderGeometryModels[widget.id]}
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
      {backgroundMode === 'video' && videoSrc && hasHevcPlaybackError ? (
        <HevcPlaybackPlaceholder
          displayScale={displayScale}
          importId={importId}
          openVideoPreviewHelp={openVideoPreviewHelp}
          platformOs={platformOs}
          videoPreviewHelpAvailable={videoPreviewHelpAvailable}
        />
      ) : null}
    </div>
  )
}
