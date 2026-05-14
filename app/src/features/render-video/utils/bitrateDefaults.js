/**
 * Bitrate calculation utility.
 * Picks the appropriate bitrate bin by resolution and derives the
 * correct value based on codec (h264/h265) and frame rate.
 */

import { BITRATE_BINS, BITRATE_FALLBACK } from '../data/bitrateDefaults'

/**
 * Returns the recommended output bitrate in Mbps for the given render settings.
 *
 * @param {number} width - Output video width in pixels.
 * @param {number} height - Output video height in pixels.
 * @param {number} fps - Output frame rate.
 * @param {string} codecName - Codec name (e.g. 'h264', 'h265', 'hevc').
 * @returns {number} Bitrate in Mbps.
 */
export function getDefaultBitrate(width, height, fps, codecName) {
  const pixels = Number(width || 0) * Number(height || 0)
  const isHevc = /h265|hevc|x265/i.test(codecName || '')
  const isHfr = Number(fps || 0) > 30
  const bin = BITRATE_BINS.find((item) => pixels <= item.maxPixels) ?? {
    ...BITRATE_FALLBACK,
    label: 'Fallback',
  }

  if (isHevc) return isHfr ? bin.h265Hfr : bin.h265
  return isHfr ? bin.h264Hfr : bin.h264
}
