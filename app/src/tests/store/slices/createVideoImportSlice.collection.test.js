/**
 * Store wrappers of the imported-video collection: committing a reviewed
 * collection, playhead and manual-sync ownership, drag previews, telemetry,
 * and undo history. Placement itself is tested with the collection recipes.
 */

import { produce } from 'immer'
import { beforeEach, describe, expect, test } from 'vite-plus/test'
import useStore from '@/store/useStore'
import { changeVideoCollection, undoHistory } from '@/features/undo-redo/undoHistory'
import { addVideos } from '@/lib/video-program/collection-edits'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { createTestVideo, videoStoreState } from '../../helpers/videoFixtures'

function telemetry({ coursePoints } = {}) {
  return {
    coverage: { speed: { source: 'direct' } },
    extended_attributes: [],
    metadata: { duration_seconds: 120, sample_count: 240 },
    file_format: 'mp4-telemetry',
    lap_start_elapsed_seconds: [],
    lap_durations_seconds: [],
    valid_attributes: ['speed'],
    sample_course_points: coursePoints,
  }
}

// Activity 08:00:00Z–08:02:00Z, Sofia (UTC+3).
const activityFile = {
  coverage: { heartrate: { source: 'direct' }, speed: { source: 'direct' } },
  extended_attributes: [],
  metadata: { duration_seconds: 120, sample_count: 7200, end_time: '2026-07-18T08:02:00Z', timezone: 'Europe/Sofia' },
  sync_time: '2026-07-18T08:00:00Z',
  file_format: 'fit',
  lap_start_elapsed_seconds: [],
  lap_durations_seconds: [],
  valid_attributes: ['speed', 'heartrate'],
}

function offsets() {
  const { videoEdits } = useStore.getState()
  return Object.fromEntries(Object.entries(videoEdits).map(([id, edits]) => [id, edits.offsetSeconds]))
}

function warnings() {
  return Object.fromEntries(useStore.getState().importedVideos.map((video) => [video.id, video.syncWarning]))
}

/** Imports videos the way the import dialog does: placed in a draft of the collection, then committed. */
function importVideos(videos) {
  const { importedVideos, videoEdits, videoSyncTimezoneMode, parsedActivity, activitySummary } = useStore.getState()
  const draft = produce({ importedVideos, videoEdits, videoSyncTimezoneMode, parsedActivity, activitySummary }, (collection) => {
    addVideos(collection, videos)
  })
  useStore.getState().commitVideoCollection(draft)
}

function resetStore() {
  useStore.setState(useStore.getInitialState(), true)
  useStore.temporal.getState().clear()
}

describe('committing a reviewed collection', () => {
  beforeEach(resetStore)

  test('replaces the collection, leaves the background image, and selects the first clip for manual sync', () => {
    const late = createTestVideo({ id: 'late', path: '/videos/late.mp4', creationTime: '2026-07-18T08:05:00Z', timeSource: 'ffprobe' })
    const early = createTestVideo({
      id: 'early',
      path: '/videos/early.mp4',
      creationTime: '2026-07-18T08:00:00Z',
      timeSource: 'ffprobe',
      importOrder: 1,
    })
    useStore.setState({ importedBackgroundImagePath: '/images/background.png' })

    importVideos([late, early])

    expect(offsets()).toEqual({ late: 300, early: 0 })
    expect(useStore.getState().manualVideoSync.videoId).toBe('early')
    expect(useStore.getState().importedBackgroundImagePath).toBeNull()
  })

  test('keeps the manual sync selection of a clip that stays and rejects videos without a preview', () => {
    importVideos([createTestVideo({ id: 'a', path: '/videos/a.mp4' })])
    useStore.getState().selectVideoSyncVideo('a')
    importVideos([createTestVideo({ id: 'b', path: '/videos/b.mp4', importOrder: 1 })])
    expect(useStore.getState().manualVideoSync.videoId).toBe('a')

    const unregistered = createTestVideo({ id: 'c', path: '/videos/c.mp4', importOrder: 2, registered: false })
    expect(() => importVideos([unregistered])).toThrow('registered for preview')
  })

  test('activity import places every automatic clip again and clearing it restores detected-time placement', () => {
    const video = createTestVideo({ creationTime: '2026-07-18T08:00:30Z', timeSource: 'gps', durationSeconds: 20 })
    importVideos([video])
    expect(offsets()).toEqual({ [video.id]: 0 })

    useStore.getState().activateActivityFile(activityFile)
    expect(offsets()).toEqual({ [video.id]: 30 })

    useStore.getState().clearActivityFile()
    expect(offsets()).toEqual({ [video.id]: 0 })
    expect(useStore.getState()).toMatchObject({ parsedActivity: null, activitySummary: null, activitySource: null })
  })
})

describe('removing a clip', () => {
  beforeEach(resetStore)

  test('moves manual sync to the first remaining clip and keeps the playhead on the timeline', () => {
    importVideos([
      createTestVideo({ id: 'early', path: '/videos/early.mp4', durationSeconds: 20, creationTime: '2026-07-18T08:00:00Z', timeSource: 'ffprobe' }),
    ])
    importVideos([
      createTestVideo({
        id: 'late',
        path: '/videos/late.mp4',
        durationSeconds: 20,
        creationTime: '2026-07-18T08:10:00Z',
        timeSource: 'ffprobe',
        importOrder: 1,
      }),
    ])
    useStore.setState({ selectedSecond: 615 })
    useStore.getState().selectVideoSyncVideo('late')

    useStore.getState().removeImportedVideo('late')
    expect(useStore.getState().manualVideoSync.videoId).toBe('early')
    expect(useStore.getState().selectedSecond).toBe(20)

    useStore.getState().removeImportedVideo('early')
    expect(useStore.getState().manualVideoSync.videoId).toBeNull()
    expect(useStore.getState().importedVideos).toEqual([])
  })
})

describe('transition defaults', () => {
  beforeEach(resetStore)

  test('stores chosen defaults and rejects malformed values', () => {
    useStore.getState().setTransitionDefaults({ type: 'none', durationSeconds: 3, splitGapThresholdSeconds: 0 })
    expect(useStore.getState().transitionDefaults).toEqual({ type: 'none', durationSeconds: 3, splitGapThresholdSeconds: 0 })

    for (const [defaults, message] of [
      [{ type: 'swirl', durationSeconds: 2, splitGapThresholdSeconds: 2 }, 'Unknown transition type'],
      [{ type: 'fade', durationSeconds: 0, splitGapThresholdSeconds: 2 }, 'positive number'],
      [{ type: 'fade', durationSeconds: Number.NaN, splitGapThresholdSeconds: 2 }, 'positive number'],
      [{ type: 'fade', durationSeconds: 2, splitGapThresholdSeconds: -1 }, 'Continuity threshold'],
    ]) {
      expect(() => useStore.getState().setTransitionDefaults(defaults)).toThrow(message)
    }
  })
})

describe('offset edits', () => {
  beforeEach(resetStore)

  function seed() {
    const first = createTestVideo({ id: 'first' })
    const second = createTestVideo({ id: 'second', importOrder: 1 })
    useStore.setState({ ...videoStoreState([first, second], { first: 10, second: 100 }), selectedSecond: 20 })
    useStore.setState((draft) => {
      draft.importedVideos.forEach((video) => {
        video.syncWarning = 'stale'
      })
    })
  }

  test('a per-clip start is set by hand, moves only that clip, and keeps the playhead on it', () => {
    seed()
    useStore.getState().setVideoOffset('first', 15, { compensatePlayhead: true })
    expect(offsets()).toEqual({ first: 15, second: 100 })
    expect(useStore.getState().videoEdits.first.offsetSource).toBe('manual')
    expect(warnings()).toEqual({ first: null, second: 'stale' })
    expect(useStore.getState().selectedSecond).toBe(25)
    expect(() => useStore.getState().setVideoOffset('missing', 1)).toThrow('Unknown imported video')
  })

  test('a reset returns the clip to automatic placement', () => {
    seed()
    useStore.getState().setVideoOffset('second', 140)
    useStore.getState().resetVideoOffset('second')
    expect(useStore.getState().videoEdits.second.offsetSource).toBe('auto')
  })

  test('a shared shift moves every clip by the same amount and keeps their individual placement', () => {
    seed()
    useStore.getState().shiftVideoOffsets(-12.5)
    expect(offsets()).toEqual({ first: -2.5, second: 87.5 })
    expect(warnings()).toEqual({ first: null, second: null })
    expect(useStore.getState().selectedSecond).toBe(20)
  })

  test('a drag preview shifts one clip or every clip without committing', () => {
    seed()
    useStore.getState().setVideoDragPreview({ videoId: 'second', deltaSeconds: 5 })
    const { importedVideos, videoEdits, videoDragPreview, transitionDefaults } = useStore.getState()
    const displayed = buildTimelineProgram({ videos: importedVideos, videoEdits, dragPreview: videoDragPreview, transitionDefaults })
    expect(displayed.clips.map(({ offsetSeconds }) => offsetSeconds)).toEqual([10, 105])
    expect(offsets()).toEqual({ first: 10, second: 100 })

    useStore.getState().setVideoDragPreview({ videoId: null, deltaSeconds: -10 })
    const shifted = buildTimelineProgram({
      videos: importedVideos,
      videoEdits,
      dragPreview: useStore.getState().videoDragPreview,
      transitionDefaults,
    })
    expect(shifted.bounds).toEqual({ startSecond: 0, endSecond: 150 })
    expect(() => useStore.getState().setVideoDragPreview({ videoId: 'missing', deltaSeconds: 1 })).toThrow('Unknown imported video')
  })
})

describe('telemetry ownership', () => {
  beforeEach(resetStore)

  test('settled telemetry stays on its clip, and late results for removed clips are discarded', () => {
    const video = createTestVideo()
    importVideos([video])
    const activity = telemetry()

    expect(useStore.getState().setVideoTelemetry(video.id, { status: 'ready', activity })).toBe(true)
    expect(useStore.getState().importedVideos[0].telemetry).toEqual({ status: 'ready', activity })
    expect(useStore.getState().parsedActivity).toBeNull()

    useStore.getState().clearImportedVideos()
    expect(useStore.getState().setVideoTelemetry(video.id, { status: 'absent', activity: null })).toBe(false)
    expect(() => useStore.getState().setVideoTelemetry(video.id, { status: 'pending', activity: null })).toThrow('settled')
  })

  test('ready telemetry fits course plots only without an external activity', () => {
    const video = createTestVideo()
    useStore.setState((draft) => {
      draft.config.plots = [{ id: 'course', value: 'course', width: 10, height: 10 }]
    })
    importVideos([video])
    const coursePoints = [
      [42.0, 23.0],
      [42.0, 23.2],
    ]

    useStore.getState().setVideoTelemetry(video.id, { status: 'ready', activity: telemetry({ coursePoints }) })

    expect(useStore.getState().config.plots[0].width).not.toBe(10)
  })

  test('clearing videos and switching to a background image empty the collection', () => {
    importVideos([createTestVideo()])
    useStore.getState().setImportedBackgroundImage('/images/background.png')
    expect(useStore.getState()).toMatchObject({ importedVideos: [], videoEdits: {}, importedBackgroundImagePath: '/images/background.png' })
    expect(useStore.getState().manualVideoSync.videoId).toBeNull()

    importVideos([createTestVideo()])
    useStore.getState().clearImportedVideos()
    expect(useStore.getState()).toMatchObject({ importedVideos: [], videoEdits: {}, importedBackgroundImagePath: null, videoDragPreview: null })
  })
})

describe('undo history across collection changes', () => {
  beforeEach(resetStore)

  test('undo restores offsets of remaining clips and never resurrects removed clips', () => {
    const kept = createTestVideo({ id: 'kept' })
    const removed = createTestVideo({ id: 'removed', path: '/videos/removed.mp4', importOrder: 1 })
    changeVideoCollection(useStore, () => importVideos([kept, removed]))
    useStore.getState().setVideoOffset('kept', 5)
    useStore.getState().setVideoOffset('removed', 50)

    const added = createTestVideo({ id: 'added', path: '/videos/added.mp4', importOrder: 2 })
    changeVideoCollection(useStore, () => {
      const state = useStore.getState()
      state.removeImportedVideo('removed')
      importVideos([added])
      state.setVideoOffset('kept', 7)
    })

    // The added clip has no recording time, so it went after the last clip.
    undoHistory(useStore)
    expect(useStore.getState().importedVideos.map(({ id }) => id)).toEqual(['kept', 'added'])
    expect(offsets()).toEqual({ kept: 5, added: 65 })

    undoHistory(useStore)
    expect(offsets()).toEqual({ kept: 0, added: 65 })
  })
})
