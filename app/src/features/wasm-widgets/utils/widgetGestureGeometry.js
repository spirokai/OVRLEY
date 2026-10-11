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

/** @param {object} bounds Rust content rectangle. @param {number[]} direction Moveable handle direction. @returns {object} Fixed corner constraint. */
export function resizeAnchor(bounds, direction) {
  const fixed = direction.map((value) => (1 - value) / 2)
  return { fixed, point: [bounds.left + bounds.width * fixed[0], bounds.top + bounds.height * fixed[1]] }
}
