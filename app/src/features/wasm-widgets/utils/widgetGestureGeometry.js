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
