/**
 * Behavior tests for the template-state seam.
 *
 * These specs document the two public materializations this module owns:
 * durable template state for save/load and editor-effective config for
 * in-app editing.
 */

import { describe, expect, test } from 'vitest'
import {
  applyGlobalDefaults,
  createDurableTemplateState,
  createEditorEffectiveConfig,
  getEffectiveWidgetData,
  syncGlobalDefaultsToConfig,
} from '@/lib/template-state'
import { normalizeGlobalDefaults, normalizeTemplateConfig } from '@/lib/template-normalization'

/* -------------------------------------------------------------------------- */
/* normalizeGlobalDefaults                                                    */
/* -------------------------------------------------------------------------- */

describe('normalizeGlobalDefaults', () => {
  test('fills missing defaults from DEFAULT_GLOBAL_DEFAULTS', () => {
    const result = normalizeGlobalDefaults({ font_text: 'Custom.ttf' })

    expect(result.font_text).toBe('Custom.ttf')
    expect(result.font_values).toBe('Arial.ttf')
    expect(result.color_text).toBe('#ffffff')
    expect(result.color_values).toBe('#ffffff')
    expect(result.color_icons).toBe('#ffffff')
    expect(result.color_units).toBe('#ffffff')
    expect(result.opacity).toBe(1)
    expect(result.scale).toBe(1)
  })

  test('picks only known keys and strips unknown ones', () => {
    const result = normalizeGlobalDefaults({ font_text: 'X.ttf', unknownKey: 'shouldBeGone', extra: 42 })

    expect(result.font_text).toBe('X.ttf')
    expect(result).not.toHaveProperty('unknownKey')
    expect(result).not.toHaveProperty('extra')
  })

  test('normalizes color fields (lowercases hex)', () => {
    const result = normalizeGlobalDefaults({ color_text: '#FFAA00' })

    expect(result.color_text).toBe('#ffaa00')
  })

  test('handles undefined / null input', () => {
    const result = normalizeGlobalDefaults(undefined)

    expect(result.font_text).toBe('Arial.ttf')
    expect(result.opacity).toBe(1)
  })

  test('merges scene style defaults', () => {
    const result = normalizeGlobalDefaults({ border_color: '#111111', border_thickness: 3 })

    expect(result.border_color).toBe('#111111')
    expect(result.border_thickness).toBe(3)
    expect(result.shadow_color).toBe('#000000')
    expect(result.shadow_strength).toBe(0)
    expect(result.shadow_distance).toBe(0)
  })
})

/* -------------------------------------------------------------------------- */
/* normalizeTemplateConfig                                                    */
/* -------------------------------------------------------------------------- */

describe('normalizeTemplateConfig', () => {
  test('strips derived and render-only keys from scene', () => {
    const config = {
      scene: {
        width: 1920,
        height: 1080,
        font: 'SceneFont.ttf',
        color: '#abcdef',
        font_size: 28,
        opacity: 0.5,
        scale: 1.2,
        composite_video_path: '/path/to/video.mp4',
        composite_video_offset_start: 5,
        composite_bitrate: '8M',
        composite_render_duration: 60,
      },
    }
    const result = normalizeTemplateConfig(config)

    expect(result.scene.width).toBe(1920)
    expect(result.scene.height).toBe(1080)
    expect(result.scene).not.toHaveProperty('font')
    expect(result.scene).not.toHaveProperty('color')
    expect(result.scene).not.toHaveProperty('font_size')
    expect(result.scene).not.toHaveProperty('opacity')
    expect(result.scene).not.toHaveProperty('scale')
    expect(result.scene).not.toHaveProperty('composite_video_path')
    expect(result.scene).not.toHaveProperty('composite_bitrate')
    expect(result.scene).not.toHaveProperty('composite_render_duration')
  })

  test('normalizes labels and strips unknown keys', () => {
    const config = {
      labels: [{ id: 'label-1', text: 'Hello', x: 10, y: 20, extraField: 'remove' }],
    }
    const result = normalizeTemplateConfig(config)

    expect(result.labels[0].text).toBe('Hello')
    expect(result.labels[0].x).toBe(10)
    expect(result.labels[0].y).toBe(20)
    expect(result.labels[0]).not.toHaveProperty('extraField')
  })

  test('normalizes values with type-specific keys', () => {
    const config = {
      values: [{ id: 'value-1', value: 'speed', x: 100, y: 200, show_units: true }],
    }
    const result = normalizeTemplateConfig(config)

    expect(result.values[0].value).toBe('speed')
    expect(result.values[0].show_units).toBe(true)
  })

  test('normalizes plots with fallback global defaults', () => {
    const config = {
      plots: [{ id: 'plot-1', value: 'elevation', x: 30, y: 40, point_label: {} }],
    }
    const globalDefaults = { font_values: 'ElevFont.ttf', color_values: '#333333' }
    const result = normalizeTemplateConfig(config, globalDefaults)

    expect(result.plots[0].value).toBe('elevation')
    expect(result.plots[0].point_label.font).toBe('ElevFont.ttf')
  })

  test('normalizes heading value widget with display_variants', () => {
    const config = {
      values: [
        {
          id: 'heading-1',
          value: 'heading',
          x: 30,
          y: 40,
          display_type: 'heading_tape',
          display_variants: {
            heading_tape: {
              width: 400,
              height: 80,
              label_font: 'Teko.ttf',
              major_tick_thickness: 4,
              minor_tick_thickness: 1,
            },
          },
        },
      ],
    }

    const result = normalizeTemplateConfig(config)

    expect(result.values[0].display_type).toBe('heading_tape')
    expect(result.values[0].display_variants.heading_tape.label_font).toBe('Teko.ttf')
    expect(result.values[0].display_variants.heading_tape.major_tick_thickness).toBe(4)
    expect(result.values[0].display_variants.heading_tape.minor_tick_thickness).toBe(1)
  })
})

/* -------------------------------------------------------------------------- */
/* createDurableTemplateState                                                 */
/* -------------------------------------------------------------------------- */

describe('createDurableTemplateState', () => {
  test('materializes durable state from config and global defaults', () => {
    const config = {
      scene: {
        width: 1920,
        height: 1080,
        start: 3,
        end: 44,
        font: 'SceneFont.ttf',
        color: '#abcdef',
        font_size: 28,
        opacity: 0.4,
        scale: 1.3,
        border_color: '#123456',
        composite_video_path: 'C:\\clip.mp4',
      },
      labels: [{ id: 'label-1', text: 'Ride', x: 10, y: 12 }],
      values: [{ id: 'value-1', value: 'speed', x: 20, y: 22, show_units: true }],
      plots: [{ id: 'plot-1', value: 'elevation', x: 30, y: 32, point_label: {} }],
    }
    const globalDefaults = {
      font_text: 'TextFont.ttf',
      font_values: 'ValueFont.ttf',
      color_text: '#111111',
      color_values: '#222222',
      color_icons: '#333333',
      color_units: '#444444',
      opacity: 0.8,
      scale: 1.6,
      border_color: '#555555',
      border_thickness: 5,
      shadow_color: '#666666',
      shadow_strength: 7,
      shadow_distance: 9,
    }

    const durableState = createDurableTemplateState({ config, globalDefaults })

    expect(durableState.settings.globalDefaults.opacity).toBe(0.8)
    expect(durableState.settings.globalDefaults.scale).toBe(1.6)
    expect(durableState.settings.globalDefaults.border_color).toBe('#555555')
    expect(durableState.config.scene).not.toHaveProperty('composite_video_path')
    expect(durableState.config.scene).not.toHaveProperty('opacity')
    expect(durableState.config.scene).not.toHaveProperty('scale')
  })

  test('handles empty config gracefully', () => {
    const result = createDurableTemplateState({ config: {}, globalDefaults: { opacity: 1 } })

    expect(result.config).toBeDefined()
    expect(result.config.scene).toBeDefined()
    expect(result.settings.globalDefaults.opacity).toBe(1)
  })

  test('handles null config gracefully', () => {
    const result = createDurableTemplateState({ config: null, globalDefaults: { opacity: 1 } })

    expect(result.config).toBeDefined()
    expect(result.settings.globalDefaults.opacity).toBe(1)
  })
})

/* -------------------------------------------------------------------------- */
/* createEditorEffectiveConfig                                                */
/* -------------------------------------------------------------------------- */

describe('createEditorEffectiveConfig', () => {
  test('merges global defaults into scene, labels, values, and plots', () => {
    const config = {
      scene: {
        width: 1920,
        height: 1080,
        start: 3,
        end: 44,
        font: 'SceneFont.ttf',
        color: '#abcdef',
        font_size: 28,
        opacity: 0.4,
        scale: 1.3,
        border_color: '#123456',
        composite_video_path: 'C:\\clip.mp4',
      },
      labels: [{ id: 'label-1', text: 'Ride', x: 10, y: 12 }],
      values: [{ id: 'value-1', value: 'speed', x: 20, y: 22, show_units: true }],
      plots: [{ id: 'plot-1', value: 'elevation', x: 30, y: 32, point_label: {} }],
    }
    const globalDefaults = {
      font_text: 'TextFont.ttf',
      font_values: 'ValueFont.ttf',
      color_text: '#111111',
      color_values: '#222222',
      color_icons: '#333333',
      color_units: '#444444',
      opacity: 0.8,
      scale: 1.6,
      border_color: '#555555',
      border_thickness: 5,
      shadow_color: '#666666',
      shadow_strength: 7,
      shadow_distance: 9,
    }

    const editorConfig = createEditorEffectiveConfig({ config, globalDefaults })

    expect(editorConfig.scene.font_text).toBe('TextFont.ttf')
    expect(editorConfig.scene.color_text).toBe('#111111')
    expect(editorConfig.scene.opacity).toBe(0.8)
    expect(editorConfig.scene.scale).toBe(1.6)
    expect(editorConfig.labels[0].font).toBe('TextFont.ttf')
    expect(editorConfig.values[0].font).toBe('ValueFont.ttf')
    expect(editorConfig.values[0].icon_color).toBe('#333333')
    expect(editorConfig.values[0].unit_color).toBe('#444444')
    expect(editorConfig.plots[0].point_label.font).toBe('ValueFont.ttf')
  })

  test('handles null config input', () => {
    const result = createEditorEffectiveConfig({ config: null, globalDefaults: {} })

    expect(result).toBeNull()
  })

  test('preserves left_right_balance balance_format seeded by normalization', () => {
    const config = {
      values: [{ id: 'value-1', value: 'left_right_balance', x: 10, y: 10 }],
    }
    const normalizedConfig = normalizeTemplateConfig(config)
    const result = createEditorEffectiveConfig({ config: normalizedConfig, globalDefaults: {} })

    expect(result.values[0].balance_format).toBe('percent_label')
  })

  test('resolves heading value widget with display_variants', () => {
    const config = {
      values: [
        {
          id: 'heading-1',
          value: 'heading',
          x: 10,
          y: 10,
          display_type: 'heading_tape',
          display_variants: {
            heading_tape: { width: 400, height: 80, label_font: 'Teko.ttf' },
          },
        },
      ],
    }
    const result = createEditorEffectiveConfig({ config, globalDefaults: { font_values: 'Fallback.ttf' } })

    expect(result.values[0].display_variants.heading_tape.label_font).toBe('Teko.ttf')
  })
})

/* -------------------------------------------------------------------------- */
/* syncGlobalDefaultsToConfig                                                 */
/* -------------------------------------------------------------------------- */

describe('syncGlobalDefaultsToConfig', () => {
  test('pushes font_text and color_text globals into label widgets', () => {
    const config = {
      labels: [{ id: 'label-1', text: 'Hello', color: '#ffffff' }],
    }
    const globals = { font_text: 'LabelFont.ttf', color_text: '#0000ff' }
    const result = syncGlobalDefaultsToConfig(config, globals)

    expect(result.labels[0].font).toBe('LabelFont.ttf')
    expect(result.labels[0].color).toBe('#0000ff')
  })

  test('pushes font_values and color_values into value widgets', () => {
    const config = {
      values: [{ id: 'val-1', value: 'speed', color: '#ffffff' }],
    }
    const globals = { font_values: 'ValueFont.ttf', color_values: '#ff0000' }
    const result = syncGlobalDefaultsToConfig(config, globals)

    expect(result.values[0].font).toBe('ValueFont.ttf')
    expect(result.values[0].color).toBe('#ff0000')
  })

  test('pushes color_values into plot widgets', () => {
    const config = {
      plots: [{ id: 'plot-1', value: 'speed', color: '#111111' }],
    }
    const globals = { color_values: '#999999' }
    const result = syncGlobalDefaultsToConfig(config, globals)

    expect(result.plots[0].color).toBe('#999999')
  })

  test('respects changedKeys filtering', () => {
    const config = {
      labels: [{ id: 'label-1', text: 'Hello', color: '#ffffff' }],
    }
    const globals = { font_text: 'LabelFont.ttf', color_text: '#0000ff' }
    const result = syncGlobalDefaultsToConfig(config, globals, ['color_text'])

    expect(result.labels[0].font).not.toBe('LabelFont.ttf')
    expect(result.labels[0].color).toBe('#0000ff')
  })

  test('handles null / missing config', () => {
    expect(syncGlobalDefaultsToConfig(null, { opacity: 1 })).toBeNull()
    expect(syncGlobalDefaultsToConfig({}, null)).toEqual({})
  })

  test('does not set unit_color on time value widgets', () => {
    const config = {
      values: [{ id: 'val-1', value: 'time', unit_color: '#000000' }],
    }
    const globals = { color_units: '#ff0000' }
    const result = syncGlobalDefaultsToConfig(config, globals)

    expect(result.values[0].unit_color).toBe('#000000')
  })
})

/* -------------------------------------------------------------------------- */
/* getEffectiveWidgetData                                                     */
/* -------------------------------------------------------------------------- */

describe('getEffectiveWidgetData', () => {
  test('resolves label widget with global font and color defaults', () => {
    const widget = { id: 'l1', category: 'labels', data: { text: 'Hello', x: 10, y: 20 } }
    const globals = { font_text: 'LabelFont.ttf', color_text: '#112233' }
    const result = getEffectiveWidgetData(widget, globals)

    expect(result.font).toBe('LabelFont.ttf')
    expect(result.color).toBe('#112233')
  })

  test('resolves value widget with global font and color defaults', () => {
    const widget = { id: 'v1', category: 'values', data: { value: 'speed', x: 10, y: 20 } }
    const globals = { font_values: 'ValFont.ttf', color_values: '#445566', color_icons: '#778899' }
    const result = getEffectiveWidgetData(widget, globals)

    expect(result.font).toBe('ValFont.ttf')
    expect(result.color).toBe('#445566')
    expect(result.icon_color).toBe('#778899')
  })

  test('resolves plot widget with global color default', () => {
    const widget = { id: 'p1', category: 'plots', data: { value: 'speed', x: 10, y: 20 } }
    const globals = { color_values: '#aabbcc' }
    const result = getEffectiveWidgetData(widget, globals)

    expect(result.color).toBe('#aabbcc')
  })

  test('returns widget as-is when category is unknown', () => {
    const widget = { id: 'u1', category: 'unknown', data: { key: 'val' } }
    const result = getEffectiveWidgetData(widget, {})

    expect(result.key).toBe('val')
  })

  test('handles null widget', () => {
    expect(getEffectiveWidgetData(null, {})).toBeNull()
  })
})

/* -------------------------------------------------------------------------- */
/* applyGlobalDefaults (backward-compatible alias)                            */
/* -------------------------------------------------------------------------- */

describe('applyGlobalDefaults', () => {
  test('is a backward-compatible alias for createEditorEffectiveConfig', () => {
    const config = {
      labels: [{ id: 'l1', text: 'Test' }],
    }
    const globalDefaults = { font_text: 'AliasFont.ttf' }

    const fromAlias = applyGlobalDefaults(config, globalDefaults)
    const fromMain = createEditorEffectiveConfig({ config, globalDefaults })

    expect(fromAlias.labels[0].font).toBe(fromMain.labels[0].font)
    expect(fromAlias.labels[0].font).toBe('AliasFont.ttf')
  })
})
