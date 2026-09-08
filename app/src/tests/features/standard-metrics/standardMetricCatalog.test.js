import { describe, expect, test } from 'vitest'

import {
  CURRENT_STANDARD_METRIC_WIDGET_TYPES,
  STANDARD_METRIC_WIDGET_TYPES,
  DISPLAY_TYPE_DEFINITIONS,
  DISPLAY_TYPE_LABEL_KEYS,
  WIDGET_TYPE_DEFINITIONS,
} from '@/lib/widget/standard-widgets'
import {
  getStandardMetricDefinition,
  getStandardMetricInterpolation,
  getStandardMetricUnitsMode,
  isStandardMetricWidgetType,
  getDisplayTypeDefinition,
  getDisplayTypeLabel,
  isBoxedDisplayType,
  getDefaultFrameDimensions,
  getSupportedDisplayTypes,
  getDisplayTypeOptions,
} from '@/lib/widget/standard-metrics'
import { isTextDisplayType, isBoxedMetricWidget } from '@/lib/widget/display-type-behavior'
import { METRIC_ICON_SVGS } from '@/lib/widget/widget-icons'
import i18next, { translateOptions } from '@/i18n'

describe('standard metric widget catalog', () => {
  test('covers the existing and Wave 1 shared standard metric widgets', () => {
    expect(CURRENT_STANDARD_METRIC_WIDGET_TYPES).toEqual([
      'speed',
      'distance',
      'gps_coordinates',
      'distance_to_home',
      'total_ascent',
      'calories',
      'heartrate',
      'cadence',
      'power',
      'engine_power',
      'engine_load',
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
      'vertical_ratio',
      'vertical_oscillation',
      'core_temperature',
      'heading',
      'altitude',
      'iso',
      'aperture',
      'shutter_speed',
      'focal_length',
      'ev',
      'color_temperature',
      'rpm',
      'throttle_position',
      'brake_position',
      'lean_angle',
      'lap_timer',
    ])
    expect(STANDARD_METRIC_WIDGET_TYPES).toEqual(expect.arrayContaining(CURRENT_STANDARD_METRIC_WIDGET_TYPES))

    const speed = getStandardMetricDefinition('speed')
    const distance = getStandardMetricDefinition('distance')

    expect(speed).toMatchObject({
      nameKey: 'widgets.types.speed.name',
      shortNameKey: 'widgets.types.speed.shortName',
      defaultDisplayUnit: 'kmh',
      showUnitsByDefault: true,
      icon: {
        assetFile: 'widget-speed.svg',
      },
    })
    expect(speed.supportedDisplayUnits.map((option) => option.value)).toEqual(['kmh', 'mph', 'kn', 'mps'])
    expect(distance).toMatchObject({
      nameKey: 'widgets.types.distance.name',
      shortNameKey: 'widgets.types.distance.shortName',
      defaultDisplayUnit: 'km',
      showUnitsByDefault: true,
      icon: {
        assetFile: 'widget-distance.svg',
      },
    })
    expect(distance.supportedDisplayUnits.map((option) => option.value)).toEqual(['m', 'km', 'mi', 'ft'])
  })

  test('exposes the new text metric definitions from the shared manifest', () => {
    expect(STANDARD_METRIC_WIDGET_TYPES).toEqual(expect.arrayContaining(['gps_coordinates', 'distance_to_home', 'total_ascent', 'calories']))
    expect(CURRENT_STANDARD_METRIC_WIDGET_TYPES).toEqual(expect.arrayContaining(['gps_coordinates', 'distance_to_home', 'total_ascent', 'calories']))
    expect(getStandardMetricDefinition('gps_coordinates')).toMatchObject({
      formatter: 'coordinates',
      defaultDisplayUnit: 'both',
      supportedDisplayUnits: [
        { value: 'latitude', labelKey: 'widget-editor.latitude', defaultLabel: 'Latitude' },
        { value: 'longitude', labelKey: 'widget-editor.longitude', defaultLabel: 'Longitude' },
        { value: 'both', labelKey: 'widget-editor.both', defaultLabel: 'Both' },
      ],
      showUnitsByDefault: false,
      category: 'general',
    })

    expect(translateOptions(getStandardMetricDefinition('gps_coordinates').supportedDisplayUnits, i18next.t)).toEqual([
      { value: 'latitude', label: 'Latitude' },
      { value: 'longitude', label: 'Longitude' },
      { value: 'both', label: 'Both' },
    ])
    expect(getStandardMetricDefinition('distance_to_home')).toMatchObject({
      formatter: 'distance',
      defaultDisplayUnit: 'm',
      category: 'other',
    })
    expect(getStandardMetricDefinition('total_ascent')).toMatchObject({
      formatter: 'elevation',
      defaultDisplayUnit: 'm',
      category: 'general',
    })
    expect(getStandardMetricDefinition('calories')).toMatchObject({
      formatter: 'integer',
      defaultDisplayUnit: 'kcal',
      showUnitsByDefault: true,
      category: 'other',
    })
    expect(getSupportedDisplayTypes('gps_coordinates')).toEqual(['text'])
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
    expect(getStandardMetricDefinition('distance').icon).toEqual({
      source: 'custom',
      assetFile: 'widget-distance.svg',
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

  test('feeds standard metric definitions into the canonical widget type catalog', () => {
    expect(WIDGET_TYPE_DEFINITIONS.speed).toBe(getStandardMetricDefinition('speed'))
    expect(WIDGET_TYPE_DEFINITIONS.distance).toBe(getStandardMetricDefinition('distance'))
    expect(WIDGET_TYPE_DEFINITIONS.pace).toBe(getStandardMetricDefinition('pace'))
    expect(WIDGET_TYPE_DEFINITIONS.core_temperature).toBe(getStandardMetricDefinition('core_temperature'))
  })

  test('Phase 1 SRT camera metrics are in the standard metric catalog', () => {
    const newTypes = ['altitude', 'iso', 'aperture', 'shutter_speed', 'focal_length', 'ev', 'color_temperature']
    for (const type of newTypes) {
      expect(isStandardMetricWidgetType(type)).toBe(true)
      const def = getStandardMetricDefinition(type)
      expect(def).toBeTruthy()
      expect(def.interpolation).toBeDefined()
      expect(def.unitsMode).toBeDefined()
    }
    expect(getStandardMetricDefinition('altitude').dataSource).toBe('elevation')
  })

  test('Phase 1 new metric definitions carry correct interpolation policy', () => {
    expect(getStandardMetricInterpolation('altitude')).toBe('linear')
    expect(getStandardMetricInterpolation('iso')).toBe('hold')
    expect(getStandardMetricInterpolation('aperture')).toBe('hold')
    expect(getStandardMetricInterpolation('shutter_speed')).toBe('hold')
    expect(getStandardMetricInterpolation('focal_length')).toBe('hold')
    expect(getStandardMetricInterpolation('ev')).toBe('hold')
    expect(getStandardMetricInterpolation('color_temperature')).toBe('hold')
  })

  test('Phase 1 new metric definitions carry correct unitsMode policy', () => {
    expect(getStandardMetricUnitsMode('altitude')).toBe('selectable')
    expect(getStandardMetricUnitsMode('iso')).toBe('hidden')
    expect(getStandardMetricUnitsMode('aperture')).toBe('hidden')
    expect(getStandardMetricUnitsMode('shutter_speed')).toBe('hidden')
    expect(getStandardMetricUnitsMode('focal_length')).toBe('selectable')
    expect(getStandardMetricUnitsMode('ev')).toBe('hidden')
    expect(getStandardMetricUnitsMode('color_temperature')).toBe('selectable')
  })

  test('registers vehicle metrics and their shared preview icons', () => {
    const vehicleTypes = ['rpm', 'throttle_position', 'brake_position', 'lean_angle', 'engine_load']
    for (const type of vehicleTypes) {
      expect(isStandardMetricWidgetType(type)).toBe(true)
      expect(METRIC_ICON_SVGS[type].innerMarkup).not.toBe('')
      expect(getStandardMetricInterpolation(type)).toBe(type === 'rpm' ? 'preserve' : 'linear')
      expect(getStandardMetricUnitsMode(type)).toBe('selectable')
    }

    expect(getStandardMetricDefinition('rpm').icon).toEqual({
      source: 'lucide',
      name: 'CircleGauge',
      assetFile: 'widget-rpm.svg',
    })
    expect(METRIC_ICON_SVGS.brake_position).toMatchObject({
      fill: 'currentColor',
      stroke: 'none',
    })
  })

  test('registers shared icons for the new metric widgets', () => {
    for (const type of ['gps_coordinates', 'distance_to_home', 'total_ascent', 'calories']) {
      expect(METRIC_ICON_SVGS[type].innerMarkup).not.toBe('')
    }
  })

  test('existing metrics carry interpolation and unitsMode defaults', () => {
    const linearTypes = ['speed', 'distance', 'heartrate', 'temperature', 'heading']
    const preserveTypes = ['cadence', 'power', 'engine_power', 'pace']
    for (const type of linearTypes) {
      expect(getStandardMetricInterpolation(type)).toBe('linear')
      expect(getStandardMetricUnitsMode(type)).toBe('selectable')
    }
    for (const type of preserveTypes) {
      expect(getStandardMetricInterpolation(type)).toBe('preserve')
      expect(getStandardMetricUnitsMode(type)).toBe('selectable')
    }
  })

  test('getStandardMetricInterpolation returns null for unknown types', () => {
    expect(getStandardMetricInterpolation('nonexistent')).toBeNull()
  })

  test('getStandardMetricUnitsMode returns null for unknown types', () => {
    expect(getStandardMetricUnitsMode('nonexistent')).toBeNull()
  })
})

describe('display type definitions', () => {
  test('each display type has a formal definition with labelKey and layoutMode', () => {
    expect(DISPLAY_TYPE_DEFINITIONS.text).toMatchObject({ labelKey: 'widgets.displayTypes.text', layoutMode: 'intrinsic' })
    expect(DISPLAY_TYPE_DEFINITIONS.linear).toMatchObject({ labelKey: 'widgets.displayTypes.linear', layoutMode: 'boxed' })
    expect(DISPLAY_TYPE_DEFINITIONS.arc).toMatchObject({ labelKey: 'widgets.displayTypes.arc', layoutMode: 'boxed' })
    expect(DISPLAY_TYPE_DEFINITIONS.corner).toMatchObject({ labelKey: 'widgets.displayTypes.corner', layoutMode: 'boxed' })
    expect(DISPLAY_TYPE_DEFINITIONS.heading_tape).toMatchObject({ labelKey: 'widgets.displayTypes.headingTape', layoutMode: 'boxed' })
    expect(DISPLAY_TYPE_DEFINITIONS.lean_angle).toMatchObject({ labelKey: 'widgets.displayTypes.leanAngle', layoutMode: 'boxed' })
  })

  test('boxed display types use explicit or derived geometry contracts', () => {
    expect(DISPLAY_TYPE_DEFINITIONS.linear.defaultFrameWidth).toBe(200)
    expect(DISPLAY_TYPE_DEFINITIONS.linear.defaultFrameHeight).toBe(24)
    expect(DISPLAY_TYPE_DEFINITIONS.arc.defaultFrameWidth).toBe(220)
    expect(DISPLAY_TYPE_DEFINITIONS.arc.defaultFrameHeight).toBe(220)
    expect(DISPLAY_TYPE_DEFINITIONS.corner.defaultFrameWidth).toBe(162)
    expect(DISPLAY_TYPE_DEFINITIONS.corner.defaultFrameHeight).toBe(162)
    expect(DISPLAY_TYPE_DEFINITIONS.lean_angle).not.toHaveProperty('defaultFrameWidth')
    expect(DISPLAY_TYPE_DEFINITIONS.lean_angle).not.toHaveProperty('defaultFrameHeight')
    expect(DISPLAY_TYPE_DEFINITIONS.lean_angle.defaults.diameter).toBe(450)
    expect(DISPLAY_TYPE_DEFINITIONS.lean_angle).not.toHaveProperty('defaultFontSize')
    expect(DISPLAY_TYPE_DEFINITIONS.lean_angle.defaults.font_size).toBe(90)
  })

  test('lean-angle display type exposes the complete static-sector contract', () => {
    expect(DISPLAY_TYPE_DEFINITIONS.lean_angle.defaults).toEqual({
      display_type: 'lean_angle',
      diameter: 450,
      show_icon: false,
      track_empty_color: '#222222',
      track_empty_opacity: 0.5,
      track_filled_color: '#dce2e8',
      track_filled_opacity: 1,
      track_border_thickness: 0,
      track_border_color: '#ffffff',
      track_thickness: 150,
      font: 'Arial.ttf',
      font_size: 90,
      color: '#ffffff',
      unit_color: '#ffffff',
      show_units: true,
      value_offset_x: 0,
      value_offset_y: 0,
    })
  })

  test('gauge defaults select continuous fill without fixed segmented geometry', () => {
    for (const displayType of ['linear', 'arc', 'corner']) {
      expect(DISPLAY_TYPE_DEFINITIONS[displayType].defaults.track_fill_style).toBe('fill')
      expect(DISPLAY_TYPE_DEFINITIONS[displayType].defaults.bar_count).toBeUndefined()
      expect(DISPLAY_TYPE_DEFINITIONS[displayType].defaults.bar_gap).toBeUndefined()
    }
  })

  test('intrinsic display types have no frame dimensions', () => {
    expect(DISPLAY_TYPE_DEFINITIONS.text.defaultFrameWidth).toBeUndefined()
    expect(DISPLAY_TYPE_DEFINITIONS.text.defaultFrameHeight).toBeUndefined()
  })

  test('DISPLAY_TYPE_LABEL_KEYS is derived from definitions', () => {
    expect(DISPLAY_TYPE_LABEL_KEYS.text).toBe('widgets.displayTypes.text')
    expect(DISPLAY_TYPE_LABEL_KEYS.linear).toBe('widgets.displayTypes.linear')
    expect(Object.keys(DISPLAY_TYPE_LABEL_KEYS)).toEqual(Object.keys(DISPLAY_TYPE_DEFINITIONS))
  })

  test('getDisplayTypeDefinition returns the definition or null', () => {
    expect(getDisplayTypeDefinition('text')).toEqual(DISPLAY_TYPE_DEFINITIONS.text)
    expect(getDisplayTypeDefinition('nonexistent')).toBeNull()
  })

  test('getDisplayTypeLabel translates known display types and rejects unknown types', () => {
    expect(getDisplayTypeLabel('text', i18next.t)).toBe('Text')
    expect(() => getDisplayTypeLabel('unknown', i18next.t)).toThrow('Unknown display type: unknown')
  })

  test('isBoxedDisplayType correctly classifies display types', () => {
    expect(isBoxedDisplayType('text')).toBe(false)
    expect(isBoxedDisplayType('linear')).toBe(true)
    expect(isBoxedDisplayType('arc')).toBe(true)
    expect(isBoxedDisplayType('corner')).toBe(true)
    expect(isBoxedDisplayType('heading_tape')).toBe(true)
    expect(isBoxedDisplayType('lean_angle')).toBe(true)
    expect(isBoxedDisplayType('nonexistent')).toBe(false)
  })

  test('getDefaultFrameDimensions returns dimensions for boxed types and null for intrinsic', () => {
    expect(getDefaultFrameDimensions('text')).toBeNull()
    expect(getDefaultFrameDimensions('linear')).toEqual({ width: 200, height: 24 })
    expect(getDefaultFrameDimensions('arc')).toEqual({ width: 220, height: 220 })
    expect(getDefaultFrameDimensions('corner')).toEqual({ width: 162, height: 162 })
    expect(getDefaultFrameDimensions('lean_angle')).toBeNull()
    expect(getDefaultFrameDimensions('nonexistent')).toBeNull()
  })

  test('getSupportedDisplayTypes respects per-metric overrides', () => {
    expect(getSupportedDisplayTypes('heading')).toEqual(['text', 'heading_tape'])
    expect(getSupportedDisplayTypes('core_temperature')).toEqual(['text'])
    expect(getSupportedDisplayTypes('lean_angle')).toEqual(['text', 'lean_angle'])
    expect(getSupportedDisplayTypes('speed')).toContain('text')
  })

  test('getDisplayTypeOptions builds dropdown options from definitions', () => {
    const headingOptions = getDisplayTypeOptions('heading', i18next.t)
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
