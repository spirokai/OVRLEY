import { describe, expect, test } from 'vitest'

import { buildMetricWidgetPreviewModel } from '@/features/widget-preview'

describe('core_temperature widget preview', () => {
  test('formats core_temperature from display_unit celsius', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: {
        category: 'values',
        type: 'core_temperature',
        data: {
          display_unit: 'celsius',
          show_units: true,
          show_icon: false,
        },
      },
      activity: {
        sample_elapsed_seconds: [0],
        core_temperature: [38.5],
      },
      previewSecond: 0,
    })

    expect(model?.valueText).toBe('39')
    expect(model?.unitText).toBe('\u00B0C')
  })

  test('formats core_temperature from display_unit fahrenheit', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: {
        category: 'values',
        type: 'core_temperature',
        data: {
          display_unit: 'fahrenheit',
          show_units: true,
          show_icon: false,
        },
      },
      activity: {
        sample_elapsed_seconds: [0],
        core_temperature: [37],
      },
      previewSecond: 0,
    })

    expect(model?.valueText).toBe('99')
    expect(model?.unitText).toBe('\u00B0F')
  })

  test('shows placeholder when core_temperature data is missing', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: {
        category: 'values',
        type: 'core_temperature',
        data: {
          display_unit: 'celsius',
          show_units: true,
          show_icon: false,
        },
      },
      activity: {
        sample_elapsed_seconds: [0],
      },
      previewSecond: 0,
    })

    expect(model?.valueText).toBe('--')
  })
})

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

  test('formats distance widgets using the series total when show_full_distance is enabled', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: {
        category: 'values',
        type: 'distance',
        data: {
          display_unit: 'km',
          decimals: 1,
          show_units: true,
          show_icon: false,
          show_full_distance: true,
        },
      },
      activity: {
        sample_elapsed_seconds: [0, 10, 20],
        distance: [0, 22100, 35700],
      },
      previewSecond: 10,
    })

    expect(model?.valueText).toBe('22.1/35.7')
    expect(model?.unitText).toBe('km')
  })

  test('formats distance widgets as current-only when show_full_distance is disabled', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: {
        category: 'values',
        type: 'distance',
        data: {
          display_unit: 'mi',
          decimals: 2,
          show_units: true,
          show_icon: false,
          show_full_distance: false,
        },
      },
      activity: {
        sample_elapsed_seconds: [0, 10, 20],
        distance: [0, 1609.344, 3218.688],
      },
      previewSecond: 10,
    })

    expect(model?.valueText).toBe('1.00')
    expect(model?.unitText).toBe('mi')
  })

  test('preserves requested trailing zeros for full distance formatting', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: {
        category: 'values',
        type: 'distance',
        data: {
          display_unit: 'km',
          decimals: 2,
          show_units: true,
          show_icon: false,
          show_full_distance: true,
        },
      },
      activity: {
        sample_elapsed_seconds: [0, 10, 20],
        distance: [0, 2300, 5000],
      },
      previewSecond: 10,
    })

    expect(model?.valueText).toBe('2.30/5.00')
    expect(model?.unitText).toBe('km')
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

  test('formats pace widgets from display_unit', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: {
        category: 'values',
        type: 'pace',
        data: {
          display_unit: 'min_per_km',
          show_units: true,
          show_icon: false,
        },
      },
      activity: {
        sample_elapsed_seconds: [0],
        pace: [275],
      },
      previewSecond: 0,
    })

    expect(model?.valueText).toBe('4:35')
    expect(model?.unitText).toBe('MIN/KM')
  })

  test('gear_position formats as integer with gear unit', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: {
        category: 'values',
        type: 'gear_position',
        data: {
          display_unit: 'gear',
          show_units: true,
          show_icon: false,
        },
      },
      activity: {
        sample_elapsed_seconds: [0],
        gear_position: [5],
      },
      previewSecond: 0,
    })

    expect(model?.valueText).toBe('5')
    expect(model?.unitText).toBe('GEAR')
  })

  test('gear_position shows placeholder when data missing', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: {
        category: 'values',
        type: 'gear_position',
        data: {
          display_unit: 'gear',
          show_units: true,
          show_icon: false,
        },
      },
      activity: {
        sample_elapsed_seconds: [0],
        gear_position: [null],
      },
      previewSecond: 0,
    })

    expect(model?.valueText).toBe('--')
    expect(model?.unitText).toBe('GEAR')
  })

  test('plain heading value widgets still build a metric preview model when display_type is omitted', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: {
        category: 'values',
        type: 'heading',
        data: {
          show_icon: true,
        },
      },
      activity: {
        sample_elapsed_seconds: [0],
        heading: [90],
      },
      previewSecond: 0,
    })

    expect(model?.valueText).toBe('90')
    expect(model?.showIcon).toBe(true)
  })

  test('boxed display types skip the metric preview model so their own presentation path is used', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: {
        category: 'values',
        type: 'heading',
        data: {
          display_type: 'heading_tape',
          width: 400,
          height: 80,
        },
      },
      activity: {
        sample_elapsed_seconds: [0],
        heading: [90],
      },
      previewSecond: 0,
    })

    expect(model).toBeNull()
  })

  test('any boxed display type skips the metric preview model', () => {
    const linearModel = buildMetricWidgetPreviewModel({
      widget: {
        category: 'values',
        type: 'speed',
        data: { display_type: 'linear' },
      },
      activity: { sample_elapsed_seconds: [0], speed: [25] },
      previewSecond: 0,
    })
    expect(linearModel).toBeNull()

    const arcModel = buildMetricWidgetPreviewModel({
      widget: {
        category: 'values',
        type: 'power',
        data: { display_type: 'arc' },
      },
      activity: { sample_elapsed_seconds: [0], power: [200] },
      previewSecond: 0,
    })
    expect(arcModel).toBeNull()
  })
})
