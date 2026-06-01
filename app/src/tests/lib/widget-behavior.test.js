import { describe, expect, test } from 'vitest'

import { isTextDisplayType, isHeadingTapeWidget, isPlotLikeWidget } from '@/lib/widget-behavior'

describe('widget behavior helpers', () => {
  test('treats missing and explicit text display types as metric text mode', () => {
    expect(isTextDisplayType('text')).toBe(true)
    expect(isTextDisplayType(undefined)).toBe(true)
    expect(isTextDisplayType(null)).toBe(true)
    expect(isTextDisplayType('heading_tape')).toBe(false)
  })

  test('recognizes heading tape widgets without changing plain heading text widgets', () => {
    expect(
      isHeadingTapeWidget({
        type: 'heading',
        data: { display_type: 'heading_tape' },
      }),
    ).toBe(true)

    expect(
      isHeadingTapeWidget({
        type: 'heading',
        data: {},
      }),
    ).toBe(false)
  })

  test('treats heading tape values as plot-like for editor interactions', () => {
    expect(
      isPlotLikeWidget({
        category: 'values',
        type: 'heading',
        data: { display_type: 'heading_tape' },
      }),
    ).toBe(true)

    expect(
      isPlotLikeWidget({
        category: 'values',
        type: 'heading',
        data: { display_type: 'text' },
      }),
    ).toBe(false)
  })

  test('treats future non-text metric display types as plot-like too', () => {
    expect(
      isPlotLikeWidget({
        category: 'values',
        type: 'speed',
        data: { display_type: 'linear' },
      }),
    ).toBe(true)

    expect(
      isPlotLikeWidget({
        category: 'values',
        type: 'power',
        data: { display_type: 'arc' },
      }),
    ).toBe(true)
  })
})
