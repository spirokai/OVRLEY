/**
 * GPX parser - extracts browser-parsed track points into RawActivity.
 */

import { safeNumber } from './raw-sample-utils.js'

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

export function parseGpxActivityFile(file, textContent) {
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

  const raw_samples = trackPoints.map((trackPoint) => {
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
      air_pressure: readTrackPointMetric(extensionValues, ['air_pressure', 'absolute_pressure', 'pressure']),
      altitude: elevation,
      cadence: readTrackPointMetric(extensionValues, ['cad', 'cadence']),
      core_temperature: readTrackPointMetric(extensionValues, ['core_temperature', 'coretemp', 'core_temp']),
      distance: readTrackPointMetric(extensionValues, ['distance', 'distance_m', 'distancemeters']),
      elevation,
      g_force: readTrackPointMetric(extensionValues, ['g_force', 'gforce']),
      gear_position: readTrackPointMetric(extensionValues, ['gear_position', 'gear', 'gear_ratio']),
      gradient: readTrackPointMetric(extensionValues, ['gradient', 'grade', 'slope']),
      ground_contact_time: readTrackPointMetric(extensionValues, ['ground_contact_time', 'groundcontacttime', 'stance_time']),
      heading: readTrackPointMetric(extensionValues, ['heading', 'course', 'bearing', 'gps_heading']),
      heartrate: readTrackPointMetric(extensionValues, ['hr', 'heartrate', 'heart_rate']),
      latitude,
      left_right_balance: readTrackPointMetric(extensionValues, ['left_right_balance', 'leftrightbalance', 'balance']),
      longitude,
      pace: readTrackPointMetric(extensionValues, ['pace']),
      power: readTrackPointMetric(extensionValues, ['power', 'powerinwatts', 'watts']),
      speed: readTrackPointMetric(extensionValues, ['speed', 'enhanced_speed']),
      stride_length: readTrackPointMetric(extensionValues, ['stride_length', 'stridelength', 'step_length']),
      stroke_rate: readTrackPointMetric(extensionValues, ['stroke_rate', 'strokerate']),
      temperature: readTrackPointMetric(extensionValues, ['atemp', 'temperature', 'temp']),
      timestamp,
      torque: readTrackPointMetric(extensionValues, ['torque']),
      vertical_oscillation: readTrackPointMetric(extensionValues, ['vertical_oscillation', 'verticaloscillation']),
      vertical_speed: readTrackPointMetric(extensionValues, ['vertical_speed', 'verticalspeed', 'vam']),
    }
  })

  return {
    file_name: file.name,
    file_format: 'gpx',
    metadata: {
      activity_name: metadataName,
      creator: documentNode.documentElement?.getAttribute('creator') || null,
    },
    raw_samples,
    options: {
      skip_idle_gap_fill: false,
      smoothing: {
        heading: { enabled: true, method: 'circular_ema', window_seconds: 0.5 },
      },
    },
  }
}
