/** @param {string} value Editor input. @param {number} fallback Value used while the input is empty. @returns {number} Parsed integer. */
export function parseInteger(value, fallback = 0) {
  if (value === '') return fallback
  const parsed = Number(value)
  if (!Number.isFinite(parsed)) throw new Error(`Invalid integer input: ${value}`)
  return Math.trunc(parsed)
}
