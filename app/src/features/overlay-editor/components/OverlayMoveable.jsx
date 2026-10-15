import Moveable from 'react-moveable'
import { createPortal } from 'react-dom'
import { CORNER_RESIZE_DIRECTIONS, EDGE_RESIZE_DIRECTIONS, SNAP_DIRECTIONS } from '../data/overlayEditorConstants'
import { useSelectionBorder } from '../hooks/useSelectionBorder'

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
      ables={SELECTION_BORDER_ABLES}
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
      onResizeGroupStart={handlers.onResizeGroupStart}
      onResizeGroup={handlers.onResizeGroup}
      onResizeGroupEnd={handlers.onResizeGroupEnd}
      onRotateStart={handlers.onRotateStart}
      onRotate={handlers.onRotate}
      onRotateEnd={handlers.onRotateEnd}
    />
  )
}

// Native Moveable lines shimmer under fractional scene transforms, even with inverse zoom.
// Draw a device-pixel-aligned SVG outside the scaled scene to keep border thickness steady.
const SELECTION_BORDER_ABLES = [
  {
    name: 'selectionBorder',
    always: true,
    render(moveable, React) {
      const managers = moveable.props.groupable && !moveable.props.hideChildMoveableDefaultLines ? [moveable, ...moveable.moveables] : [moveable]
      const borders = managers.map((manager) => {
        const { left, top, pos1, pos2, pos3, pos4 } = manager.getState()
        return [pos1, pos2, pos4, pos3].map(([x, y]) => ({ x: left + x, y: top + y }))
      })
      return [React.createElement(SelectionBorder, { key: 'selection-border', container: moveable.getContainer(), borders })]
    },
  },
]

function SelectionBorder({ container, borders }) {
  const border = useSelectionBorder(container, borders)
  if (border === null) return null // Measured in the layout effect before the first paint.
  return createPortal(
    <svg
      aria-hidden="true"
      className="ovrley-moveable ovrley-selection-border"
      style={{ position: 'fixed', inset: 0, width: '100%', height: '100%', pointerEvents: 'none', zIndex: 1, clipPath: border.clipPath }}
    >
      <g fill="none" stroke="var(--theme-color-accent)" strokeWidth={border.strokeWidth}>
        {border.polygons.map((points, index) => (
          <polygon key={index} points={points} />
        ))}
      </g>
    </svg>,
    document.body,
  )
}
