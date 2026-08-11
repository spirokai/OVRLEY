import { describe, expect, test } from 'vitest'
import { buildLapTimerPreviewModel, prepareLapLogPreview } from '@/features/widget-preview/widgets/lap-timer/model'
import {
  formatLapDelta,
  formatLapDuration,
  getLapLogDisplayState,
  getLapTimerDisplayValue,
  getLapTimerState,
} from '@/features/widget-preview/widgets/lap-timer/lapTimer'

const activity = {
  sample_elapsed_seconds: [0, 2, 4, 6, 8, 10, 12, 14],
  lap_number: [-1, -1, 0, 0, 1, 1, 2, 2],
  lap_time_seconds: [null, null, 0, 2, 0, 2, 1, 3],
  delta_to_best_lap_seconds: [null, null, null, null, 0, -0.25, 0, 0.5],
  lap_start_elapsed_seconds: [4, 8, 11],
  lap_durations_seconds: [4, 3],
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
    label_font: 'Teko.ttf',
    label_font_size: 24,
    label_color: '#abcdef',
    opacity: 1,
    show_label: true,
    label: 'Best Lap',
    lap_timer_mode: 'best_lap',
    positive_delta_color: '#00ff00',
    negative_delta_color: '#ff0000',
  },
}

describe('lap timer preview state', () => {
  test('covers out-lap, first in-progress lap, boundary, and later lap states', () => {
    expect(getLapTimerState(activity, 1)).toEqual({ lapNumber: -1, completedLapCount: 0, currentLapTime: null, bestLapTime: null })
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
    expect(model.content.labelText).toBe('BEST LAP')
    expect(model.content.valueText).toBe('00:04.00')
    expect(model.visualBounds).toHaveProperty('offsetY')
  })

  test('formats no reference, exact zero, faster, and slower deltas with matching colors', () => {
    expect(formatLapDelta(null)).toBe('+0.00')
    expect(formatLapDelta(0)).toBe('+0.00')
    expect(getLapTimerDisplayValue(activity, 5, 'delta')).toBe('+0.00')
    expect(getLapTimerDisplayValue(activity, 9, 'delta')).toBe('-0.13')
    expect(getLapTimerDisplayValue(activity, 13, 'delta')).toBe('+0.25')

    const deltaWidget = { ...widget, data: { ...widget.data, label: 'custom delta', lap_timer_mode: 'delta' } }
    const noReferenceModel = buildLapTimerPreviewModel({ widget: deltaWidget, activity, previewSecond: 5 })
    const zeroModel = buildLapTimerPreviewModel({ widget: deltaWidget, activity, previewSecond: 8 })
    const fasterModel = buildLapTimerPreviewModel({ widget: deltaWidget, activity, previewSecond: 9 })
    const slowerModel = buildLapTimerPreviewModel({ widget: deltaWidget, activity, previewSecond: 13 })
    expect(noReferenceModel.content).toMatchObject({ valueText: '+0.00', valueColor: '#ff0000' })
    expect(zeroModel.content).toMatchObject({ valueText: '+0.00', valueColor: '#ff0000' })
    expect(fasterModel.content).toMatchObject({ labelText: 'CUSTOM DELTA', valueText: '-0.13', valueColor: '#ff0000' })
    expect(slowerModel.content).toMatchObject({ labelText: 'CUSTOM DELTA', valueText: '+0.25', valueColor: '#00ff00' })
  })

  test('builds activity-wide lap-log history for out-lap, first lap, later completions, and a mid-session preview', () => {
    expect(getLapLogDisplayState(activity, 1)).toEqual({ completedRows: [], currentRow: null })
    expect(getLapLogDisplayState(activity, 5)).toEqual({
      completedRows: [],
      currentRow: { lapText: '1', timeText: '00:01.00', deltaText: '+0.00', useNegativeDeltaColor: true },
    })
    expect(getLapLogDisplayState(activity, 8)).toEqual({
      completedRows: [{ lapText: '1', timeText: '00:04.00', deltaText: '+0.00', useNegativeDeltaColor: true }],
      currentRow: { lapText: '2', timeText: '00:00.00', deltaText: '+0.00', useNegativeDeltaColor: true },
    })
    expect(getLapLogDisplayState(activity, 13)).toEqual({
      completedRows: [
        { lapText: '2', timeText: '00:03.00', deltaText: '+0.00', useNegativeDeltaColor: true },
        { lapText: '1', timeText: '00:04.00', deltaText: '+1.00', useNegativeDeltaColor: false },
      ],
      currentRow: { lapText: '3', timeText: '00:02.00', deltaText: '+0.25', useNegativeDeltaColor: false },
    })

    const returnedToPreLap = { ...activity, lap_number: [...activity.lap_number.slice(0, 6), -1, -1] }
    expect(getLapLogDisplayState(returnedToPreLap, 13)).toEqual({
      completedRows: [
        { lapText: '2', timeText: '00:03.00', deltaText: '+0.00', useNegativeDeltaColor: true },
        { lapText: '1', timeText: '00:04.00', deltaText: '+1.00', useNegativeDeltaColor: false },
      ],
      currentRow: null,
    })
  })

  test('lays out lap-log columns with shared right edges and reduced header opacity', () => {
    const logWidget = { ...widget, data: { ...widget.data, label: 'Lap Times', lap_timer_mode: 'lap_log' } }
    const model = buildLapTimerPreviewModel({
      widget: logWidget,
      activity,
      previewSecond: 13,
      lapLogPreparation: prepareLapLogPreview({ widget: logWidget, activity }),
    })

    expect(model.content).toMatchObject({ type: 'lap_log' })
    expect(model.content).not.toHaveProperty('labelText')
    expect(model.content.rows).toHaveLength(4)
    expect(model.content.rows[0].opacityMultiplier).toBe(0.7)
    expect(model.content.rows[0].fontSize).toBe(widget.data.label_font_size)
    expect(model.content.rows[0].fontFamily).toContain('Teko')
    expect(model.content.rows[1].fontSize).toBe(widget.data.font_size)
    expect(model.content.rows.slice(1).map((row) => row.cells[0].text)).toEqual(['3', '2', '1'])
    expect(model.content.rows.map((row) => row.cells[2].color)).toEqual(['#abcdef', '#00ff00', '#ff0000', '#00ff00'])
    for (let columnIndex = 0; columnIndex < 3; columnIndex += 1) {
      const rightEdges = model.content.rows.map((row) => row.cells[columnIndex].left + row.cells[columnIndex].measure.width)
      expect(new Set(rightEdges).size).toBe(1)
    }
  })

  test('rejects an unsupported lap timer mode', () => {
    expect(() => getLapTimerDisplayValue(activity, 9, 'fastest_lap')).toThrow('Unsupported lap timer mode: fastest_lap')
  })
})
