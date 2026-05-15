/**
 * Pure helper functions for render video configuration — FPS resolution,
 * bitrate formatting, and codec classification.
 * No React, no side effects.
 */

/**
 * Reduces a rational FPS pair using GCD.
 *
 * @param {number} num - FPS numerator.
 * @param {number} den - FPS denominator.
 * @returns {{num:number, den:number}} Reduced FPS pair.
 */
function reduceFps(num, den) {
  let a = Math.abs(num)
  let b = Math.abs(den)
  while (b !== 0) {
    const next = a % b
    a = b
    b = next
  }
  const gcd = Math.max(a, 1)
  return { num: num / gcd, den: den / gcd }
}

/**
 * Resolves imported-video FPS metadata to the rational fields expected by Rust.
 *
 * @param {*} fpsNum - Exact FPS numerator from ffprobe, when available.
 * @param {*} fpsDen - Exact FPS denominator from ffprobe, when available.
 * @param {*} fps - Floating FPS fallback from older metadata.
 * @returns {{num:number, den:number}|null} Reduced rational FPS or null.
 */
export function resolveCompositeFps(fpsNum, fpsDen, fps) {
  const num = Number(fpsNum)
  const den = Number(fpsDen)
  if (Number.isInteger(num) && num > 0 && Number.isInteger(den) && den > 0) {
    return reduceFps(num, den)
  }

  const value = Number(fps)
  if (!Number.isFinite(value) || value <= 0) {
    return null
  }

  const commonRates = [
    [23.976, 24000, 1001],
    [29.97, 30000, 1001],
    [59.94, 60000, 1001],
    [25, 25, 1],
    [30, 30, 1],
    [60, 60, 1],
  ]
  const match = commonRates.find(([approx]) => Math.abs(value - approx) <= 0.001)
  if (match) {
    return { num: match[1], den: match[2] }
  }

  return reduceFps(Math.round(value * 1000), 1000)
}

/**
 * Formats dialog bitrate values for FFmpeg's `-b:v` argument.
 *
 * @param {*} value - Numeric Mbps value or already formatted FFmpeg bitrate.
 * @returns {string} FFmpeg bitrate string.
 */
export function formatCompositeBitrate(value) {
  const bitrate = Number(value)
  if (Number.isFinite(bitrate) && bitrate > 0) {
    return `${bitrate}M`
  }
  if (typeof value === 'string' && value.trim()) {
    return value.trim()
  }
  return '20M'
}

/**
 * Returns whether a codec is valid for MP4 compositing output.
 *
 * @param {*} codec - Candidate FFmpeg codec name.
 * @returns {boolean} Whether the codec belongs to the composite MP4 path.
 */
export function isCompositeCodec(codec) {
  return [
    'libx264',
    'libx265',
    'h264_nvenc',
    'hevc_nvenc',
    'nnvgpu_h264',
    'nnvgpu_hevc',
    'h264_qsv',
    'hevc_qsv',
    'qsv_full_h264',
    'qsv_full_hevc',
    'h264_amf',
    'hevc_amf',
    'h264_videotoolbox',
    'hevc_videotoolbox',
    'h264_vaapi',
    'hevc_vaapi',
  ].includes(codec)
}

/**
 * Returns whether a codec value names an experimental full-QSV profile.
 *
 * @param {*} codec - Candidate codec or profile name.
 * @returns {boolean} Whether the value requires detected QSV filter init args.
 */
export function isQsvFullCodec(codec) {
  return ['qsv_full_h264', 'qsv_full_hevc'].includes(codec)
}
