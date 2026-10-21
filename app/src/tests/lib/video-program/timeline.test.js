import { describe, expect, test } from 'vitest'
import { resolvePlaybackStart, resolvePreviewTarget } from '@/lib/video-program/playback-target'
import { buildProgram } from '@/lib/video-program/program'
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

  const [firstPiece, secondPiece] = program.pieces

  test.each([
    [0, { videoId: 'first', piece: firstPiece, sourceSecond: 0, isInside: false, isDeleted: false }],
    [7.5, { videoId: 'first', piece: firstPiece, sourceSecond: 2.5, isInside: true, isDeleted: false }],
    [20, { videoId: 'second', piece: secondPiece, sourceSecond: 0, isInside: false, isDeleted: false }],
    [31, { videoId: 'second', piece: secondPiece, sourceSecond: 1, isInside: true, isDeleted: false }],
    [50, { videoId: 'second', piece: secondPiece, sourceSecond: 299 / 30, isInside: false, isDeleted: false }],
  ])('at %s', (second, expected) => {
    expect(resolvePreviewTarget(program, second)).toEqual(expected)
  })

  test('shows the deleted frame of a video inside its deleted part', () => {
    const { importedVideos, videoEdits } = videoStoreState([first], { first: 5 })
    videoEdits.first.deletedParts = [{ id: 'cut', fromSecond: 2, toSecond: 4, transition: null }]
    const cut = buildProgram({
      videos: importedVideos.map((video) => ({ ...video, ...videoEdits[video.id] })),
      exportRange: { type: 'all', from: 0, to: 0 },
      transitionDefaults: DEFAULT_TRANSITION_SETTINGS,
    })
    expect(resolvePreviewTarget(cut, 8)).toEqual({ videoId: 'first', piece: cut.pieces[0], sourceSecond: 3, isInside: false, isDeleted: true })
    expect(resolvePlaybackStart(cut, 8)).toEqual({ source: 'video', second: 9 })
  })

  test('is null without clips', () => {
    expect(resolvePreviewTarget({ pieces: [], joins: [] }, 0)).toBeNull()
  })

  test('playback skips gaps to the next piece and plays the activity before and after the videos on the timeline clock', () => {
    expect(resolvePlaybackStart(program, 0)).toEqual({ source: 'timeline', second: 0 })
    expect(resolvePlaybackStart(program, 7.5)).toEqual({ source: 'video', second: 7.5 })
    expect(resolvePlaybackStart(program, 20)).toEqual({ source: 'video', second: 30 })
    expect(resolvePlaybackStart(program, 50)).toEqual({ source: 'timeline', second: 50 })
  })
})
