/**
 * Bitrate defaults in Mbps, keyed by max pixel count.
 * Each entry: { maxPixels, label, h264, h265, h264Hfr, h265Hfr }
 * Hfr = high frame rate (>30 fps). Values in Mbps.
 */

/** @type {Array<{maxPixels: number, label: string, h264: number, h265: number, h264Hfr: number, h265Hfr: number}>} */
export const BITRATE_BINS = [
  {
    maxPixels: 2_073_600,
    label: '1080p',
    h264: 10,
    h265: 8,
    h264Hfr: 15,
    h265Hfr: 12,
  },
  {
    maxPixels: 3_686_400,
    label: '1440p',
    h264: 30,
    h265: 20,
    h264Hfr: 45,
    h265Hfr: 30,
  },
  {
    maxPixels: 8_294_400,
    label: '4K',
    h264: 60,
    h265: 40,
    h264Hfr: 90,
    h265Hfr: 60,
  },
]

/** Fallback if resolution exceeds all bins. */
export const BITRATE_FALLBACK = {
  h264: 80,
  h265: 60,
  h264Hfr: 100,
  h265Hfr: 80,
}

/**
 * Returns the recommended output bitrate in Mbps for the given render settings.
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
