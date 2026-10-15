import { describe, expect, test } from 'vitest'
import { resolvePreviewTarget } from '@/lib/video-program/playback-target'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { DEFAULT_TRANSITION_SETTINGS } from '@/lib/video-program/transition-catalog'
import { createTestVideo, videoStoreState } from '../../helpers/videoFixtures'

const first = createTestVideo({ id: 'first', durationSeconds: 10 })
const second = createTestVideo({ id: 'second', durationSeconds: 10, importOrder: 1 })

function timeline(offsets, dragPreview = null) {
  const { importedVideos, videoEdits } = videoStoreState([first, second], offsets)
  return buildTimelineProgram({ videos: importedVideos, videoEdits, dragPreview, transitionDefaults: DEFAULT_TRANSITION_SETTINGS })
}

describe('buildTimelineProgram', () => {
  test('joins entities with their edits into clips covering the whole timeline', () => {
    const result = timeline({ first: 5, second: 30 })
    expect(result.clips.map(({ id, offsetSeconds }) => [id, offsetSeconds])).toEqual([
      ['first', 5],
      ['second', 30],
    ])
    expect(result.bounds).toEqual({ startSecond: 5, endSecond: 40 })
    expect(result.program.pieces).toHaveLength(2)
  })

  test('applies a drag preview to one clip or to every clip', () => {
    expect(timeline({ first: 5, second: 30 }, { videoId: 'second', deltaSeconds: -10 }).bounds).toEqual({ startSecond: 5, endSecond: 30 })
    expect(timeline({ first: 5, second: 30 }, { videoId: null, deltaSeconds: -10 }).bounds).toEqual({ startSecond: -5, endSecond: 30 })
  })
})

describe('resolvePreviewTarget', () => {
  const { program } = timeline({ first: 5, second: 30 })

  test.each([
    [0, { videoId: 'first', sourceSecond: 0, isInside: false }],
    [7.5, { videoId: 'first', sourceSecond: 2.5, isInside: true }],
    [20, { videoId: 'second', sourceSecond: 0, isInside: false }],
    [31, { videoId: 'second', sourceSecond: 1, isInside: true }],
    [50, { videoId: 'second', sourceSecond: 299 / 30, isInside: false }],
  ])('at %s', (second, expected) => {
    expect(resolvePreviewTarget(program, second)).toEqual(expected)
  })

  test('is null without clips', () => {
    expect(resolvePreviewTarget({ pieces: [], joins: [] }, 0)).toBeNull()
  })
})
