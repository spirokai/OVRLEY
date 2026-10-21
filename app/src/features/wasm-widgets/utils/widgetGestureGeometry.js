/** @param {object} gesture Gesture-local axis lock. @param {number[]} delta Scene translation. @param {boolean} locked Ctrl key state. @returns {number[]} Constrained translation. */
export function dragTranslation(gesture, delta, locked) {
  if (!locked) {
    gesture.axis = null
    return delta
  }
  const [x, y] = delta
  if (!gesture.axis) {
    if (Math.max(Math.abs(x), Math.abs(y)) < 3) return [0, 0]
    gesture.axis = Math.abs(x) >= Math.abs(y) ? 'x' : 'y'
  }
  return gesture.axis === 'x' ? [x, 0] : [0, y]
}

/** @param {number} rotation Angle in degrees. @returns {number} Its position in the 0–360 degree slider range. */
export function normalizeRotation(rotation) {
  return ((rotation % 360) + 360) % 360
}

/** @param {object} widget Resolved widget. @param {object} bounds Initial content rectangle. @param {number} width Requested scene width. @param {number} height Requested scene height. @returns {object} Intrinsic resize request. */
export function widgetResizeGeometry(widget, bounds, width, height) {
  return widget.type === 'backdrop' ||
    widget.type === 'raster' ||
    widget.type === 'elevation' ||
    widget.type === 'diving_profile' ||
    ['heading_tape', 'heart_zones', 'linear'].includes(widget.data.display_type)
    ? { size: [Math.max(width, 0.01), Math.max(height, 0.01)] }
    : { factor: Math.max(width / bounds.width, 0.01) }
}

/** @param {object} bounds Rust content rectangle. @param {number[]} direction Moveable handle direction. @param {number} rotation Frame rotation in degrees. @param {number[]} origin Rotation origin within the frame. @returns {object} Fixed corner constraint. */
export function resizeAnchor(bounds, direction, rotation = 0, origin = [0, 0]) {
  const fixed = direction.map((value) => (1 - value) / 2)
  const angle = (rotation * Math.PI) / 180
  const x = bounds.width * fixed[0] - origin[0]
  const y = bounds.height * fixed[1] - origin[1]
  return {
    fixed,
    point: [bounds.left + origin[0] + x * Math.cos(angle) - y * Math.sin(angle), bounds.top + origin[1] + x * Math.sin(angle) + y * Math.cos(angle)],
  }
}

/** @param {object} rect Moveable group rectangle in scene coordinates. @param {number[]} direction Handle direction. @returns {object} Initial width and fixed scene point. */
export function groupResizeOrigin(rect, direction) {
  const fixed = direction.map((value) => (1 - value) / 2)
  return {
    width: rect.offsetWidth,
    point: rect.pos1.map((value, index) => value + (rect.pos2[index] - value) * fixed[0] + (rect.pos3[index] - value) * fixed[1]),
  }
}

/** @param {object[]} geometries WASM widget geometry, including optional frame rotation. @returns {object} Combined content bounds in scene coordinates. */
export function groupContentBounds(geometries) {
  const points = geometries.flatMap((geometry) =>
    [
      [1, 1],
      [1, -1],
      [-1, 1],
      [-1, -1],
    ].map((direction) => resizeAnchor(geometry.content, direction, geometry.rotation, geometry.rotation_origin).point),
  )
  const left = Math.min(...points.map(([x]) => x))
  const top = Math.min(...points.map(([, y]) => y))
  return { left, top, width: Math.max(...points.map(([x]) => x)) - left, height: Math.max(...points.map(([, y]) => y)) - top }
}
