/** Map widget edit rules shared by the inspector. */

import { parseInteger } from './widgetInput'

/** Matches the resize gesture's smallest edge. */
const MIN_MAP_EDGE = 8

/**
 * Seen from straight above, a building is only its roof, so the toggle needs a pitched camera.
 * @param {object} data Effective map data.
 * @returns {boolean} Whether this camera can show buildings.
 */
export function mapBuildingsAvailable(data) {
  return data.camera_mode === 'follow' && data.pitch > 0
}

/**
 * A circular map is square, so choosing one crops the longer side.
 * @param {object} data Effective map data.
 * @param {string} shape Chosen shape.
 * @returns {object} Widget patch.
 */
export function mapShapeUpdate(data, shape) {
  if (shape !== 'circle') return { shape }
  const diameter = Math.min(data.width, data.height)
  return { shape, width: diameter, height: diameter }
}

/**
 * @param {string} rawValue Typed diameter.
 * @param {number} diameter Current diameter, kept when the input isn't a number.
 * @returns {object} Widget patch.
 */
export function mapDiameterUpdate(rawValue, diameter) {
  const next = Math.max(parseInteger(rawValue, diameter), MIN_MAP_EDGE)
  return { width: next, height: next }
}

/**
 * @param {object} data Effective map data.
 * @returns {number} The largest corner radius this rectangle can show.
 */
export function maxMapCornerRadius(data) {
  return Math.min(data.width, data.height) / 2
}
