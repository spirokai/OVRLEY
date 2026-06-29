/**
 * FIT file parser — reads FIT binary format and converts to normalized raw samples.
 */

import FitParser from 'fit-file-parser'
import { safeNumber } from './raw-sample-utils.js'

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

  const firstSession = Array.isArray(parsedFit?.sessions) ? parsedFit.sessions[0] || null : null
  const fileId = parsedFit?.file_id || null

  const raw_samples = records.map((record) => ({
    air_pressure: safeNumber(record.absolute_pressure),
    altitude: safeNumber(record.enhanced_altitude ?? record.altitude),
    cadence: safeNumber(record.cadence),
    core_temperature: safeNumber(record.core_temperature),
    distance: safeNumber(record.distance),
    elapsed_seconds: safeNumber(getOptionalRecordValue(record, ['elapsed_time'])),
    elevation: safeNumber(record.enhanced_altitude ?? record.altitude),
    g_force: safeNumber(getOptionalRecordValue(record, ['g_force', 'gforce'])),
    gear_position: safeNumber(getOptionalRecordValue(record, ['gear_ratio', 'gear', 'front_gear'])),
    gradient: safeNumber(record.grade),
    ground_contact_time: safeNumber(getOptionalRecordValue(record, ['ground_contact_time', 'stance_time'])),
    heading: safeNumber(getOptionalRecordValue(record, ['gps_heading', 'compass_heading', 'heading', 'course_heading', 'navigation_heading'])),
    heartrate: safeNumber(record.heart_rate),
    latitude: safeNumber(record.position_lat),
    left_right_balance: (() => {
      const raw = getOptionalRecordValue(record, ['left_right_balance'])
      return raw !== null && typeof raw === 'object' ? safeNumber(raw.value) : safeNumber(raw)
    })(),
    longitude: safeNumber(record.position_long),
    pace: safeNumber(record.pace),
    power: safeNumber(record.power),
    speed: safeNumber(record.enhanced_speed ?? record.speed),
    stride_length: safeNumber(getOptionalRecordValue(record, ['stride_length', 'step_length'])),
    stroke_rate: safeNumber(getOptionalRecordValue(record, ['stroke_rate', 'running_cadence'])),
    temperature: safeNumber(record.temperature),
    timestamp: record.timestamp,
    torque: safeNumber(getOptionalRecordValue(record, ['torque'])),
    vertical_oscillation: safeNumber(record.vertical_oscillation),
    vertical_speed: safeNumber(record.vertical_speed),
  }))

  return {
    file_name: file.name,
    file_format: 'fit',
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
    raw_samples,
    options: {
      skip_idle_gap_fill: false,
      smoothing: {
        heading: { enabled: true, method: 'circular_ema', window_seconds: 0.5 },
      },
    },
  }
}
