import { describe, expect, test } from 'vite-plus/test'
import { resolvePlaybackStart, resolvePreviewTarget } from '@/lib/video-program/playback-target'
import { buildProgram } from '@/lib/video-program/program'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { DEFAULT_TRANSITION_SETTINGS } from '@/lib/video-program/transition-catalog'
import { createTestVideo, cutEdits, videoStoreState } from '../../helpers/videoFixtures'

const first = createTestVideo({ id: 'first', durationSeconds: 10 })
const second = createTestVideo({ id: 'second', durationSeconds: 10, importOrder: 1 })

function timeline(offsets, dragPreview = null) {
  const { importedVideos, videoEdits } = videoStoreState([first, second], offsets)
  return buildTimelineProgram({ videos: importedVideos, videoEdits, dragPreview, transitionDefaults: DEFAULT_TRANSITION_SETTINGS })
}

describe('resolvePreviewTarget', () => {
  const { program } = timeline({ first: 5, second: 30 })

  const [firstPiece, secondPiece] = program.pieces

  test.each([
    [0, { videoId: 'first', piece: firstPiece, sourceSecond: 0, isInside: false, isExcluded: false }],
    [7.5, { videoId: 'first', piece: firstPiece, sourceSecond: 2.5, isInside: true, isExcluded: false }],
    [20, { videoId: 'second', piece: secondPiece, sourceSecond: 0, isInside: false, isExcluded: false }],
    [31, { videoId: 'second', piece: secondPiece, sourceSecond: 1, isInside: true, isExcluded: false }],
    [50, { videoId: 'second', piece: secondPiece, sourceSecond: 299 / 30, isInside: false, isExcluded: false }],
  ])('at %s', (second, expected) => {
    expect(resolvePreviewTarget(program, second)).toEqual(expected)
  })

  function programOf(edits) {
    const { importedVideos, videoEdits } = videoStoreState([first], { first: 5 })
    Object.assign(videoEdits.first, edits)
    return buildProgram({
      videos: importedVideos.map((video) => ({ ...video, ...videoEdits[video.id] })),
      exportRange: { type: 'all', from: 0, to: 0 },
      transitionDefaults: DEFAULT_TRANSITION_SETTINGS,
    })
  }

  test('shows the excluded frame of a video inside its excluded segment and playback skips it', () => {
    const cut = programOf(
      cutEdits([
        { id: 'c1', frame: 60, excluded: true },
        { id: 'c2', frame: 120 },
      ]),
    )
    expect(resolvePreviewTarget(cut, 8)).toEqual({
      videoId: 'first',
      piece: cut.excludedSegments[0],
      sourceSecond: 3,
      isInside: false,
      isExcluded: true,
    })
    expect(resolvePlaybackStart(cut, 8)).toEqual({ source: 'video', second: 9 })
  })

  test('playback skips gaps to the next piece and plays the activity before and after the videos on the timeline clock', () => {
    expect(resolvePlaybackStart(program, 0)).toEqual({ source: 'timeline', second: 0 })
    expect(resolvePlaybackStart(program, 7.5)).toEqual({ source: 'video', second: 7.5 })
    expect(resolvePlaybackStart(program, 20)).toEqual({ source: 'video', second: 30 })
    expect(resolvePlaybackStart(program, 50)).toEqual({ source: 'timeline', second: 50 })
  })
})
