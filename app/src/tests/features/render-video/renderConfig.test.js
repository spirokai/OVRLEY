/**
 * Behavior tests for render config preparation.
 *
 * These specs document the render-focused layer that turns committed template
 * state into the backend-ready render payload.
 */

import { describe, expect, test } from 'vite-plus/test'
import { createBatchRenderTemplate, createRenderEffectiveConfig } from '@/features/render-video/utils/renderConfig'
import { createTestVideo } from '@/tests/helpers/videoFixtures'

describe('render config preparation', () => {
  test('renders the trimmed activity window and scopes the route to it', () => {
    const config = {
      scene: {
        width: 1920,
        height: 1080,
        fps: 60,
      },
      labels: [],
      values: [{ id: 'value-1', value: 'speed', display_type: 'text', x: 10, y: 20, content_alignment: 'right' }],
      plots: [],
    }
    const renderConfig = createRenderEffectiveConfig({
      config,
      fps: 60,
      exportMode: 'transparent',
      globalDefaults: {
        color_values: '#ffffff',
      },
      widgetUpdateRate: 6,
      activityWindow: { startSecond: 5.25, endSecond: 15.75, isTrimmed: true },
      codec: 'prores_ks',
      video: null,
      videoOffsetSeconds: 0,
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

  test('renders the whole untrimmed activity without scoping the route', () => {
    const renderConfig = createRenderEffectiveConfig({
      config: { scene: { width: 1920, height: 1080, fps: 30 }, labels: [], values: [], plots: [] },
      fps: 30,
      exportMode: 'transparent',
      globalDefaults: {},
      widgetUpdateRate: 1,
      activityWindow: { startSecond: 0, endSecond: 73.4, isTrimmed: false },
      codec: 'prores_ks',
      video: null,
      videoOffsetSeconds: 0,
      availableCodecs: null,
    })

    expect(renderConfig.scene.start).toBe(0)
    expect(renderConfig.scene.end).toBe(73.4)
    expect(renderConfig.scene.custom_export_range_active).toBe(false)
  })

  test('sends the canonical lean-angle diameter config to the backend', () => {
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
      fps: 60,
      exportMode: 'transparent',
      globalDefaults: {},
      widgetUpdateRate: 1,
      activityWindow: { startSecond: 0, endSecond: 50, isTrimmed: false },
      codec: 'prores_ks',
      video: null,
      videoOffsetSeconds: 0,
      availableCodecs: null,
    })

    expect(renderConfig.values[0]).toMatchObject({ diameter: 180, track_thickness: 24 })
    expect(renderConfig.values[0]).not.toHaveProperty('width')
    expect(renderConfig.values[0]).not.toHaveProperty('height')
    expect(renderConfig.values[0]).not.toHaveProperty('display_variants')
    const captured = structuredClone(config)
    const batchTemplate = createBatchRenderTemplate(config, {})
    expect(batchTemplate.values[0]).toMatchObject({ diameter: 180, track_thickness: 24 })
    expect(batchTemplate.values[0]).not.toHaveProperty('display_variants')
    expect(batchTemplate.scene).not.toHaveProperty('start')
    expect(batchTemplate.scene).not.toHaveProperty('end')
    expect(batchTemplate.scene).not.toHaveProperty('fps')
    expect(config).toEqual(captured)
  })

  test('takes scene start/end from the activity window when the durable template omits them', () => {
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
      fps: 60,
      exportMode: 'composite',
      globalDefaults: {},
      widgetUpdateRate: 1,
      activityWindow: { startSecond: 3, endSecond: 21, isTrimmed: false },
      codec: 'libx264',
      video: createTestVideo({ path: 'C:\\clip.mp4', durationSeconds: 24 }),
      videoOffsetSeconds: 0,
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
      fps: 60,
      globalDefaults: {},
      widgetUpdateRate: 1,
      activityWindow: { startSecond: 5.25, endSecond: 15.75, isTrimmed: true },
      exportMode: 'transparent',
      codec: 'prores_ks',
      video: createTestVideo({ path: 'C:\\clip.mp4', durationSeconds: 24 }),
      videoOffsetSeconds: 0,
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
      fps: 60,
      exportMode: 'composite',
      widgetUpdateRate: 1,
      activityWindow: { startSecond: 0, endSecond: 25, isTrimmed: false },
      codec: 'libx264',
      video: createTestVideo({ path: 'C:\\clip.mp4', durationSeconds: 30 }),
      videoOffsetSeconds: -5,
      availableCodecs: null,
    })

    expect(renderConfig.scene.composite_sync_offset).toBe(-5)
    expect(renderConfig.scene.composite_render_duration).toBe(30)
    expect(renderConfig.scene.start).toBe(0)
    expect(renderConfig.scene.end).toBe(25)
  })

  test('takes composite source fields from the imported video entity', () => {
    const video = createTestVideo({ path: 'C:\\portrait.mp4', durationSeconds: 20, fps: { num: 60000, den: 2002 }, width: 1080, height: 1920 })
    const renderConfig = createRenderEffectiveConfig({
      config: { scene: { width: 1920, height: 1080, fps: 60 }, labels: [], values: [], plots: [] },
      globalDefaults: {},
      fps: 60,
      exportMode: 'composite',
      widgetUpdateRate: 2,
      activityWindow: { startSecond: 0, endSecond: 40, isTrimmed: false },
      codec: 'libx264',
      qualityType: 'quality',
      qualityValue: 18,
      video,
      videoOffsetSeconds: 4,
      availableCodecs: null,
    })

    expect(renderConfig.scene).toMatchObject({
      width: 1080,
      height: 1920,
      composite_video_path: 'C:\\portrait.mp4',
      composite_sync_offset: 4,
      composite_video_fps_num: 30000,
      composite_video_fps_den: 1001,
      composite_video_duration: video.durationSeconds,
      composite_render_duration: video.durationSeconds,
      composite_video_trim_start: 0,
      composite_widget_update_rate: 2,
      qualityType: 'quality',
      qualityValue: 18,
    })
  })

  test('requires an imported video for composite export', () => {
    expect(() =>
      createRenderEffectiveConfig({
        config: { scene: { width: 1920, height: 1080, fps: 60 }, labels: [], values: [], plots: [] },
        globalDefaults: {},
        fps: 60,
        exportMode: 'composite',
        widgetUpdateRate: 1,
        activityWindow: { startSecond: 0, endSecond: 25, isTrimmed: false },
        codec: 'libx264',
        video: null,
        videoOffsetSeconds: 0,
        availableCodecs: null,
      }),
    ).toThrow('Imported video is required for composite export')
  })

  test('keeps a composite video outside the activity, which renders rasters only', () => {
    const config = createRenderEffectiveConfig({
      config: { scene: { width: 1920, height: 1080, fps: 60 }, labels: [], values: [], plots: [] },
      globalDefaults: {},
      fps: 60,
      exportMode: 'composite',
      widgetUpdateRate: 1,
      activityWindow: { startSecond: 0, endSecond: 25, isTrimmed: false },
      codec: 'libx264',
      video: createTestVideo({ path: 'C:\\clip.mp4', durationSeconds: 10 }),
      videoOffsetSeconds: -10,
      availableCodecs: null,
    })
    expect(config.scene).toMatchObject({ composite_sync_offset: -10, composite_render_duration: 10, composite_video_trim_start: 0 })
  })
})
