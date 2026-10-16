/**
 * Presentation model for the result of a video import.
 */

import { filenameFromSelectedPath } from '@/lib/utils'

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

/**
 * Builds the dialog sections for an import report.
 * @param {{failures: Array<{path: string, message: string}>, duplicates: Array<string>, reference: object|null, incompatibleVideos: Array<object>}} report Import report.
 * @param {import('i18next').TFunction} t Translation function.
 * @returns {Array<{id: string, title: string, description: string|null, items: Array<{key: string, name: string, detail: string|null}>}>} Non-empty sections.
 */
export function buildVideoImportReportSections(report, t) {
  const sections = [
    {
      id: 'failures',
      title: t('video-preview.importFailedFiles', 'Not imported'),
      description: null,
      items: report.failures.map(({ path, message }) => ({ key: path, name: filenameFromSelectedPath(path), detail: message })),
    },
    {
      id: 'duplicates',
      title: t('video-preview.importDuplicateFiles', 'Already imported'),
      description: null,
      items: report.duplicates.map((path) => ({ key: path, name: filenameFromSelectedPath(path), detail: null })),
    },
    {
      id: 'incompatible',
      title: t('video-preview.importIncompatibleVideos', 'Different resolution or frame rate'),
      description:
        report.reference === null
          ? null
          : t('video-preview.importIncompatibleDescription', {
              defaultValue:
                'These videos differ from the other videos ({{format}}). They can be exported as separate files only; remove them to export one combined file.',
              format: formatVideoFormat(report.reference, t),
            }),
      items: report.incompatibleVideos.map((video) => ({
        key: video.id,
        name: filenameFromSelectedPath(video.path),
        detail: formatVideoFormat(video, t),
      })),
    },
  ]
  return sections.filter((section) => section.items.length > 0)
}
