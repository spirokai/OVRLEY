/**
 * FIT file parser — reads FIT binary format and converts to normalized raw samples.
 * Diving extraction is based on Unabara by Arnaud Dupuis (GPL-2.0):
 * https://github.com/arnauddupuis/unabara/blob/main/src/core/format_parsers/fit_parser.cpp
 */

import FitParser from 'fit-file-parser'
import { safeGearValue, safeNumber } from './raw-sample-utils.js'

const DIVE_SUB_SPORTS = new Set(['single_gas_diving', 'multi_gas_diving', 'gauge_diving', 'apnea_diving', 'apnea_hunting', 'ccr_diving'])
const BAR_PER_ATMOSPHERE = 1.01325

function extractDiveSamples(parsedFit, records, subSport) {
  const gases = new Map()
  for (const [index, gas] of (parsedFit.dive_gases ?? []).entries()) {
    if (gas.status === 'disabled') continue
    const oxygen = safeNumber(gas.oxygen_content)
    const helium = safeNumber(gas.helium_content)
    const validMix = oxygen !== null && helium !== null && oxygen >= 0 && helium >= 0 && oxygen + helium <= 100
    gases.set(gas.message_index?.value ?? index, {
      mode: gas.mode,
      fraction_o2: validMix ? oxygen / 100 : null,
      fraction_he: validMix ? helium / 100 : null,
    })
  }
  const initialGas = [...gases.values()].find((gas) =>
    subSport === 'ccr_diving' ? gas.mode === 'closed_circuit_diluent' : gas.mode !== 'closed_circuit_diluent',
  )
  let currentGas = initialGas
  const switches = (parsedFit.events ?? [])
    .filter((event) => event.event === 'dive_gas_switched' && Number.isFinite(event.timestamp?.getTime()))
    .sort((left, right) => left.timestamp - right.timestamp)
  let switchIndex = 0
  const gasSwitchSamples = []
  const state = {
    depth: null,
    first_stop_depth: null,
    first_stop_time: null,
    time_to_surface: null,
    average_ppo2: null,
    water_temperature: null,
  }
  const fields = [
    ['depth', 'depth', 1],
    ['next_stop_depth', 'first_stop_depth', 1],
    ['next_stop_time', 'first_stop_time', 60],
    ['time_to_surface', 'time_to_surface', 60],
    // FIT reports one current ppO2 value in bar. Setpoints are never substituted.
    ['po2', 'average_ppo2', BAR_PER_ATMOSPHERE],
    ['temperature', 'water_temperature', 1],
  ]
  const recordSamples = records.map((record) => {
    while (switchIndex < switches.length && switches[switchIndex].timestamp <= record.timestamp) {
      const event = switches[switchIndex]
      currentGas = gases.get(event.data)
      if (event.timestamp > records[0].timestamp && event.timestamp < record.timestamp && currentGas) {
        gasSwitchSamples.push({
          timestamp: event.timestamp,
          elapsed_seconds: (event.timestamp - records[0].timestamp) / 1000,
          fraction_o2: currentGas.fraction_o2,
          fraction_he: currentGas.fraction_he,
        })
      }
      switchIndex++
    }
    for (const [source, target, divisor] of fields) {
      if (record[source] === undefined) continue
      const value = safeNumber(record[source])
      state[target] = value === null ? null : value / divisor
    }
    return {
      ...state,
      fraction_o2: currentGas?.fraction_o2 ?? null,
      fraction_he: currentGas?.fraction_he ?? null,
    }
  })
  return { recordSamples, gasSwitchSamples }
}

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

function getOptionalRecordNumber(record, key) {
  const raw = record[key]
  if (raw === undefined || raw === null) return null

  const value = safeNumber(raw)
  if (value === null) throw new TypeError(`FIT record.${key} must be numeric when present`)
  return value
}

function getOptionalRecordNumberFromKeys(record, keys) {
  for (const key of keys) {
    if (record[key] !== undefined && record[key] !== null) return getOptionalRecordNumber(record, key)
  }

  return null
}

/** Missing timestamps leave lap timing unavailable rather than merging separate laps. */
function extractLapTiming(laps, firstRecordTimestamp) {
  if (!laps.length || firstRecordTimestamp == null || laps.some((lap) => lap.start_time == null)) return null

  const startTimes = laps.map((lap) => lap.start_time.getTime())
  if (startTimes.some((start, index) => !Number.isFinite(start) || (index > 0 && start <= startTimes[index - 1]))) {
    throw new Error('FIT lap start_time values must be valid timestamps in strictly increasing order.')
  }

  const firstRecordTime = firstRecordTimestamp.getTime()
  if (!Number.isFinite(firstRecordTime)) throw new Error('FIT lap timing requires a valid first record timestamp.')

  // Keep the recording at t=0 and shorten the lap in progress at the first record.
  // Earlier laps have no recorded samples and do not extend the activity.
  const firstObservedLapIndex = Math.max(
    0,
    startTimes.findLastIndex((start) => start <= firstRecordTime),
  )
  return {
    sync_time: firstRecordTimestamp.toISOString(),
    lap_markers: { BeaconMarkers: startTimes.slice(firstObservedLapIndex).map((start) => Math.max(0, (start - firstRecordTime) / 1000)) },
  }
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
  const lapTiming = extractLapTiming(parsedFit.laps ?? [], records[0].timestamp)
  const subSport = firstSession?.sub_sport ?? parsedFit.sports?.[0]?.sub_sport
  const isDive = DIVE_SUB_SPORTS.has(subSport) || records.some((record) => record.depth !== undefined)
  const diveSamples = isDive ? extractDiveSamples(parsedFit, records, subSport) : null

  const raw_samples = records.map((record, index) => ({
    air_pressure: safeNumber(record.absolute_pressure),
    barometric_altitude: getOptionalRecordNumberFromKeys(record, ['enhanced_altitude', 'altitude']),
    calories: getOptionalRecordNumber(record, 'calories'),
    cadence: safeNumber(record.cadence),
    core_temperature: safeNumber(record.core_temperature),
    distance: safeNumber(record.distance),
    elapsed_seconds: safeNumber(record.elapsed_time),
    elevation: safeNumber(record.enhanced_altitude ?? record.altitude),
    g_force: safeNumber(getOptionalRecordValue(record, ['g_force', 'gforce'])),
    gear_position: safeGearValue(getOptionalRecordValue(record, ['gear_ratio', 'gear', 'front_gear'])),
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
    ...diveSamples?.recordSamples[index],
  }))
  if (diveSamples) {
    raw_samples.push(...diveSamples.gasSwitchSamples)
    raw_samples.sort((left, right) => left.elapsed_seconds - right.elapsed_seconds)
  }

  return {
    file_name: file.name,
    file_format: 'fit',
    sync_time: lapTiming?.sync_time ?? null,
    lap_markers: lapTiming?.lap_markers ?? 'None',
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
      skip_idle_gap_fill: isDive,
      smoothing: {
        pace: { enabled: true, method: 'zero_phase_ma', window_seconds: 5.0 },
        heading: { enabled: true, method: 'circular_ema', window_seconds: 3.0 },
      },
    },
  }
}
