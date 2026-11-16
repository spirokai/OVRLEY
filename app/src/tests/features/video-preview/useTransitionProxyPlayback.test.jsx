import { act, renderHook } from '@testing-library/react'
import { describe, expect, test, vi } from 'vite-plus/test'
import { useTransitionProxyPlayback } from '@/features/video-preview/hooks/useTransitionProxyPlayback'

function createProxyStub(overrides = {}) {
  const listeners = {}
  return {
    currentTime: 0,
    duration: 2,
    paused: true,
    seeking: false,
    readyState: 0,
    play: vi.fn(() => Promise.resolve()),
    pause: vi.fn(),
    addEventListener: vi.fn((name, listener) => {
      listeners[name] = listener
    }),
    removeEventListener: vi.fn(),
    emit: (name) => listeners[name]?.(),
    ...overrides,
  }
}

const URL = 'http://127.0.0.1:4321/video/proxy'

function renderProxy(video, proxy) {
  return renderHook(({ value }) => useTransitionProxyPlayback({ current: video }, value), { initialProps: { value: proxy } })
}

describe('useTransitionProxyPlayback', () => {
  test('parks a proxy on its first frame ahead of its window and reports the frame once decoded', () => {
    const video = createProxyStub()
    const { result } = renderProxy(video, { proxyUrl: URL, isInWindow: false, elapsedSeconds: 0, isPlaying: true })
    expect(video.play).not.toHaveBeenCalled()
    expect(result.current).toBe(false)

    video.readyState = 2
    act(() => video.emit('loadeddata'))
    expect(result.current).toBe(true)
  })

  test('starts playing at its window without seeking a parked proxy', () => {
    const video = createProxyStub({ readyState: 4 })
    const { rerender } = renderProxy(video, { proxyUrl: URL, isInWindow: false, elapsedSeconds: 0, isPlaying: true })
    rerender({ value: { proxyUrl: URL, isInWindow: true, elapsedSeconds: 0.04, isPlaying: true } })
    expect(video.currentTime).toBe(0)
    expect(video.play).toHaveBeenCalledTimes(1)
  })

  test('does not seek again while an earlier seek is in progress', () => {
    const video = createProxyStub({ readyState: 4, paused: false, seeking: true, currentTime: 0.2 })
    renderProxy(video, { proxyUrl: URL, isInWindow: true, elapsedSeconds: 0.6, isPlaying: true })
    expect(video.currentTime).toBe(0.2)

    const settled = createProxyStub({ readyState: 4, paused: false, currentTime: 0.2 })
    renderProxy(settled, { proxyUrl: URL, isInWindow: true, elapsedSeconds: 0.6, isPlaying: true })
    expect(settled.currentTime).toBe(0.6)
  })
})
