import Moveable from 'react-moveable'
import { CORNER_RESIZE_DIRECTIONS, EDGE_RESIZE_DIRECTIONS, SNAP_DIRECTIONS } from '../data/overlayEditorConstants'

/** @param {object} props Selection targets, gesture handlers, and viewport guides. @returns {JSX.Element|null} WASM widget interaction handles. */
export default function OverlayMoveable({
  moveableRef,
  selectedTarget,
  selectedTargets,
  isGroupSelection,
  isGroupDragActive,
  sceneElement,
  canResizeSelected,
  canRotateSelected,
  maintainAspectRatio,
  showEdgeResizeHandles,
  elementGuidelines,
  horizontalGuidelines,
  verticalGuidelines,
  zoom,
  handlers,
}) {
  if ((!selectedTarget && !selectedTargets.length) || !sceneElement) return null
  return (
    <Moveable
      ref={moveableRef}
      className="ovrley-moveable"
      target={selectedTarget ?? undefined}
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
      checkResizableError={false}
      scalable={false}
      rotatable={canRotateSelected}
      renderDirections={showEdgeResizeHandles ? EDGE_RESIZE_DIRECTIONS : CORNER_RESIZE_DIRECTIONS}
      snappable
      snapDirections={SNAP_DIRECTIONS}
      elementSnapDirections={SNAP_DIRECTIONS}
      snapThreshold={8}
      snapGap
      keepRatio={maintainAspectRatio || isGroupSelection}
      useResizeObserver
      useMutationObserver
      elementGuidelines={elementGuidelines}
      horizontalGuidelines={horizontalGuidelines}
      verticalGuidelines={verticalGuidelines}
      zoom={zoom}
      onDragStart={handlers.onDragStart}
      onDrag={handlers.onDrag}
      onDragEnd={handlers.onDragEnd}
      onDragGroupStart={handlers.onDragGroupStart}
      onDragGroup={handlers.onDragGroup}
      onDragGroupEnd={handlers.onDragGroupEnd}
      onResizeStart={handlers.onResizeStart}
      onResize={handlers.onResize}
      onResizeEnd={handlers.onResizeEnd}
      onRotateStart={handlers.onRotateStart}
      onRotate={handlers.onRotate}
      onRotateEnd={handlers.onRotateEnd}
    />
  )
}
