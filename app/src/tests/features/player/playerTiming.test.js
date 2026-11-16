import { describe, expect, test } from 'vite-plus/test'
import { formatClockDuration } from '@/lib/time-format'
import {
  createPlaybackAnchor,
  getTimelineMinimum,
  getTimelinePlaybackSecond,
  getTotalPlaybackDuration,
  resolvePlaybackSource,
  snapTimelineSecondToFrame,
} from '@/features/player/utils/playerTiming'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { DEFAULT_TRANSITION_SETTINGS } from '@/lib/video-program/transition-catalog'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

function timelineProgram(videos, offsets) {
  const { importedVideos, videoEdits } = videoStoreState(videos, offsets)
  return buildTimelineProgram({ videos: importedVideos, videoEdits, dragPreview: null, transitionDefaults: DEFAULT_TRANSITION_SETTINGS }).program
}

describe('playerTiming utilities', () => {
  test('formats timeline seconds as mm:ss or h:mm:ss labels', () => {
    expect(formatClockDuration(65)).toBe('01:05')
    expect(formatClockDuration(-65)).toBe('-01:05')
    expect(formatClockDuration(3661)).toBe('1:01:01')
    expect(snapTimelineSecondToFrame(12.38, 30, 5.25)).toBeCloseTo(12.3833333333)
  })

  test('extends total playback duration to include the imported video end', () => {
    expect(
      getTotalPlaybackDuration({
        activityDurationSeconds: 12,
        fallbackDurationSeconds: 9,
        videoBounds: { startSecond: 10, endSecond: 16 },
      }),
    ).toBe(16)
  })

  test('does not let fallback duration extend an imported video timeline', () => {
    expect(
      getTotalPlaybackDuration({
        activityDurationSeconds: 0,
        fallbackDurationSeconds: 73,
        videoBounds: { startSecond: 0, endSecond: 12.509 },
      }),
    ).toBe(12.509)
  })

  test('keeps video-clock playback scoped to the imported video window', () => {
    const shifted = timelineProgram([createTestVideo({ durationSeconds: 4 })], { 'video-1': 5 })
    const negative = timelineProgram([createTestVideo({ durationSeconds: 4 })], { 'video-1': -5 })

    const source = (program, playheadSecond, shouldUseVideoPlayback = true) =>
      resolvePlaybackSource({ shouldUseVideoPlayback, program, playheadSecond })
    expect(source(shifted, 4.99)).toEqual({ source: 'timeline', second: 4.99 })
    expect(source(shifted, 5)).toEqual({ source: 'video', second: 5 })
    expect(source(shifted, 8.99)).toEqual({ source: 'video', second: 8.99 })
    expect(source(shifted, 9)).toEqual({ source: 'timeline', second: 9 })
    expect(source(shifted, 6, false)).toEqual({ source: 'timeline', second: 6 })

    expect(source(negative, -4.99)).toEqual({ source: 'video', second: -4.99 })
    expect(source(negative, -1)).toEqual({ source: 'timeline', second: -1 })
  })

  test('uses the negative video start as the timeline minimum', () => {
    expect(getTimelineMinimum({ videoBounds: { startSecond: -5, endSecond: 25 } })).toBe(-5)
    expect(getTimelineMinimum({ videoBounds: { startSecond: 3, endSecond: 25 } })).toBe(0)
    expect(getTimelineMinimum({ videoBounds: null })).toBe(0)
    expect(
      getTotalPlaybackDuration({
        activityDurationSeconds: 25,
        fallbackDurationSeconds: 0,
        videoBounds: { startSecond: -5, endSecond: 25 },
      }),
    ).toBe(25)
  })

  test('creates timeline anchors and resolves elapsed playback seconds', () => {
    const anchor = createPlaybackAnchor({
      nowMs: 1000,
      second: 2,
      source: 'timeline',
    })

    expect(anchor).toEqual({ startedAtMs: 1000, startedSecond: 2 })
    expect(getTimelinePlaybackSecond({ anchor, nowMs: 1750 })).toBe(2.75)
    expect(createPlaybackAnchor({ nowMs: 1000, second: 4, source: 'video' })).toEqual({ startedAtMs: 0, startedSecond: 4 })
  })
})
