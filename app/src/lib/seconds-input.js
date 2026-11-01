/**
 * Pure helpers of the seconds input: what can be typed, how a draft parses, how
 * a value is shown, and how it snaps to a frame grid.
 *
 * A draft is a sum of terms (`1:39:25+3:45`, `5965-225`). Each term is a clock
 * (`5:30`, `1:01:40.5`) or plain seconds (`330`): digits, up to two `:` and one
 * `.` in the last part. Signed values can start with a `-`. Spaces are allowed
 * around `+` and `-`, and a comma is read as the decimal point.
 */

import { framesToSeconds, secondsToFrames, snapDurationToFrames } from './video-program/frame-grid'

/** Decimals shown for a value; three resolve every frame up to 500 fps. */
const SHOWN_DECIMALS = 3

const DRAFT_TERM = String.raw`(?:\d*:){0,2}\d*\.?\d*`
// The first term starts with a digit, `:` or `.`, so only a sign can come before it.
const DRAFT_SUM = String.raw`(?:(?=[\d:.])${DRAFT_TERM}(?:\s*[+-]\s*${DRAFT_TERM})*)?`
const SIGNED_DRAFT = new RegExp(String.raw`^-?${DRAFT_SUM}\s*$`)
const UNSIGNED_DRAFT = new RegExp(String.raw`^${DRAFT_SUM}\s*$`)
const COMPLETE_SUM = /^(-?)([^+-]+)((?:[+-][^+-]+)*)$/
const SUM_TERM = /([+-])([^+-]+)/g
const COMPLETE_CLOCK = /^(?:(?:(\d+):)?(\d+):)?(\d+\.?\d*|\.\d+)$/

/**
 * Normalizes typed text into a draft, or rejects it when it holds anything but a time being typed.
 * @param {string} text Text in the field.
 * @param {boolean} signed Whether negative values are allowed.
 * @returns {string|null} Draft, or null when the text is rejected.
 */
export function normalizeSecondsDraft(text, signed) {
  const draft = text.replaceAll(',', '.')
  return (signed ? SIGNED_DRAFT : UNSIGNED_DRAFT).test(draft) ? draft : null
}

function parseClockTerm(term) {
  const match = COMPLETE_CLOCK.exec(term)
  if (match === null) return null
  const [, hours, minutes, seconds] = match
  if ((hours !== undefined && Number(minutes) >= 60) || (minutes !== undefined && Number(seconds) >= 60)) return null
  return Number(hours ?? 0) * 3600 + Number(minutes ?? 0) * 60 + Number(seconds)
}

/**
 * Parses a draft that holds a complete sum of times. After a `:`, minutes and seconds stay below 60.
 * @param {string} draft Normalized draft.
 * @returns {number|null} Seconds, or null for an incomplete or invalid draft (empty, `-`, `5:`, `5:75`, `5+`).
 */
export function parseSecondsDraft(draft) {
  const match = COMPLETE_SUM.exec(draft.replace(/\s/g, ''))
  if (match === null) return null
  const [, sign, first, rest] = match
  const firstSeconds = parseClockTerm(first)
  if (firstSeconds === null) return null
  let total = sign === '-' ? -firstSeconds : firstSeconds
  for (const [, operator, term] of rest.matchAll(SUM_TERM)) {
    const seconds = parseClockTerm(term)
    if (seconds === null) return null
    total += operator === '+' ? seconds : -seconds
  }
  return total
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
 * Seconds one arrow key press adds or removes: 1, 10 with Shift, one frame with Alt.
 * @param {{shiftKey: boolean, altKey: boolean, ctrlKey: boolean, metaKey: boolean}} modifiers Modifier keys held.
 * @param {{num: number, den: number}|null} fps Frame grid of the value, or null.
 * @returns {number|null} Step, or null when the keys keep their usual meaning (Ctrl, Cmd, Shift+Alt, Alt without a frame grid).
 */
export function arrowStepSeconds({ shiftKey, altKey, ctrlKey, metaKey }, fps) {
  if (ctrlKey || metaKey || (shiftKey && altKey)) return null
  if (altKey) return fps === null ? null : framesToSeconds(1, fps)
  return shiftKey ? 10 : 1
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
