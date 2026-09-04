/**
 * Behavior tests for render config preparation.
 *
 * These specs document the render-focused layer that turns committed template
 * state into the backend-ready render payload.
 */

import { describe, expect, test } from 'vitest'
import { DEFAULT_EXPORT_RANGE } from '@/lib/template/template-constants'
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
      values: [{ id: 'value-1', value: 'speed', x: 10, y: 20, content_alignment: 'right' }],
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
        from: 5.25,
        to: 15.75,
      },
      exportCodec: 'prores_ks',
      importedVideoPath: null,
      availableCodecs: null,
    })

    expect(renderConfig.scene.update_rate).toBe(6)
    expect(renderConfig.scene.ffmpeg.codec).toBe('prores_ks')
    expect(renderConfig.scene.start).toBe(5.25)
    expect(renderConfig.scene.end).toBe(15.75)
    expect(renderConfig.scene.custom_export_range_active).toBe(true)
    expect(renderConfig.values[0].color).toBe('#ffffff')
    expect(renderConfig.values[0].content_alignment).toBe('right')
  })

  test('removes lean-angle ephemeral frame dimensions from the backend payload', () => {
    const config = {
      scene: {
        width: 1920,
        height: 1080,
        fps: 60,
        start: 0,
        end: 50,
      },
      labels: [],
      values: [
        {
          id: 'lean-angle-1',
          value: 'lean_angle',
          x: 10,
          y: 20,
          font: 'Arial.ttf',
          font_size: 60,
          display_type: 'lean_angle',
          display_variants: {
            lean_angle: {
              diameter: 180,
              track_thickness: 24,
            },
          },
        },
      ],
      plots: [],
    }

    const renderConfig = createRenderEffectiveConfig({
      config,
      globalDefaults: {},
      updateRate: 1,
      exportRange: { ...DEFAULT_EXPORT_RANGE },
      exportCodec: 'prores_ks',
      importedVideoPath: null,
      availableCodecs: null,
    })

    expect(renderConfig.values[0]).toMatchObject({ diameter: 180, track_thickness: 24 })
    expect(renderConfig.values[0]).not.toHaveProperty('width')
    expect(renderConfig.values[0]).not.toHaveProperty('height')
    expect(renderConfig.values[0]).not.toHaveProperty('display_variants')
  })

  test('rehydrates scene start/end from editor timeline when durable template config omits them', () => {
    const config = {
      scene: {
        width: 1920,
        height: 1080,
        fps: 60,
      },
      labels: [],
      values: [],
      plots: [],
    }

    const renderConfig = createRenderEffectiveConfig({
      config,
      globalDefaults: {},
      updateRate: 1,
      exportRange: { ...DEFAULT_EXPORT_RANGE },
      exportCodec: 'prores_ks',
      importedVideoPath: 'C:\\clip.mp4',
      importedVideoDuration: 24,
      importedVideoFps: 30,
      importedVideoFpsNum: 30,
      importedVideoFpsDen: 1,
      importedVideoResolution: { width: 1920, height: 1080 },
      timelineStart: 3,
      timelineEnd: 21,
      videoSyncOffsetSeconds: 0,
      availableCodecs: null,
    })

    expect(renderConfig.scene.start).toBe(3)
    expect(renderConfig.scene.end).toBe(21)
    expect(renderConfig.scene.custom_export_range_active).toBe(true)
  })

  test('skips composite scene fields for transparent override even when video is imported', () => {
    const config = {
      scene: {
        width: 1920,
        height: 1080,
        fps: 60,
      },
      labels: [],
      values: [],
      plots: [],
    }

    const renderConfig = createRenderEffectiveConfig({
      config,
      globalDefaults: {},
      updateRate: 1,
      exportRange: {
        ...DEFAULT_EXPORT_RANGE,
        type: 'custom',
        from: 5.25,
        to: 15.75,
      },
      exportMode: 'transparent',
      exportCodec: 'prores_ks',
      importedVideoPath: 'C:\\clip.mp4',
      importedVideoDuration: 24,
      importedVideoFps: 30,
      importedVideoFpsNum: 30,
      importedVideoFpsDen: 1,
      importedVideoResolution: { width: 1920, height: 1080 },
      timelineStart: 3,
      timelineEnd: 21,
      availableCodecs: null,
    })

    expect(renderConfig.scene.ffmpeg.codec).toBe('prores_ks')
    expect(renderConfig.scene.start).toBe(5.25)
    expect(renderConfig.scene.end).toBe(15.75)
    expect(renderConfig.scene.custom_export_range_active).toBe(true)
    expect(renderConfig.scene).not.toHaveProperty('composite_video_path')
    expect(renderConfig.scene).not.toHaveProperty('composite_render_duration')
  })

  test('preserves a negative composite offset for the full video export', () => {
    const renderConfig = createRenderEffectiveConfig({
      config: {
        scene: { width: 1920, height: 1080, fps: 60 },
        labels: [],
        values: [],
        plots: [],
      },
      globalDefaults: {},
      updateRate: 1,
      exportRange: { ...DEFAULT_EXPORT_RANGE },
      exportCodec: 'libx264',
      importedVideoPath: 'C:\\clip.mp4',
      importedVideoDuration: 30,
      importedVideoFps: 30,
      importedVideoFpsNum: 30,
      importedVideoFpsDen: 1,
      importedVideoResolution: { width: 1920, height: 1080 },
      timelineStart: 0,
      timelineEnd: 25,
      videoSyncOffsetSeconds: -5,
      availableCodecs: null,
    })

    expect(renderConfig.scene.composite_sync_offset).toBe(-5)
    expect(renderConfig.scene.composite_render_duration).toBe(30)
    expect(renderConfig.scene.start).toBe(0)
    expect(renderConfig.scene.end).toBe(25)
  })

  test('rejects composite ranges with no activity overlap', () => {
    expect(() =>
      createRenderEffectiveConfig({
        config: { scene: { width: 1920, height: 1080, fps: 60 }, labels: [], values: [], plots: [] },
        globalDefaults: {},
        updateRate: 1,
        exportRange: { ...DEFAULT_EXPORT_RANGE },
        exportCodec: 'libx264',
        importedVideoPath: 'C:\\clip.mp4',
        importedVideoDuration: 10,
        importedVideoFps: 30,
        importedVideoFpsNum: 30,
        importedVideoFpsDen: 1,
        importedVideoResolution: { width: 1920, height: 1080 },
        timelineStart: 0,
        timelineEnd: 25,
        videoSyncOffsetSeconds: -10,
        availableCodecs: null,
      }),
    ).toThrow(/positive overlap/)
  })
})
