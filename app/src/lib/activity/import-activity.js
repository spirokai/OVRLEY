/**
 * Activity file import pipeline — orchestrates browser and native activity parsing,
 * cache update, and store synchronization.
 */

import * as backend from '@/api/backend'
import { getCourseWidgetDimensions } from '@/features/widget-editor/utils/widgetUtils'
import useStore from '@/store/useStore'
import { syncSceneTimingToConfig } from '@/store/store-utils'
import parseFitActivityFile from './fit-parser.js'
import { parseGpxActivityFile } from './gpx-parser.js'
import { parseIgcActivityFile } from './igc-parser.js'
import { parseSrtActivityFile } from './srt-parser.js'

/**
 * Parses activity file.
 *
 * @param {*} file - File object being loaded or saved.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function parseActivityFile(file) {
  const lowerName = file.name.toLowerCase()
  let rawActivity
  if (lowerName.endsWith('.fit')) rawActivity = await parseFitActivityFile(file)
  else if (lowerName.endsWith('.srt')) rawActivity = parseSrtActivityFile(await file.text(), file.name)
  else if (lowerName.endsWith('.gpx')) rawActivity = parseGpxActivityFile(file, await file.text())
  else if (lowerName.endsWith('.igc')) rawActivity = await parseIgcActivityFile(file)
  else throw new Error(`Unsupported activity file format: ${file.name}`)

  const finalized = await backend.finalizeActivity(rawActivity)
  return finalized.parsed_activity
}

export async function activateParsedActivity({ source, parsedActivity }, storeState) {
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

function filenameFromNativePath(path) {
  const filename = path.split(/[\\/]/).at(-1)
  if (!filename) throw new Error(`Invalid activity path: ${path}`)
  return filename
}

async function importAndActivateActivity(source, loadParsedActivity, store) {
  try {
    const parsedActivity = await loadParsedActivity()
    store.clearActivitySummary()
    await activateParsedActivity({ source, parsedActivity }, store)
    return parsedActivity
  } catch (error) {
    console.error('Activity parse error:', {
      message: error.message,
      stack: error.stack,
    })
    throw error
  }
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
  await activateParsedActivity(staged, storeActions)
  return staged.parsedActivity
}

/**
 * Imports a native CSV activity path through the Rust columnar pipeline.
 *
 * @param {string} path - Native path returned by the desktop file picker.
 * @param {object} storeActions - Injected store actions.
 * @returns {Promise<object>} Promise resolving to the activated parsed activity.
 */
export async function importCsvActivityPath(path, storeActions) {
  return importActivityPath(path, storeActions)
}

/**
 * Imports a native VBO activity path through the Rust columnar pipeline.
 *
 * @param {string} path - Native path returned by the desktop file picker.
 * @param {object} storeActions - Injected store actions.
 * @returns {Promise<object>} Promise resolving to the activated parsed activity.
 */
export async function importVboActivityPath(path, storeActions) {
  return importActivityPath(path, storeActions)
}

/**
 * Parses a native activity path without mutating application state.
 *
 * @param {string} path - Absolute native activity path.
 * @returns {Promise<{source: {kind: 'file', path: string}, parsedActivity: object}>} Staged activity.
 */
export async function parseActivityPath(path) {
  const filename = filenameFromNativePath(path)
  const lowerPath = path.toLowerCase()
  let parsedActivity
  if (lowerPath.endsWith('.csv')) parsedActivity = (await backend.parseCsvActivity(path)).parsed_activity
  else if (lowerPath.endsWith('.vbo')) parsedActivity = (await backend.parseVboActivity(path)).parsed_activity
  else {
    const file = await (await import('@/lib/file-dialog')).fileFromSelectedPath(path, 'activity')
    parsedActivity = await parseActivityFile(file)
  }
  return { source: { kind: 'file', path }, parsedActivity, filename }
}

/**
 * Handles save file.
 *
 * @param {File} file - Browser File containing a GPX, FIT, SRT, or IGC activity.
 * @param {object} storeActions - Injected store actions.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export default async function importActivityFile(file, storeActions) {
  if (!(file instanceof File)) throw new Error('Activity import requires a browser File object.')
  return importAndActivateActivity(null, () => parseActivityFile(file), storeActions)
}
