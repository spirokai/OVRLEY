/** @param {object[][]} borders Scene vertices. @param {DOMRect} bounds Scene viewport bounds. @param {object} sceneSize Unscaled scene dimensions. @param {number} pixelRatio Device scale. @returns {object} Screen polygons and stroke width. */
export function projectSelectionBorders(borders, bounds, sceneSize, pixelRatio) {
  const strokePixels = Math.max(1, Math.round(pixelRatio))
  // Vertices stay fractional so the border tracks the unsnapped widget content while resizing.
  const polygons = borders.map((vertices) =>
    vertices
      .map(({ x, y }) => `${bounds.left + (x * bounds.width) / sceneSize.width},${bounds.top + (y * bounds.height) / sceneSize.height}`)
      .join(' '),
  )
  return { polygons, strokeWidth: strokePixels / pixelRatio }
}
