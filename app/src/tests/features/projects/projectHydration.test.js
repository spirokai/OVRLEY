import { expect, test } from 'vitest'
import { applyProjectOwnedState } from '@/features/projects/utils/projectHydration'
import useStore from '@/store/useStore'

test('project restoration changes only project-owned settings', () => {
  useStore.setState(useStore.getInitialState(), true)
  const parsedActivity = { canonical: 'activity' }
  const videoResolution = { width: 1920, height: 1080 }
  useStore.setState({
    activitySummary: { durationSeconds: 100 },
    parsedActivity,
    importedVideoPath: 'C:\\video.mp4',
    importedVideoDuration: 30,
    importedVideoResolution: videoResolution,
    importedVideoImportId: 'canonical-import',
  })
  useStore.getState().setBatchQueueFromPaths(['C:\\old\\queued.mp4'])
  useStore.setState({ batchSnapshot: { batchId: 'old-batch', rendererBusy: false } })

  applyProjectOwnedState(useStore, {
    editor: {
      config: { ...useStore.getState().config, scene: { ...useStore.getState().config.scene, width: 1280 } },
      globalDefaults: { ...useStore.getState().globalDefaults, color_text: '#123456' },
    },
    sync: {
      videoOffsetSeconds: 20,
      videoTimezoneMode: 'utc',
      manual: { landmarks: [], detectedLocationSecond: 30, speedThresholdKmh: 5, turnThresholdDegrees: 90 },
    },
    render: {
      renderTarget: 'batch',
      batchVideoFolder: 'C:\\videos',
      batchOutputFolder: 'C:\\renders',
      fps: 60,
      widgetUpdateRate: 2,
      exportMode: 'composite',
      codec: 'libx264',
      qualityType: 'bitrate',
      qualityValue: 20,
      range: { type: 'custom', from: 10, to: 80 },
    },
    timeline: { playheadSecond: 50, viewStart: 25, viewEnd: 75 },
  })

  const state = useStore.getState()
  expect(state.parsedActivity).toBe(parsedActivity)
  expect(state.config.scene.width).toBe(1280)
  expect(state.globalDefaults.color_text).toBe('#123456')
  expect(state.loadedTemplateSource).toBeNull()
  expect(state.lastSavedTemplateState).toBeNull()
  expect(state.importedVideoResolution).toBe(videoResolution)
  expect(state.importedVideoImportId).toBe('canonical-import')
  expect(state.videoSyncOffsetSeconds).toBe(20)
  expect(state.videoSyncTimezoneMode).toBe('utc')
  expect(state.manualVideoSync.detectedLocationSecond).toBe(30)
  expect(state.manualVideoSyncDetection.location).toEqual({ id: 'detected-course-location', type: 'location', time: 30 })
  expect(state.renderSettings).toMatchObject({ fps: 60, codec: 'libx264', qualityType: 'bitrate', qualityValue: 20 })
  expect(state.renderSettings.renderTarget).toBe('batch')
  expect(state.batchVideoFolder).toBe('C:\\videos')
  expect(state.batchOutputFolder).toBe('C:\\renders')
  expect(state.batchQueue).toEqual([])
  expect(state.batchSnapshot).toBeNull()
  expect(state.renderSettings).not.toHaveProperty('batchVideoFolder')
  expect(state.renderSettings).not.toHaveProperty('batchOutputFolder')
  expect(state.selectedSecond).toBe(50)
  expect(state.timelineViewport).toEqual({ viewStart: 25, viewEnd: 75 })
  expect(state.previewPlaybackState).toBe('paused')
})

test('project hydration rejects landmarks without matching staged video bounds', () => {
  useStore.setState(useStore.getInitialState(), true)
  useStore.setState({ importedVideoPath: 'C:\\video.mp4', importedVideoDuration: 30 })

  const project = {
    editor: {
      config: useStore.getState().config,
      globalDefaults: useStore.getState().globalDefaults,
    },
    sync: {
      videoOffsetSeconds: 0,
      videoTimezoneMode: null,
      manual: {
        landmarks: [{ id: 'stop-1', type: 'stop', videoSecond: 31 }],
        detectedLocationSecond: null,
        speedThresholdKmh: 5,
        turnThresholdDegrees: 90,
      },
    },
    render: {
      renderTarget: 'current',
      batchVideoFolder: null,
      batchOutputFolder: null,
      fps: 30,
      widgetUpdateRate: 1,
      exportMode: 'composite',
      codec: 'libx264',
      qualityType: 'quality',
      qualityValue: 18,
      range: { type: 'all', from: 0, to: 0 },
    },
    timeline: { playheadSecond: 0, viewStart: 0, viewEnd: 30 },
  }

  expect(() => applyProjectOwnedState(useStore, project)).toThrow(/within the imported video duration/)

  useStore.setState({ importedVideoPath: null, importedVideoDuration: null })
  project.sync.manual.landmarks[0].videoSecond = 1
  expect(() => applyProjectOwnedState(useStore, project)).toThrow(/require an imported video/)
})
