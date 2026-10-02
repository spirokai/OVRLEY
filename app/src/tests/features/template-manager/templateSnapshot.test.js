import { beforeEach, describe, expect, test, vi } from 'vitest'

import { loadSelectedRaster } from '@/api/backend'
import { prepareDocumentFonts } from '@/lib/font-resources'
import { TEMPLATE_FILE_FORMAT, TEMPLATE_FILE_VERSION } from '@/lib/template/template-constants'
import {
  createTemplateFilePayload,
  createTemplateState,
  normalizeTemplateConfig,
  normalizeTemplateFilePayload,
  prepareTemplateFilePayload,
} from '@/features/template-manager/utils/templateSnapshot'
import { createMetricValueDefaults } from '@/features/widget-editor/utils/widgetUtils'
import { deepEqual } from '@/store/store-utils'

vi.mock('@/lib/font-resources', () => ({ prepareDocumentFonts: vi.fn(async () => {}) }))

vi.mock('@/api/backend', () => ({
  loadSelectedRaster: vi.fn(),
}))

beforeEach(() => {
  vi.resetAllMocks()
})

describe('label font weight at template load', () => {
  test('rejects malformed present spacing rather than migrating it', () => {
    for (const letter_spacing of ['0', null, undefined, NaN, Infinity, -Infinity, 1e100]) {
      expect(() =>
        normalizeTemplateFilePayload({
          format: TEMPLATE_FILE_FORMAT,
          version: TEMPLATE_FILE_VERSION,
          config: { scene: {}, rasters: [], labels: [{ text: 'Title', letter_spacing }] },
          settings: { globalDefaults: {} },
        }),
      ).toThrow('letter_spacing')
    }
  })
  test('rejects malformed present italic values rather than migrating them', () => {
    for (const italic of ['true', null, 0, 1, undefined]) {
      expect(() =>
        normalizeTemplateFilePayload({
          format: TEMPLATE_FILE_FORMAT,
          version: TEMPLATE_FILE_VERSION,
          config: { scene: {}, rasters: [], labels: [{ text: 'Title', italic }] },
          settings: { globalDefaults: {} },
        }),
      ).toThrow('italic')
    }
  })
  test('rejects malformed present weights rather than migrating them', () => {
    for (const font_weight of ['400', null, 0, 1001, 1000.00001, NaN]) {
      expect(() =>
        normalizeTemplateFilePayload({
          format: TEMPLATE_FILE_FORMAT,
          version: TEMPLATE_FILE_VERSION,
          config: { scene: {}, rasters: [], labels: [{ text: 'Title', font_weight }] },
          settings: { globalDefaults: {} },
        }),
      ).toThrow('font_weight')
    }
  })

  test('migrates legacy templates in memory and preserves explicit weights through save/load', () => {
    for (const version of [2, 3]) {
      const source = {
        format: TEMPLATE_FILE_FORMAT,
        version,
        config: {
          scene: {},
          labels: [
            { id: 'title', text: 'Title', font: 'Teko.ttf', font_size: 60 },
            { id: 'subtitle', text: 'Subtitle', font: 'Inter ExtraBold.ttf', font_weight: 537, italic: true, letter_spacing: -1.25 },
            { id: 'upright', text: 'Upright', italic: false },
          ],
          values: [],
          plots: [],
        },
        settings: { globalDefaults: { font_text: 'Inter ExtraBold.ttf' } },
      }
      if (version === 3) source.config.rasters = []
      const loaded = normalizeTemplateFilePayload(source)
      expect(loaded.config.labels[0].font_weight).toBe(400)
      expect(loaded.config.labels[0].italic).toBe(false)
      expect(loaded.config.labels[0].letter_spacing).toBe(0)
      expect(loaded.config.labels[1]).toMatchObject({ font: 'Inter.ttf', font_weight: 537, italic: true, letter_spacing: -1.25 })
      expect(loaded.config.labels[2].italic).toBe(false)
      expect(loaded.settings.globalDefaults.font_text).toBe('Inter.ttf')
      expect(source.config.labels[0]).not.toHaveProperty('font_weight')
      expect(source.config.labels[0]).not.toHaveProperty('italic')
      expect(source.config.labels[0]).not.toHaveProperty('letter_spacing')
      expect(source.config.labels[1].font).toBe('Inter ExtraBold.ttf')
      const saved = createTemplateFilePayload({ config: loaded.config, globalDefaults: loaded.settings.globalDefaults })
      expect(normalizeTemplateFilePayload(saved)).toEqual(loaded)
    }
  })
})

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

    const normalized = normalizeTemplateConfig({ scene: {}, rasters: [], values: [lapTimer] })

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

    const normalized = normalizeTemplateConfig({ scene: {}, rasters: [], values: [lapTimer] }, { font_text: 'Teko.ttf', color_text: '#123456' })

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
        rasters: [],
        labels: [],
        values: [{ ...leanAngle, display_variants: { lean_angle: missingDiameter } }],
        plots: [],
      }),
    ).toThrow('lean_angle diameter must be a positive finite number')

    expect(() =>
      normalizeTemplateConfig({
        scene: {},
        rasters: [],
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
      rasters: [],
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
        rasters: [],
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
      config: { scene: {}, rasters: [], labels: [], values: [], plots: [] },
      globalDefaults: {},
    })

    expect(payload.format).toBe(TEMPLATE_FILE_FORMAT)
    expect(payload.version).toBe(3)
    expect(payload.config.rasters).toEqual([])
  })

  test('round-trips a standalone raster absolute path in template version 3', () => {
    const raster = {
      id: 'raster-1',
      x: 10,
      y: 20,
      width: 640,
      height: 480,
      rotation: 15,
      opacity: 0.8,
      path: 'C:\\images\\finish-line.png',
    }

    const payload = createTemplateFilePayload({
      config: { scene: {}, backdrops: [], rasters: [raster], labels: [], values: [], plots: [] },
      globalDefaults: {},
    })
    const normalized = normalizeTemplateFilePayload(payload)

    expect(payload.version).toBe(3)
    expect(normalized.config.rasters).toEqual([raster])
  })

  test('explicitly migrates template version 2 to an empty raster collection', () => {
    const normalized = normalizeTemplateFilePayload({
      format: TEMPLATE_FILE_FORMAT,
      version: 2,
      config: { scene: {}, labels: [], values: [], plots: [] },
      settings: { globalDefaults: {} },
    })

    expect(normalized.config.rasters).toEqual([])
    expect(() =>
      normalizeTemplateFilePayload({
        format: TEMPLATE_FILE_FORMAT,
        version: 2,
        config: { scene: {}, rasters: [], labels: [], values: [], plots: [] },
        settings: { globalDefaults: {} },
      }),
    ).toThrow('Version 2 template cannot contain rasters.')
  })

  test('does not repair a version 3 template that omits its raster collection', () => {
    expect(() =>
      normalizeTemplateFilePayload({
        format: TEMPLATE_FILE_FORMAT,
        version: 3,
        config: { scene: {}, labels: [], values: [], plots: [] },
        settings: { globalDefaults: {} },
      }),
    ).toThrow('Raster collection must be an array')
  })

  test('awaits fonts and raster snapshots while retaining broken rasters as recoverable resources', async () => {
    let finishFonts
    vi.mocked(prepareDocumentFonts).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finishFonts = resolve
        }),
    )
    vi.mocked(loadSelectedRaster)
      .mockResolvedValueOnce({ width: 320, height: 180, resourceId: 'snapshot-1' })
      .mockRejectedValueOnce(Object.assign(new Error('The image file is missing.'), { code: 'missing' }))

    const preparation = prepareTemplateFilePayload({
      format: TEMPLATE_FILE_FORMAT,
      version: 3,
      config: {
        scene: {},
        backdrops: [],
        rasters: [
          { id: 'ready', x: 0, y: 0, width: 100, height: 50, rotation: 0, opacity: 1, path: 'C:\\images\\ready.png' },
          { id: 'broken', x: 40, y: 50, width: 600, height: 400, rotation: 5, opacity: 0.7, path: 'C:\\images\\missing.png' },
        ],
        labels: [{ id: 'label-1', text: 'Still loads', x: 10, y: 10 }],
        values: [],
        plots: [],
      },
      settings: { globalDefaults: {} },
    })

    let completed = false
    preparation.then(() => {
      completed = true
    })
    await Promise.resolve()
    expect(completed).toBe(false)
    expect(prepareDocumentFonts).toHaveBeenCalledWith(expect.objectContaining({ config: expect.objectContaining({ labels: expect.any(Array) }) }))
    finishFonts()
    const prepared = await preparation

    expect(loadSelectedRaster).toHaveBeenCalledTimes(2)
    expect(prepared.templateState.config.rasters[1]).toMatchObject({ id: 'broken', x: 40, y: 50, width: 600, height: 400 })
    expect(prepared.templateState.config.labels).toHaveLength(1)
    expect(prepared.templateState.config.rasters[0].resourceId).toBe('snapshot-1')
    expect(prepared.templateState.config.rasters[1].resourceErrorCode).toBe('missing')
    expect(
      createTemplateState({ config: prepared.templateState.config, globalDefaults: prepared.templateState.settings.globalDefaults }).config.rasters,
    ).toEqual([
      { id: 'ready', x: 0, y: 0, width: 100, height: 50, rotation: 0, opacity: 1, path: 'C:\\images\\ready.png' },
      { id: 'broken', x: 40, y: 50, width: 600, height: 400, rotation: 5, opacity: 0.7, path: 'C:\\images\\missing.png' },
    ])
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
        rasters: [],
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
        rasters: [],
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
        rasters: [],
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
        rasters: [],
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

  test('rejects unsupported template versions explicitly', () => {
    expect(() =>
      normalizeTemplateFilePayload({
        format: TEMPLATE_FILE_FORMAT,
        version: 1,
        config: { scene: {}, labels: [], values: [], plots: [] },
        settings: { globalDefaults: {} },
      }),
    ).toThrow(`Unsupported template file version: 1. Expected ${TEMPLATE_FILE_VERSION}.`)
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
