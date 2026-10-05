import { describe, expect, test } from 'vitest'
import { createProjectDirtyState, createProjectSnapshot } from '@/features/projects/utils/projectSnapshot'
import { createPathLocator } from '@/features/projects/utils/projectPaths'
import { VIDEO_SYNC_MATCH_SCOPES } from '@/features/video-sync/data/videoSyncConstants'
import { createRenderEffectiveConfig } from '@/features/render-video/utils/renderConfig'
import useStore from '@/store/useStore'

describe('project snapshot contract', () => {
  test('projects only sources and project-owned settings', () => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState((state) => ({
      loadedTemplateSource: { kind: 'file', path: 'C:\\Events\\templates\\race.json' },
      activitySource: { kind: 'file', path: 'C:\\Events\\media\\ride.fit' },
      importedVideoPath: 'D:\\video\\lap.mp4',
      parsedActivity: { samples: [1, 2, 3] },
      activitySummary: { durationSeconds: 100 },
      importedVideoImportId: 'runtime-id',
      importedVideoPreviewUrl: 'http://127.0.0.1/runtime-id',
      importedVideoDuration: 100,
      renderSettings: {
        ...state.renderSettings,
        renderTarget: 'batch',
        fps: 60,
        codec: 'h264_nvenc',
        qualityType: 'bitrate',
        qualityValue: 35,
      },
      selectedSecond: 12.5,
      timelineViewport: { viewStart: 10, viewEnd: 30 },
    }))
    useStore.getState().setBatchVideoFolder('C:\\Events\\batch-videos')
    useStore.getState().setBatchOutputFolder('C:\\Events\\batch-renders')
    useStore.getState().setBatchQueueFromPaths(['C:\\Events\\batch-videos\\blocked.mp4'])
    useStore.getState().setBatchItemStatus(useStore.getState().batchQueue[0].id, 'blocked', 'Sync failed')

    const project = createProjectSnapshot(useStore.getState(), 'C:\\Events\\Race.oly')

    expect(project).not.toHaveProperty('template')
    expect(project.editor.config).toEqual(expect.objectContaining({ scene: expect.objectContaining({ width: 1920, height: 1080 }) }))
    expect(project.editor.config.scene.fps).toBe(project.render.fps)
    expect(project.editor.config.scene.updateRate).toBe(project.render.widgetUpdateRate)
    expect(project.editor.globalDefaults).toEqual(useStore.getState().globalDefaults)
    expect(project.sources).toEqual({
      activity: { path: { kind: 'project-relative', value: 'media/ride.fit' } },
      video: { path: { kind: 'absolute', value: 'D:\\video\\lap.mp4' } },
    })
    expect(project.render.fps).toBe(60)
    expect(project.render).toMatchObject({
      renderTarget: 'batch',
      batchVideoFolder: 'C:\\Events\\batch-videos',
      batchOutputFolder: 'C:\\Events\\batch-renders',
    })
    expect(project.render).toMatchObject({ qualityType: 'bitrate', qualityValue: 35 })
    expect(project.render).not.toHaveProperty('bitrateMbps')
    expect(project.timeline).toEqual({ playheadSecond: 12.5, viewStart: 10, viewEnd: 30 })

    const serialized = JSON.stringify(project)
    for (const forbidden of [
      'parsedActivity',
      'activitySummary',
      'importedVideoImportId',
      'importedVideoPreviewUrl',
      'importedVideoDuration',
      'isVideoMuted',
      'selectedWidgetIds',
      'batchQueue',
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
      importedVideoPath: 'C:\\Events\\video.mp4',
      renderSettings: { ...state.renderSettings, codec: 'libx265', qualityType, qualityValue },
    }))
    const state = useStore.getState()
    const project = createProjectSnapshot(state, 'C:\\Events\\Race.oly')
    expect(project.version).toBe(3)
    expect(project.editor.config.rasters).toEqual([raster])
    expect(project.rasterAssets).toEqual({})
    expect(project.render).toMatchObject({ qualityType, qualityValue })
    expect(project.render).not.toHaveProperty('bitrateMbps')
    expect(project.editor.config.scene).not.toHaveProperty('qualityType')
    expect(project.editor.config.scene).not.toHaveProperty('qualityValue')

    const renderConfig = createRenderEffectiveConfig({
      config: state.config,
      globalDefaults: state.globalDefaults,
      updateRate: state.renderSettings.widgetUpdateRate,
      exportMode: 'composite',
      exportCodec: state.renderSettings.codec,
      qualityType,
      qualityValue,
      importedVideoPath: state.importedVideoPath,
      importedVideoDuration: 30,
      importedVideoFpsNum: 30,
      importedVideoFpsDen: 1,
      importedVideoResolution: { width: 1920, height: 1080 },
      videoSyncOffsetSeconds: 0,
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
    expect(project.sources.video).toBeNull()
    expect(project.render.exportMode).toBe('transparent')
    useStore.getState().setRenderTarget('batch')
    const batchProject = createProjectSnapshot(useStore.getState(), 'C:\\Events\\Race.oly')
    expect(batchProject.render.renderTarget).toBe('batch')
    expect(batchProject.render.exportMode).toBe('composite')
  })

  test('round-trips only durable manual video-sync state', () => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({
      activitySource: { kind: 'file', path: 'C:\\Events\\ride.fit' },
      importedVideoPath: 'C:\\Events\\video.mp4',
    })
    useStore.getState().hydrateVideoSyncState({
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

    expect(project.version).toBe(3)
    expect(project.sync.manual).toEqual({
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
})
