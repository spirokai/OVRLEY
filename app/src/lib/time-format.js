import i18next from 'i18next'

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
