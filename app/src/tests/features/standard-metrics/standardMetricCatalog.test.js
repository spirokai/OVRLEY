import { describe, expect, test } from 'vitest'

import {
  CURRENT_STANDARD_METRIC_WIDGET_TYPES,
  STANDARD_METRIC_WIDGET_TYPES,
  DISPLAY_TYPE_DEFINITIONS,
  DISPLAY_TYPE_LABELS,
} from '@/lib/standard-widgets'
import {
  getStandardMetricDefinition,
  isStandardMetricWidgetType,
  getDisplayTypeDefinition,
  getDisplayTypeLabel,
  isBoxedDisplayType,
  getDefaultFrameDimensions,
  getSupportedDisplayTypes,
  getDisplayTypeOptions,
} from '@/lib/standard-metrics'
import { isTextDisplayType, isBoxedMetricWidget } from '@/lib/display-type-behavior'
import { TYPE_LABELS } from '@/lib/widget-icons'

describe('standard metric widget catalog', () => {
  test('covers the existing and Wave 1 shared standard metric widgets', () => {
    expect(CURRENT_STANDARD_METRIC_WIDGET_TYPES).toEqual([
      'speed',
      'heartrate',
      'cadence',
      'power',
      'temperature',
      'pace',
      'g_force',
      'air_pressure',
      'ground_contact_time',
      'left_right_balance',
      'stride_length',
      'stroke_rate',
      'torque',
      'vertical_speed',
      'gear_position',
      'vertical_oscillation',
      'core_temperature',
      'heading',
    ])
    expect(STANDARD_METRIC_WIDGET_TYPES).toEqual(expect.arrayContaining(CURRENT_STANDARD_METRIC_WIDGET_TYPES))

    const speed = getStandardMetricDefinition('speed')

    expect(speed).toMatchObject({
      label: 'Speed',
      defaultDisplayUnit: 'kmh',
      showUnitsByDefault: true,
      icon: {
        assetFile: 'widget-speed.svg',
      },
    })
    expect(speed.supportedDisplayUnits.map((option) => option.value)).toEqual(['kmh', 'mph', 'kn', 'mps'])
  })

  test('records the planned icon catalog for future standard metric widgets', () => {
    expect(getStandardMetricDefinition('pace').icon).toEqual({
      source: 'lucide',
      name: 'Footprints',
      assetFile: 'widget-pace.svg',
    })
    expect(getStandardMetricDefinition('air_pressure').icon).toEqual({
      source: 'lucide',
      name: 'Wind',
      assetFile: 'widget-air-pressure.svg',
    })
    expect(getStandardMetricDefinition('g_force').icon).toEqual({
      source: 'custom',
      assetFile: 'widget-g-force.svg',
    })
    expect(getStandardMetricDefinition('gear_position').icon).toEqual({
      source: 'custom',
      assetFile: 'widget-gear-position.svg',
    })
  })

  test('identifies standard metric widgets without folding in specialized widgets', () => {
    expect(isStandardMetricWidgetType('speed')).toBe(true)
    expect(isStandardMetricWidgetType('temperature')).toBe(true)
    expect(isStandardMetricWidgetType('heading')).toBe(true)
    expect(isStandardMetricWidgetType('time')).toBe(false)
    expect(isStandardMetricWidgetType('gradient')).toBe(false)
    expect(isStandardMetricWidgetType('course')).toBe(false)
  })

  test('feeds standard metric labels into shared widget label lookups', () => {
    expect(TYPE_LABELS.speed).toBe(getStandardMetricDefinition('speed').label)
    expect(TYPE_LABELS.pace).toBe(getStandardMetricDefinition('pace').label)
    expect(TYPE_LABELS.core_temperature).toBe(getStandardMetricDefinition('core_temperature').label)
  })
})

describe('display type definitions', () => {
  test('each display type has a formal definition with label and layoutMode', () => {
    expect(DISPLAY_TYPE_DEFINITIONS.text).toMatchObject({ label: 'Text', layoutMode: 'intrinsic' })
    expect(DISPLAY_TYPE_DEFINITIONS.linear).toMatchObject({ label: 'Linear', layoutMode: 'boxed' })
    expect(DISPLAY_TYPE_DEFINITIONS.bars).toMatchObject({ label: 'Bars', layoutMode: 'boxed' })
    expect(DISPLAY_TYPE_DEFINITIONS.arc).toMatchObject({ label: 'Arc', layoutMode: 'boxed' })
    expect(DISPLAY_TYPE_DEFINITIONS.corner).toMatchObject({ label: 'Corner', layoutMode: 'boxed' })
    expect(DISPLAY_TYPE_DEFINITIONS.heading_tape).toMatchObject({ label: 'Heading Tape', layoutMode: 'boxed' })
  })

  test('boxed display types include default frame dimensions', () => {
    expect(DISPLAY_TYPE_DEFINITIONS.linear.defaultFrameWidth).toBe(200)
    expect(DISPLAY_TYPE_DEFINITIONS.linear.defaultFrameHeight).toBe(60)
    expect(DISPLAY_TYPE_DEFINITIONS.arc.defaultFrameWidth).toBe(120)
    expect(DISPLAY_TYPE_DEFINITIONS.arc.defaultFrameHeight).toBe(120)
  })

  test('intrinsic display types have no frame dimensions', () => {
    expect(DISPLAY_TYPE_DEFINITIONS.text.defaultFrameWidth).toBeUndefined()
    expect(DISPLAY_TYPE_DEFINITIONS.text.defaultFrameHeight).toBeUndefined()
  })

  test('DISPLAY_TYPE_LABELS is derived from definitions', () => {
    expect(DISPLAY_TYPE_LABELS.text).toBe('Text')
    expect(DISPLAY_TYPE_LABELS.linear).toBe('Linear')
    expect(Object.keys(DISPLAY_TYPE_LABELS)).toEqual(Object.keys(DISPLAY_TYPE_DEFINITIONS))
  })

  test('getDisplayTypeDefinition returns the definition or null', () => {
    expect(getDisplayTypeDefinition('text')).toEqual(DISPLAY_TYPE_DEFINITIONS.text)
    expect(getDisplayTypeDefinition('nonexistent')).toBeNull()
  })

  test('getDisplayTypeLabel returns label or falls back to key', () => {
    expect(getDisplayTypeLabel('text')).toBe('Text')
    expect(getDisplayTypeLabel('unknown')).toBe('unknown')
  })

  test('isBoxedDisplayType correctly classifies display types', () => {
    expect(isBoxedDisplayType('text')).toBe(false)
    expect(isBoxedDisplayType('linear')).toBe(true)
    expect(isBoxedDisplayType('bars')).toBe(true)
    expect(isBoxedDisplayType('arc')).toBe(true)
    expect(isBoxedDisplayType('corner')).toBe(true)
    expect(isBoxedDisplayType('heading_tape')).toBe(true)
    expect(isBoxedDisplayType('nonexistent')).toBe(false)
  })

  test('getDefaultFrameDimensions returns dimensions for boxed types and null for intrinsic', () => {
    expect(getDefaultFrameDimensions('text')).toBeNull()
    expect(getDefaultFrameDimensions('linear')).toEqual({ width: 200, height: 60 })
    expect(getDefaultFrameDimensions('arc')).toEqual({ width: 120, height: 120 })
    expect(getDefaultFrameDimensions('nonexistent')).toBeNull()
  })

  test('getSupportedDisplayTypes respects per-metric overrides', () => {
    expect(getSupportedDisplayTypes('heading')).toEqual(['text', 'heading_tape'])
    expect(getSupportedDisplayTypes('core_temperature')).toEqual(['text'])
    expect(getSupportedDisplayTypes('speed')).toContain('text')
    expect(getSupportedDisplayTypes('speed')).toContain('linear')
  })

  test('getDisplayTypeOptions builds dropdown options from definitions', () => {
    const headingOptions = getDisplayTypeOptions('heading')
    expect(headingOptions).toEqual([
      { value: 'text', label: 'Text' },
      { value: 'heading_tape', label: 'Heading Tape' },
    ])
  })
})

describe('widget behavior helpers', () => {
  test('treats missing and explicit text display types as metric text mode', () => {
    expect(isTextDisplayType('text')).toBe(true)
    expect(isTextDisplayType(undefined)).toBe(true)
    expect(isTextDisplayType(null)).toBe(true)
    expect(isTextDisplayType('heading_tape')).toBe(false)
  })

  test('standard metric widgets derive boxed from display_type, not category', () => {
    expect(
      isBoxedMetricWidget({
        category: 'plots',
        type: 'heading',
        data: { display_type: 'text' },
      }),
    ).toBe(false)

    expect(
      isBoxedMetricWidget({
        category: 'plots',
        type: 'heading',
        data: { display_type: 'heading_tape' },
      }),
    ).toBe(true)

    expect(
      isBoxedMetricWidget({
        category: 'plots',
        type: 'heading',
        data: {},
      }),
    ).toBe(false)
  })

  test('non-metric plot widgets are always boxed', () => {
    expect(
      isBoxedMetricWidget({
        category: 'plots',
        type: 'route',
        data: {},
      }),
    ).toBe(true)

    expect(
      isBoxedMetricWidget({
        category: 'plots',
        type: 'gradient',
        data: {},
      }),
    ).toBe(true)
  })

  test('future boxed metric display types are recognized', () => {
    expect(
      isBoxedMetricWidget({
        category: 'values',
        type: 'speed',
        data: { display_type: 'linear' },
      }),
    ).toBe(true)

    expect(
      isBoxedMetricWidget({
        category: 'values',
        type: 'power',
        data: { display_type: 'arc' },
      }),
    ).toBe(true)
  })

  test('standard metric widgets in values category with text display_type are intrinsic', () => {
    expect(
      isBoxedMetricWidget({
        category: 'values',
        type: 'speed',
        data: { display_type: 'text' },
      }),
    ).toBe(false)

    expect(
      isBoxedMetricWidget({
        category: 'values',
        type: 'speed',
        data: {},
      }),
    ).toBe(false)
  })
})
