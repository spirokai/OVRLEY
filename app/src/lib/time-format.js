import i18next from 'i18next'

/**
 * Converts a time string (HH:MM:SS, MM:SS, or plain seconds) to seconds.
 * Empty/null input returns 0.
 *
 * @param {string|null|undefined} timeStr Time string in HH:MM:SS or MM:SS format.
 * @returns {number} Total seconds.
 */
export function timeToSeconds(timeStr) {
  if (timeStr === null || timeStr === undefined || timeStr === '') {
    return 0
  }

  const str = String(timeStr).trim()
  if (str === '') return 0

  const isNegative = str.startsWith('-')
  const absStr = isNegative ? str.substring(1) : str

  if (absStr.includes(':')) {
    const parts = absStr.split(':')
    let seconds = 0
    if (parts.length === 3) {
      seconds = parseInt(parts[0], 10) * 3600 + parseInt(parts[1], 10) * 60 + parseFloat(parts[2])
    } else if (parts.length === 2) {
      seconds = parseInt(parts[0], 10) * 60 + parseFloat(parts[1])
    }
    return isNegative ? -seconds : seconds
  }

  const parsed = parseFloat(str)
  return isNaN(parsed) ? 0 : parsed
}

/**
 * Converts a numeric second value to HH:MM:SS.
 *
 * @param {number} seconds Whole or fractional second value.
 * @returns {string} Zero-padded HH:MM:SS string.
 */
export function formatExportRangeTime(seconds) {
  const sign = seconds < 0 ? '-' : ''
  const wholeSeconds = Math.trunc(Math.abs(seconds))
  const hours = Math.floor(wholeSeconds / 3600)
  const minutes = Math.floor((wholeSeconds % 3600) / 60)
  const remainingSeconds = wholeSeconds % 60

  return `${sign}${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${String(remainingSeconds).padStart(2, '0')}`
}

/**
 * Formats seconds as a clock duration in mm:ss or h:mm:ss form.
 *
 * @param {number} seconds - Duration in seconds.
 * @returns {string} Clock-formatted duration.
 */
export function formatClockDuration(seconds) {
  const sign = seconds < 0 ? '-' : ''
  const wholeSeconds = Math.floor(Math.abs(seconds))
  const hours = Math.floor(wholeSeconds / 3600)
  const minutes = Math.floor((wholeSeconds % 3600) / 60)
  const remainingSeconds = wholeSeconds % 60

  if (hours > 0) {
    return `${sign}${hours}:${String(minutes).padStart(2, '0')}:${String(remainingSeconds).padStart(2, '0')}`
  }

  return `${sign}${String(minutes).padStart(2, '0')}:${String(remainingSeconds).padStart(2, '0')}`
}

/**
 * Formats seconds as a clock: `mm:ss`, or `hh:mm:ss` from one hour on, with up
 * to three decimals when the value has a fraction (`330.5` → `05:30.5`).
 *
 * @param {number} seconds - Value in seconds.
 * @returns {string} Clock text.
 */
export function formatSecondsClock(seconds) {
  const rounded = Number(Math.abs(seconds).toFixed(3))
  const sign = seconds < 0 && rounded > 0 ? '-' : ''
  const wholeSeconds = Math.floor(rounded)
  const fraction = String(Number((rounded - wholeSeconds).toFixed(3))).slice(1)
  const hours = Math.floor(wholeSeconds / 3600)
  const clock = [Math.floor((wholeSeconds % 3600) / 60), wholeSeconds % 60].map((part) => String(part).padStart(2, '0'))
  if (hours > 0) clock.unshift(String(hours).padStart(2, '0'))
  return `${sign}${clock.join(':')}${fraction}`
}

/**
 * Resolves canonical calendar and clock fields in an IANA timezone.
 *
 * @param {string} timestamp - Canonical timestamp.
 * @param {string} timezone - IANA recording timezone.
 * @returns {object} Zoned date/time fields.
 */
export function getZonedDateTimeParts(timestamp, timezone) {
  const parts = new Intl.DateTimeFormat('en-GB', {
    timeZone: timezone,
    calendar: 'gregory',
    numberingSystem: 'latn',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hourCycle: 'h23',
  }).formatToParts(new Date(timestamp))
  const values = {}
  for (const part of parts) values[part.type] = part.value
  return values
}

/**
 * Formats a canonical timestamp in its recording timezone and display language.
 *
 * @param {string} timestamp - Canonical timestamp.
 * @param {string} timezone - IANA recording timezone.
 * @param {string} [locale=i18next.resolvedLanguage] - Selected app language.
 * @returns {string} Localized date and time.
 */
export function formatZonedDateTime(timestamp, timezone, locale = i18next.resolvedLanguage) {
  return new Intl.DateTimeFormat(locale, {
    timeZone: timezone,
    hourCycle: 'h23',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(new Date(timestamp))
}
