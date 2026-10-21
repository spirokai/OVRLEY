/**
 * Pure helpers of the seconds input: what can be typed, how a draft parses, how
 * a value is shown, and how it snaps to a frame grid.
 *
 * A draft holds digits, at most one `.` and, for signed values, a leading `-`.
 * A comma is read as the decimal point.
 */

import { framesToSeconds, secondsToFrames, snapDurationToFrames } from './video-program/frame-grid'

/** Decimals shown for a value; three resolve every frame up to 500 fps. */
const SHOWN_DECIMALS = 3

const SIGNED_DRAFT = /^-?\d*\.?\d*$/
const UNSIGNED_DRAFT = /^\d*\.?\d*$/
const COMPLETE_NUMBER = /^-?(\d+\.?\d*|\.\d+)$/

/**
 * Normalizes typed text into a draft, or rejects it when it holds anything but a number being typed.
 * @param {string} text Text in the field.
 * @param {boolean} signed Whether negative values are allowed.
 * @returns {string|null} Draft, or null when the text is rejected.
 */
export function normalizeSecondsDraft(text, signed) {
  const draft = text.replace(',', '.')
  return (signed ? SIGNED_DRAFT : UNSIGNED_DRAFT).test(draft) ? draft : null
}

/**
 * Parses a draft that holds a complete number.
 * @param {string} draft Normalized draft.
 * @returns {number|null} Seconds, or null for an incomplete draft (empty, `-`, `.`).
 */
export function parseSecondsDraft(draft) {
  return COMPLETE_NUMBER.test(draft) ? Number(draft) : null
}

/**
 * Formats seconds as field text: up to three decimals, without trailing zeros.
 * @param {number} seconds Value.
 * @returns {string} Field text.
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
