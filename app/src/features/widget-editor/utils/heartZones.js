import { DISPLAY_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'

const { zoneModel } = DISPLAY_TYPE_DEFINITIONS.heart_zones

/**
 * Coggan zones for an LTHR, rounded to whole bpm. Each boundary is shared by its neighbours and
 * kept strictly ascending, so the result is always a valid manual zone list.
 *
 * @param {number} lthr - Lactate threshold heart rate in bpm.
 * @returns {{ from: number, to: number }[]} Five contiguous zones.
 */
export function deriveLthrZones(lthr) {
  const boundaries = []
  for (const share of zoneModel.lthrBoundaries) boundaries.push(Math.max(Math.round(lthr * share), (boundaries.at(-1) ?? -1) + 1))
  return boundaries.slice(0, -1).map((from, index) => ({ from, to: boundaries[index + 1] }))
}

/**
 * Switching to manual seeds the manual zones from the zones the previous mode computed.
 *
 * @param {object} variant - Current `display_variants.heart_zones`.
 * @param {'lthr'|'max_heartrate'|'manual'} zoneCalculation - Selected calculation.
 * @returns {object} Variant patch.
 */
export function buildZoneCalculationUpdate(variant, zoneCalculation) {
  if (zoneCalculation !== 'manual' || variant.zone_calculation === 'manual') return { zone_calculation: zoneCalculation }
  const lthr = variant.zone_calculation === 'lthr' ? variant.lthr : variant.max_heartrate * zoneModel.lthrShareOfMaxHeartrate
  return { zone_calculation: zoneCalculation, manual_zones: deriveLthrZones(lthr) }
}

/**
 * Replaces one bound of a manual heart-rate zone, clamped so zones stay ascending and non-overlapping.
 *
 * @param {{ from: number, to: number }[]} zones - Current manual zones.
 * @param {number} index - Zone index (0–4).
 * @param {'from'|'to'} bound - Edited bound.
 * @param {number} value - Requested bpm.
 * @returns {{ from: number, to: number }[]} Next manual zones.
 */
export function updateManualZone(zones, index, bound, value) {
  const zone = zones[index]
  const next =
    bound === 'from'
      ? { ...zone, from: Math.min(Math.max(value, index === 0 ? 0 : zones[index - 1].to), zone.to - 1) }
      : { ...zone, to: Math.max(Math.min(value, index === zones.length - 1 ? Infinity : zones[index + 1].from), zone.from + 1) }
  return zones.map((item, itemIndex) => (itemIndex === index ? next : item))
}
