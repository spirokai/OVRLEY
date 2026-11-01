/**
 * The window a video start is expected in: from an hour before what it is
 * placed against (the activity, or the videos already imported) until an hour
 * after its end. The import review and the clip warnings share it.
 */

/** How far before the start or after the end of the reference a video may start. */
export const VIDEO_START_WINDOW_SECONDS = 3600

/**
 * Checks whether a video starts outside the window around a reference.
 * @param {number} offsetSeconds Video start on the timeline.
 * @param {{from: number, to: number}} reference Start and end of the reference on the timeline.
 * @returns {boolean} Whether the start is more than the window before `from` or after `to`.
 */
export function startsOutsideWindow(offsetSeconds, { from, to }) {
  return offsetSeconds < from - VIDEO_START_WINDOW_SECONDS || offsetSeconds > to + VIDEO_START_WINDOW_SECONDS
}
