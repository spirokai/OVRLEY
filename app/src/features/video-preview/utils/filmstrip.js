/**
 * Pure filmstrip planning: the frame size of a video's strips, the zoom windows the timeline asks for, and which
 * decoder decodes them. Rust plans the frames of each strip; this side only picks strips on the same grid.
 */

import { FILMSTRIP_FINEST_SPACING_SECONDS, FILMSTRIP_FRAME_HEIGHT_PX, FILMSTRIP_FRAMES_PER_STRIP } from '../data/filmstrip'

/** The base strip of a video: up to 24 frames across the whole video. */
export const BASE_FILMSTRIP_WINDOW = Object.freeze({ kind: 'base' })

/**
 * Frame size of a video's strips: `FILMSTRIP_FRAME_HEIGHT_PX` tall at the video's display aspect ratio.
 * @param {{resolution: {width: number, height: number}}} video Imported video.
 * @returns {{frameWidth: number, frameHeight: number}} Frame size in pixels.
 */
export function filmstripFrameSize(video) {
  return {
    frameWidth: Math.round((FILMSTRIP_FRAME_HEIGHT_PX * video.resolution.width) / video.resolution.height),
    frameHeight: FILMSTRIP_FRAME_HEIGHT_PX,
  }
}

/** Frame spacing of a video's base strip: at most 24 frames, never closer than the finest spacing. */
export function baseFilmstripSpacingSeconds(durationSeconds) {
  return Math.max(FILMSTRIP_FINEST_SPACING_SECONDS, durationSeconds / FILMSTRIP_FRAMES_PER_STRIP)
}

/** Frame spacing on zoom level `level`. */
function zoomFilmstripSpacingSeconds(level) {
  return FILMSTRIP_FINEST_SPACING_SECONDS * 2 ** level
}

/**
 * The zoom level whose frames suit tiles covering `secondsPerTile` of source each: the coarsest level that still gives
 * every tile its own frame, never finer than level 0. Null when the base strip already does, so no window is needed.
 * @param {number} secondsPerTile Source seconds one filmstrip tile covers at the current zoom.
 * @param {number} durationSeconds Video duration.
 * @returns {number|null} Zoom level, or null for the base strip.
 */
export function filmstripZoomLevel(secondsPerTile, durationSeconds) {
  const baseSpacing = baseFilmstripSpacingSeconds(durationSeconds)
  if (!(secondsPerTile < baseSpacing)) return null
  let level = Math.max(0, Math.floor(Math.log2(secondsPerTile / FILMSTRIP_FINEST_SPACING_SECONDS)))
  while (level > 0 && zoomFilmstripSpacingSeconds(level) >= baseSpacing) level -= 1
  return zoomFilmstripSpacingSeconds(level) < baseSpacing ? level : null
}

/**
 * The windows of zoom level `level` that cover a stretch of a video.
 * @param {number} level Zoom level.
 * @param {number} fromSecond Stretch start in source seconds.
 * @param {number} toSecond Stretch end in source seconds.
 * @param {number} durationSeconds Video duration.
 * @returns {Array<{kind: 'zoom', level: number, window: number}>} Windows, first to last.
 */
export function filmstripZoomWindows(level, fromSecond, toSecond, durationSeconds) {
  const windowSeconds = FILMSTRIP_FRAMES_PER_STRIP * zoomFilmstripSpacingSeconds(level)
  const from = Math.max(0, fromSecond)
  const to = Math.min(toSecond, durationSeconds)
  if (!(to > from)) return []
  const first = Math.floor(from / windowSeconds)
  // A stretch ending exactly on a window edge does not reach into the next window.
  const last = Math.max(first, Math.ceil(to / windowSeconds) - 1)
  const windows = []
  for (let window = first; window <= last; window += 1) windows.push({ kind: 'zoom', level, window })
  return windows
}

/**
 * Key of a strip among a video's strips.
 * @param {{kind: 'base'}|{kind: 'zoom', level: number, window: number}} window Strip window.
 * @returns {string} `base`, or `z<level>-<window>`.
 */
export function filmstripWindowKey(window) {
  return window.kind === 'base' ? 'base' : `z${window.level}-${window.window}`
}

/**
 * Hardware decoder of filmstrips, from the startup codec detection: the GPU an export can use, through FFmpeg's own
 * decoders. Windows machines with Intel (QSV) or AMD (AMF) encoders decode through D3D11VA.
 * @param {object|null} codecs Detected codecs (`fetchAvailableCodecs`), or null when detection failed.
 * @returns {'cuda'|'vaapi'|'d3d11va'|'videotoolbox'|null} Decoder, or null to decode in software.
 */
export function filmstripHwaccel(codecs) {
  if (codecs === null) return null
  if (codecs.nvdec) return 'cuda'
  if (codecs.vaapi) return 'vaapi'
  if (codecs.qsv || codecs.h264Amf || codecs.hevcAmf) return 'd3d11va'
  if (codecs.videotoolbox) return 'videotoolbox'
  return null
}
