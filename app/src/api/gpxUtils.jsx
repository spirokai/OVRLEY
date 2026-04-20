import useStore from '../store/useStore'
import {
  clearCurrentActivityCache,
  setCurrentActivityCache,
} from './activityCache'
import * as backend from './backend'
import { finalizeParsedActivity, safeNumber } from './activityParserUtils'
import parseFitActivityFile from './fitParserUtils'

function sanitizeDebugFilename(filename) {
  const normalizedBase = String(filename || 'activity')
    .replace(/\.[^/.]+$/, '')
    .replace(/[^a-zA-Z0-9._-]+/g, '_')
    .replace(/_+/g, '_')
    .replace(/^_+|_+$/g, '')

  return `${normalizedBase || 'activity'}-parse-debug.json`
}

async function persistDebugPayload(filename, payload) {
  const debugFilename = sanitizeDebugFilename(filename)
  const contents = JSON.stringify(payload, null, 2)
  return backend.writeParseDebugFile(debugFilename, contents)
}

function normalizeExtensionKey(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '')
}

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

function parseGpxActivityFile(file, textContent) {
  const parser = new DOMParser()
  const documentNode = parser.parseFromString(textContent, 'application/xml')
  const parseError = documentNode.querySelector('parsererror')
  if (parseError) {
    throw new Error('The GPX file could not be parsed.')
  }

  const trackPoints = Array.from(
    documentNode.getElementsByTagNameNS('*', 'trkpt'),
  )
  if (!trackPoints.length) {
    throw new Error('The GPX file does not contain any track points.')
  }

  const metadataNode =
    documentNode.getElementsByTagNameNS('*', 'metadata')[0] || null
  const trackNode = documentNode.getElementsByTagNameNS('*', 'trk')[0] || null
  const metadataName =
    metadataNode?.getElementsByTagNameNS('*', 'name')[0]?.textContent?.trim() ||
    trackNode?.getElementsByTagNameNS('*', 'name')[0]?.textContent?.trim() ||
    null

  const rawSamples = trackPoints.map((trackPoint) => {
    const latitude = safeNumber(trackPoint.getAttribute('lat'))
    const longitude = safeNumber(trackPoint.getAttribute('lon'))
    const elevation = safeNumber(
      trackPoint.getElementsByTagNameNS('*', 'ele')[0]?.textContent,
    )
    const timestamp =
      trackPoint.getElementsByTagNameNS('*', 'time')[0]?.textContent?.trim() ||
      null
    const extensionValues = {}
    const extensionsNode = trackPoint.getElementsByTagNameNS(
      '*',
      'extensions',
    )[0]
    if (extensionsNode) {
      Array.from(extensionsNode.children || []).forEach((child) => {
        collectLeafExtensionValues(child, extensionValues)
      })
    }

    return {
      airPressure: readTrackPointMetric(extensionValues, [
        'air_pressure',
        'absolute_pressure',
        'pressure',
      ]),
      altitude: elevation,
      cadence: readTrackPointMetric(extensionValues, ['cad', 'cadence']),
      distance: readTrackPointMetric(extensionValues, [
        'distance',
        'distance_m',
        'distancemeters',
      ]),
      elevation,
      gForce: readTrackPointMetric(extensionValues, ['g_force', 'gforce']),
      gradient: readTrackPointMetric(extensionValues, [
        'gradient',
        'grade',
        'slope',
      ]),
      groundContactTime: readTrackPointMetric(extensionValues, [
        'ground_contact_time',
        'groundcontacttime',
        'stance_time',
      ]),
      heading: readTrackPointMetric(extensionValues, [
        'heading',
        'course',
        'bearing',
        'gps_heading',
      ]),
      heartrate: readTrackPointMetric(extensionValues, [
        'hr',
        'heartrate',
        'heart_rate',
      ]),
      latitude,
      leftRightBalance: readTrackPointMetric(extensionValues, [
        'left_right_balance',
        'leftrightbalance',
        'balance',
      ]),
      longitude,
      pace: readTrackPointMetric(extensionValues, ['pace']),
      power: readTrackPointMetric(extensionValues, [
        'power',
        'powerinwatts',
        'watts',
      ]),
      speed: readTrackPointMetric(extensionValues, ['speed', 'enhanced_speed']),
      strideLength: readTrackPointMetric(extensionValues, [
        'stride_length',
        'stridelength',
        'step_length',
      ]),
      strokeRate: readTrackPointMetric(extensionValues, [
        'stroke_rate',
        'strokerate',
      ]),
      temperature: readTrackPointMetric(extensionValues, [
        'atemp',
        'temperature',
        'temp',
      ]),
      timestamp,
      torque: readTrackPointMetric(extensionValues, ['torque']),
      verticalOscillation: readTrackPointMetric(extensionValues, [
        'vertical_oscillation',
        'verticaloscillation',
      ]),
      verticalSpeed: readTrackPointMetric(extensionValues, [
        'vertical_speed',
        'verticalspeed',
        'vam',
      ]),
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

async function ensureFileObject(fileOrPath) {
  if (fileOrPath instanceof File) return fileOrPath

  throw new Error(
    'Activity import now requires a browser File object. Path-based imports are not supported in this phase.',
  )
}

export default async function saveFile(fileOrPath) {
  const file = await ensureFileObject(fileOrPath)
  const filename = file.name
  const lowerFilename = filename.toLowerCase()

  console.log('📤 Starting GPX processing:', {
    source: 'file',
    filename,
  })

  try {
    const {
      clearActivitySummary,
      setActivitySummary,
      setDummyDurationSeconds,
      setEndSecond,
      setGpxFilename,
      setStartSecond,
      setSelectedSecond,
    } = useStore.getState()

    clearCurrentActivityCache()
    clearActivitySummary()

    const parseResult = lowerFilename.endsWith('.fit')
      ? await parseFitActivityFile(file)
      : await parseGpxActivityFile(file, await file.text())

    const { parsedActivity, debugPayload } = parseResult

    console.log('✅ Frontend activity parse successful:', {
      durationSeconds: parsedActivity?.metadata?.duration_seconds,
      format: parsedActivity?.file_format,
      samples: parsedActivity?.metadata?.sample_count,
      validAttributes: parsedActivity?.valid_attributes,
    })

    setGpxFilename(filename)
    setCurrentActivityCache(parsedActivity, debugPayload)
    setActivitySummary(parsedActivity)
    const debugPath = await persistDebugPayload(filename, debugPayload)
    console.log('✅ Parse debug JSON written:', debugPath)
    console.log('✅ Activity filename set in store:', filename)

    const durationSeconds = parsedActivity?.metadata?.duration_seconds || 0
    if (durationSeconds > 0) {
      console.log('✅ Setting activity duration:', durationSeconds, 'seconds')
      setDummyDurationSeconds(Math.floor(durationSeconds))
      setStartSecond(0)
      setEndSecond(Math.floor(durationSeconds))
      setSelectedSecond(0)
    } else {
      console.warn('⚠️ Parsed activity did not produce a duration value')
    }

    const { config, setConfig } = useStore.getState()
    if (config && durationSeconds > 0) {
      setConfig({
        ...config,
        scene: {
          ...config.scene,
          start: 0,
          end: Math.floor(durationSeconds),
        },
      })
    }

    return parsedActivity
  } catch (error) {
    console.error('❌ Activity parse error:', {
      message: error.message,
      stack: error.stack,
    })
    throw error
  }
}
