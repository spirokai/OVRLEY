import { beforeEach, describe, expect, test } from 'vite-plus/test'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG } from '@/store/store-utils'
import { createPreviewRenderRequest, createRenderRequest } from '@/features/render-video/utils/renderRequest'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

describe('render request preparation', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({ parsedActivity: { sample_elapsed_seconds: [0, 10, 20], trim_end_seconds: 20 } })
  })

  test.each([
    [3.5, 30, 3.5, 3.5 + 1 / 30],
    [3.5, 29.97, 3.5, 3.5 + 1 / 29],
    [20, 20, 19.95, 20],
  ])('prepares a one-frame PNG request at second %s', (selectedSecond, fps, start, end) => {
    useStore.setState({ selectedSecond, renderSettings: { ...useStore.getState().renderSettings, fps } })
    const request = createPreviewRenderRequest(useStore.getState())
    expect(request.config.scene).toMatchObject({ start, end, fps })
    expect(request.second).toBe(selectedSecond)
    expect(request.parsedActivity).toBe(useStore.getState().parsedActivity)
  })

  test('prepares a render-effective payload of the trimmed activity from committed template state', () => {
    useStore.setState({
      activityTrim: { startSecond: 5.25, endSecond: 15.75 },
      config: {
        ...DEFAULT_CONFIG,
        scene: { ...DEFAULT_CONFIG.scene, fps: 60 },
        values: [{ id: 'value-1', value: 'speed', display_type: 'text', x: 10, y: 20 }],
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
      parsedActivity: { sample_elapsed_seconds: [0, 10, 20], trim_end_seconds: 20 },
      outputPath: 'C:/renders/overlay.mov',
      overwrite: false,
    })
    expect(request.payload.config.scene).not.toHaveProperty('updateRate')
  })

  test('renders the whole untrimmed activity', () => {
    useStore.setState({
      config: { ...DEFAULT_CONFIG, scene: { width: DEFAULT_CONFIG.scene.width, height: DEFAULT_CONFIG.scene.height, fps: 30 } },
    })
    const request = createRenderRequest({
      editorSnapshot: useStore.getState(),
      settings: { ...useStore.getState().renderSettings, exportMode: 'transparent', outputPath: 'C:/renders/overlay.mov' },
    })
    expect(request.payload.config.scene).toMatchObject({ start: 0, end: 20, custom_export_range_active: false })
    expect(request.payload).toMatchObject({ outputPath: 'C:/renders/overlay.mov', overwrite: false })
  })

  test('renders videos through the batch service at the destination of their output mode', () => {
    useStore.setState(videoStoreState([createTestVideo({ id: 'clip', path: 'C:/clip.mp4', durationSeconds: 30 })], { clip: 10 }))
    const settings = {
      ...useStore.getState().renderSettings,
      exportMode: 'composite',
      codec: 'libx264',
      outputPath: 'C:/renders/video.mp4',
    }
    const outputs = [
      { id: 'C:/renders/video.mp4', pieces: [{ sourceId: 'source', startFrame: 0, endFrame: 900, skipOverlay: false }], transitions: [] },
    ]
    const batchReview = { inspectionId: 'inspection', outputs, offsets: { source: 10 } }
    const request = createRenderRequest({ editorSnapshot: useStore.getState(), settings, batchReview })
    expect(request.target).toBe('batch')
    expect(request.payload).toMatchObject({
      inspectionId: 'inspection',
      activity: { mode: 'externalActivity', offsets: { source: 10 } },
      destination: { kind: 'file', path: 'C:/renders/video.mp4', overwrite: false },
      outputs,
    })
    expect(createRenderRequest({ editorSnapshot: useStore.getState(), settings, batchReview, overwrite: true }).payload.destination.overwrite).toBe(
      true,
    )

    const separate = { ...settings, outputMode: 'separate' }
    expect(() => createRenderRequest({ editorSnapshot: useStore.getState(), settings: separate, batchReview })).toThrow(
      'Render output target is required',
    )
    useStore.setState({ batchOutputFolder: 'C:/renders' })
    expect(createRenderRequest({ editorSnapshot: useStore.getState(), settings: separate, batchReview }).payload.destination).toEqual({
      kind: 'sourceNamed',
      directory: 'C:/renders',
    })
  })

  test('renders the whole activity as one overlay regardless of the videos and the activity trim when asked', () => {
    useStore.setState({
      ...videoStoreState([createTestVideo({ id: 'clip', path: 'C:/clip.mp4', durationSeconds: 5 })], { clip: 10 }),
      activityTrim: { startSecond: 10, endSecond: 15 },
    })
    const request = createRenderRequest({
      editorSnapshot: useStore.getState(),
      settings: {
        ...useStore.getState().renderSettings,
        exportMode: 'transparent',
        renderFullActivity: true,
        outputPath: 'C:/renders/overlay.mov',
      },
    })
    expect(request.target).toBe('single')
    expect(request.payload.config.scene).toMatchObject({ start: 0, end: 20, custom_export_range_active: false })
    expect(request.payload.config.scene).not.toHaveProperty('video_path')
    expect(request.settings).not.toHaveProperty('renderFullActivity')
  })

  test('the activity trim decides only without videos, so a preview frame between videos ignores it', () => {
    useStore.setState({ activityTrim: { startSecond: 5, endSecond: 8 }, selectedSecond: 10 })
    // Held inside the trim: the last frame before its end.
    expect(createPreviewRenderRequest(useStore.getState()).config.scene).toMatchObject({ start: 8 - 1 / 30, end: 8 })

    useStore.setState(videoStoreState([createTestVideo({ id: 'clip', path: 'C:/clip.mp4', durationSeconds: 2 })], { clip: 15 }))
    expect(createPreviewRenderRequest(useStore.getState()).config.scene.start).toBe(10)
  })

  test('previews the frame of the clip the preview shows, against its own telemetry in clip-local time', () => {
    const telemetryActivity = { sample_elapsed_seconds: [0, 15, 30], trim_end_seconds: 30 }
    const video = createTestVideo({
      id: 'clip',
      path: 'C:/clip.mp4',
      durationSeconds: 30,
      telemetry: { status: 'ready', activity: telemetryActivity },
    })
    const other = createTestVideo({ id: 'other', path: 'C:/other.mp4', durationSeconds: 30, importOrder: 1 })
    useStore.setState({
      parsedActivity: null,
      startSecond: 0,
      endSecond: 0,
      selectedSecond: 112,
      ...videoStoreState([other, video], { other: 0, clip: 100 }),
    })

    const preview = createPreviewRenderRequest(useStore.getState())
    expect(preview.parsedActivity).toBe(telemetryActivity)
    expect(preview.second).toBe(12)
    expect(preview.config.scene.composite_video_path).toBe('C:/clip.mp4')
  })
})
