/**
 * Activity file import pipeline — orchestrates GPX/FIT parsing,
 * cache update, and store synchronization.
 */

import * as backend from '@/api/backend'
import { getCourseWidgetDimensions } from '@/features/widget-editor/utils/widgetUtils'
import useStore from '@/store/useStore'
import { syncSceneTimingToConfig } from '@/store/store-utils'
import parseFitActivityFile from './fit-parser.js'
import { parseGpxActivityFile } from './gpx-parser.js'
import { parseSrtActivityFile } from './srt-parser.js'

/**
 * Parses activity file.
 *
 * @param {*} file - File object being loaded or saved.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
async function parseActivityFile(file) {
  const lowerName = file.name.toLowerCase()
  let rawActivity
  if (lowerName.endsWith('.fit')) rawActivity = await parseFitActivityFile(file)
  else if (lowerName.endsWith('.srt')) rawActivity = parseSrtActivityFile(await file.text(), file.name)
  else if (lowerName.endsWith('.gpx')) rawActivity = parseGpxActivityFile(file, await file.text())
  else throw new Error(`Unsupported activity file format: ${file.name}`)

  const finalized = await backend.finalizeActivity(rawActivity)
  return finalized.parsed_activity
}

async function loadActivityIntoStore({ filename, parsedActivity, storeState }) {
  const { setActivityFilename, activateActivityFile } = storeState

  setActivityFilename(filename)
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
        const dims = getCourseWidgetDimensions(coursePoints)
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

/**
 * Handles ensure file object.
 *
 * @param {*} fileOrPath - File object or path pointing to an activity file.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
async function ensureFileObject(fileOrPath) {
  if (fileOrPath instanceof File) return fileOrPath

  throw new Error('Activity import now requires a browser File object. Path-based imports are not supported in this phase.')
}

/**
 * Handles save file.
 *
 * @param {*} fileOrPath - File object or path pointing to an activity file.
 * @param {object} storeActions - Injected store actions.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export default async function importActivityFile(fileOrPath, storeActions) {
  const file = await ensureFileObject(fileOrPath)
  const filename = file.name
  const store = storeActions

  try {
    store.clearActivitySummary()

    const parsedActivity = await parseActivityFile(file)

    await loadActivityIntoStore({
      filename,
      parsedActivity,
      storeState: store,
    })

    return parsedActivity
  } catch (error) {
    console.error('Activity parse error:', {
      message: error.message,
      stack: error.stack,
    })
    throw error
  }
}
