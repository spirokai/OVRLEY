/**
 * Activity import pipeline: dispatches native activity paths to their Rust parsers
 * and synchronizes the store with the result.
 */

import * as backend from '@/api/backend'
import { getCourseWidgetDimensions } from '@/lib/widget/widget-defaults'
import { WIDGET_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'
import useStore from '@/store/useStore'
import { syncSceneTimingToConfig } from '@/store/store-utils'

const PARSERS_BY_EXTENSION = {
  csv: backend.parseCsvActivity,
  fit: backend.parseFitActivity,
  gpx: backend.parseGpxActivity,
  igc: backend.parseIgcActivity,
  srt: backend.parseSrtActivity,
  ssrf: backend.parseSubsurfaceActivity,
  tcx: backend.parseTcxActivity,
  uddf: backend.parseUddfActivity,
  vbo: backend.parseVboActivity,
  xml: backend.parseSubsurfaceActivity,
}

export function activateParsedActivity({ source, parsedActivity }, storeState) {
  const { setActivitySource, activateActivityFile } = storeState

  setActivitySource(source)
  activateActivityFile(parsedActivity)

  const durationSeconds = Number(parsedActivity?.metadata?.duration_seconds || 0)
  if (Number.isFinite(durationSeconds) && durationSeconds > 0) {
    const wholeSeconds = Math.floor(durationSeconds)
    storeState.setFallbackDurationSeconds(wholeSeconds)
    storeState.setStartSecond(0)
    storeState.setEndSecond(wholeSeconds)
    storeState.setSelectedSecond(0)

    useStore.setState((state) => {
      syncSceneTimingToConfig(state, { startSecond: 0, endSecond: wholeSeconds })

      const coursePoints = parsedActivity?.sample_course_points
      if (coursePoints && state.config?.plots) {
        const dims = getCourseWidgetDimensions(coursePoints, WIDGET_TYPE_DEFINITIONS.course.courseSizing)
        if (dims) {
          for (const plot of state.config.plots) {
            if (plot.value === 'course') {
              plot.width = dims.width
              plot.height = dims.height
            }
          }
        }
      }
    })
  }
}

function filenameFromNativePath(path) {
  const filename = path.split(/[\\/]/).at(-1)
  if (!filename) throw new Error(`Invalid activity path: ${path}`)
  return filename
}

/**
 * Runs the canonical native activity load operation for any supported format.
 * @param {string} path Absolute native activity path.
 * @param {object} storeActions Activity store owner actions.
 * @returns {Promise<object>} Activated parsed activity.
 */
export async function importActivityPath(path, storeActions) {
  const staged = await parseActivityPath(path)
  storeActions.clearActivitySummary()
  activateParsedActivity(staged, storeActions)
  return staged.parsedActivity
}

/**
 * Parses a native activity path without mutating application state.
 *
 * @param {string} path - Absolute native activity path.
 * @returns {Promise<{source: {kind: 'file', path: string}, parsedActivity: object, filename: string}>} Staged activity.
 */
export async function parseActivityPath(path) {
  const filename = filenameFromNativePath(path)
  const parse = PARSERS_BY_EXTENSION[filename.split('.').at(-1).toLowerCase()]
  if (!parse) throw new Error(`Unsupported activity file format: ${filename}`)
  const { parsed_activity: parsedActivity } = await parse(path)
  return { source: { kind: 'file', path }, parsedActivity, filename }
}
