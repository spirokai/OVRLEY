import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import useVideoImport from '@/features/video-preview/hooks/useVideoImport'

const mocks = vi.hoisted(() => {
  const state = {
    importedVideoPath: null,
    importingVideo: false,
    config: { scene: { width: 1920, height: 1080 } },
    renderSettings: { widgetUpdateRate: 1 },
    setRenderFpsAndUpdateRate: vi.fn(),
    setImportedVideo: vi.fn(() => ({ width: 1920, height: 1080 })),
    setConfig: vi.fn(),
    setImportedBackgroundImage: vi.fn(),
    clearImportedVideo: vi.fn(),
    clearVideoTelemetry: vi.fn(),
    setErrorMessage: vi.fn(),
  }
  state.setImportingVideo = vi.fn((value) => {
    state.importingVideo = value
  })
  const store = (selector) => selector(state)
  store.getState = () => state
  return { state, store, open: vi.fn(), prepare: vi.fn(), register: vi.fn(), preview: vi.fn(), telemetry: vi.fn() }
})

vi.mock('@/store/useStore', () => ({ default: mocks.store }))
vi.mock('@/lib/file-dialog', () => ({ openMultiplePaths: mocks.open }))
vi.mock('@/api/backend', () => ({
  prepareVideoSequence: mocks.prepare,
  registerPreviewSequence: mocks.register,
  extractVideoSequenceTelemetry: mocks.telemetry,
  registerPreviewVideo: vi.fn(),
  importPreviewVideo: mocks.preview,
  extractVideoTelemetry: mocks.telemetry,
  clearPreviewVideo: vi.fn(),
  preparePreviewVideo: vi.fn(),
  detectCodecs: vi.fn(),
}))
vi.mock('@/features/undo-redo/undoHistory', () => ({ runWithoutEditorHistory: (_store, callback) => callback() }))

describe('multi-part video import', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.state.importingVideo = false
    mocks.open.mockResolvedValue(['C:/part2.mp4', 'C:/part1.mp4'])
    mocks.prepare.mockResolvedValue({
      metadata: {
        path: 'C:/runtime/sequence.ffconcat',
        duration: 30,
        fps: 30,
        resolution: { width: 1920, height: 1080 },
        parts: [
          { path: 'C:/part1.mp4', duration: 10, startSeconds: 0 },
          { path: 'C:/part2.mp4', duration: 20, startSeconds: 10 },
        ],
      },
      warnings: [],
    })
    mocks.register.mockResolvedValue([
      { importId: 'a', previewUrl: 'http://localhost/video/a' },
      { importId: 'b', previewUrl: 'http://localhost/video/b' },
    ])
    mocks.preview.mockResolvedValue({
      metadata: { path: 'C:/single.mov', duration: 10, fps: 30, resolution: { width: 1920, height: 1080 } },
      warnings: [],
    })
    mocks.telemetry.mockResolvedValue(null)
  })

  test('registers original chapters as a group without saving or remuxing a video', async () => {
    const { result } = renderHook(() => useVideoImport({}))
    await act(() => result.current.handleImportVideo())
    expect(mocks.prepare).toHaveBeenCalledWith(['C:/part1.mp4', 'C:/part2.mp4'])
    expect(mocks.register).toHaveBeenCalledWith(['C:/part1.mp4', 'C:/part2.mp4'])
    expect(mocks.preview).not.toHaveBeenCalled()
    expect(mocks.state.setImportedVideo).toHaveBeenCalledWith(
      expect.objectContaining({
        importedVideoDuration: 30,
        importedVideoParts: [
          expect.objectContaining({ path: 'C:/part1.mp4', startSeconds: 0, previewUrl: 'http://localhost/video/a' }),
          expect.objectContaining({ path: 'C:/part2.mp4', startSeconds: 10, previewUrl: 'http://localhost/video/b' }),
        ],
      }),
    )
    expect(mocks.state.importingVideo).toBe(false)
  })

  test('supports dropping multiple native paths', async () => {
    const { result } = renderHook(() => useVideoImport({}))
    await act(() => result.current.handleVideoFilesDrop(['C:/part2.mp4', 'C:/part1.mp4']))
    expect(mocks.prepare).toHaveBeenCalledWith(['C:/part1.mp4', 'C:/part2.mp4'])
  })

  test('keeps single-video import direct', async () => {
    mocks.open.mockResolvedValue(['C:/single.mov'])
    const { result } = renderHook(() => useVideoImport({}))
    await act(() => result.current.handleImportVideo())
    expect(mocks.prepare).not.toHaveBeenCalled()
    expect(mocks.preview).toHaveBeenCalledWith('C:/single.mov')
  })

  test('cancelling source selection preserves current media', async () => {
    mocks.open.mockResolvedValue(null)
    const { result } = renderHook(() => useVideoImport({}))
    await act(() => result.current.handleImportVideo())
    expect(mocks.prepare).not.toHaveBeenCalled()
    expect(mocks.state.clearVideoTelemetry).not.toHaveBeenCalled()
    expect(mocks.state.setImportedVideo).not.toHaveBeenCalled()
    expect(mocks.state.importingVideo).toBe(false)
  })

  test('preparation failure preserves current media and reports the error', async () => {
    mocks.prepare.mockRejectedValueOnce(new Error('Incompatible MP4 part'))
    const { result } = renderHook(() => useVideoImport({}))
    await act(() => result.current.handleImportVideo())
    expect(mocks.preview).not.toHaveBeenCalled()
    expect(mocks.state.clearVideoTelemetry).not.toHaveBeenCalled()
    expect(mocks.state.setErrorMessage).toHaveBeenCalledWith(expect.stringContaining('Incompatible MP4 part'))
    expect(mocks.state.importingVideo).toBe(false)
  })
})
