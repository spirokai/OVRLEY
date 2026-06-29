/**
 * Small extraction helpers for frontend raw activity parsers.
 */

export function safeNumber(value) {
  if (value === null || value === undefined || value === '') return null
  const numeric = Number(value)
  return Number.isFinite(numeric) ? numeric : null
}
