import { act, renderHook, waitFor } from '@testing-library/react'
import { StrictMode } from 'react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import * as backend from '@/api/backend'
import { useVideoFilmstripQueue } from '@/features/video-preview/hooks/useVideoFilmstrips'
import { createMissingVideo, createVideoMetadataSnapshot } from '@/lib/video-program/imported-video'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

vi.mock('@/api/backend', () => ({
  getCachedVideoFilmstrip: vi.fn(),
  getVideoFilmstrip: vi.fn(),
}))

vi.mock('@/features/video-preview/data/filmstrip', async (importOriginal) => ({
  ...(await importOriginal()),
  FILMSTRIP_WINDOWS_KEPT: 2,
}))

/** Every decode a test leaves pending, settled after it so the module's decode slots are free for the next test. */
let unsettled = []

function deferred() {
  let resolve
  const promise = new Promise((done) => {
    resolve = done
  })
  unsettled.push(resolve)
  return { promise, resolve }
}

const pending = () => deferred().promise

const strip = { spriteJpegBase64: btoa('jpeg'), startSeconds: 0, spacingSeconds: 10, sampleCount: 24 }
const zoom = (window) => ({ kind: 'zoom', level: 0, window })
const decodedPaths = () => vi.mocked(backend.getVideoFilmstrip).mock.calls.map(([request]) => `${request.path} ${request.window.kind}`)

describe('useVideoFilmstripQueue', () => {
  const originalCreateObjectURL = URL.createObjectURL
  const originalRevokeObjectURL = URL.revokeObjectURL
  let objectUrls = 0
  let hooks = []

  const render = (options) => {
    const hook = renderHook(() => useVideoFilmstripQueue(), options)
    hooks.push(hook)
    return hook
  }

  beforeEach(() => {
    vi.mocked(backend.getCachedVideoFilmstrip).mockReset().mockResolvedValue(null)
    vi.mocked(backend.getVideoFilmstrip).mockReset()
    objectUrls = 0
    URL.createObjectURL = vi.fn(() => `blob:sprite-${++objectUrls}`)
    URL.revokeObjectURL = vi.fn()
    useStore.setState(useStore.getInitialState(), true)
    const missing = createTestVideo({ id: 'm', path: '/videos/missing.mp4', durationSeconds: 30, importOrder: 3 })
    useStore.setState({
      ...videoStoreState([
        createTestVideo({ id: 'a', path: '/videos/a.mp4', durationSeconds: 7200, width: 3840, height: 2160 }),
        createTestVideo({ id: 'b', path: '/videos/b.mp4', durationSeconds: 30, importOrder: 1 }),
        createTestVideo({ id: 'c', path: '/videos/c.mp4', durationSeconds: 30, importOrder: 2 }),
        createMissingVideo(createVideoMetadataSnapshot(missing), { id: 'm', path: missing.path, importOrder: 3 }),
      ]),
      fetchAvailableCodecs: vi.fn().mockResolvedValue({ nvdec: true }),
    })
  })

  afterEach(async () => {
    // Free the module's decode slots: unmount, then settle every job still holding one.
    for (const hook of hooks) hook.unmount()
    hooks = []
    act(() => useStore.setState(useStore.getInitialState(), true))
    for (const resolve of unsettled) resolve(null)
    unsettled = []
    await act(async () => {})
    URL.createObjectURL = originalCreateObjectURL
    URL.revokeObjectURL = originalRevokeObjectURL
  })

  test('decodes the base strips of found videos two at a time in collection order, with the GPU decoder', async () => {
    const first = deferred()
    vi.mocked(backend.getVideoFilmstrip).mockReturnValueOnce(first.promise).mockReturnValue(pending())
    render()

    await waitFor(() => expect(backend.getVideoFilmstrip).toHaveBeenCalledTimes(2))
    expect(vi.mocked(backend.getCachedVideoFilmstrip).mock.calls[0][0]).toEqual({
      path: '/videos/a.mp4',
      durationSeconds: 7200,
      window: { kind: 'base' },
      frameWidth: 171,
      frameHeight: 96,
    })
    expect(vi.mocked(backend.getVideoFilmstrip).mock.calls[0][0]).toMatchObject({ path: '/videos/a.mp4', hwaccel: 'cuda' })
    expect(decodedPaths()).toEqual(['/videos/a.mp4 base', '/videos/b.mp4 base'])

    await act(async () => first.resolve(strip))
    expect(useStore.getState().videoFilmstrips['/videos/a.mp4'].base).toEqual({
      status: 'ready',
      spriteUrl: 'blob:sprite-1',
      startSeconds: 0,
      spacingSeconds: 10,
      sampleCount: 24,
    })
    // The finished slot goes to c; the missing video gets no filmstrip.
    await waitFor(() => expect(backend.getVideoFilmstrip).toHaveBeenCalledTimes(3))
    expect(decodedPaths()[2]).toBe('/videos/c.mp4 base')
    expect(Object.keys(useStore.getState().videoFilmstrips).sort()).toEqual(['/videos/a.mp4', '/videos/b.mp4', '/videos/c.mp4'])
  })

  test('a cached strip loads without waiting for a decode slot', async () => {
    vi.mocked(backend.getVideoFilmstrip).mockReturnValue(pending())
    vi.mocked(backend.getCachedVideoFilmstrip).mockImplementation(async (request) => (request.path === '/videos/c.mp4' ? strip : null))
    render()

    await waitFor(() => expect(useStore.getState().videoFilmstrips['/videos/c.mp4']?.base?.status).toBe('ready'))
    expect(decodedPaths()).toEqual(['/videos/a.mp4 base', '/videos/b.mp4 base'])
  })

  test('decodes the wanted zoom windows, after any base strip waiting with them', async () => {
    const [a, b] = [
      createTestVideo({ id: 'a', path: '/videos/a.mp4', durationSeconds: 7200 }),
      createTestVideo({ id: 'b', path: '/videos/b.mp4', durationSeconds: 30, importOrder: 1 }),
    ]
    useStore.setState({ ...videoStoreState([a]) })
    const base = deferred()
    const firstWindow = deferred()
    vi.mocked(backend.getVideoFilmstrip).mockReturnValueOnce(base.promise).mockReturnValueOnce(firstWindow.promise).mockResolvedValue(strip)
    act(() => useStore.getState().setWantedFilmstripWindows({ '/videos/a.mp4': [zoom(0), zoom(1), zoom(2)] }))
    render()
    await waitFor(() => expect(backend.getVideoFilmstrip).toHaveBeenCalledTimes(2))

    // Windows 1 and 2 wait; a video imported now jumps ahead of them.
    act(() => useStore.setState({ ...videoStoreState([a, b]) }))
    await waitFor(() => expect(backend.getCachedVideoFilmstrip).toHaveBeenCalledWith(expect.objectContaining({ path: '/videos/b.mp4' })))
    await act(async () => base.resolve(strip))
    await act(async () => firstWindow.resolve(strip))

    await waitFor(() => expect(useStore.getState().videoFilmstrips['/videos/a.mp4']['z0-2']?.status).toBe('ready'))
    expect(
      vi.mocked(backend.getVideoFilmstrip).mock.calls.map(([request]) => `${request.path} ${request.window.kind}${request.window.window ?? ''}`),
    ).toEqual(['/videos/a.mp4 base', '/videos/a.mp4 zoom0', '/videos/b.mp4 base', '/videos/a.mp4 zoom1', '/videos/a.mp4 zoom2'])
  })

  test('a window no longer wanted when its slot comes up is dropped unstarted', async () => {
    useStore.setState({ ...videoStoreState([createTestVideo({ id: 'a', path: '/videos/a.mp4', durationSeconds: 7200 })]) })
    const base = deferred()
    const first = deferred()
    vi.mocked(backend.getVideoFilmstrip).mockReturnValueOnce(base.promise).mockReturnValueOnce(first.promise).mockResolvedValue(strip)
    act(() => useStore.getState().setWantedFilmstripWindows({ '/videos/a.mp4': [zoom(0), zoom(1)] }))
    render()
    await waitFor(() => expect(backend.getVideoFilmstrip).toHaveBeenCalledTimes(2))

    // The view moves on while window 1 waits behind the base strip and window 0.
    act(() => useStore.getState().setWantedFilmstripWindows({}))
    await act(async () => base.resolve(strip))
    await act(async () => first.resolve(strip))

    expect(decodedPaths()).toEqual(['/videos/a.mp4 base', '/videos/a.mp4 zoom'])
    expect(Object.keys(useStore.getState().videoFilmstrips['/videos/a.mp4']).sort()).toEqual(['base', 'z0-0'])
  })

  test('past the kept number of windows, windows the timeline no longer shows are dropped', async () => {
    useStore.setState({ ...videoStoreState([createTestVideo({ id: 'a', path: '/videos/a.mp4', durationSeconds: 7200 })]) })
    vi.mocked(backend.getCachedVideoFilmstrip).mockResolvedValue(strip)
    act(() => useStore.getState().setWantedFilmstripWindows({ '/videos/a.mp4': [zoom(0), zoom(1)] }))
    render()
    await waitFor(() => expect(useStore.getState().videoFilmstrips['/videos/a.mp4']['z0-1']?.status).toBe('ready'))

    act(() => useStore.getState().setWantedFilmstripWindows({ '/videos/a.mp4': [zoom(5)] }))

    await waitFor(() => expect(useStore.getState().videoFilmstrips['/videos/a.mp4']['z0-5']?.status).toBe('ready'))
    // Three windows loaded, two kept: one unshown window goes, the shown one and the base strip stay.
    expect(Object.keys(useStore.getState().videoFilmstrips['/videos/a.mp4']).sort()).toEqual(['base', 'z0-1', 'z0-5'])
    expect(URL.revokeObjectURL).toHaveBeenCalledOnce()
  })

  test('forgets the strips of a removed video and discards one that finishes after its removal', async () => {
    const a = deferred()
    const b = deferred()
    vi.mocked(backend.getVideoFilmstrip).mockReturnValueOnce(a.promise).mockReturnValueOnce(b.promise).mockReturnValue(pending())
    render()
    await waitFor(() => expect(backend.getVideoFilmstrip).toHaveBeenCalledTimes(2))
    await act(async () => a.resolve(strip))

    act(() => useStore.getState().removeImportedVideo('a'))
    act(() => useStore.getState().removeImportedVideo('b'))
    await act(async () => b.resolve(strip))

    expect(URL.revokeObjectURL).toHaveBeenCalledExactlyOnceWith('blob:sprite-1')
    expect(URL.createObjectURL).toHaveBeenCalledOnce()
    expect(Object.keys(useStore.getState().videoFilmstrips)).toEqual(['/videos/c.mp4'])
  })

  test('a video without a decodable frame is marked failed and not retried', async () => {
    useStore.setState({ ...videoStoreState([createTestVideo({ id: 'a', path: '/videos/a.mp4' })]) })
    vi.mocked(backend.getVideoFilmstrip).mockResolvedValue(null)
    render()

    await waitFor(() => expect(useStore.getState().videoFilmstrips['/videos/a.mp4']).toEqual({ base: { status: 'failed' } }))
    expect(backend.getVideoFilmstrip).toHaveBeenCalledOnce()
  })

  test('an effect run twice (StrictMode) starts each strip once', async () => {
    useStore.setState({ ...videoStoreState([createTestVideo({ id: 'a', path: '/videos/a.mp4' })]) })
    vi.mocked(backend.getVideoFilmstrip).mockResolvedValue(strip)
    render({ wrapper: StrictMode })

    await waitFor(() => expect(useStore.getState().videoFilmstrips['/videos/a.mp4']?.base?.status).toBe('ready'))
    expect(backend.getCachedVideoFilmstrip).toHaveBeenCalledOnce()
    expect(backend.getVideoFilmstrip).toHaveBeenCalledOnce()
  })

  test('decodes in software when codec detection failed', async () => {
    useStore.setState({
      ...videoStoreState([createTestVideo({ id: 'a', path: '/videos/a.mp4' })]),
      fetchAvailableCodecs: vi.fn().mockResolvedValue(null),
    })
    vi.mocked(backend.getVideoFilmstrip).mockResolvedValue(strip)
    render()

    await waitFor(() => expect(useStore.getState().videoFilmstrips['/videos/a.mp4']?.base?.status).toBe('ready'))
    expect(vi.mocked(backend.getVideoFilmstrip).mock.calls[0][0].hwaccel).toBeNull()
  })
})
