/**
 * Renders the overlay editor portion of the application interface.
 */

import { memo, useMemo, useState } from 'react'
import { LayoutGrid, Type } from 'lucide-react'
import { Badge } from './ui/badge'
import OverlayCanvas from './overlay-editor/OverlayCanvas'
import OverlayMoveable from './overlay-editor/OverlayMoveable'
import { buildMetricWidgetPreviewModel } from './overlay-editor/metricWidgetPreviewModel'
import { WIDGET_ICONS } from './overlay-editor/constants'
import { getWidgetSceneOrigin } from './overlay-editor/overlayEditorHelpers'
import useOverlayEditorState from './overlay-editor/useOverlayEditorState'

function WidgetBadgeLayer({
  activity,
  displayScale,
  globalScale,
  hoveredWidgetId,
  previewSecond,
  selectedWidgetIds,
  widgets,
}) {
  const visibleWidgets = useMemo(() => {
    const visibleIds = new Set(selectedWidgetIds)
    if (hoveredWidgetId) {
      visibleIds.add(hoveredWidgetId)
    }

    return widgets.filter((widget) => visibleIds.has(widget.id))
  }, [hoveredWidgetId, selectedWidgetIds, widgets])

  if (!visibleWidgets.length) {
    return null
  }

  return (
    <div className="pointer-events-none absolute inset-0 z-50 overflow-visible">
      {visibleWidgets.map((widget) => {
        const Icon = WIDGET_ICONS[widget.type] || Type
        const metricPreviewModel = buildMetricWidgetPreviewModel({
          widget,
          activity,
          previewSecond,
        })
        const metricVisualBounds = metricPreviewModel?.visualBounds ?? null
        const origin = getWidgetSceneOrigin(widget, null, metricVisualBounds, {
          boundsScale: widget.category === 'plots' ? 1 : globalScale,
        })
        const left = origin.x * displayScale
        const top = Math.max(origin.y * displayScale - 24, 0)

        return (
          <div
            key={widget.id}
            className="absolute flex h-5 items-center gap-1 rounded-md border border-border/70 bg-card/90 px-2 text-[11px] font-semibold leading-none text-muted-foreground shadow-sm"
            style={{ left, top }}
          >
            <Icon className="h-3 w-3" />
            <span>{widget.type}</span>
          </div>
        )
      })}
    </div>
  )
}

/**
 * Renders the scene resolution badge.
 *
 * @param {object} props - Component props.
 * @param {*} props.height - Current scene height.
 * @param {*} props.width - Current scene width.
 * @returns {JSX.Element} Rendered component output.
 */
function CanvasStatusBadges({ height, showTemplateStatus, status, width }) {
  return (
    <div className="pointer-events-none absolute left-4 top-4 z-50 flex items-center gap-2">
      <div className="rounded-full border border-border/70 bg-card/85 px-3 py-1 text-xs font-medium text-muted-foreground shadow-lg backdrop-blur-sm">
        {width} &times; {height}
      </div>
      {showTemplateStatus ? (
        <Badge
          variant={status === 'Modified' ? 'secondary' : 'outline'}
          className={`h-6 rounded-full text-[10px] shadow-lg backdrop-blur-sm ${
            status === 'Modified'
              ? 'border-accent-border bg-surface-accent-soft text-primary'
              : 'bg-card/85'
          }`}
        >
          {status}
        </Badge>
      ) : null}
    </div>
  )
}

/**
 * Renders the empty overlay state component.
 * @returns {JSX.Element} Rendered component output.
 */
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

/**
 * Renders the overlay editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.config - Overlay template configuration data.
 * @param {*} props.globalDefaults - Value for global defaults.
 * @param {*} props.onConfigChange - Callback invoked to config change.
 * @param {*} props.zoomLevel - Current editor zoom level.
 * @param {*} props.onZoomLevelChange - Callback invoked to zoom level change.
 * @param {*} props.backgroundMode - Selected canvas background style.
 * @param {*} props.gridVisible - Whether to show the editor grid overlay.
 * @param {*} props.snapToGrid - Whether to snap Moveable to editor grid guides.
 * @param {*} props.showTemplateStatus - Whether to display the template status badge.
 * @param {*} props.templateStatus - Current template status label.
 * @returns {JSX.Element} Rendered component output.
 */
function OverlayEditor({
  config,
  globalDefaults,
  onConfigChange,
  zoomLevel,
  onZoomLevelChange,
  backgroundMode,
  gridVisible,
  snapToGrid,
  showTemplateStatus,
  templateStatus,
}) {
  const [hoveredWidgetId, setHoveredWidgetId] = useState(null)
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
    isGroupDragActive,
    maintainAspectRatio,
    moveableRef,
    previewSecond,
    sceneElement,
    sceneStyle,
    sceneSize,
    selectedTarget,
    selectedTargets,
    selectedWidgetIds,
    selectionRect,
    setSceneElement,
    showEdgeResizeHandles,
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

  if (!config) {
    return <EmptyOverlayState />
  }

  return (
    <div
      ref={viewportRef}
      className="relative flex h-full flex-1 overflow-hidden"
      onWheel={handleWheel}
    >
      <div className="relative flex h-full w-full items-center justify-center overflow-hidden p-8">
        <CanvasStatusBadges
          height={sceneSize.height}
          showTemplateStatus={showTemplateStatus}
          status={templateStatus}
          width={sceneSize.width}
        />
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
              displayScale={displayScale}
              globalScale={globalScale}
              globalOpacity={globalOpacity}
              activity={activity}
              previewSecond={previewSecond}
              backgroundMode={backgroundMode}
              gridVisible={gridVisible}
              sceneFont={config.scene?.font}
              sceneFontSize={config.scene?.font_size}
              sceneStyle={sceneStyle}
              valueFont={
                config.values?.find((value) => value.font || value.font_family)
                  ?.font ||
                config.values?.find((value) => value.font || value.font_family)
                  ?.font_family ||
                globalDefaults?.font_values
              }
              sceneSize={sceneSize}
              setSceneElement={setSceneElement}
              selectionRect={selectionRect}
              handleSceneMouseDown={handleSceneMouseDown}
              handleWidgetMouseDown={handleWidgetMouseDown}
              setHoveredWidgetId={setHoveredWidgetId}
              widgetRefCallbacks={widgetRefCallbacks}
            />

            <OverlayMoveable
              moveableRef={moveableRef}
              selectedTarget={selectedTarget}
              selectedTargets={selectedTargets}
              isGroupDragActive={isGroupDragActive}
              sceneElement={sceneElement}
              displayScale={displayScale}
              canResizeSelected={canResizeSelected}
              canScaleSelected={canScaleSelected}
              canRotateSelected={canRotateSelected}
              maintainAspectRatio={maintainAspectRatio}
              showEdgeResizeHandles={showEdgeResizeHandles}
              elementGuidelines={elementGuidelines}
              sceneSize={sceneSize}
              snapToGrid={snapToGrid}
              handlers={handlers}
            />
          </div>
          <WidgetBadgeLayer
            activity={activity}
            displayScale={displayScale}
            globalScale={globalScale}
            hoveredWidgetId={hoveredWidgetId}
            previewSecond={previewSecond}
            selectedWidgetIds={selectedWidgetIds}
            widgets={widgets}
          />
        </div>
      </div>
    </div>
  )
}

export default memo(OverlayEditor)
