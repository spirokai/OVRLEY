/**
 * Pure helper functions for scene settings — sanitization and time parsing.
 */

export { timeToSeconds } from '@/features/overlay-editor/utils/exportRange'

export function sanitizeNumber(val) {
  if (val === undefined || val === null) return val
  const sanitized = val
    .toString()
    .replace(/,/g, '')
    .replace(/^0+(?!$)/, '')
  return parseInt(sanitized, 10) || 0
}

/**
 * Parses a time offset string into seconds. Supports plain seconds, MM:SS, and HH:MM:SS formats.
 *
 * @param {string} value - Time offset string.
 * @returns {number} Offset in seconds.
 */
export function parseTimeOffset(value) {
  if (!value) return 0
  const str = String(value).trim()
  if (str === '') return 0

  const isNegative = str.startsWith('-')
  const absStr = isNegative ? str.substring(1) : str

  if (absStr.includes(':')) {
    const parts = absStr.split(':')
    let seconds = 0
    if (parts.length === 2) {
      seconds = parseInt(parts[0]) * 60 + parseFloat(parts[1])
    } else if (parts.length === 3) {
      seconds = parseInt(parts[0]) * 3600 + parseInt(parts[1]) * 60 + parseFloat(parts[2])
    }
    return isNegative ? -seconds : seconds
  }

  const parsed = parseFloat(str)
  return isNaN(parsed) ? 0 : parsed
}
