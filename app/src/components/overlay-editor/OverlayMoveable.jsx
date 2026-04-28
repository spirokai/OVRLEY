import Moveable from 'react-moveable'

export default function OverlayMoveable({
  moveableRef,
  selectedTarget,
  selectedTargets,
  isGroupDragActive,
  sceneElement,
  displayScale,
  canResizeSelected,
  canScaleSelected,
  canRotateSelected,
  maintainAspectRatio,
  elementGuidelines,
  sceneSize,
  handlers,
}) {
  const isGroupSelection = selectedTargets.length > 1
  const moveableZoom = displayScale > 0 ? 1 / displayScale : 1

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
