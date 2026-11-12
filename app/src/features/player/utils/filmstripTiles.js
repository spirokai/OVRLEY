/**
 * Pure filmstrip tile layout of video clips on the timeline. Tiles are as tall as the clip and as wide as the video's
 * aspect ratio makes them, and run from each segment's left edge: zoom changes how much source time a tile covers,
 * not its size. Each tile shows the sampled frame nearest the source time at its left edge, so zoomed in past the
 * sample spacing neighbouring tiles repeat a frame, and zoomed out they skip frames.
 */

function nearestSample({ intervalSeconds, sampleCount }, sourceSecond) {
  return Math.min(sampleCount - 1, Math.max(0, Math.round(sourceSecond / intervalSeconds)))
}

/** Background showing one cell of the sprite sheet, scaled so that the cell fills the tile. */
function spriteCellStyle({ spriteUrl, columns, rows }, sample) {
  const column = sample % columns
  const row = Math.floor(sample / columns)
  return {
    backgroundImage: `url("${spriteUrl}")`,
    backgroundSize: `${columns * 100}% ${rows * 100}%`,
    backgroundPosition: `${columns > 1 ? (column / (columns - 1)) * 100 : 0}% ${rows > 1 ? (row / (rows - 1)) * 100 : 0}%`,
  }
}

/**
 * Filmstrip tiles of one clip segment that fall inside the viewport, positioned within the segment. A segment narrower
 * than a tile shows the start of one; the segment clips the last tile.
 *
 * @param {object} options Segment placement.
 * @param {{spriteUrl: string, columns: number, rows: number, intervalSeconds: number, sampleCount: number}} options.filmstrip
 *   Ready filmstrip of the clip's video.
 * @param {number} options.sourceStartSecond Source second at the segment's left edge.
 * @param {number} options.sourceEndSecond Source second at the segment's right edge.
 * @param {number} options.leftPx Segment's left edge in viewport pixels; negative when it starts before the viewport.
 * @param {number} options.pxPerSecond Viewport pixels per second.
 * @param {number} options.tileWidthPx Tile width in pixels.
 * @param {number} options.widthPx Viewport width in pixels.
 * @returns {Array<{key: number, style: object}>} Visible tiles, left to right, keyed by their index in the segment.
 */
export function listFilmstripTiles({ filmstrip, sourceStartSecond, sourceEndSecond, leftPx, pxPerSecond, tileWidthPx, widthPx }) {
  const segmentWidthPx = (sourceEndSecond - sourceStartSecond) * pxPerSecond
  const firstIndex = Math.max(0, Math.floor(-leftPx / tileWidthPx))
  const endIndex = Math.min(Math.ceil(segmentWidthPx / tileWidthPx), Math.ceil((widthPx - leftPx) / tileWidthPx))
  const tiles = []
  for (let index = firstIndex; index < endIndex; index += 1) {
    const sourceSecond = sourceStartSecond + (index * tileWidthPx) / pxPerSecond
    tiles.push({
      key: index,
      style: { left: `${index * tileWidthPx}px`, width: `${tileWidthPx}px`, ...spriteCellStyle(filmstrip, nearestSample(filmstrip, sourceSecond)) },
    })
  }
  return tiles
}
