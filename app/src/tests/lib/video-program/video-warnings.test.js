import { describe, expect, test } from 'vite-plus/test'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { DEFAULT_TRANSITION_SETTINGS } from '@/lib/video-program/transition-catalog'
import { deriveVideoWarnings } from '@/lib/video-program/video-warnings'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

function warningsFor(videos, offsets, activityDurationSeconds = null) {
  const { importedVideos, videoEdits } = videoStoreState(videos, offsets)
  const { clips, program } = buildTimelineProgram({
    videos: importedVideos,
    videoEdits,
    dragPreview: null,
    transitionDefaults: DEFAULT_TRANSITION_SETTINGS,
  })
  return Object.fromEntries(deriveVideoWarnings(clips, program, activityDurationSeconds))
}

describe('deriveVideoWarnings', () => {
  test('lists incompatible formats, overlaps on both clips, and sync warnings', () => {
    const outside = {
      ...createTestVideo({ id: 'outside', path: '/videos/outside.mp4', importOrder: 2 }),
      syncWarning: 'Video is outside the activity',
    }
    const warnings = warningsFor(
      [
        createTestVideo({ id: 'a', path: '/videos/a.mp4', durationSeconds: 30 }),
        createTestVideo({ id: 'b', path: '/videos/b.mp4', durationSeconds: 30, fps: { num: 25, den: 1 }, importOrder: 1 }),
        outside,
      ],
      { a: 0, b: 20, outside: 200 },
    )

    expect(warnings).toEqual({
      a: [{ kind: 'overlap', params: { name: 'b.mp4' } }],
      b: [
        { kind: 'incompatible', params: {} },
        { kind: 'overlap', params: { name: 'a.mp4' } },
      ],
      outside: [{ kind: 'sync', params: { message: 'Video is outside the activity' } }],
    })
  })

  test('a clip overlapping two videos of one filename warns about each', () => {
    const warnings = warningsFor(
      [
        createTestVideo({ id: 'long', path: '/videos/long.mp4', durationSeconds: 60 }),
        createTestVideo({ id: 'day1', path: '/day1/ride.mp4', durationSeconds: 20, importOrder: 1 }),
        createTestVideo({ id: 'day2', path: '/day2/ride.mp4', durationSeconds: 20, importOrder: 2 }),
      ],
      { long: 0, day1: 10, day2: 40 },
    )
    expect(warnings.long).toEqual([
      { kind: 'overlap', params: { name: 'ride.mp4' } },
      { kind: 'overlap', params: { name: 'ride.mp4' } },
    ])
  })

  test('warns when a clip without a sync warning starts more than an hour before or after the activity', () => {
    const far = { kind: 'far', params: {} }
    const clip = (id, importOrder) => createTestVideo({ id, path: `/videos/${id}.mp4`, durationSeconds: 0.5, importOrder })
    const synced = { ...clip('synced', 4), syncWarning: 'Video is outside the activity' }
    const offsets = { early: -3601, edgeBefore: -3600, edgeAfter: 1000 + 3600, late: 1000 + 3601, synced: -5000 }
    const videos = [clip('early', 0), clip('edgeBefore', 1), clip('edgeAfter', 2), clip('late', 3), synced]

    expect(warningsFor(videos, offsets, 1000)).toEqual({
      early: [far],
      edgeBefore: [],
      edgeAfter: [],
      late: [far],
      synced: [{ kind: 'sync', params: { message: 'Video is outside the activity' } }],
    })
    // Without an activity there is nothing to be far from.
    expect(warningsFor(videos, offsets).early).toEqual([])
  })

  test('a video excluded from export does not count as an incompatible format', () => {
    const { importedVideos, videoEdits } = videoStoreState([
      createTestVideo({ id: 'a', path: '/videos/a.mp4', durationSeconds: 30 }),
      createTestVideo({ id: 'b', path: '/videos/b.mp4', durationSeconds: 30, importOrder: 1 }),
      createTestVideo({ id: 'other', path: '/videos/other.mp4', durationSeconds: 30, fps: { num: 25, den: 1 }, importOrder: 2 }),
    ])
    videoEdits.a.offsetSeconds = 0
    videoEdits.b.offsetSeconds = 100
    videoEdits.other.offsetSeconds = 200
    videoEdits.other.segments.start.excluded = true
    const { clips, program } = buildTimelineProgram({
      videos: importedVideos,
      videoEdits,
      dragPreview: null,
      transitionDefaults: DEFAULT_TRANSITION_SETTINGS,
    })
    expect(deriveVideoWarnings(clips, program, null).get('other')).toEqual([])
  })
})
