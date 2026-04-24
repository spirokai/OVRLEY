import { memo } from 'react'
import { LayoutGrid } from 'lucide-react'
import OverlayCanvas from './overlay-editor/OverlayCanvas'
import OverlayMoveable from './overlay-editor/OverlayMoveable'
import useOverlayEditorState from './overlay-editor/useOverlayEditorState'

function EmptyOverlayState() {
  return (
    <div className="flex h-full items-center justify-center p-8">
      <div className="max-w-sm rounded-xl border border-dashed border-border/70 bg-card/60 px-8 py-10 text-center shadow-[0_30px_80px_rgba(0,0,0,0.25)] backdrop-blur-sm">
        <div className="mx-auto mb-4 flex h-14 w-14 items-center justify-center bg-surface-elevated text-primary">
          <LayoutGrid className="h-6 w-6" />
        </div>
        <p className="text-sm font-semibold text-foreground">
          Overlay canvas ready
        </p>
        <p className="mt-2 text-sm text-muted-foreground">
          Load a template or add widgets to start positioning the overlay.
        </p>
      </div>
    </div>
  )
}

function OverlayEditor({
  config,
  globalDefaults,
  onConfigChange,
  zoomLevel,
  onZoomLevelChange,
  backgroundMode,
}) {
  const {
    activity,
    canResizeSelected,
    canRotateSelected,
    canScaleSelected,
    displayScale,
    elementGuidelines,
    globalOpacity,
    globalScale,
    handleSceneMouseDown,
    handleWidgetMouseDown,
    handleWheel,
    handlers,
    maintainAspectRatio,
    moveableRef,
    resolvedConfig,
    previewSecond,
    sceneElement,
    sceneSize,
    selectedTarget,
    selectedTargets,
    selectionRect,
    setSceneElement,
    viewportRef,
    widgetRefCallbacks,
    widgets,
  } = useOverlayEditorState({
    config,
    globalDefaults,
    onConfigChange,
    zoomLevel,
    onZoomLevelChange,
  })

  if (!resolvedConfig) {
    return <EmptyOverlayState />
  }

  return (
    <div
      ref={viewportRef}
      className="relative flex h-full flex-1 overflow-hidden"
      onWheel={handleWheel}
    >
      <div className="flex h-full w-full items-center justify-center overflow-hidden p-8">
        <div
          className="relative shrink-0"
          style={{
            width: sceneSize.width * displayScale,
            height: sceneSize.height * displayScale,
          }}
        >
          <div
            className="absolute left-0 top-0"
            style={{
              width: sceneSize.width,
              height: sceneSize.height,
              transform: `scale(${displayScale})`,
              transformOrigin: 'top left',
            }}
          >
            <OverlayCanvas
              widgets={widgets}
              globalScale={globalScale}
              globalOpacity={globalOpacity}
              activity={activity}
              previewSecond={previewSecond}
              backgroundMode={backgroundMode}
              sceneSize={sceneSize}
              setSceneElement={setSceneElement}
              selectionRect={selectionRect}
              handleSceneMouseDown={handleSceneMouseDown}
              handleWidgetMouseDown={handleWidgetMouseDown}
              widgetRefCallbacks={widgetRefCallbacks}
            />

            <OverlayMoveable
              moveableRef={moveableRef}
              selectedTarget={selectedTarget}
              selectedTargets={selectedTargets}
              sceneElement={sceneElement}
              displayScale={displayScale}
              canResizeSelected={canResizeSelected}
              canScaleSelected={canScaleSelected}
              canRotateSelected={canRotateSelected}
              maintainAspectRatio={maintainAspectRatio}
              elementGuidelines={elementGuidelines}
              sceneSize={sceneSize}
              handlers={handlers}
            />
          </div>
        </div>
      </div>
    </div>
  )
}

export default memo(OverlayEditor)
