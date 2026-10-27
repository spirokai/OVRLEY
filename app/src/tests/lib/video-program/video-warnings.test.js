import { describe, expect, test } from 'vite-plus/test'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { DEFAULT_TRANSITION_SETTINGS } from '@/lib/video-program/transition-catalog'
import { deriveVideoWarnings } from '@/lib/video-program/video-warnings'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

function warningsFor(videos, offsets) {
  const { importedVideos, videoEdits } = videoStoreState(videos, offsets)
  const { clips, program } = buildTimelineProgram({
    videos: importedVideos,
    videoEdits,
    dragPreview: null,
    transitionDefaults: DEFAULT_TRANSITION_SETTINGS,
  })
  return Object.fromEntries(deriveVideoWarnings(clips, program))
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

  test('gives every clip an entry, empty when nothing is wrong', () => {
    expect(warningsFor([createTestVideo({ id: 'a', path: '/videos/a.mp4' })], { a: 0 })).toEqual({ a: [] })
  })
})
