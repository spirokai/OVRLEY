import { beforeEach, describe, expect, test } from 'vitest'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG } from '@/store/store-utils'
import { createRenderRequest } from '@/features/render-video/utils/renderRequest'

describe('render request preparation', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({ parsedActivity: { sample_elapsed_seconds: [0, 10, 20] }, endSecond: 20 })
  })

  test('prepares a render-effective payload from committed template state', () => {
    useStore.setState({
      config: {
        ...DEFAULT_CONFIG,
        scene: { ...DEFAULT_CONFIG.scene, fps: 60 },
        values: [{ id: 'value-1', value: 'speed', x: 10, y: 20 }],
      },
      globalDefaults: { ...useStore.getState().globalDefaults, color_values: '#abcdef' },
    })
    const request = createRenderRequest({
      editorSnapshot: useStore.getState(),
      settings: {
        ...useStore.getState().renderSettings,
        fps: 60,
        widgetUpdateRate: 6,
        exportMode: 'transparent',
        range: { type: 'custom', from: 5.25, to: 15.75 },
        outputPath: 'C:/renders/overlay.mov',
      },
    })

    expect(request.payload).toMatchObject({
      config: {
        scene: {
          start: 5.25,
          end: 15.75,
          fps: 60,
          update_rate: 6,
          custom_export_range_active: true,
          ffmpeg: { codec: 'prores_ks', prores_profile: '4444', pix_fmt: 'yuva444p10le' },
        },
        values: [expect.objectContaining({ id: 'value-1', color: '#abcdef' })],
      },
      parsedActivity: { sample_elapsed_seconds: [0, 10, 20] },
      outputPath: 'C:/renders/overlay.mov',
      overwrite: false,
    })
    expect(request.payload.config.scene).not.toHaveProperty('updateRate')
  })

  test('uses editor timeline bounds when hydrated template config omitted scene timing', () => {
    useStore.setState({
      config: { ...DEFAULT_CONFIG, scene: { width: DEFAULT_CONFIG.scene.width, height: DEFAULT_CONFIG.scene.height, fps: 30 } },
      startSecond: 8,
      endSecond: 42,
    })
    const request = createRenderRequest({
      editorSnapshot: useStore.getState(),
      settings: { ...useStore.getState().renderSettings, exportMode: 'transparent', outputPath: 'C:/renders/overlay.mov' },
    })
    expect(request.payload.config.scene).toMatchObject({ start: 8, end: 42 })
    expect(request.payload).toMatchObject({ outputPath: 'C:/renders/overlay.mov', overwrite: false })
  })

  test('clamps a composite custom range to the imported video when submitting the job', () => {
    useStore.setState({
      importedVideoPath: 'C:/clip.mp4',
      importedVideoDuration: 30,
      importedVideoFps: 30,
      importedVideoFpsNum: 30,
      importedVideoFpsDen: 1,
      importedVideoResolution: { width: 1920, height: 1080 },
      videoSyncOffsetSeconds: 10,
    })
    const settings = {
      ...useStore.getState().renderSettings,
      exportMode: 'composite',
      codec: 'libx264',
      range: { type: 'custom', from: 5.25, to: 39.75 },
      outputPath: 'C:/renders/video.mp4',
    }
    const request = createRenderRequest({ editorSnapshot: useStore.getState(), settings })
    expect(request.payload.config.scene).toMatchObject({
      start: 10,
      end: 39.75,
      composite_sync_offset: 10,
      composite_video_trim_start: 0,
      composite_render_duration: 29.75,
    })
    expect(request.payload).toMatchObject({ outputPath: 'C:/renders/video.mp4', overwrite: false })
    expect(() =>
      createRenderRequest({
        editorSnapshot: useStore.getState(),
        settings: { ...settings, range: { type: 'custom', from: 0, to: 5 } },
      }),
    ).toThrow('Custom export range must overlap the imported video range')
  })
})
