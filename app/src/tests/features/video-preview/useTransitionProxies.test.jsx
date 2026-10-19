import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import * as backend from '@/api/backend'
import { useTransitionProxyQueue } from '@/features/video-preview/hooks/useTransitionProxies'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

vi.mock('@/api/backend', () => ({
  renderTransitionProxy: vi.fn(),
  releaseTransitionProxy: vi.fn().mockResolvedValue(null),
}))

const fade = { type: 'fade', durationSeconds: 2, splitGapThresholdSeconds: 2, dialogShown: true }

function deferred() {
  let resolve
  let reject
  const promise = new Promise((done, fail) => {
    resolve = done
    reject = fail
  })
  return { promise, resolve, reject }
}

describe('useTransitionProxyQueue', () => {
  beforeEach(() => {
    vi.mocked(backend.renderTransitionProxy).mockReset()
    vi.mocked(backend.releaseTransitionProxy).mockClear()
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({
      ...videoStoreState(
        [
          createTestVideo({ id: 'a', path: '/videos/a.mp4', durationSeconds: 30 }),
          createTestVideo({ id: 'b', path: '/videos/b.mp4', durationSeconds: 30, importOrder: 1 }),
          createTestVideo({ id: 'c', path: '/videos/c.mp4', durationSeconds: 30, importOrder: 2 }),
        ],
        { a: 0, b: 40, c: 80 },
      ),
      transitionDefaults: fade,
      selectedSecond: 75,
    })
  })

  test('renders one proxy at a time, nearest the playhead first, and releases proxies no join needs', async () => {
    const first = deferred()
    const second = deferred()
    vi.mocked(backend.renderTransitionProxy).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
    renderHook(() => useTransitionProxyQueue())

    // The window into c (68-70 s) is nearer the playhead than the one into b.
    expect(backend.renderTransitionProxy).toHaveBeenCalledOnce()
    expect(vi.mocked(backend.renderTransitionProxy).mock.calls[0][0]).toMatchObject({ incoming: { path: '/videos/c.mp4' } })
    await act(async () => first.resolve({ importId: 'proxy-c', previewUrl: 'http://127.0.0.1:4321/video/proxy-c' }))
    expect(Object.values(useStore.getState().transitionProxies)).toContainEqual({
      status: 'ready',
      importId: 'proxy-c',
      url: 'http://127.0.0.1:4321/video/proxy-c',
    })
    await waitFor(() => expect(backend.renderTransitionProxy).toHaveBeenCalledTimes(2))
    expect(vi.mocked(backend.renderTransitionProxy).mock.calls[1][0]).toMatchObject({ incoming: { path: '/videos/b.mp4' } })
    await act(async () => second.reject(new Error('ffmpeg failed')))
    expect(Object.values(useStore.getState().transitionProxies).map((proxy) => proxy.status)).toEqual(['ready', 'failed'])

    // A hard cut into c no longer needs its proxy; the failed one is not retried.
    act(() => useStore.getState().setJoinTransition({ kind: 'clip', ownerId: 'c' }, { type: 'none', durationSeconds: 0 }))
    await waitFor(() => expect(backend.releaseTransitionProxy).toHaveBeenCalledExactlyOnceWith('proxy-c'))
    expect(Object.values(useStore.getState().transitionProxies).map((proxy) => proxy.status)).toEqual(['failed'])
    expect(backend.renderTransitionProxy).toHaveBeenCalledTimes(2)
  })

  test('releases a proxy that finishes after the proxies were forgotten', async () => {
    const pending = deferred()
    vi.mocked(backend.renderTransitionProxy).mockReturnValue(pending.promise)
    renderHook(() => useTransitionProxyQueue())
    act(() => useStore.getState().clearTransitionProxies())
    await act(async () => pending.resolve({ importId: 'late', previewUrl: 'http://127.0.0.1:4321/video/late' }))
    expect(backend.releaseTransitionProxy).toHaveBeenCalledWith('late')
  })
})
