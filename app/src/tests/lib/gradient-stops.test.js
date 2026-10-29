import { describe, expect, test } from 'vite-plus/test'

import {
  addGradientStop,
  moveGradientStop,
  removeGradientStop,
  sampleGradientColor,
  setGradientStopColor,
  toGradientOffset,
  toLinearGradientCss,
} from '@/lib/gradient-stops'

const stops = [
  { offset: 1, color: '#ffffff' },
  { offset: 0, color: '#000000' },
]

describe('gradient stops', () => {
  test('builds a CSS gradient ordered by offset', () => {
    expect(toLinearGradientCss(stops)).toBe('linear-gradient(90deg, #000000 0%, #ffffff 100%)')
  })

  test('clamps and rounds offsets', () => {
    expect(toGradientOffset(-0.2)).toBe(0)
    expect(toGradientOffset(1.4)).toBe(1)
    expect(toGradientOffset(0.123456)).toBe(0.123)
  })

  test('samples interpolated colors and clamps outside the stop range', () => {
    expect(sampleGradientColor(stops, 0.5)).toBe('#808080')
    expect(sampleGradientColor([{ offset: 0.2, color: '#ff0000' }, ...stops.slice(0, 1)], 0.1)).toBe('#ff0000')
  })

  test('adds a stop colored to match the gradient at its offset', () => {
    expect(addGradientStop(stops, 0.25)).toEqual([...stops, { offset: 0.25, color: '#404040' }])
  })

  test('moves, recolors, and removes stops by index without reordering', () => {
    expect(moveGradientStop(stops, 1, 0.4)).toEqual([stops[0], { offset: 0.4, color: '#000000' }])
    expect(setGradientStopColor(stops, 0, '#ff0000')).toEqual([{ offset: 1, color: '#ff0000' }, stops[1]])
    expect(removeGradientStop(stops, 0)).toEqual([stops[1]])
  })
})
