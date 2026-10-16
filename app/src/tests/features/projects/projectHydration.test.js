import { expect, test } from 'vitest'
import { applyProjectOwnedState } from '@/features/projects/utils/projectHydration'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

test('project restoration changes only project-owned settings', () => {
  useStore.setState(useStore.getInitialState(), true)
  const parsedActivity = { canonical: 'activity' }
  const video = createTestVideo({ id: 'canonical-video', path: 'C:\\video.mp4', durationSeconds: 30 })
  useStore.setState({
    activitySummary: { durationSeconds: 100 },
    parsedActivity,
    ...videoStoreState([video]),
  })
  useStore.getState().setBatchQueueFromPaths(['C:\\old\\queued.mp4'])
  useStore.setState({ batchSnapshot: { batchId: 'old-batch', rendererBusy: false } })

  applyProjectOwnedState(useStore, {
    editor: {
      config: { ...useStore.getState().config, scene: { ...useStore.getState().config.scene, width: 1280 } },
      globalDefaults: { ...useStore.getState().globalDefaults, color_text: '#123456' },
    },
    sources: { activity: null, videos: [projectVideo('canonical-video', 20)] },
    sync: {
      videoTimezoneMode: 'utc',
      manual: { videoId: 'canonical-video', landmarks: [], detectedLocationSecond: 30, speedThresholdKmh: 5, turnThresholdDegrees: 90 },
    },
    transitionDefaults: { type: 'fade', durationSeconds: 1, splitGapThresholdSeconds: 4, dialogShown: true },
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
  expect(state.importedVideos).toHaveLength(1)
  expect(state.importedVideos[0].resolution).toBe(video.resolution)
  expect(state.importedVideos[0].preview).toBe(video.preview)
  expect(state.videoEdits['canonical-video'].offsetSeconds).toBe(20)
  expect(state.manualVideoSync.videoId).toBe('canonical-video')
  expect(state.videoSyncTimezoneMode).toBe('utc')
  expect(state.transitionDefaults).toEqual({ type: 'fade', durationSeconds: 1, splitGapThresholdSeconds: 4, dialogShown: true })
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

function projectVideo(id, offsetSeconds, edits = {}) {
  return { id, path: { kind: 'absolute', value: `C:\\${id}.mp4` }, offsetSeconds, deletedParts: [], transitionIn: null, ...edits }
}

function createProject({ videos, manual = {}, timeline = { playheadSecond: 0, viewStart: 0, viewEnd: 30 } }) {
  return {
    editor: {
      config: useStore.getState().config,
      globalDefaults: useStore.getState().globalDefaults,
    },
    sources: { activity: null, videos },
    sync: {
      videoTimezoneMode: null,
      manual: {
        videoId: videos[0]?.id ?? null,
        landmarks: [],
        detectedLocationSecond: null,
        speedThresholdKmh: 5,
        turnThresholdDegrees: 90,
        ...manual,
      },
    },
    transitionDefaults: { type: 'wipeleft', durationSeconds: 2, splitGapThresholdSeconds: 2, dialogShown: false },
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
    timeline,
  }
}

test('project hydration restores every loaded video with its saved edits', () => {
  useStore.setState(useStore.getInitialState(), true)
  useStore.setState(
    videoStoreState([createTestVideo({ id: 'clip-a', durationSeconds: 30 }), createTestVideo({ id: 'clip-b', durationSeconds: 40, importOrder: 1 })]),
  )
  const deletedPart = { id: 'cut-1', fromSecond: 5, toSecond: 10, transition: { type: 'dissolve', durationSeconds: 1.5 } }
  const project = createProject({
    videos: [
      projectVideo('clip-a', -12.5),
      projectVideo('clip-b', 70, { deletedParts: [deletedPart], transitionIn: { type: 'none', durationSeconds: 0 } }),
    ],
    manual: { videoId: 'clip-b', landmarks: [{ id: 'stop-1', type: 'stop', videoSecond: 40 }] },
    timeline: { playheadSecond: -20, viewStart: -12.5, viewEnd: 110 },
  })

  applyProjectOwnedState(useStore, project)

  const state = useStore.getState()
  expect(state.videoEdits).toEqual({
    'clip-a': { offsetSeconds: -12.5, deletedParts: [], transitionIn: null },
    'clip-b': { offsetSeconds: 70, deletedParts: [deletedPart], transitionIn: { type: 'none', durationSeconds: 0 } },
  })
  expect(state.videoEdits['clip-b'].deletedParts[0]).not.toBe(deletedPart)
  expect(state.manualVideoSync.videoId).toBe('clip-b')
  expect(state.manualVideoSync.landmarks).toEqual([{ id: 'stop-1', type: 'stop', videoSecond: 40 }])
  expect(state.selectedSecond).toBe(-12.5)
  expect(state.timelineViewport).toEqual({ viewStart: -12.5, viewEnd: 110 })
})

test('project hydration drops the landmarks of a skipped sync video', () => {
  useStore.setState(useStore.getInitialState(), true)
  useStore.setState(videoStoreState([createTestVideo({ id: 'clip-a', durationSeconds: 30 })]))
  const project = createProject({
    videos: [projectVideo('clip-a', 0), projectVideo('clip-skipped', 40)],
    manual: { videoId: 'clip-skipped', landmarks: [{ id: 'stop-1', type: 'stop', videoSecond: 50 }] },
  })

  applyProjectOwnedState(useStore, project)

  expect(useStore.getState().manualVideoSync).toMatchObject({ videoId: 'clip-a', landmarks: [] })
  expect(Object.keys(useStore.getState().videoEdits)).toEqual(['clip-a'])

  useStore.setState(videoStoreState([]))
  applyProjectOwnedState(useStore, project)
  expect(useStore.getState().manualVideoSync).toMatchObject({ videoId: null, landmarks: [] })
})

test('project hydration rejects saved edits outside the probed video duration', () => {
  useStore.setState(useStore.getInitialState(), true)
  useStore.setState(videoStoreState([createTestVideo({ id: 'clip-a', durationSeconds: 30 })]))

  const landmarkProject = createProject({
    videos: [projectVideo('clip-a', 0)],
    manual: { landmarks: [{ id: 'stop-1', type: 'stop', videoSecond: 31 }] },
  })
  expect(() => applyProjectOwnedState(useStore, landmarkProject)).toThrow(/within the imported video duration/)

  const deletedPartProject = createProject({
    videos: [projectVideo('clip-a', 0, { deletedParts: [{ id: 'cut-1', fromSecond: 20, toSecond: 31, transition: null }] })],
  })
  expect(() => applyProjectOwnedState(useStore, deletedPartProject)).toThrow(/Deleted video parts must be within/)

  expect(() => applyProjectOwnedState(useStore, createProject({ videos: [projectVideo('other', 0)] }))).toThrow(/not part of the project/)
})
