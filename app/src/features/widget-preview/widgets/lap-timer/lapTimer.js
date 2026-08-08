const PLACEHOLDER = '--:--.--'

/**
 * Formats a lap duration using the renderer's MM:SS.ss / HH:MM:SS.ss contract.
 * @param {number} durationSeconds - Validated duration in seconds.
 * @returns {string} Formatted duration.
 */
export function formatLapDuration(durationSeconds) {
  const hundredths = Math.round(durationSeconds * 100)
  const hours = Math.floor(hundredths / 360000)
  const minutes = hours > 0 ? Math.floor(hundredths / 6000) % 60 : Math.floor(hundredths / 6000)
  const seconds = Math.floor(hundredths / 100) % 60
  const remainder = hundredths % 100
  if (hours > 0)
    return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}.${String(remainder).padStart(2, '0')}`
  return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}.${String(remainder).padStart(2, '0')}`
}

/**
 * Resolves the lap timer state at an activity timestamp.
 * @param {object|null} activity - Parsed activity with lap timing fields.
 * @param {number} previewSecond - Absolute activity timestamp in seconds.
 * @returns {{ lapNumber: number, currentLapTime: number|null, bestLapTime: number|null }} Lap state.
 */
export function getLapTimerState(activity, previewSecond) {
  if (activity === null) return { lapNumber: -1, currentLapTime: null, bestLapTime: null }
  let startedLapCount = 0
  for (const lapStart of activity.lap_start_elapsed_seconds) {
    if (lapStart <= previewSecond) startedLapCount += 1
    else break
  }
  if (startedLapCount === 0) return { lapNumber: -1, currentLapTime: null, bestLapTime: null }

  const lapNumber = startedLapCount - 1
  const currentLapTime = previewSecond - activity.lap_start_elapsed_seconds[lapNumber]
  if (lapNumber === 0) return { lapNumber, currentLapTime, bestLapTime: null }

  const bestLapTime = activity.lap_durations_best_so_far_seconds[lapNumber - 1]
  if (bestLapTime === undefined) throw new Error(`Best lap metadata is missing for completed lap ${lapNumber}`)
  return { lapNumber, currentLapTime, bestLapTime }
}

/**
 * Returns the display value for a lap timer widget.
 * @param {object|null} activity - Parsed activity with lap timing fields.
 * @param {number} previewSecond - Absolute activity timestamp in seconds.
 * @param {'current_lap'|'best_lap'} mode - Lap timer readout mode.
 * @returns {string} Formatted widget value.
 */
export function getLapTimerDisplayValue(activity, previewSecond, mode) {
  if (mode !== 'current_lap' && mode !== 'best_lap') throw new Error(`Unsupported lap timer mode: ${mode}`)
  const state = getLapTimerState(activity, previewSecond)
  if (state.lapNumber < 0) return PLACEHOLDER
  return formatLapDuration(mode === 'current_lap' ? state.currentLapTime : (state.bestLapTime ?? state.currentLapTime))
}
