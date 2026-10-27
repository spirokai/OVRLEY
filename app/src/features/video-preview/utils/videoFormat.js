/**
 * Display label of a video's format and length.
 */

import { formatSecondsClock } from '@/lib/time-format'

/**
 * Formats a display resolution, exact frame rate, and duration, e.g. `1920×1080 · 29.97 fps · 05:30.5`.
 * @param {{resolution: {width: number, height: number}, fps: {num: number, den: number}, durationSeconds: number}} video Video format and length.
 * @param {import('i18next').TFunction} t Translation function.
 * @returns {string} Display label.
 */
export function formatVideoDetails({ resolution, fps, durationSeconds }, t) {
  return t('video-preview.videoDetails', {
    defaultValue: '{{width}}×{{height}} · {{fps}} fps · {{duration}}',
    width: resolution.width,
    height: resolution.height,
    fps: Math.round((fps.num / fps.den) * 100) / 100,
    duration: formatSecondsClock(durationSeconds),
  })
}
