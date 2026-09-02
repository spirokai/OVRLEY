/**
 * Behavior tests for the video sync controls hook.
 */

import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test } from 'vitest'
import { useVideoSyncControls } from '@/features/toolbar'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG } from '@/store/store-utils'

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
      })
    })

    const { result } = renderHook(() => useVideoSyncControls())

    act(() => {
      result.current.submitOffsetInput('1:30')
    })

    expect(useStore.getState().videoSyncOffsetSeconds).toBe(90)
    expect(result.current.offsetInput).toBe('90')
  })

  test('parses colon-delimited time (H:MM:SS)', () => {
    act(() => {
      useStore.setState({
        aspectRatio: '16:9',
        updateRate: 1,
        config: { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } },
      })
    })

    const { result } = renderHook(() => useVideoSyncControls())

    act(() => {
      result.current.submitOffsetInput('1:00:30')
    })

    expect(useStore.getState().videoSyncOffsetSeconds).toBe(3630)
  })

  test('parses decimal seconds and rounds to 1 decimal', () => {
    act(() => {
      useStore.setState({
        aspectRatio: '16:9',
        updateRate: 1,
        config: { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } },
      })
    })

    const { result } = renderHook(() => useVideoSyncControls())

    act(() => {
      result.current.submitOffsetInput('5.55')
    })

    expect(useStore.getState().videoSyncOffsetSeconds).toBe(5.6)
  })
})
