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
