import { interpolateNumericSeries, MISSING_SAMPLE_POLICY } from '@/lib/interpolation'
import { getHoldSeriesValue } from '@/features/overlay-editor/utils/overlayEditorUtils'

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
 * Formats a delta with an explicit sign and two decimals.
 * @param {number|null} deltaSeconds - Delta in seconds, or null before a reference exists.
 * @returns {string} Formatted delta.
 */
export function formatLapDelta(deltaSeconds) {
  if (deltaSeconds === null || deltaSeconds === 0) return '+0.00'
  const roundedMagnitude = Math.round(Math.abs(deltaSeconds) * 100) / 100
  return `${deltaSeconds < 0 ? '-' : '+'}${roundedMagnitude.toFixed(2)}`
}

function getDeltaAt(activity, previewSecond) {
  if (activity === null) return null
  return interpolateNumericSeries(activity.sample_elapsed_seconds, activity.delta_to_best_lap_seconds, previewSecond, MISSING_SAMPLE_POLICY.PRESERVE)
}

/**
 * Resolves the lap timer state at an activity timestamp.
 * @param {object|null} activity - Parsed activity with lap timing fields.
 * @param {number} previewSecond - Absolute activity timestamp in seconds.
 * @returns {{ lapNumber: number, currentLapTime: number|null, bestLapTime: number|null }} Lap state.
 */
export function getLapTimerState(activity, previewSecond) {
  if (activity === null) return { lapNumber: -1, currentLapTime: null, bestLapTime: null }
  const lapNumber = getHoldSeriesValue(activity.sample_elapsed_seconds, activity.lap_number, previewSecond)
  if (lapNumber === null) throw new Error(`Lap number is missing at ${previewSecond} seconds`)

  let startedLapCount = 0
  for (const lapStart of activity.lap_start_elapsed_seconds) {
    if (lapStart <= previewSecond) startedLapCount += 1
    else break
  }
  const completedLapCount = Math.max(startedLapCount - 1, 0)
  const currentLapTime = lapNumber >= 0 ? previewSecond - activity.lap_start_elapsed_seconds[lapNumber] : null
  if (completedLapCount === 0) return { lapNumber, currentLapTime, bestLapTime: null }

  const bestLapTime = activity.lap_durations_best_so_far_seconds[completedLapCount - 1]
  if (bestLapTime === undefined) throw new Error(`Best lap metadata is missing for completed lap ${completedLapCount}`)
  return { lapNumber, currentLapTime, bestLapTime }
}

/**
 * Returns the display value for a lap timer widget.
 * @param {object|null} activity - Parsed activity with lap timing fields.
 * @param {number} previewSecond - Absolute activity timestamp in seconds.
 * @param {'current_lap'|'best_lap'|'delta'} mode - Lap timer readout mode.
 * @returns {{ valueText: string, useNegativeDeltaColor: boolean }} Formatted widget state.
 */
export function getLapTimerDisplayState(activity, previewSecond, mode) {
  if (mode === 'delta') {
    const delta = getDeltaAt(activity, previewSecond)
    return { valueText: formatLapDelta(delta), useNegativeDeltaColor: delta === null || delta <= 0 }
  }
  if (mode !== 'current_lap' && mode !== 'best_lap') throw new Error(`Unsupported lap timer mode: ${mode}`)
  const state = getLapTimerState(activity, previewSecond)
  if (state.lapNumber < 0) {
    return {
      valueText: mode === 'best_lap' && state.bestLapTime !== null ? formatLapDuration(state.bestLapTime) : PLACEHOLDER,
      useNegativeDeltaColor: false,
    }
  }
  return {
    valueText: formatLapDuration(mode === 'current_lap' ? state.currentLapTime : (state.bestLapTime ?? state.currentLapTime)),
    useNegativeDeltaColor: false,
  }
}

/**
 * Returns the formatted display value for a lap timer widget.
 * @param {object|null} activity - Parsed activity with lap timing fields.
 * @param {number} previewSecond - Absolute activity timestamp in seconds.
 * @param {'current_lap'|'best_lap'|'delta'} mode - Lap timer readout mode.
 * @returns {string} Formatted widget value.
 */
export function getLapTimerDisplayValue(activity, previewSecond, mode) {
  return getLapTimerDisplayState(activity, previewSecond, mode).valueText
}
