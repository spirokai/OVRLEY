/**
 * Provides overlay editor helpers for overlay moveable.
 */

import Moveable from 'react-moveable'

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
 * @param {*} props.elementGuidelines - Value for element guidelines.
 * @param {*} props.sceneSize - Numeric scene size value.
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
  elementGuidelines,
  sceneSize,
  handlers,
}) {
  const isGroupSelection = selectedTargets.length > 1
  // With rootContainer={document.body}, the Moveable UI is rendered at body
  // level, not inside the scaled parent container. No zoom compensation needed.
  const moveableZoom = 1.5

  if ((!selectedTarget && !selectedTargets.length) || !sceneElement) {
    return null
  }

  return (
    <Moveable
      ref={moveableRef}
      className="cyclemetry-moveable"
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
      renderDirections={['nw', 'ne', 'sw', 'se']}
      snappable
      snapThreshold={8}
      snapGap
      keepRatio={maintainAspectRatio}
      useResizeObserver
      useMutationObserver
      elementGuidelines={elementGuidelines}
      horizontalGuidelines={[0, sceneSize.height / 2, sceneSize.height]}
      verticalGuidelines={[0, sceneSize.width / 2, sceneSize.width]}
      zoom={moveableZoom}
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
