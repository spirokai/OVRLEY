/** Switch unit fields between the metric and imperial systems through the manifest's [metric, imperial] pairs. */

import { convertAltitudeInputValue } from './altitude'
import { UNIT_CONVERSIONS, UNIT_FIELDS, UNIT_TOGGLES, WIDGET_COLLECTIONS, WIDGET_TYPE_DEFINITIONS } from './widget-catalog'

/** Supported unit systems; manifest defaults are metric. */
export const UNIT_SYSTEMS = ['metric', 'imperial']

/** @param {string} system Unit system from user input. @returns {string} The same system, or throws when unknown. */
export function requireUnitSystem(system) {
  if (!UNIT_SYSTEMS.includes(system)) throw new Error(`Unknown unit system: ${system}`)
  return system
}

/** A metric's display unit only switches to a unit the metric offers, so speed keeps m/s and vertical oscillation keeps cm. */
function acceptsUnit(data, field, unit) {
  return field !== 'display_unit' || WIDGET_TYPE_DEFINITIONS[data.value].supportedDisplayUnits.some((option) => option.value === unit)
}

function convertUnit(data, field, system) {
  const [from, to] = system === 'imperial' ? [0, 1] : [1, 0]
  const pair = UNIT_CONVERSIONS.find((candidate) => candidate[from] === data[field] && acceptsUnit(data, field, candidate[to]))
  return pair === undefined ? data[field] : pair[to]
}

/**
 * Converts whichever unit fields and label toggles an object carries: widget data, a display variant, or display defaults.
 * Widget data names its type in `value`; a starting altitude is converted with the unit its altitude source follows.
 * @param {object} data Widget data or a part of it.
 * @param {string} system Target unit system.
 * @returns {object} The object in the target system.
 */
export function convertUnits(data, system) {
  const next = { ...data }
  for (const field of UNIT_FIELDS) {
    if (Object.hasOwn(data, field)) next[field] = convertUnit(data, field, system)
  }
  if (Object.hasOwn(data, 'starting_altitude')) {
    const { unitField } = WIDGET_TYPE_DEFINITIONS[data.value].altitudeSource
    next.starting_altitude = convertAltitudeInputValue(data.starting_altitude, data[unitField], next[unitField])
  }
  for (const [metric, imperial] of UNIT_TOGGLES) {
    if (Object.hasOwn(data, metric) && data[metric] !== data[imperial]) {
      next[metric] = system === 'metric'
      next[imperial] = system === 'imperial'
    }
  }
  if (Object.hasOwn(data, 'display_variants')) {
    next.display_variants = {}
    for (const [type, variant] of Object.entries(data.display_variants)) next.display_variants[type] = convertUnits(variant, system)
  }
  return next
}

/**
 * @param {object} config Canonical overlay config.
 * @param {string} system Target unit system.
 * @returns {object} Config with every widget in the target system.
 */
export function convertConfigUnits(config, system) {
  const next = { ...config }
  for (const category of Object.keys(WIDGET_COLLECTIONS)) next[category] = config[category].map((data) => convertUnits(data, system))
  return next
}
