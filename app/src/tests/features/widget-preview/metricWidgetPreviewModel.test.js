import { describe, expect, test } from 'vitest'

import { buildMetricWidgetPreviewModel } from '@/features/widget-preview'
import { deriveActivityMetricSeries } from '@/lib/activity/metric-series'
import { isFiniteNumber, roundValue, safeNumber } from '@/lib/activity/parse-helpers'

describe('gear_position extraction pipeline', () => {
  test('extracts gearPosition from normalizedRawSamples into metricSeriesMap', () => {
    const { metricSeriesMap } = deriveActivityMetricSeries({
      courseSeries: [],
      distanceSeries: [0, 100],
      elevationBaseSeries: [0, 0],
      elapsedSeries: [0, 10],
      normalizedRawSamples: [{ gearPosition: null }, { gearPosition: 5 }],
      useLegacyGpxDerivations: false,
      helpers: { isFiniteNumber, roundValue, safeNumber },
    })

    expect(metricSeriesMap.gear_position).toBeDefined()
    expect(metricSeriesMap.gear_position.source).toBe('direct')
    expect(metricSeriesMap.gear_position.series).toEqual([null, 5])
  })
})

describe('vertical_oscillation extraction pipeline', () => {
  test('extracts verticalOscillation from normalizedRawSamples into metricSeriesMap', () => {
    const { metricSeriesMap } = deriveActivityMetricSeries({
      courseSeries: [],
      distanceSeries: [0, 100],
      elevationBaseSeries: [0, 0],
      elapsedSeries: [0, 10],
      normalizedRawSamples: [{ verticalOscillation: null }, { verticalOscillation: 85 }],
      useLegacyGpxDerivations: false,
      helpers: { isFiniteNumber, roundValue, safeNumber },
    })

    expect(metricSeriesMap.vertical_oscillation).toBeDefined()
    expect(metricSeriesMap.vertical_oscillation.source).toBe('direct')
    expect(metricSeriesMap.vertical_oscillation.series).toEqual([null, 85])
  })
})

describe('core_temperature extraction pipeline', () => {
  test('extracts coreTemperature from normalizedRawSamples into metricSeriesMap', () => {
    const { metricSeriesMap } = deriveActivityMetricSeries({
      courseSeries: [],
      distanceSeries: [0, 100],
      elevationBaseSeries: [0, 0],
      elapsedSeries: [0, 10],
      normalizedRawSamples: [{ coreTemperature: null }, { coreTemperature: 37.5 }],
      useLegacyGpxDerivations: false,
      helpers: { isFiniteNumber, roundValue, safeNumber },
    })

    expect(metricSeriesMap.core_temperature).toBeDefined()
    expect(metricSeriesMap.core_temperature.source).toBe('direct')
    expect(metricSeriesMap.core_temperature.series).toEqual([null, 37.5])
  })
})

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
})
