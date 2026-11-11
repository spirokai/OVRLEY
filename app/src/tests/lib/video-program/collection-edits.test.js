import { produce } from 'immer'
import { describe, expect, test } from 'vite-plus/test'
import { createActivitySummary } from '@/lib/activity/activity-summary'
import {
  addVideos,
  anchorVideo,
  findUndeterminedVideoIds,
  moveVideoNextTo,
  placeAutomaticVideos,
  removeVideo,
  resetVideoOffset,
  setVideoCreationTimeSource,
  setVideoOffset,
  setVideosTimezoneMode,
  setVideoTimezoneMode,
  shiftVideoOffsets,
  syncVideos,
} from '@/lib/video-program/collection-edits'
import { createTestVideo } from '@/tests/helpers/videoFixtures'

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

function collection(activity = null) {
  return {
    importedVideos: [],
    videoEdits: {},
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

  // Moscow (UTC+3): 10:51 clock text fits as local and as UTC.
  const wideActivity = {
    ...activityFile,
    sync_time: '2026-07-18T07:20:03.000Z',
    metadata: { ...activityFile.metadata, end_time: '2026-07-18T11:20:03.000Z', timezone: 'Europe/Moscow' },
  }
  const cameraClock = { creationTime: '2026-07-18T10:51:00.000000Z', timeSource: 'ffprobe' }

  test('ambiguous camera clocks infer a timezone and selecting one for every clip places them again', () => {
    let state = apply(collection(wideActivity), addVideos, [clip('video', cameraClock)])
    expect(offsets(state)).toEqual({ video: 1857 })
    expect(state.videoEdits.video.timezoneMode).toBe('local')

    state = apply(state, setVideosTimezoneMode, 'utc')
    expect(offsets(state)).toEqual({ video: 12657 })
    expect(state.videoEdits.video.timezoneMode).toBe('utc')
  })

  test("one clip's timezone mode places only that clip again", () => {
    let state = apply(collection(wideActivity), addVideos, [clip('first', cameraClock), clip('second', { ...cameraClock, importOrder: 1 })])
    state = apply(state, setVideoTimezoneMode, 'second', 'utc')
    expect(offsets(state)).toEqual({ first: 1857, second: 12657 })
    expect(state.videoEdits.first.timezoneMode).toBe('local')
    expect(state.videoEdits.second.timezoneMode).toBe('utc')
  })

  test("one clip's recording time comes from its filename, read as local time, or from detection", () => {
    const named = clip('named', { ...cameraClock, path: '/videos/GX_20260718_090000.mp4' })
    let state = apply(collection(wideActivity), addVideos, [named, clip('other', { ...cameraClock, importOrder: 1 })])
    state = apply(state, setVideoTimezoneMode, 'named', 'utc')

    state = apply(state, setVideoCreationTimeSource, 'named', 'filename')
    expect(state.importedVideos[0]).toMatchObject({ timeSource: 'filename' })
    expect(state.videoEdits.named.timezoneMode).toBe('local')
    // 09:00 Moscow time is 06:00Z, before the activity starts at 07:20:03Z.
    expect(offsets(state)).toEqual({ named: -4803, other: 1857 })

    state = apply(state, setVideoCreationTimeSource, 'named', 'detected')
    expect(state.importedVideos[0]).toMatchObject({ creationTime: cameraClock.creationTime, timeSource: 'ffprobe' })
    expect(offsets(state)).toEqual({ named: 1857, other: 1857 })
    expect(() => apply(state, setVideoCreationTimeSource, 'other', 'filename')).toThrow('no valid YYYYMMDD_HHMMSS timestamp')
  })

  test('a clip placed by hand keeps its start when its recording time changes, until it is synced', () => {
    let state = apply(collection(wideActivity), addVideos, [clip('video', cameraClock)])
    state = apply(state, setVideoOffset, 'video', 100)
    state = apply(state, setVideoTimezoneMode, 'video', 'utc')
    expect(offsets(state)).toEqual({ video: 100 })

    state = apply(state, resetVideoOffset, 'video')
    expect(offsets(state)).toEqual({ video: 12657 })
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

    state = apply(state, setVideosTimezoneMode, 'utc')
    expect(offsets(state)).toEqual({ first: 10, second: 75 })
  })

  test('syncing every clip places the ones set by hand by detected time again', () => {
    let state = apply(synced(), setVideoOffset, 'second', 70)
    state = apply(state, shiftVideoOffsets, 5)
    state = apply(state, syncVideos)
    expect(offsets(state)).toEqual({ first: 10, second: 40 })
    expect(state.videoEdits.second.offsetSource).toBe('auto')
  })

  test('reset returns an undeterminable clip next to its anchor', () => {
    const known = clip('known', { creationTime: '2026-07-18T08:00:00Z', timeSource: 'ffprobe', durationSeconds: 20 })
    let state = apply(collection(), addVideos, [known, clip('unknown', { durationSeconds: 20, importOrder: 1 })])
    state = apply(state, setVideoOffset, 'unknown', 500)
    state = apply(state, resetVideoOffset, 'unknown')
    expect(offsets(state)).toEqual({ known: 0, unknown: 20 })
    expect(warnings(state).unknown).toBeNull()
  })
})
