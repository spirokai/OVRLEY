import { beforeEach, describe, expect, test } from 'vitest'
import useStore from '@/store/useStore'
import { createImportedVideo } from '@/store/slices/createVideoImportSlice'
import { changeVideoCollection, undoHistory } from '@/features/undo-redo/undoHistory'
import { resolveActivityContext, resolveAvailableMetrics } from '@/lib/activity/activity-context'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { createTestVideo, videoStoreState } from '../../helpers/videoFixtures'

function telemetry({ durationSeconds = 120, attributes = ['speed'], coursePoints } = {}) {
  return {
    coverage: Object.fromEntries(attributes.map((attribute) => [attribute, { source: 'direct' }])),
    extended_attributes: [],
    metadata: { duration_seconds: durationSeconds, sample_count: 240 },
    file_format: 'mp4-telemetry',
    valid_attributes: attributes,
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
  valid_attributes: ['speed', 'heartrate'],
}

const preview = { importId: 'import-1', url: 'http://127.0.0.1:1/video/import-1', warnings: [] }

function metadata(overrides = {}) {
  return { path: '/videos/ride.mp4', duration: 60, fpsNum: 30, fpsDen: 1, resolution: { width: 1920, height: 1080 }, hasAudio: true, ...overrides }
}

function offsets() {
  const { videoEdits } = useStore.getState()
  return Object.fromEntries(Object.entries(videoEdits).map(([id, edits]) => [id, edits.offsetSeconds]))
}

function warnings() {
  return Object.fromEntries(useStore.getState().importedVideos.map((video) => [video.id, video.syncWarning]))
}

function resetStore() {
  useStore.setState(useStore.getInitialState(), true)
  useStore.temporal.getState().clear()
}

describe('createImportedVideo ingress', () => {
  test('stores exact reduced FPS, a frame-exact duration, and display-oriented resolution', () => {
    const video = createImportedVideo(metadata({ duration: 10.0103, fpsNum: 60000, fpsDen: 2002, rotationDegrees: 90 }), {
      id: 'v',
      importOrder: 3,
      preview,
    })
    expect(video.fps).toEqual({ num: 30000, den: 1001 })
    expect(video.frameCount).toBe(300)
    expect(video.durationSeconds).toBeCloseTo(10.01, 12)
    expect(video.resolution).toEqual({ width: 1080, height: 1920 })
    expect(video).toMatchObject({ id: 'v', importOrder: 3, preview, syncWarning: null, telemetry: { status: 'pending', activity: null } })
  })

  test.each([
    ['a missing path', { path: '' }, 'must include a path'],
    ['a non-positive duration', { duration: 0 }, 'invalid duration'],
    ['a missing FPS numerator', { fpsNum: undefined }, 'invalid frame rate numerator'],
    ['a fractional FPS denominator', { fpsDen: 1.5 }, 'invalid frame rate denominator'],
    ['unknown audio presence', { hasAudio: undefined }, 'audio presence'],
    ['a sub-frame duration', { duration: 0.001 }, 'at least one frame'],
  ])('rejects %s', (_label, overrides, message) => {
    expect(() => createImportedVideo(metadata(overrides), { importOrder: 0, preview })).toThrow(message)
  })

  test('accepts settled telemetry and rejects inconsistent telemetry', () => {
    const activity = telemetry()
    expect(createImportedVideo(metadata(), { importOrder: 0, preview, telemetry: { status: 'ready', activity } }).telemetry.activity).toBe(activity)
    expect(() => createImportedVideo(metadata(), { importOrder: 0, preview, telemetry: { status: 'absent', activity } })).toThrow(
      'Only ready video telemetry carries an activity',
    )
    expect(() => createImportedVideo(metadata(), { importOrder: 0, preview, telemetry: { status: 'done', activity: null } })).toThrow(
      'Invalid video telemetry status',
    )
  })
})

describe('placement', () => {
  beforeEach(resetStore)

  test('without an external activity clips sit at detected real time relative to the earliest clip', () => {
    const late = createTestVideo({ id: 'late', creationTime: '2026-07-18T08:05:00Z', timeSource: 'ffprobe', importOrder: 0 })
    const early = createTestVideo({ id: 'early', creationTime: '2026-07-18T08:00:00Z', timeSource: 'ffprobe', importOrder: 1 })

    useStore.getState().setImportedVideos([late, early])

    expect(offsets()).toEqual({ late: 300, early: 0 })
    expect(warnings()).toEqual({ late: null, early: null })
    expect(useStore.getState().manualVideoSync.videoId).toBe('late')
  })

  test('with an external activity each clip syncs independently and clips outside it keep their signed offset', () => {
    useStore.getState().activateActivityFile(activityFile)
    const inside = createTestVideo({ id: 'inside', creationTime: '2026-07-18T08:00:30Z', timeSource: 'gps', durationSeconds: 20 })
    const before = createTestVideo({ id: 'before', creationTime: '2026-07-18T07:58:00Z', timeSource: 'gps', durationSeconds: 20 })

    useStore.getState().setImportedVideos([inside, before])

    expect(offsets()).toEqual({ inside: 30, before: -120 })
    expect(warnings()).toEqual({ inside: null, before: 'Video is outside the activity' })
  })

  test('activity import resyncs every clip and clearing it restores detected-time placement', () => {
    const video = createTestVideo({ creationTime: '2026-07-18T08:00:30Z', timeSource: 'gps', durationSeconds: 20 })
    useStore.getState().setImportedVideos([video])
    expect(offsets()).toEqual({ [video.id]: 0 })

    useStore.getState().activateActivityFile(activityFile)
    expect(offsets()).toEqual({ [video.id]: 30 })
    expect(useStore.getState().parsedActivity).toBe(activityFile)

    useStore.getState().clearActivityFile()
    const state = useStore.getState()
    expect(offsets()).toEqual({ [video.id]: 0 })
    expect(state.parsedActivity).toBeNull()
    expect(state.activitySummary).toBeNull()
    expect(state.activitySource).toBeNull()
  })

  test('ambiguous camera clocks infer a timezone and an explicit selection recomputes every clip', () => {
    // Moscow (UTC+3): 10:51 clock text fits as local and as UTC.
    const wideActivity = {
      ...activityFile,
      sync_time: '2026-07-18T07:20:03.000Z',
      metadata: { ...activityFile.metadata, end_time: '2026-07-18T11:20:03.000Z', timezone: 'Europe/Moscow' },
    }
    useStore.getState().activateActivityFile(wideActivity)
    const video = createTestVideo({ creationTime: '2026-07-18T10:51:00.000000Z', timeSource: 'ffprobe' })

    useStore.getState().setImportedVideos([video])
    expect(offsets()).toEqual({ [video.id]: 1857 })
    expect(useStore.getState().videoSyncTimezoneMode).toBe('local')

    useStore.getState().setVideoSyncTimezoneMode('utc')
    expect(offsets()).toEqual({ [video.id]: 12657 })
    expect(useStore.getState().videoSyncTimezoneMode).toBe('utc')
  })

  test('filename timestamps apply to every clip that has one and reset restores detected times', () => {
    const named = createTestVideo({ id: 'named', path: '/videos/VID_20260718_080100.mp4', creationTime: '2026-07-18T08:00:00Z', timeSource: 'gps' })
    const plain = createTestVideo({ id: 'plain', path: '/videos/plain.mp4', creationTime: '2026-07-18T08:00:00Z', timeSource: 'gps' })
    useStore.getState().setImportedVideos([named, plain])

    useStore.getState().setVideoCreationTimesFromFilenames()
    let videos = useStore.getState().importedVideos
    expect(videos.map(({ creationTime, timeSource }) => [creationTime, timeSource])).toEqual([
      ['2026-07-18T08:01:00', 'filename'],
      ['2026-07-18T08:00:00Z', 'gps'],
    ])
    expect(useStore.getState().videoSyncTimezoneMode).toBeNull()

    useStore.getState().resetVideoCreationTimes()
    videos = useStore.getState().importedVideos
    expect(videos.map(({ timeSource }) => timeSource)).toEqual(['gps', 'gps'])
  })

  test('filename timestamps are rejected when no clip has one', () => {
    useStore.getState().setImportedVideos([createTestVideo({ path: '/videos/plain.mp4' })])
    expect(() => useStore.getState().setVideoCreationTimesFromFilenames()).toThrow('No video filename contains')
  })
})

describe('adding videos to the collection', () => {
  beforeEach(resetStore)

  function clip(id, options = {}) {
    return createTestVideo({ id, path: `/videos/${id}.mp4`, ...options })
  }

  test('the first import places clips like a full sync and selects the first clip for manual sync', () => {
    const late = clip('late', { creationTime: '2026-07-18T08:05:00Z', timeSource: 'ffprobe' })
    const early = clip('early', { creationTime: '2026-07-18T08:00:00Z', timeSource: 'ffprobe', importOrder: 1 })
    useStore.setState({ importedBackgroundImagePath: '/images/background.png' })

    useStore.getState().addImportedVideos([late, early])

    expect(offsets()).toEqual({ late: 300, early: 0 })
    expect(useStore.getState().manualVideoSync.videoId).toBe('late')
    expect(useStore.getState().importedBackgroundImagePath).toBeNull()
  })

  test('existing clips keep their edits and new clips follow the shared correction of the collection', () => {
    useStore.getState().activateActivityFile(activityFile)
    const first = clip('first', { creationTime: '2026-07-18T08:00:10Z', timeSource: 'gps', durationSeconds: 20 })
    const second = clip('second', { creationTime: '2026-07-18T08:00:40Z', timeSource: 'gps', durationSeconds: 20, importOrder: 1 })
    useStore.getState().addImportedVideos([first, second])
    useStore.getState().shiftVideoOffsets(2)
    useStore.getState().setVideoOffset('second', 50)
    useStore.getState().selectVideoSyncVideo('second')

    const added = clip('added', { creationTime: '2026-07-18T08:01:20Z', timeSource: 'gps', durationSeconds: 20, importOrder: 2 })
    useStore.getState().addImportedVideos([added])

    expect(offsets()).toEqual({ first: 12, second: 50, added: 82 })
    expect(useStore.getState().manualVideoSync.videoId).toBe('second')
    expect(useStore.getState().importedVideos.map((video) => video.id)).toEqual(['first', 'second', 'added'])
  })

  test('without an external activity new clips keep their real-time distance to the existing clips', () => {
    const existing = clip('existing', { creationTime: '2026-07-18T08:10:00Z', timeSource: 'ffprobe' })
    useStore.getState().addImportedVideos([existing])
    useStore.getState().setVideoOffset('existing', 40)

    const earlier = clip('earlier', { creationTime: '2026-07-18T08:05:00Z', timeSource: 'ffprobe', importOrder: 1 })
    const unknown = clip('unknown', { creationTime: null, importOrder: 2 })
    useStore.getState().addImportedVideos([earlier, unknown])

    expect(offsets()).toEqual({ existing: 40, earlier: -260, unknown: 0 })
    expect(warnings()).toEqual({ existing: null, earlier: null, unknown: 'Could not determine video creation time' })
  })

  test('rejects an empty addition and paths already in the collection', () => {
    useStore.getState().addImportedVideos([createTestVideo({ id: 'a', path: '/videos/a.mp4' })])
    expect(() => useStore.getState().addImportedVideos([])).toThrow('At least one video')
    expect(() => useStore.getState().addImportedVideos([createTestVideo({ id: 'b', path: '/videos/a.mp4', importOrder: 1 })])).toThrow(
      'already imported',
    )
    expect(() =>
      useStore
        .getState()
        .addImportedVideos([
          createTestVideo({ id: 'c', path: '/videos/c.mp4', importOrder: 1 }),
          createTestVideo({ id: 'd', path: '/videos/c.mp4', importOrder: 2 }),
        ]),
    ).toThrow('already imported')
    expect(useStore.getState().importedVideos.map((video) => video.id)).toEqual(['a'])
  })
})

describe('placement anchors', () => {
  beforeEach(resetStore)

  function clip(id, options = {}) {
    return createTestVideo({ id, path: `/videos/${id}.mp4`, durationSeconds: 20, ...options })
  }

  test('anchored clips sit next to their anchor and follow it through a full sync until their time is known', () => {
    const known = clip('known', { creationTime: '2026-07-18T08:00:30Z', timeSource: 'gps' })
    const first = clip('first', { importOrder: 1 })
    const second = clip('second', { importOrder: 2 })
    useStore.getState().addImportedVideos([known, first, second])
    useStore.getState().anchorVideo('second', 'first', 'after')
    useStore.getState().anchorVideo('first', 'known', 'before')
    expect(offsets()).toEqual({ known: 0, first: -20, second: 0 })
    expect(warnings()).toEqual({ known: null, first: null, second: null })

    useStore.getState().activateActivityFile(activityFile)
    expect(offsets()).toEqual({ known: 30, first: 10, second: 30 })
    expect(warnings()).toEqual({ known: null, first: null, second: null })

    useStore.setState((draft) => {
      draft.importedVideos.find((video) => video.id === 'first').creationTime = '2026-07-18T08:01:00Z'
      draft.importedVideos.find((video) => video.id === 'first').timeSource = 'gps'
    })
    useStore.getState().computeVideoSync()
    expect(offsets()).toEqual({ known: 30, first: 60, second: 80 })
  })

  test('rejects anchors to unknown clips, to the clip itself, through cycles, and unknown sides', () => {
    useStore.getState().addImportedVideos([clip('a'), clip('b', { importOrder: 1 })])
    useStore.getState().anchorVideo('b', 'a', 'after')

    expect(() => useStore.getState().anchorVideo('a', 'missing', 'after')).toThrow('Unknown imported video')
    expect(() => useStore.getState().anchorVideo('a', 'a', 'after')).toThrow('relative to itself')
    expect(() => useStore.getState().anchorVideo('a', 'b', 'before')).toThrow('relative to itself')
    expect(() => useStore.getState().anchorVideo('b', 'a', 'over')).toThrow('before or after')
  })
})

describe('transition defaults', () => {
  beforeEach(resetStore)

  test('stores chosen defaults, marks the dialog as shown, and rejects malformed values', () => {
    useStore.getState().setTransitionDefaults({ type: 'none', durationSeconds: 3, splitGapThresholdSeconds: 0 })
    expect(useStore.getState().transitionDefaults).toEqual({ type: 'none', durationSeconds: 3, splitGapThresholdSeconds: 0, dialogShown: true })

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

  test('a per-clip offset moves only that clip and clears only its warning', () => {
    seed()
    useStore.getState().setVideoOffset('first', 15, { compensatePlayhead: true })
    expect(offsets()).toEqual({ first: 15, second: 100 })
    expect(warnings()).toEqual({ first: null, second: 'stale' })
    expect(useStore.getState().selectedSecond).toBe(25)
  })

  test('a shared shift moves every clip by the same amount and keeps their individual placement', () => {
    seed()
    useStore.getState().shiftVideoOffsets(-12.5)
    expect(offsets()).toEqual({ first: -2.5, second: 87.5 })
    expect(warnings()).toEqual({ first: null, second: null })
    expect(useStore.getState().selectedSecond).toBe(20)
  })

  test('rejects non-finite edits and unknown clips', () => {
    seed()
    expect(() => useStore.getState().setVideoOffset('first', Number.NaN)).toThrow('finite')
    expect(() => useStore.getState().setVideoOffset('missing', 1)).toThrow('Unknown imported video')
    expect(() => useStore.getState().shiftVideoOffsets(Infinity)).toThrow('finite')
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
    useStore.getState().setImportedVideos([video])
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
    useStore.getState().setImportedVideos([video])
    const coursePoints = [
      [42.0, 23.0],
      [42.0, 23.2],
    ]

    useStore.getState().setVideoTelemetry(video.id, { status: 'ready', activity: telemetry({ coursePoints }) })

    expect(useStore.getState().config.plots[0].width).not.toBe(10)
  })

  test('clearing videos and switching to a background image empty the collection', () => {
    useStore.getState().setImportedVideos([createTestVideo()])
    useStore.getState().setImportedBackgroundImage('/images/background.png')
    expect(useStore.getState()).toMatchObject({ importedVideos: [], videoEdits: {}, importedBackgroundImagePath: '/images/background.png' })
    expect(useStore.getState().manualVideoSync.videoId).toBeNull()

    useStore.getState().setImportedVideos([createTestVideo()])
    useStore.getState().clearImportedVideos()
    expect(useStore.getState()).toMatchObject({ importedVideos: [], videoEdits: {}, importedBackgroundImagePath: null, videoDragPreview: null })
  })
})

describe('undo history across collection changes', () => {
  beforeEach(resetStore)

  test('undo restores offsets of remaining clips and never resurrects removed clips', () => {
    const kept = createTestVideo({ id: 'kept' })
    const removed = createTestVideo({ id: 'removed', importOrder: 1 })
    changeVideoCollection(useStore, () => useStore.getState().setImportedVideos([kept, removed]))
    useStore.getState().setVideoOffset('kept', 5)
    useStore.getState().setVideoOffset('removed', 50)

    const added = createTestVideo({ id: 'added', importOrder: 2 })
    changeVideoCollection(useStore, () => {
      const state = useStore.getState()
      state.setImportedVideos([kept, added])
      state.setVideoOffset('kept', 7)
    })

    undoHistory(useStore)
    expect(useStore.getState().importedVideos.map(({ id }) => id)).toEqual(['kept', 'added'])
    expect(offsets()).toEqual({ kept: 5, added: 0 })

    undoHistory(useStore)
    expect(offsets()).toEqual({ kept: 0, added: 0 })
  })
})

describe('activity context', () => {
  test('an external activity drives every timeline second from zero', () => {
    const context = resolveActivityContext({ parsedActivity: activityFile, activitySummary: { durationSeconds: 120 }, clip: null })
    expect(context).toEqual({ activity: activityFile, summary: { durationSeconds: 120 }, originSecond: 0, videoId: null })
  })

  test('without an external activity the clip supplies its own telemetry from its start', () => {
    const activity = telemetry()
    const clip = { ...createTestVideo({ telemetry: { status: 'ready', activity } }), offsetSeconds: 300 }
    const context = resolveActivityContext({ parsedActivity: null, activitySummary: null, clip })
    expect(context).toMatchObject({ activity, originSecond: 300, videoId: clip.id })
    expect(context.summary.availableMetrics).toEqual([{ attribute: 'speed', source: 'direct' }])

    const pending = { ...createTestVideo(), offsetSeconds: 0 }
    expect(resolveActivityContext({ parsedActivity: null, activitySummary: null, clip: pending })).toMatchObject({ activity: null, summary: null })
    expect(resolveActivityContext({ parsedActivity: null, activitySummary: null, clip: null })).toBeNull()
  })

  test('available metrics come from the external activity or from every clip telemetry', () => {
    const videos = [
      createTestVideo({ id: 'a', telemetry: { status: 'ready', activity: telemetry({ attributes: ['speed'] }) } }),
      createTestVideo({ id: 'b', telemetry: { status: 'ready', activity: telemetry({ attributes: ['speed', 'cadence'] }) } }),
      createTestVideo({ id: 'c' }),
    ]
    expect(resolveAvailableMetrics(null, videos).map(({ attribute }) => attribute)).toEqual(['speed', 'cadence'])
    const externalSummary = { availableMetrics: [{ attribute: 'heartrate', source: 'direct' }] }
    expect(resolveAvailableMetrics(externalSummary, videos)).toBe(externalSummary.availableMetrics)
  })
})
