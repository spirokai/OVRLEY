/**
 * Pure filmstrip layout of video clips on the timeline. Tiles are as tall as the clip and as wide as the video's
 * aspect ratio makes them, and run from each segment's left edge: zoom changes how much source time a tile covers,
 * not its size. Each tile shows the frame of the finest loaded strip whose stretch holds the source time at its left
 * edge, so zoomed in past a strip's spacing neighbouring tiles repeat a frame, and zoomed out they skip frames.
 *
 * Zoomed in past the base strip's spacing, the timeline asks for zoom windows (`listWantedFilmstripWindows`); until
 * they load, the base strip stands in.
 */

import { filmstripWindowKey, filmstripZoomLevel, filmstripZoomWindows } from '@/features/video-preview/utils/filmstrip'

/**
 * Width of a video's filmstrip tiles: the clip height at the video's display aspect ratio.
 * @param {{width: number, height: number}} resolution Video resolution.
 * @param {number} tileHeightPx Clip height in pixels.
 * @returns {number} Tile width in whole pixels.
 */
export function filmstripTileWidthPx(resolution, tileHeightPx) {
  return Math.round((tileHeightPx * resolution.width) / resolution.height)
}

function stripEndSecond(strip) {
  return strip.startSeconds + strip.sampleCount * strip.spacingSeconds
}

/** The finest strip whose stretch holds `sourceSecond`, or the coarsest strip when none does. */
function stripAt(strips, sourceSecond) {
  let best = null
  for (const strip of strips) {
    if (sourceSecond < strip.startSeconds || sourceSecond >= stripEndSecond(strip)) continue
    if (best === null || strip.spacingSeconds < best.spacingSeconds) best = strip
  }
  return best ?? strips.reduce((coarsest, strip) => (strip.spacingSeconds > coarsest.spacingSeconds ? strip : coarsest))
}

function sampleAt(strip, sourceSecond) {
  return Math.min(strip.sampleCount - 1, Math.max(0, Math.floor((sourceSecond - strip.startSeconds) / strip.spacingSeconds)))
}

/** Background showing one frame of a strip's sprite (frames side by side), scaled so that the frame fills the tile. */
function spriteFrameStyle({ spriteUrl, sampleCount }, sample) {
  return {
    backgroundImage: `url("${spriteUrl}")`,
    backgroundSize: `${sampleCount * 100}% 100%`,
    backgroundPosition: `${sampleCount > 1 ? (sample / (sampleCount - 1)) * 100 : 0}% 0%`,
  }
}

/**
 * Filmstrip tiles of one clip segment that fall inside the viewport, positioned within the segment. A segment narrower
 * than a tile shows the start of one; the segment clips the last tile.
 *
 * @param {object} options Segment placement.
 * @param {Array<{spriteUrl: string, startSeconds: number, spacingSeconds: number, sampleCount: number}>} options.strips
 *   Ready strips of the clip's video, at least its base strip.
 * @param {number} options.sourceStartSecond Source second at the segment's left edge.
 * @param {number} options.sourceEndSecond Source second at the segment's right edge.
 * @param {number} options.leftPx Segment's left edge in viewport pixels; negative when it starts before the viewport.
 * @param {number} options.pxPerSecond Viewport pixels per second.
 * @param {number} options.tileWidthPx Tile width in pixels.
 * @param {number} options.widthPx Viewport width in pixels.
 * @returns {Array<{key: number, style: object}>} Visible tiles, left to right, keyed by their index in the segment.
 */
export function listFilmstripTiles({ strips, sourceStartSecond, sourceEndSecond, leftPx, pxPerSecond, tileWidthPx, widthPx }) {
  const segmentWidthPx = (sourceEndSecond - sourceStartSecond) * pxPerSecond
  const firstIndex = Math.max(0, Math.floor(-leftPx / tileWidthPx))
  const endIndex = Math.min(Math.ceil(segmentWidthPx / tileWidthPx), Math.ceil((widthPx - leftPx) / tileWidthPx))
  const tiles = []
  for (let index = firstIndex; index < endIndex; index += 1) {
    const sourceSecond = sourceStartSecond + (index * tileWidthPx) / pxPerSecond
    const strip = stripAt(strips, sourceSecond)
    tiles.push({
      key: index,
      style: { left: `${index * tileWidthPx}px`, width: `${tileWidthPx}px`, ...spriteFrameStyle(strip, sampleAt(strip, sourceSecond)) },
    })
  }
  return tiles
}

/**
 * Zoom windows the timeline shows at its current zoom: for each found video clip zoomed in past its base strip's
 * spacing, the windows of the matching zoom level under the clip's visible part. A video on several clips asks once per
 * window.
 *
 * @param {object} options Timeline view.
 * @param {Array<{path: string, missing: boolean, offsetSeconds: number, durationSeconds: number, resolution: {width: number, height: number}}>} options.clips
 *   Displayed video clips.
 * @param {number} options.viewStart Viewport start in timeline seconds.
 * @param {number} options.viewEnd Viewport end in timeline seconds.
 * @param {number} options.widthPx Viewport width in pixels.
 * @param {number} options.tileHeightPx Clip height in pixels.
 * @returns {Record<string, Array<{kind: 'zoom', level: number, window: number}>>} Wanted windows per video path.
 */
export function listWantedFilmstripWindows({ clips, viewStart, viewEnd, widthPx, tileHeightPx }) {
  const wanted = {}
  if (!(widthPx > 0) || !(viewEnd > viewStart)) return wanted
  const pxPerSecond = widthPx / (viewEnd - viewStart)
  for (const clip of clips) {
    if (clip.missing) continue
    const secondsPerTile = filmstripTileWidthPx(clip.resolution, tileHeightPx) / pxPerSecond
    const level = filmstripZoomLevel(secondsPerTile, clip.durationSeconds)
    if (level === null) continue
    const windows = filmstripZoomWindows(level, viewStart - clip.offsetSeconds, viewEnd - clip.offsetSeconds, clip.durationSeconds)
    if (windows.length === 0) continue
    const known = new Set((wanted[clip.path] ?? []).map(filmstripWindowKey))
    wanted[clip.path] = [...(wanted[clip.path] ?? []), ...windows.filter((window) => !known.has(filmstripWindowKey(window)))]
  }
  return wanted
}
