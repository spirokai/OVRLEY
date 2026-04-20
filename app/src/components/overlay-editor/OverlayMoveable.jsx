import Moveable from 'react-moveable'

export default function OverlayMoveable({
  moveableRef,
  selectedTarget,
  sceneElement,
  canResizeSelected,
  canScaleSelected,
  canRotateSelected,
  elementGuidelines,
  sceneSize,
  handlers,
}) {
  if (!selectedTarget || !sceneElement) {
    return null
  }

  return (
    <Moveable
      ref={moveableRef}
      className="cyclemetry-moveable"
      target={selectedTarget}
      container={sceneElement}
      origin={false}
      edge={false}
      draggable
      resizable={canResizeSelected}
      scalable={canScaleSelected}
      rotatable={canRotateSelected}
      renderDirections={['nw', 'ne', 'sw', 'se']}
      snappable
      snapThreshold={8}
      snapGap
      keepRatio={false}
      elementGuidelines={elementGuidelines}
      horizontalGuidelines={[0, sceneSize.height / 2, sceneSize.height]}
      verticalGuidelines={[0, sceneSize.width / 2, sceneSize.width]}
      bounds={{
        left: 0,
        top: 0,
        right: sceneSize.width,
        bottom: sceneSize.height,
      }}
      zoom={1}
      onDragStart={handlers.onDragStart}
      onDrag={handlers.onDrag}
      onDragEnd={handlers.onDragEnd}
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
