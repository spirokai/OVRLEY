import { describe, expect, test } from 'vitest'
import { formatClockDuration } from '@/lib/time-format'
import {
  createPlaybackAnchor,
  getTimelineMinimum,
  getTimelinePlaybackSecond,
  getTotalPlaybackDuration,
  resolvePlaybackSource,
  snapTimelineSecondToFrame,
} from '@/features/player/utils/playerTiming'

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
        importedVideoDuration: 6,
        importedVideoPath: 'C:\\clips\\ride.mp4',
        videoSyncOffsetSeconds: 10,
      }),
    ).toBe(16)
  })

  test('does not let fallback duration extend an imported video timeline', () => {
    expect(
      getTotalPlaybackDuration({
        activityDurationSeconds: 0,
        fallbackDurationSeconds: 73,
        importedVideoDuration: 12.509,
        importedVideoPath: 'C:\\clips\\GoPro-telemetry.MP4',
        videoSyncOffsetSeconds: 0,
      }),
    ).toBe(12.509)
  })

  test('keeps video-clock playback scoped to the imported video window', () => {
    const baseOptions = {
      importedVideoDuration: 4,
      shouldUseVideoPlayback: true,
      videoSyncOffsetSeconds: 5,
    }

    expect(resolvePlaybackSource({ ...baseOptions, playheadSecond: 4.99 })).toBe('timeline')
    expect(resolvePlaybackSource({ ...baseOptions, playheadSecond: 5 })).toBe('video')
    expect(resolvePlaybackSource({ ...baseOptions, playheadSecond: 8.99 })).toBe('video')
    expect(resolvePlaybackSource({ ...baseOptions, playheadSecond: 9 })).toBe('timeline')

    expect(resolvePlaybackSource({ ...baseOptions, videoSyncOffsetSeconds: -5, importedVideoDuration: 4, playheadSecond: -4.99 })).toBe('video')
    expect(resolvePlaybackSource({ ...baseOptions, videoSyncOffsetSeconds: -5, importedVideoDuration: 4, playheadSecond: -1 })).toBe('timeline')
  })

  test('uses the negative video start as the timeline minimum', () => {
    expect(getTimelineMinimum({ hasVideo: true, videoSyncOffsetSeconds: -5 })).toBe(-5)
    expect(
      getTotalPlaybackDuration({
        activityDurationSeconds: 25,
        fallbackDurationSeconds: 0,
        importedVideoDuration: 30,
        importedVideoPath: 'C:\\clips\\ride.mp4',
        videoSyncOffsetSeconds: -5,
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
