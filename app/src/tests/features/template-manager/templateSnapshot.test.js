import { describe, expect, test } from 'vitest'

import { TEMPLATE_FILE_FORMAT, TEMPLATE_FILE_VERSION } from '@/lib/template/template-constants'
import {
  createTemplateFilePayload,
  createTemplateState,
  normalizeTemplateConfig,
  normalizeTemplateFilePayload,
} from '@/features/template-manager/utils/templateSnapshot'
import { createMetricValueDefaults } from '@/features/widget-editor/utils/widgetUtils'
import { deepEqual } from '@/store/store-utils'

describe('template snapshot standard metric schema', () => {
  test('creates standard metric defaults with display_unit as the canonical unit field', () => {
    const speedDefaults = createMetricValueDefaults('speed')
    const temperatureDefaults = createMetricValueDefaults('temperature')

    expect(speedDefaults.display_unit).toBe('kmh')
    expect(speedDefaults).not.toHaveProperty('speed_unit')
    expect(temperatureDefaults.display_unit).toBe('celsius')
    expect(temperatureDefaults).not.toHaveProperty('temperature_unit')
  })

  test('seeds the arc font size as shared widget data rather than variant data', () => {
    const speedDefaults = createMetricValueDefaults('speed', undefined, { displayType: 'arc' })

    expect(speedDefaults.font_size).toBe(60)
    expect(speedDefaults.display_variants.arc).not.toHaveProperty('font_size')
  })

  test('preserves the flat lap timer contract during durable normalization', () => {
    const lapTimer = createMetricValueDefaults('lap_timer', undefined, { lapTimerMode: 'best_lap' })

    const normalized = normalizeTemplateConfig({ scene: {}, values: [lapTimer] })

    expect(normalized.values[0]).toMatchObject({
      value: 'lap_timer',
      display_type: 'lap_timer',
      lap_timer_mode: 'best_lap',
      show_label: true,
      label: 'Best Lap',
      label_font: 'Arial.ttf',
      label_font_size: 17.5,
      label_color: '#ffffff',
      positive_delta_color: '#ff6e83',
      negative_delta_color: '#61ffab',
    })
    expect(normalized.values[0]).not.toHaveProperty('display_variants')
  })

  test('populates missing lap timer label typography from globals and mode defaults on template load', () => {
    const lapTimer = createMetricValueDefaults('lap_timer', undefined, { lapTimerMode: 'lap_log' })
    delete lapTimer.label_font
    delete lapTimer.label_font_size
    delete lapTimer.label_color

    const normalized = normalizeTemplateConfig({ scene: {}, values: [lapTimer] }, { font_text: 'Teko.ttf', color_text: '#123456' })

    expect(normalized.values[0]).toMatchObject({
      label_font: 'Teko.ttf',
      label_font_size: 15,
      label_color: '#123456',
    })
  })

  test('creates Delta with its label and manifest colors instead of the global text color', () => {
    const lapTimer = createMetricValueDefaults(
      'lap_timer',
      { color_values: '#ffffff', font_text: 'Teko.ttf', color_text: '#123456' },
      { lapTimerMode: 'delta' },
    )

    expect(lapTimer).toMatchObject({
      value: 'lap_timer',
      display_type: 'lap_timer',
      lap_timer_mode: 'delta',
      show_label: true,
      label: 'Delta',
      label_font: 'Teko.ttf',
      label_font_size: 17.5,
      label_color: '#123456',
      positive_delta_color: '#ff6e83',
      negative_delta_color: '#61ffab',
    })
  })

  test('creates the canonical lap-log display with its default label', () => {
    const lapTimer = createMetricValueDefaults('lap_timer', { color_values: '#abcdef' }, { lapTimerMode: 'lap_log' })

    expect(lapTimer).toMatchObject({
      value: 'lap_timer',
      display_type: 'lap_timer',
      lap_timer_mode: 'lap_log',
      show_label: true,
      label: 'Lap Log',
      font_size: 30,
      label_font_size: 15,
      color: '#abcdef',
    })
  })

  test('partitions lean-angle shared defaults from variant geometry', () => {
    const leanAngleDefaults = createMetricValueDefaults('lean_angle', undefined, { displayType: 'lean_angle' })
    const themedLeanAngleDefaults = createMetricValueDefaults(
      'lean_angle',
      {
        font_values: 'Roboto.ttf',
        color_values: '#123456',
        color_units: '#abcdef',
      },
      { displayType: 'lean_angle' },
    )

    expect(leanAngleDefaults).toMatchObject({
      font: 'Arial.ttf',
      font_size: 90,
      color: '#ffffff',
      unit_color: '#ffffff',
      show_units: true,
    })
    expect(leanAngleDefaults.display_variants.lean_angle).toMatchObject({
      diameter: 450,
      track_thickness: 150,
      value_offset_x: 0,
      value_offset_y: 0,
    })
    expect(leanAngleDefaults.display_variants.lean_angle).not.toHaveProperty('width')
    expect(leanAngleDefaults.display_variants.lean_angle).not.toHaveProperty('height')
    for (const sharedKey of ['display_type', 'show_icon', 'font', 'font_size', 'color', 'unit_color', 'show_units']) {
      expect(leanAngleDefaults.display_variants.lean_angle).not.toHaveProperty(sharedKey)
    }
    expect(themedLeanAngleDefaults.font).toBe('Roboto.ttf')
    expect(themedLeanAngleDefaults.color).toBe('#123456')
    expect(themedLeanAngleDefaults.unit_color).toBe('#abcdef')
    expect(themedLeanAngleDefaults.display_variants.lean_angle).not.toHaveProperty('font')
  })

  test('rejects malformed durable lean-angle geometry at normalization', () => {
    const leanAngle = createMetricValueDefaults('lean_angle', undefined, { displayType: 'lean_angle' })
    const { diameter: _diameter, ...missingDiameter } = leanAngle.display_variants.lean_angle

    expect(() =>
      normalizeTemplateConfig({
        scene: {},
        labels: [],
        values: [{ ...leanAngle, display_variants: { lean_angle: missingDiameter } }],
        plots: [],
      }),
    ).toThrow('lean_angle diameter must be a positive finite number')

    expect(() =>
      normalizeTemplateConfig({
        scene: {},
        labels: [],
        values: [{ ...leanAngle, display_variants: { lean_angle: { ...leanAngle.display_variants.lean_angle, width: 180 } } }],
        plots: [],
      }),
    ).toThrow('lean_angle does not accept width or height; use diameter')
  })

  test('seeds G-force label typography from value globals', () => {
    const defaults = createMetricValueDefaults('g_force', { font_values: 'Roboto.ttf' }, { displayType: 'g_force' })

    expect(defaults.display_variants.g_force.label_font).toBe('Roboto.ttf')
    expect(defaults.display_variants.g_force.label_font_size).toBe(50)
    expect(defaults.display_variants.g_force.axis_horizontal).toBe('x')
    expect(defaults.display_variants.g_force.axis_vertical).toBe('y')
    expect(defaults.display_variants.g_force.invert_horizontal).toBe(false)
    expect(defaults.display_variants.g_force.invert_vertical).toBe(false)
  })

  test('normalizes standard metric widgets with display_unit and strips legacy unit fields', () => {
    const normalized = normalizeTemplateConfig({
      scene: {},
      labels: [],
      values: [
        {
          value: 'temperature',
          x: 10,
          y: 20,
          show_units: true,
          display_unit: 'fahrenheit',
          speed_unit: 'kmh',
          temperature_unit: 'celsius',
        },
      ],
      plots: [],
    })

    expect(normalized.values).toEqual([
      expect.objectContaining({
        value: 'temperature',
        display_unit: 'fahrenheit',
      }),
    ])
    expect(normalized.values[0]).not.toHaveProperty('speed_unit')
    expect(normalized.values[0]).not.toHaveProperty('temperature_unit')
  })

  test('does not persist boolean display_unit defaults for widgets without a string unit', () => {
    const state = createTemplateState({
      config: {
        scene: { width: 1920, height: 1080, fps: 30, updateRate: 1 },
        labels: [],
        values: [
          { id: 'gradient-1', value: 'gradient', x: 10, y: 20 },
          { id: 'time-1', value: 'time', x: 30, y: 40 },
        ],
        plots: [],
      },
      globalDefaults: {},
    })

    expect(state.config.values[0].display_unit).toBeUndefined()
    expect(state.config.values[1].display_unit).toBeUndefined()
  })

  test('stamps new template payloads with the current template file version', () => {
    const payload = createTemplateFilePayload({
      config: { scene: {}, labels: [], values: [], plots: [] },
      globalDefaults: {},
    })

    expect(payload.format).toBe(TEMPLATE_FILE_FORMAT)
    expect(payload.version).toBe(TEMPLATE_FILE_VERSION)
  })

  test('saves only template-wide scene defaults and widget update rate', () => {
    const payload = createTemplateFilePayload({
      config: {
        scene: {
          width: 1920,
          height: 1080,
          fps: 30,
          updateRate: 5,
          start: 12,
          end: 144,
          font: 'TemplateFont.ttf',
          color: '#ffffff',
          ffmpeg: { codec: 'prores_ks' },
        },
        labels: [],
        values: [],
        plots: [],
      },
      globalDefaults: {},
    })

    expect(payload.config.scene).toEqual({
      width: 1920,
      height: 1080,
      fps: 30,
      updateRate: 5,
    })
  })

  test('loads template scene updateRate without importing scene start/end', () => {
    const normalized = normalizeTemplateFilePayload({
      format: TEMPLATE_FILE_FORMAT,
      version: TEMPLATE_FILE_VERSION,
      config: {
        scene: { width: 1920, height: 1080, fps: 30, updateRate: 3, start: 5, end: 90 },
        labels: [],
        values: [],
        plots: [],
      },
      settings: { globalDefaults: {} },
    })

    expect(normalized.config.scene).toEqual({
      width: 1920,
      height: 1080,
      fps: 30,
      updateRate: 3,
    })
  })

  test('preserves stable widget ids when saving a template payload', () => {
    const payload = createTemplateFilePayload({
      config: {
        scene: {},
        labels: [{ id: 'widget-1', text: 'Label', x: 0, y: 0, color: '#ffffff' }],
        values: [
          { id: 'widget-2', value: 'speed', x: 10, y: 20 },
          { id: 'widget-3', value: 'heading', x: 30, y: 40 },
        ],
        plots: [],
      },
      globalDefaults: {},
    })

    expect(payload.config.labels[0].id).toBe('widget-1')
    expect(payload.config.values[0].id).toBe('widget-2')
    expect(payload.config.values[1].id).toBe('widget-3')
  })

  test('upgrades legacy templates without widget ids when loading them', () => {
    const normalized = normalizeTemplateFilePayload({
      format: TEMPLATE_FILE_FORMAT,
      version: TEMPLATE_FILE_VERSION,
      config: {
        scene: {},
        labels: [{ text: 'Legacy label', x: 0, y: 0 }],
        values: [
          { value: 'speed', x: 10, y: 20 },
          { value: 'heading', x: 30, y: 40 },
        ],
        plots: [],
      },
      settings: { globalDefaults: {} },
    })

    expect(normalized.config.labels[0].id).toMatch(/^widget-\d+$/)
    expect(normalized.config.values[0].id).toMatch(/^widget-\d+$/)
    expect(normalized.config.values[1].id).toMatch(/^widget-\d+$/)
  })

  test('deepEqual returns true for structurally equal template states', () => {
    const state = {
      config: { scene: { width: 1920, height: 1080, fps: 30 }, labels: [], values: [{ value: 'speed', x: 10 }], plots: [] },
      settings: { globalDefaults: { color_values: '#ffffff' } },
    }
    const copy = JSON.parse(JSON.stringify(state))

    expect(deepEqual(state, copy)).toBe(true)
  })

  test('deepEqual returns false when config differs', () => {
    const left = {
      config: { scene: { width: 1920, height: 1080 }, labels: [], values: [], plots: [] },
      settings: { globalDefaults: {} },
    }
    const right = {
      config: { scene: { width: 1280, height: 720 }, labels: [], values: [], plots: [] },
      settings: { globalDefaults: {} },
    }

    expect(deepEqual(left, right)).toBe(false)
  })

  test('deepEqual returns false when settings differ', () => {
    const left = {
      config: { scene: {}, labels: [], values: [], plots: [] },
      settings: { globalDefaults: { color_values: '#ffffff' } },
    }
    const right = {
      config: { scene: {}, labels: [], values: [], plots: [] },
      settings: { globalDefaults: { color_values: '#000000' } },
    }

    expect(deepEqual(left, right)).toBe(false)
  })

  test('rejects older template versions explicitly', () => {
    expect(() =>
      normalizeTemplateFilePayload({
        format: TEMPLATE_FILE_FORMAT,
        version: TEMPLATE_FILE_VERSION - 1,
        config: { scene: {}, labels: [], values: [], plots: [] },
        settings: { globalDefaults: {} },
      }),
    ).toThrow(`Unsupported template file version: ${TEMPLATE_FILE_VERSION - 1}. Expected ${TEMPLATE_FILE_VERSION}.`)
  })

  test.each([
    ['config', 'invalid', { globalDefaults: {} }, 'Template config must be an object.'],
    ['config array', [], { globalDefaults: {} }, 'Template config must be an object.'],
    ['settings', {}, 'invalid', 'Template settings must be an object.'],
    ['settings array', {}, [], 'Template settings must be an object.'],
    ['global defaults', {}, { globalDefaults: 'invalid' }, 'Template settings.globalDefaults must be an object.'],
  ])('rejects malformed %s envelope fields', (_case, config, settings, expectedMessage) => {
    expect(() =>
      normalizeTemplateFilePayload({
        format: TEMPLATE_FILE_FORMAT,
        version: TEMPLATE_FILE_VERSION,
        config,
        settings,
      }),
    ).toThrow(expectedMessage)
  })
})
