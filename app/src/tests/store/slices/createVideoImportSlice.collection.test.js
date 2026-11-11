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
import { createTestVideo } from '../../helpers/videoFixtures'

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

function offsets() {
  const { videoEdits } = useStore.getState()
  return Object.fromEntries(Object.entries(videoEdits).map(([id, edits]) => [id, edits.offsetSeconds]))
}

/** Imports videos the way the import dialog does: placed in a draft of the collection, then committed. */
function importVideos(videos) {
  const { importedVideos, videoEdits, parsedActivity, activitySummary } = useStore.getState()
  const draft = produce({ importedVideos, videoEdits, parsedActivity, activitySummary }, (collection) => {
    addVideos(collection, videos)
  })
  useStore.getState().commitVideoCollection(draft)
}

function resetStore() {
  useStore.setState(useStore.getInitialState(), true)
  useStore.temporal.getState().clear()
}

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
