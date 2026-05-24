import { describe, expect, test } from 'vitest'

import { buildMetricWidgetPreviewModel } from '@/features/widget-preview'

function makeMetricWidget(type, data = {}) {
  return {
    category: 'values',
    type,
    data: {
      show_units: true,
      show_icon: false,
      ...data,
    },
  }
}

function makeActivity(type, value) {
  return {
    sample_elapsed_seconds: [0],
    [type]: [value],
  }
}

describe('Wave 1 metric formatting', () => {
  test('g_force formats with g unit', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('g_force', { display_unit: 'g', decimals: 1 }),
      activity: makeActivity('g_force', 1.5),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('1.5')
    expect(model?.unitText).toBe('G')
  })

  test('g_force converts to m/s²', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('g_force', { display_unit: 'mps2', decimals: 1 }),
      activity: makeActivity('g_force', 1),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('9.8')
    expect(model?.unitText).toBe('M/S^2')
  })

  test('air_pressure formats with hPa', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('air_pressure', { display_unit: 'hpa' }),
      activity: makeActivity('air_pressure', 1.013),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('1013')
    expect(model?.unitText).toBe('HPA')
  })

  test('ground_contact_time formats with ms', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('ground_contact_time', { display_unit: 'ms' }),
      activity: makeActivity('ground_contact_time', 250),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('250')
    expect(model?.unitText).toBe('MS')
  })

  test('stride_length formats with m', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('stride_length', { display_unit: 'm', decimals: 2 }),
      activity: makeActivity('stride_length', 1.25),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('1.25')
    expect(model?.unitText).toBe('M')
  })

  test('stroke_rate formats with spm', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('stroke_rate', { display_unit: 'spm' }),
      activity: makeActivity('stroke_rate', 85),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('85')
    expect(model?.unitText).toBe('SPM')
  })

  test('torque formats with Nm', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('torque', { display_unit: 'nm', decimals: 1 }),
      activity: makeActivity('torque', 35.5),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('35.5')
    expect(model?.unitText).toBe('NM')
  })

  test('vertical_speed formats with m/s', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('vertical_speed', { display_unit: 'mps', decimals: 1 }),
      activity: makeActivity('vertical_speed', 5.2),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('5.2')
    expect(model?.unitText).toBe('M/S')
  })

  test('vertical_speed converts to ft/min', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('vertical_speed', { display_unit: 'ftmin', decimals: 1 }),
      activity: makeActivity('vertical_speed', 1),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('196.9')
    expect(model?.unitText).toBe('FT/MIN')
  })

  test('pace formats with min/km display', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('pace', { display_unit: 'min_per_km' }),
      activity: makeActivity('pace', 275),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('4:35')
    expect(model?.unitText).toBe('MIN/KM')
  })

  test('pace formats with min/mi display', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('pace', { display_unit: 'min_per_mi' }),
      activity: makeActivity('pace', 275),
      previewSecond: 0,
    })
    // 275 s/km * 1.609344 = 442.57 s/mi → rounds to 443 = 7:23
    expect(model?.valueText).toBe('7:23')
    expect(model?.unitText).toBe('MIN/MI')
  })
})

describe('Wave 1 placeholder behavior', () => {
  test('g_force shows placeholder when data missing', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('g_force', { display_unit: 'g' }),
      activity: { sample_elapsed_seconds: [0], g_force: [null] },
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('--')
  })

  test('air_pressure shows placeholder when data missing', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('air_pressure', { display_unit: 'hpa' }),
      activity: { sample_elapsed_seconds: [0], air_pressure: [null] },
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('--')
    expect(model?.unitText).toBe('HPA')
  })

  test('left_right_balance shows placeholder when data missing', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('left_right_balance', { display_unit: 'percent' }),
      activity: { sample_elapsed_seconds: [0], left_right_balance: [null] },
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('-- / --')
    expect(model?.unitText).toBe('')
  })
})

describe('left_right_balance format variants', () => {
  test('formats as default percent_label (L% / R%)', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('left_right_balance', { display_unit: 'percent' }),
      activity: makeActivity('left_right_balance', 52),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('52% / 48%')
    expect(model?.unitText).toBe('')
  })

  test('formats as plain (L / R)', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('left_right_balance', { display_unit: 'percent', balance_format: 'plain' }),
      activity: makeActivity('left_right_balance', 60),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('60 / 40')
    expect(model?.unitText).toBe('')
  })

  test('formats as l_prefix (L L / R R)', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('left_right_balance', { display_unit: 'percent', balance_format: 'l_prefix' }),
      activity: makeActivity('left_right_balance', 48),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('L48 / R52')
    expect(model?.unitText).toBe('')
  })

  test('formats as l_suffix (L L / R R)', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('left_right_balance', { display_unit: 'percent', balance_format: 'l_suffix' }),
      activity: makeActivity('left_right_balance', 70),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('70L / 30R')
    expect(model?.unitText).toBe('')
  })

  test('placeholder uses -- / -- for all formats', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('left_right_balance', { display_unit: 'percent', balance_format: 'percent_label' }),
      activity: { sample_elapsed_seconds: [0], left_right_balance: [null] },
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('-- / --')
    expect(model?.unitText).toBe('')
  })
})
