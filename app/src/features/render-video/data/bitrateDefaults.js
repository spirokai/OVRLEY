/**
 * Bitrate defaults in Mbps, keyed by max pixel count.
 * Each entry: { maxPixels, label, h264, h265, h264Hfr, h265Hfr }
 * Hfr = high frame rate (>30 fps). Values in Mbps.
 *
 * The companion function getDefaultBitrate lives in ../utils/bitrateDefaults.js.
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
