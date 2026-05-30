import { describe, expect, test } from 'vitest'

import {
  headingOffset,
  isCardinalDegree,
  cardinalLabelForDegree,
  visibleTicks,
  visibleLabels,
  chevronVertices,
} from '@/features/widget-preview/utils/headingGeometry'

describe('headingOffset', () => {
  test('returns 0 for heading 0', () => {
    expect(headingOffset(0, 5)).toBe(0)
  })

  test('scales heading by pixels_per_degree', () => {
    expect(headingOffset(90, 5)).toBe(450)
    expect(headingOffset(360, 5)).toBe(1800)
  })
})

describe('isCardinalDegree', () => {
  test('detects 45-degree multiples', () => {
    expect(isCardinalDegree(0)).toBe(true)
    expect(isCardinalDegree(45)).toBe(true)
    expect(isCardinalDegree(90)).toBe(true)
    expect(isCardinalDegree(180)).toBe(true)
    expect(isCardinalDegree(270)).toBe(true)
    expect(isCardinalDegree(315)).toBe(true)
  })

  test('rejects non-cardinal degrees', () => {
    expect(isCardinalDegree(15)).toBe(false)
    expect(isCardinalDegree(30)).toBe(false)
    expect(isCardinalDegree(100)).toBe(false)
  })
})

describe('cardinalLabelForDegree', () => {
  test('returns correct labels for all 8 cardinal positions', () => {
    expect(cardinalLabelForDegree(0)).toBe('N')
    expect(cardinalLabelForDegree(45)).toBe('NE')
    expect(cardinalLabelForDegree(90)).toBe('E')
    expect(cardinalLabelForDegree(135)).toBe('SE')
    expect(cardinalLabelForDegree(180)).toBe('S')
    expect(cardinalLabelForDegree(225)).toBe('SW')
    expect(cardinalLabelForDegree(270)).toBe('W')
    expect(cardinalLabelForDegree(315)).toBe('NW')
  })

  test('returns null for non-cardinal degrees', () => {
    expect(cardinalLabelForDegree(30)).toBeNull()
    expect(cardinalLabelForDegree(100)).toBeNull()
  })
})

describe('visibleTicks', () => {
  test('produces ticks starting at degree 0 when heading is 0', () => {
    const ticks = visibleTicks(0, 5, 200, 15, 3, true, true)
    expect(ticks.length).toBeGreaterThan(0)
    expect(ticks[0].degree).toBe(0)
    expect(ticks[0].x).toBeCloseTo(0, 1)
  })

  test('includes major ticks at configured interval', () => {
    const ticks = visibleTicks(0, 5, 200, 15, 3, true, true)
    const majors = ticks.filter((t) => t.isMajor)
    expect(majors.length).toBeGreaterThan(0)
    expect(majors[0].degree).toBe(0)
    expect(majors[1].degree).toBe(15)
  })

  test('includes minor ticks between majors', () => {
    const ticks = visibleTicks(0, 5, 200, 15, 3, true, true)
    const minors = ticks.filter((t) => !t.isMajor)
    expect(minors.length).toBeGreaterThan(0)
    expect(minors[0].degree).toBe(5)
  })

  test('respects show_major_ticks flag', () => {
    const all = visibleTicks(0, 5, 200, 15, 3, true, true)
    const majorOnly = visibleTicks(0, 5, 200, 15, 3, true, false)
    expect(all.length).toBeGreaterThan(majorOnly.length)
    expect(majorOnly.every((t) => t.isMajor)).toBe(true)
  })

  test('respects show_minor_ticks flag', () => {
    const all = visibleTicks(0, 5, 200, 15, 3, true, true)
    const minorOnly = visibleTicks(0, 5, 200, 15, 3, false, true)
    expect(all.length).toBeGreaterThan(minorOnly.length)
    expect(minorOnly.every((t) => !t.isMajor || t.isCardinal)).toBe(true)
  })

  test('keeps cardinal ticks visible when regular ticks are hidden', () => {
    const ticks = visibleTicks(0, 5, 500, 15, 3, false, false)
    expect(ticks.map((tick) => tick.degree)).toEqual([0, 45, 90])
    expect(ticks.every((tick) => tick.isCardinal)).toBe(true)
  })

  test('marks cardinal degrees correctly', () => {
    const ticks = visibleTicks(0, 5, 200, 15, 3, true, false)
    const tick0 = ticks.find((t) => t.degree === 0)
    const tick15 = ticks.find((t) => t.degree === 15)
    expect(tick0.isCardinal).toBe(true)
    expect(tick15.isCardinal).toBe(false)
  })

  test('wraps ticks at 360 boundary', () => {
    // At heading=350, ppd=5, width=100: degree=0 should wrap into view
    const ticks = visibleTicks(350, 5, 100, 15, 3, true, true)
    const tick0 = ticks.find((t) => t.degree === 0)
    expect(tick0).toBeDefined()
    expect(tick0.x).toBeCloseTo(50, 1) // (0*5 - 350*5) % 1800 = -1750 % 1800 = 50
  })

  test('returns empty for invalid dimensions', () => {
    expect(visibleTicks(0, 0, 200, 15, 3, true, true)).toEqual([])
    expect(visibleTicks(0, 5, 0, 15, 3, true, true)).toEqual([])
  })
})

describe('visibleLabels', () => {
  test('shows cardinal labels at 45-degree positions', () => {
    const ticks = [
      { degree: 0, x: 0, isCardinal: true, isMajor: true },
      { degree: 15, x: 75, isCardinal: false, isMajor: true },
      { degree: 30, x: 150, isCardinal: false, isMajor: true },
    ]
    const labels = visibleLabels(ticks, true, true)
    expect(labels.length).toBe(3)
    expect(labels[0].text).toBe('N')
    expect(labels[0].isMajorLabel).toBe(true)
    expect(labels[1].text).toBe('15')
    expect(labels[1].isMajorLabel).toBe(false)
    expect(labels[2].text).toBe('30')
  })

  test('cardinal labels take priority over numeric at same position', () => {
    const ticks = [{ degree: 0, x: 0, isCardinal: true, isMajor: true }]
    const labels = visibleLabels(ticks, true, true)
    expect(labels.length).toBe(1)
    expect(labels[0].text).toBe('N')
    expect(labels[0].isMajorLabel).toBe(true)
  })

  test('respects show_minor_labels flag', () => {
    const ticks = [
      { degree: 0, x: 0, isCardinal: true, isMajor: true },
      { degree: 15, x: 75, isCardinal: false, isMajor: true },
    ]
    const none = visibleLabels(ticks, false, false)
    expect(none.length).toBe(0)

    const minorOnly = visibleLabels(ticks, true, false)
    expect(minorOnly.length).toBe(2)
    expect(minorOnly[0].text).toBe('0')
  })

  test('respects show_major_labels flag', () => {
    const ticks = [
      { degree: 0, x: 0, isCardinal: true, isMajor: true },
      { degree: 15, x: 75, isCardinal: false, isMajor: true },
    ]
    const majorOnly = visibleLabels(ticks, false, true)
    expect(majorOnly.length).toBe(1)
    expect(majorOnly[0].text).toBe('N')
  })
})

describe('chevronVertices', () => {
  test('top chevron points down', () => {
    const verts = chevronVertices(200, 0, 10, true)
    expect(verts[0].x).toBeCloseTo(194, 1)
    expect(verts[1].x).toBeCloseTo(206, 1)
    expect(verts[2].y).toBeCloseTo(10, 1)
  })

  test('bottom chevron points up', () => {
    const verts = chevronVertices(200, 80, 10, false)
    expect(verts[2].y).toBeCloseTo(70, 1)
  })
})
