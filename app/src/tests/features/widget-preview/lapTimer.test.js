import { describe, expect, test } from 'vitest'
import { buildLapTimerPreviewModel } from '@/features/widget-preview/widgets/lap-timer/model'
import { formatLapDuration, getLapTimerDisplayValue, getLapTimerState } from '@/features/widget-preview/widgets/lap-timer/lapTimer'

const activity = {
  sample_elapsed_seconds: [0, 2, 4, 6, 8, 10, 12, 14],
  lap_number: [-1, -1, 0, 0, 1, 1, 2, 2],
  lap_time_seconds: [null, null, 0, 2, 0, 2, 1, 3],
  lap_start_elapsed_seconds: [4, 8, 11],
  lap_durations_best_so_far_seconds: [4, 3],
  trim_end_seconds: 14,
}

const widget = {
  id: 'lap-timer-1',
  type: 'lap_timer',
  category: 'values',
  data: {
    font: 'Arial.ttf',
    font_size: 72,
    color: '#ffffff',
    opacity: 1,
    show_label: true,
    label: 'Best Lap',
    lap_timer_mode: 'best_lap',
  },
}

describe('lap timer preview state', () => {
  test('covers out-lap, first in-progress lap, boundary, and later lap states', () => {
    expect(getLapTimerState(activity, 1)).toEqual({ lapNumber: -1, currentLapTime: null, bestLapTime: null })
    expect(getLapTimerDisplayValue(activity, 5, 'current_lap')).toBe('00:01.00')
    expect(getLapTimerDisplayValue(activity, 5, 'best_lap')).toBe('00:01.00')
    expect(getLapTimerDisplayValue(activity, 8, 'best_lap')).toBe('00:04.00')
    expect(getLapTimerDisplayValue(activity, 13, 'best_lap')).toBe('00:03.00')
  })

  test('uses the original lap boundary when the scene starts mid-lap', () => {
    expect(getLapTimerDisplayValue(activity, 5, 'current_lap')).toBe('00:01.00')
    expect(getLapTimerDisplayValue(activity, 9, 'current_lap')).toBe('00:01.00')
    expect(getLapTimerDisplayValue(activity, 9, 'best_lap')).toBe('00:04.00')
  })

  test('formats durations beyond one hour and builds matching label/value content', () => {
    expect(formatLapDuration(3661.2)).toBe('01:01:01.20')
    const model = buildLapTimerPreviewModel({ widget, activity, previewSecond: 9 })
    expect(model.content.labelText).toBe('Best Lap')
    expect(model.content.valueText).toBe('00:04.00')
    expect(model.visualBounds).toHaveProperty('offsetY')
  })

  test('rejects an unsupported lap timer mode', () => {
    expect(() => getLapTimerDisplayValue(activity, 9, 'fastest_lap')).toThrow('Unsupported lap timer mode: fastest_lap')
  })
})
