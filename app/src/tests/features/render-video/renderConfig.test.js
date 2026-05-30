/**
 * Behavior tests for render config preparation.
 *
 * These specs document the render-focused layer that turns committed template
 * state into the backend-ready render payload.
 */

import { describe, expect, test } from 'vitest'
import { DEFAULT_EXPORT_RANGE } from '@/features/template-manager/data/templateConstants'
import { createRenderEffectiveConfig } from '@/features/render-video/utils/renderConfig'

describe('render config preparation', () => {
  test('materializes render-effective config without changing durable template semantics', () => {
    const config = {
      scene: {
        width: 1920,
        height: 1080,
        fps: 60,
        start: 0,
        end: 50,
      },
      labels: [],
      values: [{ id: 'value-1', value: 'speed', x: 10, y: 20 }],
      plots: [],
    }
    const renderConfig = createRenderEffectiveConfig({
      config,
      globalDefaults: {
        color_values: '#ffffff',
      },
      updateRate: 6,
      exportRange: {
        ...DEFAULT_EXPORT_RANGE,
        type: 'custom',
        fromTime: '00:00:05',
        toTime: '00:00:15',
      },
      exportCodec: 'prores_ks',
      importedVideoPath: null,
      availableCodecs: null,
    })

    expect(renderConfig.scene.update_rate).toBe(6)
    expect(renderConfig.scene.ffmpeg.codec).toBe('prores_ks')
    expect(renderConfig.scene.start).toBe(5)
    expect(renderConfig.scene.end).toBe(15)
    expect(renderConfig.scene.custom_export_range_active).toBe(true)
    expect(renderConfig.values[0].color).toBe('#ffffff')
  })
})
