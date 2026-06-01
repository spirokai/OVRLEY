import { describe, expect, test } from 'vitest'

import { getGradientTriangleHeight } from '@/features/widget-preview/utils/formatUtils'

describe('getGradientTriangleHeight', () => {
  test('uses the full gradient angle when computing triangle height', () => {
    const width = 72
    const expected = width * Math.tan((10 * Math.PI) / 180)

    expect(getGradientTriangleHeight(10, width)).toBeCloseTo(expected, 6)
  })

  test('returns zero for missing or zero gradient values', () => {
    expect(getGradientTriangleHeight(null, 72)).toBe(0)
    expect(getGradientTriangleHeight(0, 72)).toBe(0)
  })
})
