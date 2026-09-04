import { describe, expect, test } from 'vitest'

import { buildMetricWidgetPreviewModel } from '@/features/widget-preview'
import { formatCoordinates } from '@/features/widget-preview/widgets/metric/format'

function makeMetricWidget(type, data = {}) {
  return {
    category: 'values',
    type,
    data: {
      display_type: 'text',
      content_alignment: 'left',
      font: 'Arial.ttf',
      font_size: 60,
      decimals: 0,
      balance_format: 'percent_label',
      show_units: true,
      show_icon: false,
      icon_size: 28,
      icon_offset_x: 0,
      icon_offset_y: 0,
      ...data,
    },
  }
}

function makeActivity(type, value) {
  return {
    trim_end_seconds: 0,
    sample_elapsed_seconds: [0],
    [type]: [value],
  }
}

describe('Vehicle metric formatting', () => {
  test.each([
    ['rpm', 7250, 'rpm', '7250', 'RPM'],
    ['throttle_position', 42.5, 'percent', '42.5', '%'],
    ['brake_position', 18.5, 'percent', '18.5', '%'],
    ['lean_angle', -37.5, 'degrees', '-37.5', '°'],
  ])('%s uses standard decimal formatting', (type, value, displayUnit, expectedValue, expectedUnit) => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget(type, { display_unit: displayUnit, decimals: type === 'rpm' ? 0 : 1 }),
      activity: makeActivity(type, value),
      previewSecond: 0,
    })

    expect(model?.valueText).toBe(expectedValue)
    expect(model?.unitText).toBe(expectedUnit)
  })
})

describe('Speed metric formatting', () => {
  test('preserves the requested decimal places after unit conversion', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('speed', { display_unit: 'kmh', decimals: 1 }),
      activity: makeActivity('speed', 10),
      previewSecond: 0,
    })

    expect(model?.valueText).toBe('36.0')
    expect(model?.unitText).toBe('KM/H')
  })
})

describe('Wave 2 metric formatting', () => {
  test('distance_to_home uses the shared distance conversion', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('distance_to_home', { display_unit: 'km', decimals: 2 }),
      activity: makeActivity('distance_to_home', 2500),
      previewSecond: 0,
    })

    expect(model?.valueText).toBe('2.50')
    expect(model?.unitText).toBe('KM')
  })

  test('vertical_oscillation formats as mm', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('vertical_oscillation', { display_unit: 'mm', decimals: 1 }),
      activity: makeActivity('vertical_oscillation', 85),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('85.0')
    expect(model?.unitText).toBe('MM')
  })

  test('vertical_oscillation converts to cm', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('vertical_oscillation', { display_unit: 'cm', decimals: 1 }),
      activity: makeActivity('vertical_oscillation', 100),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('10.0')
    expect(model?.unitText).toBe('CM')
  })

  test('vertical_oscillation preserves missing samples as zero', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('vertical_oscillation', { display_unit: 'mm' }),
      activity: { trim_end_seconds: 0, sample_elapsed_seconds: [0], vertical_oscillation: [null] },
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('0')
    expect(model?.unitText).toBe('MM')
  })
})

describe('total ascent and GPS coordinate formatting', () => {
  test('formats total ascent as current over full ascent', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('total_ascent', {
        display_unit: 'm',
        show_full_ascent: true,
        show_units: true,
      }),
      activity: {
        trim_end_seconds: 20,
        sample_elapsed_seconds: [0, 10, 20],
        total_ascent: [0, 12, 25],
      },
      previewSecond: 10,
    })

    expect(model?.valueText).toBe('12/25')
    expect(model?.unitText).toBe('M')
  })

  test('shows the ascent placeholder before an activity is loaded', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('total_ascent', {
        display_unit: 'm',
        show_full_ascent: true,
        show_units: true,
      }),
      activity: null,
      previewSecond: 0,
    })

    expect(model?.valueText).toBe('--')
    expect(model?.unitText).toBe('M')
  })

  test('formats DMS and DDM coordinates for both hemispheres', () => {
    expect(formatCoordinates([40.446111, -73.987222], 'both', 'dms', '#fff')).toEqual({
      type: 'coordinates',
      lines: [
        { direction: 'N', valueText: '40°26′46″', directionColor: '#fff' },
        { direction: 'W', valueText: '73°59′14″', directionColor: '#fff' },
      ],
    })
    expect(formatCoordinates([-40.446111, 73.987222], 'both', 'ddm', '#fff')).toEqual({
      type: 'coordinates',
      lines: [
        { direction: 'S', valueText: '40°26.767′', directionColor: '#fff' },
        { direction: 'E', valueText: '73°59.233′', directionColor: '#fff' },
      ],
    })
  })

  test('formats equator and prime meridian without negative directions', () => {
    expect(formatCoordinates([0, 0], 'both', 'dms', '#fff')).toEqual({
      type: 'coordinates',
      lines: [
        { direction: 'N', valueText: '0°00′00″', directionColor: '#fff' },
        { direction: 'E', valueText: '0°00′00″', directionColor: '#fff' },
      ],
    })
  })

  test('formats coordinate values with zero padding', () => {
    expect(formatCoordinates([8.1, -8.1], 'both', 'dms', '#fff')).toEqual({
      type: 'coordinates',
      lines: [
        { direction: 'N', valueText: '8°06′00″', directionColor: '#fff' },
        { direction: 'W', valueText: '8°06′00″', directionColor: '#fff' },
      ],
    })
    expect(formatCoordinates([8, -8], 'both', 'ddm', '#fff')).toEqual({
      type: 'coordinates',
      lines: [
        { direction: 'N', valueText: '8°00.000′', directionColor: '#fff' },
        { direction: 'W', valueText: '8°00.000′', directionColor: '#fff' },
      ],
    })
  })

  test('builds explicit stacked coordinate layout in both mode', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('gps_coordinates', {
        display_unit: 'both',
        coordinate_format: 'dms',
        prefix: '',
        suffix: '',
        show_units: false,
      }),
      activity: {
        trim_end_seconds: 0,
        sample_elapsed_seconds: [0],
        course: [[40.446111, -73.987222]],
      },
      previewSecond: 0,
    })

    expect(model?.content.type).toBe('coordinates')
    expect(model?.content.layout.fontSize).toBe(24)
    expect(model?.content.layout.lines.map((line) => line.direction)).toEqual(['N', 'W'])
  })

  test('keeps directions left-aligned and coordinate values right-aligned', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('gps_coordinates', {
        display_unit: 'both',
        coordinate_format: 'dms',
        prefix: '',
        suffix: '',
        show_units: false,
      }),
      activity: {
        trim_end_seconds: 0,
        sample_elapsed_seconds: [0],
        course: [[8.1, -73.987222]],
      },
      previewSecond: 0,
    })

    const lines = model?.content.layout.lines
    expect(lines?.[0].directionLeft).toBe(lines?.[1].directionLeft)
    expect(lines?.[0].valueLeft + lines?.[0].valueWidth).toBeCloseTo(lines?.[1].valueLeft + lines?.[1].valueWidth)
  })

  test('shows coordinate placeholders when no activity is selected', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('gps_coordinates', {
        display_unit: 'both',
        coordinate_format: 'dms',
        prefix: '',
        suffix: '',
        show_units: false,
      }),
      activity: null,
      previewSecond: 0,
    })

    expect(model?.content.type).toBe('coordinates')
    expect(model?.content.layout.lines.map((line) => line.valueText)).toEqual(['--°--′--″', '--°--′--″'])
  })
})

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

  test('gear_position formats as integer', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('gear_position', { display_unit: 'gear' }),
      activity: makeActivity('gear_position', '5'),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('5')
    expect(model?.unitText).toBe('GEAR')
  })

  test('gear_position formats zero as neutral', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('gear_position', { display_unit: 'gear' }),
      activity: makeActivity('gear_position', '0'),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('N')
  })

  test('gear_position preserves drivetrain labels', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('gear_position', { display_unit: 'gear' }),
      activity: makeActivity('gear_position', '52-34'),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('52-34')
  })

  test('gear_position shows placeholder when missing', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('gear_position', { display_unit: 'gear' }),
      activity: { trim_end_seconds: 0, sample_elapsed_seconds: [0], gear_position: [null] },
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('--')
    expect(model?.unitText).toBe('GEAR')
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

  test('vertical_speed converts to ft/h', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('vertical_speed', { display_unit: 'ftph', decimals: 1 }),
      activity: makeActivity('vertical_speed', 1),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('11811.0')
    expect(model?.unitText).toBe('FT/H')
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
      activity: { trim_end_seconds: 0, sample_elapsed_seconds: [0], g_force: [null] },
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('--')
  })

  test('air_pressure shows placeholder when data missing', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('air_pressure', { display_unit: 'hpa' }),
      activity: { trim_end_seconds: 0, sample_elapsed_seconds: [0], air_pressure: [null] },
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('--')
    expect(model?.unitText).toBe('HPA')
  })

  test('left_right_balance preserves missing samples as zero', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('left_right_balance', { display_unit: 'percent' }),
      activity: { trim_end_seconds: 0, sample_elapsed_seconds: [0], left_right_balance: [null] },
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('0%/100%')
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
    expect(model?.valueText).toBe('52%/48%')
    expect(model?.unitText).toBe('')
  })

  test('formats as plain (L / R)', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('left_right_balance', { display_unit: 'percent', balance_format: 'plain' }),
      activity: makeActivity('left_right_balance', 60),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('60/40')
    expect(model?.unitText).toBe('')
  })

  test('formats as l_prefix (L L / R R)', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('left_right_balance', { display_unit: 'percent', balance_format: 'l_prefix' }),
      activity: makeActivity('left_right_balance', 48),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('L48/R52')
    expect(model?.unitText).toBe('')
  })

  test('formats as l_suffix (L L / R R)', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('left_right_balance', { display_unit: 'percent', balance_format: 'l_suffix' }),
      activity: makeActivity('left_right_balance', 70),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('70L/30R')
    expect(model?.unitText).toBe('')
  })

  test('zero values use the selected format', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('left_right_balance', { display_unit: 'percent', balance_format: 'percent_label' }),
      activity: { trim_end_seconds: 0, sample_elapsed_seconds: [0], left_right_balance: [null] },
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('0%/100%')
    expect(model?.unitText).toBe('')
  })

  test.each([100, 127])('formats %s as neutral balance', (value) => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('left_right_balance', { display_unit: 'percent', balance_format: 'percent_label' }),
      activity: makeActivity('left_right_balance', value),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('50%/50%')
  })
})

describe('Phase 4 camera metric formatting', () => {
  test('aperture formats as F/x.x', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('aperture', {}),
      activity: makeActivity('aperture', 2.8),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('F/2.8')
    expect(model?.unitText).toBe('')
  })

  test('aperture formats single decimal place', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('aperture', {}),
      activity: makeActivity('aperture', 1.7),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('F/1.7')
    expect(model?.unitText).toBe('')
  })

  test('aperture shows placeholder when missing', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('aperture', {}),
      activity: { trim_end_seconds: 0, sample_elapsed_seconds: [0], aperture: [null] },
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('--')
    expect(model?.unitText).toBe('')
  })

  test('shutter_speed formats as reciprocal', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('shutter_speed', {}),
      activity: makeActivity('shutter_speed', 0.0003125),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('1/3200')
    expect(model?.unitText).toBe('')
  })

  test('shutter_speed formats 0.5 as 1/2', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('shutter_speed', {}),
      activity: makeActivity('shutter_speed', 0.5),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('1/2')
    expect(model?.unitText).toBe('')
  })

  test('shutter_speed formats 1/50 equivalent', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('shutter_speed', {}),
      activity: makeActivity('shutter_speed', 0.02),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('1/50')
    expect(model?.unitText).toBe('')
  })

  test('shutter_speed shows placeholder when missing', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('shutter_speed', {}),
      activity: { trim_end_seconds: 0, sample_elapsed_seconds: [0], shutter_speed: [null] },
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('--')
    expect(model?.unitText).toBe('')
  })

  test('ev formats positive values with plus sign', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('ev', { decimals: 1 }),
      activity: makeActivity('ev', 0.5),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('+0.5')
    expect(model?.unitText).toBe('')
  })

  test('ev formats negative values with minus sign', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('ev', { decimals: 1 }),
      activity: makeActivity('ev', -1.0),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('-1.0')
    expect(model?.unitText).toBe('')
  })

  test('ev formats zero without sign', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('ev', { decimals: 1 }),
      activity: makeActivity('ev', 0),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('0.0')
    expect(model?.unitText).toBe('')
  })

  test('ev shows placeholder when missing', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('ev', {}),
      activity: { trim_end_seconds: 0, sample_elapsed_seconds: [0], ev: [null] },
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('--')
    expect(model?.unitText).toBe('')
  })

  test('iso formats as integer', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('iso', {}),
      activity: makeActivity('iso', 800),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('800')
    expect(model?.unitText).toBe('')
  })

  test('altitude formats with unit', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('altitude', { show_units: true, display_unit: 'm', decimals: 1 }),
      activity: makeActivity('elevation', 42.5),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('42.5')
    expect(model?.unitText).toBe('M')
  })

  test('altitude converts meters to feet', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('altitude', { show_units: true, display_unit: 'ft', decimals: 0 }),
      activity: makeActivity('elevation', 100),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('328')
    expect(model?.unitText).toBe('FT')
  })

  test('focal_length formats with optional mm unit', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('focal_length', { show_units: true, display_unit: 'mm', decimals: 2 }),
      activity: makeActivity('focal_length', 24),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('24.00')
    expect(model?.unitText).toBe('MM')
  })

  test('color_temperature formats with optional K unit', () => {
    const model = buildMetricWidgetPreviewModel({
      widget: makeMetricWidget('color_temperature', { show_units: true, display_unit: 'kelvin' }),
      activity: makeActivity('color_temperature', 5491),
      previewSecond: 0,
    })
    expect(model?.valueText).toBe('5491')
    expect(model?.unitText).toBe('K')
  })
})
