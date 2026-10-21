/**
 * Display label of a video format.
 */

/**
 * Formats a display resolution and exact frame rate, e.g. `1920×1080 · 29.97 fps`.
 * @param {{resolution: {width: number, height: number}, fps: {num: number, den: number}}} format Video format.
 * @param {import('i18next').TFunction} t Translation function.
 * @returns {string} Display label.
 */
export function formatVideoFormat({ resolution, fps }, t) {
  return t('video-preview.videoFormat', {
    defaultValue: '{{width}}×{{height}} · {{fps}} fps',
    width: resolution.width,
    height: resolution.height,
    fps: Math.round((fps.num / fps.den) * 100) / 100,
  })
}
