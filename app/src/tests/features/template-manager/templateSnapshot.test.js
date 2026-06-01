import { describe, expect, test } from 'vitest'

import { TEMPLATE_FILE_FORMAT, TEMPLATE_FILE_VERSION } from '@/features/template-manager/data/templateConstants'
import {
  createTemplateFilePayload,
  normalizeTemplateConfig,
  normalizeTemplateFilePayload,
  templateStatesEqual,
} from '@/features/template-manager/utils/templateSnapshot'
import { createMetricValueDefaults } from '@/features/widget-editor/utils/widgetUtils'

describe('template snapshot standard metric schema', () => {
  test('creates standard metric defaults with display_unit as the canonical unit field', () => {
    const speedDefaults = createMetricValueDefaults('speed')
    const temperatureDefaults = createMetricValueDefaults('temperature')

    expect(speedDefaults.display_unit).toBe('kmh')
    expect(speedDefaults).not.toHaveProperty('speed_unit')
    expect(temperatureDefaults.display_unit).toBe('celsius')
    expect(temperatureDefaults).not.toHaveProperty('temperature_unit')
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
        values: [{ id: 'widget-2', value: 'speed', x: 10, y: 20 }],
        plots: [{ id: 'widget-3', value: 'heading', x: 30, y: 40 }],
      },
      globalDefaults: {},
    })

    expect(payload.config.labels[0].id).toBe('widget-1')
    expect(payload.config.values[0].id).toBe('widget-2')
    expect(payload.config.plots[0].id).toBe('widget-3')
  })

  test('upgrades legacy templates without widget ids when loading them', () => {
    const normalized = normalizeTemplateFilePayload({
      format: TEMPLATE_FILE_FORMAT,
      version: TEMPLATE_FILE_VERSION,
      config: {
        scene: {},
        labels: [{ text: 'Legacy label', x: 0, y: 0 }],
        values: [{ value: 'speed', x: 10, y: 20 }],
        plots: [{ value: 'heading', x: 30, y: 40 }],
      },
      settings: { globalDefaults: {} },
    })

    expect(normalized.config.labels[0].id).toMatch(/^widget-\d+$/)
    expect(normalized.config.values[0].id).toMatch(/^widget-\d+$/)
    expect(normalized.config.plots[0].id).toMatch(/^widget-\d+$/)
  })

  test('templateStatesEqual returns true for structurally equal template states', () => {
    const state = {
      config: { scene: { width: 1920, height: 1080, fps: 30 }, labels: [], values: [{ value: 'speed', x: 10 }], plots: [] },
      settings: { globalDefaults: { color_values: '#ffffff' } },
    }
    const copy = JSON.parse(JSON.stringify(state))

    expect(templateStatesEqual(state, copy)).toBe(true)
  })

  test('templateStatesEqual returns false when config differs', () => {
    const left = {
      config: { scene: { width: 1920, height: 1080 }, labels: [], values: [], plots: [] },
      settings: { globalDefaults: {} },
    }
    const right = {
      config: { scene: { width: 1280, height: 720 }, labels: [], values: [], plots: [] },
      settings: { globalDefaults: {} },
    }

    expect(templateStatesEqual(left, right)).toBe(false)
  })

  test('templateStatesEqual returns false when settings differ', () => {
    const left = {
      config: { scene: {}, labels: [], values: [], plots: [] },
      settings: { globalDefaults: { color_values: '#ffffff' } },
    }
    const right = {
      config: { scene: {}, labels: [], values: [], plots: [] },
      settings: { globalDefaults: { color_values: '#000000' } },
    }

    expect(templateStatesEqual(left, right)).toBe(false)
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
})
