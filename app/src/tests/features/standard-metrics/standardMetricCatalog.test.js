import { describe, expect, test } from 'vitest'

import {
  CURRENT_STANDARD_METRIC_WIDGET_TYPES,
  STANDARD_METRIC_WIDGET_TYPES,
  getStandardMetricDefinition,
  isStandardMetricWidgetType,
} from '@/lib/standard-metrics'
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
