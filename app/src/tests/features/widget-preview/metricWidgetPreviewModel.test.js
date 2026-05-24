import { describe, expect, test } from 'vitest'

import { buildMetricWidgetPreviewModel } from '@/features/widget-preview'

describe('metric widget preview model standard metric units', () => {
  test('formats speed widgets from display_unit', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: {
        category: 'values',
        type: 'speed',
        data: {
          display_unit: 'mph',
          show_units: true,
          show_icon: false,
        },
      },
      activity: {
        sample_elapsed_seconds: [0],
        speed: [10],
      },
      previewSecond: 0,
    })

    expect(model?.valueText).toBe('22')
    expect(model?.unitText).toBe('MPH')
  })

  test('formats temperature widgets from display_unit', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: {
        category: 'values',
        type: 'temperature',
        data: {
          display_unit: 'fahrenheit',
          show_units: true,
          show_icon: false,
        },
      },
      activity: {
        sample_elapsed_seconds: [0],
        temperature: [20],
      },
      previewSecond: 0,
    })

    expect(model?.valueText).toBe('68')
    expect(model?.unitText).toBe('\u00B0F')
  })
})
