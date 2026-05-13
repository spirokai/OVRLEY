const PRESET_UPDATE_RATES = {
  24: [1, 2, 4, 8],
  30: [1, 2, 3, 5],
  60: [1, 2, 6, 10],
}

/**
 * Returns a positive integer FPS value.
 *
 * @param {*} fps - Input FPS value.
 * @returns {number} Sanitized integer FPS.
 */
export function sanitizeIntegerFps(fps) {
  return Math.max(Math.trunc(Number(fps) || 1), 1)
}

/**
 * Returns update-rate divisors for an integer FPS.
 *
 * @param {*} fps - Layout FPS value.
 * @returns {number[]} Available update-rate values including full rate.
 */
export function getUpdateRateOptions(fps) {
  const safeFps = sanitizeIntegerFps(fps)
  const preset = PRESET_UPDATE_RATES[safeFps]
  if (preset) {
    return preset
  }

  const maxUsefulDivisor = Math.floor(safeFps / 3)
  const divisors = []
  for (let divisor = 2; divisor <= maxUsefulDivisor; divisor += 1) {
    if (safeFps % divisor === 0) {
      divisors.push(divisor)
    }
  }

  if (divisors.length <= 3) {
    return [1, ...divisors]
  }

  const selected = [divisors[0], divisors[Math.floor((divisors.length - 1) / 2)], divisors[divisors.length - 1]]

  return [1, ...Array.from(new Set(selected))]
}

/**
 * Normalizes update rate to an available divisor for the FPS.
 *
 * @param {*} fps - Layout FPS value.
 * @param {*} updateRate - Requested update rate.
 * @returns {number} Valid update rate.
 */
export function normalizeUpdateRateForFps(fps, updateRate) {
  const options = getUpdateRateOptions(fps)
  const requested = Math.max(Math.trunc(Number(updateRate) || 1), 1)
  return options.includes(requested) ? requested : 1
}

/**
 * Returns encoded/container FPS after applying update rate.
 *
 * @param {*} fps - Layout FPS value.
 * @param {*} updateRate - Update-rate divisor.
 * @returns {number} Effective container FPS.
 */
export function getContainerFps(fps, updateRate) {
  const safeFps = sanitizeIntegerFps(fps)
  const safeUpdateRate = normalizeUpdateRateForFps(safeFps, updateRate)
  return safeFps / safeUpdateRate
}
