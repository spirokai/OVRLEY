/**
 * Behavior tests for the template-state seam.
 *
 * These specs document the two public materializations this module owns:
 * durable template state for save/load and editor-effective config for
 * in-app editing.
 */

import { describe, expect, test } from 'vitest'
import { createDurableTemplateState, createEditorEffectiveConfig } from '@/lib/template-state'

describe('template-state seam', () => {
  test('materializes durable and editor-effective template shapes explicitly', () => {
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
    const editorConfig = createEditorEffectiveConfig({ config, globalDefaults })

    expect(durableState.settings.globalDefaults.opacity).toBe(0.8)
    expect(durableState.settings.globalDefaults.scale).toBe(1.6)
    expect(durableState.settings.globalDefaults.border_color).toBe('#555555')
    expect(durableState.config.scene).not.toHaveProperty('composite_video_path')
    expect(durableState.config.scene).not.toHaveProperty('opacity')
    expect(durableState.config.scene).not.toHaveProperty('scale')

    expect(editorConfig.scene.font).toBe('TextFont.ttf')
    expect(editorConfig.scene.color).toBe('#111111')
    expect(editorConfig.scene.opacity).toBe(0.8)
    expect(editorConfig.scene.scale).toBe(1.6)
    expect(editorConfig.labels[0].font).toBe('TextFont.ttf')
    expect(editorConfig.values[0].font).toBe('ValueFont.ttf')
    expect(editorConfig.values[0].icon_color).toBe('#333333')
    expect(editorConfig.values[0].unit_color).toBe('#444444')
    expect(editorConfig.plots[0].point_label.font).toBe('ValueFont.ttf')
  })
})
