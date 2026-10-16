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

// Matches `Fps::frame_count_for_duration` in ovrley_core: a fractional frame within
// 1/10,000 of a frame of the boundary is snapped down, any larger remainder adds a frame.
const FRAME_BOUNDARY_TOLERANCE = 1 / 10_000

/**
 * Counts the frames of a source duration with the native renderer's rule, so frame
 * indices of the program and the export agree.
 * @param {number} durationSeconds Source duration.
 * @param {{num: number, den: number}} fps Exact frame rate.
 * @returns {number} Frame count.
 */
export function frameCountForDuration(durationSeconds, fps) {
  const frames = (durationSeconds * fps.num) / fps.den
  const whole = Math.floor(frames)
  const remainder = frames - whole
  if (remainder === 0 || (whole > 0 && remainder <= FRAME_BOUNDARY_TOLERANCE)) return whole
  return whole + 1
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
