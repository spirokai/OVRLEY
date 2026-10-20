/**
 * Pure helpers for editing linear gradient stops.
 *
 * A gradient is an array of `{ offset, color }` stops where `offset` is in [0, 1]
 * and `color` is a normalized `#rrggbb` hex string. Array order is insertion order;
 * rendering sorts by offset so dragged stops keep a stable index.
 */

export const MIN_GRADIENT_STOPS = 2
export const MAX_GRADIENT_STOPS = 5

const OFFSET_PRECISION = 1000

/**
 * Clamps an offset into [0, 1] and rounds it to a stable precision.
 *
 * @param {number} offset - Raw offset.
 * @returns {number} Offset in [0, 1].
 */
export function toGradientOffset(offset) {
  return Math.round(Math.min(1, Math.max(0, offset)) * OFFSET_PRECISION) / OFFSET_PRECISION
}

/**
 * Returns a copy of the stops ordered by offset.
 *
 * @param {{ offset: number, color: string }[]} stops - Gradient stops.
 * @returns {{ offset: number, color: string }[]} Sorted copy.
 */
export function sortGradientStops(stops) {
  return [...stops].sort((a, b) => a.offset - b.offset)
}

/**
 * Builds a CSS linear-gradient for the stops.
 *
 * @param {{ offset: number, color: string }[]} stops - Gradient stops.
 * @param {string} [angle] - CSS gradient angle.
 * @returns {string} CSS `linear-gradient(...)` value.
 */
export function toLinearGradientCss(stops, angle = '90deg') {
  const colorStops = sortGradientStops(stops).map(({ offset, color }) => `${color} ${offset * 100}%`)
  return `linear-gradient(${angle}, ${colorStops.join(', ')})`
}

/**
 * Splits a `#rrggbb` color into channels.
 *
 * @param {string} hex - Normalized hex color.
 * @returns {number[]} `[r, g, b]` channels.
 */
function hexToChannels(hex) {
  const value = Number.parseInt(hex.slice(1), 16)
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255]
}

/**
 * Joins channels into a `#rrggbb` color.
 *
 * @param {number[]} channels - `[r, g, b]` channels.
 * @returns {string} Normalized hex color.
 */
function channelsToHex(channels) {
  return `#${channels.map((channel) => Math.round(channel).toString(16).padStart(2, '0')).join('')}`
}

/**
 * Samples the interpolated gradient color at an offset.
 *
 * @param {{ offset: number, color: string }[]} stops - Gradient stops.
 * @param {number} offset - Offset in [0, 1].
 * @returns {string} Hex color at the offset.
 */
export function sampleGradientColor(stops, offset) {
  const sorted = sortGradientStops(stops)
  const first = sorted[0]
  const last = sorted.at(-1)

  if (offset <= first.offset) return first.color
  if (offset >= last.offset) return last.color

  const upperIndex = sorted.findIndex((stop) => stop.offset >= offset)
  const lower = sorted[upperIndex - 1]
  const upper = sorted[upperIndex]
  const t = (offset - lower.offset) / (upper.offset - lower.offset)
  const lowerChannels = hexToChannels(lower.color)
  const upperChannels = hexToChannels(upper.color)

  return channelsToHex(lowerChannels.map((channel, index) => channel + (upperChannels[index] - channel) * t))
}

/**
 * Adds a stop at the offset, colored to match the gradient at that point.
 *
 * @param {{ offset: number, color: string }[]} stops - Gradient stops.
 * @param {number} offset - Offset in [0, 1].
 * @returns {{ offset: number, color: string }[]} Next stops.
 */
export function addGradientStop(stops, offset) {
  return [...stops, { offset, color: sampleGradientColor(stops, offset) }]
}

/**
 * Moves the stop at the index to a new offset.
 *
 * @param {{ offset: number, color: string }[]} stops - Gradient stops.
 * @param {number} index - Stop index.
 * @param {number} offset - Offset in [0, 1].
 * @returns {{ offset: number, color: string }[]} Next stops.
 */
export function moveGradientStop(stops, index, offset) {
  return stops.map((stop, stopIndex) => (stopIndex === index ? { ...stop, offset } : stop))
}

/**
 * Recolors the stop at the index.
 *
 * @param {{ offset: number, color: string }[]} stops - Gradient stops.
 * @param {number} index - Stop index.
 * @param {string} color - Hex color.
 * @returns {{ offset: number, color: string }[]} Next stops.
 */
export function setGradientStopColor(stops, index, color) {
  return stops.map((stop, stopIndex) => (stopIndex === index ? { ...stop, color } : stop))
}

/**
 * Removes the stop at the index.
 *
 * @param {{ offset: number, color: string }[]} stops - Gradient stops.
 * @param {number} index - Stop index.
 * @returns {{ offset: number, color: string }[]} Next stops.
 */
export function removeGradientStop(stops, index) {
  return stops.filter((_, stopIndex) => stopIndex !== index)
}
