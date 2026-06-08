/**
 * SRT activity parser — converts DJI-style SRT subtitle cues into the
 * canonical raw-sample format consumed by `finalizeParsedActivity()`.
 *
 * Supports both Format A (bracketed `[key: value]` telemetry) and
 * Format B (legacy line-oriented `KEY:VALUE` telemetry).
 */

import { finalizeParsedActivity } from './parser.js'

/**
 * Parse a cue timing line like `00:00:02,001 --> 00:00:02,035` into elapsed seconds.
 * @param {string} line
 * @returns {number|null}
 */
function parseCueStartSeconds(line) {
  const match = line.match(/^(\d{2}):(\d{2}):(\d{2}),(\d{3})\s*-->/)
  if (!match) return null
  const hours = parseInt(match[1], 10)
  const minutes = parseInt(match[2], 10)
  const seconds = parseInt(match[3], 10)
  const millis = parseInt(match[4], 10)
  return hours * 3600 + minutes * 60 + seconds + millis / 1000
}

/**
 * Parse a shutter speed string into numeric seconds.
 * Supports reciprocal forms like `1/3200.0` and `1/50`, and decimal forms like `0.5`.
 * @param {string} raw — the shutter string from the SRT field
 * @returns {number|null}
 */
function parseShutter(raw) {
  if (!raw) return null
  const trimmed = raw.trim()
  // Decimal form: 0.5, 2.0, etc.
  if (/^\d+(\.\d+)?$/.test(trimmed)) {
    const num = parseFloat(trimmed)
    return Number.isFinite(num) ? num : null
  }
  // Reciprocal form: 1/3200.0, 1/50
  const recip = trimmed.match(/^1\/(\d+(?:\.\d+)?)$/)
  if (recip) {
    const denom = parseFloat(recip[1])
    if (Number.isFinite(denom) && denom > 0) return 1 / denom
  }
  return null
}

/**
 * Extract the first timestamp-like line from the body text (Format A).
 * Format A has a standalone line like `2025-07-23 10:21:41.694`.
 * @param {string[]} bodyLines
 * @returns {string|null}
 */
function extractFormatATimestamp(bodyLines) {
  for (const line of bodyLines) {
    const match = line.match(/^(\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2}\.\d{3})/)
    if (match) return match[1]
  }
  return null
}

/**
 * Convert a DJI-style timestamp to ISO 8601.
 * Input: `2025-07-23 10:21:41.694`
 * Output: `2025-07-23T10:21:41.694Z` (best-effort, assumes UTC)
 * @param {string} ts
 * @returns {string|null}
 */
function toIsoTimestamp(ts) {
  if (!ts) return null
  try {
    const replaced = ts.replace(' ', 'T') + '.000Z'
    // If original already has fractional seconds, use them
    const withFraction = ts.replace(' ', 'T').replace(/\.(\d{3})$/, '.$1Z')
    if (withFraction.includes('Z')) return withFraction
    return replaced
  } catch {
    return null
  }
}

/**
 * Parse bracketed key-value fields from a body line.
 * Handles `[key: value]` and `[key: value1 key2: value2]` groups.
 * @param {string} text
 * @returns {Record<string, string>}
 */
function parseFormatABracketedFields(text) {
  const fields = {}
  const bracketRegex = /\[([^\]]+)\]/g
  let bracketMatch
  while ((bracketMatch = bracketRegex.exec(text)) !== null) {
    const inner = bracketMatch[1]
    // Split on `key: value` pairs within the brackets
    const kvRegex = /([a-zA-Z_]+)\s*:\s*([^\]]*?)(?=\s*[a-zA-Z_]+\s*:|$)/g
    let kvMatch
    while ((kvMatch = kvRegex.exec(inner)) !== null) {
      fields[kvMatch[1]] = kvMatch[2].trim()
    }
  }
  return fields
}

/**
 * Strip HTML-like tags from a cue body to extract plain telemetry text.
 * @param {string[]} bodyLines
 * @returns {string} cleaned body text with newlines preserved
 */
function stripHtmlFromBody(bodyLines) {
  return bodyLines.map((line) => line.replace(/<[^>]+>/g, '')).join('\n')
}

/**
 * Parse Format A (bracketed telemetry) body text into a raw sample.
 * @param {string[]} bodyLines
 * @param {number} elapsedSeconds
 * @returns {object} raw sample
 */
function parseFormatACue(bodyLines, elapsedSeconds) {
  const cleanedText = stripHtmlFromBody(bodyLines)
  const fields = parseFormatABracketedFields(cleanedText)
  const timestampLine = extractFormatATimestamp(bodyLines)

  return {
    elapsedSeconds,
    timestamp: toIsoTimestamp(timestampLine),
    latitude: parseFloat(fields.latitude) || null,
    longitude: parseFloat(fields.longitude) || null,
    altitude: parseFloat(fields.abs_alt) || null,
    elevation: parseFloat(fields.abs_alt) || null,
    iso: parseFloat(fields.iso) || null,
    aperture: parseFloat(fields.fnum) || null,
    shutterSpeed: parseShutter(fields.shutter),
    focalLength: parseFloat(fields.focal_len) || null,
    ev: fields.ev !== undefined ? parseFloat(fields.ev) || 0 : null,
    colorTemperature: parseFloat(fields.ct) || null,
  }
}

/**
 * Detect whether the SRT content uses bracketed telemetry (Format A) by checking
 * the first few cues for `[key: value]` patterns.
 * @param {string} text — full SRT file content
 * @returns {boolean}
 */
function detectFormatA(text) {
  const firstBlock = text.slice(0, 3000)
  return /\[[a-zA-Z_]+\s*:/.test(firstBlock)
}

// ── Format B (legacy line-oriented) ──────────────────────────────────────

/**
 * Extract timestamp from a HOME(...) line.
 * Input: `HOME(149.0251,-20.2532) 2017.08.05 14:12:00`
 * @param {string} line
 * @returns {string|null}
 */
function extractFormatBTimestamp(line) {
  const match = line.match(/(\d{4}\.\d{2}\.\d{2}\s+\d{2}:\d{2}:\d{2})/)
  return match ? match[1] : null
}

/**
 * Convert Format B timestamp to ISO 8601.
 * Input: `2017.08.05 14:12:00` → Output: `2017-08-05T14:12:00.000Z`
 * @param {string} ts
 * @returns {string|null}
 */
function toFormatBIso(ts) {
  if (!ts) return null
  try {
    const [datePart, timePart] = ts.split(' ')
    const isoDate = datePart.replace(/\./g, '-')
    return isoDate + 'T' + timePart + '.000Z'
  } catch {
    return null
  }
}

/**
 * Parse GPS tuple like `GPS(149.0251,-20.2532,27)`.
 * @param {string} line
 * @returns {{ latitude: number|null, longitude: number|null, altitude: number|null }}
 */
function parseFormatBGps(line) {
  const match = line.match(/GPS\(([^)]+)\)/)
  if (!match) return { latitude: null, longitude: null, altitude: null }
  const parts = match[1].split(',')
  return {
    latitude: parseFloat(parts[0]) || null,
    longitude: parseFloat(parts[1]) || null,
    altitude: parts.length > 2 ? parseFloat(parts[2]) || null : null,
  }
}

/**
 * Parse compact camera line like `ISO:100 Shutter:60 EV: Fnum:2.2`.
 * Tolerates blank values (e.g. `EV: `).
 * @param {string} line
 * @returns {{ iso: number|null, shutter: string|null, ev: number|null, aperture: number|null }}
 */
function parseFormatBCamera(line) {
  const result = { iso: null, shutter: null, ev: null, aperture: null }
  const isoMatch = line.match(/ISO:(\S+)/)
  if (isoMatch && isoMatch[1]) result.iso = parseFloat(isoMatch[1]) || null
  const shutterMatch = line.match(/Shutter:(\S+)/)
  if (shutterMatch && shutterMatch[1]) result.shutter = shutterMatch[1]
  const evMatch = line.match(/EV:(\S+)/)
  if (evMatch && evMatch[1]) result.ev = parseFloat(evMatch[1])
  const fnumMatch = line.match(/Fnum:(\S+)/)
  if (fnumMatch && fnumMatch[1]) result.aperture = parseFloat(fnumMatch[1]) || null
  return result
}

/**
 * Parse BAROMETER value from a line.
 * @param {string} line
 * @returns {number|null}
 */
function parseFormatBBarometer(line) {
  const match = line.match(/BAROMETER:(\S+)/)
  return match ? parseFloat(match[1]) || null : null
}

/**
 * Parse Format B (line-oriented) body text into a raw sample.
 * @param {string[]} bodyLines
 * @param {number} elapsedSeconds
 * @returns {object} raw sample
 */
function parseFormatBCue(bodyLines, elapsedSeconds) {
  let timestamp = null
  let latitude = null
  let longitude = null
  let altitude = null
  let iso = null
  let shutterRaw = null
  let ev = null
  let aperture = null

  for (const line of bodyLines) {
    const trimmed = line.trim()
    if (!trimmed) continue

    if (trimmed.startsWith('HOME(')) {
      const ts = extractFormatBTimestamp(trimmed)
      if (ts) timestamp = toFormatBIso(ts)
      continue
    }

    if (trimmed.startsWith('GPS(')) {
      const gps = parseFormatBGps(trimmed)
      latitude = gps.latitude
      longitude = gps.longitude
      altitude = gps.altitude
      continue
    }

    if (trimmed.startsWith('ISO:')) {
      const cam = parseFormatBCamera(trimmed)
      iso = cam.iso
      shutterRaw = cam.shutter
      ev = cam.ev
      aperture = cam.aperture
      continue
    }

    // BAROMETER may appear on same line as GPS or standalone
    const baro = parseFormatBBarometer(trimmed)
    if (baro !== null && altitude === null) {
      altitude = baro
    }
  }

  return {
    elapsedSeconds,
    timestamp,
    latitude,
    longitude,
    altitude,
    elevation: altitude,
    iso,
    aperture,
    shutterSpeed: parseFormatBShutter(shutterRaw),
    focalLength: null,
    ev: ev !== null ? ev : null,
    colorTemperature: null,
  }
}

/**
 * Parse a Format B shutter speed into numeric seconds.
 * Format B stores shutter as the denominator of a reciprocal fraction
 * (e.g. "60" means 1/60s, "3200" means 1/3200s).
 * @param {string|null} raw
 * @returns {number|null}
 */
function parseFormatBShutter(raw) {
  if (!raw) return null
  const trimmed = raw.trim()
  const num = parseFloat(trimmed)
  if (!Number.isFinite(num) || num <= 0) return null
  return 1 / num
}

// ── Cue extraction ───────────────────────────────────────────────────────

/**
 * Split SRT text into individual cues (index, timing, body lines).
 * @param {string} text
 * @returns {Array<{ index: number, timingLine: string, bodyLines: string[] }>}
 */
function splitCues(text) {
  const cues = []
  const blocks = text.split(/\n\s*\n/)
  let currentCue = null

  for (const block of blocks) {
    const lines = block.trim().split('\n')
    if (lines.length === 0) continue

    const firstLine = lines[0].trim()
    // Check if first line is a cue index (integer)
    if (/^\d+$/.test(firstLine) && lines.length >= 2) {
      // Start a new cue
      const timingLine = lines[1].trim()
      const bodyLines = lines.slice(2).filter((l) => l.trim())
      if (timingLine.includes('-->')) {
        cues.push({ index: parseInt(firstLine, 10), timingLine, bodyLines })
      }
    }
  }
  return cues
}

// ── Public API ───────────────────────────────────────────────────────────

/**
 * Parse an SRT activity file into the canonical `{ parsedActivity, debugPayload }` shape.
 * @param {string} text — raw SRT file content
 * @param {string} fileName — source file name
 * @returns {{ parsedActivity: object, debugPayload: object }}
 */
export function parseSrtActivityFile(text, fileName) {
  const isFormatA = detectFormatA(text)
  const cues = splitCues(text)

  const rawSamples = cues
    .map((cue) => {
      const elapsedSeconds = parseCueStartSeconds(cue.timingLine)
      if (elapsedSeconds === null) return null
      return isFormatA ? parseFormatACue(cue.bodyLines, elapsedSeconds) : parseFormatBCue(cue.bodyLines, elapsedSeconds)
    })
    .filter(Boolean)

  return finalizeParsedActivity({
    fileName,
    fileFormat: 'srt',
    metadata: {},
    rawSamples,
    options: { skipIdleGapFill: true },
  })
}
