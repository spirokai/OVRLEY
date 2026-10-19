/**
 * Behavior tests for the video sync controls hook.
 */

import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test } from 'vitest'
import { useVideoSyncControls } from '@/features/toolbar'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG } from '@/store/store-utils'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

describe('useVideoSyncControls', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
  })

  test('parses and rounds a simple time string on blur', () => {
    act(() => {
      useStore.setState({
        aspectRatio: '16:9',
        updateRate: 1,
        config: { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } },
        ...videoStoreState([createTestVideo()]),
      })
    })

    const { result } = renderHook(() => useVideoSyncControls())

    act(() => {
      result.current.submitOffsetInput('1:30')
    })

    expect(useStore.getState().videoEdits['video-1'].offsetSeconds).toBe(90)
    expect(result.current.offsetInput).toBe('90')
  })

  test('parses colon-delimited time (H:MM:SS)', () => {
    act(() => {
      useStore.setState({
        aspectRatio: '16:9',
        updateRate: 1,
        config: { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } },
        ...videoStoreState([createTestVideo()]),
      })
    })

    const { result } = renderHook(() => useVideoSyncControls())

    act(() => {
      result.current.submitOffsetInput('1:00:30')
    })

    expect(useStore.getState().videoEdits['video-1'].offsetSeconds).toBe(3630)
  })

  test('parses decimal seconds and rounds to 1 decimal', () => {
    act(() => {
      useStore.setState({
        aspectRatio: '16:9',
        updateRate: 1,
        config: { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } },
        ...videoStoreState([createTestVideo()]),
      })
    })

    const { result } = renderHook(() => useVideoSyncControls())

    act(() => {
      result.current.submitOffsetInput('5.55')
    })

    expect(useStore.getState().videoEdits['video-1'].offsetSeconds).toBe(5.6)
  })

  test('shows the earliest clip offset and shifts every clip by the same amount, keeping their relative placement', () => {
    act(() => {
      useStore.setState(
        videoStoreState(
          [
            createTestVideo({ id: 'video-1', path: '/videos/late.mp4', importOrder: 0 }),
            createTestVideo({ id: 'video-2', path: '/videos/early.mp4', importOrder: 1 }),
          ],
          { 'video-1': 15, 'video-2': 10 },
        ),
      )
    })

    const { result } = renderHook(() => useVideoSyncControls())
    expect(result.current.offsetInput).toBe('10')

    act(() => {
      result.current.submitOffsetInput('20')
    })

    expect(useStore.getState().videoEdits['video-1'].offsetSeconds).toBe(25)
    expect(useStore.getState().videoEdits['video-2'].offsetSeconds).toBe(20)
    expect(result.current.offsetInput).toBe('20')

    act(() => {
      result.current.incrementOffset(-1.5)
    })

    expect(useStore.getState().videoEdits['video-1'].offsetSeconds).toBe(23.5)
    expect(useStore.getState().videoEdits['video-2'].offsetSeconds).toBe(18.5)
    expect(result.current.offsetInput).toBe('18.5')
  })
})
