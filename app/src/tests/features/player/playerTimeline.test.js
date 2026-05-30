/**
 * Regression tests for player timeline helpers.
 *
 * These helpers define which clock owns preview playback and how far the
 * playable range extends when imported video sits beyond activity timing.
 */

import { describe, expect, test } from 'vitest'
import { getTotalPlaybackDuration, resolvePlaybackSource } from '@/features/player/utils/playerTimeline'

describe('playerTimeline helpers', () => {
  test('extends total playback duration to include the imported video end', () => {
    expect(
      getTotalPlaybackDuration({
        activityDurationSeconds: 12,
        dummyDurationSeconds: 9,
        importedVideoDuration: 6,
        importedVideoPath: 'C:\\clips\\ride.mp4',
        videoSyncOffsetSeconds: 10,
      }),
    ).toBe(16)
  })

  test('keeps video-clock playback scoped to the imported video window', () => {
    const baseOptions = {
      shouldUseVideoPlayback: true,
      videoSyncOffsetSeconds: 5,
      importedVideoDuration: 4,
    }

    expect(resolvePlaybackSource({ ...baseOptions, playheadSecond: 4.99 })).toBe('timeline')
    expect(resolvePlaybackSource({ ...baseOptions, playheadSecond: 5 })).toBe('video')
    expect(resolvePlaybackSource({ ...baseOptions, playheadSecond: 8.99 })).toBe('video')
    expect(resolvePlaybackSource({ ...baseOptions, playheadSecond: 9 })).toBe('timeline')
  })
})
