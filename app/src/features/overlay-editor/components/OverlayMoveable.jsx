/**
 * Renders the overlay moveable component.
 */

import Moveable from 'react-moveable'
import { useMemo } from 'react'
import { getEditorGridSize } from '../utils/overlayEditorUtils'
import { CORNER_RESIZE_DIRECTIONS, EDGE_RESIZE_DIRECTIONS, MOVEABLE_ZOOM } from '../data/overlayEditorConstants'

function getGridGuidelines(max, gridSize, enabled) {
  const guidelines = [0, max / 2, max]

  if (enabled) {
    for (let value = gridSize; value < max; value += gridSize) {
      guidelines.push(value)
    }
  }

  return [...new Set(guidelines)].sort((left, right) => left - right)
}

/**
 * Renders the overlay moveable component.
 *
 * @param {object} props - Component props.
 * @param {*} props.moveableRef - Value for moveable ref.
 * @param {*} props.selectedTarget - Value for selected target.
 * @param {*} props.selectedTargets - Value for selected targets.
 * @param {*} props.isGroupDragActive - Boolean flag for is group drag active.
 * @param {*} props.sceneElement - Value for scene element.
 * @param {*} props.displayScale - Value for display scale.
 * @param {*} props.canResizeSelected - Value for can resize selected.
 * @param {*} props.canScaleSelected - Value for can scale selected.
 * @param {*} props.canRotateSelected - Value for can rotate selected.
 * @param {*} props.maintainAspectRatio - Value for maintain aspect ratio.
 * @param {*} props.showEdgeResizeHandles - Whether to render edge resize handles.
 * @param {*} props.elementGuidelines - Value for element guidelines.
 * @param {*} props.sceneSize - Numeric scene size value.
 * @param {*} props.snapToGrid - Whether to snap Moveable to editor grid guides.
 * @param {*} props.handlers - Value for handlers.
 * @returns {JSX.Element} Rendered component output.
 */
export default function OverlayMoveable({
  moveableRef,
  selectedTarget,
  selectedTargets,
  isGroupDragActive,
  sceneElement,
  canResizeSelected,
  canScaleSelected,
  canRotateSelected,
  maintainAspectRatio,
  showEdgeResizeHandles,
  elementGuidelines,
  sceneSize,
  snapToGrid,
  handlers,
}) {
  const isGroupSelection = selectedTargets.length > 1
  const gridSize = getEditorGridSize(sceneSize)
  const horizontalGuidelines = useMemo(() => getGridGuidelines(sceneSize.height, gridSize, snapToGrid), [gridSize, sceneSize.height, snapToGrid])
  const verticalGuidelines = useMemo(() => getGridGuidelines(sceneSize.width, gridSize, snapToGrid), [gridSize, sceneSize.width, snapToGrid])

  if ((!selectedTarget && !selectedTargets.length) || !sceneElement) {
    return null
  }

  return (
    <Moveable
      ref={moveableRef}
      className="ovrley-moveable"
      target={selectedTarget || undefined}
      targets={selectedTargets.length ? selectedTargets : undefined}
      container={sceneElement}
      rootContainer={document.body}
      origin={false}
      edge={false}
      groupable={isGroupSelection}
      passDragArea={isGroupSelection}
      hideChildMoveableDefaultLines={isGroupDragActive && isGroupSelection}
      draggable
      resizable={canResizeSelected}
      scalable={canScaleSelected}
      rotatable={canRotateSelected}
      renderDirections={showEdgeResizeHandles ? EDGE_RESIZE_DIRECTIONS : CORNER_RESIZE_DIRECTIONS}
      snappable
      snapThreshold={8}
      snapGap
      keepRatio={maintainAspectRatio}
      useResizeObserver
      useMutationObserver
      elementGuidelines={elementGuidelines}
      horizontalGuidelines={horizontalGuidelines}
      verticalGuidelines={verticalGuidelines}
      zoom={MOVEABLE_ZOOM}
      onDragStart={handlers.onDragStart}
      onDrag={handlers.onDrag}
      onDragEnd={handlers.onDragEnd}
      onDragGroupStart={handlers.onDragGroupStart}
      onDragGroup={handlers.onDragGroup}
      onDragGroupEnd={handlers.onDragGroupEnd}
      onResizeStart={handlers.onResizeStart}
      onResize={handlers.onResize}
      onResizeEnd={handlers.onResizeEnd}
      onScaleStart={handlers.onScaleStart}
      onScale={handlers.onScale}
      onScaleEnd={handlers.onScaleEnd}
      onRotateStart={handlers.onRotateStart}
      onRotate={handlers.onRotate}
      onRotateEnd={handlers.onRotateEnd}
    />
  )
}
