import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import useStore from '@/store/useStore'

describe('setSelectedSecond', () => {
  beforeEach(() => {
    useStore.setState({
      selectedSecond: 0,
      startSecond: 0,
      endSecond: 73,
    })
  })

  test('sets the selectedSecond state', () => {
    useStore.getState().setSelectedSecond(42.5)
    expect(useStore.getState().selectedSecond).toBe(42.5)
  })

  test('handles finite numbers', () => {
    useStore.getState().setSelectedSecond(10)
    expect(useStore.getState().selectedSecond).toBe(10)
  })

  test('handles non-finite input by clamping to 0', () => {
    useStore.getState().setSelectedSecond(NaN)
    expect(useStore.getState().selectedSecond).toBe(0)
  })

  test('guards duplicate playhead and scrub updates without skipping state transitions', () => {
    const listener = vi.fn()
    const unsubscribe = useStore.subscribe(listener)

    useStore.getState().setSelectedSecond(0)
    expect(listener).not.toHaveBeenCalled()

    useStore.getState().beginPreviewScrub(0)
    expect(useStore.getState().previewPlaybackState).toBe('scrubbing')
    expect(listener).toHaveBeenCalledTimes(1)

    useStore.getState().updatePreviewScrub(0)
    expect(listener).toHaveBeenCalledTimes(1)

    useStore.getState().commitPreviewScrub(0)
    expect(useStore.getState().previewPlaybackState).toBe('paused')
    expect(listener).toHaveBeenCalledTimes(2)

    useStore.getState().commitPreviewScrub(0)
    expect(listener).toHaveBeenCalledTimes(2)

    unsubscribe()
  })
})
