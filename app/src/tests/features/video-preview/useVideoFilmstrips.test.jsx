import { act, renderHook, waitFor } from '@testing-library/react'
import { StrictMode } from 'react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import * as backend from '@/api/backend'
import { useVideoFilmstripQueue } from '@/features/video-preview/hooks/useVideoFilmstrips'
import { createMissingVideo, createVideoMetadataSnapshot } from '@/lib/video-program/imported-video'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

vi.mock('@/api/backend', () => ({
  getVideoFilmstrip: vi.fn(),
}))

function deferred() {
  let resolve
  const promise = new Promise((done) => {
    resolve = done
  })
  return { promise, resolve }
}

const sheet = { spriteJpegBase64: btoa('jpeg'), columns: 2, rows: 2 }

describe('useVideoFilmstripQueue', () => {
  const originalCreateObjectURL = URL.createObjectURL
  const originalRevokeObjectURL = URL.revokeObjectURL
  let objectUrls = 0

  beforeEach(() => {
    vi.mocked(backend.getVideoFilmstrip).mockReset()
    objectUrls = 0
    URL.createObjectURL = vi.fn(() => `blob:sprite-${++objectUrls}`)
    URL.revokeObjectURL = vi.fn()
    useStore.setState(useStore.getInitialState(), true)
    const missing = createTestVideo({ id: 'm', path: '/videos/missing.mp4', durationSeconds: 30, importOrder: 3 })
    useStore.setState({
      ...videoStoreState([
        createTestVideo({ id: 'a', path: '/videos/a.mp4', durationSeconds: 237, width: 3840, height: 2160 }),
        createTestVideo({ id: 'b', path: '/videos/b.mp4', durationSeconds: 30, importOrder: 1 }),
        createTestVideo({ id: 'c', path: '/videos/c.mp4', durationSeconds: 30, importOrder: 2 }),
        createMissingVideo(createVideoMetadataSnapshot(missing), { id: 'm', path: missing.path, importOrder: 3 }),
      ]),
      fetchAvailableCodecs: vi.fn().mockResolvedValue({ nvdec: true }),
    })
  })

  afterEach(() => {
    URL.createObjectURL = originalCreateObjectURL
    URL.revokeObjectURL = originalRevokeObjectURL
  })

  test('decodes two found videos at a time in collection order, with their plan and the GPU decoder', async () => {
    const first = deferred()
    vi.mocked(backend.getVideoFilmstrip)
      .mockReturnValueOnce(first.promise)
      .mockReturnValue(new Promise(() => {}))
    renderHook(() => useVideoFilmstripQueue())

    await waitFor(() => expect(backend.getVideoFilmstrip).toHaveBeenCalledTimes(2))
    expect(vi.mocked(backend.getVideoFilmstrip).mock.calls[0][0]).toEqual({
      path: '/videos/a.mp4',
      intervalSeconds: 20,
      sampleCount: 12,
      frameWidth: 171,
      frameHeight: 96,
      hwaccel: 'cuda',
    })
    expect(vi.mocked(backend.getVideoFilmstrip).mock.calls[1][0]).toMatchObject({ path: '/videos/b.mp4' })

    await act(async () => first.resolve(sheet))
    expect(useStore.getState().videoFilmstrips['/videos/a.mp4']).toEqual({
      status: 'ready',
      spriteUrl: 'blob:sprite-1',
      columns: 2,
      rows: 2,
      intervalSeconds: 20,
      sampleCount: 12,
    })
    // The finished slot goes to c; the missing video gets no filmstrip.
    await waitFor(() => expect(backend.getVideoFilmstrip).toHaveBeenCalledTimes(3))
    expect(vi.mocked(backend.getVideoFilmstrip).mock.calls[2][0]).toMatchObject({ path: '/videos/c.mp4' })
    expect(Object.keys(useStore.getState().videoFilmstrips).sort()).toEqual(['/videos/a.mp4', '/videos/b.mp4', '/videos/c.mp4'])
  })

  test('forgets the filmstrip of a removed video and discards one that finishes after its removal', async () => {
    const a = deferred()
    const b = deferred()
    vi.mocked(backend.getVideoFilmstrip)
      .mockReturnValueOnce(a.promise)
      .mockReturnValueOnce(b.promise)
      .mockReturnValue(new Promise(() => {}))
    renderHook(() => useVideoFilmstripQueue())
    await waitFor(() => expect(backend.getVideoFilmstrip).toHaveBeenCalledTimes(2))
    await act(async () => a.resolve(sheet))

    act(() => useStore.getState().removeImportedVideo('a'))
    act(() => useStore.getState().removeImportedVideo('b'))
    await act(async () => b.resolve(sheet))

    expect(URL.revokeObjectURL).toHaveBeenCalledExactlyOnceWith('blob:sprite-1')
    expect(URL.createObjectURL).toHaveBeenCalledOnce()
    expect(Object.keys(useStore.getState().videoFilmstrips)).toEqual(['/videos/c.mp4'])
  })

  test('a video without a decodable frame is marked failed and not retried', async () => {
    useStore.setState({ ...videoStoreState([createTestVideo({ id: 'a', path: '/videos/a.mp4' })]) })
    vi.mocked(backend.getVideoFilmstrip).mockResolvedValue(null)
    renderHook(() => useVideoFilmstripQueue())

    await waitFor(() => expect(useStore.getState().videoFilmstrips['/videos/a.mp4']).toEqual({ status: 'failed' }))
    expect(backend.getVideoFilmstrip).toHaveBeenCalledOnce()
  })

  test('an effect run twice (StrictMode) starts each filmstrip once', async () => {
    useStore.setState({ ...videoStoreState([createTestVideo({ id: 'a', path: '/videos/a.mp4' })]) })
    vi.mocked(backend.getVideoFilmstrip).mockResolvedValue(sheet)
    renderHook(() => useVideoFilmstripQueue(), { wrapper: StrictMode })

    await waitFor(() => expect(useStore.getState().videoFilmstrips['/videos/a.mp4']?.status).toBe('ready'))
    expect(backend.getVideoFilmstrip).toHaveBeenCalledOnce()
  })

  test('decodes in software when codec detection failed', async () => {
    useStore.setState({
      ...videoStoreState([createTestVideo({ id: 'a', path: '/videos/a.mp4' })]),
      fetchAvailableCodecs: vi.fn().mockResolvedValue(null),
    })
    vi.mocked(backend.getVideoFilmstrip).mockResolvedValue(sheet)
    renderHook(() => useVideoFilmstripQueue())

    await waitFor(() => expect(useStore.getState().videoFilmstrips['/videos/a.mp4']?.status).toBe('ready'))
    expect(vi.mocked(backend.getVideoFilmstrip).mock.calls[0][0].hwaccel).toBeNull()
  })
})
