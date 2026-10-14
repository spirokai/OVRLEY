import { describe, expect, test } from 'vitest'
import {
  getBarGeometry,
  getLinearBarGapMax,
  getLinearTrackCornerRadiusMax,
  getArcTrackCornerRadiusMax,
  getSuggestedArcBarGeometry,
  getSuggestedLinearBarGeometry,
} from '@/features/widget-editor/utils/gaugeGeometry'

describe('gaugeBarGeometry', () => {
  test('suggests a fixed bar count independent of track span', () => {
    const compact = getSuggestedLinearBarGeometry({ width: 100, height: 20, orientation: 'horizontal' })
    const wide = getSuggestedLinearBarGeometry({ width: 200, height: 20, orientation: 'horizontal' })
    expect(wide.count).toBe(compact.count)
    expect(wide.gap).toBe(4)
    expect(compact.gap).toBeLessThanOrEqual(4)
    expect(Object.values(compact).every(Number.isInteger)).toBe(true)
    expect(Object.values(wide).every(Number.isInteger)).toBe(true)
  })

  test('suggests only whole-number arc geometry and limits', () => {
    const suggestion = getSuggestedArcBarGeometry({ radius: 73.5, sweepAngle: 137, trackThickness: 13.3, borderThickness: 1.2 })
    expect(Object.values(suggestion).every(Number.isInteger)).toBe(true)
  })

  test('scales the whole-pixel gap maximum with track span', () => {
    const compactMax = getLinearBarGapMax({ width: 100, height: 20, orientation: 'horizontal', bar_count: 5 })
    const wideMax = getLinearBarGapMax({ width: 1000, height: 20, orientation: 'horizontal', bar_count: 5 })
    expect(compactMax).toBe(22)
    expect(wideMax).toBe(247)
  })

  test('limits a segmented linear corner radius by one bar width', () => {
    expect(
      getLinearTrackCornerRadiusMax({
        width: 100,
        height: 20,
        orientation: 'horizontal',
        track_fill_style: 'bars',
        bar_count: 10,
        bar_gap: 4,
      }),
    ).toBe(3)
  })

  test('limits a segmented arc corner radius by one segment extent', () => {
    expect(
      getArcTrackCornerRadiusMax({
        radius: 50,
        sweepAngle: 180,
        trackThickness: 12,
        track_fill_style: 'bars',
        bar_count: 20,
        bar_gap: 4,
      }),
    ).toBe(2)
  })

  test('clamps an explicit gap without overflowing the span', () => {
    expect(getBarGeometry({ span: 20, bar_count: 5, bar_gap: 10 })).toEqual({ count: 5, gap: 2.5, extent: 2 })
  })
})
