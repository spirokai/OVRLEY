/**
 * Main overlay editor component — renders the canvas, Moveable resize handles,
 * widget badge labels, zoom-to-fit viewport, and empty state.
 *
 * Acts as a thin shell that delegates state orchestration to useOverlayEditorState
 * and passes grouped props to child components.
 */

import { memo, useMemo, useState } from 'react'
import { LayoutGrid, Tag } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import OverlayCanvas from './OverlayCanvas'
import OverlayMoveable from './OverlayMoveable'
import { buildMetricWidgetPreviewModel } from '@/features/widget-preview'
import { WIDGET_ICONS } from '../data/overlayEditorConfig'
import { getWidgetSceneOrigin } from '../utils/overlayEditorHelpers'
import useOverlayEditorState from '../hooks/useOverlayEditorState'

/**
 * Renders floating badge labels above selected/hovered widgets showing their type.
 * Only visible during widget selection — hidden entirely when no widgets are selected.
 */
function WidgetBadgeLayer({ activity, displayScale, globalScale, hoveredWidgetId, previewSecond, selectedWidgetIds, widgets }) {
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
        const Icon = WIDGET_ICONS[widget.type] || Tag
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
 * Displays scene resolution and optional template save status badge
 * in the top-left corner of the editor viewport.
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
            status === 'Modified' ? 'border-accent-border bg-surface-accent-soft text-primary' : 'bg-card/85'
          }`}
        >
          {status}
        </Badge>
      ) : null}
    </div>
  )
}

/**
 * Placeholder shown when no config is loaded — guides the user to load a template
 * or add widgets to begin editing.
 */
function EmptyOverlayState() {
  return (
    <div className="flex h-full items-center justify-center p-8">
      <div className="max-w-sm rounded-xl border border-dashed border-border/70 bg-card/60 px-8 py-10 text-center shadow-[0_30px_80px_rgba(0,0,0,0.25)] backdrop-blur-sm">
        <div className="mx-auto mb-4 flex h-14 w-14 items-center justify-center bg-surface-elevated text-primary">
          <LayoutGrid className="h-6 w-6" />
        </div>
        <p className="text-sm font-semibold text-foreground">Overlay canvas ready</p>
        <p className="mt-2 text-sm text-muted-foreground">Load a template or add widgets to start positioning the overlay.</p>
      </div>
    </div>
  )
}

/**
 * @param {object} props
 * @param {object|null} props.config - Current overlay template config.
 * @param {object} props.globalDefaults - Global default values (opacity, scale, styles).
 * @param {Function} props.onConfigChange - Callback to update the template config.
 * @param {number} props.zoomLevel - Current viewport zoom multiplier.
 * @param {Function} props.onZoomLevelChange - Callback to change zoom level.
 * @param {string} props.backgroundMode - Canvas background mode (black, checker, white, video).
 * @param {boolean} props.gridVisible - Whether the editor grid overlay is visible.
 * @param {boolean} props.snapToGrid - Whether Moveable snapping is enabled.
 * @param {boolean} props.showTemplateStatus - Whether to display the template save-status badge.
 * @param {string} props.templateStatus - Current template save status text.
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
    exportRange,
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

  const valueFont =
    config?.values?.find((value) => value.font || value.font_family)?.font ||
    config?.values?.find((value) => value.font || value.font_family)?.font_family ||
    globalDefaults?.font_values

  const canvasSceneProps = useMemo(
    () => ({ sceneFont: config?.scene?.font, sceneFontSize: config?.scene?.font_size, sceneStyle, valueFont, sceneSize }),
    [config?.scene?.font, config?.scene?.font_size, sceneStyle, valueFont, sceneSize],
  )
  const canvasDisplayProps = useMemo(
    () => ({ displayScale, globalScale, globalOpacity, backgroundMode, gridVisible }),
    [displayScale, globalScale, globalOpacity, backgroundMode, gridVisible],
  )
  const canvasDataProps = useMemo(
    () => ({ widgets, activity, previewSecond, selectionRect, exportRange }),
    [widgets, activity, previewSecond, selectionRect, exportRange],
  )
  const canvasCallbacks = useMemo(
    () => ({ setSceneElement, handleSceneMouseDown, handleWidgetMouseDown, setHoveredWidgetId, widgetRefCallbacks }),
    [setSceneElement, handleSceneMouseDown, handleWidgetMouseDown, setHoveredWidgetId, widgetRefCallbacks],
  )

  if (!config) {
    return <EmptyOverlayState />
  }

  return (
    <div ref={viewportRef} className="relative flex h-full flex-1 overflow-hidden" onWheel={handleWheel}>
      <div className="relative flex h-full w-full items-center justify-center overflow-hidden p-8">
        <CanvasStatusBadges height={sceneSize.height} showTemplateStatus={showTemplateStatus} status={templateStatus} width={sceneSize.width} />
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
            <OverlayCanvas sceneProps={canvasSceneProps} displayProps={canvasDisplayProps} dataProps={canvasDataProps} callbacks={canvasCallbacks} />

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
