/**
 * Shared numeric, geospatial, and timestamp helper functions for activity parsing.
 * Pure functions — no React, no side effects.
 */

/**
 * Checks whether a value is a finite number.
 *
 * @param {*} value - Input value.
 * @returns {boolean} True if value is a finite number.
 */
export function isFiniteNumber(value) {
  return typeof value === 'number' && Number.isFinite(value)
}

/**
 * Rounds a value to a given number of decimal digits.
 *
 * @param {*} value - Input value.
 * @param {number} [digits=6] - Number of decimal places.
 * @returns {number|null} Rounded value, or null if input is not finite.
 */
export function roundValue(value, digits = 6) {
  if (!isFiniteNumber(value)) return null
  const scale = 10 ** digits
  return Math.round(value * scale) / scale
}

/**
 * Safely converts a value to a number, returning null for invalid inputs.
 *
 * @param {*} value - Input value.
 * @returns {number|null} Numeric value or null.
 */
export function safeNumber(value) {
  if (value === null || value === undefined || value === '') {
    return null
  }

  if (typeof value === 'number') {
    return Number.isFinite(value) ? value : null
  }

  if (typeof value === 'string') {
    const normalized = value.trim()
    if (!normalized) return null
    const numeric = Number(normalized)
    return Number.isFinite(numeric) ? numeric : null
  }

  return null
}

/**
 * Safely parses a timestamp value to an ISO string.
 *
 * @param {*} value - Timestamp value (Date, ISO string, or falsy).
 * @returns {string|null} ISO timestamp string or null.
 */
export function safeTimestamp(value) {
  if (!value) return null
  const date = value instanceof Date ? value : new Date(value)
  if (!Number.isFinite(date.getTime())) return null
  return date.toISOString()
}

/**
 * Calculates the distance in meters between two lat/lon points using the Haversine formula.
 *
 * @param {number} lat1 - Latitude of the first point.
 * @param {number} lon1 - Longitude of the first point.
 * @param {number} lat2 - Latitude of the second point.
 * @param {number} lon2 - Longitude of the second point.
 * @returns {number} Distance in meters.
 */
export function haversineDistanceMeters(lat1, lon1, lat2, lon2) {
  if (!isFiniteNumber(lat1) || !isFiniteNumber(lon1) || !isFiniteNumber(lat2) || !isFiniteNumber(lon2)) {
    return 0
  }

  const earthRadiusMeters = 6371000
  const lat1Rad = (lat1 * Math.PI) / 180
  const lat2Rad = (lat2 * Math.PI) / 180
  const latDeltaRad = ((lat2 - lat1) * Math.PI) / 180
  const lonDeltaRad = ((lon2 - lon1) * Math.PI) / 180

  const a = Math.sin(latDeltaRad / 2) ** 2 + Math.cos(lat1Rad) * Math.cos(lat2Rad) * Math.sin(lonDeltaRad / 2) ** 2

  const c = 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a))
  return earthRadiusMeters * c
}

/**
 * Calculates the bearing in degrees between two lat/lon points.
 *
 * @param {number[]} fromPoint - [latitude, longitude] of the origin.
 * @param {number[]} toPoint - [latitude, longitude] of the destination.
 * @returns {number|null} Bearing in degrees (0–360), or null if invalid.
 */
export function calculateBearingDegrees(fromPoint, toPoint) {
  if (!fromPoint || !toPoint) return null

  const [fromLat, fromLon] = fromPoint
  const [toLat, toLon] = toPoint
  if (!isFiniteNumber(fromLat) || !isFiniteNumber(fromLon) || !isFiniteNumber(toLat) || !isFiniteNumber(toLon)) {
    return null
  }

  const fromLatRad = (fromLat * Math.PI) / 180
  const toLatRad = (toLat * Math.PI) / 180
  const lonDeltaRad = ((toLon - fromLon) * Math.PI) / 180
  const y = Math.sin(lonDeltaRad) * Math.cos(toLatRad)
  const x = Math.cos(fromLatRad) * Math.sin(toLatRad) - Math.sin(fromLatRad) * Math.cos(toLatRad) * Math.cos(lonDeltaRad)

  const bearing = (Math.atan2(y, x) * 180) / Math.PI
  return (bearing + 360) % 360
}
