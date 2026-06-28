/**
 * Activity file import pipeline — orchestrates GPX/FIT parsing,
 * cache update, store synchronization, and debug payload persistence.
 */

import * as backend from '@/api/backend'
import parseFitActivityFile from './fit-parser.js'
import { parseGpxActivityFile } from './gpx-parser.js'
import { parseSrtActivityFile } from './srt-parser.js'

/**
 * Handles sanitize debug filename.
 *
 * @param {*} filename - Target filename for the operation.
 * @returns {*} Result produced by the helper.
 */
function sanitizeDebugFilename(filename) {
  const normalizedBase = String(filename || 'activity')
    .replace(/\.[^/.]+$/, '')
    .replace(/[^a-zA-Z0-9._-]+/g, '_')
    .replace(/_+/g, '_')
    .replace(/^_+|_+$/g, '')

  return `${normalizedBase || 'activity'}-parse-debug.json`
}

/**
 * Handles persist debug payload.
 *
 * @param {*} filename - Target filename for the operation.
 * @param {*} payload - Structured payload produced by the helper.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
async function persistDebugPayload(filename, payload) {
  try {
    const debugFilename = sanitizeDebugFilename(filename)
    const contents = JSON.stringify(payload, null, 2)
    return await backend.writeParseDebugFile(debugFilename, contents)
  } catch (error) {
    console.warn('Failed to write parse debug file (non-fatal):', error)
    return null
  }
}

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
  return {
    parsedActivity: finalized.parsed_activity,
    debugPayload: finalized.debug_payload,
  }
}

/**
 * Synchronizes scene duration with activity.
 *
 * @param {*} durationSeconds - Numeric duration seconds value.
 * @param {*} storeState - Current store snapshot used for synchronization.
 * @returns {*} Result produced by the helper.
 */
function syncSceneDurationWithActivity(durationSeconds, storeState) {
  if (!Number.isFinite(durationSeconds) || durationSeconds <= 0) {
    console.warn('Parsed activity did not produce a duration value')
    return
  }

  const wholeSeconds = Math.floor(durationSeconds)
  const { config, setConfig, setDummyDurationSeconds, setEndSecond, setSelectedSecond, setStartSecond } = storeState

  console.log('Setting activity duration:', durationSeconds, 'seconds')
  setDummyDurationSeconds(wholeSeconds)
  setStartSecond(0)
  setEndSecond(wholeSeconds)
  setSelectedSecond(0)

  if (config) {
    setConfig({
      ...config,
      scene: {
        ...config.scene,
        start: 0,
        end: wholeSeconds,
      },
    })
  }
}

/**
 * Loads a parsed activity into store state.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.filename - Target filename for the operation.
 * @param {*} options.parsedActivity - Normalized activity payload used by the app.
 * @param {*} options.debugPayload - Value for debug payload.
 * @param {*} options.storeState - Current store snapshot used for synchronization.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
async function loadActivityIntoStore({ filename, parsedActivity, debugPayload, storeState }) {
  const { setActivitySummary, setActivityFilename, setParsedActivity } = storeState

  setActivityFilename(filename)
  setParsedActivity(parsedActivity)
  setActivitySummary(parsedActivity)
  if (debugPayload) {
    const debugPath = await persistDebugPayload(filename, debugPayload)
    console.log('Parse debug JSON written:', debugPath)
  }
  console.log('Activity filename set in store:', filename)

  syncSceneDurationWithActivity(parsedActivity?.metadata?.duration_seconds || 0, storeState)
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

  console.log('Starting activity processing:', {
    source: 'file',
    filename,
  })

  try {
    store.clearActivitySummary()

    const { parsedActivity, debugPayload } = await parseActivityFile(file)

    console.log('Frontend activity parse successful:', {
      durationSeconds: parsedActivity?.metadata?.duration_seconds,
      format: parsedActivity?.file_format,
      samples: parsedActivity?.metadata?.sample_count,
      validAttributes: parsedActivity?.valid_attributes,
    })

    await loadActivityIntoStore({
      filename,
      parsedActivity,
      debugPayload,
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
