import { buildZoneCalculationUpdate, deriveLthrZones, updateManualZone } from '@/features/widget-editor/utils/heartZones'
import { DISPLAY_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'

const DEFAULTS = DISPLAY_TYPE_DEFINITIONS.heart_zones.variantDefaults

test('derived LTHR zones are contiguous whole bpm and match the manifest manual defaults', () => {
  expect(deriveLthrZones(165)).toEqual(DEFAULTS.manual_zones)
})

test('derived zones stay strictly ascending when rounding collapses boundaries', () => {
  expect(deriveLthrZones(3).map(({ from, to }) => [from, to])).toEqual([
    [1, 2],
    [2, 3],
    [3, 4],
    [4, 5],
    [5, 6],
  ])
})

test('switching to manual seeds manual zones from the previous calculation', () => {
  expect(buildZoneCalculationUpdate({ ...DEFAULTS, zone_calculation: 'lthr', lthr: 165 }, 'manual')).toEqual({
    zone_calculation: 'manual',
    manual_zones: deriveLthrZones(165),
  })
  expect(buildZoneCalculationUpdate({ ...DEFAULTS, zone_calculation: 'max_heartrate', max_heartrate: 200 }, 'manual').manual_zones).toEqual(
    deriveLthrZones(180),
  )
  expect(buildZoneCalculationUpdate({ ...DEFAULTS, zone_calculation: 'manual' }, 'lthr')).toEqual({ zone_calculation: 'lthr' })
})

const ZONES = [
  { from: 80, to: 110 },
  { from: 110, to: 130 },
  { from: 130, to: 150 },
  { from: 150, to: 170 },
  { from: 170, to: 190 },
]

test('manual zone edits replace one bound without mutating the zones', () => {
  const next = updateManualZone(ZONES, 2, 'to', 145)
  expect(next[2]).toEqual({ from: 130, to: 145 })
  expect(ZONES[2]).toEqual({ from: 130, to: 150 })
  expect(next.filter((zone, index) => zone !== ZONES[index])).toHaveLength(1)
})

test('manual zone edits stay ascending and non-overlapping', () => {
  expect(updateManualZone(ZONES, 1, 'from', 90)[1].from).toBe(110)
  expect(updateManualZone(ZONES, 1, 'to', 140)[1].to).toBe(130)
  expect(updateManualZone(ZONES, 1, 'to', 100)[1].to).toBe(111)
  expect(updateManualZone(ZONES, 0, 'from', -5)[0].from).toBe(0)
  expect(updateManualZone(ZONES, 4, 'to', 230)[4].to).toBe(230)
})
