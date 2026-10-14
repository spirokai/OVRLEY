import { cn } from '@/lib/utils'
import { VideoPreviewSurface } from '@/features/video-preview'
import { WasmWidgetCanvas } from '@/features/wasm-widgets/components/WasmWidgetCanvas'
import { CANVAS_BACKGROUND_COLORS } from '../data/overlayEditorConstants'
import { useWasmWidgets } from '@/features/wasm-widgets/hooks/useWasmWidgets'

/** @param {object} props Resolved preview state, background, grid, and pointer action. @returns {JSX.Element} Overlay scene and WASM canvases. */
export default function OverlayCanvas({ state, backgroundMode, displayScale, gridVisible, handleWidgetMouseDown }) {
  const {
    widgets,
    scene,
    activity,
    previewSecond,
    globalDefaults,
    setSceneElement,
    sceneSize,
    backgroundImageSrc,
    gridRef,
    gridStyle,
    widgetRefCallbacks,
    handleCanvasMouseOver,
    handleCanvasMouseLeave,
  } = state
  const { attach, error } = useWasmWidgets({
    widgets,
    scene,
    activity,
    second: previewSecond,
    defaultFont: globalDefaults.font_values,
  })
  return (
    <div ref={setSceneElement} data-testid="overlay-scene" className="relative overflow-visible" style={sceneSize}>
      {backgroundMode !== 'transparent' ? (
        <div
          className={cn(
            'pointer-events-none absolute inset-0 rounded-sm shadow-[0_5px_20px_3px_rgba(0,0,0,0.2)] border border-border/50',
            backgroundMode === 'checker' && !gridVisible && 'bg-overlay-grid-muted',
          )}
          style={{ backgroundColor: CANVAS_BACKGROUND_COLORS[backgroundMode] }}
        />
      ) : null}
      {backgroundMode === 'video' ? <VideoPreviewSurface displayScale={displayScale} isActive /> : null}
      {backgroundMode === 'image' && backgroundImageSrc !== null ? (
        <img src={backgroundImageSrc} alt="" className="pointer-events-none absolute inset-0 h-full w-full object-cover" draggable="false" />
      ) : null}
      {gridVisible ? <canvas ref={gridRef} className="pointer-events-none absolute inset-0" aria-hidden="true" style={gridStyle} /> : null}
      <div
        data-testid="widget-layer"
        className="absolute inset-0 overflow-visible"
        onMouseOver={handleCanvasMouseOver}
        onMouseLeave={handleCanvasMouseLeave}
      >
        {error ? (
          <div role="alert" className="absolute z-50 bg-destructive p-2 text-sm text-white">
            {error.message}
          </div>
        ) : null}
        {widgets.map((widget) => (
          <WasmWidgetCanvas
            key={widget.id}
            widget={widget}
            attach={attach}
            registerNode={widgetRefCallbacks[widget.id]}
            handleWidgetMouseDown={handleWidgetMouseDown}
          />
        ))}
      </div>
    </div>
  )
}
