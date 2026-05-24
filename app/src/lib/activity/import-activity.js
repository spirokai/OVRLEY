/**
 * Activity file import pipeline — orchestrates GPX/FIT parsing,
 * cache update, store synchronization, and debug payload persistence.
 */

import useStore from '@/store/useStore'
import { clearCurrentActivityCache, setCurrentActivityCache } from '@/lib/activity/cache'
import * as backend from '@/api/backend'
import { finalizeParsedActivity } from './parser.js'
import { safeNumber } from './parse-helpers.js'
import parseFitActivityFile from './fit-parser.js'

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
  const debugFilename = sanitizeDebugFilename(filename)
  const contents = JSON.stringify(payload, null, 2)
  return backend.writeParseDebugFile(debugFilename, contents)
}

/**
 * Normalizes extension key.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {*} Derived data structure for downstream use.
 */
function normalizeExtensionKey(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '')
}

/**
 * Handles collect leaf extension values.
 *
 * @param {*} element - Value for element.
 * @param {*} target - Target object, element, or value being updated.
 * @returns {*} Result produced by the helper.
 */
function collectLeafExtensionValues(element, target) {
  const childElements = Array.from(element.children || [])
  if (!childElements.length) {
    const key = normalizeExtensionKey(element.localName)
    const value = element.textContent?.trim()
    if (key && value) {
      target[key] = value
    }
    return
  }

  childElements.forEach((child) => collectLeafExtensionValues(child, target))
}

/**
 * Reads track point metric.
 *
 * @param {*} extensionValues - Value for extension values.
 * @param {*} aliases - Alternate metric keys to inspect.
 * @returns {*} Requested value or structure.
 */
function readTrackPointMetric(extensionValues, aliases) {
  for (const alias of aliases) {
    const normalizedAlias = normalizeExtensionKey(alias)
    if (!(normalizedAlias in extensionValues)) continue

    const numericValue = safeNumber(extensionValues[normalizedAlias])
    if (numericValue !== null) {
      return numericValue
    }
  }

  return null
}

/**
 * Parses gpx activity file.
 *
 * @param {*} file - File object being loaded or saved.
 * @param {*} textContent - Value for text content.
 * @returns {object} Result produced by the helper.
 */
function parseGpxActivityFile(file, textContent) {
  const parser = new DOMParser()
  const documentNode = parser.parseFromString(textContent, 'application/xml')
  const parseError = documentNode.querySelector('parsererror')
  if (parseError) {
    throw new Error('The GPX file could not be parsed.')
  }

  const trackPoints = Array.from(documentNode.getElementsByTagNameNS('*', 'trkpt'))
  if (!trackPoints.length) {
    throw new Error('The GPX file does not contain any track points.')
  }

  const metadataNode = documentNode.getElementsByTagNameNS('*', 'metadata')[0] || null
  const trackNode = documentNode.getElementsByTagNameNS('*', 'trk')[0] || null
  const metadataName =
    metadataNode?.getElementsByTagNameNS('*', 'name')[0]?.textContent?.trim() ||
    trackNode?.getElementsByTagNameNS('*', 'name')[0]?.textContent?.trim() ||
    null

  const rawSamples = trackPoints.map((trackPoint) => {
    const latitude = safeNumber(trackPoint.getAttribute('lat'))
    const longitude = safeNumber(trackPoint.getAttribute('lon'))
    const elevation = safeNumber(trackPoint.getElementsByTagNameNS('*', 'ele')[0]?.textContent)
    const timestamp = trackPoint.getElementsByTagNameNS('*', 'time')[0]?.textContent?.trim() || null
    const extensionValues = {}
    const extensionsNode = trackPoint.getElementsByTagNameNS('*', 'extensions')[0]
    if (extensionsNode) {
      Array.from(extensionsNode.children || []).forEach((child) => {
        collectLeafExtensionValues(child, extensionValues)
      })
    }

    return {
      airPressure: readTrackPointMetric(extensionValues, ['air_pressure', 'absolute_pressure', 'pressure']),
      altitude: elevation,
      cadence: readTrackPointMetric(extensionValues, ['cad', 'cadence']),
      distance: readTrackPointMetric(extensionValues, ['distance', 'distance_m', 'distancemeters']),
      elevation,
      gForce: readTrackPointMetric(extensionValues, ['g_force', 'gforce']),
      gearPosition: readTrackPointMetric(extensionValues, ['gear_position', 'gear', 'gear_ratio']),
      gradient: readTrackPointMetric(extensionValues, ['gradient', 'grade', 'slope']),
      groundContactTime: readTrackPointMetric(extensionValues, ['ground_contact_time', 'groundcontacttime', 'stance_time']),
      heading: readTrackPointMetric(extensionValues, ['heading', 'course', 'bearing', 'gps_heading']),
      heartrate: readTrackPointMetric(extensionValues, ['hr', 'heartrate', 'heart_rate']),
      latitude,
      leftRightBalance: readTrackPointMetric(extensionValues, ['left_right_balance', 'leftrightbalance', 'balance']),
      longitude,
      pace: readTrackPointMetric(extensionValues, ['pace']),
      power: readTrackPointMetric(extensionValues, ['power', 'powerinwatts', 'watts']),
      speed: readTrackPointMetric(extensionValues, ['speed', 'enhanced_speed']),
      strideLength: readTrackPointMetric(extensionValues, ['stride_length', 'stridelength', 'step_length']),
      strokeRate: readTrackPointMetric(extensionValues, ['stroke_rate', 'strokerate']),
      temperature: readTrackPointMetric(extensionValues, ['atemp', 'temperature', 'temp']),
      timestamp,
      torque: readTrackPointMetric(extensionValues, ['torque']),
      verticalOscillation: readTrackPointMetric(extensionValues, ['vertical_oscillation', 'verticaloscillation']),
      verticalSpeed: readTrackPointMetric(extensionValues, ['vertical_speed', 'verticalspeed', 'vam']),
    }
  })

  return finalizeParsedActivity({
    fileName: file.name,
    fileFormat: 'gpx',
    metadata: {
      activity_name: metadataName,
      creator: documentNode.documentElement?.getAttribute('creator') || null,
    },
    rawSamples,
    options: {
      useLegacyGpxDerivations: true,
    },
  })
}

/**
 * Parses activity file.
 *
 * @param {*} file - File object being loaded or saved.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
async function parseActivityFile(file) {
  return file.name.toLowerCase().endsWith('.fit') ? parseFitActivityFile(file) : parseGpxActivityFile(file, await file.text())
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
 * Applies parsed activity to store.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.filename - Target filename for the operation.
 * @param {*} options.parsedActivity - Normalized activity payload used by the app.
 * @param {*} options.debugPayload - Value for debug payload.
 * @param {*} options.storeState - Current store snapshot used for synchronization.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
async function applyParsedActivityToStore({ filename, parsedActivity, debugPayload, storeState }) {
  const { setActivitySummary, setGpxFilename } = storeState

  setGpxFilename(filename)
  setCurrentActivityCache(parsedActivity)
  setActivitySummary(parsedActivity)
  const debugPath = await persistDebugPayload(filename, debugPayload)
  console.log('Parse debug JSON written:', debugPath)
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
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export default async function saveFile(fileOrPath) {
  const file = await ensureFileObject(fileOrPath)
  const filename = file.name

  console.log('Starting activity processing:', {
    source: 'file',
    filename,
  })

  try {
    const storeState = useStore.getState()
    const { clearActivitySummary } = storeState

    clearCurrentActivityCache()
    clearActivitySummary()

    const { parsedActivity, debugPayload } = await parseActivityFile(file)

    console.log('Frontend activity parse successful:', {
      durationSeconds: parsedActivity?.metadata?.duration_seconds,
      format: parsedActivity?.file_format,
      samples: parsedActivity?.metadata?.sample_count,
      validAttributes: parsedActivity?.valid_attributes,
    })

    await applyParsedActivityToStore({
      filename,
      parsedActivity,
      debugPayload,
      storeState,
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
