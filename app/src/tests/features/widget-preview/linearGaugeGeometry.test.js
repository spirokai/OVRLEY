import { describe, expect, test } from 'vitest'
import {
  formatLinearGaugeLabel,
  getFillPercentage,
  getLinearFillRect,
  getLinearGaugeRange,
  getLinearGaugeLayout,
} from '@/features/widget-preview/utils/linearGaugeGeometry'

describe('linearGaugeGeometry', () => {
  test('fill percentage clamps values into the configured range', () => {
    expect(getFillPercentage(50, 0, 100)).toBe(0.5)
    expect(getFillPercentage(-10, 0, 100)).toBe(0)
    expect(getFillPercentage(120, 0, 100)).toBe(1)
    expect(getFillPercentage(10, 10, 10)).toBe(0)
  })

  test('linear fill rect matches horizontal and vertical backend geometry', () => {
    expect(getLinearFillRect({ x: 10, y: 20, width: 200, height: 40, fill: 0.25, orientation: 'horizontal' })).toEqual({
      x: 10,
      y: 20,
      width: 50,
      height: 40,
    })
    expect(getLinearFillRect({ x: 10, y: 20, width: 200, height: 40, fill: 0.25, orientation: 'vertical' })).toEqual({
      x: 10,
      y: 50,
      width: 200,
      height: 10,
    })
  })

  test('linear fill rect stays inside the track border', () => {
    expect(
      getLinearFillRect({
        x: 10,
        y: 20,
        width: 200,
        height: 40,
        fill: 0.25,
        orientation: 'horizontal',
        borderThickness: 2,
      }),
    ).toEqual({
      x: 12,
      y: 22,
      width: 49,
      height: 36,
    })
    expect(
      getLinearFillRect({
        x: 10,
        y: 20,
        width: 200,
        height: 40,
        fill: 0.25,
        orientation: 'vertical',
        borderThickness: 2,
      }),
    ).toEqual({
      x: 12,
      y: 49,
      width: 196,
      height: 9,
    })
  })

  test('range derives from activity values and falls back to preview placeholder', () => {
    expect(getLinearGaugeRange([10, null, 30, 50])).toEqual({ min: 10, max: 50 })
    expect(getLinearGaugeRange([])).toEqual({ min: 0, max: 100 })
  })

  test('linear layout uses 50 percent placeholder fill without activity values', () => {
    expect(getLinearGaugeLayout({ value: null, values: [], width: 200, height: 40 }).fill).toBe(0.5)
  })

  test('labels keep integer ranges compact and decimal ranges visible', () => {
    expect(formatLinearGaugeLabel(10)).toBe('10')
    expect(formatLinearGaugeLabel(10.24)).toBe('10.2')
  })
})
