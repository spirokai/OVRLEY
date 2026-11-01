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
      a: [{ kind: 'overlap', message: 'Overlaps b.mp4' }],
      b: [
        { kind: 'incompatible', message: 'Different resolution or frame rate than the other videos' },
        { kind: 'overlap', message: 'Overlaps a.mp4' },
      ],
      outside: [{ kind: 'sync', message: 'Video is outside the activity' }],
    })
  })

  test('warns when a clip without a sync warning starts more than an hour before or after the activity', () => {
    const far = { kind: 'far', message: 'Starts more than an hour before or after the activity' }
    const clip = (id, importOrder) => createTestVideo({ id, path: `/videos/${id}.mp4`, durationSeconds: 0.5, importOrder })
    const synced = { ...clip('synced', 4), syncWarning: 'Video is outside the activity' }
    const offsets = { early: -3601, edgeBefore: -3600, edgeAfter: 1000 + 3600, late: 1000 + 3601, synced: -5000 }
    const videos = [clip('early', 0), clip('edgeBefore', 1), clip('edgeAfter', 2), clip('late', 3), synced]

    expect(warningsFor(videos, offsets, 1000)).toEqual({
      early: [far],
      edgeBefore: [],
      edgeAfter: [],
      late: [far],
      synced: [{ kind: 'sync', message: 'Video is outside the activity' }],
    })
    // Without an activity there is nothing to be far from.
    expect(warningsFor(videos, offsets).early).toEqual([])
  })

  test('gives every clip an entry, empty when nothing is wrong', () => {
    expect(warningsFor([createTestVideo({ id: 'a', path: '/videos/a.mp4' })], { a: 0 })).toEqual({ a: [] })
  })
})
