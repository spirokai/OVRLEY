/**
 * Behavior tests for the video sync controls hook.
 */

import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test } from 'vitest'
import { useVideoSyncControls } from '@/features/toolbar'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

describe('useVideoSyncControls', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
  })

  test('shows the earliest clip offset on its frames and shifts every clip by the same amount, keeping their relative placement', () => {
    act(() => {
      useStore.setState(
        videoStoreState(
          [
            createTestVideo({ id: 'video-1', path: '/videos/late.mp4', importOrder: 0 }),
            createTestVideo({ id: 'video-2', path: '/videos/early.mp4', importOrder: 1, fps: { num: 25, den: 1 } }),
          ],
          { 'video-1': 15, 'video-2': 10 },
        ),
      )
    })

    const { result } = renderHook(() => useVideoSyncControls())
    expect(result.current).toMatchObject({ sharedOffsetSeconds: 10, sharedOffsetFps: { num: 25, den: 1 } })

    act(() => {
      expect(result.current.commitSharedOffset(18.5)).toBeNull()
    })

    expect(useStore.getState().videoEdits['video-1'].offsetSeconds).toBe(23.5)
    expect(useStore.getState().videoEdits['video-2'].offsetSeconds).toBe(18.5)
    expect(result.current.sharedOffsetSeconds).toBe(18.5)
  })

  test('returns the reason a shift is rejected to the field', () => {
    const { result } = renderHook(() => useVideoSyncControls())
    expect(result.current.commitSharedOffset(5)).toBe('A video must be loaded before shifting video offsets')
  })
})
