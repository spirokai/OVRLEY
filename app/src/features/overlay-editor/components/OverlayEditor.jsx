/**
 * Presents the overlay editor canvas, controls, selection, and video-sync screens.
 */

import { memo } from 'react'
import { LayoutGrid } from 'lucide-react'
import useStore from '@/store/useStore'
import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import { WIDGET_ICONS } from '@/lib/widget/widget-icons'
import { getWidgetTypeName } from '@/lib/widget/widget-presentation'
import EditorToolbar from '@/features/app-shell/components/EditorToolbar'
import { PROJECT_STATUS_TRANSLATIONS } from '../data/overlayEditorConstants'
import { VideoSyncMarkControls, VideoSyncPreviewScreens } from '@/features/video-sync'
import useOverlayEditorState from '../hooks/useOverlayEditorState'
import OverlayCanvas from './OverlayCanvas'
import OverlayMoveable from './OverlayMoveable'

function OverlayEditorContent(props) {
  const {
    editorShell,
    backgroundMode,
    gridVisible,
    importedBackgroundImageFilename,
    importedVideoFilename,
    showProjectStatus,
    projectStatus,
    undoRedoControls,
    videoSyncMode,
  } = props
  const { t } = useTranslation()
  const statusTranslation = PROJECT_STATUS_TRANSLATIONS[projectStatus]
  const editor = useOverlayEditorState(props)
  const platformOs = useStore((state) => state.platformOs)
  const {
    viewport,
    viewportContentSize,
    selectionRect,
    videoSyncMarkControls,
    setStageElement,
    handleSceneMouseDown,
    activity,
    previewSecond,
    sceneSize,
    setSceneElement,
    handleWidgetMouseDown,
    moveableProps,
    widgetBadges,
  } = editor
  const { displayScale, viewportRef, scrollViewportRef, handleWheel } = viewport
  return (
    <div ref={viewportRef} className="relative flex h-full flex-1 overflow-hidden">
      <div data-testid="canvas-status-badges" className="pointer-events-none absolute right-8 top-4 z-50 flex items-center gap-2">
        {showProjectStatus ? (
          <Badge
            variant={projectStatus === 'Modified' ? 'secondary' : 'outline'}
            className={`h-6 rounded-xs text-[0.741rem] shadow-lg ${projectStatus === 'Modified' ? 'border-accent-border bg-surface-accent-soft/20 text-primary' : 'bg-card/85'}`}
          >
            {t(statusTranslation.key, statusTranslation.defaultLabel)}
          </Badge>
        ) : null}
        <div className="rounded-xs border border-border/70 bg-card/85 px-3 py-1 text-xs font-medium text-muted-foreground shadow-lg">
          {sceneSize.width} &times; {sceneSize.height}
        </div>
      </div>
      <div data-testid="canvas-editor-toolbar" className="pointer-events-auto absolute left-1/2 top-4 z-50 -translate-x-1/2">
        <EditorToolbar
          backgroundMode={editorShell.editorBackgroundMode}
          onSetBackgroundMode={editorShell.setEditorBackgroundMode}
          importedBackgroundImageFilename={importedBackgroundImageFilename}
          importedVideoFilename={importedVideoFilename}
          zoomLevel={editorShell.editorZoomLevel}
          onZoomIn={editorShell.increaseZoom}
          onZoomOut={editorShell.decreaseZoom}
          onResetZoom={editorShell.resetZoom}
          gridVisible={editorShell.editorGridVisible}
          onSetGridVisible={editorShell.setEditorGridVisible}
          snapToGrid={editorShell.editorSnapToGrid}
          onSetSnapToGrid={editorShell.setEditorSnapToGrid}
          undoRedoControls={undoRedoControls}
        />
      </div>
      <div ref={scrollViewportRef} data-overlay-viewport className="absolute left-0 right-0 top-12 bottom-0 overflow-auto" onWheel={handleWheel}>
        <div
          ref={setStageElement}
          data-testid="overlay-editor-stage"
          className="relative grid min-h-full min-w-full w-max place-items-center overflow-visible p-4"
          onMouseDown={videoSyncMode ? undefined : handleSceneMouseDown}
        >
          <div
            className="relative shrink-0"
            style={{ width: viewportContentSize.width * displayScale, height: viewportContentSize.height * displayScale }}
          >
            {videoSyncMode ? (
              <VideoSyncPreviewScreens
                activity={activity}
                detection={videoSyncMarkControls.detection}
                displayScale={displayScale}
                onDeleteCourseLocation={videoSyncMarkControls.onDeleteCourseLocation}
                onSetCourseLocation={videoSyncMarkControls.onSetCourseLocation}
                previewSecond={previewSecond}
                sceneSize={sceneSize}
                setSceneElement={setSceneElement}
              />
            ) : (
              // CSS zoom, unlike a scale transform, does not force an offscreen render surface around the composited widget canvases.
              // Zoom also creates no stacking context, so isolate keeps Moveable's z-index 3000 controls below app modals.
              // WebKitGTK reports offsetLeft/offsetTop wrongly inside a CSS zoom container when GNOME text scaling is not 1.0,
              // which misplaces Moveable's selection. Linux scales with a transform instead.
              <div
                className="absolute left-0 top-0 isolate"
                style={
                  platformOs === 'linux'
                    ? { ...sceneSize, transform: `scale(${displayScale})`, transformOrigin: 'top left' }
                    : { ...sceneSize, zoom: displayScale }
                }
              >
                <OverlayCanvas
                  state={editor}
                  backgroundMode={backgroundMode}
                  displayScale={displayScale}
                  gridVisible={gridVisible}
                  handleWidgetMouseDown={handleWidgetMouseDown}
                />
                <OverlayMoveable {...moveableProps} />
              </div>
            )}
            {!videoSyncMode && widgetBadges.length ? (
              <div data-testid="widget-badge-layer" className="pointer-events-none absolute inset-0 z-50 overflow-visible">
                {widgetBadges.map((badge) => {
                  const Icon = WIDGET_ICONS[badge.type]
                  return (
                    <div
                      key={badge.id}
                      data-widget-badge-id={badge.id}
                      className="absolute flex h-5 items-center gap-1 rounded-xs border border-border/70 bg-card/90 px-2 text-[0.815rem] font-semibold leading-none text-muted-foreground shadow-sm"
                      style={{ left: badge.left, top: badge.top, transform: `rotate(${badge.rotation}deg)`, transformOrigin: 'top left' }}
                    >
                      <Icon className="h-3 w-3" />
                      <span>{getWidgetTypeName(badge.type, t)}</span>
                    </div>
                  )
                })}
              </div>
            ) : null}
            {videoSyncMode ? (
              <div
                className="pointer-events-none absolute left-0 top-0 @container-[size]"
                style={{ width: sceneSize.width * displayScale, height: '100%' }}
              >
                <VideoSyncMarkControls {...videoSyncMarkControls} />
              </div>
            ) : null}
          </div>
          {!videoSyncMode && selectionRect ? (
            <div
              data-testid="selection-rect"
              className="pointer-events-none absolute z-40 border border-primary/70 bg-primary/10"
              style={{ left: selectionRect.x, top: selectionRect.y, width: selectionRect.width, height: selectionRect.height }}
            />
          ) : null}
        </div>
      </div>
    </div>
  )
}

/** @param {object} props Editor inputs; config may be null before a template is loaded. @returns {JSX.Element} Overlay editor. */
function OverlayEditor(props) {
  const { t } = useTranslation()
  if (props.config !== null) return <OverlayEditorContent {...props} />
  return (
    <div className="flex h-full items-center justify-center p-8">
      <div className="max-w-sm rounded-sm border border-dashed border-border/70 bg-card/60 px-8 py-10 text-center shadow-[0_30px_80px_rgba(0,0,0,0.25)]">
        <div className="mx-auto mb-4 flex h-14 w-14 items-center justify-center bg-surface-elevated text-primary">
          <LayoutGrid className="h-6 w-6" />
        </div>
        <p className="text-sm font-semibold text-foreground">{t('overlay-editor.overlayCanvasReady', 'Overlay canvas ready')}</p>
        <p className="mt-2 text-sm text-muted-foreground">
          {t('overlay-editor.emptyEditorHint', 'Load a template or add widgets to start positioning the overlay.')}
        </p>
      </div>
    </div>
  )
}

export default memo(OverlayEditor)
