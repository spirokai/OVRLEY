import { describe, expect, test } from 'vitest'
import { createProjectDirtyState, createProjectSnapshot } from '@/features/projects/utils/projectSnapshot'
import { createPathLocator } from '@/features/projects/utils/projectPaths'
import { VIDEO_SYNC_MATCH_SCOPES } from '@/features/video-sync/data/videoSyncConstants'
import { createRenderEffectiveConfig } from '@/features/render-video/utils/renderConfig'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

describe('project snapshot contract', () => {
  test('projects only sources and project-owned settings', () => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState((state) => ({
      loadedTemplateSource: { kind: 'file', path: 'C:\\Events\\templates\\race.json' },
      activitySource: { kind: 'file', path: 'C:\\Events\\media\\ride.fit' },
      ...videoStoreState([createTestVideo({ id: 'runtime-video', path: 'D:\\video\\lap.mp4', durationSeconds: 100 })], { 'runtime-video': 7.5 }),
      parsedActivity: { samples: [1, 2, 3] },
      activitySummary: { durationSeconds: 100 },
      renderSettings: {
        ...state.renderSettings,
        outputMode: 'separate',
        fps: 60,
        codec: 'h264_nvenc',
        qualityType: 'bitrate',
        qualityValue: 35,
      },
      selectedSecond: 12.5,
      timelineViewport: { viewStart: 10, viewEnd: 30 },
    }))
    useStore.getState().setBatchOutputFolder('C:\\Events\\batch-renders')
    useStore.getState().setBatchItemSkipOverlay('runtime-video', true)

    const project = createProjectSnapshot(useStore.getState(), 'C:\\Events\\Race.oly')

    expect(project).not.toHaveProperty('template')
    expect(project.editor.config).toEqual(expect.objectContaining({ scene: expect.objectContaining({ width: 1920, height: 1080 }) }))
    expect(project.editor.config.scene.fps).toBe(project.render.fps)
    expect(project.editor.config.scene.updateRate).toBe(project.render.widgetUpdateRate)
    expect(project.editor.globalDefaults).toEqual(useStore.getState().globalDefaults)
    expect(project.sources).toEqual({
      activity: { path: { kind: 'project-relative', value: 'media/ride.fit' } },
      videos: [
        {
          id: 'runtime-video',
          path: { kind: 'absolute', value: 'D:\\video\\lap.mp4' },
          offsetSeconds: 7.5,
          deletedParts: [],
          transitionIn: null,
          placementAnchor: null,
        },
      ],
    })
    expect(project.sync).not.toHaveProperty('videoOffsetSeconds')
    expect(project.transitionDefaults).toEqual({ type: 'wipeleft', durationSeconds: 2, splitGapThresholdSeconds: 2, dialogShown: false })
    expect(project.render.fps).toBe(60)
    expect(project.render).toMatchObject({
      outputMode: 'separate',
      batchOutputFolder: 'C:\\Events\\batch-renders',
    })
    expect(project.render).toMatchObject({ qualityType: 'bitrate', qualityValue: 35 })
    expect(project.render).not.toHaveProperty('bitrateMbps')
    expect(project.timeline).toEqual({ playheadSecond: 12.5, viewStart: 10, viewEnd: 30 })

    const serialized = JSON.stringify(project)
    for (const forbidden of [
      'parsedActivity',
      'activitySummary',
      'importedVideos',
      'videoEdits',
      'http://127.0.0.1',
      'frameCount',
      'importOrder',
      'isVideoMuted',
      'selectedWidgetIds',
      'batchQueue',
      'batchSkipOverlay',
      'batchVideoFolder',
      'batchActiveItemId',
      'batchRunning',
      'skipOverlay',
      'blocked.mp4',
      'Sync failed',
    ]) {
      expect(serialized).not.toContain(forbidden)
    }

    const dirtyState = createProjectDirtyState(project)
    expect(dirtyState).not.toHaveProperty('savedAt')
    expect(dirtyState.content.editor).toEqual(project.editor)
    expect(dirtyState.content.transitionDefaults).toEqual(project.transitionDefaults)
    expect(dirtyState.timeline).toEqual(project.timeline)
  })

  test.each([
    ['quality', 27],
    ['bitrate', 37.5],
  ])('preserves rasters and %s settings across project and render snapshots', (qualityType, qualityValue) => {
    useStore.setState(useStore.getInitialState(), true)
    const raster = {
      id: 'raster-image',
      x: 10,
      y: 20,
      width: 320,
      height: 240,
      rotation: 15,
      opacity: 0.75,
      path: 'C:\\Events\\logo.png',
    }
    const resourceId = 'session-image-resource'
    useStore.setState((state) => ({
      config: { ...state.config, rasters: [{ ...raster, resourceId }] },
      ...videoStoreState([createTestVideo({ path: 'C:\\Events\\video.mp4', durationSeconds: 30 })]),
      renderSettings: { ...state.renderSettings, codec: 'libx265', qualityType, qualityValue },
    }))
    const state = useStore.getState()
    const project = createProjectSnapshot(state, 'C:\\Events\\Race.oly')
    expect(project.version).toBe(4)
    expect(project.editor.config.rasters).toEqual([raster])
    expect(project.rasterAssets).toEqual({})
    expect(project.render).toMatchObject({ qualityType, qualityValue })
    expect(project.render).not.toHaveProperty('bitrateMbps')
    expect(project.editor.config.scene).not.toHaveProperty('qualityType')
    expect(project.editor.config.scene).not.toHaveProperty('qualityValue')

    const renderConfig = createRenderEffectiveConfig({
      config: state.config,
      globalDefaults: state.globalDefaults,
      fps: state.renderSettings.fps,
      widgetUpdateRate: state.renderSettings.widgetUpdateRate,
      range: state.renderSettings.range,
      exportMode: 'composite',
      codec: state.renderSettings.codec,
      qualityType,
      qualityValue,
      video: state.importedVideos[0],
      videoOffsetSeconds: state.videoEdits[state.importedVideos[0].id].offsetSeconds,
      timelineStart: 0,
      timelineEnd: 30,
    })
    expect(renderConfig.rasters).toEqual([{ ...raster, resourceId }])
    expect(renderConfig.scene).toMatchObject({ qualityType, qualityValue, ffmpeg: { codec: 'libx265' } })
  })

  test('creates child-relative and external absolute locators', () => {
    expect(createPathLocator('C:\\Events\\media\\ride.fit', 'C:\\Events\\Race.oly')).toEqual({
      kind: 'project-relative',
      value: 'media/ride.fit',
    })
    expect(createPathLocator('D:\\media\\ride.fit', 'C:\\Events\\Race.oly')).toEqual({
      kind: 'absolute',
      value: 'D:\\media\\ride.fit',
    })
  })

  test('editor widget state is identical regardless of the selected template source', () => {
    useStore.setState(useStore.getInitialState(), true)
    const withoutTemplate = createProjectSnapshot(useStore.getState(), 'C:\\Events\\Race.oly')
    useStore.setState({ loadedTemplateSource: { kind: 'bundled', templateId: 'acid-titanium.json' } })
    const withTemplate = createProjectSnapshot(useStore.getState(), 'C:\\Events\\Race.oly')

    expect(withTemplate.editor).toEqual(withoutTemplate.editor)
    expect(withTemplate).not.toHaveProperty('template')
  })

  test('serializes a valid transparent mode when the default composite source is absent', () => {
    useStore.setState(useStore.getInitialState(), true)

    const project = createProjectSnapshot(useStore.getState(), 'C:\\Events\\Race.oly')

    expect(useStore.getState().renderSettings.exportMode).toBe('composite')
    expect(project.sources.videos).toEqual([])
    expect(project.render.exportMode).toBe('transparent')
    useStore.getState().setOutputMode('separate')
    const batchProject = createProjectSnapshot(useStore.getState(), 'C:\\Events\\Race.oly')
    expect(batchProject.render.outputMode).toBe('separate')
    expect(batchProject.render.exportMode).toBe('composite')
  })

  test('round-trips only durable manual video-sync state', () => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({
      activitySource: { kind: 'file', path: 'C:\\Events\\ride.fit' },
      ...videoStoreState([createTestVideo({ path: 'C:\\Events\\video.mp4', durationSeconds: 30 })]),
    })
    useStore.getState().hydrateVideoSyncState({
      videoId: 'video-1',
      landmarks: [
        { id: 'stop-1', type: 'stop', videoSecond: 4 },
        { id: 'left-1', type: 'leftTurn', videoSecond: 8 },
        { id: 'location-1', type: 'location', videoSecond: 12, activitySecond: null },
      ],
      detectedLocationSecond: 33.5,
      speedThresholdKmh: 7,
      turnThresholdDegrees: 120,
    })
    const revision = useStore.getState().beginVideoSyncCalculation(VIDEO_SYNC_MATCH_SCOPES.ALL)
    useStore.getState().completeVideoSyncCalculation(VIDEO_SYNC_MATCH_SCOPES.ALL, revision, { detection: {}, candidates: [{ offset: 4 }] })

    const project = createProjectSnapshot(useStore.getState(), 'C:\\Events\\Race.oly')

    expect(project.version).toBe(4)
    expect(project.sync.manual).toEqual({
      videoId: 'video-1',
      landmarks: [
        { id: 'stop-1', type: 'stop', videoSecond: 4 },
        { id: 'left-1', type: 'leftTurn', videoSecond: 8 },
        { id: 'location-1', type: 'location', videoSecond: 12, activitySecond: null },
      ],
      detectedLocationSecond: 33.5,
      speedThresholdKmh: 7,
      turnThresholdDegrees: 120,
    })
    expect(JSON.stringify(project)).not.toContain('manualVideoSyncResults')
  })

  test('saves every video in import order with its edits and the transition defaults', () => {
    useStore.setState(useStore.getInitialState(), true)
    const later = createTestVideo({ id: 'video-2', path: 'C:\\Events\\clips\\b.mp4', importOrder: 1 })
    const earlier = createTestVideo({ id: 'video-1', path: 'D:\\a.mp4' })
    const deletedPart = { id: 'cut-1', fromSecond: 5, toSecond: 10, transition: { type: 'dissolve', durationSeconds: 1.5 } }
    useStore.setState({
      importedVideos: [later, earlier],
      videoEdits: {
        'video-1': { offsetSeconds: -12.5, deletedParts: [], transitionIn: null, placementAnchor: null },
        'video-2': {
          offsetSeconds: 70,
          deletedParts: [deletedPart],
          transitionIn: { type: 'none', durationSeconds: 0 },
          placementAnchor: { videoId: 'video-1', side: 'after' },
        },
      },
      transitionDefaults: { type: 'none', durationSeconds: 3, splitGapThresholdSeconds: 0.5, dialogShown: true },
    })

    const project = createProjectSnapshot(useStore.getState(), 'C:\\Events\\Race.oly')

    expect(project.sources.videos).toEqual([
      {
        id: 'video-1',
        path: { kind: 'absolute', value: 'D:\\a.mp4' },
        offsetSeconds: -12.5,
        deletedParts: [],
        transitionIn: null,
        placementAnchor: null,
      },
      {
        id: 'video-2',
        path: { kind: 'project-relative', value: 'clips/b.mp4' },
        offsetSeconds: 70,
        deletedParts: [deletedPart],
        transitionIn: { type: 'none', durationSeconds: 0 },
        placementAnchor: { videoId: 'video-1', side: 'after' },
      },
    ])
    expect(project.sources.videos[1].deletedParts[0]).not.toBe(deletedPart)
    expect(project.transitionDefaults).toEqual({ type: 'none', durationSeconds: 3, splitGapThresholdSeconds: 0.5, dialogShown: true })
    expect(project.render.exportMode).toBe('composite')
  })
})
