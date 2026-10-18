/** @param {object[][]} borders Scene vertices. @param {DOMRect} bounds Scene viewport bounds. @param {object} sceneSize Unscaled scene dimensions. @param {number} pixelRatio Device scale. @returns {object} Pixel-aligned screen polygons and stroke width. */
export function getPixelAlignedBorders(borders, bounds, sceneSize, pixelRatio) {
  const strokePixels = Math.max(1, Math.round(pixelRatio))
  const centerOffset = (strokePixels % 2) / 2
  const snap = (value) => (Math.round(value * pixelRatio - centerOffset) + centerOffset) / pixelRatio
  const polygons = borders.map((vertices) =>
    vertices
      .map(({ x, y }) => `${snap(bounds.left + (x * bounds.width) / sceneSize.width)},${snap(bounds.top + (y * bounds.height) / sceneSize.height)}`)
      .join(' '),
  )
  return { polygons, strokeWidth: strokePixels / pixelRatio }
}
