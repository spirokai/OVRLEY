/**
 * Exact rational frame-grid conversions for imported source videos.
 */

/**
 * Converts seconds to the nearest frame index on an exact rational frame grid.
 * @param {number} seconds Time in seconds.
 * @param {{num: number, den: number}} fps Exact frame rate.
 * @returns {number} Nearest frame index.
 */
export function secondsToFrames(seconds, fps) {
  return Math.round((seconds * fps.num) / fps.den)
}

/**
 * Converts a frame index to seconds on an exact rational frame grid.
 * @param {number} frames Frame index or frame count.
 * @param {{num: number, den: number}} fps Exact frame rate.
 * @returns {number} Time in seconds.
 */
export function framesToSeconds(frames, fps) {
  return (frames * fps.den) / fps.num
}

/**
 * Reports whether two exact frame rates describe the same rate.
 * @param {{num: number, den: number}} left Exact frame rate.
 * @param {{num: number, den: number}} right Exact frame rate.
 * @returns {boolean} Whether both rates are equal.
 */
export function fpsEquals(left, right) {
  return left.num * right.den === right.num * left.den
}
