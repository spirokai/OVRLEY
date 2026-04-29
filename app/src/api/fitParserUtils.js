/**
 * Implements API helpers for fit parser utils.
 */

import FitParser from 'fit-file-parser'
import { finalizeParsedActivity, safeNumber } from './activityParserUtils'

/**
 * Returns optional record value.
 *
 * @param {*} record - Value for record.
 * @param {*} keys - Lookup keys to inspect in priority order.
 * @returns {*} Requested value or structure.
 */
function getOptionalRecordValue(record, keys) {
  for (const key of keys) {
    if (record[key] !== undefined && record[key] !== null) {
      return record[key]
    }
  }

  return null
}

/**
 * Parses fit activity file.
 *
 * @param {*} file - File object being loaded or saved.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export default async function parseFitActivityFile(file) {
  const fitParser = new FitParser({
    elapsedRecordField: true,
    force: true,
    lengthUnit: 'm',
    mode: 'list',
    pressureUnit: 'bar',
    speedUnit: 'm/s',
    temperatureUnit: 'celsius',
  })

  const parsedFit = await fitParser.parseAsync(await file.arrayBuffer())
  const records = Array.isArray(parsedFit?.records) ? parsedFit.records : []

  if (!records.length) {
    throw new Error('The FIT file does not contain any record messages.')
  }

  const firstSession = Array.isArray(parsedFit?.sessions)
    ? parsedFit.sessions[0] || null
    : null
  const fileId = parsedFit?.file_id || null

  const rawSamples = records.map((record) => ({
    airPressure: safeNumber(record.absolute_pressure),
    altitude: safeNumber(record.enhanced_altitude ?? record.altitude),
    cadence: safeNumber(record.cadence),
    distance: safeNumber(record.distance),
    elapsedSeconds: safeNumber(
      getOptionalRecordValue(record, ['elapsed_time']),
    ),
    elevation: safeNumber(record.enhanced_altitude ?? record.altitude),
    gForce: safeNumber(getOptionalRecordValue(record, ['g_force', 'gforce'])),
    gradient: safeNumber(record.grade),
    groundContactTime: safeNumber(
      getOptionalRecordValue(record, ['ground_contact_time', 'stance_time']),
    ),
    heading: safeNumber(
      getOptionalRecordValue(record, [
        'gps_heading',
        'compass_heading',
        'heading',
        'course_heading',
        'navigation_heading',
      ]),
    ),
    heartrate: safeNumber(record.heart_rate),
    latitude: safeNumber(record.position_lat),
    leftRightBalance: record.left_right_balance ?? null,
    longitude: safeNumber(record.position_long),
    pace: safeNumber(record.pace),
    power: safeNumber(record.power),
    speed: safeNumber(record.enhanced_speed ?? record.speed),
    strideLength: safeNumber(
      getOptionalRecordValue(record, ['stride_length', 'step_length']),
    ),
    strokeRate: safeNumber(
      getOptionalRecordValue(record, ['stroke_rate', 'running_cadence']),
    ),
    temperature: safeNumber(record.temperature),
    timestamp: record.timestamp,
    torque: safeNumber(getOptionalRecordValue(record, ['torque'])),
    verticalOscillation: safeNumber(record.vertical_oscillation),
    verticalSpeed: safeNumber(record.vertical_speed),
  }))

  return finalizeParsedActivity({
    fileName: file.name,
    fileFormat: 'fit',
    metadata: {
      creator: fileId?.product_name || null,
      file_created_at: fileId?.time_created || null,
      source_device_manufacturer: fileId?.manufacturer || null,
      source_device_product: fileId?.product_name || null,
      sport: firstSession?.sport || null,
      sub_sport: firstSession?.sub_sport || null,
      total_elapsed_time: safeNumber(firstSession?.total_elapsed_time),
      total_timer_time: safeNumber(firstSession?.total_timer_time),
    },
    rawSamples,
  })
}
