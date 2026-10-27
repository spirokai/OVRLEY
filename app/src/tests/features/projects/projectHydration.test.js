import { expect, test } from 'vite-plus/test'
import {
  applyPreparedProjectState,
  applyProjectOwnedState,
  createProjectManualVideoSync,
  createProjectVideoEdits,
} from '@/features/projects/utils/projectHydration'
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
  useStore.getState().setBatchItemSkipOverlay('canonical-video', true)
  useStore.setState({ batchSnapshot: { batchId: 'old-batch', rendererBusy: false } })

  const manual = { videoId: 'canonical-video', landmarks: [], detectedLocationSecond: 30, speedThresholdKmh: 5, turnThresholdDegrees: 90 }
  applyProjectOwnedState(
    useStore,
    {
      editor: {
        config: { ...useStore.getState().config, scene: { ...useStore.getState().config.scene, width: 1280 } },
        globalDefaults: { ...useStore.getState().globalDefaults, color_text: '#123456' },
      },
      sources: { activity: null, videos: [projectVideo('canonical-video', 20)] },
      sync: {
        videoTimezoneMode: 'utc',
        manual: { videoId: 'canonical-video', landmarks: [], detectedLocationSecond: 30, speedThresholdKmh: 5, turnThresholdDegrees: 90 },
      },
      laps: { disabled: [] },
      transitionDefaults: { type: 'fade', durationSeconds: 1, splitGapThresholdSeconds: 4 },
      render: {
        outputMode: 'separate',
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
    },
    manual,
  )

  const state = useStore.getState()
  expect(state.parsedActivity).toBe(parsedActivity)
  expect(state.config.scene.width).toBe(1280)
  expect(state.globalDefaults.color_text).toBe('#123456')
  expect(state.loadedTemplateSource).toBeNull()
  expect(state.lastSavedTemplateState).toBeNull()
  expect(state.importedVideos).toHaveLength(1)
  expect(state.importedVideos[0].resolution).toBe(video.resolution)
  expect(state.importedVideos[0].preview).toBe(video.preview)
  // The collection (offsets, timezone mode) is committed before the project-owned settings.
  expect(state.videoEdits['canonical-video'].offsetSeconds).toBe(0)
  expect(state.manualVideoSync.videoId).toBe('canonical-video')
  expect(state.transitionDefaults).toEqual({ type: 'fade', durationSeconds: 1, splitGapThresholdSeconds: 4 })
  expect(state.manualVideoSync.detectedLocationSecond).toBe(30)
  expect(state.manualVideoSyncDetection.location).toEqual({ id: 'detected-course-location', type: 'location', time: 30 })
  expect(state.renderSettings).toMatchObject({ fps: 60, codec: 'libx264', qualityType: 'bitrate', qualityValue: 20 })
  expect(state.renderSettings.outputMode).toBe('separate')
  expect(state.batchOutputFolder).toBe('C:\\renders')
  expect(state.batchSkipOverlay).toEqual({})
  expect(state.batchSnapshot).toBeNull()
  expect(state.renderSettings).not.toHaveProperty('batchOutputFolder')
  expect(state.selectedSecond).toBe(50)
  expect(state.timelineViewport).toEqual({ viewStart: 25, viewEnd: 75 })
  expect(state.previewPlaybackState).toBe('paused')
})

function projectVideo(id, offsetSeconds, edits = {}) {
  return {
    id,
    path: { kind: 'absolute', value: `C:\\${id}.mp4` },
    offsetSeconds,
    offsetSource: 'auto',
    deletedParts: [],
    transitionIn: null,
    placementAnchor: null,
    ...edits,
  }
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
    laps: { disabled: [] },
    transitionDefaults: { type: 'wipeleft', durationSeconds: 2, splitGapThresholdSeconds: 2 },
    render: {
      outputMode: 'combined',
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

test('saved edits of every loaded video are restored and checked against the probed videos', () => {
  const videos = [createTestVideo({ id: 'clip-a', durationSeconds: 30 }), createTestVideo({ id: 'clip-b', durationSeconds: 40, importOrder: 1 })]
  const deletedPart = { id: 'cut-1', fromSecond: 5, toSecond: 10, transition: { type: 'dissolve', durationSeconds: 1.5 } }
  const projectVideos = [
    projectVideo('clip-a', -12.5, { offsetSource: 'manual' }),
    projectVideo('clip-b', 70, {
      deletedParts: [deletedPart],
      transitionIn: { type: 'none', durationSeconds: 0 },
      placementAnchor: { videoId: 'clip-a', side: 'after' },
    }),
  ]

  const edits = createProjectVideoEdits(videos, projectVideos)
  expect(edits).toEqual({
    'clip-a': { offsetSeconds: -12.5, offsetSource: 'manual', deletedParts: [], transitionIn: null, placementAnchor: null },
    'clip-b': {
      offsetSeconds: 70,
      offsetSource: 'auto',
      deletedParts: [deletedPart],
      transitionIn: { type: 'none', durationSeconds: 0 },
      placementAnchor: { videoId: 'clip-a', side: 'after' },
    },
  })
  expect(edits['clip-b'].deletedParts[0]).not.toBe(deletedPart)
  // An anchor to a skipped video is dropped; the clip keeps its offset.
  expect(createProjectVideoEdits([videos[1]], projectVideos)['clip-b']).toMatchObject({ offsetSeconds: 70, placementAnchor: null })
})

test('the landmarks of a skipped sync video are dropped', () => {
  const manual = { videoId: 'clip-skipped', landmarks: [{ id: 'stop-1', type: 'stop', videoSecond: 50 }], detectedLocationSecond: null }
  expect(createProjectManualVideoSync([createTestVideo({ id: 'clip-a', durationSeconds: 30 })], manual)).toMatchObject({
    videoId: 'clip-a',
    landmarks: [],
  })
  expect(createProjectManualVideoSync([], manual)).toMatchObject({ videoId: null, landmarks: [] })
})

test('saved edits outside the probed video duration are rejected before the project replaces the session', () => {
  const videos = [createTestVideo({ id: 'clip-a', durationSeconds: 30 })]
  expect(() => createProjectManualVideoSync(videos, { videoId: 'clip-a', landmarks: [{ id: 'stop-1', type: 'stop', videoSecond: 31 }] })).toThrow(
    /within the imported video duration/,
  )
  expect(() =>
    createProjectVideoEdits(videos, [projectVideo('clip-a', 0, { deletedParts: [{ id: 'cut-1', fromSecond: 20, toSecond: 31, transition: null }] })]),
  ).toThrow(/Deleted video parts must be within/)
  expect(() => createProjectVideoEdits(videos, [projectVideo('other', 0)])).toThrow(/not part of the project/)
})

test('a loaded project keeps its saved placements although the activity is activated during the load', () => {
  useStore.setState(useStore.getInitialState(), true)
  // Detected at 30 s into the activity, saved at 45 s.
  const video = createTestVideo({ id: 'clip-a', durationSeconds: 20, creationTime: '2026-07-18T08:00:30Z', timeSource: 'gps' })
  const project = createProject({ videos: [projectVideo('clip-a', 45)], timeline: { playheadSecond: 0, viewStart: 0, viewEnd: 120 } })
  const activity = {
    coverage: { speed: { source: 'direct' } },
    extended_attributes: [],
    metadata: { duration_seconds: 120, sample_count: 120, end_time: '2026-07-18T08:02:00Z', timezone: 'Europe/Sofia' },
    sync_time: '2026-07-18T08:00:00Z',
    file_format: 'fit',
    lap_start_elapsed_seconds: [],
    lap_durations_seconds: [],
    valid_attributes: ['speed'],
  }

  applyPreparedProjectState(useStore, project, {
    activity: { source: { path: 'C:\\ride.fit' }, parsedActivity: activity },
    collection: { importedVideos: [video], videoEdits: createProjectVideoEdits([video], project.sources.videos), videoSyncTimezoneMode: null },
    manualVideoSync: createProjectManualVideoSync([video], project.sync.manual),
  })

  expect(useStore.getState().videoEdits['clip-a'].offsetSeconds).toBe(45)
  expect(useStore.getState().parsedActivity).toBe(activity)
})
