/**
 * Pure helpers of the seconds input: what can be typed, how a draft parses, how
 * a value is shown, and how it snaps to a frame grid.
 *
 * A draft is a clock (`5:30`, `1:01:40.5`) or plain seconds (`330`): digits, up
 * to two `:`, one `.` in the last part and, for signed values, a leading `-`.
 * A comma is read as the decimal point.
 */

import { framesToSeconds, secondsToFrames, snapDurationToFrames } from './video-program/frame-grid'

/** Decimals shown for a value; three resolve every frame up to 500 fps. */
const SHOWN_DECIMALS = 3

const SIGNED_DRAFT = /^-?(?:\d*:){0,2}\d*\.?\d*$/
const UNSIGNED_DRAFT = /^(?:\d*:){0,2}\d*\.?\d*$/
const COMPLETE_CLOCK = /^(-?)(?:(?:(\d+):)?(\d+):)?(\d+\.?\d*|\.\d+)$/

/**
 * Normalizes typed text into a draft, or rejects it when it holds anything but a time being typed.
 * @param {string} text Text in the field.
 * @param {boolean} signed Whether negative values are allowed.
 * @returns {string|null} Draft, or null when the text is rejected.
 */
export function normalizeSecondsDraft(text, signed) {
  const draft = text.replace(',', '.')
  return (signed ? SIGNED_DRAFT : UNSIGNED_DRAFT).test(draft) ? draft : null
}

/**
 * Parses a draft that holds a complete time. After a `:`, minutes and seconds stay below 60.
 * @param {string} draft Normalized draft.
 * @returns {number|null} Seconds, or null for an incomplete or invalid draft (empty, `-`, `5:`, `5:75`).
 */
export function parseSecondsDraft(draft) {
  const match = COMPLETE_CLOCK.exec(draft)
  if (match === null) return null
  const [, sign, hours, minutes, seconds] = match
  if ((hours !== undefined && Number(minutes) >= 60) || (minutes !== undefined && Number(seconds) >= 60)) return null
  const total = Number(hours ?? 0) * 3600 + Number(minutes ?? 0) * 60 + Number(seconds)
  return sign === '-' ? -total : total
}

/**
 * Formats seconds as plain seconds: up to three decimals, without trailing zeros.
 * @param {number} seconds Value.
 * @returns {string} Seconds text.
 */
export function formatSecondsText(seconds) {
  return String(Number(seconds.toFixed(SHOWN_DECIMALS)) || 0)
}

/**
 * Snaps a value to a frame grid: a time to the nearest frame, a duration to the
 * nearest whole number of frames, at least one.
 * @param {number} seconds Parsed value.
 * @param {{num: number, den: number}|null} fps Frame grid, or null to keep the value.
 * @param {'time'|'duration'} mode Kind of value.
 * @returns {number} Snapped seconds.
 */
export function snapSeconds(seconds, fps, mode) {
  if (fps === null) return seconds
  return mode === 'duration' ? snapDurationToFrames(seconds, fps) : framesToSeconds(secondsToFrames(seconds, fps), fps)
}
