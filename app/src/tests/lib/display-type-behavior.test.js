import { describe, expect, test } from 'vitest'
import { isTextDisplayType, isBoxedMetricWidget } from '@/lib/widget/display-type-behavior'

describe('display-type-behavior', () => {
  test('isTextDisplayType', () => {
    expect(isTextDisplayType('text')).toBe(true)
    expect(isTextDisplayType(undefined)).toBe(true)
    expect(isTextDisplayType(null)).toBe(true)
    expect(isTextDisplayType('heading_tape')).toBe(false)
    expect(isTextDisplayType('linear')).toBe(false)
  })

  test('isBoxedMetricWidget derives from display_type for standard metrics', () => {
    expect(isBoxedMetricWidget({ category: 'values', type: 'speed', data: { display_type: 'text' } })).toBe(false)
    expect(isBoxedMetricWidget({ category: 'values', type: 'speed', data: { display_type: 'linear' } })).toBe(true)
    expect(isBoxedMetricWidget({ category: 'values', type: 'speed', data: {} })).toBe(false)
  })

  test('isBoxedMetricWidget falls back to category for non-metric widgets', () => {
    expect(isBoxedMetricWidget({ category: 'plots', type: 'route', data: {} })).toBe(true)
    expect(isBoxedMetricWidget({ category: 'labels', type: 'label', data: {} })).toBe(false)
  })

  test('isBoxedMetricWidget handles null/undefined', () => {
    expect(isBoxedMetricWidget(null)).toBe(false)
    expect(isBoxedMetricWidget(undefined)).toBe(false)
  })
})
