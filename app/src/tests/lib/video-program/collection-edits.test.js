import { produce } from 'immer'
import { describe, expect, test } from 'vitest'
import { createActivitySummary } from '@/lib/activity/activity-summary'
import {
  addVideos,
  anchorVideo,
  applyFilenameCreationTimes,
  findUndeterminedVideoIds,
  moveVideoNextTo,
  placeAutomaticVideos,
  removeVideo,
  resetCreationTimes,
  resetVideoOffset,
  setVideoOffset,
  setVideoSyncTimezoneMode,
  shiftVideoOffsets,
} from '@/lib/video-program/collection-edits'
import { createTestVideo } from '@/tests/helpers/videoFixtures'

// Activity 08:00:00Z–08:02:00Z, Sofia (UTC+3).
const activityFile = {
  coverage: { heartrate: { source: 'direct' }, speed: { source: 'direct' } },
  extended_attributes: [],
  metadata: { duration_seconds: 120, sample_count: 7200, end_time: '2026-07-18T08:02:00Z', timezone: 'Europe/Sofia' },
  sync_time: '2026-07-18T08:00:00Z',
  file_format: 'fit',
  valid_attributes: ['speed', 'heartrate'],
}

function collection(activity = null) {
  return {
    importedVideos: [],
    videoEdits: {},
    videoSyncTimezoneMode: null,
    parsedActivity: activity,
    activitySummary: activity === null ? null : createActivitySummary(activity),
  }
}

function apply(state, recipe, ...args) {
  return produce(state, (draft) => void recipe(draft, ...args))
}

function clip(id, options = {}) {
  return createTestVideo({ id, path: `/videos/${id}.mp4`, registered: false, ...options })
}

function offsets(state) {
  return Object.fromEntries(Object.entries(state.videoEdits).map(([id, edits]) => [id, edits.offsetSeconds]))
}

function warnings(state) {
  return Object.fromEntries(state.importedVideos.map((video) => [video.id, video.syncWarning]))
}

describe('placement', () => {
  test('without an external activity clips sit at detected real time relative to the earliest clip', () => {
    const late = clip('late', { creationTime: '2026-07-18T08:05:00Z', timeSource: 'ffprobe' })
    const early = clip('early', { creationTime: '2026-07-18T08:00:00Z', timeSource: 'ffprobe', importOrder: 1 })
    const state = apply(collection(), addVideos, [late, early])
    expect(offsets(state)).toEqual({ late: 300, early: 0 })
    expect(warnings(state)).toEqual({ late: null, early: null })
  })

  test('with an external activity each clip syncs independently and clips outside it keep their signed offset', () => {
    const inside = clip('inside', { creationTime: '2026-07-18T08:00:30Z', timeSource: 'gps', durationSeconds: 20 })
    const before = clip('before', { creationTime: '2026-07-18T07:58:00Z', timeSource: 'gps', durationSeconds: 20, importOrder: 1 })
    const state = apply(collection(activityFile), addVideos, [inside, before])
    expect(offsets(state)).toEqual({ inside: 30, before: -120 })
    expect(warnings(state)).toEqual({ inside: null, before: 'Video is outside the activity' })
  })

  test('ambiguous camera clocks infer a timezone and an explicit selection places every clip again', () => {
    // Moscow (UTC+3): 10:51 clock text fits as local and as UTC.
    const wideActivity = {
      ...activityFile,
      sync_time: '2026-07-18T07:20:03.000Z',
      metadata: { ...activityFile.metadata, end_time: '2026-07-18T11:20:03.000Z', timezone: 'Europe/Moscow' },
    }
    let state = apply(collection(wideActivity), addVideos, [clip('video', { creationTime: '2026-07-18T10:51:00.000000Z', timeSource: 'ffprobe' })])
    expect(offsets(state)).toEqual({ video: 1857 })
    expect(state.videoSyncTimezoneMode).toBe('local')

    state = apply(state, setVideoSyncTimezoneMode, 'utc')
    expect(offsets(state)).toEqual({ video: 12657 })
    expect(state.videoSyncTimezoneMode).toBe('utc')
  })

  test('filename timestamps apply to every clip that has one and reset restores detected times', () => {
    const named = clip('named', { path: '/videos/VID_20260718_080100.mp4', creationTime: '2026-07-18T08:00:00Z', timeSource: 'gps' })
    const plain = clip('plain', { creationTime: '2026-07-18T08:00:00Z', timeSource: 'gps', importOrder: 1 })
    let state = apply(collection(), addVideos, [named, plain])

    state = apply(state, applyFilenameCreationTimes)
    expect(state.importedVideos.map(({ creationTime, timeSource }) => [creationTime, timeSource])).toEqual([
      ['2026-07-18T08:01:00', 'filename'],
      ['2026-07-18T08:00:00Z', 'gps'],
    ])
    state = apply(state, resetCreationTimes)
    expect(state.importedVideos.map(({ timeSource }) => timeSource)).toEqual(['gps', 'gps'])
    expect(() => apply(apply(collection(), addVideos, [clip('only')]), applyFilenameCreationTimes)).toThrow('No video filename contains')
  })
})

describe('adding videos', () => {
  test('existing clips keep their edits and new clips follow the shared correction of the automatic clips', () => {
    const first = clip('first', { creationTime: '2026-07-18T08:00:10Z', timeSource: 'gps', durationSeconds: 20 })
    const second = clip('second', { creationTime: '2026-07-18T08:00:40Z', timeSource: 'gps', durationSeconds: 20, importOrder: 1 })
    let state = apply(collection(activityFile), addVideos, [first, second])
    state = apply(state, shiftVideoOffsets, 2)
    state = apply(state, setVideoOffset, 'second', 50)

    const added = clip('added', { creationTime: '2026-07-18T08:01:20Z', timeSource: 'gps', durationSeconds: 20, importOrder: 2 })
    state = apply(state, addVideos, [added])

    expect(offsets(state)).toEqual({ first: 12, second: 50, added: 82 })
    expect(state.importedVideos.map((video) => video.id)).toEqual(['first', 'second', 'added'])
  })

  test('without an automatic clip new clips keep their real-time distance to a clip placed by hand', () => {
    let state = apply(collection(), addVideos, [clip('existing', { creationTime: '2026-07-18T08:10:00Z', timeSource: 'ffprobe' })])
    state = apply(state, setVideoOffset, 'existing', 40)

    const earlier = clip('earlier', { creationTime: '2026-07-18T08:05:00Z', timeSource: 'ffprobe', importOrder: 1 })
    state = apply(state, addVideos, [earlier])
    expect(offsets(state)).toEqual({ existing: 40, earlier: -260 })
  })

  test('with an activity, a clip placed by hand does not shift new clips', () => {
    let state = apply(collection(activityFile), addVideos, [
      clip('first', { creationTime: '2026-07-18T08:00:10Z', timeSource: 'gps', durationSeconds: 20 }),
    ])
    state = apply(state, setVideoOffset, 'first', 9000)
    state = apply(state, addVideos, [clip('added', { creationTime: '2026-07-18T08:01:00Z', timeSource: 'gps', durationSeconds: 20, importOrder: 1 })])
    expect(offsets(state)).toEqual({ first: 9000, added: 60 })
  })

  test('a clip whose time cannot be determined goes after the last placed clip', () => {
    const known = clip('known', { creationTime: '2026-07-18T08:00:00Z', timeSource: 'ffprobe', durationSeconds: 20 })
    const first = clip('first', { durationSeconds: 20, importOrder: 1 })
    const second = clip('second', { durationSeconds: 20, importOrder: 2 })
    const state = apply(collection(), addVideos, [known, first, second])

    expect(offsets(state)).toEqual({ known: 0, first: 20, second: 40 })
    expect(state.videoEdits.first.placementAnchor).toEqual({ videoId: 'known', side: 'after' })
    expect(state.videoEdits.second.placementAnchor).toEqual({ videoId: 'first', side: 'after' })
    expect(warnings(state)).toEqual({ known: null, first: null, second: null })
    expect(findUndeterminedVideoIds(state)).toEqual(new Set(['first', 'second']))
  })

  test('the first undeterminable clip of an empty collection stays at the start', () => {
    const state = apply(collection(), addVideos, [clip('first', { durationSeconds: 30 }), clip('second', { importOrder: 1 })])
    expect(offsets(state)).toEqual({ first: 0, second: 30 })
    expect(state.videoEdits.first.placementAnchor).toBeNull()
  })

  test('rejects an empty addition and paths already in the collection', () => {
    const state = apply(collection(), addVideos, [clip('a')])
    expect(() => apply(state, addVideos, [])).toThrow('At least one video')
    expect(() => apply(state, addVideos, [clip('b', { path: '/videos/a.mp4', importOrder: 1 })])).toThrow('already imported')
    expect(() => apply(state, addVideos, [clip('c', { importOrder: 1 }), clip('d', { path: '/videos/c.mp4', importOrder: 2 })])).toThrow(
      'already imported',
    )
  })
})

describe('placement anchors', () => {
  function undetermined() {
    const known = clip('known', { creationTime: '2026-07-18T08:00:30Z', timeSource: 'gps', durationSeconds: 20 })
    return apply(collection(), addVideos, [
      known,
      clip('first', { durationSeconds: 20, importOrder: 1 }),
      clip('second', { durationSeconds: 20, importOrder: 2 }),
    ])
  }

  test('anchored clips sit next to their anchor and follow it through placement until their time is known', () => {
    let state = apply(undetermined(), anchorVideo, 'first', 'known', 'before')
    expect(offsets(state)).toEqual({ known: 0, first: -20, second: 0 })

    state = produce(state, (draft) => {
      draft.parsedActivity = activityFile
      draft.activitySummary = createActivitySummary(activityFile)
      placeAutomaticVideos(draft)
    })
    expect(offsets(state)).toEqual({ known: 30, first: 10, second: 30 })

    state = produce(state, (draft) => {
      Object.assign(draft.importedVideos[1], { creationTime: '2026-07-18T08:01:00Z', timeSource: 'gps' })
      placeAutomaticVideos(draft)
    })
    expect(offsets(state)).toEqual({ known: 30, first: 60, second: 80 })
  })

  test('moving an anchored clip past the clip anchored to it hands its anchor on instead of forming a cycle', () => {
    const state = apply(undetermined(), moveVideoNextTo, 'first', 'second', 'after')
    expect(state.videoEdits.second.placementAnchor).toEqual({ videoId: 'known', side: 'after' })
    expect(state.videoEdits.first.placementAnchor).toEqual({ videoId: 'second', side: 'after' })
    expect(offsets(state)).toEqual({ known: 0, second: 20, first: 40 })
  })

  test('moving a clip into an occupied place inserts it there, before the clip that held the place', () => {
    const state = apply(undetermined(), moveVideoNextTo, 'second', 'known', 'after')
    expect(state.videoEdits.second.placementAnchor).toEqual({ videoId: 'known', side: 'after' })
    expect(state.videoEdits.first.placementAnchor).toEqual({ videoId: 'second', side: 'after' })
    expect(offsets(state)).toEqual({ known: 0, first: 40, second: 20 })
  })

  test('rejects anchors to unknown clips, to the clip itself, through cycles, and unknown sides', () => {
    const state = undetermined()
    expect(() => apply(state, anchorVideo, 'first', 'missing', 'after')).toThrow('Unknown imported video')
    expect(() => apply(state, anchorVideo, 'first', 'first', 'after')).toThrow('relative to itself')
    expect(() => apply(state, anchorVideo, 'first', 'second', 'before')).toThrow('relative to itself')
    expect(() => apply(state, anchorVideo, 'second', 'first', 'over')).toThrow('before or after')
  })

  test('removing a clip re-anchors the clips anchored to it and keeps the others in place', () => {
    let state = apply(undetermined(), removeVideo, 'first')
    expect(state.importedVideos.map((video) => video.id)).toEqual(['known', 'second'])
    expect(Object.keys(state.videoEdits)).toEqual(['known', 'second'])
    expect(state.videoEdits.second).toMatchObject({ offsetSeconds: 40, placementAnchor: { videoId: 'known', side: 'after' } })

    state = apply(state, removeVideo, 'known')
    expect(state.videoEdits.second).toMatchObject({ offsetSeconds: 40, placementAnchor: null })
    expect(() => apply(state, removeVideo, 'known')).toThrow('Unknown imported video')
  })
})

describe('starts set by hand', () => {
  function synced() {
    const first = clip('first', { creationTime: '2026-07-18T08:00:10Z', timeSource: 'gps', durationSeconds: 20 })
    const second = clip('second', { creationTime: '2026-07-18T08:00:40Z', timeSource: 'gps', durationSeconds: 20, importOrder: 1 })
    return apply(collection(activityFile), addVideos, [first, second])
  }

  test('a start set by hand survives detecting times again, an automatic one does not', () => {
    let state = apply(synced(), setVideoOffset, 'second', 70)
    expect(state.videoEdits.second.offsetSource).toBe('manual')
    state = apply(state, shiftVideoOffsets, 5)
    expect(offsets(state)).toEqual({ first: 15, second: 75 })

    state = apply(state, setVideoSyncTimezoneMode, 'utc')
    expect(offsets(state)).toEqual({ first: 10, second: 75 })
  })

  test('reset places the clip like the other automatic clips again', () => {
    let state = apply(synced(), shiftVideoOffsets, 3)
    state = apply(state, setVideoOffset, 'second', 70)
    state = apply(state, resetVideoOffset, 'second')
    expect(state.videoEdits.second.offsetSource).toBe('auto')
    expect(offsets(state)).toEqual({ first: 13, second: 43 })
  })

  test('reset returns an undeterminable clip next to its anchor', () => {
    const known = clip('known', { creationTime: '2026-07-18T08:00:00Z', timeSource: 'ffprobe', durationSeconds: 20 })
    let state = apply(collection(), addVideos, [known, clip('unknown', { durationSeconds: 20, importOrder: 1 })])
    state = apply(state, setVideoOffset, 'unknown', 500)
    state = apply(state, resetVideoOffset, 'unknown')
    expect(offsets(state)).toEqual({ known: 0, unknown: 20 })
    expect(warnings(state).unknown).toBeNull()
  })

  test('rejects non-finite edits and unknown clips', () => {
    const state = synced()
    expect(() => apply(state, setVideoOffset, 'first', Number.NaN)).toThrow('finite')
    expect(() => apply(state, setVideoOffset, 'missing', 1)).toThrow('Unknown imported video')
    expect(() => apply(state, shiftVideoOffsets, Infinity)).toThrow('finite')
    expect(() => apply(collection(), shiftVideoOffsets, 1)).toThrow('A video must be loaded')
  })
})
