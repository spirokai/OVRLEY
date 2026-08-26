/**
 * IGC flight-log adapter.
 *
 * This module is the format-specific edge of the activity import path for
 * `.igc` files. It uses `igc-parser` to read flight-recorder text, then emits
 * the shared RawActivity contract consumed by the Rust finalizer. The backend
 * stays format-agnostic: timestamps, units, metadata, and optional IGC record
 * extensions are normalized here before finalization.
 *
 * Mapping summary:
 * - B-record GPS fixes become one RawSample each.
 * - GPS altitude maps to `elevation`; pressure altitude is not emitted because
 *   IGC is not currently a documented barometric source for the shared schema.
 * - Invalid V-flag fixes keep their timestamp slot but clear position and
 *   altitude values so the backend can preserve the source timeline.
 * - Optional IGC extension fields are promoted when present:
 *   GSP centi-km/h -> `speed` m/s, TRT degrees -> `heading`,
 *   VAT centi-m/s -> m/s, and OAT -> `temperature` using a file-level
 *   scale inference for whole/deci/centi-degree vendor variants.
 */

import IGCParser from 'igc-parser'
import { safeNumber } from './raw-sample-utils.js'

/**
 * Normalizes non-finite values from conversion callbacks to RawSample nulls.
 *
 * @param {number|null} value - Numeric value after parsing/conversion.
 * @returns {number|null} Finite number or null.
 */
function finiteOrNull(value) {
  return Number.isFinite(value) ? value : null
}

/**
 * Reads a numeric extension value and applies a unit divisor.
 *
 * @param {*} value - Raw extension field value from igc-parser.
 * @param {number} divisor - Unit conversion divisor.
 * @returns {number|null} Converted numeric value or null.
 */
function scaledNumber(value, divisor) {
  const numeric = safeNumber(value)
  return numeric === null ? null : numeric / divisor
}

/**
 * Extracts a value from possible extension entry shapes.
 *
 * Current igc-parser output exposes extensions as an object keyed by code, but
 * this helper also accepts tuple/object entries to keep the adapter tolerant of
 * parser representation changes without changing the public RawActivity shape.
 *
 * @param {*} entry - Extension entry.
 * @param {string} code - Three-letter IGC extension code.
 * @returns {*|null} Raw extension value when the entry matches the code.
 */
function extensionEntryValue(entry, code) {
  if (Array.isArray(entry)) {
    return entry[0] === code ? entry[1] : null
  }

  if (!entry || typeof entry !== 'object') {
    return null
  }

  const entryCode = entry.code ?? entry.name ?? entry.key ?? entry.type
  if (entryCode !== code) {
    return null
  }

  return entry.value ?? entry.rawValue ?? entry.raw ?? entry.data ?? null
}

/**
 * Reads a raw extension value by three-letter code.
 *
 * @param {*} extensions - Extension collection from an IGC fix.
 * @param {string} code - Three-letter IGC extension code.
 * @returns {*|null} Raw extension value or null when absent.
 */
function readExtensionValue(extensions, code) {
  if (!extensions) return null

  if (typeof extensions.get === 'function') {
    return extensions.get(code) ?? extensions.get(code.toLowerCase()) ?? null
  }

  if (Array.isArray(extensions)) {
    for (const entry of extensions) {
      const value = extensionEntryValue(entry, code)
      if (value !== null && value !== undefined) return value
    }
    return null
  }

  if (typeof extensions === 'object') {
    return extensions[code] ?? extensions[code.toLowerCase()] ?? null
  }

  return null
}

/**
 * Reads and converts an optional fix extension.
 *
 * @param {object} fix - IGC B-record fix from igc-parser.
 * @param {string} code - Three-letter IGC extension code.
 * @param {Function} transform - Conversion callback for the raw value.
 * @returns {number|null} Converted finite value or null.
 */
function readExt(fix, code, transform) {
  const rawValue = readExtensionValue(fix.extensions, code)
  if (rawValue === null || rawValue === undefined || rawValue === '') return null

  return finiteOrNull(transform(rawValue))
}

/**
 * Normalizes an igc-parser timestamp into Unix epoch milliseconds.
 *
 * @param {object} fix - IGC B-record fix from igc-parser.
 * @returns {number|null} Epoch milliseconds or null when invalid.
 */
function timestampMillis(fix) {
  if (fix.timestamp instanceof Date) return fix.timestamp.getTime()
  if (typeof fix.timestamp === 'number') return fix.timestamp

  const parsed = Date.parse(fix.timestamp)
  return Number.isFinite(parsed) ? parsed : null
}

/**
 * Infers the OAT temperature divisor from every OAT value in the file.
 *
 * Logger vendors encode OAT inconsistently. This keeps the decision at file
 * level so one zero-padded sample such as `0244` is interpreted with the same
 * scale as the rest of the series:
 * - 24   -> 24 C
 * - 0244 -> 24.4 C
 * - 2400 -> 24.00 C
 *
 * @param {Array<object>} fixes - IGC B-record fixes from igc-parser.
 * @returns {number} Divisor for raw OAT values.
 */
function inferOatDivisor(fixes) {
  const rawValues = fixes
    .map((fix) => safeNumber(readExtensionValue(fix.extensions, 'OAT')))
    .filter((value) => value !== null)
    .map(Math.abs)

  if (!rawValues.length) return 1

  const maxRawValue = Math.max(...rawValues)
  if (maxRawValue >= 1000) return 100
  if (maxRawValue >= 100) return 10
  return 1
}

/**
 * Converts one IGC B-record fix into a RawSample.
 *
 * @param {object} fix - IGC B-record fix from igc-parser.
 * @param {number} firstTimestamp - First fix timestamp in epoch milliseconds.
 * @param {number} oatDivisor - File-level OAT unit divisor.
 * @returns {object} RawSample-compatible telemetry sample.
 */
function rawSampleFromFix(fix, firstTimestamp, oatDivisor) {
  const timestamp = timestampMillis(fix)
  if (timestamp === null) {
    throw new Error('The IGC file contains a fix without a valid timestamp.')
  }

  const validFix = fix.valid !== false
  const timestampIso = new Date(timestamp).toISOString()
  const elapsedSeconds = (timestamp - firstTimestamp) / 1000
  const speed = readExt(fix, 'GSP', (value) => scaledNumber(value, 360))
  const heading = readExt(fix, 'TRT', safeNumber)
  const verticalSpeed = readExt(fix, 'VAT', (value) => scaledNumber(value, 100))
  const temperature = readExt(fix, 'OAT', (value) => scaledNumber(value, oatDivisor))

  return {
    timestamp: timestampIso,
    elapsed_seconds: elapsedSeconds,
    latitude: validFix ? safeNumber(fix.latitude) : null,
    longitude: validFix ? safeNumber(fix.longitude) : null,
    elevation: validFix ? safeNumber(fix.gpsAltitude) : null,
    speed,
    heading,
    vertical_speed: verticalSpeed,
    temperature,
  }
}

/**
 * Parses an IGC browser File into the shared RawActivity contract.
 *
 * Parsing is lenient so per-line vendor quirks are captured in metadata while
 * usable fixes still import. A file with no fixes is rejected because the
 * backend finalizer needs at least one timeline sample to produce activity
 * series.
 *
 * @param {File|{name: string, text: Function}} file - Browser File-like IGC input.
 * @returns {Promise<object>} RawActivity payload for backend finalization.
 */
export async function parseIgcActivityFile(file) {
  const result = IGCParser.parse(await file.text(), { lenient: true })
  const fixes = Array.isArray(result?.fixes) ? result.fixes : []

  if (!fixes.length) {
    throw new Error('The IGC file does not contain any fix records.')
  }

  const firstTimestamp = timestampMillis(fixes[0])
  if (firstTimestamp === null) {
    throw new Error('The IGC file contains a fix without a valid timestamp.')
  }

  const oatDivisor = inferOatDivisor(fixes)

  return {
    file_name: file.name,
    file_format: 'igc',
    metadata: {
      activity_name: result.date && result.pilot ? `${result.date} - ${result.pilot}` : result.date || null,
      date: result.date || null,
      glider_type: result.gliderType || null,
      timezone: result.timezone || null,
      logger_manufacturer: result.loggerManufacturer || null,
      logger_type: result.loggerType || null,
      parse_errors: result.errors?.length ? result.errors.map((error) => String(error.message || error)) : null,
    },
    raw_samples: fixes.map((fix) => rawSampleFromFix(fix, firstTimestamp, oatDivisor)),
    options: {
      skip_idle_gap_fill: false,
      smoothing: {
        pace: { enabled: true, method: 'zero_phase_ma', window_seconds: 5.0 },
        heading: { enabled: true, method: 'circular_ema', window_seconds: 0.5 },
      },
    },
  }
}

export default parseIgcActivityFile
