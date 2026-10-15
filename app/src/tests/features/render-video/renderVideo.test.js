import { beforeEach, describe, expect, test } from 'vitest'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG } from '@/store/store-utils'
import { createPreviewRenderRequest, createRenderRequest } from '@/features/render-video/utils/renderRequest'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

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
    useStore.setState(videoStoreState([createTestVideo({ id: 'clip', path: 'C:/clip.mp4', durationSeconds: 30 })], { clip: 10 }))
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

  test('renders an MP4-only clip against its own telemetry in clip-local time', () => {
    const telemetryActivity = { sample_elapsed_seconds: [0, 15, 30], trim_end_seconds: 30 }
    const video = createTestVideo({
      id: 'clip',
      path: 'C:/clip.mp4',
      durationSeconds: 30,
      telemetry: { status: 'ready', activity: telemetryActivity },
    })
    useStore.setState({ parsedActivity: null, startSecond: 0, endSecond: 0, selectedSecond: 112, ...videoStoreState([video], { clip: 100 }) })
    const settings = {
      ...useStore.getState().renderSettings,
      exportMode: 'composite',
      codec: 'libx264',
      range: { type: 'custom', from: 105, to: 120 },
      outputPath: 'C:/renders/video.mp4',
    }

    const request = createRenderRequest({ editorSnapshot: useStore.getState(), settings })
    expect(request.payload.parsedActivity).toBe(telemetryActivity)
    expect(request.payload.config.scene).toMatchObject({
      start: 5,
      end: 20,
      composite_video_path: 'C:/clip.mp4',
      composite_sync_offset: 5,
      composite_video_trim_start: 5,
      composite_render_duration: 15,
    })

    const fullRequest = createRenderRequest({
      editorSnapshot: useStore.getState(),
      settings: { ...settings, range: { type: 'all', from: 0, to: 0 } },
    })
    expect(fullRequest.payload.config.scene).toMatchObject({ start: 0, end: 30, composite_sync_offset: 0, composite_render_duration: 30 })

    const preview = createPreviewRenderRequest(useStore.getState())
    expect(preview.parsedActivity).toBe(telemetryActivity)
    expect(preview.second).toBe(12)
  })

  test('rejects a current-video render of several clips', () => {
    useStore.setState(
      videoStoreState([createTestVideo({ id: 'a', path: 'C:/a.mp4' }), createTestVideo({ id: 'b', path: 'C:/b.mp4', importOrder: 1 })], {
        a: 0,
        b: 5,
      }),
    )
    expect(() =>
      createRenderRequest({
        editorSnapshot: useStore.getState(),
        settings: { ...useStore.getState().renderSettings, exportMode: 'composite', codec: 'libx264', outputPath: 'C:/renders/video.mp4' },
      }),
    ).toThrow('Exporting several videos is not supported yet')
  })
})
